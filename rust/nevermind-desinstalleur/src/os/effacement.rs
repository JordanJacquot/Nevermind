//! Effacer les données sans jamais suivre un lien (audit du 08/11, M4 : le désinstalleur tourne en administrateur, et
//! le dossier des données appartient au compte, qui peut y poser une jonction à tout moment).
//!
//! Chaque fichier et chaque dossier est ouvert sans suivre un lien (FILE_FLAG_OPEN_REPARSE_POINT), puis Windows dit où
//! il est vraiment (GetFinalPathNameByHandleW) : s'il n'est pas exactement là où il devait être (un dossier parent a été
//! remplacé par une jonction, même pendant le travail), il n'est pas touché. Ce qui est vérifié est ensuite lu et effacé
//! par cette même poignée, jamais rouvert par son chemin. Un lien (jonction, lien symbolique) est effacé seul, ce qu'il
//! vise n'est jamais lu.

use std::io;
use std::path::Path;
use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, DELETE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_BASIC_INFO, FILE_DISPOSITION_FLAG_DELETE,
    FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO,
    FILE_DISPOSITION_INFO_EX, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FULL_DIR_INFO,
    FILE_LIST_DIRECTORY, FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, FileBasicInfo, FileDispositionInfo, FileDispositionInfoEx,
    FileFullDirectoryInfo, FileFullDirectoryRestartInfo, GETFINALPATHNAMEBYHANDLE_FLAGS, GetFileInformationByHandleEx,
    GetFinalPathNameByHandleW, GetLongPathNameW, OPEN_EXISTING, SYNCHRONIZE, SetFileInformationByHandle,
    VOLUME_NAME_DOS,
};
use windows::core::HSTRING;

/// Le texte d'erreur quand un chemin passe par un lien
pub const PAR_UN_LIEN: &str = "le dossier passe par un lien, il n'est pas touché";

/// Une erreur de Windows en erreur d'entrée-sortie (son message habituel)
fn erreur(e: windows::core::Error) -> io::Error {
    let c = e.code().0 as u32;
    if c & 0xFFFF_0000 == 0x8007_0000 {
        io::Error::from_raw_os_error((c & 0xFFFF) as i32)
    } else {
        io::Error::other(e.message())
    }
}

/// Une poignée ouverte ici, refermée à la fin
struct Ouvert {
    h: HANDLE,
    /// Où il est (vérifié), sans « \\?\ »
    chemin: String,
    attributs: u32,
}

impl Drop for Ouvert {
    fn drop(&mut self) {
        // SAFETY : poignée ouverte par ce module, refermée une seule fois
        unsafe {
            let _ = CloseHandle(self.h);
        }
    }
}

impl Ouvert {
    fn lien(&self) -> bool {
        self.attributs & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
    }
    fn dossier(&self) -> bool {
        self.attributs & FILE_ATTRIBUTE_DIRECTORY.0 != 0
    }
}

/// Le chemin tel que Windows l'écrit en entier (noms courts « RUNNER~1 » rallongés), sans « \ » final ; `None` s'il
/// n'existe pas. (GetLongPathNameW ne suit pas les jonctions : il ne fait que rallonger les noms.)
fn chemin_long(p: &Path) -> Option<String> {
    let h = HSTRING::from(p.as_os_str());
    let mut b = vec![0u16; 32768];
    // SAFETY : tampon et taille cohérents
    let n = unsafe { GetLongPathNameW(&h, Some(&mut b)) } as usize;
    if n == 0 || n > b.len() {
        return None;
    }
    Some(String::from_utf16_lossy(&b[..n]).trim_end_matches('\\').to_string())
}

/// Où est vraiment ce qu'ouvre cette poignée, sans « \\?\ »
fn chemin_final(h: HANDLE) -> io::Result<String> {
    let mut b = vec![0u16; 32768];
    // SAFETY : tampon et taille cohérents
    let n = unsafe {
        GetFinalPathNameByHandleW(h, &mut b, GETFINALPATHNAMEBYHANDLE_FLAGS(FILE_NAME_NORMALIZED.0 | VOLUME_NAME_DOS.0))
    } as usize;
    if n == 0 || n > b.len() {
        return Err(io::Error::last_os_error());
    }
    let s = String::from_utf16_lossy(&b[..n]);
    Ok(if let Some(r) = s.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{r}")
    } else {
        s.strip_prefix("\\\\?\\").unwrap_or(&s).to_string()
    })
}

