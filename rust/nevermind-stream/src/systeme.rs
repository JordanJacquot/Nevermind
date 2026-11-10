//! Ce que le bloc demande au PC, derrière un trait (les tests donnent un faux PC) : les logiciels de stream installés,
//! les processus ouverts (avec leur chemin), les cartes graphiques et la taille de l'écran. Tout en lecture seule.
//!
//! La détection d'OBS ouvert se fait par le CHEMIN de obs64.exe : Streamlabs lance lui aussi un obs64.exe (son
//! moteur, dans `...\obs-studio-node\`), qui ne doit pas être pris pour OBS Studio.

use crate::calcul::{Carte, Logiciel};
use std::path::{Path, PathBuf};

/// Un logiciel de stream trouvé
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub logiciel: Logiciel,
    /// Le dossier d'installation (celui de `bin\64bit\obs64.exe`, ou de `Streamlabs OBS.exe`)
    pub dossier: PathBuf,
    /// Le dossier de ses réglages (`...\obs-studio`, `...\slobs-client`)
    pub reglages: PathBuf,
    /// La version écrite par son installeur, si elle est connue
    pub version: Option<String>,
}

/// Un processus ouvert
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Processus {
    /// Le nom de l'exe (« obs64.exe »)
    pub nom: String,
    /// Son chemin complet (`None` : illisible)
    pub chemin: Option<PathBuf>,
}

/// Le PC, vu du bloc
pub trait Systeme: Send {
    /// Les logiciels de stream installés (relus à chaque appel)
    fn installations(&self) -> Vec<Installation>;
    /// Les processus ouverts qui comptent ici (obs64.exe, obs32.exe, Streamlabs OBS.exe)
    fn processus(&self) -> Vec<Processus>;
    /// Les cartes graphiques
    fn cartes(&self) -> Vec<Carte>;
    /// La taille de l'écran principal (pixels)
    fn ecran(&self) -> Option<(u32, u32)>;
    /// Un OBS que rien n'a installé (dézippé, portable), vu ouvert dans ce dossier : son installation s'il y est
    /// encore. Par défaut : seulement un OBS en mode portable (ses réglages sont dans son dossier).
    fn obs_vu(&self, dossier: &Path) -> Option<Installation> {
        (exe_obs(dossier).is_file() && obs_portable(dossier)).then(|| Installation {
            logiciel: Logiciel::Obs,
            dossier: dossier.to_path_buf(),
            reglages: dossier.join("config").join("obs-studio"),
            version: None,
        })
    }
}

/// Les dossiers des OBS Studio ouverts qui ne sont pas dans les installations connues (un OBS portable, dézippé) :
/// d'après le chemin de leur obs64.exe, sans le moteur de Streamlabs
pub fn obs_ouverts_inconnus(processus: &[Processus], installations: &[Installation]) -> Vec<PathBuf> {
    let mut l: Vec<PathBuf> = Vec::new();
    for p in processus {
        if !(p.nom.eq_ignore_ascii_case("obs64.exe") || p.nom.eq_ignore_ascii_case("obs32.exe")) {
            continue;
        }
        let Some(c) = &p.chemin else { continue };
        if moteur_streamlabs(c, installations) {
            continue;
        }
        let Some(d) = dossier_obs(&c.to_string_lossy()) else { continue };
        let connu = installations.iter().any(|i| i.logiciel == Logiciel::Obs && meme_dossier(&i.dossier, &d));
        if !connu && !l.iter().any(|x| meme_dossier(x, &d)) {
            l.push(d);
        }
    }
    l
}

/// Le même dossier (sans tenir compte des majuscules ni de la barre finale)
pub fn meme(a: &Path, b: &Path) -> bool {
    meme_dossier(a, b)
}

/// Le chemin est celui du moteur de Streamlabs (un obs64.exe qui n'est pas OBS Studio)
fn moteur_streamlabs(c: &Path, installations: &[Installation]) -> bool {
    c.to_string_lossy().to_lowercase().contains("obs-studio-node")
        || installations.iter().any(|s| s.logiciel == Logiciel::Streamlabs && sous(c, &s.dossier))
}

/// Les fichiers qui mettent OBS en mode portable (à la racine de son installation)
pub const MARQUES_PORTABLE: [&str; 4] =
    ["portable_mode", "obs_portable_mode", "portable_mode.txt", "obs_portable_mode.txt"];

/// OBS est en mode portable (ses réglages restent dans son dossier)
pub fn obs_portable(dossier: &Path) -> bool {
    MARQUES_PORTABLE.iter().any(|m| dossier.join(m).is_file())
}

