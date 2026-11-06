//! Les petits formats de la version PowerShell, à l'identique : nombres « {0:N0} » et « {0:N1} » de la culture
//! fr-FR (espace fine insécable U+202F entre les milliers, virgule), tailles (Format-Size), durées (Format-PlayTime),
//! deux-points des textes de l'écran (Format-TexteKit), et les arrondis de PowerShell ([int] et [math]::Round
//! arrondissent au pair, « {0:N0} » arrondit loin de zéro).

/// Séparateur des milliers de fr-FR sous Windows 11 (lu sur le PC de Jordan : 8239 = U+202F).
pub const MILLIERS: char = '\u{202f}';

/// `[int]$x` de PowerShell : arrondi au pair (2,5 donne 2, 3,5 donne 4).
pub fn ps_int(x: f64) -> i64 {
    if !x.is_finite() {
        return 0;
    }
    x.round_ties_even() as i64
}

/// `[math]::Round($x, $n)` : arrondi au pair à n décimales (même calcul que .NET Framework : mise à l'échelle, arrondi
/// au pair, retour).
pub fn ps_round(x: f64, n: i32) -> f64 {
    if !x.is_finite() {
        return x;
    }
    if n == 0 {
        return x.round_ties_even();
    }
    let p = 10f64.powi(n);
    let v = x * p;
    // (.NET : si la mise à l'échelle déborde, la valeur est rendue telle quelle)
    if v.abs() >= 1e16 {
        return x;
    }
    v.round_ties_even() / p
}

/// Les 15 chiffres significatifs que .NET Framework garde avant de formater un double, arrondis loin de zéro à
/// `decimales` décimales : (signe négatif, partie entière, partie décimale).
fn chiffres(x: f64, decimales: usize) -> (bool, String, String) {
    if !x.is_finite() || x == 0.0 {
        return (false, "0".into(), "0".repeat(decimales));
    }
    let s = format!("{:.14e}", x.abs());
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let e: i64 = exp.parse().unwrap_or(0);
    let d: Vec<u8> = mant.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();
    // valeur = d0,d1d2... x 10^e
    let (mut ent, mut frac): (Vec<u8>, Vec<u8>) = if e >= 0 {
        let e = e as usize;
        let mut ent: Vec<u8> = d.iter().take(e + 1).copied().collect();
        while ent.len() < e + 1 {
            ent.push(0);
        }
        (ent, d.iter().skip(e + 1).copied().collect())
    } else {
        let mut f = vec![0u8; (-e - 1) as usize];
        f.extend(&d);
        (vec![0], f)
    };
    while frac.len() <= decimales {
        frac.push(0);
    }
    let retenue = frac[decimales] >= 5;
    frac.truncate(decimales);
    if retenue {
        let mut fini = false;
        for c in frac.iter_mut().rev() {
            if *c == 9 {
                *c = 0;
            } else {
                *c += 1;
                fini = true;
                break;
            }
        }
        if !fini {
            for c in ent.iter_mut().rev() {
                if *c == 9 {
                    *c = 0;
                } else {
                    *c += 1;
                    fini = true;
                    break;
                }
            }
            if !fini {
                ent.insert(0, 1);
            }
        }
    }
    let ent: String = ent.iter().map(|c| char::from(b'0' + c)).collect();
    let ent = ent.trim_start_matches('0');
    let ent = if ent.is_empty() { "0".to_string() } else { ent.to_string() };
    let dec: String = frac.iter().map(|c| char::from(b'0' + c)).collect();
    let nul = ent == "0" && dec.bytes().all(|b| b == b'0');
    (x < 0.0 && !nul, ent, dec)
}

fn grouper(ent: &str) -> String {
    let mut out = String::new();
    let n = ent.len();
    for (i, c) in ent.chars().enumerate() {
        if i > 0 && (n - i).is_multiple_of(3) {
            out.push(MILLIERS);
        }
        out.push(c);
    }
    out
}

/// `'{0:N<d>}' -f $x` en fr-FR.
pub fn nombre(x: f64, decimales: usize) -> String {
    let (neg, ent, dec) = chiffres(x, decimales);
    let mut s = String::new();
    if neg {
        s.push('-');
    }
    s.push_str(&grouper(&ent));
    if decimales > 0 {
        s.push(',');
        s.push_str(&dec);
    }
    s
}

/// `'{0:N0}' -f $x`
pub fn n0(x: f64) -> String {
    nombre(x, 0)
}

const KO: f64 = 1024.0;
const MO: f64 = 1024.0 * 1024.0;
const GO: f64 = 1024.0 * 1024.0 * 1024.0;
const TO: f64 = 1024.0 * 1024.0 * 1024.0 * 1024.0;

/// Format-Size (systeme.ps1)
pub fn taille(octets: f64) -> String {
    if octets >= TO {
        return format!("{} To", nombre(octets / TO, 1));
    }
    if octets >= GO {
        return format!("{} Go", nombre(octets / GO, 1));
    }
    if octets >= MO {
        return format!("{} Mo", nombre(octets / MO, 0));
    }
    format!("{} Ko", nombre(octets / KO, 0))
}

