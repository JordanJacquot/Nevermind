//! Effacer les données sans jamais suivre un lien (audit du 08/11, M4 : le désinstalleur tourne en administrateur, et
//! le dossier des données appartient au compte, qui peut y poser une jonction à tout moment).
//!
//! Chaque fichier et chaque dossier est ouvert sans suivre un lien (FILE_FLAG_OPEN_REPARSE_POINT), puis Windows dit où
//! il est vraiment (GetFinalPathNameByHandleW) : s'il n'est pas exactement là où il devait être (un dossier parent a été
//! remplacé par une jonction, même pendant le travail), il n'est pas touché. Ce qui est vérifié est ensuite lu et effacé
//! par cette même poignée, jamais rouvert par son chemin. Un lien (jonction, lien symbolique) est effacé seul, ce qu'il
//! vise n'est jamais lu.
//!
//! Lot C du second audit (M-A et M1) : en administrateur, plus rien n'est lu ni écrit par un simple chemin dans un
//! dossier du compte (données, bureau, menu Démarrer). Le dossier est ouvert et vérifié comme plus haut
//! ([`ouvrir_donnees`]) ; chaque fichier est ensuite ouvert PAR RAPPORT À LA POIGNÉE de ce dossier (NtCreateFile avec
//! RootDirectory) : son nom ne traverse aucun autre dossier, un lien posé à ce nom est ouvert lui-même puis refusé, un
//! fichier qui a un autre nom ailleurs (lien dur) aussi. Il est lu, complété ([`Donnees::ajouter`]), écrit à côté puis
//! renommé à sa place ([`Donnees::remplacer`]), ou effacé ([`Raccourci::effacer`]) par cette même poignée.

use std::io;
use std::path::Path;
use windows::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows::Wdk::Storage::FileSystem::{
    FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_IF, FILE_OPEN_REPARSE_POINT,
    FILE_RENAME_INFORMATION, FILE_SYNCHRONOUS_IO_NONALERT, FileRenameInformation, NTCREATEFILE_CREATE_DISPOSITION,
    NtCreateFile, NtSetInformationFile,
};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_NO_MORE_FILES, HANDLE, NTSTATUS, OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError, UNICODE_STRING,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, DELETE, FILE_ACCESS_RIGHTS, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_APPEND_DATA,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_ATTRIBUTE_TAG_INFO, FILE_BASIC_INFO, FILE_DISPOSITION_FLAG_DELETE,
    FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO,
    FILE_DISPOSITION_INFO_EX, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FULL_DIR_INFO,
    FILE_LIST_DIRECTORY, FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
    FileAttributeTagInfo, FileBasicInfo, FileDispositionInfo, FileDispositionInfoEx, FileFullDirectoryInfo,
    FileFullDirectoryRestartInfo, FileStandardInfo, FlushFileBuffers, GETFINALPATHNAMEBYHANDLE_FLAGS,
    GetFileInformationByHandleEx, GetFinalPathNameByHandleW, GetLongPathNameW, OPEN_EXISTING, ReadFile, SYNCHRONIZE,
    SetFileInformationByHandle, VOLUME_NAME_DOS, WriteFile,
};
use windows::Win32::System::IO::IO_STATUS_BLOCK;
use windows::core::{HSTRING, PWSTR};

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

/// L'étiquette des fichiers « dans le nuage » (OneDrive avec « Fichiers à la demande », et tout autre service de
/// synchronisation qui passe par Windows) : IO_REPARSE_TAG_CLOUD, et ses 16 variantes 0x9000_X01A
const ETIQUETTE_NUAGE: u32 = 0x9000_001A;
const MASQUE_NUAGE: u32 = 0xFFFF_0FFF;

/// Les attributs tels que ce module les juge (restes du 10/11, OneDrive « nuage ») : un fichier ou un dossier gardé
/// par un service de synchronisation porte un point d'analyse à l'étiquette « nuage ». Ce n'est pas un lien : il ne
/// mène nulle part ailleurs, il est bien là où il est (le chemin final est vérifié à part). Il est donc vu comme un
/// fichier ou un dossier ordinaire. Avant, en administrateur, un Bureau dans OneDrive pouvait passer pour un lien, et
/// le raccourci de Nevermind y restait. Une jonction, un lien symbolique ou toute autre étiquette restent des liens.
pub(crate) fn attributs_juges(attributs: u32, etiquette: u32) -> u32 {
    if attributs & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 && etiquette & MASQUE_NUAGE == ETIQUETTE_NUAGE {
        attributs & !FILE_ATTRIBUTE_REPARSE_POINT.0
    } else {
        attributs
    }
}

