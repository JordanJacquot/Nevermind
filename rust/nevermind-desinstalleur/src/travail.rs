//! La désinstallation elle-même (`Invoke-Uninstall`, systeme.ps1:688-779), dans le même ordre :
//!
//! 1. ce qui va être fait (réglages à remettre, raccourci et lancement au démarrage, fichiers), les changements
//!    définitifs qui restent (ils protègent le PC), et la question des données, sauf si le choix est imposé ;
//! 2. l'Aide arrêtée (sa tâche, puis son programme, reconnu par son chemin) ;
//! 3. « Tout restaurer » (`Restore-AllSettings`, repris du bloc pc : `nevermind_pc::corrections::tout_restaurer`) ;
//! 4. les données (`Remove-DonneesDesinstallation`) ;
//! 5. le lancement au démarrage, les anciennes tâches, la tâche de l'Aide et ce qu'elle retenait (HKLM), les
//!    raccourcis du bureau et du menu Démarrer ;
//! 6. le message de fin, puis les fichiers de Nevermind supprimés (seulement ceux livrés avec lui, jamais le reste du
//!    dossier) juste après la fermeture de ce programme.
//!
//! Tout ce qui touche Windows passe par [`Systeme`] (et par le `Pc` du bloc pc pour la restauration) : les juges les
//! remplacent par un faux PC qui note chaque geste, exactement comme le faux PC de outils\exporter-juges-desinstalleur.ps1.

use crate::choix::Choix;
use crate::donnees;
use nevermind_pc::corrections::{self, Moteur, Sauvegarde};
use nevermind_pc::pc::Pc;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// La tâche planifiée de l'Aide (créée par Mindset)
pub const TACHE_AIDE_DOSSIER: &str = "\\Mindset\\";
pub const TACHE_AIDE_NOM: &str = "Nevermind (aide)";
/// Le lancement de Nevermind au démarrage de Windows (`$AutoStartTask`)
pub const TACHE_DEMARRAGE: &str = "Nevermind (démarrage)";
/// Les tâches de démarrage des anciens noms (`$OldAutoStartTasks` : OptiGame jusqu'à 1.0.55, Nexo en 1.0.56)
pub const ANCIENNES_TACHES: [&str; 2] = ["OptiGame (démarrage)", "Nexo (démarrage)"];
/// Ce que l'Aide retient elle-même (les DNS d'origine, les valeurs d'origine)
pub const CLE_AIDE: &str = "HKLM:\\SOFTWARE\\Mindset\\Nevermind\\Aide";

/// Ce qui, hors restauration, touche Windows ou l'écran
pub trait Systeme {
    /// historique.json (`Import-History`), pour les changements définitifs
    fn historique(&mut self) -> Vec<Value>;
    /// `Test-AutoStart` : la tâche « Nevermind (démarrage) » existe
    fn demarrage_auto(&mut self) -> bool;
    /// `Test-DesktopShortcut` : le raccourci du bureau lance cette copie
    fn raccourci_bureau(&mut self) -> bool;
    /// La fenêtre de désinstallation (`Show-ChoixDesinstallation`) : le choix, ou `None` si l'utilisateur annule
    fn question(&mut self, texte: &str) -> Option<Choix>;
    /// Le message de fin (`Show-MessageFinDesinstallation`)
    fn message_fin(&mut self, texte: &str);
    /// Une ligne au journal de Nevermind (`Write-Log`)
    fn journal(&mut self, ligne: &str);
    /// `Stop-ScheduledTask -TaskPath '\Mindset\' -TaskName 'Nevermind (aide)'` (erreurs ignorées)
    fn arreter_tache_aide(&mut self);
    /// Les programmes en cours : numéro et chemin complet (vide si illisible)
    fn processus(&mut self) -> Vec<(u32, String)>;
    /// Ferme ce programme et attend sa fin (3 s au plus ; erreurs ignorées)
    fn fermer_processus(&mut self, pid: u32);
    /// `Set-AutoStart $false` : la tâche « Nevermind (démarrage) » retirée si elle existe
    fn retirer_demarrage(&mut self) -> Result<(), String>;
    /// Une tâche de ce nom existe (`Get-ScheduledTask -TaskName`)
    fn tache_existe(&mut self, nom: &str) -> bool;
    /// `Unregister-ScheduledTask -TaskName` (erreurs ignorées)
    fn retirer_tache(&mut self, nom: &str);
    /// `Unregister-ScheduledTask -TaskPath '\Mindset\' -TaskName 'Nevermind (aide)'` (erreurs ignorées)
    fn retirer_tache_aide(&mut self);
    /// `Remove-Item HKLM:\SOFTWARE\Mindset\Nevermind\Aide -Recurse` (erreurs ignorées)
    fn effacer_cle_aide(&mut self);
    /// Le raccourci du bureau effacé (erreurs ignorées)
    fn effacer_raccourci_bureau(&mut self);
    /// `Remove-MenuShortcut` : le raccourci du menu Démarrer vers cette copie (et le dossier « Mindset » vide)
    fn retirer_raccourci_menu(&mut self);
    /// Le fichier existe
    fn existe(&mut self, chemin: &Path) -> bool {
        chemin.exists()
    }
    /// Les fichiers supprimés après la fermeture de ce programme : la commande de la version PowerShell (cmd.exe
    /// caché), et ce qu'elle supprime
    fn lancer_suppression(&mut self, commande: &str, suppression: &Suppression);
}

