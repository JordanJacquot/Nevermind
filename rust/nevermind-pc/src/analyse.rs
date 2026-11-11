//! L'analyse complète (`Invoke-Analysis`, analyse.ps1) : la santé de chaque composant (cartes), ce qui freine les jeux
//! (constats et corrections), et le score. Elle reçoit ce qui a été lu du PC ([`Entrees`] : les données lentes de
//! `$AnalysisDataWork`, les programmes au démarrage, les jeux) et lit le reste par [`Pc`] (registre, écrans, powercfg).

use crate::modele::{Analyse, Barre, Carte, Changements, Constat, Correction, Declaration, Execution, couleurs};
use crate::net::{self, champ_entier, champ_liste, champ_nombre, champ_texte, champ_vrai, n_fr, taille};
use crate::pc::Pc;
use crate::reglages::{self, Demarrage};
use crate::score;
use crate::temps::{Moment, jours_entre};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;

const GO: f64 = 1024.0 * 1024.0 * 1024.0;

/// Un jeu installé (pour la carte graphique d'un portable à deux cartes)
#[derive(Debug, Clone, PartialEq)]
pub struct Jeu {
    pub nom: String,
    pub exes: Vec<String>,
}

/// Ce qui a été lu du PC avant l'analyse.
#[derive(Debug, Clone, Default)]
pub struct Entrees {
    /// Les données de `$AnalysisDataWork` (mêmes noms de champs que la version PowerShell)
    pub data: Value,
    pub demarrage: Vec<Demarrage>,
    pub jeux: Vec<Jeu>,
    /// `Invoke-Smi` (nvidia-smi) : les champs lus, `None` si pas de nvidia-smi
    pub smi: Option<HashMap<String, Option<f64>>>,
    /// `Get-GpuVram` : la mémoire vidéo de chaque carte (par son nom)
    pub vram: HashMap<String, f64>,
    /// Les batteries des portables (root\wmi) : BatteryStaticData, BatteryFullChargedCapacity, BatteryCycleCount
    pub cim: Value,
    /// `$env:firmware_type -eq 'UEFI'`
    pub uefi: bool,
    pub ignores: Vec<String>,
    /// Le plan d'alimentation créé par Nevermind (sauvegarde.json)
    pub plan_cree: Option<String>,
}

fn re(s: &str) -> Regex {
    Regex::new(s).unwrap()
}
static VIRTUELLE: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta"));
static DEDIEE: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)NVIDIA|GeForce|Radeon RX|Radeon Pro|Arc"));
static NVIDIA: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)NVIDIA|GeForce"));
/// Les cartes NVIDIA que le dernier pilote « Game Ready » prend encore en charge : Turing et après (toutes les RTX, les
/// GTX 16xx, les MX 4xx et 5xx, les T400 à T2000, Quadro ou non, Max-Q ou non). Les GTX 10xx et avant (Pascal,
/// Maxwell...) restent sur la branche 580 : le dernier pilote ne s'installe pas sur elles (second audit du 09/11).
/// Lot K : la Quadro T2000 (Turing, portables) manquait.
static NVIDIA_SUIVIE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\bRTX\b|\bGTX\s*16\d\d|\bMX\s*[45]\d\d\b|\bT(400|500|550|600|1000|1200|2000)\b"));
static AMD: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)AMD|Radeon"));
static INTEL: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)Intel"));
static BASIQUE: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)Microsoft Basic"));
static ESPACES: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));
static MARQUE_VIDE: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^(Unknown|Undefined|0+)$"));
static GPU2: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)GpuPreference=2"));
static MICROSOFT: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)Microsoft "));

/// `Get-LoadColor`
pub fn couleur_charge(p: f64, warn: f64, bad: f64) -> &'static str {
    if p >= bad {
        couleurs::BAD
    } else if p >= warn {
        couleurs::WARN
    } else {
        couleurs::OK
    }
}

/// `Format-Duration`
fn duree(secondes: i64) -> String {
    let jours = secondes as f64 / 86400.0;
    let h = (secondes % 86400) / 3600;
    let m = (secondes % 3600) / 60;
    if jours >= 1.0 { format!("{} j {} h", jours.floor() as i64, h) } else { format!("{h} h {m} min") }
}

/// `ConvertTo-NvidiaVersion` : 32.0.16.1714 donne 617.14
pub fn version_nvidia(v: &str) -> Option<String> {
    let d: String = v.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.len() < 5 {
        return None;
    }
    let d = &d[d.len() - 5..];
    Some(format!("{}.{}", d[..3].parse::<i64>().unwrap_or(0), &d[3..]))
}

/// Une carte NVIDIA encore prise en charge par le dernier pilote « Game Ready », celui que NVIDIA donne à Nevermind
/// (`NvLatest`) : pour les autres, ce pilote n'est pas le leur, et seul l'âge du pilote installé compte
pub fn nvidia_suivie(nom: &str) -> bool {
    NVIDIA.is_match(nom) && NVIDIA_SUIVIE.is_match(nom)
}

/// `Get-DriverLink`
pub fn lien_pilote(nom: &str) -> &'static str {
    if NVIDIA.is_match(nom) {
        "https://www.nvidia.com/fr-fr/drivers/"
    } else if AMD.is_match(nom) {
        "https://www.amd.com/fr/support/download/drivers.html"
    } else if INTEL.is_match(nom) {
        "https://www.intel.fr/content/www/fr/fr/support/detect.html"
    } else {
        "ms-settings:windowsupdate"
    }
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Redémarrer dans le BIOS (`$BiosRun`, `$BiosChanges`, `$BiosConfirm`, `$BiosDone`)
fn correction_bios(mut c: Correction) -> Correction {
    c.execution = Some(Execution::Bios);
    c.changements = Changements::Liste(vec![Declaration::texte(
        "Redémarre le PC dans 10 secondes, directement dans le BIOS (shutdown.exe /r /fw /t 10). Aucun réglage n'est changé : c'est toi qui choisis dans le BIOS.",
    )]);
    c.libelle = "Redémarrer dans le BIOS".into();
    c.confirmer = Some("Le PC va redémarrer directement dans le BIOS dans 10 secondes.\n\nEnregistre ton travail et ferme tes jeux avant. Continuer ?".into());
    c.sans_analyse = true;
    c.fait = Some("Redémarrage dans le BIOS dans 10 secondes...".into());
    c
}

/// `$BatSettings` : le nom court et l'identifiant de chaque réglage « batterie »
pub const REGLAGES_BATTERIE: [(&str, &str); 5] = [
    ("CritAction", "637ea02f-bbcb-4015-8e2c-a1c7b9c0b546"),
    ("CritLevel", "9a66d8d7-4ff7-4ef9-b5a2-5a326ca2a469"),
    ("LowLevel", "8183ba9a-e910-48da-8769-14ae6dc1170a"),
    ("LowNotify", "bcded951-187b-4d05-bccc-f7e51960c258"),
    ("LowAction", "d8742dcb-3e6a-4b3c-b3fe-374623cdcf06"),
];
pub const SOUS_GROUPE_BATTERIE: &str = "e73a048d-bf27-4f12-9731-8b2076e8891f";
pub const ACTIONS_BATTERIE: [&str; 4] = ["ne rien faire", "mise en veille", "mise en veille prolongée", "arrêt du PC"];

/// Les réglages « batterie » du plan actif (`Get-BatteryPowerSettings`) : (nom, [secteur, batterie]) dans l'ordre lu
pub type Batterie = Vec<(String, [Option<i64>; 2])>;

pub fn batterie_get(p: &Batterie, k: &str) -> Option<[Option<i64>; 2]> {
    p.iter().find(|(n, _)| n == k).map(|(_, v)| *v)
}

/// `Get-BatteryPowerSettings` : lu dans `powercfg /q SCHEME_CURRENT <sous-groupe batterie>`
pub fn reglages_batterie(pc: &mut dyn Pc) -> Batterie {
    static AC: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)(alternatif|\bAC\b).*:\s*0x([0-9a-fA-F]+)"));
    static DC: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)(continu|\bDC\b).*:\s*0x([0-9a-fA-F]+)"));
    let (_, o) = pc.powercfg(&["/q".into(), "SCHEME_CURRENT".into(), SOUS_GROUPE_BATTERIE.into()]);
    let mut r: Batterie = Vec::new();
    let mut cur: Option<String> = None;
    for l in o.lines() {
        if let Some(g) = reglages::GUID.find(l) {
            let g = g.as_str();
            cur = None;
            for (k, guid) in REGLAGES_BATTERIE {
                if guid.eq_ignore_ascii_case(g) {
                    cur = Some(k.to_string());
                    if let Some(x) = r.iter_mut().find(|(n, _)| n == k) {
                        x.1 = [None, None];
                    } else {
                        r.push((k.to_string(), [None, None]));
                    }
                }
            }
            continue;
        }
        let Some(k) = &cur else { continue };
        let i = if let Some(c) = AC.captures(l) {
            Some((0, i64::from_str_radix(&c[2], 16).unwrap_or(0) as i32 as i64))
        } else {
            DC.captures(l).map(|c| (1, i64::from_str_radix(&c[2], 16).unwrap_or(0) as i32 as i64))
        };
        if let Some((j, v)) = i
            && let Some(x) = r.iter_mut().find(|(n, _)| n == k)
        {
            x.1[j] = Some(v);
        }
    }
    r
}

/// `Test-HibernateOn`
fn hibernation(pc: &dyn Pc) -> bool {
    match reglages::valeur(pc, "HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Power", "HibernateEnabled") {
        Some(v) if !v.est_liste() => v.entier() == Some(1),
        _ => pc.hiberfil(),
    }
}