/// Les attributs, lus par la poignée
fn attributs(h: HANDLE) -> io::Result<u32> {
    let mut i = FILE_BASIC_INFO::default();
    // SAFETY : structure de la bonne taille
    unsafe {
        GetFileInformationByHandleEx(
            h,
            FileBasicInfo,
            (&mut i as *mut FILE_BASIC_INFO).cast(),
            size_of::<FILE_BASIC_INFO>() as u32,
        )
        .map_err(erreur)?;
    }
    Ok(i.FileAttributes)
}

/// Ouvre `chemin` sans suivre de lien, et vérifie qu'il est bien là où on l'attend (`attendu`)
fn ouvrir(chemin: &str, attendu: &str) -> io::Result<Ouvert> {
    let acces = DELETE.0 | FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0 | FILE_WRITE_ATTRIBUTES.0 | SYNCHRONIZE.0;
    // SAFETY : ouverture simple ; la poignée est refermée par Ouvert
    let h = unsafe {
        CreateFileW(
            &HSTRING::from(chemin),
            acces,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    }
    .map_err(erreur)?;
    let mut o = Ouvert { h, chemin: String::new(), attributs: 0 };
    o.chemin = chemin_final(o.h)?;
    if !suite_commun::chemins::egal(&o.chemin, attendu) {
        return Err(io::Error::other(PAR_UN_LIEN));
    }
    o.attributs = attributs(o.h)?;
    Ok(o)
}

/// Ouvre le dossier des données (vérifié jusqu'à sa racine) ; `Ok(None)` s'il n'existe pas
fn ouvrir_racine(d: &Path) -> io::Result<Option<Ouvert>> {
    let Some(attendu) = chemin_long(d) else {
        return match std::fs::symlink_metadata(d) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
            Ok(_) => Err(io::Error::other(PAR_UN_LIEN)),
        };
    };
    ouvrir(&attendu, &attendu).map(Some)
}

/// Les noms et attributs de ce que contient le dossier, lus par sa poignée
fn entrees(d: &Ouvert) -> io::Result<Vec<(String, u32)>> {
    let mut l = Vec::new();
    let mut b = vec![0u64; 8192]; // 64 Ko, alignés
    let mut premier = true;
    loop {
        let classe = if premier { FileFullDirectoryRestartInfo } else { FileFullDirectoryInfo };
        premier = false;
        // SAFETY : tampon aligné, de la taille donnée
        let r = unsafe { GetFileInformationByHandleEx(d.h, classe, b.as_mut_ptr().cast(), (b.len() * 8) as u32) };
        match r {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
            Err(e) => return Err(erreur(e)),
        }
        let octets = b.as_ptr() as *const u8;
        let mut pos = 0usize;
        loop {
            // SAFETY : Windows a écrit une suite de FILE_FULL_DIR_INFO dans le tampon ; chaque position vient de
            // NextEntryOffset et reste dans le tampon ; lectures non alignées
            let (suivant, attributs, nom) = unsafe {
                let p = octets.add(pos) as *const FILE_FULL_DIR_INFO;
                let i = std::ptr::read_unaligned(p);
                let debut = std::ptr::addr_of!((*p).FileName) as *const u8;
                let n = (i.FileNameLength / 2) as usize;
                let mut nom = vec![0u16; n];
                std::ptr::copy_nonoverlapping(debut, nom.as_mut_ptr().cast::<u8>(), n * 2);
                (i.NextEntryOffset as usize, i.FileAttributes, String::from_utf16_lossy(&nom))
            };
            if nom != "." && nom != ".." {
                l.push((nom, attributs));
            }
            if suivant == 0 || pos + suivant >= b.len() * 8 {
                break;
            }
            pos += suivant;
        }
    }
    Ok(l)
}

