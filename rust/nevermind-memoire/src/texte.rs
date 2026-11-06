//! Les petits outils de texte de la version PowerShell, à l'identique : `Format-Size` (avec les nombres à la française
//! de .NET Framework), les arrondis de PowerShell, l'ordre des textes de `Sort-Object`, les dates.

/// Le séparateur des milliers des nombres à la française de Windows (.NET Framework, culture fr-FR) : l'espace fine
/// insécable (vérifié par les juges exportés de la version PowerShell).
pub const SEPARATEUR_MILLIERS: char = '\u{202f}';

const KO: f64 = 1024.0;
const MO: f64 = 1024.0 * 1024.0;
const GO: f64 = 1024.0 * 1024.0 * 1024.0;
const TO: f64 = 1024.0 * 1024.0 * 1024.0 * 1024.0;

/// `Format-Size` (systeme.ps1) : « 1,5 Go », « 450 Mo », « 12 Ko »
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

/// Un nombre comme `'{0:N1}' -f $x` en français avec .NET Framework : 15 chiffres significatifs d'abord, puis
/// l'arrondi au plus loin de zéro, les milliers séparés, la virgule.
pub fn nombre(x: f64, decimales: usize) -> String {
    if !x.is_finite() {
        return if x.is_nan() {
            "NaN".into()
        } else if x > 0.0 {
            "∞".into()
        } else {
            "-∞".into()
        };
    }
    let negatif = x < 0.0;
    // Les 15 chiffres significatifs (comme .NET Framework) : mantisse et exposant
    let e = format!("{:.14e}", x.abs());
    let (mantisse, exposant) = e.split_once('e').unwrap_or((&e, "0"));
    let exposant: i32 = exposant.parse().unwrap_or(0);
    let chiffres: Vec<u8> = mantisse.bytes().filter(u8::is_ascii_digit).map(|c| c - b'0').collect();
    // Chiffres avant la virgule = exposant + 1 (peut être 0 ou négatif)
    let avant = exposant + 1;
    // Le nombre en chiffres décimaux, assez long : partie entière + `decimales` chiffres, puis le chiffre d'arrondi
    let total = avant + decimales as i32;
    let mut garde: Vec<u8> = Vec::new();
    for i in 0..total.max(0) {
        garde.push(*chiffres.get(i as usize).unwrap_or(&0));
    }
    let suivant = if total < 0 { 0 } else { *chiffres.get(total as usize).unwrap_or(&0) };
    if suivant >= 5 {
        // au plus loin de zéro
        let mut i = garde.len();
        let mut retenue = true;
        while retenue && i > 0 {
            i -= 1;
            if garde[i] == 9 {
                garde[i] = 0;
            } else {
                garde[i] += 1;
                retenue = false;
            }
        }
        if retenue {
            garde.insert(0, 1);
        }
    }
    let n_entier = garde.len().saturating_sub(decimales);
    let (entier, decimal) = garde.split_at(n_entier);
    let mut entier: String = entier.iter().map(|c| (b'0' + c) as char).collect();
    let decimal: String = decimal.iter().map(|c| (b'0' + c) as char).collect();
    let decimal = format!("{decimal:0<decimales$}");
    let entier_nu = entier.trim_start_matches('0');
    entier = if entier_nu.is_empty() { "0".into() } else { entier_nu.into() };
    let mut groupe = String::new();
    let n = entier.chars().count();
    for (k, c) in entier.chars().enumerate() {
        if k > 0 && (n - k).is_multiple_of(3) {
            groupe.push(SEPARATEUR_MILLIERS);
        }
        groupe.push(c);
    }
    let zero = groupe == "0" && decimal.chars().all(|c| c == '0');
    let signe = if negatif && !zero { "-" } else { "" };
    if decimales == 0 { format!("{signe}{groupe}") } else { format!("{signe}{groupe},{decimal}") }
}

/// `[math]::Round(x)` puis `[int]` : l'arrondi au pair le plus proche (règle de .NET)
pub fn arrondi_pair(x: f64) -> i64 {
    if x.is_finite() { x.round_ties_even() as i64 } else { 0 }
}

/// L'ordre des textes de `Sort-Object` (culture française de Windows, sans tenir compte des majuscules) : les
/// traits d'union et apostrophes ne comptent pas, les signes passent avant les chiffres, puis les lettres sans accent ;
/// à égalité, la minuscule d'abord, puis l'ordre des caractères.
pub fn comparer_culture(a: &str, b: &str) -> std::cmp::Ordering {
    fn cle(s: &str) -> Vec<(u8, u32)> {
        s.chars()
            .filter(|c| *c != '-' && *c != '\'' && *c != '\u{2019}')
            .map(|c| {
                let base = sans_accent(c).to_lowercase().next().unwrap_or(c);
                if base.is_alphabetic() {
                    (2, base as u32)
                } else if base.is_ascii_digit() {
                    (1, base as u32)
                } else {
                    (0, base as u32)
                }
            })
            .collect()
    }
    cle(a).cmp(&cle(b)).then_with(|| {
        // (à égalité : la minuscule passe avant la majuscule)
        let inverse = |s: &str| -> Vec<u32> { s.chars().map(|c| if c.is_uppercase() { 1 } else { 0 }).collect() };
        inverse(a).cmp(&inverse(b)).then_with(|| a.cmp(b))
    })
}