/// `Get-UpsConfigAdvice` : les problèmes, et les réglages à corriger (dans l'ordre)
pub fn conseil_onduleur(p: &Batterie, hib: bool) -> (Vec<String>, Vec<(String, i64)>) {
    let dc = |k: &str| batterie_get(p, k).and_then(|v| v[1]);
    let mut pr = Vec::new();
    let mut fix: Vec<(String, i64)> = Vec::new();
    let act = dc("CritAction");
    let crit = dc("CritLevel");
    let low = dc("LowLevel");
    let notify = dc("LowNotify");
    let ok_act = act == Some(3) || (act == Some(2) && hib);
    if let Some(a) = act
        && !ok_act
    {
        pr.push(
            match a {
                0 => "quand la batterie de l'onduleur sera presque vide, Windows ne fera rien : le PC s'éteindra d'un coup",
                1 => "quand la batterie sera presque vide, Windows mettra le PC en veille : la veille a besoin de courant, le PC s'éteindra d'un coup",
                2 => "Windows doit passer en veille prolongée, mais elle est désactivée sur ce PC : rien ne se passera",
                _ => "l'action sur batterie critique est inconnue",
            }
            .to_string(),
        );
        fix.push(("CritAction".into(), 3));
    }
    if let Some(c) = crit
        && c < 20
    {
        pr.push(format!(
            "l'arrêt est prévu à {c} % : sur un onduleur, ça ne laisse que quelques secondes{}",
            if act == Some(2) { ", trop peu pour écrire la mémoire sur le disque" } else { "" }
        ));
        fix.push(("CritLevel".into(), 25));
    }
    if let Some(l) = low
        && l < 40
    {
        pr.push(format!("l'alerte « batterie faible » arrive à {l} %, trop tard pour enregistrer ta partie"));
        fix.push(("LowLevel".into(), 50));
    }
    if notify == Some(0) {
        pr.push("aucune alerte ne prévient quand la batterie baisse".into());
        fix.push(("LowNotify".into(), 1));
    }
    if !fix.is_empty()
        && let Some(la) = batterie_get(p, "LowAction")
        && la[1] != Some(0)
    {
        fix.push(("LowAction".into(), 0));
    }
    (pr, fix)
}

/// `Format-BatSetting`
fn texte_batterie(p: &Batterie, niveau: &str, action: &str, alerte: &str) -> String {
    let l = match batterie_get(p, niveau) {
        Some(v) => format!("{} %", v[1].map(|x| x.to_string()).unwrap_or_default()),
        None => "?".into(),
    };
    let a = match batterie_get(p, action) {
        Some([_, Some(i)]) => {
            // (un indice négatif part de la fin, comme en PowerShell)
            let j = if i < 0 { ACTIONS_BATTERIE.len() as i64 + i } else { i };
            usize::try_from(j).ok().and_then(|j| ACTIONS_BATTERIE.get(j)).copied().unwrap_or("").to_string()
        }
        _ => "?".into(),
    };
    let n = if alerte.is_empty() {
        String::new()
    } else {
        match batterie_get(p, alerte) {
            Some(v) => if v[1] == Some(1) { ", avec une alerte" } else { ", sans alerte" }.into(),
            None => String::new(),
        }
    };
    format!("à {l} : {a}{n}")
}

/// `Get-BatChemistry`
fn chimie(code: &Value, win32: &Value) -> String {
    let mut t = String::new();
    if net::vrai(code) {
        let n = net::nombre_de(code);
        if n >= 0.0 && n <= u32::MAX as f64 {
            let b = (net::entier(n) as u32).to_le_bytes();
            t = b.iter().map(|&c| if c < 128 { c as char } else { '?' }).collect::<String>();
            t = t.trim_matches('\0').trim().to_string();
        }
    }
    let tl = t.to_lowercase();
    if tl.starts_with("pb") || net::entier(net::nombre_de(win32)) == 3 {
        return "Plomb (durée de vie 3 à 5 ans)".into();
    }
    if tl.contains("li") {
        return "Lithium".into();
    }
    if tl.contains("ni") {
        return "Nickel".into();
    }
    t
}

struct Constats(Vec<Constat>);
impl Constats {
    fn ajoute(&mut self, statut: &str, titre: &str, detail: &str, poids: i64, id: &str, c: Option<Correction>) {
        self.0.push(Constat::nouveau(statut, titre, detail, poids, id, c));
    }
}

