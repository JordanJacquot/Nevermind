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
    /// Les demandes faites à l'Aide du contexte (mode jeu, « Couper les envois »)
    aide_ctx: Arc<Mutex<Vec<String>>>,
}

/// Le chemin de OneDrive sur le faux PC : fermé par le mode jeu, sa relance « ratée » est dite dans le journal
const ONEDRIVE: &str = r"C:\Users\Essai\AppData\Local\Microsoft\OneDrive\OneDrive.exe";

/// Un bloc « Trafic » d'essai : « Couper les envois » envoie `sauvegarde-appliquer` à l'hôte, comme le vrai
struct TraficEssai;
impl Bloc for TraficEssai {
    fn nom(&self) -> &'static str {
        "trafic"
    }
    fn pages(&mut self, _: &Contexte, _: &str) -> Map<String, Value> {
        Map::new()
    }
    fn action(&mut self, _: &Contexte, id: &str, _: &Value, effets: &mut Effets) -> bool {
        if id != "trafic:couper" {
            return false;
        }
        effets.ajouter(nevermind_noyau::Effet::Signal {
            nom: "sauvegarde-appliquer".into(),
            donnees: json!({ "Geste": "Couper les envois", "Page": "Trafic", "Titre": "Envois à Microsoft réduits",
                "Details": [], "Message": "",
                "Ecritures": [{ "Type": "reg", "Path": r"HKLM:\SOFTWARE\MindsetEssaiLotA\DataCollection", "Name": "AllowTelemetry", "Value": 0 }] }),
        });
        true
    }
}

impl Essai {
    fn nouveau(nom: &str) -> Essai {
        Essai::nouveau_avec(nom, false)
    }

