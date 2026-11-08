//! Le seul module qui parle à Windows directement. LECTURES seulement (registre, disques, applis du menu Démarrer,
//! heure, nom d'un programme), plus les deux gestes que l'utilisateur demande lui-même : lancer un jeu et choisir le
//! programme d'un jeu à ajouter. Le code « unsafe » (appels de Windows) n'est permis qu'ici.
#![allow(unsafe_code)]

use crate::systeme::Ruche;
use crate::texte::Moment;
use windows::Win32::Storage::FileSystem::{
    GetDriveTypeW, GetFileVersionInfoSizeW, GetFileVersionInfoW, GetLogicalDrives, VerQueryValueW,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, FOS_FILEMUSTEXIST, FileOpenDialog, IEnumShellItems, IFileOpenDialog,
    IShellItem, KF_FLAG_DEFAULT, SHGetKnownFolderItem, SIGDN_FILESYSPATH, SIGDN_NORMALDISPLAY,
    SIGDN_PARENTRELATIVEPARSING,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows_registry::{CLASSES_ROOT, CURRENT_USER, Key, LOCAL_MACHINE, Type};

/// L'Organizer : le fil, les fenêtres du jeu, les raccourcis, la souris, la barre flottante et la vue.
pub mod organizer;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn racine(r: Ruche) -> &'static Key {
    match r {
        Ruche::Utilisateur => CURRENT_USER,
        Ruche::Machine => LOCAL_MACHINE,
        Ruche::Classes => CLASSES_ROOT,
    }
}

/// « %ProgramFiles%\x » développé (REG_EXPAND_SZ, comme Get-ItemProperty).
fn developper(s: &str) -> String {
    let mut sortie = String::new();
    let mut reste = s;
    while let Some(i) = reste.find('%') {
        sortie.push_str(&reste[..i]);
        let apres = &reste[i + 1..];
        match apres.find('%') {
            Some(j) => {
                let nom = &apres[..j];
                match std::env::var(nom) {
                    Ok(v) if !nom.is_empty() => sortie.push_str(&v),
                    _ => {
                        sortie.push('%');
                        sortie.push_str(nom);
                        sortie.push('%');
                    }
                }
                reste = &apres[j + 1..];
            }
            None => {
                sortie.push('%');
                reste = apres;
            }
        }
    }
    sortie.push_str(reste);
    sortie
}

pub fn reg_texte(r: Ruche, cle: &str, nom: &str) -> Option<String> {
    let k = racine(r).open(cle).ok()?;
    let ty = k.get_type(nom).ok()?;
    match ty {
        Type::String => k.get_string(nom).ok(),
        Type::ExpandString => k.get_string(nom).ok().map(|s| developper(&s)),
        Type::U32 => k.get_u32(nom).ok().map(|v| v.to_string()),
        Type::U64 => k.get_u64(nom).ok().map(|v| v.to_string()),
        Type::MultiString => k.get_multi_string(nom).ok().map(|v| v.join(" ")),
        _ => None,
    }
}

pub fn reg_nombre(r: Ruche, cle: &str, nom: &str) -> Option<u32> {
    racine(r).open(cle).ok()?.get_u32(nom).ok()
}

