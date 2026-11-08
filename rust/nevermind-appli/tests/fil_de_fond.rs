//! Audit G3 : un geste long ne fige plus Nevermind. Tous les blocs tournent sur un seul fil ; un geste qui attend
//! l'Aide (ici « Couper ce qui est sûr » de la page RAM, avec une fausse Aide qui met jusqu'à 60 s à répondre pour
//! le premier service) part dans un travail de fond. Pendant ce temps, à travers l'hôte entier :
//! - le texte d'attente (« Un instant... ») est montré tout de suite ;
//! - les tics continuent : un bloc compteur, la détection d'une partie et la mesure des FPS du bloc perf ;
//! - les pages répondent (changement de page) ;
//! - un deuxième « Couper » ne repart pas ;
//! - à la fin, la même fiche de résultat qu'avant ;
//! - Nevermind se ferme vite, même pendant le travail.
//!
//! Rien ne touche le vrai PC : faux PC de la mémoire, fausse Aide, faux système du bloc perf, dossier d'essai.

use nevermind_appli::hote::{Ecran, Hote};
use nevermind_appli::pack::Images;
use nevermind_appli::pont::MessagePage;
use nevermind_appli::theme::Habillage;
use nevermind_memoire::BlocMemoire;
use nevermind_memoire::constat::Cible;
use nevermind_memoire::donnees::{Donnees, Pilote};
use nevermind_memoire::pc::{Aide, Lecture, Pc};
use nevermind_noyau::{Bloc, Contexte, Effets, dossier_essai};
use nevermind_perf::BlocPerf;
use nevermind_perf::systeme::{FauxCompteur, FauxSysteme, Partage, Programme};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// La page vue par l'hôte : tout ce qu'elle reçoit
#[derive(Default)]
struct Page {
    etats: Vec<Value>,
    ordres: Vec<Value>,
}
struct EcranEssai(Arc<Mutex<Page>>);
impl Ecran for EcranEssai {
    fn etat(&mut self, e: Value) {
        self.0.lock().unwrap().etats.push(e);
    }
    fn ordre(&mut self, o: Value) {
        self.0.lock().unwrap().ordres.push(o);
    }
}

/// Le faux PC de la page RAM : les données d'un juge, deux services
struct FauxPcMemoire(Donnees);
impl Pc for FauxPcMemoire {
    fn lire(&self, _: &[String], _: &HashMap<String, Vec<Pilote>>) -> Lecture {
        Lecture { donnees: Some(self.0.clone()), journal: vec![], erreur: None }
    }
    fn chercher_pilotes(&self, _: &[String]) -> HashMap<String, Vec<Pilote>> {
        HashMap::new()
    }
    fn contexte_fermeture(&self) -> Result<nevermind_memoire::pc::ContexteRegle, String> {
        Err("pas dans ce test".into())
    }
    fn fiche(&self, pid: i64, chemin: &str) -> nevermind_memoire::pc::Fiche {
        nevermind_memoire::pc::Fiche {
            id: pid,
            chemin: chemin.into(),
            session: None,
            compte: String::new(),
            protection: None,
            eleve: None,
        }
    }
    fn verifier(&self, _: i64, _: i64, _: &str) -> i32 {
        0
    }
    fn fermer_poliment(&self, _: i64, _: i64, _: &str) -> i32 {
        -2
    }
    fn forcer(&self, _: i64, _: i64, _: &str) -> i32 {
        -2
    }
    fn vivant(&self, _: i64) -> i32 {
        0
    }
    fn etat_fermeture(&self, _: &[Cible]) -> &'static str {
        "rangee"
    }
    fn service(&self, nom: &str) -> Option<(String, bool)> {
        (nom == "AdobeARMservice").then(|| ("Automatic".to_string(), true))
    }
    fn bureau(&self) -> Option<PathBuf> {
        None
    }
}