    /// `mode_jeu` : le mode jeu est activé (OneDrive fermé au début d'une partie, relancé à la fin)
    fn nouveau_avec(nom: &str, mode_jeu: bool) -> Essai {
        let dossier = dossier_essai(&format!("fil-de-fond-{nom}"));
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(dossier.join("donnees")).unwrap();
        let ctx = Contexte::nouveau(dossier.join("fichiers"), dossier.join("donnees"), false, true);
        // (l'Aide du bloc perf : la mesure des FPS, demandée dans les réglages ; chaque lecture comptée)
        ctx.ecrire_reglage("FpsMeasure", json!(true), false).unwrap();
        if mode_jeu {
            ctx.ecrire_reglage("GameMode", json!(true), false).unwrap();
        }
        let fps_lus = Arc::new(AtomicUsize::new(0));
        let f = fps_lus.clone();
        let aide_ctx = Arc::new(Mutex::new(Vec::new()));
        let ac = aide_ctx.clone();
        ctx.fausse_aide(Arc::new(move |a: &str, _: &Value| {
            if a == "fps.lire" {
                f.fetch_add(1, Ordering::SeqCst);
                return Ok(json!(""));
            }
            ac.lock().unwrap().push(a.to_string());
            if a == "processus.fermer" {
                return Ok(json!(1));
            }
            Ok(json!("ok"))
        }));
        let partage = Arc::new(Mutex::new(Partage::default()));
        let pm = ctx.appli.join("outils-tiers").join("PresentMon.exe").to_string_lossy().into_owned();
        let mut chemins = HashMap::new();
        chemins.insert(4242, r"C:\Jeux\RocketLeague\Binaries\Win64\RocketLeague.exe".to_string());
        chemins.insert(77, ONEDRIVE.to_string());
        let sys = FauxSysteme {
            partage: Some(partage.clone()),
            chemins,
            fichiers: vec![pm],
            cpu: Some((100, 1000)),
            ram: Some((16e9, 8e9)),
            lancements_rates: vec![ONEDRIVE.to_string()],
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
            vec![Box::new(Compteur(tics.clone())), Box::new(perf), Box::new(memoire), Box::new(TraficEssai)],
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
            aide_ctx,
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

impl Essai {
    /// Le journal de Nevermind (journal.txt du dossier d'essai)
    fn journal(&self) -> String {
        std::fs::read_to_string(self.hote.ctx.fichier_journal()).unwrap_or_default()
    }

    /// « Couper ce qui est sûr » lancé, et l'Aide lente atteinte : le geste de la page RAM tient le verrou
    fn ram_tient_le_verrou(&mut self) {
        let duree = self.couper();
        assert!(duree < Duration::from_secs(2), "le « Oui » a pris {duree:?}");
        let fin = Instant::now() + Duration::from_secs(5);
        while self.aide.demandes.load(Ordering::SeqCst) == 0 {
            assert!(Instant::now() < fin, "l'Aide n'est pas appelée");
            self.tourner(Duration::from_millis(50));
        }
        assert_eq!(
            self.hote.ctx.prendre_geste("Essai", "Essai").unwrap_err(),
            "Attends la fin de « Couper ce qui est sûr » (RAM)"
        );
    }

    /// Une partie de Rocket League commence, OneDrive ouvert (le mode jeu le ferme)
    fn partie(&mut self) {
        {
            let mut p = self.partage.lock().unwrap();
            p.progs.push(Programme { pid: 77, nom: "OneDrive".into(), ..Default::default() });
            p.progs.push(Programme { pid: 4242, nom: "RocketLeague".into(), ..Default::default() });
            p.premier_plan = 4242;
        }
        let fin = Instant::now() + Duration::from_secs(15);
        while !self.hote.partie() {
            assert!(Instant::now() < fin, "la partie n'est pas vue");
            self.tourner(Duration::from_millis(100));
        }
        // (OneDrive a été fermé par l'Aide)
        self.partage.lock().unwrap().progs.retain(|p| p.pid != 77);
    }
}

/// Lot A du second audit : deux gestes de deux pages en même temps. Pendant « Couper ce qui est sûr » (page RAM, l'Aide
/// lente), « Couper les envois » (Trafic) est REFUSÉ avec « Attends la fin de « Couper ce qui est sûr » (RAM) » ;
/// rien ne part tout seul quand la RAM a fini ; ensuite, un nouveau clic passe.
#[test]
fn deux_gestes_de_deux_pages_le_second_est_refuse() {
    let mut es = Essai::nouveau("deux-pages");
    es.ram_tient_le_verrou();
    es.page.lock().unwrap().ordres.clear();
    es.message(MessagePage::Action { id: "trafic:couper".into(), valeur: Value::Null, ecran: "trafic".into() });
    assert!(
        es.ordres().contains(
            &json!({ "kit": "message", "texte": "Attends la fin de « Couper ce qui est sûr » (RAM)", "bravo": false })
        ),
        "le clic n'est pas refusé avec le bon texte : {:?}",
        es.ordres()
    );
    let _ = es.feu.take().unwrap().send(());
    es.jusqu_a("la fiche de résultat de la RAM", |p| {
        p.etats.last().is_some_and(|e| e.to_string().contains("C'est fait !"))
    });
    es.tourner(Duration::from_millis(500));
    assert!(!es.aide_ctx.lock().unwrap().iter().any(|a| a == "reg.ecrire"), "le clic refusé est parti plus tard");
    assert!(!es.hote.ctx.historique().iter().any(|h| h["Titre"] == "Envois à Microsoft réduits"));
    // Un nouveau clic passe
    es.message(MessagePage::Action { id: "trafic:couper".into(), valeur: Value::Null, ecran: "trafic".into() });
    let fin = Instant::now() + Duration::from_secs(10);
    while !es.hote.ctx.historique().iter().any(|h| h["Titre"] == "Envois à Microsoft réduits") {
        assert!(Instant::now() < fin, "le nouveau clic ne passe pas");
        es.tourner(Duration::from_millis(100));
    }
}

/// Ajustement A du lot A : le mode jeu au début et à la fin d'une partie n'est jamais refusé ni retardé, même quand un
/// geste de l'utilisateur tient le verrou (ici « Couper ce qui est sûr », l'Aide lente) : il passe en priorité.
#[test]
fn le_mode_jeu_passe_en_priorite_pendant_le_geste_d_une_autre_page() {
    let mut es = Essai::nouveau_avec("mode-jeu", true);
    es.ram_tient_le_verrou();
    es.partie();
    assert!(
        es.aide_ctx.lock().unwrap().iter().any(|a| a == "processus.fermer"),
        "le début de partie n'a pas fermé OneDrive : {:?}",
        es.aide_ctx.lock().unwrap()
    );
    // La partie finit pendant que la RAM tient toujours le verrou : la remise en état est faite tout de suite
    es.partage.lock().unwrap().progs.clear();
    let fin = Instant::now() + Duration::from_secs(15);
    while es.hote.partie() {
        assert!(Instant::now() < fin, "la fin de partie n'est pas vue");
        es.tourner(Duration::from_millis(100));
    }
    let j = es.journal();
    assert!(j.contains("Mode jeu: fin de Rocket League"), "{j}");
    assert!(j.contains("à relancer à la main: OneDrive"), "la relance de OneDrive n'a pas été tentée : {j}");
    assert_eq!(
        es.hote.ctx.prendre_geste("Essai", "Essai").unwrap_err(),
        "Attends la fin de « Couper ce qui est sûr » (RAM)",
        "la RAM tient toujours son verrou"
    );
}

/// Ajustements A et C : Nevermind se ferme pendant une partie, alors qu'un geste de l'utilisateur tient le verrou (et
/// attend l'Aide). La remise en état de fin de partie est faite avant de fermer, dans le délai ; le verrou est rendu :
/// un NOUVEAU geste passe ensuite.
#[test]
fn la_fermeture_remet_le_mode_jeu_et_rend_le_verrou() {
    let mut es = Essai::nouveau_avec("fermeture-mode-jeu", true);
    es.ram_tient_le_verrou();
    es.partie();
    let d = Instant::now();
    es.hote.fermer();
    assert!(d.elapsed() < Duration::from_secs(6), "la fermeture a pris {:?}", d.elapsed());
    let j = es.journal();
    assert!(j.contains("Mode jeu: fin de Rocket League"), "la fin de partie n'est pas faite avant de fermer : {j}");
    assert!(j.contains("à relancer à la main: OneDrive"), "la relance de OneDrive n'a pas été tentée : {j}");
    let n = es.hote.ctx.prendre_geste("Nouveau geste", "Essai");
    assert!(n.is_ok(), "après la fermeture, un nouveau geste ne passe pas : {n:?}");
}

/// Petit du lot A : à la fermeture, ce qu'un travail a fini mais que personne n'a encore relevé est relevé avant de
/// fermer (avant, la ligne d'historique de « Couper ce qui est sûr », écrite à la relève, était perdue : plus de
/// « Annuler » pour ce geste).
#[test]
fn la_fermeture_releve_les_travaux_finis() {
    let mut es = Essai::nouveau("fermeture-releve");
    es.ram_tient_le_verrou();
    let _ = std::fs::remove_file(es.hote.ctx.donnees.join("historique.json"));
    // L'Aide répond, le travail finit ; Nevermind se ferme avant le passage suivant de la minuterie
    let _ = es.feu.take().unwrap().send(());
    let fin = Instant::now() + Duration::from_secs(10);
    while es.hote.ctx.refus_geste().is_some() {
        assert!(Instant::now() < fin, "le travail ne finit pas");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(100));
    assert!(es.hote.ctx.historique().is_empty(), "déjà relevé : l'essai ne prouve rien");
    es.hote.fermer();
    let h = es.hote.ctx.historique();
    assert!(
        h.iter().any(|x| x["Log"].as_array().is_some_and(|l| l.iter().any(|e| e["Type"] == "svc"))),
        "la ligne d'historique du travail fini est perdue à la fermeture : {h:?}"
    );
}
