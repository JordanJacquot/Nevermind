//! Nevermind en Rust : le bloc « reseau ». Le scan des appareils du réseau et leur fiche (reseau.ps1,
//! reseau-avance.ps1), l'audit de sécurité (audit-reseau.ps1), la page Réseau (ecran-reseau.ps1), la page Connexion
//! (ecran-protection.ps1, onglets.ps1 : test du ping, DNS) et la carte réseau bridée (systeme.ps1).
//!
//! Tout ce qui touche le PC (ping, liste des voisins, carte réseau, Wi-Fi) passe par une `Sonde` : la vraie
//! (`windows`) dans l'appli, une fausse dans les tests. Ce qui écrit passe par l'Aide (`ctx.aide`).

pub mod appareil;
pub mod audit;
pub mod bloc;
pub mod fichiers;
pub mod net;
pub mod pages;
pub mod panneau;
pub mod scan;
pub mod sonde;
pub mod texte;
pub mod upnp;
#[cfg(windows)]
pub mod windows;

pub use bloc::BlocReseau;
