//! Les travaux de fond d'un bloc (audit G3) : un geste long (point de restauration, SMBv1, services, recherche des
//! jeux, analyse WMI...) ne doit jamais figer le fil des blocs, qui fait aussi les FPS, la détection des parties et
//! les pages. Comme les « tâches » de Mindset (`Start-Tache`), mais typées : le travail rend ce que le bloc veut
//! (une session, une liste de jeux...), pas seulement du JSON.
//!
//! - Le bloc garde un [`Travaux<R>`] et appelle [`Travaux::lancer`] avec une clé (« restauration », « smb1 »...) :
//!   le travail part sur un fil à lui, avec le contexte partagé ([`Fond::ctx`]). Une clé déjà en cours n'est pas
//!   relancée (un double clic ne fait pas le geste deux fois).
//! - Le travail peut montrer tout de suite un texte d'attente ou une bulle ([`Fond::effet`]) : l'effet est rendu
//!   au bloc sans attendre la fin.
//! - À la fin, le fil prévient le contexte ; l'hôte appelle alors `Bloc::travaux_finis` sur le fil des blocs, et le
//!   bloc relève ses résultats avec [`Travaux::relever`]. L'état du bloc n'est donc jamais touché depuis un autre fil.
//! - Une panique du travail devient une erreur (`Err`), notée au journal.
//!
//! Sans contexte partagé ([`Contexte::partage`] : les tests d'un bloc seul, les juges), le travail est fait sur
//! place, tout de suite, et son résultat attend le prochain [`Travaux::relever`] : le bloc se comporte alors
//! exactement comme avant (le relevé fait juste après le lancement rend déjà le résultat).

use crate::{Contexte, Effet, Geste};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

/// Ce qu'un travail rend au bloc : un effet montré en chemin, ou sa fin.
#[derive(Debug)]
pub enum Rendu<R> {
    /// Un effet demandé pendant le travail (texte d'attente, bulle...), à appliquer tout de suite
    Effet(Effet),
    /// Le travail `cle` est fini : son résultat, ou l'erreur d'une panique
    Fini { cle: String, resultat: Result<R, String> },
}

/// Ce que reçoit un travail : le contexte (partagé), et l'envoi d'un effet au bloc sans attendre la fin.
pub struct Fond<'a> {
    pub ctx: &'a Contexte,
    envoi: &'a dyn Fn(Effet),
}

impl Fond<'_> {
    /// Un effet rendu au bloc tout de suite (par exemple `signaux::occupe("Désactivation de SMBv1...")`)
    pub fn effet(&self, e: Effet) {
        (self.envoi)(e)
    }
}

/// Les travaux de fond d'un bloc, chacun sous sa clé.
pub struct Travaux<R> {
    envoi: Sender<Rendu<R>>,
    reception: Receiver<Rendu<R>>,
    en_cours: Vec<String>,
    /// Les rendus d'un travail fait sur place (sans contexte partagé)
    sur_place: VecDeque<Rendu<R>>,
}

impl<R> std::fmt::Debug for Travaux<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Travaux").field("en_cours", &self.en_cours).finish_non_exhaustive()
    }
}

impl<R: Send + 'static> Default for Travaux<R> {
    fn default() -> Self {
        let (envoi, reception) = channel();
        Travaux { envoi, reception, en_cours: Vec::new(), sur_place: VecDeque::new() }
    }
}

/// Le texte d'une panique, comme le message d'une exception
pub fn texte_panique(p: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = p.downcast_ref::<&str>() {
        format!("erreur imprévue ({s})")
    } else if let Some(s) = p.downcast_ref::<String>() {
        format!("erreur imprévue ({s})")
    } else {
        "erreur imprévue".into()
    }
}

