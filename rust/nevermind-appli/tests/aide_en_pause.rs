//! Lot G (trouvaille M1 de l'audit du 10/11) : une Aide muette quelques secondes en pleine partie.
//!
//! Depuis le lot F, une Aide en pause (une demande restée sans réponse) refuse chaque demande tout de suite. Avant ce
//! lot, `fps.lire` (toutes les 250 ms) coupait la mesure des FPS après 20 refus, donc 5 s, pour toute la partie ; et
//! `fps.arreter` et `flux.arreter`, refusés pendant la pause, n'étaient jamais refaits.
//!
//! Les blocs perf et trafic tournent ensemble, avec le contexte PARTAGÉ (comme l'appli : c'est lui qui donne la pause)
//! et une fausse Aide sur le canal de la copie d'essai (`fausse_aide::lancer`, jamais la vraie Aide). La fausse Aide
//! sert une demande à la fois, comme une Aide occupée : muette, elle garde chaque connexion jusqu'à ce que Nevermind
//! coupe. Elle tient l'état de ses mesures (PresentMon d'une partie ou en veille, trace réseau), pour voir ce qui
//! tourne encore après la reprise.

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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const RL: &str = r"C:\JeuxEssaiNevermind\rocketleague\Binaries\Win64\RocketLeague.exe";
const BF6: &str = r"C:\JeuxEssaiNevermind\Battlefield 6\bf6.exe";

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

/// La mesure des FPS qui tourne dans la fausse Aide
#[derive(Clone, Debug, PartialEq)]
enum Fps {
    Rien,
    /// `fps.demarrer` pour ce processus
    Partie(u64),
    /// `fps.veiller`
    Veille,
}

/// La fausse Aide : ce qu'elle a traité (seulement quand elle parle) et l'état de ses mesures
struct FausseAide {
    parle: AtomicBool,
    /// Les demandes traitées, dans l'ordre (action, paramètres)
    traitees: Mutex<Vec<(String, Value)>>,
    fps: Mutex<Fps>,
    flux: Mutex<bool>,
    /// L'en-tête de PresentMon est-il déjà donné pour la mesure en cours ?
    entete: Mutex<bool>,
}

impl FausseAide {
    fn repondre(&self, a: &str, p: &Value) -> Option<Value> {
        if !self.parle.load(Ordering::SeqCst) {
            return None;
        }
        self.traitees.lock().unwrap().push((a.to_string(), p.clone()));
        let r = match a {
            "fps.demarrer" => {
                *self.fps.lock().unwrap() = Fps::Partie(p["Pid"].as_u64().unwrap_or(0));
                *self.entete.lock().unwrap() = false;
                json!("")
            }
            "fps.veiller" => {
                *self.fps.lock().unwrap() = Fps::Veille;
                *self.entete.lock().unwrap() = false;
                json!("")
            }
            "fps.arreter" => {
                *self.fps.lock().unwrap() = Fps::Rien;
                json!("")
            }
            "fps.lire" => {
                let pid = match *self.fps.lock().unwrap() {
                    Fps::Partie(pid) => pid,
                    _ => return Some(fausse_aide::ok(json!(""))),
                };
                let mut e = self.entete.lock().unwrap();
                let mut t = String::new();
                if !*e {
                    *e = true;
                    t.push_str("Application,ProcessID,MsBetweenPresents,PresentMode,MsCPUBusy,MsGPUBusy\n");
                }
                for _ in 0..15 {
                    t.push_str(&format!("RocketLeague.exe,{pid},16.667,Hardware: Independent Flip,2.0,3.0\n"));
                }
                json!(t)
            }
            "flux.demarrer" | "flux.veiller" => {
                *self.flux.lock().unwrap() = true;
                json!("")
            }
            "flux.arreter" => {
                *self.flux.lock().unwrap() = false;
                json!("")
            }
            "flux.lire" => {
                if !*self.flux.lock().unwrap() {
                    return Some(fausse_aide::refus("La trace réseau ne tourne pas."));
                }
                json!([])
            }
            _ => json!("ok"),
        };
        Some(fausse_aide::ok(r))
    }

