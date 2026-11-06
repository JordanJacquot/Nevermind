//! Les tests des composants jugés contre la version PowerShell (tests/juges/tests-composants.json, exporté par
//! outils\exporter-juges-tests.ps1) : des PC simulés et des mesures inventées, et, à chaque geste, le panneau du test
//! (pendant et après), la page Tests des composants, les messages, ce qui est demandé au moteur et les gestes sur le PC.

use nevermind_pc::tests::banc::{Demande, Echantillon, Lieu, Resultat, Travail};
use nevermind_pc::tests_composants::{Geste, Tests};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

struct FauxLieu {
    vols: HashMap<String, (f64, f64)>,
    systeme: String,
    temp: String,
    smi: bool,
}

impl Lieu for FauxLieu {
    fn volume(&self, lettre: &str) -> Option<(f64, f64)> {
        self.vols.get(lettre).copied()
    }
    fn lecteur_systeme(&self) -> String {
        self.systeme.clone()
    }
    fn temp(&self) -> String {
        self.temp.clone()
    }
    fn smi(&self) -> bool {
        self.smi
    }
}

fn lire(nom: &str) -> Vec<Value> {
    let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/juges").join(nom);
    let t = std::fs::read_to_string(&f)
        .unwrap_or_else(|_| panic!("juges introuvables : {nom} (outils\\exporter-juges-tests.ps1)"));
    serde_json::from_str(t.trim_start_matches('\u{feff}')).expect("juges illisibles")
}

/// Les cas, et les PC simulés qu'ils reprennent (ceux des juges du bloc pc, analyse.json)
fn juges() -> (Vec<Value>, Vec<Value>) {
    (lire("tests-composants.json"), lire("analyse.json"))
}

/// Les nombres en f64 (PowerShell écrit 5 ou 5.0 selon le type), et sans les clés que l'hôte pose lui-même
fn normal(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap_or(0.0)),
        Value::Array(a) => Value::Array(a.iter().map(normal).collect()),
        Value::Object(o) => {
            let mut m = Map::new();
            for (k, x) in o {
                if k == "parent" && x.is_object() || k == "retour" || k == "numero" {
                    continue;
                }
                m.insert(k.clone(), normal(x));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}

/// La première différence entre deux valeurs (chemin et valeurs), rien si pareilles
fn ecart(a: &Value, b: &Value, chemin: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                let (p, q) = (x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null));
                if let Some(e) = ecart(p, q, &format!("{chemin}.{k}")) {
                    return Some(e);
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            for i in 0..x.len().max(y.len()) {
                let (p, q) = (x.get(i).unwrap_or(&Value::Null), y.get(i).unwrap_or(&Value::Null));
                if let Some(e) = ecart(p, q, &format!("{chemin}[{i}]")) {
                    return Some(e);
                }
            }
            None
        }
        (Value::Number(x), Value::Number(y)) => {
            let (p, q) = (x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0));
            ((p - q).abs() > 1e-9 * p.abs().max(1.0)).then(|| format!("{chemin} : Rust {p} / PowerShell {q}"))
        }
        _ => (a != b).then(|| format!("{chemin} : Rust {a} / PowerShell {b}")),
    }
}

fn travail(nom: &str) -> Option<Travail> {
    Some(match nom {
        "disque" => Travail::Disque,
        "erreurs" => Travail::Erreurs,
        "cpu" => Travail::Cpu,
        "memoire" => Travail::Memoire,
        "reseau" => Travail::Reseau,
        "lag" => Travail::Lag,
        _ => return None,
    })
}

fn resultat(r: &Value, t: Travail) -> Resultat {
    if r.is_null() {
        return Resultat::Rien;
    }
    if r.get("Cancelled").is_some_and(|x| x.as_bool() == Some(true)) {
        return Resultat::Arrete;
    }
    if let Some(e) = r.get("Error").and_then(Value::as_str) {
        return Resultat::Erreur(e.into());
    }
    let l = r.get("R").and_then(Value::as_array).cloned().unwrap_or_default();
    if t == Travail::Erreurs {
        Resultat::Lignes(l.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect())
    } else {
        Resultat::R(l.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect())
    }
}

/// Ce que le pas a produit
#[derive(Default)]
struct Sortie {
    pendant: Vec<Value>,
    messages: Vec<String>,
    gestes: Vec<String>,
    demande: Value,
}

struct Rejeu {
    t: Tests,
    lieu: FauxLieu,
}

impl Rejeu {
    fn gestes(&mut self, gestes: Vec<Geste>, mesure: &Value, s: &mut Sortie) {
        for g in gestes {
            match g {
                Geste::Demande(d) => self.demande(d, mesure, s),
                Geste::Lancer(a) => s.gestes.push(format!("lancer {}", a.join(" "))),
                Geste::Batterie => s.gestes.push(format!(
                    "lancer powercfg.exe /batteryreport /output \"{}\\rapport-batterie.html\"",
                    self.lieu.temp
                )),
                Geste::Ouvrir(a) => s.gestes.push(format!("ouvrir {a}")),
                Geste::Pixels(i) => s.gestes.push(format!("pixels {i}")),
            }
        }
    }

