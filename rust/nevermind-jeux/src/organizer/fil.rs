//! Le fil à part de l'Organizer : il ne tourne que quand l'Organizer est allumé ou que sa vue est ouverte. Éteint et
//! vue fermée, il n'existe pas (rien ne consomme). Le bloc lui parle par une file de commandes ; il rend ses messages
//! (journal, bulles d'information) par une autre, vidée au tic du bloc.

use nevermind_noyau::{Contexte, Effet, Effets};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

/// Ce que le bloc demande au fil.
#[derive(Debug, Clone, PartialEq)]
pub enum Commande {
    /// Le bouton « Organizer Dofus » de la page Jeux (Show-OrgView $true).
    OuvrirVue,
    /// L'Organizer caché depuis Paramètres (Set-OrgVisible $false) : vue fermée, Organizer coupé.
    Cacher,
    /// Nevermind se ferme.
    Quitter,
}

/// La file des commandes, et la façon de réveiller le fil.
#[derive(Default)]
pub struct File {
    pub commandes: VecDeque<Commande>,
    /// Le fil a fini (ou va finir) : il ne lira plus rien.
    pub fini: bool,
    pub reveil: Option<Box<dyn Fn() + Send>>,
}

/// Ce que le fil rend au bloc.
#[derive(Default)]
pub struct Sortie {
    pub statuts: Vec<String>,
    pub journal: Vec<String>,
}

#[derive(Default)]
pub struct Partage {
    pub file: Mutex<File>,
    pub sortie: Mutex<Sortie>,
}

pub fn verrou<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Partage {
    pub fn journal(&self, m: &str) {
        verrou(&self.sortie).journal.push(m.to_string());
    }
    pub fn statut(&self, m: &str) {
        verrou(&self.sortie).statuts.push(m.to_string());
    }
}

/// Ce dont le fil a besoin pour démarrer.
#[derive(Debug, Clone)]
pub struct Params {
    pub donnees: PathBuf,
    /// Le thème choisi (réglage « Theme »).
    pub theme: String,
    /// La vue ouverte dès le départ.
    pub vue: bool,
    /// Le début du nom des programmes du jeu (Get-Process -Name 'Dofus*').
    pub programme: String,
    /// Essai sur un bureau à part : l'écoute globale de la souris n'est jamais posée.
    pub essai: bool,
    /// Essai sur un bureau à part : la fenêtre « devant » simulée (un bureau qui n'est pas à l'écran n'a pas de
    /// fenêtre devant), mise à jour par chaque mise au premier plan.
    pub devant_essai: Option<Arc<std::sync::atomic::AtomicIsize>>,
}

struct Fil {
    partage: Arc<Partage>,
    fil: Option<std::thread::JoinHandle<()>>,
}

static FIL: Mutex<Option<Fil>> = Mutex::new(None);
static DEMARRE: Mutex<bool> = Mutex::new(false);

/// Le nom du programme du jeu (comme Get-DofusWindows).
pub const PROGRAMME: &str = "dofus";

fn params(ctx: &Contexte, vue: bool) -> Params {
    Params {
        donnees: ctx.donnees.clone(),
        theme: ctx.reglage("Theme").map(|v| crate::json::texte_de(&v)).unwrap_or_default(),
        vue,
        programme: PROGRAMME.into(),
        essai: false,
        devant_essai: None,
    }
}

/// Envoie une commande au fil s'il tourne encore (`false` sinon).
fn envoyer(p: &Partage, c: Commande) -> bool {
    let mut f = verrou(&p.file);
    if f.fini {
        return false;
    }
    f.commandes.push_back(c);
    if let Some(r) = &f.reveil {
        r();
    }
    true
}

fn lancer(ctx: &Contexte, vue: bool) {
    let mut g = verrou(&FIL);
    if let Some(f) = g.as_ref() {
        let tourne = !verrou(&f.partage.file).fini;
        if tourne && (!vue || envoyer(&f.partage, Commande::OuvrirVue)) {
            return;
        }
    }
    // (l'ancien fil a fini : on reprend ses derniers messages avant de le remplacer)
    if let Some(mut f) = g.take() {
        if let Some(h) = f.fil.take() {
            let _ = h.join();
        }
        vider(ctx, &f.partage, None);
    }
    let partage = Arc::new(Partage::default());
    let fil = demarrer_fil(params(ctx, vue), partage.clone());
    *g = Some(Fil { partage, fil });
}

#[cfg(windows)]
fn demarrer_fil(p: Params, partage: Arc<Partage>) -> Option<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("Nevermind : Organizer".into())
        .spawn(move || crate::windows::organizer::tourner(p, partage))
        .ok()
}

