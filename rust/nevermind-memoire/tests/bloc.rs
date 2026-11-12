//! Le déroulé du bloc (tester-memoire.ps1, sections 6 et 7) sur un faux PC et une fausse Aide : la lecture en tâche de
//! fond, la recherche du pilote, le rapport qui attend, « Couper ce qui est sûr », et « Fermer » (confirmation,
//! question, ne répond plus, rangée, en partie). Jamais un vrai processus fermé, jamais la vraie Aide.

use aide_regles::fermeture::{Contexte as ContexteRegle, Fiche, Reference};
use nevermind_memoire::BlocMemoire;
use nevermind_memoire::constat::Cible;
use nevermind_memoire::donnees::{Donnees, Pilote};
use nevermind_memoire::pc::{Aide, Lecture, Pc};
use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Le faux PC : les données d'un juge, et des processus simulés
#[derive(Default)]
struct FauxPc {
    donnees: Mutex<Option<Donnees>>,
    pilotes: Mutex<HashMap<String, Vec<Pilote>>>,
    vivants: Mutex<HashSet<i64>>,
    etat: Mutex<&'static str>,
    gestes: Mutex<Vec<String>>,
    services: Mutex<HashMap<String, (String, bool)>>,
    repris: Mutex<HashSet<i64>>,
    /// `etat_fermeture` lent (une appli occupée : jusqu'à 5 s par processus en vrai) ; lot I
    etat_lent: Mutex<std::time::Duration>,
}

impl Pc for FauxPc {
    fn lire(&self, demarrage: &[String], _: &HashMap<String, Vec<Pilote>>) -> Lecture {
        self.gestes.lock().unwrap().push(format!("lire:{}", demarrage.join(",")));
        Lecture {
            donnees: self.donnees.lock().unwrap().clone(), journal: vec!["Mémoire : essai".into()], erreur: None
        }
    }
    fn chercher_pilotes(&self, tags: &[String]) -> HashMap<String, Vec<Pilote>> {
        self.gestes.lock().unwrap().push(format!("pilotes:{}", tags.join(",")));
        self.pilotes.lock().unwrap().clone()
    }
    fn contexte_fermeture(&self) -> Result<ContexteRegle, String> {
        Ok(ContexteRegle {
            windows: r"C:\Windows".into(),
            programmes: r"C:\Program Files".into(),
            antivirus: vec![],
            services_pid: Some(HashSet::new()),
            reference: Some(Reference {
                id: 9000,
                session: Some(1),
                compte: "S-1-5-21-ESSAI-1001".into(),
                eleve: Some(0),
            }),
            processus: HashMap::new(),
        })
    }
    fn fiche(&self, pid: i64, chemin: &str) -> Fiche {
        Fiche {
            id: pid,
            chemin: chemin.into(),
            session: Some(1),
            compte: "S-1-5-21-ESSAI-1001".into(),
            protection: Some(0),
            eleve: Some(0),
        }
    }
    fn verifier(&self, pid: i64, _: i64, _: &str) -> i32 {
        if self.repris.lock().unwrap().contains(&pid) {
            return -1;
        }
        i32::from(self.vivants.lock().unwrap().contains(&pid))
    }
    fn fermer_poliment(&self, _: &nevermind_noyau::Permis, pid: i64, _: i64, _: &str) -> i32 {
        self.gestes.lock().unwrap().push(format!("poli:{pid}"));
        1
    }
    fn forcer(&self, _: &nevermind_noyau::Permis, pid: i64, _: i64, _: &str) -> i32 {
        self.gestes.lock().unwrap().push(format!("force:{pid}"));
        1
    }
    fn vivant(&self, pid: i64) -> i32 {
        i32::from(self.vivants.lock().unwrap().contains(&pid))
    }
    fn etat_fermeture(&self, _: &[Cible]) -> &'static str {
        std::thread::sleep(*self.etat_lent.lock().unwrap());
        *self.etat.lock().unwrap()
    }
    fn service(&self, nom: &str) -> Option<(String, bool)> {
        self.services.lock().unwrap().get(nom).cloned()
    }
    fn bureau(&self) -> Option<PathBuf> {
        None
    }
}

/// La fausse Aide : note chaque demande, refuse celles qu'on lui dit
#[derive(Default)]
struct FausseAide {
    demandes: Mutex<Vec<Value>>,
    refuser: Mutex<HashSet<String>>,
}

