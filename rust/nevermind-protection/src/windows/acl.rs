//! `$protege` (securite.ps1, aide-client.ps1) : seul un administrateur peut modifier ce fichier. Ni lui ni son dossier
//! ne donnent un droit d'écriture (0xD0046) à Tout le monde, aux Utilisateurs authentifiés, aux Utilisateurs ou à
//! l'utilisateur lui-même. Illisible : pas protégé.

use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT};
use windows::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, GetAce, GetTokenInformation, PSECURITY_DESCRIPTOR,
    PSID, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{HSTRING, PWSTR};

/// Un SID en texte (« S-1-5-32-545 »)
fn sid_texte(sid: PSID) -> Option<String> {
    let mut p = PWSTR::null();
    // SAFETY : texte alloué par Windows, rendu par LocalFree
    unsafe {
        ConvertSidToStringSidW(sid, &mut p).ok()?;
        let s = p.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(p.0.cast())));
        s
    }
}

/// Le SID de l'utilisateur de ce processus
fn utilisateur() -> Option<String> {
    // SAFETY : jeton de ce processus, refermé à la fin ; tampon local assez grand
    unsafe {
        let mut jeton = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut jeton).ok()?;
        let mut n = 0u32;
        let _ = GetTokenInformation(jeton, TokenUser, None, 0, &mut n);
        let mut b = vec![0u8; n as usize];
        let ok = GetTokenInformation(jeton, TokenUser, Some(b.as_mut_ptr().cast()), n, &mut n).is_ok();
        let _ = windows::Win32::Foundation::CloseHandle(jeton);
        if !ok {
            return None;
        }
        let u = &*(b.as_ptr() as *const TOKEN_USER);
        sid_texte(u.User.Sid)
    }
}

/// Le chemin donne un droit d'écriture à l'un de ces SID (`None` : illisible)
fn ecriture_permise(chemin: &str, sids: &[String]) -> Option<bool> {
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut sd = PSECURITY_DESCRIPTOR::default();
    // SAFETY : descripteur alloué par Windows, rendu par LocalFree ; chaque entrée lue dans la liste de Windows
    unsafe {
        let r = GetNamedSecurityInfoW(
            &HSTRING::from(chemin),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut dacl),
            None,
            &mut sd,
        );
        if r.is_err() || dacl.is_null() {
            if !sd.0.is_null() {
                let _ = LocalFree(Some(HLOCAL(sd.0)));
            }
            // (pas de liste du tout : tout le monde a tous les droits)
            return if r.is_err() { None } else { Some(true) };
        }
        let mut permise = false;
        for i in 0..u32::from((*dacl).AceCount) {
            let mut ace = std::ptr::null_mut();
            if GetAce(dacl, i, &mut ace).is_err() || ace.is_null() {
                continue;
            }
            let tete = &*(ace as *const ACE_HEADER);
            if tete.AceType != 0 {
                continue; // (seulement « Autoriser »)
            }
            let a = &*(ace as *const ACCESS_ALLOWED_ACE);
            if a.Mask & 0xD0046 == 0 {
                continue;
            }
            let sid = PSID(std::ptr::addr_of!(a.SidStart) as *mut _);
            if sid_texte(sid).is_some_and(|s| sids.contains(&s)) {
                permise = true;
                break;
            }
        }
        let _ = LocalFree(Some(HLOCAL(sd.0)));
        Some(permise)
    }
}

pub fn protege(chemin: &str) -> bool {
    let mut sids = vec!["S-1-1-0".to_string(), "S-1-5-11".into(), "S-1-5-32-545".into()];
    match utilisateur() {
        Some(u) => sids.push(u),
        None => return false,
    }
    let dossier = chemin.rsplit_once('\\').map(|x| x.0.to_string()).unwrap_or_default();
    [chemin.to_string(), dossier].iter().all(|x| ecriture_permise(x, &sids) == Some(false))
}