/// L'exe d'OBS Studio dans son dossier d'installation
pub fn exe_obs(dossier: &Path) -> PathBuf {
    dossier.join("bin").join("64bit").join("obs64.exe")
}

/// Le dossier de réglages d'OBS installé là : `<installation>\config\obs-studio` en portable, sinon celui du compte
pub fn reglages_obs(dossier: &Path, appdata: &Path) -> PathBuf {
    if obs_portable(dossier) { dossier.join("config").join("obs-studio") } else { appdata.join("obs-studio") }
}

/// Le dossier d'installation d'OBS d'après un chemin donné par le registre (le dossier lui-même, ou son exe, ou
/// l'icône de désinstallation) : `None` si obs64.exe n'y est pas
pub fn dossier_obs(indice: &str) -> Option<PathBuf> {
    let t = indice.trim().trim_matches('"');
    let t = t.split(',').next().unwrap_or(t).trim().trim_matches('"');
    if t.is_empty() {
        return None;
    }
    let mut p = PathBuf::from(t);
    for _ in 0..4 {
        if exe_obs(&p).is_file() {
            return Some(p);
        }
        if !p.pop() {
            break;
        }
    }
    None
}

/// Le dossier d'installation de Streamlabs d'après un chemin du registre : `None` si son exe n'y est pas
pub fn dossier_streamlabs(indice: &str) -> Option<PathBuf> {
    let t = indice.trim().trim_matches('"');
    let t = t.split(',').next().unwrap_or(t).trim().trim_matches('"');
    if t.is_empty() {
        return None;
    }
    let mut p = PathBuf::from(t);
    for _ in 0..3 {
        if p.join("Streamlabs OBS.exe").is_file() {
            return Some(p);
        }
        if !p.pop() {
            break;
        }
    }
    None
}

fn meme_dossier(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches(['\\', '/']))
}

fn sous(p: &Path, dossier: &Path) -> bool {
    let p = p.to_string_lossy().to_lowercase().replace('/', "\\");
    let d = dossier.to_string_lossy().to_lowercase().replace('/', "\\");
    let d = d.trim_end_matches('\\');
    !d.is_empty() && p.starts_with(&format!("{d}\\"))
}

/// Ajoute une installation si son dossier de réglages n'est pas déjà dans la liste
pub fn ajouter(l: &mut Vec<Installation>, i: Installation) {
    if !l.iter().any(|x| x.logiciel == i.logiciel && meme_dossier(&x.reglages, &i.reglages)) {
        l.push(i);
    }
}

/// Le logiciel est ouvert, d'après les processus. OBS Studio : un obs64.exe (ou obs32.exe) qui n'est pas le moteur de
/// Streamlabs (ni dans son dossier, ni dans obs-studio-node) ; un chemin illisible compte pour OBS si Streamlabs
/// n'est pas ouvert (dans le doute, on ne touche à rien). Streamlabs : « Streamlabs OBS.exe », ou son moteur.
pub fn ouvert(logiciel: Logiciel, processus: &[Processus], installations: &[Installation]) -> bool {
    let appli_streamlabs = processus.iter().any(|p| p.nom.eq_ignore_ascii_case("Streamlabs OBS.exe"));
    let moteur_streamlabs = |c: &Path| moteur_streamlabs(c, installations);
    let obs: Vec<&Processus> = processus
        .iter()
        .filter(|p| p.nom.eq_ignore_ascii_case("obs64.exe") || p.nom.eq_ignore_ascii_case("obs32.exe"))
        .collect();
    match logiciel {
        Logiciel::Obs => obs.iter().any(|p| match &p.chemin {
            Some(c) => !moteur_streamlabs(c),
            None => !appli_streamlabs,
        }),
        Logiciel::Streamlabs => {
            appli_streamlabs || obs.iter().any(|p| p.chemin.as_deref().is_some_and(moteur_streamlabs))
        }
    }
}

/// Le PC d'essai (construction d'essai de Nevermind) : jamais le registre ni les vrais réglages ; les logiciels
/// viennent du fichier `streameur-essai.json` du dossier de données d'essai
/// (`{ "obs": "<dossier d'un OBS portable>", "streamlabs": "<dossier de réglages>" }`). Les cartes, l'écran et les
/// processus sont lus pour de vrai (lecture seule).
pub struct SystemeEssai {
    pub fichier: PathBuf,
}

