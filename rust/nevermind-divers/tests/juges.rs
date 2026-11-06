//! Les juges exportés de la version PowerShell (outils\exporter-juges-jeux.ps1) pour le bloc divers : texte de
//! recherche, fautes de frappe, Find-Settings sur des centaines de mots, pages Rechercher, Paramètres et Aide.

use nevermind_divers::BlocDivers;
use nevermind_divers::recherche::{self, JeuConnu};
use nevermind_noyau::{Bloc, Contexte, Effets};
use serde_json::{Value, json};
use std::path::Path;

fn juge(nom: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("juges").join(format!("{nom}.json"));
    let t = std::fs::read_to_string(&p).unwrap();
    let mut v: Value = serde_json::from_str(t.trim_start_matches('\u{feff}')).unwrap();
    vides_en_null(&mut v);
    v
}

fn vides_en_null(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (_, x) in m.iter_mut() {
                if matches!(x, Value::Object(o) if o.is_empty()) {
                    *x = Value::Null;
                } else {
                    vides_en_null(x);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(vides_en_null),
        _ => {}
    }
}

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        x => x.to_string(),
    }
}

#[test]
fn texte_et_fautes() {
    let j = &juge("texte")[0];
    let mut faux = Vec::new();
    for c in j["recherche"].as_array().unwrap() {
        let r = recherche::texte_recherche(&s(&c["e"]));
        if r != s(&c["s"]) {
            faux.push(format!("{:?} : {r:?} au lieu de {:?}", c["e"], c["s"]));
        }
    }
    for c in j["fautes"].as_array().unwrap() {
        let a: Vec<char> = s(&c["a"]).chars().collect();
        let b: Vec<char> = s(&c["b"]).chars().collect();
        if recherche::une_faute(&a, &b) != c["s"].as_bool().unwrap() {
            faux.push(format!("faute {c}"));
        }
    }
    assert!(faux.is_empty(), "{}", faux.join("\n"));
}

#[test]
fn find_settings() {
    let mut faux = Vec::new();
    let mut n = 0;
    for c in juge("recherche").as_array().unwrap() {
        let jeux: Vec<JeuConnu> = c["jeux"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|g| JeuConnu { nom: s(&g["Name"]), source: s(&g["Source"]) })
            .collect();
        let reglages: Vec<String> = c["reglages"].as_array().cloned().unwrap_or_default().iter().map(s).collect();
        let idx = recherche::index(c["geree"].as_bool().unwrap(), c["organizer"].as_bool().unwrap(), &reglages, &jeux);
        for q in c["requetes"].as_array().unwrap() {
            n += 1;
            let r: Vec<Value> = recherche::chercher(&idx, &s(&q["q"]), 8)
                .iter()
                .map(|x| json!({ "t": x.t, "ou": x.ou, "p": x.p, "tab": x.tab, "org": x.org, "jeu": x.jeu }))
                .collect();
            let att: Vec<Value> = q["r"].as_array().cloned().unwrap_or_default();
            if r != att {
                faux.push(format!(
                    "[geree {} org {}] « {} » :\n  attendu {:?}\n  obtenu  {:?}",
                    c["geree"],
                    c["organizer"],
                    s(&q["q"]),
                    att.iter().map(|x| s(&x["t"])).collect::<Vec<_>>(),
                    r.iter().map(|x| s(&x["t"])).collect::<Vec<_>>()
                ));
            }
        }
    }
    println!("Find-Settings : {n} recherches, {} écarts", faux.len());
    assert!(faux.is_empty(), "{} écarts :\n{}", faux.len(), faux.join("\n"));
}

fn ecart(chemin: &str, a: &Value, o: &Value, sortie: &mut Vec<String>) {
    match (a, o) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                ecart(
                    &format!("{chemin}.{k}"),
                    x.get(k).unwrap_or(&Value::Null),
                    y.get(k).unwrap_or(&Value::Null),
                    sortie,
                );
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                sortie.push(format!("{chemin} : {} éléments au lieu de {}", y.len(), x.len()));
            }
            for k in 0..x.len().min(y.len()) {
                ecart(&format!("{chemin}[{k}]"), &x[k], &y[k], sortie);
            }
        }
        _ => {
            if a != o {
                sortie.push(format!("{chemin} : attendu {a}, obtenu {o}"));
            }
        }
    }
}

fn pages_juges(cas: &Value, faux: &mut Vec<String>) {
    for (i, c) in cas.as_array().unwrap().iter().enumerate() {
        let d = std::env::temp_dir().join(format!("nevermind-essai-divers-{}-{i}", std::process::id()));
        let donnees = d.join("donnees");
        std::fs::create_dir_all(&donnees).unwrap();
        std::fs::write(donnees.join("parametres.json"), s(&c["parametres"])).unwrap();
        if c["organizer"].as_bool().unwrap() {
            std::fs::write(donnees.join("organizer.json"), r#"{"On":true}"#).unwrap();
        }
        let ctx = Contexte::nouveau(d.join("appli"), donnees, c["geree"].as_bool().unwrap(), true);
        let mut b = BlocDivers::default();
        let mut ef = Effets::default();
        b.signal(&ctx, "etat-hote", &c["etat"], &mut ef);
        b.action(&ctx, "recherche", &c["q"], &mut ef);
        let p = b.pages(&ctx, "parametres");
        // Divergence voulue (DIVERGENCES.md, n° 1) : géré par Mindset, la version PowerShell envoie les blocs de
        // l'onglet Thème en un seul objet au lieu d'une liste (PowerShell déplie la liste d'un élément) ; Rust envoie
        // la liste.
        let mut attendu = c["pages"].clone();
        if let Some(bl) = attendu.pointer_mut("/parametres/onglets/3/blocs")
            && bl.is_object()
        {
            *bl = Value::Array(vec![bl.clone()]);
        }
        for k in ["recherche", "parametres", "aide"] {
            ecart(&format!("[cas {i}].{k}"), &attendu[k], p.get(k).unwrap_or(&Value::Null), faux);
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn pages_rechercher_parametres_aide() {
    let mut faux = Vec::new();
    pages_juges(&juge("pages"), &mut faux);
    assert!(faux.is_empty(), "{} écarts :\n{}", faux.len(), faux.join("\n"));
}

#[test]
fn contre_epreuve() {
    let mut cas = juge("pages");
    cas[0]["pages"]["aide"]["titre"] = json!("Aidd");
    let mut faux = Vec::new();
    pages_juges(&Value::Array(vec![cas[0].clone()]), &mut faux);
    assert!(!faux.is_empty());
}