/// Les attributs, lus par la poignée avec l'étiquette du point d'analyse ([`attributs_juges`])
fn attributs(h: HANDLE) -> io::Result<u32> {
    let mut i = FILE_ATTRIBUTE_TAG_INFO::default();
    // SAFETY : structure de la bonne taille
    unsafe {
        GetFileInformationByHandleEx(
            h,
            FileAttributeTagInfo,
            (&mut i as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
        .map_err(erreur)?;
    }
    Ok(attributs_juges(i.FileAttributes, i.ReparseTag))
}

/// Ouvre `chemin` sans suivre de lien, et vérifie qu'il est bien là où on l'attend (`attendu`)
fn ouvrir(chemin: &str, attendu: &str) -> io::Result<Ouvert> {
    ouvrir_avec(chemin, attendu, DELETE.0 | FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0 | FILE_WRITE_ATTRIBUTES.0)
}

/// [`ouvrir`], avec ces droits
fn ouvrir_avec(chemin: &str, attendu: &str, acces: u32) -> io::Result<Ouvert> {
    let acces = acces | SYNCHRONIZE.0;
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
    match ouvrir(&attendu, &attendu) {
        // (un dossier qu'un autre programme tient sans permettre de l'effacer, le bureau par l'Explorateur par
        //  exemple : ouvert sans le droit de l'effacer, le reste se fait pareil ; lot C)
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(32) => {
            ouvrir_avec(&attendu, &attendu, FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0).map(Some)
        }
        r => r.map(Some),
    }
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

// ---------------------------------------------------------------------------------------------------------------------
// Lot C : un fichier du dossier vérifié, ouvert par rapport à la poignée du dossier, jamais par son chemin
// ---------------------------------------------------------------------------------------------------------------------

/// Le texte d'erreur quand un fichier est un lien (ou a un autre nom ailleurs : lien dur)
pub const FICHIER_LIEN: &str = "le fichier est un lien, il n'est pas touché";

/// Au plus tant d'octets lus d'un fichier des données (bien plus que ce que Nevermind y écrit)
const TAILLE_MAX_LUE: usize = 64 * 1024 * 1024;

/// Un seul nom de fichier : ni « \ », ni « / », ni « : » (pas de flux caché), ni « . » ou « .. », ni caractère de
/// contrôle. Ainsi le nom ne peut désigner que quelque chose de ce dossier.
fn nom_simple(nom: &str) -> io::Result<()> {
    if nom.is_empty()
        || nom == "."
        || nom == ".."
        || nom.contains(['\\', '/', ':'])
        || nom.chars().any(|c| (c as u32) < 32)
        || nom.encode_utf16().count() > 255
    {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "nom de fichier refusé"));
    }
    Ok(())
}

/// Le nombre de noms de ce fichier (plus d'un : un lien dur, posé peut-être vers un fichier qui n'est pas au compte)
fn noms(h: HANDLE) -> io::Result<u32> {
    let mut i = FILE_STANDARD_INFO::default();
    // SAFETY : structure de la bonne taille
    unsafe {
        GetFileInformationByHandleEx(
            h,
            FileStandardInfo,
            (&mut i as *mut FILE_STANDARD_INFO).cast(),
            size_of::<FILE_STANDARD_INFO>() as u32,
        )
        .map_err(erreur)?;
    }
    Ok(i.NumberOfLinks)
}

/// Ouvre le fichier `nom` du dossier ouvert `d`, par rapport à sa poignée (NtCreateFile avec RootDirectory : le dossier
/// est celui qui a été vérifié, même s'il a été remplacé depuis par une jonction). Jamais un dossier ; un lien posé à
/// ce nom est ouvert lui-même (FILE_OPEN_REPARSE_POINT), puis refusé, comme un fichier qui a un autre nom ailleurs.
fn ouvrir_dans(d: &Ouvert, nom: &str, acces: u32, disposition: NTCREATEFILE_CREATE_DISPOSITION) -> io::Result<Ouvert> {
    let h = nt_ouvrir_dans(d, nom, acces, disposition, FILE_NON_DIRECTORY_FILE)?;
    let mut o = Ouvert { h, chemin: dans(d, nom), attributs: 0 };
    o.attributs = attributs(o.h)?;
    if o.lien() || noms(o.h)? > 1 {
        return Err(io::Error::other(FICHIER_LIEN));
    }
    Ok(o)
}

/// NtCreateFile de `nom` par rapport à la poignée du dossier ouvert `d` (jamais un lien suivi), avec ces options en plus
fn nt_ouvrir_dans(
    d: &Ouvert,
    nom: &str,
    acces: u32,
    disposition: NTCREATEFILE_CREATE_DISPOSITION,
    options: windows::Wdk::Storage::FileSystem::NTCREATEFILE_CREATE_OPTIONS,
) -> io::Result<HANDLE> {
    nom_simple(nom)?;
    let mut large: Vec<u16> = nom.encode_utf16().collect();
    let octets = (large.len() * 2) as u16;
    let texte = UNICODE_STRING { Length: octets, MaximumLength: octets, Buffer: PWSTR(large.as_mut_ptr()) };
    let objet = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: d.h,
        ObjectName: &texte,
        Attributes: OBJ_CASE_INSENSITIVE,
        ..Default::default()
    };
    let mut h = HANDLE::default();
    let mut io_bloc = IO_STATUS_BLOCK::default();
    // SAFETY : structures remplies ici, vivantes pendant l'appel (le nom aussi) ; la poignée rendue est refermée par
    // Ouvert
    let st = unsafe {
        NtCreateFile(
            &mut h,
            FILE_ACCESS_RIGHTS(acces | FILE_READ_ATTRIBUTES.0 | SYNCHRONIZE.0),
            &objet,
            &mut io_bloc,
            None,
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            disposition,
            options | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
            None,
            0,
        )
    };
    ntstatus(st)?;
    Ok(h)
}

