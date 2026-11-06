//! Ce que le bloc lit et fait directement dans Windows (comme natif.cs et donnees.ps1) : le registre (lecture partout,
//! écriture seulement dans celui de l'utilisateur), les écrans et leur fréquence, le curseur du mode d'alimentation,
//! la souris, les services (lecture), powercfg et les autres commandes (toujours sans fenêtre), WMI, les journaux
//! d'événements. Le seul endroit du bloc où le code `unsafe` est permis.
#![allow(unsafe_code)]

pub mod journal;
pub mod wmi;

use crate::pc::ValeurReg;
use std::os::windows::process::CommandExt;
use windows::Win32::Graphics::Gdi::{
    CDS_UPDATEREGISTRY, ChangeDisplaySettingsExW, DEVMODEW, DISPLAY_DEVICEW, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT,
    DM_PELSWIDTH, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE, EnumDisplayDevicesW, EnumDisplaySettingsW,
};
use windows::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QUERY_SERVICE_CONFIGW, QueryServiceConfigW, QueryServiceStatus,
    SC_MANAGER_CONNECT, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_SETMOUSE, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SystemParametersInfoW,
};
use windows::core::{GUID, HSTRING, PCWSTR};

/// Aucune fenêtre de console ne s'ouvre (CREATE_NO_WINDOW)
const SANS_FENETRE: u32 = 0x0800_0000;

/// Ruche et sous-clé d'un chemin PowerShell (HKCU:\..., HKLM:\..., Registry::HKEY_USERS\...)
fn ruche(chemin: &str) -> Option<(&'static windows_registry::Key, String)> {
    let p = chemin.strip_prefix("Registry::").unwrap_or(chemin);
    let u = p.to_ascii_uppercase();
    for (pre, k) in [
        ("HKCU:", windows_registry::CURRENT_USER),
        ("HKEY_CURRENT_USER", windows_registry::CURRENT_USER),
        ("HKLM:", windows_registry::LOCAL_MACHINE),
        ("HKEY_LOCAL_MACHINE", windows_registry::LOCAL_MACHINE),
        ("HKEY_USERS", windows_registry::USERS),
        ("HKEY_CLASSES_ROOT", windows_registry::CLASSES_ROOT),
    ] {
        if u.starts_with(pre) {
            return Some((k, p[pre.len()..].trim_start_matches('\\').to_string()));
        }
    }
    None
}

fn depuis_utf16(o: &[u8]) -> Vec<u16> {
    o.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
}

/// `Get-RegState` : la valeur (rien si absente), une erreur si la clé ne s'ouvre pas pour une autre raison
pub fn reg_lire(chemin: &str, nom: &str) -> Result<Option<ValeurReg>, String> {
    let Some((r, sous)) = ruche(chemin) else { return Err(format!("Chemin de registre non pris en charge: {chemin}")) };
    let k = match r.open(&sous) {
        Ok(k) => k,
        Err(e) if e.code().0 as u32 == 0x8007_0002 || e.code().0 as u32 == 0x8007_0003 => return Ok(None),
        Err(e) => return Err(e.message()),
    };
    let v = match k.get_value(nom) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    use windows_registry::Type;
    let o: &[u8] = &v;
    let texte = |o: &[u8]| {
        let w = depuis_utf16(o);
        let fin = w.iter().position(|c| *c == 0).unwrap_or(w.len());
        String::from_utf16_lossy(&w[..fin])
    };
    Ok(Some(match v.ty() {
        Type::U32 => ValeurReg::DWord(i64::from(i32::from_le_bytes(
            o.get(..4).and_then(|x| x.try_into().ok()).unwrap_or([0; 4]),
        ))),
        Type::U64 => ValeurReg::QWord(i64::from_le_bytes(o.get(..8).and_then(|x| x.try_into().ok()).unwrap_or([0; 8]))),
        Type::String => ValeurReg::Texte(texte(o)),
        Type::ExpandString => ValeurReg::TexteEtendu(texte(o)),
        Type::MultiString => {
            let w = depuis_utf16(o);
            ValeurReg::Multi(w.split(|c| *c == 0).filter(|x| !x.is_empty()).map(String::from_utf16_lossy).collect())
        }
        _ => ValeurReg::Octets(o.to_vec()),
    }))
}

