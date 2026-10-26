//! Programmes lancés avec Windows qui protègent le PC (`Test-AideDemarrageSecurite`, aide-actions.ps1) : Sécurité
//! Windows et les antivirus inscrits au Centre de sécurité. L'Aide ne les coupe jamais au démarrage.

use crate::dossiers::{self, Dossiers};
use crate::{exes, texte};

/// Ce dont la règle a besoin de lire sur le PC (seulement si la demande ne le donne pas)
pub trait LecturesDemarrage {
    /// La commande de l'entrée `nom` dans la clé Run de Windows (HKLM, vue 64 bits ; `run32` : WOW6432Node)
    fn commande_run(&self, run32: bool, nom: &str) -> Option<String>;
    /// Les programmes des antivirus inscrits au Centre de sécurité
    fn antivirus(&self) -> Vec<String>;
}

/// L'entrée `nom` de la clé StartupApproved `cle` (Run, Run32 ou StartupFolder) protège-t-elle le PC ?
/// `commande` : la commande de l'entrée (`None` : lue dans la clé Run) ; `antivirus` : programmes des antivirus
/// (`None` : lus sur le PC).
pub fn securite(
    cle: &str,
    nom: &str,
    commande: Option<&str>,
    antivirus: Option<&[String]>,
    dossiers: &Dossiers,
    pc: &dyn LecturesDemarrage,
) -> bool {
    let debuts = ["SecurityHealth", "Windows Defender", "WindowsDefender", "Microsoft Defender", "MicrosoftDefender"];
    if debuts.iter().chain(["Windows Security", "Sécurité Windows"].iter()).any(|d| texte::commence(nom, d))
        || exes::windows(texte::nom_sans_extension(nom))
    {
        return true;
    }
    let commande = match commande {
        Some(c) => c.to_string(),
        None => {
            let run32 = texte::finit(cle, "\\Run32");
            if run32 || texte::finit(cle, "\\Run") {
                pc.commande_run(run32, nom).unwrap_or_default()
            } else {
                String::new()
            }
        }
    };
    if commande.is_empty() {
        return false;
    }
    let Some(chemin) = dossiers.programme_de_commande(&commande) else {
        return false;
    };
    if exes::windows(texte::nom_fichier(&chemin)) || dossiers::dans_defender(&chemin) {
        return true;
    }
    let lus;
    let av = match antivirus {
        Some(a) => a,
        None => {
            lus = pc.antivirus();
            &lus
        }
    };
    dossiers::est_antivirus(av, &chemin, &dossiers.windows)
}
