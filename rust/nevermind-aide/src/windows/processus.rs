//! Ce que Windows dit des processus (`AideProcessus`, aide-actions.ps1) : chemin, session, compte, protection,
//! droits administrateur ; et le compte de l'Aide elle-même.

use super::outils::{MemoireLocale, Poignee, texte};
use aide_regles::fermeture::Fiche;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE, LUID};
use windows::Win32::Security::Authorization::{ConvertSidToStringSidW, ConvertStringSidToSidW};
use windows::Win32::Security::{
    AdjustTokenPrivileges, CheckTokenMembership, GetTokenInformation, LUID_AND_ATTRIBUTES, LookupAccountNameW,
    LookupPrivilegeValueW, PSID, SE_PRIVILEGE_ENABLED, SID_NAME_USE, TOKEN_ADJUST_PRIVILEGES, TOKEN_ELEVATION,
    TOKEN_PRIVILEGES, TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenUser,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcess, GetProcessInformation, OpenProcess, OpenProcessToken, PROCESS_ACCESS_RIGHTS,
    PROCESS_NAME_WIN32, PROCESS_PROTECTION_LEVEL_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    ProcessProtectionLevelInfo, QueryFullProcessImageNameW, ReleaseMutex, TerminateProcess,
};
use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR};

fn ouvrir(pid: i64, acces: PROCESS_ACCESS_RIGHTS) -> Option<Poignee> {
    let pid = u32::try_from(pid).ok()?;
    // SAFETY : simple ouverture, la poignée est refermée par Poignee
    unsafe { OpenProcess(acces, false, pid).ok().map(Poignee) }
}

/// Le chemin complet du programme d'un processus (vide si illisible)
pub fn chemin(pid: i64) -> String {
    let Some(h) = ouvrir(pid, PROCESS_QUERY_LIMITED_INFORMATION) else { return String::new() };
    let mut b = vec![0u16; 32768];
    let mut n = b.len() as u32;
    // SAFETY : tampon et taille cohérents
    match unsafe { QueryFullProcessImageNameW(h.0, PROCESS_NAME_WIN32, PWSTR(b.as_mut_ptr()), &mut n) } {
        Ok(()) => String::from_utf16_lossy(&b[..n as usize]),
        Err(_) => String::new(),
    }
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
    let h = ouvrir(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut j = HANDLE::default();
    // SAFETY : lecture seule du jeton
    unsafe { OpenProcessToken(h.0, TOKEN_QUERY, &mut j).ok()? };
    Some(Poignee(j))
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
    let Some(h) = ouvrir(pid, PROCESS_QUERY_LIMITED_INFORMATION) else { return -1 };
    let mut info = PROCESS_PROTECTION_LEVEL_INFORMATION::default();
    // SAFETY : structure de la bonne taille
    let r = unsafe {
        GetProcessInformation(
            h.0,
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

/// Lancé en administrateur : 0 non, 1 oui, -1 illisible (traité comme oui)
fn eleve(pid: i64) -> i64 {
    let Some(j) = jeton(pid) else { return -1 };
    match info_jeton(j.0, TokenElevation) {
        Some(b) => {
            // SAFETY : le tampon contient un TOKEN_ELEVATION
            let e = unsafe { &*(b.as_ptr() as *const TOKEN_ELEVATION) };
            i64::from(e.TokenIsElevated != 0)
        }
        None => -1,
    }
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

/// Ferme un processus (comme Process.Kill de .NET : code de sortie -1)
pub fn fermer(pid: i64) -> bool {
    let Some(h) = ouvrir(pid, PROCESS_TERMINATE) else { return false };
    // SAFETY : poignée ouverte avec le droit de fermer
    unsafe { TerminateProcess(h.0, u32::MAX).is_ok() }
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

/// Le verrou de l'Aide (une seule à la fois), relâché quand il n'est plus utilisé
pub struct Verrou(Poignee);

impl Drop for Verrou {
    fn drop(&mut self) {
        // SAFETY : verrou pris par ce processus
        unsafe {
            let _ = ReleaseMutex(self.0.0);
        }
    }
}

/// Prend le verrou de ce nom, `None` s'il existe déjà (une autre Aide tourne)
pub fn verrou(nom: &str) -> Option<Verrou> {
    // SAFETY : création d'un verrou nommé ; GetLastError lu juste après
    unsafe {
        let h = CreateMutexW(None, true, &HSTRING::from(nom)).ok()?;
        let deja = GetLastError() == ERROR_ALREADY_EXISTS;
        let v = Poignee(h);
        if deja {
            return None;
        }
        Some(Verrou(v))
    }
}
