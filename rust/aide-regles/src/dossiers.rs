//! Les dossiers de Windows, tels que Windows les donne : jamais lus dans les variables d'environnement (l'Aide lancée
//! par la demande de Windows reçoit celles de l'utilisateur, qu'un programme sans droits peut changer).

use crate::{chemin, texte};

/// Les dossiers de Windows dont les règles se servent
#[derive(Clone, Debug, Default)]
pub struct Dossiers {
    /// C:\Windows
    pub windows: String,
    /// C:\Program Files
    pub programmes: String,
    /// C:\Program Files (x86)
    pub programmes_x86: String,
    /// C:\ProgramData
    pub donnees_communes: String,
}

impl Dossiers {
    /// La valeur d'une variable de Windows citée dans une commande (`%ProgramFiles%`...), prise dans ces dossiers
    fn variable(&self, nom: &str) -> Option<String> {
        let v = match texte::minuscules(nom).as_str() {
            "windir" | "systemroot" => self.windows.clone(),
            "programfiles" | "programw6432" => self.programmes.clone(),
            "programfiles(x86)" => self.programmes_x86.clone(),
            "programdata" | "allusersprofile" => self.donnees_communes.clone(),
            "commonprogramfiles" => format!("{}\\Common Files", self.programmes),
            "commonprogramfiles(x86)" => format!("{}\\Common Files", self.programmes_x86),
            _ => return None,
        };
        // (dossier inconnu sur ce PC : on ne devine pas)
        if v.is_empty() || v.starts_with('\\') { None } else { Some(v) }
    }

    /// Un chemin écrit par Windows (« %ProgramFiles%\...\x.exe », « "C:\...\x.exe" -k ... ») : le programme seul, en
    /// chemin complet (`ConvertFrom-AideCheminWindows`). `None` si ce n'est pas un programme « X:\...\x.exe » ou si
    /// une variable est inconnue.
    pub fn programme_de_commande(&self, commande: &str) -> Option<String> {
        let mut t = texte::sans_espaces(commande).to_string();
        if let Some(r) = t.strip_prefix('"') {
            t = match r.find('"') {
                Some(i) => r[..i].to_string(),
                None => r.to_string(),
            };
        } else if let Some(i) = texte::position(&t, ".exe") {
            t.truncate(i + 4);
        }
        // Les variables %...% (de 1 à 40 lettres), trouvées dans le texte de départ puis remplacées partout
        let mut trouvees = Vec::new();
        let lettres: Vec<(usize, char)> = t.char_indices().collect();
        let mut k = 0;
        while k < lettres.len() {
            if lettres[k].1 == '%'
                && let Some(n) = lettres[k + 1..].iter().take(41).position(|&(_, c)| c == '%')
                && n >= 1
            {
                let debut = lettres[k].0;
                let fin = lettres[k + 1 + n].0;
                trouvees.push((t[debut..=fin].to_string(), t[debut + 1..fin].to_string()));
                k += n + 2;
                continue;
            }
            k += 1;
        }
        for (entier, nom) in trouvees {
            let v = self.variable(&nom)?;
            t = t.replace(&entier, &v);
        }
        if let Some(r) = texte::reste_apres(&t, "\\SystemRoot\\") {
            t = format!("{}\\{r}", self.windows);
        }
        let b = t.as_bytes();
        if b.len() < 3 || !b[0].is_ascii_alphabetic() || b[1] != b':' || b[2] != b'\\' || !texte::finit(&t, ".exe") {
            return None;
        }
        chemin::complet(&t)
    }

    /// Le chemin est-il dans le dossier de Windows ?
    pub fn dans_windows(&self, chemin: &str) -> bool {
        !self.windows.is_empty() && texte::commence(chemin, &format!("{}\\", self.windows.trim_end_matches('\\')))
    }
}

/// Un programme d'antivirus (`Test-AideAntivirus`) : un des programmes déclarés au Centre de sécurité (`antivirus`),
/// ou un fichier de leur dossier, sauf dans le dossier de Windows (`windows`) où seul le fichier déclaré compte.
pub fn est_antivirus(antivirus: &[String], chemin: &str, windows: &str) -> bool {
    let w = format!("{}\\", windows.trim_end_matches('\\'));
    for av in antivirus {
        if texte::egal(av, chemin) {
            return true;
        }
        let Some(d) = texte::dossier_de(av) else {
            continue;
        };
        if texte::commence(d, &w) || d.chars().count() <= 3 {
            continue;
        }
        if texte::commence(chemin, &format!("{d}\\")) {
            return true;
        }
    }
    false
}

/// Un chemin dans un dossier de Microsoft Defender (`\(Windows|Microsoft) Defender( Advanced Threat Protection)?\`)
pub fn dans_defender(chemin: &str) -> bool {
    [
        "\\Windows Defender\\",
        "\\Microsoft Defender\\",
        "\\Windows Defender Advanced Threat Protection\\",
        "\\Microsoft Defender Advanced Threat Protection\\",
    ]
    .iter()
    .any(|m| texte::contient(chemin, m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pc() -> Dossiers {
        Dossiers {
            windows: "C:\\Windows".into(),
            programmes: "C:\\Program Files".into(),
            programmes_x86: "C:\\Program Files (x86)".into(),
            donnees_communes: "C:\\ProgramData".into(),
        }
    }

    #[test]
    fn commandes_de_windows() {
        let d = pc();
        let p = |c: &str| d.programme_de_commande(c);
        assert_eq!(p("\"C:\\Program Files\\A\\a.exe\" -x"), Some("C:\\Program Files\\A\\a.exe".into()));
        assert_eq!(
            p("C:\\Windows\\system32\\svchost.exe -k netsvcs -p"),
            Some("C:\\Windows\\system32\\svchost.exe".into())
        );
        assert_eq!(p("%SystemRoot%\\System32\\x.exe"), Some("C:\\Windows\\System32\\x.exe".into()));
        assert_eq!(
            p("%ProgramFiles%\\Windows Defender\\MsMpEng.exe"),
            Some("C:\\Program Files\\Windows Defender\\MsMpEng.exe".into())
        );
        assert_eq!(p("\\SystemRoot\\System32\\drivers\\x.exe"), Some("C:\\Windows\\System32\\drivers\\x.exe".into()));
        assert_eq!(p("%MONDOSSIER%\\truc.exe"), None);
        assert_eq!(p("windowsdefender://threat"), None);
        assert_eq!(p("C:\\x\\..\\y.exe"), Some("C:\\y.exe".into()));
        assert_eq!(p(""), None);
    }

    #[test]
    fn antivirus() {
        let av = vec![
            "C:\\Program Files\\AVX\\Moteur\\avx.exe".to_string(),
            "C:\\Windows\\System32\\avxreport.exe".to_string(),
        ];
        assert!(est_antivirus(&av, "C:\\Program Files\\AVX\\Moteur\\avxui.exe", "C:\\Windows"));
        assert!(est_antivirus(&av, "C:\\Windows\\System32\\avxreport.exe", "C:\\Windows"));
        assert!(!est_antivirus(&av, "C:\\Windows\\System32\\cmd.exe", "C:\\Windows"));
        assert!(!est_antivirus(&av, "C:\\Program Files\\AVX\\Moteur2\\a.exe", "C:\\Windows"));
        assert!(dans_defender("C:\\ProgramData\\Microsoft\\Windows Defender\\Platform\\x.exe"));
    }
}
