//! Le JSON écrit comme `ConvertTo-Json` de Windows PowerShell 5.1 : les fichiers de données de Nevermind
//! (parametres.json, historique.json, sauvegarde.json...) gardent exactement leur forme, et la version PowerShell
//! les relit sans surprise (même retrait, mêmes échappements).
//!
//! - Échappements de JavaScriptSerializer : `'`, `<`, `>`, `&` en `'`, `<`, `>`, `&` ; les
//!   caractères de contrôle en `\b \t \n \f \r` ou `\u00xx` ; U+0085, U+2028 et U+2029 en `\uxxxx`. Les accents
//!   restent tels quels.
//! - Forme lisible (sans `-Compress`) : chaque membre d'un objet est décalé de 4 espaces, suivi de `":  "` (deux
//!   espaces) ; un objet ou une liste en valeur est aligné sur la colonne où il commence ; une liste ou un objet vide
//!   s'écrit sur trois lignes (la ligne du milieu vide). Fins de ligne `\r\n`.
//! - Nombres : entiers tels quels ; à virgule comme `Double.ToString("R")` de .NET Framework (« 1.2E-05 »,
//!   « 1E+15 »).
//!
//! Divergence (DIVERGENCES.md) : l'ordre des clés est celui de l'objet donné (ordre du fichier lu, puis des ajouts) ;
//! une table PowerShell (`@{}`) les écrivait dans l'ordre de ses cases de hachage.

use serde_json::Value;

/// Le texte d'un texte JSON, échappé comme JavaScriptSerializer (avec ses guillemets).
pub fn texte(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\u{8}' => o.push_str("\\b"),
            '\t' => o.push_str("\\t"),
            '\n' => o.push_str("\\n"),
            '\u{c}' => o.push_str("\\f"),
            '\r' => o.push_str("\\r"),
            '\'' | '<' | '>' | '&' | '\u{85}' | '\u{2028}' | '\u{2029}' => o.push_str(&format!("\\u{:04x}", c as u32)),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Un nombre à virgule comme `Double.ToString("R")` de .NET Framework.
pub fn nombre_virgule(f: f64) -> String {
    if !f.is_finite() {
        // (ConvertTo-Json écrit « Infinity » ou « NaN » sans guillemets : du JSON que personne ne relit)
        return if f.is_nan() {
            "NaN".into()
        } else if f > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if f == 0.0 {
        return "0".into();
    }
    let sci = format!("{f:e}");
    let (mantisse, exposant) = sci.split_once('e').unwrap_or((&sci, "0"));
    let e: i32 = exposant.parse().unwrap_or(0);
    if !(-4..15).contains(&e) {
        let signe = if e < 0 { '-' } else { '+' };
        return format!("{mantisse}E{signe}{:02}", e.abs());
    }
    format!("{f}")
}

fn nombre(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    nombre_virgule(n.as_f64().unwrap_or(0.0))
}

/// `ConvertTo-Json -Compress`.
pub fn compact(v: &Value) -> String {
    let mut o = String::new();
    ecrire_compact(v, &mut o);
    o
}

fn ecrire_compact(v: &Value, o: &mut String) {
    match v {
        Value::Null => o.push_str("null"),
        Value::Bool(b) => o.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => o.push_str(&nombre(n)),
        Value::String(s) => o.push_str(&texte(s)),
        Value::Array(a) => {
            o.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    o.push(',');
                }
                ecrire_compact(x, o);
            }
            o.push(']');
        }
        Value::Object(m) => {
            o.push('{');
            for (i, (k, x)) in m.iter().enumerate() {
                if i > 0 {
                    o.push(',');
                }
                o.push_str(&texte(k));
                o.push(':');
                ecrire_compact(x, o);
            }
            o.push('}');
        }
    }
}

/// `ConvertTo-Json` (forme lisible de PowerShell 5.1).
pub fn lisible(v: &Value) -> String {
    let mut o = String::new();
    ecrire_lisible(v, 0, &mut o);
    o
}

fn espaces(o: &mut String, n: usize) {
    o.extend(std::iter::repeat_n(' ', n));
}

fn longueur(s: &str) -> usize {
    s.encode_utf16().count()
}

fn ecrire_lisible(v: &Value, colonne: usize, o: &mut String) {
    match v {
        Value::Array(a) => {
            o.push_str("[\r\n");
            if a.is_empty() {
                o.push_str("\r\n");
            }
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    o.push_str(",\r\n");
                }
                espaces(o, colonne + 4);
                ecrire_lisible(x, colonne + 4, o);
            }
            if !a.is_empty() {
                o.push_str("\r\n");
            }
            espaces(o, colonne);
            o.push(']');
        }
        Value::Object(m) => {
            o.push_str("{\r\n");
            if m.is_empty() {
                o.push_str("\r\n");
            }
            for (i, (k, x)) in m.iter().enumerate() {
                if i > 0 {
                    o.push_str(",\r\n");
                }
                espaces(o, colonne + 4);
                let cle = texte(k);
                let decalage = colonne + 4 + longueur(&cle) + 3;
                o.push_str(&cle);
                o.push_str(":  ");
                ecrire_lisible(x, decalage, o);
            }
            if !m.is_empty() {
                o.push_str("\r\n");
            }
            espaces(o, colonne);
            o.push('}');
        }
        autre => ecrire_compact(autre, o),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn forme_de_powershell() {
        let v = json!({ "d": [], "clé'x": { "e": {}, "f": [{}, [], "a"] } });
        let attendu = "{\r\n    \"d\":  [\r\n\r\n          ],\r\n    \"clé\\u0027x\":  {\r\n                       \"e\":  {\r\n\r\n                             },\r\n                       \"f\":  [\r\n                                 {\r\n\r\n                                 },\r\n                                 [\r\n\r\n                                 ],\r\n                                 \"a\"\r\n                             ]\r\n                   }\r\n}";
        assert_eq!(lisible(&v), attendu);
    }

    #[test]
    fn nombres_comme_dotnet() {
        assert_eq!(nombre_virgule(1.2e-5), "1.2E-05");
        assert_eq!(nombre_virgule(1e15), "1E+15");
        assert_eq!(nombre_virgule(1e20), "1E+20");
        assert_eq!(nombre_virgule(1234567.891), "1234567.891");
        assert_eq!(nombre_virgule(0.1), "0.1");
        assert_eq!(nombre_virgule(3.0), "3");
        assert_eq!(nombre_virgule(-0.0), "0");
    }
}
