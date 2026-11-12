//! Lot K (audit du 10/11) : le verrou « une seule Aide à la fois », sur le vrai Windows. Seulement des noms d'essai
//! (un compte « S-1-5-21-0-0-0-<processus> » qui n'existe pas) : jamais celui de la vraie Aide. Sans droits
//! administrateur (le PC de développement), l'espace de noms privé ne peut pas être créé : l'Aide garde alors le nom
//! public, comme avant. Avec les droits (la machine de GitHub), le verrou protégé est pris.
//!
//! Les essais de ce module passent l'un après l'autre (`SERIE`) : ils partagent les espaces de noms de ce processus.

use super::verrou::{ANCIEN_PREFIXE, ANCIENNE_FRONTIERE, Creation, espace_prive, noms_espace, prendre};
use super::verrou_aide;
use crate::systeme::Verrouillage;
use std::sync::mpsc;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Duration;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::HSTRING;

/// Un essai de ce module à la fois
static SERIE: Mutex<()> = Mutex::new(());

fn serie() -> std::sync::MutexGuard<'static, ()> {
    SERIE.lock().unwrap_or_else(|e| e.into_inner())
}

fn nom(essai: &str) -> String {
    format!(r"Global\Nevermind-Aide-S-1-5-21-0-0-0-{}-{essai}", std::process::id())
}

/// Une Aide d'essai, dans son propre fil (comme une Aide est seule dans son processus) : elle prend le verrou, dit ce
/// qu'elle a eu, et le garde jusqu'à ce que la valeur soit jetée
struct Aide {
    pris: bool,
    texte: Option<String>,
    arret: Option<mpsc::Sender<()>>,
    fil: Option<thread::JoinHandle<()>>,
}

fn aide(n: &str) -> Aide {
    let (tx, rx) = mpsc::channel();
    let (arret, attente) = mpsc::channel::<()>();
    let n = n.to_string();
    let fil = thread::spawn(move || {
        let (pris, texte, garde) = match verrou_aide(&n) {
            Verrouillage::Pris { garde, remarque } => (true, remarque, Some(garde)),
            Verrouillage::Occupe(p) => (false, Some(p), None),
        };
        let _ = tx.send((pris, texte));
        let _ = attente.recv();
        drop(garde);
    });
    let (pris, texte) = rx.recv().expect("l'Aide d'essai n'a rien dit");
    Aide { pris, texte, arret: Some(arret), fil: Some(fil) }
}

/// Le nom sans `Global\`
fn court(n: &str) -> &str {
    n.strip_prefix("Global\\").unwrap_or(n)
}

/// Un « créateur » d'essai, dans son fil : il crée l'espace de noms privé (frontière, préfixe). Avec `verrou`, il le
/// prend après l'attente donnée (en réessayant tant qu'un autre le tient), comme une Aide un peu lente. Il garde tout
/// jusqu'à ce que la valeur soit jetée.
struct Createur {
    /// `None` : pas d'espace (sans droits administrateur) ; `Some(false)` : ouvert, il existait déjà
    cree: Option<bool>,
    /// Le verrou pris ou non (avec `verrou`)
    tient: mpsc::Receiver<bool>,
    arret: Option<mpsc::Sender<()>>,
    fil: Option<thread::JoinHandle<()>>,
}

