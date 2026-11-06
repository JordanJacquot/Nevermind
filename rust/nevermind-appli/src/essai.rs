//! La construction d'essai (option cargo `essai`), jamais livrée : elle tourne seulement sur un dossier d'essai et sur
//! un bureau Windows à part (outils\essai-fenetre.ps1 de cette crate). L'exe livré ne contient pas ce module.
//!
//! Trois variables, toutes obligatoires (sinon le programme s'arrête aussitôt, sans rien faire) :
//! - `NEVERMIND_ESSAI_DONNEES` : le dossier de données de Nevermind ; le dossier WebView2 va dans son parent
//!   (comme `<données de la suite>\WebView2-Tauri\Nevermind`) ;
//! - `NEVERMIND_ESSAI_LOCAL` : le faux `%LOCALAPPDATA%` (suite.json, afficher-page.txt) ;
//! - `NEVERMIND_ESSAI_NOM` : le nom des verrous d'essai, `Local\NevermindEssai-<nom>-Instance` et `-OptiGame`.
//!
//! En plus, dans cette construction : HKLM n'est jamais lu, rien n'est ouvert par l'Explorateur, aucun signal n'est
//! levé, seul ce programme peut être relancé (windows\systeme.rs) ; `capture.demande` dans le dossier de données fait
//! écrire l'image de la fenêtre dans `capture.png` ; `actions.demande` y fait jouer des messages de la page (un JSON
//! par ligne) ; l'état envoyé est écrit dans `etat.json` (comme `$env:ESSAI_DOSSIER` de la version PowerShell) ;
//! `NEVERMIND_ECRAN` ouvre cette page à l'arrivée de l'écran (comme la version PowerShell).

use std::path::PathBuf;

use crate::vie::demarrage::Noms;

pub const CAPTURE_DEMANDE: &str = "capture.demande";
pub const CAPTURE: &str = "capture.png";
pub const ACTIONS_DEMANDE: &str = "actions.demande";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Essai {
    pub donnees: PathBuf,
    pub local: PathBuf,
    pub nom: String,
}

impl Essai {
    pub fn noms(&self) -> Noms {
        Noms {
            verrous: vec![
                format!(r"Local\NevermindEssai-{}-Instance", self.nom),
                format!(r"Local\NevermindEssai-{}-OptiGame", self.nom),
            ],
        }
    }
}

fn nom_valable(n: &str) -> bool {
    (1..=40).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

pub fn depuis(donnees: Option<String>, local: Option<String>, nom: Option<String>) -> Result<Essai, String> {
    let absolu = |d: &String| std::path::Path::new(d).is_absolute();
    let donnees = donnees.filter(absolu).ok_or("NEVERMIND_ESSAI_DONNEES manque ou incomplet")?;
    let local = local.filter(absolu).ok_or("NEVERMIND_ESSAI_LOCAL manque ou incomplet")?;
    let nom = nom.filter(|n| nom_valable(n)).ok_or("NEVERMIND_ESSAI_NOM manque ou mal écrit")?;
    Ok(Essai { donnees: PathBuf::from(donnees), local: PathBuf::from(local), nom })
}

pub fn lire() -> Result<Essai, String> {
    let v = |n: &str| std::env::var(n).ok();
    depuis(v("NEVERMIND_ESSAI_DONNEES"), v("NEVERMIND_ESSAI_LOCAL"), v("NEVERMIND_ESSAI_NOM"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reglages_d_essai() {
        let s = |t: &str| Some(t.to_string());
        let e = depuis(s(r"C:\T\Donnees\Nevermind"), s(r"C:\T\Local"), s("essai-42")).unwrap();
        assert_eq!(e.noms().verrous[0], r"Local\NevermindEssai-essai-42-Instance");
        assert!(
            e.noms().verrous.iter().all(|v| !v.starts_with(r"Local\Nevermind-") && !v.contains("OptiGame-Instance"))
        );
        assert!(depuis(s("relatif"), s(r"C:\T"), s("a")).is_err());
        assert!(depuis(s(r"C:\D"), None, s("a")).is_err());
        assert!(depuis(s(r"C:\D"), s(r"C:\T"), s("a b")).is_err());
    }
}
