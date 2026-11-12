//! Lot K (audit du 10/11) : le verrou « une seule Aide à la fois », sur le vrai Windows. Seulement des noms d'essai
//! (un compte « S-1-5-21-0-0-0-<processus> » qui n'existe pas) : jamais celui de la vraie Aide. Sans droits
//! administrateur (le PC de développement), l'espace de noms privé ne peut pas être créé : l'Aide garde alors le nom
//! public, comme avant. Avec les droits (la machine de GitHub), le verrou protégé est pris.
//!
//! (12/11) Chaque Aide d'essai est un processus à part (ce programme d'essai relancé sur `role_enfant`, sans fenêtre),
//! comme les vraies Aides. Avant, elles étaient des fils d'un même processus : les préfixes des espaces de noms privés
//! y sont communs à tous les fils, et refermer l'espace dans un fil gênait les autres (run 43151526633, rouge sans
//! défaut du vrai verrou, vérifié entre processus par le run 43157769634). Les essais de ce module passent l'un après
//! l'autre (`SERIE`).

use super::verrou_aide;
use crate::systeme::Verrouillage;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, ChildStdin, Command, Stdio};
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

/// La variable qui donne son rôle au processus enfant
const ROLE: &str = "NEVERMIND_ESSAI_VERROU_ROLE";
/// Le début des lignes que l'enfant adresse à l'essai (le reste vient du lanceur d'essais)
const LIGNE: &str = "ESSAI-VERROU ";

/// Une Aide d'essai, dans son propre processus : elle prend le verrou, dit ce qu'elle a eu, et le garde jusqu'à ce
/// que la valeur soit jetée (son entrée refermée)
struct Aide {
    pris: bool,
    texte: Option<String>,
    enfant: Child,
    entree: Option<ChildStdin>,
}

fn aide(n: &str) -> Aide {
    let exe = std::env::current_exe().expect("programme d'essai");
    let mut enfant = Command::new(exe)
        .args(["windows::essais_verrou::role_enfant", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env(ROLE, n)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x08000000)
        .spawn()
        .expect("processus d'essai");
    let sortie = enfant.stdout.take().expect("sortie");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for l in BufReader::new(sortie).lines().map_while(Result::ok) {
            // (la ligne suit, sur la même ligne, le « test ... » du lanceur d'essais)
            if let Some(i) = l.find(LIGNE) {
                let _ = tx.send(l[i + LIGNE.len()..].to_string());
            }
        }
    });
    let entree = enfant.stdin.take();
    let mut a = Aide { pris: false, texte: None, enfant, entree };
    let l = rx.recv_timeout(Duration::from_secs(30)).expect("le processus d'essai n'a rien dit");
    let (etat, texte) = l.split_once(' ').unwrap_or((l.as_str(), ""));
    a.pris = etat == "PRIS";
    a.texte = (!texte.is_empty()).then(|| texte.to_string());
    a
}

impl Drop for Aide {
    fn drop(&mut self) {
        drop(self.entree.take());
        for _ in 0..300 {
            if let Ok(Some(_)) = self.enfant.try_wait() {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let _ = self.enfant.kill();
        let _ = self.enfant.wait();
    }
}

/// Le processus enfant : une Aide d'essai pour le nom donné par `ROLE` (sans elle, cet essai ne fait rien)
#[test]
#[ignore = "lancé seulement par les essais de ce module, dans un processus à part"]
fn role_enfant() {
    let Ok(n) = std::env::var(ROLE) else { return };
    let dire = |x: &str| {
        let mut s = std::io::stdout();
        let _ = writeln!(s, "{LIGNE}{x}");
        let _ = s.flush();
    };
    let garde = match verrou_aide(&n) {
        Verrouillage::Pris { garde, remarque } => {
            dire(&format!("PRIS {}", remarque.unwrap_or_default()));
            Some(garde)
        }
        Verrouillage::Occupe(x) => {
            dire(&format!("OCCUPE {x}"));
            None
        }
    };
    // (gardé jusqu'à ce que l'essai referme l'entrée)
    let mut tout = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut tout);
    drop(garde);
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

/// (12/11) Le vrai comportement de Windows, entre processus séparés (run 43157769634) : l'espace de noms privé de
/// l'Aide (commun à tous ses verrous) reste trouvable après l'arrêt du processus qui l'a créé, tant qu'un autre
/// processus le tient ouvert. L'Aide x crée l'espace ; l'Aide y l'ouvre et tient son verrou ; x s'arrête : une seconde
/// Aide y est toujours refusée. Si Windows changeait (espace introuvable une fois son créateur parti), elle créerait un
/// espace neuf, vide, et passerait : cet essai échouerait.
#[test]
fn l_espace_reste_trouvable_apres_l_arret_de_son_createur() {
    let _s = serie();
    let x = aide(&nom("createur-x"));
    let y = aide(&nom("tient-y"));
    assert!(x.pris && y.pris, "deux noms, deux Aides : {:?} {:?}", x.texte, y.texte);
    drop(x);
    let y2 = aide(&nom("tient-y"));
    assert!(!y2.pris, "une seconde Aide passe une fois arrêtée l'Aide qui avait créé l'espace : {:?}", y2.texte);
    drop(y2);
    drop(y);
}

/// (12/11) Des Aides lancées toutes ensemble, chacune dans son processus, plusieurs fois : à chaque fois une seule
/// passe (jamais deux, jamais zéro)
#[test]
fn des_aides_lancees_ensemble_une_seule_passe() {
    let _s = serie();
    for tour in 0..12 {
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
        let pris = aides.iter().filter(|a| a.pris).count();
        assert_eq!(pris, 1, "tour {tour} : {pris} Aides passent : {textes:?}");
    }
}
