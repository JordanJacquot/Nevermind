//! Le bloc entier contre un faux Windows (jamais le vrai PC, jamais le réseau) : actions de l'écran, panneaux,
//! signaux, fichiers écrits dans un dossier d'essai, et le rythme de la surveillance (légèreté).

use nevermind_noyau::{Bloc, Contexte, Effet, Effets, dossier_essai};
use nevermind_trafic::BlocTrafic;
use nevermind_trafic::lag::pings::Pingeur;
use nevermind_trafic::lag::session::Depart;
use nevermind_trafic::microsoft::LecturesWindows;
use nevermind_trafic::systeme::Systeme;
use nevermind_trafic::trafic::etat::Service;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct FauxWindows {
    lectures_tcp: AtomicU64,
    http: Mutex<Vec<String>>,
}

impl LecturesWindows for FauxWindows {
    fn reg_nombre(&self, chemin: &str, nom: &str) -> Option<i64> {
        match (chemin.ends_with("AdvertisingInfo"), nom) {
            (true, "Enabled") => Some(1),
            _ if nom == "AllowTelemetry" => Some(1),
            _ => Some(0),
        }
    }
    fn reg_texte(&self, _: &str, _: &str) -> String {
        "{ABCDEF12-0000}".into()
    }
    fn service(&self, _: &str) -> Option<(bool, String)> {
        Some((true, "Automatic".into()))
    }
    fn compte_microsoft(&self) -> bool {
        false
    }
}

struct PasDePing;
impl Pingeur for PasDePing {
    fn ping(&self, _: &str) -> Option<f64> {
        Some(12.0)
    }
    fn trace(&self, _: &str, _: u8, _: u32) -> Vec<String> {
        Vec::new()
    }
}

impl Systeme for FauxWindows {
    fn tcp(&self) -> (Vec<String>, bool) {
        self.lectures_tcp.fetch_add(1, Ordering::Relaxed);
        (vec!["7000004|20.1.2.3|4444|50000|5|300|200".into(), "7000008|13.107.4.50|443|50001|5|10|10".into()], false)
    }
    fn udp(&self) -> Vec<String> {
        Vec::new()
    }
    fn processus(&self) -> Vec<(u32, u32, String)> {
        vec![(7000004, 1, "outil".into()), (7000008, 1, "chrome".into())]
    }
    fn chemin(&self, pid: u32) -> String {
        if pid == 7000004 {
            r"C:\Users\Joueur\Downloads\outil.exe".into()
        } else {
            r"C:\Program Files\Google\Chrome\Application\chrome.exe".into()
        }
    }
    fn services(&self) -> HashMap<u32, Vec<Service>> {
        HashMap::new()
    }
    fn cache_dns(&self) -> Vec<(String, String)> {
        vec![("13.107.4.50".into(), "v10.events.data.microsoft.com".into())]
    }
    fn signature(&self, chemin: &str) -> (String, String) {
        if chemin.contains("chrome") {
            ("Valid".into(), "Google LLC".into())
        } else {
            ("NotSigned".into(), String::new())
        }
    }
    fn description(&self, _: &str) -> String {
        String::new()
    }
    fn reseau(&self) -> Option<Depart> {
        Some(Depart { passerelle: "192.168.1.254".into(), wifi: false, carte: "Ethernet Intel".into() })
    }
    fn pingeur(&self) -> Arc<dyn Pingeur> {
        Arc::new(PasDePing)
    }
    fn wifi(&self) -> Vec<String> {
        Vec::new()
    }
    fn http(&self, url: &str, _: &str) -> Result<String, String> {
        self.http.lock().unwrap().push(url.into());
        Err("pas de réseau dans les tests".into())
    }
    fn nom_inverse(&self, _: &str) -> String {
        String::new()
    }
}

fn contexte(nom: &str) -> Contexte {
    let d = dossier_essai(nom);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Contexte::nouveau(d.clone(), d, false, true)
}

fn attendre_travaux(b: &mut BlocTrafic, ctx: &Contexte, e: &mut Effets) {
    // (les signatures et le cache DNS arrivent en arrière-plan : un passage plus tard)
    std::thread::sleep(std::time::Duration::from_millis(150));
    let _ = b.action(ctx, "clic:BtnTraffic", &Value::Null, e);
    let _ = b.action(ctx, "clic:BtnTraffic", &Value::Null, e);
}