/// La fausse Aide de la page RAM : la première demande attend le feu du test (60 s au plus), comme un service qui
/// met du temps à s'arrêter
struct AideLente {
    feu: Mutex<Option<Receiver<()>>>,
    demandes: AtomicUsize,
}
impl Aide for AideLente {
    fn demander(&self, _: &Contexte, _: &str, _: &Value) -> Result<Value, String> {
        self.demandes.fetch_add(1, Ordering::SeqCst);
        if let Some(r) = self.feu.lock().unwrap().take() {
            let _ = r.recv_timeout(Duration::from_secs(60));
        }
        Ok(json!("ok"))
    }
}

/// Un bloc qui compte ses tics (toutes les 100 ms)
struct Compteur(Arc<AtomicUsize>);
impl Bloc for Compteur {
    fn nom(&self) -> &'static str {
        "compteur"
    }
    fn pages(&mut self, _: &Contexte, _: &str) -> Map<String, Value> {
        Map::new()
    }
    fn action(&mut self, _: &Contexte, _: &str, _: &Value, _: &mut Effets) -> bool {
        false
    }
    fn tic(&mut self, _: &Contexte, _: bool, _: &mut Effets) -> Option<Duration> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Some(Duration::from_millis(100))
    }
}

fn juge_memoire() -> Donnees {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("nevermind-memoire").join("tests").join("juges");
    for f in std::fs::read_dir(d).unwrap().flatten() {
        if !f.file_name().to_string_lossy().starts_with("constats-") {
            continue;
        }
        let v: Value = serde_json::from_str(&std::fs::read_to_string(f.path()).unwrap()).unwrap();
        if let Some(j) = v.as_array().unwrap().iter().find(|j| j["nom"] == "faux PC") {
            return Donnees::depuis_json(&j["donnees"]);
        }
    }
    panic!("juge « faux PC » absent")
}

struct Essai {
    hote: Hote,
    page: Arc<Mutex<Page>>,
    debut: Instant,
    tics: Arc<AtomicUsize>,
    fps_lus: Arc<AtomicUsize>,
    partage: Arc<Mutex<Partage>>,
    aide: Arc<AideLente>,
    feu: Option<Sender<()>>,
    dossier: PathBuf,
}

impl Essai {
    fn nouveau(nom: &str) -> Essai {
        let dossier = dossier_essai(&format!("fil-de-fond-{nom}"));
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(dossier.join("donnees")).unwrap();
        let ctx = Contexte::nouveau(dossier.join("fichiers"), dossier.join("donnees"), false, true);
        // (l'Aide du bloc perf : la mesure des FPS, demandée dans les réglages ; chaque lecture comptée)
        ctx.ecrire_reglage("FpsMeasure", json!(true), false).unwrap();
        let fps_lus = Arc::new(AtomicUsize::new(0));
        let f = fps_lus.clone();
        ctx.fausse_aide(Arc::new(move |a: &str, _: &Value| {
            if a == "fps.lire" {
                f.fetch_add(1, Ordering::SeqCst);
                return Ok(json!(""));
            }
            Ok(json!("ok"))
        }));
        let partage = Arc::new(Mutex::new(Partage::default()));
        let pm = ctx.appli.join("outils-tiers").join("PresentMon.exe").to_string_lossy().into_owned();
        let mut chemins = HashMap::new();
        chemins.insert(4242, r"C:\Jeux\RocketLeague\Binaries\Win64\RocketLeague.exe".to_string());
        let sys = FauxSysteme {
            partage: Some(partage.clone()),
            chemins,
            fichiers: vec![pm],
            cpu: Some((100, 1000)),
            ram: Some((16e9, 8e9)),
            ..Default::default()
        };
        let mut perf = BlocPerf::avec(Box::new(sys), Box::new(FauxCompteur::default()));
        // (l'horloge du vrai temps : l'hôte tourne en temps réel)
        perf.horloge = None;
        let mut e = Effets::default();
        perf.signal(
            &ctx,
            "jeux-liste",
            &json!([{ "nom": "Rocket League", "exes": [r"C:\Jeux\RocketLeague\Binaries\Win64\RocketLeague.exe"], "source": "Epic" }]),
            &mut e,
        );
        let (feu, attente) = channel();
        let aide = Arc::new(AideLente { feu: Mutex::new(Some(attente)), demandes: AtomicUsize::new(0) });
        let memoire = BlocMemoire::avec(Arc::new(FauxPcMemoire(juge_memoire())), aide.clone());
        let tics = Arc::new(AtomicUsize::new(0));
        let hote = Hote::nouveau(
            vec![Box::new(Compteur(tics.clone())), Box::new(perf), Box::new(memoire)],
            ctx,
            Habillage::depuis(&suite_commun::themes::Theme::neon()),
            "Salut".into(),
            "2.0.2",
            Images::default(),
            Box::new(|| "cable".into()),
        );
        let mut es = Essai {
            hote,
            page: Arc::new(Mutex::new(Page::default())),
            debut: Instant::now(),
            tics,
            fps_lus,
            partage,
            aide,
            feu: Some(feu),
            dossier,
        };
        es.message(MessagePage::Pret);
        es
    }

