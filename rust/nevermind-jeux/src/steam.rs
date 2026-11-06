//! Steam : bibliothèques (libraryfolders.vdf), fiches d'installation (appmanifest_*.acf), jeux et restes de jeux
//! désinstallés (Get-SteamLibraries, Read-SteamManifests, Get-SteamGames d'optimisations.ps1 ; Measure-LeftoverDir et
//! Remove-LeftoverDirs de bibliotheque.ps1).

use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

pub static PAS_DES_JEUX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(wallpaper_engine|Steamworks Shared|SteamVR|Steam Controller Configs|Steamworks Common Redistributables|GameSave|Minecraft Launcher)$").unwrap()
});

/// Le dossier principal de Steam, puis les bibliothèques de libraryfolders.vdf (lu en UTF-8, comme Steam l'écrit).
pub fn bibliotheques(steam: &str) -> Vec<String> {
    static CHEMIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)"path"\s+"([^"]+)""#).unwrap());
    let mut libs = vec![steam.to_string()];
    let vdf = crate::texte::joindre(steam, "steamapps\\libraryfolders.vdf");
    if let Ok(o) = std::fs::read(&vdf) {
        let t = String::from_utf8_lossy(&o);
        for m in CHEMIN.captures_iter(&t) {
            libs.push(m[1].replace("\\\\", "\\"));
        }
    }
    libs
}

/// Les fiches d'installation d'un dossier « steamapps » : noms et numéros rangés par dossier d'installation (minuscules).
#[derive(Debug, Default)]
pub struct Fiches {
    pub complet: bool,
    pub noms: HashMap<String, String>,
    pub ids: HashMap<String, String>,
}

pub fn lire_fiches(steamapps: &str) -> Fiches {
    static DIR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)"installdir"\s+"([^"]+)""#).unwrap());
    static NOM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)"name"\s+"([^"]+)""#).unwrap());
    static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)"appid"\s+"(\d+)""#).unwrap());
    static CHIFFRES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+)").unwrap());
    let mut r = Fiches { complet: true, ..Default::default() };
    if !Path::new(steamapps).is_dir() {
        return r;
    }
    let Ok(lecture) = std::fs::read_dir(steamapps) else {
        r.complet = false;
        return r;
    };
    let mut fiches: Vec<String> = lecture
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| {
            let b = n.to_lowercase();
            b.starts_with("appmanifest_") && b.ends_with(".acf")
        })
        .collect();
    fiches.sort();
    for f in fiches {
        let chemin = crate::texte::joindre(steamapps, &f);
        let txt = std::fs::read(&chemin).ok().map(|o| String::from_utf8_lossy(&o).to_string());
        let Some(txt) = txt.filter(|t| !t.is_empty()) else {
            r.complet = false;
            continue;
        };
        let Some(d) = DIR.captures(&txt) else {
            r.complet = false;
            continue;
        };
        let dir = d[1].to_lowercase();
        if let Some(n) = NOM.captures(&txt) {
            r.noms.insert(dir.clone(), n[1].to_string());
        }
        let id = if let Some(i) = ID.captures(&txt) {
            i[1].to_string()
        } else if let Some(i) = CHIFFRES.captures(&f) {
            i[1].to_string()
        } else {
            "0".into()
        };
        r.ids.insert(dir, id);
    }
    r
}

/// Un jeu Steam trouvé (complété ensuite par la recherche des exécutables).
#[derive(Debug, Clone, PartialEq)]
pub struct DossierSteam {
    pub nom: String,
    pub dossier: String,
    pub appid: String,
}

/// Un dossier de « steamapps\common » qu'aucune fiche ne cite (jeu désinstallé), ou une bibliothèque dont une fiche est
/// illisible (`incomplet`).
#[derive(Debug, Clone, PartialEq)]
pub struct Reste {
    pub nom: String,
    pub dossier: String,
    pub incomplet: bool,
}

#[derive(Debug, Default)]
pub struct JeuxSteam {
    pub dossiers: Vec<DossierSteam>,
    pub restes: Vec<Reste>,
    pub common: Vec<String>,
}

