//! Les petites règles de PowerShell qu'il faut refaire à l'identique : comparaisons sans tenir compte des majuscules
//! (`-eq`, `-contains`, `-match`, les clés des tables), découpage (`-split`), arrondis et nombres à la française.

use regex::Regex;

/// `-eq` de PowerShell entre deux textes (sans tenir compte des majuscules)
pub fn egal(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

/// `-contains` de PowerShell
pub fn contient(liste: &[String], x: &str) -> bool {
    liste.iter().any(|e| egal(e, x))
}

/// Une expression rationnelle de PowerShell (`-match` est toujours sans majuscules)
pub fn re(motif: &str) -> Regex {
    Regex::new(&format!("(?i){motif}")).unwrap_or_else(|_| Regex::new("$^").expect("motif vide"))
}

/// `$a -split '\|'` : toutes les parties (une ligne vide donne une partie vide)
pub fn parts(ligne: &str) -> Vec<String> {
    ligne.split('|').map(str::to_string).collect()
}

/// `$a -split '\|', N` : au plus N parties, la dernière garde le reste
pub fn parts_n(ligne: &str, n: usize) -> Vec<String> {
    ligne.splitn(n, '|').map(str::to_string).collect()
}

/// La partie `i` (`$p[i]`), vide si elle n'existe pas (`[string]$null`)
pub fn part(p: &[String], i: usize) -> &str {
    p.get(i).map(String::as_str).unwrap_or("")
}

/// `n un` ou `n plusieurs` (le `$plural` de l'audit)
pub fn pluriel(n: usize, un: &str, plusieurs: &str) -> String {
    if n > 1 { format!("{n} {plusieurs}") } else { format!("{n} {un}") }
}

/// `[int]` de PowerShell sur un nombre à virgule : arrondi au pair le plus proche
pub fn arrondi_pair(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// `[math]::Round(x, d)` de .NET (au pair le plus proche)
pub fn arrondi_dec(x: f64, d: i32) -> f64 {
    let p = 10f64.powi(d);
    (x * p).round_ties_even() / p
}

/// Un nombre comme PowerShell l'écrit dans un texte (`"$x"`, culture neutre) : sans « .0 » pour un entier
pub fn nombre(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") }
}

/// Les chiffres de .NET Framework avant un format (`ToString('0.#')`, `'{0:N0}'`) : le nombre ramené à 15 chiffres
/// significatifs, puis arrondi au plus loin de zéro. Rend (négatif, chiffres de la partie entière, chiffres après la
/// virgule) avec `decimales` chiffres après la virgule.
fn chiffres_net(x: f64, decimales: usize) -> (bool, String, String) {
    let neg = x < 0.0;
    let s = format!("{:.14e}", x.abs());
    let (mantisse, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let mut chiffres: Vec<u8> = mantisse.bytes().filter(u8::is_ascii_digit).map(|c| c - b'0').collect();
    // position de la virgule : après (exp + 1) chiffres
    let mut virgule = exp + 1;
    if virgule < 0 {
        let mut v = vec![0u8; (-virgule) as usize];
        v.extend(chiffres);
        chiffres = v;
        virgule = 0;
    }
    let garder = virgule as usize + decimales;
    while chiffres.len() <= garder {
        chiffres.push(0);
    }
    let arrondir = chiffres[garder] >= 5;
    chiffres.truncate(garder);
    let mut virgule = virgule as usize;
    if arrondir {
        let mut i = chiffres.len();
        loop {
            if i == 0 {
                chiffres.insert(0, 1);
                virgule += 1;
                break;
            }
            i -= 1;
            if chiffres[i] == 9 {
                chiffres[i] = 0;
            } else {
                chiffres[i] += 1;
                break;
            }
        }
    }
    let entier: String = chiffres[..virgule].iter().map(|c| char::from(b'0' + c)).collect();
    let entier = entier.trim_start_matches('0').to_string();
    let entier = if entier.is_empty() { "0".to_string() } else { entier };
    let fraction: String = chiffres[virgule..].iter().map(|c| char::from(b'0' + c)).collect();
    let zero = entier == "0" && fraction.bytes().all(|c| c == b'0');
    (neg && !zero, entier, fraction)
}

/// `x.ToString('0.#', fr-FR)` : un chiffre après la virgule au plus, virgule française
pub fn format_0_diese(x: f64) -> String {
    let (neg, e, f) = chiffres_net(x, 1);
    let f = f.trim_end_matches('0');
    format!("{}{e}{}", if neg { "-" } else { "" }, if f.is_empty() { String::new() } else { format!(",{f}") })
}

/// `'{0:N0}' -f x` en fr-FR (.NET Framework) : sans décimale, milliers séparés par une espace fine insécable
/// (U+202F, comme les données de culture de Windows 11)
pub fn format_n0(x: f64) -> String {
    format_n(x, 0)
}

/// `'{0:Nd}' -f x` en fr-FR
pub fn format_n(x: f64, d: usize) -> String {
    let (neg, e, f) = chiffres_net(x, d);
    let mut r = String::new();
    let n = e.len();
    for (i, c) in e.chars().enumerate() {
        if i > 0 && (n - i) % 3 == 0 {
            r.push('\u{202f}');
        }
        r.push(c);
    }
    if d > 0 {
        r.push(',');
        r.push_str(&f);
    }
    format!("{}{r}", if neg { "-" } else { "" })
}

/// `[version]$ip` pour trier les adresses (une adresse qui n'en est pas une : 255.255.255.255, comme le try/catch)
pub fn cle_version(ip: &str) -> (i64, i64, i64, i64) {
    let p: Vec<&str> = ip.split('.').collect();
    let n: Option<Vec<i64>> =
        p.iter().map(|x| x.trim().parse::<i64>().ok().filter(|v| (0..=i64::from(i32::MAX)).contains(v))).collect();
    match n {
        Some(v) if (2..=4).contains(&v.len()) => {
            (v[0], v[1], v.get(2).copied().unwrap_or(-1), v.get(3).copied().unwrap_or(-1))
        }
        _ => (255, 255, 255, 255),
    }
}

/// `[int]::TryParse` (espaces autour permis, signe permis)
pub fn entier(s: &str) -> Option<i64> {
    let t = s.trim();
    let t = t.strip_prefix('+').unwrap_or(t);
    t.parse::<i64>().ok().filter(|v| i32::try_from(*v).is_ok())
}

/// `[uri]::UnescapeDataString`
pub fn desechapper(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out.clone()).unwrap_or_else(|_| String::from_utf8_lossy(&out).into_owned())
}

fn hex(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|d| d as u8)
}

/// Le texte d'un JSON comme `[string]$x` le donne
pub fn chaine(v: Option<&serde_json::Value>) -> String {
    use serde_json::Value;
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => if *b { "True" } else { "False" }.to_string(),
        Some(Value::Number(n)) => n.as_f64().map(nombre).unwrap_or_else(|| n.to_string()),
        Some(Value::Array(a)) => a.iter().map(|x| chaine(Some(x))).collect::<Vec<_>>().join(" "),
        Some(Value::Object(_)) => String::new(),
    }
}

