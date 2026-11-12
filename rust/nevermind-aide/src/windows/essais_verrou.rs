//! Lot K (audit du 10/11) : le verrou « une seule Aide à la fois », sur le vrai Windows. Seulement des noms d'essai
//! (un compte « S-1-5-21-0-0-0-<processus> » qui n'existe pas) : jamais celui de la vraie Aide. Sans droits
//! administrateur (le PC de développement), l'espace de noms privé ne peut pas être créé : l'Aide garde alors le nom
//! public, comme avant. Avec les droits (la machine de GitHub), le verrou protégé est pris.
//!
//! (12/11) Chaque Aide d'essai, et chaque « créateur » d'espace de noms, est un processus à part (ce programme
//! d'essai relancé sur `role_enfant`, sans fenêtre), comme les vraies Aides : dans un même processus, les préfixes des
//! espaces de noms privés sont communs à tous les fils, et les essais ne disaient plus rien du vrai cas. Les essais de
//! ce module passent l'un après l'autre (`SERIE`).

use super::verrou::{ANCIEN_PREFIXE, ANCIENNE_FRONTIERE, Creation, espace_prive, noms_espace, prendre};
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

/// Le nom sans `Global\`
fn court(n: &str) -> &str {
    n.strip_prefix("Global\\").unwrap_or(n)
}

/// La variable qui donne son rôle au processus enfant
const ROLE: &str = "NEVERMIND_ESSAI_VERROU_ROLE";
/// Le début des lignes que l'enfant adresse à l'essai (le reste vient du lanceur d'essais)
const LIGNE: &str = "ESSAI-VERROU ";

/// Un processus enfant d'essai : il dit ce qu'il a eu, ligne par ligne, et garde tout jusqu'à ce que la valeur soit
/// jetée (son entrée refermée)
struct Enfant {
    enfant: Child,
    entree: Option<ChildStdin>,
    lignes: mpsc::Receiver<String>,
}

