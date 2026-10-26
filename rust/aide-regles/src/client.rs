//! Qui peut parler à l'Aide (`Test-AideClient`, aide-actions.ps1) : seulement Mindset.exe du dossier de cette copie
//! de Nevermind (le programme qui fait tourner Nevermind), reconnu par son chemin complet lu par Windows. Jamais par
//! le nom : toutes les applis de la suite tournent dans un « Mindset.exe ». Le relais Nevermind.exe ne parle jamais à
//! l'Aide.

use crate::{chemin, texte};

/// Le programme `chemin_client` (tel que Windows le donne) est-il Mindset.exe du dossier `racine` ?
/// Le chemin reçu est comparé tel quel : rien n'est « nettoyé » qui pourrait faire passer un autre fichier pour lui.
pub fn accepte(chemin_client: &str, racine: &str) -> bool {
    if chemin_client.is_empty() || racine.is_empty() {
        return false;
    }
    match chemin::complet(&chemin::joindre(racine, "Mindset.exe")) {
        Some(attendu) => texte::egal(chemin_client, &attendu),
        None => false,
    }
}
