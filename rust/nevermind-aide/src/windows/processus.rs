//! Ce que Windows dit des processus (`AideProcessus`, aide-actions.ps1) : chemin, session, compte, protection,
//! droits administrateur ; et le compte de l'Aide elle-même.

use super::outils::{MemoireLocale, Poignee, texte};
use crate::systeme::ProcessusTenu;
use aide_regles::fermeture::Fiche;
use windows::Win32::Foundation::{FILETIME, GetLastError, HANDLE, LUID, WAIT_TIMEOUT};
use windows::Win32::Security::Authorization::{ConvertSidToStringSidW, ConvertStringSidToSidW};
use windows::Win32::Security::{
    AdjustTokenPrivileges, CheckTokenMembership, GetTokenInformation, LUID_AND_ATTRIBUTES, LookupAccountNameW,
    LookupPrivilegeValueW, PSID, SE_PRIVILEGE_ENABLED, SID_NAME_USE, TOKEN_ADJUST_PRIVILEGES, TOKEN_ELEVATION,
    TOKEN_PRIVILEGES, TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenSessionId, TokenUser,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetProcessInformation, GetProcessTimes, OpenProcess, OpenProcessToken, PROCESS_ACCESS_RIGHTS,
    PROCESS_NAME_WIN32, PROCESS_PROTECTION_LEVEL_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, ProcessProtectionLevelInfo, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
};
use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR};

fn ouvrir(pid: i64, acces: PROCESS_ACCESS_RIGHTS) -> Option<Poignee> {
    let pid = u32::try_from(pid).ok()?;
    // SAFETY : simple ouverture, la poignée est refermée par Poignee
    unsafe { OpenProcess(acces, false, pid).ok().map(Poignee) }
}

/// Le chemin complet du programme d'un processus (vide si illisible)
pub fn chemin(pid: i64) -> String {
    ouvrir(pid, PROCESS_QUERY_LIMITED_INFORMATION).map(|h| chemin_poignee(h.0)).unwrap_or_default()
}

/// Un SID en texte (« S-1-5-18 »)
pub fn sid_texte(sid: PSID) -> Option<String> {
    let mut p = PWSTR::null();
    // SAFETY : `sid` est un SID valable ; le texte rendu est libéré par MemoireLocale
    unsafe {
        ConvertSidToStringSidW(sid, &mut p).ok()?;
        let _m = MemoireLocale(p.0.cast());
        p.to_string().ok()
    }
}

/// Une information du jeton (taille lue d'abord), dans un tampon aligné
fn info_jeton(jeton: HANDLE, classe: windows::Win32::Security::TOKEN_INFORMATION_CLASS) -> Option<Vec<u64>> {
    let mut n = 0u32;
    // SAFETY : premier appel pour connaître la taille, second avec un tampon de cette taille
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

/// Le compte (SID) d'un jeton
fn compte_du_jeton(jeton: HANDLE) -> Option<String> {
    let b = info_jeton(jeton, TokenUser)?;
    // SAFETY : le tampon contient un TOKEN_USER rempli par Windows
    let u = unsafe { &*(b.as_ptr() as *const TOKEN_USER) };
    sid_texte(u.User.Sid)
}

/// Le jeton d'un processus
fn jeton(pid: i64) -> Option<Poignee> {
    jeton_poignee(ouvrir(pid, PROCESS_QUERY_LIMITED_INFORMATION)?.0)
}

/// Le compte d'un processus (SID), vide si Windows ne le dit pas
pub fn compte(pid: i64) -> String {
    jeton(pid).and_then(|j| compte_du_jeton(j.0)).unwrap_or_default()
}

/// La session Windows d'un processus
fn session(pid: i64) -> Option<i64> {
    let mut s = 0u32;
    // SAFETY : simple lecture
    unsafe { ProcessIdToSessionId(u32::try_from(pid).ok()?, &mut s).ok()? };
    Some(i64::from(s))
}

/// Niveau de protection : 0 aucun, 1 protégé (antivirus, Windows), -1 illisible (traité comme protégé)
pub fn protection(pid: i64) -> i64 {
    ouvrir(pid, PROCESS_QUERY_LIMITED_INFORMATION).map(|h| protection_poignee(h.0)).unwrap_or(-1)
}

/// Lancé en administrateur : 0 non, 1 oui, -1 illisible (traité comme oui)
fn eleve(pid: i64) -> i64 {
    eleve_jeton(jeton(pid).as_ref())
}

/// La fiche d'un processus (`Read-AideFiche`)
pub fn fiche(pid: i64, chemin_connu: &str) -> Fiche {
    let mut c = chemin(pid);
    if c.is_empty() {
        c = chemin_connu.to_string();
    }
    Fiche {
        id: pid,
        chemin: c,
        session: session(pid),
        compte: compte(pid),
        protection: Some(protection(pid)),
        eleve: Some(eleve(pid)),
    }
}

/// Les processus en cours : numéro, parent, nom du programme
pub fn liste() -> Vec<(i64, i64, String)> {
    let mut r = Vec::new();
    // SAFETY : instantané refermé par Poignee ; structure initialisée avec sa taille
    unsafe {
        let Ok(h) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return r };
        let h = Poignee(h);
        let mut e = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(h.0, &mut e).is_ok();
        while ok {
            r.push((i64::from(e.th32ProcessID), i64::from(e.th32ParentProcessID), texte(&e.szExeFile)));
            ok = Process32NextW(h.0, &mut e).is_ok();
        }
    }
    r
}

