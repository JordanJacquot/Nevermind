//! Windows pour le désinstalleur (le seul module, avec le code repris de l'Aide, où le code « unsafe » est permis) :
//!
//! - [`SystemeWindows`] : le [`Systeme`] de la désinstallation (tâches planifiées par schtasks.exe, programmes, clé de
//!   l'Aide, raccourcis, la question et le message, la commande de fin) ;
//! - [`PcDesinstallation`] : le `Pc` du bloc pc (`PcWindows`) pour « Tout restaurer », avec les demandes de l'Aide
//!   faites sur place ([`AideSurPlace`]), les règles du pare-feu comptées et les cartes réseau retrouvées ;
//! - [`relancer_en_administrateur`] : la demande de Windows (une seule), comme Nevermind.ps1:330-350 ;
//! - lot C du second audit : en administrateur, les dossiers du compte (données, bureau, menu Démarrer) ne sont jamais
//!   lus ni écrits par un simple chemin ([`contexte_administrateur`], [`ecrire_donnees`], [`effacer_raccourci_de`],
//!   le module [`effacement`]) ; la partie sans droits efface ensuite les raccourcis du compte
//!   ([`retirer_raccourcis_du_compte`]).
//!
//! Tout programme lancé l'est sans fenêtre (CREATE_NO_WINDOW).

#![allow(unsafe_code)]

pub mod effacement;
pub mod fenetre;

use crate::choix::Choix;
use crate::sur_place::AideSurPlace;
use crate::travail::{self, Relue, Suppression, Systeme};
use nevermind_noyau::{Contexte, Permis};
use nevermind_pc::pc::{Pc, ResultatNettoyage, ValeurReg};
use nevermind_pc::systeme::PcWindows;
use nevermind_pc::temps::Moment;
use serde_json::Value;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::NetworkManagement::IpHelper::{ConvertInterfaceGuidToLuid, ConvertInterfaceLuidToIndex};
use windows::Win32::NetworkManagement::Ndis::NET_LUID_LH;
use windows::Win32::Security::TOKEN_QUERY;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile,
    IPersistStream, STGM_READ,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, INFINITE, OpenProcessToken, WaitForSingleObject,
};
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_Programs, GetUserProfileDirectoryW, IShellLinkW, KF_FLAG_DEFAULT_PATH,
    KF_FLAG_DONT_VERIFY, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHCreateMemStream, SHELLEXECUTEINFOW,
    SHGetKnownFolderPath, ShellExecuteExW, ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{GUID, HSTRING, Interface, PCWSTR, PWSTR};

/// CREATE_NO_WINDOW : aucun programme lancé n'ouvre de fenêtre
const SANS_FENETRE: u32 = 0x0800_0000;

/// Le dossier System32
fn system32() -> String {
    nevermind_noyau::windows::dossier_systeme().trim_end_matches('\\').to_string()
}

/// `%LOCALAPPDATA%` du compte, à son emplacement par défaut (lot H) : jamais là où le compte l'a redirigé.
///
/// Le compte peut rediriger son dossier Local AppData sans aucun droit (`HKCU\...\User Shell Folders`). En
/// administrateur, avec « Supprimer mes données », un vrai dossier à l'ancien nom des données de Nevermind, choisi
/// ainsi par un programme sans droits, serait effacé. Donc :
/// - `SHGetKnownFolderPath` avec `KF_FLAG_DEFAULT_PATH` : le chemin par défaut (`<profil>\AppData\Local`), qui ne lit pas
///   la redirection (`KF_FLAG_DONT_VERIFY` : il est rendu même s'il n'existe pas, rien n'est créé) ;
/// - et ce chemin doit être dans le profil du compte tel que Windows le note (`GetUserProfileDirectoryW`, lu dans
///   `HKLM\...\ProfileList`, que le compte ne peut pas changer) ; sinon (une variable `USERPROFILE` changée par le
///   compte, par exemple), c'est `<profil>\AppData\Local` qui est pris.
///
/// Le dossier lui-même est ensuite ouvert par une poignée vérifiée, qui refuse un lien ou une jonction sur son chemin
/// (`effacement::ouvrir_donnees`), comme les autres données.
pub fn localappdata() -> String {
    local_dans_le_profil(&dossier_par_defaut(&FOLDERID_LocalAppData), &profil_du_compte())
}

/// Le choix de [`localappdata`] : le chemin par défaut s'il est bien celui du profil, sinon `<profil>\AppData\Local`
fn local_dans_le_profil(defaut: &str, profil: &str) -> String {
    let profil = profil.trim_end_matches('\\');
    if profil.is_empty() {
        return defaut.to_string();
    }
    let attendu = format!("{profil}\\AppData\\Local");
    if suite_commun::chemins::egal(defaut.trim_end_matches('\\'), &attendu) { defaut.to_string() } else { attendu }
}

/// Un dossier connu à son emplacement par défaut, jamais redirigé (vide s'il n'est pas connu)
fn dossier_par_defaut(id: &GUID) -> String {
    // SAFETY : texte rendu par Windows, libéré avec CoTaskMemFree
    unsafe {
        match SHGetKnownFolderPath(id, KF_FLAG_DEFAULT_PATH | KF_FLAG_DONT_VERIFY, None) {
            Ok(p) => {
                let t = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as *const _));
                t
            }
            Err(_) => String::new(),
        }
    }
}

/// Le dossier du profil de ce compte, tel que Windows le note (`HKLM\...\ProfileList`) ; vide s'il est illisible
fn profil_du_compte() -> String {
    // SAFETY : poignée du jeton de ce programme, refermée ; tampon et taille cohérents
    unsafe {
        let mut jeton = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut jeton).is_err() {
            return String::new();
        }
        let _j = Poignee(jeton);
        let mut b = vec![0u16; 1024];
        let mut n = b.len() as u32;
        if GetUserProfileDirectoryW(jeton, Some(PWSTR(b.as_mut_ptr())), &mut n).is_err() {
            return String::new();
        }
        let fin = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        String::from_utf16_lossy(&b[..fin])
    }
}

/// schtasks.exe avec ces arguments (une tâche arrêtée ou retirée : ça change le PC, avec le permis du verrou des
/// gestes ; lot J, suite), sans fenêtre : son code de sortie (`None` s'il n'a pas pu se lancer)
fn schtasks(_p: &Permis, args: &[&str]) -> Option<i32> {
    Command::new(format!("{}\\schtasks.exe", system32()))
        .args(args)
        .creation_flags(SANS_FENETRE)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()
        .and_then(|s| s.code())
}

/// La tâche planifiée `nom` existe (`schtasks /Query`, une lecture : sans permis), sans fenêtre
fn tache_presente(nom: &str) -> bool {
    Command::new(format!("{}\\schtasks.exe", system32()))
        .args(["/Query", "/TN", nom])
        .creation_flags(SANS_FENETRE)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()
        .and_then(|s| s.code())
        == Some(0)
}

/// Retire cette tâche planifiée : son erreur, dite simplement
fn retirer(p: &Permis, tache: &str) -> Result<(), String> {
    match schtasks(p, &["/Delete", "/TN", tache, "/F"]) {
        Some(0) => Ok(()),
        Some(c) => Err(format!("la tâche « {tache} » n'a pas pu être retirée (schtasks a répondu {c})")),
        None => Err("schtasks.exe n'a pas pu se lancer".into()),
    }
}

/// La cible d'un raccourci (.lnk), vide s'il n'existe pas ou est illisible (`Get-ShortcutTarget`)
pub fn cible_raccourci(lnk: &Path) -> String {
    if !lnk.is_file() {
        return String::new();
    }
    // SAFETY : COM ouvert pour ce fil le temps de la lecture, objets du système, tampon de taille connue
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let r = (|| -> windows::core::Result<String> {
            let lien: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            let fichier: IPersistFile = lien.cast()?;
            fichier.Load(&HSTRING::from(lnk.as_os_str()), STGM_READ)?;
            let mut b = vec![0u16; 32768];
            lien.GetPath(&mut b, std::ptr::null_mut(), 0)?;
            let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
            Ok(String::from_utf16_lossy(&b[..n]))
        })();
        if init {
            CoUninitialize();
        }
        r.unwrap_or_default()
    }
}

/// La cible d'un raccourci lu en mémoire (son contenu, lu par la poignée qui servira à l'effacer : lot C), vide s'il
/// est illisible
pub fn cible_raccourci_octets(octets: &[u8]) -> String {
    // SAFETY : COM ouvert pour ce fil le temps de la lecture, objets du système, tampon de taille connue
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let r = (|| -> windows::core::Result<String> {
            let lien: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            let flux = SHCreateMemStream(Some(octets))
                .ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_OUTOFMEMORY))?;
            let p: IPersistStream = lien.cast()?;
            p.Load(&flux)?;
            let mut b = vec![0u16; 32768];
            lien.GetPath(&mut b, std::ptr::null_mut(), 0)?;
            let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
            Ok(String::from_utf16_lossy(&b[..n]))
        })();
        if init {
            CoUninitialize();
        }
        r.unwrap_or_default()
    }
}