impl Aide for FausseAide {
    fn demander(&self, _: &Contexte, action: &str, p: &Value) -> Result<Value, String> {
        self.demandes.lock().unwrap().push(json!({ "action": action, "parametres": p }));
        if self.refuser.lock().unwrap().contains(p["Nom"].as_str().unwrap_or("")) {
            return Err("Le service ne répond pas.".into());
        }
        Ok(json!("ok"))
    }
}

fn juge(nom: &str) -> Donnees {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("juges");
    for f in std::fs::read_dir(d).unwrap().flatten() {
        if !f.file_name().to_string_lossy().starts_with("constats-") {
            continue;
        }
        let v: Value = serde_json::from_str(&std::fs::read_to_string(f.path()).unwrap()).unwrap();
        if let Some(j) = v.as_array().unwrap().iter().find(|j| j["nom"] == nom) {
            return Donnees::depuis_json(&j["donnees"]);
        }
    }
    panic!("juge {nom} absent")
}

struct Essai {
    bloc: BlocMemoire,
    pc: Arc<FauxPc>,
    aide: Arc<FausseAide>,
    ctx: Contexte,
}

impl Essai {
    fn nouveau(d: Donnees) -> Essai {
        let pc = Arc::new(FauxPc::default());
        *pc.donnees.lock().unwrap() = Some(d);
        *pc.etat.lock().unwrap() = "question";
        let aide = Arc::new(FausseAide::default());
        let dossier = nevermind_noyau::dossier_essai(&format!("memoire-{}", rand()));
        let _ = std::fs::create_dir_all(&dossier);
        let ctx = Contexte::nouveau(dossier.clone(), dossier, false, true);
        Essai { bloc: BlocMemoire::avec(pc.clone(), aide.clone()), pc, aide, ctx }
    }
    fn action(&mut self, id: &str, v: Value) -> Vec<Effet> {
        let mut e = Effets::default();
        self.bloc.action(&self.ctx, id, &v, &mut e);
        e.0
    }
    /// Les tics jusqu'à ce que le travail de fond soit relevé
    fn tics(&mut self) -> Vec<Effet> {
        let mut tout = Vec::new();
        for _ in 0..6 {
            self.bloc.attendre_fond();
            let mut e = Effets::default();
            self.bloc.tic(&self.ctx, false, &mut e);
            tout.extend(e.0);
        }
        tout
    }
    fn page(&mut self) -> Value {
        self.bloc.pages(&self.ctx, "perf").remove("memoire").unwrap()
    }
}

fn rand() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64 % 1_000_000_007
}

fn questions(e: &[Effet]) -> Vec<Value> {
    e.iter()
        .filter_map(|x| match x {
            Effet::Signal { nom, donnees } if nom == "question" => Some(donnees.clone()),
            _ => None,
        })
        .collect()
}

