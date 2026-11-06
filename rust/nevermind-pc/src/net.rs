//! Les petites règles de .NET et de PowerShell dont dépendent les textes et l'ordre de la version PowerShell :
//! arrondis (`[math]::Round`, `[int]`), formats français (`{0:N1}`, `Format-Size`), conversions souples
//! (`[string]`, `[int]`, vrai ou faux d'une valeur), et le tri de `Sort-Object` (pas stable : le même tri que .NET).

use serde_json::Value;
use std::cmp::Ordering;

/// `[math]::Round(x)` : au plus proche, et à égalité vers le pair (2,5 donne 2).
pub fn arrondi(x: f64) -> f64 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 { 2.0 * (x / 2.0).round() } else { r }
}

/// `[int]x` (et `[long]`) : comme `[math]::Round`.
pub fn entier(x: f64) -> i64 {
    if x.is_nan() { 0 } else { arrondi(x) as i64 }
}

/// Le nombre écrit par .NET avec 15 chiffres significatifs (ce que .NET Framework garde avant d'arrondir un format).
fn chiffres15(x: f64) -> (bool, Vec<u8>, i32) {
    // x = 0,d1d2...d15 × 10^exp
    let s = format!("{:.14e}", x.abs());
    let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = e.parse().unwrap_or(0) + 1;
    let digits: Vec<u8> = m.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();
    (x.is_sign_negative() && x != 0.0, digits, exp)
}

/// `x.ToString("N<dec>")` en français (fr-FR) : groupes de trois séparés par une espace fine insécable (U+202F),
/// virgule décimale, arrondi « au plus loin de zéro » sur les 15 chiffres significatifs (comme .NET Framework).
pub fn n_fr(x: f64, dec: usize) -> String {
    nombre(x, dec, true)
}

/// `x.ToString("F<dec>")` en français : sans les groupes de trois.
pub fn f_fr(x: f64, dec: usize) -> String {
    nombre(x, dec, false)
}

fn nombre(x: f64, dec: usize, groupes: bool) -> String {
    if !x.is_finite() {
        return if x.is_nan() {
            "NaN".into()
        } else if x > 0.0 {
            "∞".into()
        } else {
            "-∞".into()
        };
    }
    let (neg, d, exp) = chiffres15(x);
    // Les chiffres de la partie entière et des décimales voulues, arrondis au plus loin de zéro
    let garde = exp + dec as i32; // nombre de chiffres gardés (depuis le premier chiffre significatif)
    let mut v: Vec<u8> = Vec::new();
    let mut retenue = false;
    if garde > 0 {
        for i in 0..garde as usize {
            v.push(*d.get(i).unwrap_or(&0));
        }
        if d.get(garde as usize).copied().unwrap_or(0) >= 5 {
            retenue = true;
        }
    } else if garde == 0 && d.first().copied().unwrap_or(0) >= 5 {
        retenue = true;
    }
    let mut exp_final = exp;
    if retenue {
        let mut i = v.len();
        loop {
            if i == 0 {
                v.insert(0, 1);
                exp_final += 1;
                break;
            }
            i -= 1;
            if v[i] == 9 {
                v[i] = 0;
            } else {
                v[i] += 1;
                break;
            }
        }
    }
    // v : les chiffres depuis 10^(exp_final-1) jusqu'à 10^-dec
    let nb_entiers = exp_final.max(0) as usize;
    let mut tous: Vec<u8> = Vec::new();
    if exp_final < 0 {
        tous.extend(std::iter::repeat_n(0, (-exp_final) as usize));
    }
    tous.extend(&v);
    let total = nb_entiers + dec;
    while tous.len() < total {
        tous.push(0);
    }
    let (ent, decs) = tous.split_at(tous.len().saturating_sub(dec));
    let mut ent: String = ent.iter().map(|c| (b'0' + c) as char).collect();
    let ent_trim = ent.trim_start_matches('0');
    ent = if ent_trim.is_empty() { "0".into() } else { ent_trim.into() };
    let mut s = String::new();
    if groupes {
        let n = ent.chars().count();
        for (i, c) in ent.chars().enumerate() {
            if i > 0 && (n - i).is_multiple_of(3) {
                s.push('\u{202f}');
            }
            s.push(c);
        }
    } else {
        s = ent;
    }
    if dec > 0 {
        s.push(',');
        s.extend(decs.iter().map(|c| (b'0' + c) as char));
    }
    let zero = s.chars().all(|c| c == '0' || c == ',' || c == '\u{202f}');
    if neg && !zero {
        s.insert(0, '-');
    }
    s
}

