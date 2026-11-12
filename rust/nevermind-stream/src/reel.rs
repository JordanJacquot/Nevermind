//! Le vrai PC (lecture seule, sans droits administrateur) : le registre pour trouver OBS et Streamlabs, les
//! processus, les cartes graphiques, l'écran. Ailleurs que sous Windows : rien.

use crate::calcul::{Carte, Logiciel};
use crate::systeme::{Installation, Processus, Systeme, ajouter, dossier_obs, dossier_streamlabs, reglages_obs};
use std::path::PathBuf;

/// Le vrai PC
pub struct SystemeReel;

/// Les clés de désinstallation lues (les deux vues du registre, et celles du compte)
#[cfg(windows)]
const DESINSTALLATIONS: [(bool, &str); 3] = [
    (true, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    (true, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
    (false, r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
];

/// Une entrée de désinstallation : (nom de la clé, DisplayName, InstallLocation, DisplayIcon, UninstallString, DisplayVersion)
#[cfg(windows)]
type Entree = (String, String, String, String, String, String);

#[cfg(windows)]
fn desinstallations() -> Vec<Entree> {
    use windows_registry::{CURRENT_USER, LOCAL_MACHINE};
    let mut r = Vec::new();
    for (machine, chemin) in DESINSTALLATIONS {
        let racine = if machine { LOCAL_MACHINE } else { CURRENT_USER };
        let Ok(k) = racine.options().read().open(chemin) else { continue };
        let Ok(noms) = k.keys() else { continue };
        for n in noms {
            let Ok(s) = k.options().read().open(&n) else { continue };
            let v = |x: &str| s.get_string(x).unwrap_or_default();
            let nom = v("DisplayName");
            let interessant = n.eq_ignore_ascii_case("OBS Studio")
                || n.eq_ignore_ascii_case("Steam App 1905180")
                || nom.starts_with("OBS Studio")
                || nom.starts_with("Streamlabs");
            if interessant {
                r.push((n, nom, v("InstallLocation"), v("DisplayIcon"), v("UninstallString"), v("DisplayVersion")));
            }
        }
    }
    r
}

#[cfg(windows)]
fn cle_obs() -> Vec<String> {
    use windows_registry::LOCAL_MACHINE;
    [r"SOFTWARE\OBS Studio", r"SOFTWARE\WOW6432Node\OBS Studio"]
        .iter()
        .filter_map(|c| LOCAL_MACHINE.options().read().open(c).ok())
        .filter_map(|k| k.get_string("").ok())
        .collect()
}

impl Systeme for SystemeReel {
    #[cfg(windows)]
    fn installations(&self) -> Vec<Installation> {
        let mut l = Vec::new();
        let Some(appdata) = crate::windows::appdata() else { return l };
        let programmes = crate::windows::program_files();
        let entrees = desinstallations();
        // OBS Studio : la clé de l'installeur, la désinstallation (dont Steam), le dossier par défaut
        let mut indices: Vec<(String, Option<String>)> = cle_obs().into_iter().map(|d| (d, None)).collect();
        for (n, nom, lieu, icone, desinst, version) in &entrees {
            if n.eq_ignore_ascii_case("OBS Studio")
                || n.eq_ignore_ascii_case("Steam App 1905180")
                || nom.starts_with("OBS Studio")
            {
                for i in [lieu, icone, desinst] {
                    indices.push((i.clone(), Some(version.clone()).filter(|v| !v.is_empty())));
                }
            }
        }
        if let Some(p) = &programmes {
            indices.push((p.join("obs-studio").to_string_lossy().into_owned(), None));
        }
        for (indice, version) in indices {
            if let Some(d) = dossier_obs(&indice) {
                let reglages = reglages_obs(&d, &appdata);
                ajouter(&mut l, Installation { logiciel: Logiciel::Obs, dossier: d, reglages, version });
            }
        }
        // Streamlabs Desktop
        let mut indices: Vec<(String, Option<String>)> = Vec::new();
        for (_, nom, lieu, icone, desinst, version) in &entrees {
            if nom.starts_with("Streamlabs") {
                for i in [lieu, icone, desinst] {
                    indices.push((i.clone(), Some(version.clone()).filter(|v| !v.is_empty())));
                }
            }
        }
        if let Some(p) = &programmes {
            indices.push((p.join("Streamlabs OBS").to_string_lossy().into_owned(), None));
        }
        for (indice, version) in indices {
            if let Some(d) = dossier_streamlabs(&indice) {
                let reglages = appdata.join("slobs-client");
                ajouter(&mut l, Installation { logiciel: Logiciel::Streamlabs, dossier: d, reglages, version });
            }
        }
        l
    }
    #[cfg(not(windows))]
    fn installations(&self) -> Vec<Installation> {
        let _ = (dossier_obs, dossier_streamlabs, reglages_obs, ajouter);
        Vec::new()
    }
    fn processus(&self) -> Vec<Processus> {
        processus()
    }
    /// Un OBS dézippé vu ouvert : ses réglages dans son dossier s'il est en mode portable, sinon ceux du compte
    fn obs_vu(&self, dossier: &std::path::Path) -> Option<Installation> {
        if !crate::systeme::exe_obs(dossier).is_file() {
            return None;
        }
        #[cfg(windows)]
        let appdata = crate::windows::appdata()?;
        #[cfg(not(windows))]
        let appdata = PathBuf::new();
        let reglages = reglages_obs(dossier, &appdata);
        Some(Installation { logiciel: Logiciel::Obs, dossier: dossier.to_path_buf(), reglages, version: None })
    }
    fn cartes(&self) -> Vec<Carte> {
        cartes()
    }
    fn streamlabs(
        &self,
        ressource: &str,
        methode: &str,
        args: &[serde_json::Value],
    ) -> Result<serde_json::Value, String> {
        crate::tube::appeler(crate::tube::TUBE, ressource, methode, args)
    }
    fn ecran(&self) -> Option<(u32, u32)> {
        ecran()
    }
}

/// Les processus qui comptent (obs64.exe, obs32.exe, Streamlabs OBS.exe), avec leur chemin
pub fn processus() -> Vec<Processus> {
    #[cfg(windows)]
    {
        crate::windows::processus(&["obs64.exe", "obs32.exe", "Streamlabs OBS.exe"])
            .into_iter()
            .map(|(nom, chemin)| Processus { nom, chemin: chemin.map(PathBuf::from) })
            .collect()
    }
    #[cfg(not(windows))]
    {
        let _ = PathBuf::new();
        Vec::new()
    }
}

/// Les cartes graphiques (DXGI, sans les cartes logicielles)
pub fn cartes() -> Vec<Carte> {
    #[cfg(windows)]
    {
        crate::windows::cartes()
    }
    #[cfg(not(windows))]
    Vec::new()
}

/// La taille de l'écran principal
pub fn ecran() -> Option<(u32, u32)> {
    #[cfg(windows)]
    {
        crate::windows::ecran()
    }
    #[cfg(not(windows))]
    None
}
