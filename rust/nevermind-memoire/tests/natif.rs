//! Les vrais gestes de Windows, seulement sur un programme d'essai lancé par ce test (caché, sans fenêtre : cmd.exe
//! qui attend sur son entrée) : poignée gardée, vérification (heure de démarrage, chemin), fiche, fermeture forcée.
//! Jamais un programme de l'utilisateur.
#![cfg(windows)]

use nevermind_memoire::constat::Cible;
use nevermind_memoire::pc::Pc;
use nevermind_memoire::windows::{self, PcWindows};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

/// Les poignées gardées sont communes au processus : un essai à la fois
static UN_A_LA_FOIS: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn programme_d_essai() {
    let _seul = UN_A_LA_FOIS.lock().unwrap_or_else(|e| e.into_inner());
    let systeme = windows::dossiers().windows;
    let cmd = format!(r"{systeme}\System32\cmd.exe");
    let mut enfant = Command::new(&cmd)
        .arg("/q")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .expect("programme d'essai");
    let pid = enfant.id();
    let pc = PcWindows;
    let debut = windows::garder(pid);
    assert_ne!(debut, 0, "poignée pas gardée");
    let chemin = pc.fiche(i64::from(pid), "").chemin;
    assert!(chemin.to_lowercase().ends_with(r"\system32\cmd.exe"), "{chemin}");
    let f = pc.fiche(i64::from(pid), "");
    assert_eq!(f.protection, Some(0));
    assert!(!f.compte.is_empty() && f.session.is_some_and(|s| s >= 0));
    let p = i64::from(pid);
    // (le permis d'un contexte d'essai dont ce fil tient le verrou des gestes ; lot J, suite)
    let dc = nevermind_noyau::dossier_essai("memoire-natif-permis");
    let ctx = nevermind_noyau::Contexte::nouveau(dc.join("a"), dc.join("d"), false, true);
    let _g = ctx.prendre_geste("Fermer", "Essai").unwrap();
    let permis = ctx.exiger_geste().unwrap();
    assert_eq!(pc.verifier(p, debut, &chemin), 1);
    assert_eq!(pc.verifier(p, debut - 1, &chemin), -1, "numéro repris pas vu");
    assert_eq!(pc.verifier(p, debut, r"C:\autre.exe"), -1, "autre chemin pas vu");
    assert_eq!(pc.forcer(&permis, p, debut - 1, &chemin), -1, "fiche périmée forcée");
    assert_eq!(pc.vivant(p), 1);
    // Sans fenêtre : rien à fermer poliment, et l'état est « rangée »
    assert_eq!(pc.fermer_poliment(&permis, p, debut, &chemin), 0);
    assert_eq!(pc.etat_fermeture(&[Cible { pid: p, debut, chemin: chemin.clone(), octets: 0.0 }]), "rangee");
    // La bonne fiche : fermée de force
    assert_eq!(pc.forcer(&permis, p, debut, &chemin), 1);
    let _ = enfant.wait();
    assert_eq!(pc.vivant(p), 0);
    assert_eq!(pc.verifier(p, debut, &chemin), 0, "déjà fermé pas vu");
    assert_eq!(pc.forcer(&permis, p, debut, &chemin), -2);
    windows::relacher();
    assert_eq!(pc.vivant(p), -1, "poignée pas rendue");
}

#[test]
fn lecture_reelle_coherente() {
    let _seul = UN_A_LA_FOIS.lock().unwrap_or_else(|e| e.into_inner());
    let l = windows::lire(&[], &Default::default());
    let d = l.donnees.expect("lecture");
    windows::relacher();
    assert!(d.visible > 0.0 && d.disponible > 0.0 && d.disponible <= d.visible);
    assert!(d.processus.iter().any(|p| p.pid == 4 && p.nom == "System"));
    assert!(d.processus.iter().any(|p| p.pid == i64::from(std::process::id())), "ce test absent de la liste");
    assert!(!d.services.is_empty());
    let moi = d.moi.as_ref().unwrap();
    assert!(!moi.compte.is_empty());
    // Ce test lui-même : « fait partie de la suite » non, mais c'est Nevermind (référence) : jamais fermable
    let c = nevermind_memoire::constat::constat(&d);
    assert!(c.applis.iter().all(|a| !a.cibles.iter().any(|x| x.pid == moi.id)));
    assert!(c.pourcent > 0 && c.pourcent <= 100);
}
