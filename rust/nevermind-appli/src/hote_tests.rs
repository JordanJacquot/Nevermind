//! Les tests de l'hôte : les juges exportés de la version PowerShell (tests\juges\hote-accueil.json, par
//! outils\exporter-juges-hote.ps1), puis la navigation, le panneau, les gestes, les effets, la veille.

use super::*;
use crate::vie::demarrage::Suite;
use std::cell::RefCell;
use std::rc::Rc;

/// Ce que la page a reçu.
#[derive(Default)]
struct Page {
    etats: Vec<Value>,
    ordres: Vec<Value>,
    /// Les deux, dans l'ordre reçu
    suite: Vec<Value>,
}
struct EcranEssai(Rc<RefCell<Page>>);
impl Ecran for EcranEssai {
    fn etat(&mut self, e: Value) {
        self.0.borrow_mut().suite.push(
            json!({ "etat": e.get("pages").map(|p| p.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>())) }),
        );
        self.0.borrow_mut().etats.push(e);
    }
    fn ordre(&mut self, o: Value) {
        self.0.borrow_mut().suite.push(o.clone());
        self.0.borrow_mut().ordres.push(o);
    }
}

/// Un partage entre le test et un bloc factice (un bloc doit pouvoir changer de fil : `Send`).
#[derive(Default)]
struct P<T>(std::sync::Arc<std::sync::Mutex<T>>);
impl<T> Clone for P<T> {
    fn clone(&self) -> Self {
        P(std::sync::Arc::clone(&self.0))
    }
}
impl<T> P<T> {
    fn new(v: T) -> Self {
        P(std::sync::Arc::new(std::sync::Mutex::new(v)))
    }
    fn borrow(&self) -> std::sync::MutexGuard<'_, T> {
        self.0.lock().unwrap()
    }
    fn borrow_mut(&self) -> std::sync::MutexGuard<'_, T> {
        self.0.lock().unwrap()
    }
}

/// Un bloc factice : des pages données, des actions qu'il prend, des effets qu'il demande.
#[derive(Default)]
struct Factice {
    pages: P<Map<String, Value>>,
    prend: Vec<String>,
    effets: P<Vec<Effet>>,
    recus: P<Vec<String>>,
    tics: P<Vec<bool>>,
}
impl Bloc for Factice {
    fn nom(&self) -> &'static str {
        "factice"
    }
    fn pages(&mut self, _: &Contexte, _: &str) -> Map<String, Value> {
        self.pages.borrow().clone()
    }
    fn action(&mut self, _: &Contexte, id: &str, v: &Value, effets: &mut Effets) -> bool {
        self.recus.borrow_mut().push(format!("action {id} {v}"));
        if self.prend.iter().any(|p| p == id) {
            for e in self.effets.borrow_mut().drain(..) {
                effets.ajouter(e);
            }
            return true;
        }
        false
    }
    fn tic(&mut self, _: &Contexte, cache: bool, _: &mut Effets) -> Option<Duration> {
        self.tics.borrow_mut().push(cache);
        Some(Duration::from_secs(1))
    }
    fn signal(&mut self, _: &Contexte, nom: &str, d: &Value, _: &mut Effets) {
        self.recus.borrow_mut().push(format!("signal {nom} {d}"));
    }
}

fn contexte(nom: &str) -> Contexte {
    let d = nevermind_noyau::dossier_essai(&format!("hote-{nom}"));
    let _ = std::fs::create_dir_all(&d);
    Contexte::nouveau(d.clone(), d, true, true)
}

fn hote(blocs: Vec<Box<dyn Bloc>>, nom: &str) -> Hote {
    let t = suite_commun::themes::Theme::neon();
    Hote::nouveau(
        blocs,
        contexte(nom),
        Habillage::depuis(&t),
        "Salut Jordan".into(),
        "1.15.6",
        Images::default(),
        Box::new(|| "cable".into()),
    )
}

// ---------------------------------------------------------------------------------------------------------------
// Les juges
// ---------------------------------------------------------------------------------------------------------------

/// Les adresses propres à chaque version ramenées à une seule : `https://theme.local/` (PowerShell) et
/// `http://pack.localhost/` (Rust) ; une image du PC (`https://fichier.local/<jeton>` et
/// `http://fichier.localhost/<jeton>`, jeton au hasard) devient « FICHIER.<ext> ».
fn normaliser(v: &mut Value) {
    match v {
        Value::String(s) => {
            if let Some(r) = s.strip_prefix("https://theme.local/") {
                *s = format!("http://pack.localhost/{r}");
            } else if let Some(r) =
                s.strip_prefix("https://fichier.local/").or_else(|| s.strip_prefix("http://fichier.localhost/"))
            {
                *s = format!("FICHIER.{}", r.rsplit('.').next().unwrap_or(""));
            }
        }
        Value::Array(l) => l.iter_mut().for_each(normaliser),
        Value::Object(o) => o.values_mut().for_each(normaliser),
        _ => {}
    }
}

/// Deux valeurs JSON pareilles, les nombres comparés comme des nombres (1 et 1.0), l'ordre des champs ignoré. Rend
/// le chemin de la première différence.
fn difference(a: &Value, b: &Value, chemin: &str) -> Option<String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x.as_f64() != y.as_f64()).then(|| format!("{chemin} : {x} / {y}")),
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                let (u, w) = (x.get(k), y.get(k));
                match (u, w) {
                    (Some(u), Some(w)) => {
                        if let Some(d) = difference(u, w, &format!("{chemin}.{k}")) {
                            return Some(d);
                        }
                    }
                    _ => return Some(format!("{chemin}.{k} : {u:?} / {w:?}")),
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{chemin} : {} éléments / {}", x.len(), y.len()));
            }
            x.iter().zip(y).enumerate().find_map(|(i, (u, w))| difference(u, w, &format!("{chemin}[{i}]")))
        }
        _ => (a != b).then(|| format!("{chemin} : {a} / {b}")),
    }
}

