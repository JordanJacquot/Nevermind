//! Ce que PowerShell fait sans le dire, refait à l'identique : un nombre ou une valeur mis en texte (`"$x"`, `[string]`),
//! ce qui compte pour vrai (`if ($x)`), les comparaisons sans tenir compte des majuscules (`-eq`, `-like`, les clés d'une
//! table `@{}`), les formats `N0` et `0.#` de .NET, et les dates.

use serde_json::Value;

/// `[string]$x` et `"$x"` : vide pour `$null`, True/False, un nombre comme .NET, une liste en mots séparés d'une espace.
pub fn texte(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Value::Number(n) => nombre_json(n),
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(texte).collect::<Vec<_>>().join(" "),
        // (un objet JSON lu par ConvertFrom-Json s'écrit « @{...} » : jamais attendu ici)
        Value::Object(_) => String::new(),
    }
}

/// Un nombre JSON comme PowerShell l'écrit (entier tel quel, décimal sur 15 chiffres significatifs)
pub fn nombre_json(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    decimal(n.as_f64().unwrap_or(0.0))
}

/// Un `double` en texte comme `[double]::ToString()` de .NET Framework (format « G », 15 chiffres significatifs)
pub fn decimal(x: f64) -> String {
    if !x.is_finite() {
        return if x.is_nan() {
            "NaN".into()
        } else if x > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if x == 0.0 {
        return "0".into();
    }
    let (neg, chiffres, exp) = chiffres_15(x);
    // (« G » : écriture scientifique quand l'exposant est <= -5 ou >= 15)
    let corps = if !(-4..15).contains(&exp) {
        let mut m = chiffres[..1].to_string();
        if chiffres.len() > 1 {
            m.push('.');
            m.push_str(&chiffres[1..]);
        }
        format!("{m}E{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    } else if exp < 0 {
        format!("0.{}{}", "0".repeat((-exp - 1) as usize), chiffres)
    } else {
        let e = exp as usize;
        if chiffres.len() <= e + 1 {
            format!("{}{}", chiffres, "0".repeat(e + 1 - chiffres.len()))
        } else {
            format!("{}.{}", &chiffres[..=e], &chiffres[e + 1..])
        }
    };
    if neg { format!("-{corps}") } else { corps }
}

/// Les 15 chiffres significatifs de |x| (zéros de fin retirés) et l'exposant du premier
fn chiffres_15(x: f64) -> (bool, String, i32) {
    let s = format!("{:.14e}", x.abs());
    let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = e.parse().unwrap_or(0);
    let mut c: String = m.chars().filter(char::is_ascii_digit).collect();
    while c.len() > 1 && c.ends_with('0') {
        c.pop();
    }
    (x < 0.0, c, exp)
}

/// Arrondi « loin de zéro » à `n` décimales, sur les 15 chiffres significatifs (comme les formats de .NET Framework) :
/// rend la partie entière et les décimales en chiffres.
fn arrondi(x: f64, n: usize) -> (bool, String, String) {
    if x == 0.0 || !x.is_finite() {
        return (false, "0".into(), "0".repeat(n));
    }
    let (neg, chiffres, exp) = chiffres_15(x);
    // Tous les chiffres avec la virgule placée : entier . decimales
    let (mut ent, mut dec) = if exp < 0 {
        ("0".to_string(), format!("{}{}", "0".repeat((-exp - 1) as usize), chiffres))
    } else {
        let e = exp as usize + 1;
        if chiffres.len() <= e {
            (format!("{}{}", chiffres, "0".repeat(e - chiffres.len())), String::new())
        } else {
            (chiffres[..e].to_string(), chiffres[e..].to_string())
        }
    };
    let monter = dec.as_bytes().get(n).is_some_and(|c| *c >= b'5');
    while dec.len() < n {
        dec.push('0');
    }
    dec.truncate(n);
    if monter {
        let mut tout: Vec<u8> = format!("{ent}{dec}").into_bytes();
        let mut i = tout.len();
        loop {
            if i == 0 {
                tout.insert(0, b'1');
                break;
            }
            i -= 1;
            if tout[i] == b'9' {
                tout[i] = b'0';
            } else {
                tout[i] += 1;
                break;
            }
        }
        let t = String::from_utf8(tout).unwrap_or_default();
        let coupe = t.len() - n;
        ent = t[..coupe].to_string();
        dec = t[coupe..].to_string();
    }
    let ent = ent.trim_start_matches('0').to_string();
    let ent = if ent.is_empty() { "0".to_string() } else { ent };
    let zero = ent == "0" && dec.bytes().all(|c| c == b'0');
    (neg && !zero, ent, dec)
}

/// `'{0:N0}' -f $x` en français (fr-FR de Windows : espace fine insécable entre les milliers)
pub fn format_n0(x: f64) -> String {
    let (neg, ent, _) = arrondi(x, 0);
    let mut r = String::new();
    for (i, c) in ent.chars().enumerate() {
        if i > 0 && (ent.len() - i).is_multiple_of(3) {
            r.push('\u{202f}');
        }
        r.push(c);
    }
    if neg { format!("-{r}") } else { r }
}

/// `$x.ToString('0.#', fr-FR)` : une décimale au plus, virgule française
pub fn format_0_diese(x: f64) -> String {
    let (neg, ent, dec) = arrondi(x, 1);
    let s = if dec == "0" { ent } else { format!("{ent},{dec}") };
    if neg { format!("-{s}") } else { s }
}

/// `if ($x)` : ce que PowerShell compte pour vrai
pub fn vrai(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        // (une liste d'un seul élément vaut cet élément ; vide : faux)
        Value::Array(a) => match a.len() {
            0 => false,
            1 => vrai(&a[0]),
            _ => true,
        },
        Value::Object(_) => true,
    }
}

/// `@($x)` : une liste (un seul élément s'il n'en est pas une ; `$null` donne une liste d'un `$null`)
pub fn liste(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.clone(),
        autre => vec![autre.clone()],
    }
}