impl Systeme for SystemeEssai {
    fn installations(&self) -> Vec<Installation> {
        let Ok(o) = std::fs::read(&self.fichier) else { return Vec::new() };
        let Ok(j) = serde_json::from_slice::<serde_json::Value>(&o) else { return Vec::new() };
        let mut l = Vec::new();
        if let Some(d) = j["obs"].as_str().map(PathBuf::from)
            && exe_obs(&d).is_file()
            && obs_portable(&d)
        {
            l.push(Installation {
                logiciel: Logiciel::Obs,
                reglages: d.join("config").join("obs-studio"),
                dossier: d,
                version: None,
            });
        }
        if let Some(d) = j["streamlabs"].as_str().map(PathBuf::from).filter(|d| d.is_dir()) {
            l.push(Installation { logiciel: Logiciel::Streamlabs, dossier: d.clone(), reglages: d, version: None });
        }
        l
    }
    fn processus(&self) -> Vec<Processus> {
        crate::reel::processus()
    }
    fn cartes(&self) -> Vec<Carte> {
        crate::reel::cartes()
    }
    fn ecran(&self) -> Option<(u32, u32)> {
        crate::reel::ecran()
    }
    /// (essai : un vrai OBS ouvert sur le PC n'est jamais pris, même portable : seul le fichier d'essai compte)
    fn obs_vu(&self, _dossier: &Path) -> Option<Installation> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(nom: &str, chemin: Option<&str>) -> Processus {
        Processus { nom: nom.into(), chemin: chemin.map(PathBuf::from) }
    }

    #[test]
    fn obs_ouvert_par_le_chemin() {
        let sl = Installation {
            logiciel: Logiciel::Streamlabs,
            dossier: PathBuf::from(r"C:\Program Files\Streamlabs OBS"),
            reglages: PathBuf::from(r"C:\Users\x\AppData\Roaming\slobs-client"),
            version: None,
        };
        let inst = vec![sl];
        // le moteur de Streamlabs n'est pas OBS Studio
        let moteur = p(
            "obs64.exe",
            Some(r"C:\Program Files\Streamlabs OBS\resources\app.asar.unpacked\node_modules\obs-studio-node\obs64.exe"),
        );
        let appli = p("Streamlabs OBS.exe", Some(r"C:\Program Files\Streamlabs OBS\Streamlabs OBS.exe"));
        assert!(!ouvert(Logiciel::Obs, &[moteur.clone(), appli.clone()], &inst));
        assert!(ouvert(Logiciel::Streamlabs, &[moteur.clone(), appli.clone()], &inst));
        assert!(ouvert(Logiciel::Streamlabs, std::slice::from_ref(&moteur), &[]));
        let obs = p("OBS64.EXE", Some(r"C:\Program Files\obs-studio\bin\64bit\obs64.exe"));
        assert!(ouvert(Logiciel::Obs, &[obs.clone(), moteur.clone()], &inst));
        assert!(!ouvert(Logiciel::Streamlabs, &[obs], &inst));
        // un chemin illisible : OBS compte pour ouvert, sauf si Streamlabs est là (c'est son moteur)
        assert!(ouvert(Logiciel::Obs, &[p("obs64.exe", None)], &[]));
        assert!(!ouvert(Logiciel::Obs, &[p("obs64.exe", None), appli], &inst));
        assert!(!ouvert(Logiciel::Obs, &[p("chrome.exe", Some(r"C:\x\chrome.exe"))], &[]));
        assert!(!ouvert(Logiciel::Obs, &[], &[]));
    }

    #[test]
    fn dossiers_et_portable() {
        let d = std::env::temp_dir().join(format!("nevermind-stream-sys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let bin = d.join("obs").join("bin").join("64bit");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("obs64.exe"), b"MZ").unwrap();
        let racine = d.join("obs");
        let appdata = d.join("Roaming");
        assert_eq!(dossier_obs(&racine.to_string_lossy()), Some(racine.clone()));
        assert_eq!(dossier_obs(&format!("\"{}\",0", bin.join("obs64.exe").display())), Some(racine.clone()));
        assert_eq!(dossier_obs(&d.join("ailleurs").to_string_lossy()), None);
        assert_eq!(dossier_obs(""), None);
        assert_eq!(reglages_obs(&racine, &appdata), appdata.join("obs-studio"));
        std::fs::write(racine.join("portable_mode.txt"), b"").unwrap();
        assert_eq!(reglages_obs(&racine, &appdata), racine.join("config").join("obs-studio"));
        let mut l = Vec::new();
        let i = Installation {
            logiciel: Logiciel::Obs,
            dossier: racine.clone(),
            reglages: appdata.join("obs-studio"),
            version: None,
        };
        ajouter(&mut l, i.clone());
        ajouter(&mut l, Installation { dossier: d.join("steam"), ..i });
        assert_eq!(l.len(), 1, "deux OBS qui partagent les mêmes réglages : une seule entrée");
        let _ = std::fs::remove_dir_all(&d);
    }
}
