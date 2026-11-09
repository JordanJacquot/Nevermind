//! Un seul geste qui change le PC à la fois, toutes pages confondues (lot A du second audit, G-A et M-C).
//!
//! Depuis les travaux de fond (audit G3), deux pages pouvaient changer le PC en même temps : un DNS choisi dans Réseau
//! pendant « Appliquer » de Ton PC disparaissait de la sauvegarde, et « Couper le Bureau à distance » fait pendant
//! « Couper les envois » entrait dans l'historique du Trafic (« Annuler » le rouvrait). Maintenant :
//!
//! - chaque geste qui change le PC prend le verrou commun du contexte ([`Contexte::prendre_geste`]) au moment du clic ;
//!   si un autre geste le tient, le clic est REFUSÉ avec « Attends la fin de « <nom> » (<page>) », et rien ne part
//!   tout seul plus tard ;
//! - chaque geste a son propre journal d'action, attaché au verrou ([`Contexte::commencer_changements`]...) : le
//!   journal d'un geste ne peut plus recevoir le changement d'un autre ;
//! - les gestes automatiques (mode jeu au début et à la fin d'une partie) ne sont jamais refusés : ils passent en
//!   priorité ([`Contexte::geste_prioritaire`]) ; les profils de jeux, qui notent dans la sauvegarde, attendent leur
//!   tour ([`Contexte::attendre_geste`]) ;
//! - « Appliquer » rend le verrou pendant le point de restauration (jusqu'à 10 min, rien de noté dans la sauvegarde)
//!   et le reprend ensuite, à son tour ([`Contexte::suspendre_geste`], [`Contexte::reprendre_geste`]) ;
//! - le verrou est rendu quoi qu'il arrive : à la fin du geste ([`Geste`] le rend en disparaissant, même pendant une
//!   panique), après une panique qui aurait empoisonné le verrou (il est repris, jamais bloquant), et à la fermeture de
//!   Nevermind ([`Contexte::rendre_les_gestes`]).
//!
//! Chaque écriture qui change le PC vérifie que son fil tient bien un geste ([`Contexte::exiger_geste`]) : un geste
//! oublié échoue au lieu de passer en silence. Le garde-fou `tests/gestes_sous_verrou.rs` relève chaque endroit des
//! sources qui change le PC et vérifie qu'il passe par le verrou.

use crate::Contexte;
use serde_json::Value;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

/// Un geste qui tient (ou attend) le verrou
#[derive(Debug, Clone)]
struct Tenu {
    id: u64,
    nom: String,
    page: String,
    /// Le fil qui fait le geste (celui du clic, puis celui du travail de fond)
    fil: ThreadId,
    /// Le journal de l'action (`$script:RunLog`) : `None` hors d'une action
    journal: Option<Vec<Value>>,
}

impl Tenu {
    fn texte(&self) -> String {
        texte_refus(&self.nom, &self.page)
    }
}

#[derive(Default)]
struct Etat {
    /// Le geste de l'utilisateur qui tient le verrou
    tenu: Option<Tenu>,
    /// Les gestes automatiques passés en priorité (mode jeu) : ils ne notent rien dans la sauvegarde
    prioritaires: Vec<Tenu>,
    /// Un geste qui a rendu le verrou le temps du point de restauration (ajustement B) ; `attend` : il veut le reprendre
    suspendus: Vec<(Tenu, bool)>,
    /// Les gestes automatiques qui attendent leur tour (profils de jeux), dans l'ordre
    file: Vec<u64>,
    suivant: u64,
}

impl Etat {
    fn nouvel_id(&mut self) -> u64 {
        self.suivant += 1;
        self.suivant
    }

    /// Ce qui empêche un nouveau geste de l'utilisateur de partir : le geste en cours, un geste prioritaire, un geste
    /// qui reprend le verrou après son point de restauration, ou un geste automatique qui attend son tour
    fn refus(&self) -> Option<String> {
        if let Some(t) = &self.tenu {
            return Some(t.texte());
        }
        if let Some(t) = self.prioritaires.first() {
            return Some(t.texte());
        }
        if let Some((t, _)) = self.suspendus.iter().find(|(_, attend)| *attend) {
            return Some(t.texte());
        }
        None
    }