/// Les programmes qui lancent la copie de ce dossier (`Get-AppExes` : raccourcis, tâche)
fn exes_de(racine: &Path) -> [String; 2] {
    let r = racine.to_string_lossy().trim_end_matches('\\').to_string();
    [format!("{r}\\Mindset.exe"), format!("{r}\\Nevermind.exe")]
}

/// Le dossier « Mindset » du menu Démarrer du compte
fn dossier_menu() -> PathBuf {
    PathBuf::from(nevermind_noyau::windows::dossier_connu(&FOLDERID_Programs)).join("Mindset")
}

/// Le nom des raccourcis de Nevermind (bureau, menu Démarrer)
const RACCOURCI: &str = "Nevermind.lnk";

/// Partie sans droits (lot C, M1), une fois la désinstallation finie : les raccourcis du bureau et du menu Démarrer qui
/// lancent cette copie, effacés par le compte lui-même (ils sont à lui : aucun droit n'est nécessaire, et un dossier
/// redirigé ou remplacé par une jonction ne lui fait rien effacer qu'il ne pourrait effacer seul). Ceux que la partie
/// administrateur n'a pas pu effacer sans suivre de lien partent ainsi.
pub fn retirer_raccourcis_du_compte(racine: &Path) {
    let exes = exes_de(racine);
    let lance = |l: &Path| {
        let t = cible_raccourci(l);
        !t.is_empty() && exes.iter().any(|e| suite_commun::chemins::egal(e, &t))
    };
    if let Some(b) = nevermind_noyau::windows::dossier_bureau() {
        let l = b.join(RACCOURCI);
        if lance(&l) {
            let _ = std::fs::remove_file(&l);
        }
    }
    let d = dossier_menu();
    let l = d.join(RACCOURCI);
    if lance(&l) {
        let _ = std::fs::remove_file(&l);
        if std::fs::read_dir(&d).is_ok_and(|mut r| r.next().is_none()) {
            let _ = std::fs::remove_dir(&d);
        }
    }
}

/// Une poignée de processus, refermée à la fin
struct Poignee(HANDLE);
impl Drop for Poignee {
    fn drop(&mut self) {
        // SAFETY : poignée ouverte par ce module
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// La demande de Windows (`Start-Process -Verb RunAs`) pour ce programme avec ces arguments, puis l'attente de sa fin :
/// son code de sortie. `Err` : refusée (ou impossible).
pub fn relancer_en_administrateur(arguments: &str) -> Result<u32, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dossier = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let verbe = HSTRING::from("runas");
    let fichier = HSTRING::from(exe.as_os_str());
    let args = HSTRING::from(arguments);
    let dir = HSTRING::from(dossier.as_os_str());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: PCWSTR(verbe.as_ptr()),
        lpFile: PCWSTR(fichier.as_ptr()),
        lpParameters: PCWSTR(args.as_ptr()),
        lpDirectory: PCWSTR(dir.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY : structure remplie, textes vivants pendant l'appel ; la poignée du processus est refermée
    unsafe {
        ShellExecuteExW(&mut info).map_err(|e| e.message())?;
        if info.hProcess.is_invalid() || info.hProcess.0.is_null() {
            return Ok(0);
        }
        let h = Poignee(info.hProcess);
        if WaitForSingleObject(h.0, INFINITE) != WAIT_OBJECT_0 {
            return Ok(1);
        }
        let mut code = 0u32;
        let _ = GetExitCodeProcess(h.0, &mut code);
        Ok(code)
    }
}

/// Le numéro d'une carte réseau d'après son identifiant (`Get-NetAdapter -IncludeHidden | Where InterfaceGuid`)
fn index_carte(guid: &str) -> Option<i64> {
    let g = GUID::try_from(guid.trim().trim_matches(['{', '}'])).ok()?;
    let mut luid = NET_LUID_LH::default();
    let mut index = 0u32;
    // SAFETY : structures de taille connue
    unsafe {
        if ConvertInterfaceGuidToLuid(&g, &mut luid).is_err() {
            return None;
        }
        if ConvertInterfaceLuidToIndex(&luid, &mut index).is_err() {
            return None;
        }
    }
    Some(i64::from(index))
}

// ---------------------------------------------------------------------------------------------------------------------
// Le Systeme de la désinstallation
// ---------------------------------------------------------------------------------------------------------------------

/// Le vrai Windows, pour la désinstallation de cette copie
pub struct SystemeWindows<'a> {
    pub ctx: &'a Contexte,
    pub aide: &'a AideSurPlace,
    /// Le dossier des programmes de cette copie
    pub racine: PathBuf,
    /// Piloté par Mindset (`-GarderDonnees` ou `-SupprimerDonnees`) : le message de fin n'est pas montré, il va au
    /// journal (le désinstalleur de Mindset attend la fin de celui-ci et montre son propre bilan ; DIVERGENCES.md)
    pub muet: bool,
    /// Lot H (M2) : les réglages relus après la fermeture de l'Aide et de Nevermind (`Some(None)` : relus, mais
    /// illisibles) ; `None` : pas encore relus. C'est la référence de [`ecrire_donnees`] à la fin.
    pub reglages_relus: Option<Option<serde_json::Map<String, Value>>>,
}

/// Le raccourci de Nevermind dans ce dossier du compte (bureau, dossier Mindset du menu Démarrer), s'il lance la copie
/// de `racine` (lot C, M1). Le dossier est ouvert et vérifié (pas un lien, et pas atteint par un lien : un dossier
/// redirigé ou remplacé par une jonction est refusé), le raccourci ouvert par rapport à lui, lu par sa poignée ; c'est
/// cette même poignée qui l'efface ensuite. Rend aussi le dossier ouvert (pour l'effacer s'il devient vide).
pub fn raccourci_de_cette_copie(dossier: &Path, racine: &Path) -> Option<(effacement::Donnees, effacement::Raccourci)> {
    let effacement::EtatDonnees::Dossier(d) = effacement::ouvrir_donnees(dossier).ok()? else { return None };
    let r = d.raccourci(RACCOURCI).ok()??;
    let t = cible_raccourci_octets(&r.octets);
    (!t.is_empty() && exes_de(racine).iter().any(|e| suite_commun::chemins::egal(e, &t))).then_some((d, r))
}

/// Efface le raccourci de Nevermind de ce dossier du compte s'il lance la copie de `racine`, par la poignée qui l'a lu
/// ([`raccourci_de_cette_copie`]) ; `vider` : le dossier aussi s'il devient vide (le dossier « Mindset » du menu).
/// Vrai si le raccourci est effacé.
pub fn effacer_raccourci_de(dossier: &Path, racine: &Path, vider: bool) -> bool {
    let Some((d, r)) = raccourci_de_cette_copie(dossier, racine) else { return false };
    if r.effacer().is_err() {
        return false;
    }
    if vider {
        d.effacer_si_vide();
    }
    true
}

impl Systeme for SystemeWindows<'_> {
    fn historique(&mut self) -> Vec<Value> {
        self.ctx.historique()
    }
    fn demarrage_auto(&mut self) -> bool {
        self.tache_existe(travail::TACHE_DEMARRAGE)
    }
    fn raccourci_bureau(&mut self) -> bool {
        nevermind_noyau::windows::dossier_bureau().is_some_and(|b| raccourci_de_cette_copie(&b, &self.racine).is_some())
    }
    fn question(&mut self, texte: &str) -> Option<Choix> {
        fenetre::question(texte)
    }
    fn message_fin(&mut self, texte: &str) {
        if !self.muet {
            fenetre::message_fin(texte);
        } else if self.ctx.donnees.is_dir() {
            // (données supprimées : rien n'est recréé pour une ligne de journal)
            let ligne: Vec<&str> = texte.split_whitespace().collect();
            self.ctx.journal(&format!("Désinstallation, message de fin : {}", ligne.join(" ")));
        }
    }
    fn journal(&mut self, ligne: &str) {
        // (données supprimées : rien n'est recréé pour une ligne de journal)
        if self.ctx.donnees.is_dir() {
            self.ctx.journal(ligne);
        }
    }
    fn arreter_tache_aide(&mut self, p: &Permis) {
        let _ = schtasks(p, &["/End", "/TN", &format!("{}{}", travail::TACHE_AIDE_DOSSIER, travail::TACHE_AIDE_NOM)]);
    }
    fn processus(&mut self) -> Vec<(u32, String)> {
        self.aide.processus()
    }
    fn fermer_processus(&mut self, _p: &Permis, pid: u32, chemin: &Path) {
        // (lot C : par une poignée gardée, son chemin relu par elle, comme l'Aide ; jamais rouvert par son numéro)
        let _ = self.aide.fermer(pid, &chemin.to_string_lossy());
    }
    fn retirer_demarrage(&mut self, p: &Permis) -> Result<(), String> {
        if !self.tache_existe(travail::TACHE_DEMARRAGE) {
            return Ok(());
        }
        retirer(p, travail::TACHE_DEMARRAGE)
    }
    fn tache_existe(&mut self, nom: &str) -> bool {
        tache_presente(nom)
    }
    fn retirer_tache(&mut self, p: &Permis, nom: &str) -> Result<(), String> {
        retirer(p, nom)
    }
    fn retirer_tache_aide(&mut self, p: &Permis) -> Result<(), String> {
        let t = format!("{}{}", travail::TACHE_AIDE_DOSSIER, travail::TACHE_AIDE_NOM);
        if !self.tache_existe(&t) {
            return Ok(());
        }
        retirer(p, &t)
    }
    fn effacer_cle_aide(&mut self, _p: &Permis) -> Result<(), String> {
        let c = "SOFTWARE\\Mindset\\Nevermind\\Aide";
        if windows_registry::LOCAL_MACHINE.open(c).is_err() {
            return Ok(());
        }
        windows_registry::LOCAL_MACHINE.remove_tree(c).map_err(|e| e.message())
    }
    // (lot C, M1 : en administrateur, un raccourci n'est jamais effacé par son chemin ; il est relu puis effacé par la
    //  même poignée, ouverte par rapport à son dossier vérifié. Lancé sans les droits, le désinstalleur repasse ensuite
    //  sous le compte : `retirer_raccourcis_du_compte`)
    fn effacer_raccourci_bureau(&mut self) {
        if let Some(b) = nevermind_noyau::windows::dossier_bureau() {
            effacer_raccourci_de(&b, &self.racine, false);
        }
    }
    fn retirer_raccourci_menu(&mut self) {
        effacer_raccourci_de(&dossier_menu(), &self.racine, true);
    }
    fn lancer_suppression(&mut self, commande: &str, suppression: &Suppression) {
        // Lot H : tout ce qui est à effacer est d'abord déplacé dans un dossier « à effacer » neuf (même disque : un
        // simple renommage, même pour ce programme en cours), et la commande de fin n'efface plus que lui. Une
        // réinstallation faite pendant ses 3 s d'attente n'est donc plus effacée par les noms de ses fichiers.
        // Nevermind.exe disparaît aussitôt de sa place (Mindset le voit tout de suite : DIVERGENCES.md, point 4).
        let (reste, notes) = mettre_de_cote(&self.racine, suppression);
        for n in &notes {
            self.journal(n);
        }
        // Ce qui n'a pas pu être déplacé : comme avant, effacé par son nom (tout de suite si possible, puis par la
        // commande de fin)
        let moi = std::env::current_exe().ok();
        for f in &reste.fichiers {
            if moi.as_ref().is_none_or(|m| !suite_commun::chemins::egal(&m.to_string_lossy(), &f.to_string_lossy())) {
                let _ = std::fs::remove_file(f);
            }
        }
        // (`commande` est le texte de la version PowerShell, celui des juges : le vrai lancement efface le dossier « à
        // effacer » et ce qui reste, avec PING.EXE par son chemin complet)
        let _ = commande;
        if let Err(e) = commande_fin_de(&reste).spawn() {
            self.ctx.journal(&format!("Désinstallation : la suppression des fichiers n'a pas pu se lancer ({e})"));
        }
    }
    fn relire_donnees(&mut self) -> Option<Relue> {
        // (lot H, M2 : comme au départ, sans suivre de lien ; le contexte reçoit ce qui est relu)
        self.reglages_relus = Some(relire_donnees(self.ctx, &self.ctx.donnees));
        let s = self.ctx.sauvegarde();
        Some(Relue {
            probleme: s.probleme.clone(),
            sauvegarde: nevermind_pc::corrections::Sauvegarde::depuis(&s.en_json()),
        })
    }
}