// ---------------------------------------------------------------------------------------------------------------------
// Un processus tenu par sa poignée (audit du 08/11, M6) : ouvert une seule fois, lu et fermé par cette poignée
// ---------------------------------------------------------------------------------------------------------------------

/// Le chemin complet du programme d'un processus ouvert (vide si illisible)
fn chemin_poignee(h: HANDLE) -> String {
    let mut b = vec![0u16; 32768];
    let mut n = b.len() as u32;
    // SAFETY : tampon et taille cohérents
    match unsafe { QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(b.as_mut_ptr()), &mut n) } {
        Ok(()) => String::from_utf16_lossy(&b[..n as usize]),
        Err(_) => String::new(),
    }
}

/// Le jeton d'un processus ouvert
fn jeton_poignee(h: HANDLE) -> Option<Poignee> {
    let mut j = HANDLE::default();
    // SAFETY : lecture seule du jeton
    unsafe { OpenProcessToken(h, TOKEN_QUERY, &mut j).ok()? };
    Some(Poignee(j))
}

/// Niveau de protection d'un processus ouvert : 0 aucun, 1 protégé, -1 illisible (traité comme protégé)
fn protection_poignee(h: HANDLE) -> i64 {
    let mut info = PROCESS_PROTECTION_LEVEL_INFORMATION::default();
    // SAFETY : structure de la bonne taille
    let r = unsafe {
        GetProcessInformation(
            h,
            ProcessProtectionLevelInfo,
            (&mut info as *mut PROCESS_PROTECTION_LEVEL_INFORMATION).cast(),
            size_of::<PROCESS_PROTECTION_LEVEL_INFORMATION>() as u32,
        )
    };
    match r {
        Ok(()) if info.ProtectionLevel.0 == 0xFFFF_FFFE => 0, // PROTECTION_LEVEL_NONE
        Ok(()) => 1,
        Err(_) => -1,
    }
}

/// Lancé en administrateur (d'après son jeton) : 0 non, 1 oui, -1 illisible (traité comme oui)
fn eleve_jeton(j: Option<&Poignee>) -> i64 {
    let Some(j) = j else { return -1 };
    match info_jeton(j.0, TokenElevation) {
        Some(b) => {
            // SAFETY : le tampon contient un TOKEN_ELEVATION
            let e = unsafe { &*(b.as_ptr() as *const TOKEN_ELEVATION) };
            i64::from(e.TokenIsElevated != 0)
        }
        None => -1,
    }
}

/// La session Windows d'après le jeton
fn session_jeton(j: Option<&Poignee>) -> Option<i64> {
    let b = info_jeton(j?.0, TokenSessionId)?;
    // SAFETY : le tampon contient un DWORD (TokenSessionId)
    Some(i64::from(unsafe { *(b.as_ptr() as *const u32) }))
}

/// Un processus tenu par sa poignée (lire, fermer, attendre), refermée à la fin
pub struct Tenu {
    pid: i64,
    h: Poignee,
}

/// Ouvre le processus et garde sa poignée (`None` s'il ne s'ouvre pas)
pub fn tenir(pid: i64) -> Option<Tenu> {
    let h = ouvrir(pid, PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE)?;
    Some(Tenu { pid, h })
}

impl ProcessusTenu for Tenu {
    fn fiche(&self) -> Fiche {
        let j = jeton_poignee(self.h.0);
        Fiche {
            id: self.pid,
            chemin: chemin_poignee(self.h.0),
            session: session_jeton(j.as_ref()),
            compte: j.as_ref().and_then(|j| compte_du_jeton(j.0)).unwrap_or_default(),
            protection: Some(protection_poignee(self.h.0)),
            eleve: Some(eleve_jeton(j.as_ref())),
        }
    }
    fn debut(&self) -> Option<i64> {
        let (mut c, mut f, mut k, mut u) =
            (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        // SAFETY : lecture des temps d'un processus ouvert
        unsafe { GetProcessTimes(self.h.0, &mut c, &mut f, &mut k, &mut u).ok()? };
        Some((i64::from(c.dwHighDateTime) << 32) | i64::from(c.dwLowDateTime))
    }
    fn actif(&self) -> bool {
        // SAFETY : attente nulle sur une poignée ouverte avec SYNCHRONIZE
        unsafe { WaitForSingleObject(self.h.0, 0) == WAIT_TIMEOUT }
    }
    fn chemin(&self) -> String {
        chemin_poignee(self.h.0)
    }
    fn terminer(&self) -> bool {
        // SAFETY : poignée ouverte avec le droit de fermer ; le processus a été revérifié juste avant
        unsafe { TerminateProcess(self.h.0, u32::MAX).is_ok() }
    }
}

/// Le compte de l'Aide, et si elle a les droits administrateur (membre actif du groupe Administrateurs)
pub fn moi() -> (String, bool) {
    let mut j = HANDLE::default();
    // SAFETY : jeton de ce processus, refermé par Poignee
    let compte = unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut j).is_ok() {
            let j = Poignee(j);
            compte_du_jeton(j.0).unwrap_or_default()
        } else {
            String::new()
        }
    };
    let mut admin = false;
    let mut sid = PSID::default();
    // SAFETY : SID créé par Windows et libéré par MemoireLocale ; vérification sur le jeton de ce fil/processus
    unsafe {
        if ConvertStringSidToSidW(&HSTRING::from("S-1-5-32-544"), &mut sid).is_ok() {
            let _m = MemoireLocale(sid.0);
            let mut membre = BOOL::default();
            admin = CheckTokenMembership(None, sid, &mut membre).is_ok() && membre.as_bool();
        }
    }
    (compte, admin)
}