    /// Le geste de ce fil (celui de l'utilisateur ou un prioritaire), s'il en tient un
    fn du_fil(&mut self, fil: ThreadId) -> Option<&mut Tenu> {
        if let Some(t) = self.tenu.as_mut().filter(|t| t.fil == fil) {
            return Some(t);
        }
        self.prioritaires.iter_mut().find(|t| t.fil == fil)
    }
}

/// Le verrou commun « un geste qui change le PC à la fois » (dans le contexte, partagé par tous les blocs)
#[derive(Default)]
pub struct Verrou {
    etat: Mutex<Etat>,
    libre: Condvar,
}

impl Verrou {
    /// L'état, même après une panique d'un autre fil (un verrou empoisonné est repris, jamais bloquant)
    fn etat(&self) -> MutexGuard<'_, Etat> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn rendre(&self, id: u64) {
        let mut e = self.etat();
        if e.tenu.as_ref().is_some_and(|t| t.id == id) {
            e.tenu = None;
        }
        e.prioritaires.retain(|t| t.id != id);
        e.suspendus.retain(|(t, _)| t.id != id);
        e.file.retain(|x| *x != id);
        drop(e);
        self.libre.notify_all();
    }

    /// Tests : empoisonne le verrou (un fil panique en le tenant), pour vérifier qu'il est repris
    #[doc(hidden)]
    pub fn empoisonner(self: &Arc<Self>) {
        let v = self.clone();
        let _ = std::thread::spawn(move || {
            let _g = v.etat.lock();
            panic!("panique d'essai en tenant le verrou des gestes");
        })
        .join();
    }
}

/// Le texte d'un clic refusé : « Attends la fin de « Appliquer » (Ton PC) »
pub fn texte_refus(nom: &str, page: &str) -> String {
    format!("Attends la fin de « {nom} » ({page})")
}

/// Un geste qui tient le verrou. Il le rend en disparaissant (fin du geste, panique du travail, travail pas lancé).
/// Il peut changer de fil : pris au clic, sur le fil des blocs, il part avec le travail de fond ([`Geste::ici`]).
pub struct Geste {
    verrou: Arc<Verrou>,
    id: u64,
    nom: String,
    page: String,
}

impl std::fmt::Debug for Geste {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Geste").field("nom", &self.nom).field("page", &self.page).finish()
    }
}

impl Geste {
    /// Le geste se fait maintenant sur ce fil (le travail de fond qui l'emporte)
    pub fn ici(&self) {
        let fil = std::thread::current().id();
        let mut e = self.verrou.etat();
        if let Some(t) = e.tenu.as_mut().filter(|t| t.id == self.id) {
            t.fil = fil;
        }
        if let Some(t) = e.prioritaires.iter_mut().find(|t| t.id == self.id) {
            t.fil = fil;
        }
        if let Some((t, _)) = e.suspendus.iter_mut().find(|(t, _)| t.id == self.id) {
            t.fil = fil;
        }
    }

    /// Son nom (« Appliquer »)
    pub fn nom(&self) -> &str {
        &self.nom
    }

    /// Sa page (« Ton PC »)
    pub fn page(&self) -> &str {
        &self.page
    }

    /// Ce geste tient-il encore le verrou ? (faux après la fermeture de Nevermind)
    pub fn tient(&self) -> bool {
        let e = self.verrou.etat();
        e.tenu.as_ref().is_some_and(|t| t.id == self.id) || e.prioritaires.iter().any(|t| t.id == self.id)
    }
}

impl Drop for Geste {
    fn drop(&mut self) {
        self.verrou.rendre(self.id);
    }
}

/// L'attente la plus longue d'un geste qui reprend le verrou après son point de restauration (ou d'un profil de jeu)
pub const ATTENTE_REPRISE: Duration = Duration::from_secs(30 * 60);