impl<R: Send + 'static> Travaux<R> {
    /// Le travail `cle` est-il en cours ?
    pub fn en_cours(&self, cle: &str) -> bool {
        self.en_cours.iter().any(|c| c == cle)
    }

    /// Un travail au moins est-il en cours ?
    pub fn occupe(&self) -> bool {
        !self.en_cours.is_empty()
    }

    /// Lance le travail `cle` sur un fil à lui (sur place sans contexte partagé). `false` s'il est déjà en cours :
    /// rien n'est relancé.
    pub fn lancer<F>(&mut self, ctx: &Contexte, cle: &str, travail: F) -> bool
    where
        F: FnOnce(&Fond) -> R + Send + 'static,
    {
        if self.en_cours(cle) {
            ctx.journal(&format!("Travail de fond : {cle} déjà en cours, pas relancé"));
            return false;
        }
        self.en_cours.push(cle.to_string());
        let Some(partage) = ctx.partage_actuel() else {
            self.faire_sur_place(ctx, cle, travail);
            return true;
        };
        // (le travail est gardé à part : si le fil ne peut pas démarrer, il est fait sur place, comme avant)
        let boite = Arc::new(Mutex::new(Some(travail)));
        let pour_fil = boite.clone();
        let envoi = self.envoi.clone();
        let c = cle.to_string();
        let lance = std::thread::Builder::new().name(format!("nevermind-travail-{cle}")).spawn(move || {
            let Some(t) = pour_fil.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
            let envoi_effet = {
                let envoi = envoi.clone();
                let ctx = partage.clone();
                move |e: Effet| {
                    let _ = envoi.send(Rendu::Effet(e));
                    ctx.travail_a_relever();
                }
            };
            let fond = Fond { ctx: &partage, envoi: &envoi_effet };
            let resultat = catch_unwind(AssertUnwindSafe(|| t(&fond))).map_err(|p| texte_panique(p.as_ref()));
            if let Err(e) = &resultat {
                partage.journal(&format!("Travail de fond : {c} en erreur ({e})"));
            }
            // (le rendu d'abord, puis le signal : l'hôte qui voit le signal trouve toujours le rendu)
            let _ = envoi.send(Rendu::Fini { cle: c, resultat });
            partage.travail_a_relever();
        });
        if let Err(e) = lance {
            ctx.journal(&format!("Travail de fond : {cle} fait sur place, le fil n'a pas démarré ({e})"));
            if let Some(t) = boite.lock().unwrap_or_else(|e| e.into_inner()).take() {
                self.faire_sur_place(ctx, cle, t);
            }
        }
        true
    }

    /// Lance un travail qui change le PC avec son geste (pris au clic, [`Contexte::prendre_geste`]) : le geste part
    /// avec le travail sur son fil, et rend le verrou à la fin, même après une panique. `false` (clé déjà en cours) :
    /// le geste est rendu tout de suite.
    pub fn lancer_geste<F>(&mut self, ctx: &Contexte, cle: &str, geste: Geste, travail: F) -> bool
    where
        F: FnOnce(&Fond) -> R + Send + 'static,
    {
        self.lancer(ctx, cle, move |f| {
            geste.ici();
            let r = travail(f);
            drop(geste);
            r
        })
    }

    fn faire_sur_place<F>(&mut self, ctx: &Contexte, cle: &str, travail: F)
    where
        F: FnOnce(&Fond) -> R,
    {
        let effets = RefCell::new(Vec::new());
        let envoi = |e: Effet| effets.borrow_mut().push(e);
        let fond = Fond { ctx, envoi: &envoi };
        let resultat = catch_unwind(AssertUnwindSafe(|| travail(&fond))).map_err(|p| texte_panique(p.as_ref()));
        if let Err(e) = &resultat {
            ctx.journal(&format!("Travail de fond : {cle} en erreur ({e})"));
        }
        self.sur_place.extend(effets.into_inner().into_iter().map(Rendu::Effet));
        self.sur_place.push_back(Rendu::Fini { cle: cle.to_string(), resultat });
    }

    /// Ce que les travaux ont rendu depuis le dernier relevé, dans l'ordre (sur le fil des blocs). Un travail fini
    /// n'est plus « en cours ».
    pub fn relever(&mut self) -> Vec<Rendu<R>> {
        let mut r: Vec<Rendu<R>> = self.sur_place.drain(..).collect();
        while let Ok(x) = self.reception.try_recv() {
            r.push(x);
        }
        for x in &r {
            if let Rendu::Fini { cle, .. } = x
                && let Some(i) = self.en_cours.iter().position(|c| c == cle)
            {
                self.en_cours.remove(i);
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn ctx(nom: &str) -> Contexte {
        Contexte::nouveau(
            crate::dossier_essai(&format!("{nom}-a")),
            crate::dossier_essai(&format!("{nom}-d")),
            false,
            true,
        )
    }

    #[test]
    fn sans_contexte_partage_le_travail_est_fait_sur_place() {
        let c = ctx("travaux-place");
        let mut t: Travaux<u32> = Travaux::default();
        assert!(t.lancer(&c, "a", |f| {
            f.effet(crate::signaux::occupe("Un instant..."));
            7
        }));
        let r = t.relever();
        assert!(matches!(&r[0], Rendu::Effet(Effet::Signal { nom, .. }) if nom == "occupe"));
        assert!(matches!(&r[1], Rendu::Fini { cle, resultat: Ok(7) } if cle == "a"));
        assert!(!t.occupe());
    }

    #[test]
    fn sur_un_fil_rien_n_attend_et_une_cle_ne_part_pas_deux_fois() {
        let c = ctx("travaux-fil").partage();
        let mut t: Travaux<u32> = Travaux::default();
        let (feu, attente) = std::sync::mpsc::channel::<()>();
        let debut = Instant::now();
        assert!(t.lancer(&c, "long", move |f| {
            f.effet(crate::signaux::occupe("En cours..."));
            let _ = attente.recv_timeout(Duration::from_secs(60));
            1
        }));
        assert!(debut.elapsed() < Duration::from_secs(1), "lancer n'attend pas le travail");
        assert!(!t.lancer(&c, "long", |_| 2), "la même clé n'est pas relancée");
        assert!(t.en_cours("long"));
        // L'effet d'attente arrive avant la fin
        let fin = Instant::now() + Duration::from_secs(10);
        let mut vus = Vec::new();
        while vus.is_empty() && Instant::now() < fin {
            vus = t.relever();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(matches!(&vus[0], Rendu::Effet(_)));
        assert!(c.travaux_a_relever());
        assert!(t.occupe());
        feu.send(()).unwrap();
        let mut fini = None;
        while fini.is_none() && Instant::now() < fin {
            for x in t.relever() {
                if let Rendu::Fini { resultat, .. } = x {
                    fini = Some(resultat);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(fini, Some(Ok(1)));
        assert!(!t.occupe());
    }

    #[test]
    fn une_panique_devient_une_erreur() {
        let c = ctx("travaux-panique").partage();
        let mut t: Travaux<u32> = Travaux::default();
        t.lancer(&c, "p", |_| panic!("boum"));
        let fin = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(Rendu::Fini { resultat, .. }) = t.relever().pop() {
                assert_eq!(resultat, Err("erreur imprévue (boum)".into()));
                break;
            }
            assert!(Instant::now() < fin);
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Ajustement C du lot A : un geste dont le travail panique rend le verrou ; un NOUVEAU geste passe ensuite.
    /// Pareil sans contexte partagé (fait sur place), et quand la clé est déjà prise (le geste est rendu tout de suite).
    #[test]
    fn un_geste_qui_panique_rend_le_verrou() {
        for partage in [true, false] {
            let c = std::sync::Arc::new(ctx(&format!("travaux-geste-{partage}")));
            let c = if partage { std::sync::Arc::into_inner(c).unwrap().partage() } else { c };
            let mut t: Travaux<u32> = Travaux::default();
            let g = c.prendre_geste("Tout restaurer", "Ton PC").unwrap();
            t.lancer_geste(&c, "p", g, |f| {
                assert!(f.ctx.exiger_geste().is_ok(), "le travail tient le geste");
                panic!("boum")
            });
            let fin = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(Rendu::Fini { resultat, .. }) = t.relever().pop() {
                    assert_eq!(resultat, Err("erreur imprévue (boum)".into()));
                    break;
                }
                assert!(Instant::now() < fin);
                std::thread::sleep(Duration::from_millis(10));
            }
            let n = c.prendre_geste("Changer le DNS", "Réseau");
            assert!(n.is_ok(), "après la panique, un nouveau geste passe : {n:?}");
            drop(n);
            // (la clé déjà prise : rien n'est lancé, le geste est rendu)
            let (feu, attente) = std::sync::mpsc::channel::<()>();
            let g1 = c.prendre_geste("Un", "Essai").unwrap();
            if !partage {
                feu.send(()).unwrap();
            }
            t.lancer_geste(&c, "k", g1, move |_| {
                let _ = attente.recv_timeout(Duration::from_secs(10));
                1
            });
            if partage {
                let g2 = c.prendre_geste("Deux", "Essai");
                assert_eq!(g2.unwrap_err(), "Attends la fin de « Un » (Essai)");
                feu.send(()).unwrap();
            }
            while t.occupe() {
                t.relever();
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(c.prendre_geste("Trois", "Essai").is_ok());
        }
    }
}
