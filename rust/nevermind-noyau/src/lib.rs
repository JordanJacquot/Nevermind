//! Le noyau de Nevermind en Rust (passage complet en Rust, branche rust-complet).
//!
//! Nevermind est découpé en blocs, comme ses écrans en PowerShell : chaque bloc donne ses pages au kit (les mêmes
//! que `Get-PagesX`, au champ près) et reçoit les actions de l'écran (comme `Register-ActionVue` et `Invoke-ActionX`).
//! L'hôte (`nevermind-appli`, Tauri) assemble les pages de tous les blocs, les envoie au kit, passe chaque action aux
//! blocs jusqu'à ce que l'un la prenne, et exécute les effets demandés.
//!
//! LE CONTRAT CI-DESSOUS EST COMMUN À TOUS LES BLOCS : ne pas changer les signatures du trait `Bloc`, de `Contexte`
//! ni de `Effet` sans passer par l'intégration (on ajoute, on ne casse pas).

use serde_json::{Map, Value};
use std::path::PathBuf;
use std::time::Duration;

pub mod aide;
pub mod assistance;
pub mod contexte;
pub mod fausse_aide;
pub mod fichiers;
pub mod journal;
pub mod json_ps;
pub mod mises_a_jour;
pub mod moment;
pub mod pont;
pub mod registre;
pub mod sauvegarde;
pub mod signaux;
pub mod windows;
pub use assistance::pages_assistance;
pub use contexte::{Contexte, horodatage};

/// Ce qu'un bloc demande à l'hôte (il ne touche jamais la fenêtre lui-même).
#[derive(Debug, Clone, PartialEq)]
pub enum Effet {
    /// Ouvrir un écran du kit (comme `Open-EcranVue`).
    OuvrirEcran(String),
    /// Redessiner l'écran (comme `Update-EcranNevermind`).
    Rafraichir,
    /// Ouvrir une adresse ou un dossier (comme `Open-Url`).
    OuvrirAdresse(String),
    /// Une bulle d'information à l'écran (texte simple, en français).
    Annonce(String),
    /// Un message pour les autres blocs (par exemple « partie-commencee »), reçu par `Bloc::signal`.
    Signal { nom: String, donnees: Value },
    /// Quitter Nevermind proprement.
    Quitter,
}

/// La liste des effets demandés pendant un appel.
#[derive(Debug, Default)]
pub struct Effets(pub Vec<Effet>);

impl Effets {
    pub fn ajouter(&mut self, e: Effet) {
        self.0.push(e);
    }
}

/// Un bloc de Nevermind (Ton PC, Performances, Protection, Jeux, Réseau, Divers...).
pub trait Bloc: Send {
    /// Son nom court (pour le journal), par exemple "jeux".
    fn nom(&self) -> &'static str;

    /// Ses pages pour l'écran montré (`ecran` = `$script:Vue.Ecran`), à fusionner dans l'état envoyé au kit : mêmes clés
    /// et mêmes champs que la fonction `Get-PagesX` correspondante.
    fn pages(&mut self, ctx: &Contexte, ecran: &str) -> Map<String, Value>;

    /// Une action de l'écran (identifiant et valeur envoyés par kit.js). `true` si ce bloc l'a prise.
    fn action(&mut self, ctx: &Contexte, id: &str, valeur: &Value, effets: &mut Effets) -> bool;

    /// Le travail régulier (mesures, minuteries) ; renvoie le délai avant le prochain appel, `None` = plus besoin.
    /// `cache` : l'écran de Nevermind n'est pas regardé (réduit, derrière un jeu) : faire le moins possible.
    fn tic(&mut self, _ctx: &Contexte, _cache: bool, _effets: &mut Effets) -> Option<Duration> {
        None
    }

    /// Un message d'un autre bloc (voir `Effet::Signal`).
    fn signal(&mut self, _ctx: &Contexte, _nom: &str, _donnees: &Value, _effets: &mut Effets) {}

    /// Nevermind se ferme : remettre ce qui doit l'être, arrêter les mesures.
    fn fermer(&mut self, _ctx: &Contexte) {}
}

/// Un dossier d'essai (jamais le vrai dossier de données) : utilisé par les tests de chaque bloc.
pub fn dossier_essai(nom: &str) -> PathBuf {
    std::env::temp_dir().join(format!("nevermind-essai-{nom}-{}", std::process::id()))
}