/// Tout le contenu, lu par la poignée
fn lire_tout(o: &Ouvert) -> io::Result<Vec<u8>> {
    let mut tout = Vec::new();
    let mut b = vec![0u8; 64 * 1024];
    loop {
        let mut n = 0u32;
        // SAFETY : tampon et taille cohérents, poignée ouverte en lecture (synchrone)
        unsafe { ReadFile(o.h, Some(&mut b), Some(&mut n), None) }.map_err(erreur)?;
        if n == 0 {
            return Ok(tout);
        }
        tout.extend_from_slice(&b[..n as usize]);
        if tout.len() > TAILLE_MAX_LUE {
            return Err(io::Error::other("fichier trop grand"));
        }
    }
}

/// Écrit tout par la poignée
fn ecrire_tout(o: &Ouvert, mut octets: &[u8]) -> io::Result<()> {
    while !octets.is_empty() {
        let mut n = 0u32;
        let morceau = &octets[..octets.len().min(1 << 20)];
        // SAFETY : tampon et taille cohérents, poignée ouverte en écriture (synchrone)
        unsafe { WriteFile(o.h, Some(morceau), Some(&mut n), None) }.map_err(erreur)?;
        if n == 0 {
            return Err(io::Error::from(io::ErrorKind::WriteZero));
        }
        octets = &octets[n as usize..];
    }
    Ok(())
}

