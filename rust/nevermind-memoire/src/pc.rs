//! Ce que le bloc demande au PC : la lecture (seule), et les gestes de « Fermer » sur des processus déjà gardés.
//! Le module `windows` le remplit avec les vrais appels ; les tests avec un faux PC (jamais un vrai processus fermé).

use crate::constat::Cible;
use crate::donnees::{Donnees, Pilote};
// (publics : un faux PC des tests de l'hôte en a besoin)
pub use aide_regles::fermeture::{Contexte as ContexteRegle, Fiche};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Le résultat d'une lecture : les données, et les lignes pour le journal
pub struct Lecture {
    pub donnees: Option<Donnees>,
    pub journal: Vec<String>,
    pub erreur: Option<String>,
}

/// Le PC (lecture seule, sauf les deux gestes de fermeture, sur des processus vérifiés)
pub trait Pc: Send + Sync {
    /// `Get-DonneesMemoire` : `demarrage` (les programmes de la page Démarrage), `connus` (les pilotes déjà retrouvés)
    fn lire(&self, demarrage: &[String], connus: &HashMap<String, Vec<Pilote>>) -> Lecture;
    /// `Find-PilotesEtiquettes` : le pilote qui porte chaque étiquette (fichiers .sys, lecture seule)
    fn chercher_pilotes(&self, tags: &[String]) -> HashMap<String, Vec<Pilote>>;
    /// `Get-ContexteFermeture` : la règle relue juste avant de fermer (services, antivirus, Nevermind lui-même)
    fn contexte_fermeture(&self) -> Result<ContexteRegle, String>;
    /// `Read-AideFiche` : ce que Windows dit d'un processus
    fn fiche(&self, pid: i64, chemin: &str) -> Fiche;
    /// `[NmFermeture]::Verifier` : 1 c'est bien lui, 0 il est déjà fermé, -1 pas le même ou plus de poignée
    fn verifier(&self, pid: i64, debut: i64, chemin: &str) -> i32;
    /// `[NmFermeture]::FermerPoliment` : WM_CLOSE à ses fenêtres ; le nombre de fenêtres, -1 pas le même, -2 déjà fermé
    fn fermer_poliment(&self, pid: i64, debut: i64, chemin: &str) -> i32;
    /// `[NmFermeture]::Forcer` : 1 fait, -1 pas le même, -2 déjà fermé, -3 refusé par Windows
    fn forcer(&self, pid: i64, debut: i64, chemin: &str) -> i32;
    /// `[NmFermeture]::Vivant` : 1 toujours là, 0 fermé, -1 plus de poignée
    fn vivant(&self, pid: i64) -> i32;
    /// `Get-EtatFermeture` : « bloquee », « question » ou « rangee »
    fn etat_fermeture(&self, restants: &[Cible]) -> &'static str;
    /// `Get-Service` : son type de démarrage (« Automatic », « Manual »...) et s'il tourne ; `None` : absent
    fn service(&self, nom: &str) -> Option<(String, bool)>;
    /// Le Bureau de l'utilisateur (pour le rapport)
    fn bureau(&self) -> Option<PathBuf>;
}

/// L'Aide (`Invoke-Aide`) : par défaut celle du contexte (`ctx.aide`) ; une fausse Aide dans les tests
pub trait Aide: Send + Sync {
    fn demander(&self, ctx: &nevermind_noyau::Contexte, action: &str, parametres: &Value) -> Result<Value, String>;
}

/// L'Aide du contexte
pub struct AideDuContexte;

impl Aide for AideDuContexte {
    fn demander(&self, ctx: &nevermind_noyau::Contexte, action: &str, parametres: &Value) -> Result<Value, String> {
        ctx.aide(action, parametres, 120_000)
    }
}

/// Un PC qui ne lit rien (hors Windows)
pub struct PcAbsent;

impl Pc for PcAbsent {
    fn lire(&self, _: &[String], _: &HashMap<String, Vec<Pilote>>) -> Lecture {
        Lecture { donnees: None, journal: vec![], erreur: Some("lecture impossible hors de Windows".into()) }
    }
    fn chercher_pilotes(&self, _: &[String]) -> HashMap<String, Vec<Pilote>> {
        HashMap::new()
    }
    fn contexte_fermeture(&self) -> Result<ContexteRegle, String> {
        Err("hors de Windows".into())
    }
    fn fiche(&self, pid: i64, chemin: &str) -> Fiche {
        Fiche { id: pid, chemin: chemin.into(), ..Default::default() }
    }
    fn verifier(&self, _: i64, _: i64, _: &str) -> i32 {
        -1
    }
    fn fermer_poliment(&self, _: i64, _: i64, _: &str) -> i32 {
        -1
    }
    fn forcer(&self, _: i64, _: i64, _: &str) -> i32 {
        -1
    }
    fn vivant(&self, _: i64) -> i32 {
        -1
    }
    fn etat_fermeture(&self, _: &[Cible]) -> &'static str {
        "rangee"
    }
    fn service(&self, _: &str) -> Option<(String, bool)> {
        None
    }
    fn bureau(&self) -> Option<PathBuf> {
        None
    }
}
