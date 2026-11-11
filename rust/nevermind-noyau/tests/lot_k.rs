//! Lot K (petits restants de l'audit du 10/11) : la file d'attente des gestes vidée à la fermeture, et les fichiers
//! temporaires de la sauvegarde laissés par un arrêt brutal effacés au démarrage.
//!
//! Rien ne touche le vrai PC : contexte d'essai, dossier d'essai.

use nevermind_noyau::{Contexte, dossier_essai};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn dossier(nom: &str) -> PathBuf {
    let d = dossier_essai(&format!("lot-k-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("a")).unwrap();
    std::fs::create_dir_all(d.join("d")).unwrap();
    d
}

/// Lot A (audit3, défaut 2) : un profil de jeu attend son tour pendant un geste de la page RAM, puis Nevermind se
/// ferme. Avant : `rendre_les_gestes` libérait le verrou et le profil, réveillé, le prenait aussitôt et changeait le PC
/// pendant la fermeture. Maintenant il échoue, et un profil qui arrive après la fermeture échoue aussi.
#[test]
fn un_profil_en_file_ne_part_pas_pendant_la_fermeture() {
    let d = dossier("file-fermeture");
    let c = Contexte::nouveau(d.join("a"), d.join("d"), false, true).partage();
    let ram = c.prendre_geste("Fermer", "RAM").unwrap();
    let c2 = c.clone();
    let profil = std::thread::spawn(move || {
        c2.attendre_geste("Profil de jeu", "Jeux", Duration::from_secs(10)).map(|g| g.nom().to_string())
    });
    // (le profil est bien dans la file : un clic est refusé à son nom... ou à celui du geste RAM)
    std::thread::sleep(Duration::from_millis(300));
    assert!(c.refus_geste().is_some());
    c.rendre_les_gestes();
    let r = profil.join().unwrap();
    assert!(r.is_err(), "le profil en file a pris le verrou pendant la fermeture : {r:?}");
    assert!(r.unwrap_err().contains("Nevermind se ferme"), "le refus ne dit pas pourquoi");
    drop(ram);
    // un profil qui arrive après la fermeture ne part pas non plus
    let r = c.attendre_geste("Profil de jeu", "Jeux", Duration::from_secs(2));
    assert!(r.is_err(), "un profil arrivé après la fermeture a pris le verrou");
    // la remise en état de fin de partie (prioritaire) passe toujours
    let fin = c.geste_prioritaire("Fin de partie", "Mode jeu");
    assert!(fin.tient());
    drop(fin);
    let _ = std::fs::remove_dir_all(&d);
}

fn vieillir(f: &Path) {
    let h = std::fs::OpenOptions::new().write(true).open(f).unwrap();
    h.set_modified(SystemTime::now() - Duration::from_secs(3600)).unwrap();
}

/// Lot A (audit3, défaut 6) : un `sauvegarde.json.<pid>-<n>.tmp` laissé par un arrêt brutal (Nevermind arrêté net
/// entre l'écriture du fichier temporaire et le remplacement) n'était jamais effacé. Maintenant il l'est au démarrage
/// (la première lecture de la sauvegarde) et par « Tout restaurer ». Seulement les siens : un autre fichier, un
/// dossier ou un lien qui porte ce nom n'est jamais touché, et rien n'est suivi.
#[test]
fn les_fichiers_temporaires_laisses_sont_effaces_au_demarrage() {
    let d = dossier("tmp-restes");
    let donnees = d.join("d");
    let restes = ["sauvegarde.json.4242-0.tmp", "sauvegarde.json.17-12.tmp", "Sauvegarde.JSON.99-3.TMP"];
    for n in restes {
        std::fs::write(donnees.join(n), b"{}").unwrap();
        vieillir(&donnees.join(n));
    }
    // ce qui n'est pas à Nevermind, ou pas un fichier temporaire de sauvegarde : jamais touché
    let gardes =
        ["autre.json.4242-0.tmp", "sauvegarde.json.abc.tmp", "sauvegarde.json.4242-0.tmp.txt", "sauvegarde.json"];
    for n in gardes {
        std::fs::write(donnees.join(n), br#"{"Registry":{}}"#).unwrap();
        vieillir(&donnees.join(n));
    }
    std::fs::create_dir_all(donnees.join("sauvegarde.json.5-5.tmp")).unwrap();
    // le fichier d'une écriture en cours (tout récent) : jamais effacé, il n'est pas un reste
    let en_cours = format!("sauvegarde.json.{}-999.tmp", std::process::id());
    std::fs::write(donnees.join(&en_cours), b"{}").unwrap();
    // un lien qui porte ce nom (s'il peut être créé sans droits) : le lien et ce qu'il vise restent
    let cible = d.join("cible.txt");
    std::fs::write(&cible, b"a garder").unwrap();
    let lien = donnees.join("sauvegarde.json.6-6.tmp");
    let lien_cree = std::os::windows::fs::symlink_file(&cible, &lien).is_ok();

    let ctx = Contexte::nouveau(d.join("a"), donnees.clone(), false, true);
    let _ = ctx.sauvegarde();
    for n in restes {
        assert!(!donnees.join(n).exists(), "{n} est resté après le démarrage");
    }
    for n in gardes {
        assert!(donnees.join(n).exists(), "{n} a été effacé");
    }
    assert!(donnees.join("sauvegarde.json.5-5.tmp").is_dir(), "un dossier a été effacé");
    assert!(donnees.join(&en_cours).exists(), "le fichier d'une écriture en cours a été effacé");
    if lien_cree {
        assert!(std::fs::symlink_metadata(&lien).is_ok(), "le lien a été effacé");
    }
    assert_eq!(std::fs::read(&cible).unwrap(), b"a garder");

    // « Tout restaurer » (sauvegarde effacée) : un reste laissé entre-temps part aussi
    std::fs::write(donnees.join("sauvegarde.json.4242-7.tmp"), b"{}").unwrap();
    vieillir(&donnees.join("sauvegarde.json.4242-7.tmp"));
    let g = ctx.prendre_geste("Tout restaurer", "Ton PC").unwrap();
    ctx.exiger_geste().and_then(|p| ctx.effacer_sauvegarde(&p)).unwrap();
    drop(g);
    assert!(!donnees.join("sauvegarde.json.4242-7.tmp").exists(), "« Tout restaurer » a laissé le fichier temporaire");
    let _ = std::fs::remove_dir_all(&d);
}
