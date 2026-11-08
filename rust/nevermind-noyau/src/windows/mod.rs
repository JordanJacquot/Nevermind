//! Le seul module du noyau qui parle directement à Windows (et le seul où le code « unsafe » est permis) :
//! - le compte (SID) et les droits de Nevermind, la traduction d'un nom de compte en SID ;
//! - le remplacement d'un fichier d'un seul coup (`[IO.File]::Replace`) ;
//! - le texte des programmes de Windows (page de code OEM, comme PowerShell le lit) ;
//! - les variables d'environnement « %...% » (`GetValue` sans DoNotExpandEnvironmentNames) ;
//! - le bureau de l'utilisateur ; la demande de Windows (« Oui » pour lancer l'Aide) ;
//! - le canal de l'Aide ([`canal`]) et le réseau par WinHTTP ([`http`]).
//!
//! Aucune fenêtre ne s'ouvre ici : la seule chose qui peut s'afficher est la demande de Windows, et seulement quand
//! l'utilisateur a cliqué sur une action qui en a besoin (jamais en mode test).

#![allow(unsafe_code)]

pub mod canal;
pub mod http;
pub mod route;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Globalization::{CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS, MultiByteToWideChar};
use windows::Win32::Security::Authorization::{ConvertSidToStringSidW, ConvertStringSidToSidW};
use windows::Win32::Security::{
    CheckTokenMembership, GetTokenInformation, LookupAccountNameW, PSID, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::Storage::FileSystem::{REPLACEFILE_WRITE_THROUGH, ReplaceFileW};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, KF_FLAG_DEFAULT, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, SHGetKnownFolderPath,
    ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR};

/// Une poignée de Windows, refermée quand elle n'est plus utilisée
pub(crate) struct Poignee(pub HANDLE);

impl Drop for Poignee {
    fn drop(&mut self) {
        if !self.0.is_invalid() && !self.0.0.is_null() {
            // SAFETY : la poignée nous appartient et n'est refermée qu'ici
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

// SAFETY : une poignée de Windows peut servir depuis n'importe quel fil
unsafe impl Send for Poignee {}
unsafe impl Sync for Poignee {}

/// Une mémoire donnée par Windows (LocalAlloc), rendue quand elle n'est plus utilisée
pub(crate) struct MemoireLocale(pub *mut core::ffi::c_void);

impl Drop for MemoireLocale {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY : mémoire allouée par Windows avec LocalAlloc, rendue une seule fois
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0)));
            }
        }
    }
}

/// Le message d'une erreur de Windows, en clair (dans la langue de Windows)
pub(crate) fn message(e: &windows::core::Error) -> String {
    let m = e.message();
    let m = m.trim();
    if m.is_empty() { format!("erreur 0x{:08X}", e.code().0) } else { m.to_string() }
}

/// Un SID en texte (« S-1-5-18 »)
pub(crate) fn sid_texte(sid: PSID) -> Option<String> {
    let mut p = PWSTR::null();
    // SAFETY : `sid` est un SID valable ; le texte rendu est libéré par MemoireLocale
    unsafe {
        ConvertSidToStringSidW(sid, &mut p).ok()?;
        let _m = MemoireLocale(p.0.cast());
        p.to_string().ok()
    }
}

/// Le SID du compte qui fait tourner Nevermind (`[Security.Principal.WindowsIdentity]::GetCurrent().User.Value`)
pub fn sid_utilisateur() -> String {
    let mut j = HANDLE::default();
    // SAFETY : jeton de ce processus en lecture, refermé par Poignee ; tampon de la taille demandée par Windows
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut j).is_err() {
            return String::new();
        }
        let j = Poignee(j);
        let mut n = 0u32;
        let _ = GetTokenInformation(j.0, TokenUser, None, 0, &mut n);
        if n == 0 {
            return String::new();
        }
        let mut b = vec![0u64; (n as usize).div_ceil(8)];
        if GetTokenInformation(j.0, TokenUser, Some(b.as_mut_ptr().cast()), n, &mut n).is_err() {
            return String::new();
        }
        let u = &*(b.as_ptr() as *const TOKEN_USER);
        sid_texte(u.User.Sid).unwrap_or_default()
    }
}