/// Renomme ce qu'ouvre la poignée en `nom`, dans le dossier ouvert `d` (le sien : un nom simple, rapporté à la poignée
/// du dossier, il ne peut pas partir ailleurs). Ce qui porte déjà ce nom est remplacé, un lien compris (le lien
/// lui-même, jamais ce qu'il vise).
fn renommer(o: &Ouvert, d: &Ouvert, nom: &str) -> io::Result<()> {
    nom_simple(nom)?;
    let large: Vec<u16> = nom.encode_utf16().collect();
    // (directement à Windows, NtSetInformationFile : SetFileInformationByHandle, lui, rapporte un nom simple au dossier
    //  courant du programme, jamais à celui du fichier)
    let taille = size_of::<FILE_RENAME_INFORMATION>() + large.len() * 2;
    let mut b = vec![0u64; taille.div_ceil(8)];
    let p = b.as_mut_ptr() as *mut FILE_RENAME_INFORMATION;
    let mut io_bloc = IO_STATUS_BLOCK::default();
    // SAFETY : tampon aligné, assez grand pour la structure et le nom ; RootDirectory : la poignée du dossier, ouverte
    // tant que dure l'appel
    let st = unsafe {
        (*p).Anonymous.ReplaceIfExists = true;
        (*p).RootDirectory = d.h;
        (*p).FileNameLength = (large.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(large.as_ptr(), std::ptr::addr_of_mut!((*p).FileName).cast::<u16>(), large.len());
        NtSetInformationFile(o.h, &mut io_bloc, p as *const _, taille as u32, FileRenameInformation)
    };
    ntstatus(st)
}

/// Un code de retour de Windows (NTSTATUS) en erreur d'entrée-sortie
fn ntstatus(st: NTSTATUS) -> io::Result<()> {
    if st.is_ok() {
        return Ok(());
    }
    // SAFETY : simple conversion du code
    let code = unsafe { RtlNtStatusToDosError(st) };
    Err(io::Error::from_raw_os_error(code as i32))
}

/// Un raccourci (.lnk) ouvert par rapport à son dossier vérifié : son contenu, lu par sa poignée, qui sert aussi à
/// l'effacer (jamais rouvert par son chemin)
pub struct Raccourci {
    o: Ouvert,
    /// Le contenu du fichier
    pub octets: Vec<u8>,
}

impl Raccourci {
    /// Efface ce raccourci, par sa poignée
    pub fn effacer(self) -> io::Result<()> {
        effacer(&self.o)
    }
}

impl Donnees {
    /// Le contenu de ce fichier du dossier ; `None` s'il n'existe pas. Un lien est refusé (jamais suivi).
    pub fn lire(&self, nom: &str) -> io::Result<Option<Vec<u8>>> {
        match ouvrir_dans(&self.0, nom, FILE_READ_DATA.0, FILE_OPEN) {
            Ok(o) => lire_tout(&o).map(Some),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Ajoute ces octets à la fin de ce fichier du dossier (créé s'il n'existe pas). Un lien est refusé.
    pub fn ajouter(&self, nom: &str, octets: &[u8]) -> io::Result<()> {
        // (seulement le droit d'ajouter : Windows écrit toujours à la fin)
        let o = ouvrir_dans(&self.0, nom, FILE_APPEND_DATA.0, FILE_OPEN_IF)?;
        ecrire_tout(&o, octets)
    }

    /// Remplace ce fichier du dossier : écrit d'abord à côté (`<nom>.tmp`, créé neuf), puis renommé à sa place. Un lien
    /// posé à la place du fichier est remplacé lui-même, ce qu'il vise n'est pas touché.
    pub fn remplacer(&self, nom: &str, octets: &[u8]) -> io::Result<()> {
        let tmp = format!("{nom}.tmp");
        match self.effacer_fichier(&tmp) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        let o = ouvrir_dans(&self.0, &tmp, FILE_WRITE_DATA.0 | DELETE.0, FILE_CREATE)?;
        let r = ecrire_tout(&o, octets)
            // SAFETY : poignée ouverte en écriture
            .and_then(|()| unsafe { FlushFileBuffers(o.h) }.map_err(erreur))
            .and_then(|()| renommer(&o, &self.0, nom));
        if r.is_err() {
            let _ = effacer(&o);
        }
        r
    }

    /// Où est ce dossier (vérifié à l'ouverture)
    pub fn chemin(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.0.chemin)
    }

    /// Lot H : un dossier neuf de ce nom dans ce dossier, créé par rapport à sa poignée (il ne doit pas exister : rien
    /// de déjà là n'est repris), rendu ouvert
    pub fn creer_dossier(&self, nom: &str) -> io::Result<Donnees> {
        let h = nt_ouvrir_dans(
            &self.0,
            nom,
            DELETE.0 | FILE_LIST_DIRECTORY.0 | FILE_ADD_FILE.0 | FILE_ADD_SUBDIRECTORY.0 | FILE_WRITE_ATTRIBUTES.0,
            FILE_CREATE,
            FILE_DIRECTORY_FILE,
        )?;
        let mut o = Ouvert { h, chemin: String::new(), attributs: 0 };
        o.chemin = chemin_final(o.h)?;
        o.attributs = attributs(o.h)?;
        if o.lien() || !o.dossier() {
            return Err(io::Error::other(PAR_UN_LIEN));
        }
        Ok(Donnees(o))
    }

    /// Lot H : déplace `source` (un fichier ou un dossier, sur le même disque) dans ce dossier, sous le nom `nom`.
    /// `source` est ouvert sans suivre de lien et vérifié (il est bien là où il doit être, aucun dossier de son chemin
    /// n'est une jonction), puis renommé par sa poignée : un lien posé à sa place est déplacé lui-même, ce qu'il vise
    /// n'est jamais touché. Un programme en cours (ce désinstalleur) peut être déplacé ainsi.
    pub fn deplacer_ici(&self, source: &Path, nom: &str) -> io::Result<()> {
        let Some(attendu) = chemin_long(source) else {
            return Err(match std::fs::symlink_metadata(source) {
                Err(e) => e,
                Ok(_) => io::Error::other(PAR_UN_LIEN),
            });
        };
        let o = ouvrir_avec(&attendu, &attendu, DELETE.0 | FILE_READ_ATTRIBUTES.0)?;
        renommer(&o, &self.0, nom)
    }

    /// Ce raccourci du dossier (`None` s'il n'existe pas), ouvert pour être lu puis peut-être effacé. Un lien est refusé.
    pub fn raccourci(&self, nom: &str) -> io::Result<Option<Raccourci>> {
        match ouvrir_dans(&self.0, nom, FILE_READ_DATA.0 | DELETE.0 | FILE_WRITE_ATTRIBUTES.0, FILE_OPEN) {
            Ok(o) => {
                let octets = lire_tout(&o)?;
                Ok(Some(Raccourci { o, octets }))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }
}
