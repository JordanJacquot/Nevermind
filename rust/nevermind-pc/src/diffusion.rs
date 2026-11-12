//! Ce que le bloc donne aux autres blocs par signal (leurs pages montrent des données lues ici) :
//! - « reglages-jeu » (page Optimisation gaming, bloc perf : `Build-GamingTab`) ;
//! - « demarrage » (page Démarrage, bloc perf : `Update-StartupList`, `Get-StartupInfo`) et « demarrage-programmes »
//!   (bloc mémoire) ;
//! - « nettoyage » (page Nettoyage, bloc perf : `Invoke-CleanScan`) ;
//! - « analyse-pc » (bloc perf : constats, cartes graphiques, type de PC, disques, pilote NVIDIA, plan d'alimentation).

use crate::modele::Analyse;
use crate::net;
use crate::nettoyage::Cible;
use crate::pc::Pc;
use crate::reglages::{self, Demarrage};
use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;

/// L'état des cases des pages du bloc perf (les cases cochées : `$script:TweakRows`, `$script:CleanRows`)
#[derive(Debug, Clone, Default)]
pub struct Cases {
    /// (id du réglage, fait, coché)
    pub reglages: Vec<(String, bool, bool)>,
    pub restauration: bool,
    /// (catégorie, taille, nombre, cochée)
    pub nettoyage: Vec<(Cible, f64, i64, bool)>,
    pub nettoyage_lu: bool,
    /// Les entrées de démarrage dans l'ordre montré
    pub demarrage: Vec<(Demarrage, String, &'static str)>,
}

/// `Build-GamingTab`, la lecture : chaque réglage, fait ou pas (powercfg pour le plan d'alimentation : dans un travail
/// de fond, jamais sur le fil des blocs ; gels)
pub fn faits_reglages(pc: &mut dyn Pc, build: i64, plan_cree: Option<&str>) -> Vec<(String, bool)> {
    reglages::disponibles(build).into_iter().map(|t| (t.id.to_string(), reglages::test(pc, t.id, plan_cree))).collect()
}

/// `Build-GamingTab`, les cases : chaque réglage lu, coché par défaut s'il est conseillé (une case déjà cochée ou
/// décochée à la main le reste)
pub fn prendre_reglages(faits: Vec<(String, bool)>, c: &mut Cases) {
    let anciens = std::mem::take(&mut c.reglages);
    c.reglages = faits
        .into_iter()
        .map(|(id, ok)| {
            let coche = if ok {
                false
            } else {
                anciens
                    .iter()
                    .find(|(i, f, _)| *i == id && !f)
                    .map(|x| x.2)
                    .unwrap_or_else(|| reglages::reglage(&id).is_some_and(|t| t.conseille))
            };
            (id, ok, coche)
        })
        .collect();
}

pub fn signal_reglages(c: &Cases) -> Value {
    let lignes: Vec<Value> = c
        .reglages
        .iter()
        .filter_map(|(id, ok, coche)| {
            let t = reglages::reglage(id)?;
            Some(json!({ "id": id, "titre": t.titre, "description": t.description, "impact": t.impact, "recommande": t.conseille, "redemarrage": t.redemarrage, "active": !ok, "coche": coche, "fait": ok }))
        })
        .collect();
    json!({ "lignes": lignes, "restauration": c.restauration })
}

static SURS: LazyLock<&'static Regex> = LazyLock::new(|| &reglages::SANS_RISQUE);
static GARDER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(Realtek|RtkAud|NVIDIA|AMD|Radeon|Intel|SecurityHealth|Sécurité Windows|Windows Security|Defender|Avast|AVG|Kaspersky|Bitdefender|Norton|McAfee|ESET|Malwarebytes|Synaptics|ELAN|Dolby|Nahimic|Waves|MaxxAudio|Wacom|Bluetooth)\b").unwrap()
});
static APPAREILS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(Logitech|LGHUB|Razer|Corsair|iCUE|SteelSeries|HyperX|NGENUITY|Roccat|Glorious|Armoury|Aura|MSI Center|Mystic Light|Alienware|Stream Deck|Elgato)\b").unwrap()
});

