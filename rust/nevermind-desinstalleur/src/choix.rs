//! Le choix des données, venu de la ligne de commande (`Get-ChoixDonneesImpose`, systeme.ps1, et la lecture des
//! options du lanceur C#, outils\lanceur.cs:156-166).
//!
//! « Désinstaller Nevermind.exe -GarderDonnees » ou « -SupprimerDonnees » (le désinstalleur de Mindset les passe) : le
//! choix est fait, aucune question. Sans option, ou avec les deux à la fois : la question est posée. Comme le lanceur,
//! seule la ligne de commande compte (une variable NEVERMIND_DONNEES héritée d'ailleurs n'est jamais lue).

/// Ce que l'utilisateur a choisi pour ses données
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choix {
    /// Les données restent (préférences, jeux, mesures, journal) : une réinstallation les retrouve
    Garder,
    /// Tout le dossier des données part
    Supprimer,
}

impl Choix {
    /// Le mot de la version PowerShell (« garder », « supprimer »)
    pub fn mot(self) -> &'static str {
        match self {
            Choix::Garder => "garder",
            Choix::Supprimer => "supprimer",
        }
    }

    /// L'option de la ligne de commande qui donne ce choix
    pub fn option(self) -> &'static str {
        match self {
            Choix::Garder => "-GarderDonnees",
            Choix::Supprimer => "-SupprimerDonnees",
        }
    }
}

/// `a.Trim('-', '/')` de .NET : les « - » et « / » retirés des deux bouts
fn nom_option(a: &str) -> &str {
    a.trim_matches(|c| c == '-' || c == '/')
}

/// Le choix imposé par les arguments (lanceur.cs:156-166) : `None` s'il n'y en a pas, ou s'ils se contredisent (la
/// question est alors posée). Les noms ne tiennent pas compte des majuscules ; un argument inconnu est ignoré.
pub fn choix_impose<I, S>(arguments: I) -> Option<Choix>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    // (null au départ, puis « garder », « supprimer », ou « » quand les deux sont donnés)
    let mut donnees: Option<Option<Choix>> = None;
    for a in arguments {
        let nom = nom_option(a.as_ref());
        for c in [Choix::Garder, Choix::Supprimer] {
            let attendu = if c == Choix::Garder { "GarderDonnees" } else { "SupprimerDonnees" };
            if nom.eq_ignore_ascii_case(attendu) {
                // (`donnees = donnees == null || donnees == "garder" ? "garder" : ""`, et de même pour « supprimer »)
                donnees = Some(match donnees {
                    None => Some(c),
                    Some(Some(x)) if x == c => Some(c),
                    _ => None,
                });
            }
        }
    }
    donnees.flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(a: &[&str]) -> Option<Choix> {
        choix_impose(a.iter().copied())
    }

    #[test]
    fn comme_le_lanceur() {
        assert_eq!(c(&[]), None);
        assert_eq!(c(&["-GarderDonnees"]), Some(Choix::Garder));
        assert_eq!(c(&["-SupprimerDonnees"]), Some(Choix::Supprimer));
        assert_eq!(c(&["/supprimerdonnees"]), Some(Choix::Supprimer));
        assert_eq!(c(&["--GARDERDONNEES--"]), Some(Choix::Garder));
        assert_eq!(c(&["-GarderDonnees", "-GarderDonnees"]), Some(Choix::Garder));
        // Les deux à la fois : la question est posée, dans un ordre comme dans l'autre, même répétés ensuite
        assert_eq!(c(&["-GarderDonnees", "-SupprimerDonnees"]), None);
        assert_eq!(c(&["-SupprimerDonnees", "-GarderDonnees"]), None);
        assert_eq!(c(&["-SupprimerDonnees", "-GarderDonnees", "-GarderDonnees"]), None);
        // Inconnu, ou mal écrit : ignoré
        assert_eq!(c(&["-Garder", "Données", "-Uninstall"]), None);
        assert_eq!(c(&["-Uninstall", "-SupprimerDonnees"]), Some(Choix::Supprimer));
    }
}