/// `@($x | Where-Object { $_ })` en textes (`[string]` de chaque élément gardé)
pub fn textes_vrais(v: &Value) -> Vec<String> {
    liste(v).iter().filter(|x| vrai(x)).map(texte).collect()
}

/// `-eq` entre deux textes (sans tenir compte des majuscules)
pub fn egal(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// `$a -like "$debut*"` (sans tenir compte des majuscules ; les jokers du début ne sont pas attendus)
pub fn commence(a: &str, debut: &str) -> bool {
    a.to_lowercase().starts_with(&debut.to_lowercase())
}

/// `$a -like "*$fin"`
pub fn finit(a: &str, fin: &str) -> bool {
    a.to_lowercase().ends_with(&fin.to_lowercase())
}

/// `[int]::TryParse` (espaces autour et signe permis)
pub fn entier_texte(t: &str) -> Option<i64> {
    let t = t.trim();
    let (signe, c) = match t.strip_prefix('-') {
        Some(r) => (-1, r),
        None => (1, t.strip_prefix('+').unwrap_or(t)),
    };
    if c.is_empty() || !c.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    c.parse::<i64>()
        .ok()
        .filter(|n| *n <= i64::from(i32::MAX) + 1)
        .map(|n| signe * n)
        .filter(|n| i32::try_from(*n).is_ok())
}

/// `[int]$x` : arrondi au pair le plus proche pour un décimal, texte lu ; `None` si PowerShell lèverait une erreur
pub fn entier(v: &Value) -> Option<i64> {
    match v {
        Value::Null => Some(0),
        Value::Bool(b) => Some(i64::from(*b)),
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|x| x.round_ties_even() as i64)),
        Value::String(s) => {
            if s.trim().is_empty() {
                return Some(0);
            }
            entier_texte(s).or_else(|| s.trim().parse::<f64>().ok().map(|x| x.round_ties_even() as i64))
        }
        _ => None,
    }
}

/// `[double]$x`
pub fn reel(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse().unwrap_or(0.0),
        Value::Bool(b) => f64::from(u8::from(*b)),
        _ => 0.0,
    }
}

/// Une date et une heure (heure locale, comme les `[datetime]` de PowerShell)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub annee: i64,
    pub mois: u32,
    pub jour: u32,
    pub heure: u32,
    pub minute: u32,
    pub seconde: u32,
}

