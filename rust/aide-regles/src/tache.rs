//! Nevermind ne lance la tâche de l'Aide que si c'est la tâche SYSTEM pour son utilisateur (`Get-AideEtatTache`,
//! aide-client.ps1), lue dans le XML de `schtasks /Query /XML` (que l'utilisateur a le droit de lire).

use crate::{texte, xml};

/// L'état de la tâche :
/// - `"systeme"` : la bonne (en SYSTEM, NevermindAide.exe de ce dossier, pour cet utilisateur (`-Sid`), active) ;
/// - `"autre"` : une tâche à ce nom, mais pas celle-là (l'ancienne « droits les plus élevés », une tâche désactivée...) ;
/// - `""` : pas de tâche (ou illisible pour cet utilisateur).
///
/// `traduire` : un nom de compte en SID (Windows), pour une tâche dont le compte est écrit par son nom.
pub fn etat(xml_tache: &str, exe: &str, sid: &str, traduire: &dyn Fn(&str) -> Option<String>) -> &'static str {
    if xml_tache.is_empty() {
        return "";
    }
    let Some(doc) = xml::lire(xml_tache) else {
        return "autre";
    };
    if doc.nom != "Task" {
        return "autre";
    }
    let mut compte = doc.texte_de(&["Principals", "Principal", "UserId"]);
    if !compte.is_empty() && !texte::commence(&compte, "S-1-") {
        compte = traduire(&compte).unwrap_or_default();
    }
    let actions = doc.chercher(&["Actions", "Exec"]);
    let active = !texte::egal(&doc.texte_de(&["Settings", "Enabled"]), "false");
    if texte::egal(&compte, "S-1-5-18") && active && actions.len() == 1 {
        let commande = actions[0].texte_de(&["Command"]);
        let arguments = actions[0].texte_de(&["Arguments"]);
        if texte::egal(commande.trim_matches('"'), exe) && texte::sans_espaces(&arguments) == format!("-Sid {sid}") {
            return "systeme";
        }
    }
    "autre"
}
