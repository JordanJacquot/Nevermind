//! Le juge « veille et mesure par l'Aide » (`tests/juges/veille-aide.json`, exporté de la version PowerShell par
//! `outils\exporter-juges-veille.ps1`) : les blocs perf et trafic, ensemble, contre une fausse Aide sur le canal de
//! la copie d'essai (jamais la vraie Aide), font exactement la même suite de demandes que la version PowerShell au
//! démarrage, au lancement d'un jeu (à anti-triche ou non), pendant et à la fin d'une partie, quand l'Aide refuse ;
//! ils écrivent les mêmes lignes au journal et la même partie dans fps.json.
//!
//! Ce qui n'est pas comparé : les lectures elles-mêmes (`fps.lire` toutes les 250 ms, `flux.lire` toutes les 2 s, au
//! rythme de l'horloge de chaque version : seulement « la mesure a été lue ») et les lignes du repli sur place de
//! PowerShell (PresentMon ou la trace lancés par Nevermind lui-même, pas portés).

use nevermind_noyau::{Bloc, Contexte, Effet, Effets, dossier_essai, fausse_aide};
use nevermind_perf::BlocPerf;
use nevermind_perf::systeme::{FauxCompteur, FauxSysteme, Partage, Programme};
use nevermind_trafic::BlocTrafic;
use nevermind_trafic::lag::pings::Pingeur;
use nevermind_trafic::lag::session::Depart;
use nevermind_trafic::microsoft::LecturesWindows;
use nevermind_trafic::systeme::Systeme;
use nevermind_trafic::trafic::etat::Service;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

fn juge() -> Value {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("juges").join("veille-aide.json");
    let t = std::fs::read_to_string(&p).expect("juge illisible");
    serde_json::from_str(t.trim_start_matches('\u{feff}')).expect("juge abîmé")
}

fn texte(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn liste(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.clone(),
        Value::Null => Vec::new(),
        x => vec![x.clone()],
    }
}

/// Le faux Windows du bloc trafic : les mêmes programmes que celui du bloc perf, pas de réseau
struct FauxReseau(Arc<Mutex<Partage>>);
struct PasDePing;
impl Pingeur for PasDePing {
    fn ping(&self, _: &str) -> Option<f64> {
        Some(12.0)
    }
    fn trace(&self, _: &str, _: u8, _: u32) -> Vec<String> {
        Vec::new()
    }
}
impl LecturesWindows for FauxReseau {
    fn reg_nombre(&self, _: &str, _: &str) -> Option<i64> {
        None
    }
    fn reg_texte(&self, _: &str, _: &str) -> String {
        String::new()
    }
    fn service(&self, _: &str) -> Option<(bool, String)> {
        None
    }
    fn compte_microsoft(&self) -> bool {
        false
    }
}
impl Systeme for FauxReseau {
    fn tcp(&self) -> (Vec<String>, bool) {
        (Vec::new(), false)
    }
    fn udp(&self) -> Vec<String> {
        Vec::new()
    }
    fn processus(&self) -> Vec<(u32, u32, String)> {
        self.0.lock().unwrap().progs.iter().map(|p| (p.pid, 1, p.nom.clone())).collect()
    }
    fn chemin(&self, _: u32) -> String {
        String::new()
    }
    fn services(&self) -> HashMap<u32, Vec<Service>> {
        HashMap::new()
    }
    fn cache_dns(&self) -> Vec<(String, String)> {
        Vec::new()
    }
    fn signature(&self, _: &str) -> (String, String) {
        (String::new(), String::new())
    }
    fn description(&self, _: &str) -> String {
        String::new()
    }
    fn reseau(&self) -> Option<Depart> {
        Some(Depart { passerelle: "192.168.1.254".into(), wifi: false, carte: "Ethernet".into() })
    }
    fn pingeur(&self) -> Arc<dyn Pingeur> {
        Arc::new(PasDePing)
    }
    fn wifi(&self) -> Vec<String> {
        Vec::new()
    }
    fn http(&self, _: &str, _: &str) -> Result<String, String> {
        Err("pas de réseau dans les tests".into())
    }
    fn nom_inverse(&self, _: &str) -> String {
        String::new()
    }
}

/// Ce que fait l'hôte des signaux qui comptent ici : partie et mesure du lag entre les deux blocs, jeu ajouté à la
/// veille (hote_communs.rs : `ajouter_fps_veille`)
fn router(perf: &mut BlocPerf, trafic: &mut BlocTrafic, ctx: &Contexte, e: Effets) {
    let mut file: Vec<Effet> = e.0;
    while !file.is_empty() {
        let mut suite = Effets::default();
        for x in file {
            if let Effet::Signal { nom, donnees } = x {
                match nom.as_str() {
                    "partie-commencee" | "partie-finie" => trafic.signal(ctx, &nom, &donnees, &mut suite),
                    "mesure-lag" => perf.signal(ctx, &nom, &donnees, &mut suite),
                    "veille-jeu-ajouter" => ctx.ajouter_fps_veille(&texte(&donnees["Processus"])),
                    _ => {}
                }
            }
        }
        file = suite.0;
    }
}

