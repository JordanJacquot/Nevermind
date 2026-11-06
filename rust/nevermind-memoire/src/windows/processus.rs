//! Les processus : la liste et leur mémoire (un seul appel à NtQuerySystemInformation), leur fiche pour la règle
//! « fermable » (session, compte, protection, administrateur : `AideProcessus`, processus.ps1), et leurs poignées
//! gardées jusqu'à la prochaine lecture (`NmFermeture`, memoire.ps1).

use super::lectures::version;
use crate::donnees::Processus;
use aide_regles::fermeture::Fiche;
use std::collections::HashMap;
use std::sync::Mutex;
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};
use windows::Wdk::System::Threading::{NtQueryInformationProcess, ProcessCommandLineInformation};
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, WAIT_TIMEOUT};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_INFORMATION_CLASS, TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenUser,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    GetProcessInformation, GetProcessTimes, OpenProcess, OpenProcessToken, PROCESS_ACCESS_RIGHTS, PROCESS_NAME_WIN32,
    PROCESS_PROTECTION_LEVEL_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    ProcessProtectionLevelInfo, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
};
use windows::core::PWSTR;

/// Une poignée refermée quand elle n'est plus utilisée
struct Poignee(HANDLE);

impl Drop for Poignee {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY : la poignée nous appartient et n'est refermée qu'ici
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

fn ouvrir(pid: u32, acces: PROCESS_ACCESS_RIGHTS) -> Option<Poignee> {
    // SAFETY : simple ouverture, refermée par Poignee
    unsafe { OpenProcess(acces, false, pid).ok().map(Poignee) }
}

/// Les poignées gardées (numéro du processus, poignée) : Windows ne redonne pas ce numéro tant qu'elle est ouverte
static GARDES: Mutex<Option<HashMap<u32, isize>>> = Mutex::new(None);

fn gardes() -> std::sync::MutexGuard<'static, Option<HashMap<u32, isize>>> {
    GARDES.lock().unwrap_or_else(|e| e.into_inner())
}

fn poignee_gardee(pid: i64) -> Option<HANDLE> {
    let pid = u32::try_from(pid).ok()?;
    gardes().as_ref().and_then(|g| g.get(&pid)).map(|&h| HANDLE(h as *mut core::ffi::c_void))
}

fn creation(h: HANDLE) -> Option<i64> {
    let (mut c, mut f, mut k, mut u) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY : lecture des temps d'un processus ouvert
    unsafe { GetProcessTimes(h, &mut c, &mut f, &mut k, &mut u).ok()? };
    Some((i64::from(c.dwHighDateTime) << 32) | i64::from(c.dwLowDateTime))
}

fn actif(h: HANDLE) -> bool {
    // SAFETY : attente nulle sur une poignée ouverte
    unsafe { WaitForSingleObject(h, 0) == WAIT_TIMEOUT }
}

/// `Garder(pid)` : ouvre le processus (lire, fermer, attendre) et garde sa poignée ; son heure de démarrage, 0 sinon
pub fn garder(pid: u32) -> i64 {
    let Some(p) = ouvrir(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE) else {
        return 0;
    };
    let Some(c) = creation(p.0).filter(|_| actif(p.0)) else { return 0 };
    let h = p.0;
    std::mem::forget(p);
    let mut g = gardes();
    let m = g.get_or_insert_with(HashMap::new);
    if let Some(avant) = m.insert(pid, h.0 as isize) {
        // SAFETY : l'ancienne poignée de ce numéro nous appartient
        unsafe {
            let _ = CloseHandle(HANDLE(avant as *mut core::ffi::c_void));
        }
    }
    c
}

/// `Relacher()` : toutes les poignées gardées sont rendues
pub fn relacher() {
    if let Some(m) = gardes().take() {
        for (_, h) in m {
            // SAFETY : poignées gardées par `garder`, refermées une seule fois
            unsafe {
                let _ = CloseHandle(HANDLE(h as *mut core::ffi::c_void));
            }
        }
    }
}

/// `Verifier` : 1 c'est bien lui, 0 il est déjà fermé, -1 pas le même ou plus de poignée
pub fn verifier(pid: i64, debut: i64, chemin: &str) -> i32 {
    let Some(h) = poignee_gardee(pid) else { return -1 };
    if creation(h) != Some(debut) {
        return -1;
    }
    if !actif(h) {
        return 0;
    }
    let c = chemin_poignee(h);
    if c.is_empty() || c.to_lowercase() != chemin.to_lowercase() {
        return -1;
    }
    1
}

/// `Vivant` : 1 toujours là, 0 fermé, -1 plus de poignée
pub fn vivant(pid: i64) -> i32 {
    match poignee_gardee(pid) {
        None => -1,
        Some(h) => i32::from(actif(h)),
    }
}

/// `Forcer` (après `verifier`) : 1 fait, -3 refusé par Windows
pub fn terminer(pid: i64) -> i32 {
    let Some(h) = poignee_gardee(pid) else { return -1 };
    // SAFETY : poignée gardée, ouverte avec le droit de fermer ; le processus a été revérifié juste avant
    if unsafe { TerminateProcess(h, 1) }.is_ok() { 1 } else { -3 }
}

