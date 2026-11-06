//! La page Sauvegarde : l'historique des changements (historique.json, `Import-History`), ce qui s'annule ou non
//! (`Test-HistoriqueDefinitif`, `Test-HistoriqueNote`), les détails techniques (`Get-DetailsTechniques`) et le nombre de
//! réglages gardés dans la sauvegarde (`Get-BackupCount`). Lecture seulement : l'annulation est faite par l'hôte.

use crate::ps;
use serde_json::{Map, Value};
use std::path::Path;

pub const HAUTE_PERF: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
pub const NORMAL: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
pub const MEILLEURES_PERF: &str = "ded574b5-45a0-4f42-8737-46345c09c238";
pub const OPTIMALES: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";
/// `$BatSettings` (dans l'ordre) et `$BatNoms`
const BATTERIE: [(&str, &str, &str); 5] = [
    ("CritAction", "637ea02f-bbcb-4015-8e2c-a1c7b9c0b546", "action à batterie critique"),
    ("CritLevel", "9a66d8d7-4ff7-4ef9-b5a2-5a326ca2a469", "niveau de batterie critique"),
    ("LowLevel", "8183ba9a-e910-48da-8769-14ae6dc1170a", "niveau de batterie faible"),
    ("LowNotify", "bcded951-187b-4d05-bccc-f7e51960c258", "alerte de batterie faible"),
    ("LowAction", "d8742dcb-3e6a-4b3c-b3fe-374623cdcf06", "action à batterie faible"),
];
const ACTIONS_BATTERIE: [&str; 4] = ["ne rien faire", "mise en veille", "mise en veille prolongée", "arrêt du PC"];

/// Une ligne de l'historique (après `Import-History`)
#[derive(Debug, Clone, PartialEq)]
pub struct Entree {
    pub id: String,
    pub date: String,
    pub titre: String,
    pub annule: bool,
    pub details: Vec<String>,
    /// Les changements, valeurs remises au bon type (ConvertFrom-HistoryEntry)
    pub log: Vec<Map<String, Value>>,
}

/// La clé d'une table de PowerShell (sans tenir compte des majuscules)
fn cle<'a>(o: &'a Map<String, Value>, k: &str) -> Option<&'a Value> {
    o.get(k).or_else(|| o.iter().find(|(c, _)| c.eq_ignore_ascii_case(k)).map(|x| x.1))
}
fn champ(o: &Map<String, Value>, k: &str) -> Value {
    cle(o, k).cloned().unwrap_or(Value::Null)
}
fn txt(o: &Map<String, Value>, k: &str) -> String {
    ps::texte(&champ(o, k))
}

/// `ConvertFrom-HistoryEntry` : `None` si une valeur ne se convertit pas (PowerShell s'arrête alors de lire l'historique)
fn convertir(e: &Map<String, Value>) -> Option<Map<String, Value>> {
    let mut h = e.clone();
    let sorte = txt(e, "Type");
    if ps::egal(&sorte, "reg") && ps::vrai(&champ(e, "Existed")) {
        let v = champ(e, "Value");
        let kind = txt(e, "Kind");
        let nv = match kind.to_lowercase().as_str() {
            "binary" => {
                let mut l = Vec::new();
                for x in ps::liste(&v) {
                    let n = ps::entier(&x)?;
                    if !(0..=255).contains(&n) {
                        return None;
                    }
                    l.push(Value::from(n));
                }
                Value::Array(l)
            }
            "dword" => {
                let n = ps::entier(&v)?;
                i32::try_from(n).ok()?;
                Value::from(n)
            }
            "qword" => Value::from(ps::entier(&v)?),
            "multistring" => Value::Array(ps::liste(&v).iter().map(|x| Value::String(ps::texte(x))).collect()),
            _ => Value::String(ps::texte(&v)),
        };
        h.insert("Value".into(), nv);
    }
    if ps::egal(&sorte, "dns") {
        let l = |v: &Value| Value::Array(ps::textes_vrais(v).into_iter().map(Value::String).collect());
        h.insert("Servers".into(), l(&champ(e, "Servers")));
        if cle(e, "Nouveau").is_some() {
            h.insert("Nouveau".into(), l(&champ(e, "Nouveau")));
        }
    }
    Some(h)
}

/// `Import-History` depuis le contenu de historique.json (déjà lu en JSON)
pub fn importer(json: Option<&Value>) -> Vec<Entree> {
    let mut r = Vec::new();
    let Some(j) = json.filter(|j| !j.is_null()) else { return r };
    for x in ps::liste(j) {
        if !ps::vrai(&x) {
            continue;
        }
        let Some(o) = x.as_object() else { continue };
        let mut log = Vec::new();
        for e in ps::liste(&champ(o, "Log")).iter().filter(|e| ps::vrai(e)) {
            let Some(eo) = e.as_object() else { continue };
            match convertir(eo) {
                Some(c) => log.push(c),
                // (une erreur arrête la lecture : les lignes déjà lues restent)
                None => return r,
            }
        }
        r.push(Entree {
            id: txt(o, "Id"),
            date: txt(o, "Date"),
            titre: txt(o, "Titre"),
            annule: ps::vrai(&champ(o, "Annule")),
            details: ps::textes_vrais(&champ(o, "Details")),
            log,
        });
    }
    r
}

