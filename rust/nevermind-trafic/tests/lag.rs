//! La mesure du lag (Update-LagSession, Stop-LagSession), sur une fausse trace réseau et de faux pings : les cas de
//! outils\tester-reseau.ps1 (anciens serveurs plus pingés, box des autres joueurs écartées, annonces 224 à 255
//! jamais prises pour le serveur) et la mesure gardée à la fin. Aucun ping ne part.

use nevermind_trafic::ip;
use nevermind_trafic::lag::pings::{Pingeur, Pings};
use nevermind_trafic::lag::session::{Env, Session};
use nevermind_trafic::serveurs::{Annuaire, Entree, Lieu};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Faux pings : note les hôtes pingés et ceux abandonnés
#[derive(Clone, Default)]
struct FauxPings {
    hotes: Arc<Mutex<Vec<String>>>,
    retires: Arc<Mutex<Vec<usize>>>,
    series: Arc<Mutex<HashMap<usize, Vec<f64>>>>,
}

impl Pings for FauxPings {
    fn ajouter(&mut self, hote: &str) -> usize {
        let mut h = self.hotes.lock().unwrap();
        h.push(hote.into());
        h.len() - 1
    }
    fn retirer(&mut self, i: usize) {
        self.retires.lock().unwrap().push(i);
    }
    fn serie(&self, i: usize) -> Vec<f64> {
        self.series.lock().unwrap().get(&i).cloned().unwrap_or_default()
    }
    fn arreter(&mut self) {}
    fn hote(&self, i: usize) -> String {
        self.hotes.lock().unwrap().get(i).cloned().unwrap_or_default()
    }
}

struct PasDePing;
impl Pingeur for PasDePing {
    fn ping(&self, _: &str) -> Option<f64> {
        None
    }
    fn trace(&self, _: &str, _: u8, _: u32) -> Vec<String> {
        Vec::new()
    }
}

struct FauxEnv {
    flux: Vec<String>,
    /// Les connexions TCP (pid|adresse|port) : le repli sans la trace
    tcp: Vec<String>,
    annuaire: Annuaire,
    lieux: HashMap<String, Lieu>,
    journal: Vec<String>,
}

impl Env for FauxEnv {
    fn flux(&mut self) -> Vec<String> {
        self.flux.clone()
    }
    fn tcp(&mut self) -> Vec<String> {
        self.tcp.clone()
    }
    fn nom_processus(&mut self, pid: u32) -> String {
        format!("Programme {pid}")
    }
    fn proprietaire(&mut self, ip: &str) -> Option<(String, String, String)> {
        self.annuaire.proprietaire(ip).map(|e| (e.o.clone(), e.n.clone(), e.libelle()))
    }
    fn lieu(&self, ip: &str) -> Option<Lieu> {
        self.lieux.get(ip).cloned()
    }
    fn chercher_proprietaire(&mut self, _: &str) {}
    fn chercher_lieu(&mut self, _: &str) {}
    fn journal(&mut self, m: &str) {
        self.journal.push(m.into());
    }
    fn moi(&self) -> u32 {
        1
    }
}

fn entree(de: &str, a: &str, o: &str, n: &str) -> Entree {
    Entree { s: ip::hex(de), e: ip::hex(a), o: o.into(), c: "FR".into(), n: n.into(), d: "2026-10-22".into() }
}

/// pid|proto|adresse|port|paquets envoyés|paquets reçus|octets envoyés|octets reçus|?|trous|plus long trou|vu il y a
fn flux(ip: &str, recus: u32) -> String {
    format!("1234|udp|{ip}|3659|{recus}|{recus}|1000|1000|0|0|0|100")
}

fn session(p: &FauxPings) -> Session {
    Session::demarrer(
        "Jeu de test",
        1234,
        0.0,
        0.0,
        None,
        true,
        Box::new(p.clone()),
        Arc::new(PasDePing),
        Arc::new(Vec::new),
    )
}

/// Un passage ; `jusqua` : jusqu'au prochain changement de serveur permis (tous les 15 passages)
fn passer(s: &mut Session, e: &mut FauxEnv, t: &mut f64, jusqua: bool) {
    loop {
        *t += 2.0;
        s.passage(e, *t);
        if !jusqua || s.passages.is_multiple_of(15) {
            break;
        }
    }
}