/// Le SID d'un nom de compte (« SYSTEM », « PC\Élodie »), comme NTAccount.Translate
pub fn traduire(nom: &str) -> Option<String> {
    let n = HSTRING::from(nom);
    let mut taille_sid = 0u32;
    let mut taille_domaine = 0u32;
    let mut usage = SID_NAME_USE::default();
    // SAFETY : premier appel pour les tailles, second avec des tampons de ces tailles
    unsafe {
        let _ = LookupAccountNameW(PCWSTR::null(), &n, None, &mut taille_sid, None, &mut taille_domaine, &mut usage);
        if taille_sid == 0 {
            return None;
        }
        let mut sid = vec![0u64; (taille_sid as usize).div_ceil(8)];
        let mut domaine = vec![0u16; taille_domaine as usize + 1];
        LookupAccountNameW(
            PCWSTR::null(),
            &n,
            Some(PSID(sid.as_mut_ptr().cast())),
            &mut taille_sid,
            Some(PWSTR(domaine.as_mut_ptr())),
            &mut taille_domaine,
            &mut usage,
        )
        .ok()?;
        sid_texte(PSID(sid.as_mut_ptr().cast()))
    }
}

/// Active un privilège de l'Aide (« SeSystemEnvironmentPrivilege » pour lire Secure Boot)
pub fn privilege(nom: &str) -> bool {
    let mut j = HANDLE::default();
    // SAFETY : jeton de ce processus, refermé par Poignee ; structure à un seul privilège
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut j).is_err() {
            return false;
        }
        let j = Poignee(j);
        let mut luid = LUID::default();
        if LookupPrivilegeValueW(PCWSTR::null(), &HSTRING::from(nom), &mut luid).is_err() {
            return false;
        }
        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
        };
        AdjustTokenPrivileges(j.0, false, Some(&tp), 0, None, None).is_ok() && GetLastError().0 == 0
    }
}

#[cfg(test)]
mod essais {
    use super::*;
    use std::os::windows::process::CommandExt;

    /// Le vrai Windows, sans droits, sur un programme lancé par l'essai lui-même (PING.EXE caché, fermé à la fin) : la
    /// fiche, l'heure de démarrage et le chemin sont lus par la poignée tenue, puis il est fermé par elle ; une fois
    /// fini, la poignée le dit (plus actif), et rien d'autre n'est touché
    #[test]
    fn un_processus_tenu_est_lu_et_ferme_par_sa_poignee() {
        let ping = format!(r"{}\PING.EXE", nevermind_noyau_sys32());
        let mut enfant = std::process::Command::new(&ping)
            .args(["-n", "60", "127.0.0.1"])
            .creation_flags(0x0800_0000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = i64::from(enfant.id());
        let t = tenir(pid).expect("le processus de l'essai ne s'ouvre pas");
        let f = t.fiche();
        assert!(f.chemin.eq_ignore_ascii_case(&ping), "{}", f.chemin);
        assert_eq!(f.compte, moi().0);
        assert_eq!(f.session, session(pid));
        assert_eq!(f.protection, Some(0));
        assert_eq!(f.eleve, Some(eleve(pid)));
        let debut = t.debut().expect("heure de démarrage illisible");
        assert!(t.actif());
        assert!(t.chemin().eq_ignore_ascii_case(&ping));
        assert!(t.terminer());
        let fin = enfant.wait().unwrap();
        assert!(!fin.success());
        // Fini : la poignée tenue le voit, garde la même heure de démarrage (le numéro n'a pas pu être redonné)
        assert!(!t.actif());
        assert_eq!(t.debut(), Some(debut));
        drop(t);
    }

    fn nevermind_noyau_sys32() -> String {
        let mut b = vec![0u16; 1024];
        // SAFETY : tampon et taille cohérents
        let n = unsafe { windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut b)) } as usize;
        String::from_utf16_lossy(&b[..n.min(b.len())])
    }
}