/// L'état que l'hôte Rust envoie pour l'entrée d'un juge.
fn etat_rust(entree: &Value, packs: &Path, n: usize) -> Value {
    let e = |k: &str| entree.get(k).cloned().unwrap_or(Value::Null);
    let choix = crate::theme::choisir(e("theme").as_str().unwrap_or(""), false, &Suite::default(), packs);
    let phrase = crate::theme::phrase(&choix.theme, e("prenom").as_str().unwrap_or(""));
    let mut blocs: Vec<Box<dyn Bloc>> = Vec::new();
    for (i, p) in e("blocs").as_array().cloned().unwrap_or_default().into_iter().enumerate() {
        let mut pages = p.as_object().cloned().unwrap_or_default();
        if i == 0 {
            pages.insert("_accueil".into(), e("faits"));
        }
        blocs.push(Box::new(Factice { pages: P::new(pages), ..Factice::default() }));
    }
    let connexion = e("connexion").as_str().unwrap_or("").to_string();
    let mut h = Hote::nouveau(
        blocs,
        contexte(&format!("juge-{n}")),
        Habillage::depuis(&choix.theme),
        phrase,
        "1.15.6",
        Images::default(),
        Box::new(move || connexion.clone()),
    );
    h.k.accrochee = e("accrochee").as_bool().unwrap_or(false);
    h.k.pret = e("pret").as_bool().unwrap_or(false);
    h.k.retour_ecran = e("retour").as_str().map(str::to_string);
    let mut v = h.etat_initial(Duration::ZERO);
    normaliser(&mut v);
    v
}