    /// Les actions traitées depuis la n-ième
    fn depuis(&self, n: usize) -> Vec<String> {
        self.traitees.lock().unwrap().iter().skip(n).map(|x| x.0.clone()).collect()
    }

    fn nombre(&self) -> usize {
        self.traitees.lock().unwrap().len()
    }

    fn compte_depuis(&self, n: usize, a: &str) -> usize {
        self.depuis(n).iter().filter(|x| *x == a).count()
    }
}

/// Ce que fait l'hôte des signaux qui comptent ici : partie et mesure du lag entre les deux blocs
fn router(perf: &mut BlocPerf, trafic: &mut BlocTrafic, ctx: &Contexte, e: Effets) {
    let mut file: Vec<Effet> = e.0;
    while !file.is_empty() {
        let mut suite = Effets::default();
        for x in file {
            if let Effet::Signal { nom, donnees } = x {
                match nom.as_str() {
                    "partie-commencee" | "partie-finie" => trafic.signal(ctx, &nom, &donnees, &mut suite),
                    "mesure-lag" => perf.signal(ctx, &nom, &donnees, &mut suite),
                    _ => {}
                }
            }
        }
        file = suite.0;
    }
}

struct Essai {
    ctx: Arc<Contexte>,
    perf: BlocPerf,
    trafic: BlocTrafic,
    pc: Arc<Mutex<Partage>>,
    aide: Arc<FausseAide>,
    serveur: Option<nevermind_noyau::windows::canal::FauxServeur>,
    debut: Instant,
    d: std::path::PathBuf,
}

