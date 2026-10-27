//! Les règles de l'Aide de Nevermind (plan Rust, étape 1a), reprises à l'identique de la version PowerShell
//! (Nevermind\fichiers\modules\aide-actions.ps1, NevermindAide.ps1, aide-client.ps1 et natif.cs).
//!
//! Seulement des décisions, sans rien toucher sur le PC : ce que Windows doit dire (un fichier existe, une carte
//! réseau, les antivirus...) est demandé au travers du trait [`demandes::Pc`], que NevermindAide.exe remplit avec les
//! vrais appels de Windows et que les tests remplissent avec un faux PC.
//!
//! Le corpus de cas `cas\aide\*.json` du dépôt est passé à ces règles par `cargo test` (tests\corpus.rs), comme il
//! est passé à la version PowerShell par outils\tester-aide*.ps1.
//!
//! Servira aussi à la DLL de Nevermind (plan, 3.7) : règle « fermable », nom du canal, variables dangereuses.

#![deny(unsafe_op_in_unsafe_fn)]
#![forbid(unsafe_code)]

pub mod canal;
pub mod carte;
pub mod chemin;
pub mod client;
pub mod demandes;
pub mod demarrage;
pub mod dossiers;
pub mod exes;
pub mod fermeture;
pub mod nettoyage;
pub mod ps;
pub mod registre;
pub mod restauration;
pub mod serveur;
pub mod sha256;
pub mod sid;
pub mod tache;
pub mod taches_compte;
pub mod texte;
pub mod variables;
mod xml;