/// Où est cette copie de Nevermind
#[derive(Debug, Clone)]
pub struct Lieux {
    /// Le dossier des programmes (celui de Désinstaller Nevermind.exe : `Get-AppRoot`)
    pub racine: PathBuf,
    /// Le dossier des données (`$DataDir`)
    pub donnees: PathBuf,
    /// Ce programme (jamais fermé)
    pub pid: u32,
}

/// Ce que la commande de fin supprime
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Suppression {
    /// Les fichiers de Nevermind présents (`del /f /q`)
    pub fichiers: Vec<PathBuf>,
    /// Les dossiers supprimés avec leur contenu (`rmdir /s /q`)
    pub dossiers: Vec<PathBuf>,
    /// Les dossiers supprimés s'ils sont vides (`rmdir`), dans l'ordre
    pub vides: Vec<PathBuf>,
}

/// Comment ça s'est fini
#[derive(Debug, Clone, PartialEq)]
pub enum Fin {
    /// L'utilisateur a annulé : rien n'est touché
    Annulee,
    /// Fait (le message de fin dit ce qui a pu être fait)
    Faite { message: String, erreurs: Vec<String> },
}

/// `$script:History | ... | Where Type -eq 'definitif'` : « - Quoi (pour revenir en arrière : Retour) », sans doublon
/// (`Select-Object -Unique`, qui tient compte des majuscules)
fn restent_en_place(historique: &[Value]) -> Vec<String> {
    let texte = nevermind_pc::net::texte;
    let mut l: Vec<String> = Vec::new();
    for e in historique.iter().filter(|e| nevermind_pc::net::vrai(e)) {
        let journal = match e.get("Log") {
            Some(Value::Array(a)) => a.clone(),
            Some(Value::Null) | None => Vec::new(),
            Some(x) => vec![x.clone()],
        };
        for x in journal {
            // (`-eq` de PowerShell : sans tenir compte des majuscules)
            if x.get("Type").map(texte).is_some_and(|t| t.eq_ignore_ascii_case("definitif")) {
                let t = format!(
                    "  - {} (pour revenir en arrière : {})",
                    x.get("Quoi").map(texte).unwrap_or_default(),
                    x.get("Retour").map(texte).unwrap_or_default()
                );
                if !l.contains(&t) {
                    l.push(t);
                }
            }
        }
    }
    l
}

/// Un chemin dans un dossier, comme `Join-Path` (une seule « \ » entre les deux)
fn dans(dossier: &Path, nom: &str) -> PathBuf {
    PathBuf::from(format!("{}\\{nom}", dossier.to_string_lossy().trim_end_matches('\\')))
}