/// Les lignes du journal comparées : (état de la veille, le reste), dans l'ordre. Le message d'erreur d'une demande
/// faite en arrière-plan par PowerShell (« Exception lors de l'appel de « Texte »... ») est ramené au message de
/// l'Aide ; les lignes du repli sur place de PowerShell sont laissées de côté.
fn lignes(journal: &[String], jeux: &[String]) -> (Vec<String>, Vec<String>) {
    let mut veille = Vec::new();
    let mut autres = Vec::new();
    for l in journal {
        let mut l = l.replace('\u{a0}', " ");
        while let Some(i) = l.find("Exception lors de l'appel de « Texte » avec « 3 » argument(s) : « ") {
            let reste = &l[i + "Exception lors de l'appel de « Texte » avec « 3 » argument(s) : « ".len()..];
            let Some(j) = reste.find(" »") else { break };
            l = format!("{}{}{}", &l[..i], &reste[..j], &reste[j + " »".len()..]);
        }
        if l.starts_with("Mesure des FPS : en veille") || l.starts_with("Mesure des FPS : veille impossible") {
            veille.push(l);
        } else if l.starts_with("Mesure des FPS : par l'Aide")
            || l.starts_with("Mesure des FPS par l'Aide impossible")
            || l.starts_with("Mesure des FPS : bloquée")
            || l.starts_with("Mesure des FPS: aucune image")
            || l.contains(" refuse une mesure démarrée après lui")
            || l.starts_with("Lag : trace réseau")
            || (l.starts_with("Aide : ") && l.contains("refusé ou raté"))
            || l.starts_with("Mode jeu: ")
            || l.starts_with("Lag: mesure de la connexion")
            || jeux.iter().any(|j| l == format!("Mesure des FPS: {j}"))
        {
            autres.push(l);
        }
    }
    (veille, autres)
}

fn meme_nombre(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() < 1e-6,
        _ => a == b,
    }
}

