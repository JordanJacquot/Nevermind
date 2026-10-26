//! Petits outils sur les textes, pour comparer comme .NET et PowerShell.
//!
//! PowerShell compare sans tenir compte des majuscules (`-eq`, `-match`, `-in`...) ; .NET le fait lettre par lettre
//! (`OrdinalIgnoreCase` : majuscule simple de chaque lettre). Une lettre dont la majuscule (ou la minuscule) s'écrit en
//! plusieurs lettres reste telle quelle, comme dans .NET (« İ » ne devient pas « i̇ »).

/// Majuscule simple d'une lettre (elle-même si sa majuscule fait plusieurs lettres)
pub fn majuscule(c: char) -> char {
    let mut m = c.to_uppercase();
    match (m.next(), m.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

/// Minuscule simple d'une lettre (elle-même si sa minuscule fait plusieurs lettres)
pub fn minuscule(c: char) -> char {
    let mut m = c.to_lowercase();
    match (m.next(), m.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

/// Le texte en minuscules, comme `ToLowerInvariant` de .NET (aucune normalisation des accents)
pub fn minuscules(s: &str) -> String {
    s.chars().map(minuscule).collect()
}

/// Égaux sans tenir compte des majuscules (`OrdinalIgnoreCase`, `-eq` de PowerShell)
pub fn egal(a: &str, b: &str) -> bool {
    a.chars().map(majuscule).eq(b.chars().map(majuscule))
}

/// `s` commence par `debut`, sans tenir compte des majuscules
pub fn commence(s: &str, debut: &str) -> bool {
    reste_apres(s, debut).is_some()
}

/// Ce qui suit `debut` dans `s` (sans tenir compte des majuscules), ou `None` si `s` ne commence pas ainsi
pub fn reste_apres<'a>(s: &'a str, debut: &str) -> Option<&'a str> {
    let mut lettres = s.char_indices();
    for d in debut.chars() {
        match lettres.next() {
            Some((_, c)) if majuscule(c) == majuscule(d) => {}
            _ => return None,
        }
    }
    Some(match lettres.next() {
        Some((i, _)) => &s[i..],
        None => "",
    })
}

/// `s` finit par `fin`, sans tenir compte des majuscules
pub fn finit(s: &str, fin: &str) -> bool {
    let mut a = s.chars().rev();
    fin.chars().rev().all(|f| a.next().is_some_and(|c| majuscule(c) == majuscule(f)))
}

/// Position (en octets) de la première occurrence de `morceau` dans `s`, sans tenir compte des majuscules
pub fn position(s: &str, morceau: &str) -> Option<usize> {
    s.char_indices().map(|(i, _)| i).find(|&i| commence(&s[i..], morceau))
}

/// `s` contient `morceau`, sans tenir compte des majuscules
pub fn contient(s: &str, morceau: &str) -> bool {
    position(s, morceau).is_some()
}

/// Un caractère de contrôle (`[\x00-\x1f]`)
pub fn a_controle(s: &str) -> bool {
    s.chars().any(|c| (c as u32) < 0x20)
}

/// Longueur comme la compte .NET (en unités UTF-16)
pub fn longueur(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Sans les espaces du début et de la fin (`Trim()` de .NET)
pub fn sans_espaces(s: &str) -> &str {
    s.trim_matches(char::is_whitespace)
}

/// Une lettre de mot (`\w` des expressions de .NET : lettres, chiffres et « _ »)
pub fn lettre_de_mot(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Le nom du fichier d'un chemin (`[IO.Path]::GetFileName`) : ce qui suit le dernier « \ », « / » ou « : »
pub fn nom_fichier(chemin: &str) -> &str {
    match chemin.rfind(['\\', '/', ':']) {
        Some(i) => &chemin[i + 1..],
        None => chemin,
    }
}

/// Le nom du fichier sans son extension (`[IO.Path]::GetFileNameWithoutExtension`)
pub fn nom_sans_extension(chemin: &str) -> &str {
    let n = nom_fichier(chemin);
    match n.rfind('.') {
        Some(i) => &n[..i],
        None => n,
    }
}

/// Le dossier d'un chemin (`[IO.Path]::GetDirectoryName`), sans le « \ » final ; `None` pour une racine
pub fn dossier_de(chemin: &str) -> Option<&str> {
    let i = chemin.rfind(['\\', '/'])?;
    let d = &chemin[..i];
    // (« C:\x.exe » : le dossier est « C:\ », racine comprise)
    if d.len() == 2 && d.ends_with(':') {
        return Some(&chemin[..=i]);
    }
    if d.is_empty() { None } else { Some(d) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparaisons() {
        assert!(egal("C:\\WINDOWS", "c:\\windows"));
        assert!(!egal("C:\\WINDOWS", "c:\\windows\\"));
        assert!(commence("HKLM:\\Software\\x", "HKLM:\\SOFTWARE\\"));
        assert_eq!(reste_apres("Nevermind : bloque Truc", "nevermind : BLOQUE "), Some("Truc"));
        assert!(finit("truc.EXE", ".exe"));
        assert!(!finit("exe", ".exe"));
        assert_eq!(position("abc.ExE x", ".exe"), Some(3));
        assert_eq!(minuscules("ÉTÉ İstanbul"), "été İstanbul");
        assert_eq!(nom_fichier("C:\\a\\b.exe"), "b.exe");
        assert_eq!(nom_sans_extension("SecurityHealthSystray.lnk"), "SecurityHealthSystray");
        assert_eq!(dossier_de("C:\\a\\b.exe"), Some("C:\\a"));
        assert_eq!(dossier_de("C:\\b.exe"), Some("C:\\"));
        assert_eq!(longueur("é"), 1);
    }
}