/// Le dossier « à effacer » de la commande de fin : neuf, dans le dossier des programmes, à un nom unique (le numéro de
/// ce programme et l'heure)
fn nom_a_effacer() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("a-effacer-{}-{t}", std::process::id())
}

/// Lot H : déplace les fichiers et dossiers à effacer dans un dossier « à effacer » neuf, créé dans le dossier des
/// programmes (`racine`) ouvert et vérifié ; chacun est ouvert sans suivre de lien, vérifié (là où il doit être, pas
/// atteint par une jonction), puis renommé par sa poignée (`effacement::Donnees::deplacer_ici`). Rend ce que la
/// commande de fin doit effacer : ce dossier, puis ce qui n'a pas pu être déplacé (par son nom, comme avant), puis les
/// dossiers vides. Et les lignes du journal qui disent ce qui n'a pas pu être déplacé.
fn mettre_de_cote(racine: &Path, s: &Suppression) -> (Suppression, Vec<String>) {
    let mut notes = Vec::new();
    let present = |p: &Path| std::fs::symlink_metadata(p).is_ok();
    let a_deplacer: Vec<&PathBuf> = s.fichiers.iter().chain(&s.dossiers).filter(|p| present(p)).collect();
    if a_deplacer.is_empty() {
        return (s.clone(), notes);
    }
    let dossier = match effacement::ouvrir_donnees(racine) {
        Ok(effacement::EtatDonnees::Dossier(d)) => d.creer_dossier(&nom_a_effacer()),
        Ok(_) => Err(std::io::Error::other(effacement::PAR_UN_LIEN)),
        Err(e) => Err(e),
    };
    let cible = match dossier {
        Ok(c) => c,
        Err(e) => {
            notes.push(format!(
                "Désinstallation : le dossier à effacer n'a pas pu être créé ({}) : les fichiers sont effacés par leur nom",
                crate::donnees::message_erreur(&e)
            ));
            return (s.clone(), notes);
        }
    };
    let mut reste = Suppression { fichiers: Vec::new(), dossiers: vec![cible.chemin()], vides: s.vides.clone() };
    for (i, p) in a_deplacer.into_iter().enumerate() {
        let nom = format!("{i}-{}", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        if let Err(e) = cible.deplacer_ici(p, &nom) {
            if e.kind() == std::io::ErrorKind::NotFound {
                continue;
            }
            notes.push(format!(
                "Désinstallation : {} n'a pas pu être mis de côté ({}) : il est effacé par son nom",
                p.display(),
                crate::donnees::message_erreur(&e)
            ));
            if s.fichiers.contains(p) { reste.fichiers.push(p.clone()) } else { reste.dossiers.push(p.clone()) }
        }
    }
    (reste, notes)
}

// ---------------------------------------------------------------------------------------------------------------------
// Le dossier des données, en administrateur (lot C, M-A)
// ---------------------------------------------------------------------------------------------------------------------

/// Ce que le désinstalleur en administrateur lit des données, sans suivre de lien : le dossier ouvert et vérifié, puis
/// chaque fichier ouvert par rapport à lui (`effacement`). Le compte peut remplacer ce dossier par une jonction à tout
/// moment : rien n'est jamais lu ni écrit par un simple chemin.
#[derive(Debug)]
pub struct DonneesLues {
    /// sauvegarde.json (`Ok(None)` : absent ; une erreur : illisible, ou le dossier passe par un lien)
    pub sauvegarde: Result<Option<Vec<u8>>, String>,
    /// sauvegarde.json.bak
    pub secours: Option<Vec<u8>>,
    /// parametres.json (ou sa copie de secours) ; `None` : absent ou illisible
    pub reglages: Option<serde_json::Map<String, Value>>,
    /// historique.json (ou sa copie de secours)
    pub historique: Option<Value>,
}

/// Un fichier JSON du dossier ouvert (ou sa copie de secours), comme `Read-JsonSur` le lit, sans rien mettre de côté
fn json_du_dossier(d: &effacement::Donnees, nom: &str) -> Option<Value> {
    [nom.to_string(), format!("{nom}.bak")].iter().find_map(|f| {
        let o = d.lire(f).ok()??;
        let t = suite_commun::json_sur::texte_du_fichier(&o);
        serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok().filter(|v| !v.is_null())
    })
}

/// Rien de lu, sauf ce que dit la sauvegarde (absente, ou pourquoi elle n'a pas pu être lue)
fn sans_rien(sauvegarde: Result<Option<Vec<u8>>, String>) -> DonneesLues {
    DonneesLues { sauvegarde, secours: None, reglages: None, historique: None }
}

/// Lit les données de Nevermind (sauvegarde, réglages, historique) sans suivre de lien
pub fn lire_donnees(dossier: &Path) -> DonneesLues {
    match effacement::ouvrir_donnees(dossier) {
        Err(e) => sans_rien(Err(crate::donnees::message_erreur(&e))),
        Ok(effacement::EtatDonnees::Absent) => sans_rien(Ok(None)),
        Ok(effacement::EtatDonnees::Lien) => sans_rien(Err(effacement::PAR_UN_LIEN.into())),
        Ok(effacement::EtatDonnees::Dossier(d)) => DonneesLues {
            sauvegarde: d.lire("sauvegarde.json").map_err(|e| crate::donnees::message_erreur(&e)),
            secours: d.lire("sauvegarde.json.bak").ok().flatten(),
            reglages: match json_du_dossier(&d, "parametres.json") {
                Some(Value::Object(m)) => Some(m),
                _ => None,
            },
            historique: json_du_dossier(&d, "historique.json"),
        },
    }
}

/// Le contexte de la partie administrateur (lot C, M-A) : sans disque (le dossier des données n'est ni créé ni
/// raccourci, rien n'y est lu ni écrit par chemin), avec la sauvegarde, les réglages et l'historique lus sans suivre de
/// lien ([`lire_donnees`]). Rend aussi les réglages lus (pour [`ecrire_donnees`]).
pub fn contexte_administrateur(
    racine: &Path,
    donnees: &Path,
    geree: bool,
) -> (Contexte, Option<serde_json::Map<String, Value>>) {
    let ctx = Contexte::sans_disque(racine.to_path_buf(), donnees.to_path_buf(), geree);
    ctx.definir_racine(racine.to_path_buf());
    let reglages_lus = relire_donnees(&ctx, donnees);
    (ctx, reglages_lus)
}

/// Lit (ou relit, lot H : après la fermeture de l'Aide et de Nevermind) la sauvegarde, les réglages et l'historique
/// sans suivre de lien ([`lire_donnees`]), et les donne au contexte sans disque, exactement comme au départ. Rend les
/// réglages lus (`None` : absents ou illisibles), la référence de [`ecrire_donnees`].
pub fn relire_donnees(ctx: &Contexte, donnees: &Path) -> Option<serde_json::Map<String, Value>> {
    let lues = lire_donnees(donnees);
    let reglages_lus = lues.reglages.clone();
    ctx.fournir_reglages(lues.reglages.unwrap_or_default());
    ctx.fournir_historique(lues.historique);
    // (`Import-Backup` : le fichier, sa copie de secours, puis les valeurs d'origine gardées dans le registre)
    ctx.importer_sauvegarde_lue(lues.sauvegarde, lues.secours);
    reglages_lus
}

/// À la fin, en administrateur : les lignes du journal gardées en mémoire ajoutées à journal.txt, et les états
/// d'origine changés par « Tout restaurer » (les services et l'alimentation remis ne sont plus notés) écrits dans
/// parametres.json. Seulement si le dossier des données est encore là, ouvert et vérifié ; chaque fichier ouvert par
/// rapport à lui, jamais par son chemin (un lien posé à leur place est refusé, ou remplacé lui-même). `reglages_lus` :
/// les réglages relus juste avant la remise en état, la référence (`None` : illisibles, rien n'est réécrit).
///
/// Lot H (M2) : parametres.json est relu ici, et seuls `SvcOriginal` et `PcfgOriginal` y changent, s'ils ont changé
/// depuis la référence ; tout le reste (une préférence écrite entre-temps par Nevermind) est gardé tel quel. Illisible
/// à ce moment-là : il n'est pas touché.
pub fn ecrire_donnees(dossier: &Path, ctx: &Contexte, reglages_lus: Option<&serde_json::Map<String, Value>>) {
    let lignes = ctx.journal_en_attente();
    let Ok(effacement::EtatDonnees::Dossier(d)) = effacement::ouvrir_donnees(dossier) else { return };
    if !lignes.is_empty() {
        let mut texte = lignes.join("\r\n");
        texte.push_str("\r\n");
        let _ = d.ajouter("journal.txt", texte.as_bytes());
    }
    let Some(avant) = reglages_lus else { return };
    let maintenant = ctx.reglages();
    let changes: Vec<&str> = nevermind_noyau::contexte::REGLAGES_D_ORIGINE
        .iter()
        .copied()
        .filter(|k| maintenant.get(*k) != avant.get(*k))
        .collect();
    if changes.is_empty() {
        return;
    }
    let Some(Value::Object(mut fichier)) = json_du_dossier(&d, "parametres.json") else { return };
    for k in changes {
        match maintenant.get(k) {
            Some(v) => {
                fichier.insert(k.to_string(), v.clone());
            }
            None => {
                fichier.remove(k);
            }
        }
    }
    let texte = nevermind_noyau::json_ps::lisible(&Value::Object(fichier));
    let _ = d.remplacer("parametres.json", texte.as_bytes());
}

/// Le dossier de Windows, tel que Windows le donne (jamais `%SystemRoot%`, que le compte peut changer)
fn dossier_windows() -> String {
    let mut b = vec![0u16; 1024];
    // SAFETY : tampon et taille cohérents
    let n = unsafe { windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW(Some(&mut b)) } as usize;
    String::from_utf16_lossy(&b[..n.min(b.len())]).trim_end_matches('\\').to_string()
}

/// L'environnement de cmd.exe pour la commande de fin : neuf, fait ici avec les dossiers que Windows donne, comme
/// celui des programmes que lance l'Aide (`Windows::environnement`). Rien du compte (ni TEMP, ni PATHEXT, ni PATH).
fn environnement_fin(system32: &str, windows: &str) -> Vec<(String, String)> {
    vec![
        ("SystemRoot".to_string(), windows.to_string()),
        ("windir".to_string(), windows.to_string()),
        ("SystemDrive".to_string(), windows.get(..2).unwrap_or("C:").to_string()),
        ("ComSpec".to_string(), format!("{system32}\\cmd.exe")),
        ("PATH".to_string(), format!("{system32};{windows}")),
    ]
}

/// Le lancement de la commande de fin (audit du 08/11, G2 : le désinstalleur tourne en administrateur) :
/// - cmd.exe de System32, avec `/d` (l'AutoRun de `HKCU\Software\Microsoft\Command Processor`, que le compte peut
///   écrire sans droits, n'est pas exécuté), `/v:off` et `/e:on` (les réglages DelayedExpansion et EnableExtensions de
///   cette même clé sont ignorés), `/s` (la commande est passée entre guillemets, telle quelle) ;
/// - dossier courant System32 (jamais le dossier temporaire du compte, où un « ping.bat » pourrait attendre ; la
///   commande ne tient pas non plus le dossier de Nevermind, qu'elle supprime) ;
/// - environnement neuf ([`environnement_fin`]).
fn commande_fin_de(s: &Suppression) -> Command {
    let ping = format!("\"{}\\PING.EXE\"", system32());
    commande_fin(&travail::commande_suppression_avec(s, &ping))
}

fn commande_fin(commande: &str) -> Command {
    let sys32 = system32();
    let mut c = Command::new(format!("{sys32}\\cmd.exe"));
    c.raw_arg(format!("/d /e:on /v:off /s /c \"{commande}\""))
        .env_clear()
        .envs(environnement_fin(&sys32, &dossier_windows()))
        .current_dir(&sys32)
        .creation_flags(SANS_FENETRE)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    c
}

// ---------------------------------------------------------------------------------------------------------------------
// Le Pc de « Tout restaurer »
// ---------------------------------------------------------------------------------------------------------------------

/// Le `PcWindows` du bloc pc, avec ce que la désinstallation fait autrement : les demandes de l'Aide faites sur place
/// (voie « SurPlace »), les règles du pare-feu de Nevermind comptées (`Get-NetFirewallRule`), les cartes réseau
/// retrouvées par leur identifiant (`Get-NetAdapter`), et la clé des valeurs d'origine de la machine effacée quand
/// l'Aide n'a pas pu les oublier (le désinstalleur a les droits).
pub struct PcDesinstallation<'a> {
    pub pc: PcWindows<'a>,
    pub aide: &'a AideSurPlace,
    /// Le contexte (sans disque) de `pc`
    pub ctx: &'a Contexte,
}