fn createur(frontiere: String, prefixe: String, verrou: Option<(String, Duration)>) -> Createur {
    let (tx, rx) = mpsc::channel();
    let (tx_tient, tient) = mpsc::channel();
    let (arret, attente) = mpsc::channel::<()>();
    let fil = thread::spawn(move || {
        let e = espace_prive(&frontiere, &prefixe, true);
        let cree = e.as_ref().ok().map(|(_, c)| *c);
        let _ = tx.send(cree);
        let mut tenu = None;
        if let (Some(true), Some((v, d))) = (cree, verrou) {
            thread::sleep(d);
            for _ in 0..400 {
                if let Creation::Tenu(t) = prendre(&v) {
                    tenu = Some(t);
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            let _ = tx_tient.send(tenu.is_some());
        }
        let _ = attente.recv();
        drop(tenu);
        drop(e);
    });
    let cree = rx.recv().expect("le créateur n'a rien dit");
    Createur { cree, tient, arret: Some(arret), fil: Some(fil) }
}

impl Drop for Createur {
    fn drop(&mut self) {
        drop(self.arret.take());
        if let Some(f) = self.fil.take() {
            let _ = f.join();
        }
    }
}

impl Drop for Aide {
    fn drop(&mut self) {
        drop(self.arret.take());
        if let Some(f) = self.fil.take() {
            let _ = f.join();
        }
    }
}

/// Lot E : une seconde Aide (même compte, même copie) s'arrête au verrou ; une fois la première arrêtée, la suivante
/// passe
#[test]
fn une_seconde_aide_s_arrete_au_verrou() {
    let _s = serie();
    let n = nom("seconde");
    let premiere = aide(&n);
    assert!(premiere.pris, "la première Aide est refusée : {:?}", premiere.texte);
    assert!(premiere.texte.is_none(), "rien ne tenait ce nom : {:?}", premiere.texte);
    let seconde = aide(&n);
    assert!(!seconde.pris, "la seconde Aide passe le verrou");
    drop(seconde);
    drop(premiere);
    assert!(aide(&n).pris, "après la première, la suivante est refusée");
}

/// Un programme sans droits prend le nom public avant l'Aide. Avec les droits administrateur : l'Aide continue avec
/// son verrou protégé et le note. Sans (secours d'essai) : elle s'arrête, en disant pourquoi. Jamais en silence.
#[test]
fn le_nom_public_pris_avant_l_aide() {
    let _s = serie();
    let n = nom("squat");
    // SAFETY : un verrou nommé d'essai, refermé à la fin
    let squat = unsafe { CreateMutexW(None, false, &HSTRING::from(n.as_str())) }.expect("verrou d'essai");
    let a = aide(&n);
    if a.pris {
        let r = a.texte.clone().expect("le nom pris n'est pas noté");
        assert!(r.contains("déjà pris par un autre programme"), "{r}");
        // et une seconde Aide s'arrête toujours, au verrou protégé
        assert!(!aide(&n).pris, "la seconde Aide passe le verrou protégé");
    } else {
        let p = a.texte.clone().unwrap_or_default();
        assert!(p.contains("déjà pris"), "l'Aide s'arrête sans dire pourquoi : {p}");
    }
    drop(a);
    // SAFETY : notre poignée
    unsafe {
        let _ = CloseHandle(squat);
    }
}

/// (12/11) Le cas réel du run 43151526633 : deux Aides démarrent ensemble ; la première crée l'espace de noms privé,
/// la seconde l'ouvre et prend le verrou avant elle ; la première voit le verrou pris et s'arrête, ce qui ferme
/// l'espace (Windows : une fois fermé par son créateur, il ne peut plus être ouvert). Une troisième Aide, plus tard,
/// doit toujours être refusée : avant, elle créait un espace neuf, vide, et passait.
#[test]
fn le_createur_de_l_espace_s_arrete_et_la_suivante_est_refusee() {
    let _s = serie();
    let n = nom("createur");
    // Le « créateur » : l'espace de noms privé de ce verrou créé le premier, sans prendre le verrou
    let (frontiere, prefixe, _) = noms_espace(court(&n));
    let c = createur(frontiere, prefixe, None);
    match c.cree {
        // (sans droits administrateur : pas d'espace de noms privé, rien à essayer ici)
        None => return,
        Some(cree) => assert!(cree, "l'espace de noms d'essai existait déjà"),
    }
    // L'Aide qui a ouvert l'espace du créateur. Il s'arrête peu après (il a vu le verrou pris) : l'espace est fermé.
    let fermer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        drop(c);
    });
    let gagnante = aide(&n);
    let _ = fermer.join();
    // (jamais zéro : le créateur parti, celle qui reste passe, sans attendre la fin des essais)
    assert!(gagnante.pris, "l'Aide restée seule est refusée : {:?}", gagnante.texte);
    assert!(gagnante.texte.is_none(), "elle n'a pas créé son propre espace : {:?}", gagnante.texte);
    let suivante = aide(&n);
    assert!(
        !suivante.pris,
        "une seconde Aide passe le verrou une fois l'espace fermé par son créateur : {:?}",
        suivante.texte
    );
}

