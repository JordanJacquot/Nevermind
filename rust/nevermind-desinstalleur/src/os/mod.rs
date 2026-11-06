//! Windows pour le désinstalleur (le seul module, avec le code repris de l'Aide, où le code « unsafe » est permis) :
//!
//! - [`SystemeWindows`] : le [`Systeme`] de la désinstallation (tâches planifiées par schtasks.exe, programmes, clé de
//!   l'Aide, raccourcis, la question et le message, la commande de fin) ;
//! - [`PcDesinstallation`] : le `Pc` du bloc pc (`PcWindows`) pour « Tout restaurer », avec les demandes de l'Aide
//!   faites sur place ([`AideSurPlace`]), les règles du pare-feu comptées et les cartes réseau retrouvées ;
//! - [`relancer_en_administrateur`] : la demande de Windows (une seule), comme Nevermind.ps1:330-350.
//!
//! Tout programme lancé l'est sans fenêtre (CREATE_NO_WINDOW).

#![allow(unsafe_code)]

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
    STGM_READ,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess,
    WaitForSingleObject,
};
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_Programs, IShellLinkW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS,
    SHELLEXECUTEINFOW, ShellExecuteExW, ShellLink,
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

/// Ferme le programme de ce numéro et attend sa fin (3 s au plus), comme `$p.Kill(); $p.WaitForExit(3000)`
fn fermer(pid: u32) {
    // SAFETY : poignée demandée à Windows, refermée par Poignee
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid) else { return };
        let h = Poignee(h);
        if TerminateProcess(h.0, 1).is_ok() {
            let _ = WaitForSingleObject(h.0, 3000);
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

impl SystemeWindows<'_> {
    /// `Get-AppExes` : les programmes qui lancent cette copie (raccourcis, tâche)
    fn exes_appli(&self) -> [String; 2] {
        let r = self.racine.to_string_lossy().trim_end_matches('\\').to_string();
        [format!("{r}\\Mindset.exe"), format!("{r}\\Nevermind.exe")]
    }

    fn lance_cette_copie(&self, lnk: &Path) -> bool {
        let t = cible_raccourci(lnk);
        !t.is_empty() && self.exes_appli().iter().any(|e| suite_commun::chemins::egal(e, &t))
    }

    fn raccourci_du_bureau(&self) -> Option<PathBuf> {
        nevermind_noyau::windows::dossier_bureau().map(|d| d.join("Nevermind.lnk"))
    }

    fn raccourci_du_menu(&self) -> PathBuf {
        PathBuf::from(nevermind_noyau::windows::dossier_connu(&FOLDERID_Programs)).join("Mindset").join("Nevermind.lnk")
    }
}

impl Systeme for SystemeWindows<'_> {
    fn historique(&mut self) -> Vec<Value> {
        self.ctx.historique()
    }
    fn demarrage_auto(&mut self) -> bool {
        self.tache_existe(travail::TACHE_DEMARRAGE)
    }
    fn raccourci_bureau(&mut self) -> bool {
        self.raccourci_du_bureau().is_some_and(|l| self.lance_cette_copie(&l))
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
        self.ctx.journal(ligne);
    }
    fn arreter_tache_aide(&mut self) {
        let _ = schtasks(&["/End", "/TN", &format!("{}{}", travail::TACHE_AIDE_DOSSIER, travail::TACHE_AIDE_NOM)]);
    }
    fn processus(&mut self) -> Vec<(u32, String)> {
        self.aide.processus()
    }
    fn fermer_processus(&mut self, pid: u32) {
        fermer(pid);
    }
    fn retirer_demarrage(&mut self) -> Result<(), String> {
        if !self.tache_existe(travail::TACHE_DEMARRAGE) {
            return Ok(());
        }
        match schtasks(&["/Delete", "/TN", travail::TACHE_DEMARRAGE, "/F"]) {
            Some(0) => Ok(()),
            Some(c) => Err(format!(
                "la tâche « {} » n'a pas pu être retirée (schtasks a répondu {c})",
                travail::TACHE_DEMARRAGE
            )),
            None => Err("schtasks.exe n'a pas pu se lancer".into()),
        }
    }
    fn tache_existe(&mut self, nom: &str) -> bool {
        schtasks(&["/Query", "/TN", nom]) == Some(0)
    }
    fn retirer_tache(&mut self, nom: &str) {
        let _ = schtasks(&["/Delete", "/TN", nom, "/F"]);
    }
    fn retirer_tache_aide(&mut self) {
        let _ =
            schtasks(&["/Delete", "/TN", &format!("{}{}", travail::TACHE_AIDE_DOSSIER, travail::TACHE_AIDE_NOM), "/F"]);
    }
    fn effacer_cle_aide(&mut self) {
        let _ = windows_registry::LOCAL_MACHINE.remove_tree("SOFTWARE\\Mindset\\Nevermind\\Aide");
    }
    fn effacer_raccourci_bureau(&mut self) {
        if let Some(l) = self.raccourci_du_bureau() {
            let _ = std::fs::remove_file(l);
        }
    }
    fn retirer_raccourci_menu(&mut self) {
        let l = self.raccourci_du_menu();
        if !self.lance_cette_copie(&l) {
            return;
        }
        let _ = std::fs::remove_file(&l);
        if let Some(d) = l.parent()
            && std::fs::read_dir(d).is_ok_and(|mut r| r.next().is_none())
        {
            let _ = std::fs::remove_dir(d);
        }
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
        // (dans le dossier temporaire : la commande ne doit pas tenir le dossier de Nevermind, qu'elle supprime)
        let _ = Command::new(format!("{}\\cmd.exe", system32()))
            .raw_arg(format!("/c {commande}"))
            .current_dir(std::env::temp_dir())
            .creation_flags(SANS_FENETRE)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
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
}

