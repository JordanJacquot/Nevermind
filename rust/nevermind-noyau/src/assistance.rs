//! Historique des changements, signalement d'un problème, visite guidée, nouveautés et bulles pour Mindset
//! (modules\assistance.ps1, et la visite de l'écran façon console, accueil-console.ps1).
//!
//! - L'historique (historique.json) est partagé par tous les blocs : il vit dans le `Contexte` (`historique`,
//!   `ajouter_historique`, `marquer_historique_annule`...). Les pages qui le montrent (page Sauvegarde) sont celles
//!   du bloc Protection ; ici, les règles et les textes (`details_techniques`, `historique_definitif`...).
//! - La visite guidée, « Quoi de neuf » et « Signaler un problème » s'affichent dans la fiche et le panneau de
//!   l'écran : [`pages_assistance`] les donne (clés `fiche` et `panneau`, comme `Get-FicheKit` et `Get-PanneauKit`),
//!   et [`action`] reçoit leurs boutons.
//! - Les bulles d'une copie gérée par Mindset passent par notification.demande, une à la fois
//!   (`Send-NotifySuivante`) : [`Contexte::notifier`] et [`Contexte::envoyer_notification_suivante`].

use crate::aide::{texte_ps, vrai};
use crate::contexte::verrou;
use crate::{Contexte, Effet, Effets, moment, registre};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------------------------------------------
// L'état de l'assistance (fiche ou panneau ouvert, bulles en attente)
// ---------------------------------------------------------------------------------------------------------------

/// La fiche de l'assistance ouverte à l'écran
#[derive(Debug, Clone, PartialEq)]
pub enum Fiche {
    /// La visite guidée, à cette étape (0 = la première)
    Visite(usize),
    /// Une fiche de résultat (`Show-ResultSheet` sans changement à annuler) : titre, lignes, note
    Resultat { titre: String, lignes: Vec<String>, note: String },
}

/// Une bulle en attente pour Mindset
#[derive(Debug, Clone)]
struct Bulle {
    json: String,
    quand: Instant,
    titre: String,
}

#[derive(Debug, Default)]
pub struct EtatAssistance {
    fiche: Option<Fiche>,
    fiche_de: Option<String>,
    panneau: bool,
    panneau_de: Option<String>,
    ecran: String,
    bulles: Vec<Bulle>,
    derniere_bulle: Option<Instant>,
    /// « Signaler un problème » (la lecture de l'Aide, jusqu'à 30 s, puis le zip) : dans un travail de fond (audit G3)
    signalement: crate::travaux::Travaux<Result<std::path::PathBuf, String>>,
}

/// Les étapes de la visite guidée (celles de l'écran façon console, Initialize-AccueilConsole)
pub const VISITE: [(&str, &[&str]); 3] = [
    (
        "Bienvenue dans Nevermind",
        &[
            "Nevermind analyse ton PC et le règle pour que tes jeux tournent au mieux.",
            "Rien n'est modifié sans ton accord : chaque changement passe par un bouton sur lequel tu cliques.",
        ],
    ),
    (
        "L'accueil",
        &[
            "Les grandes tuiles : Ton PC et sa note sur 100, Performances, Protection, Jeux, Réseau, Trafic et Overlay.",
            "Fais glisser la rangée à la souris, ou utilise les flèches du clavier : un clic sur la tuile du milieu (ou Entrée) l'ouvre, Échap revient.",
            "En haut, le logo ramène à l'accueil ; les onglets mènent aux mêmes pages, et les petites icônes à droite : la loupe pour rechercher un réglage (ou Ctrl + K), Paramètres et Aide.",
            "Un souci ? « Signaler un problème » est dans l'Aide.",
        ],
    ),
    (
        "Tu peux revenir en arrière",
        &[
            "Chaque changement est sauvegardé avant d'être fait.",
            "Après une correction, « Revenir en arrière » annule tout de suite. Plus tard, la page Sauvegarde (dans Protection) garde l'historique : tu peux annuler chaque changement, sauf quatre protections du PC qui restent en place (marquées « Définitif »).",
            "Dans Ton PC, choisis un point à régler, lis la fiche, puis « Exécuter » pour que Nevermind le corrige.",
        ],
    ),
];

// ---------------------------------------------------------------------------------------------------------------
// L'historique des changements
// ---------------------------------------------------------------------------------------------------------------

fn liste(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.clone(),
        Value::Null => Vec::new(),
        autre => vec![autre.clone()],
    }
}

fn int_ps(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().unwrap_or_else(|| n.as_f64().map_or(0, |f| f.round_ties_even() as i64)),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        Value::Bool(b) => i64::from(*b),
        _ => 0,
    }
}

/// `ConvertFrom-HistoryEntry` : remet les valeurs lues dans le JSON au bon type pour le registre
pub fn entree_historique(e: &Value) -> Value {
    let Value::Object(m) = e else { return Value::Object(Map::new()) };
    let mut h = m.clone();
    if texte_ps(&h.get("Type").cloned().unwrap_or_default()) == "reg" && vrai(h.get("Existed").unwrap_or(&Value::Null))
    {
        let v = h.get("Value").cloned().unwrap_or_default();
        let nouveau = match texte_ps(h.get("Kind").unwrap_or(&Value::Null)).as_str() {
            "Binary" => json!(liste(&v).iter().map(|x| (int_ps(x) as u8) as i64).collect::<Vec<_>>()),
            "DWord" => json!(int_ps(&v) as i32),
            "QWord" => json!(int_ps(&v)),
            "MultiString" => json!(liste(&v).iter().map(texte_ps).collect::<Vec<_>>()),
            _ => json!(texte_ps(&v)),
        };
        h.insert("Value".into(), nouveau);
    }
    if texte_ps(h.get("Type").unwrap_or(&Value::Null)) == "dns" {
        let s: Vec<String> =
            liste(h.get("Servers").unwrap_or(&Value::Null)).iter().filter(|x| vrai(x)).map(texte_ps).collect();
        h.insert("Servers".into(), json!(s));
        if let Some(n) = h.get("Nouveau").cloned() {
            let n: Vec<String> = liste(&n).iter().filter(|x| vrai(x)).map(texte_ps).collect();
            h.insert("Nouveau".into(), json!(n));
        }
    }
    Value::Object(h)
}

/// Une ligne de l'historique telle que Nevermind la garde (`@{ Id; Date; Titre; Details; Log; Annule }`)
pub fn ligne_historique(x: &Value) -> Value {
    json!({
        "Id": texte_ps(&x["Id"]),
        "Date": texte_ps(&x["Date"]),
        "Titre": texte_ps(&x["Titre"]),
        "Details": liste(&x["Details"]).iter().filter(|d| vrai(d)).map(texte_ps).collect::<Vec<_>>(),
        "Log": liste(&x["Log"]).iter().filter(|d| vrai(d)).map(entree_historique).collect::<Vec<_>>(),
        "Annule": vrai(&x["Annule"]),
    })
}