/// Les fichiers livrés avec Nevermind (et ses anciens noms), jamais le reste du dossier (systeme.ps1:737-760)
pub fn fichiers_livres(racine: &Path) -> Vec<PathBuf> {
    let ici = dans(racine, "fichiers");
    let mut l: Vec<PathBuf> = [
        "Mindset.exe",
        "Nevermind.exe",
        "Désinstaller Nevermind.exe",
        "NevermindAide.exe",
        "NevermindPont.dll",
        "OptiGame.exe",
        "Désinstaller OptiGame.exe",
        "Nexo.exe",
        "Désinstaller Nexo.exe",
        "LISEZMOI.txt",
    ]
    .iter()
    .map(|n| dans(racine, n))
    .collect();
    l.extend(
        [
            "Nevermind.ps1",
            "OptiGame.ps1",
            "NevermindAide.ps1",
            "Nevermind.ico",
            "OptiGame.ico",
            "cle-publique.xml",
            "Lancer Nevermind (secours).bat",
            "Désinstaller Nevermind (secours).bat",
            "Lancer OptiGame (secours).bat",
            "Lancer Nexo (secours).bat",
            "Désinstaller Nexo (secours).bat",
            "Désinstaller OptiGame (secours).bat",
        ]
        .iter()
        .map(|n| dans(&ici, n)),
    );
    l
}

/// La commande de fin (systeme.ps1:774-781) : attendre 3 s que ce programme soit fermé, puis supprimer
pub fn commande_suppression(s: &Suppression) -> String {
    let mut cmd = String::from("ping 127.0.0.1 -n 4 >nul");
    for f in &s.fichiers {
        cmd.push_str(&format!(" & del /f /q \"{}\"", f.display()));
    }
    for d in &s.dossiers {
        cmd.push_str(&format!(" & rmdir /s /q \"{}\"", d.display()));
    }
    for d in &s.vides {
        cmd.push_str(&format!(" & rmdir \"{}\"", d.display()));
    }
    cmd
}

