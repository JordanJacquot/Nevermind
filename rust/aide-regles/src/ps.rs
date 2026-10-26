//! Les paramètres d'une demande, lus comme PowerShell les lisait (`Get-AideParam`, `Get-AideListe`, `Get-AideTexte`,
//! et les conversions `[string]`, `[bool]`, `[int]`), pour accepter et refuser exactement les mêmes demandes.

use crate::texte;
use serde_json::Value;

/// Un paramètre de la demande (nom sans tenir compte des majuscules, comme une propriété d'objet PowerShell).
/// `None` si absent, ou si les paramètres ne sont pas un objet.
pub fn param<'a>(p: &'a Value, nom: &str) -> Option<&'a Value> {
    match p {
        Value::Object(o) => o.iter().find(|(k, _)| texte::egal(k, nom)).map(|(_, v)| v),
        _ => None,
    }
}

/// Un nombre JSON écrit comme PowerShell l'écrit (`[string]`) : entier tel quel, décimal au plus court
fn nombre(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    let f = n.as_f64().unwrap_or(0.0);
    if f.fract() == 0.0 && f.abs() < 1e15 { format!("{f:.0}") } else { format!("{f}") }
}

/// `[string]` d'une valeur : vide pour rien, « True »/« False », les éléments d'une liste séparés par une espace
pub fn chaine(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => if *b { "True" } else { "False" }.to_string(),
        Some(Value::Number(n)) => nombre(n),
        Some(Value::Array(a)) => a.iter().map(|x| chaine(Some(x))).collect::<Vec<_>>().join(" "),
        Some(Value::Object(o)) => {
            let champs: Vec<String> = o.iter().map(|(k, x)| format!("{k}={}", chaine(Some(x)))).collect();
            format!("@{{{}}}", champs.join("; "))
        }
    }
}

/// `[bool]` d'une valeur, à la manière de PowerShell (un texte non vide est vrai, même « false »)
pub fn vrai(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => match a.len() {
            0 => false,
            1 => vrai(a.first()),
            _ => true,
        },
        Some(Value::Object(_)) => true,
    }
}

/// Arrondi « au pair le plus proche », comme .NET quand il convertit un décimal en entier
fn arrondi_pair(f: f64) -> f64 {
    let r = f.round();
    if (f - f.trunc()).abs() == 0.5 && r % 2.0 != 0.0 { r - f.signum() } else { r }
}

/// `[int]` d'une valeur. Une valeur impossible à convertir est une erreur (pas un refus), comme en PowerShell.
pub fn entier(v: Option<&Value>) -> Result<i32, String> {
    let erreur = || format!("Impossible de convertir « {} » en nombre entier.", chaine(v));
    let f = match v {
        None | Some(Value::Null) => return Ok(0),
        Some(Value::Bool(b)) => return Ok(i32::from(*b)),
        Some(Value::Number(n)) => match n.as_i64() {
            Some(i) => return i32::try_from(i).map_err(|_| erreur()),
            None => n.as_f64().ok_or_else(erreur)?,
        },
        Some(Value::String(s)) => {
            let t = texte::sans_espaces(s);
            if t.is_empty() {
                return Ok(0);
            }
            if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                return i32::from_str_radix(h, 16).map_err(|_| erreur());
            }
            if let Ok(i) = t.parse::<i64>() {
                return i32::try_from(i).map_err(|_| erreur());
            }
            if !t.bytes().all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b)) {
                return Err(erreur());
            }
            t.parse::<f64>().map_err(|_| erreur())?
        }
        Some(_) => return Err(erreur()),
    };
    let r = arrondi_pair(f);
    if r.is_finite() && r >= f64::from(i32::MIN) && r <= f64::from(i32::MAX) { Ok(r as i32) } else { Err(erreur()) }
}

/// `[int]::TryParse` / `[long]::TryParse` d'un texte (espaces autour et signe permis, chiffres ASCII seulement)
pub fn lire_entier(s: &str) -> Option<i64> {
    let t = texte::sans_espaces(s);
    let (signe, chiffres) = match t.as_bytes().first() {
        Some(b'-') => (-1i64, &t[1..]),
        Some(b'+') => (1, &t[1..]),
        _ => (1, t),
    };
    if chiffres.is_empty() || !chiffres.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut n: i64 = 0;
    for b in chiffres.bytes() {
        n = n.checked_mul(10)?.checked_add(i64::from(b - b'0') * signe)?;
    }
    Some(n)
}

/// Une liste de textes (`Get-AideListe`) : les éléments non vides, en texte. Une valeur seule compte pour une liste
/// d'un élément ; rien donne une liste vide.
pub fn liste(p: &Value, nom: &str) -> Vec<String> {
    match param(p, nom) {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => {
            a.iter().filter(|x| !x.is_null()).map(|x| chaine(Some(x))).filter(|s| !s.is_empty()).collect()
        }
        Some(x) => {
            let s = chaine(Some(x));
            if s.is_empty() { Vec::new() } else { vec![s] }
        }
    }
}

/// Un texte obligatoire (`Get-AideTexte`) : un vrai texte, non vide, de `max` caractères au plus, sans caractère de
/// contrôle. Sinon : « Demande refusée : « Nom » n'est pas valable. »
pub fn texte_requis(p: &Value, nom: &str, max: usize) -> Result<String, String> {
    match param(p, nom) {
        Some(Value::String(s)) if !s.is_empty() && texte::longueur(s) <= max && !texte::a_controle(s) => Ok(s.clone()),
        _ => Err(format!("Demande refusée : « {nom} » n'est pas valable.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn conversions() {
        assert_eq!(chaine(Some(&json!(2.0))), "2");
        assert_eq!(chaine(Some(&json!(2.5))), "2.5");
        assert_eq!(chaine(Some(&json!([1, "a"]))), "1 a");
        assert_eq!(chaine(Some(&json!(true))), "True");
        assert!(vrai(Some(&json!("false"))) && !vrai(Some(&json!(""))) && !vrai(Some(&json!([0]))));
        assert_eq!(entier(Some(&json!("12"))), Ok(12));
        assert_eq!(entier(Some(&json!(12.5))), Ok(12));
        assert_eq!(entier(Some(&json!(13.5))), Ok(14));
        assert_eq!(entier(Some(&json!(""))), Ok(0));
        assert!(entier(Some(&json!("abc"))).is_err());
        assert!(entier(Some(&json!([1]))).is_err());
        assert_eq!(lire_entier(" -12 "), Some(-12));
        assert_eq!(lire_entier("1234; calc"), None);
        assert_eq!(lire_entier("99999999999999999999"), None);
        assert_eq!(liste(&json!({"x": [null, "", "a", 4]}), "X"), vec!["a", "4"]);
        assert_eq!(liste(&json!({"x": "seul"}), "x"), vec!["seul"]);
        assert!(texte_requis(&json!({"Nom": 5}), "Nom", 10).is_err());
    }
}