/// `Format-Size` (systeme.ps1) : « 1,5 Go », « 512 Mo », « 12 Ko ».
pub fn taille(octets: f64) -> String {
    const KO: f64 = 1024.0;
    if octets >= KO * KO * KO * KO {
        format!("{} To", n_fr(octets / (KO * KO * KO * KO), 1))
    } else if octets >= KO * KO * KO {
        format!("{} Go", n_fr(octets / (KO * KO * KO), 1))
    } else if octets >= KO * KO {
        format!("{} Mo", n_fr(octets / (KO * KO), 0))
    } else {
        format!("{} Ko", n_fr(octets / KO, 0))
    }
}

/// Un nombre comme PowerShell l'écrit dans un texte (`"$x"`) : entier tel quel, sinon 15 chiffres significatifs.
pub fn texte_nombre(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        return format!("{}", x as i64);
    }
    let (neg, d, exp) = chiffres15(x);
    let mut d = d;
    while d.len() > 1 && d.last() == Some(&0) {
        d.pop();
    }
    let chiffres: String = d.iter().map(|c| (b'0' + c) as char).collect();
    let mut s = if !(-4..=15).contains(&exp) {
        let mut m = chiffres.clone();
        if m.len() > 1 {
            m.insert(1, '.');
        }
        format!("{m}E{}{:02}", if exp > 0 { "+" } else { "-" }, (exp - 1).abs())
    } else if exp <= 0 {
        format!("0.{}{}", "0".repeat((-exp) as usize), chiffres)
    } else if (exp as usize) >= chiffres.len() {
        format!("{}{}", chiffres, "0".repeat(exp as usize - chiffres.len()))
    } else {
        format!("{}.{}", &chiffres[..exp as usize], &chiffres[exp as usize..])
    };
    if neg {
        s.insert(0, '-');
    }
    s
}

/// `[string]$v` d'une valeur JSON (rien : texte vide ; vrai/faux : « True »/« False »).
pub fn texte(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Number(n) => texte_nombre(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(texte).collect::<Vec<_>>().join(" "),
        Value::Object(_) => "System.Collections.Hashtable".into(),
    }
}

/// Le champ d'un objet en texte (`[string]$o.Nom`).
pub fn champ_texte(o: &Value, nom: &str) -> String {
    texte(o.get(nom).unwrap_or(&Value::Null))
}

/// `[double]$v` (rien ou illisible : 0).
pub fn nombre_de(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().replace(',', ".").parse().unwrap_or(0.0),
        Value::Bool(b) => f64::from(u8::from(*b)),
        _ => 0.0,
    }
}

pub fn champ_nombre(o: &Value, nom: &str) -> f64 {
    nombre_de(o.get(nom).unwrap_or(&Value::Null))
}

/// `[int]$o.Nom`
pub fn champ_entier(o: &Value, nom: &str) -> i64 {
    entier(champ_nombre(o, nom))
}

/// Vrai ou faux comme `if ($v)` en PowerShell (rien, 0, texte vide, faux, liste vide : faux).
pub fn vrai(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => match a.len() {
            0 => false,
            1 => vrai(&a[0]),
            _ => true,
        },
        Value::Object(_) => true,
    }
}

