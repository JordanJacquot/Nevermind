//! Les juges exportés de la version PowerShell (outils\exporter-juges-perf.ps1) : chaque cas est rejoué ici et doit
//! donner exactement la même sortie (les nombres à 1e-9 près, ils sont arrondis à 6 décimales dans les juges).

use nevermind_perf::{diag, fps, jeu, pages, texte};
use serde_json::{Value, json};
use std::path::PathBuf;

fn juge(nom: &str) -> Vec<Value> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("juges").join(nom);
    let t = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{nom} illisible : {e}"));
    // (comme Nevermind avant d'envoyer une page au kit, vue.ps1 : un « rien » de PowerShell, écrit {} par ConvertTo-Json,
    // devient null)
    let t = t.trim_start_matches('\u{feff}').replace("\":{}", "\":null");
    let v: Value = serde_json::from_str(&t).unwrap_or_else(|e| panic!("{nom} abîmé : {e}"));
    v["cas"].as_array().cloned().unwrap_or_default()
}

/// Égalité de deux valeurs JSON, les nombres comparés comme des doubles ; rend le chemin de la première différence
pub fn difference(a: &Value, b: &Value, chemin: &str) -> Option<String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap_or(f64::NAN), y.as_f64().unwrap_or(f64::NAN));
            if (x - y).abs() <= 1e-9 * x.abs().max(1.0) { None } else { Some(format!("{chemin} : {x} au lieu de {y}")) }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{chemin} : {} éléments au lieu de {} ({} / {})", x.len(), y.len(), a, b));
            }
            x.iter().zip(y).enumerate().find_map(|(i, (p, q))| difference(p, q, &format!("{chemin}[{i}]")))
        }
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys() {
                if !y.contains_key(k) {
                    return Some(format!("{chemin}.{k} : en trop ({})", x[k]));
                }
            }
            for k in y.keys() {
                match x.get(k) {
                    None => return Some(format!("{chemin}.{k} : manque (attendu {})", y[k])),
                    Some(v) => {
                        if let Some(d) = difference(v, &y[k], &format!("{chemin}.{k}")) {
                            return Some(d);
                        }
                    }
                }
            }
            None
        }
        _ => {
            if a == b {
                None
            } else {
                Some(format!("{chemin} : {a} au lieu de {b}"))
            }
        }
    }
}

/// Divergence voulue (DIVERGENCES.md, « Dalles d'outils ») : `resume`, `niveau` et `jugement` des dalles n'existent
/// pas en PowerShell ; ils sont retirés de la sortie Rust avant la comparaison, tout le reste est comparé tel quel.
pub fn sans_resume_ni_niveau(mut v: Value) -> Value {
    fn retirer(v: &mut Value) {
        match v {
            Value::Array(a) => a.iter_mut().for_each(retirer),
            Value::Object(o) => {
                if o.get("type").and_then(Value::as_str) == Some("outils") {
                    for x in o.get_mut("elements").and_then(Value::as_array_mut).into_iter().flatten() {
                        if let Some(x) = x.as_object_mut() {
                            x.remove("resume");
                            x.remove("niveau");
                            x.remove("jugement");
                        }
                    }
                }
                o.values_mut().for_each(retirer);
            }
            _ => {}
        }
    }
    retirer(&mut v);
    v
}

