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
    /// Tests : la lecture des règles du pare-feu échoue (le vrai pare-feu n'est pas lu ; lot J)
    pare_feu_illisible: bool,
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
        AideSurPlace { systeme, etat, journal: Journal::default(), pare_feu_illisible: false }
    }

    /// Tests : la lecture des règles du pare-feu échoue (COM refusé, service arrêté...), sans lire le vrai pare-feu
    #[doc(hidden)]
    pub fn avec_pare_feu_illisible(mut self) -> Self {
        self.pare_feu_illisible = true;
        self
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

    /// Ferme le programme de ce numéro, seulement s'il est bien `attendu`, puis attend sa fin (3 s au plus). Comme
    /// l'Aide (audit du 08/11, M6 ; lot C ici) : une seule poignée, gardée de la vérification à la fermeture ; son
    /// chemin est relu par elle juste avant, jamais rouvert par son numéro (qui a pu être redonné à un autre programme
    /// depuis la liste). Vrai si Windows l'a fermé.
    pub fn fermer(&self, pid: u32, attendu: &str) -> bool {
        fermer_tenu(&self.systeme, pid, attendu)
    }

    /// Les règles du pare-feu posées par Nevermind (« Nevermind : bloque ... », ou l'ancien nom « OptiGame : bloque
    /// ... ») : `Get-NetFirewallRule -DisplayName` de Restore-AllSettings
    pub fn regles_pare_feu(&self) -> usize {
        self.regles_pare_feu_lues().unwrap_or(0)
    }

    /// [`AideSurPlace::regles_pare_feu`], `None` quand la lecture a échoué (lot J, comme Ton PC au lot K : « Tout
    /// restaurer » tente alors le déblocage, au lieu de croire qu'il n'y a aucune règle)
    pub fn regles_pare_feu_lues(&self) -> Option<usize> {
        let regles = if self.pare_feu_illisible {
            Err("lecture refusée (essai)".to_string())
        } else {
            self.systeme.pare_feu_regles()
        };
        regles
            .map(|l| {
                l.iter()
                    .filter(|n| commence_par(n, "Nevermind : bloque ") || commence_par(n, "OptiGame : bloque "))
                    .count()
            })
            .ok()
    }
}

/// [`AideSurPlace::fermer`], sur le système de l'Aide
fn fermer_tenu<S: crate::systeme::Systeme>(s: &S, pid: u32, attendu: &str) -> bool {
    let Some(t) = s.tenir_processus(i64::from(pid)) else { return false };
    // (tout ce qui décide est lu par la poignée gardée : toujours en cours, et bien ce programme)
    if attendu.is_empty() || !t.actif() || !suite_commun::chemins::egal(&t.chemin(), attendu) {
        return false;
    }
    if !t.terminer() {
        return false;
    }
    let fin = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while t.actif() && std::time::Instant::now() < fin {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    true
}

/// `-DisplayName '<debut>*'` : le nom commence par ce début (sans tenir compte des majuscules, comme PowerShell)
fn commence_par(nom: &str, debut: &str) -> bool {
    nom.len() >= debut.len()
        && nom.is_char_boundary(debut.len())
        && nom[..debut.len()].to_lowercase() == debut.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt;

    /// Un programme à moi (PING.EXE de System32, sans fenêtre), qui attend une trentaine de secondes
    fn ping() -> (std::process::Child, String) {
        let exe = format!("{}\\PING.EXE", nevermind_noyau::windows::dossier_systeme().trim_end_matches('\\'));
        let enfant = std::process::Command::new(&exe)
            .args(["-n", "30", "127.0.0.1"])
            .creation_flags(0x0800_0000)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        (enfant, exe)
    }

    /// Lot C (P4 de l'audit de l'Aide) : le programme n'est fermé que si la poignée qui le ferme dit bien le chemin
    /// attendu. Un numéro redonné entre la liste et la fermeture désigne un autre programme : il n'est pas touché.
    #[test]
    fn un_numero_redonne_a_un_autre_programme_n_est_pas_ferme() {
        let s = Windows::new();
        let (mut enfant, exe) = ping();
        let pid = enfant.id();
        // (le numéro de la liste désignait « Mindset.exe » de Nevermind ; il est maintenant à un autre programme)
        assert!(!fermer_tenu(&s, pid, "C:\\Mindset\\Programmes\\Nevermind\\Mindset.exe"));
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(enfant.try_wait().unwrap().is_none(), "un autre programme a été fermé");
        // Le bon programme, lui, est fermé (et attendu)
        assert!(fermer_tenu(&s, pid, &exe));
        assert!(enfant.try_wait().unwrap().is_some(), "le programme n'a pas été fermé");
        let _ = enfant.kill();
        let _ = enfant.wait();
    }
}
