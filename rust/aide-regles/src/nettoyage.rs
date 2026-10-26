//! Nettoyage : les dossiers de Windows que seule l'Aide peut vider (`$AideNettoyage`, aide-actions.ps1). Jamais un
//! chemin envoyé par Nevermind. Seulement des dossiers où un utilisateur sans droits ne peut pas écrire : l'Aide efface
//! par le chemin, et un dossier remplacé par un lien entre la liste et l'effacement lui ferait effacer ailleurs.
//! (C:\Windows\Temp et les rapports d'erreurs n'en font donc plus partie : Windows les vide lui-même.)

use crate::texte;

/// Une catégorie du nettoyage
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Categorie {
    /// Téléchargements de Windows Update
    Wu,
    /// Cache de l'optimisation de la distribution
    Do,
    /// Vidages mémoire après un écran bleu
    Dump,
}

/// La catégorie de ce nom (sans tenir compte des majuscules, comme la table de PowerShell)
pub fn categorie(id: &str) -> Option<Categorie> {
    [("wu", Categorie::Wu), ("do", Categorie::Do), ("dump", Categorie::Dump)]
        .into_iter()
        .find(|(n, _)| texte::egal(id, n))
        .map(|(_, c)| c)
}

/// Les dossiers d'une catégorie, dans le dossier de Windows donné par Windows
pub fn dossiers(c: Categorie, windows: &str) -> Vec<String> {
    let w = windows.trim_end_matches('\\');
    let d = match c {
        Categorie::Wu => "SoftwareDistribution\\Download",
        Categorie::Do => {
            "ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache"
        }
        Categorie::Dump => "Minidump",
    };
    vec![format!("{w}\\{d}")]
}

/// Un nombre écrit à la française, avec `decimales` chiffres après la virgule (`{0:N1}` de .NET en français)
fn nombre_fr(x: f64, decimales: usize) -> String {
    let s = format!("{x:.decimales$}");
    let (entier, dec) = match s.split_once('.') {
        Some((e, d)) => (e.to_string(), Some(d.to_string())),
        None => (s, None),
    };
    let mut groupes = String::new();
    for (i, c) in entier.chars().enumerate() {
        if i > 0 && (entier.len() - i) % 3 == 0 {
            groupes.push('\u{a0}');
        }
        groupes.push(c);
    }
    match dec {
        Some(d) => format!("{groupes},{d}"),
        None => groupes,
    }
}

/// Une taille lisible (« 1,5 Go », « 12,0 Mo », « 3 Ko »), comme le journal du nettoyage de PowerShell
pub fn taille(octets: f64) -> String {
    const KO: f64 = 1024.0;
    if octets >= KO * KO * KO {
        format!("{} Go", nombre_fr(octets / (KO * KO * KO), 1))
    } else if octets >= KO * KO {
        format!("{} Mo", nombre_fr(octets / (KO * KO), 1))
    } else {
        format!("{} Ko", nombre_fr((octets / KO).ceil().max(1.0), 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_et_tailles() {
        assert_eq!(categorie("WU"), Some(Categorie::Wu));
        assert_eq!(categorie("wintemp"), None);
        assert_eq!(categorie("C:\\Users"), None);
        assert_eq!(dossiers(Categorie::Dump, "C:\\Windows"), vec!["C:\\Windows\\Minidump".to_string()]);
        assert_eq!(taille(1536.0 * 1024.0 * 1024.0), "1,5 Go");
        assert_eq!(taille(100.0), "1 Ko");
        assert_eq!(taille(5000.0 * 1024.0), "4,9 Mo");
        assert_eq!(taille(1000.0 * 1024.0), "1\u{a0}000 Ko");
    }
}