/// Le JSON d'un fichier sûr (`Read-JsonSur`) : le fichier, sinon sa copie .bak ; rien s'il est vide ou abîmé
pub fn lire_json(fichier: &Path) -> Option<Value> {
    let bak = fichier.with_file_name(format!("{}.bak", fichier.file_name()?.to_string_lossy()));
    for f in [fichier.to_path_buf(), bak] {
        let Ok(t) = std::fs::read_to_string(&f) else { continue };
        let t = t.trim_start_matches('\u{feff}');
        if t.trim().is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(t) {
            return Some(v);
        }
    }
    None
}

impl Entree {
    fn types(&self) -> impl Iterator<Item = String> + '_ {
        self.log.iter().map(|e| txt(e, "Type"))
    }
    /// Seulement des changements définitifs
    pub fn definitif(&self) -> bool {
        !self.log.is_empty() && self.types().all(|t| ps::egal(&t, "definitif"))
    }
    /// Seulement noté (une appli fermée)
    pub fn note(&self) -> bool {
        !self.log.is_empty() && self.types().all(|t| ps::egal(&t, "note"))
    }
    /// Peut s'annuler (bouton « Annuler »)
    pub fn annulable(&self) -> bool {
        !self.annule && !self.definitif() && !self.note()
    }
    /// `Get-RetourDefinitif`
    pub fn retour(&self) -> String {
        let mut vus: Vec<String> = Vec::new();
        for e in &self.log {
            if ps::egal(&txt(e, "Type"), "definitif") && ps::vrai(&champ(e, "Retour")) {
                let r = txt(e, "Retour");
                if !vus.contains(&r) {
                    vus.push(r);
                }
            }
        }
        vus.join(" ")
    }
}

/// `Format-ValeurTech`
fn valeur_tech(v: &Value) -> String {
    match v {
        Value::Null => "vide".into(),
        Value::Array(a) => {
            let mut t = a.iter().take(16).map(ps::texte).collect::<Vec<_>>().join(",");
            if a.len() > 16 {
                t.push_str("...");
            }
            t
        }
        autre => {
            let t = ps::texte(autre);
            if t.chars().count() > 120 { format!("{}...", t.chars().take(117).collect::<String>()) } else { t }
        }
    }
}

fn demarrage_service(t: &str) -> String {
    match t.to_lowercase().as_str() {
        "automatic" => "automatique".into(),
        "manual" => "manuel".into(),
        "disabled" => "désactivé".into(),
        _ => t.into(),
    }
}

fn plan(guid: &str, nom: &str, cree: &str) -> String {
    let mut n = nom.to_string();
    if n.is_empty() {
        n = match guid.to_lowercase().as_str() {
            HAUTE_PERF => "Performances élevées",
            NORMAL => "Utilisation normale",
            OPTIMALES => "Performances optimales",
            _ => "",
        }
        .into();
        if n.is_empty() && !guid.is_empty() && ps::egal(guid, cree) {
            n = "copie de Performances élevées".into();
        }
    }
    if n.is_empty() { guid.into() } else { format!("« {n} » ({guid})") }
}

fn curseur(guid: &str) -> String {
    if ps::egal(guid, MEILLEURES_PERF) { format!("« Meilleures performances » ({guid})") } else { guid.into() }
}

fn serveurs(v: &Value) -> String {
    let l = ps::textes_vrais(v);
    if l.is_empty() { "automatique (box)".into() } else { l.join(", ") }
}

/// `Format-ValeurBat`
fn valeur_bat(k: &str, v: &Value) -> String {
    if v.is_null() {
        return "inconnu".into();
    }
    if ps::finit(k, "Action") {
        let a = ps::entier(v).and_then(|i| {
            let n = ACTIONS_BATTERIE.len() as i64;
            // (un indice négatif part de la fin, comme dans un tableau de PowerShell)
            let i = if i < 0 { i + n } else { i };
            (0..n).contains(&i).then(|| ACTIONS_BATTERIE[i as usize])
        });
        return a.map(str::to_string).unwrap_or_else(|| ps::texte(v));
    }
    if ps::egal(k, "LowNotify") {
        return if ps::entier(v) == Some(1) { "activée".into() } else { "coupée".into() };
    }
    format!("{} %", ps::texte(v))
}