    fn ecran(&self) -> EcranEssai {
        EcranEssai(self.page.clone())
    }

    /// Un message de la page ; rend le temps qu'il a pris
    fn message(&mut self, m: MessagePage) -> Duration {
        let mut e = self.ecran();
        let d = Instant::now();
        let t = self.debut.elapsed();
        let _ = self.hote.recevoir(m, t, &mut e);
        d.elapsed()
    }

    fn action(&mut self, id: &str) -> Duration {
        self.message(MessagePage::Action { id: id.into(), valeur: Value::Null, ecran: "memoire".into() })
    }

    /// La minuterie de l'hôte pendant `duree` (toutes les 100 ms, comme le fil des blocs à peu près)
    fn tourner(&mut self, duree: Duration) {
        let fin = Instant::now() + duree;
        while Instant::now() < fin {
            let mut e = self.ecran();
            let t = self.debut.elapsed();
            let _ = self.hote.minuterie(t, &mut e);
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Tourne jusqu'à ce que `ok` soit vrai (10 s au plus)
    fn jusqu_a(&mut self, quoi: &str, ok: impl Fn(&Page) -> bool) {
        let fin = Instant::now() + Duration::from_secs(10);
        while !ok(&self.page.lock().unwrap()) {
            assert!(Instant::now() < fin, "pas vu : {quoi}");
            self.tourner(Duration::from_millis(100));
        }
    }

    fn ordres(&self) -> Vec<Value> {
        self.page.lock().unwrap().ordres.clone()
    }

    /// La question posée par la page RAM (la dernière), et le nombre de questions
    fn question(&self) -> Option<(String, usize)> {
        let q: Vec<Value> = self.ordres().into_iter().filter(|o| o["kit"] == "question").collect();
        q.last().map(|o| (o["id"].as_str().unwrap_or("").to_string(), q.len()))
    }

    /// « Couper ce qui est sûr », puis « Oui » : rend le temps du « Oui »
    fn couper(&mut self) -> Duration {
        self.action("memoire:analyser");
        self.jusqu_a("le constat de la mémoire", |p| {
            p.etats.last().is_some_and(|e| e.to_string().contains("memoire:couper"))
        });
        self.action("memoire:couper");
        let (id, _) = self.question().expect("la question « Couper ce qui est sûr ? »");
        self.page.lock().unwrap().ordres.clear();
        self.message(MessagePage::Reponse { id, valeur: "oui".into() })
    }
}

impl Drop for Essai {
    fn drop(&mut self) {
        if let Some(f) = self.feu.take() {
            let _ = f.send(());
        }
        let _ = std::fs::remove_dir_all(&self.dossier);
    }
}

#[test]
fn un_geste_long_ne_fige_plus_nevermind() {
    let mut es = Essai::nouveau("geste-long");
    let duree = es.couper();
    // Le geste rend la main tout de suite (avant : 60 s, le temps de l'Aide), et l'attente est montrée aussitôt
    assert!(duree < Duration::from_secs(2), "le « Oui » a pris {duree:?} : le fil des blocs était figé");
    assert!(
        es.ordres().contains(&json!({ "kit": "occupe", "texte": "Un instant..." })),
        "le texte d'attente n'est pas montré tout de suite : {:?}",
        es.ordres()
    );
    es.jusqu_a("la première demande à l'Aide", |_| true);
    let fin = Instant::now() + Duration::from_secs(5);
    while es.aide.demandes.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < fin, "l'Aide n'est pas appelée");
        std::thread::sleep(Duration::from_millis(20));
    }

