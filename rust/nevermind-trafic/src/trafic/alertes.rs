//! Ce qui est anormal dans le trafic (`Get-TrafficAlerts`, `Test-GamePath`, `Test-SuitePath`, `Get-TrafficMarks`,
//! `Get-AppSigLabel`) : signature, dossier, outils de Windows détournés, ports, gros envois, prise en main à distance.

use super::etat::{App, port_suspect};
use crate::format::octets;
use regex::Regex;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::LazyLock;

pub static LOLBINS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(powershell|pwsh|cmd|wscript|cscript|mshta|rundll32|regsvr32|certutil|bitsadmin|msbuild|installutil|regasm|regsvcs|cmstp|wmic|forfiles|msiexec|hh)$").expect("motif")
});
pub static OUTILS_DISTANCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(anydesk|teamviewer\w*|rustdesk|screenconnect\.\w+|connectwise\w*|logmein\w*|splashtop\w*|ultraviewer\w*|supremo\w*|rutserv|rfusclient|aeroadmin|ammyy\w*|remotepc\w*|zohoassist\w*|getscreen\w*)$").expect("motif")
});
static ENVOI_OK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(onedrive|dropbox|googledrivefs|box|megasync|icloud\w*|obs\w*|streamlabs\w*|discord|steam\w*|teams|ms-teams|zoom|skype|chrome|msedge|firefox|opera|brave|vivaldi|qbittorrent|utorrent|bittorrent|transmission\w*|backblaze\w*|synology\w*|nvcontainer|nvidia share|shadowplay|medal\w*|outplayed|epicgameslauncher|claude|chatgpt|cursor|code|windsurf|copilot)$").expect("motif")
});
static JEU: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\\steamapps\\common\\|\\Epic Games\\|\\Riot Games\\|\\Battle\.net\\|\\Ubisoft Game Launcher\\games\\|\\EA Games\\|\\GOG Galaxy\\Games\\|\\XboxGames\\|\\Netmarble Game\\|-Win64-Shipping\.exe$|\\Binaries\\Win64\\").expect("motif")
});
static SUITE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^[a-z]:\\Mindset\\Programmes\\[^\\]+\\[^\\]+\.exe$").expect("motif"));
static RISQUE1: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\\(AppData\\Local\\Temp|Temp|Downloads|Téléchargements|Users\\Public)\\").expect("motif")
});
static RISQUE2: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^[a-z]:\\ProgramData\\[^\\]+\.exe$").expect("motif"));
static RISQUE3: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\\AppData\\Roaming\\[^\\]+\.exe$").expect("motif"));

/// Dossiers des jeux installés : les jeux sont souvent non signés, ce n'est pas suspect. `exes` : les .exe des jeux
/// connus (en minuscules, `$script:GameIndex`)
pub fn chemin_jeu(chemin: &str, exes: &[String]) -> bool {
    if chemin.is_empty() {
        return false;
    }
    if JEU.is_match(chemin) {
        return true;
    }
    let bas = chemin.to_lowercase();
    exes.contains(&bas)
}

/// Programmes de la suite Mindset (installés par Mindset dans <disque>:\Mindset\Programmes\<appli>\ ou
/// Program Files\Mindset\<appli>\) : pas signés, ce n'est pas suspect
pub fn chemin_suite(chemin: &str) -> bool {
    if chemin.is_empty() {
        return false;
    }
    if SUITE.is_match(chemin) {
        return true;
    }
    for pf in [std::env::var("ProgramW6432").ok(), std::env::var("ProgramFiles").ok()].into_iter().flatten() {
        if pf.is_empty() {
            continue;
        }
        let rx = format!(r"(?i)^{}\\Mindset\\[^\\]+\\[^\\]+\.exe$", regex::escape(pf.trim_end_matches('\\')));
        if Regex::new(&rx).is_ok_and(|r| r.is_match(chemin)) {
            return true;
        }
    }
    false
}

/// Les marques « C'est normal » et « analysé sans rien trouver » (`Get-TrafficMarks`) : clé (en minuscules) et date
pub fn marques(v: Option<Value>) -> Vec<(String, String)> {
    let mut r: Vec<(String, String)> = Vec::new();
    for l in crate::valeur::liste(&v.unwrap_or(Value::Null)) {
        let t = crate::valeur::texte(&l);
        let (k, d) = match t.split_once('|') {
            Some((k, d)) => (k.to_string(), d.to_string()),
            None => (t.clone(), String::new()),
        };
        if k.is_empty() {
            continue;
        }
        let bas = k.to_lowercase();
        match r.iter_mut().find(|(x, _)| x.to_lowercase() == bas) {
            Some(x) => x.1 = d,
            None => r.push((k, d)),
        }
    }
    r
}

pub fn marque<'a>(m: &'a [(String, String)], cle: &str) -> Option<&'a str> {
    let bas = cle.to_lowercase();
    m.iter().find(|(k, _)| k.to_lowercase() == bas).map(|(_, d)| d.as_str())
}

/// Une alerte : le programme, sa gravité (« bad », « warn », « info ») et pourquoi
#[derive(Clone, Debug, PartialEq)]
pub struct Alerte {
    pub cle: String,
    pub niveau: &'static str,
    pub pourquoi: Vec<String>,
}

fn rang(n: &str) -> u8 {
    match n {
        "bad" => 0,
        "warn" => 1,
        _ => 2,
    }
}