/// Un identifiant de 12 caractères hexadécimaux (`[guid]::NewGuid().ToString('N').Substring(0, 12)`)
pub fn identifiant() -> String {
    use std::hash::{BuildHasher, Hasher};
    static COMPTEUR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(COMPTEUR.fetch_add(1, std::sync::atomic::Ordering::SeqCst));
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    h.write_u32(std::process::id());
    format!("{:016x}", h.finish())[..12].to_string()
}

/// `Test-HistoriqueDefinitif` : une entrée dont rien ne s'annule (seulement des changements définitifs)
pub fn historique_definitif(h: &Value) -> bool {
    let l = liste(&h["Log"]);
    !l.is_empty() && l.iter().all(|e| e["Type"].as_str() == Some("definitif"))
}

/// `Test-HistoriqueNote` : une entrée seulement notée, sans rien à remettre (une appli fermée : Type « note »)
pub fn historique_note(h: &Value) -> bool {
    let l = liste(&h["Log"]);
    !l.is_empty() && l.iter().all(|e| e["Type"].as_str() == Some("note"))
}

/// `Test-HistoriquePartDefinitive` : des changements définitifs et d'autres annulables
pub fn historique_part_definitive(h: &Value) -> bool {
    liste(&h["Log"]).iter().any(|e| e["Type"].as_str() == Some("definitif"))
}

/// `Get-RetourDefinitif` : la façon de revenir en arrière d'une entrée (changements définitifs)
pub fn retour_definitif(h: &Value) -> String {
    let mut vus: Vec<String> = Vec::new();
    for e in liste(&h["Log"]) {
        if e["Type"].as_str() == Some("definitif") && vrai(&e["Retour"]) {
            let r = texte_ps(&e["Retour"]);
            if !vus.contains(&r) {
                vus.push(r);
            }
        }
    }
    vus.join(" ")
}

/// `Get-HistoryItems` : les lignes « •  ... » d'une fiche de résultat
pub fn elements_historique(lignes: &[String]) -> Vec<String> {
    lignes
        .iter()
        .filter(|l| l.starts_with('•'))
        .map(|l| l.trim_start_matches('•').trim_start().trim().to_string())
        .collect()
}

/// `Get-HistoryTitle` : le titre lisible d'une fiche de résultat
pub fn titre_historique(titre: &str, lignes: &[String]) -> String {
    let e = elements_historique(lignes);
    match e.len() {
        0 => titre.to_string(),
        1 => e[0].clone(),
        n => format!(
            "{n} changements : {}{}",
            e.iter().take(3).cloned().collect::<Vec<_>>().join(", "),
            if n > 3 { "..." } else { "" }
        ),
    }
}

/// Les plans d'alimentation connus (optimisations.ps1)
pub const PLAN_PERFORMANCES: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
pub const PLAN_NORMAL: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
pub const PLAN_OPTIMAL: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";
pub const CURSEUR_PERFORMANCES: &str = "ded574b5-45a0-4f42-8737-46345c09c238";
/// Les réglages « batterie » (`$BatSettings`, analyse.ps1) et leur nom (`$BatNoms`, tableau-de-bord.ps1)
pub const REGLAGES_BATTERIE: [(&str, &str, &str); 5] = [
    ("CritAction", "637ea02f-bbcb-4015-8e2c-a1c7b9c0b546", "action à batterie critique"),
    ("CritLevel", "9a66d8d7-4ff7-4ef9-b5a2-5a326ca2a469", "niveau de batterie critique"),
    ("LowLevel", "8183ba9a-e910-48da-8769-14ae6dc1170a", "niveau de batterie faible"),
    ("LowNotify", "bcded951-187b-4d05-bccc-f7e51960c258", "alerte de batterie faible"),
    ("LowAction", "d8742dcb-3e6a-4b3c-b3fe-374623cdcf06", "action à batterie faible"),
];
const ACTIONS_BATTERIE: [&str; 4] = ["ne rien faire", "mise en veille", "mise en veille prolongée", "arrêt du PC"];

/// `Format-ValeurTech`
pub fn valeur_tech(v: &Value) -> String {
    match v {
        Value::Null => "vide".into(),
        Value::Array(a) => {
            let mut t = a.iter().take(16).map(texte_ps).collect::<Vec<_>>().join(",");
            if a.len() > 16 {
                t.push_str("...");
            }
            t
        }
        autre => {
            let t = texte_ps(autre);
            if t.chars().count() > 120 { format!("{}...", t.chars().take(117).collect::<String>()) } else { t }
        }
    }
}

fn demarrage_service(t: &str) -> String {
    match t {
        "Automatic" => "automatique".into(),
        "Manual" => "manuel".into(),
        "Disabled" => "désactivé".into(),
        autre => autre.into(),
    }
}

fn plan_alim(guid: &str, nom: &str, plan_cree: Option<&str>) -> String {
    let mut n = nom.to_string();
    if n.is_empty() {
        n = match guid {
            PLAN_PERFORMANCES => "Performances élevées".into(),
            PLAN_NORMAL => "Utilisation normale".into(),
            PLAN_OPTIMAL => "Performances optimales".into(),
            _ => String::new(),
        };
        if n.is_empty() && !guid.is_empty() && Some(guid) == plan_cree {
            n = "copie de Performances élevées".into();
        }
    }
    if n.is_empty() { guid.to_string() } else { format!("« {n} » ({guid})") }
}

fn curseur_alim(guid: &str) -> String {
    if guid == CURSEUR_PERFORMANCES { format!("« Meilleures performances » ({guid})") } else { guid.to_string() }
}

fn serveurs_dns(s: &Value) -> String {
    let l: Vec<String> = liste(s).iter().filter(|x| vrai(x)).map(texte_ps).collect();
    if l.is_empty() { "automatique (box)".into() } else { l.join(", ") }
}

/// `Format-ValeurBat`
pub fn valeur_batterie(cle: &str, v: &Value) -> String {
    if v.is_null() {
        return "inconnu".into();
    }
    if cle.ends_with("Action") {
        let i = int_ps(v);
        if (0..4).contains(&i) {
            return ACTIONS_BATTERIE[i as usize].into();
        }
        return texte_ps(v);
    }
    if cle == "LowNotify" {
        return if int_ps(v) == 1 { "activée".into() } else { "coupée".into() };
    }
    format!("{} %", texte_ps(v))
}

