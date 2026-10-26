//! La règle « fermable » (`Get-RefusFermeture`, aide-actions.ps1) : LA règle, une seule, pour l'Aide
//! (« processus.fermer ») et pour la page RAM de Nevermind (bouton « Fermer »).
//!
//! Seulement un programme ordinaire, du même compte et de la même session que celui qui demande (Nevermind). Jamais :
//! un service, un processus protégé (moteur d'antivirus), un programme d'antivirus (inscrit au Centre de sécurité, ou
//! de Microsoft Defender), un programme de Windows (son nom ou son dossier), une appli du Microsoft Store, une appli de
//! la suite Mindset (reconnue à son emplacement), un programme lancé en administrateur quand celui qui demande ne
//! l'est pas. Dans le doute (compte, session, chemin ou protection illisible) : refusé.

use crate::{dossiers, exes, texte};
use std::collections::{HashMap, HashSet};

/// Celui pour qui on ferme (Nevermind) : `None` dans un champ = illisible
#[derive(Clone, Debug, Default)]
pub struct Reference {
    pub id: i64,
    pub session: Option<i64>,
    pub compte: String,
    pub eleve: Option<i64>,
}

/// Ce que Windows dit d'un processus. Protection et eleve : 0 non, 1 oui, -1 illisible ; `None` : absent.
#[derive(Clone, Debug, Default)]
pub struct Fiche {
    pub id: i64,
    pub chemin: String,
    pub session: Option<i64>,
    pub compte: String,
    pub protection: Option<i64>,
    pub eleve: Option<i64>,
}

/// Un processus connu, pour remonter les parents
#[derive(Clone, Debug, Default)]
pub struct Parente {
    pub parent: i64,
    pub chemin: String,
}

/// Ce que la règle sait du PC
#[derive(Clone, Debug, Default)]
pub struct Contexte {
    /// Dossier de Windows
    pub windows: String,
    /// Dossier Program Files (pour reconnaître Program Files\Mindset)
    pub programmes: String,
    /// Programmes des antivirus inscrits au Centre de sécurité
    pub antivirus: Vec<String>,
    /// Numéros des processus de service (`None` : liste illisible)
    pub services_pid: Option<HashSet<i64>>,
    /// Celui pour qui on ferme
    pub reference: Option<Reference>,
    /// Les processus connus (vide : pas lus)
    pub processus: HashMap<i64, Parente>,
}

/// Un emplacement de la suite Mindset (`Test-AideCheminSuite`) : ses programmes (Program Files\Mindset,
/// X:\Mindset\Programmes) et le dossier « ia » des données de Mindcare, d'où tourne son assistant
pub fn chemin_suite(chemin: &str, programmes: &str) -> bool {
    if chemin.is_empty() {
        return false;
    }
    if !programmes.is_empty() && texte::commence(chemin, &format!("{programmes}\\Mindset\\")) {
        return true;
    }
    let b = chemin.as_bytes();
    if b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\' {
        let r = &chemin[3..];
        if texte::commence(r, "Mindset\\Programmes\\") || texte::commence(r, "Mindset\\Donnees\\Mindcare\\ia\\") {
            return true;
        }
    }
    texte::contient(chemin, "\\AppData\\Local\\Mindset\\Mindcare\\ia\\")
        || texte::contient(chemin, "\\AppData\\Local\\Mindset\\Mindcare (travail)\\ia\\")
}

/// Une appli de la suite (`Test-AideSuite`) : à son emplacement, celui qui demande lui-même, ou (si les processus
/// sont connus) un processus lancé par la suite (sa WebView2) ou qui a lancé celui qui demande
fn suite(ctx: &Contexte, id: i64, chemin: &str) -> bool {
    if chemin_suite(chemin, &ctx.programmes) {
        return true;
    }
    let reference = ctx.reference.as_ref().map_or(0, |r| r.id);
    if reference != 0 && id == reference {
        return true;
    }
    if ctx.processus.is_empty() {
        return false;
    }
    let mut x = id;
    for _ in 0..8 {
        let Some(p) = ctx.processus.get(&x) else { break };
        x = p.parent;
        if x <= 4 {
            break;
        }
        let chemin_parent = ctx.processus.get(&x).map_or("", |p| p.chemin.as_str());
        if x == reference || chemin_suite(chemin_parent, &ctx.programmes) {
            return true;
        }
    }
    if reference != 0 {
        let mut x = reference;
        for _ in 0..8 {
            let Some(p) = ctx.processus.get(&x) else { break };
            x = p.parent;
            if x <= 4 {
                break;
            }
            if x == id {
                return true;
            }
        }
    }
    false
}

/// Pourquoi ce processus ne peut pas être fermé (`None` : il peut l'être). Le texte est montré à l'utilisateur.
/// `eleve_permis` : l'Aide, en administrateur, ferme aussi un programme de l'utilisateur lancé en administrateur.
pub fn refus(ctx: &Contexte, fiche: &Fiche, eleve_permis: bool) -> Option<&'static str> {
    let id = fiche.id;
    let chemin = fiche.chemin.as_str();
    if id <= 4 {
        return Some("fait partie de Windows");
    }
    if ctx.services_pid.as_ref().is_none_or(|s| s.contains(&id)) {
        return Some("est un service");
    }
    let r = match &ctx.reference {
        Some(r) if r.session.is_some_and(|s| s >= 0) && !r.compte.is_empty() => r,
        _ => return Some("programme qui fait la demande inconnu"),
    };
    if fiche.session != r.session {
        return Some("ne tourne pas dans ta session");
    }
    if fiche.compte.is_empty() || !texte::egal(&fiche.compte, &r.compte) {
        return Some("n'appartient pas à ton compte");
    }
    if fiche.protection != Some(0) {
        return Some("est protégé par Windows (antivirus)");
    }
    if chemin.is_empty() {
        return Some("emplacement illisible");
    }
    let windows = ctx.windows.trim_end_matches('\\');
    if exes::windows(texte::nom_fichier(chemin)) || texte::commence(chemin, &format!("{windows}\\")) {
        return Some("fait partie de Windows");
    }
    if dossiers::est_antivirus(&ctx.antivirus, chemin, windows) || dossiers::dans_defender(chemin) {
        return Some("est un programme de ton antivirus");
    }
    if texte::contient(chemin, "\\WindowsApps\\") {
        return Some("est une appli du Microsoft Store");
    }
    if suite(ctx, id, chemin) {
        return Some("fait partie de la suite Mindset");
    }
    if !eleve_permis && r.eleve.unwrap_or(0) != 1 && fiche.eleve != Some(0) {
        return Some("tourne en administrateur");
    }
    None
}