#[test]
fn surveillance_alertes_et_panneaux() {
    let ctx = contexte("bloc-trafic");
    let w = Arc::new(FauxWindows::default());
    let mut b = BlocTrafic::avec_systeme(w.clone());
    let mut e = Effets::default();
    // Avant : rien ne tourne, aucune lecture
    let p = b.pages(&ctx, "trafic");
    assert_eq!(p["trafic"]["module"]["titre"], "Surveillance arrêtée");
    assert_eq!(p["trafic"]["module"]["boutons"][0]["texte"], "Démarrer la surveillance");
    assert!(b.tic(&ctx, false, &mut e).is_some());
    assert_eq!(w.lectures_tcp.load(Ordering::Relaxed), 0, "une lecture sans surveillance");
    // Démarrée
    assert!(b.action(&ctx, "clic:BtnTraffic", &Value::Null, &mut e));
    assert!(e.0.contains(&Effet::Annonce("Surveillance de ce qui sort du PC activée.".into())));
    attendre_travaux(&mut b, &ctx, &mut e);
    let p = b.pages(&ctx, "trafic");
    let t = &p["trafic"];
    assert_eq!(t["module"]["titre"], "1 à vérifier", "{t}");
    let blocs = t["blocs"].as_array().unwrap();
    let al = blocs.iter().find(|x| x["icone"] == "alerte").expect("alerte");
    assert!(al["note"].as_str().unwrap().contains("port 4444"), "{al}");
    // Une bulle pour l'alerte, une seule fois
    let bulles =
        |e: &Effets| e.0.iter().filter(|x| matches!(x, Effet::Signal { nom, .. } if nom == "notification")).count();
    assert_eq!(bulles(&e), 1);
    // Le détail d'un programme dans le panneau
    let mut e2 = Effets::default();
    assert!(b.action(&ctx, "trafic-app:0", &Value::Null, &mut e2));
    assert!(e2.0.contains(&Effet::OuvrirEcran("panneau".into())));
    let p = b.pages(&ctx, "panneau");
    assert_eq!(p["panneau"]["parent"], "trafic");
    assert_eq!(p["panneau"]["blocs"][0]["texte"], "●  LE PROGRAMME");
    // « C'est normal » : une question, puis la confiance notée dans les réglages
    let mut e3 = Effets::default();
    assert!(b.action(&ctx, "trafic-al:0:0", &Value::Null, &mut e3));
    let q =
        e3.0.iter()
            .find_map(|x| match x {
                Effet::Signal { nom, donnees } if nom == "question" => Some(donnees.clone()),
                _ => None,
            })
            .expect("question");
    assert!(b.action(&ctx, q["action"].as_str().unwrap(), &json!({ "id": q["id"], "valeur": "oui" }), &mut e3));
    let conf = ctx.reglage("TrafficTrusted").expect("marque");
    assert!(conf[0].as_str().unwrap().starts_with(r"c:\users\joueur\downloads\outil.exe|"), "{conf}");
    let p = b.pages(&ctx, "trafic");
    assert_eq!(p["trafic"]["module"]["titre"], "Rien d'anormal");
    // Ce que Windows envoie à Microsoft : l'identifiant publicitaire activé, « Couper » passe par la sauvegarde
    let mut e4 = Effets::default();
    assert!(b.action(&ctx, "trafic-microsoft", &Value::Null, &mut e4));
    let p = b.pages(&ctx, "panneau");
    let pm = &p["panneau"];
    assert_eq!(pm["titre"], "Ce que Windows envoie à Microsoft");
    let tout =
        pm["blocs"].as_array().unwrap().iter().find(|x| x["texte"] == "Tout couper").expect("Tout couper").clone();
    assert!(b.action(&ctx, tout["id"].as_str().unwrap(), &Value::Null, &mut e4));
    let s =
        e4.0.iter()
            .find_map(|x| match x {
                Effet::Signal { nom, donnees } if nom == "sauvegarde-appliquer" => Some(donnees.clone()),
                _ => None,
            })
            .expect("sauvegarde");
    assert!(s["Ecritures"].as_array().unwrap().iter().any(|x| x["Name"] == "Enabled" && x["Value"] == 0));
    // Fermer le panneau : retour à la page d'où il a été ouvert
    let mut e5 = Effets::default();
    assert!(b.action(&ctx, "panneau:close", &Value::Null, &mut e5));
    assert!(e5.0.contains(&Effet::OuvrirEcran("trafic".into())));
    assert!(!b.action(&ctx, "panneau:close", &Value::Null, &mut e5), "un panneau d'un autre bloc pris par celui-ci");
    // Annuaire et pays coupés par défaut : aucune demande à rdap.org ni à ipinfo.io
    assert!(w.http.lock().unwrap().is_empty());
    // Le rythme : 2 s sur la page, 5 s ailleurs, 10 s caché, 30 s pendant une partie
    let _ = b.pages(&ctx, "trafic");
    assert_eq!(b.delai_trafic(), 2.0);
    let _ = b.pages(&ctx, "accueil");
    assert_eq!(b.delai_trafic(), 5.0);
    let _ = b.tic(&ctx, true, &mut e);
    assert_eq!(b.delai_trafic(), 10.0);
    b.signal(&ctx, "partie-commencee", &json!({ "jeu": "Jeu", "exe": "jeu.exe", "pid": 99 }), &mut e);
    assert_eq!(b.delai_trafic(), 30.0);
    assert!(b.mesure_en_cours(), "la mesure du lag démarre avec la partie");
    b.signal(&ctx, "partie-finie", &json!({ "jeu": "Jeu", "exe": "jeu.exe", "pid": 99, "duree_s": 10 }), &mut e);
    assert!(!b.mesure_en_cours());
    b.fermer(&ctx);
    let _ = std::fs::remove_dir_all(&ctx.donnees);
}