impl Contexte {
    fn verrou_gestes(&self) -> &Arc<Verrou> {
        &self.gestes
    }

    /// Un clic qui change le PC : le verrou est pris, ou le clic est refusé avec « Attends la fin de « X » (Page) ».
    /// Rien n'est mis en attente : un clic refusé ne part jamais tout seul plus tard.
    pub fn prendre_geste(&self, nom: &str, page: &str) -> Result<Geste, String> {
        let v = self.verrou_gestes();
        let mut e = v.etat();
        if let Some(refus) = e.refus().or_else(|| (!e.file.is_empty()).then(|| texte_refus("un réglage", "en attente")))
        {
            drop(e);
            self.journal(&format!("Geste « {nom} » ({page}) refusé : {refus}"));
            return Err(refus);
        }
        let id = e.nouvel_id();
        e.tenu = Some(Tenu { id, nom: nom.into(), page: page.into(), fil: std::thread::current().id(), journal: None });
        Ok(Geste { verrou: v.clone(), id, nom: nom.into(), page: page.into() })
    }

    /// Un geste automatique qui ne doit jamais être refusé ni retardé (le mode jeu au début et à la fin d'une partie,
    /// la remise en état à la fermeture) : il passe en priorité, même si un geste de l'utilisateur tient le verrou.
    /// Il ne note rien dans la sauvegarde ; il a son propre journal. Pendant ce temps, les clics sont refusés.
    pub fn geste_prioritaire(&self, nom: &str, page: &str) -> Geste {
        let v = self.verrou_gestes();
        let mut e = v.etat();
        let id = e.nouvel_id();
        e.prioritaires.push(Tenu {
            id,
            nom: nom.into(),
            page: page.into(),
            fil: std::thread::current().id(),
            journal: None,
        });
        Geste { verrou: v.clone(), id, nom: nom.into(), page: page.into() }
    }

    /// Un geste automatique qui note dans la sauvegarde (un profil de jeu) : il attend son tour, dans l'ordre, au
    /// plus `delai` (hors du fil des blocs). Pendant qu'il attend, les nouveaux clics sont refusés (il passe avant).
    pub fn attendre_geste(&self, nom: &str, page: &str, delai: Duration) -> Result<Geste, String> {
        let v = self.verrou_gestes();
        let fin = Instant::now() + delai;
        let mut e = v.etat();
        let id = e.nouvel_id();
        e.file.push(id);
        loop {
            let premier = e.file.first() == Some(&id);
            if premier && e.refus().is_none() {
                e.file.retain(|x| *x != id);
                e.tenu = Some(Tenu {
                    id,
                    nom: nom.into(),
                    page: page.into(),
                    fil: std::thread::current().id(),
                    journal: None,
                });
                return Ok(Geste { verrou: v.clone(), id, nom: nom.into(), page: page.into() });
            }
            let reste = fin.saturating_duration_since(Instant::now());
            if reste.is_zero() {
                let refus = e.refus().unwrap_or_default();
                e.file.retain(|x| *x != id);
                drop(e);
                v.libre.notify_all();
                return Err(refus);
            }
            e = v.libre.wait_timeout(e, reste.min(Duration::from_millis(500))).unwrap_or_else(|p| p.into_inner()).0;
        }
    }

    /// Ce que dirait un clic refusé maintenant (`None` : le verrou est libre)
    pub fn refus_geste(&self) -> Option<String> {
        let e = self.verrou_gestes().etat();
        e.refus().or_else(|| (!e.file.is_empty()).then(|| texte_refus("un réglage", "en attente")))
    }

    /// Le geste en cours (« « Appliquer » (Ton PC) »), pour l'écran et le journal
    pub fn geste_en_cours(&self) -> Option<(String, String)> {
        let e = self.verrou_gestes().etat();
        e.tenu.as_ref().or(e.prioritaires.first()).map(|t| (t.nom.clone(), t.page.clone()))
    }