/// `Get-StartupInfo` (le nom de l'entrée : le nom du produit lu dans l'exécutable n'est pas repris, voir DIVERGENCES.md)
pub fn conseil(d: &Demarrage) -> (&'static str, &'static str, &'static str) {
    let fichier = d.exe.rsplit(['\\', '/']).next().unwrap_or("");
    let nom =
        if fichier.to_lowercase().starts_with("epicgameslauncher") { "Epic Games Launcher" } else { d.nom.as_str() };
    let texte = format!("{nom} {} {fichier}", d.nom);
    if SURS.is_match(&texte) {
        ("safe", "Tu peux le désactiver", "Il se lance quand tu l'ouvres, pas besoin qu'il démarre avec Windows.")
    } else if GARDER.is_match(&texte) {
        ("keep", "À garder", "Pilote ou protection de ton PC : laisse-le activé.")
    } else if APPAREILS.is_match(&texte) {
        ("choice", "À toi de voir", "Garde-le si tu utilises les réglages de ta souris, ton clavier ou tes lumières.")
    } else {
        ("choice", "À toi de voir", "Désactive-le si tu ne t'en sers pas dès que tu allumes ton PC.")
    }
}

/// `Update-StartupList` : triées (activées d'abord, puis conseil, puis nom)
pub fn lire_demarrage(pc: &dyn Pc, brut: &[Demarrage], c: &mut Cases) {
    let mut l: Vec<(Demarrage, String, &'static str)> = brut
        .iter()
        .map(|d| {
            let mut x = d.clone();
            x.actif = reglages::demarrage_actif(pc, &d.approuve, &d.nom_valeur);
            let nom = if x.exe.to_lowercase().rsplit('\\').next().is_some_and(|f| f.starts_with("epicgameslauncher")) {
                "Epic Games Launcher".to_string()
            } else {
                x.nom.clone()
            };
            let g = conseil(&x).0;
            (x, nom, g)
        })
        .collect();
    let rang = |g: &str| match g {
        "safe" => 0,
        "choice" => 1,
        _ => 2,
    };
    net::trier(&mut l, |a, b| {
        (!a.0.actif).cmp(&!b.0.actif).then(rang(a.2).cmp(&rang(b.2))).then(a.1.to_lowercase().cmp(&b.1.to_lowercase()))
    });
    c.demarrage = l;
}

pub fn signal_demarrage(c: &Cases) -> Value {
    let on = c.demarrage.iter().filter(|x| x.0.actif).count();
    let texte = if on > 0 {
        format!(
            "{on} programme{} quand tu allumes ton PC. Moins il y en a, plus il démarre vite et plus il reste de mémoire pour tes jeux. Rien n'est supprimé : tu peux changer d'avis quand tu veux.",
            if on > 1 { "s se lancent" } else { " se lance" }
        )
    } else {
        "Aucun programme ne se lance quand tu allumes ton PC.".into()
    };
    let entrees: Vec<Value> = c
        .demarrage
        .iter()
        .map(|(d, nom, _)| {
            let (g, e, p) = conseil(d);
            json!({ "nom": nom, "actif": d.actif, "genre": g, "etiquette": e, "pourquoi": p, "image": null })
        })
        .collect();
    json!({ "entrees": entrees, "texte": texte })
}

pub fn signal_programmes(c: &Cases) -> Value {
    json!({ "programmes": c.demarrage.iter().map(|(d, nom, _)| json!({ "Nom": nom, "Commande": d.commande, "Exe": d.exe, "Enabled": d.actif, "Source": d.source })).collect::<Vec<_>>() })
}

/// `Invoke-CleanScan` : la taille de chaque catégorie (celles de Windows par l'Aide)
pub fn lire_nettoyage(pc: &mut dyn Pc, c: &mut Cases) {
    let anciens = c.nettoyage.clone();
    c.nettoyage = crate::nettoyage::cibles(pc)
        .into_iter()
        .filter(|t| !t.optionnelle || std::path::Path::new(&t.chemins[0]).exists())
        .map(|t| {
            let (taille, nombre) = if t.aide {
                match pc.aide(
                    "nettoyage.lister",
                    crate::pc::parametres(&[("Id", json!(t.id)), ("Top", json!(300))]),
                    120000,
                ) {
                    Ok(r) => (net::champ_nombre(&r, "Size"), net::champ_entier(&r, "Count")),
                    Err(_) => (0.0, 0),
                }
            } else {
                let r = crate::nettoyage::lister_chemins(&t.chemins, &t.exclus, std::time::SystemTime::now(), 0);
                (net::champ_nombre(&r, "Size"), net::champ_entier(&r, "Count"))
            };
            let choix = anciens.iter().find(|x| x.0.id == t.id).map(|x| x.3).unwrap_or(t.defaut);
            let coche = choix && nombre > 0;
            (t, taille, nombre, coche)
        })
        .collect();
    c.nettoyage_lu = true;
}

pub fn signal_nettoyage(c: &Cases) -> Value {
    let tout: f64 = c.nettoyage.iter().map(|x| x.1).sum();
    let choisi: f64 = c.nettoyage.iter().filter(|x| x.3).map(|x| x.1).sum();
    let texte = if tout == 0.0 {
        "Rien à nettoyer : ton PC est déjà propre.".to_string()
    } else {
        format!("{} à libérer sur {} possibles.", net::taille(choisi), net::taille(tout))
    };
    let bouton =
        if choisi > 0.0 { format!("Nettoyer {}", net::taille(choisi)) } else { "Nettoyer la sélection".into() };
    let lignes: Vec<Value> = c
        .nettoyage
        .iter()
        .map(|(t, taille, n, coche)| json!({ "id": t.id, "titre": t.titre, "desc": t.desc, "taille": taille, "nombre": n, "active": *n > 0, "coche": coche }))
        .collect();
    json!({ "lignes": lignes, "texte": texte, "bouton": bouton })
}

/// « analyse-pc » : ce dont le bloc perf a besoin de l'analyse
pub fn signal_analyse(a: &Analyse, data: &Value, plan_ok: bool) -> Value {
    let analyse: Vec<Value> = a.actifs.iter().map(|f| json!({ "id": f.id, "statut": f.statut, "titre": f.titre, "detail": f.detail, "gain": f.gain, "auto": f.auto() })).collect();
    let gpus: Vec<Value> = net::champ_liste(data, "GPUs").iter().map(|g| json!(net::champ_texte(g, "Name"))).collect();
    let disques: Vec<Value> = net::champ_liste(data, "Disks")
        .iter()
        .map(|d| {
            let k = d.get("Disk").cloned().unwrap_or(Value::Null);
            json!({ "lettres": d.get("Letters"), "bus": net::champ_texte(&k, "BusType"), "media": net::champ_texte(&k, "MediaType") })
        })
        .collect();
    let nv = net::champ_liste(data, "GPUs").into_iter().find(|g| {
        let n = net::champ_texte(g, "Name").to_lowercase();
        n.contains("nvidia") || n.contains("geforce")
    });
    let nvidia = nv.map(|g| {
        let derniere = data.get("NvLatest").filter(|x| net::vrai(x));
        json!({ "installe": crate::analyse::version_nvidia(&net::champ_texte(&g, "DriverVersion")), "dernier": derniere.map(|d| net::champ_texte(d, "Version")), "url": derniere.map(|d| net::champ_texte(d, "Url")) })
    });
    json!({ "analyse": analyse, "gpus": gpus, "portable": a.portable, "onduleur": a.onduleur, "disques": disques, "nvidia": nvidia, "planOk": plan_ok, "score": a.score })
}