/// `Invoke-Analysis`
pub fn analyser(pc: &mut dyn Pc, e: &Entrees) -> Analyse {
    let data = &e.data;
    let maintenant = pc.maintenant();
    let mut f = Constats(Vec::new());
    let mut cartes: Vec<Carte> = Vec::new();
    let mut info: Vec<(String, String)> = Vec::new();
    let set_info = |info: &mut Vec<(String, String)>, k: &str, v: String| {
        if let Some(x) = info.iter_mut().find(|(c, _)| c == k) {
            x.1 = v;
        } else {
            info.push((k.into(), v));
        }
    };
    let os = data.get("OS").cloned().unwrap_or(Value::Null);
    let build = champ_entier(&os, "BuildNumber");
    let fixe = reglages::est_fixe(data);
    let toutes = champ_liste(data, "Battery");
    let onduleurs: Vec<Value> = toutes.iter().filter(|b| fixe || reglages::est_onduleur(b)).cloned().collect();
    let batteries: Vec<Value> = toutes.iter().filter(|b| !(fixe || reglages::est_onduleur(b))).cloned().collect();
    let indices: Vec<Value> = champ_liste(data, "UpsHints").into_iter().filter(net::vrai).collect();
    let a_onduleur = !onduleurs.is_empty() || !indices.is_empty();
    let portable = reglages::est_portable(&batteries, data);
    if portable && batteries.iter().any(|b| champ_entier(b, "BatteryStatus") == 1) {
        f.ajoute("warn", "Portable sur batterie", "Sur batterie, Windows bride le processeur et la carte graphique. Branche le chargeur pour jouer.", 2, "laptop-battery", Some(
            Correction::conseil("Sur batterie, le processeur et la carte graphique tournent au ralenti pour économiser l'énergie: tu peux perdre la moitié de tes FPS.",
                &["Branche le chargeur de ton portable avant de jouer.", "Dans Paramètres > Système > Alimentation, choisis le mode « Meilleures performances ».", "Relance l'analyse."])
            .ouvre("ms-settings:powersleep", "Paramètres d'alimentation")));
    }

    // --- Processeur
    let cpu = data.get("CPU").cloned().unwrap_or(Value::Null);
    let nom_cpu = ESPACES.replace_all(&champ_texte(&cpu, "Name"), " ").trim().to_string();
    let coeurs = champ_texte(&cpu, "NumberOfCores");
    let threads = champ_texte(&cpu, "NumberOfLogicalProcessors");
    set_info(&mut info, "Processeur", format!("{nom_cpu} ({coeurs} cœurs, {threads} threads)"));
    let mut c = Carte::nouvelle("CPU", "Processeur", &nom_cpu);
    c.ligne("Cœurs", &format!("{coeurs} cœurs, {threads} threads"), None);
    c.ligne("Fréquence de base", &format!("{} GHz", n_fr(champ_nombre(&cpu, "MaxClockSpeed") / 1000.0, 1)), None);
    if champ_vrai(&cpu, "L3CacheSize") {
        c.ligne(
            "Cache L3",
            &format!("{} Mo", net::texte_nombre(net::arrondi(champ_nombre(&cpu, "L3CacheSize") / 1024.0))),
            None,
        );
    }
    c.ligne("Température", "Non fournie par Windows (utilise HWiNFO)", Some(couleurs::MUTED));
    cartes.push(c);

    // --- Carte graphique
    let gpus: Vec<Value> =
        champ_liste(data, "GPUs").into_iter().filter(|g| !VIRTUELLE.is_match(&champ_texte(g, "Name"))).collect();
    set_info(&mut info, "Carte graphique", gpus.iter().map(|g| champ_texte(g, "Name")).collect::<Vec<_>>().join(" + "));
    let a_dediee = gpus.iter().any(|g| DEDIEE.is_match(&champ_texte(g, "Name")));
    for g in &gpus {
        let nom = champ_texte(g, "Name");
        let mut c = Carte::nouvelle("GPU", "Carte graphique", &nom);
        if BASIQUE.is_match(&nom) {
            c.note("bad", "Aucun vrai pilote installé: les jeux tournent très mal.");
            c.action = Some("ms-settings:windowsupdate".into());
            c.libelle_action = Some("Mettre à jour".into());
            f.ajoute("bad", "Pilote graphique manquant", "Windows utilise un pilote générique: les jeux tournent très mal. Installe le pilote de ta carte graphique.", 3, "gpu-nodriver", Some(
                Correction::conseil("Sans le vrai pilote, la carte graphique ne sert presque à rien: pas d'accélération 3D correcte.",
                    &["Clique sur « Windows Update » et installe toutes les mises à jour, y compris les mises à jour facultatives de pilotes.", "Si ça ne suffit pas, télécharge le pilote sur le site du fabricant de ta carte (NVIDIA, AMD ou Intel).", "Redémarre puis relance l'analyse."])
                .ouvre("ms-settings:windowsupdate", "Windows Update")));
            cartes.push(c);
            continue;
        }
        if let Some(v) = e.vram.get(&nom).copied().filter(|v| *v != 0.0) {
            c.ligne("Mémoire vidéo", &taille(v), None);
        }
        let w = if a_dediee && !DEDIEE.is_match(&nom) { 1 } else { 2 };
        let version = champ_texte(g, "DriverVersion");
        let nv_installe = if nvidia_suivie(&nom) { version_nvidia(&version) } else { None };
        let nv_dernier = data.get("NvLatest").filter(|v| net::vrai(v)).cloned();
        if let (Some(nvi), Some(nvl)) = (&nv_installe, &nv_dernier) {
            let derniere = champ_texte(nvl, "Version");
            let vieux = net::version_avant(nvi, &derniere).unwrap_or(false);
            c.ligne(
                "Pilote",
                &format!(
                    "{nvi}{}",
                    if vieux { format!(" (la {derniere} est sortie)") } else { " (le plus récent)".into() }
                ),
                Some(if vieux { couleurs::WARN } else { couleurs::TEXTE }),
            );
            if vieux {
                let url = champ_texte(nvl, "Url");
                c.note("warn", &format!("Nouveau pilote NVIDIA disponible : {derniere}."));
                c.action = Some(url.clone());
                c.libelle_action = Some("Télécharger le pilote".into());
                f.ajoute("warn", &format!("Nouveau pilote NVIDIA disponible ({derniere})"), &format!("Tu as la version {nvi}. La dernière version « Game Ready » est la {derniere}."), w, &format!("gpu-driver:{nom}"), Some(
                    Correction::conseil("Chaque pilote « Game Ready » apporte des optimisations pour les jeux récents et corrige des bugs (plantages, textures qui clignotent...).",
                        &["Ouvre l'application NVIDIA si tu l'as (onglet Pilotes) et clique sur Télécharger, ou clique sur « Page du pilote ».", "Lance l'installation (installation rapide). L'écran peut clignoter, c'est normal.", "Relance l'analyse de Nevermind."])
                    .ouvre(&url, "Page du pilote")));
            } else {
                f.ajoute(
                    "ok",
                    &format!("Pilote graphique à jour ({nom})"),
                    &format!("Tu as le dernier pilote NVIDIA « Game Ready » ({nvi})."),
                    w,
                    &format!("gpu-driver:{nom}"),
                    None,
                );
            }
        } else if let Some(date) = g.get("DriverDate").and_then(|d| d.as_str()).and_then(Moment::lire) {
            let age = jours_entre(date, maintenant);
            c.ligne(
                "Pilote",
                &format!("{version} du {}", date.jjmmaaaa()),
                Some(if age > 180 { couleurs::WARN } else { couleurs::TEXTE }),
            );
            if age > 180 {
                let mois = (age as f64 / 30.0).floor() as i64;
                c.note(
                    "warn",
                    &format!("Pilote vieux d'environ {mois} mois: mets-le à jour pour de meilleures performances."),
                );
                c.action = Some(lien_pilote(&nom).into());
                c.libelle_action = Some("Télécharger le pilote".into());
                let etapes: &[&str] = if NVIDIA.is_match(&nom) {
                    &[
                        "Ouvre l'application NVIDIA si tu l'as (onglet Pilotes), ou clique sur « Site du pilote ».",
                        "Télécharge le dernier pilote « Game Ready » pour ta carte.",
                        "Lance l'installation (installation rapide). L'écran peut clignoter, c'est normal.",
                        "Relance l'analyse de Nevermind.",
                    ]
                } else if AMD.is_match(&nom) {
                    &[
                        "Ouvre AMD Software (clic droit sur le bureau) et va dans « Pilotes et logiciels », ou clique sur « Site du pilote ».",
                        "Installe la dernière version recommandée.",
                        "Redémarre si l'installation le demande, puis relance l'analyse.",
                    ]
                } else {
                    &[
                        "Clique sur « Site du pilote » et laisse l'assistant détecter ta carte.",
                        "Installe le pilote proposé.",
                        "Redémarre si besoin, puis relance l'analyse.",
                    ]
                };
                f.ajoute("warn", &format!("Pilote graphique ancien ({nom})"), &format!("Ton pilote date d'il y a environ {mois} mois (version {version})."), w, &format!("gpu-driver:{nom}"), Some(
                    Correction::conseil("Chaque nouveau pilote apporte des optimisations pour les jeux récents et corrige des bugs (plantages, textures qui clignotent...).", etapes)
                        .ouvre(lien_pilote(&nom), "Site du pilote")));
            } else {
                f.ajoute(
                    "ok",
                    &format!("Pilote graphique à jour ({nom})"),
                    &format!("Pilote de moins de 6 mois (version {version})."),
                    w,
                    &format!("gpu-driver:{nom}"),
                    None,
                );
            }
        }
        if NVIDIA.is_match(&nom)
            && let Some(smi) = &e.smi
        {
            let v = |k: &str| smi.get(k).copied().flatten();
            if let Some(t) = v("temperature.gpu") {
                let t = net::entier(t);
                c.ligne("Température", &format!("{t} °C"), Some(couleur_charge(t as f64, 80.0, 87.0)));
                if t >= 80 {
                    c.note("warn", &format!("Carte graphique chaude ({t} °C) alors que le PC ne joue pas forcément: dépoussière le PC et vérifie les ventilateurs."));
                }
            }
            if let Some(x) = v("fan.speed") {
                c.ligne("Ventilateurs", &format!("{} %", net::entier(x)), None);
            }
            if let Some(x) = v("power.draw") {
                let lim = v("power.limit").filter(|l| *l != 0.0);
                c.ligne(
                    "Consommation",
                    &format!(
                        "{} W{}",
                        n_fr(x, 0),
                        lim.map(|l| format!(" sur {} W max", n_fr(l, 0))).unwrap_or_default()
                    ),
                    None,
                );
            }
            if let (Some(wc), Some(wm)) =
                (v("pcie.link.width.current").filter(|x| *x != 0.0), v("pcie.link.width.max").filter(|x| *x != 0.0))
            {
                let (wc, wm) = (net::entier(wc), net::entier(wm));
                c.ligne(
                    "Liaison PCIe",
                    &format!("x{wc} (max x{wm})"),
                    Some(if wc < wm { couleurs::WARN } else { couleurs::TEXTE }),
                );
                if !portable && wc < wm && wm >= 16 {
                    c.note("warn", &format!("La carte fonctionne en x{wc} au lieu de x{wm}: vérifie qu'elle est branchée sur le premier port PCIe (le plus proche du processeur)."));
                    f.ajoute("warn", &format!("Carte graphique en PCIe x{wc}"), &format!("Elle devrait être en x{wm}. Souvent elle est branchée sur le mauvais port de la carte mère: tu peux perdre des FPS."), 2, "gpu-pcie", Some(
                        Correction::conseil("Avec moins de lignes PCIe, la carte graphique reçoit ses données moins vite, surtout quand sa mémoire vidéo est pleine.",
                            &["Éteins le PC et débranche le câble d'alimentation.", "Ouvre le boîtier: la carte graphique doit être sur le port PCIe le plus haut (le plus proche du processeur), souvent renforcé en métal.", "Vérifie qu'elle est bien enfoncée et que le clip de verrouillage est fermé.", "Si elle est déjà sur le bon port, un SSD branché sur certains emplacements M.2 peut partager ses lignes: regarde le manuel de ta carte mère."])));
                }
            }
        }
        cartes.push(c);
    }

    // --- Portable à deux cartes graphiques : les jeux doivent utiliser la carte dédiée
    let dediee = gpus.iter().find(|g| DEDIEE.is_match(&champ_texte(g, "Name")));
    let integrees = gpus
        .iter()
        .filter(|g| {
            let n = champ_texte(g, "Name");
            !DEDIEE.is_match(&n) && !BASIQUE.is_match(&n)
        })
        .count();
    if portable
        && let Some(dg) = dediee
        && integrees > 0
    {
        let dgpu = champ_texte(dg, "Name");
        let a_regler: Vec<&Jeu> =
            e.jeux.iter().filter(|j| j.exes.iter().any(|x| !GPU2.is_match(&reglages::preference_gpu(pc, x)))).collect();
        if e.jeux.is_empty() {
            f.ajoute("info", "Jeux sur la carte graphique dédiée", "Aucun jeu trouvé. Pour tes autres jeux, choisis « Hautes performances » dans Paramètres > Écran > Graphiques.", 0, "hybrid-gpu", Some(
                Correction::conseil(&format!("Ton portable a deux cartes graphiques. Windows peut lancer un jeu sur la puce intégrée, beaucoup moins puissante que ta {dgpu}."),
                    &["Ouvre Paramètres > Système > Écran > Graphiques.", "Ajoute ton jeu (bouton « Parcourir ») s'il n'est pas dans la liste.", "Clique dessus, puis « Options » et choisis « Hautes performances »."])
                .ouvre("ms-settings:display-advancedgraphics", "Paramètres graphiques")));
        } else if !a_regler.is_empty() {
            let noms: Vec<String> = a_regler.iter().map(|j| j.nom.clone()).collect();
            let liste = if noms.len() > 6 {
                format!("{} et {} autre{}", noms[..6].join(", "), noms.len() - 6, if noms.len() > 7 { "s" } else { "" })
            } else {
                noms.join(", ")
            };
            let n = a_regler.len();
            let x = if n > 1 { "x" } else { "" };
            let mut ch = Vec::new();
            for j in &a_regler {
                for d in reglages::changements_profil_gpu(&j.exes) {
                    if !GPU2.is_match(&reglages::preference_gpu(pc, &d.nom)) {
                        ch.push(d);
                    }
                }
            }
            let corr = Correction {
                auto: true,
                quoi: vec![format!("Régler {n} jeu{x} Steam / Epic sur « Hautes performances » pour qu'ils utilisent toujours la {dgpu} : {liste}."), "C'est le même réglage que Paramètres > Écran > Graphiques, en une seule fois.".into()],
                pourquoi: "Ton portable a deux cartes graphiques. Sur la puce intégrée, un jeu peut tourner 3 à 5 fois moins vite. Pense aussi à brancher le chargeur: sur batterie, la carte dédiée est bridée.".into(),
                ..Default::default()
            }
            .changements(Changements::Liste(ch))
            .ouvre("ms-settings:display-advancedgraphics", "Paramètres graphiques");
            f.ajoute(
                "warn",
                &format!("{n} jeu{x} sans carte graphique imposée"),
                &format!("Windows peut lancer ces jeux sur la puce intégrée au lieu de ta {dgpu} : {liste}."),
                2,
                "hybrid-gpu",
                Some(corr),
            );
        } else {
            f.ajoute(
                "ok",
                "Jeux sur la carte graphique dédiée",
                &format!("Tes {} jeux Steam / Epic utilisent la {dgpu}.", e.jeux.len()),
                2,
                "hybrid-gpu",
                None,
            );
        }
    }

    // --- Écrans
    let ecrans = pc.ecrans();
    if !ecrans.is_empty() {
        let n = ecrans.len();
        let pl = if n > 1 { "s" } else { "" };
        let mut c = Carte::nouvelle("HZ", "Écrans", &format!("{n} écran{pl} connecté{pl}"));
        let mut textes = Vec::new();
        for (i, d) in ecrans.iter().enumerate() {
            let i = i + 1;
            let p: Vec<&str> = d.split('|').collect();
            let num = |k: usize| p.get(k).and_then(|x| x.trim().parse::<f64>().ok()).map(net::entier).unwrap_or(0);
            let (w, h, cur, max) = (num(2), num(3), num(4), num(5));
            textes.push(format!("{w}x{h} à {cur} Hz"));
            let dev = p.first().copied().unwrap_or("");
            let id = format!("display:{dev}");
            let principal = n == 1 || (p.len() > 6 && p[6] == "1");
            let role = if n > 1 { if principal { " (principal)" } else { " (secondaire)" } } else { "" };
            let bride = max > 60 && (max - cur) >= 5;
            let ignore = bride && e.ignores.iter().any(|x| x.eq_ignore_ascii_case(&id));
            let sev = if principal { "bad" } else { "warn" };
            let couleur = if ignore {
                "#A6A1BC"
            } else if bride {
                couleurs::de(sev)
            } else {
                couleurs::TEXTE
            };
            let texte = format!(
                "{w}x{h} à {cur} Hz{}",
                if ignore {
                    " (volontaire)".into()
                } else if bride {
                    format!(" (peut faire {max} Hz)")
                } else {
                    String::new()
                }
            );
            c.ligne(&format!("Écran {i}{role}"), &texte, Some(couleur));
            if bride {
                if !ignore {
                    c.note(
                        sev,
                        &format!(
                            "L'écran {i} tourne à {cur} Hz au lieu de {max} Hz.{}",
                            if !principal {
                                " C'est un écran secondaire : ça ne compte pas dans la note."
                            } else {
                                ""
                            }
                        ),
                    );
                }
                let detail = format!(
                    "Ton écran {i} ({w} x {h}) peut monter à {max} Hz mais Windows l'utilise à {cur} Hz.{}",
                    if !principal {
                        " Écran secondaire : simple avertissement, il ne fait pas baisser ta note."
                    } else {
                        ""
                    }
                );
                let corr = Correction {
                    auto: true,
                    quoi: vec![format!("Passer l'écran {i} ({w} x {h}) de {cur} Hz à {max} Hz, sans changer sa résolution.")],
                    pourquoi: "Plus de Hz, c'est une image plus fluide et moins de latence: c'est le réglage qui se voit le plus en jeu. Si tu utilises cet écran seulement pour des vidéos, clique sur « Ignorer (c'est voulu) »: il ne comptera plus dans ton score. Si l'écran devient noir après le changement, ne touche à rien: Nevermind revient tout seul à l'ancien réglage au bout de 15 secondes.".into(),
                    ..Default::default()
                }
                .changements(Changements::Liste(vec![Declaration { sorte: "display".into(), ecran: dev.into(), hz: max, ..Default::default() }]))
                .ouvre("ms-settings:display-advanced", "Affichage avancé");
                f.ajoute(
                    sev,
                    &format!("Écran {i}{role} bridé à {cur} Hz"),
                    &detail,
                    if principal { 3 } else { 0 },
                    &id,
                    Some(corr),
                );
            } else if cur > 60 {
                f.ajoute(
                    "ok",
                    &format!("Écran {i}{role} à {cur} Hz"),
                    "Ton écran tourne à sa fréquence maximale.",
                    if principal { 3 } else { 0 },
                    &id,
                    None,
                );
            } else {
                f.ajoute("info", &format!("Écran {i}{role} en 60 Hz"), "Ton écran est à sa fréquence maximale (60 Hz). Un écran 144 Hz est l'une des meilleures améliorations pour jouer.", 0, &id, None);
            }
        }
        set_info(&mut info, "Écran", textes.join(" + "));
        if c.statut != "ok" {
            c.action = Some("ms-settings:display-advanced".into());
            c.libelle_action = Some("Affichage avancé".into());
        }
        cartes.push(c);
    }

    // --- Mémoire vive
    let mem = champ_liste(data, "Mem");
    if !mem.is_empty() {
        let premiere = &mem[0];
        let total_go = net::entier(net::arrondi(mem.iter().map(|m| champ_nombre(m, "Capacity")).sum::<f64>() / GO));
        let vitesse = if champ_vrai(premiere, "ConfiguredClockSpeed") {
            champ_entier(premiere, "ConfiguredClockSpeed")
        } else {
            champ_entier(premiere, "Speed")
        };
        let sorte = match champ_entier(premiere, "SMBIOSMemoryType") {
            24 => "DDR3",
            26 => "DDR4",
            30 => "LPDDR4",
            34 => "DDR5",
            35 => "LPDDR5",
            _ => "",
        };
        let nb = mem.len();
        set_info(
            &mut info,
            "Mémoire",
            format!("{total_go} Go {sorte} à {vitesse} MT/s ({nb} barrette{})", if nb > 1 { "s" } else { "" }),
        );
        let mut c = Carte::nouvelle("RAM", "Mémoire vive", &format!("{total_go} Go {sorte} à {vitesse} MT/s"));
        let tot = champ_nombre(&os, "TotalVisibleMemorySize") * 1024.0;
        let utilise = tot - champ_nombre(&os, "FreePhysicalMemory") * 1024.0;
        let pct = 100.0 * utilise / tot;
        c.barres.push(Barre {
            libelle: "Utilisée en ce moment".into(),
            valeur: pct,
            texte: format!("{} sur {}", taille(utilise), taille(tot)),
            couleur: couleur_charge(pct, 80.0, 90.0).into(),
        });
        if pct >= 85.0 {
            c.note(
                "warn",
                "Mémoire presque pleine en ce moment: ferme ton navigateur et les programmes inutiles avant de jouer.",
            );
        }
        if nb >= 2 {
            c.ligne("Mode", "Double canal", Some(couleurs::OK));
        } else {
            c.ligne("Mode", "Simple canal", Some(couleurs::WARN));
        }
        for (k, m) in mem.iter().enumerate() {
            let mut marque = champ_texte(m, "Manufacturer").trim().to_string();
            if MARQUE_VIDE.is_match(&marque) {
                marque.clear();
            }
            let morceaux = [
                format!("{} Go", net::texte_nombre(net::arrondi(champ_nombre(m, "Capacity") / GO))),
                marque,
                champ_texte(m, "PartNumber").trim().to_string(),
            ];
            c.ligne(
                &format!("Barrette {}", k + 1),
                &morceaux.iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" "),
                None,
            );
        }
        let md = data.get("MemDiag").filter(|v| net::vrai(v));
        if let Some(md) = md {
            let quand = md
                .get("TimeCreated")
                .and_then(|t| t.as_str())
                .and_then(Moment::lire)
                .map(|m| m.jjmmaaaa())
                .unwrap_or_default();
            let id = champ_entier(md, "Id");
            if id == 1101 || id == 1201 {
                c.ligne("Test mémoire Windows", &format!("Aucune erreur ({quand})"), Some(couleurs::OK));
            } else if id == 1102 || id == 1202 {
                c.ligne("Test mémoire Windows", &format!("Erreurs détectées ({quand})"), Some(couleurs::BAD));
                c.note("bad", "Le test mémoire de Windows a trouvé des erreurs. Désactive l'XMP / EXPO pour tester, et si ça continue, une barrette est défectueuse.");
                let corr = Correction {
                    pourquoi:
                        "Une mémoire qui fait des erreurs provoque plantages, écrans bleus et fichiers corrompus."
                            .into(),
                    etapes: s(&[
                        "Entre dans le BIOS et remets la mémoire en réglage par défaut (désactive XMP / EXPO).",
                        "Relance le test mémoire de Windows avec le bouton ci-dessous.",
                        "Si les erreurs continuent, teste les barrettes une par une: celle qui provoque des erreurs est défectueuse.",
                    ]),
                    quoi: s(&["Lancer le test mémoire de Windows (le PC redémarre, le test dure environ 15 minutes)."]),
                    execution: Some(Execution::TestMemoire),
                    changements: Changements::Liste(vec![Declaration::texte(
                        "Lance le test mémoire de Windows (mdsched.exe) : il propose de redémarrer maintenant ou au prochain démarrage. Aucun réglage n'est changé.",
                    )]),
                    libelle: "Lancer le test mémoire".into(),
                    sans_analyse: true,
                    fait: Some("Le test mémoire de Windows est lancé: choisis « Redémarrer maintenant ».".into()),
                    ..Default::default()
                };
                f.ajoute(
                    "bad",
                    "Mémoire défectueuse ou instable",
                    "Le dernier test mémoire de Windows a trouvé des erreurs: plantages et écrans bleus possibles.",
                    3,
                    "ram-errors",
                    Some(corr),
                );
            }
        } else {
            c.ligne("Test mémoire Windows", "Jamais lancé", Some(couleurs::MUTED));
        }
        c.action = Some("run:mdsched.exe".into());
        c.libelle_action = Some("Tester la mémoire (redémarre le PC)".into());
        cartes.push(c);

        let fix_ram = Correction::conseil(
            "Quand la mémoire est pleine, Windows utilise le disque à la place: grosses saccades garanties.",
            &[
                "En attendant: ferme ton navigateur, Discord et les programmes inutiles avant de jouer.",
                "Regarde combien d'emplacements libres a ta carte mère (ou si ton portable accepte plus de mémoire).",
                "Achète un kit identique à ta mémoire actuelle (même type et même vitesse), ou un kit de 2 x 8 Go / 2 x 16 Go.",
            ],
        );
        if total_go < 8 {
            f.ajoute(
                "bad",
                &format!("Seulement {total_go} Go de mémoire"),
                "Trop peu pour les jeux actuels: 16 Go est le minimum conseillé aujourd'hui.",
                2,
                "ram-amount",
                Some(fix_ram),
            );
        } else if total_go < 16 {
            f.ajoute(
                "warn",
                &format!("{total_go} Go de mémoire"),
                "Ça passe, mais beaucoup de jeux récents conseillent 16 Go.",
                2,
                "ram-amount",
                Some(fix_ram),
            );
        } else {
            f.ajoute(
                "ok",
                &format!("{total_go} Go de mémoire"),
                "Suffisant pour les jeux actuels.",
                2,
                "ram-amount",
                None,
            );
        }
        let fix_canal = Correction::conseil(
            "En double canal, la mémoire a deux fois plus de débit. Les jeux qui dépendent du processeur y gagnent beaucoup.",
            &[
                "Ajoute une 2e barrette identique (même capacité, même vitesse), ou remplace par un kit de 2 barrettes.",
                "Sur une carte mère à 4 emplacements, mets les 2 barrettes sur les ports A2 et B2 (en général le 2e et le 4e en partant du processeur, voir le manuel).",
                "Relance l'analyse: la ligne « Mode » doit indiquer « Double canal ».",
            ],
        );
        if nb == 1 {
            if portable {
                f.ajoute("warn", "Une seule barrette de mémoire", "La mémoire fonctionne sans doute en simple canal, ce qui peut coûter beaucoup de FPS (surtout avec une puce graphique intégrée). Vérifie si ton portable accepte une 2e barrette.", 1, "ram-channel", Some(fix_canal));
            } else {
                f.ajoute("bad", "Une seule barrette de mémoire", "La mémoire fonctionne en simple canal: jusqu'à 10 à 30 % de FPS en moins dans certains jeux. Ajoute une 2e barrette identique (ou prends un kit de 2).", 3, "ram-channel", Some(fix_canal));
            }
        } else {
            f.ajoute("ok", "Mémoire en double canal", &format!("{nb} barrettes installées."), 2, "ram-channel", None);
        }
        if !portable && (sorte == "DDR4" || sorte == "DDR5") {
            let lente = (sorte == "DDR4" && vitesse <= 2666) || (sorte == "DDR5" && vitesse <= 4800);
            if lente {
                let corr = correction_bios(Correction {
                    pourquoi: "Sans profil XMP / EXPO, la mémoire tourne à une vitesse de sécurité: souvent 10 à 20 % de FPS minimum en moins dans les jeux gourmands en processeur.".into(),
                    etapes: s(&["Clique sur « Redémarrer dans le BIOS » (enregistre ton travail avant).", "Dans le BIOS, cherche XMP (Intel), EXPO ou DOCP (AMD). Souvent dans le menu Ai Tweaker, OC ou Extreme Tweaker.", "Choisis le Profil 1, puis enregistre et quitte avec F10.", "Si le PC devient instable, retourne dans le BIOS et remets ce réglage sur Auto."]),
                    quoi: s(&["Redémarrer le PC directement dans le BIOS, pour que tu n'aies pas à chercher la bonne touche au démarrage."]),
                    ..Default::default()
                });
                f.ajoute("warn", &format!("Mémoire à {vitesse} MT/s: profil XMP / EXPO à vérifier"), &format!("La plupart des barrettes gaming sont vendues pour une vitesse plus élevée, mais il faut activer le profil XMP (Intel) ou EXPO / DOCP (AMD) dans le BIOS. Si tes barrettes sont vendues pour {vitesse} MT/s, tout va bien."), 2, "ram-xmp", Some(corr));
            } else {
                f.ajoute(
                    "ok",
                    &format!("Mémoire à {vitesse} MT/s"),
                    "Le profil XMP / EXPO semble actif.",
                    2,
                    "ram-xmp",
                    None,
                );
            }
        }
    }

    // --- Disques
    let disque_sys = champ_texte(data, "SysDisk");
    for dd in champ_liste(data, "Disks") {
        let d = dd.get("Disk").cloned().unwrap_or(Value::Null);
        let media = champ_texte(&d, "MediaType");
        let bus = champ_texte(&d, "BusType");
        let eq = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        let sorte = if eq(&bus, "NVMe") {
            "SSD NVMe"
        } else if eq(&media, "SSD") {
            "SSD"
        } else if eq(&media, "HDD") {
            "Disque dur"
        } else {
            "Disque"
        };
        let tag = if eq(&bus, "USB") {
            "USB"
        } else if eq(&media, "HDD") {
            "HDD"
        } else {
            "SSD"
        };
        let est_sys = champ_texte(&d, "DeviceId") == disque_sys;
        let nom_brut = champ_texte(&d, "FriendlyName");
        let sous = format!(
            "{sorte}, {}{}{}",
            taille(champ_nombre(&d, "Size")),
            if eq(&bus, "USB") { ", externe" } else { "" },
            if est_sys { ", Windows" } else { "" }
        );
        let mut c = Carte::nouvelle(tag, nom_brut.trim(), &sous);
        for pt in champ_liste(&dd, "Vols") {
            let v = pt.get("Vol").cloned().unwrap_or(Value::Null);
            if !net::vrai(&v) || !champ_vrai(&v, "Size") {
                continue;
            }
            let t = champ_nombre(&v, "Size");
            let libre = champ_nombre(&v, "SizeRemaining");
            let pct = 100.0 * (t - libre) / t;
            let lettre = champ_texte(&pt, "DriveLetter");
            let etiquette = champ_texte(&v, "FileSystemLabel");
            let lib = format!(
                "Lecteur {lettre}:{}",
                if etiquette.is_empty() { String::new() } else { format!(" {etiquette}") }
            );
            c.barres.push(Barre {
                libelle: lib,
                valeur: pct,
                texte: format!("{} libres sur {}", taille(libre), taille(t)),
                couleur: couleur_charge(pct, 80.0, 90.0).into(),
            });
            if pct >= 90.0 {
                c.note("warn", &format!("Le lecteur {lettre}: est presque plein."));
            }
        }
        match champ_texte(&d, "HealthStatus").to_lowercase().as_str() {
            "healthy" => c.ligne("État SMART", "Bon", Some(couleurs::OK)),
            "warning" => {
                c.ligne("État SMART", "Avertissement", Some(couleurs::WARN));
                c.note("warn", "Le disque signale un problème: sauvegarde tes fichiers importants.");
            }
            "unhealthy" => {
                c.ligne("État SMART", "Défaillant", Some(couleurs::BAD));
                c.note(
                    "bad",
                    "Le disque est en train de lâcher: sauvegarde tes fichiers MAINTENANT et prévois de le remplacer.",
                );
            }
            _ => c.ligne("État SMART", "Inconnu", Some(couleurs::MUTED)),
        }
        let rel = dd.get("Rel").filter(|v| net::vrai(v)).cloned();
        if let Some(rel) = rel {
            if rel.get("Wear").is_some_and(|x| !x.is_null()) && tag != "HDD" {
                let usure = champ_entier(&rel, "Wear");
                c.ligne("Usure", &format!("{usure} %"), Some(couleur_charge(usure as f64, 70.0, 90.0)));
                if usure >= 90 {
                    c.note(
                        "bad",
                        &format!("SSD usé à {usure} %: il approche de sa fin de vie, prévois son remplacement."),
                    );
                } else if usure >= 70 {
                    c.note("warn", &format!("SSD usé à {usure} %: surveille-le et sauvegarde tes fichiers."));
                }
            }
            if champ_nombre(&rel, "Temperature") > 0.0 {
                let t = champ_entier(&rel, "Temperature");
                let seuil = if tag == "HDD" { 50 } else { 70 };
                c.ligne(
                    "Température",
                    &format!("{t} °C"),
                    Some(couleur_charge(t as f64, seuil as f64, (seuil + 10) as f64)),
                );
                if t >= seuil {
                    c.note("warn", &format!("Disque chaud ({t} °C): vérifie la ventilation du boîtier (un dissipateur aide beaucoup sur un SSD NVMe)."));
                }
            }
            if champ_nombre(&rel, "PowerOnHours") > 0.0 {
                c.ligne("Heures d'utilisation", &format!("{} h", n_fr(champ_nombre(&rel, "PowerOnHours"), 0)), None);
            }
            if rel.get("ReadErrorsUncorrected").is_some_and(|x| !x.is_null()) {
                let re = champ_entier(&rel, "ReadErrorsUncorrected");
                c.ligne(
                    "Erreurs de lecture",
                    &re.to_string(),
                    Some(if re > 0 { couleurs::WARN } else { couleurs::OK }),
                );
                if re > 0 {
                    c.note(
                        "warn",
                        &format!("{re} erreur(s) de lecture non corrigée(s): sauvegarde tes fichiers importants."),
                    );
                }
            }
        } else {
            c.ligne("Usure et température", "Non fournies par ce disque", Some(couleurs::MUTED));
        }
        let fix_disque = Correction {
            pourquoi: c.notes.iter().map(|n| n.texte.clone()).collect::<Vec<_>>().join(" "),
            etapes: s(&[
                "Copie tes fichiers importants (photos, documents, sauvegardes de jeux) sur un autre disque ou dans le cloud.",
                "Regarde le détail dans la carte du disque (usure, température, erreurs, remplissage).",
                "Si le disque est défaillant ou très usé, remplace-le rapidement: les jeux se réinstallent, tes fichiers perso non.",
            ]),
            ..Default::default()
        };
        if c.statut == "bad" {
            f.ajoute(
                "bad",
                &format!("Disque en mauvaise santé: {nom_brut}"),
                "Sauvegarde tes fichiers importants dès maintenant.",
                3,
                &format!("disk:{nom_brut}"),
                Some(fix_disque),
            );
        } else if c.statut == "warn" {
            f.ajoute(
                "warn",
                &format!("Disque à surveiller: {nom_brut}"),
                "Détails dans la carte du disque, dans « Santé des composants ».",
                2,
                &format!("disk:{nom_brut}"),
                Some(fix_disque),
            );
        }
        if est_sys {
            if eq(&media, "HDD") {
                f.ajoute("bad", "Windows est sur un disque dur mécanique", "Démarrage lent, chargements très longs et saccades dans les jeux récents. Passer à un SSD est l'amélioration la plus visible possible.", 3, "disk-hdd", Some(
                    Correction::conseil("Un SSD est 5 à 50 fois plus rapide qu'un disque dur: Windows démarre en quelques secondes et les jeux chargent beaucoup plus vite.",
                        &["Achète un SSD (un NVMe si ta carte mère a un emplacement M.2, sinon un SSD SATA 2,5 pouces).", "Clone ton disque actuel vers le SSD avec le logiciel gratuit du fabricant (Samsung Magician, Crucial Acronis, WD...), ou réinstalle Windows dessus.", "Garde l'ancien disque dur pour stocker tes fichiers."])));
            } else if sorte.starts_with("SSD") {
                f.ajoute("ok", &format!("Windows est sur un {sorte}"), "Chargements rapides.", 3, "disk-hdd", None);
            }
            set_info(&mut info, "Disque système", format!("{sorte} {nom_brut}"));
        }
        cartes.push(c);
    }
    let ld = data.get("LogicalC").cloned().unwrap_or(Value::Null);
    if net::vrai(&ld) && champ_vrai(&ld, "Size") {
        let libre = champ_nombre(&ld, "FreeSpace");
        let go = net::entier(net::arrondi(libre / GO));
        let pct = net::arrondi(100.0 * libre / champ_nombre(&ld, "Size"));
        let fix_place = Correction {
            auto: true,
            quoi: s(&["Supprimer les fichiers temporaires de Windows et de ton compte.", "Supprimer les fichiers de mises à jour Windows déjà installées et les anciens rapports d'erreurs.", "Tes documents, tes jeux et la corbeille ne sont pas touchés."]),
            pourquoi: "Windows et les jeux ralentissent quand le disque système est plein, et les mises à jour de jeux peuvent échouer. Si ça ne suffit pas, désinstalle les jeux auxquels tu ne joues plus.".into(),
            execution: Some(Execution::Nettoyage),
            changements: Changements::Nettoyage,
            ..Default::default()
        }
        .ouvre("tab:4", "Nettoyage");
        if pct < 10.0 {
            f.ajoute(
                "bad",
                &format!("Disque presque plein ({go} Go libres)"),
                "Windows et les jeux ralentissent quand le disque est plein.",
                2,
                "disk-space",
                Some(fix_place),
            );
        } else if pct < 20.0 {
            f.ajoute(
                "warn",
                &format!("Espace disque limité ({go} Go libres)"),
                "Garde au moins 20 % de libre pour de bonnes performances.",
                2,
                "disk-space",
                Some(fix_place),
            );
        } else {
            f.ajoute("ok", &format!("Espace disque suffisant ({go} Go libres)"), "", 2, "disk-space", None);
        }
    }

    // --- Carte mère et BIOS
    let bb = data.get("BaseBoard").cloned().unwrap_or(Value::Null);
    let bios = data.get("BIOS").cloned().unwrap_or(Value::Null);
    let mut c = Carte::nouvelle(
        "BIOS",
        "Carte mère et BIOS",
        format!("{} {}", champ_texte(&bb, "Manufacturer"), champ_texte(&bb, "Product")).trim(),
    );
    if net::vrai(&bios) {
        c.ligne("Version du BIOS", &champ_texte(&bios, "SMBIOSBIOSVersion"), None);
        if let Some(date) = bios.get("ReleaseDate").and_then(|d| d.as_str()).and_then(Moment::lire) {
            let ans = ((maintenant.0 - date.0) as f64 / 86400.0 / 365.0).floor() as i64;
            let age =
                if ans >= 1 { format!(" (il y a {ans} an{})", if ans > 1 { "s" } else { "" }) } else { String::new() };
            c.ligne(
                "Date du BIOS",
                &format!("{}{age}", date.jjmmaaaa()),
                Some(if ans >= 3 { couleurs::WARN } else { couleurs::TEXTE }),
            );
            if ans >= 3 && !portable {
                c.note("info", "BIOS de plus de 3 ans: une mise à jour depuis le site du fabricant peut améliorer la stabilité et la compatibilité mémoire. À faire avec prudence (ne jamais couper le courant pendant la mise à jour).");
            }
        }
    }
    if e.uefi {
        c.ligne("Démarrage", "UEFI", None);
    } else {
        c.ligne("Démarrage", "Legacy (ancien BIOS)", Some(couleurs::WARN));
    }
    match data.get("SecureBoot") {
        Some(Value::Null) | None => c.ligne("Secure Boot", "Non disponible", Some(couleurs::MUTED)),
        Some(v) => {
            if net::vrai(v) {
                c.ligne("Secure Boot", "Activé", Some(couleurs::OK));
            } else {
                c.ligne("Secure Boot", "Désactivé", Some(couleurs::WARN));
                c.note("warn", "Certains jeux récents (Valorant, Battlefield 6, Call of Duty) exigent le Secure Boot pour leur anti-triche. Active-le dans le BIOS si un jeu refuse de se lancer.");
                let corr = correction_bios(Correction {
                    pourquoi: "Les anti-triche récents vérifient que le Secure Boot est actif. Sans lui, certains jeux refusent de démarrer.".into(),
                    etapes: s(&["Clique sur « Redémarrer dans le BIOS » (enregistre ton travail avant).", "Va dans le menu Boot (ou Sécurité) > Secure Boot et mets-le sur « Enabled ». Sur certaines cartes, il faut d'abord mettre « OS Type » sur « Windows UEFI mode ».", "Enregistre avec F10 et redémarre."]),
                    quoi: s(&["Redémarrer le PC directement dans le BIOS."]),
                    ..Default::default()
                });
                f.ajoute(
                    "warn",
                    "Secure Boot désactivé",
                    "Des jeux comme Valorant, Battlefield 6 ou les derniers Call of Duty peuvent refuser de se lancer.",
                    1,
                    "secureboot",
                    Some(corr),
                );
            }
        }
    }
    if champ_vrai(data, "TpmOk") {
        let tpm = data.get("Tpm").cloned().unwrap_or(Value::Null);
        if net::vrai(&tpm) {
            let v = champ_texte(&tpm, "SpecVersion");
            c.ligne("Puce TPM", &format!("Version {}", v.split(',').next().unwrap_or("").trim()), Some(couleurs::OK));
        } else {
            c.ligne("Puce TPM", "Non détectée", Some(couleurs::WARN));
        }
    }
    cartes.push(c);

    // --- Stabilité
    let bsod = champ_entier(data, "Bsod");
    let crash = champ_entier(data, "Crash");
    let whea = champ_entier(data, "WheaErr");
    let whea_ok = champ_entier(data, "WheaWarn");
    let allume = os
        .get("LastBootUpTime")
        .and_then(|d| d.as_str())
        .and_then(Moment::lire)
        .map(|d| maintenant.0 - d.0)
        .unwrap_or(maintenant.0);
    let mut c = Carte::nouvelle("SYS", "Stabilité du système", "Sur les 30 derniers jours");
    c.ligne(
        "Écrans bleus",
        &bsod.to_string(),
        Some(if bsod >= 2 {
            couleurs::BAD
        } else if bsod != 0 {
            couleurs::WARN
        } else {
            couleurs::OK
        }),
    );
    c.ligne("Arrêts brutaux", &crash.to_string(), Some(if crash != 0 { couleurs::WARN } else { couleurs::OK }));
    c.ligne("Erreurs matérielles", &whea.to_string(), Some(if whea != 0 { couleurs::BAD } else { couleurs::OK }));
    c.ligne("Erreurs corrigées", &whea_ok.to_string(), Some(if whea_ok >= 20 { couleurs::WARN } else { couleurs::OK }));
    c.ligne("Allumé depuis", &duree(allume), None);
    let fix_crash = Correction::conseil("Les plantages viennent le plus souvent d'un pilote défectueux, d'un réglage XMP / overclock trop agressif ou d'une surchauffe.",
        &["Clique sur « Historique » pour voir quand le PC a planté et quel programme ou pilote est en cause.", "Mets à jour Windows et le pilote de ta carte graphique.", "Si tu as activé XMP / EXPO, un overclock ou un undervolt, remets les réglages par défaut dans le BIOS pour tester.", "Surveille les températures en jeu avec HWiNFO: au-delà de 90 °C, nettoie le PC ou améliore le refroidissement."])
        .ouvre("run:perfmon.exe /rel", "Historique");
    if bsod >= 2 {
        c.note("bad", &format!("{bsod} écrans bleus ce mois ci. Causes fréquentes: pilote défectueux, XMP / overclock instable, surchauffe."));
        f.ajoute(
            "bad",
            &format!("{bsod} écrans bleus en 30 jours"),
            "Ton PC plante régulièrement.",
            3,
            "bsod",
            Some(fix_crash.clone()),
        );
    } else if bsod == 1 {
        c.note("warn", "Un écran bleu ce mois ci. Si ça se reproduit, regarde l'historique de fiabilité.");
        f.ajoute(
            "warn",
            "Un écran bleu en 30 jours",
            "Un plantage isolé n'est pas grave, mais surveille si ça se reproduit.",
            2,
            "bsod",
            Some(fix_crash.clone()),
        );
    }
    if whea != 0 {
        c.note("bad", &format!("{whea} erreur(s) matérielle(s) grave(s). Souvent un overclock, un undervolt ou un profil XMP instable, parfois une surchauffe."));
        f.ajoute("bad", "Erreurs matérielles détectées", &format!("Windows a enregistré {whea} erreur(s) matérielle(s) grave(s) ce mois ci. Souvent un overclock / undervolt ou un profil XMP instable."), 2, "whea", Some(fix_crash));
    } else if whea_ok >= 20 {
        c.note("warn", &format!("{whea_ok} erreurs matérielles corrigées automatiquement. Pas critique, mais souvent signe d'un réglage limite (overclock, undervolt, XMP)."));
    }
    if crash > bsod {
        c.note("info", "Arrêts brutaux: coupure de courant, bouton d'alimentation maintenu, ou PC qui s'éteint seul (alimentation, surchauffe) si ça arrive en jeu.");
    }
    if allume as f64 / 86400.0 >= 7.0 {
        c.note("info", "Pense à redémarrer de temps en temps: avec le démarrage rapide de Windows, « Arrêter » ne remet pas tout à zéro.");
    }
    c.action = Some("run:perfmon.exe /rel".into());
    c.libelle_action = Some("Historique de fiabilité".into());
    cartes.push(c);

    // --- Batterie
    if portable {
        let b = batteries.first().cloned().unwrap_or(Value::Null);
        let mut c = Carte::nouvelle("BAT", "Batterie", champ_texte(&b, "Name").trim());
        let cim = |classe: &str, champ: &str| -> Value {
            e.cim.get(classe).and_then(|x| x.get(champ)).cloned().unwrap_or(Value::Null)
        };
        let design = cim("BatteryStaticData", "DesignedCapacity");
        let plein = cim("BatteryFullChargedCapacity", "FullChargedCapacity");
        if net::vrai(&design) && net::vrai(&plein) {
            let (dz, pl) = (net::nombre_de(&design), net::nombre_de(&plein));
            let sante = (100.0 * pl / dz).min(100.0);
            c.barres.push(Barre {
                libelle: "Santé de la batterie".into(),
                valeur: sante,
                texte: format!("{} %", net::entier(sante)),
                couleur: if sante < 60.0 {
                    couleurs::BAD
                } else if sante < 80.0 {
                    couleurs::WARN
                } else {
                    couleurs::OK
                }
                .into(),
            });
            c.ligne("Capacité d'origine", &format!("{} mWh", n_fr(dz, 0)), None);
            c.ligne("Capacité actuelle", &format!("{} mWh", n_fr(pl, 0)), None);
            if sante < 60.0 {
                c.note("bad", "La batterie a perdu beaucoup de capacité: elle mérite d'être remplacée.");
            } else if sante < 80.0 {
                c.note("warn", "La batterie commence à s'user.");
            }
        }
        let cycles = cim("BatteryCycleCount", "CycleCount");
        if net::vrai(&cycles) {
            c.ligne("Cycles de charge", &net::texte(&cycles), None);
        }
        if b.get("EstimatedChargeRemaining").is_some_and(|x| !x.is_null()) {
            c.ligne(
                "Charge",
                &format!(
                    "{} %{}",
                    champ_texte(&b, "EstimatedChargeRemaining"),
                    if champ_entier(&b, "BatteryStatus") == 2 { ", sur secteur" } else { ", sur batterie" }
                ),
                None,
            );
        }
        cartes.push(c);
    }

    // --- Onduleur
    cartes_onduleur(pc, &onduleurs, &indices, data, &mut cartes, &mut f);

    // --- Réseau
    let reseau = data.get("Net").filter(|v| net::vrai(v)).cloned();
    if let Some(n) = &reseau {
        let wifi = champ_vrai(n, "Wifi");
        set_info(
            &mut info,
            "Réseau",
            format!("{}, {}", if wifi { "Wi-Fi" } else { "Ethernet (câble)" }, champ_texte(n, "Speed")),
        );
        let mut c = Carte::nouvelle("NET", "Réseau", &champ_texte(n, "Desc"));
        if wifi {
            c.ligne("Connexion", "Wi-Fi", Some(couleurs::WARN));
        } else {
            c.ligne("Connexion", "Câble Ethernet", Some(couleurs::OK));
        }
        c.ligne("Vitesse de la carte", &champ_texte(n, "Speed"), None);
        c.action = Some("tab:3".into());
        c.libelle_action = Some("Tester le ping".into());
        if wifi {
            c.note("info", "Le Wi-Fi ajoute de la latence et des pertes de paquets. Pour jouer en ligne, le câble reste le plus stable.");
            f.ajoute("warn", "Connexion en Wi-Fi", "Le Wi-Fi ajoute de la latence et des pertes de paquets, surtout loin de la box.", 1, "wifi", Some(
                Correction::conseil("En Wi-Fi, le ping varie et des paquets se perdent: ça se traduit par du lag et des tirs qui ne comptent pas.",
                    &["Si possible, branche un câble Ethernet entre le PC et la box (un long câble plat se cache facilement).", "Sinon, un adaptateur CPL (réseau par les prises électriques) est une bonne alternative.", "En Wi-Fi, connecte-toi au réseau 5 GHz ou 6 GHz de ta box plutôt qu'au 2,4 GHz, et rapproche le PC de la box."])
                .ouvre("tab:3", "Tester le ping")));
        } else {
            f.ajoute("ok", "Connexion par câble", "La connexion la plus stable pour jouer en ligne.", 1, "wifi", None);
        }
        cartes.push(c);
    }
    set_info(
        &mut info,
        "Windows",
        format!(
            "{} (build {})",
            MICROSOFT.replace_all(&champ_texte(&os, "Caption"), ""),
            champ_texte(&os, "BuildNumber")
        ),
    );
    set_info(&mut info, "Type", if portable { "PC portable" } else { "PC fixe" }.into());

    // --- Démarrage
    let actifs: Vec<&Demarrage> = e.demarrage.iter().filter(|d| d.actif).collect();
    let n = actifs.len();
    if n > 12 {
        let surs: Vec<&&Demarrage> = actifs
            .iter()
            .filter(|d| reglages::SANS_RISQUE.is_match(&d.nom) || reglages::SANS_RISQUE.is_match(&d.commande))
            .collect();
        let corr = if !surs.is_empty() {
            Correction {
                auto: true,
                quoi: vec![format!("Désactiver le lancement automatique de : {}.", surs.iter().map(|d| d.nom.clone()).collect::<Vec<_>>().join(", ")), "Ces programmes restent installés et se lancent normalement quand tu les ouvres.".into()],
                pourquoi: "Chaque programme au démarrage ralentit l'allumage du PC et occupe de la mémoire pendant tes parties. Pour les autres, choisis toi-même dans la page Démarrage (dans Performances).".into(),
                ..Default::default()
            }
            .changements(Changements::Liste(surs.iter().map(|d| reglages::changement_demarrage(d, false)).collect()))
            .ouvre("tab:2", "Démarrage")
        } else {
            Correction::conseil("Chaque programme au démarrage ralentit l'allumage du PC et occupe de la mémoire pendant tes parties.",
                &["Ouvre la page Démarrage (dans Performances).", "Désactive les programmes dont tu n'as pas besoin dès l'allumage (launchers, messageries, outils de mise à jour).", "Garde l'antivirus et les pilotes (audio, carte graphique, souris)."])
            .ouvre("tab:2", "Démarrage")
        };
        f.ajoute(
            "warn",
            &format!("{n} programmes se lancent au démarrage"),
            "Ils ralentissent l'allumage du PC et occupent de la mémoire pendant tes parties.",
            1,
            "startup",
            Some(corr),
        );
    } else {
        let mut k =
            Constat::nouveau("ok", &format!("{n} programmes au démarrage"), "Nombre raisonnable.", 1, "startup", None);
        k.action = "tab:2".into();
        k.libelle_action = "Voir".into();
        f.0.push(k);
    }

    // --- Sécurité (information seulement)
    let hvci = reglages::valeur(
        pc,
        "HKLM:\\SYSTEM\\CurrentControlSet\\Control\\DeviceGuard\\Scenarios\\HypervisorEnforcedCodeIntegrity",
        "Enabled",
    );
    if matches!(&hvci, Some(v) if !v.est_liste() && v.entier() == Some(1)) {
        f.ajoute("info", "Intégrité de la mémoire activée", "Cette protection de Windows bloque certains logiciels malveillants mais peut coûter quelques pourcents de FPS. Microsoft conseille de la laisser activée: Nevermind n'y touche pas, c'est à toi de décider.", 0, "hvci", Some(
            Correction::conseil("C'est un compromis entre sécurité et performances. Microsoft recommande de la laisser activée, et certains anti-triche l'exigent.",
                &["Si tu veux la désactiver: ouvre Sécurité Windows > Sécurité des appareils > Isolation du noyau.", "Coupe « Intégrité de la mémoire » et redémarre.", "Si un jeu ou un anti-triche la réclame, réactive-la au même endroit."])
            .ouvre("windowsdefender://coreisolation", "Isolation du noyau")));
    }

    // --- Réglages gaming
    for t in reglages::disponibles(build) {
        if reglages::test(pc, t.id, e.plan_cree.as_deref()) {
            f.ajoute("ok", t.titre, t.ok, reglages::poids(t), &format!("tweak:{}", t.id), None);
        } else {
            let st = if !t.conseille { "info" } else { "warn" };
            let corr = Correction {
                auto: true,
                quoi: vec![t.quoi.into()],
                pourquoi: t.description.into(),
                changements: reglages::changements(t.id),
                execution: Some(Execution::Reglage(t.id.into())),
                redemarrage: t.redemarrage,
                restauration: true,
                ..Default::default()
            }
            .ouvre("tab:1", "Optimisation gaming");
            f.ajoute(st, t.titre, t.ko, reglages::poids(t), &format!("tweak:{}", t.id), Some(corr));
        }
    }

    // --- Score
    let mut tous = f.0;
    let m = score::mesurer(&mut tous, &e.ignores);
    let ordre = |s: &str| match s {
        "bad" => 0,
        "warn" => 1,
        "info" => 2,
        "ok" => 3,
        _ => -1, // ($order[inconnu] : rien, avant tout le reste)
    };
    net::trier(&mut tous, |a, b| {
        ordre(&a.statut)
            .cmp(&ordre(&b.statut))
            .then((-(a.gain as i64)).cmp(&-(b.gain as i64)))
            .then((-a.poids).cmp(&-b.poids))
    });
    let ignore = |k: &Constat| e.ignores.iter().any(|x| x.eq_ignore_ascii_case(&k.id));
    let ignores: Vec<Constat> = tous.iter().filter(|k| ignore(k) && k.statut != "ok").cloned().collect();
    let actifs: Vec<Constat> = tous.into_iter().filter(|k| !ignore(k)).collect();
    let (appreciation, couleur) = score::apparence(m.0);
    Analyse {
        info,
        sante: sante(&cartes),
        cartes,
        actifs,
        ignores,
        score: m.0,
        potentiel: m.1,
        appreciation: appreciation.into(),
        couleur: couleur.into(),
        portable,
        onduleur: a_onduleur,
        build,
    }
}

