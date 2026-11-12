//! Lot K (audit du 10/11) : le verrou « une seule Aide à la fois », sur le vrai Windows. Seulement des noms d'essai
//! (un compte « S-1-5-21-0-0-0-<processus> » qui n'existe pas) : jamais celui de la vraie Aide. Sans droits
//! administrateur (le PC de développement), l'espace de noms privé ne peut pas être créé : l'Aide garde alors le nom
//! public, comme avant. Avec les droits (la machine de GitHub), le verrou protégé est pris.
//!
//! Les essais de ce module passent l'un après l'autre (`SERIE`) : ils partagent les espaces de noms de ce processus.

use super::verrou_aide;
use crate::systeme::Verrouillage;
use std::sync::Mutex;
use std::sync::mpsc;
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
    // Le « créateur » : l'espace de noms privé ouvert le premier, sans prendre le verrou, dans un fil à lui
    let (tx, rx) = mpsc::channel();
    let (arret, attente) = mpsc::channel::<()>();
    let createur = thread::spawn(move || {
        let e = super::verrou::espace_prive();
        let _ = tx.send(e.is_ok());
        let _ = attente.recv();
        drop(e);
    });
    if !rx.recv().expect("le créateur n'a rien dit") {
        // (sans droits administrateur : pas d'espace de noms privé, rien à essayer ici)
        drop(arret);
        let _ = createur.join();
        return;
    }
    // L'Aide qui a ouvert l'espace du créateur. Il s'arrête peu après (il a vu le verrou pris) : l'espace est fermé.
    let fermer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        drop(arret);
        let _ = createur.join();
    });
    let gagnante = aide(&n);
    let _ = fermer.join();
    assert!(gagnante.pris, "l'Aide restée seule est refusée : {:?}", gagnante.texte);
    let suivante = aide(&n);
    assert!(
        !suivante.pris,
        "une seconde Aide passe le verrou une fois l'espace fermé par son créateur : {:?}",
        suivante.texte
    );
}