/// Les noms des valeurs d'une clé
pub fn reg_noms(chemin: &str) -> Option<Vec<String>> {
    let (r, sous) = ruche(chemin)?;
    let k = r.open(&sous).ok()?;
    Some(k.values().ok()?.map(|(n, _)| n).collect())
}

/// Écrire une valeur du registre de l'utilisateur (jamais ailleurs : le reste passe par l'Aide)
pub fn reg_utilisateur_ecrire(chemin: &str, nom: &str, v: &ValeurReg) -> Result<(), String> {
    let Some((r, sous)) = ruche(chemin).filter(|(r, _)| std::ptr::eq(*r, windows_registry::CURRENT_USER)) else {
        return Err("Seul le registre de l'utilisateur est écrit par Nevermind.".into());
    };
    let k = r.create(&sous).map_err(|e| e.message())?;
    let r = match v {
        ValeurReg::DWord(n) => k.set_u32(nom, *n as u32),
        ValeurReg::QWord(n) => k.set_u64(nom, *n as u64),
        ValeurReg::Texte(s) => k.set_string(nom, s),
        ValeurReg::TexteEtendu(s) => k.set_expand_string(nom, s),
        ValeurReg::Multi(l) => k.set_multi_string(nom, &l.iter().map(|s| s.as_str()).collect::<Vec<_>>()),
        ValeurReg::Octets(o) => k.set_bytes(nom, windows_registry::Type::Bytes, o),
    };
    r.map_err(|e| e.message())
}

pub fn reg_utilisateur_effacer(chemin: &str, nom: &str) -> Result<(), String> {
    let Some((r, sous)) = ruche(chemin).filter(|(r, _)| std::ptr::eq(*r, windows_registry::CURRENT_USER)) else {
        return Err("Seul le registre de l'utilisateur est écrit par Nevermind.".into());
    };
    let Ok(k) = r.options().read().write().open(&sous) else { return Ok(()) };
    let _ = k.remove_value(nom);
    Ok(())
}

/// Une clé entière du registre de l'utilisateur effacée (les copies d'origine, quand tout est remis)
pub fn reg_utilisateur_effacer_cle(sous: &str) {
    let _ = windows_registry::CURRENT_USER.remove_tree(sous);
}

pub fn reg_utilisateur_cle(sous: &str) -> Option<windows_registry::Key> {
    windows_registry::CURRENT_USER.create(sous).ok()
}

fn texte_large(w: &[u16]) -> String {
    let fin = w.iter().position(|c| *c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..fin])
}

/// `GetDisplays` : « appareil|nom|largeur|hauteur|fréquence|fréquence max|principal » de chaque écran actif
pub fn ecrans() -> Vec<String> {
    let mut l = Vec::new();
    for i in 0..32u32 {
        let mut d = DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
        // SAFETY : structure initialisée avec sa taille, lue seulement si l'appel réussit
        if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), i, &mut d, 0) }.as_bool() {
            break;
        }
        if d.StateFlags.0 & 1 == 0 {
            continue;
        }
        let nom = HSTRING::from(texte_large(&d.DeviceName));
        let mut cur = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
        // SAFETY : idem
        if !unsafe { EnumDisplaySettingsW(&nom, ENUM_CURRENT_SETTINGS, &mut cur) }.as_bool() {
            continue;
        }
        let mut max = cur.dmDisplayFrequency;
        for n in 0..4096u32 {
            let mut m = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
            // SAFETY : idem
            if !unsafe { EnumDisplaySettingsW(&nom, ENUM_DISPLAY_SETTINGS_MODE(n), &mut m) }.as_bool() {
                break;
            }
            if m.dmPelsWidth == cur.dmPelsWidth && m.dmPelsHeight == cur.dmPelsHeight && m.dmDisplayFrequency > max {
                max = m.dmDisplayFrequency;
            }
        }
        l.push(format!(
            "{}|{}|{}|{}|{}|{}|{}",
            texte_large(&d.DeviceName),
            texte_large(&d.DeviceString),
            cur.dmPelsWidth,
            cur.dmPelsHeight,
            cur.dmDisplayFrequency,
            max,
            if d.StateFlags.0 & 4 != 0 { "1" } else { "0" }
        ));
    }
    l
}

