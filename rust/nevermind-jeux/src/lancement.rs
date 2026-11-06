//! Lancer et désinstaller un jeu (bibliotheque.ps1) : par son launcher (connexion, mises à jour, anti-triche), sinon son
//! exécutable ; désinstaller toujours par le désinstalleur du jeu ou de son launcher, jamais en effaçant un dossier.

use crate::biblio::Jeu;
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq)]
pub enum Lancement {
    /// Une adresse ouverte par Windows (steam://, uplay://, shell:AppsFolder\...).
    Adresse(String),
    /// Un programme lancé dans son dossier.
    Exe { chemin: String, arguments: String },
}

/// Comment lancer un jeu (Get-GameLaunch). `applis` : les applis du menu Démarrer (jeux Xbox).
pub fn lancement(g: &Jeu, applis: &dyn Fn() -> Vec<(String, String)>) -> Lancement {
    let mut l = g.lancement.clone();
    if l.to_lowercase().starts_with("exe|") {
        let x: Vec<&str> = l.splitn(3, '|').collect();
        return Lancement::Exe {
            chemin: x.get(1).unwrap_or(&"").to_string(),
            arguments: x.get(2).unwrap_or(&"").to_string(),
        };
    }
    if l.to_lowercase().starts_with("xbox:") {
        let nom = l[5..].to_string();
        let b = nom.to_lowercase();
        // (-like "$name*" : le nom suivi de n'importe quoi, sans tenir compte de la casse)
        if let Some((_, id)) =
            applis().into_iter().find(|(n, _)| n.to_lowercase() == b || n.to_lowercase().starts_with(&b))
        {
            return Lancement::Adresse(format!("shell:AppsFolder\\{id}"));
        }
        l = String::new();
    }
    if !l.is_empty() {
        return Lancement::Adresse(l);
    }
    Lancement::Exe { chemin: g.premier_exe().to_string(), arguments: String::new() }
}

/// Une commande déclarée à Windows : « "C:\...\x.exe" arguments » ou « C:\...\x.exe arguments » (Split-Command).
pub fn couper_commande(c: &str) -> (String, String) {
    static GUILLEMETS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?is)^"([^"]+)"\s*(.*)$"#).unwrap());
    static EXE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)^(.+?\.exe)\s*(.*)$").unwrap());
    let c = c.trim();
    if let Some(m) = GUILLEMETS.captures(c) {
        return (m[1].to_string(), m[2].to_string());
    }
    if let Some(m) = EXE.captures(c) {
        return (m[1].to_string(), m[2].to_string());
    }
    (c.to_string(), String::new())
}

#[derive(Debug, Clone, PartialEq)]
pub enum Desinstallation {
    Aucune,
    /// Le launcher s'ouvre : l'utilisateur désinstalle depuis lui.
    Launcher(String),
    Adresse(String),
    Exe {
        chemin: String,
        arguments: String,
    },
}

/// Comment désinstaller un jeu (Get-GameUninstall).
pub fn desinstallation(g: &Jeu) -> Desinstallation {
    static ADRESSE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[a-z][a-z0-9+.-]*://").unwrap());
    let u = g.desinstall.clone();
    if u.is_empty() && (g.source.is_empty() || g.source.eq_ignore_ascii_case("Steam")) {
        return Desinstallation::Launcher("steam://nav/games".into());
    }
    if u.is_empty() {
        return Desinstallation::Aucune;
    }
    if u.to_lowercase().starts_with("launcher|") {
        return Desinstallation::Launcher(u[9..].to_string());
    }
    if ADRESSE.is_match(&u) {
        return Desinstallation::Adresse(u);
    }
    let (chemin, arguments) = couper_commande(&u);
    Desinstallation::Exe { chemin, arguments }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commandes() {
        assert_eq!(couper_commande(r#""C:\a b\u.exe" /x"#), (r"C:\a b\u.exe".into(), "/x".into()));
        assert_eq!(couper_commande(r"C:\a b\u.exe /S"), (r"C:\a b\u.exe".into(), "/S".into()));
        assert_eq!(couper_commande("MsiExec.exe /X{1}"), ("MsiExec.exe".into(), "/X{1}".into()));
    }
}