/// (12/11) L'autre cas de la course : le créateur de l'espace vit et prend le verrou un peu après l'Aide qui l'a
/// seulement ouvert. Celle-ci l'a rendu entre-temps, et s'arrête : jamais deux.
#[test]
fn le_createur_vivant_garde_le_verrou() {
    let _s = serie();
    let n = nom("createur-vivant");
    let (frontiere, prefixe, verrou) = noms_espace(court(&n));
    let c = createur(frontiere, prefixe, Some((verrou, Duration::from_millis(150))));
    match c.cree {
        None => return,
        Some(cree) => assert!(cree, "l'espace de noms d'essai existait déjà"),
    }
    let a = aide(&n);
    assert!(
        !a.pris,
        "l'Aide qui a seulement ouvert l'espace garde le verrou alors que son créateur vit : {:?}",
        a.texte
    );
    assert!(a.texte.as_deref().unwrap_or_default().contains("Une autre Aide"), "{:?}", a.texte);
    assert!(c.tient.recv().unwrap_or(false), "le créateur n'a pas pu prendre le verrou");
}

/// (12/11) Chaque verrou a son espace : l'arrêt de l'Aide d'un autre compte (qui avait créé l'espace, quand il était
/// commun) ne laisse plus passer une seconde Aide de ce compte
#[test]
fn l_aide_d_un_autre_compte_s_arrete() {
    let _s = serie();
    let x = aide(&nom("compte-x"));
    let y = aide(&nom("compte-y"));
    assert!(x.pris && y.pris, "deux comptes, deux Aides : {:?} {:?}", x.texte, y.texte);
    drop(x);
    let y2 = aide(&nom("compte-y"));
    assert!(!y2.pris, "une seconde Aide du compte y passe, l'Aide du compte x arrêtée : {:?}", y2.texte);
    drop(y2);
    drop(y);
}

/// (12/11) Une Aide d'avant (Nevermind 2.3.0 à 3.0, espace de noms commun) tient le verrou de ce nom : la nouvelle
/// s'arrête
#[test]
fn une_aide_d_avant_tient_le_verrou() {
    let _s = serie();
    let n = nom("avant");
    let verrou = format!("{ANCIEN_PREFIXE}\\{}", court(&n));
    let c = createur(ANCIENNE_FRONTIERE.into(), ANCIEN_PREFIXE.into(), Some((verrou, Duration::ZERO)));
    // (sans droits, ou l'ancien espace déjà là, sur un PC où une Aide d'avant tourne : rien à essayer ici)
    if c.cree != Some(true) {
        return;
    }
    assert!(c.tient.recv().unwrap_or(false), "l'Aide d'avant d'essai n'a pas pu prendre le verrou");
    let a = aide(&n);
    assert!(!a.pris, "la nouvelle Aide passe alors qu'une Aide d'avant tient le verrou");
    assert!(a.texte.as_deref().unwrap_or_default().contains("version précédente"), "{:?}", a.texte);
}

/// (12/11) Des Aides lancées toutes ensemble, plusieurs fois : à chaque fois une seule passe (jamais deux, jamais
/// zéro), et sans garder le verrou d'un espace qu'elle n'a pas créé
#[test]
fn des_aides_lancees_ensemble_une_seule_passe() {
    let _s = serie();
    for tour in 0..15 {
        let n = nom(&format!("course-{tour}"));
        let depart = Arc::new(Barrier::new(4));
        let fils: Vec<_> = (0..4)
            .map(|_| {
                let (d, n) = (depart.clone(), n.clone());
                thread::spawn(move || {
                    d.wait();
                    aide(&n)
                })
            })
            .collect();
        let aides: Vec<Aide> = fils.into_iter().map(|f| f.join().expect("fil d'essai")).collect();
        let textes: Vec<_> = aides.iter().map(|a| (a.pris, a.texte.clone())).collect();
        let pris: Vec<_> = aides.iter().filter(|a| a.pris).collect();
        assert_eq!(pris.len(), 1, "tour {tour} : {} Aides passent : {textes:?}", pris.len());
        assert!(pris[0].texte.is_none(), "tour {tour} : {textes:?}");
    }
}