/// Nevermind tourne-t-il en administrateur (membre actif du groupe Administrateurs, `IsInRole(Administrator)`) ?
pub fn est_admin() -> bool {
    let mut sid = PSID::default();
    // SAFETY : SID créé par Windows et libéré par MemoireLocale ; vérification sur le jeton de ce processus
    unsafe {
        if ConvertStringSidToSidW(&HSTRING::from("S-1-5-32-544"), &mut sid).is_err() {
            return false;
        }
        let _m = MemoireLocale(sid.0);
        let mut membre = BOOL::default();
        CheckTokenMembership(None, sid, &mut membre).is_ok() && membre.as_bool()
    }
}

/// Le SID d'un nom de compte (« SYSTEM », « PC\Élodie »), comme `NTAccount.Translate`
pub fn traduire_compte(nom: &str) -> Option<String> {
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

/// `[IO.File]::Replace(tmp, fichier, bak)` : `tmp` prend la place de `fichier` d'un coup, l'ancien devient `bak`.
pub fn remplacer_fichier(
    tmp: &std::path::Path,
    fichier: &std::path::Path,
    bak: &std::path::Path,
) -> std::io::Result<()> {
    // SAFETY : trois chemins terminés par un zéro, vivants pendant l'appel
    unsafe {
        ReplaceFileW(
            &HSTRING::from(fichier.as_os_str()),
            &HSTRING::from(tmp.as_os_str()),
            &HSTRING::from(bak.as_os_str()),
            REPLACEFILE_WRITE_THROUGH,
            None,
            None,
        )
    }
    .map_err(|e| std::io::Error::from_raw_os_error(e.code().0 & 0xFFFF))
}

/// Le texte écrit par un programme de Windows (schtasks...), dans la page de code OEM de la console, comme
/// PowerShell le lit quand il capture sa sortie.
pub fn texte_oem(o: &[u8]) -> String {
    if o.is_empty() {
        return String::new();
    }
    // SAFETY : premier appel pour la taille, second avec un tampon de cette taille
    unsafe {
        let n = MultiByteToWideChar(CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), o, None);
        if n <= 0 {
            return String::from_utf8_lossy(o).into_owned();
        }
        let mut b = vec![0u16; n as usize];
        let n = MultiByteToWideChar(CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), o, Some(&mut b));
        String::from_utf16_lossy(&b[..n.max(0) as usize])
    }
}

/// Remplace les « %VARIABLE% » d'un texte (une valeur ExpandString du registre lue par `GetValue`)
pub fn etendre_variables(t: &str) -> String {
    let src = HSTRING::from(t);
    // SAFETY : premier appel pour la taille, second avec un tampon de cette taille
    unsafe {
        let n = ExpandEnvironmentStringsW(&src, None);
        if n == 0 {
            return t.to_string();
        }
        let mut b = vec![0u16; n as usize];
        let n = ExpandEnvironmentStringsW(&src, Some(&mut b));
        if n == 0 {
            return t.to_string();
        }
        let fin = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        String::from_utf16_lossy(&b[..fin])
    }
}

/// Un dossier connu de Windows (Program Files...), lu par Windows et jamais dans les variables d'environnement
pub fn dossier_connu(id: &windows::core::GUID) -> String {
    // SAFETY : texte rendu par Windows, libéré avec CoTaskMemFree
    unsafe {
        match SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) {
            Ok(p) => {
                let t = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as *const _));
                t
            }
            Err(_) => String::new(),
        }
    }
}

/// Le bureau de l'utilisateur (`[Environment]::GetFolderPath('Desktop')`)
pub fn dossier_bureau() -> Option<std::path::PathBuf> {
    let d = dossier_connu(&FOLDERID_Desktop);
    if d.is_empty() { None } else { Some(std::path::PathBuf::from(d)) }
}

