//! Essai réel du mode streameur sur un OBS Studio PORTABLE (jamais celui du PC) : le bloc entier, comme dans Nevermind,
//! en construction d'essai (le contexte est en mode essai : aucun registre, aucun vrai réglage ; le logiciel vient du
//! fichier streameur-essai.json du dossier de données d'essai).
//!
//!   cargo run --offline -p nevermind-stream --example essai-obs -- <données d'essai> <OBS portable> <commande> [...]
//!
//! Commandes : `etat` (la page), `regler <envoi en Mb/s> [plateforme]` (la mesure donnée au bloc, puis « Optimiser »),
//! `remettre` (« Remettre comme avant »). Le résultat est écrit en JSON sur la sortie (rien de la clé de stream).

use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use nevermind_stream::BlocStream;
use serde_json::{Value, json};

fn annonces(e: &Effets) -> Vec<String> {
    e.0.iter()
        .filter_map(|x| match x {
            Effet::Annonce(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 4 {
        eprintln!("essai-obs <données d'essai> <OBS portable> <etat|regler|remettre> [envoi Mb/s] [plateforme]");
        std::process::exit(2);
    }
    let donnees = std::path::PathBuf::from(&a[1]);
    std::fs::create_dir_all(&donnees).expect("dossier de données d'essai");
    let essai = json!({ "obs": a[2] });
    std::fs::write(donnees.join("streameur-essai.json"), essai.to_string()).expect("streameur-essai.json");
    let ctx = Contexte::nouveau(donnees.clone(), donnees.clone(), false, true);
    let mut b = BlocStream::default();
    let mut e = Effets::default();
    let mut r = json!({});
    match a[3].as_str() {
        "etat" => {}
        "regler" => {
            let envoi: f64 = a.get(4).and_then(|x| x.parse().ok()).unwrap_or(10.0);
            b.signal(&ctx, "debit-mesure", &json!({ "R": [12.0, 200.0, envoi, 0.0] }), &mut e);
            if let Some(p) = a.get(5) {
                b.action(&ctx, "stream:plateforme:obs", &json!(p), &mut e);
            }
            let _ = b.pages(&ctx, "streameur");
            b.action(&ctx, "stream:optimiser:obs", &Value::Null, &mut e);
        }
        "remettre" => {
            let _ = b.pages(&ctx, "streameur");
            b.action(&ctx, "stream:remettre:obs", &Value::Null, &mut e);
        }
        x => {
            eprintln!("commande inconnue : {x}");
            std::process::exit(2);
        }
    }
    let p = b.pages(&ctx, "streameur");
    r["annonces"] = json!(annonces(&e));
    r["page"] = p["streameur"].clone();
    println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
}