fn verifier(nom: &str, cas: &[Value], min: usize, rejouer: impl Fn(&Value) -> Value) {
    assert!(cas.len() >= min, "{nom} : {} cas, au moins {min} attendus", cas.len());
    let mut echecs = Vec::new();
    for c in cas {
        // Divergence voulue (DIVERGENCES.md, « Mode jeu : aucune appli cochée ») : en PowerShell, une liste vide de
        // GameModeApps sort de Get-Setting comme $null et redevient la liste par défaut. Le juge est rejoué avec ce que
        // PowerShell lisait vraiment (absent) ; le test `aucune_appli_cochee` vérifie la correction.
        let mut e = c["entree"].clone();
        if e.get("appsModeJeu").and_then(Value::as_array).is_some_and(|a| a.is_empty()) {
            e["appsModeJeu"] = Value::Null;
        }
        let obtenu = rejouer(&e);
        if let Some(d) = difference(&obtenu, &c["attendu"], "") {
            echecs.push(format!("{} : {d}", c["nom"].as_str().unwrap_or("?")));
        }
    }
    assert!(
        echecs.is_empty(),
        "{nom} : {} cas sur {} différents :\n{}",
        echecs.len(),
        cas.len(),
        echecs.iter().take(15).cloned().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn pages_comme_powershell() {
    let cas = juge("pages.json");
    verifier("pages.json", &cas, 400, |e| sans_resume_ni_niveau(Value::Object(pages::pages(e))));
}

/// Second audit du 09/11 : la dalle Nettoyage ne disait « à nettoyer » que par sa couleur (orange au-delà de 1 Go, vert
/// sinon, barre toujours pleine). Son jugement est maintenant aussi écrit (`jugement`, ajouté à l'état par kit.js),
/// sur chaque juge : « à nettoyer » quand elle est orange, rien quand elle est verte.
#[test]
fn nettoyage_dit_son_jugement_en_mots() {
    let (mut orange, mut vert) = (0, 0);
    for c in juge("pages.json") {
        let p = Value::Object(pages::pages(&c["entree"]));
        let blocs = p["perf"]["blocs"].as_array().cloned().unwrap_or_default();
        for x in blocs
            .iter()
            .filter(|b| b["type"] == "outils")
            .flat_map(|b| b["elements"].as_array().cloned().unwrap_or_default())
        {
            if x["id"] != "nettoyage" {
                continue;
            }
            match x["ton"].as_str() {
                Some("attention") => {
                    orange += 1;
                    assert_eq!(x["jugement"], "à nettoyer", "{}", c["nom"]);
                }
                _ => {
                    vert += 1;
                    assert!(x["jugement"].is_null(), "{}", c["nom"]);
                }
            }
        }
    }
    assert!(orange > 0 && vert > 0, "juges sans dalle Nettoyage orange ou verte : {orange} {vert}");
}

#[test]
fn mode_jeu_comme_powershell() {
    let cas = juge("mode-jeu.json");
    let debut: Vec<Value> = cas.iter().filter(|c| c["entree"].get("reprise").is_none()).cloned().collect();
    let reprise: Vec<Value> = cas.iter().filter(|c| c["entree"].get("reprise").is_some()).cloned().collect();
    verifier("mode-jeu.json (début)", &debut, 250, |e| {
        let mode_on = pages::vrai(e.get("modeJeu"));
        let sel = jeu::selection(e.get("appsModeJeu"));
        let refuses: Vec<String> = pages::liste(e.get("refuses")).iter().map(|x| pages::ch(Some(x))).collect();
        let ouverts: Vec<(String, String)> = pages::liste(e.get("processus"))
            .iter()
            .map(|p| (pages::ch(p.get("nom")), pages::ch(p.get("chemin"))))
            .collect();
        let fermees = jeu::fermer_pour_partie(mode_on, &sel, &ouverts, |_, procs| {
            procs.iter().filter(|p| !refuses.contains(&p.0)).count()
        });
        let j = pages::ch(e.get("jeu"));
        let fichier = jeu::fichier_partie(&j, 4242, "jeu", "", &fermees).map(|mut v| {
            v.as_object_mut().map(|o| o.remove("Start"));
            v
        });
        let bulles: Vec<Value> =
            jeu::bulle_debut(&j, &fermees).into_iter().map(|(t, x)| json!({ "titre": t, "texte": x })).collect();
        let n_jeux = pages::nb(e.get("nJeux")) as i64;
        json!({
            "selection": sel, "fermees": fermees.iter().map(|f| f.json()).collect::<Vec<_>>(), "fichier": fichier, "bulles": bulles,
            "etatEnJeu": jeu::etat_mode_jeu(mode_on, Some((j.clone(), fermees.len() as i64)), n_jeux), "etatAttente": jeu::etat_mode_jeu(mode_on, None, n_jeux)
        })
    });
    verifier("mode-jeu.json (reprise)", &reprise, 10, |e| {
        let j = &e["reprise"];
        let fermees = jeu::reprise(j);
        let echecs: Vec<String> = pages::liste(e.get("echecsRelance")).iter().map(|x| pages::ch(Some(x))).collect();
        if fermees.is_empty() {
            return json!({ "session": null, "relances": [], "bulles": [] });
        }
        if pages::vrai(e.get("jeuOuvert")) {
            return json!({ "session": { "jeu": j["Game"], "pid": j["Pid"], "fermees": fermees.iter().map(|f| json!({ "id": f.id, "nom": f.nom, "chemin": f.chemin, "args": f.args })).collect::<Vec<_>>() }, "relances": [], "bulles": [] });
        }
        let mut relances = Vec::new();
        let rates = jeu::relancer(
            &fermees,
            |_| false,
            |c, a| {
                relances.push(format!("{c}|{}", a.unwrap_or("")));
                !echecs.iter().any(|x| x == c)
            },
        );
        let (t, x) = jeu::bulle_fin(&rates);
        json!({ "session": null, "relances": relances, "bulles": [{ "titre": t, "texte": x }] })
    });
}

#[test]
fn detection_comme_powershell() {
    let cas = juge("detection.json");
    verifier("detection.json", &cas, 250, |e| {
        let idx = jeu::index_jeux(&pages::liste(e.get("jeux")));
        let ouverts: Vec<jeu::Ouvert> = pages::liste(e.get("processus"))
            .iter()
            .map(|p| jeu::Ouvert {
                pid: pages::nb(p.get("pid")) as u32,
                nom: pages::ch(p.get("nom")),
                chemin: pages::ch(p.get("chemin")),
            })
            .collect();
        match jeu::detecter(&idx, &ouverts) {
            Some((j, pid)) => json!({ "jeu": j, "pid": pid }),
            None => Value::Null,
        }
    });
}

/// `.ToString(InvariantCulture)` de .NET Framework : 15 chiffres significatifs
fn g15(x: f64) -> String {
    let v: f64 = format!("{x:.14e}").parse().unwrap_or(x);
    format!("{v}")
}

fn r6(x: f64) -> f64 {
    texte::ps_round(x, 6)
}

#[test]
fn parties_comme_powershell() {
    let cas = juge("parties.json");
    verifier("parties.json", &cas, 80, |e| {
        let ent = pages::ch(e.get("entete"));
        let cols: Vec<&str> = ent.split(',').collect();
        let mode = pages::ch(e.get("mode"));
        let pause = pages::nb(e.get("pause")) as i64;
        let mut im = fps::Images::default();
        im.ligne(&ent);
        let mut n: i64 = 0;
        let mut echantillons = Vec::new();
        for m in pages::liste(e.get("morceaux")) {
            let ms = pages::nb(m.get("ms"));
            let nb_i = pages::nb(m.get("nb")) as i64;
            let ecart = pages::nb(m.get("ecart"));
            let sac = pages::nb(m.get("saccade")) as i64;
            for _ in 0..nb_i {
                let mut ft = ms;
                if ecart != 0.0 {
                    ft *= 1.0 + ecart * (n as f64 * 0.7).sin();
                }
                if sac != 0 && n % sac == 3 {
                    ft = 80.0 + (n % 7) as f64;
                }
                let ft = texte::ps_round(ft, 3);
                let v: Vec<String> = cols
                    .iter()
                    .map(|c| match *c {
                        "Application" => "jeu.exe".into(),
                        "ProcessID" => "42".into(),
                        "PresentMode" => mode.clone(),
                        "MsCPUBusy" => g15(ft * 0.6),
                        "MsGPUBusy" => g15(ft * 0.93),
                        "Autre" => "x".into(),
                        _ => g15(ft),
                    })
                    .collect();
                im.en_pause = pause >= 0 && n >= pause && n < pause + 300;
                im.ligne(&v.join(","));
                n += 1;
                if n % 500 == 0 {
                    echantillons.push(r6(im.echantillon()));
                }
            }
        }
        im.en_pause = false;
        let b = im.bilan();
        let direct = im.direct();
        let exclusif = im.dernier_mode.to_lowercase().contains("legacy");
        let courbe: Vec<f64> = echantillons.iter().filter(|x| **x > 0.0).map(|x| texte::ps_round(*x, 1)).collect();
        let nom = pages::ch(e.get("jeu"));
        let t = fps::Partie {
            nom: &nom,
            cle: "jeu",
            debut_s: "2026-11-05T21:30:00",
            exclusif,
            courbe: &courbe,
            refus: pages::vrai(e.get("refus")),
        };
        let diag = json!({ "Stutters": texte::ps_int(b.occupe[2]) });
        let (enreg, bulle) = match fps::fin_partie(&t, &b, Some(diag), "x") {
            fps::FinPartie::Rien => (Value::Null, Value::Null),
            fps::FinPartie::BulleSeule(a, c) => (Value::Null, json!({ "titre": a, "texte": c })),
            fps::FinPartie::Enregistrer { partie, bulle, probleme } => {
                let mut p = partie.as_object().cloned().unwrap_or_default();
                p.remove("Id");
                p.remove("Diag");
                p.insert("Probleme".into(), json!(probleme));
                let v = fps::verdict(p["Avg"].as_f64().unwrap_or(0.0), p["Low1"].as_f64().unwrap_or(0.0));
                p.insert("Verdict".into(), json!([v.0, v.1]));
                (Value::Object(p), json!({ "titre": bulle.0, "texte": bulle.1 }))
            }
        };
        json!({
            "resume": b.resume.map(r6), "palier": b.palier.map(r6), "occupe": b.occupe.map(r6), "direct": direct.map(r6), "echantillons": echantillons,
            "images": im.nombre(), "exclusif": exclusif, "enregistrement": enreg, "bulle": bulle
        })
    });
}

#[test]
fn diagnostic_comme_powershell() {
    let cas = juge("diagnostic.json");
    let (diags, mut releves): (Vec<Value>, Vec<Value>) =
        cas.into_iter().partition(|c| c["entree"].get("releves").is_none());
    // (à égalité, l'ordre de Sort-Object n'est pas sûr : Top et TopMem sont rangés par nom des deux côtés)
    for c in &mut releves {
        for k in ["Top", "TopMem"] {
            if let Some(a) = c["attendu"].get_mut(k).and_then(Value::as_array_mut) {
                a.sort_by_key(|x| x["Name"].as_str().unwrap_or("").to_lowercase());
            }
        }
    }
    verifier("diagnostic.json (diagnostic)", &diags, 350, |e| {
        let mut d = e["diag"].clone();
        if pages::vrai(e.get("sansDiag")) {
            d = Value::Null;
        }
        let s = json!({ "Id": "cette", "Game": "Mon jeu", "Key": "jeu", "Avg": e["avg"], "Low1": e["low1"], "Seconds": e["seconds"], "Series": e["series"], "Diag": d });
        let nv = e.get("nvidia").filter(|n| !n.is_null()).map(|n| {
            (
                n.get("installe").filter(|x| !x.is_null()).map(|x| pages::ch(Some(x))),
                pages::ch(n.get("dernier")),
                pages::ch(n.get("url")),
            )
        });
        let c = diag::ContexteDiagnostic {
            plan_ok: pages::vrai(e.get("plan")),
            jeu_connu: pages::vrai(e.get("jeuConnu")),
            prio: pages::vrai(e.get("prio")),
            nom_jeu: "Mon jeu".into(),
            autres_parties: pages::nb(e.get("autresParties")) as usize,
            ecran_a_regler: pages::vrai(e.get("ecranAnalyse"))
                && pages::ch(e["diag"].get("Display")) == r"\\.\DISPLAY1",
            nvidia: nv,
        };
        let mut v = diag::diagnostic(&s, &c).json();
        // (le juge ne garde pas « quoi », ce que fait le bouton)
        if let Some(el) = v.get_mut("elements").and_then(Value::as_array_mut) {
            for x in el {
                if let Some(a) = x.get_mut("actions").and_then(Value::as_array_mut) {
                    for b in a {
                        b.as_object_mut().map(|o| o.remove("quoi"));
                    }
                }
            }
        }
        v
    });
    verifier("diagnostic.json (relevés)", &releves, 100, |e| {
        let mut t = diag::Releves::default();
        for r in pages::liste(e.get("releves")) {
            let o = |k: &str| r.get(k).filter(|v| !v.is_null()).map(|v| pages::nb(Some(v)));
            t.sys.push(diag::Releve {
                cpu: o("Cpu"),
                perf: o("Perf"),
                ram: o("Ram"),
                gpu: o("Gpu"),
                temp: o("Temp"),
                vram: o("Vram"),
                power: o("Power"),
                disk: o("Disk"),
            });
        }
        for p in pages::liste(e.get("processus")) {
            let lignes: Vec<String> = pages::liste(p.get("lignes")).iter().map(|l| pages::ch(Some(l))).collect();
            t.programmes(&lignes, pages::nb(p.get("secondes")));
        }
        t.sur_batterie = pages::vrai(e.get("batterie"));
        t.chemin = pages::ch(e.get("chemin"));
        let occ = pages::liste(e.get("occupe"));
        let occ = [pages::nb(occ.first()), pages::nb(occ.get(1)), pages::nb(occ.get(2))];
        let disques = vec![
            diag::Disque { lettres: vec!["C".into()], bus: "NVMe".into(), media: "SSD".into() },
            diag::Disque { lettres: vec!["D".into()], bus: "SATA".into(), media: "HDD".into() },
        ];
        let ecrans: Vec<String> = pages::liste(e.get("ecrans")).iter().map(|x| pages::ch(Some(x))).collect();
        let gpus = pages::liste(e.get("gpus"));
        let c = diag::ContexteDiag {
            coeurs: pages::nb(e.get("coeurs")) as usize,
            disques: &disques,
            ecrans: &ecrans,
            gpus: &gpus,
            portable: pages::vrai(e.get("portable")),
            onduleur: pages::vrai(e.get("onduleur")),
            mode_jeu: pages::vrai(e.get("modeJeu")),
        };
        let mut v = diag::donnees(&t, &occ, &c);
        for k in ["Top", "TopMem"] {
            if let Some(a) = v.get_mut(k).and_then(Value::as_array_mut) {
                a.sort_by_key(|x| x["Name"].as_str().unwrap_or("").to_lowercase());
            }
        }
        v
    });
}

#[test]
fn temperature_comme_powershell() {
    let cas = juge("temperature.json");
    verifier("temperature.json", &cas, 60, |e| {
        let mut a = jeu::AlerteChaleur::default();
        let mut out = Vec::new();
        for (i, t) in pages::liste(e.get("temperatures")).iter().enumerate() {
            let t = if t.is_null() { None } else { Some(pages::nb(Some(t))) };
            if let Some(x) = a.tester(t, i as f64) {
                out.push(json!({ "seconde": i, "texte": x }));
            }
        }
        Value::Array(out)
    });
}

/// La contre-épreuve : un juge changé d'un seul octet doit faire échouer la comparaison
#[test]
fn contre_epreuve() {
    let cas = juge("pages.json");
    let c = &cas[7];
    let obtenu = sans_resume_ni_niveau(Value::Object(pages::pages(&c["entree"])));
    assert!(difference(&obtenu, &c["attendu"], "").is_none());
    let mut faux = c["attendu"].clone();
    let t = faux["perf"]["titre"].as_str().unwrap_or("").replacen('P', "p", 1);
    faux["perf"]["titre"] = json!(t);
    assert!(difference(&obtenu, &faux, "").is_some(), "un octet changé dans le juge doit se voir");
    let mut faux2 = c["attendu"].clone();
    faux2["optimisation"]["onglets"][1]["module"]["resume"][0]["valeur"] = json!("99");
    assert!(difference(&obtenu, &faux2, "").is_some());
}

/// La correction voulue : aucune appli cochée, aucune appli fermée (PowerShell fermait les 6 applis par défaut)
#[test]
fn aucune_appli_cochee() {
    assert!(jeu::selection(Some(&json!([]))).is_empty());
    assert_eq!(jeu::selection(None).len(), 6);
    assert_eq!(jeu::selection(Some(&Value::Null)).len(), 6);
    let ouverts = vec![("OneDrive".to_string(), r"C:\x\OneDrive.exe".to_string())];
    let f = jeu::fermer_pour_partie(true, &jeu::selection(Some(&json!([]))), &ouverts, |_, p| p.len());
    assert!(f.is_empty());
}
