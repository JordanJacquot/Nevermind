//! Petit de l'audit du 08/11 : jeux.json (le temps de jeu) et organizer.json (les réglages de l'Organizer) pris par
//! un autre programme au moment de la lecture (un antivirus, une sauvegarde dans le cloud) étaient lus vides, puis
//! réécrits avec seulement ce que Nevermind venait d'ajouter : tout le reste était perdu. Comme la version PowerShell
//! (Read-JsonSur et Save-JsonSur, donnees.ps1:57 et 85) : un fichier qui n'a pas pu être lu n'est pas réécrit.

#![cfg(windows)]

use nevermind_jeux::organizer::Config;
use nevermind_jeux::parties::JournalParties;
use nevermind_jeux::texte::Moment;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

fn dossier(nom: &str) -> PathBuf {
    let d = nevermind_noyau::dossier_essai(&format!("jeux-pris-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Le fichier ouvert par « un autre programme », sans partage : personne d'autre ne peut le lire tant qu'il est tenu
fn prendre(p: &Path) -> std::fs::File {
    std::fs::OpenOptions::new().read(true).share_mode(0).open(p).unwrap()
}

#[test]
fn jeux_json_pris_pendant_la_lecture_n_est_pas_ecrase() {
    let d = dossier("jeux");
    let f = d.join("jeux.json");
    let avant = r#"{"Hades":{"Last":"2026-11-01T20:00:00","Seconds":36000,"Count":42}}"#;
    std::fs::write(&f, avant).unwrap();
    let tenu = prendre(&f);
    let mut j = JournalParties::lire(None, &d);
    drop(tenu);
    // (la partie finie juste après : avant, jeux.json était réécrit avec ce seul jeu, les 10 h de Hades perdues)
    j.ajouter(None, &d, "Celeste", Moment::depuis(2026, 11, 8, 20, 0, 0), Moment::depuis(2026, 11, 8, 21, 0, 0));
    assert_eq!(std::fs::read_to_string(&f).unwrap(), avant, "jeux.json pris pendant la lecture a été écrasé");
    // Lu normalement la fois suivante : la partie s'ajoute
    let mut j = JournalParties::lire(None, &d);
    j.ajouter(None, &d, "Celeste", Moment::depuis(2026, 11, 8, 20, 0, 0), Moment::depuis(2026, 11, 8, 21, 0, 0));
    let j = JournalParties::lire(None, &d);
    assert_eq!(j.get("Hades").map(|t| t.parties), Some(42));
    assert_eq!(j.get("Celeste").map(|t| t.parties), Some(1));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn organizer_json_pris_pendant_la_lecture_n_est_pas_ecrase() {
    let d = dossier("organizer");
    let f = d.join("organizer.json");
    let avant = r#"{"On":true,"Order":["Iop","Cra"],"Bar":true,"BarAlways":false,"BarX":10,"BarY":20,"Classes":{},"Hotkeys":{"Iop":"F1"}}"#;
    std::fs::write(&f, avant).unwrap();
    let tenu = prendre(&f);
    let mut c = Config::lire(None, &d);
    drop(tenu);
    c.barre = false;
    c.ecrire(None, &d);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), avant, "organizer.json pris pendant la lecture a été écrasé");
    let _ = std::fs::remove_dir_all(&d);
}