    fn demande(&mut self, d: Demande, mesure: &Value, s: &mut Sortie) {
        match d {
            Demande::Mesure(tr, arg) => {
                s.demande = json!({ "travail": tr.nom(), "arg": arg });
                for x in mesure["tics"].as_array().cloned().unwrap_or_default() {
                    self.t.direct.cpu_perf = x["perf"].as_f64();
                    if x["nouveau"].as_bool() == Some(true) {
                        self.t.direct.mesure += 1;
                    }
                    let e = Echantillon {
                        phase: x["phase"].as_str().unwrap_or("").into(),
                        progres: x["progres"].as_f64().unwrap_or(0.0),
                        direct: x["direct"].as_f64().unwrap_or(0.0),
                        perf: self.t.direct.cpu_perf,
                        mesure: self.t.direct.mesure,
                    };
                    self.t.tic(&e);
                    if x["lire"].as_bool() == Some(true) {
                        s.pendant.push(self.t.panneau().unwrap_or(Value::Null));
                    }
                }
                let r = resultat(&mesure["resultat"], tr);
                for d in self.t.fin(r) {
                    self.demande(d, mesure, s);
                }
            }
            Demande::Sante(k, id) => {
                s.demande = json!({ "travail": "sante", "arg": id });
                let sante = &mesure["sante"];
                let disque = sante.get("D").filter(|x| !x.is_null()).cloned();
                let rel = sante.get("Rel").filter(|x| !x.is_null()).cloned();
                let aide = mesure.get("relAide").filter(|x| !x.is_null()).cloned();
                for d in self.t.sante_lue(k, disque.as_ref(), rel.as_ref(), aide.as_ref(), &self.lieu) {
                    self.demande(d, mesure, s);
                }
            }
            Demande::Gpu(k) => {
                let smi = mesure["smi"].as_str().map(str::to_string);
                for d in self.t.gpu_lu(k, smi.as_deref(), &self.lieu) {
                    self.demande(d, mesure, s);
                }
            }
            Demande::EffacerAncien(f) => s.gestes.push(format!("effacer {f}")),
            Demande::Message(m) => s.messages.push(m),
            Demande::Occupe(_) | Demande::Debit(_) => {}
        }
    }
}

fn texte_liste(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default()
}

#[test]
fn tests_des_composants_comme_powershell() {
    let (cas, pcs) = juges();
    assert!(!cas.is_empty());
    let (mut etapes, mut erreurs) = (0, Vec::new());
    let mut vus: HashMap<String, usize> = HashMap::new();
    for c in &cas {
        let e = &c["entree"];
        let mut t = Tests::default();
        t.donnees = pcs[e["pc"].as_u64().unwrap_or(0) as usize]["entree"]["data"].clone();
        t.ecrans = texte_liste(&e["ecrans"]);
        t.portable = e["portable"].as_bool().unwrap_or(false);
        t.troisdmark = e["troisdmark"].as_bool().unwrap_or(false);
        t.mesures(&e["live"]);
        let lieu = FauxLieu {
            vols: HashMap::new(),
            systeme: e["systemdrive"].as_str().unwrap_or("C:").into(),
            temp: e["temp"].as_str().unwrap_or("").into(),
            smi: e["smi"].as_bool().unwrap_or(false),
        };
        let mut r = Rejeu { t, lieu };
        for (i, pas) in c["etapes"].as_array().cloned().unwrap_or_default().iter().enumerate() {
            etapes += 1;
            let ou = format!("cas {} pas {i} ({})", c["numero"], pas["geste"]);
            let geste = pas["geste"].as_str().unwrap_or("").to_string();
            let mesure = pas.get("mesure").cloned().unwrap_or(Value::Null);
            r.lieu.vols = mesure
                .get("vols")
                .and_then(Value::as_object)
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| {
                            (k.clone(), (v["Size"].as_f64().unwrap_or(0.0), v["SizeRemaining"].as_f64().unwrap_or(0.0)))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mut s = Sortie::default();
            r.t.page();
            if geste == "mesures" {
                let l = &mesure["live"];
                r.t.direct.gpu = l["Gpu"].as_f64().unwrap_or(0.0);
                r.t.direct.gpu_temp = l["GpuTemp"].as_f64();
                r.t.direct.gpu_conso = l["GpuPower"].as_f64().unwrap_or(0.0);
                r.t.direct.mesure += 1;
                let live = r.t.direct.clone();
                r.t.banc.tic_gpu(&live);
            } else if geste.starts_with("test-b:") {
                let g = r.t.clic(&geste, &r.lieu).unwrap_or_default();
                r.gestes(g, &mesure, &mut s);
            } else if geste.starts_with("panneau:") {
                r.t.panneau();
                let g = r.t.panneau_action(&geste, &r.lieu);
                r.gestes(g, &mesure, &mut s);
            }
            let sortie = &pas["sortie"];
            let mut comparer = |quoi: &str, rust: Value, ps: &Value| {
                *vus.entry(quoi.to_string()).or_default() += 1;
                if let Some(x) = ecart(&normal(&rust), &normal(ps), quoi) {
                    erreurs.push(format!("{ou} : {x}"));
                }
            };
            comparer("panneau", r.t.panneau().unwrap_or(Value::Null), &sortie["panneau"]);
            comparer("pendant", Value::Array(s.pendant.clone()), &sortie["pendant"]);
            comparer("tests", r.t.page(), &sortie["tests"]);
            comparer("messages", json!(s.messages), &sortie["messages"]);
            comparer("gestes", json!(s.gestes), &sortie["gestes"]);
            comparer("demande", s.demande.clone(), &sortie["demande"]);
            let _ = travail;
        }
    }
    eprintln!("Juges des tests des composants : {} cas, {etapes} pas, {} écarts", cas.len(), erreurs.len());
    for e in erreurs.iter().take(25) {
        eprintln!("  {e}");
    }
    assert!(erreurs.is_empty(), "{} écarts avec la version PowerShell", erreurs.len());
}