/// La désinstallation. `sv` et `probleme` : la sauvegarde des réglages d'origine (`Import-Backup`) et son problème
/// éventuel (« abimee », « absente »). `impose` : le choix venu de la ligne de commande (pas de question).
pub fn desinstaller(
    pc: &mut dyn Pc,
    sys: &mut dyn Systeme,
    mut sv: Sauvegarde,
    probleme: Option<&str>,
    lieux: &Lieux,
    impose: Option<Choix>,
) -> Fin {
    // (services et réglages « batterie » remis par Restore-AllSettings : comptés aussi, sinon ils restaient changés)
    let n = corrections::compte_total(&sv, pc);
    let mut etapes: Vec<String> = Vec::new();
    if n > 0 {
        let s = if n > 1 { "s" } else { "" };
        etapes.push(format!("  - remettre les {n} réglage{s} de Windows modifié{s} par Nevermind comme avant"));
    }
    if sys.demarrage_auto() || sys.raccourci_bureau() {
        etapes.push("  - retirer son raccourci du bureau et son lancement au démarrage".into());
    }
    etapes.push("  - supprimer les fichiers de Nevermind de ce dossier".into());
    // Les changements de sécurité définitifs restent (ils protègent le PC) : ils sont dits, avec la façon de revenir
    // en arrière. Même dans une entrée annulée (« Tout restaurer ») : sa part définitive, elle, est toujours en place.
    let restent = restent_en_place(&sys.historique());
    let mut avertir = String::new();
    let texte_probleme = nevermind_noyau::sauvegarde::texte_probleme(probleme);
    if !texte_probleme.is_empty() {
        avertir.push_str(&format!("\n\nAttention : {texte_probleme}"));
    }
    if !restent.is_empty() {
        avertir
            .push_str(&format!("\n\nCes changements protègent ton PC et restent en place :\n{}", restent.join("\n")));
    }
    let choix = match impose {
        Some(c) => {
            // Piloté par Mindset : pas de question (l'utilisateur a déjà choisi), ce qu'il faut savoir est dit à la fin
            sys.journal(&format!(
                "Désinstallation demandée par Mindset (données {})",
                if c == Choix::Garder { "gardées" } else { "supprimées" }
            ));
            c
        }
        None => {
            let q = format!(
                "Nevermind va :\n{}\n\nLes points de restauration Windows sont conservés.{avertir}",
                etapes.join("\n")
            );
            match sys.question(&q) {
                Some(c) => c,
                None => return Fin::Annulee,
            }
        }
    };
    let garder = choix == Choix::Garder;

    let mut erreurs: Vec<String> = Vec::new();
    // L'Aide ouverte garderait NevermindAide.exe : on la ferme (le désinstalleur, lui, a les droits et fait tout
    // lui-même). Reconnue par son chemin, jamais par son nom seul (seule l'Aide de ce dossier). L'Aide en SYSTEM :
    // d'abord arrêtée par sa tâche, que Windows arrête même quand le processus refuse d'être fermé.
    let exe_aide = dans(&lieux.racine, "NevermindAide.exe");
    sys.arreter_tache_aide();
    for (pid, chemin) in sys.processus() {
        if pid != lieux.pid && egal_chemin(&chemin, &exe_aide) {
            sys.fermer_processus(pid);
        }
    }
    // (DIVERGENCES.md, point 3 : Nevermind lui-même, s'il est ouvert depuis ce dossier, est fermé aussi : il tiendrait
    // ses programmes et ses données ouverts)
    let exes_appli = [dans(&lieux.racine, "Mindset.exe"), dans(&lieux.racine, "Nevermind.exe")];
    for (pid, chemin) in sys.processus() {
        if pid != lieux.pid && exes_appli.iter().any(|e| egal_chemin(&chemin, e)) {
            sys.fermer_processus(pid);
        }
    }
    if n > 0 {
        let mut m = Moteur::nouveau(pc, &mut sv, false);
        erreurs.extend(corrections::tout_restaurer(&mut m));
    }
    if garder {
        sys.journal(&format!(
            "Désinstallation : réglages de Windows remis comme avant{}, données gardées",
            if erreurs.is_empty() { String::new() } else { format!(" sauf {}", erreurs.len()) }
        ));
    }
    let err_donnees = donnees::retirer_donnees(
        &lieux.donnees,
        garder,
        !erreurs.is_empty() || probleme.is_some_and(|p| !p.is_empty()),
    );
    erreurs.extend(err_donnees.iter().cloned());
    if let Err(e) = sys.retirer_demarrage() {
        erreurs.push(format!("Démarrage automatique: {e}"));
    }
    for t in ANCIENNES_TACHES {
        if sys.tache_existe(t) {
            sys.retirer_tache(t);
        }
    }
    // L'Aide : sa tâche planifiée (créée par Mindset) et ce qu'elle retenait (DNS d'origine)
    sys.retirer_tache_aide();
    sys.effacer_cle_aide();
    if sys.raccourci_bureau() {
        sys.effacer_raccourci_bureau();
    }
    sys.retirer_raccourci_menu();

    // Fichiers de l'application : uniquement ceux livrés avec Nevermind, jamais le reste du dossier
    let ici = dans(&lieux.racine, "fichiers");
    let fichiers: Vec<PathBuf> = fichiers_livres(&lieux.racine).into_iter().filter(|f| sys.existe(f)).collect();

    let mut msg = String::from("Nevermind est désinstallé.");
    if !garder {
        msg.push_str(&format!(
            "\n\nSes données sont supprimées{}.",
            if err_donnees.is_empty() { "" } else { ", sauf ce qui est indiqué plus bas" }
        ));
    } else if lieux.donnees.is_dir() {
        msg.push_str(&format!(
            "\n\nTes données sont gardées dans {} : si tu réinstalles Nevermind, tu retrouveras tes préférences, tes jeux et tes mesures.",
            lieux.donnees.display()
        ));
    }
    if n > 0 {
        msg.push_str("\n\nRedémarre le PC pour que tous les réglages d'origine soient pris en compte.");
    }
    if !erreurs.is_empty() {
        msg.push_str(&format!("\n\nCertains éléments n'ont pas pu être restaurés :\n{}", erreurs.join("\n")));
    }
    // Piloté par Mindset, la question n'a pas été posée : ce qu'elle disait d'important est dit ici
    if impose.is_some() {
        msg.push_str(&avertir);
    }
    sys.message_fin(&msg);

    // Les fichiers sont supprimés juste après la fermeture de ce programme (il est en cours d'utilisation)
    let suppression = Suppression {
        fichiers,
        dossiers: vec![
            dans(&ici, "modules"),
            dans(&ici, "outils-tiers"),
            dans(&ici, "cles-publiques"),
            dans(&lieux.racine, "aide-temp"),
        ],
        vides: vec![ici, lieux.racine.clone()],
    };
    sys.lancer_suppression(&commande_suppression(&suppression), &suppression);
    Fin::Faite { message: msg, erreurs }
}

/// Deux chemins de programmes égaux (`-eq` de PowerShell : sans tenir compte des majuscules)
fn egal_chemin(a: &str, b: &Path) -> bool {
    !a.is_empty() && suite_commun::chemins::egal(a, &b.to_string_lossy())
}
