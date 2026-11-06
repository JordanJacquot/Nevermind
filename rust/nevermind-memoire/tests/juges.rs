//! Les juges exportés de la version PowerShell (outils\exporter-juges-memoire.ps1) : pour chaque PC simulé, le constat,
//! les pages du kit et le rapport de la version Rust doivent être ceux de memoire.ps1, au champ près.
//!
//! Seule liberté : l'ordre de deux applis qui prennent exactement la même mémoire (PowerShell les range dans l'ordre
//! de sa table de hachage, qui n'a pas de sens ; Rust garde l'ordre de la lecture). Les comparaisons rangent donc ces
//! égalités de la même façon des deux côtés (voir DIVERGENCES.md).

use nevermind_memoire::constat::{self, Constat};
use nevermind_memoire::donnees::Donnees;
use nevermind_memoire::{fermeture, listes, page, rapport, texte};
use serde_json::{Value, json};
use std::path::PathBuf;

fn dossier() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("juges")
}

fn lire(nom: &str) -> Value {
    let t = std::fs::read_to_string(dossier().join(nom)).expect("juge");
    serde_json::from_str(t.trim_start_matches('\u{feff}')).expect("juge en JSON")
}

/// Égalité de deux valeurs JSON, les nombres à un milliardième près (sommes de doubles)
fn pareil(a: &Value, b: &Value, ou: &str, ecarts: &mut Vec<String>) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0));
            if (x - y).abs() > 1e-6 * x.abs().max(1.0) {
                ecarts.push(format!("{ou} : {x} au lieu de {y}"));
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    Some(w) => pareil(v, w, &format!("{ou}.{k}"), ecarts),
                    None => ecarts.push(format!("{ou}.{k} : en trop ({v})")),
                }
            }
            for k in y.keys() {
                if !x.contains_key(k) {
                    ecarts.push(format!("{ou}.{k} : manque ({})", y[k]));
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                ecarts.push(format!("{ou} : {} éléments au lieu de {}", x.len(), y.len()));
            }
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                pareil(v, w, &format!("{ou}[{i}]"), ecarts);
            }
        }
        _ => {
            if a != b {
                ecarts.push(format!("{ou} : {a} au lieu de {b}"));
            }
        }
    }
}

/// Le constat Rust dans la forme de l'export PowerShell
fn projection(c: &Constat) -> Value {
    json!({
        "Installee": c.installee, "InstalleeLue": c.installee_lue, "Visible": c.visible, "Reservee": c.reservee,
        "Utilisee": c.utilisee, "Disponible": c.disponible, "Cache": c.cache, "PoolNP": c.pool_np, "PoolP": c.pool_p,
        "Compressee": c.compressee, "Pourcent": c.pourcent, "TotalApplis": c.total_applis, "Reste": c.reste,
        "Applis": c.applis.iter().map(|g| json!({
            "Cle": g.cle, "Nom": g.nom, "Kind": g.kind, "Note": g.note, "Etiquette": g.etiquette, "Octets": g.octets,
            "Nb": g.nb, "Exes": g.exes, "Services": g.services, "Corrections": g.corrections,
            "Cibles": g.cibles.iter().map(|x| json!({ "Pid": x.pid, "Debut": x.debut, "Chemin": x.chemin, "Octets": x.octets })).collect::<Vec<_>>(),
            "Refus": g.refus, "Fermable": g.fermable, "Jeu": g.jeu, "Lanceur": g.lanceur, "AuDemarrage": g.au_demarrage,
            "ParWindows": g.par_windows })).collect::<Vec<_>>(),
        "Alertes": c.alertes.iter().map(|a| json!({ "Id": a.id, "Ton": a.ton, "Titre": a.titre, "Texte": a.texte, "Quoi": a.quoi })).collect::<Vec<_>>(),
        "Corrections": c.corrections.iter().map(|x| json!({ "Id": x.id, "Nom": x.nom })).collect::<Vec<_>>(),
        "Changements": c.corrections.iter().map(|x| json!({ "Type": "svc", "Name": x.service, "Demarrage": x.demarrage })).collect::<Vec<_>>(),
    })
}

/// Les applis rangées par mémoire puis par clé (les égalités dans le même ordre des deux côtés)
fn ranger_applis(c: &mut Value) {
    if let Some(a) = c["Applis"].as_array_mut() {
        a.sort_by(|x, y| {
            let (ox, oy) = (x["Octets"].as_f64().unwrap_or(0.0), y["Octets"].as_f64().unwrap_or(0.0));
            oy.partial_cmp(&ox).unwrap().then_with(|| x["Cle"].as_str().cmp(&y["Cle"].as_str()))
        });
    }
}