impl Pc for PcDesinstallation<'_> {
    fn reg_lire(&self, chemin: &str, nom: &str) -> Result<Option<ValeurReg>, String> {
        self.pc.reg_lire(chemin, nom)
    }
    fn reg_noms(&self, chemin: &str) -> Option<Vec<String>> {
        self.pc.reg_noms(chemin)
    }
    fn reg_utilisateur_ecrire(&mut self, chemin: &str, nom: &str, valeur: &ValeurReg) -> Result<(), String> {
        self.pc.reg_utilisateur_ecrire(chemin, nom, valeur)
    }
    fn reg_utilisateur_effacer(&mut self, chemin: &str, nom: &str) -> Result<(), String> {
        self.pc.reg_utilisateur_effacer(chemin, nom)
    }
    fn origine_utilisateur(&mut self, cle: &str, json: &str) {
        self.pc.origine_utilisateur(cle, json);
    }
    fn origine_utilisateur_oublier(&mut self, cle: &str) {
        self.pc.origine_utilisateur_oublier(cle);
    }
    fn origines_utilisateur_effacer(&mut self) {
        self.pc.origines_utilisateur_effacer();
    }
    fn origines_machine_effacer(&mut self) -> Result<(), String> {
        // (`[Microsoft.Win32.Registry]::LocalMachine.DeleteSubKeyTree($OriginesMachine, $false)` : en administrateur,
        // une clé absente n'est pas une erreur)
        let c = "SOFTWARE\\Mindset\\Nevermind\\Aide\\Origines";
        if windows_registry::LOCAL_MACHINE.open(c).is_err() {
            return Ok(());
        }
        windows_registry::LOCAL_MACHINE.remove_tree(c).map_err(|e| e.message())
    }
    fn aide(&mut self, action: &str, parametres: Value, _delai_ms: u64) -> Result<Value, String> {
        self.aide.demander(action, &parametres)
    }
    fn powercfg(&mut self, args: &[String]) -> (i32, String) {
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
        self.pc.curseur_ecrire(guid)
    }
    fn ecrans(&self) -> Vec<String> {
        self.pc.ecrans()
    }
    fn ecran_frequence(&mut self, appareil: &str, hz: i64) -> i32 {
        self.pc.ecran_frequence(appareil, hz)
    }
    fn souris_relire(&mut self, seuil1: i64, seuil2: i64, vitesse: i64) {
        self.pc.souris_relire(seuil1, seuil2, vitesse);
    }
    fn service(&self, nom: &str) -> Option<(String, bool)> {
        self.pc.service(nom)
    }
    fn lancer(&mut self, args: &[String]) {
        self.pc.lancer(args);
    }
    fn ouvrir(&mut self, adresse: &str) {
        self.pc.ouvrir(adresse);
    }
    fn commande(&mut self, exe: &str, args: &[String]) -> i32 {
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
        self.pc.nettoyer_dossier(id, chemins, exclus)
    }
    fn reglage(&self, nom: &str) -> Option<Value> {
        self.pc.reglage(nom)
    }
    fn ecrire_reglage(&mut self, nom: &str, valeur: Value, exiger: bool) -> Result<(), String> {
        self.pc.ecrire_reglage(nom, valeur, exiger)
    }
    fn sauvegarde_ecrire(&mut self, sauvegarde: &Value) -> Result<(), String> {
        self.pc.sauvegarde_ecrire(sauvegarde)
    }
    fn sauvegarde_effacer(&mut self) {
        self.pc.sauvegarde_effacer();
    }
    fn ignores_ecrire(&mut self, ignores: &[String]) {
        self.pc.ignores_ecrire(ignores);
    }
    fn historique_ajouter(&mut self, titre: &str, details: &[String], journal: &[Value]) -> Option<String> {
        self.pc.historique_ajouter(titre, details, journal)
    }
    fn historique_annule(&mut self, id: &str) {
        self.pc.historique_annule(id);
    }
    fn journal_nettoyage(&mut self, nom: &str, lignes: &[String]) -> String {
        self.pc.journal_nettoyage(nom, lignes)
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
        self.pc.definitifs()
    }
    fn maintenant(&self) -> Moment {
        self.pc.maintenant()
    }
}
