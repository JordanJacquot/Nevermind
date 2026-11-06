//! Les petits outils de texte de la version PowerShell, à l'identique : texte de recherche (ConvertTo-SearchText),
//! nombres à la française ('{0:N0}' -f), arrondis de PowerShell ([int]), durées (Format-PlayTime), dates
//! (Format-LastPlayed) et le tri des noms (Sort-Object, qui compare comme la langue de Windows).

use std::cmp::Ordering;
use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::UnicodeNormalization;

/// Le séparateur des milliers de Windows en français (« 1 234 ») : espace fine insécable, comme .NET sur Windows 11.
pub const SEP_MILLIERS: char = '\u{202F}';

/// Minuscules, sans accents ni ponctuation (ConvertTo-SearchText, recherche.ps1).
pub fn texte_recherche(texte: &str) -> String {
    let bas: String = texte.chars().map(minuscule).collect();
    let mut sortie = String::with_capacity(bas.len());
    for c in bas.nfd() {
        if get_general_category(c) == GeneralCategory::NonspacingMark {
            continue;
        }
        sortie.push(if lettre_ou_chiffre(c) { c } else { ' ' });
    }
    sortie.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `ToLowerInvariant` de .NET : une lettre donne une lettre (correspondance simple).
pub fn minuscule(c: char) -> char {
    // (le I à point de la langue turque reste tel quel, comme ToLowerInvariant de .NET Framework)
    if c == '\u{130}' {
        return c;
    }
    c.to_lowercase().next().unwrap_or(c)
}

/// `char.IsLetterOrDigit` de .NET : lettres (Lu, Ll, Lt, Lm, Lo) et chiffres décimaux (Nd).
pub fn lettre_ou_chiffre(c: char) -> bool {
    use GeneralCategory::*;
    matches!(
        get_general_category(c),
        UppercaseLetter | LowercaseLetter | TitlecaseLetter | ModifierLetter | OtherLetter | DecimalNumber
    )
}

/// `[int]$x` de PowerShell : arrondi au pair le plus proche (2,5 donne 2 ; 3,5 donne 4).
pub fn entier_ps(x: f64) -> i64 {
    if x.is_nan() {
        return 0;
    }
    x.round_ties_even() as i64
}

/// Arrondi de .NET Framework pour l'affichage ('{0:N0}') : la valeur ramenée à 15 chiffres, puis la moitié vers le haut.
fn arrondi_affichage(x: f64, decimales: i32) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let p = 10f64.powi(decimales);
    // (15 chiffres significatifs, comme le format « R » de .NET Framework avant l'arrondi)
    let ordre = x.abs().log10().floor() as i32;
    let f = 10f64.powi(14 - ordre);
    let x15 = (x * f).round() / f;
    let v = (x15.abs() * p + 0.5 + 1e-9).floor() / p;
    if x < 0.0 { -v } else { v }
}

fn grouper(entier: u64) -> String {
    let s = entier.to_string();
    let mut sortie = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            sortie.push(SEP_MILLIERS);
        }
        sortie.push(c);
    }
    sortie
}

/// '{0:N0}' -f $x en français : « 1 234 », « -5 ».
pub fn n0(x: f64) -> String {
    nd(x, 0)
}

/// '{0:N1}' -f $x en français : « 1,5 ».
pub fn n1(x: f64) -> String {
    nd(x, 1)
}

/// '{0:Nd}' -f $x en français (virgule décimale, milliers séparés).
pub fn nd(x: f64, decimales: i32) -> String {
    let v = arrondi_affichage(x, decimales);
    let neg = v < 0.0;
    let a = v.abs();
    let p = 10u64.pow(decimales as u32);
    let total = (a * p as f64).round() as u64;
    let (ent, frac) = (total / p, total % p);
    let mut s = String::new();
    if neg && total > 0 {
        s.push('-');
    }
    s.push_str(&grouper(ent));
    if decimales > 0 {
        s.push(',');
        s.push_str(&format!("{:0width$}", frac, width = decimales as usize));
    }
    s
}

