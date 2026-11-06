//! Outils communs des tests : lire un juge, comparer deux valeurs JSON comme ConvertTo-Json les écrit (un nombre
//! entier et le même à virgule sont égaux ; l'ordre des champs d'un objet ne compte pas).

#![allow(dead_code)]

use serde_json::Value;
use std::path::PathBuf;

pub fn juge(nom: &str) -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("juges").join(nom);
    let t = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{} illisible : {e}", p.display()));
    serde_json::from_str(t.trim_start_matches('\u{feff}')).expect("juge abîmé")
}

/// La première différence entre deux valeurs (`None` si égales), avec son chemin
pub fn difference(a: &Value, b: &Value, chemin: &str) -> Option<String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap_or(f64::NAN), y.as_f64().unwrap_or(f64::NAN));
            if x == y || (x - y).abs() < 1e-9 * x.abs().max(1.0) {
                None
            } else {
                Some(format!("{chemin} : {x} au lieu de {y}"))
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in y {
                let w = x.get(k).unwrap_or(&Value::Null);
                if let Some(d) = difference(w, v, &format!("{chemin}.{k}")) {
                    return Some(d);
                }
            }
            for k in x.keys() {
                if !y.contains_key(k) && !x[k].is_null() {
                    return Some(format!("{chemin}.{k} : en trop ({})", x[k]));
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!(
                    "{chemin} : {} éléments au lieu de {} ({} / {})",
                    x.len(),
                    y.len(),
                    short(a),
                    short(b)
                ));
            }
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                if let Some(d) = difference(v, w, &format!("{chemin}[{i}]")) {
                    return Some(d);
                }
            }
            None
        }
        _ => {
            if a == b {
                None
            } else {
                Some(format!("{chemin} : {} au lieu de {}", short(a), short(b)))
            }
        }
    }
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 300 { format!("{}...", s.chars().take(300).collect::<String>()) } else { s }
}

/// Vérifie une série de cas ; panique avec les premières différences
pub fn verifier(nom: &str, diffs: Vec<String>, total: usize) {
    if !diffs.is_empty() {
        let n = diffs.len();
        panic!(
            "{nom} : {n} cas sur {total} différents :\n{}",
            diffs.into_iter().take(12).collect::<Vec<_>>().join("\n")
        );
    }
    eprintln!("{nom} : {total} cas identiques");
}