/// `@($x)` d'un JSON : une liste de textes (une valeur seule devient une liste d'un élément, rien une liste vide)
pub fn liste(v: Option<&serde_json::Value>) -> Vec<String> {
    use serde_json::Value;
    match v {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a.iter().map(|x| chaine(Some(x))).collect(),
        Some(x) => vec![chaine(Some(x))],
    }
}

/// `[bool]$x` d'un JSON
pub fn vrai(v: Option<&serde_json::Value>) -> bool {
    use serde_json::Value;
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(_)) => true,
    }
}

/// Un nombre d'un JSON (`[double]$x`)
pub fn reel(v: Option<&serde_json::Value>) -> f64 {
    use serde_json::Value;
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_net() {
        assert_eq!(format_0_diese(2.55), "2,6");
        assert_eq!(format_0_diese(1.0), "1");
        assert_eq!(format_0_diese(99.95), "100");
        assert_eq!(format_0_diese(0.04), "0");
        assert_eq!(format_n0(1234.5), "1\u{202f}235");
        assert_eq!(format_n0(940.4), "940");
        assert_eq!(format_n0(0.5), "1");
        assert_eq!(format_n0(1_234_567.0), "1\u{202f}234\u{202f}567");
    }

    #[test]
    fn versions() {
        assert!(cle_version("192.168.1.2") < cle_version("192.168.1.10"));
        assert_eq!(cle_version("fe80::1"), (255, 255, 255, 255));
    }
}