pub fn champ_vrai(o: &Value, nom: &str) -> bool {
    vrai(o.get(nom).unwrap_or(&Value::Null))
}

/// `@($o.Nom)` : la liste (rien : liste vide ; une seule valeur : liste d'un élément).
pub fn liste(v: &Value) -> Vec<Value> {
    match v {
        Value::Null => Vec::new(),
        Value::Array(a) => a.clone(),
        x => vec![x.clone()],
    }
}

pub fn champ_liste(o: &Value, nom: &str) -> Vec<Value> {
    liste(o.get(nom).unwrap_or(&Value::Null))
}

/// Le tri de `Sort-Object` de Windows PowerShell 5.1 : le tri rapide « à profondeur limitée » de .NET Framework
/// (`Array.Sort` avec un IComparer, PAS stable : deux éléments égaux peuvent changer d'ordre, exactement comme dans la
/// version PowerShell ; vérifié sur des listes observées dans PowerShell 5.1, voir les tests).
pub fn trier<T, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], mut cmp: F) {
    if v.len() < 2 {
        return;
    }
    rapide(v, 0, v.len() as isize - 1, 32, &mut cmp);
}

fn echange_si_plus_grand<T, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], cmp: &mut F, a: isize, b: isize) {
    if a != b && cmp(&v[a as usize], &v[b as usize]) == Ordering::Greater {
        v.swap(a as usize, b as usize);
    }
}

fn rapide<T, F: FnMut(&T, &T) -> Ordering>(
    v: &mut [T],
    mut gauche: isize,
    mut droite: isize,
    mut prof: i32,
    cmp: &mut F,
) {
    loop {
        if prof == 0 {
            tas(v, gauche, droite, cmp);
            return;
        }
        let (mut i, mut j) = (gauche, droite);
        let milieu = i + ((j - i) >> 1);
        echange_si_plus_grand(v, cmp, i, milieu);
        echange_si_plus_grand(v, cmp, i, j);
        echange_si_plus_grand(v, cmp, milieu, j);
        let x = milieu as usize;
        // (le pivot est suivi par sa place : il peut être échangé pendant le partage)
        let mut px = x;
        loop {
            while cmp(&v[i as usize], &v[px]) == Ordering::Less {
                i += 1;
            }
            while cmp(&v[px], &v[j as usize]) == Ordering::Less {
                j -= 1;
            }
            if i > j {
                break;
            }
            if i < j {
                v.swap(i as usize, j as usize);
                if px == i as usize {
                    px = j as usize;
                } else if px == j as usize {
                    px = i as usize;
                }
            }
            i += 1;
            j -= 1;
            if i > j {
                break;
            }
        }
        prof -= 1;
        if j - gauche <= droite - i {
            if gauche < j {
                rapide(v, gauche, j, prof, cmp);
            }
            gauche = i;
        } else {
            if i < droite {
                rapide(v, i, droite, prof, cmp);
            }
            droite = j;
        }
        if gauche >= droite {
            return;
        }
    }
}

fn tas<T, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], lo: isize, hi: isize, cmp: &mut F) {
    let n = hi - lo + 1;
    let mut i = n / 2;
    while i >= 1 {
        descendre(v, i, n, lo, cmp);
        i -= 1;
    }
    let mut i = n;
    while i > 1 {
        v.swap(lo as usize, (lo + i - 1) as usize);
        descendre(v, 1, i - 1, lo, cmp);
        i -= 1;
    }
}

fn descendre<T, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], mut i: isize, n: isize, lo: isize, cmp: &mut F) {
    // (comme DownHeap de .NET : l'élément de départ descend jusqu'à sa place)
    while i <= n / 2 {
        let mut enfant = 2 * i;
        if enfant < n && cmp(&v[(lo + enfant - 1) as usize], &v[(lo + enfant) as usize]) == Ordering::Less {
            enfant += 1;
        }
        if cmp(&v[(lo + i - 1) as usize], &v[(lo + enfant - 1) as usize]) != Ordering::Less {
            break;
        }
        v.swap((lo + i - 1) as usize, (lo + enfant - 1) as usize);
        i = enfant;
    }
}

