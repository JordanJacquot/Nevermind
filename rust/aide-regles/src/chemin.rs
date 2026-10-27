//! Chemin complet d'un fichier, comme `[IO.Path]::GetFullPath` le donne pour un chemin « X:\... ».
//!
//! Seule la forme « lettre, deux-points, \ » est prise. Les autres (réseau `\\serveur\...`, `\\?\...`, « C:truc »,
//! « \truc ») donnent `None` : les règles de l'Aide les refusent (PowerShell les refusait déjà presque toutes, par
//! un chemin complet différent du chemin reçu ; les chemins réseau sont refusés en plus).

/// Le chemin complet de `p` : « / » devient « \ », les « \ » répétés, « . » et « .. » sont résolus, et la fin du
/// dernier morceau perd ses points et ses espaces (règle de Windows). Un « \ » final reste.
/// `None` si `p` n'est pas un chemin « X:\... » ou contient un « : » plus loin (flux de fichier, refusé par .NET).
pub fn complet(p: &str) -> Option<String> {
    let b = p.as_bytes();
    if b.len() < 3 || !b[0].is_ascii_alphabetic() || b[1] != b':' || !(b[2] == b'\\' || b[2] == b'/') {
        return None;
    }
    let reste = &p[3..];
    if reste.contains(':') || p.contains('\0') {
        return None;
    }
    let mut morceaux: Vec<&str> = Vec::new();
    for m in reste.split(['\\', '/']) {
        match m {
            "" | "." => {}
            ".." => {
                morceaux.pop();
            }
            _ => morceaux.push(m),
        }
    }
    let finit_par_separateur = reste.ends_with(['\\', '/']);
    let mut dernier = String::new();
    if !finit_par_separateur && let Some(d) = morceaux.pop() {
        dernier = d.trim_end_matches(['.', ' ']).to_string();
    }
    let mut r = String::with_capacity(p.len());
    r.push_str(&p[..2]);
    r.push('\\');
    r.push_str(&morceaux.join("\\"));
    if !dernier.is_empty() {
        if !morceaux.is_empty() {
            r.push('\\');
        }
        r.push_str(&dernier);
    } else if finit_par_separateur && !morceaux.is_empty() {
        r.push('\\');
    }
    Some(r)
}

/// Un chemin « enraciné » au sens de .NET (`IsPathRooted`) : il commence par « \ », « / » ou « X: »
pub fn enracine(p: &str) -> bool {
    let b = p.as_bytes();
    b.first().is_some_and(|&c| c == b'\\' || c == b'/') || (b.len() >= 2 && b[1] == b':')
}

/// `Join-Path dossier nom` : un seul « \ » entre les deux
pub fn joindre(dossier: &str, nom: &str) -> String {
    if dossier.ends_with(['\\', '/']) { format!("{dossier}{nom}") } else { format!("{dossier}\\{nom}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comme_getfullpath() {
        let c = |p: &str| complet(p);
        assert_eq!(c("C:\\a\\b"), Some("C:\\a\\b".into()));
        assert_eq!(c("C:\\a\\b\\"), Some("C:\\a\\b\\".into()));
        assert_eq!(c("C:\\a\\x\\..\\b"), Some("C:\\a\\b".into()));
        assert_eq!(c("C:\\a\\\\b"), Some("C:\\a\\b".into()));
        assert_eq!(c("C:/a/b"), Some("C:\\a\\b".into()));
        assert_eq!(c("C:\\a\\b.exe "), Some("C:\\a\\b.exe".into()));
        assert_eq!(c("C:\\..\\..\\x"), Some("C:\\x".into()));
        assert_eq!(c("C:\\"), Some("C:\\".into()));
        assert_eq!(c("c:\\Windows\\..\\Jeux"), Some("c:\\Jeux".into()));
        assert_eq!(c("C:x.exe"), None);
        assert_eq!(c("\\\\serveur\\x.exe"), None);
        assert_eq!(c("C:\\a.exe:flux"), None);
        assert!(enracine("C:x") && enracine("\\x") && !enracine("x.exe"));
    }
}
