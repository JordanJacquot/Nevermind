//! La voie « sur place » de la désinstallation (aide-client.ps1, `$AideTravail` avec `SurPlace` et `Admin`) : le
//! désinstalleur, en administrateur, fait lui-même les demandes de la liste stricte de l'Aide, au lieu de les envoyer à
//! NevermindAide.exe (réveiller l'Aide la laisserait tenir NevermindAide.exe, qu'il faut supprimer ; et l'Aide n'écoute
//! que Mindset.exe de ce dossier).
//!
//! Chaque demande passe exactement par le même code que dans l'Aide : `serveur::repondre` (la demande lue, vérifiée par
//! `aide_regles::demandes::verifier`, faite par `actions::faire`, et la réponse `{ ok, r | e }`). Ce qui n'est pas sur
//! la liste de l'Aide est refusé ici aussi.

use crate::actions::EtatAide;
use crate::serveur::{Journal, repondre};
use crate::systeme::Systeme as _;
use crate::windows::Windows;
use serde_json::{Value, json};

/// Les demandes de l'Aide faites dans ce processus
pub struct AideSurPlace {
    systeme: Windows,
    etat: EtatAide,
    journal: Journal,
}

impl AideSurPlace {
    /// Seulement en administrateur (sinon chaque demande échouerait, comme dans la version PowerShell)
    pub fn nouvelle() -> AideSurPlace {
        let systeme = Windows::new();
        let moi = systeme.identite();
        let etat = EtatAide {
            admin: moi.admin,
            compte: moi.compte.clone(),
            pid: moi.pid,
            temp: String::new(),
            mesures: Default::default(),
        };
        AideSurPlace { systeme, etat, journal: Journal::default() }
    }

    /// Une demande (`Invoke-AideAction <a> <p> -Client $PID`) : son résultat (`r`), ou le message d'erreur de l'Aide
    pub fn demander(&self, action: &str, parametres: &Value) -> Result<Value, String> {
        let ligne = json!({ "a": action, "p": parametres }).to_string();
        let (reponse, _) = repondre(&ligne, &self.systeme, std::process::id(), &self.etat, &self.journal);
        let j: Value = serde_json::from_str(&reponse).map_err(|e| format!("Réponse illisible ({e})"))?;
        if j.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(j.get("r").cloned().unwrap_or(Value::Null))
        } else {
            Err(j.get("e").map(nevermind_pc::net::texte).unwrap_or_default())
        }
    }

    /// Les programmes en cours : numéro et chemin complet (lus comme l'Aide les lit)
    pub fn processus(&self) -> Vec<(u32, String)> {
        self.systeme
            .chemins_processus()
            .into_iter()
            .filter_map(|(pid, chemin)| u32::try_from(pid).ok().map(|p| (p, chemin)))
            .collect()
    }

    /// Les règles du pare-feu posées par Nevermind (« Nevermind : bloque ... », ou l'ancien nom « OptiGame : bloque
    /// ... ») : `Get-NetFirewallRule -DisplayName` de Restore-AllSettings
    pub fn regles_pare_feu(&self) -> usize {
        self.systeme
            .pare_feu_regles()
            .map(|l| {
                l.iter()
                    .filter(|n| commence_par(n, "Nevermind : bloque ") || commence_par(n, "OptiGame : bloque "))
                    .count()
            })
            .unwrap_or(0)
    }
}

/// `-DisplayName '<debut>*'` : le nom commence par ce début (sans tenir compte des majuscules, comme PowerShell)
fn commence_par(nom: &str, debut: &str) -> bool {
    nom.len() >= debut.len()
        && nom.is_char_boundary(debut.len())
        && nom[..debut.len()].to_lowercase() == debut.to_lowercase()
}
