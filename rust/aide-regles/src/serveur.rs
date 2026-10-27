//! Nevermind reconnaît l'Aide (`AideClient.ServeurAccepte`, natif.cs) : un canal créé par le système (l'Aide en
//! SYSTEM) ou par les administrateurs (l'Aide lancée par la demande de Windows), qu'un programme sans droits ne peut
//! pas créer ; sinon, celui qui répond doit être NevermindAide.exe de ce dossier.

use crate::texte;

/// Nevermind parle-t-il au programme `chemin` (créateur du canal), propriétaire du canal `proprietaire` ?
pub fn accepte(chemin: &str, proprietaire: &str, exe_attendu: &str) -> bool {
    if proprietaire == "S-1-5-18" || proprietaire == "S-1-5-32-544" {
        return true;
    }
    !chemin.is_empty() && !exe_attendu.is_empty() && texte::egal(chemin, exe_attendu)
}