#[test]
fn serveurs_suivis_et_box_des_autres_joueurs() {
    let p = FauxPings::default();
    let mut s = session(&p);
    let mut e = FauxEnv {
        flux: Vec::new(),
        tcp: Vec::new(),
        annuaire: Annuaire::default(),
        lieux: HashMap::new(),
        journal: Vec::new(),
    };
    for x in [
        entree("35.207.0.0", "35.207.255.255", "Google LLC", "GOOGLE-CLOUD"),
        entree("63.176.0.0", "63.179.255.255", "Amazon.com, Inc.", "AMAZO-4"),
        entree("90.100.0.0", "90.101.255.255", "Wanadoo France Technical", "IP2000-ADSL-BAS"),
        entree("83.196.0.0", "83.196.255.255", "Wanadoo France Technical", "IP2000-ADSL-BAS"),
    ] {
        e.annuaire.ajouter(x);
    }
    let mut t = 0.0;
    let hote = |i: usize| p.hotes.lock().unwrap()[i].clone();
    e.flux = vec![flux("35.207.176.221", 200)];
    passer(&mut s, &mut e, &mut t, false);
    assert_eq!(s.serveur.as_ref().map(|x| x.ip.as_str()), Some("35.207.176.221"), "premier serveur");
    let a = s.cibles["srv"];
    assert_eq!(hote(a), "35.207.176.221");
    // Le serveur change : l'ancien n'est plus pingé
    e.flux = vec![flux("35.207.176.221", 60), flux("63.178.238.177", 400)];
    passer(&mut s, &mut e, &mut t, true);
    assert_eq!(s.serveur.as_ref().unwrap().ip, "63.178.238.177");
    assert!(p.retires.lock().unwrap().contains(&a), "l'ancien serveur reste pingé");
    let b = s.cibles["srv"];
    assert!(b != a && hote(b) == "63.178.238.177");
    // La box d'un joueur (ligne ADSL d'Orange, propriétaire connu) : jamais prise pour le serveur
    e.flux = vec![flux("63.178.238.177", 60), flux("90.101.111.7", 900)];
    passer(&mut s, &mut e, &mut t, true);
    assert_eq!(s.serveur.as_ref().unwrap().ip, "63.178.238.177");
    assert!(s.pairs.contains("90.101.111.7"));
    assert!(!p.hotes.lock().unwrap().contains(&"90.101.111.7".to_string()));
    assert!(!p.retires.lock().unwrap().contains(&b));
    assert_eq!(e.journal.iter().filter(|m| m.contains("90.101.111.7") && m.contains("autre joueur")).count(), 1);
    // Une box inconnue au moment du choix : écartée dès que son propriétaire est connu
    e.flux = vec![flux("63.178.238.177", 60), flux("88.165.8.13", 900)];
    passer(&mut s, &mut e, &mut t, true);
    assert_eq!(s.serveur.as_ref().unwrap().ip, "88.165.8.13");
    let c = s.cibles["srv"];
    e.annuaire.ajouter(entree("88.160.0.0", "88.191.255.255", "Administrative", "FR-PROXAD-ADSL"));
    e.annuaire.manques.clear();
    passer(&mut s, &mut e, &mut t, false);
    assert!(p.retires.lock().unwrap().contains(&c), "la box reste pingée une fois reconnue");
    assert!(s.pairs.contains("88.165.8.13"));
    assert_eq!(s.serveur.as_ref().map(|x| x.ip.as_str()), Some("63.178.238.177"), "le vrai serveur n'est pas repris");
    // La box n'est plus jamais choisie
    e.flux = vec![flux("88.165.8.13", 900), flux("35.207.109.101", 300)];
    passer(&mut s, &mut e, &mut t, true);
    assert_eq!(s.serveur.as_ref().map(|x| x.ip.as_str()), Some("35.207.109.101"));
    // Les annonces au réseau local (FC 27 : 255.255.255.255:9999, groupes 224 à 239) ne sont jamais le serveur
    e.flux = vec![
        flux("255.255.255.255", 5000),
        flux("239.255.255.250", 3000),
        flux("224.0.0.251", 2000),
        flux("35.207.109.101", 300),
    ];
    passer(&mut s, &mut e, &mut t, true);
    assert_eq!(s.serveur.as_ref().map(|x| x.ip.as_str()), Some("35.207.109.101"));
    assert!(
        !p.hotes
            .lock()
            .unwrap()
            .iter()
            .any(|h| h.starts_with("22") || h.starts_with("23") || h.starts_with("24") || h.starts_with("25"))
    );
    // Le nom (ipinfo) : « ...abo.wanadoo.fr » est une box
    e.lieux.insert(
        "86.200.1.2".into(),
        Lieu { pays: "FR".into(), ville: "Metz".into(), hote: "lfbn-nan-1-234-56.w86-200.abo.wanadoo.fr".into() },
    );
    e.flux = vec![flux("86.200.1.2", 5000), flux("35.207.109.101", 300)];
    passer(&mut s, &mut e, &mut t, true);
    assert_eq!(s.serveur.as_ref().map(|x| x.ip.as_str()), Some("35.207.109.101"));
    assert!(s.pairs.contains("86.200.1.2"));
}