/// `Get-DetailsTechniques` : `cree` = CreatedScheme de la sauvegarde
pub fn details_techniques(h: &Entree, cree: &str) -> Vec<String> {
    let mut r = Vec::new();
    for e in &h.log {
        let apres = cle(e, "Nouveau").is_some();
        let n = champ(e, "Nouveau");
        let t = |k: &str| txt(e, k);
        let ligne = match t("Type").to_lowercase().as_str() {
            "reg" => {
                let avant =
                    if ps::vrai(&champ(e, "Existed")) { valeur_tech(&champ(e, "Value")) } else { "absente".into() };
                let ap = if ps::vrai(&champ(e, "Efface")) {
                    "effacée".into()
                } else if apres {
                    valeur_tech(&n)
                } else {
                    "modifiée".into()
                };
                format!("Registre {} : valeur « {} », avant {avant}, après {ap}", t("Path"), t("Name"))
            }
            "svc" => format!(
                "Service {} : démarrage avant {}{}{}",
                t("Name"),
                demarrage_service(&t("StartType")),
                if ps::vrai(&champ(e, "Running")) { ", il tournait" } else { "" },
                if apres { format!(", après {}, arrêté", demarrage_service(&ps::texte(&n))) } else { String::new() }
            ),
            "power" => format!(
                "Plan d'alimentation : avant {}{}",
                plan(&t("Guid"), &t("Nom"), cree),
                if apres { format!(", après {}", plan(&ps::texte(&n), "", cree)) } else { String::new() }
            ),
            "pcfg" => {
                let k = if ps::vrai(&champ(e, "Key")) {
                    Some(t("Key"))
                } else {
                    BATTERIE.iter().find(|b| ps::egal(b.1, &t("Guid"))).map(|b| b.0.to_string())
                };
                match k.as_deref().and_then(|k| BATTERIE.iter().find(|b| ps::egal(b.0, k)).map(|b| (k, b.2))) {
                    Some((k, nom)) => format!(
                        "Réglage d'alimentation, {nom} : avant secteur {}, batterie {}{}",
                        valeur_bat(k, &champ(e, "Ac")),
                        valeur_bat(k, &champ(e, "Dc")),
                        if apres {
                            format!(", après {} (secteur et batterie)", valeur_bat(k, &n))
                        } else {
                            String::new()
                        }
                    ),
                    None => format!(
                        "Réglage d'alimentation {} : avant secteur {}, batterie {}{}",
                        t("Guid"),
                        t("Ac"),
                        t("Dc"),
                        if apres { format!(", après {} (secteur et batterie)", ps::texte(&n)) } else { String::new() }
                    ),
                }
            }
            "display" => format!(
                "Écran {} : fréquence avant {} Hz{}",
                t("Device"),
                t("Hz"),
                if apres { format!(", après {} Hz", ps::texte(&n)) } else { String::new() }
            ),
            "overlay" => format!(
                "Curseur du mode d'alimentation de Windows : avant {}{}",
                curseur(&t("Guid")),
                if apres { format!(", après {}", curseur(&ps::texte(&n))) } else { String::new() }
            ),
            "dns" => format!(
                "DNS de la carte réseau : avant {}{}",
                serveurs(&champ(e, "Servers")),
                if apres { format!(", après {}", serveurs(&n)) } else { String::new() }
            ),
            "fw" => format!(
                "Pare-feu : règle « {} », avant aucune règle, après Internet bloqué pour ce programme",
                t("Name")
            ),
            "definitif" => format!("{} (définitif)", t("Quoi")),
            "note" => {
                // (@($e.Chemins).Count : 1 quand il n'y en a pas, « @($null) » ; la liste vide seule vaut 0)
                let c = ps::liste(&champ(e, "Chemins"));
                if c.is_empty() {
                    t("Quoi")
                } else {
                    format!("{} : {}", t("Quoi"), c.iter().map(ps::texte).collect::<Vec<_>>().join(", "))
                }
            }
            _ => continue,
        };
        r.push(ligne);
    }
    r
}

/// Ce que garde la sauvegarde (sauvegarde.json) et `Get-BackupCount`
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sauvegarde {
    pub nombre: usize,
    pub plan_cree: String,
}

/// `$script:Backup` lu depuis sauvegarde.json (forme de Read-FichierSauvegarde), plus SvcOriginal et PcfgOriginal
pub fn sauvegarde(json: Option<&Value>, svc: Option<&Value>, pcfg: Option<&Value>) -> Sauvegarde {
    let mut n = 0usize;
    let mut cree = String::new();
    if let Some(o) = json.and_then(Value::as_object) {
        for k in ["Registry", "Dns", "Displays"] {
            n += cle(o, k).and_then(Value::as_object).map(Map::len).unwrap_or(0);
        }
        for k in ["PowerScheme", "Overlay"] {
            if cle(o, k).is_some_and(ps::vrai) {
                n += 1;
            }
        }
        cree = txt(o, "CreatedScheme");
    }
    let compte = |v: Option<&Value>| match v {
        None | Some(Value::Null) => 0,
        Some(Value::Array(a)) => a.len(),
        Some(_) => 1,
    };
    n += compte(svc) + compte(pcfg);
    Sauvegarde { nombre: n, plan_cree: cree }
}