/// `Show-HealthCards` : « 5 en bon état, 1 à surveiller »
fn sante(cartes: &[Carte]) -> String {
    let (mut ok, mut warn, mut bad) = (0, 0, 0);
    for c in cartes {
        match c.statut.as_str() {
            "warn" => warn += 1,
            "bad" => bad += 1,
            _ => ok += 1,
        }
    }
    let mut p = vec![format!("{ok} en bon état")];
    if warn > 0 {
        p.push(format!("{warn} à surveiller"));
    }
    if bad > 0 {
        p.push(format!("{bad} avec un problème"));
    }
    p.join(", ")
}

/// `Add-UpsCards`
fn cartes_onduleur(
    pc: &mut dyn Pc,
    onduleurs: &[Value],
    indices: &[Value],
    data: &Value,
    cartes: &mut Vec<Carte>,
    f: &mut Constats,
) {
    let p: Option<Batterie> = if !onduleurs.is_empty() { Some(reglages_batterie(pc)) } else { None };
    let hib = hibernation(pc);
    let conseil = p.as_ref().filter(|p| !p.is_empty()).map(|p| conseil_onduleur(p, hib));
    for u in onduleurs {
        let nom = champ_texte(u, "Name").trim().to_string();
        let w = data.get("BatWmi").cloned().unwrap_or(Value::Null);
        let mut c = Carte::nouvelle("UPS", "Onduleur", if nom.is_empty() { "Onduleur" } else { &nom });
        let sur_batterie = champ_entier(u, "BatteryStatus") == 1;
        if sur_batterie {
            c.ligne("Alimentation", "Sur batterie (coupure de courant)", Some(couleurs::BAD));
        } else {
            c.ligne("Alimentation", "Sur secteur", Some(couleurs::TEXTE));
        }
        if net::vrai(&w) && champ_vrai(&w, "Maker") {
            c.ligne("Fabricant", &champ_texte(&w, "Maker"), None);
        }
        if net::vrai(&w) && champ_vrai(&w, "Serial") {
            c.ligne("Numéro de série", &champ_texte(&w, "Serial"), None);
        }
        let ch = chimie(w.get("Chem").unwrap_or(&Value::Null), u.get("Chemistry").unwrap_or(&Value::Null));
        if !ch.is_empty() {
            c.ligne("Batterie", &ch, None);
        }
        if u.get("EstimatedChargeRemaining").is_some_and(|x| !x.is_null()) {
            let charge = champ_entier(u, "EstimatedChargeRemaining");
            c.barres.push(Barre {
                libelle: "Charge de la batterie".into(),
                valeur: charge as f64,
                texte: format!("{charge} %"),
                couleur: if charge < 30 {
                    couleurs::BAD
                } else if charge < 70 {
                    couleurs::WARN
                } else {
                    couleurs::OK
                }
                .into(),
            });
        }
        let mut sante: Option<f64> = None;
        if net::vrai(&w) && champ_nombre(&w, "Design") > 0.0 && champ_nombre(&w, "Full") > 0.0 {
            let h = net::arrondi(100.0 * champ_nombre(&w, "Full") / champ_nombre(&w, "Design")).min(100.0);
            sante = Some(h);
            c.barres.push(Barre {
                libelle: "Santé de la batterie".into(),
                valeur: h,
                texte: format!("{} %", net::texte_nombre(h)),
                couleur: if h < 40.0 {
                    couleurs::BAD
                } else if h < 60.0 {
                    couleurs::WARN
                } else {
                    couleurs::OK
                }
                .into(),
            });
        }
        let rt = champ_nombre(&w, "Runtime");
        let duree = if net::vrai(&w) && rt > 0.0 && rt < 360000.0 {
            net::entier(net::arrondi(rt / 60.0))
        } else {
            champ_entier(u, "EstimatedRunTime")
        };
        if duree > 0 && duree < 10000 {
            c.ligne("Autonomie estimée", &format!("{duree} min"), None);
        }
        if net::vrai(&w) && champ_nombre(&w, "Volt") > 0.0 {
            c.ligne("Tension", &format!("{} V", n_fr(champ_nombre(&w, "Volt") / 1000.0, 1)), None);
        }
        if let Some(p) = p.as_ref().filter(|p| !p.is_empty()) {
            c.ligne("Batterie faible", &texte_batterie(p, "LowLevel", "LowAction", "LowNotify"), None);
            let rouge = conseil.as_ref().is_some_and(|(_, fx)| fx.iter().any(|(k, _)| k == "CritAction"));
            c.ligne(
                "Batterie critique",
                &texte_batterie(p, "CritLevel", "CritAction", ""),
                Some(if rouge { couleurs::BAD } else { couleurs::TEXTE }),
            );
        }
        if sur_batterie {
            c.note(
                "bad",
                "Coupure de courant : ton PC tourne sur l'onduleur. Enregistre ton travail et quitte ta partie.",
            );
            f.ajoute("bad", "Coupure de courant", &format!("Ton PC tourne sur l'onduleur ({nom}){}.", if duree > 0 && duree < 10000 { format!(", environ {duree} minutes d'autonomie") } else { String::new() }), 1, "ups-battery", Some(
                Correction::conseil("Quand la batterie de l'onduleur sera vide, le PC s'éteindra d'un coup : les parties et documents non enregistrés seront perdus.",
                    &["Enregistre ce qui est ouvert et quitte ta partie.", "Éteins le PC proprement si le courant ne revient pas vite."])));
        }
        let problemes = conseil.as_ref().map(|(pr, _)| !pr.is_empty()).unwrap_or(false);
        if let Some((pr, fx)) = conseil.as_ref().filter(|(pr, _)| !pr.is_empty()) {
            let a = |k: &str| fx.iter().any(|(x, _)| x == k);
            let grave = a("CritAction");
            c.note(if grave { "bad" } else { "warn" }, &format!("Réglages à revoir : {}.", pr[0]));
            let mut quoi = Vec::new();
            if a("CritAction") {
                quoi.push("À batterie critique : arrêt propre du PC (tes fichiers sont enregistrés par Windows, rien n'est corrompu).".to_string());
            }
            if a("CritLevel") {
                quoi.push("Arrêt à 25 % de batterie, pour qu'il ait le temps de se faire.".into());
            }
            if a("LowLevel") || a("LowNotify") {
                quoi.push(
                    "Alerte à 50 % de batterie, pour que tu aies le temps d'enregistrer et de quitter ta partie."
                        .into(),
                );
            }
            let detail = pr
                .iter()
                .map(|x| {
                    let mut ch = x.chars();
                    match ch.next() {
                        Some(p) => p.to_uppercase().collect::<String>() + ch.as_str(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(". ");
            let corr = Correction {
                auto: true,
                quoi,
                pourquoi: "Pendant une coupure, l'onduleur ne tient que quelques minutes. Si Windows ne s'éteint pas proprement avant, le PC se coupe d'un coup : partie perdue, fichiers en cours d'écriture abîmés.".into(),
                fait: Some("L'onduleur éteindra le PC proprement avant d'être vide.".into()),
                ..Default::default()
            }
            .changements(Changements::Liste(fx.iter().map(|(k, v)| Declaration { sorte: "pcfg".into(), cle: k.clone(), nombre: *v, ..Default::default() }).collect()));
            f.ajoute(
                if grave { "bad" } else { "warn" },
                "Onduleur : arrêt automatique mal réglé",
                &format!("{detail}."),
                if grave { 2 } else { 1 },
                "ups-config",
                Some(corr),
            );
        }
        let usee = sante.is_some_and(|h| h < 60.0);
        if let Some(h) = sante.filter(|h| *h < 60.0) {
            let hs = net::texte_nombre(h);
            c.note(
                if h < 40.0 { "bad" } else { "warn" },
                &format!("Batterie usée ({hs} % de sa capacité d'origine) : pense à la remplacer."),
            );
            f.ajoute(if h < 40.0 { "bad" } else { "warn" }, "Batterie de l'onduleur usée", &format!("Elle ne garde plus que {hs} % de sa capacité d'origine : l'autonomie pendant une coupure est bien plus courte."), 1, "ups-health", Some(
                Correction::conseil("Les batteries au plomb des onduleurs s'usent en 3 à 5 ans, même sans coupure. Une batterie usée peut lâcher dès le début d'une coupure.",
                    &["Regarde la référence de la batterie sur l'étiquette de l'onduleur ou dans sa notice (chez APC par exemple : « RBC » suivi d'un numéro).", "Une batterie de remplacement coûte bien moins cher qu'un onduleur neuf, et se change souvent sans outil.", "Rapporte l'ancienne dans un point de collecte (magasin de bricolage, déchetterie)."])));
        } else if duree > 0 && duree < 5 && !sur_batterie {
            c.note(
                "warn",
                &format!("Autonomie courte ({duree} min) : l'onduleur est très chargé ou sa batterie fatigue."),
            );
            f.ajoute("warn", "Onduleur : autonomie très courte", &format!("Pendant une coupure, il ne tiendrait qu'environ {duree} minutes."), 1, "ups-runtime", Some(
                Correction::conseil("Plus on branche d'appareils sur l'onduleur, moins il tient. Un PC de jeu avec une grosse carte graphique consomme beaucoup.",
                    &["Ne branche sur les prises « batterie » de l'onduleur que le PC, l'écran et la box : pas l'imprimante, les enceintes ou un radiateur.", "Si ça ne change rien, sa batterie est peut-être usée : regarde sa date d'achat (3 à 5 ans de vie)."])));
        }
        if !sur_batterie && !problemes && !usee {
            c.note("ok", "Il protège ton PC des coupures et des surtensions, et Windows l'éteindra proprement avant qu'il soit vide.");
        }
        cartes.push(c);
    }
    if onduleurs.is_empty() && !indices.is_empty() {
        let usb: Vec<&Value> = indices.iter().filter(|h| champ_texte(h, "Kind") == "usb").collect();
        let mut logiciels: Vec<String> = Vec::new();
        for h in indices.iter().filter(|h| champ_texte(h, "Kind") != "usb") {
            let n = champ_texte(h, "Name");
            if !logiciels.contains(&n) {
                logiciels.push(n);
            }
        }
        let marque = usb.iter().find_map(|h| reglages::marque_usb(&champ_texte(h, "Vid")));
        static UPS: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)ups|onduleur"));
        let titre = if let Some(m) = marque.filter(|m| !m.contains("générique")) {
            format!("Onduleur {m}")
        } else if let Some(h) = usb.iter().find(|h| UPS.is_match(&champ_texte(h, "Name"))) {
            champ_texte(h, "Name")
        } else {
            "Onduleur".into()
        };
        let mut c = Carte::nouvelle("UPS", "Onduleur", &titre);
        let mut trouve = Vec::new();
        if !usb.is_empty() {
            trouve.push(format!("câble USB{}", marque.map(|m| format!(" ({m})")).unwrap_or_default()));
        }
        if !logiciels.is_empty() {
            trouve.push(format!("logiciel {}", logiciels.iter().take(2).cloned().collect::<Vec<_>>().join(", ")));
        }
        c.ligne("Repéré grâce à", &trouve.join(", "), None);
        c.ligne("État de la batterie", "Non transmis à Windows", Some("#A6A1BC"));
        let premier = logiciels.first().cloned().unwrap_or_default();
        c.note("ok", &format!("{} (charge, coupure de courant).", if !usb.is_empty() { "Il est branché au PC, mais il ne donne pas son état à Windows".to_string() } else { format!("{premier} est installé : un onduleur est sûrement relié à ce PC, mais il ne donne pas son état à Windows") }));
        c.note("warn", &if !logiciels.is_empty() { format!("C'est {premier} qui doit éteindre le PC pendant une coupure : ouvre-le et vérifie que l'arrêt automatique est activé (vers 25 % de batterie).") } else { "Pour que le PC s'éteigne proprement pendant une coupure, installe le logiciel de la marque de l'onduleur et active son arrêt automatique.".to_string() });
        cartes.push(c);
    }
}