/// Efface ce qu'ouvre la poignée (un fichier, un lien seul, ou un dossier vide), même en lecture seule
fn effacer(o: &Ouvert) -> io::Result<()> {
    let ex = FILE_DISPOSITION_INFO_EX {
        Flags: windows::Win32::Storage::FileSystem::FILE_DISPOSITION_INFO_EX_FLAGS(
            FILE_DISPOSITION_FLAG_DELETE.0
                | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0
                | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
        ),
    };
    // SAFETY : structure de la bonne taille, poignée ouverte avec le droit d'effacer
    let r = unsafe {
        SetFileInformationByHandle(
            o.h,
            FileDispositionInfoEx,
            (&ex as *const FILE_DISPOSITION_INFO_EX).cast(),
            size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
        )
    };
    if r.is_ok() {
        return Ok(());
    }
    // (un disque qui ne connaît pas cette façon, FAT par exemple : la lecture seule d'abord retirée, puis l'effacement
    // habituel, fait quand la poignée se ferme)
    if o.attributs & FILE_ATTRIBUTE_READONLY.0 != 0 {
        let reste = o.attributs & !FILE_ATTRIBUTE_READONLY.0;
        let basic = FILE_BASIC_INFO {
            FileAttributes: if reste == 0 { FILE_ATTRIBUTE_NORMAL.0 } else { reste },
            ..Default::default()
        };
        // SAFETY : structure de la bonne taille ; des dates à 0 ne sont pas changées
        unsafe {
            let _ = SetFileInformationByHandle(
                o.h,
                FileBasicInfo,
                (&basic as *const FILE_BASIC_INFO).cast(),
                size_of::<FILE_BASIC_INFO>() as u32,
            );
        }
    }
    let d = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY : structure de la bonne taille
    unsafe {
        SetFileInformationByHandle(
            o.h,
            FileDispositionInfo,
            (&d as *const FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .map_err(erreur)
}

/// Le chemin de `nom` dans ce dossier ouvert
fn dans(d: &Ouvert, nom: &str) -> String {
    format!("{}\\{nom}", d.chemin.trim_end_matches('\\'))
}

/// Note la première erreur
fn noter(premiere: &mut Option<io::Error>, e: io::Error) {
    if premiere.is_none() {
        *premiere = Some(e);
    }
}

/// Vide ce dossier ouvert (sous-dossiers compris, liens retirés seuls), puis l'efface. Continue malgré une erreur ;
/// la première est notée.
fn vider_et_effacer(d: Ouvert, premiere: &mut Option<io::Error>) {
    match entrees(&d) {
        Ok(l) => {
            for (nom, _) in l {
                let c = dans(&d, &nom);
                match ouvrir(&c, &c) {
                    Ok(o) if !o.lien() && o.dossier() => {
                        #[cfg(test)]
                        crate::donnees::tests::avant_lecture(Path::new(&o.chemin));
                        vider_et_effacer(o, premiere);
                    }
                    Ok(o) => {
                        if let Err(e) = effacer(&o) {
                            noter(premiere, e);
                        }
                    }
                    Err(e) => noter(premiere, e),
                }
            }
        }
        Err(e) => noter(premiere, e),
    }
    if let Err(e) = effacer(&d) {
        noter(premiere, e);
    }
}

/// `supprimer_dossier` : le dossier et tout ce qu'il contient, sans jamais suivre un lien (un lien est retiré seul).
/// Rend la première erreur.
pub fn supprimer_dossier(d: &Path) -> io::Result<()> {
    let Some(o) = ouvrir_racine(d)? else { return Err(io::Error::from(io::ErrorKind::NotFound)) };
    if o.lien() || !o.dossier() {
        return effacer(&o);
    }
    let mut premiere = None;
    vider_et_effacer(o, &mut premiere);
    match premiere {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Le dossier des données, ouvert et vérifié
pub struct Donnees(Ouvert);

/// Ce qu'est le dossier des données
pub enum EtatDonnees {
    /// Absent (ou ce n'est pas un dossier) : rien à faire
    Absent,
    /// Un lien (jonction, lien symbolique) : jamais suivi
    Lien,
    Dossier(Donnees),
}

/// Ouvre le dossier des données, vérifié jusqu'à sa racine
pub fn ouvrir_donnees(d: &Path) -> io::Result<EtatDonnees> {
    Ok(match ouvrir_racine(d)? {
        None => EtatDonnees::Absent,
        Some(o) if o.lien() => EtatDonnees::Lien,
        Some(o) if !o.dossier() => EtatDonnees::Absent,
        Some(o) => EtatDonnees::Dossier(Donnees(o)),
    })
}

impl Donnees {
    /// Les noms de ce que contient le dossier, sauf les dossiers (et les jonctions)
    pub fn fichiers(&self) -> io::Result<Vec<String>> {
        Ok(entrees(&self.0)?.into_iter().filter(|(_, a)| a & FILE_ATTRIBUTE_DIRECTORY.0 == 0).map(|(n, _)| n).collect())
    }

    /// Efface ce fichier du dossier (un lien seul : ce qu'il vise n'est pas touché)
    pub fn effacer_fichier(&self, nom: &str) -> io::Result<()> {
        let c = dans(&self.0, nom);
        let o = ouvrir(&c, &c)?;
        if o.dossier() && !o.lien() {
            return Err(io::Error::from(io::ErrorKind::IsADirectory));
        }
        effacer(&o)
    }

    /// Le dossier est vide : il est effacé
    pub fn effacer_si_vide(self) {
        if entrees(&self.0).is_ok_and(|l| l.is_empty()) {
            let _ = effacer(&self.0);
        }
    }
}
