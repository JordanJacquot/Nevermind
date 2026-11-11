//! Lot K (petits restants de l'audit du 10/11) pour Ton PC : les lignes « pare-feu » quand la lecture des règles
//! échoue.
//!
//! Rien ne touche le vrai PC : contexte d'essai, fausse Aide (rien n'est changé pour de vrai), dossier d'essai, nombre
//! de règles du pare-feu donné par l'essai (le vrai pare-feu n'est jamais lu).

use nevermind_noyau::Contexte;
use nevermind_pc::corrections::Sauvegarde;
use nevermind_pc::systeme::PcWindows;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn dossier(nom: &str) -> PathBuf {
    let d = nevermind_noyau::dossier_essai(&format!("pc-lot-k-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("a")).unwrap();
    std::fs::create_dir_all(d.join("d")).unwrap();
    d
}

fn fichier_sauvegarde(ctx: &Contexte) -> Value {
    let t = std::fs::read_to_string(ctx.fichier_sauvegarde()).unwrap_or_default();
    serde_json::from_str(t.trim_start_matches('\u{feff}')).unwrap_or(Value::Null)
}

/// « Tout restaurer » quand la lecture des règles du pare-feu échoue, avec une Aide qui répond `aide` à
/// « pare-feu.debloquer-tout » : l'historique après coup, et les actions demandées à l'Aide
fn tout_restaurer_sans_lire_le_pare_feu(nom: &str, aide: Result<Value, String>) -> (Vec<Value>, Vec<String>) {
    let d = dossier(nom);
    let ctx = Contexte::nouveau(d.join("a"), d.join("d"), false, true);
    let demandes: Arc<Mutex<Vec<String>>> = Arc::default();
    let d2 = demandes.clone();
    ctx.fausse_aide(Arc::new(move |a: &str, _p: &Value| {
        d2.lock().unwrap().push(a.to_string());
        if a == "pare-feu.debloquer-tout" {
            return aide.clone();
        }
        Ok(json!("OK"))
    }));
    let _g = ctx.prendre_geste("Tout restaurer", "Ton PC").unwrap();
    let cle = r"HKLM:\SOFTWARE\MindsetEssaiLotK";
    ctx.changer_registre(cle, "X", &json!(1), "DWord").unwrap();
    ctx.ajouter_historique(
        "Réglage X",
        &[],
        vec![json!({ "Type": "reg", "Path": cle, "Name": "X", "Existed": false })],
    );
    ctx.ajouter_historique(
        "Internet bloqué pour Jeu",
        &[],
        vec![json!({ "Type": "fw", "Name": "Nevermind : bloque Jeu" })],
    );
    // (la lecture des règles du pare-feu échoue : COM refusé, service arrêté...)
    let mut p = PcWindows::nouveau(&ctx).avec_regles_pare_feu_illisibles();
    let mut s =
        nevermind_pc::session::Session { sv: Sauvegarde::depuis(&fichier_sauvegarde(&ctx)), ..Default::default() };
    s.restaurer_tout(&mut p, &[]);
    s.repondre(&mut p, true);
    let h = ctx.historique();
    let demandes = demandes.lock().unwrap().clone();
    let _ = std::fs::remove_dir_all(&d);
    (h, demandes)
}

fn ligne(h: &[Value], titre: &str) -> Value {
    h.iter().find(|x| x["Titre"] == titre).cloned().unwrap_or(Value::Null)
}

/// Lot A (audit3, défaut 3) : la lecture des règles du pare-feu échoue pendant « Tout restaurer ». Avant : elle
/// comptait 0 règle, rien n'était débloqué, aucune erreur « Pare-feu: », et la ligne « Internet bloqué pour X » était
/// marquée annulée alors que la règle était toujours là (plus de « Annuler »). Maintenant le déblocage est tenté quand
/// même : s'il échoue, la ligne reste à annuler.
#[test]
fn pare_feu_illisible_et_deblocage_refuse_la_ligne_reste_a_annuler() {
    let (h, demandes) = tout_restaurer_sans_lire_le_pare_feu("illisible-refuse", Err("refusé par Windows".into()));
    assert!(demandes.iter().any(|a| a == "pare-feu.debloquer-tout"), "le déblocage n'a pas été tenté : {demandes:?}");
    assert_eq!(ligne(&h, "Internet bloqué pour Jeu")["Annule"], false, "la règle est peut-être encore là : {h:?}");
    assert_eq!(ligne(&h, "Réglage X")["Annule"], true, "la ligne bien remise est marquée annulée : {h:?}");
}

/// La même lecture qui échoue, mais le déblocage réussit : les règles sont parties, la ligne est bien annulée
#[test]
fn pare_feu_illisible_et_deblocage_fait_la_ligne_est_annulee() {
    let (h, demandes) = tout_restaurer_sans_lire_le_pare_feu("illisible-fait", Ok(json!("OK")));
    assert!(demandes.iter().any(|a| a == "pare-feu.debloquer-tout"), "le déblocage n'a pas été tenté : {demandes:?}");
    assert_eq!(ligne(&h, "Internet bloqué pour Jeu")["Annule"], true, "{h:?}");
}
