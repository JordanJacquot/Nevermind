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
use crate::travail::{self, Suppression, Systeme};
use nevermind_noyau::Contexte;
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
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile,
    IPersistStream, STGM_READ,
};
use windows::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_Programs, IShellLinkW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
    SHCreateMemStream, SHELLEXECUTEINFOW, ShellExecuteExW, ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{GUID, HSTRING, Interface, PCWSTR};

/// CREATE_NO_WINDOW : aucun programme lancé n'ouvre de fenêtre
const SANS_FENETRE: u32 = 0x0800_0000;

/// Le dossier System32
fn system32() -> String {
    nevermind_noyau::windows::dossier_systeme().trim_end_matches('\\').to_string()
}

/// `%LOCALAPPDATA%`, tel que Windows le donne
pub fn localappdata() -> String {
    nevermind_noyau::windows::dossier_connu(&FOLDERID_LocalAppData)
}

/// schtasks.exe avec ces arguments, sans fenêtre : son code de sortie (`None` s'il n'a pas pu se lancer)
fn schtasks(args: &[&str]) -> Option<i32> {
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

/// Retire cette tâche planifiée : son erreur, dite simplement
fn retirer(tache: &str) -> Result<(), String> {
    match schtasks(&["/Delete", "/TN", tache, "/F"]) {
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
    fn arreter_tache_aide(&mut self) {
        let _ = schtasks(&["/End", "/TN", &format!("{}{}", travail::TACHE_AIDE_DOSSIER, travail::TACHE_AIDE_NOM)]);
    }
    fn processus(&mut self) -> Vec<(u32, String)> {
        self.aide.processus()
    }
    fn fermer_processus(&mut self, pid: u32, chemin: &Path) {
        // (lot C : par une poignée gardée, son chemin relu par elle, comme l'Aide ; jamais rouvert par son numéro)
        let _ = self.aide.fermer(pid, &chemin.to_string_lossy());
    }
    fn retirer_demarrage(&mut self) -> Result<(), String> {
        if !self.tache_existe(travail::TACHE_DEMARRAGE) {
            return Ok(());
        }
        retirer(travail::TACHE_DEMARRAGE)
    }
    fn tache_existe(&mut self, nom: &str) -> bool {
        schtasks(&["/Query", "/TN", nom]) == Some(0)
    }
    fn retirer_tache(&mut self, nom: &str) -> Result<(), String> {
        retirer(nom)
    }
    fn retirer_tache_aide(&mut self) -> Result<(), String> {
        let t = format!("{}{}", travail::TACHE_AIDE_DOSSIER, travail::TACHE_AIDE_NOM);
        if !self.tache_existe(&t) {
            return Ok(());
        }
        retirer(&t)
    }
    fn effacer_cle_aide(&mut self) -> Result<(), String> {
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
        // Tout ce qui peut déjà partir part tout de suite (Nevermind.exe disparaît donc avant la fin de ce programme :
        // Mindset le voit aussitôt) ; la commande refait le tout une fois ce programme fermé (DIVERGENCES.md, point 4)
        let moi = std::env::current_exe().ok();
        for f in &suppression.fichiers {
            if moi.as_ref().is_none_or(|m| !suite_commun::chemins::egal(&m.to_string_lossy(), &f.to_string_lossy())) {
                let _ = std::fs::remove_file(f);
            }
        }
        // (`commande` est le texte de la version PowerShell, celui des juges : le vrai lancement refait la même
        // commande, avec PING.EXE par son chemin complet)
        let _ = commande;
        if let Err(e) = commande_fin_de(suppression).spawn() {
            self.ctx.journal(&format!("Désinstallation : la suppression des fichiers n'a pas pu se lancer ({e})"));
        }
    }
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
    let lues = lire_donnees(donnees);
    let reglages_lus = lues.reglages.clone();
    ctx.fournir_reglages(lues.reglages.unwrap_or_default());
    ctx.fournir_historique(lues.historique);
    // (`Import-Backup` : le fichier, sa copie de secours, puis les valeurs d'origine gardées dans le registre)
    ctx.importer_sauvegarde_lue(lues.sauvegarde, lues.secours);
    (ctx, reglages_lus)
}

/// À la fin, en administrateur : les lignes du journal gardées en mémoire ajoutées à journal.txt, et les réglages
/// changés par « Tout restaurer » (les services et l'alimentation remis ne sont plus notés) réécrits dans
/// parametres.json. Seulement si le dossier des données est encore là, ouvert et vérifié ; chaque fichier ouvert par
/// rapport à lui, jamais par son chemin (un lien posé à leur place est refusé, ou remplacé lui-même). `reglages_lus` :
/// les réglages lus au début (`None` : illisibles, rien n'est réécrit).
pub fn ecrire_donnees(dossier: &Path, ctx: &Contexte, reglages_lus: Option<&serde_json::Map<String, Value>>) {
    let lignes = ctx.journal_en_attente();
    let Ok(effacement::EtatDonnees::Dossier(d)) = effacement::ouvrir_donnees(dossier) else { return };
    if !lignes.is_empty() {
        let mut texte = lignes.join("\r\n");
        texte.push_str("\r\n");
        let _ = d.ajouter("journal.txt", texte.as_bytes());
    }
    if let Some(avant) = reglages_lus {
        let maintenant = ctx.reglages();
        if &maintenant != avant {
            let texte = nevermind_noyau::json_ps::lisible(&Value::Object(maintenant));
            let _ = d.remplacer("parametres.json", texte.as_bytes());
        }
    }
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

impl Pc for PcDesinstallation<'_> {
    fn exiger_geste(&self) -> Result<(), String> {
        self.pc.exiger_geste()
    }
    fn reg_lire(&self, chemin: &str, nom: &str) -> Result<Option<ValeurReg>, String> {
        self.pc.reg_lire(chemin, nom)
    }
    fn reg_noms(&self, chemin: &str) -> Option<Vec<String>> {
        self.pc.reg_noms(chemin)
    }
    fn reg_utilisateur_ecrire(&mut self, chemin: &str, nom: &str, valeur: &ValeurReg) -> Result<(), String> {
        self.exiger_geste()?;
        self.pc.reg_utilisateur_ecrire(chemin, nom, valeur)
    }
    fn reg_utilisateur_effacer(&mut self, chemin: &str, nom: &str) -> Result<(), String> {
        self.exiger_geste()?;
        self.pc.reg_utilisateur_effacer(chemin, nom)
    }
    fn origine_utilisateur(&mut self, cle: &str, json: &str) {
        if self.exiger_geste().is_ok() {
            self.pc.origine_utilisateur(cle, json);
        }
    }
    fn origine_utilisateur_oublier(&mut self, cle: &str) {
        if self.exiger_geste().is_ok() {
            self.pc.origine_utilisateur_oublier(cle);
        }
    }
    fn origines_utilisateur_effacer(&mut self) {
        if self.exiger_geste().is_ok() {
            self.pc.origines_utilisateur_effacer();
        }
    }
    fn origines_machine_effacer(&mut self) -> Result<(), String> {
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
        if nevermind_noyau::aide::change_le_pc(action) {
            self.exiger_geste()?;
        }
        self.aide.demander(action, &parametres)
    }
    fn powercfg(&mut self, args: &[String]) -> (i32, String) {
        let lecture = matches!(args.first().map(|s| s.as_str()), Some("/getactivescheme" | "/list" | "/q"));
        if !lecture && self.exiger_geste().is_err() {
            return (1, String::new());
        }
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
    fn curseur(&self) -> Option<String> {
        self.pc.curseur()
    }
    fn curseur_ecrire(&mut self, guid: &str) -> i32 {
        if self.exiger_geste().is_err() {
            return 1;
        }
        self.pc.curseur_ecrire(guid)
    }
    fn ecrans(&self) -> Vec<String> {
        self.pc.ecrans()
    }
    fn ecran_frequence(&mut self, appareil: &str, hz: i64) -> i32 {
        if self.exiger_geste().is_err() {
            return 1;
        }
        self.pc.ecran_frequence(appareil, hz)
    }
    fn souris_relire(&mut self, seuil1: i64, seuil2: i64, vitesse: i64) {
        if self.exiger_geste().is_ok() {
            self.pc.souris_relire(seuil1, seuil2, vitesse);
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
    fn commande(&mut self, exe: &str, args: &[String]) -> i32 {
        if exe.eq_ignore_ascii_case("shutdown.exe") && self.exiger_geste().is_err() {
            return 1;
        }
        self.pc.commande(exe, args)
    }
    fn regles_pare_feu(&self) -> usize {
        self.aide.regles_pare_feu()
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
    fn nettoyer_dossier(&mut self, id: &str, chemins: &[String], exclus: &[String]) -> ResultatNettoyage {
        if self.exiger_geste().is_err() {
            return ResultatNettoyage::default();
        }
        self.pc.nettoyer_dossier(id, chemins, exclus)
    }
    fn reglage(&self, nom: &str) -> Option<Value> {
        self.pc.reglage(nom)
    }
    fn ecrire_reglage(&mut self, nom: &str, valeur: Value, exiger: bool) -> Result<(), String> {
        self.pc.ecrire_reglage(nom, valeur, exiger)
    }
    // (lot C, M-A : en administrateur, rien n'est écrit dans le dossier des données, que le compte peut remplacer par un
    //  lien. La sauvegarde, l'historique et les points ignorés ne changent qu'en mémoire : sauvegarde.json et
    //  l'historique partent de toute façon avec les données ; le journal est gardé en mémoire par le contexte sans
    //  disque, puis ajouté sans suivre de lien à la fin, comme les réglages changés : `ecrire_donnees`)
    fn sauvegarde_ecrire(&mut self, _sauvegarde: &Value) -> Result<(), String> {
        self.exiger_geste()
    }
    fn sauvegarde_fusionner(&mut self, _base: &Value, _voulue: &Value) -> Result<Option<Value>, String> {
        // (rien à fusionner : personne d'autre n'écrit la sauvegarde pendant la désinstallation ; celle du geste reste)
        self.exiger_geste()?;
        Ok(None)
    }
    fn sauvegarde_effacer(&mut self) {}
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
        ctx.ecrire_reglage("SvcOriginal", json!([]), false).unwrap();
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
        ctx.ecrire_reglage("SvcOriginal", json!([]), false).unwrap();
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
}