pub fn reg_sous_cles(r: Ruche, cle: &str) -> Vec<String> {
    match racine(r).open(cle) {
        Ok(k) => k.keys().map(|it| it.collect()).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn disques_fixes() -> Vec<String> {
    let masque = unsafe { GetLogicalDrives() };
    let mut v = Vec::new();
    for i in 0..26u32 {
        if masque & (1 << i) == 0 {
            continue;
        }
        let racine = format!("{}:\\", (b'A' + i as u8) as char);
        let h = HSTRING::from(racine.as_str());
        // 3 = DRIVE_FIXED ; « prêt » : son dossier racine se lit
        if unsafe { GetDriveTypeW(&h) } == 3 && std::fs::read_dir(&racine).is_ok() {
            v.push(racine);
        }
    }
    v
}

pub fn heure_locale() -> Moment {
    let t = unsafe { GetLocalTime() };
    Moment::depuis(t.wYear as i64, t.wMonth as u32, t.wDay as u32, t.wHour as u32, t.wMinute as u32, t.wSecond as u32)
}

/// Lance un jeu (ou son désinstalleur) dans son dossier, sans console (comme son icône). Les arguments sont passés
/// tels quels. Un programme qui demande les droits administrateur (beaucoup de désinstalleurs, certains jeux) ne peut
/// pas être lancé ainsi (erreur 740) : il passe alors par ShellExecute, comme un double clic, et Windows pose sa
/// question (M8, audit du 08/11 : l'échec n'allait qu'au journal). Attend la réponse : hors du fil des blocs.
pub fn lancer(exe: &str, arguments: &str, dossier: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    /// ERROR_ELEVATION_REQUIRED : « l'opération demandée nécessite une élévation »
    const DROITS_DEMANDES: i32 = 740;
    let mut c = std::process::Command::new(exe);
    if !arguments.trim().is_empty() {
        c.raw_arg(arguments);
    }
    if !dossier.is_empty() {
        c.current_dir(dossier);
    }
    c.creation_flags(CREATE_NO_WINDOW);
    match c.spawn() {
        Ok(_) => Ok(()),
        Err(e) if e.raw_os_error() == Some(DROITS_DEMANDES) => {
            nevermind_noyau::windows::ouvrir_programme(exe, arguments.trim(), dossier)
        }
        Err(e) => Err(format!("Windows n'a pas pu le lancer ({e})")),
    }
}

struct Com(bool);
impl Com {
    fn ouvrir() -> Self {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        Com(hr.is_ok())
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

fn nom_element(it: &IShellItem, sigdn: windows::Win32::UI::Shell::SIGDN) -> Option<String> {
    unsafe {
        let p: PWSTR = it.GetDisplayName(sigdn).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

/// Les applis du menu Démarrer (nom, identifiant), comme Get-StartApps.
pub fn applis_demarrer() -> Vec<(String, String)> {
    let _com = Com::ouvrir();
    let mut v = Vec::new();
    unsafe {
        let Ok(dossier) = SHGetKnownFolderItem::<IShellItem>(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None) else {
            return v;
        };
        let Ok(liste) = dossier.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) else { return v };
        loop {
            let mut un: [Option<IShellItem>; 1] = [None];
            let mut lus = 0u32;
            if liste.Next(&mut un, Some(&mut lus)).is_err() || lus == 0 {
                break;
            }
            if let Some(it) = un[0].take() {
                let nom = nom_element(&it, SIGDN_NORMALDISPLAY).unwrap_or_default();
                let id = nom_element(&it, SIGDN_PARENTRELATIVEPARSING).unwrap_or_default();
                if !nom.is_empty() && !id.is_empty() {
                    v.push((nom, id));
                }
            }
        }
    }
    v
}

/// La fenêtre « Choisis le programme du jeu (.exe) ».
pub fn choisir_exe() -> Option<String> {
    let _com = Com::ouvrir();
    unsafe {
        let d: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let titre = HSTRING::from("Choisis le programme du jeu (.exe)");
        let _ = d.SetTitle(&titre);
        let nom = HSTRING::from("Programme du jeu (*.exe)");
        let motif = HSTRING::from("*.exe");
        let filtres = [COMDLG_FILTERSPEC { pszName: PCWSTR(nom.as_ptr()), pszSpec: PCWSTR(motif.as_ptr()) }];
        let _ = d.SetFileTypes(&filtres);
        if let Ok(o) = d.GetOptions() {
            let _ = d.SetOptions(o | FOS_FILEMUSTEXIST);
        }
        d.Show(None).ok()?;
        let it = d.GetResult().ok()?;
        nom_element(&it, SIGDN_FILESYSPATH)
    }
}

fn chaine_version(bloc: &[u8], chemin: &str) -> Option<String> {
    unsafe {
        let mut p: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut n = 0u32;
        let h = HSTRING::from(chemin);
        if !VerQueryValueW(bloc.as_ptr() as *const _, &h, &mut p, &mut n).as_bool() || p.is_null() || n == 0 {
            return None;
        }
        let s = std::slice::from_raw_parts(p as *const u16, n as usize);
        let fin = s.iter().position(|c| *c == 0).unwrap_or(s.len());
        Some(String::from_utf16_lossy(&s[..fin]).trim().to_string())
    }
}

/// ProductName et FileDescription d'un programme.
pub fn infos_programme(exe: &str) -> (String, String) {
    unsafe {
        let h = HSTRING::from(exe);
        let taille = GetFileVersionInfoSizeW(&h, None);
        if taille == 0 {
            return Default::default();
        }
        let mut bloc = vec![0u8; taille as usize];
        if GetFileVersionInfoW(&h, None, taille, bloc.as_mut_ptr() as *mut _).is_err() {
            return Default::default();
        }
        let mut p: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut n = 0u32;
        let t = HSTRING::from("\\VarFileInfo\\Translation");
        let mut langues = Vec::new();
        if VerQueryValueW(bloc.as_ptr() as *const _, &t, &mut p, &mut n).as_bool() && !p.is_null() && n >= 4 {
            let s = std::slice::from_raw_parts(p as *const u16, (n / 2) as usize);
            for c in s.chunks(2) {
                if c.len() == 2 {
                    langues.push(format!("{:04x}{:04x}", c[0], c[1]));
                }
            }
        }
        langues.push("040904b0".into());
        langues.push("040904e4".into());
        let lire = |champ: &str| {
            langues.iter().find_map(|l| chaine_version(&bloc, &format!("\\StringFileInfo\\{l}\\{champ}")))
        };
        (lire("ProductName").unwrap_or_default(), lire("FileDescription").unwrap_or_default())
    }
}