fn messages(e: &[Effet]) -> Vec<String> {
    e.iter()
        .filter_map(|x| match x {
            Effet::Annonce(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

fn signal<'a>(e: &'a [Effet], n: &str) -> Option<&'a Value> {
    e.iter().find_map(|x| match x {
        Effet::Signal { nom, donnees } if nom == n => Some(donnees),
        _ => None,
    })
}

fn repondre(es: &mut Essai, q: &Value, valeur: &str) -> Vec<Effet> {
    es.action("memoire-reponse", json!({ "id": q["id"], "valeur": valeur }))
}

#[test]
fn lecture_puis_pilote_puis_rapport() {
    let mut d = juge("faux PC");
    d.pool_np = 1.3 * 1024.0 * 1024.0 * 1024.0;
    d.etiquettes =
        vec![nevermind_memoire::donnees::Etiquette { tag: "RTKx".into(), octets: 450.0 * 1048576.0, pilotes: vec![] }];
    let mut es = Essai::nouveau(d);
    let dossier = es.ctx.donnees.clone();
    // Page sans constat : « Regarder » ; arriver sur la page lance la lecture
    let p = es.page();
    assert_eq!(p["module"]["boutons"][0]["id"], "memoire:analyser");
    let e = es.action("ecran", json!("memoire"));
    assert!(es.bloc.lecture_en_cours());
    assert!(signal(&e, "occupe").is_some_and(|o| o["texte"] == "Je regarde ce qui prend la mémoire..."));
    let p = es.page();
    assert_eq!(p["module"]["boutons"], json!([]), "un bouton en double pendant la lecture");
    assert_eq!(p["actions"], json!([]));
    // « Créer un rapport » pendant la lecture : il attend
    let e = es.action("memoire:rapport", Value::Null);
    assert_eq!(messages(&e), ["Je finis de regarder la mémoire, puis je pose le rapport sur le Bureau."]);
    // Les données arrivent : le constat sans le nom du pilote, puis la recherche ; ensuite le rapport
    *es.pc.pilotes.lock().unwrap() = HashMap::from([(
        "RTKx".to_string(),
        vec![Pilote {
            fichier: "rt640x64.sys".into(),
            description: "Realtek PCIe GbE Family Controller".into(),
            societe: "Realtek".into(),
        }],
    )]);
    es.bloc.attendre_fond();
    let mut e = Effets::default();
    es.bloc.tic(&es.ctx, false, &mut e);
    let al = &es.bloc.constat().unwrap().alertes[0];
    assert!(!al.texte.contains("probablement"), "un pilote inventé avant la recherche");
    assert!(signal(&e.0, "memoire-constat").is_some());
    let p = es.page();
    assert!(p["blocs"].to_string().contains("Je cherche encore quel pilote"));
    let e = es.tics();
    assert!(es.bloc.constat().unwrap().alertes[0].texte.contains("rt640x64.sys"));
    assert!(es.pc.gestes.lock().unwrap().contains(&"pilotes:RTKx".to_string()));
    let m = messages(&e);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(m[0].starts_with("Rapport posé sur le Bureau : « Nevermind, rapport mémoire du 24-10-2026 à 14h30.txt »"));
    let f = dossier.join("Nevermind, rapport mémoire du 24-10-2026 à 14h30.txt");
    let o = std::fs::read(&f).unwrap();
    assert_eq!(&o[..3], &[0xEF, 0xBB, 0xBF], "rapport sans BOM");
    assert!(String::from_utf8_lossy(&o).contains("rt640x64.sys"));
    let _ = std::fs::remove_dir_all(&dossier);
}

#[test]
fn couper_ce_qui_est_sur() {
    let mut es = Essai::nouveau(juge("faux PC"));
    es.pc.services.lock().unwrap().insert("AdobeARMservice".into(), ("Automatic".into(), true));
    es.pc.services.lock().unwrap().insert("HPSysInfoCap".into(), ("Automatic".into(), false));
    es.aide.refuser.lock().unwrap().insert("HPSysInfoCap".into());
    es.action("memoire:analyser", Value::Null);
    es.tics();
    let e = es.action("memoire:couper", Value::Null);
    let q = &questions(&e)[0];
    assert!(q["texte"].as_str().unwrap().starts_with("Nevermind va couper ce qui ne sert pas au quotidien :\n\n•  Mise à jour d'Adobe Acrobat (AdobeARMservice) : passe en « manuel »"));
    let det = q["details"]["lignes"].as_array().unwrap();
    assert_eq!(det[0], "Service AdobeARMservice : démarrage maintenant automatique, il tourne, après manuel, arrêté");
    assert_eq!(det[1], "Service GoogleUpdaterService140.0.1.0 : absent de ce PC, rien à faire");
    // « Non » : rien
    repondre(&mut es, q, "non");
    assert!(es.aide.demandes.lock().unwrap().is_empty());
    let q = questions(&es.action("memoire:couper", Value::Null))[0].clone();
    let e = repondre(&mut es, &q, "oui");
    let d = es.aide.demandes.lock().unwrap().clone();
    assert_eq!(d.len(), 2, "{d:?}");
    assert_eq!(
        d[0],
        json!({ "action": "service.regler", "parametres": { "Nom": "AdobeARMservice", "Demarrage": "Manual", "Arreter": true } })
    );
    let r = signal(&e, "resultat").unwrap();
    assert_eq!(r["titre"], "C'est fait !");
    assert_eq!(r["lignes"][0], "1 chose coupée :");
    assert_eq!(r["lignes"][1], "•  Mise à jour d'Adobe Acrobat (AdobeARMservice) : passe en « manuel »");
    assert_eq!(r["lignes"][2], "Pas pu être fait :");
    assert!(r["log"][0]["StartType"] == "Automatic" && r["log"][0]["Running"] == true);
    // L'état d'origine noté avant le changement
    let orig = es.ctx.reglage("SvcOriginal").unwrap();
    assert!(orig.as_array().unwrap().contains(&json!("AdobeARMservice|Automatic|True")), "{orig}");
    assert!(es.bloc.lecture_en_cours(), "la liste n'est pas relue");
}

/// Le faux PC « fermer : Discord » avec Chrome (20, 21) vivants
fn essai_chrome() -> (Essai, usize) {
    let mut es = Essai::nouveau(juge("fermer : Discord"));
    es.action("memoire:analyser", Value::Null);
    es.tics();
    let i = es.bloc.constat().unwrap().applis.iter().position(|a| a.nom == "Google Chrome").unwrap();
    *es.pc.vivants.lock().unwrap() = HashSet::from([20, 21]);
    (es, i)
}

fn historique(e: &[Effet]) -> Option<Value> {
    signal(e, "historique").cloned()
}

#[test]
fn fermer_poliment() {
    let (mut es, i) = essai_chrome();
    let p = es.page();
    assert!(p["blocs"].to_string().contains(&format!("memoire-fermer:{i}")));
    let e = es.action(&format!("memoire-fermer:{i}"), Value::Null);
    let q = &questions(&e)[0];
    assert!(
        q["texte"]
            .as_str()
            .unwrap()
            .starts_with("Fermer Google Chrome ? Environ 400 Mo libérés.\n\nIl se ferme comme avec la croix")
    );
    let e = repondre(&mut es, q, "oui");
    assert!(signal(&e, "occupe").is_some_and(|o| o["texte"] == "Je ferme Google Chrome..."));
    assert!(es.bloc.fermeture_en_cours());
    // Plus aucun bouton « Fermer » pendant la fermeture
    assert!(!es.page()["blocs"].to_string().contains("memoire-fermer:"));
    es.tics();
    assert_eq!(
        *es.pc.gestes.lock().unwrap().iter().filter(|g| g.starts_with("poli:")).cloned().collect::<Vec<_>>(),
        ["poli:20", "poli:21"]
    );
    es.pc.vivants.lock().unwrap().clear();
    let e = es.tics();
    let h = historique(&e).expect("pas d'historique");
    assert_eq!(h["titre"], "Fermé : Google Chrome");
    assert_eq!(h["details"][0], r"C:\Program Files\Google\Chrome\Application\chrome.exe");
    assert_eq!(h["details"][1], "fermeture polie (comme la croix)");
    assert_eq!(h["details"][2], "environ 400 Mo libérés (estimation)");
    assert_eq!(h["log"][0]["Type"], "note");
    assert_eq!(messages(&e), ["Google Chrome est fermé. Environ 400 Mo de mémoire libérés (estimation)."]);
    assert!(!es.bloc.fermeture_en_cours());
    assert!(es.pc.gestes.lock().unwrap().iter().filter(|g| g.starts_with("lire")).count() >= 2, "pas relue");
}

#[test]
fn fermer_question_bloquee_rangee() {
    // Il pose une question : le dire, sans proposer de forcer
    let (mut es, i) = essai_chrome();
    let q = questions(&es.action(&format!("memoire-fermer:{i}"), Value::Null))[0].clone();
    repondre(&mut es, &q, "oui");
    es.tics();
    es.pc.vivants.lock().unwrap().remove(&21);
    es.bloc.vieillir_fermeture(6);
    let e = es.tics();
    let q = questions(&e);
    assert_eq!(q.len(), 1);
    assert!(q[0]["texte"].as_str().unwrap().starts_with("Google Chrome te pose une question"));
    assert_eq!(q[0]["boutons"], json!([{ "id": "ok", "texte": "OK" }]));
    assert!(!es.pc.gestes.lock().unwrap().iter().any(|g| g.starts_with("force")));
    assert_eq!(historique(&e).unwrap()["titre"], "Fermé en partie : Google Chrome");

    // Il ne répond plus : « Forcer la fermeture » proposé ; accepté, ce qui reste est forcé
    let (mut es, i) = essai_chrome();
    *es.pc.etat.lock().unwrap() = "bloquee";
    let q = questions(&es.action(&format!("memoire-fermer:{i}"), Value::Null))[0].clone();
    repondre(&mut es, &q, "oui");
    es.tics();
    es.bloc.vieillir_fermeture(6);
    let q = questions(&es.tics())[0].clone();
    assert!(
        q["texte"].as_str().unwrap().contains("ne répond plus") && q["texte"].as_str().unwrap().contains("sera perdu")
    );
    assert_eq!(q["boutons"][0]["texte"], "Forcer la fermeture");
    assert_eq!(q["boutons"][1]["texte"], "Laisser ouvert");
    let e = repondre(&mut es, &q, "forcer");
    assert!(signal(&e, "occupe").is_some_and(|o| o["texte"] == "Je force la fermeture de Google Chrome..."));
    es.tics();
    assert_eq!(es.pc.gestes.lock().unwrap().iter().filter(|g| g.starts_with("force:")).count(), 2);
    es.pc.vivants.lock().unwrap().clear();
    let e = es.tics();
    assert_eq!(historique(&e).unwrap()["details"][1], "fermeture forcée");

    // « Laisser ouvert » : rien de forcé ni noté, la liste est relue
    let (mut es, i) = essai_chrome();
    *es.pc.etat.lock().unwrap() = "rangee";
    let q = questions(&es.action(&format!("memoire-fermer:{i}"), Value::Null))[0].clone();
    repondre(&mut es, &q, "oui");
    es.tics();
    es.bloc.vieillir_fermeture(6);
    let q = questions(&es.tics())[0].clone();
    assert!(q["texte"].as_str().unwrap().contains("rangé près de l'horloge"));
    let e = repondre(&mut es, &q, "non");
    assert!(historique(&e).is_none());
    assert!(!es.pc.gestes.lock().unwrap().iter().any(|g| g.starts_with("force")));
    assert!(es.bloc.lecture_en_cours());
}

#[test]
fn fermer_refus() {
    // Confirmation refusée : rien
    let (mut es, i) = essai_chrome();
    let q = questions(&es.action(&format!("memoire-fermer:{i}"), Value::Null))[0].clone();
    repondre(&mut es, &q, "non");
    assert!(!es.bloc.fermeture_en_cours());
    // Une ligne sans bouton (Windows) : rien, même appelée directement
    let k = es.bloc.constat().unwrap().applis.iter().position(|a| a.kind == "windows").unwrap();
    let e = es.action(&format!("memoire-fermer:{k}"), Value::Null);
    assert!(e.is_empty(), "{e:?}");
    // Un lanceur pendant une partie lancée depuis la lecture : pas fermé
    let mut d = juge("jeux : lanceurs sans jeu");
    d.jeux = None;
    let mut es = Essai::nouveau(d);
    es.action("memoire:analyser", Value::Null);
    es.tics();
    let k = es.bloc.constat().unwrap().applis.iter().position(|a| a.exes.contains(&"steam.exe".to_string())).unwrap();
    let mut e = Effets::default();
    es.bloc.signal(&es.ctx, "partie-commencee", &json!({ "pid": 999 }), &mut e);
    let e = es.action(&format!("memoire-fermer:{k}"), Value::Null);
    assert_eq!(messages(&e), ["Steam sert à ton jeu en cours : Nevermind ne le ferme pas pendant la partie."]);
    // Numéro repris entre la lecture et le geste : refusé, dit, relu
    let (mut es, i) = essai_chrome();
    *es.pc.repris.lock().unwrap() = HashSet::from([20, 21]);
    let q = questions(&es.action(&format!("memoire-fermer:{i}"), Value::Null))[0].clone();
    repondre(&mut es, &q, "oui");
    let e = es.tics();
    assert!(historique(&e).is_none());
    assert!(messages(&e)[0].contains("il a changé depuis que Nevermind a regardé la mémoire"), "{:?}", messages(&e));
    assert!(!es.pc.gestes.lock().unwrap().iter().any(|g| g.starts_with("poli")));
}

/// Lot I, point 5 : une appli qui ne se ferme pas au bout de 5 s. Savoir si elle pose une question, ne répond plus ou
/// s'est rangée près de l'horloge demande à Windows jusqu'à 5 s par processus encore occupé. Avant, c'était dans le
/// tic, sur le fil des blocs ; maintenant dans un fil à part, et la question arrive ensuite comme avant.
#[test]
fn fermer_une_appli_occupee_ne_fige_pas_les_tics() {
    use std::time::{Duration, Instant};
    let (mut es, i) = essai_chrome();
    *es.pc.etat.lock().unwrap() = "bloquee";
    *es.pc.etat_lent.lock().unwrap() = Duration::from_secs(3);
    let q = questions(&es.action(&format!("memoire-fermer:{i}"), Value::Null))[0].clone();
    repondre(&mut es, &q, "oui");
    es.tics();
    es.bloc.vieillir_fermeture(6);
    let fin = Instant::now() + Duration::from_secs(15);
    let mut tout = Vec::new();
    while questions(&tout).is_empty() {
        assert!(Instant::now() < fin, "la question ne vient pas : {tout:?}");
        let mut e = Effets::default();
        let d = Instant::now();
        es.bloc.tic(&es.ctx, false, &mut e);
        assert!(d.elapsed() < Duration::from_secs(1), "un tic a pris {:?} : le fil des blocs attendait", d.elapsed());
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(questions(&tout)[0]["boutons"][0]["texte"], "Forcer la fermeture");
}