/// `Get-DetailsTechniques` : ce qui a été changé exactement, avant et après (issue #7). `plan_cree` : le plan
/// créé par Nevermind (`$script:Backup.CreatedScheme`).
pub fn details_techniques(h: &Value, plan_cree: Option<&str>) -> Vec<String> {
    let mut r = Vec::new();
    for e in liste(&h["Log"]) {
        let apres = e.get("Nouveau").is_some();
        let s = |n: &str| texte_ps(&e[n]);
        let ligne = match s("Type").as_str() {
            "reg" => {
                let avant = if vrai(&e["Existed"]) { valeur_tech(&e["Value"]) } else { "absente".into() };
                let ap = if vrai(&e["Efface"]) {
                    "effacée".into()
                } else if apres {
                    valeur_tech(&e["Nouveau"])
                } else {
                    "modifiée".into()
                };
                format!("Registre {} : valeur « {} », avant {avant}, après {ap}", s("Path"), s("Name"))
            }
            "svc" => format!(
                "Service {} : démarrage avant {}{}{}",
                s("Name"),
                demarrage_service(&s("StartType")),
                if vrai(&e["Running"]) { ", il tournait" } else { "" },
                if apres { format!(", après {}, arrêté", demarrage_service(&s("Nouveau"))) } else { String::new() }
            ),
            "power" => format!(
                "Plan d'alimentation : avant {}{}",
                plan_alim(&s("Guid"), &s("Nom"), plan_cree),
                if apres { format!(", après {}", plan_alim(&s("Nouveau"), "", plan_cree)) } else { String::new() }
            ),
            "pcfg" => {
                let k = if vrai(&e["Key"]) {
                    s("Key")
                } else {
                    REGLAGES_BATTERIE.iter().find(|b| b.1 == s("Guid")).map(|b| b.0.to_string()).unwrap_or_default()
                };
                match REGLAGES_BATTERIE.iter().find(|b| b.0 == k) {
                    Some(b) => format!(
                        "Réglage d'alimentation, {} : avant secteur {}, batterie {}{}",
                        b.2,
                        valeur_batterie(&k, &e["Ac"]),
                        valeur_batterie(&k, &e["Dc"]),
                        if apres {
                            format!(", après {} (secteur et batterie)", valeur_batterie(&k, &e["Nouveau"]))
                        } else {
                            String::new()
                        }
                    ),
                    None => format!(
                        "Réglage d'alimentation {} : avant secteur {}, batterie {}{}",
                        s("Guid"),
                        s("Ac"),
                        s("Dc"),
                        if apres { format!(", après {} (secteur et batterie)", s("Nouveau")) } else { String::new() }
                    ),
                }
            }
            "display" => format!(
                "Écran {} : fréquence avant {} Hz{}",
                s("Device"),
                s("Hz"),
                if apres { format!(", après {} Hz", s("Nouveau")) } else { String::new() }
            ),
            "overlay" => format!(
                "Curseur du mode d'alimentation de Windows : avant {}{}",
                curseur_alim(&s("Guid")),
                if apres { format!(", après {}", curseur_alim(&s("Nouveau"))) } else { String::new() }
            ),
            "dns" => format!(
                "DNS de la carte réseau : avant {}{}",
                serveurs_dns(&e["Servers"]),
                if apres { format!(", après {}", serveurs_dns(&e["Nouveau"])) } else { String::new() }
            ),
            "fw" => format!(
                "Pare-feu : règle « {} », avant aucune règle, après Internet bloqué pour ce programme",
                s("Name")
            ),
            "definitif" => format!("{} (définitif)", s("Quoi")),
            "note" => {
                // (comme `@($e.Chemins).Count` : sans liste, `@($null)` compte un élément vide, d'où « : » seul)
                let c: Vec<String> = match &e["Chemins"] {
                    Value::Array(a) => a.iter().map(texte_ps).collect(),
                    autre => vec![texte_ps(autre)],
                };
                format!("{}{}", s("Quoi"), if c.is_empty() { String::new() } else { format!(" : {}", c.join(", ")) })
            }
            _ => continue,
        };
        r.push(ligne);
    }
    r
}

impl Contexte {
    /// Le fichier de l'historique
    pub fn fichier_historique(&self) -> PathBuf {
        self.donnees.join("historique.json")
    }

    /// `Import-History` : relit historique.json
    pub fn importer_historique(&self) {
        let v = self.lire_json(&self.fichier_historique());
        self.fournir_historique(v);
    }

    /// `Import-History` sur le contenu de historique.json lu ailleurs (lot C : le désinstalleur en administrateur le
    /// lit lui-même, sans suivre de lien) ; `None` s'il n'y en a pas. Rien n'est lu ni écrit ici.
    pub fn fournir_historique(&self, contenu: Option<Value>) {
        let l = match contenu {
            None => Vec::new(),
            Some(v) => liste(&v).iter().filter(|x| vrai(x)).map(ligne_historique).collect(),
        };
        *verrou(&self.historique) = Some(l);
    }

    /// `$script:History` : les entrées de l'historique, la plus récente d'abord (importées au premier appel)
    pub fn historique(&self) -> Vec<Value> {
        if verrou(&self.historique).is_none() {
            self.importer_historique();
        }
        verrou(&self.historique).clone().unwrap_or_default()
    }

    /// `Save-History` : écrit historique.json (une erreur est notée au journal)
    pub fn enregistrer_historique(&self) {
        let l = self.historique();
        let sortie: Vec<Value> = l
            .iter()
            .map(|h| {
                let mut o = Map::new();
                for k in ["Id", "Date", "Titre", "Details", "Log", "Annule"] {
                    o.insert(k.into(), h.get(k).cloned().unwrap_or(Value::Null));
                }
                Value::Object(o)
            })
            .collect();
        if let Err(e) = self.ecrire_json(&self.fichier_historique(), &Value::Array(sortie), false, false) {
            self.journal(&format!("Écriture de l'historique impossible: {e}"));
        }
    }

    /// `Add-History` : ajoute une ligne (seulement si quelque chose a vraiment changé) ; rend son identifiant.
    /// Les 100 plus récentes sont gardées.
    pub fn ajouter_historique(&self, titre: &str, details: &[String], log: Vec<Value>) -> Option<String> {
        if log.is_empty() {
            return None;
        }
        let _ = self.historique();
        let id = identifiant();
        let date = moment::date_historique(&self.maintenant());
        {
            let mut g = verrou(&self.historique);
            let h = g.get_or_insert_with(Vec::new);
            h.insert(
                0,
                json!({ "Id": id, "Date": date, "Titre": titre, "Details": details.iter().filter(|d| !d.is_empty()).collect::<Vec<_>>(), "Log": log, "Annule": false }),
            );
            h.truncate(100);
        }
        self.enregistrer_historique();
        Some(id)
    }

    /// `Add-ChangementDefinitif` : un changement que « Tout restaurer » n'annule pas (il protège le PC), noté dans
    /// les changements de l'action en cours, ou directement dans l'historique, avec la façon de revenir en arrière
    pub fn ajouter_changement_definitif(&self, titre: &str, quoi: &str, retour: &str) {
        let e = json!({ "Type": "definitif", "Quoi": quoi, "Retour": retour });
        if !self.noter_changement(e.clone()) {
            let _ = self.ajouter_historique(titre, &[quoi.to_string()], vec![e]);
        }
    }