/// La page, avec le numéro de chaque bouton « Fermer » remplacé par la clé de son appli, et les lignes des applis
/// rangées (pour les égalités)
fn ranger_page(p: &Value, cles: &[String]) -> Value {
    let mut p = p.clone();
    if let Some(blocs) = p["blocs"].as_array_mut() {
        for b in blocs.iter_mut() {
            if let Some(id) = b["bouton"]["id"].as_str()
                && let Some(k) = id.strip_prefix("memoire-fermer:").and_then(|k| k.parse::<usize>().ok())
            {
                b["bouton"]["id"] = json!(format!("memoire-fermer:{}", cles.get(k).cloned().unwrap_or_default()));
            }
        }
        let debut = blocs.iter().position(|b| b["texte"] == "Par appli").map(|i| i + 1);
        let fin = blocs.iter().position(|b| b["texte"] == "La part cachée");
        if let (Some(d), Some(f)) = (debut, fin) {
            blocs[d..f].sort_by_key(canon);
        }
    }
    p
}

/// Un texte stable d'une valeur (les champs rangés par nom)
fn canon(v: &Value) -> String {
    match v {
        Value::Object(o) => {
            let mut k: Vec<&String> = o.keys().collect();
            k.sort();
            format!(
                "{{{}}}",
                k.iter().map(|k| format!("{k:?}:{}", canon(&o[k.as_str()]))).collect::<Vec<_>>().join(",")
            )
        }
        Value::Array(a) => format!("[{}]", a.iter().map(canon).collect::<Vec<_>>().join(",")),
        x => x.to_string(),
    }
}

/// Le rapport, avec les paires de lignes de « PAR APPLI » rangées (pour les égalités)
fn ranger_rapport(r: &str) -> Vec<String> {
    let l: Vec<String> = r.split("\r\n").map(str::to_string).collect();
    let Some(d) = l.iter().position(|x| x.starts_with("PAR APPLI")) else { return l };
    let f = l.iter().skip(d).position(String::is_empty).map_or(l.len(), |x| x + d);
    let mut paires: Vec<String> = l[d + 1..f].chunks(2).map(|c| c.join("\n")).collect();
    paires.sort();
    let mut o = l[..=d].to_vec();
    o.extend(paires);
    o.extend(l[f..].iter().cloned());
    o
}

/// Compare un juge ; rend les écarts
fn comparer(j: &Value) -> Vec<String> {
    let d = Donnees::depuis_json(&j["donnees"]);
    let c = constat::constat(&d);
    let mut ecarts = Vec::new();
    // L'ordre de Rust : du plus gros au plus petit
    for w in c.applis.windows(2) {
        if w[1].octets > w[0].octets {
            ecarts.push(format!("ordre : {} avant {}", w[0].nom, w[1].nom));
        }
    }
    let cles_rust: Vec<String> = c.applis.iter().map(|a| a.cle.clone()).collect();
    let cles_ps: Vec<String> = j["constat"]["Applis"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["Cle"].as_str().unwrap_or("").to_string())
        .collect();
    let (mut a, mut b) = (projection(&c), j["constat"].clone());
    ranger_applis(&mut a);
    ranger_applis(&mut b);
    pareil(&a, &b, "constat", &mut ecarts);
    for (k, attendu) in j["pages"].as_object().unwrap() {
        let (en_cours, fermeture) = k.split_once('|').unwrap();
        let p = page::page(Some(&c), en_cours, fermeture == "True");
        pareil(&ranger_page(&p, &cles_rust), &ranger_page(attendu, &cles_ps), &format!("page[{k}]"), &mut ecarts);
    }
    let r = rapport::texte(&c);
    // (PowerShell : sans le champ « Etiquettes », @($null) compte un élément vide : voir DIVERGENCES.md)
    let sans_etiquettes = j["donnees"].get("Etiquettes").is_none_or(Value::is_null);
    let (x, y) = (ranger_rapport(&r), ranger_rapport(j["rapport"].as_str().unwrap()));
    if x != y && !sans_etiquettes {
        for (i, (u, v)) in x.iter().zip(&y).enumerate() {
            if u != v {
                ecarts.push(format!("rapport ligne {i} : « {u} » au lieu de « {v} »"));
                break;
            }
        }
        if x.len() != y.len() {
            ecarts.push(format!("rapport : {} lignes au lieu de {}", x.len(), y.len()));
        }
    }
    if rapport::nom_fichier(&c) != j["fichier"].as_str().unwrap() {
        ecarts.push(format!("fichier du rapport : {}", rapport::nom_fichier(&c)));
    }
    ecarts
}

