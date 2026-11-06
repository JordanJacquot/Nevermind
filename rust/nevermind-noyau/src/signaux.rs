//! Les signaux communs vers l'hôte (règle du coordinateur, D:\Projets\_plans\signaux-communs-nevermind-rust.md) :
//! le noyau n'a pas d'effet « question », « occupé » ni « résultat » ; ce sont des `Effet::Signal` aux noms et aux
//! champs fixés, que l'hôte exécute. Ces fonctions les fabriquent, pour que tous les blocs les écrivent pareil.
//!
//! - [`question`] : la question du kit (`Confirm-Action`) ; l'hôte renvoie l'action `action` avec la valeur
//!   `{id, valeur: <bouton choisi>}` ; abandonnée : le dernier bouton ;
//! - [`occupe`] : la pastille « en cours » (texte vide : fini) ;
//! - [`resultat`] : la fiche de résultat (`Show-ResultSheet`) ;
//! - [`historique`] : une ligne de l'historique (`Add-History`), avec « Revenir en arrière » ;
//! - [`sauvegarde_changee`] : le résumé de la sauvegarde est à relire (`Update-BackupSummary`) ;
//! - [`partie_commencee`] / [`partie_finie`] : envoyés par le bloc perf.
//!
//! Un effet demandé pendant `pages()` est rendu au `tic` ou à l'action suivante.

use crate::Effet;
use serde_json::{Map, Value, json};

pub const QUESTION: &str = "question";
pub const OCCUPE: &str = "occupe";
pub const RESULTAT: &str = "resultat";
pub const HISTORIQUE: &str = "historique";
pub const SAUVEGARDE_CHANGEE: &str = "sauvegarde-changee";
pub const PARTIE_COMMENCEE: &str = "partie-commencee";
pub const PARTIE_FINIE: &str = "partie-finie";

fn signal(nom: &str, donnees: Value) -> Effet {
    Effet::Signal { nom: nom.into(), donnees }
}

/// `question` : {id, titre, texte, boutons, details?, action}
pub fn question(
    id: &str,
    titre: &str,
    texte: &str,
    boutons: &[&str],
    details: Option<&[String]>,
    action: &str,
) -> Effet {
    let mut m = Map::new();
    m.insert("id".into(), json!(id));
    m.insert("titre".into(), json!(titre));
    m.insert("texte".into(), json!(texte));
    m.insert("boutons".into(), json!(boutons));
    if let Some(d) = details {
        m.insert("details".into(), json!(d));
    }
    m.insert("action".into(), json!(action));
    signal(QUESTION, Value::Object(m))
}

/// La réponse à une question, reçue par `Bloc::action` (`{id, valeur}`) : l'identifiant et le bouton choisi
pub fn reponse_question(valeur: &Value) -> (String, String) {
    (valeur["id"].as_str().unwrap_or("").into(), valeur["valeur"].as_str().unwrap_or("").into())
}

/// `occupe` : {texte} (vide : fini)
pub fn occupe(texte: &str) -> Effet {
    signal(OCCUPE, json!({ "texte": texte }))
}

/// `resultat` : {titre, lignes, log?, note?}
pub fn resultat(titre: &str, lignes: &[String], log: Option<&[Value]>, note: Option<&str>) -> Effet {
    let mut m = Map::new();
    m.insert("titre".into(), json!(titre));
    m.insert("lignes".into(), json!(lignes));
    if let Some(l) = log {
        m.insert("log".into(), json!(l));
    }
    if let Some(n) = note {
        m.insert("note".into(), json!(n));
    }
    signal(RESULTAT, Value::Object(m))
}

/// `historique` : {titre, details?, log?}
pub fn historique(titre: &str, details: Option<&[String]>, log: Option<&[Value]>) -> Effet {
    let mut m = Map::new();
    m.insert("titre".into(), json!(titre));
    if let Some(d) = details {
        m.insert("details".into(), json!(d));
    }
    if let Some(l) = log {
        m.insert("log".into(), json!(l));
    }
    signal(HISTORIQUE, Value::Object(m))
}

/// `sauvegarde-changee` : {}
pub fn sauvegarde_changee() -> Effet {
    signal(SAUVEGARDE_CHANGEE, json!({}))
}

/// `partie-commencee` : {jeu, exe, pid}
pub fn partie_commencee(jeu: &str, exe: &str, pid: u32) -> Effet {
    signal(PARTIE_COMMENCEE, json!({ "jeu": jeu, "exe": exe, "pid": pid }))
}

/// `partie-finie` : {jeu, exe, pid, duree_s}
pub fn partie_finie(jeu: &str, exe: &str, pid: u32, duree_s: u64) -> Effet {
    signal(PARTIE_FINIE, json!({ "jeu": jeu, "exe": exe, "pid": pid, "duree_s": duree_s }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn champs_fixes() {
        let Effet::Signal { nom, donnees } = question("q1", "Installer ?", "Texte", &["Oui", "Non"], None, "maj:oui")
        else {
            panic!()
        };
        assert_eq!(nom, "question");
        assert_eq!(
            donnees,
            json!({ "id": "q1", "titre": "Installer ?", "texte": "Texte", "boutons": ["Oui", "Non"], "action": "maj:oui" })
        );
        assert_eq!(reponse_question(&json!({ "id": "q1", "valeur": "Oui" })), ("q1".into(), "Oui".into()));
        assert_eq!(occupe(""), Effet::Signal { nom: "occupe".into(), donnees: json!({ "texte": "" }) });
        assert_eq!(sauvegarde_changee(), Effet::Signal { nom: "sauvegarde-changee".into(), donnees: json!({}) });
    }
}