fn enfant(role: &[&str]) -> Enfant {
    let exe = std::env::current_exe().expect("programme d'essai");
    let mut enfant = Command::new(exe)
        .args(["windows::essais_verrou::role_enfant", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env(ROLE, role.join("|"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x08000000)
        .spawn()
        .expect("processus d'essai");
    let sortie = enfant.stdout.take().expect("sortie");
    let (tx, lignes) = mpsc::channel();
    thread::spawn(move || {
        for l in BufReader::new(sortie).lines().map_while(Result::ok) {
            // (la première suit, sur la même ligne, le « test ... » du lanceur d'essais)
            if let Some(i) = l.find(LIGNE) {
                let _ = tx.send(l[i + LIGNE.len()..].to_string());
            }
        }
    });
    let entree = enfant.stdin.take();
    Enfant { enfant, entree, lignes }
}

impl Enfant {
    /// La prochaine ligne de l'enfant (30 s au plus)
    fn ligne(&self) -> String {
        self.lignes.recv_timeout(Duration::from_secs(30)).expect("le processus d'essai n'a rien dit")
    }
}

impl Drop for Enfant {
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

/// Une Aide d'essai, dans son propre processus : elle prend le verrou, dit ce qu'elle a eu, et le garde jusqu'à ce que
/// la valeur soit jetée
struct Aide {
    pris: bool,
    texte: Option<String>,
    _enfant: Enfant,
}

fn aide(n: &str) -> Aide {
    let e = enfant(&["aide", n]);
    let l = e.ligne();
    let (etat, texte) = l.split_once(' ').unwrap_or((l.as_str(), ""));
    let texte = (!texte.is_empty()).then(|| texte.to_string());
    Aide { pris: etat == "PRIS", texte, _enfant: e }
}

/// Un « créateur » (ou une Aide d'avant) d'essai, dans son propre processus : il crée l'espace de noms privé
/// (frontière, préfixe), ou seulement l'ouvre. Avec `verrou`, il le prend après l'attente donnée (en réessayant tant
/// qu'un autre le tient), comme une Aide un peu lente.
struct Createur {
    /// `None` : pas d'espace (sans droits administrateur) ; `Some(false)` : ouvert, il existait déjà
    cree: Option<bool>,
    enfant: Enfant,
}

fn createur(creer: bool, frontiere: &str, prefixe: &str, verrou: Option<(&str, u64)>) -> Createur {
    let (v, d) = verrou.unwrap_or(("", 0));
    let ms = d.to_string();
    let e = enfant(&["espace", if creer { "1" } else { "0" }, frontiere, prefixe, v, &ms]);
    let cree = match e.ligne().as_str() {
        "CREE" => Some(true),
        "OUVERT" => Some(false),
        _ => None,
    };
    Createur { cree, enfant: e }
}

impl Createur {
    /// Le verrou pris ou non (avec `verrou`)
    fn tient(&self) -> bool {
        self.enfant.ligne() == "TIENT"
    }
}

/// Le processus enfant : son rôle vient de `ROLE` (sans elle, cet essai ne fait rien)
#[test]
#[ignore = "lancé seulement par les essais de ce module, dans un processus à part"]
fn role_enfant() {
    let Ok(role) = std::env::var(ROLE) else { return };
    let dire = |x: &str| {
        let mut s = std::io::stdout();
        let _ = writeln!(s, "{LIGNE}{x}");
        let _ = s.flush();
    };
    let attendre = || {
        let mut tout = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut tout);
    };
    let p: Vec<&str> = role.split('|').collect();
    match p.as_slice() {
        ["aide", n] => {
            let garde = match verrou_aide(n) {
                Verrouillage::Pris { garde, remarque } => {
                    dire(&format!("PRIS {}", remarque.unwrap_or_default()));
                    Some(garde)
                }
                Verrouillage::Occupe(x) => {
                    dire(&format!("OCCUPE {x}"));
                    None
                }
            };
            attendre();
            drop(garde);
        }
        ["espace", creer, frontiere, prefixe, verrou, ms] => {
            let e = espace_prive(frontiere, prefixe, *creer == "1");
            dire(match &e {
                Ok((_, true)) => "CREE",
                Ok((_, false)) => "OUVERT",
                Err(_) => "AUCUN",
            });
            let mut tenu = None;
            if e.is_ok() && !verrou.is_empty() {
                thread::sleep(Duration::from_millis(ms.parse().unwrap_or(0)));
                for _ in 0..400 {
                    if let Creation::Tenu(t) = prendre(verrou) {
                        tenu = Some(t);
                        break;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                dire(if tenu.is_some() { "TIENT" } else { "LIBRE" });
            }
            attendre();
            drop(tenu);
            drop(e);
        }
        _ => panic!("rôle inconnu : {role}"),
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

/// (12/11) La cause, côté Windows (doc de `CreatePrivateNamespaceW`) : un espace de noms privé ouvert par un
/// processus qui y tient un verrou ne peut plus être retrouvé une fois que son créateur s'est arrêté ; un autre
/// processus en crée alors un neuf, vide, et y prend le même verrou
#[test]
fn windows_un_espace_ferme_par_son_createur_est_recree_vide() {
    let _s = serie();
    let (frontiere, prefixe, verrou) = noms_espace(&format!("{}-mecanisme", court(&nom(""))));
    let c = createur(true, &frontiere, &prefixe, None);
    match c.cree {
        // (sans droits administrateur : pas d'espace de noms privé, rien à essayer ici)
        None => return,
        Some(cree) => assert!(cree, "l'espace de noms d'essai existait déjà"),
    }
    let a = createur(false, &frontiere, &prefixe, Some((&verrou, 0)));
    assert_eq!(a.cree, Some(false), "l'espace du créateur n'est pas ouvert");
    assert!(a.tient(), "le processus qui a ouvert l'espace n'a pas pris le verrou");
    drop(c);
    let b = createur(true, &frontiere, &prefixe, Some((&verrou, 0)));
    assert_eq!(b.cree, Some(true), "l'espace n'a pas été recréé une fois fermé par son créateur");
    assert!(b.tient(), "le verrou de l'espace neuf n'est pas libre");
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
    let c = createur(true, &frontiere, &prefixe, None);
    match c.cree {
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
    let c = createur(true, &frontiere, &prefixe, Some((&verrou, 400)));
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
    assert!(c.tient(), "le créateur n'a pas pu prendre le verrou");
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
    let c = createur(true, ANCIENNE_FRONTIERE, ANCIEN_PREFIXE, Some((&verrou, 0)));
    // (sans droits, ou l'ancien espace déjà là, sur un PC où une Aide d'avant tourne : rien à essayer ici)
    if c.cree != Some(true) {
        return;
    }
    assert!(c.tient(), "l'Aide d'avant d'essai n'a pas pu prendre le verrou");
    let a = aide(&n);
    assert!(!a.pris, "la nouvelle Aide passe alors qu'une Aide d'avant tient le verrou");
    assert!(a.texte.as_deref().unwrap_or_default().contains("version précédente"), "{:?}", a.texte);
}

/// (12/11) Des Aides lancées toutes ensemble, plusieurs fois : à chaque fois une seule passe (jamais deux, jamais
/// zéro), et sans garder le verrou d'un espace qu'elle n'a pas créé
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
        let pris: Vec<_> = aides.iter().filter(|a| a.pris).collect();
        assert_eq!(pris.len(), 1, "tour {tour} : {} Aides passent : {textes:?}", pris.len());
        assert!(pris[0].texte.is_none(), "tour {tour} : {textes:?}");
    }
}