/// Une durée de jeu (Format-PlayTime, jeu.ps1). Reprend ses arrondis à la PowerShell.
pub fn duree_jeu(secondes: f64) -> String {
    if secondes < 90.0 {
        return format!("{} s", entier_ps(secondes));
    }
    let m = entier_ps(secondes / 60.0);
    if m < 60 {
        return format!("{m} min");
    }
    format!("{} h {:02}", entier_ps(m as f64 / 60.0), m.rem_euclid(60))
}

/// Un moment sans fuseau (comme un [datetime] « Unspecified » de PowerShell), en secondes depuis le 1er janvier 1970.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Moment(pub i64);

impl Moment {
    pub fn depuis(a: i64, mo: u32, j: u32, h: u32, mi: u32, s: u32) -> Self {
        Moment(jours_depuis_civil(a, mo, j) * 86400 + h as i64 * 3600 + mi as i64 * 60 + s as i64)
    }
    pub fn jour(&self) -> i64 {
        self.0.div_euclid(86400)
    }
    pub fn parties(&self) -> (i64, u32, u32, u32, u32, u32) {
        let (a, m, j) = civil(self.jour());
        let r = self.0.rem_euclid(86400);
        (a, m, j, (r / 3600) as u32, ((r % 3600) / 60) as u32, (r % 60) as u32)
    }
    /// « dd/MM »
    pub fn jj_mm(&self) -> String {
        let (_, m, j, ..) = self.parties();
        format!("{j:02}/{m:02}")
    }
    /// « HH:mm »
    pub fn hh_mm(&self) -> String {
        let (_, _, _, h, mi, _) = self.parties();
        format!("{h:02}:{mi:02}")
    }
    /// « dd/MM/yyyy »
    pub fn jj_mm_aaaa(&self) -> String {
        let (a, m, j, ..) = self.parties();
        format!("{j:02}/{m:02}/{a:04}")
    }
    /// Le format « s » de .NET : « 2026-11-06T15:04:05 »
    pub fn iso(&self) -> String {
        let (a, m, j, h, mi, s) = self.parties();
        format!("{a:04}-{m:02}-{j:02}T{h:02}:{mi:02}:{s:02}")
    }
}

/// Lit une date comme `[datetime]$texte` de PowerShell pour les formats que Nevermind écrit : « 2026-11-06T15:04:05 »
/// (avec ou sans fraction, avec ou sans fuseau : le fuseau est ignoré, comme une date déjà locale), « 2026-11-06 15:04 »,
/// « 2026-11-06 ». `None` si ce n'est pas une date.
pub fn lire_date(texte: &str) -> Option<Moment> {
    let t = texte.trim();
    let b = t.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |s: &str| s.parse::<u32>().ok();
    let a: i64 = t.get(0..4)?.parse().ok()?;
    let mo = num(t.get(5..7)?)?;
    let j = num(t.get(8..10)?)?;
    if !(1..=12).contains(&mo) || j == 0 || j > jours_du_mois(a, mo) {
        return None;
    }
    let (mut h, mut mi, mut s) = (0, 0, 0);
    if b.len() > 10 {
        if b[10] != b'T' && b[10] != b' ' {
            return None;
        }
        let reste = &t[11..];
        let heure: String = reste.chars().take_while(|c| c.is_ascii_digit() || *c == ':').collect();
        let morceaux: Vec<&str> = heure.split(':').collect();
        if morceaux.len() < 2 {
            return None;
        }
        h = num(morceaux[0])?;
        mi = num(morceaux[1])?;
        if morceaux.len() > 2 {
            s = num(morceaux[2])?;
        }
        if h > 23 || mi > 59 || s > 59 {
            return None;
        }
    }
    Some(Moment::depuis(a, mo, j, h, mi, s))
}

fn bissextile(a: i64) -> bool {
    (a % 4 == 0 && a % 100 != 0) || a % 400 == 0
}

fn jours_du_mois(a: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if bissextile(a) {
                29
            } else {
                28
            }
        }
    }
}