    /// Une écriture qui change le PC vérifie que son fil tient un geste (de l'utilisateur ou prioritaire) : sinon
    /// elle est refusée, rien n'est changé.
    pub fn exiger_geste(&self) -> Result<(), String> {
        let fil = std::thread::current().id();
        let mut e = self.verrou_gestes().etat();
        if e.du_fil(fil).is_some() {
            return Ok(());
        }
        let refus = e.refus();
        drop(e);
        self.journal(&format!(
            "Changement du PC refusé : aucun geste ne tient le verrou sur ce fil{}",
            refus.map(|r| format!(" ({r})")).unwrap_or_default()
        ));
        Err("Un autre réglage est en cours : rien n'a été changé. Réessaie dans un instant.".into())
    }

    /// Ce fil tient-il un geste ?
    pub fn geste_tenu_ici(&self) -> bool {
        let fil = std::thread::current().id();
        self.verrou_gestes().etat().du_fil(fil).is_some()
    }

    /// Ajustement B : le geste de ce fil rend le verrou le temps d'une longue attente qui ne change rien de ce que
    /// notent les autres (le point de restauration, jusqu'à 10 min) ; les autres pages restent utilisables. Rend
    /// l'identifiant à donner à [`Contexte::reprendre_geste`] (`None` : ce fil ne tenait pas le verrou).
    pub fn suspendre_geste(&self) -> Option<u64> {
        let fil = std::thread::current().id();
        let v = self.verrou_gestes();
        let mut e = v.etat();
        let t = e.tenu.take_if(|t| t.fil == fil)?;
        let id = t.id;
        e.suspendus.push((t, false));
        drop(e);
        v.libre.notify_all();
        Some(id)
    }

    /// La suite du geste suspendu : il reprend le verrou à son tour (il attend la fin du geste en cours, au plus
    /// [`ATTENTE_REPRISE`]) ; pendant qu'il attend, les nouveaux clics sont refusés. Fermeture de Nevermind ou
    /// attente trop longue : une erreur, la suite du geste ne change rien.
    pub fn reprendre_geste(&self, id: u64) -> Result<(), String> {
        let v = self.verrou_gestes();
        let fin = Instant::now() + ATTENTE_REPRISE;
        let mut e = v.etat();
        let mut annonce = false;
        loop {
            let Some(i) = e.suspendus.iter().position(|(t, _)| t.id == id) else {
                return Err("Nevermind se ferme : la suite n'a pas été faite.".into());
            };
            e.suspendus[i].1 = true;
            if e.tenu.is_none() && e.prioritaires.is_empty() {
                let (mut t, _) = e.suspendus.remove(i);
                t.fil = std::thread::current().id();
                e.tenu = Some(t);
                return Ok(());
            }
            if !annonce {
                annonce = true;
                let r = e.refus().unwrap_or_default();
                drop(e);
                self.journal(&format!("Geste repris après le point de restauration : {r}"));
                e = v.etat();
                continue;
            }
            let reste = fin.saturating_duration_since(Instant::now());
            if reste.is_zero() {
                let r = e.refus().unwrap_or_default();
                return Err(format!("{r} : la suite n'a pas été faite, rien d'autre n'a été changé."));
            }
            e = v.libre.wait_timeout(e, reste.min(Duration::from_millis(500))).unwrap_or_else(|p| p.into_inner()).0;
        }
    }

    /// Fermeture de Nevermind (ajustement C) : le verrou est rendu, quel que soit le geste qui le tient encore (un
    /// travail de fond qui attend l'Aide). Un nouveau geste passe ensuite ; le geste coupé ne rend rien d'autre.
    pub fn rendre_les_gestes(&self) {
        let v = self.verrou_gestes();
        let mut e = v.etat();
        let avant: Vec<String> = e
            .tenu
            .iter()
            .chain(e.prioritaires.iter())
            .chain(e.suspendus.iter().map(|(t, _)| t))
            .map(Tenu::texte)
            .collect();
        e.tenu = None;
        e.prioritaires.clear();
        e.suspendus.clear();
        drop(e);
        v.libre.notify_all();
        if !avant.is_empty() {
            self.journal(&format!("Fermeture : verrou des gestes rendu ({})", avant.join(", ")));
        }
    }