/// Les packs d'essai du juge, écrits dans un dossier d'essai.
fn ecrire_packs(juge: &Value, nom: &str) -> std::path::PathBuf {
    let d = nevermind_noyau::dossier_essai(&format!("hote-juge-packs-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    for (id, p) in juge["packs"].as_object().cloned().unwrap_or_default() {
        let dp = d.join(&id);
        std::fs::create_dir_all(&dp).unwrap();
        for f in p["Fichiers"].as_array().cloned().unwrap_or_default() {
            std::fs::write(dp.join(f.as_str().unwrap()), [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0])
                .unwrap();
        }
        std::fs::write(dp.join("pack.json"), p["Json"].to_string()).unwrap();
    }
    d
}

fn lire_juge(f: &Path) -> Value {
    let t = std::fs::read_to_string(f).unwrap_or_else(|e| panic!("{} : {e}", f.display()));
    serde_json::from_str(t.trim_start_matches('\u{feff}')).expect("juge lisible")
}

/// Rejoue un juge ; rend les différences (cas, chemin).
fn rejouer(juge: &Value, nom: &str) -> Vec<String> {
    let packs = ecrire_packs(juge, nom);
    let mut diffs = Vec::new();
    for (n, c) in juge["cas"].as_array().expect("cas").iter().enumerate() {
        let r = etat_rust(&c["entree"], &packs, n);
        let mut ps = c["sortie"].clone();
        normaliser(&mut ps);
        if let Some(d) = difference(&ps, &r, "") {
            diffs.push(format!("cas {n} ({}) : {d}", c["entree"]["theme"]));
        }
    }
    let _ = std::fs::remove_dir_all(&packs);
    diffs
}

fn juge_accueil() -> Value {
    lire_juge(&Path::new(env!("CARGO_MANIFEST_DIR")).join(r"tests\juges\hote-accueil.json"))
}

#[test]
fn juges_de_l_accueil() {
    let j = juge_accueil();
    let n = j["cas"].as_array().map_or(0, Vec::len);
    assert!(n >= 300, "{n} cas seulement");
    let d = rejouer(&j, "accueil");
    assert!(
        d.is_empty(),
        "{} cas sur {n} différents :\n{}",
        d.len(),
        d.iter().take(15).cloned().collect::<Vec<_>>().join("\n")
    );
}

/// Un juge fait ailleurs (une autre graine : des cas jamais vus), donné par `NEVERMIND_JUGE_HOTE`.
#[test]
fn juge_d_ailleurs() {
    let Ok(f) = std::env::var("NEVERMIND_JUGE_HOTE") else { return };
    let j = lire_juge(Path::new(&f));
    let d = rejouer(&j, "ailleurs");
    assert!(d.is_empty(), "{} différents :\n{}", d.len(), d.iter().take(15).cloned().collect::<Vec<_>>().join("\n"));
}

/// Contre-épreuve : un seul changement dans la sortie attendue fait échouer le juge.
#[test]
fn contre_epreuve() {
    let base = juge_accueil();
    let un = |chemin: &[&str], v: Value| {
        let mut j = base.clone();
        j["cas"] = json!([base["cas"][0].clone()]);
        let mut x = &mut j["cas"][0]["sortie"];
        for c in chemin {
            x = &mut x[*c];
        }
        *x = v;
        rejouer(&j, "contre")
    };
    assert!(un(&[], base["cas"][0]["sortie"].clone()).is_empty(), "le cas tel quel passe");
    for (chemin, v) in [
        (&["app", "phrase"][..], json!("Salut Jordam")),
        (&["accueil", "tuiles"], json!([])),
        (&["connexion"], json!("wifi2")),
        (&["focus"], json!("t:pc")),
        (&["theme"], json!("rubis")),
    ] {
        assert!(!un(chemin, v).is_empty(), "{chemin:?} changé : le juge doit échouer");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Les mots de l'accueil, à la main (au-delà des juges)
// ---------------------------------------------------------------------------------------------------------------

#[test]
fn format_n0_francais() {
    assert_eq!(format_n0(143.6), "144");
    assert_eq!(format_n0(60.5), "61", "arrondi loin de zéro");
    assert_eq!(format_n0(1234.5), "1\u{202F}235");
    assert_eq!(format_n0(1_234_567.0), "1\u{202F}234\u{202F}567");
}

#[test]
fn objets_vides_en_null_comme_powershell() {
    let mut v = json!({ "a": {}, "b": [{}, { "c": {} }], "d": { "e": 1 } });
    vides_en_null(&mut v);
    assert_eq!(v, json!({ "a": null, "b": [{}, { "c": null }], "d": { "e": 1 } }));
}

#[test]
fn cle_sans_les_mesures_en_direct() {
    let e = |v: i64| {
        json!({ "pages": { "pc": { "blocs": [{ "type": "mesures", "elements": [{ "id": "cpu", "valeur": v }] }],
        "sections": [{ "blocs": [{ "type": "mesures", "elements": [{ "id": "gpu", "valeur": v }] }] }] } } })
    };
    assert_eq!(cle_etat(&e(10)), cle_etat(&e(90)), "une mesure qui varie ne redessine pas la page");
    assert_ne!(cle_etat(&e(10)), cle_etat(&json!({ "pages": {} })));
}

// ---------------------------------------------------------------------------------------------------------------
// Gestes, effets, panneau, veille
// ---------------------------------------------------------------------------------------------------------------

fn action(id: &str, ecran: &str) -> MessagePage {
    MessagePage::Action { id: id.into(), valeur: Value::Null, ecran: ecran.into() }
}

#[test]
fn gestes_donnes_aux_blocs_dans_l_ordre() {
    let premier = Factice { prend: vec!["clic:BtnPing".into()], ..Factice::default() };
    let (r1, r2) = (premier.recus.clone(), P::new(Vec::new()));
    let second =
        Factice { prend: vec!["clic:BtnPing".into(), "analyser".into()], recus: r2.clone(), ..Factice::default() };
    let mut h = hote(vec![Box::new(premier), Box::new(second)], "gestes");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    h.recevoir(action("clic:BtnPing", "accueil"), Duration::ZERO, &mut e);
    assert_eq!(r1.borrow().last().unwrap(), "action clic:BtnPing null");
    assert!(!r2.borrow().iter().any(|x| x.starts_with("action clic")), "pris par le premier : pas donné au second");
    h.recevoir(action("analyser", "pc"), Duration::ZERO, &mut e);
    assert!(r2.borrow().iter().any(|x| x == "action analyser null"));
    // page:<n> : la page du kit (accueil-console.ps1:212, Show-Page)
    h.recevoir(action("page:9", "accueil"), Duration::ZERO, &mut e);
    assert_eq!(p.borrow().ordres.last(), Some(&json!({ "kit": "aller", "ecran": "jeux" })));
    // L'écran montré est dit aux blocs
    h.recevoir(MessagePage::Ecran { id: "jeux".into() }, Duration::ZERO, &mut e);
    assert!(r1.borrow().iter().any(|x| x == r#"signal ecran {"id":"jeux"}"#));
}

#[test]
fn effets_des_blocs() {
    let b = Factice { prend: vec!["x".into()], ..Factice::default() };
    let eff = b.effets.clone();
    let autre = Factice::default();
    let recu = autre.recus.clone();
    let mut h = hote(vec![Box::new(b), Box::new(autre)], "effets");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    // Pas encore prête : l'écran demandé attend (`ecran` de l'état), et l'annonce est perdue (Send-OrdreVue)
    eff.borrow_mut().push(Effet::OuvrirEcran("jeux".into()));
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    assert_eq!(h.retour_ecran(), Some("jeux"));
    assert_eq!(p.borrow().etats.last().unwrap()["ecran"], "jeux");
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    assert_eq!(h.retour_ecran(), None, "il ne sert qu'une fois");
    assert_eq!(p.borrow().etats.last().unwrap()["ecran"], Value::Null);
    eff.borrow_mut().extend([
        Effet::Annonce("Analyse de ton PC lancée".into()),
        Effet::OuvrirAdresse("https://exemple.fr".into()),
        Effet::Signal { nom: "partie-commencee".into(), donnees: json!({ "pid": 4 }) },
        Effet::Quitter,
    ]);
    let d = h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    assert_eq!(
        p.borrow().ordres.last(),
        Some(&json!({ "kit": "message", "texte": "Analyse de ton PC lancée", "bravo": false }))
    );
    assert_eq!(d, vec![Demande::OuvrirAdresse("https://exemple.fr".into()), Demande::Partie(true), Demande::Quitter]);
    assert!(h.partie());
    assert!(recu.borrow().iter().any(|x| x == r#"signal partie-commencee {"pid":4}"#), "le signal va aux autres blocs");
}

#[test]
fn panneau_ouvert_puis_ferme() {
    let b = Factice { prend: vec!["panneau:close".into()], ..Factice::default() };
    let (pages, recus) = (b.pages.clone(), b.recus.clone());
    let mut h = hote(vec![Box::new(b)], "panneau");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    h.recevoir(MessagePage::Ecran { id: "tests".into() }, Duration::ZERO, &mut e);
    pages.borrow_mut().insert("panneau".into(), json!({ "titre": "Test du disque", "numero": 1, "blocs": [] }));
    h.recevoir(action("clic:BtnTest", "tests"), Duration::ZERO, &mut e);
    let etat = p.borrow().etats.last().unwrap().clone();
    assert_eq!(
        etat["pages"]["panneau"],
        json!({ "titre": "Test du disque", "blocs": [], "retour": "appli", "parent": "tests" })
    );
    assert_eq!(p.borrow().ordres.last(), Some(&json!({ "kit": "aller", "ecran": "panneau" })));
    h.recevoir(MessagePage::Ecran { id: "panneau".into() }, Duration::ZERO, &mut e);
    // Retour (B) sur le panneau : son bouton Fermer
    h.recevoir(action("retour", "panneau"), Duration::ZERO, &mut e);
    assert!(recus.borrow().iter().any(|x| x == "action panneau:close null"));
    // Le bloc le ferme : retour à la page d'où il venait
    pages.borrow_mut().remove("panneau");
    h.mettre_a_jour(Duration::from_secs(1), &mut e);
    assert_eq!(p.borrow().ordres.last(), Some(&json!({ "kit": "aller", "ecran": "tests" })));
    assert!(p.borrow().etats.last().unwrap()["pages"].get("panneau").is_none());
    // Rouvert depuis l'accueil, puis quitté par une autre page : fermé comme par Retour (Close-QuitteKit)
    h.recevoir(MessagePage::Ecran { id: "accueil".into() }, Duration::ZERO, &mut e);
    pages.borrow_mut().insert("panneau".into(), json!({ "titre": "Ping", "numero": 2 }));
    h.mettre_a_jour(Duration::from_secs(2), &mut e);
    assert_eq!(
        p.borrow().etats.last().unwrap()["pages"]["panneau"]["parent"],
        Value::Null,
        "depuis l'accueil : pas de parent"
    );
    h.recevoir(MessagePage::Ecran { id: "panneau".into() }, Duration::ZERO, &mut e);
    let avant = recus.borrow().iter().filter(|x| *x == "action panneau:close null").count();
    h.recevoir(MessagePage::Ecran { id: "fiche".into() }, Duration::ZERO, &mut e);
    assert_eq!(
        recus.borrow().iter().filter(|x| *x == "action panneau:close null").count(),
        avant,
        "vers la fiche : gardé"
    );
    h.recevoir(MessagePage::Ecran { id: "reseau".into() }, Duration::ZERO, &mut e);
    assert_eq!(recus.borrow().iter().filter(|x| *x == "action panneau:close null").count(), avant + 1);
}

#[test]
fn rien_n_est_envoye_quand_personne_ne_regarde() {
    let b = Factice::default();
    let (pages, tics) = (b.pages.clone(), b.tics.clone());
    let mut h = hote(vec![Box::new(b)], "veille");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    h.minuterie(Duration::from_millis(200), &mut e);
    assert_eq!(*tics.borrow(), vec![false], "regardée : tic normal");
    h.regardee(false, Duration::from_millis(300), &mut e);
    let n = p.borrow().etats.len();
    pages.borrow_mut().insert("pc".into(), json!({ "titre": "change" }));
    for i in 2..60 {
        h.minuterie(Duration::from_millis(200 * i), &mut e);
    }
    h.mettre_a_jour(Duration::from_secs(13), &mut e);
    assert_eq!(p.borrow().etats.len(), n, "caché : rien n'est refait ni envoyé");
    assert!(tics.borrow()[1..].iter().all(|c| *c), "caché : les tics le savent");
    h.recevoir(action("x", "accueil"), Duration::from_secs(13), &mut e);
    h.minuterie(Duration::from_secs(14), &mut e);
    assert!(p.borrow().ordres.is_empty(), "les ordres attendent");
    h.regardee(true, Duration::from_secs(15), &mut e);
    assert_eq!(p.borrow().etats.last().unwrap()["pages"]["pc"]["titre"], "change", "refait au retour");
}

#[test]
fn quitter_ou_reduire() {
    let mut h = hote(vec![], "quitter");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    // Pas d'écran : quitte tout de suite
    let d = h.recevoir(action("fermer", "accueil"), Duration::ZERO, &mut e);
    assert_eq!(d, vec![Demande::Quitter]);
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    h.recevoir(action("fermer", "accueil"), Duration::ZERO, &mut e);
    let q = p.borrow().ordres.last().unwrap().clone();
    assert_eq!(q["kit"], "question");
    assert_eq!(q["titre"], "Quitter Nevermind ?");
    let id = q["id"].as_str().unwrap().to_string();
    assert_eq!(
        h.recevoir(MessagePage::Reponse { id: "autre".into(), valeur: "quit".into() }, Duration::ZERO, &mut e),
        vec![]
    );
    assert_eq!(
        h.recevoir(MessagePage::Reponse { id, valeur: "tray".into() }, Duration::ZERO, &mut e),
        vec![Demande::Reduire]
    );
}

#[test]
fn menu_de_mindset_seulement_accroche() {
    let mut h = hote(vec![], "menu");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    assert!(h.recevoir(MessagePage::Menu, Duration::ZERO, &mut e).is_empty());
    h.accrocher(true, Duration::ZERO, &mut e);
    let ronds = p.borrow().etats.last().unwrap()["accueil"]["ronds"].as_array().unwrap().len();
    assert_eq!(ronds, 5, "Menu et Fermer dans Mindset");
    assert_eq!(h.recevoir(MessagePage::Menu, Duration::ZERO, &mut e), vec![Demande::MenuMindset]);
    assert_eq!(h.recevoir(action("menu", "accueil"), Duration::ZERO, &mut e), vec![Demande::MenuMindset]);
}

#[test]
fn mesures_en_direct_de_la_page_pc() {
    let b = Factice::default();
    let pages = b.pages.clone();
    pages.borrow_mut().insert(
        "pc".into(),
        json!({ "blocs": [{ "type": "mesures", "elements": [{ "id": "cpu", "nom": "CPU", "valeur": 12, "texte": "12 %" }, { "nom": "sans id" }] }] }),
    );
    let mut h = hote(vec![Box::new(b)], "mesures");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    h.recevoir(MessagePage::Ecran { id: "pc".into() }, Duration::ZERO, &mut e);
    let n = p.borrow().etats.len();
    for i in 1..=8 {
        h.minuterie(Duration::from_millis(200 * i), &mut e);
    }
    assert_eq!(
        p.borrow().ordres.last(),
        Some(
            &json!({ "kit": "mesures", "valeurs": { "cpu": { "valeur": 12, "texte": "12 %", "note": null, "ton": "" } } })
        )
    );
    pages.borrow_mut()["pc"]["blocs"][0]["elements"][0]["valeur"] = json!(80);
    h.mettre_a_jour(Duration::from_secs(2), &mut e);
    assert_eq!(p.borrow().etats.len(), n, "une mesure qui change ne redessine pas la page");
}

#[test]
fn un_bloc_qui_plante_n_arrete_rien() {
    struct Casse;
    impl Bloc for Casse {
        fn nom(&self) -> &'static str {
            "casse"
        }
        fn pages(&mut self, _: &Contexte, _: &str) -> Map<String, Value> {
            panic!("pages cassées")
        }
        fn action(&mut self, _: &Contexte, _: &str, _: &Value, _: &mut Effets) -> bool {
            panic!("action cassée")
        }
        fn tic(&mut self, _: &Contexte, _: bool, _: &mut Effets) -> Option<Duration> {
            panic!("tic cassé")
        }
    }
    let b = Factice::default();
    b.pages.borrow_mut().insert("pc".into(), json!({ "titre": "ok" }));
    let mut h = hote(vec![Box::new(Casse), Box::new(b)], "casse");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    h.minuterie(Duration::from_millis(200), &mut e);
    assert_eq!(p.borrow().etats.last().unwrap()["pages"]["pc"]["titre"], "ok");
    assert_eq!(h.tics[0], Some(Duration::from_millis(200) + APRES_ERREUR));
}

#[test]
fn images_des_blocs_servies_par_jeton() {
    let b = Factice::default();
    b.pages.borrow_mut().insert(
        "jeux".into(),
        json!({ "blocs": [{ "image": r"fichier:C:\Jeux\a.jpg" }, { "image": "fichier:relatif.png" }] }),
    );
    let mut h = hote(vec![Box::new(b)], "images");
    let v = h.etat_initial(Duration::ZERO);
    let a = v["pages"]["jeux"]["blocs"][0]["image"].as_str().unwrap();
    assert!(a.starts_with("http://fichier.localhost/") && a.ends_with(".jpg"), "{a}");
    assert_eq!(v["pages"]["jeux"]["blocs"][1]["image"], "fichier:relatif.png", "un chemin relatif n'est pas une image");
}

// ---------------------------------------------------------------------------------------------------------------
// Les signaux communs (question, occupe, resultat, historique, sauvegarde-changee)
// ---------------------------------------------------------------------------------------------------------------

fn signal(nom: &str, d: Value) -> Effet {
    Effet::Signal { nom: nom.into(), donnees: d }
}

#[test]
fn question_d_un_bloc_et_sa_reponse() {
    let b = Factice { prend: vec!["x".into(), "memoire-reponse".into()], ..Factice::default() };
    let (eff, recus) = (b.effets.clone(), b.recus.clone());
    let mut h = hote(vec![Box::new(b)], "question");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    // Pas prête : pas posée, le dernier bouton tout de suite
    eff.borrow_mut().push(signal(
        "question",
        json!({ "id": "memoire-1", "texte": "Fermer ?", "action": "memoire-reponse",
        "boutons": [{ "id": "oui", "texte": "Oui" }, { "id": "non", "texte": "Non", "style": "second" }] }),
    ));
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    assert!(
        recus.borrow().iter().any(|r| r == r#"action memoire-reponse {"id":"memoire-1","valeur":"non"}"#),
        "{:?}",
        recus.borrow()
    );
    // Prête : posée, avec ses détails
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    eff.borrow_mut().push(signal("question", json!({ "id": "memoire-2", "titre": "", "texte": "Couper ?", "action": "memoire-reponse",
        "boutons": [{ "id": "oui", "texte": "Oui" }, { "id": "non", "texte": "Non" }], "details": { "titre": "Détails techniques", "lignes": ["a"] } })));
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    let q = p.borrow().ordres.iter().rev().find(|o| o["kit"] == "question").cloned().unwrap();
    assert_eq!(q["texte"], "Couper ?");
    assert_eq!(q["details"]["lignes"], json!(["a"]));
    assert_eq!(h.questions_en_attente(), 1);
    let kid = q["id"].as_str().unwrap().to_string();
    h.recevoir(MessagePage::Reponse { id: kid, valeur: "oui".into() }, Duration::ZERO, &mut e);
    assert!(recus.borrow().iter().any(|r| r == r#"action memoire-reponse {"id":"memoire-2","valeur":"oui"}"#));
    assert_eq!(h.questions_en_attente(), 0);
    // Posée puis abandonnée (la page se recharge) : le dernier bouton
    eff.borrow_mut().push(signal(
        "question",
        json!({ "id": "memoire-3", "texte": "?", "action": "memoire-reponse",
        "boutons": [{ "id": "forcer", "texte": "Forcer" }, { "id": "attendre", "texte": "Attendre" }] }),
    ));
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    assert!(recus.borrow().iter().any(|r| r == r#"action memoire-reponse {"id":"memoire-3","valeur":"attendre"}"#));
}

#[test]
fn pastille_occupe() {
    let b = Factice { prend: vec!["x".into()], ..Factice::default() };
    let eff = b.effets.clone();
    let mut h = hote(vec![Box::new(b)], "occupe");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    eff.borrow_mut().push(signal("occupe", json!({ "texte": "Analyse..." })));
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    assert_eq!(p.borrow().ordres.last(), Some(&json!({ "kit": "occupe", "texte": "Analyse..." })));
}

/// Petit du lot A : une pastille « en cours » par travail, et non une seule. Le point de restauration de Ton PC tourne
/// encore quand « Couper ce qui est sûr » (page RAM) finit : la pastille reste, avec le texte de ce qui tourne encore ;
/// elle ne disparaît que quand plus rien ne tourne. Avant : la fin de la RAM l'effaçait (l'utilisateur croyait que
/// c'était fini et pouvait quitter).
#[test]
fn une_pastille_par_travail() {
    let pc = Factice { prend: vec!["pc".into()], ..Factice::default() };
    let ram = Factice { prend: vec!["ram".into()], ..Factice::default() };
    let (e_pc, e_ram) = (pc.effets.clone(), ram.effets.clone());
    let mut h = hote(vec![Box::new(pc), Box::new(ram)], "pastilles");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    let occupe = |t: &str| json!({ "kit": "occupe", "texte": t });
    let derniere = |p: &Rc<RefCell<Page>>| p.borrow().ordres.iter().rev().find(|o| o["kit"] == "occupe").cloned();
    // (le texte d'un travail de fond porte sa clé, comme ceux que rend `Travaux`)
    let restauration = "Création du point de restauration (ça peut prendre une minute)...";
    e_pc.borrow_mut().push(signal("occupe", json!({ "texte": restauration, "travail": "pc" })));
    h.recevoir(action("pc", "optimisation"), Duration::ZERO, &mut e);
    e_ram.borrow_mut().push(signal("occupe", json!({ "texte": "Un instant..." })));
    h.recevoir(action("ram", "memoire"), Duration::ZERO, &mut e);
    assert_eq!(derniere(&p), Some(occupe("Un instant...")), "le plus récent est montré");
    e_ram.borrow_mut().push(signal("occupe", json!({ "texte": "" })));
    h.recevoir(action("ram", "memoire"), Duration::ZERO, &mut e);
    assert_eq!(
        derniere(&p),
        Some(occupe(restauration)),
        "la fin de la RAM a effacé la pastille du point de restauration"
    );
    e_pc.borrow_mut().push(signal("occupe", json!({ "texte": "" })));
    h.recevoir(action("pc", "optimisation"), Duration::ZERO, &mut e);
    assert_eq!(derniere(&p), Some(occupe("")), "plus rien ne tourne : la pastille s'en va");
}

#[test]
fn fiche_de_resultat_et_historique() {
    let b = Factice { prend: vec!["x".into(), "fiche:annuler".into()], ..Factice::default() };
    let (eff, recus) = (b.effets.clone(), b.recus.clone());
    let mut h = hote(vec![Box::new(b)], "resultat");
    let _ = std::fs::remove_file(h.ctx.donnees.join("historique.json"));
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    h.recevoir(MessagePage::Ecran { id: "optimisation".into() }, Duration::ZERO, &mut e);
    eff.borrow_mut().push(signal(
        "resultat",
        json!({ "titre": "Optimisation faite", "note": "Redémarre le PC.",
        "lignes": ["•  Mode jeu activé", "•  Plan performances", "Score : 70 → 80", "1. Redémarre"],
        "log": [{ "Type": "reg", "Path": "HKCU:\\x", "Name": "y" }] }),
    ));
    h.recevoir(action("x", "optimisation"), Duration::ZERO, &mut e);
    assert_eq!(p.borrow().ordres.last(), Some(&json!({ "kit": "aller", "ecran": "fiche" })));
    let f = p.borrow().etats.last().unwrap()["pages"]["fiche"].clone();
    assert_eq!(f["titre"], "Optimisation faite");
    assert_eq!(f["parent"], "optimisation");
    assert_eq!(f["blocs"][0], json!({ "type": "liste", "elements": ["Mode jeu activé", "Plan performances"] }));
    assert_eq!(f["blocs"][1], json!({ "type": "texte", "texte": "Score : 70 → 80" }));
    assert_eq!(f["blocs"][2], json!({ "type": "etapes", "elements": [{ "texte": "Redémarre" }] }));
    assert_eq!(f["blocs"][3], json!({ "type": "note", "texte": "Redémarre le PC." }));
    assert_eq!(f["blocs"][5]["id"], "fiche:ignore");
    assert_eq!(f["blocs"][6], json!({ "type": "bouton", "id": "fiche:close", "texte": "OK", "style": "second" }));
    // L'historique : « 2 changements : ... », en tête du fichier ; les blocs savent que la sauvegarde a changé
    let hist: Value =
        serde_json::from_str(&std::fs::read_to_string(h.ctx.donnees.join("historique.json")).unwrap()).unwrap();
    assert_eq!(hist[0]["Titre"], "2 changements : Mode jeu activé, Plan performances");
    assert!(hist[0]["Date"].as_str().is_some_and(|d| d.contains(" à ")), "{}", hist[0]["Date"]);
    assert_eq!(hist[0]["Annule"], false);
    assert!(recus.borrow().iter().any(|r| r == "signal sauvegarde-changee {}"));
    // Revenir en arrière : fermée, et l'annulation demandée aux blocs avec le journal des changements
    h.recevoir(MessagePage::Ecran { id: "fiche".into() }, Duration::ZERO, &mut e);
    h.recevoir(action("fiche:ignore", "fiche"), Duration::ZERO, &mut e);
    assert!(recus.borrow().iter().any(|r| r.starts_with("action fiche:annuler") && r.contains("\"historique\":\"")));
    assert_eq!(p.borrow().ordres.last(), Some(&json!({ "kit": "aller", "ecran": "optimisation" })));
    assert!(p.borrow().etats.last().unwrap()["pages"].get("fiche").is_none());
    // Sans journal : pas de « Revenir en arrière » ; quittée par une autre page, elle se ferme
    eff.borrow_mut().push(signal("resultat", json!({ "titre": "Rien", "lignes": [] })));
    h.recevoir(action("x", "optimisation"), Duration::ZERO, &mut e);
    let f = p.borrow().etats.last().unwrap()["pages"]["fiche"].clone();
    assert_eq!(f["blocs"].as_array().unwrap().len(), 1);
    h.recevoir(MessagePage::Ecran { id: "fiche".into() }, Duration::ZERO, &mut e);
    h.recevoir(MessagePage::Ecran { id: "reseau".into() }, Duration::ZERO, &mut e);
    assert!(p.borrow().etats.last().unwrap()["pages"].get("fiche").is_none());
    // Un signal « historique » sans journal ne fait rien ; avec, une ligne de plus (100 au plus)
    eff.borrow_mut().push(signal("historique", json!({ "titre": "Rien" })));
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    for i in 0..105 {
        eff.borrow_mut().push(signal(
            "historique",
            json!({ "titre": format!("Pare-feu {i}"), "details": ["a", ""], "log": [{ "Type": "fw" }] }),
        ));
        h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    }
    let hist: Value =
        serde_json::from_str(&std::fs::read_to_string(h.ctx.donnees.join("historique.json")).unwrap()).unwrap();
    assert_eq!(hist.as_array().unwrap().len(), 100);
    assert_eq!(hist[0]["Titre"], "Pare-feu 104");
    assert_eq!(hist[0]["Details"], json!(["a"]));
}

#[test]
fn sauvegarde_appliquer_notee_pour_revenir_en_arriere() {
    let b = Factice { prend: vec!["x".into()], ..Factice::default() };
    let (eff, recus) = (b.effets.clone(), b.recus.clone());
    let mut h = hote(vec![Box::new(b)], "sauvegarde-appliquer");
    let _ = std::fs::remove_file(h.ctx.donnees.join("historique.json"));
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    // (une clé de l'utilisateur : le contexte d'essai écrit dans son faux registre, jamais dans le vrai ; le service
    // d'essai est introuvable : aucune demande à l'Aide)
    let cle = r"HKCU:\Software\NevermindEssaiHote\Inexistante";
    eff.borrow_mut().push(signal(
        "sauvegarde-appliquer",
        json!({ "Titre": "Envois à Microsoft réduits", "Details": ["Pubs"],
        "Ecritures": [{ "Type": "reg", "Path": cle, "Name": "Enabled", "Value": 0, "Kind": "DWord" },
                      { "Type": "svc", "Name": "NevermindEssaiService", "Demarrage": "Disabled" }],
        "Message": "1 réglage(s) coupé(s). Annulable depuis la page Sauvegarde, historique." }),
    ));
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    // (les écritures passent par l'Aide, jusqu'à 2 min chacune : dans un travail de fond, audit G3 ; la fin est
    //  relevée par la minuterie)
    let debut = std::time::Instant::now();
    let mut t = Duration::ZERO;
    while h.ctx.historique().is_empty() {
        assert!(debut.elapsed() < Duration::from_secs(10), "sauvegarde-appliquer ne finit pas");
        std::thread::sleep(Duration::from_millis(20));
        t += Duration::from_millis(200);
        h.minuterie(t, &mut e);
    }
    assert_eq!(h.ctx.registre_etat(cle, "Enabled")["Value"], 0, "écrit (faux registre d'essai)");
    let hist = h.ctx.historique();
    assert_eq!(hist[0]["Titre"], "Envois à Microsoft réduits");
    assert_eq!(hist[0]["Log"][0]["Type"], "reg", "le changement noté : « Revenir en arrière » le défait");
    assert_eq!(hist[0]["Log"][0]["Existed"], false);
    let o = p.borrow().ordres.clone();
    assert!(
        o.iter()
            .any(|x| x["texte"].as_str().is_some_and(|t| t.starts_with("Certains réglages n'ont pas pu être changés")))
    );
    assert!(o.iter().any(|x| x["texte"] == "1 réglage(s) coupé(s). Annulable depuis la page Sauvegarde, historique."));
    assert!(recus.borrow().iter().any(|r| r == "signal sauvegarde-changee {}"));
}

/// Audit G3 : `sauvegarde-appliquer` coupe des services par l'Aide (jusqu'à 60 s chacun) : dans un travail de fond.
/// Le geste rend la main tout de suite ; à la fin, la même ligne d'historique qu'avant. Fausse Aide, faux service.
#[test]
fn sauvegarde_appliquer_ne_fige_pas() {
    let b = Factice { prend: vec!["x".into()], ..Factice::default() };
    let eff = b.effets.clone();
    let mut h = hote(vec![Box::new(b)], "sauvegarde-appliquer-fond");
    let _ = std::fs::remove_file(h.ctx.donnees.join("historique.json"));
    h.ctx.oublier_reglages();
    h.etat_service = std::sync::Arc::new(|_| Some(("Automatic".into(), true)));
    let (feu, attente) = std::sync::mpsc::channel::<()>();
    let attente = std::sync::Mutex::new(Some(attente));
    h.ctx.fausse_aide(std::sync::Arc::new(move |_a: &str, _p: &Value| {
        if let Some(r) = attente.lock().unwrap().take() {
            let _ = r.recv_timeout(Duration::from_secs(60));
        }
        Ok(json!("ok"))
    }));
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    eff.borrow_mut().push(signal(
        "sauvegarde-appliquer",
        json!({ "Titre": "Services coupés", "Details": [],
        "Ecritures": [{ "Type": "svc", "Name": "NevermindEssaiService", "Demarrage": "Disabled" }], "Message": "" }),
    ));
    let debut = std::time::Instant::now();
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    assert!(
        debut.elapsed() < Duration::from_secs(2),
        "le geste a pris {:?} : le fil des blocs était figé",
        debut.elapsed()
    );
    assert!(h.ctx.historique().is_empty(), "fini avant la réponse de l'Aide ?");
    feu.send(()).unwrap();
    let mut t = Duration::ZERO;
    while h.ctx.historique().is_empty() {
        assert!(debut.elapsed() < Duration::from_secs(10), "sauvegarde-appliquer ne finit pas");
        std::thread::sleep(Duration::from_millis(20));
        t += Duration::from_millis(200);
        h.minuterie(t, &mut e);
    }
    let hist = h.ctx.historique();
    assert_eq!(hist[0]["Titre"], "Services coupés");
    assert_eq!(hist[0]["Log"][0]["Type"], "svc");
    assert_eq!(hist[0]["Log"][0]["Name"], "NevermindEssaiService");
}

#[test]
fn bulle_et_son_clic() {
    let b = Factice { prend: vec!["x".into(), "lag:7".into()], ..Factice::default() };
    let (eff, recus) = (b.effets.clone(), b.recus.clone());
    let mut h = hote(vec![Box::new(b)], "bulle");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    let _ = std::fs::remove_file(h.ctx.donnees.join("notification.demande"));
    eff.borrow_mut().push(signal("notification", json!({ "Titre": "Connexion pendant Valorant", "Texte": "Clique", "Action": "lag:7", "Valeur": null, "Ecran": "jeux" })));
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    let n: Value =
        serde_json::from_str(&std::fs::read_to_string(h.ctx.donnees.join("notification.demande")).unwrap()).unwrap();
    assert_eq!(n["titre"], "Connexion pendant Valorant");
    let id = n["id"].as_str().unwrap().to_string();
    assert!(!h.bulle_cliquee("000000000000", Duration::ZERO, &mut e), "une bulle inconnue");
    assert!(h.bulle_cliquee(&id, Duration::ZERO, &mut e));
    assert!(recus.borrow().iter().any(|r| r == "action lag:7 null"));
    assert_eq!(p.borrow().ordres.last(), Some(&json!({ "kit": "aller", "ecran": "jeux" })));
    // Le bloc perf écrit ses clés en minuscules
    eff.borrow_mut().push(signal("notification", json!({ "titre": "FPS", "texte": "Partie mesurée", "action": "x" })));
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    assert_eq!(h.notifications.last().map(|n| n.1.as_str()), Some("x"));
    // Le bloc réseau envoie « bulle » { titre, texte, ecran }
    eff.borrow_mut()
        .push(signal("bulle", json!({ "titre": "Nouvel appareil", "texte": "Un téléphone", "ecran": "reseau" })));
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    assert_eq!(h.notifications.last().map(|n| n.3.as_str()), Some("reseau"));
}

#[test]
fn le_lag_va_dans_la_page_jeux() {
    let mut p = json!({
        "jeux": { "onglets": [{ "id": "biblio" }, { "id": "perf", "onglets": [{ "id": "fps" }, { "id": "lag", "nom": "Lag" }] }] },
        "jeux-lag": { "module": { "titre": "Ping" }, "blocs": [{ "type": "texte", "texte": "a" }] },
    });
    placer_lag(p.as_object_mut().unwrap());
    assert!(p.get("jeux-lag").is_none());
    assert_eq!(
        p["jeux"]["onglets"][1]["onglets"][1],
        json!({ "id": "lag", "nom": "Lag", "module": { "titre": "Ping" }, "blocs": [{ "type": "texte", "texte": "a" }] })
    );
    // Sans la page Jeux : retirée quand même
    let mut p = json!({ "jeux-lag": { "blocs": [] } });
    placer_lag(p.as_object_mut().unwrap());
    assert_eq!(p, json!({}));
}

#[test]
fn signaux_fabriques_par_le_noyau() {
    // `nevermind_noyau::signaux::question` donne ses boutons en simples textes
    let b = Factice { prend: vec!["x".into(), "maj:oui".into()], ..Factice::default() };
    let (eff, recus) = (b.effets.clone(), b.recus.clone());
    let mut h = hote(vec![Box::new(b)], "signaux-noyau");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    eff.borrow_mut().push(nevermind_noyau::signaux::question(
        "q1",
        "Installer ?",
        "Texte",
        &["Oui", "Non"],
        None,
        "maj:oui",
    ));
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    let q = p.borrow().ordres.iter().rev().find(|o| o["kit"] == "question").cloned().unwrap();
    assert_eq!(q["boutons"], json!([{ "id": "Oui", "texte": "Oui" }, { "id": "Non", "texte": "Non" }]));
    h.recevoir(
        MessagePage::Reponse { id: q["id"].as_str().unwrap().into(), valeur: "Oui".into() },
        Duration::ZERO,
        &mut e,
    );
    assert!(recus.borrow().iter().any(|r| r == r#"action maj:oui {"id":"q1","valeur":"Oui"}"#));
    eff.borrow_mut().push(nevermind_noyau::signaux::occupe("Un instant..."));
    h.recevoir(action("x", "accueil"), Duration::ZERO, &mut e);
    assert_eq!(p.borrow().ordres.last(), Some(&json!({ "kit": "occupe", "texte": "Un instant..." })));
}

#[test]
fn l_assistance_du_noyau_a_sa_fiche_et_son_panneau() {
    let mut h = hote(vec![], "assistance");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    h.recevoir(action("visite", "aide"), Duration::ZERO, &mut e);
    let f = p.borrow().etats.last().unwrap()["pages"]["fiche"].clone();
    assert_eq!(f["titre"], "Bienvenue dans Nevermind");
    assert!(p.borrow().ordres.iter().any(|o| *o == json!({ "kit": "aller", "ecran": "fiche" })));
    h.recevoir(action("fiche:close", "fiche"), Duration::ZERO, &mut e);
    assert!(p.borrow().etats.last().unwrap()["pages"].get("fiche").is_none());
    h.recevoir(MessagePage::Ecran { id: "aide".into() }, Duration::ZERO, &mut e);
    h.recevoir(action("clic:BtnReportProblem", "aide"), Duration::ZERO, &mut e);
    let pa = p.borrow().etats.last().unwrap()["pages"]["panneau"].clone();
    assert_eq!(pa["titre"], "Signaler un problème");
    assert_eq!(pa["parent"], "aide");
    h.recevoir(action("panneau:close", "panneau"), Duration::ZERO, &mut e);
    assert!(p.borrow().etats.last().unwrap()["pages"].get("panneau").is_none());
}

/// Un bloc mène à une page qui vient d'apparaître (la fiche d'un jeu) : l'état qui la contient part avant l'ordre d'y
/// aller (sinon le kit retombe sur l'accueil : essai du 06/11 sur le bureau à part).
#[test]
fn page_nouvelle_envoyee_avant_d_y_aller() {
    let b = Factice { prend: vec!["jeu:0".into()], ..Factice::default() };
    let (pages, eff) = (b.pages.clone(), b.effets.clone());
    let mut h = hote(vec![Box::new(b)], "page-nouvelle");
    let p = Rc::new(RefCell::new(Page::default()));
    let mut e = EcranEssai(Rc::clone(&p));
    h.recevoir(MessagePage::Pret, Duration::ZERO, &mut e);
    pages.borrow_mut().insert("jeu".into(), json!({ "titre": "Un jeu", "blocs": [] }));
    eff.borrow_mut().push(Effet::Rafraichir);
    eff.borrow_mut().push(Effet::OuvrirEcran("jeu".into()));
    p.borrow_mut().suite.clear();
    h.recevoir(action("jeu:0", "jeux"), Duration::ZERO, &mut e);
    let suite = p.borrow().suite.clone();
    let aller = suite.iter().position(|o| o == &json!({ "kit": "aller", "ecran": "jeu" })).expect("aller jeu");
    let etat = suite
        .iter()
        .position(|o| o["etat"].as_array().is_some_and(|l| l.iter().any(|k| k == "jeu")))
        .expect("un état avec la page « jeu »");
    assert!(etat < aller, "l'état doit partir avant l'ordre : {suite:?}");
}

/// La carte de Nevermind dans Mindset (`Write-EtatMindset`) : etat.json écrit pour une copie gérée, même cachée, une
/// fois l'analyse du PC donnée, en UTF-8 sans BOM, seulement quand elle change ; jamais pour une copie non gérée.
#[test]
fn etat_json_pour_mindset() {
    let d = nevermind_noyau::dossier_essai("hote-etat-mindset");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let t = suite_commun::themes::Theme::neon();
    let nouveau = |geree: bool| {
        Hote::nouveau(
            Vec::new(),
            Contexte::nouveau(d.clone(), d.clone(), geree, false),
            Habillage::depuis(&t),
            String::new(),
            "1.15.6",
            Images::default(),
            Box::new(|| "cable".into()),
        )
    };
    let signal = |d: Value| {
        let mut e = Effets::default();
        e.ajouter(Effet::Signal { nom: "etat-mindset".into(), donnees: d });
        e
    };
    let f = d.join("etat.json");
    let mut h = nouveau(true);
    h.k.regardee = false;
    // La protection seule : pas encore (`-not $A`)
    h.appliquer(None, signal(json!({ "protection": 64 })), Duration::ZERO, &mut SansEcran, 0);
    assert!(!f.exists());
    let pc = json!({ "pc": { "Score": 72, "AFaire": "Plan d'alimentation", "Urgent": true, "NbAFaire": 3 } });
    h.appliquer(None, signal(pc.clone()), Duration::ZERO, &mut SansEcran, 0);
    let o = std::fs::read(&f).unwrap();
    assert_eq!(o[0], b'{', "sans BOM");
    let v: Value = serde_json::from_slice(&o).unwrap();
    let cles: Vec<&String> = v.as_object().unwrap().keys().collect();
    assert_eq!(cles, ["Version", "Score", "Protection", "AFaire", "Urgent", "NbAFaire", "Maj"]);
    assert_eq!((v["Version"].as_i64(), v["Score"].as_i64(), v["Protection"].as_i64()), (Some(1), Some(72), Some(64)));
    assert_eq!(
        (v["AFaire"].as_str(), v["Urgent"].as_bool(), v["NbAFaire"].as_i64()),
        (Some("Plan d'alimentation"), Some(true), Some(3))
    );
    assert!(!d.join("etat.json.tmp").exists() && !d.join("etat.json.bak").exists());
    // Rien de neuf : pas réécrit
    std::fs::write(&f, "garde").unwrap();
    h.appliquer(None, signal(pc), Duration::ZERO, &mut SansEcran, 0);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "garde");
    // Un changement : réécrit
    h.appliquer(None, signal(json!({ "protection": 90 })), Duration::ZERO, &mut SansEcran, 0);
    let v: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
    assert_eq!(v["Protection"].as_i64(), Some(90));
    // Pas gérée : rien
    std::fs::remove_file(&f).unwrap();
    let mut h = nouveau(false);
    h.appliquer(None, signal(json!({ "pc": { "Score": 50 } })), Duration::ZERO, &mut SansEcran, 0);
    assert!(!f.exists());
    let _ = std::fs::remove_dir_all(&d);
}

/// « Lancer » d'un jeu : les adresses des lanceurs que la bibliothèque produit (nevermind-jeux) passent, le reste non
/// (07/11/2026 : steam://rungameid/4080220, EA SPORTS FC 27, refusée, « Ouverture impossible »).
#[test]
fn les_adresses_des_lanceurs_de_jeux_sont_permises() {
    for a in [
        "steam://rungameid/4080220",
        "steam://nav/games",
        "steam://uninstall/4080220",
        "com.epicgames.launcher://apps/Fortnite?action=launch&silent=true",
        "uplay://launch/635/0",
        "battlenet://Pro",
        r"shell:AppsFolder\Microsoft.MinecraftUWP_8wekyb3d8bbwe!App",
        "windowsdefender://threatsettings",
        "ms-settings:appsfeatures",
        "https://github.com/JordanJacquot/Nevermind",
        r"C:\Jeux\jeu.exe",
    ] {
        assert!(adresse_permise(a), "refusée à tort : {a}");
    }
    for a in [
        "",
        "cmd.exe",
        "file:///C:/x.bat",
        "javascript:alert(1)",
        "ms-msdt:/id",
        "search-ms:query=x",
        " steam://rungameid/1",
        "steamx",
        "é",
    ] {
        assert!(!adresse_permise(a), "permise à tort : {a}");
    }
}