#[test]
fn mesure_gardee_a_la_fin() {
    let p = FauxPings::default();
    let mut s = session(&p);
    let mut e = FauxEnv {
        flux: vec![flux("35.207.176.221", 200)],
        tcp: Vec::new(),
        annuaire: Annuaire::default(),
        lieux: HashMap::new(),
        journal: Vec::new(),
    };
    e.annuaire.ajouter(entree("35.207.0.0", "35.207.255.255", "Google LLC", "GOOGLE-CLOUD"));
    let mut t = 0.0;
    passer(&mut s, &mut e, &mut t, false);
    // Des pings : la référence stable, le serveur à 110 ms (loin), sur 2 minutes
    let mut refs = Vec::new();
    let mut srv = Vec::new();
    for i in 0..240 {
        let x = i as f64 * 0.5;
        refs.extend([x, 12.0 + (i % 3) as f64]);
        srv.extend([x, 110.0 + (i % 2) as f64]);
    }
    p.series.lock().unwrap().insert(s.cibles["ref"], refs);
    p.series.lock().unwrap().insert(s.cibles["srv"], srv);
    let rec = s.finir(&mut e, 125.0, "2026-11-06T21:00:00.0000000+01:00", "abcdef0123").expect("mesure gardée");
    assert_eq!(rec["Seconds"], 125);
    assert_eq!(rec["Step"], 1);
    assert_eq!(rec["Server"]["Owner"], "Google LLC (France)");
    assert_eq!(rec["Stats"]["srv"]["Med"], 111);
    assert_eq!(rec["Diag"]["Level"], "warn");
    assert_eq!(rec["Diag"]["Title"], "Le serveur du jeu est loin");
    // Trop courte : pas gardée
    let p2 = FauxPings::default();
    let s2 = session(&p2);
    assert!(s2.finir(&mut e, 59.0, "x", "y").is_none());
    let _: Value = rec;
}

/// Lot F : l'Aide dormait au début de la partie, la mesure part sans la trace (le serveur de connexion du jeu, en
/// TCP, est suivi après 3 passages). La trace reprend en cours de partie (`reprendre_trace`) : dès le passage
/// suivant, le serveur de la partie qu'elle montre (UDP) remplace le serveur de connexion, sans attendre le contrôle
/// des 15 passages, et ses pertes sont suivies.
#[test]
fn la_trace_reprise_en_cours_de_partie_trouve_le_serveur_de_la_partie() {
    let p = FauxPings::default();
    let mut s = Session::demarrer(
        "Jeu de test",
        1234,
        0.0,
        0.0,
        None,
        false,
        Box::new(p.clone()),
        Arc::new(PasDePing),
        Arc::new(Vec::new),
    );
    let mut e = FauxEnv {
        flux: vec![flux("35.207.176.221", 200)],
        tcp: vec!["1234|63.178.238.177|443".into()],
        annuaire: Annuaire::default(),
        lieux: HashMap::new(),
        journal: Vec::new(),
    };
    let mut t = 0.0;
    for _ in 0..3 {
        passer(&mut s, &mut e, &mut t, false);
    }
    assert_eq!(s.serveur.as_ref().map(|x| (x.ip.as_str(), x.proto.as_str())), Some(("63.178.238.177", "tcp")));
    assert_ne!(s.passages % 15, 14, "le passage suivant serait celui du contrôle des 15 passages");
    s.reprendre_trace();
    assert!(s.etw);
    passer(&mut s, &mut e, &mut t, false);
    assert_eq!(
        s.serveur.as_ref().map(|x| (x.ip.as_str(), x.proto.as_str())),
        Some(("35.207.176.221", "udp")),
        "le serveur de la partie n'est pas pris dès la reprise de la trace"
    );
    // (puis il reste le serveur suivi, comme avant)
    passer(&mut s, &mut e, &mut t, false);
    assert_eq!(s.serveur.as_ref().unwrap().ip, "35.207.176.221");
}