impl Essai {
    /// `guette` : Battlefield 6 installé (la veille des FPS et de la trace réseau sert avant lui)
    fn nouveau(nom: &str, guette: bool) -> Essai {
        let d = dossier_essai(&format!("aide-en-pause-{nom}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("donnees")).unwrap();
        let ctx = Contexte::nouveau(d.join("fichiers"), d.join("donnees"), false, true);
        ctx.ecrire_reglage("FpsMeasure", json!(true), false).unwrap();
        // (sans droits, comme Nevermind chez Jordan)
        ctx.regler_admin_essai(false).unwrap();
        let ctx = ctx.partage();
        let aide = Arc::new(FausseAide {
            parle: AtomicBool::new(true),
            traitees: Mutex::new(Vec::new()),
            fps: Mutex::new(Fps::Rien),
            flux: Mutex::new(false),
            entete: Mutex::new(false),
        });
        let a = aide.clone();
        let serveur = fausse_aide::lancer(&ctx, move |action, p| a.repondre(action, p)).expect("fausse Aide");
        let pc = Arc::new(Mutex::new(Partage::default()));
        let pm = ctx.appli.join("outils-tiers").join("PresentMon.exe").to_string_lossy().into_owned();
        let mut chemins = HashMap::new();
        for pid in [7000201u32, 7000202, 7000203] {
            chemins.insert(pid, RL.to_string());
        }
        let sys = FauxSysteme {
            partage: Some(pc.clone()),
            chemins,
            fichiers: vec![pm],
            cpu: Some((100, 1000)),
            ram: Some((16e9, 8e9)),
            ..Default::default()
        };
        let mut perf = BlocPerf::avec(Box::new(sys), Box::new(FauxCompteur::default()));
        let mut trafic = BlocTrafic::avec_systeme(Arc::new(FauxReseau(pc.clone())));
        let mut jeux = vec![json!({ "exes": [RL], "nom": "Rocket League", "source": "Epic" })];
        if guette {
            jeux.push(json!({ "exes": [BF6], "nom": "Battlefield 6", "source": "EA" }));
        }
        let mut e = Effets::default();
        perf.signal(&ctx, "jeux-liste", &Value::Array(jeux), &mut e);
        router(&mut perf, &mut trafic, &ctx, e);
        Essai { ctx, perf, trafic, pc, aide, serveur: Some(serveur), debut: Instant::now(), d }
    }

    /// Un passage des deux blocs, à l'heure réelle (la pause de l'Aide compte en vrai temps)
    fn pas(&mut self) {
        self.perf.horloge = Some(self.debut.elapsed().as_secs_f64());
        let mut e = Effets::default();
        self.perf.tic(&self.ctx, true, &mut e);
        router(&mut self.perf, &mut self.trafic, &self.ctx, e);
        let mut e = Effets::default();
        self.trafic.tic(&self.ctx, true, &mut e);
        router(&mut self.perf, &mut self.trafic, &self.ctx, e);
    }

    /// Les blocs tournent pendant ce temps
    fn tourner(&mut self, d: Duration) {
        let fin = Instant::now() + d;
        while Instant::now() < fin {
            self.pas();
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Les blocs tournent jusqu'à ce que `ok` soit vrai (faux si le temps est passé avant)
    fn jusqu_a(&mut self, d: Duration, ok: impl Fn(&FausseAide) -> bool) -> bool {
        let fin = Instant::now() + d;
        while Instant::now() < fin {
            if ok(&self.aide) {
                return true;
            }
            self.pas();
            std::thread::sleep(Duration::from_millis(50));
        }
        ok(&self.aide)
    }

    fn lancer_jeu(&mut self, pid: u32) {
        let mut p = self.pc.lock().unwrap();
        p.progs.push(Programme { pid, nom: "RocketLeague".into(), ..Default::default() });
        p.premier_plan = pid;
    }

    fn quitter_jeu(&mut self, pid: u32) {
        let mut p = self.pc.lock().unwrap();
        p.progs.retain(|x| x.pid != pid);
        p.premier_plan = 0;
    }

    /// Une partie qui commence et dont les FPS et la trace réseau sont lus par l'Aide
    fn partie_mesuree(&mut self, pid: u32) {
        self.lancer_jeu(pid);
        let n = self.aide.nombre();
        let ok = self.jusqu_a(Duration::from_secs(15), |a| {
            *a.fps.lock().unwrap() == Fps::Partie(u64::from(pid))
                && *a.flux.lock().unwrap()
                && a.compte_depuis(n, "fps.lire") >= 4
        });
        assert!(ok, "la partie n'est pas mesurée avant la pause : {:?}", self.aide.depuis(0));
    }

    /// L'Aide se tait, et les blocs tournent jusqu'à ce qu'une demande lui soit restée sans réponse (la pause
    /// commence)
    fn aide_muette(&mut self) {
        let recues = |s: &Essai| s.serveur.as_ref().map(|x| x.demandes().len()).unwrap_or(0);
        self.aide.parle.store(false, Ordering::SeqCst);
        let avant = recues(self);
        let fin = Instant::now() + Duration::from_secs(10);
        while recues(self) == avant {
            assert!(Instant::now() < fin, "aucune demande n'arrive à l'Aide muette");
            self.pas();
            std::thread::sleep(Duration::from_millis(50));
        }
        // (la demande reçue attend son délai : un passage de plus la laisse finir sans réponse)
        self.pas();
    }

    fn finir(mut self) {
        drop(self.serveur.take());
        let _ = std::fs::remove_dir_all(&self.d);
    }
}

/// M1 : une Aide muette 6 s en pleine partie. Avant, les 20 refus de la pause arrivaient en 5 s et la mesure des FPS
/// s'arrêtait pour toute la partie. Maintenant les FPS reprennent quand l'Aide répond de nouveau.
#[test]
fn les_fps_reprennent_apres_une_aide_muette_6_s() {
    let mut e = Essai::nouveau("fps", false);
    e.partie_mesuree(7000201);
    e.aide_muette();
    e.tourner(Duration::from_secs(6));
    e.aide.parle.store(true, Ordering::SeqCst);
    let n = e.aide.nombre();
    let ok = e.jusqu_a(Duration::from_secs(30), |a| a.compte_depuis(n, "fps.lire") >= 8);
    assert!(
        ok,
        "les FPS ne reprennent pas après 6 s d'Aide muette : demandes traitées depuis la reprise {:?}",
        e.aide.depuis(n)
    );
    assert_eq!(*e.aide.fps.lock().unwrap(), Fps::Partie(7000201), "la mesure de la partie ne tourne plus");
    e.finir();
}

/// M1, la cause seule : même avec une pause courte (1 s au lieu de 60 s, levée par le réveil dès que l'Aide répond),
/// les refus « en pause » ne comptent pas comme des lectures ratées. Avant, 20 refus en 5 s coupaient la mesure
/// pour toute la partie, même une fois la pause levée.
#[test]
fn les_refus_de_la_pause_ne_coupent_pas_la_mesure() {
    let mut e = Essai::nouveau("refus-pause", false);
    e.ctx.regler_pause_essai(Duration::from_secs(1)).unwrap();
    e.partie_mesuree(7000201);
    e.aide_muette();
    e.tourner(Duration::from_secs(6));
    e.aide.parle.store(true, Ordering::SeqCst);
    let n = e.aide.nombre();
    let ok = e.jusqu_a(Duration::from_secs(30), |a| a.compte_depuis(n, "fps.lire") >= 8);
    assert!(
        ok,
        "la mesure des FPS est coupée par les refus de la pause : demandes traitées depuis la reprise {:?}",
        e.aide.depuis(n)
    );
    e.finir();
}

/// Les arrêts refusés pendant la pause : la partie finit pendant que l'Aide se tait. Avant, `fps.arreter` et
/// `flux.arreter` étaient refusés et jamais refaits : PresentMon et la trace réseau tournaient dans l'Aide jusqu'à sa
/// fermeture. Maintenant l'Aide les reçoit quand elle répond de nouveau.
#[test]
fn un_arret_demande_pendant_la_pause_est_recu_a_la_reprise() {
    let mut e = Essai::nouveau("arret", false);
    e.partie_mesuree(7000201);
    e.aide_muette();
    e.quitter_jeu(7000201);
    // (la fin de partie est vue en 5 s au plus : les deux arrêts sont demandés pendant que l'Aide se tait)
    e.tourner(Duration::from_secs(7));
    e.aide.parle.store(true, Ordering::SeqCst);
    let n = e.aide.nombre();
    let ok = e.jusqu_a(Duration::from_secs(30), |a| {
        a.compte_depuis(n, "fps.arreter") >= 1 && a.compte_depuis(n, "flux.arreter") >= 1
    });
    assert!(ok, "les arrêts demandés pendant la pause ne sont pas refaits à la reprise : {:?}", e.aide.depuis(n));
    assert!(!*e.aide.flux.lock().unwrap(), "la trace réseau tourne encore dans l'Aide");
    assert_ne!(*e.aide.fps.lock().unwrap(), Fps::Partie(7000201), "PresentMon mesure encore le jeu fermé");
    e.finir();
}

/// Ajustement B de l'avis : un arrêt gardé ne coupe jamais la mesure suivante. La partie finit pendant que l'Aide se
/// tait, puis une nouvelle partie commence AVANT que l'Aide réponde : après la reprise, c'est la mesure de la
/// nouvelle partie (FPS et trace réseau du lag) qui tourne, et elle tourne encore quelques secondes plus tard.
#[test]
fn un_arret_garde_ne_coupe_pas_la_partie_suivante() {
    let mut e = Essai::nouveau("nouvelle-partie", false);
    e.partie_mesuree(7000201);
    e.aide_muette();
    e.quitter_jeu(7000201);
    e.tourner(Duration::from_secs(6));
    e.lancer_jeu(7000202);
    e.tourner(Duration::from_secs(6));
    e.aide.parle.store(true, Ordering::SeqCst);
    let n = e.aide.nombre();
    let tourne = |a: &FausseAide| *a.fps.lock().unwrap() == Fps::Partie(7000202) && *a.flux.lock().unwrap();
    let ok = e.jusqu_a(Duration::from_secs(30), |a| tourne(a) && a.compte_depuis(n, "fps.lire") >= 4);
    assert!(ok, "la mesure de la nouvelle partie ne tourne pas après la reprise : {:?}", e.aide.depuis(n));
    // (quelques secondes plus tard : aucun arrêt gardé n'est arrivé après le départ de la nouvelle mesure)
    e.tourner(Duration::from_secs(6));
    assert!(tourne(&e.aide), "un arrêt gardé a coupé la nouvelle partie : {:?}", e.aide.depuis(n));
    let apres = e.aide.depuis(n);
    let dernier = |pre: &str| apres.iter().rposition(|x| x.starts_with(pre));
    let depart_fps = apres.iter().rposition(|x| x == "fps.demarrer");
    assert!(
        dernier("fps.arreter").is_none_or(|i| depart_fps.is_some_and(|d| i < d)),
        "fps.arreter envoyé après le départ de la nouvelle mesure : {apres:?}"
    );
    e.finir();
}

/// Même chose avec la veille de la trace réseau (Battlefield 6 installé) : la trace de la partie est arrêtée pendant
/// que l'Aide se tait, puis la veille (`flux.veiller`, et `fps.veiller`) est redemandée après la reprise. L'arrêt
/// gardé ne doit jamais arriver après elle : la veille tourne, et tourne encore quelques secondes plus tard.
#[test]
fn un_arret_de_trace_garde_ne_coupe_pas_la_veille() {
    let mut e = Essai::nouveau("veille", true);
    // (la veille d'avant les jeux guettés, au démarrage)
    let ok = e.jusqu_a(Duration::from_secs(10), |a| *a.flux.lock().unwrap() && *a.fps.lock().unwrap() == Fps::Veille);
    assert!(ok, "pas de veille au démarrage : {:?}", e.aide.depuis(0));
    e.partie_mesuree(7000201);
    e.aide_muette();
    e.quitter_jeu(7000201);
    e.tourner(Duration::from_secs(7));
    e.aide.parle.store(true, Ordering::SeqCst);
    let n = e.aide.nombre();
    let veille = |a: &FausseAide| *a.flux.lock().unwrap() && *a.fps.lock().unwrap() == Fps::Veille;
    let ok = e.jusqu_a(Duration::from_secs(30), |a| veille(a) && a.compte_depuis(n, "flux.veiller") >= 1);
    assert!(ok, "la veille ne tourne pas après la reprise : {:?}", e.aide.depuis(n));
    e.tourner(Duration::from_secs(6));
    let apres = e.aide.depuis(n);
    assert!(veille(&e.aide), "un arrêt gardé a coupé la veille : {apres:?}");
    let i_arret = apres.iter().rposition(|x| x == "flux.arreter");
    let i_veille = apres.iter().rposition(|x| x == "flux.veiller");
    assert!(i_arret.is_none_or(|i| i_veille.is_some_and(|v| i < v)), "flux.arreter envoyé après la veille : {apres:?}");
    e.finir();
}

/// Ajustement B, fin : Nevermind se ferme pendant la pause (l'Aide répond de nouveau, mais la pause n'est pas encore
/// levée). Les arrêts de la mesure des FPS et de la trace réseau sont tentés une dernière fois, dans le temps laissé
/// par Mindset (6 s en tout).
#[test]
fn un_arret_garde_est_tente_a_la_fermeture() {
    let mut e = Essai::nouveau("fermeture", false);
    e.partie_mesuree(7000201);
    e.aide_muette();
    e.aide.parle.store(true, Ordering::SeqCst);
    // (sans passage des blocs : rien ne lève la pause. La fausse Aide sert une demande à la fois : la sonde envoyée
    //  pendant qu'elle se taisait la tient jusqu'à son délai, 10 s ; ensuite elle est libre)
    std::thread::sleep(Duration::from_secs(12));
    let n = e.aide.nombre();
    let t = Instant::now();
    e.perf.fermer(&e.ctx);
    e.trafic.fermer(&e.ctx);
    let duree = t.elapsed();
    assert!(
        e.aide.compte_depuis(n, "fps.arreter") >= 1 && e.aide.compte_depuis(n, "flux.arreter") >= 1,
        "les arrêts ne sont pas tentés à la fermeture pendant la pause : {:?}",
        e.aide.depuis(n)
    );
    assert!(duree < Duration::from_secs(5), "la fermeture a pris {duree:?} (Mindset coupe à 6 s)");
    e.finir();
}