fn chemin_poignee(h: HANDLE) -> String {
    let mut b = vec![0u16; 1024];
    let mut n = b.len() as u32;
    // SAFETY : tampon et taille cohérents
    match unsafe { QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(b.as_mut_ptr()), &mut n) } {
        Ok(()) => String::from_utf16_lossy(&b[..n as usize]),
        Err(_) => String::new(),
    }
}

fn info_jeton(jeton: HANDLE, classe: TOKEN_INFORMATION_CLASS) -> Option<Vec<u64>> {
    let mut n = 0u32;
    // SAFETY : premier appel pour la taille, second avec un tampon aligné de cette taille
    unsafe {
        let _ = GetTokenInformation(jeton, classe, None, 0, &mut n);
        if n == 0 {
            return None;
        }
        let mut b = vec![0u64; (n as usize).div_ceil(8)];
        GetTokenInformation(jeton, classe, Some(b.as_mut_ptr().cast()), n, &mut n).ok()?;
        Some(b)
    }
}

/// Ce qu'on lit sur un processus ouvert : compte (SID) et administrateur (0, 1, -1 illisible)
fn compte_et_eleve(h: HANDLE) -> (String, i64) {
    let mut j = HANDLE::default();
    // SAFETY : lecture seule du jeton
    if unsafe { OpenProcessToken(h, TOKEN_QUERY, &mut j) }.is_err() {
        return (String::new(), -1);
    }
    let j = Poignee(j);
    let compte = info_jeton(j.0, TokenUser)
        .and_then(|b| {
            // SAFETY : le tampon contient un TOKEN_USER rempli par Windows
            let u = unsafe { &*(b.as_ptr() as *const TOKEN_USER) };
            let mut p = PWSTR::null();
            // SAFETY : SID valable ; le texte rendu est libéré par LocalFree
            unsafe {
                ConvertSidToStringSidW(u.User.Sid, &mut p).ok()?;
                let t = p.to_string().ok();
                let _ = windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(p.0.cast())));
                t
            }
        })
        .unwrap_or_default();
    let eleve = info_jeton(j.0, TokenElevation)
        .map(|b| {
            // SAFETY : le tampon contient un TOKEN_ELEVATION
            let e = unsafe { &*(b.as_ptr() as *const TOKEN_ELEVATION) };
            i64::from(e.TokenIsElevated != 0)
        })
        .unwrap_or(-1);
    (compte, eleve)
}

fn session(pid: u32) -> i64 {
    let mut s = 0u32;
    // SAFETY : simple lecture
    if unsafe { ProcessIdToSessionId(pid, &mut s) }.is_ok() { i64::from(s) } else { -1 }
}

fn protection(h: HANDLE) -> i64 {
    let mut p = PROCESS_PROTECTION_LEVEL_INFORMATION::default();
    // SAFETY : la structure a la taille annoncée
    let ok = unsafe {
        GetProcessInformation(
            h,
            ProcessProtectionLevelInfo,
            (&mut p as *mut PROCESS_PROTECTION_LEVEL_INFORMATION).cast(),
            std::mem::size_of::<PROCESS_PROTECTION_LEVEL_INFORMATION>() as u32,
        )
    };
    if ok.is_err() {
        return -1;
    }
    if p.ProtectionLevel.0 == 0xFFFF_FFFE { 0 } else { 1 } // (PROTECTION_LEVEL_NONE)
}

/// Celui pour qui on ferme (`Get-AideReference`)
pub struct Ref {
    pub id: i64,
    pub session: Option<i64>,
    pub compte: String,
    pub eleve: Option<i64>,
}

pub fn reference(pid: u32) -> Ref {
    let (compte, eleve) = match ouvrir(pid, PROCESS_QUERY_LIMITED_INFORMATION) {
        Some(p) => compte_et_eleve(p.0),
        None => (String::new(), -1),
    };
    Ref { id: i64::from(pid), session: Some(session(pid)), compte, eleve: Some(eleve) }
}

/// `Read-AideFiche` : ce que Windows dit d'un processus (même pour un autre compte)
pub fn fiche(pid: i64, chemin: &str) -> Fiche {
    let Ok(p32) = u32::try_from(pid) else { return Fiche { id: pid, chemin: chemin.into(), ..Default::default() } };
    let (c, compte, prot, eleve) = match ouvrir(p32, PROCESS_QUERY_LIMITED_INFORMATION) {
        Some(h) => {
            let (compte, eleve) = compte_et_eleve(h.0);
            (chemin_poignee(h.0), compte, protection(h.0), eleve)
        }
        None => (String::new(), String::new(), -1, -1),
    };
    Fiche {
        id: pid,
        chemin: if c.is_empty() { chemin.into() } else { c },
        session: Some(session(p32)),
        compte,
        protection: Some(prot),
        eleve: Some(eleve),
    }
}