impl Date {
    /// « aaaa-MM-jjTHH:mm:ss » (le format des juges)
    pub fn lire(t: &str) -> Option<Date> {
        let b = t.as_bytes();
        if b.len() != 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
            return None;
        }
        let n = |a: usize, z: usize| t.get(a..z).and_then(|x| x.parse::<u32>().ok());
        let d = Date {
            annee: i64::from(n(0, 4)?),
            mois: n(5, 7)?,
            jour: n(8, 10)?,
            heure: n(11, 13)?,
            minute: n(14, 16)?,
            seconde: n(17, 19)?,
        };
        (1..=12).contains(&d.mois).then_some(d)
    }

    /// `[datetime][long]$ticks` (intervalles de 100 ns depuis le 1er janvier de l'an 1)
    pub fn depuis_ticks(ticks: i64) -> Date {
        let s = ticks.div_euclid(10_000_000);
        let (j, r) = (s.div_euclid(86400), s.rem_euclid(86400));
        // (jours depuis le 1er janvier de l'an 1 jusqu'au 1er janvier 1970 : 719162)
        let (a, m, d) = civil(j - 719_162);
        Date {
            annee: a,
            mois: m,
            jour: d,
            heure: (r / 3600) as u32,
            minute: ((r % 3600) / 60) as u32,
            seconde: (r % 60) as u32,
        }
    }

    pub fn ticks(&self) -> i64 {
        let j = jours_civils(self.annee, self.mois, self.jour) + 719_162;
        (j * 86400 + i64::from(self.heure) * 3600 + i64::from(self.minute) * 60 + i64::from(self.seconde)) * 10_000_000
    }

    /// `ToString('dd/MM à HH:mm')`
    pub fn jour_heure(&self) -> String {
        format!("{:02}/{:02} à {:02}:{:02}", self.jour, self.mois, self.heure, self.minute)
    }

    /// `ToString('dd/MM/yyyy HH:mm')`
    pub fn complete(&self) -> String {
        format!("{:02}/{:02}/{:04} {:02}:{:02}", self.jour, self.mois, self.annee, self.heure, self.minute)
    }

    /// `ToString('dd/MM/yyyy à HH:mm')` (la date d'une ligne de l'historique)
    pub fn historique(&self) -> String {
        format!("{:02}/{:02}/{:04} à {:02}:{:02}", self.jour, self.mois, self.annee, self.heure, self.minute)
    }

    /// « aaaa-MM-jjTHH:mm:ss »
    pub fn iso(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.annee, self.mois, self.jour, self.heure, self.minute, self.seconde
        )
    }
}

fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn jours_civils(a: i64, m: u32, d: u32) -> i64 {
    let a = if m <= 2 { a - 1 } else { a };
    let era = a.div_euclid(400);
    let yoe = a.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `\w` de .NET (lettre, chiffre, trait bas)
pub fn mot(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres_comme_dotnet() {
        assert_eq!(decimal(12.5), "12.5");
        assert_eq!(decimal(0.1 + 0.2), "0.3");
        assert_eq!(decimal(2.25), "2.25");
        assert_eq!(decimal(1e20), "1E+20");
        assert_eq!(decimal(0.00001), "1E-05");
        assert_eq!(decimal(150.0), "150");
        assert_eq!(format_n0(1234.5), "1\u{202f}235");
        assert_eq!(format_n0(48.5), "49");
        assert_eq!(format_n0(912.345), "912");
        assert_eq!(format_n0(1_234_567.0), "1\u{202f}234\u{202f}567");
        assert_eq!(format_0_diese(2.5), "2,5");
        assert_eq!(format_0_diese(100.0), "100");
        assert_eq!(format_0_diese(0.95), "1");
        assert_eq!(format_0_diese(866.7), "866,7");
    }

    #[test]
    fn dates_et_ticks() {
        let d = Date::lire("2026-11-04T21:05:07").unwrap();
        assert_eq!(Date::depuis_ticks(d.ticks()), d);
        assert_eq!(d.jour_heure(), "04/11 à 21:05");
        assert_eq!(Date::depuis_ticks(0).iso(), "0001-01-01T00:00:00");
        assert!(Date::lire("2026-13-01T00:00:00").is_none());
    }
}