/// Format-PlayTime (jeu.ps1), arrondis de PowerShell compris (90 minutes donnent « 2 h 30 », comme l'original).
pub fn duree_jeu(secondes: f64) -> String {
    if secondes < 90.0 {
        return format!("{} s", ps_int(secondes));
    }
    let m = ps_int(secondes / 60.0);
    if m < 60 {
        return format!("{m} min");
    }
    format!("{} h {:02}", ps_int(m as f64 / 60.0), m.rem_euclid(60))
}

fn minuscule_kit(c: char) -> bool {
    c.is_ascii_lowercase() || ('\u{e0}'..='\u{ff}').contains(&c)
}

/// Les espaces de `\s` en .NET
fn espace_net(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | '\u{85}') || c.is_whitespace()
}

/// Format-TexteKit (accueil-console.ps1) : « mot: suite » devient « mot : suite ».
pub fn texte_kit(t: &str) -> String {
    let c: Vec<char> = t.chars().collect();
    let mut out = String::with_capacity(t.len() + 4);
    for i in 0..c.len() {
        if c[i] == ':'
            && i >= 2
            && minuscule_kit(c[i - 1])
            && minuscule_kit(c[i - 2])
            && c.get(i + 1).is_some_and(|&x| espace_net(x))
        {
            out.push_str(" :");
        } else {
            out.push(c[i]);
        }
    }
    out
}

/// Une chaîne en minuscules comme `ToLower()` de .NET (culture courante).
pub fn minuscules(t: &str) -> String {
    t.to_lowercase()
}

/// Égalité sans tenir compte des majuscules (`-eq`, `-contains` de PowerShell)
pub fn egal(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Lettre sans accent, pour classer comme la culture fr-FR (Sort-Object) : d'abord sans accent ni majuscule
fn sans_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'a',
        'ç' | 'Ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ñ' | 'Ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' => 'y',
        _ => c.to_lowercase().next().unwrap_or(c),
    }
}

/// Comparaison de deux noms comme Sort-Object en fr-FR (approchée : lettres sans accent ni majuscule, les tirets et
/// apostrophes ne comptent pas ; à égalité, les accents puis les majuscules).
pub fn comparer_noms(a: &str, b: &str) -> std::cmp::Ordering {
    let cle = |s: &str| -> Vec<char> { s.chars().filter(|c| *c != '-' && *c != '\'').map(sans_accent).collect() };
    cle(a)
        .cmp(&cle(b))
        .then_with(|| {
            let acc = |s: &str| -> Vec<bool> {
                s.chars().map(|c| sans_accent(c) != c.to_lowercase().next().unwrap_or(c)).collect()
            };
            acc(a).cmp(&acc(b))
        })
        .then_with(|| {
            // (minuscule avant majuscule)
            let maj = |s: &str| -> Vec<bool> { s.chars().map(char::is_uppercase).collect() };
            maj(a).cmp(&maj(b))
        })
}

/// Pluriel simple : `s` si n > 1
pub fn s(n: i64) -> &'static str {
    if n > 1 { "s" } else { "" }
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn nombres_comme_dotnet() {
        assert_eq!(n0(0.0), "0");
        assert_eq!(n0(0.5), "1");
        assert_eq!(n0(1.5), "2");
        assert_eq!(n0(2.5), "3");
        assert_eq!(n0(-2.5), "-3");
        assert_eq!(n0(1234.5), "1\u{202f}235");
        assert_eq!(n0(999.5), "1\u{202f}000");
        assert_eq!(n0(1234567.0), "1\u{202f}234\u{202f}567");
        assert_eq!(nombre(1.15, 1), "1,2");
        assert_eq!(nombre(1.05, 1), "1,1");
        assert_eq!(nombre(0.04, 1), "0,0");
        assert_eq!(nombre(0.05, 1), "0,1");
        assert_eq!(nombre(9.96, 1), "10,0");
        assert_eq!(nombre(1234.56, 1), "1\u{202f}234,6");
        assert_eq!(nombre(-0.04, 1), "0,0");
        assert_eq!(n0(0.0004), "0");
    }

    #[test]
    fn tailles() {
        assert_eq!(taille(0.0), "0 Ko");
        assert_eq!(taille(512.0), "1 Ko");
        assert_eq!(taille(1.5 * GO), "1,5 Go");
        assert_eq!(taille(1023.0 * MO), "1\u{202f}023 Mo");
    }

    #[test]
    fn durees_et_textes() {
        assert_eq!(duree_jeu(30.4), "30 s");
        assert_eq!(duree_jeu(5400.0), "2 h 30");
        assert_eq!(duree_jeu(600.0), "10 min");
        assert_eq!(texte_kit("Il sert: au démarrage."), "Il sert : au démarrage.");
        assert_eq!(texte_kit("A: b"), "A: b");
        assert_eq!(texte_kit("ab:c"), "ab:c");
        assert_eq!(ps_int(2.5), 2);
        assert_eq!(ps_int(3.5), 4);
    }
}