/// La ligne de commande d'un processus (seulement Edge et Chrome : « --no-startup-window »)
fn ligne_de_commande(h: HANDLE) -> String {
    let mut b = vec![0u64; 4096];
    let mut n = 0u32;
    // SAFETY : le tampon reçoit un UNICODE_STRING suivi de son texte
    let st = unsafe {
        NtQueryInformationProcess(h, ProcessCommandLineInformation, b.as_mut_ptr().cast(), (b.len() * 8) as u32, &mut n)
    };
    if st.0 != 0 {
        return String::new();
    }
    // SAFETY : Windows a rempli un UNICODE_STRING au début du tampon, dont le texte est dans le même tampon
    unsafe {
        let longueur = *(b.as_ptr() as *const u16) as usize / 2;
        let texte = *((b.as_ptr() as *const u8).add(8) as *const *const u16);
        if texte.is_null() {
            return String::new();
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(texte, longueur))
    }
}

/// La liste des processus (`Win32_Process` et `Win32_PerfRawData_PerfProc_Process`), avec leur fiche et, pour ceux
/// du même compte et de la même session que Nevermind, leur poignée gardée
pub fn liste(moi: &Ref) -> Result<Vec<Processus>, String> {
    const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;
    let mut taille = 512 * 1024usize;
    let mut b: Vec<u64>;
    loop {
        b = vec![0u64; taille / 8];
        let mut rendu = 0u32;
        // SAFETY : tampon de la taille annoncée
        let st = unsafe {
            NtQuerySystemInformation(SystemProcessInformation, b.as_mut_ptr().cast(), (b.len() * 8) as u32, &mut rendu)
        };
        if st.0 == STATUS_INFO_LENGTH_MISMATCH && taille < 64 * 1024 * 1024 {
            taille = (taille * 2).max(rendu as usize + 64 * 1024);
            continue;
        }
        if st.0 != 0 {
            return Err(format!("NtQuerySystemInformation 0x{:08x}", st.0));
        }
        break;
    }
    let octets = |o: usize, n: usize| -> &[u8] {
        // SAFETY : lecture dans le tampon rempli par Windows (bornes vérifiées par l'appelant)
        unsafe { std::slice::from_raw_parts((b.as_ptr() as *const u8).add(o), n) }
    };
    let lire_u32 = |o: usize| u32::from_le_bytes(octets(o, 4).try_into().unwrap_or([0; 4]));
    let lire_u64 = |o: usize| u64::from_le_bytes(octets(o, 8).try_into().unwrap_or([0; 8]));
    let fin = b.len() * 8;
    let mut versions: HashMap<String, (String, String, String)> = HashMap::new();
    let mut l = Vec::new();
    let mut o = 0usize;
    loop {
        if o + 256 > fin {
            break;
        }
        // Disposition 64 bits de SYSTEM_PROCESS_INFORMATION
        let suivant = lire_u32(o) as usize;
        let prive = lire_u64(o + 8) as f64;
        let longueur = usize::from(u16::from_le_bytes(octets(o + 56, 2).try_into().unwrap_or([0; 2])));
        let texte = lire_u64(o + 64) as usize;
        let pid = lire_u64(o + 80) as u32;
        let parent = lire_u64(o + 88) as u32;
        let ensemble = lire_u64(o + 144) as f64;
        let mut nom = if texte != 0 && longueur > 0 {
            // SAFETY : le nom pointe dans le tampon rempli par Windows
            String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(texte as *const u16, longueur / 2) })
        } else {
            String::new()
        };
        if pid == 0 {
            nom = "System Idle Process".into();
        }
        let mut p = Processus {
            pid: i64::from(pid),
            parent: i64::from(parent),
            nom,
            octets: if pid != 0 { prive } else { ensemble },
            ..Default::default()
        };
        if crate::texte::egal(&p.nom, "Memory Compression") {
            p.octets = p.octets.max(ensemble);
        }
        if pid > 4 {
            match ouvrir(pid, PROCESS_QUERY_LIMITED_INFORMATION) {
                Some(h) => {
                    p.chemin = chemin_poignee(h.0);
                    let (compte, eleve) = compte_et_eleve(h.0);
                    p.compte = compte;
                    p.eleve = Some(eleve);
                    p.protection = Some(protection(h.0));
                    let bas = crate::constat::sans_exe(&p.nom).to_lowercase();
                    if bas == "msedge" || bas == "chrome" {
                        p.ligne = ligne_de_commande(h.0);
                    }
                }
                None => {
                    p.protection = Some(-1);
                    p.eleve = Some(-1);
                }
            }
            p.image = Some(p.chemin.clone());
            p.session = Some(session(pid));
            p.debut = if i64::from(pid) != moi.id
                && !p.compte.is_empty()
                && p.compte == moi.compte
                && p.session == moi.session
            {
                garder(pid)
            } else {
                0
            };
        }
        if !p.chemin.is_empty() {
            let v = versions.entry(p.chemin.clone()).or_insert_with(|| version(&p.chemin));
            p.produit = v.0.clone();
            p.description = v.1.clone();
            p.societe = v.2.clone();
        }
        l.push(p);
        if suivant == 0 {
            break;
        }
        o += suivant;
    }
    Ok(l)
}
