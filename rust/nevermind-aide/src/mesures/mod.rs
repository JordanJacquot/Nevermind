//! Les mesures continues que l'Aide fait tourner pour Nevermind (plan Rust, étape 1b) : PresentMon (FPS), la trace
//! réseau (le vrai serveur du jeu pendant « Lag en ligne ») et les compteurs TCP (Trafic). Elles demandent les droits
//! administrateur ; Nevermind en lit les résultats par le canal, comme avec la version PowerShell.
//!
//! Seulement la logique (ce qui est gardé, compté, rendu) : les appels de Windows passent par le trait `Systeme`, que
//! les essais remplacent pour rejouer des enregistrements (cas\mesures\*.json).

pub mod flux;
pub mod fps;
pub mod ip;
pub mod tcp;

use crate::systeme::Systeme;
use std::sync::Mutex;

/// Les mesures de l'Aide, gardées d'une demande à l'autre
#[derive(Default)]
pub struct Mesures {
    pub fps: fps::Fps,
    pub flux: flux::Flux,
    pub tcp: Mutex<tcp::CompteursTcp>,
}

impl Mesures {
    /// Quand l'Aide s'arrête : PresentMon et la trace réseau s'arrêtent avec elle
    pub fn arreter_tout(&self, s: &dyn Systeme) {
        self.fps.arreter(s);
        self.flux.arreter();
    }
}