/// `Get-TrafficAlerts`
pub fn alertes<'a>(
    apps: impl Iterator<Item = &'a App>,
    confiance: &[(String, String)],
    analyses: &[(String, String)],
    exes: &[String],
) -> Vec<Alerte> {
    let mut r = Vec::new();
    for a in apps {
        if a.moi || marque(confiance, &a.cle).is_some() {
            continue;
        }
        let net: Vec<_> = a.dest.values().filter(|d| !d.privee).collect();
        if net.is_empty() {
            continue;
        }
        let mut why: Vec<String> = Vec::new();
        let mut level = "info";
        let sig = a.sig.as_deref().unwrap_or("");
        let analyse = marque(analyses, &a.cle);
        let unsigned = (sig == "NotSigned" || sig == "NotTrusted")
            && analyse.is_none()
            && !chemin_jeu(&a.chemin, exes)
            && !chemin_suite(&a.chemin);
        let risky = RISQUE1.is_match(&a.chemin) || RISQUE2.is_match(&a.chemin) || RISQUE3.is_match(&a.chemin);
        if sig == "HashMismatch" {
            why.push("sa signature numérique est invalide (le fichier a été modifié)".into());
            level = "bad";
        }
        if unsigned && risky {
            why.push(
                "il n'est signé par aucun éditeur et il est rangé dans un dossier où les virus aiment se cacher".into(),
            );
            level = "bad";
        } else if unsigned && sig != "HashMismatch" {
            why.push("il n'est signé par aucun éditeur connu".into());
            if level != "bad" {
                level = "warn";
            }
        }
        if LOLBINS.is_match(&a.nom) {
            if a.parent_sig == "Valid" {
                why.push(format!(
                    "outil de Windows lancé par {}{}, c'est normal",
                    a.parent,
                    if a.parent_pub.is_empty() { String::new() } else { format!(" (signé : {})", a.parent_pub) }
                ));
            } else if !a.parent.is_empty() || !sig.is_empty() {
                why.push(format!(
                    "c'est un outil de Windows que les virus détournent souvent pour télécharger ou envoyer des données{}",
                    if a.parent.is_empty() { String::new() } else { format!(" (lancé par {}, non signé)", a.parent) }
                ));
                if level != "bad" {
                    level = "warn";
                }
            }
        }
        // Un programme signé par son éditeur (un jeu, par exemple) peut utiliser ces ports pour ses serveurs :
        // pour lui, seuls les ports du réseau Tor restent signalés.
        let signe = sig == "Valid";
        let mut ports: Vec<String> = Vec::new();
        for d in &net {
            if let Some(lib) = port_suspect(d.port)
                && (!signe || matches!(d.port, 9001 | 9030 | 9050 | 9150))
            {
                let p = format!("port {} : {lib}", d.port);
                if !ports.contains(&p) {
                    ports.push(p);
                }
            }
        }
        if !ports.is_empty() {
            why.push(format!("il utilise un port inhabituel ({})", ports.join(", ")));
            if level != "bad" {
                level = "warn";
            }
        }
        let mo200 = 200.0 * 1024.0 * 1024.0;
        if a.out > mo200 && a.out > 3.0 * a.entrant && !ENVOI_OK.is_match(&a.nom) {
            why.push(format!("il envoie beaucoup plus qu'il ne reçoit ({} envoyés)", octets(a.out)));
            if level != "bad" {
                level = "warn";
            }
        }
        let mut ips: HashSet<&str> = a.ips_anciennes.iter().map(String::as_str).collect();
        for d in &net {
            ips.insert(d.distante.as_str());
        }
        if ips.len() > 150 && !ENVOI_OK.is_match(&a.nom) {
            why.push(format!("il contacte énormément d'adresses différentes ({})", ips.len()));
            if level != "bad" {
                level = "warn";
            }
        }
        if OUTILS_DISTANCE.is_match(&a.nom) {
            why.push(
                "c'est un logiciel de prise en main à distance : quelqu'un peut voir et contrôler ton écran".into(),
            );
            if level == "info" {
                level = "warn";
            }
        }
        if !why.is_empty() {
            if let Some(d) = analyse {
                why.push(format!("analysé par l'antivirus le {d} : aucun virus trouvé"));
            }
            r.push(Alerte { cle: a.cle.clone(), niveau: level, pourquoi: why });
        }
    }
    // (tri stable : à gravité égale, l'ordre où les programmes ont été vus)
    r.sort_by_key(|x| rang(x.niveau));
    r
}

/// La signature d'un programme en mots (`Get-AppSigLabel`) : texte et couleur
pub fn libelle_signature(a: &App) -> (String, &'static str) {
    match a.sig.as_deref() {
        Some("Valid") => {
            (if a.editeur.is_empty() { "Signé".into() } else { format!("Signé : {}", a.editeur) }, "#22D37A")
        }
        Some("HashMismatch") => ("Signature invalide".into(), "#F04438"),
        None => ("Vérification...".into(), "#A6A1BC"),
        Some("Unknown") => ("Signature non vérifiable".into(), "#A6A1BC"),
        Some("NotTrusted") => ("Certificat non reconnu".into(), "#F5A524"),
        Some(_) => {
            if a.chemin.is_empty() {
                ("Programme système".into(), "#A6A1BC")
            } else {
                ("Non signé".into(), "#F5A524")
            }
        }
    }
}
