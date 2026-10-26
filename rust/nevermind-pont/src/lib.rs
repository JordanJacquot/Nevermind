//! NevermindPont.dll : le pont entre Nevermind (PowerShell) et les règles de l'Aide en Rust (plan Rust, 3.7 et étape
//! 1a, point D). Nevermind ne garde plus sa propre copie de ces règles (aide-actions.ps1) : il appelle ici LE même code
//! que NevermindAide.exe (la bibliothèque `aide-regles`).
//!
//! Ce que Nevermind demande (voir [`demandes`]) :
//! - la règle « fermable » de la bulle RAM et du mode jeu (`fermeture.refus`) ;
//! - le nom du canal de l'Aide (`canal`) ;
//! - le programme d'une commande écrite par Windows (`dossiers.programmes`), pour les antivirus inscrits ;
//! - les dossiers de Windows que l'Aide vide (`nettoyage.dossiers`), montrés dans la page Nettoyage.
//!
//! L'interface C (voir [`ffi`]) est très simple : un texte JSON en entrée, un texte JSON en sortie, une fonction pour
//! rendre ce texte. Toute erreur Rust est rattrapée à la frontière : Nevermind reçoit `{ "ok": false, ... }` et
//! continue, il ne se ferme jamais à cause de la DLL. La DLL ne lit rien sur le PC et ne change rien : elle décide
//! seulement, à partir de ce que Nevermind lui donne.

#![deny(unsafe_op_in_unsafe_fn)]

pub mod demandes;
mod ffi;