/// La demande normale de Windows (`Start-Process -Verb RunAs`) : l'utilisateur clique « Oui » pour lancer ce
/// programme avec les droits administrateur. `Err` : refusée (ou impossible).
pub fn demande_windows(exe: &str, arguments: &str, dossier: &str) -> Result<(), String> {
    let verbe = HSTRING::from("runas");
    let fichier = HSTRING::from(exe);
    let args = HSTRING::from(arguments);
    let dir = HSTRING::from(dossier);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verbe.as_ptr()),
        lpFile: PCWSTR(fichier.as_ptr()),
        lpParameters: PCWSTR(args.as_ptr()),
        lpDirectory: PCWSTR(dir.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY : structure remplie, textes vivants pendant l'appel ; la poignée du processus est refermée
    unsafe {
        ShellExecuteExW(&mut info).map_err(|e| message(&e))?;
        if !info.hProcess.is_invalid() && !info.hProcess.0.is_null() {
            drop(Poignee(info.hProcess));
        }
    }
    Ok(())
}

/// Lance un programme comme un double clic (`Start-Process`, ShellExecute) : s'il demande les droits administrateur
/// (mdsched.exe, un désinstalleur, certains jeux), Windows pose sa question normale, ce que `std::process::Command`
/// ne sait pas faire (erreur 740). Attend la réponse de l'utilisateur à cette question : à appeler hors du fil des
/// blocs. `Err` : le texte à montrer (refusé par l'utilisateur, ou impossible).
pub fn ouvrir_programme(exe: &str, arguments: &str, dossier: &str) -> Result<(), String> {
    use windows::Win32::Foundation::ERROR_CANCELLED;
    use windows::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC};
    let fichier = HSTRING::from(exe);
    let args = HSTRING::from(arguments);
    let dir = HSTRING::from(dossier);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        // (aucune boîte d'erreur de Windows : l'erreur revient ici, et Nevermind la dit avec ses mots)
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
        lpFile: PCWSTR(fichier.as_ptr()),
        lpParameters: if arguments.is_empty() { PCWSTR::null() } else { PCWSTR(args.as_ptr()) },
        lpDirectory: if dossier.is_empty() { PCWSTR::null() } else { PCWSTR(dir.as_ptr()) },
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY : COM ouvert et refermé sur ce fil (ShellExecuteEx le demande) ; structure remplie, textes vivants
    // pendant l'appel ; la poignée du processus est refermée
    unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
        let r = ShellExecuteExW(&mut info);
        if r.is_ok() && !info.hProcess.is_invalid() && !info.hProcess.0.is_null() {
            drop(Poignee(info.hProcess));
        }
        if com {
            CoUninitialize();
        }
        r.map_err(|e| {
            if e.code() == ERROR_CANCELLED.to_hresult() {
                "tu as répondu « Non » à la demande de Windows".to_string()
            } else {
                message(&e)
            }
        })
    }
}

/// Le dossier System32 (`[Environment]::GetFolderPath('System')`)
pub fn dossier_systeme() -> String {
    let mut b = vec![0u16; 260];
    // SAFETY : tampon et taille cohérents
    let n = unsafe { windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut b)) } as usize;
    String::from_utf16_lossy(&b[..n.min(b.len())])
}

/// « fr-FR, interface fr-FR » : la langue du compte et celle de l'interface de Windows (`Get-Culture`,
/// `Get-UICulture`)
pub fn langues() -> String {
    use windows::Win32::Globalization::{GetUserDefaultLocaleName, GetUserDefaultUILanguage, LCIDToLocaleName};
    let mut b = vec![0u16; 85];
    let mut u = vec![0u16; 85];
    // SAFETY : tampons à nous, de leur taille
    unsafe {
        let n = GetUserDefaultLocaleName(&mut b);
        let culture = if n > 1 { String::from_utf16_lossy(&b[..n as usize - 1]) } else { String::new() };
        let lcid = u32::from(GetUserDefaultUILanguage());
        let m = LCIDToLocaleName(lcid, Some(&mut u), 0);
        let interface = if m > 1 { String::from_utf16_lossy(&u[..m as usize - 1]) } else { String::new() };
        format!("{culture}, interface {interface}")
    }
}

/// « 1920 x 1080 » : la taille de l'écran principal
pub fn ecran_principal() -> String {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    // SAFETY : simple lecture
    unsafe { format!("{} x {}", GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}
