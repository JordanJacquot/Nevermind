//! Lot C du second audit (M-A) : le contexte « sans disque » du désinstalleur en administrateur ne lit ni n'écrit
//! RIEN dans le dossier des données (le compte peut le remplacer par une jonction à tout moment) : il n'est ni créé
//! ni raccourci, le journal reste en mémoire, une sauvegarde abîmée n'est jamais renommée, un réglage changé ne l'est
//! qu'en mémoire, une écriture de fichier est refusée.

use nevermind_noyau::Contexte;
use serde_json::json;
use std::path::{Path, PathBuf};

fn dossier_essai(nom: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nevermind-noyau-{nom}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Ce que contient le dossier (noms et contenus)
fn photo(d: &Path) -> Vec<(String, Vec<u8>)> {
    let mut l: Vec<(String, Vec<u8>)> = std::fs::read_dir(d)
        .unwrap()
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap_or_default()))
        .collect();
    l.sort();
    l
}

#[test]
fn le_contexte_sans_disque_ne_touche_jamais_au_dossier_des_donnees() {
    let base = dossier_essai("sans-disque");
    let donnees = base.join("donnees");
    std::fs::create_dir_all(&donnees).unwrap();
    // Un journal de plus de 1 Mo et de 3000 lignes (raccourci par `Contexte::nouveau`), une sauvegarde abîmée (mise
    // de côté par `importer_sauvegarde`), des réglages
    let ligne = format!("{}\r\n", "x".repeat(300));
    std::fs::write(donnees.join("journal.txt"), ligne.repeat(4000)).unwrap();
    std::fs::write(donnees.join("sauvegarde.json"), "{ abîmé").unwrap();
    std::fs::write(donnees.join("parametres.json"), r#"{ "Theme": "neon" }"#).unwrap();
    let avant = photo(&donnees);

    let ctx = Contexte::sans_disque(base.join("appli"), donnees.clone(), true);
    assert!(ctx.est_sans_disque());
    // Ce qu'on lui donne, lu ailleurs : rien n'est lu ici
    ctx.fournir_reglages(serde_json::from_str(r#"{ "SvcOriginal": ["DiagTrack|Automatic|True"] }"#).unwrap());
    ctx.fournir_historique(Some(json!([{ "Id": "a", "Titre": "Essai", "Log": [] }])));
    ctx.importer_sauvegarde_lue(Ok(Some(b"{ abime".to_vec())), None);
    assert_eq!(ctx.sauvegarde().probleme.as_deref(), Some("abimee"));
    assert_eq!(ctx.reglage("SvcOriginal"), Some(json!(["DiagTrack|Automatic|True"])));
    assert_eq!(ctx.reglage("Theme"), None, "parametres.json a été lu");
    assert_eq!(ctx.historique().len(), 1);
    // Ce qu'il fait (sous le verrou des gestes, comme le désinstalleur) : rien sur le disque
    let _geste = ctx.prendre_geste("Désinstaller", "Désinstallation").unwrap();
    ctx.exiger_geste().and_then(|p| ctx.ecrire_reglage_d_origine(&p, "SvcOriginal", json!([]), true)).unwrap();
    assert!(ctx.exiger_geste().and_then(|p| ctx.enregistrer_sauvegarde(&p)).is_err());
    ctx.journal("Désinstallation : une ligne");
    ctx.noter_erreur("Désinstallation : une erreur");
    ctx.ecrire_reglage("Theme", json!("autre"), true).unwrap();
    assert_eq!(ctx.reglage("Theme"), Some(json!("autre")));
    assert!(ctx.ecrire_json(&donnees.join("ignores.json"), &json!([]), false, false).is_err());
    assert_eq!(ctx.lire_json(&donnees.join("parametres.json")), None);
    ctx.importer_sauvegarde();
    let _ = ctx.exiger_geste().and_then(|p| ctx.effacer_sauvegarde(&p));
    assert!(photo(&donnees) == avant, "le dossier des données a changé");
    // Le journal est en mémoire, dans l'ordre, et rendu une seule fois
    let l = ctx.journal_en_attente();
    assert_eq!(l.len(), 3, "{l:?}");
    // (la sauvegarde abîmée est dite, sans « mise de côté » : elle ne l'est pas)
    assert!(l[0].contains(" Sauvegarde abîmée (") && !l[0].contains("mise de côté"), "{l:?}");
    assert!(l[1].ends_with(" Désinstallation : une ligne") && l[2].ends_with(" Désinstallation : une erreur"));
    assert!(ctx.journal_en_attente().is_empty());

    // Un dossier qui n'existe pas n'est pas créé
    let absent = base.join("absent");
    let ctx = Contexte::sans_disque(base.join("appli"), absent.clone(), true);
    ctx.journal("rien");
    ctx.importer_sauvegarde();
    assert!(!absent.exists(), "le dossier des données a été créé");
    let _ = std::fs::remove_dir_all(&base);
}

/// La contre-épreuve : le contexte habituel, lui, crée le dossier, raccourcit le journal et met la sauvegarde abîmée de
/// côté (c'est ce que faisait le désinstalleur en administrateur avant le lot C)
#[test]
fn le_contexte_habituel_ecrit_dans_le_dossier_des_donnees() {
    let base = dossier_essai("avec-disque");
    let donnees = base.join("donnees");
    std::fs::create_dir_all(&donnees).unwrap();
    let ligne = format!("{}\r\n", "x".repeat(300));
    std::fs::write(donnees.join("journal.txt"), ligne.repeat(4000)).unwrap();
    std::fs::write(donnees.join("sauvegarde.json"), "{ abîmé").unwrap();
    let ctx = Contexte::nouveau(base.join("appli"), donnees.clone(), true, true);
    assert!(donnees.join("journal-ancien.txt").exists());
    ctx.importer_sauvegarde();
    assert!(!donnees.join("sauvegarde.json").exists());
    let _ = std::fs::remove_dir_all(&base);
}
