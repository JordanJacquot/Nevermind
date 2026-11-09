//! Ce que le bloc demande à Windows, derrière un trait : le vrai (module `windows`) ou un faux pour les essais
//! (`Faux`), qui ne touche jamais le PC.

use crate::etat::{Environnement, EtatS};
use crate::ps::Date;
use crate::regles::{DetectionLue, Detections};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Ce que lit la vérification (`$SecDataWork`, le fichier hosts, le proxy, le contrôle des comptes, le démarrage)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lecture {
    pub s: EtatS,
    pub env: Environnement,
}

/// Une analyse de Microsoft Defender
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeAnalyse {
    Rapide,
    Complete,
    Personnalisee,
}

pub trait Systeme: Send + Sync {
    /// La lecture complète (lente : fichiers, signatures, Defender). Appelée hors du fil de l'écran.
    fn lire(&self) -> Lecture;
    /// Nevermind tourne en administrateur (`$script:EstAdmin`)
    fn est_admin(&self) -> bool;
    /// Une variable d'environnement (pour `%SystemRoot%`...)
    fn variable(&self, nom: &str) -> Option<String>;
    /// Le fichier existe
    fn existe(&self, chemin: &str) -> bool;
    /// Le script existe et seul un administrateur peut le modifier (`$protege` de aide-client.ps1)
    fn script_protege(&self, chemin: &str) -> bool;
    /// `$env:TEMP` et `"$env:SystemDrive\Users\Public"`
    fn temp(&self) -> String;
    fn public(&self) -> String;
    /// Le fichier hosts de Windows
    fn hosts(&self) -> String;
    /// L'heure locale
    fn maintenant(&self) -> Date;

    // --- Gestes (jamais appelés par les essais avec le vrai système)
    /// Une analyse de Defender, jusqu'à sa fin (dans un fil à part) ; `arret` : l'utilisateur l'arrête
    fn analyser(&self, sorte: TypeAnalyse, chemins: &[String], arret: &AtomicBool) -> Result<(), String>;
    /// La mise à jour de la base de virus (Update-MpSignature)
    fn mettre_a_jour_base(&self) -> Result<(), String>;
    /// Supprimer les menaces actives (Remove-MpThreat)
    fn supprimer_menaces(&self) -> Result<(), String>;
    /// Désactiver une tâche planifiée puis la relire : `Ok(true)` si elle est bien désactivée
    fn desactiver_tache(&self, nom: &str, chemin: &str) -> Result<bool, String>;
    /// Les détections de Defender lues sur place, et les noms des menaces (Get-MpThreatDetection, Get-MpThreat)
    fn detections(&self) -> Detections;
    /// Un fichier dans le Bloc-notes
    fn bloc_notes(&self, chemin: &str);
    /// L'Explorateur, l'élément sélectionné
    fn montrer(&self, chemin: &str);
    /// La fenêtre « Choisis le dossier à analyser » (`Invoke-FolderScan`, FolderBrowserDialog) : le dossier, ou rien
    fn choisir_dossier(&self, _titre: &str) -> Option<String> {
        None
    }
}

/// Un faux système : tout est simulé et noté dans `gestes`
#[derive(Default)]
pub struct Faux {
    pub lecture: Mutex<Lecture>,
    pub admin: bool,
    pub gestes: Mutex<Vec<String>>,
    /// Tâches que Windows refuse de désactiver, et celles qui restent actives sans erreur
    pub refus: Vec<String>,
    pub muettes: Vec<String>,
    pub echec_analyse: Option<String>,
    pub detections_lues: Mutex<Option<Result<Vec<DetectionLue>, String>>>,
    pub noms: Vec<(String, String)>,
    pub heure: Mutex<Option<Date>>,
    pub analyse_en_cours: AtomicBool,
    /// La fenêtre « Choisis le dossier à analyser » : elle attend la réponse donnée par l'essai (5 s au plus, comme
    /// un utilisateur qui parcourt ses dossiers) ; sans réponse prévue, rien n'est choisi
    pub choix_dossier: Mutex<Option<std::sync::mpsc::Receiver<Option<String>>>>,
}

impl Faux {
    fn noter(&self, g: String) {
        self.gestes.lock().unwrap_or_else(|e| e.into_inner()).push(g);
    }
    pub fn gestes(&self) -> Vec<String> {
        self.gestes.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Systeme for Faux {
    fn lire(&self) -> Lecture {
        self.noter("lire".into());
        self.lecture.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    fn est_admin(&self) -> bool {
        self.admin
    }
    fn variable(&self, nom: &str) -> Option<String> {
        nom.eq_ignore_ascii_case("SystemRoot").then(|| "C:\\WINDOWS".to_string())
    }
    fn existe(&self, _chemin: &str) -> bool {
        false
    }
    fn script_protege(&self, _chemin: &str) -> bool {
        false
    }
    fn temp(&self) -> String {
        "C:\\Users\\x\\AppData\\Local\\Temp".into()
    }
    fn public(&self) -> String {
        "C:\\Users\\Public".into()
    }
    fn hosts(&self) -> String {
        "C:\\WINDOWS\\System32\\drivers\\etc\\hosts".into()
    }
    fn maintenant(&self) -> Date {
        self.heure.lock().unwrap_or_else(|e| e.into_inner()).unwrap_or(Date {
            annee: 2026,
            mois: 11,
            jour: 6,
            heure: 3,
            minute: 0,
            seconde: 0,
        })
    }
    fn analyser(&self, sorte: TypeAnalyse, chemins: &[String], arret: &AtomicBool) -> Result<(), String> {
        self.noter(format!("analyser {sorte:?} {}", chemins.join(" | ")));
        self.analyse_en_cours.store(true, Ordering::SeqCst);
        // (l'essai décide de la fin en levant `arret` ou en laissant finir tout de suite)
        let _ = arret.load(Ordering::SeqCst);
        self.analyse_en_cours.store(false, Ordering::SeqCst);
        match &self.echec_analyse {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
    fn mettre_a_jour_base(&self) -> Result<(), String> {
        self.noter("mettre à jour la base".into());
        Ok(())
    }
    fn supprimer_menaces(&self) -> Result<(), String> {
        self.noter("supprimer les menaces".into());
        Ok(())
    }
    fn desactiver_tache(&self, nom: &str, chemin: &str) -> Result<bool, String> {
        self.noter(format!("désactiver {chemin}{nom}"));
        if self.refus.iter().any(|r| r == nom) {
            return Err("Accès refusé.".into());
        }
        Ok(!self.muettes.iter().any(|r| r == nom))
    }
    fn detections(&self) -> Detections {
        let d = self.detections_lues.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or(Ok(Vec::new()));
        (d, self.noms.clone())
    }
    fn bloc_notes(&self, chemin: &str) {
        self.noter(format!("bloc-notes {chemin}"));
    }
    fn montrer(&self, chemin: &str) {
        self.noter(format!("montrer {chemin}"));
    }
    fn choisir_dossier(&self, _titre: &str) -> Option<String> {
        self.noter("choisir un dossier".into());
        let r = self.choix_dossier.lock().unwrap_or_else(|e| e.into_inner()).take()?;
        r.recv_timeout(std::time::Duration::from_secs(5)).ok().flatten()
    }
}