#[cfg(not(windows))]
fn demarrer_fil(_p: Params, partage: Arc<Partage>) -> Option<std::thread::JoinHandle<()>> {
    verrou(&partage.file).fini = true;
    None
}

fn vider(ctx: &Contexte, p: &Partage, effets: Option<&mut Effets>) {
    let (statuts, journal) = {
        let mut s = verrou(&p.sortie);
        (std::mem::take(&mut s.statuts), std::mem::take(&mut s.journal))
    };
    for l in journal {
        ctx.journal(&l);
    }
    if let Some(e) = effets {
        for s in statuts {
            e.ajouter(Effet::Annonce(s));
        }
    }
}

/// Le fil tourne-t-il ?
pub fn actif() -> bool {
    verrou(&FIL).as_ref().is_some_and(|f| !verrou(&f.partage.file).fini)
}

/// Au tic du bloc : au premier, l'Organizer reprend s'il était allumé ; à chaque fois, ses messages sont rendus.
/// Mode test : rien ne démarre (aucune fenêtre, aucun raccourci).
pub fn tic(ctx: &Contexte, effets: &mut Effets) {
    {
        let mut d = verrou(&DEMARRE);
        if !*d {
            *d = true;
            if !ctx.essai && super::Config::lire(Some(ctx), &ctx.donnees).on {
                lancer(ctx, false);
            }
        }
    }
    if let Some(f) = verrou(&FIL).as_ref() {
        vider(ctx, &f.partage, Some(effets));
    }
}

/// Le délai avant le prochain tic du bloc : une seconde au plus tant que le fil tourne (pour rendre ses messages).
pub fn delai(d: Duration) -> Duration {
    if actif() { d.min(Duration::from_secs(1)) } else { d }
}

/// Le bouton « Organizer Dofus » (page Jeux, recherche) : la vue s'ouvre.
pub fn ouvrir_vue(ctx: &Contexte) {
    if ctx.essai {
        return;
    }
    lancer(ctx, true);
}

/// L'Organizer caché depuis Paramètres : `true` si le fil tournait (il ferme la vue et coupe l'Organizer lui-même).
pub fn cacher() -> bool {
    verrou(&FIL).as_ref().is_some_and(|f| envoyer(&f.partage, Commande::Cacher))
}

/// Nevermind se ferme : le fil rend les raccourcis, cache la barre et s'arrête.
pub fn fermer(ctx: &Contexte) {
    let f = verrou(&FIL).take();
    if let Some(mut f) = f {
        envoyer(&f.partage, Commande::Quitter);
        if let Some(h) = f.fil.take() {
            let _ = h.join();
        }
        vider(ctx, &f.partage, None);
    }
}

/// L'essai de la vraie barre et des raccourcis, sur un bureau Windows à part seulement (exemple `organizer_essai`,
/// lancé par outils\essai-organizer.ps1) : jamais dans Nevermind.
#[cfg(windows)]
#[doc(hidden)]
pub mod essai {
    use super::{Params, Partage, verrou};
    pub use crate::windows::organizer::{
        Etat, essai_captures as captures, essai_geste as geste, essai_messages as messages, essai_poster as poster,
    };
    use std::sync::Arc;

    /// Le fil avec ces réglages (sans Contexte) ; renvoie ce qu'il partage et son fil.
    pub fn lancer(p: Params) -> (Arc<Partage>, std::thread::JoinHandle<()>) {
        let partage = Arc::new(Partage::default());
        let fil = super::demarrer_fil(p, partage.clone()).expect("fil de l'Organizer");
        (partage, fil)
    }

    /// Demande au fil de s'arrêter, et l'attend.
    pub fn arreter(partage: &Partage, fil: std::thread::JoinHandle<()>) {
        super::envoyer(partage, super::Commande::Quitter);
        let _ = fil.join();
    }

    /// Le fil a-t-il fini ?
    pub fn fini(partage: &Partage) -> bool {
        verrou(&partage.file).fini
    }

    /// Les lignes du journal et les bulles rendues depuis la dernière fois.
    pub fn sorties(partage: &Partage) -> (Vec<String>, Vec<String>) {
        let mut s = verrou(&partage.sortie);
        (std::mem::take(&mut s.journal), std::mem::take(&mut s.statuts))
    }

    /// Envoie une commande au fil.
    pub fn commande(partage: &Partage, c: super::Commande) -> bool {
        super::envoyer(partage, c)
    }
}
