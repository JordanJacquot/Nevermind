//! NevermindAide.exe : la seule partie de Nevermind qui tourne en administrateur (plan Rust, étape 1a). Remplace le
//! lanceur C# (outils\lanceur.cs, partie AIDE) et les scripts NevermindAide.ps1 et modules\aide-actions.ps1.
//!
//! Lancé par la tâche planifiée « \Mindset\Nevermind (aide) » que Mindset crée à l'installation, en SYSTEM, avec
//! `-Sid <utilisateur>` (inscrit par l'administrateur qui a créé la tâche, jamais déduit d'autre chose) ; ou, en
//! secours, par la demande de Windows, sous le compte de l'utilisateur avec les droits administrateur (le SID reçu doit
//! alors être celui de ce compte). Aucune fenêtre, jamais (en SYSTEM, personne ne la verrait). Aucun fichier écrit
//! hors du dossier protégé, et rien lu chez l'utilisateur (ni variables, ni profil, ni modules).
//!
//! Même canal, mêmes demandes, mêmes réponses que la version PowerShell : Nevermind ne voit pas la différence.

#![windows_subsystem = "windows"]
#![deny(unsafe_op_in_unsafe_fn)]

mod actions;
#[cfg(test)]
mod essais;
mod serveur;
mod systeme;
mod windows;

use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
    let sid = aide_regles::sid::lire_arguments(&args);
    // (avant tout : DLL de System32 seulement, dossier courant System32, sécurité de COM)
    let _processus = windows::preparer_processus();
    let pc = Arc::new(windows::Windows::new());
    serveur::lancer(pc, sid.as_deref());
    // Aucun fil laissé par une demande ne garde le processus ouvert
    std::process::exit(0);
}
