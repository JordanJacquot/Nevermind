//! Ce que Nevermind sait de la protection du PC : l'état `$S` de `Get-ProtectionStatus` (antivirus, pare-feu, contrôle
//! des comptes, exclusions, menaces, points suspects), et ce qui sert à le juger (éléments approuvés, fichier hosts,
//! proxy, programmes au démarrage). Il se lit depuis le JSON des juges, qui a la forme de `$S`.

use crate::ps::{self, Date};
use serde_json::{Map, Value, json};

/// L'état de Microsoft Defender (`Get-MpComputerStatus`)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mp {
    pub mode: String,
    pub temps_reel: bool,
    pub age_base: i64,
    pub age_analyse: i64,
    pub fin_analyse: Option<Date>,
}

impl Mp {
    /// `$mp.AMRunningMode -eq 'Normal'`
    pub fn normal(&self) -> bool {
        ps::egal(&self.mode, "Normal")
    }
}

/// Une menace connue de Defender (`Get-MpThreat`)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Menace {
    pub id: String,
    pub nom: String,
    pub gravite: Value,
    pub active: bool,
}

/// Une détection de Defender (`Get-MpThreatDetection`)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Detection {
    pub id: String,
    pub statut: Value,
    pub quand: Option<Date>,
    pub ressources: Vec<Value>,
}

/// Une tâche planifiée suspecte
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tache {
    pub nom: String,
    pub chemin: String,
    pub cmd: String,
    pub grave: bool,
}

/// Un programme au démarrage (`Get-StartupItems` : Nom, Exe, Enabled)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Demarrage {
    pub nom: String,
    pub exe: String,
    pub actif: bool,
}

/// Un élément approuvé par l'utilisateur (« C'est moi ») : réglage SecuriteApprouves
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Approuve {
    pub cle: String,
    pub texte: String,
    pub sorte: String,
}

impl Approuve {
    pub fn nouveau(sorte: &str, cle: String, texte: String) -> Approuve {
        Approuve { cle, texte, sorte: sorte.to_string() }
    }
    pub fn json(&self) -> Value {
        json!({ "Key": self.cle, "Text": self.texte, "Kind": self.sorte })
    }
}

/// `$S` de Get-ProtectionStatus
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EtatS {
    pub mp: Option<Mp>,
    pub autres_av: Vec<String>,
    pub pare_feu_coupe: Vec<String>,
    pub uac: bool,
    pub exclusions: Vec<String>,
    pub exclusions_nombre: i64,
    pub menaces: Vec<Menace>,
    pub detections: Vec<Detection>,
    pub doubles: Vec<String>,
    pub scripts: Vec<String>,
    pub caches: Vec<String>,
    pub demarrage_suspect: Vec<String>,
    pub taches: Vec<Tache>,
}

impl EtatS {
    pub fn pare_feu(&self) -> bool {
        self.pare_feu_coupe.is_empty()
    }
    pub fn actives(&self) -> Vec<&Menace> {
        self.menaces.iter().filter(|m| m.active).collect()
    }
    pub fn defender_ok(&self) -> bool {
        self.mp.as_ref().is_some_and(Mp::normal)
    }
}

/// Le fichier hosts et le proxy (lus au moment de la vérification) et les programmes au démarrage
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Environnement {
    /// Les lignes du fichier hosts, `None` s'il n'a pas pu être lu
    pub hosts: Option<Vec<String>>,
    /// ProxyEnable, AutoConfigURL, ProxyServer (registre de l'utilisateur), tels quels
    pub proxy_actif: Value,
    pub proxy_pac: Value,
    pub proxy_serveur: Value,
    pub demarrage: Vec<Demarrage>,
}

fn chaine(o: &Map<String, Value>, k: &str) -> String {
    o.get(k).map(ps::texte).unwrap_or_default()
}

fn date(v: Option<&Value>) -> Option<Date> {
    v.and_then(Value::as_str).and_then(Date::lire)
}

/// Les éléments vrais d'une liste (`@($x | Where-Object { $_ })`)
fn vrais(v: Option<&Value>) -> Vec<Value> {
    v.map(ps::liste).unwrap_or_default().into_iter().filter(ps::vrai).collect()
}

impl Mp {
    pub fn depuis(v: &Value) -> Option<Mp> {
        let o = v.as_object()?;
        Some(Mp {
            mode: chaine(o, "AMRunningMode"),
            temps_reel: o.get("RealTimeProtectionEnabled").is_some_and(ps::vrai),
            age_base: o.get("AntivirusSignatureAge").and_then(ps::entier).unwrap_or(0),
            age_analyse: o.get("QuickScanAge").map(|x| ps::reel(x) as i64).unwrap_or(0),
            fin_analyse: date(o.get("QuickScanEndTime")),
        })
    }
}

