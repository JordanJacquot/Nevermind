//! Les juges exportés de la version PowerShell (outils\exporter-juges-protection.ps1) : pour chaque état simulé, les
//! pages Protection, Sécurité et Sauvegarde (Connexion : bloc réseau) doivent être les mêmes au champ près (après la remise en liste
//! que fait kit.js, `normaliser`), ainsi que la note et les gestes de chaque point à vérifier.

use nevermind_protection::etat::{self, Environnement, EtatS};
use nevermind_protection::historique;
use nevermind_protection::regles::{self, ContexteTaches};
use nevermind_protection::{CasPages, pages_pour};
use serde_json::{Value, json};

/// Les champs que kit.js remet en liste quand PowerShell envoie un objet seul (`LISTES` de kit.js)
const LISTES: [&str; 17] = [
    "tuiles",
    "ronds",
    "blocs",
    "onglets",
    "sections",
    "elements",
    "rangs",
    "boutons",
    "resume",
    "options",
    "appareils",
    "actions",
    "choix",
    "points",
    "bascules",
    "touches",
    "quand",
];

/// Divergence voulue (DIVERGENCES.md, « Dalles d'outils ») : `resume` et `niveau` des dalles n'existent pas en
/// PowerShell ; ils sont retirés de la sortie Rust avant la comparaison, tout le reste est comparé tel quel.
fn sans_resume_ni_niveau(v: &mut Value) {
    match v {
        Value::Array(a) => a.iter_mut().for_each(sans_resume_ni_niveau),
        Value::Object(o) => {
            if o.get("type").and_then(Value::as_str) == Some("outils") {
                for x in o.get_mut("elements").and_then(Value::as_array_mut).into_iter().flatten() {
                    if let Some(x) = x.as_object_mut() {
                        x.remove("resume");
                        x.remove("niveau");
                    }
                }
            }
            o.values_mut().for_each(sans_resume_ni_niveau);
        }
        _ => {}
    }
}

fn normaliser(v: &mut Value) {
    match v {
        Value::Array(a) => a.iter_mut().for_each(normaliser),
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                if LISTES.contains(&k.as_str()) && !x.is_null() && !x.is_array() {
                    *x = Value::Array(vec![x.take()]);
                }
                normaliser(x);
            }
        }
        _ => {}
    }
}

/// Le premier endroit où deux valeurs diffèrent
fn difference(a: &Value, b: &Value, chemin: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                let (p, q) = (x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null));
                if x.contains_key(k) != y.contains_key(k) {
                    return Some(format!("{chemin}.{k} : PowerShell {p} / Rust {q}"));
                }
                if let Some(d) = difference(p, q, &format!("{chemin}.{k}")) {
                    return Some(d);
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{chemin} : {} éléments / {} ; PowerShell {a}\nRust {b}", x.len(), y.len()));
            }
            x.iter().zip(y).enumerate().find_map(|(i, (p, q))| difference(p, q, &format!("{chemin}[{i}]")))
        }
        _ => (a != b).then(|| format!("{chemin} : PowerShell {a} / Rust {b}")),
    }
}

fn juges(nom: &str) -> Vec<Value> {
    // (NEVERMIND_JUGES_PROTECTION : un autre dossier de juges, par exemple des cas tirés avec une autre graine)
    let d = std::env::var("NEVERMIND_JUGES_PROTECTION")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("juges"));
    let f = d.join(nom);
    let t = std::fs::read_to_string(&f).unwrap_or_else(|e| panic!("{} illisible : {e}", f.display()));
    match serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).expect("juges abîmés") {
        Value::Array(a) => a,
        autre => vec![autre],
    }
}

/// Ce que Rust rend pour l'entrée d'un juge : pages normalisées, note, gestes des points
fn sortie_rust(e: &Value) -> (Value, Value, Value) {
    let s = e.get("S").and_then(EtatS::depuis);
    let env = Environnement::depuis(e);
    let reglages = e.get("Reglages").cloned().unwrap_or(Value::Null);
    let approuves = etat::approuves(reglages.get("SecuriteApprouves"));
    let sauve = historique::sauvegarde(e.get("Sauvegarde"), reglages.get("SvcOriginal"), reglages.get("PcfgOriginal"));
    let hist = historique::importer(e.get("Historique"));
    let (pages, note, points) =
        pages_pour(CasPages { s, env: &env, approuves: &approuves, sauvegarde: &sauve, historique: &hist });
    let mut p = Value::Object(pages);
    normaliser(&mut p);
    sans_resume_ni_niveau(&mut p);
    let gestes: Vec<Value> = points
        .iter()
        .map(|c| {
            json!({ "titre": c.titre,
                "actions": c.actions.iter().map(|a| json!({ "texte": a.texte, "confirmer": a.confirmer.clone().unwrap_or_default() })).collect::<Vec<_>>(),
                "approuver": c.confiance.iter().map(etat::Approuve::json).collect::<Vec<_>>() })
        })
        .collect();
    (p, note.map(Value::from).unwrap_or(Value::Null), Value::Array(gestes))
}

fn sortie_powershell(s: &Value) -> (Value, Value, Value) {
    let mut p: Value = serde_json::from_str(s["pages"].as_str().expect("pages en texte")).expect("pages abîmées");
    // (la page Connexion est donnée par le bloc réseau)
    p.as_object_mut().expect("pages").remove("connexion");
    normaliser(&mut p);
    let mut g = s.get("actionsSec").cloned().unwrap_or(json!([]));
    if !g.is_array() {
        g = json!([g]);
    }
    for c in g.as_array_mut().into_iter().flatten() {
        for k in ["actions", "approuver"] {
            if let Some(x) = c.get_mut(k)
                && !x.is_array()
            {
                *x = if x.is_null() { json!([]) } else { json!([x.take()]) };
            }
        }
    }
    (p, s.get("score").cloned().unwrap_or(Value::Null), g)
}