    /// `Set-HistoryUndone` : cette entrée est annulée
    pub fn marquer_historique_annule(&self, id: &str) {
        if id.is_empty() || verrou(&self.historique).is_none() {
            return;
        }
        if let Some(h) = verrou(&self.historique).as_mut() {
            for e in h.iter_mut() {
                if e["Id"].as_str() == Some(id) {
                    e["Annule"] = json!(true);
                }
            }
        }
        self.enregistrer_historique();
    }

    /// `Test-HistoriqueRemis` : plus rien de ce que l'entrée a changé n'attend dans la sauvegarde.
    /// `pare_feu_echoue` : la remise du pare-feu a échoué (`$script:RestaurationPareFeu -eq $false`).
    pub fn historique_remis(&self, h: &Value, pare_feu_echoue: bool) -> bool {
        let s = self.sauvegarde();
        let reg_reste: std::collections::HashSet<String> = s
            .registre
            .values()
            .map(|e| format!("{}|{}", registre::cle_normale(&texte_ps(&e["Path"])), texte_ps(&e["Name"])))
            .collect();
        let premier = |v: &Value| -> Vec<String> {
            liste(v).iter().map(|x| texte_ps(x).split('|').next().unwrap_or("").to_string()).collect()
        };
        let svc = premier(&self.reglage_ou("SvcOriginal", json!([])));
        let pcfg = premier(&self.reglage_ou("PcfgOriginal", json!([])));
        for e in liste(&h["Log"]) {
            if !vrai(&e) {
                continue;
            }
            let reste = match texte_ps(&e["Type"]).as_str() {
                "reg" => reg_reste.contains(&format!(
                    "{}|{}",
                    registre::cle_normale(&texte_ps(&e["Path"])),
                    texte_ps(&e["Name"])
                )),
                "svc" => svc.iter().any(|x| x.eq_ignore_ascii_case(&texte_ps(&e["Name"]))),
                "pcfg" => pcfg.iter().any(|x| {
                    !x.is_empty()
                        && (x.eq_ignore_ascii_case(&texte_ps(&e["Guid"]))
                            || x.eq_ignore_ascii_case(&texte_ps(&e["Key"])))
                }),
                "power" => s.plan.as_deref().is_some_and(|p| !p.is_empty()),
                "overlay" => s.curseur.as_deref().is_some_and(|p| !p.is_empty()),
                "dns" => !s.dns.is_empty(),
                "display" => s.ecrans.contains_key(&texte_ps(&e["Device"])),
                "fw" => pare_feu_echoue,
                _ => false,
            };
            if reste {
                return false;
            }
        }
        true
    }