    // Pendant l'attente de l'Aide : un jeu démarre ; les tics, la partie, les FPS et les pages continuent
    let tics = es.tics.load(Ordering::SeqCst);
    {
        let mut p = es.partage.lock().unwrap();
        p.progs.push(Programme { pid: 4242, nom: "RocketLeague".into(), ..Default::default() });
        p.premier_plan = 4242;
    }
    es.tourner(Duration::from_secs(7));
    let vus = es.tics.load(Ordering::SeqCst) - tics;
    assert!(vus >= 30, "seulement {vus} tics en 7 s pendant le geste long");
    assert!(es.hote.partie(), "la partie n'est pas vue pendant le geste long");
    assert!(es.fps_lus.load(Ordering::SeqCst) > 0, "les FPS ne sont pas mesurés pendant le geste long");
    let avant = es.ordres().len();
    let d = es.message(MessagePage::Action { id: "page:1".into(), valeur: Value::Null, ecran: "accueil".into() });
    assert!(d < Duration::from_secs(1), "changer de page a pris {d:?}");
    assert!(
        es.ordres()[avant..].iter().any(|o| o["kit"] == "aller"),
        "la page ne change pas : {:?}",
        &es.ordres()[avant..]
    );

    // Un deuxième « Couper ce qui est sûr » pendant le travail : pas de deuxième question, rien ne repart
    let (_, n) = es.question().unwrap_or_default();
    es.action("memoire:couper");
    assert_eq!(es.question().map(|q| q.1).unwrap_or(0), n, "une deuxième question pendant le travail");
    assert_eq!(es.aide.demandes.load(Ordering::SeqCst), 1, "le geste est reparti");

    // L'Aide répond : la même fiche de résultat qu'avant
    let _ = es.feu.take().unwrap().send(());
    es.jusqu_a("la fiche de résultat", |p| p.etats.last().is_some_and(|e| e.to_string().contains("C'est fait !")));
    assert!(es.ordres().contains(&json!({ "kit": "occupe", "texte": "" })), "la pastille n'est pas retirée");
    assert!(
        es.ordres().iter().any(|o| o["kit"] == "message" && o["texte"] == "Mémoire : ce qui est sûr est coupé."),
        "{:?}",
        es.ordres()
    );
}

#[test]
fn fermer_pendant_un_geste_long_ne_bloque_pas() {
    let mut es = Essai::nouveau("fermer");
    let duree = es.couper();
    assert!(duree < Duration::from_secs(2), "le « Oui » a pris {duree:?} : le fil des blocs était figé");
    let fin = Instant::now() + Duration::from_secs(5);
    while es.aide.demandes.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < fin, "l'Aide n'est pas appelée");
        es.tourner(Duration::from_millis(50));
    }
    let d = Instant::now();
    es.hote.fermer();
    assert!(d.elapsed() < Duration::from_secs(2), "la fermeture a attendu le travail ({:?})", d.elapsed());
}