/// `SetRefreshRate` : la fréquence d'un écran, même résolution (0 si réussi)
pub fn ecran_frequence(appareil: &str, hz: i64) -> i32 {
    let nom = HSTRING::from(appareil);
    let mut cur = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
    // SAFETY : structure initialisée avec sa taille
    if !unsafe { EnumDisplaySettingsW(&nom, ENUM_CURRENT_SETTINGS, &mut cur) }.as_bool() {
        return -100;
    }
    cur.dmDisplayFrequency = hz as u32;
    cur.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY;
    // SAFETY : DEVMODEW valide, pas de fenêtre ni de paramètre en plus
    unsafe { ChangeDisplaySettingsExW(&nom, Some(&cur), None, CDS_UPDATEREGISTRY, None) }.0
}

// (fonctions de powrprof.dll absentes de sa bibliothèque d'import : cherchées au moment de l'appel)
fn powrprof(nom: &str) -> Option<unsafe extern "system" fn() -> isize> {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    // SAFETY : chargement d'une DLL de Windows et recherche d'une fonction par son nom (terminé par un zéro)
    unsafe {
        let m = LoadLibraryW(&HSTRING::from("powrprof.dll")).ok()?;
        let n = std::ffi::CString::new(nom).ok()?;
        GetProcAddress(m, windows::core::PCSTR(n.as_ptr().cast()))
    }
}

/// Le curseur du mode d'alimentation (`GetOverlay`)
pub fn curseur() -> Option<String> {
    let f = powrprof("PowerGetEffectiveOverlayScheme")?;
    // SAFETY : signature de PowerGetEffectiveOverlayScheme(GUID*) -> DWORD
    let f: unsafe extern "system" fn(*mut GUID) -> u32 = unsafe { std::mem::transmute(f) };
    let mut g = GUID::zeroed();
    // SAFETY : écrit un GUID à l'adresse donnée
    if unsafe { f(&mut g) } != 0 {
        return None;
    }
    Some(format!("{g:?}").to_lowercase())
}

pub fn curseur_ecrire(guid: &str) -> i32 {
    let Some(f) = powrprof("PowerSetActiveOverlayScheme") else { return -1 };
    // SAFETY : signature de PowerSetActiveOverlayScheme(GUID) -> DWORD
    let f: unsafe extern "system" fn(GUID) -> u32 = unsafe { std::mem::transmute(f) };
    let g = GUID::try_from(guid).unwrap_or(GUID::zeroed());
    // SAFETY : GUID passé par valeur
    unsafe { f(g) as i32 }
}

/// `Sync-Mouse` : Windows relit les réglages de la souris (seuil 1, seuil 2, accélération)
pub fn souris(t1: i64, t2: i64, v: i64) {
    let mut p = [t1 as i32, t2 as i32, v as i32];
    // SAFETY : tableau de 3 entiers, comme SPI_SETMOUSE l'attend
    let _ = unsafe {
        SystemParametersInfoW(SPI_SETMOUSE, 0, Some(p.as_mut_ptr().cast()), SPIF_UPDATEINIFILE | SPIF_SENDCHANGE)
    };
}

/// Un programme lancé sans fenêtre de console : (code de sortie, texte écrit)
pub fn commande(exe: &str, args: &[String]) -> (i32, String) {
    match std::process::Command::new(exe).args(args).creation_flags(SANS_FENETRE).output() {
        Ok(o) => {
            let t: String = o.stdout.iter().map(|&b| cp850(b)).collect();
            (o.status.code().unwrap_or(1), t)
        }
        Err(_) => (1, String::new()),
    }
}