/// Un cas du juge rejoué ; `None` si tout est pareil, sinon la première différence
fn rejouer(c: &Value, n: usize) -> Option<String> {
    let nom = texte(&c["nom"]);
    let d = dossier_essai(&format!("veille-aide-{n}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("donnees")).unwrap();
    let ctx = Contexte::nouveau(d.join("fichiers"), d.join("donnees"), false, true);
    if let Some(r) = c["reglages"].as_object() {
        for (k, v) in r {
            let v = if k == "FpsVeille" { Value::Array(liste(v)) } else { v.clone() };
            ctx.ecrire_reglage(k, v, false).unwrap();
        }
    }
    // La fausse Aide : la réponse du scénario, sinon « ok » ; les lignes de PresentMon à la première lecture
    let reponses: HashMap<String, Value> = c["reponses"]
        .as_object()
        .map(|o| o.iter().map(|(k, v)| (k.clone(), serde_json::from_str(&texte(v)).unwrap())).collect())
        .unwrap_or_default();
    let lignes_fps = Arc::new(Mutex::new((String::new(), false)));
    let lf = lignes_fps.clone();
    let serveur = fausse_aide::lancer(&ctx, move |a, _| {
        let mut l = lf.lock().unwrap();
        if a == "fps.demarrer" {
            l.1 = false;
        }
        if let Some(r) = reponses.get(a) {
            return Some(r.clone());
        }
        if a == "fps.lire" {
            let t = if l.1 { String::new() } else { l.0.clone() };
            l.1 = true;
            return Some(fausse_aide::ok(json!(t)));
        }
        Some(fausse_aide::ok(json!("ok")))
    })
    .expect("fausse Aide");

    let partage = Arc::new(Mutex::new(Partage::default()));
    let mut chemins = HashMap::new();
    for e in liste(&c["evenements"]) {
        if e["lance"]["pid"].is_number() {
            chemins.insert(e["lance"]["pid"].as_u64().unwrap() as u32, texte(&e["lance"]["chemin"]));
        }
    }
    let pm = ctx.appli.join("outils-tiers").join("PresentMon.exe").to_string_lossy().into_owned();
    let sys = FauxSysteme {
        partage: Some(partage.clone()),
        chemins,
        fichiers: vec![pm],
        cpu: Some((100, 1000)),
        ram: Some((16e9, 8e9)),
        ..Default::default()
    };
    let mut perf = BlocPerf::avec(Box::new(sys), Box::new(FauxCompteur::default()));
    let mut trafic = BlocTrafic::avec_systeme(Arc::new(FauxReseau(partage.clone())));
    let mut e = Effets::default();
    perf.signal(&ctx, "jeux-liste", &Value::Array(liste(&c["jeux"])), &mut e);
    router(&mut perf, &mut trafic, &ctx, e);
    let images = liste(&c["images"]);
    let entete = texte(&c["entete"]);
    let fin = c["fin"].as_f64().unwrap();
    let mut pas = 0u32;
    while f64::from(pas) * 0.25 <= fin {
        let t = f64::from(pas) * 0.25;
        for ev in liste(&c["evenements"]).iter().filter(|x| x["t"].as_f64() == Some(t)) {
            if ev["lance"]["pid"].is_number() {
                let pid = ev["lance"]["pid"].as_u64().unwrap() as u32;
                let pn = texte(&ev["lance"]["nom"]);
                let mut p = partage.lock().unwrap();
                p.progs.push(Programme { pid, nom: pn.clone(), ..Default::default() });
                p.premier_plan = pid;
                let mut s = format!("{entete}\n");
                for seg in &images {
                    for i in 0..seg[2].as_u64().unwrap() {
                        let ms = if i % 2 == 1 { texte(&seg[1]) } else { texte(&seg[0]) };
                        s.push_str(&format!(
                            "{pn}.exe,{pid},{ms},Hardware: Independent Flip,{},{}\n",
                            texte(&seg[3]),
                            texte(&seg[4])
                        ));
                    }
                }
                lignes_fps.lock().unwrap().0 = s;
            }
            if ev["quitte"]["pid"].is_number() {
                let pn = texte(&ev["quitte"]["nom"]);
                let mut p = partage.lock().unwrap();
                p.progs.retain(|x| x.nom != pn);
                p.premier_plan = 0;
            }
            if liste(&ev["jeux"]).first().is_some_and(|j| j["nom"].is_string()) {
                let mut e = Effets::default();
                perf.signal(&ctx, "jeux-liste", &Value::Array(liste(&ev["jeux"])), &mut e);
                router(&mut perf, &mut trafic, &ctx, e);
            }
        }
        perf.horloge = Some(t);
        let mut e = Effets::default();
        perf.tic(&ctx, true, &mut e);
        router(&mut perf, &mut trafic, &ctx, e);
        let mut e = Effets::default();
        trafic.tic(&ctx, true, &mut e);
        router(&mut perf, &mut trafic, &ctx, e);
        pas += 1;
    }
    // (le bilan de la dernière partie se calcule dans un autre fil)
    for _ in 0..200 {
        let mut e = Effets::default();
        perf.tic(&ctx, true, &mut e);
        router(&mut perf, &mut trafic, &ctx, e);
        if std::fs::metadata(ctx.donnees.join("fps.json")).is_ok() || liste(&c["parties"]).is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    // Les demandes
    let recues: Vec<Value> = serveur.demandes().iter().map(|l| serde_json::from_str::<Value>(l).unwrap()).collect();
    let lu_fps = recues.iter().any(|d| d["a"] == "fps.lire");
    let demandes: Vec<Value> = recues
        .iter()
        .filter(|d| d["a"] != "fps.lire" && d["a"] != "flux.lire")
        .map(|d| {
            let p = if d["p"].is_null() { json!({}) } else { d["p"].clone() };
            json!({ "a": d["a"], "p": p })
        })
        .collect();
    let attendu: Vec<Value> = liste(&c["demandes"])
        .iter()
        .map(|d| json!({ "a": d["a"], "p": if d["p"].is_null() { json!({}) } else { d["p"].clone() } }))
        .collect();
    let court =
        |l: &[Value]| l.iter().map(|d| format!("{} {}", texte(&d["a"]), d["p"])).collect::<Vec<_>>().join(" | ");
    if demandes != attendu {
        return Some(format!("{nom} : demandes\n  Rust : {}\n  PS   : {}", court(&demandes), court(&attendu)));
    }
    if lu_fps != c["lectures"]["fps"].as_bool().unwrap_or(false) {
        return Some(format!("{nom} : mesure des FPS lue : {lu_fps}"));
    }

    // Le journal
    let jeux: Vec<String> = liste(&c["jeux"]).iter().map(|j| texte(&j["nom"])).collect();
    let journal: Vec<String> = std::fs::read_to_string(ctx.fichier_journal())
        .unwrap_or_default()
        .lines()
        .map(|l| l.split_once(' ').map(|x| x.1).unwrap_or("").to_string())
        .collect();
    let ps: Vec<String> = liste(&c["journal"]).iter().map(texte).collect();
    let (rv, ra) = lignes(&journal, &jeux);
    let (pv, pa) = lignes(&ps, &jeux);
    if rv != pv {
        return Some(format!("{nom} : état de la veille au journal\n  Rust : {rv:?}\n  PS   : {pv:?}"));
    }
    if ra != pa {
        return Some(format!("{nom} : journal\n  Rust : {ra:?}\n  PS   : {pa:?}"));
    }

    // Les jeux guettés, les parties enregistrées
    let veille = liste(&ctx.reglage("FpsVeille").unwrap_or(Value::Null));
    if veille != liste(&c["fpsVeille"]) {
        return Some(format!("{nom} : FpsVeille {veille:?} au lieu de {:?}", c["fpsVeille"]));
    }
    let parties = match std::fs::read_to_string(ctx.donnees.join("fps.json")) {
        Ok(t) => liste(&serde_json::from_str(t.trim_start_matches('\u{feff}')).unwrap()),
        Err(_) => Vec::new(),
    };
    let attendues = liste(&c["parties"]);
    if parties.len() != attendues.len() {
        return Some(format!("{nom} : {} partie(s) dans fps.json au lieu de {}", parties.len(), attendues.len()));
    }
    for (r, p) in parties.iter().zip(&attendues) {
        for k in ["Game", "Key", "Avg", "Low1", "Low01", "Seconds", "Frames", "Exclusive", "MenuSec", "MenuFps"] {
            if !meme_nombre(&r[k], &p[k]) {
                return Some(format!("{nom} : fps.json {k} = {} au lieu de {}", r[k], p[k]));
            }
        }
        let (rs, psr) = (liste(&r["Series"]), liste(&p["Series"]));
        if rs.len() != psr.len() || rs.iter().zip(&psr).any(|(a, b)| !meme_nombre(a, b)) {
            return Some(format!("{nom} : fps.json Series = {rs:?} au lieu de {psr:?}"));
        }
        if !r["Id"].is_string() || !r["Date"].is_string() {
            return Some(format!("{nom} : fps.json sans Id ni Date"));
        }
    }
    drop(serveur);
    let _ = std::fs::remove_dir_all(&d);
    None
}

#[test]
fn meme_suite_de_demandes_que_powershell() {
    let j = juge();
    let cas = liste(&j["cas"]);
    assert!(cas.len() >= 8, "juge incomplet");
    let echecs: Vec<String> = cas.iter().enumerate().filter_map(|(i, c)| rejouer(c, i)).collect();
    assert!(echecs.is_empty(), "{} cas sur {} différents :\n{}", echecs.len(), cas.len(), echecs.join("\n"));
}

/// Contre-épreuve : un juge changé d'un rien (une demande en moins, un autre nom guetté, une autre moyenne) doit
/// faire échouer le cas
#[test]
fn contre_epreuve() {
    let j = juge();
    let cas = liste(&j["cas"]);
    let bf6 = cas.iter().position(|c| texte(&c["nom"]).starts_with("BF6 lancé")).unwrap();
    let mut a = cas[bf6].clone();
    // (la veille de la trace réseau oubliée : le défaut de la 2.0.0)
    let l: Vec<Value> = liste(&a["demandes"]).into_iter().filter(|d| d["a"] != "flux.veiller").collect();
    a["demandes"] = Value::Array(l);
    assert!(rejouer(&a, 100).is_some_and(|e| e.contains("demandes")));
    let mut b = cas[bf6].clone();
    b["parties"][0]["Avg"] = json!(b["parties"][0]["Avg"].as_f64().unwrap() + 0.1);
    assert!(rejouer(&b, 101).is_some_and(|e| e.contains("Avg")));
    let mut c = cas[0].clone();
    c["demandes"][0]["p"]["Noms"][0] = json!("bf7.exe");
    assert!(rejouer(&c, 102).is_some());
}

/// La cause de « aucune partie mesurée » en 2.0.0 : le dossier de l'appli donné aux blocs était celui de Mindset.exe,
/// et PresentMon y était cherché (`<dossier>\outils-tiers\PresentMon.exe`) au lieu de `fichiers\outils-tiers`, où
/// le zip le met (outils\contenu-rust.ps1) et où l'Aide le lance. Le bloc perf s'arrêtait là, sans rien au journal.
#[test]
fn presentmon_cherche_la_ou_le_zip_le_met() {
    let racine = std::path::Path::new(r"D:\Mindset\Programmes\Nevermind");
    let pm = nevermind_appli::programme::dossier_appli(racine).join("outils-tiers").join("PresentMon.exe");
    let contenu = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("outils").join("contenu-rust.ps1"),
    )
    .unwrap();
    let dans_zip = r"'fichiers\outils-tiers\PresentMon.exe'";
    assert!(contenu.contains(dans_zip), "contenu-rust.ps1 ne met plus PresentMon au même endroit");
    assert_eq!(pm, racine.join(r"fichiers\outils-tiers\PresentMon.exe"));
}