/// `[version]a -lt [version]b` (« 581.57 » contre « 615.14 ») ; `None` si l'une n'est pas une version.
pub fn version_avant(a: &str, b: &str) -> Option<bool> {
    let lire = |s: &str| -> Option<Vec<i64>> {
        let p: Vec<i64> = s.split('.').map(|x| x.trim().parse::<i64>().ok()).collect::<Option<Vec<_>>>()?;
        if p.len() < 2 || p.len() > 4 || p.iter().any(|x| *x < 0) { None } else { Some(p) }
    };
    let (a, b) = (lire(a)?, lire(b)?);
    // (.NET : un numéro absent vaut -1, moins que 0)
    for i in 0..4 {
        let x = a.get(i).copied().unwrap_or(-1);
        let y = b.get(i).copied().unwrap_or(-1);
        if x != y {
            return Some(x < y);
        }
    }
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrondis_de_dotnet() {
        assert_eq!(arrondi(2.5), 2.0);
        assert_eq!(arrondi(3.5), 4.0);
        assert_eq!(arrondi(-2.5), -2.0);
        assert_eq!(entier(2.6), 3);
        assert_eq!(n_fr(1234567.891, 1), "1\u{202f}234\u{202f}567,9");
        assert_eq!(n_fr(0.15, 1), "0,2");
        assert_eq!(n_fr(2.5, 0), "3");
        assert_eq!(n_fr(0.5, 0), "1");
        assert_eq!(n_fr(1.5, 0), "2");
        assert_eq!(n_fr(2.25, 1), "2,3");
        assert_eq!(n_fr(2.35, 1), "2,4");
        assert_eq!(n_fr(-0.4, 0), "0");
        assert_eq!(n_fr(9.96, 1), "10,0");
        assert_eq!(n_fr(0.004, 1), "0,0");
        assert_eq!(n_fr(999.95, 1), "1\u{202f}000,0");
        assert_eq!(texte_nombre(1.0 / 3.0), "0.333333333333333");
        assert_eq!(texte_nombre(0.3), "0.3");
        assert_eq!(texte_nombre(1e21), "1E+21");
        assert_eq!(taille(1.5 * 1024.0 * 1024.0 * 1024.0), "1,5 Go");
    }

    #[test]
    fn le_tri_de_sort_object() {
        // Observé dans Windows PowerShell 5.1 : 1..40 trié par ($_ % 3)
        let mut v: Vec<i64> = (1..=40).collect();
        trier(&mut v, |a, b| (a % 3).cmp(&(b % 3)));
        let attendu = "12,27,9,30,18,21,15,24,6,36,3,33,39,31,37,22,28,34,25,40,10,13,7,1,4,19,16,5,17,35,20,38,2,11,14,26,29,23,32,8";
        let t = |v: &[i64]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
        assert_eq!(t(&v), attendu);
        for (n, a) in [
            (3, "3,2,1"),
            (5, "4,5,3,1,2"),
            (16, "11,12,9,10,15,16,13,14,3,4,1,2,7,8,5,6"),
            (17, "12,13,10,11,16,17,14,15,9,3,4,1,2,7,8,5,6"),
            (20, "14,15,13,11,12,19,20,18,16,17,4,5,3,1,2,9,10,8,6,7"),
        ] {
            let mut v: Vec<i64> = (1..=n).collect();
            trier(&mut v, |_, _| Ordering::Equal);
            assert_eq!(t(&v), a, "{n} éléments égaux");
        }
        let mut v: Vec<i64> = (1..=20).collect();
        trier(&mut v, |a, b| (a % 2).cmp(&(b % 2)));
        assert_eq!(t(&v), "12,20,8,18,16,14,4,2,10,6,15,19,1,17,5,9,7,3,13,11");
    }
}