#[test]
fn lag_mesure_coupee_et_mesures_gardees() {
    let ctx = contexte("bloc-lag");
    // Une mesure ancienne (serveur de connexion pris pour le serveur de la partie) : diagnostic refait une fois
    let vieille = json!([{ "Id": "ancienne01", "Date": "2026-10-01T20:00:00.0000000+02:00", "Game": "Jeu", "Quick": false, "Seconds": 600, "Etw": false,
        "Server": { "Ip": "35.207.1.2", "Port": 443, "Proto": "tcp", "Owner": "", "Pays": "", "Ville": "" },
        "Stats": { "ref": { "Med": 12, "P95": 15, "Jit": 1, "Loss": 0, "Dead": false }, "srv": { "Med": 120, "P95": 125, "Jit": 1, "Loss": 0, "Dead": false } },
        "Diag": { "Level": "warn", "Title": "Le serveur du jeu est loin", "Findings": [{ "Status": "warn", "Title": "Le serveur du jeu est loin" }] } }]);
    std::fs::write(ctx.donnees.join("lag.json"), vieille.to_string()).unwrap();
    let mut b = BlocTrafic::avec_systeme(Arc::new(FauxWindows::default()));
    let mut e = Effets::default();
    let p = b.pages(&ctx, "jeux");
    let l = &p["jeux-lag"];
    assert_eq!(l["module"]["titre"], "Ta connexion était stable", "{l}");
    let relu: Value = serde_json::from_str(&std::fs::read_to_string(ctx.donnees.join("lag.json")).unwrap()).unwrap();
    assert_eq!(relu[0]["Diag"]["Findings"][1]["Title"], "Serveur de la partie non repéré");
    // Mesure coupée : la partie ne lance rien
    assert!(b.action(&ctx, "lag-mesure", &json!(false), &mut e));
    b.signal(&ctx, "partie-commencee", &json!({ "jeu": "Jeu", "exe": "jeu.exe", "pid": 99 }), &mut e);
    assert!(!b.mesure_en_cours());
    // Le test rapide se lance quand même, et le bouton disparaît pendant la mesure
    assert!(b.action(&ctx, "lag-test", &Value::Null, &mut e));
    assert!(b.mesure_en_cours());
    let p = b.pages(&ctx, "jeux");
    assert!(!p["jeux-lag"]["blocs"].as_array().unwrap().iter().any(|x| x["id"] == "lag-test"));
    // Le détail d'une mesure
    let mut e2 = Effets::default();
    assert!(b.action(&ctx, "lag:ancienne01", &Value::Null, &mut e2));
    let p = b.pages(&ctx, "panneau");
    assert_eq!(p["panneau"]["titre"], "Connexion : Jeu");
    assert_eq!(p["panneau"]["icone"], "ondes");
    b.fermer(&ctx);
    assert!(!b.mesure_en_cours());
    let _ = std::fs::remove_dir_all(&ctx.donnees);
}
