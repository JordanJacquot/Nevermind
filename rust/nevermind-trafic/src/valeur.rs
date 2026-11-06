//! Lire une donnée JSON comme PowerShell lit un objet : un champ absent vaut `$null`, un nombre en texte se lit,
//! et « vrai » suit les règles de PowerShell (`if ($x)`).

use serde_json::{Map, Value, json};

/// Le champ `nom` d'un objet (`Value::Null` s'il manque)
pub fn champ<'a>(v: &'a Value, nom: &str) -> &'a Value {
    static NUL: Value = Value::Null;
    v.get(nom).unwrap_or(&NUL)
}

/// Un nombre (`[double]$x`) : `None` pour `$null`
pub fn nombre(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().replace(',', ".").parse().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// `[double]$x` : 0 pour `$null`
pub fn num(v: &Value) -> f64 {
    nombre(v).unwrap_or(0.0)
}

/// `[string]$x`
pub fn texte(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Number(n) => n.as_f64().map(crate::format::ps_nombre).unwrap_or_default(),
        Value::Array(a) => a.iter().map(texte).collect::<Vec<_>>().join(" "),
        Value::Object(_) => "System.Collections.Hashtable".into(),
    }
}

/// `if ($x)` en PowerShell
pub fn vrai(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => match a.len() {
            0 => false,
            1 => vrai(&a[0]),
            _ => true,
        },
        Value::Object(_) => true,
    }
}

/// `@($x)` : une liste (rien pour `$null`, un seul élément pour une valeur seule)
pub fn liste(v: &Value) -> Vec<Value> {
    match v {
        Value::Null => Vec::new(),
        Value::Array(a) => a.clone(),
        x => vec![x.clone()],
    }
}

/// Un objet vide
pub fn objet() -> Map<String, Value> {
    Map::new()
}

/// Un nombre JSON tel que ConvertTo-Json l'écrit (entier s'il n'a pas de virgule)
pub fn n(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 9.0e15 { json!(x as i64) } else { json!(x) }
}