    /// Tests : le verrou des gestes (pour l'empoisonner)
    #[doc(hidden)]
    pub fn verrou_des_gestes(&self) -> Arc<Verrou> {
        self.gestes.clone()
    }

    // -----------------------------------------------------------------------------------------------------------
    // Le journal de l'action du geste (`$script:RunLog`), attaché au verrou (M-C)
    // -----------------------------------------------------------------------------------------------------------

    /// Une action commence : ses changements sont notés dans le journal du geste de ce fil
    /// (`$script:RunLog = New-Object ArrayList`). Un fil sans geste n'a pas de journal.
    pub fn commencer_changements(&self) {
        let fil = std::thread::current().id();
        if let Some(t) = self.verrou_gestes().etat().du_fil(fil) {
            t.journal = Some(Vec::new());
        }
    }

    /// Un changement de l'action en cours (`$script:RunLog.Add(...)`), dans le journal du geste de CE fil seulement
    pub fn noter_changement(&self, e: Value) -> bool {
        let fil = std::thread::current().id();
        match self.verrou_gestes().etat().du_fil(fil).and_then(|t| t.journal.as_mut()) {
            Some(l) => {
                l.push(e);
                true
            }
            None => false,
        }
    }

    /// Une action est en cours sur ce fil (`$null -ne $script:RunLog`)
    pub fn changements_en_cours(&self) -> bool {
        let fil = std::thread::current().id();
        self.verrou_gestes().etat().du_fil(fil).is_some_and(|t| t.journal.is_some())
    }