fn sans_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
        'ç' => 'c',
        'Ç' => 'C',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'È' | 'É' | 'Ê' | 'Ë' => 'E',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
        'ñ' => 'n',
        'Ñ' => 'N',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'O',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
        'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

/// Égalité de textes sans tenir compte des majuscules (`-eq`, `-contains` de PowerShell)
pub fn egal(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

/// `-contains` de PowerShell (sans tenir compte des majuscules)
pub fn contient(liste: &[String], x: &str) -> bool {
    liste.iter().any(|e| egal(e, x))
}

/// Le dossier d'un chemin (`[IO.Path]::GetDirectoryName`), vide pour une racine ou un nom seul
pub fn dossier(chemin: &str) -> String {
    match chemin.rfind(['\\', '/']) {
        Some(i) => {
            let d = &chemin[..i];
            // (« C:\x.exe » : le dossier est « C:\ » ; « C:\ » seul n'en a pas)
            if d.len() == 2 && d.ends_with(':') {
                if i + 1 == chemin.len() { String::new() } else { format!("{d}\\") }
            } else {
                d.to_string()
            }
        }
        None => String::new(),
    }
}

/// Le nom d'un fichier dans un chemin (`[IO.Path]::GetFileName`)
pub fn nom_fichier(chemin: &str) -> &str {
    match chemin.rfind(['\\', '/']) {
        Some(i) => &chemin[i + 1..],
        None => chemin,
    }
}

/// `StartsWith(x, OrdinalIgnoreCase)`
pub fn commence(texte: &str, debut: &str) -> bool {
    texte.len() >= debut.len()
        && texte.is_char_boundary(debut.len())
        && texte[..debut.len()].to_lowercase() == debut.to_lowercase()
}

/// Une date et une heure (heure locale du PC), comme `[datetime]` de PowerShell
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Date {
    pub annee: u32,
    pub mois: u32,
    pub jour: u32,
    pub heure: u32,
    pub minute: u32,
    pub seconde: u32,
}

impl Date {
    /// « 2026-10-24T14:30:00 » (le format `s` de PowerShell)
    pub fn lire(t: &str) -> Option<Date> {
        let t = t.trim();
        let (j, h) = t.split_once(['T', ' ']).unwrap_or((t, "00:00:00"));
        let mut d = j.split('-').map(|x| x.parse::<u32>().ok());
        let mut hh = h.split(':').map(|x| x.split('.').next().unwrap_or("0").parse::<u32>().ok());
        Some(Date {
            annee: d.next()??,
            mois: d.next()??,
            jour: d.next()??,
            heure: hh.next().flatten().unwrap_or(0),
            minute: hh.next().flatten().unwrap_or(0),
            seconde: hh.next().flatten().unwrap_or(0),
        })
    }
    /// Le format `s` : « 2026-10-24T14:30:00 »
    pub fn texte(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.annee, self.mois, self.jour, self.heure, self.minute, self.seconde
        )
    }
    /// `ToString('dd/MM/yyyy à HH:mm')`
    pub fn jour_et_heure(&self) -> String {
        format!("{:02}/{:02}/{:04} à {:02}:{:02}", self.jour, self.mois, self.annee, self.heure, self.minute)
    }
    /// `ToString('dd-MM-yyyy')`
    pub fn jour_tirets(&self) -> String {
        format!("{:02}-{:02}-{:04}", self.jour, self.mois, self.annee)
    }
    /// `ToString('HH\hmm')`
    pub fn heure_h(&self) -> String {
        format!("{:02}h{:02}", self.heure, self.minute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailles() {
        assert_eq!(taille(0.0), "0 Ko");
        assert_eq!(taille(512.0), "1 Ko");
        assert_eq!(taille(450.0 * MO), "450 Mo");
        assert_eq!(taille(1.3 * GO), "1,3 Go");
        assert_eq!(taille(1.25 * GO), "1,3 Go");
        assert_eq!(taille(1000.0 * MO), format!("1{SEPARATEUR_MILLIERS}000 Mo"));
        assert_eq!(taille(15.7 * GO), "15,7 Go");
    }

    #[test]
    fn arrondis() {
        assert_eq!(arrondi_pair(2.5), 2);
        assert_eq!(arrondi_pair(3.5), 4);
        assert_eq!(arrondi_pair(74.52), 75);
    }

    #[test]
    fn dossiers() {
        assert_eq!(dossier(r"C:\Program Files\X\x.exe"), r"C:\Program Files\X");
        assert_eq!(dossier(r"C:\x.exe"), r"C:\");
        assert_eq!(dossier("x.exe"), "");
    }
}
