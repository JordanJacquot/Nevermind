//! Essai réel du mode streameur sur un VRAI Streamlabs Desktop, installé sur la machine jetable de GitHub Actions
//! (workflow .github\workflows\streamlabs-reel.yml, script outils\essai-streamlabs-reel.ps1). Jamais sur un vrai PC :
//! il change pour de vrai les réglages de Streamlabs du compte (%APPDATA%\slobs-client), et refuse de tourner ailleurs.
//!
//! Le bloc entier, comme dans Nevermind, sur le vrai PC de l'essai (registre, processus, cartes, écran : SystemeReel) ;
//! seul le dossier des données de Nevermind est un dossier d'essai.
//!
//!   cargo run -p nevermind-stream --example essai-streamlabs -- <données d'essai> <commande> [...]
//!
//! Commandes : `etat` (la page), `regler <envoi en Mb/s> [plateforme] [image de base LxH]` (la mesure donnée au bloc, puis « Optimiser » ;
//! l'image de base est celle que Streamlabs donne par son API, pour calculer le plan attendu),
//! `remettre` (« Remettre comme avant »). Le résultat est écrit en JSON sur la sortie : les annonces, la page, et le plan
//! que Nevermind a calculé (résolution, images par seconde, débit, encodeur), pour que l'essai vérifie ce que Streamlabs
//! a vraiment pris. Rien de la clé de stream.

use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use nevermind_stream::BlocStream;
use nevermind_stream::calcul::{Entree, Logiciel, Plateforme, Version, calculer};
use nevermind_stream::reel::SystemeReel;
use nevermind_stream::reglages;
use nevermind_stream::systeme::Systeme;
use serde_json::{Value, json};

fn annonces(e: &Effets) -> Vec<String> {
    e.0.iter()
        .filter_map(|x| match x {
            Effet::Annonce(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

/// Le plan que le bloc calcule (les mêmes entrées : cartes, image de base, plateforme, version de Streamlabs)
fn plan(envoi_kbps: u32, plateforme: Plateforme, base: Option<(u32, u32)>) -> Value {
    let s = SystemeReel;
    let Some(i) = s.installations().into_iter().find(|i| i.logiciel == Logiciel::Streamlabs) else {
        return Value::Null;
    };
    let etat = reglages::lire(Logiciel::Streamlabs, &i.reglages).ok();
    let version = etat.as_ref().and_then(|e| e.version).or_else(|| i.version.as_deref().and_then(Version::lire));
    let p = calculer(&Entree {
        envoi_kbps,
        cartes: s.cartes(),
        base: base.unwrap_or_else(|| reglages::base(etat.as_ref(), s.ecran())),
        plateforme,
        version,
    });
    json!({
        "sortie": [p.sortie.0, p.sortie.1], "ips": p.ips, "video_kbps": p.video_kbps, "son_kbps": p.son_kbps,
        "encodeur": p.encodeur.simple(), "image_cle_s": p.image_cle_s, "plateforme": p.plateforme.id(),
        "reglages": i.reglages, "dossier": i.dossier, "version": i.version,
    })
}

fn main() {
    if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true") {
        eprintln!(
            "Refusé : cet essai change pour de vrai les réglages de Streamlabs du compte. Il ne tourne que sur la machine jetable de GitHub Actions."
        );
        std::process::exit(2);
    }
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("essai-streamlabs <données d'essai> <etat|regler|remettre> [envoi Mb/s] [plateforme]");
        std::process::exit(2);
    }
    let donnees = std::path::PathBuf::from(&a[1]);
    std::fs::create_dir_all(&donnees).expect("dossier de données d'essai");
    let ctx = Contexte::nouveau(donnees.clone(), donnees.clone(), false, true);
    let mut b = BlocStream::avec_systeme(Box::new(SystemeReel));
    let mut e = Effets::default();
    let mut r = json!({});
    match a[2].as_str() {
        "etat" => {}
        "regler" => {
            let envoi: f64 = a.get(3).and_then(|x| x.parse().ok()).unwrap_or(10.0);
            b.signal(&ctx, "debit-mesure", &json!({ "R": [12.0, 200.0, envoi, 0.0] }), &mut e);
            let p = a.get(4).and_then(|p| Plateforme::depuis_id(p)).unwrap_or(Plateforme::Twitch);
            if a.get(4).is_some() {
                b.action(&ctx, "stream:plateforme:streamlabs", &json!(p.id()), &mut e);
            }
            let base = a.get(5).and_then(|b| b.split_once('x')).and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)));
            r["plan"] = plan((envoi * 1000.0).round() as u32, p, base);
            let _ = b.pages(&ctx, "streameur");
            b.action(&ctx, "stream:optimiser:streamlabs", &Value::Null, &mut e);
        }
        "remettre" => {
            let _ = b.pages(&ctx, "streameur");
            b.action(&ctx, "stream:remettre:streamlabs", &Value::Null, &mut e);
        }
        x => {
            eprintln!("commande inconnue : {x}");
            std::process::exit(2);
        }
    }
    let p = b.pages(&ctx, "streameur");
    r["annonces"] = json!(annonces(&e));
    r["page"] = p["streameur"].clone();
    r["copie"] = std::fs::read(donnees.join("streameur").join("streamlabs").join("copie.json"))
        .ok()
        .and_then(|o| serde_json::from_slice::<Value>(&o).ok())
        .unwrap_or(Value::Null);
    println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
}