    /// L'action de ce fil est finie : ses changements (`$script:RunLog = $null`)
    pub fn finir_changements(&self) -> Vec<Value> {
        let fil = std::thread::current().id();
        self.verrou_gestes().etat().du_fil(fil).and_then(|t| t.journal.take()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx(nom: &str) -> Contexte {
        Contexte::nouveau(
            crate::dossier_essai(&format!("{nom}-a")),
            crate::dossier_essai(&format!("{nom}-d")),
            false,
            true,
        )
    }

    #[test]
    fn un_seul_geste_a_la_fois_et_le_texte_du_refus() {
        let c = ctx("gestes-un");
        let g = c.prendre_geste("Appliquer", "Ton PC").unwrap();
        assert_eq!(c.prendre_geste("DNS", "Réseau").unwrap_err(), "Attends la fin de « Appliquer » (Ton PC)");
        assert!(c.exiger_geste().is_ok());
        drop(g);
        assert!(c.prendre_geste("DNS", "Réseau").is_ok(), "le verrou rendu, un nouveau geste passe");
    }

    #[test]
    fn un_fil_sans_geste_ne_change_rien() {
        let c = ctx("gestes-fil").partage();
        let g = c.prendre_geste("Couper les envois", "Trafic").unwrap();
        let c2 = c.clone();
        let r = std::thread::spawn(move || c2.exiger_geste()).join().unwrap();
        assert!(r.is_err(), "un autre fil ne profite pas du verrou");
        let c3 = c.clone();
        let r = std::thread::spawn(move || {
            g.ici();
            c3.exiger_geste()
        })
        .join()
        .unwrap();
        assert!(r.is_ok(), "le geste emporté par le travail de fond");
        assert!(c.prendre_geste("x", "y").is_ok(), "rendu à la fin du fil");
    }

    #[test]
    fn le_journal_est_celui_du_geste_de_ce_fil() {
        let c = ctx("gestes-journal").partage();
        let g = c.prendre_geste("Couper les envois", "Trafic").unwrap();
        c.commencer_changements();
        c.noter_changement(json!(1));
        let c2 = c.clone();
        let note = std::thread::spawn(move || {
            let _p = c2.geste_prioritaire("Fin de partie", "Mode jeu");
            c2.noter_changement(json!(2))
        })
        .join()
        .unwrap();
        assert!(!note, "le prioritaire n'a pas ouvert de journal : rien n'est noté");
        assert_eq!(c.finir_changements(), vec![json!(1)]);
        drop(g);
    }

    #[test]
    fn une_panique_rend_le_verrou_et_un_verrou_empoisonne_est_repris() {
        let c = ctx("gestes-panique").partage();
        let c2 = c.clone();
        let _ = std::thread::spawn(move || {
            let _g = c2.prendre_geste("Tout restaurer", "Ton PC").unwrap();
            panic!("boum");
        })
        .join();
        assert!(c.prendre_geste("Nouveau", "Réseau").is_ok(), "après la panique");
        c.verrou_des_gestes().empoisonner();
        let g = c.prendre_geste("Encore", "Réseau");
        assert!(g.is_ok(), "un verrou empoisonné est repris");
    }

    #[test]
    fn le_prioritaire_passe_et_refuse_les_clics() {
        let c = ctx("gestes-prio");
        let g = c.prendre_geste("Appliquer", "Ton PC").unwrap();
        let p = c.geste_prioritaire("Fin de partie", "Mode jeu");
        assert!(p.tient());
        drop(g);
        assert_eq!(c.prendre_geste("DNS", "Réseau").unwrap_err(), "Attends la fin de « Fin de partie » (Mode jeu)");
        drop(p);
        assert!(c.prendre_geste("DNS", "Réseau").is_ok());
    }

    #[test]
    fn suspendu_pendant_le_point_de_restauration_puis_repris_a_son_tour() {
        let c = ctx("gestes-suspendu").partage();
        let g = c.prendre_geste("Appliquer", "Ton PC").unwrap();
        let id = c.suspendre_geste().unwrap();
        assert!(c.exiger_geste().is_err(), "rien ne change pendant le point de restauration");
        let autre = c.prendre_geste("DNS", "Réseau").expect("les autres pages restent utilisables");
        let c2 = c.clone();
        let (feu, recu) = std::sync::mpsc::channel();
        let h = std::thread::spawn(move || {
            let r = c2.reprendre_geste(id);
            feu.send(()).unwrap();
            r.map(|_| c2.exiger_geste().is_ok())
        });
        assert!(recu.recv_timeout(Duration::from_millis(300)).is_err(), "il attend la fin du DNS");
        assert_eq!(
            c.prendre_geste("Bloquer", "Trafic").unwrap_err(),
            "Attends la fin de « DNS » (Réseau)",
            "un clic pendant ce temps est refusé"
        );
        drop(autre);
        assert_eq!(h.join().unwrap(), Ok(true));
        assert_eq!(c.prendre_geste("Bloquer", "Trafic").unwrap_err(), "Attends la fin de « Appliquer » (Ton PC)");
        drop(g);
        assert!(c.prendre_geste("Bloquer", "Trafic").is_ok());
    }

    #[test]
    fn un_geste_automatique_attend_son_tour() {
        let c = ctx("gestes-file").partage();
        let g = c.prendre_geste("Appliquer", "Ton PC").unwrap();
        let c2 = c.clone();
        let h = std::thread::spawn(move || {
            c2.attendre_geste("Profil de jeu", "Jeux", Duration::from_secs(10))
                .map(|g| (g.nom().to_string(), c2.exiger_geste().is_ok()))
        });
        std::thread::sleep(Duration::from_millis(200));
        drop(g);
        assert_eq!(h.join().unwrap(), Ok(("Profil de jeu".to_string(), true)));
        assert!(c.prendre_geste("x", "y").is_ok());
    }

    #[test]
    fn la_fermeture_rend_le_verrou() {
        let c = ctx("gestes-fermeture");
        let g = c.prendre_geste("Couper les envois", "Trafic").unwrap();
        c.rendre_les_gestes();
        assert!(!g.tient());
        let n = c.prendre_geste("Fin de partie", "Mode jeu").expect("un nouveau geste passe après la fermeture");
        drop(g);
        assert!(n.tient(), "le geste coupé ne rend pas le verrou d'un autre");
    }
}