/// powercfg de System32 (le désinstalleur en administrateur : jamais un « powercfg.exe » pris ailleurs)
fn powercfg_system32(args: &[String]) -> (i32, String) {
    // (pris dans System32 par son chemin complet, jamais cherché ailleurs : en administrateur, un « powercfg.exe »
    // posé dans le dossier de Nevermind ne doit jamais être lancé à sa place)
    match Command::new(format!("{}\\powercfg.exe", system32()))
        .args(args)
        .creation_flags(SANS_FENETRE)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(o) => (o.status.code().unwrap_or(1), nevermind_noyau::windows::texte_oem(&o.stdout)),
        Err(_) => (1, String::new()),
    }
}

impl Pc for PcDesinstallation<'_> {
    fn exiger_geste(&self) -> Result<Permis, String> {
        self.pc.exiger_geste()
    }
    fn reg_lire(&self, chemin: &str, nom: &str) -> Result<Option<ValeurReg>, String> {
        self.pc.reg_lire(chemin, nom)
    }
    fn reg_noms(&self, chemin: &str) -> Option<Vec<String>> {
        self.pc.reg_noms(chemin)
    }
    fn reg_utilisateur_ecrire(
        &mut self,
        p: &Permis,
        chemin: &str,
        nom: &str,
        valeur: &ValeurReg,
    ) -> Result<(), String> {
        self.exiger_geste()?;
        self.pc.reg_utilisateur_ecrire(p, chemin, nom, valeur)
    }
    fn reg_utilisateur_effacer(&mut self, p: &Permis, chemin: &str, nom: &str) -> Result<(), String> {
        self.exiger_geste()?;
        self.pc.reg_utilisateur_effacer(p, chemin, nom)
    }
    fn origine_utilisateur(&mut self, p: &Permis, cle: &str, json: &str) {
        if self.exiger_geste().is_ok() {
            self.pc.origine_utilisateur(p, cle, json);
        }
    }
    fn origine_utilisateur_oublier(&mut self, p: &Permis, cle: &str) {
        if self.exiger_geste().is_ok() {
            self.pc.origine_utilisateur_oublier(p, cle);
        }
    }
    fn origines_utilisateur_effacer(&mut self, p: &Permis) {
        if self.exiger_geste().is_ok() {
            self.pc.origines_utilisateur_effacer(p);
        }
    }
    fn origines_machine_effacer(&mut self, _p: &Permis) -> Result<(), String> {
        self.exiger_geste()?;
        // (`[Microsoft.Win32.Registry]::LocalMachine.DeleteSubKeyTree($OriginesMachine, $false)` : en administrateur,
        // une clé absente n'est pas une erreur)
        let c = "SOFTWARE\\Mindset\\Nevermind\\Aide\\Origines";
        if windows_registry::LOCAL_MACHINE.open(c).is_err() {
            return Ok(());
        }
        windows_registry::LOCAL_MACHINE.remove_tree(c).map_err(|e| e.message())
    }
    fn aide(&mut self, action: &str, parametres: Value, _delai_ms: u64) -> Result<Value, String> {
        // (lot J : une action qui change le PC passe par aide_qui_change, avec le permis)
        if nevermind_noyau::aide::change_le_pc(action) {
            self.ctx.journal(&format!(
                "Aide : {action} refusé, une action qui change le PC passe par le verrou des gestes"
            ));
            return Err("Un autre réglage est en cours : rien n'a été changé. Réessaie dans un instant.".into());
        }
        self.aide.demander(action, &parametres)
    }
    fn aide_qui_change(
        &mut self,
        _p: &Permis,
        action: &str,
        parametres: Value,
        _delai_ms: u64,
    ) -> Result<Value, String> {
        self.exiger_geste()?;
        self.aide.demander(action, &parametres)
    }
    fn powercfg(&mut self, args: &[String]) -> (i32, String) {
        // (lot J : seulement une lecture ; un changement passe par powercfg_changer, avec le permis)
        let lecture = matches!(args.first().map(|s| s.as_str()), Some("/getactivescheme" | "/list" | "/q"));
        if !lecture {
            return (1, String::new());
        }
        powercfg_system32(args)
    }
    fn powercfg_changer(&mut self, _p: &Permis, args: &[String]) -> (i32, String) {
        if self.exiger_geste().is_err() {
            return (1, String::new());
        }
        powercfg_system32(args)
    }
    fn curseur(&self) -> Option<String> {
        self.pc.curseur()
    }
    fn curseur_ecrire(&mut self, p: &Permis, guid: &str) -> i32 {
        if self.exiger_geste().is_err() {
            return 1;
        }
        self.pc.curseur_ecrire(p, guid)
    }
    fn ecrans(&self) -> Vec<String> {
        self.pc.ecrans()
    }
    fn ecran_frequence(&mut self, p: &Permis, appareil: &str, hz: i64) -> i32 {
        if self.exiger_geste().is_err() {
            return 1;
        }
        self.pc.ecran_frequence(p, appareil, hz)
    }
    fn souris_relire(&mut self, p: &Permis, seuil1: i64, seuil2: i64, vitesse: i64) {
        if self.exiger_geste().is_ok() {
            self.pc.souris_relire(p, seuil1, seuil2, vitesse);
        }
    }
    fn service(&self, nom: &str) -> Option<(String, bool)> {
        self.pc.service(nom)
    }
    fn lancer(&mut self, args: &[String]) -> Result<(), String> {
        self.pc.lancer(args)
    }
    fn ouvrir(&mut self, adresse: &str) {
        self.pc.ouvrir(adresse);
    }
    fn commande(&mut self, p: &Permis, exe: &str, args: &[String]) -> i32 {
        self.pc.commande(p, exe, args)
    }
    fn rapport_batterie(&mut self, fichier: &str) -> i32 {
        self.pc.rapport_batterie(fichier)
    }
    fn regles_pare_feu(&self) -> usize {
        self.aide.regles_pare_feu()
    }
    // (lot J : une lecture ratée n'est pas « aucune règle » ; « Tout restaurer » tente alors le déblocage)
    fn regles_pare_feu_lues(&self) -> Option<usize> {
        self.aide.regles_pare_feu_lues()
    }
    fn index_carte(&self, guid: &str) -> Option<i64> {
        index_carte(guid)
    }
    fn dossier_temp(&self) -> String {
        self.pc.dossier_temp()
    }
    fn dossier_windows(&self) -> String {
        self.pc.dossier_windows()
    }
    fn hiberfil(&self) -> bool {
        self.pc.hiberfil()
    }
    fn nettoyer_dossier(&mut self, p: &Permis, id: &str, chemins: &[String], exclus: &[String]) -> ResultatNettoyage {
        if self.exiger_geste().is_err() {
            return ResultatNettoyage::default();
        }
        self.pc.nettoyer_dossier(p, id, chemins, exclus)
    }
    fn reglage(&self, nom: &str) -> Option<Value> {
        self.pc.reglage(nom)
    }
    fn ecrire_reglage(&mut self, nom: &str, valeur: Value, exiger: bool) -> Result<(), String> {
        self.pc.ecrire_reglage(nom, valeur, exiger)
    }
    fn ecrire_reglage_d_origine(&mut self, p: &Permis, nom: &str, valeur: Value, exiger: bool) -> Result<(), String> {
        self.pc.ecrire_reglage_d_origine(p, nom, valeur, exiger)
    }
    // (lot C, M-A : en administrateur, rien n'est écrit dans le dossier des données, que le compte peut remplacer par un
    //  lien. La sauvegarde, l'historique et les points ignorés ne changent qu'en mémoire : sauvegarde.json et
    //  l'historique partent de toute façon avec les données ; le journal est gardé en mémoire par le contexte sans
    //  disque, puis ajouté sans suivre de lien à la fin, comme les réglages changés : `ecrire_donnees`)
    fn sauvegarde_ecrire(&mut self, _p: &Permis, _sauvegarde: &Value) -> Result<(), String> {
        self.exiger_geste().map(|_| ())
    }
    fn sauvegarde_fusionner(&mut self, _p: &Permis, _base: &Value, _voulue: &Value) -> Result<Option<Value>, String> {
        // (rien à fusionner : personne d'autre n'écrit la sauvegarde pendant la désinstallation ; celle du geste reste)
        self.exiger_geste()?;
        Ok(None)
    }
    fn sauvegarde_effacer(&mut self, _p: &Permis) {}
    fn ignores_ecrire(&mut self, _ignores: &[String]) {}
    fn historique_ajouter(&mut self, _titre: &str, _details: &[String], _journal: &[Value]) -> Option<String> {
        None
    }
    fn historique_annule(&mut self, _id: &str) {}
    fn journal_nettoyage(&mut self, _nom: &str, _lignes: &[String]) -> String {
        String::new()
    }
    fn journal(&mut self, ligne: &str) {
        self.pc.journal(ligne);
    }
    fn message(&mut self, texte: &str) {
        self.pc.message(texte);
    }
    fn question(&mut self, texte: &str) -> Option<bool> {
        self.pc.question(texte)
    }
    fn statut(&mut self, texte: &str) {
        self.pc.statut(texte);
    }
    fn definitifs(&mut self) -> Vec<String> {
        // (l'historique lu au début, sans suivre de lien : jamais relu par son chemin)
        let mut l: Vec<String> = Vec::new();
        for e in self.ctx.historique() {
            for x in nevermind_pc::net::champ_liste(&e, "Log") {
                let q = nevermind_pc::net::champ_texte(&x, "Quoi");
                if nevermind_pc::net::champ_texte(&x, "Type") == "definitif" && !l.contains(&q) {
                    l.push(q);
                }
            }
        }
        l
    }
    fn maintenant(&self) -> Moment {
        self.pc.maintenant()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(c: &Command) -> String {
        c.get_args().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ")
    }

    /// Lot H : Local AppData est toujours celui du profil du compte, jamais un dossier choisi par une redirection (ni
    /// par une variable USERPROFILE changée). Sur ce PC (rien n'y est redirigé ni changé), on vérifie seulement que
    /// c'est bien `<profil>\AppData\Local` ; la redirection elle-même est essayée sur GitHub (cas 9 de
    /// outils\tester-desinstalleur-rust.ps1).
    #[test]
    fn local_appdata_est_toujours_celui_du_profil() {
        let a = "C:\\Users\\a\\AppData\\Local";
        assert_eq!(local_dans_le_profil(a, "C:\\Users\\a"), a);
        assert_eq!(
            local_dans_le_profil("c:\\users\\a\\appdata\\local\\", "C:\\Users\\a\\"),
            "c:\\users\\a\\appdata\\local\\"
        );
        assert_eq!(local_dans_le_profil("D:\\Piege\\Donnees\\..", "C:\\Users\\a"), a);
        assert_eq!(local_dans_le_profil("D:\\Piege", "C:\\Users\\a"), a);
        assert_eq!(local_dans_le_profil("", "C:\\Users\\a"), a);
        // (profil illisible : le chemin par défaut de Windows, seul)
        assert_eq!(local_dans_le_profil("C:\\Users\\a\\AppData\\Local", ""), a);
        let profil = profil_du_compte();
        assert!(!profil.is_empty(), "profil illisible");
        let l = localappdata();
        assert!(suite_commun::chemins::egal(&l, &format!("{profil}\\AppData\\Local")), "{l} n'est pas dans {profil}");
    }

    /// G2 (audit du 08/11) : en administrateur, cmd.exe ne doit rien prendre de l'utilisateur : ni son AutoRun (`/d`),
    /// ni un « ping » trouvé ailleurs que dans System32, ni son dossier temporaire comme dossier courant, ni son
    /// environnement
    #[test]
    fn la_commande_de_fin_ne_prend_rien_de_l_utilisateur() {
        let s = Suppression { fichiers: vec![PathBuf::from("C:\\X\\a.exe")], ..Default::default() };
        let c = commande_fin_de(&s);
        let sys32 = system32();
        assert!(suite_commun::chemins::egal(&c.get_program().to_string_lossy(), &format!("{sys32}\\cmd.exe")));
        let a = arguments(&c);
        assert!(a.starts_with("/d "), "AutoRun de cmd.exe non coupé : {a}");
        assert!(a.contains("/v:off"), "{a}");
        assert!(a.contains(&format!("\"{sys32}\\PING.EXE\" 127.0.0.1 -n 4 >nul")), "ping sans son chemin : {a}");
        assert!(!a.contains(" ping "), "{a}");
        let dossier = c.get_current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
        assert!(suite_commun::chemins::egal(&dossier, &sys32), "dossier courant : {dossier}");
        // L'environnement : seulement ce qui est fait ici (une variable du compte ne passe pas)
        let env: Vec<(String, Option<String>)> = c
            .get_envs()
            .map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned())))
            .collect();
        let noms: Vec<String> = env.iter().map(|(k, _)| k.to_ascii_uppercase()).collect();
        for n in ["SYSTEMROOT", "WINDIR", "COMSPEC", "PATH"] {
            assert!(noms.contains(&n.to_string()), "{n} absent : {env:?}");
        }
        for n in ["TEMP", "TMP", "USERPROFILE", "PATHEXT", "APPDATA"] {
            assert!(!noms.contains(&n.to_string()), "{n} transmis : {env:?}");
        }
        let path = env.iter().find(|(k, _)| k.eq_ignore_ascii_case("PATH")).and_then(|(_, v)| v.clone());
        assert!(path.is_some_and(|p| p.split(';').all(|d| suite_commun::chemins::egal(d, &sys32)
            || d.to_ascii_lowercase().starts_with(&dossier_windows().to_ascii_lowercase()))));
    }

    /// La commande de fin, lancée pour de vrai (sans droits, sans fenêtre) sur un dossier d'essai : les guillemets
    /// passent, PING.EXE attend, puis les fichiers et dossiers partent
    #[test]
    fn la_commande_de_fin_supprime_vraiment() {
        let base = std::env::temp_dir().join(format!("nevermind-desinst-fin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ici = base.join("Nevermind & co").join("fichiers");
        std::fs::create_dir_all(ici.join("modules")).unwrap();
        std::fs::write(ici.join("modules").join("m.ps1"), "x").unwrap();
        std::fs::write(base.join("Nevermind & co").join("Mindset.exe"), "x").unwrap();
        std::fs::write(base.join("Nevermind & co").join("autre.txt"), "à garder").unwrap();
        let racine = base.join("Nevermind & co");
        let s = Suppression {
            fichiers: vec![racine.join("Mindset.exe")],
            dossiers: vec![ici.join("modules")],
            vides: vec![ici.clone(), racine.clone()],
        };
        let mut c = commande_fin_de(&s);
        let mut enfant = c.spawn().unwrap();
        let debut = std::time::Instant::now();
        let fin = loop {
            if let Some(st) = enfant.try_wait().unwrap() {
                break Some(st);
            }
            if debut.elapsed() > std::time::Duration::from_secs(30) {
                let _ = enfant.kill();
                break None;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        };
        assert!(fin.is_some(), "cmd.exe ne s'est pas fini");
        assert!(debut.elapsed() >= std::time::Duration::from_secs(2), "pas d'attente avant la suppression");
        assert!(!racine.join("Mindset.exe").exists());
        assert!(!ici.exists(), "dossier fichiers resté");
        assert!(racine.join("autre.txt").exists(), "un fichier qui n'est pas de Nevermind est parti");
        let _ = std::fs::remove_dir_all(&base);
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Lot C (M-A, M1) : en administrateur, rien n'est lu ni écrit par un simple chemin dans un dossier du compte
    // -----------------------------------------------------------------------------------------------------------------

    use crate::donnees::tests::jonction;
    use serde_json::json;

    fn base_essai(nom: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("nevermind-desinst-c-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    /// Ce que contient un dossier (noms et contenus, dans l'ordre des noms) : rien ne doit y changer
    fn photo(d: &Path) -> Vec<(String, Vec<u8>)> {
        let mut l: Vec<(String, Vec<u8>)> = std::fs::read_dir(d)
            .unwrap()
            .flatten()
            .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap_or_default()))
            .collect();
        l.sort();
        l
    }

    /// Des données comme les prépare un programme sans droits : un journal de plus de 1 Mo et de plus de 3000 lignes
    /// (raccourci au démarrage, ses premières lignes écrites dans journal-ancien.txt), une sauvegarde abîmée (renommée
    /// à côté), des réglages et un historique
    fn donnees_piegees(d: &Path) {
        std::fs::create_dir_all(d).unwrap();
        let ligne = format!("{}\r\n", "x".repeat(300));
        std::fs::write(d.join("journal.txt"), ligne.repeat(4000)).unwrap();
        std::fs::write(d.join("sauvegarde.json"), "{ pas du JSON").unwrap();
        std::fs::write(d.join("parametres.json"), r#"{ "SvcOriginal": ["DiagTrack|Automatic|True"] }"#).unwrap();
        std::fs::write(d.join("historique.json"), "[]").unwrap();
    }

    /// Ce que fait la désinstallation en administrateur, sur le contexte de la partie administrateur : des lignes au
    /// journal, un service remis (son état d'origine n'est plus noté), la sauvegarde lue ; puis la fin
    fn comme_la_desinstallation(racine: &Path, donnees: &Path) -> Option<String> {
        let (ctx, lus) = contexte_administrateur(racine, donnees, true);
        let _geste = ctx.prendre_geste("Désinstaller", "Désinstallation").unwrap();
        ctx.journal("Désinstallation demandée par Mindset (données gardées)");
        ctx.exiger_geste().and_then(|p| ctx.ecrire_reglage_d_origine(&p, "SvcOriginal", json!([]), false)).unwrap();
        let probleme = ctx.sauvegarde().probleme;
        ecrire_donnees(donnees, &ctx, lus.as_ref());
        probleme
    }

    /// M-A : le dossier des données remplacé par une jonction (le compte a « Modification » sur D:\Mindset\Donnees) :
    /// rien n'apparaît ni ne change dans le dossier visé
    #[test]
    fn en_administrateur_rien_n_est_ecrit_a_travers_une_jonction() {
        let base = base_essai("jonction");
        let cible = base.join("cible");
        donnees_piegees(&cible);
        let avant = photo(&cible);
        let donnees = base.join("Donnees").join("Nevermind");
        std::fs::create_dir_all(base.join("Donnees")).unwrap();
        jonction(&donnees, &cible);
        let probleme = comme_la_desinstallation(&base.join("Programmes").join("Nevermind"), &donnees);
        assert!(photo(&cible) == avant, "le dossier visé par la jonction a changé");
        // (rien n'a pu être lu : la sauvegarde est dite abîmée, jamais vide en silence)
        assert_eq!(probleme.as_deref(), Some("abimee"));
        // Un dossier parent remplacé par une jonction : pareil
        let parent = base.join("Parent");
        jonction(&parent, &base);
        let probleme = comme_la_desinstallation(&base.join("Programmes").join("Nevermind"), &parent.join("cible"));
        assert!(photo(&cible) == avant, "le dossier atteint par une jonction parente a changé");
        assert_eq!(probleme.as_deref(), Some("abimee"));
        let _ = std::fs::remove_dir(&parent);
        let _ = std::fs::remove_dir(&donnees);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// M-A : un vrai dossier des données. Tout est lu sans rien changer (ni journal raccourci, ni sauvegarde abîmée
    /// renommée, ni ligne écrite avant la fin) ; à la fin, le journal est complété et les réglages réécrits, par
    /// rapport au dossier vérifié
    #[test]
    fn en_administrateur_les_donnees_sont_lues_puis_completees_a_la_fin() {
        let base = base_essai("vraies");
        let donnees = base.join("Donnees").join("Nevermind");
        donnees_piegees(&donnees);
        let racine = base.join("Programmes").join("Nevermind");
        let (ctx, lus) = contexte_administrateur(&racine, &donnees, true);
        let _geste = ctx.prendre_geste("Désinstaller", "Désinstallation").unwrap();
        let avant = photo(&donnees);
        assert_eq!(ctx.sauvegarde().probleme.as_deref(), Some("abimee"));
        assert_eq!(ctx.reglage("SvcOriginal"), Some(json!(["DiagTrack|Automatic|True"])));
        ctx.journal("Désinstallation : essai");
        ctx.exiger_geste().and_then(|p| ctx.ecrire_reglage_d_origine(&p, "SvcOriginal", json!([]), false)).unwrap();
        assert!(photo(&donnees) == avant, "écrit avant la fin (ou par son chemin)");
        ecrire_donnees(&donnees, &ctx, lus.as_ref());
        let journal = std::fs::read_to_string(donnees.join("journal.txt")).unwrap();
        assert!(journal.starts_with(&"x".repeat(300)), "le journal a été raccourci");
        assert!(journal.ends_with(" Désinstallation : essai\r\n"), "ligne absente : {:?}", journal.lines().last());
        assert!(!donnees.join("journal-ancien.txt").exists());
        assert!(donnees.join("sauvegarde.json").exists(), "la sauvegarde abîmée a été renommée");
        let p: Value =
            serde_json::from_str(&std::fs::read_to_string(donnees.join("parametres.json")).unwrap()).unwrap();
        assert_eq!(p["SvcOriginal"], json!([]));
        assert!(!donnees.join("parametres.json.tmp").exists());
        // Une bonne sauvegarde est lue telle quelle ; un journal absent est créé
        std::fs::write(
            donnees.join("sauvegarde.json"),
            r#"{ "Registry": { "HKCU:\\Software\\Essai|Valeur": { "Path": "HKCU:\\Software\\Essai", "Name": "Valeur", "Existed": true, "Value": 1, "Kind": "DWord" } } }"#,
        )
        .unwrap();
        std::fs::remove_file(donnees.join("journal.txt")).unwrap();
        let (ctx, lus) = contexte_administrateur(&racine, &donnees, true);
        let s = ctx.sauvegarde();
        assert_eq!(s.probleme, None);
        assert!(s.registre.contains_key("HKCU:\\Software\\Essai|Valeur"));
        ctx.journal("Désinstallation : encore");
        ecrire_donnees(&donnees, &ctx, lus.as_ref());
        assert!(
            std::fs::read_to_string(donnees.join("journal.txt")).unwrap().ends_with(" Désinstallation : encore\r\n")
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// M-A : un fichier des données remplacé par un lien dur vers un fichier qui n'est pas au compte n'est ni lu ni
    /// complété
    #[test]
    fn un_lien_dur_dans_les_donnees_n_est_jamais_suivi() {
        let base = base_essai("lien-dur");
        let donnees = base.join("donnees");
        std::fs::create_dir_all(&donnees).unwrap();
        let precieux = base.join("precieux.txt");
        std::fs::write(&precieux, "ne pas toucher").unwrap();
        std::fs::hard_link(&precieux, donnees.join("journal.txt")).unwrap();
        let effacement::EtatDonnees::Dossier(d) = effacement::ouvrir_donnees(&donnees).unwrap() else { panic!() };
        let refus = |r: std::io::Result<()>| r.is_err_and(|e| e.to_string() == effacement::FICHIER_LIEN);
        assert!(refus(d.ajouter("journal.txt", b"piege\r\n")));
        assert!(refus(d.lire("journal.txt").map(|_| ())));
        assert_eq!(std::fs::read_to_string(&precieux).unwrap(), "ne pas toucher");
        // Remplacé : le lien dur lui-même est remplacé par le nouveau fichier, ce qu'il vise n'est pas touché
        d.remplacer("journal.txt", b"neuf").unwrap();
        assert_eq!(std::fs::read_to_string(donnees.join("journal.txt")).unwrap(), "neuf");
        assert_eq!(std::fs::read_to_string(&precieux).unwrap(), "ne pas toucher");
        assert!(!donnees.join("journal.txt.tmp").exists());
        // (un nom qui sortirait du dossier est refusé)
        for nom in ["..\\precieux.txt", "a/b", "journal.txt:flux", "..", ""] {
            assert!(d.lire(nom).is_err() && d.ajouter(nom, b"x").is_err(), "{nom} accepté");
        }
        assert_eq!(std::fs::read_to_string(&precieux).unwrap(), "ne pas toucher");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Un vrai raccourci (.lnk) vers `cible`
    fn creer_raccourci(lnk: &Path, cible: &Path) {
        std::fs::create_dir_all(lnk.parent().unwrap()).unwrap();
        // SAFETY : COM ouvert pour ce fil le temps de l'écriture, objets du système
        unsafe {
            let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let lien: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).unwrap();
            lien.SetPath(&HSTRING::from(cible.as_os_str())).unwrap();
            let f: IPersistFile = lien.cast().unwrap();
            f.Save(&HSTRING::from(lnk.as_os_str()), true).unwrap();
            if init {
                CoUninitialize();
            }
        }
        assert!(lnk.is_file());
    }

    /// M1 : le bureau (ou le menu Démarrer) redirigé vers un dossier qui est une jonction, ou qui passe par une : le
    /// raccourci que vise la jonction n'est jamais effacé, même s'il lance bien cette copie
    #[test]
    fn un_raccourci_atteint_par_une_jonction_n_est_jamais_efface() {
        let base = base_essai("raccourcis");
        let racine = base.join("Programmes").join("Nevermind");
        std::fs::create_dir_all(&racine).unwrap();
        std::fs::write(racine.join("Mindset.exe"), "x").unwrap();
        let cible = base.join("protege");
        creer_raccourci(&cible.join(RACCOURCI), &racine.join("Mindset.exe"));
        creer_raccourci(&cible.join("Mindset").join(RACCOURCI), &racine.join("Mindset.exe"));
        // Le bureau redirigé vers une jonction
        let bureau = base.join("Bureau");
        jonction(&bureau, &cible);
        assert!(!effacer_raccourci_de(&bureau, &racine, false));
        // Le menu Démarrer redirigé vers un dossier qui passe par une jonction
        let menu = base.join("Menu");
        jonction(&menu, &cible);
        assert!(!effacer_raccourci_de(&menu.join("Mindset"), &racine, true));
        assert!(cible.join(RACCOURCI).is_file(), "le raccourci visé par la jonction du bureau a été effacé");
        assert!(
            cible.join("Mindset").join(RACCOURCI).is_file(),
            "le raccourci visé par la jonction du menu a été effacé"
        );
        let _ = std::fs::remove_dir(&bureau);
        let _ = std::fs::remove_dir(&menu);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// M1 : un bureau redirigé vers un vrai dossier (OneDrive par exemple) : le raccourci de cette copie part, par sa
    /// poignée ; celui d'une autre copie reste ; le dossier « Mindset » du menu part s'il est vide
    #[test]
    fn le_raccourci_de_cette_copie_part_par_sa_poignee() {
        let base = base_essai("raccourcis-vrais");
        let racine = base.join("Programmes").join("Nevermind");
        std::fs::create_dir_all(&racine).unwrap();
        let bureau = base.join("OneDrive").join("Bureau");
        creer_raccourci(&bureau.join(RACCOURCI), &racine.join("Mindset.exe"));
        assert!(raccourci_de_cette_copie(&bureau, &racine).is_some());
        assert!(effacer_raccourci_de(&bureau, &racine, false));
        assert!(!bureau.join(RACCOURCI).exists());
        assert!(bureau.is_dir(), "le bureau ne part jamais");
        // Le raccourci d'une autre copie reste
        creer_raccourci(&bureau.join(RACCOURCI), &base.join("Ailleurs").join("Mindset.exe"));
        assert!(!effacer_raccourci_de(&bureau, &racine, false));
        assert!(bureau.join(RACCOURCI).is_file());
        // Le menu : le dossier « Mindset » vide part, pas s'il garde autre chose
        let menu = base.join("Menu").join("Mindset");
        creer_raccourci(&menu.join(RACCOURCI), &racine.join("Nevermind.exe"));
        assert!(effacer_raccourci_de(&menu, &racine, true));
        assert!(!menu.exists(), "le dossier Mindset vide est resté");
        creer_raccourci(&menu.join(RACCOURCI), &racine.join("Mindset.exe"));
        std::fs::write(menu.join("Evermind.lnk"), "x").unwrap();
        assert!(effacer_raccourci_de(&menu, &racine, true));
        assert!(menu.join("Evermind.lnk").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Lot H, M2 : à la fin, parametres.json est relu ; seuls les états d'origine (services, alimentation) y changent
    // -----------------------------------------------------------------------------------------------------------------

    /// Une préférence écrite par Nevermind pendant la désinstallation (après la lecture du début) est gardée ; seuls
    /// SvcOriginal et PcfgOriginal changent, et seulement s'ils ont été changés par la remise en état
    #[test]
    fn a_la_fin_seuls_les_etats_d_origine_changent_dans_les_reglages() {
        let base = base_essai("reglages-fin");
        let donnees = base.join("Donnees").join("Nevermind");
        std::fs::create_dir_all(&donnees).unwrap();
        std::fs::write(
            donnees.join("parametres.json"),
            r#"{ "SvcOriginal": ["DiagTrack|Automatic|True"], "PcfgOriginal": ["a|b|1|2"], "Theme": "neon" }"#,
        )
        .unwrap();
        let (ctx, lus) = contexte_administrateur(&base.join("Programmes").join("Nevermind"), &donnees, true);
        let _geste = ctx.prendre_geste("Désinstaller", "Désinstallation").unwrap();
        // Nevermind, encore ouvert, écrit ses préférences pendant ce temps
        std::fs::write(
            donnees.join("parametres.json"),
            r#"{ "SvcOriginal": ["DiagTrack|Automatic|True"], "PcfgOriginal": ["a|b|1|2"], "Theme": "clair", "Nouvelle": 7 }"#,
        )
        .unwrap();
        // La remise en état : le service est remis (plus noté), l'alimentation ne change pas
        ctx.exiger_geste().and_then(|p| ctx.ecrire_reglage_d_origine(&p, "SvcOriginal", json!([]), false)).unwrap();
        ecrire_donnees(&donnees, &ctx, lus.as_ref());
        let p: Value =
            serde_json::from_str(&std::fs::read_to_string(donnees.join("parametres.json")).unwrap()).unwrap();
        assert_eq!(
            p.get("Nouvelle"),
            Some(&json!(7)),
            "la préférence écrite entre-temps par Nevermind est perdue : {p}"
        );
        assert_eq!(p["Theme"], json!("clair"), "la préférence changée entre-temps est remise à l'ancienne : {p}");
        assert_eq!(p["SvcOriginal"], json!([]), "{p}");
        assert_eq!(p["PcfgOriginal"], json!(["a|b|1|2"]), "{p}");
        assert!(!donnees.join("parametres.json.tmp").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Lot H, point 3 : une réinstallation faite juste après la désinstallation n'est pas effacée par la commande de fin
    // -----------------------------------------------------------------------------------------------------------------

    /// Le Systeme de la désinstallation, pour un essai (la commande de fin seulement)
    fn systeme<'a>(ctx: &'a Contexte, aide: &'a AideSurPlace, racine: &Path) -> SystemeWindows<'a> {
        SystemeWindows { ctx, aide, racine: racine.to_path_buf(), muet: true, reglages_relus: None }
    }

    /// Une copie installée pour l'essai : ses programmes, ses dossiers, et un fichier qui n'est pas à elle
    fn copie_installee(racine: &Path, quoi: &str) {
        let ici = racine.join("fichiers");
        std::fs::create_dir_all(ici.join("modules")).unwrap();
        std::fs::create_dir_all(racine.join("aide-temp")).unwrap();
        std::fs::write(ici.join("modules").join("m.ps1"), quoi).unwrap();
        std::fs::write(racine.join("aide-temp").join("t.txt"), quoi).unwrap();
        for f in ["Mindset.exe", "Nevermind.exe", "NevermindAide.exe", "version.json"] {
            std::fs::write(racine.join(f), quoi).unwrap();
        }
    }

    /// Ce que la désinstallation fait supprimer à la fin (comme `travail::desinstaller`)
    fn suppression_de(racine: &Path) -> Suppression {
        let ici = racine.join("fichiers");
        Suppression {
            fichiers: travail::fichiers_livres(racine).into_iter().filter(|f| f.exists()).collect(),
            dossiers: vec![
                ici.join("modules"),
                ici.join("outils-tiers"),
                ici.join("cles-publiques"),
                racine.join("aide-temp"),
            ],
            vides: vec![ici, racine.to_path_buf()],
        }
    }

    /// Attend que la commande de fin ait fini (PING.EXE 3 s, puis l'effacement) : elle n'est pas rendue par
    /// `lancer_suppression`, on attend donc largement
    fn attendre_la_commande_de_fin() {
        std::thread::sleep(std::time::Duration::from_secs(8));
    }

    #[test]
    fn une_reinstallation_juste_apres_n_est_pas_effacee() {
        let base = base_essai("reinstallation");
        let racine = base.join("Nevermind");
        let donnees = base.join("donnees");
        std::fs::create_dir_all(&donnees).unwrap();
        copie_installee(&racine, "ancienne");
        std::fs::write(racine.join("autre.txt"), "à garder").unwrap();
        let ctx = Contexte::sans_disque(racine.clone(), donnees.clone(), false);
        let aide = AideSurPlace::nouvelle();
        let s = suppression_de(&racine);
        systeme(&ctx, &aide, &racine).lancer_suppression(&travail::commande_suppression(&s), &s);
        // Réinstallée aussitôt (bien avant la fin des 3 s d'attente de la commande de fin)
        copie_installee(&racine, "nouvelle");
        attendre_la_commande_de_fin();
        for f in ["Mindset.exe", "Nevermind.exe", "NevermindAide.exe", "version.json"] {
            assert_eq!(
                std::fs::read_to_string(racine.join(f)).ok().as_deref(),
                Some("nouvelle"),
                "{f} de la nouvelle installation a été effacé par la commande de fin"
            );
        }
        assert!(
            racine.join("fichiers").join("modules").join("m.ps1").exists(),
            "les modules réinstallés ont été effacés"
        );
        assert!(racine.join("aide-temp").join("t.txt").exists(), "aide-temp réinstallé a été effacé");
        assert!(racine.join("autre.txt").exists(), "un fichier qui n'est pas de Nevermind est parti");
        // (ce qui a été mis de côté pour être effacé est bien parti)
        let restes: Vec<String> = std::fs::read_dir(&racine)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("a-effacer"))
            .collect();
        assert!(restes.is_empty(), "le dossier à effacer est resté : {restes:?}");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Sans réinstallation : tout ce qui est à Nevermind part, comme avant, le reste du dossier reste
    #[test]
    fn sans_reinstallation_la_commande_de_fin_efface_tout() {
        let base = base_essai("fin-normale");
        let racine = base.join("Nevermind");
        let donnees = base.join("donnees");
        std::fs::create_dir_all(&donnees).unwrap();
        copie_installee(&racine, "x");
        let ctx = Contexte::sans_disque(racine.clone(), donnees.clone(), false);
        let aide = AideSurPlace::nouvelle();
        let s = suppression_de(&racine);
        systeme(&ctx, &aide, &racine).lancer_suppression(&travail::commande_suppression(&s), &s);
        // (Nevermind.exe disparaît tout de suite : Mindset le voit aussitôt)
        assert!(!racine.join("Nevermind.exe").exists(), "Nevermind.exe est encore là");
        attendre_la_commande_de_fin();
        assert!(!racine.exists(), "le dossier des programmes est resté");
        // Avec un fichier qui n'est pas à Nevermind : il reste, et le dossier aussi
        copie_installee(&racine, "x");
        std::fs::write(racine.join("autre.txt"), "à garder").unwrap();
        let s = suppression_de(&racine);
        systeme(&ctx, &aide, &racine).lancer_suppression(&travail::commande_suppression(&s), &s);
        attendre_la_commande_de_fin();
        assert!(racine.join("autre.txt").exists());
        let mut reste: Vec<String> =
            std::fs::read_dir(&racine).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into()).collect();
        reste.sort();
        assert_eq!(reste, vec!["autre.txt"]);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Un dossier qui ne peut pas être mis de côté (tenu ouvert par un programme) : effacé par son nom, comme avant,
    /// et c'est noté au journal
    #[test]
    fn un_dossier_tenu_ouvert_est_efface_par_son_nom() {
        use std::os::windows::fs::OpenOptionsExt;
        let base = base_essai("fin-repli");
        let racine = base.join("Nevermind");
        let donnees = base.join("donnees");
        std::fs::create_dir_all(&donnees).unwrap();
        copie_installee(&racine, "x");
        let ctx = Contexte::sans_disque(racine.clone(), donnees.clone(), false);
        let aide = AideSurPlace::nouvelle();
        let s = suppression_de(&racine);
        // aide-temp ouvert sans permettre de l'effacer ni de le renommer (FILE_FLAG_BACKUP_SEMANTICS : un dossier)
        let tenu = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1 | 2)
            .custom_flags(0x0200_0000)
            .open(racine.join("aide-temp"))
            .unwrap();
        systeme(&ctx, &aide, &racine).lancer_suppression(&travail::commande_suppression(&s), &s);
        drop(tenu);
        let j = ctx.journal_en_attente().join("\n");
        assert!(j.contains("aide-temp"), "le repli n'est pas noté au journal : {j:?}");
        attendre_la_commande_de_fin();
        assert!(!racine.exists(), "le dossier des programmes est resté");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Un programme en cours d'exécution (comme ce désinstalleur lui-même) est bien mis de côté : il quitte sa place
    /// aussitôt, sans attendre la commande de fin
    #[test]
    fn un_programme_en_cours_est_mis_de_cote() {
        let base = base_essai("en-cours");
        let racine = base.join("Nevermind");
        let donnees = base.join("donnees");
        std::fs::create_dir_all(&donnees).unwrap();
        copie_installee(&racine, "x");
        // (une copie de PING.EXE sous le nom d'un programme de Nevermind, lancée sans fenêtre, qui attend 10 s)
        let exe = racine.join("Nevermind.exe");
        std::fs::copy(format!("{}\\PING.EXE", system32()), &exe).unwrap();
        let mut enfant = Command::new(&exe)
            .args(["127.0.0.1", "-n", "11"])
            .creation_flags(SANS_FENETRE)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let ctx = Contexte::sans_disque(racine.clone(), donnees.clone(), false);
        let aide = AideSurPlace::nouvelle();
        let s = suppression_de(&racine);
        systeme(&ctx, &aide, &racine).lancer_suppression(&travail::commande_suppression(&s), &s);
        let deplace = !exe.exists();
        let j = ctx.journal_en_attente().join("\n");
        let _ = enfant.kill();
        let _ = enfant.wait();
        assert!(deplace, "le programme en cours n'a pas quitté sa place : {j:?}");
        assert!(j.is_empty(), "rien ne devait être effacé par son nom : {j:?}");
        attendre_la_commande_de_fin();
        assert!(!racine.exists(), "le dossier des programmes est resté");
        let _ = std::fs::remove_dir_all(&base);
    }
}
