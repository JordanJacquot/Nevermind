//! Les nombres et les textes comme la version PowerShell les écrit (culture fr-FR de .NET Framework) :
//! `'{0:N1}' -f` (arrondi au plus loin de zéro, espace fine insécable U+202F entre les milliers, virgule),
//! `"$x"` (culture invariante, 15 chiffres), `[math]::Round` et `[int]` (arrondi au pair).

/// L'espace des milliers de la culture fr-FR (Windows 10 et 11)
pub const ESPACE_MILLIERS: char = '\u{202F}';

/// Les 15 chiffres significatifs d'un nombre (comme .NET Framework) et la place de la virgule
fn chiffres15(x: f64) -> (Vec<u8>, i32) {
    let s = format!("{:.14e}", x.abs());
    let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
    let chiffres: Vec<u8> = m.bytes().filter(u8::is_ascii_digit).map(|c| c - b'0').collect();
    (chiffres, e.parse::<i32>().unwrap_or(0))
}

/// `'{0:Nd}' -f x` en fr-FR
pub fn nombre_n(x: f64, decimales: usize) -> String {
    if !x.is_finite() {
        return if x.is_nan() {
            "NaN".into()
        } else if x > 0.0 {
            "∞".into()
        } else {
            "-∞".into()
        };
    }
    let (ch, e) = chiffres15(x);
    // Chiffres avant la virgule : e + 1 (peut être <= 0)
    let avant = e + 1;
    // On garde avant + decimales chiffres, arrondis au plus loin de zéro sur le chiffre suivant
    let garder = avant + decimales as i32;
    let mut d: Vec<u8> = Vec::new();
    if garder > 0 {
        for i in 0..garder as usize {
            d.push(*ch.get(i).unwrap_or(&0));
        }
        let suivant = if garder >= 0 { *ch.get(garder as usize).unwrap_or(&0) } else { 0 };
        if suivant >= 5 {
            let mut i = d.len();
            loop {
                if i == 0 {
                    d.insert(0, 1);
                    break;
                }
                i -= 1;
                if d[i] == 9 {
                    d[i] = 0;
                } else {
                    d[i] += 1;
                    break;
                }
            }
        }
    } else if garder == 0 && ch.first().copied().unwrap_or(0) >= 5 {
        d.push(1);
    }
    // d contient les chiffres jusqu'à la dernière décimale gardée
    let total = d.len();
    let n_frac = decimales.min(total);
    let mut entier: Vec<u8> = d[..total - n_frac].to_vec();
    let mut frac: Vec<u8> = d[total - n_frac..].to_vec();
    while frac.len() < decimales {
        frac.insert(0, 0);
    }
    while entier.len() > 1 && entier[0] == 0 {
        entier.remove(0);
    }
    if entier.is_empty() {
        entier.push(0);
    }
    let nul = entier.iter().all(|&c| c == 0) && frac.iter().all(|&c| c == 0);
    let mut r = String::new();
    if x < 0.0 && !nul {
        r.push('-');
    }
    let n = entier.len();
    for (i, c) in entier.iter().enumerate() {
        if i > 0 && (n - i).is_multiple_of(3) {
            r.push(ESPACE_MILLIERS);
        }
        r.push((b'0' + c) as char);
    }
    if decimales > 0 {
        r.push(',');
        for c in frac {
            r.push((b'0' + c) as char);
        }
    }
    r
}