pub fn jours_depuis_civil(a: i64, m: u32, j: u32) -> i64 {
    let a = if m <= 2 { a - 1 } else { a };
    let era = a.div_euclid(400);
    let yoe = a.rem_euclid(400);
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + j as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Quand un jeu a été joué pour la dernière fois (Format-LastPlayed, bibliotheque.ps1).
pub fn derniere_partie(iso: &str, maintenant: Moment) -> String {
    if iso.is_empty() {
        return "jamais lancé avec Nevermind ouvert".into();
    }
    // ([datetime] d'un texte qui n'est pas une date : PowerShell s'arrête, la page n'est pas faite ; ici, « jamais »)
    let Some(d) = lire_date(iso) else { return "jamais lancé avec Nevermind ouvert".into() };
    let jours = maintenant.jour() - d.jour();
    if jours <= 0 {
        format!("aujourd'hui à {}", d.hh_mm())
    } else if jours == 1 {
        "hier".into()
    } else if jours < 7 {
        format!("il y a {jours} jours")
    } else {
        format!("le {}", d.jj_mm_aaaa())
    }
}

/// La clé de tri d'un texte, proche de la comparaison de Windows en français : sans accents, sans casse, les tirets et
/// apostrophes ignorés.
fn cle_tri(s: &str) -> Vec<(u8, char)> {
    let mut v = Vec::new();
    for c in s.chars().map(minuscule).collect::<String>().nfd() {
        if get_general_category(c) == GeneralCategory::NonspacingMark || c == '-' || c == '\'' || c == '\u{AD}' {
            continue;
        }
        // (ordre de Windows : espaces et signes, puis chiffres, puis lettres)
        let rang = if c.is_whitespace() {
            0
        } else if lettre_ou_chiffre(c) {
            if c.is_ascii_digit() { 2 } else { 3 }
        } else {
            1
        };
        v.push((rang, c));
    }
    v
}

/// Compare deux textes comme Sort-Object (langue de Windows, sans tenir compte de la casse) : d'abord sans accents,
/// puis avec les accents, puis les tirets et apostrophes (ignorés au premier passage).
pub fn comparer_culture(a: &str, b: &str) -> Ordering {
    cle_tri(a).cmp(&cle_tri(b)).then_with(|| {
        let accents = |s: &str| -> Vec<char> {
            s.chars().map(minuscule).filter(|c| !matches!(c, '-' | '\'' | '\u{AD}')).collect::<String>().nfd().collect()
        };
        accents(a).cmp(&accents(b)).then_with(|| {
            let n = |s: &str| s.chars().filter(|c| matches!(c, '-' | '\'' | '\u{AD}')).count();
            n(a).cmp(&n(b))
        })
    })
}

/// Le tri de Sort-Object (l'ancien tri rapide de .NET Framework, pas stable). Reproduit le même ordre pour les
/// éléments égaux.
pub fn trier_ps<T: Clone, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], mut cmp: F) {
    let n = v.len();
    if n < 2 {
        return;
    }
    rapide(v, 0, n as isize - 1, 32, &mut cmp);
}

fn plus_grand<T, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], a: isize, b: isize, cmp: &mut F) {
    if a != b && cmp(&v[a as usize], &v[b as usize]) == Ordering::Greater {
        v.swap(a as usize, b as usize);
    }
}

fn tas<T, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], lo: isize, hi: isize, cmp: &mut F) {
    let n = hi - lo + 1;
    let mut i = n >> 1;
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
    while i <= n >> 1 {
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

/// « Rocket League® » sans les signes ®, ™ et © (comme `-replace '[®™©]', ''`).
pub fn sans_marques(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '®' | '™' | '©')).collect()
}

/// Le nom d'un fichier sans son extension (`[IO.Path]::GetFileNameWithoutExtension`).
pub fn nom_sans_extension(chemin: &str) -> String {
    let nom = nom_fichier(chemin);
    match nom.rfind('.') {
        Some(i) => nom[..i].to_string(),
        None => nom.to_string(),
    }
}

/// Le nom d'un fichier (`[IO.Path]::GetFileName`).
pub fn nom_fichier(chemin: &str) -> &str {
    match chemin.rfind(['\\', '/']) {
        Some(i) => &chemin[i + 1..],
        None => chemin,
    }
}