    /// `Set-HistoryAllUndone` : après « Tout restaurer », les entrées entièrement remises sont annulées (pas celles
    /// qui n'ont que des changements définitifs ou des notes)
    pub fn marquer_historique_tout_annule(&self, pare_feu_echoue: bool) {
        let l = self.historique();
        let a_annuler: Vec<bool> = l
            .iter()
            .map(|h| {
                !vrai(&h["Annule"])
                    && !historique_definitif(h)
                    && !historique_note(h)
                    && self.historique_remis(h, pare_feu_echoue)
            })
            .collect();
        if let Some(h) = verrou(&self.historique).as_mut() {
            for (e, a) in h.iter_mut().zip(a_annuler) {
                if a {
                    e["Annule"] = json!(true);
                }
            }
        }
        self.enregistrer_historique();
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Visite guidée, « Quoi de neuf », bienvenue
// ---------------------------------------------------------------------------------------------------------------

impl Contexte {
    /// `Show-Tour` : la visite guidée commence (et ne sera plus proposée au lancement)
    pub fn montrer_visite(&self, effets: &mut Effets) {
        let _ = self.ecrire_reglage("TourDone", json!(true), false);
        self.ouvrir_fiche(Fiche::Visite(0), effets);
    }

    /// Une fiche de résultat sans changement à annuler (`Show-ResultSheet $Titre $Lignes $null $Note`)
    pub fn montrer_resultat(&self, titre: &str, lignes: Vec<String>, note: &str, effets: &mut Effets) {
        self.ouvrir_fiche(Fiche::Resultat { titre: titre.into(), lignes, note: note.into() }, effets);
    }

    fn ouvrir_fiche(&self, f: Fiche, effets: &mut Effets) {
        let mut a = verrou(&self.assistance);
        if a.fiche.is_none() {
            a.fiche_de = Some(if a.ecran.is_empty() { "accueil".into() } else { a.ecran.clone() });
        }
        a.fiche = Some(f);
        drop(a);
        effets.ajouter(Effet::Rafraichir);
        effets.ajouter(Effet::OuvrirEcran("fiche".into()));
    }

    fn fermer_fiche(&self, effets: &mut Effets) {
        let mut a = verrou(&self.assistance);
        a.fiche = None;
        let de = a.fiche_de.take().filter(|d| d != "fiche").unwrap_or_else(|| "accueil".into());
        drop(a);
        effets.ajouter(Effet::OuvrirEcran(de));
        effets.ajouter(Effet::Rafraichir);
    }

    /// La fiche de l'assistance ouverte (s'il y en a une)
    pub fn fiche_assistance(&self) -> Option<Fiche> {
        verrou(&self.assistance).fiche.clone()
    }

    /// `Invoke-WelcomeChecks` : au lancement, la visite guidée la première fois, « Quoi de neuf » après une mise à
    /// jour. Un ancien utilisateur (plus d'un démarrage au journal) n'a pas la visite, mais les nouveautés.
    pub fn verifier_bienvenue(&self, transport: &dyn crate::mises_a_jour::Transport, effets: &mut Effets) {
        let mut last = texte_ps(&self.reglage_ou("LastVersion", json!("")));
        let mut tour = self.reglage_bool("TourDone", false);
        if last.is_empty() && !tour && demarrages_au_journal(&self.fichier_journal()) > 1 {
            let _ = self.ecrire_reglage("TourDone", json!(true), false);
            tour = true;
            last = "précédente".into();
        }
        if !tour {
            let _ = self.ecrire_reglage("LastVersion", json!(self.version()), false);
            self.montrer_visite(effets);
            return;
        }
        self.montrer_nouveautes(&last, transport, effets);
    }

    /// `Show-WhatsNew` : les nouveautés de la version installée (lues sur GitHub, sauf si c'est coupé)
    pub fn montrer_nouveautes(&self, last: &str, transport: &dyn crate::mises_a_jour::Transport, effets: &mut Effets) {
        let v = self.version();
        let _ = self.ecrire_reglage("LastVersion", json!(v), false);
        if last.is_empty() || last == v {
            return;
        }
        let en_ligne = self.reglage_bool("NouveautesEnLigne", true);
        let notes = if en_ligne { crate::mises_a_jour::notes_version(transport, &v) } else { String::new() };
        let lignes = lignes_nouveautes(&notes, en_ligne);
        let note = if last == "précédente" {
            "Nevermind vient d'être mis à jour.".to_string()
        } else {
            format!("Tu avais la version {last}.")
        };
        self.montrer_resultat(&format!("Quoi de neuf dans la version {v}"), lignes, &note, effets);
    }
}

/// Le nombre de lignes « Démarrage (OptiGame|Nexo|Nevermind) » du journal (`Select-String`, sans tenir compte des
/// majuscules)
pub fn demarrages_au_journal(fichier: &Path) -> usize {
    let Ok(o) = std::fs::read(fichier) else { return 0 };
    let t = suite_commun::json_sur::texte_du_fichier(&o).to_lowercase();
    crate::journal::lignes(&t)
        .iter()
        .filter(|l| ["démarrage optigame", "démarrage nexo", "démarrage nevermind"].iter().any(|m| l.contains(m)))
        .count()
}

/// Les lignes de « Quoi de neuf » : chaque ligne des notes, « - » ou « * » en tête devient « •  »
pub fn lignes_nouveautes(notes: &str, en_ligne: bool) -> Vec<String> {
    if !en_ligne {
        return vec![
            "Le détail des nouveautés n'est pas lu sur Internet : c'est coupé dans Paramètres, Mises à jour.".into(),
        ];
    }
    let mut l: Vec<String> = Vec::new();
    for x in notes.split('\n') {
        let x = x.strip_suffix('\r').unwrap_or(x).trim();
        let mut t = x.to_string();
        if (x.starts_with('-') || x.starts_with('*')) && x[1..].starts_with(char::is_whitespace) {
            t = format!("•  {}", x[1..].trim_start());
        }
        if !t.is_empty() {
            l.push(t);
        }
    }
    if l.is_empty() {
        l.push("Corrections et améliorations.".into());
    }
    l
}

// ---------------------------------------------------------------------------------------------------------------
// Les pages : la fiche (visite, nouveautés) et le panneau « Signaler un problème »
// ---------------------------------------------------------------------------------------------------------------

/// Le panneau « Signaler un problème » (`Show-ReportPanel`) s'ouvre
impl Contexte {
    pub fn montrer_signalement(&self, effets: &mut Effets) {
        let mut a = verrou(&self.assistance);
        if !a.panneau {
            a.panneau_de = Some(if a.ecran.is_empty() { "accueil".into() } else { a.ecran.clone() });
        }
        a.panneau = true;
        drop(a);
        effets.ajouter(Effet::Rafraichir);
        effets.ajouter(Effet::OuvrirEcran("panneau".into()));
    }

    fn fermer_signalement(&self, effets: &mut Effets) {
        let mut a = verrou(&self.assistance);
        a.panneau = false;
        let de = a.panneau_de.take().filter(|d| d != "panneau" && d != "fiche").unwrap_or_else(|| "accueil".into());
        drop(a);
        effets.ajouter(Effet::OuvrirEcran(de));
        effets.ajouter(Effet::Rafraichir);
    }
}

fn parent(de: &Option<String>, exclus: &[&str]) -> Value {
    match de {
        Some(d) if !exclus.contains(&d.as_str()) => json!(d),
        _ => Value::Null,
    }
}

/// Les blocs d'une fiche à partir de ses lignes (`Get-FicheKit` : « 1. » en étapes, « • » en liste, le reste en
/// texte), puis la note (encadré)
fn blocs_lignes(lignes: &[String]) -> Vec<Value> {
    let mut blocs = Vec::new();
    let mut l: Vec<String> = Vec::new();
    let mut sorte = "";
    let finir = |blocs: &mut Vec<Value>, l: &mut Vec<String>, sorte: &str| {
        if !l.is_empty() {
            blocs.push(if sorte == "etapes" {
                json!({ "type": "etapes", "elements": l.iter().map(|t| json!({ "texte": t })).collect::<Vec<_>>() })
            } else {
                json!({ "type": "liste", "elements": l })
            });
            l.clear();
        }
    };
    for t in lignes {
        let etape = t
            .find(|c: char| !c.is_ascii_digit())
            .filter(|&i| i > 0 && t[i..].starts_with('.') && t[i + 1..].starts_with(char::is_whitespace));
        if let Some(i) = etape {
            if sorte != "etapes" {
                finir(&mut blocs, &mut l, sorte);
            }
            sorte = "etapes";
            l.push(t[i + 1..].trim_start().to_string());
            continue;
        }
        if t.starts_with('•') && t['•'.len_utf8()..].starts_with(char::is_whitespace) {
            if sorte != "liste" {
                finir(&mut blocs, &mut l, sorte);
            }
            sorte = "liste";
            l.push(t['•'.len_utf8()..].trim_start().to_string());
            continue;
        }
        finir(&mut blocs, &mut l, sorte);
        blocs.push(json!({ "type": "texte", "texte": t }));
    }
    finir(&mut blocs, &mut l, sorte);
    blocs
}

/// La fiche de l'écran (`Get-FicheKit`) pour une fiche de l'assistance. `de` : la page d'où elle a été ouverte.
pub fn page_fiche(f: &Fiche, de: &Option<String>) -> Value {
    match f {
        Fiche::Visite(i) => {
            let i = (*i).min(VISITE.len() - 1);
            let (titre, lignes) = VISITE[i];
            let derniere = i == VISITE.len() - 1;
            let mut blocs: Vec<Value> = lignes.iter().map(|l| json!({ "type": "texte", "texte": l })).collect();
            if !derniere {
                blocs.push(json!({ "type": "bouton", "id": "fiche:close", "texte": "Passer", "style": "second" }));
            }
            json!({
                "titre": titre, "sous": format!("Étape {} sur {}", i + 1, VISITE.len()), "icone": "etoile", "couleur": "P",
                "retour": "appli", "parent": parent(de, &["accueil", "fiche"]),
                "module": { "icone": "etoile", "couleur": "P", "titre": "", "texte": "",
                    "boutons": [{ "id": "fiche:run", "texte": if derniere { "C'est parti" } else { "Suivant" } }] },
                "blocs": blocs,
            })
        }
        Fiche::Resultat { titre, lignes, note } => {
            let mut blocs = blocs_lignes(lignes);
            if !note.is_empty() {
                blocs.push(json!({ "type": "note", "texte": note }));
            }
            blocs.push(json!({ "type": "bouton", "id": "fiche:close", "texte": "OK", "style": "second" }));
            json!({
                "titre": if titre.is_empty() { "Nevermind" } else { titre }, "sous": "", "icone": "ok", "couleur": "vert",
                "retour": "appli", "parent": parent(de, &["accueil", "fiche"]),
                "module": { "icone": "ok", "couleur": "vert", "titre": "", "texte": "", "boutons": [] },
                "blocs": blocs,
            })
        }
    }
}

/// Le texte du panneau « Signaler un problème »
pub const SIGNALEMENT_EXPLICATION: &str =
    "Explique en une ou deux phrases ce qui ne va pas (ce que tu faisais, ce qui s'est passé) :";
pub const SIGNALEMENT_NOTE: &str = "Nevermind crée un fichier .zip sur ton bureau avec ta phrase, les infos du PC (Windows, composants, score) et le journal de Nevermind. Il ne contient ni tes fichiers, ni ton nom, ni tes mots de passe. Envoie-le à la personne qui t'a donné Nevermind (Discord par exemple).";

/// Le panneau de l'écran (`Get-PanneauKit`) pour « Signaler un problème ». Comme en PowerShell, la zone de texte de
/// la fenêtre d'avant n'est pas montrée par l'écran (le fichier est créé sans phrase) : voir DIVERGENCES.md.
pub fn page_signalement(de: &Option<String>) -> Value {
    json!({
        "titre": "Signaler un problème", "sous": "Un fichier à envoyer, sans données personnelles", "icone": "aide", "couleur": "P",
        "retour": "appli", "parent": parent(de, &["accueil", "panneau", "fiche"]),
        "module": { "icone": "aide", "couleur": "P", "titre": "Rapport", "texte": "Un fichier à envoyer, sans données personnelles",
            "boutons": [{ "id": "panneau:close", "texte": "Fermer", "style": "second" }] },
        "blocs": [
            { "type": "texte", "texte": SIGNALEMENT_EXPLICATION },
            { "type": "texte", "texte": SIGNALEMENT_NOTE },
            { "type": "bouton", "id": "panneau:b:0", "texte": "Créer le fichier", "style": null },
        ],
    })
}

/// Les pages de l'assistance pour l'écran montré : `fiche` (visite, nouveautés) et `panneau` (signalement), quand
/// elles sont ouvertes. Le bloc « divers » (ou l'hôte) les fusionne dans l'état envoyé au kit.
pub fn pages_assistance(ctx: &Contexte, ecran: &str) -> Map<String, Value> {
    let mut a = verrou(&ctx.assistance);
    if ecran != "fiche" && ecran != "panneau" {
        a.ecran = ecran.to_string();
    }
    let mut p = Map::new();
    if let Some(f) = &a.fiche {
        p.insert("fiche".into(), page_fiche(f, &a.fiche_de));
    }
    if a.panneau {
        p.insert("panneau".into(), page_signalement(&a.panneau_de));
    }
    p
}

/// Ce que l'utilisateur fait dans les pages de l'assistance : « visite » (Revoir la visite), les boutons de la
/// fiche ouverte (fiche:run, fiche:close), « clic:BtnReportProblem » et les boutons du panneau. `true` si l'action a
/// été prise. Le fichier du signalement est créé sur le bureau (`infos` : ce que les autres blocs savent du PC).
pub fn action(ctx: &Contexte, id: &str, _valeur: &Value, infos: &InfosSignalement, effets: &mut Effets) -> bool {
    match id {
        "visite" => {
            ctx.montrer_visite(effets);
            true
        }
        "clic:BtnReportProblem" => {
            ctx.montrer_signalement(effets);
            true
        }
        "fiche:run" | "fiche:close" | "fiche:open" | "fiche:ignore" => {
            let Some(f) = ctx.fiche_assistance() else { return false };
            match (id, f) {
                ("fiche:run", Fiche::Visite(i)) => {
                    if i + 1 >= VISITE.len() {
                        ctx.fermer_fiche(effets);
                    } else {
                        verrou(&ctx.assistance).fiche = Some(Fiche::Visite(i + 1));
                        effets.ajouter(Effet::Rafraichir);
                    }
                }
                ("fiche:close", _) => ctx.fermer_fiche(effets),
                _ => {}
            }
            true
        }
        "panneau:close" | "panneau:b:0" => {
            if !verrou(&ctx.assistance).panneau {
                return false;
            }
            ctx.fermer_signalement(effets);
            if id == "panneau:b:0" {
                // (la lecture de l'Aide et le zip : dans un travail de fond, audit G3 ; la fin est relevée par
                //  `relever_assistance` ; déjà en cours : rien de plus)
                let infos = infos.clone();
                let mut t = std::mem::take(&mut verrou(&ctx.assistance).signalement);
                t.lancer(ctx, "signalement", move |fond| fond.ctx.signaler_probleme(None, "", &infos));
                verrou(&ctx.assistance).signalement = t;
                ctx.relever_assistance(effets);
            }
            true
        }
        _ => false,
    }
}

impl Contexte {
    /// La fin de « Signaler un problème » (sur le fil des blocs) : le fichier sur le bureau, ou l'erreur
    pub fn relever_assistance(&self, effets: &mut Effets) {
        let rendus = verrou(&self.assistance).signalement.relever();
        for r in rendus {
            let r = match r {
                crate::travaux::Rendu::Effet(e) => {
                    effets.ajouter(e);
                    continue;
                }
                crate::travaux::Rendu::Fini { resultat, .. } => resultat.and_then(|x| x),
            };
            match r {
                Ok(zip) => {
                    effets.ajouter(Effet::Annonce("Fichier créé sur le bureau.".into()));
                    effets.ajouter(Effet::OuvrirAdresse(zip.to_string_lossy().into_owned()));
                    effets.ajouter(Effet::Annonce(message_signalement(&zip)));
                }
                Err(e) => {
                    self.noter_erreur(&format!("Signaler un problème : {e}"));
                    effets.ajouter(Effet::Annonce(e));
                }
            }
        }
    }
}

/// Le message montré quand le fichier du signalement est sur le bureau
pub fn message_signalement(zip: &Path) -> String {
    format!(
        "Le fichier « {} » est sur ton bureau.\n\nEnvoie-le à la personne qui t'a donné Nevermind (par Discord par exemple), avec une phrase qui explique ce qui ne va pas.\n\nIl ne contient ni tes fichiers, ni ton nom, ni tes mots de passe.",
        crate::fichiers::nom(zip)
    )
}

// ---------------------------------------------------------------------------------------------------------------
// Signaler un problème : un zip sur le bureau, sans données personnelles
// ---------------------------------------------------------------------------------------------------------------

/// Ce que les autres blocs savent du PC, pour le fichier du signalement (`$script:LastAnalysis`,
/// `$script:SecurityScore`, la page ouverte)
#[derive(Debug, Default, Clone)]
pub struct InfosSignalement {
    /// Le score d'optimisation, s'il y a eu une analyse
    pub score: Option<String>,
    /// Les infos de l'analyse (« clé », « valeur »)
    pub infos: Vec<(String, String)>,
    /// Les points analysés (« état », « titre »)
    pub points: Vec<(String, String)>,
    /// La note de protection sur 100
    pub protection: Option<i64>,
    /// La page ouverte quand le problème est signalé
    pub page: String,
}

/// Le masque des données personnelles : le dossier du profil (`\Users\<nom>`), le nom du compte et celui du PC
/// sont remplacés (sans tenir compte des majuscules)
pub struct Masque {
    pub profil: String,
    pub compte: String,
    pub pc: String,
}

impl Masque {
    /// Le masque de ce PC (`$env:USERPROFILE`, `$env:USERNAME`, `$env:COMPUTERNAME`)
    pub fn du_pc() -> Masque {
        let v = |n: &str| std::env::var(n).unwrap_or_default();
        let profil = v("USERPROFILE");
        let profil = profil.trim_end_matches('\\').rsplit('\\').next().unwrap_or("").to_string();
        Masque { profil, compte: v("USERNAME"), pc: v("COMPUTERNAME") }
    }

    /// Le texte sans les données personnelles
    pub fn appliquer(&self, t: &str) -> String {
        let mut t = t.to_string();
        if self.profil.chars().count() >= 3 {
            t = remplacer_profil(&t, &self.profil);
        }
        if !self.compte.is_empty() {
            t = remplacer_sans_casse(&t, &self.compte, "<utilisateur>");
        }
        if !self.pc.is_empty() {
            t = remplacer_sans_casse(&t, &self.pc, "<pc>");
        }
        t
    }
}

fn mot(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Remplace `quoi` partout, sans tenir compte des majuscules (comme `-replace [regex]::Escape(...)`)
pub fn remplacer_sans_casse(t: &str, quoi: &str, par: &str) -> String {
    let bas = t.to_lowercase();
    let q = quoi.to_lowercase();
    if q.is_empty() || bas.len() != t.len() {
        // (une minuscule qui change de longueur : comparaison caractère par caractère)
        return remplacer_lent(t, quoi, par, |_, _| true);
    }
    let mut r = String::with_capacity(t.len());
    let mut i = 0;
    while let Some(p) = bas[i..].find(&q) {
        r.push_str(&t[i..i + p]);
        r.push_str(par);
        i += p + q.len();
    }
    r.push_str(&t[i..]);
    r
}

fn remplacer_lent(t: &str, quoi: &str, par: &str, garder: impl Fn(&[char], usize) -> bool) -> String {
    let c: Vec<char> = t.chars().collect();
    let q: Vec<char> = quoi.chars().collect();
    let egal = |a: char, b: char| a == b || a.to_lowercase().eq(b.to_lowercase());
    let mut r = String::new();
    let mut i = 0;
    while i < c.len() {
        if i + q.len() <= c.len() && (0..q.len()).all(|k| egal(c[i + k], q[k])) && garder(&c, i + q.len()) {
            r.push_str(par);
            i += q.len();
        } else {
            r.push(c[i]);
            i += 1;
        }
    }
    r
}

/// `(?i)\\Users\\<profil>(?=\\|\b|$)` devient `\Users\<utilisateur>`
fn remplacer_profil(t: &str, profil: &str) -> String {
    let dernier = profil.chars().last().unwrap_or(' ');
    remplacer_lent(t, &format!("\\Users\\{profil}"), "\\Users\\<utilisateur>", |c, fin| match c.get(fin) {
        None | Some('\\') => true,
        Some(&x) => mot(dernier) != mot(x),
    })
}

/// `Set-Content -Encoding UTF8` : UTF-8 avec BOM, chaque ligne finie par « \r\n »
fn ecrire_texte_ps(f: &Path, t: &str) -> std::io::Result<()> {
    let mut o = vec![0xEF, 0xBB, 0xBF];
    o.extend_from_slice(t.as_bytes());
    o.extend_from_slice(b"\r\n");
    std::fs::write(f, o)
}

impl Contexte {
    /// `Export-ProblemReport` : crée « Nevermind problème <date>.zip » dans `dest` (le bureau si `None`), avec les
    /// infos du PC, la phrase de l'utilisateur, les erreurs, la fin du journal (et celui de l'Aide si elle tourne),
    /// et les fichiers de données, sans données personnelles. Rend le chemin du zip.
    pub fn signaler_probleme(
        &self,
        dest: Option<&Path>,
        description: &str,
        infos: &InfosSignalement,
    ) -> Result<PathBuf, String> {
        let masque = Masque::du_pc();
        let h = self.maintenant();
        let mut fichiers: Vec<(String, String)> = Vec::new();
        let os = "HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
        let lire = |n: &str| crate::registre::valeur(os, n).map(|v| texte_ps(&v)).unwrap_or_default();
        let mut info = vec![
            format!(
                "Nevermind {}{}",
                self.version(),
                if self.reglage_bool("Beta", false) { " (versions bêta activées)" } else { "" }
            ),
            format!("Date : {}", moment::date_courte(&h)),
            format!(
                "Windows : {} {} (build {}.{})",
                lire("ProductName"),
                lire("DisplayVersion"),
                lire("CurrentBuildNumber"),
                lire("UBR")
            ),
            format!("Langue : {}", langues()),
            "Moteur : Nevermind en Rust".to_string(),
            format!("Écran principal : {}", ecran_principal()),
            format!("Dossier de l'app : {}", self.appli.display()),
        ];
        if let Some(s) = &infos.score {
            info.push(String::new());
            info.push(format!("Score d'optimisation : {s}"));
            for (k, v) in &infos.infos {
                info.push(format!("{k} : {v}"));
            }
            info.push(String::new());
            info.push("Points analysés :".into());
            for (e, t) in &infos.points {
                info.push(format!("[{e}] {t}"));
            }
        }
        if let Some(p) = infos.protection {
            info.push(String::new());
            info.push(format!("Protection : {p} sur 100"));
        }
        fichiers.push(("infos.txt".into(), masque.appliquer(&info.join("\r\n"))));
        if !description.trim().is_empty() {
            fichiers.push((
                "description.txt".into(),
                masque.appliquer(&format!(
                    "Ce qui ne va pas :\r\n{}\r\n\r\nPage ouverte : {}",
                    description.trim(),
                    infos.page
                )),
            ));
        }
        fichiers.push(("erreurs.txt".into(), masque.appliquer(&self.erreurs().join("\r\n"))));
        let j = self.fichier_journal();
        if j.exists() {
            fichiers.push((
                "journal.txt".into(),
                masque.appliquer(&crate::journal::dernieres_lignes(&j, 400).join("\r\n")),
            ));
        }
        // Journal de l'Aide (gardé en mémoire par elle, 200 lignes) : seulement si elle tourne, sans la réveiller
        if self.aide_joignable(1)
            && let Ok(v) = self.aide_sans_demande("aide.journal", &json!({}), 10000)
        {
            let l: Vec<String> = liste(&v).iter().map(texte_ps).collect();
            fichiers.push(("journal-aide.txt".into(), masque.appliquer(&l.join("\r\n"))));
        }
        for n in ["sauvegarde.json", "historique.json", "parametres.json", "ignores.json"] {
            let p = self.donnees.join(n);
            if let Ok(o) = std::fs::read(&p) {
                fichiers.push((n.into(), masque.appliquer(&suite_commun::json_sur::texte_du_fichier(&o))));
            }
        }
        let dossier = match dest {
            Some(d) => d.to_path_buf(),
            None => crate::windows::dossier_bureau().ok_or("Le bureau est introuvable.")?,
        };
        let zip = dossier.join(format!("Nevermind problème {}.zip", moment::date_fichier(&h)));
        creer_zip(&zip, &fichiers).map_err(|_| "Le fichier n'a pas pu être créé.".to_string())?;
        if !zip.exists() {
            return Err("Le fichier n'a pas pu être créé.".into());
        }
        Ok(zip)
    }
}

/// Le zip du signalement : chaque fichier en UTF-8 avec BOM (`Set-Content -Encoding UTF8`)
fn creer_zip(zip: &Path, fichiers: &[(String, String)]) -> Result<(), String> {
    let tmp = std::env::temp_dir().join(format!("Nevermind-probleme-{}", identifiant()));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let r = (|| {
        for (n, t) in fichiers {
            ecrire_texte_ps(&tmp.join(n), t).map_err(|e| e.to_string())?;
        }
        if zip.exists() {
            std::fs::remove_file(zip).map_err(|e| e.to_string())?;
        }
        let f = std::fs::File::create(zip).map_err(|e| e.to_string())?;
        let mut w = zip::ZipWriter::new(f);
        let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (n, _) in fichiers {
            let octets = std::fs::read(tmp.join(n)).map_err(|e| e.to_string())?;
            w.start_file(n.as_str(), o).map_err(|e| e.to_string())?;
            std::io::Write::write_all(&mut w, &octets).map_err(|e| e.to_string())?;
        }
        w.finish().map_err(|e| e.to_string())?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

fn langues() -> String {
    langue_imposee().unwrap_or_else(crate::windows::langues)
}

/// La langue imposée par NEVERMIND_LANGUE : seulement dans la construction d'essai (option cargo `essai`, jamais
/// livrée), pour des signalements identiques d'un PC à l'autre. Dans l'appli livrée, la langue est celle de Windows.
#[cfg(feature = "essai")]
fn langue_imposee() -> Option<String> {
    std::env::var("NEVERMIND_LANGUE").ok()
}

#[cfg(not(feature = "essai"))]
fn langue_imposee() -> Option<String> {
    None
}

fn ecran_principal() -> String {
    crate::windows::ecran_principal()
}

// ---------------------------------------------------------------------------------------------------------------
// Les bulles d'une copie gérée par Mindset (notification.demande)
// ---------------------------------------------------------------------------------------------------------------

/// L'écart entre deux bulles (une bulle vue quelques secondes avant la suivante)
pub const ECART_BULLES: Duration = Duration::from_secs(6);

impl Contexte {
    /// `Show-Notify` d'une copie gérée : la bulle est mise en file pour Mindset (notification.demande). Rend son
    /// numéro (un clic dessus revient par afficher.demande « notif:<numéro> »).
    pub fn notifier(&self, titre: &str, texte: &str) -> String {
        let id = identifiant();
        let mut m = Map::new();
        m.insert("titre".into(), json!(titre));
        m.insert("texte".into(), json!(texte));
        m.insert("id".into(), json!(id));
        verrou(&self.assistance).bulles.push(Bulle {
            json: crate::json_ps::compact(&Value::Object(m)),
            quand: Instant::now(),
            titre: titre.to_string(),
        });
        self.envoyer_notification_suivante();
        id
    }

    /// `Send-NotifySuivante` : la bulle suivante part quand Mindset a pris la précédente et que celle-ci a été vue
    /// quelques secondes (à appeler toutes les secondes tant qu'il en reste : `bulles_en_attente`). Une bulle de
    /// plus de 5 minutes ne sert plus à rien. Écrite à côté puis renommée : Mindset ne lit jamais un fichier à moitié
    /// écrit.
    pub fn envoyer_notification_suivante(&self) {
        let f = self.donnees.join("notification.demande");
        let mut notes = Vec::new();
        {
            let mut a = verrou(&self.assistance);
            let vieilles: Vec<Bulle> =
                a.bulles.iter().filter(|b| b.quand.elapsed() >= Duration::from_secs(300)).cloned().collect();
            a.bulles.retain(|b| b.quand.elapsed() < Duration::from_secs(300));
            for b in vieilles {
                notes.push(format!("Bulle non montrée (Mindset ne l'a pas prise) : {}", b.titre));
            }
            let libre = !f.exists() && a.derniere_bulle.is_none_or(|d| d.elapsed() >= ECART_BULLES);
            if !a.bulles.is_empty() && libre {
                let b = a.bulles.remove(0);
                let tmp = crate::fichiers::a_cote(&f, ".ecriture");
                match std::fs::write(&tmp, b.json.as_bytes()).and_then(|()| std::fs::rename(&tmp, &f)) {
                    Ok(()) => a.derniere_bulle = Some(Instant::now()),
                    Err(e) => {
                        notes.push(format!("Notification impossible: {e}"));
                        let _ = std::fs::remove_file(&tmp);
                    }
                }
            }
        }
        for n in notes {
            self.journal(&n);
        }
    }

    /// Des bulles attendent encore (l'hôte rappelle `envoyer_notification_suivante` toutes les secondes)
    pub fn bulles_en_attente(&self) -> bool {
        !verrou(&self.assistance).bulles.is_empty()
    }
}

#[cfg(test)]
mod langue {
    /// Petit de l'audit du 08/11 : NEVERMIND_LANGUE (la langue écrite dans « Signaler un problème ») n'est lue que
    /// dans la construction d'essai ; dans l'appli livrée, une variable posée par n'importe qui ne change rien. Le test
    /// se relance lui-même avec la variable posée (sans fenêtre), pour ne pas toucher l'environnement des autres tests.
    #[test]
    fn la_variable_de_langue_ne_change_rien_dans_l_appli_livree() {
        const FAUSSE: &str = "xx-ESSAI, interface xx-ESSAI";
        if std::env::var("NEVERMIND_LANGUE").is_ok() {
            if cfg!(feature = "essai") {
                return;
            }
            assert_ne!(super::langues(), FAUSSE, "la variable NEVERMIND_LANGUE est lue dans l'appli livrée");
            return;
        }
        use std::os::windows::process::CommandExt;
        let o = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "assistance::langue::la_variable_de_langue_ne_change_rien_dans_l_appli_livree"])
            .env("NEVERMIND_LANGUE", FAUSSE)
            .creation_flags(0x0800_0000)
            .output()
            .unwrap();
        let sortie = String::from_utf8_lossy(&o.stdout);
        assert!(o.status.success() && sortie.contains("1 passed"), "{sortie}");
    }
}