/// `"$x"` pour un nombre à virgule : culture invariante, 15 chiffres significatifs (`Double.ToString()`)
pub fn ps_nombre(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if !x.is_finite() {
        return if x.is_nan() {
            "NaN".into()
        } else if x > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    let (ch, e) = chiffres15(x);
    let mut ch = ch;
    while ch.len() > 1 && ch.last() == Some(&0) {
        ch.pop();
    }
    let signe = if x < 0.0 { "-" } else { "" };
    let txt: String = ch.iter().map(|c| (b'0' + c) as char).collect();
    if !(-5..15).contains(&e) {
        let (a, b) = txt.split_at(1);
        let m = if b.is_empty() { a.to_string() } else { format!("{a}.{b}") };
        return format!("{signe}{m}E{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
    }
    if e < 0 {
        format!("{signe}0.{}{txt}", "0".repeat((-e - 1) as usize))
    } else {
        let avant = (e + 1) as usize;
        if txt.len() <= avant {
            format!("{signe}{txt}{}", "0".repeat(avant - txt.len()))
        } else {
            format!("{signe}{}.{}", &txt[..avant], &txt[avant..])
        }
    }
}

/// `[math]::Round(x, d)` (arrondi au pair, comme .NET)
pub fn arrondi(x: f64, d: i32) -> f64 {
    if d == 0 {
        return x.round_ties_even();
    }
    let p = 10f64.powi(d);
    let v = x * p;
    if v.abs() > 1e16 {
        return x;
    }
    v.round_ties_even() / p
}

/// `[int]x` (arrondi au pair)
pub fn entier(x: f64) -> i64 {
    x.round_ties_even() as i64
}

const KO: f64 = 1024.0;
const MO: f64 = 1024.0 * 1024.0;
const GO: f64 = 1024.0 * 1024.0 * 1024.0;

/// `Format-Bytes` (trafic.ps1)
pub fn octets(b: f64) -> String {
    if b >= GO {
        return format!("{} Go", nombre_n(b / GO, 1));
    }
    if b >= MO {
        return format!("{} Mo", nombre_n(b / MO, 1));
    }
    if b >= KO {
        return format!("{} Ko", nombre_n(b / KO, 0));
    }
    format!("{} o", entier(b))
}

/// `Format-Ms` (lag.ps1) : « ? » si absent ou négatif
pub fn ms(v: Option<f64>) -> String {
    match v {
        Some(x) if x >= 0.0 => format!("{} ms", nombre_n(x, 0)),
        _ => "?".into(),
    }
}

/// `Format-PlayTime` (jeu.ps1)
pub fn duree_jeu(s: f64) -> String {
    if s < 90.0 {
        return format!("{} s", entier(s));
    }
    let m = entier(s / 60.0);
    if m < 60 {
        return format!("{m} min");
    }
    format!("{} h {:02}", entier(m as f64 / 60.0), m % 60)
}

/// Les noms de pays connus (`$CountryNames`, trafic.ps1)
const PAYS: &[(&str, &str)] = &[
    ("US", "États-Unis"),
    ("FR", "France"),
    ("IE", "Irlande"),
    ("DE", "Allemagne"),
    ("NL", "Pays-Bas"),
    ("GB", "Royaume-Uni"),
    ("BE", "Belgique"),
    ("CH", "Suisse"),
    ("ES", "Espagne"),
    ("IT", "Italie"),
    ("SE", "Suède"),
    ("FI", "Finlande"),
    ("PL", "Pologne"),
    ("LU", "Luxembourg"),
    ("AT", "Autriche"),
    ("CA", "Canada"),
    ("JP", "Japon"),
    ("KR", "Corée du Sud"),
    ("CN", "Chine"),
    ("HK", "Hong Kong"),
    ("TW", "Taïwan"),
    ("SG", "Singapour"),
    ("IN", "Inde"),
    ("AU", "Australie"),
    ("BR", "Brésil"),
    ("RU", "Russie"),
    ("UA", "Ukraine"),
    ("IL", "Israël"),
    ("AE", "Émirats arabes unis"),
    ("ZA", "Afrique du Sud"),
    ("EU", "Europe"),
];

/// `ToTitleCase` de .NET : une lettre en début de mot passe en majuscule (l'apostrophe ne coupe pas le mot)
fn titre(s: &str) -> String {
    let mut r = String::new();
    let mut dans_mot = false;
    for c in s.chars() {
        if c.is_alphabetic() {
            if dans_mot {
                r.push(c);
            } else {
                r.extend(c.to_uppercase());
            }
            dans_mot = true;
        } else {
            r.push(c);
            dans_mot = c == '\'' || c == '\u{2019}' || c.is_numeric();
        }
    }
    r
}

/// `Format-Country` (trafic.ps1)
pub fn pays(c: &str) -> String {
    if c.is_empty() {
        return String::new();
    }
    if c.chars().count() == 2 {
        let m = c.to_uppercase();
        return PAYS.iter().find(|(k, _)| *k == m).map(|(_, v)| v.to_string()).unwrap_or(m);
    }
    let bas = c.to_lowercase();
    if bas.contains("united states") || bas == "usa" {
        return "États-Unis".into();
    }
    titre(&bas)
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn nombres() {
        assert_eq!(nombre_n(1234567.5, 0), "1\u{202F}234\u{202F}568");
        assert_eq!(nombre_n(1536.25, 1), "1\u{202F}536,3");
        assert_eq!(nombre_n(12.5, 0), "13");
        assert_eq!(nombre_n(0.4, 0), "0");
        assert_eq!(nombre_n(0.04, 1), "0,0");
        assert_eq!(nombre_n(999.96, 1), "1\u{202F}000,0");
        assert_eq!(ps_nombre(2.5), "2.5");
        assert_eq!(ps_nombre(100.0), "100");
        assert_eq!(ps_nombre(0.1 + 0.2), "0.3");
        assert_eq!(arrondi(2.5, 0), 2.0);
        assert_eq!(entier(89.5), 90);
        assert_eq!(entier(88.5), 88);
    }
}