fn sous_dossiers(d: &str) -> Vec<(String, String, Option<SystemTime>)> {
    let d = &crate::texte::lettre_majuscule(d);
    let mut v: Vec<(String, String, Option<SystemTime>)> = std::fs::read_dir(d)
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .map(|e| {
                    let n = e.file_name().to_string_lossy().to_string();
                    let m = e.metadata().ok().and_then(|m| m.modified().ok());
                    (n.clone(), crate::texte::joindre(d, &n), m)
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|a| a.0.to_lowercase());
    v
}

/// Les jeux Steam et les restes (Get-SteamGames). `maintenant` : pour la règle des 7 jours.
pub fn jeux_steam(steam: &str, maintenant: SystemTime) -> JeuxSteam {
    let mut r = JeuxSteam::default();
    let limite = maintenant.checked_sub(Duration::from_secs(7 * 86400)).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut vus = std::collections::HashSet::new();
    for lib in bibliotheques(steam) {
        let cle = lib.trim_end_matches('\\').to_lowercase();
        if !vus.insert(cle) {
            continue;
        }
        let fiches = lire_fiches(&crate::texte::joindre(&lib, "steamapps"));
        let common = crate::texte::joindre(&lib, "steamapps\\common");
        if !Path::new(&common).exists() {
            continue;
        }
        r.common.push(common.to_lowercase());
        if !fiches.complet {
            r.restes.push(Reste { nom: String::new(), dossier: lib.clone(), incomplet: true });
        }
        for (nom, plein, modif) in sous_dossiers(&common) {
            if PAS_DES_JEUX.is_match(&nom) {
                continue;
            }
            let bas = nom.to_lowercase();
            match fiches.ids.get(&bas) {
                Some(id) => r.dossiers.push(DossierSteam {
                    nom: fiches.noms.get(&bas).cloned().unwrap_or_else(|| nom.clone()),
                    dossier: plein,
                    appid: id.clone(),
                }),
                None => {
                    if fiches.complet && modif.map(|m| m < limite).unwrap_or(false) {
                        r.restes.push(Reste { nom, dossier: plein, incomplet: false });
                    }
                }
            }
        }
    }
    r
}

/// Taille d'un reste, et s'il a bougé depuis moins de `jours` jours (lui ou un fichier dedans). Un dossier qui n'a pas
/// pu être parcouru en entier compte comme récent (prudence). (Measure-LeftoverDir)
pub fn mesurer_reste(dossier: &str, jours: u64) -> (f64, bool) {
    let limite = SystemTime::now().checked_sub(Duration::from_secs(jours * 86400)).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut taille = 0.0;
    let mut recent = false;
    match std::fs::metadata(dossier) {
        Ok(m) => {
            if m.modified().map(|t| t > limite).unwrap_or(true) {
                recent = true;
            }
        }
        Err(_) => return (0.0, true),
    }
    let mut pile = vec![dossier.to_string()];
    while let Some(d) = pile.pop() {
        let Ok(it) = std::fs::read_dir(&d) else {
            recent = true;
            continue;
        };
        for e in it {
            let Ok(e) = e else {
                recent = true;
                continue;
            };
            let Ok(m) = e.metadata() else {
                recent = true;
                continue;
            };
            if m.modified().map(|t| t > limite).unwrap_or(true) {
                recent = true;
            }
            if m.is_dir() {
                pile.push(e.path().to_string_lossy().to_string());
            } else {
                taille += m.len() as f64;
            }
        }
    }
    (taille, recent)
}

/// Le résultat de la suppression d'un reste : la raison s'il n'est pas supprimé, et `garde` s'il ne doit plus être
/// proposé (Steam s'en sert de nouveau, ou il vient de bouger). (Remove-LeftoverDirs)
#[derive(Debug, Clone, PartialEq)]
pub struct Suppression {
    pub dossier: String,
    pub raison: String,
    pub garde: bool,
}

pub fn supprimer_restes(dossiers: &[String]) -> Vec<Suppression> {
    static FORME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(.+\\steamapps)\\common\\([^\\]+)$").unwrap());
    let mut v = Vec::new();
    for d in dossiers {
        let mut err = String::new();
        let mut garde = false;
        match FORME.captures(d) {
            None => err = "ce n'est pas un dossier de jeu de Steam".into(),
            Some(m) => {
                let steamapps = m[1].to_string();
                let nom = m[2].to_lowercase();
                let fiches = lire_fiches(&steamapps);
                if !fiches.complet {
                    err = "gardé par prudence, une fiche de Steam n'a pas pu être lue (réessaie plus tard)".into();
                } else if fiches.ids.contains_key(&nom) {
                    err = "gardé, Steam s'en sert de nouveau (jeu réinstallé ou en cours de mise à jour)".into();
                    garde = true;
                } else if mesurer_reste(d, 7).1 {
                    err = "gardé par prudence, il a été modifié il y a moins de 7 jours".into();
                    garde = true;
                }
            }
        }
        if err.is_empty() {
            if std::fs::remove_dir_all(d).is_err() {
                // (fichiers en lecture seule : débloqués, puis un second essai)
                debloquer(d);
                if let Err(e) = std::fs::remove_dir_all(d) {
                    err = format!("{e} (ferme le jeu ou son launcher, puis réessaie)");
                }
            }
            if err.is_empty() && Path::new(d).exists() {
                err = "des fichiers sont encore utilisés (ferme le jeu ou son launcher, puis réessaie)".into();
            }
        }
        v.push(Suppression { dossier: d.clone(), raison: err, garde });
    }
    v
}

fn debloquer(d: &str) {
    let mut pile = vec![d.to_string()];
    while let Some(x) = pile.pop() {
        let Ok(it) = std::fs::read_dir(&x) else { continue };
        for e in it.flatten() {
            let p = e.path();
            if p.is_dir() {
                pile.push(p.to_string_lossy().to_string());
            } else if let Ok(m) = std::fs::metadata(&p) {
                let mut perm = m.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                perm.set_readonly(false);
                let _ = std::fs::set_permissions(&p, perm);
            }
        }
    }
}