fn tous_les_juges() -> Vec<Value> {
    let mut l = Vec::new();
    let mut f: Vec<_> = std::fs::read_dir(dossier()).unwrap().flatten().map(|e| e.path()).collect();
    f.sort();
    for p in f {
        let n = p.file_name().unwrap().to_string_lossy().into_owned();
        if n.starts_with("constats-") {
            l.extend(lire(&n).as_array().unwrap().iter().cloned());
        }
    }
    l
}

#[test]
fn constats_pages_et_rapports() {
    let juges = tous_les_juges();
    assert!(juges.len() >= 250, "seulement {} juges", juges.len());
    let mut rates = Vec::new();
    for j in &juges {
        let e = comparer(j);
        if !e.is_empty() {
            rates.push(format!("{} : {}", j["nom"], e.iter().take(4).cloned().collect::<Vec<_>>().join(" | ")));
        }
    }
    assert!(rates.is_empty(), "{} juges sur {} ratés :\n{}", rates.len(), juges.len(), rates.join("\n"));
}

/// La contre-épreuve : un seul détail changé dans un juge le fait échouer
#[test]
fn contre_epreuve() {
    let juges = tous_les_juges();
    let j = juges.iter().find(|j| j["nom"] == "fermer : Discord").unwrap().clone();
    assert!(comparer(&j).is_empty());
    type Changement = Box<dyn Fn(&mut Value)>;
    let changements: Vec<Changement> = vec![
        Box::new(|j| j["constat"]["Applis"][0]["Nom"] = json!("Autre nom")),
        Box::new(|j| {
            j["constat"]["Applis"][0]["Fermable"] = json!(!j["constat"]["Applis"][0]["Fermable"].as_bool().unwrap())
        }),
        Box::new(|j| j["constat"]["Pourcent"] = json!(j["constat"]["Pourcent"].as_i64().unwrap() + 1)),
        Box::new(|j| j["pages"]["|False"]["sous"] = json!("Ce qui prend la mémoire de ton PC.")),
        Box::new(|j| {
            let r = j["rapport"].as_str().unwrap().replacen("Mo", "Ko", 1);
            j["rapport"] = json!(r)
        }),
        Box::new(|j| {
            let o = j["donnees"]["Processus"][3]["Octets"].as_f64().unwrap();
            j["donnees"]["Processus"][3]["Octets"] = json!(o + 1048576.0);
        }),
        Box::new(|j| {
            j["donnees"]["Processus"].as_array_mut().unwrap().last_mut().unwrap()["Compte"] = json!("S-1-5-21-AUTRE")
        }),
    ];
    for (i, ch) in changements.iter().enumerate() {
        let mut x = j.clone();
        ch(&mut x);
        assert!(!comparer(&x).is_empty(), "le changement {i} n'est pas vu");
    }
}

#[test]
fn petites_fonctions() {
    let p = lire("petites.json");
    for t in p["tailles"].as_array().unwrap() {
        assert_eq!(texte::taille(t[0].as_f64().unwrap()), t[1].as_str().unwrap(), "Format-Size {}", t[0]);
    }
    for g in p["genres"].as_array().unwrap() {
        assert_eq!(listes::genre_pilote(g[0].as_str().unwrap()), g[1].as_str().unwrap(), "genre {}", g[0]);
    }
    for m in p["messages"].as_array().unwrap() {
        let (f, t) = fermeture::message("Discord « été »", m[0].as_str().unwrap());
        assert_eq!((f, t.as_str()), (m[1].as_bool().unwrap(), m[2].as_str().unwrap()), "message {}", m[0]);
    }
    for o in p["origines"].as_array().unwrap() {
        assert_eq!(constat::origine(o[0].as_bool().unwrap(), o[1].as_bool().unwrap()), o[2].as_str().unwrap());
    }
    for (k, attendu) in p["pagesSansConstat"].as_object().unwrap() {
        let mut e = Vec::new();
        pareil(&page::page(None, k, false), attendu, &format!("page sans constat [{k}]"), &mut e);
        assert!(e.is_empty(), "{e:?}");
    }
}

/// Aucun tiret long ni demi-cadratin dans ce que montre le bloc
#[test]
fn pas_de_tiret_long() {
    for j in tous_les_juges().iter().take(80) {
        let d = Donnees::depuis_json(&j["donnees"]);
        let c = constat::constat(&d);
        let t = format!("{}{}", page::page(Some(&c), "pilotes", false), rapport::texte(&c));
        assert!(!t.contains('\u{2013}') && !t.contains('\u{2014}'), "{}", j["nom"]);
    }
}