/// Lancer un programme sans attendre (`Start-Process`)
pub fn lancer(args: &[String]) {
    if let Some((exe, reste)) = args.split_first() {
        let mut c = std::process::Command::new(exe);
        for a in reste {
            c.args(a.split_whitespace());
        }
        let _ = c.creation_flags(SANS_FENETRE).spawn();
    }
}

/// La console de Windows écrit en page de code OEM (850 en français) : les accents de powercfg
fn cp850(b: u8) -> char {
    const H: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜø£Ø×ƒáíóúñÑªº¿®¬½¼¡«»░▒▓│┤ÁÂÀ©╣║╗╝¢¥┐└┴┬├─┼ãÃ╚╔╩╦╠═╬¤ðÐÊËÈıÍÎÏ┘┌█▄¦Ì▀ÓßÔÒõÕµþÞÚÛÙýÝ¯´\u{ad}±‗¾¶§÷¸°¨·¹³²■\u{a0}";
    if b < 128 { b as char } else { H.chars().nth((b - 128) as usize).unwrap_or('?') }
}

/// Un service : (démarrage, il tourne), lu sans droits administrateur
pub fn service(nom: &str) -> Option<(String, bool)> {
    // SAFETY : poignées ouvertes et fermées ici ; la configuration est lue dans un tampon de la taille demandée
    unsafe {
        let scm = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT).ok()?;
        let s = OpenServiceW(scm, &HSTRING::from(nom), SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS);
        let Ok(s) = s else {
            let _ = CloseServiceHandle(scm);
            return None;
        };
        let mut besoin = 0u32;
        let _ = QueryServiceConfigW(s, None, 0, &mut besoin);
        let mut tampon = vec![0u8; besoin.max(64) as usize];
        let cfg = tampon.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
        let debut = if QueryServiceConfigW(s, Some(cfg), tampon.len() as u32, &mut besoin).is_ok() {
            match (*cfg).dwStartType.0 {
                2 => "Automatic",
                3 => "Manual",
                4 => "Disabled",
                0 => "Boot",
                1 => "System",
                _ => "",
            }
        } else {
            ""
        };
        let mut st = SERVICE_STATUS::default();
        let tourne = QueryServiceStatus(s, &mut st).is_ok() && st.dwCurrentState == SERVICE_RUNNING;
        let _ = CloseServiceHandle(s);
        let _ = CloseServiceHandle(scm);
        Some((debut.to_string(), tourne))
    }
}

/// Les sous-clés d'une clé du registre
pub fn reg_sous_cles(chemin: &str) -> Vec<String> {
    let Some((r, sous)) = ruche(chemin) else { return Vec::new() };
    let Ok(k) = r.open(&sous) else { return Vec::new() };
    k.keys().map(|i| i.collect()).unwrap_or_default()
}

/// `Get-VolInfo` : la taille, la place libre et le nom d'un lecteur (rien s'il n'est pas prêt)
pub fn volume(lettre: &str) -> serde_json::Value {
    use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetVolumeInformationW};
    let racine = HSTRING::from(format!("{lettre}:\\"));
    let (mut libre, mut total) = (0u64, 0u64);
    // SAFETY : sorties écrites dans des variables locales
    if unsafe { GetDiskFreeSpaceExW(&racine, Some(&mut libre), Some(&mut total), None) }.is_err() {
        return serde_json::Value::Null;
    }
    let mut nom = [0u16; 261];
    // SAFETY : tampon de la taille annoncée
    let _ = unsafe { GetVolumeInformationW(&racine, Some(&mut nom), None, None, None, None) };
    serde_json::json!({ "Size": total as f64, "SizeRemaining": libre as f64, "FileSystemLabel": texte_large(&nom) })
}

/// `$env:firmware_type -eq 'UEFI'`
pub fn uefi() -> bool {
    use windows::Win32::System::SystemInformation::{FIRMWARE_TYPE, FirmwareTypeUefi, GetFirmwareType};
    let mut t = FIRMWARE_TYPE::default();
    // SAFETY : écrit le type à l'adresse donnée
    unsafe { GetFirmwareType(&mut t) }.is_ok() && t == FirmwareTypeUefi
}