/// Le dossier d'un fichier (`Split-Path -Parent`).
pub fn dossier_parent(chemin: &str) -> String {
    let c = chemin.trim_end_matches(['\\', '/']);
    match c.rfind(['\\', '/']) {
        Some(0) => c[..1].to_string(),
        Some(i) => {
            let p = &c[..i];
            // « C: » devient « C:\ »
            if p.len() == 2 && p.ends_with(':') { format!("{p}\\") } else { p.to_string() }
        }
        None => String::new(),
    }
}

/// `Join-Path` simple.
pub fn joindre(a: &str, b: &str) -> String {
    if a.is_empty() {
        return b.to_string();
    }
    // (Join-Path remet les barres obliques en barres de Windows)
    let a = a.replace('/', "\\");
    let b = b.replace('/', "\\");
    format!("{}\\{}", a.trim_end_matches('\\'), b.trim_start_matches('\\'))
}

/// L'ancien tri rapide de .NET (DepthLimitedQuickSort), celui de Windows PowerShell 5.1.
fn rapide<T: Clone, F: FnMut(&T, &T) -> Ordering>(
    v: &mut [T],
    mut gauche: isize,
    mut droite: isize,
    mut prof: i64,
    cmp: &mut F,
) {
    loop {
        if prof == 0 {
            tas(v, gauche, droite, cmp);
            return;
        }
        let (mut i, mut j) = (gauche, droite);
        let milieu = i + ((j - i) >> 1);
        plus_grand(v, i, milieu, cmp);
        plus_grand(v, i, j, cmp);
        plus_grand(v, milieu, j, cmp);
        let x = v[milieu as usize].clone();
        loop {
            while cmp(&v[i as usize], &x) == Ordering::Less {
                i += 1;
            }
            while cmp(&x, &v[j as usize]) == Ordering::Less {
                j -= 1;
            }
            if i > j {
                break;
            }
            if i < j {
                v.swap(i as usize, j as usize);
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

/// Get-ChildItem rend la lettre du disque en majuscule (« d:\steam » donne « D:\steam »).
pub fn lettre_majuscule(p: &str) -> String {
    if p.as_bytes().get(1) == Some(&b':') { format!("{}{}", p[..1].to_uppercase(), &p[1..]) } else { p.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recherche() {
        assert_eq!(texte_recherche("  Éléphant-Rose  2 "), "elephant rose 2");
        assert_eq!(texte_recherche("Rocket League®"), "rocket league");
        assert_eq!(texte_recherche("Œuvre l'été"), "œuvre l ete");
    }

    #[test]
    fn nombres() {
        assert_eq!(n0(1234.5), "1\u{202F}235");
        assert_eq!(n0(2.5), "3");
        assert_eq!(n0(-0.4), "0");
        assert_eq!(n1(1.25), "1,3");
        assert_eq!(entier_ps(2.5), 2);
        assert_eq!(entier_ps(3.5), 4);
    }

    #[test]
    fn durees() {
        assert_eq!(duree_jeu(45.0), "45 s");
        assert_eq!(duree_jeu(89.5), "90 s");
        assert_eq!(duree_jeu(600.0), "10 min");
        assert_eq!(duree_jeu(5400.0), "2 h 30");
        assert_eq!(duree_jeu(3700.0), "1 h 02");
    }

    #[test]
    fn dates() {
        let n = Moment::depuis(2026, 11, 6, 15, 0, 0);
        assert_eq!(derniere_partie("2026-11-06T09:05:00", n), "aujourd'hui à 09:05");
        assert_eq!(derniere_partie("2026-11-05T23:59:59", n), "hier");
        assert_eq!(derniere_partie("2026-11-01T10:00:00", n), "il y a 5 jours");
        assert_eq!(derniere_partie("2026-10-01T10:00:00", n), "le 01/10/2026");
        assert_eq!(derniere_partie("", n), "jamais lancé avec Nevermind ouvert");
        assert_eq!(lire_date("2026-11-06T15:04:05.123+01:00").unwrap().iso(), "2026-11-06T15:04:05");
    }
}
