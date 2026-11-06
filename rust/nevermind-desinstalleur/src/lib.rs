//! « Désinstaller Nevermind.exe » en Rust (passage complet de Nevermind en Rust) : remplace la désinstallation de la
//! version PowerShell (`Invoke-Uninstall` et `Remove-DonneesDesinstallation` de systeme.ps1, la relance en
//! administrateur de Nevermind.ps1:330-350, et le lanceur C# compilé avec UNINSTALL, outils\lanceur.cs).
//!
//! - [`choix`] : -GarderDonnees / -SupprimerDonnees (le choix des données, sans question) ;
//! - [`conseils`] : piloté par Mindset, ce que la fenêtre de fin aurait dit d'important, passé à Mindset ;
//! - [`donnees`] : le dossier des données, et ce qui en part ;
//! - [`travail`] : la désinstallation elle-même, contre un `Systeme` (le vrai : `os`, ou le faux PC des juges) ;
//! - `sur_place` : la voie « sur place » de la version PowerShell (aide-client.ps1, `SurPlace`) : en administrateur, le
//!   désinstalleur fait lui-même les demandes de la liste stricte de l'Aide (vérifiées par les mêmes règles), sans
//!   réveiller l'Aide dont il doit supprimer le programme ;
//! - `os` : Windows (tâches, processus, raccourcis, la question et le message, la relance en administrateur).
//!
//! « Tout restaurer » n'est pas récrit ici : c'est celui du bloc pc (`nevermind_pc::corrections::tout_restaurer`), sur
//! la sauvegarde du noyau (`Contexte::sauvegarde`).

#![deny(unsafe_op_in_unsafe_fn)]

pub mod choix;
pub mod conseils;
pub mod donnees;
pub mod travail;

#[cfg(windows)]
pub mod os;
#[cfg(windows)]
pub mod sur_place;

// Le code des actions de NevermindAide.exe (rust\nevermind-aide), repris tel quel pour la voie « sur place » : le même
// code fait les mêmes demandes, avec les mêmes vérifications (aide_regles::demandes::verifier). nevermind-aide n'est
// qu'un programme (pas de bibliothèque) : ses modules sont lus ici à leur place, sans être copiés ni changés. Ils
// doivent être à la racine de la crate, sous leurs noms (ils se citent par `crate::actions`, `crate::windows`...).
// Leur module windows autorise lui-même le code « unsafe » (comme dans l'Aide).
#[cfg(windows)]
#[allow(dead_code, clippy::all)]
#[path = "../../nevermind-aide/src/actions.rs"]
mod actions;
#[cfg(windows)]
#[allow(dead_code, clippy::all)]
#[path = "../../nevermind-aide/src/mesures/mod.rs"]
mod mesures;
#[cfg(windows)]
#[allow(dead_code, clippy::all)]
#[path = "../../nevermind-aide/src/serveur.rs"]
mod serveur;
#[cfg(windows)]
#[allow(dead_code, clippy::all)]
#[path = "../../nevermind-aide/src/systeme.rs"]
mod systeme;
#[cfg(windows)]
#[allow(dead_code, clippy::all)]
#[path = "../../nevermind-aide/src/windows/mod.rs"]
mod windows;

/// En administrateur, avant tout le reste : les DLL ne sont cherchées que dans System32, le dossier courant devient
/// System32, les dossiers de Windows sont remis tels que Windows les donne, la sécurité de COM est posée. Le même code
/// que NevermindAide.exe (`windows::preparer_processus`). Rend ce qui doit vivre jusqu'à la fin.
#[cfg(windows)]
pub fn preparer_processus() -> impl Drop {
    windows::preparer_processus()
}