impl EtatS {
    /// L'état depuis le JSON (forme de `$S`) ; `None` pour `null` (pas encore vérifié)
    pub fn depuis(v: &Value) -> Option<EtatS> {
        let o = v.as_object()?;
        let textes = |k: &str| vrais(o.get(k)).iter().map(ps::texte).collect::<Vec<_>>();
        Some(EtatS {
            mp: o.get("Mp").and_then(Mp::depuis),
            autres_av: textes("OtherAv"),
            pare_feu_coupe: textes("FirewallOff"),
            uac: o.get("Uac").is_some_and(ps::vrai),
            exclusions: textes("Exclusions"),
            exclusions_nombre: o.get("ExclusionsNombre").and_then(ps::entier).unwrap_or(0),
            menaces: vrais(o.get("Threats"))
                .iter()
                .filter_map(Value::as_object)
                .map(|m| Menace {
                    id: chaine(m, "ThreatID"),
                    nom: chaine(m, "ThreatName"),
                    gravite: m.get("SeverityID").cloned().unwrap_or(Value::Null),
                    active: m.get("IsActive").is_some_and(ps::vrai),
                })
                .collect(),
            detections: vrais(o.get("Detections"))
                .iter()
                .filter_map(Value::as_object)
                .map(|d| Detection {
                    id: chaine(d, "ThreatID"),
                    statut: d.get("ThreatStatusID").cloned().unwrap_or(Value::Null),
                    quand: date(d.get("InitialDetectionTime")),
                    ressources: d.get("Resources").map(ps::liste).unwrap_or_default(),
                })
                .collect(),
            doubles: textes("Double"),
            scripts: textes("Scripts"),
            caches: textes("Hidden"),
            demarrage_suspect: textes("SusStart"),
            taches: vrais(o.get("Tasks")).iter().filter_map(Value::as_object).map(Tache::depuis).collect(),
        })
    }

    /// En JSON (même forme)
    pub fn json(&self) -> Value {
        json!({
            "Mp": self.mp.as_ref().map(|m| json!({ "AMRunningMode": m.mode, "RealTimeProtectionEnabled": m.temps_reel,
                "AntivirusSignatureAge": m.age_base, "QuickScanAge": m.age_analyse, "QuickScanEndTime": m.fin_analyse.map(|d| d.iso()) })),
            "OtherAv": self.autres_av, "FirewallOff": self.pare_feu_coupe, "Uac": self.uac, "Exclusions": self.exclusions,
            "ExclusionsNombre": self.exclusions_nombre,
            "Threats": self.menaces.iter().map(|m| json!({ "ThreatID": m.id, "ThreatName": m.nom, "SeverityID": m.gravite, "IsActive": m.active })).collect::<Vec<_>>(),
            "Detections": self.detections.iter().map(|d| json!({ "ThreatID": d.id, "ThreatStatusID": d.statut, "InitialDetectionTime": d.quand.map(|x| x.iso()), "Resources": d.ressources })).collect::<Vec<_>>(),
            "Double": self.doubles, "Scripts": self.scripts, "Hidden": self.caches, "SusStart": self.demarrage_suspect,
            "Tasks": self.taches.iter().map(Tache::json).collect::<Vec<_>>(),
        })
    }
}

impl Tache {
    pub fn depuis(t: &Map<String, Value>) -> Tache {
        Tache {
            nom: chaine(t, "Name"),
            chemin: chaine(t, "Path"),
            cmd: chaine(t, "Cmd"),
            grave: t.get("Bad").is_some_and(ps::vrai),
        }
    }
    pub fn json(&self) -> Value {
        json!({ "Name": self.nom, "Path": self.chemin, "Cmd": self.cmd, "Bad": self.grave })
    }
}

impl Environnement {
    /// Depuis l'entrée d'un juge : Hosts, Proxy, Demarrage
    pub fn depuis(e: &Value) -> Environnement {
        let proxy = e.get("Proxy").cloned().unwrap_or(Value::Null);
        let p = |k: &str| proxy.get(k).cloned().unwrap_or(Value::Null);
        Environnement {
            hosts: e.get("Hosts").filter(|h| !h.is_null()).map(|h| ps::liste(h).iter().map(ps::texte).collect()),
            proxy_actif: p("ProxyEnable"),
            proxy_pac: p("AutoConfigURL"),
            proxy_serveur: p("ProxyServer"),
            demarrage: vrais(e.get("Demarrage"))
                .iter()
                .filter_map(Value::as_object)
                .map(|d| Demarrage {
                    nom: chaine(d, "Nom"),
                    exe: chaine(d, "Exe"),
                    actif: d.get("Enabled").is_some_and(ps::vrai),
                })
                .collect(),
        }
    }
}

/// Les éléments approuvés (`Get-SecTrusted` : ceux qui ont une clé)
pub fn approuves(reglage: Option<&Value>) -> Vec<Approuve> {
    let v = reglage.cloned().unwrap_or(Value::Array(Vec::new()));
    ps::liste(&v)
        .iter()
        .filter(|x| ps::vrai(x))
        .filter_map(Value::as_object)
        .filter(|o| o.get("Key").is_some_and(ps::vrai))
        .map(|o| Approuve { cle: chaine(o, "Key"), texte: chaine(o, "Text"), sorte: chaine(o, "Kind") })
        .collect()
}