#[test]
fn pages_comme_powershell() {
    let j = juges("pages.json");
    assert!(j.len() >= 600, "trop peu de juges ({})", j.len());
    let mut echecs = Vec::new();
    for c in &j {
        let nom = c["nom"].as_str().unwrap_or("?");
        let (pr, nr, gr) = sortie_rust(&c["entree"]);
        let (pp, np, gp) = sortie_powershell(&c["sortie"]);
        if let Some(d) = difference(&pp, &pr, "pages") {
            echecs.push(format!("{nom} : {d}"));
        } else if np != nr {
            echecs.push(format!("{nom} : note PowerShell {np} / Rust {nr}"));
        } else if let Some(d) = difference(&gp, &gr, "points") {
            echecs.push(format!("{nom} : {d}"));
        }
    }
    assert!(
        echecs.is_empty(),
        "{} juges sur {} diffèrent :\n{}",
        echecs.len(),
        j.len(),
        echecs[..echecs.len().min(8)].join("\n")
    );
}

/// La contre-épreuve : un juge changé d'un caractère doit être vu
#[test]
fn contre_epreuve() {
    let j = juges("pages.json");
    let mut vus = 0;
    for c in j.iter().take(40) {
        let (pr, _, _) = sortie_rust(&c["entree"]);
        let texte = c["sortie"]["pages"].as_str().unwrap_or_default();
        // (« Revérifier » est dans chaque page Protection : une lettre changée)
        let faux = texte.replacen("Rev\\u00e9rifier", "Rev\\u00e9rifiez", 1).replacen("Revérifier", "Revérifiez", 1);
        assert_ne!(faux, texte, "le texte à changer est absent du juge {}", c["nom"]);
        let mut p: Value = serde_json::from_str(&faux).unwrap();
        p.as_object_mut().unwrap().remove("connexion");
        normaliser(&mut p);
        if difference(&p, &pr, "pages").is_some() {
            vus += 1;
        }
    }
    assert_eq!(vus, 40, "un juge changé n'a pas été vu");
}

#[test]
fn lecture_de_l_aide_comme_powershell() {
    let j = juges("aide.json");
    assert!(j.len() >= 200);
    // (les juges ont été exportés sur un PC où %SystemRoot% s'écrit « C:\WINDOWS » ; la machine de GitHub écrit
    // « C:\Windows » : la même valeur est donnée ici, pour comparer la règle et pas la machine)
    let var = |n: &str| match n.to_ascii_lowercase().as_str() {
        "systemroot" | "windir" => Some(r"C:\WINDOWS".to_string()),
        _ => std::env::var(n).ok().filter(|v| !v.is_empty()),
    };
    let non = |_: &str| false;
    let c = ContexteTaches {
        temp: "C:\\Users\\x\\AppData\\Local\\Temp",
        public: &format!("{}\\Users\\Public", std::env::var("SystemDrive").unwrap_or_default()),
        variable: &var,
        script_protege: &non,
        existe: &non,
        propre_tache: None,
    };
    let mut echecs = Vec::new();
    for cas in &j {
        let e = &cas["entree"];
        let mut s = EtatS::depuis(
            &json!({ "Exclusions": e["Local"]["Exclusions"], "Tasks": e["Local"]["Tasks"], "Uac": true }),
        )
        .expect("état");
        let lecture = e.get("Lecture").filter(|l| !l.is_null());
        let taches = e.get("Taches").filter(|t| !t.is_null()).ok_or_else(|| "Aide injoignable.".to_string());
        regles::completer_par_aide(&mut s, lecture, taches, &c);
        let r = json!({
            "ExclusionsNombre": s.exclusions_nombre,
            "Threats": s.menaces.iter().map(|m| json!({ "ThreatID": m.id, "ThreatName": m.nom, "SeverityID": m.gravite, "IsActive": m.active })).collect::<Vec<_>>(),
            "Active": s.actives().iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
            "Detections": s.detections.iter().map(|d| json!({ "ThreatID": d.id, "ThreatStatusID": d.statut, "InitialDetectionTime": d.quand.map(|q| q.iso()), "Resources": d.ressources })).collect::<Vec<_>>(),
            "Tasks": s.taches.iter().map(|t| t.json()).collect::<Vec<_>>(),
        });
        let mut p = cas["sortie"].clone();
        normaliser_listes(&mut p, &["Threats", "Active", "Detections", "Tasks", "Resources"]);
        if let Some(d) = difference(&p, &r, "aide") {
            echecs.push(format!("{} : {d}", cas["nom"]));
        }
        // (contre-épreuve : la même sortie avec une tâche en plus doit être vue)
        let mut faux = p.clone();
        faux["Tasks"]
            .as_array_mut()
            .expect("liste")
            .push(json!({ "Name": "x", "Path": "x", "Cmd": "x", "Bad": false }));
        assert!(difference(&faux, &r, "aide").is_some(), "contre-épreuve non vue");
    }
    assert!(
        echecs.is_empty(),
        "{} cas sur {} diffèrent :\n{}",
        echecs.len(),
        j.len(),
        echecs[..echecs.len().min(8)].join("\n")
    );
}

fn normaliser_listes(v: &mut Value, cles: &[&str]) {
    match v {
        Value::Array(a) => a.iter_mut().for_each(|x| normaliser_listes(x, cles)),
        Value::Object(o) => {
            for (k, x) in o.iter_mut() {
                if cles.contains(&k.as_str()) && !x.is_array() {
                    *x = if x.is_null() { json!([]) } else { Value::Array(vec![x.take()]) };
                }
                normaliser_listes(x, cles);
            }
        }
        _ => {}
    }
}
