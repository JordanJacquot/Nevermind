//! Nevermind en Rust : le bloc « trafic ». Lag en ligne (lag.ps1), la page Trafic (trafic.ps1) et ce que Windows
//! envoie à Microsoft (microsoft.ps1), avec leurs pages de l'écran (ecran-jeux.ps1, ecran-panneau.ps1).
//!
//! Légèreté (le but du passage en Rust) : rien ne tourne quand ce n'est pas utile. Pas de mesure du lag hors
//! partie ou test rapide ; la surveillance du Trafic ne tourne que si l'utilisateur l'a démarrée, toutes les 2 s
//! quand sa page est regardée, toutes les 5 s sur une autre page, toutes les 10 s quand l'écran est caché et
//! toutes les 30 s pendant une partie ; les connexions sont lues par Windows directement (GetExtendedTcpTable,
//! GetPerTcpConnectionEStats), sans PowerShell.
//!
//! Signaux attendus : « partie-commencee » { jeu, exe, pid } et « partie-finie » { jeu, exe, pid, duree_s } (bloc Perf), « jeux-index »
//! { Exes: [...] } (les .exe des jeux installés). Signaux envoyés : voir `signaux`.

pub mod fichiers;
pub mod format;
pub mod ip;
pub mod lag;
pub mod microsoft;
pub mod pages;
pub mod serveurs;
pub mod systeme;
pub mod trafic;
pub mod valeur;
pub mod windows;

use lag::session::{Env, Session};
use nevermind_noyau::travaux::Rendu;
use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use serde_json::{Map, Value, json};
use serveurs::{Annuaire, Entree, Lieu, Reponse};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};
use systeme::Systeme;
use trafic::alertes::{self, Alerte};
use trafic::etat::{Processus, Releve, Service};

/// Les noms des signaux échangés avec les autres blocs et l'hôte
pub mod signaux {
    /// Reçu du bloc Perf : une partie commence { jeu, exe, pid }
    pub const PARTIE_COMMENCEE: &str = "partie-commencee";
    /// Reçu du bloc Perf : la partie est finie { jeu, exe, pid, duree_s }
    pub const PARTIE_FINIE: &str = "partie-finie";
    /// Reçu : les jeux installés { Exes: ["c:\\...\\jeu.exe"] }
    pub const JEUX_INDEX: &str = "jeux-index";
    /// Envoyé à l'hôte (signal commun) : poser une question (`Confirm-Action`) { id, titre, texte, boutons, details,
    /// action } ; l'hôte rappelle `action(action, { id, valeur })`
    pub const QUESTION: &str = "question";
    /// Envoyé à l'hôte (signal commun) : une ligne de l'historique (`Add-History`) { titre, details, log }
    pub const HISTORIQUE: &str = "historique";
    /// Envoyé à l'hôte : une bulle de Windows (`Show-Notify`) { Titre, Texte, Action, Valeur, Ecran }
    pub const NOTIFICATION: &str = "notification";
    /// Envoyé au bloc de la Sauvegarde : des écritures notées pour « Revenir en arrière » (Set-Reg, Set-ServiceOff
    /// puis Add-History) { Titre, Details, Ecritures, Historique, Message }
    pub const SAUVEGARDE: &str = "sauvegarde-appliquer";
    /// Envoyé au bloc Protection : une analyse Defender d'un programme (`Invoke-TrafficScan`) { Sorte, Chemins,
    /// Titre, Marque } ; s'il ne trouve rien, il ajoute `Marque.Cle` au réglage `Marque.Reglage`
    pub const ANALYSE: &str = "analyse-demandee";
    /// Envoyé au bloc des FPS : la trace réseau a été refusée par l'anti-triche de ce jeu, la lancer avant lui la
    /// prochaine fois (`Add-FpsVeilleJeu`) { Processus }
    pub const VEILLE_JEU: &str = "veille-jeu-ajouter";
    /// Envoyé au bloc Perf : une mesure du lag commence ou finit (partie ou test rapide) { en_cours } ; pendant
    /// qu'elle tourne, la trace réseau n'est pas mise en veille (`$script:LagSession` dans Update-FpsVeille)
    pub const MESURE_LAG: &str = "mesure-lag";
}

/// Le panneau de détail ouvert par ce bloc
#[derive(Clone, Debug, PartialEq)]
enum Detail {
    Lag(String),
    App(String),
    Microsoft,
}

struct PanneauOuvert {
    detail: Detail,
    de: String,
    montre: bool,
    boutons: Vec<pages::BoutonPanneau>,
    reglages: Vec<microsoft::Reglage>,
}

/// Une action qui attend la réponse à une question
#[derive(Clone, Debug)]
enum Attente {
    Confiance(String),
    Bloquer(String),
}

/// Les travaux en arrière-plan (signatures, annuaires, cache DNS) : leurs réponses arrivent au prochain passage
#[derive(Default)]
struct Travaux {
    signatures: Option<Receiver<Vec<ResultatSig>>>,
    annuaire: Vec<Receiver<Vec<Reponse>>>,
    lieux: Vec<Receiver<(String, Result<Lieu, String>)>>,
    dns: Option<Receiver<Vec<(String, String)>>>,
    dns_a: Option<f64>,
    annuaire_fait: HashSet<String>,
    annuaire_essais: HashMap<String, u32>,
    annuaire_en_cours: bool,
}

struct ResultatSig {
    cle: String,
    sig: String,
    editeur: String,
    parent: String,
    parent_sig: String,
    parent_pub: String,
    desc: String,
}

pub struct BlocTrafic {
    sys: Option<Arc<dyn Systeme>>,
    debut: Instant,
    annuaire: Annuaire,
    sessions: Option<Vec<Value>>,
    lag: Option<Session>,
    lieux: HashMap<String, Lieu>,
    releve: Option<Releve>,
    surveillance: bool,
    alertes: Vec<Alerte>,
    partie: Option<(String, u32)>,
    ecran: String,
    cache: bool,
    panneau: Option<PanneauOuvert>,
    kit_apps: Vec<String>,
    kit_alertes: Vec<Alerte>,
    attente: Option<Attente>,
    travaux: Travaux,
    /// Les gestes longs par l'Aide (bloquer un programme) : dans un fil à part (audit G3)
    fond: nevermind_noyau::travaux::Travaux<Vec<Effet>>,
    /// Le programme dont le blocage est en cours (pour dire en clair pourquoi un autre « Oui » est refusé)
    bloque_en_cours: Option<String>,
    jeux_exes: Vec<String>,
    compteurs_ok: bool,
    aide_tcp_ko: Option<f64>,
    passage_trafic: Option<f64>,
    passage_lag: Option<f64>,
    /// Passages de la surveillance du Trafic et de la mesure du lag faits (pour la mesure de légèreté)
    pub lectures: u64,
}

impl Default for BlocTrafic {
    fn default() -> Self {
        BlocTrafic {
            sys: None,
            debut: Instant::now(),
            annuaire: Annuaire::default(),
            sessions: None,
            lag: None,
            lieux: HashMap::new(),
            releve: None,
            surveillance: false,
            alertes: Vec::new(),
            partie: None,
            ecran: String::new(),
            cache: false,
            panneau: None,
            kit_apps: Vec::new(),
            kit_alertes: Vec::new(),
            attente: None,
            travaux: Travaux::default(),
            fond: Default::default(),
            bloque_en_cours: None,
            jeux_exes: Vec::new(),
            compteurs_ok: false,
            aide_tcp_ko: None,
            passage_trafic: None,
            passage_lag: None,
            lectures: 0,
        }
    }
}

const ACTION_QUESTION: &str = "trafic-reponse";

/// Une date au format « o » de .NET (« 2026-11-06T21:03:12.1234567+01:00 »)
pub fn date_o(d: chrono::DateTime<chrono::Local>) -> String {
    use chrono::Timelike;
    format!("{}.{:07}{}", d.format("%Y-%m-%dT%H:%M:%S"), d.nanosecond() % 1_000_000_000 / 100, d.format("%:z"))
}

fn id_aleatoire() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    format!("{:016x}", h.finish())[..10].to_string()
}

/// Les processus vus par Windows, pour regrouper les connexions par programme
struct Procs<'a> {
    sys: &'a dyn Systeme,
    services: std::cell::RefCell<Option<HashMap<u32, Vec<Service>>>>,
}

impl Processus for Procs<'_> {
    fn noms(&self) -> HashMap<u32, String> {
        self.sys.processus().into_iter().map(|(p, _, n)| (p, n)).collect()
    }
    fn chemin(&self, pid: u32) -> String {
        self.sys.chemin(pid)
    }
    fn services(&self, pid: u32) -> Vec<Service> {
        let mut s = self.services.borrow_mut();
        s.get_or_insert_with(|| self.sys.services()).get(&pid).cloned().unwrap_or_default()
    }
}

/// L'environnement d'une mesure du lag (le bloc lui-même)
struct EnvLag<'a> {
    ctx: &'a Contexte,
    sys: &'a dyn Systeme,
    annuaire: &'a mut Annuaire,
    lieux: &'a HashMap<String, Lieu>,
    a_chercher: Vec<String>,
    lieux_a_chercher: Vec<String>,
}

impl Env for EnvLag<'_> {
    fn flux(&mut self) -> Vec<String> {
        // (Get-FluxReseau : Invoke-Aide -SansDemande, jamais de demande de Windows pendant une partie)
        match self.ctx.aide_de_fond("flux.lire", &json!({}), 15000) {
            Ok(v) => valeur::liste(&v).iter().filter(|x| valeur::vrai(x)).map(valeur::texte).collect(),
            Err(_) => Vec::new(),
        }
    }
    fn tcp(&mut self) -> Vec<String> {
        self.sys.tcp().0
    }
    fn nom_processus(&mut self, pid: u32) -> String {
        let mut n = format!("Programme {pid}");
        if let Some((_, _, nom)) = self.sys.processus().into_iter().find(|(p, _, _)| *p == pid) {
            n = nom;
            if n == "svchost" {
                if let Some(s) = self.sys.services().get(&pid).and_then(|l| l.first().cloned()) {
                    n = format!("Windows : {}", s.titre);
                }
            } else {
                let d = self.sys.description(&self.sys.chemin(pid));
                if !d.is_empty() && d.chars().count() < 50 {
                    n = d;
                }
            }
        }
        n
    }
    fn proprietaire(&mut self, ip: &str) -> Option<(String, String, String)> {
        self.annuaire.proprietaire(ip).map(|e| (e.o.clone(), e.n.clone(), e.libelle()))
    }
    fn lieu(&self, ip: &str) -> Option<Lieu> {
        self.lieux.get(ip).cloned()
    }
    fn chercher_proprietaire(&mut self, ip: &str) {
        self.a_chercher.push(ip.into());
    }
    fn chercher_lieu(&mut self, ip: &str) {
        self.lieux_a_chercher.push(ip.into());
    }
    fn journal(&mut self, m: &str) {
        self.ctx.journal(m);
    }
    fn moi(&self) -> u32 {
        std::process::id()
    }
}

impl BlocTrafic {
    /// Un bloc qui lit un autre Windows (les tests : un faux système)
    pub fn avec_systeme(sys: Arc<dyn Systeme>) -> BlocTrafic {
        BlocTrafic { sys: Some(sys), ..Default::default() }
    }

    fn sys(&mut self) -> Arc<dyn Systeme> {
        self.sys.get_or_insert_with(|| Arc::new(systeme::SystemeWindows::default())).clone()
    }

    fn maintenant(&self) -> f64 {
        self.debut.elapsed().as_secs_f64()
    }

    /// Une mesure du lag est en cours
    pub fn mesure_en_cours(&self) -> bool {
        self.lag.is_some()
    }

    /// La surveillance du Trafic tourne
    pub fn surveillance_active(&self) -> bool {
        self.surveillance
    }

    fn bool_reglage(ctx: &Contexte, nom: &str, defaut: bool) -> bool {
        ctx.reglage(nom).map(|v| valeur::vrai(&v)).unwrap_or(defaut)
    }

    /// `Test-LagMeasure` (mesure du lag active par défaut)
    pub fn mesure_lag(ctx: &Contexte) -> bool {
        Self::bool_reglage(ctx, "LagMeasure", true)
    }
    /// `Test-AnnuaireServeurs` (coupé par défaut : vie privée)
    pub fn annuaire_permis(ctx: &Contexte) -> bool {
        Self::bool_reglage(ctx, "TrafficLookup", false)
    }
    /// `Test-PaysServeur` (coupé par défaut : vie privée)
    pub fn pays_permis(ctx: &Contexte) -> bool {
        Self::bool_reglage(ctx, "LagPays", false)
    }

    fn charger_annuaire(&mut self, ctx: &Contexte) {
        if self.annuaire.charge {
            return;
        }
        let v = fichiers::lire(&ctx.donnees.join("serveurs.json"), &|m| ctx.journal(m))
            .ok()
            .flatten()
            .unwrap_or(Value::Null);
        self.annuaire = Annuaire::depuis(&v, chrono::Local::now().date_naive());
    }

    fn sauver_annuaire(&mut self, ctx: &Contexte) {
        let v = self.annuaire.json();
        let _ = fichiers::ecrire(&ctx.donnees.join("serveurs.json"), &v);
    }

    /// `Get-LagSessions` (avec les anciennes mesures corrigées une fois)
    fn sessions(&mut self, ctx: &Contexte) -> &mut Vec<Value> {
        if self.sessions.is_none() {
            let mut l: Vec<Value> = Vec::new();
            match fichiers::lire(&ctx.donnees.join("lag.json"), &|m| ctx.journal(m)) {
                Ok(Some(v)) => l = valeur::liste(&v).into_iter().filter(valeur::vrai).collect(),
                Ok(None) => {}
                Err(()) => ctx.journal("Lag: historique illisible (pris par un autre programme)"),
            }
            let mut refaits = 0;
            for s in l.iter_mut() {
                let vieux = valeur::liste(valeur::champ(valeur::champ(s, "Diag"), "Findings")).iter().any(|f| {
                    let t = valeur::texte(valeur::champ(f, "Title"));
                    t == "Le serveur du jeu est loin" || t == "Le serveur du jeu avait des soucis"
                });
                if lag::diag::connexion_seule(s) && vieux && s.is_object() {
                    let d = lag::diag::diagnostic(s);
                    s["Diag"] = d.json();
                    refaits += 1;
                }
            }
            self.sessions = Some(l);
            if refaits > 0 {
                ctx.journal(&format!("Lag: {refaits} ancienne(s) mesure(s) corrigée(s) (serveur de connexion pris pour le serveur de la partie)"));
                self.sauver_sessions(ctx);
            }
        }
        self.sessions.get_or_insert_with(Vec::new)
    }

    fn sauver_sessions(&mut self, ctx: &Contexte) {
        let l = self.sessions(ctx);
        while l.len() > 30 {
            l.remove(0);
        }
        let v = Value::Array(l.clone());
        if let Err(e) = fichiers::ecrire(&ctx.donnees.join("lag.json"), &v) {
            ctx.journal(&format!("Lag: {e}"));
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // Lag en ligne
    // -----------------------------------------------------------------------------------------------------------

    /// `Start-LagSession`
    pub fn demarrer_lag(&mut self, ctx: &Contexte, jeu: &str, pid: u32, secondes: f64, effets: &mut Effets) {
        if self.lag.is_some() {
            return;
        }
        let sys = self.sys();
        self.charger_annuaire(ctx);
        let reseau = sys.reseau();
        // Trace réseau par l'Aide (le serveur de la partie, en UDP) ; sinon le serveur de connexion du jeu (TCP)
        // (Start-FluxReseau : Test-MesuresAide, puis Invoke-Aide -SansDemande ; une trace déjà lancée par la veille
        //  d'avant le jeu, flux.veiller du bloc perf, est reprise telle quelle par l'Aide)
        let mut etw = false;
        let mut erreur = String::new();
        if ctx.mesures_aide() {
            match ctx.aide_de_fond("flux.demarrer", &json!({}), 20000) {
                Ok(_) => {
                    etw = true;
                    ctx.journal("Lag : trace réseau par l'Aide");
                }
                Err(e) => {
                    ctx.journal(&format!("Lag : trace réseau par l'Aide impossible ({e})"));
                    erreur = e;
                }
            }
        }
        // (un anti-triche refuse la trace une fois le jeu lancé : elle sera lancée avant lui la prochaine fois,
        //  Test-FpsRefus puis Add-FpsVeilleJeu)
        if !etw
            && pid != 0
            && nevermind_noyau::aide::fps_refus(&erreur)
            && let Some((_, _, nom)) = sys.processus().into_iter().find(|(p, _, _)| *p == pid)
        {
            // (tout de suite, comme Add-FpsVeilleJeu dans Start-LagSession : la ligne du journal vient avant celle de
            //  la mesure ; le signal reste pour l'hôte, qui n'a alors plus rien à ajouter)
            ctx.ajouter_fps_veille(&nom);
            effets.ajouter(Effet::Signal { nom: signaux::VEILLE_JEU.into(), donnees: json!({ "Processus": nom }) });
        }
        let pings = Box::new(lag::pings::Moniteur::nouveau(sys.pingeur(), 500));
        let s2 = sys.clone();
        let s = Session::demarrer(
            jeu,
            pid,
            secondes,
            self.maintenant(),
            reseau,
            etw,
            pings,
            sys.pingeur(),
            Arc::new(move || s2.wifi()),
        );
        ctx.journal(&format!(
            "Lag: mesure de la connexion ({jeu}){}",
            if etw { ", serveur repérable en UDP" } else { "" }
        ));
        self.lag = Some(s);
        self.passage_lag = Some(self.maintenant());
        effets.ajouter(Effet::Signal { nom: signaux::MESURE_LAG.into(), donnees: json!({ "en_cours": true }) });
        effets.ajouter(Effet::Rafraichir);
    }

    fn passer_lag(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let sys = self.sys();
        self.recevoir_lieux(ctx);
        self.recevoir_annuaire(ctx);
        let maintenant = self.maintenant();
        let Some(mut s) = self.lag.take() else { return };
        let mut env = EnvLag {
            ctx,
            sys: sys.as_ref(),
            annuaire: &mut self.annuaire,
            lieux: &self.lieux,
            a_chercher: Vec::new(),
            lieux_a_chercher: Vec::new(),
        };
        let fini = s.passage(&mut env, maintenant);
        let (a, l) = (std::mem::take(&mut env.a_chercher), std::mem::take(&mut env.lieux_a_chercher));
        self.lectures += 1;
        self.lag = Some(s);
        for ip in a {
            self.chercher_proprietaires(ctx, vec![ip]);
        }
        for ip in l {
            self.chercher_lieu(ctx, &ip);
        }
        if fini {
            self.arreter_lag(ctx, effets);
        }
    }

    /// `Stop-LagSession`
    pub fn arreter_lag(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let Some(mut s) = self.lag.take() else { return };
        self.passage_lag = None;
        effets.ajouter(Effet::Signal { nom: signaux::MESURE_LAG.into(), donnees: json!({ "en_cours": false }) });
        // (Stop-FluxReseau : Invoke-Aide -SansDemande ; par le canal seul, sans réveil ni pause : la trace vit dans
        //  l'Aide, une Aide qui ne répond pas n'en a plus ; lot B)
        if s.etw {
            let _ = ctx.aide_texte("flux.arreter", "{}", 15000);
        }
        self.recevoir_annuaire(ctx);
        self.recevoir_lieux(ctx);
        if let Some(srv) = s.serveur.as_mut()
            && let Some(l) = self.lieux.get(&srv.ip)
        {
            srv.pays = l.pays.clone();
            srv.ville = l.ville.clone();
        }
        let sys = self.sys();
        let maintenant = self.maintenant();
        let date = date_o(chrono::Local::now());
        let jeu = s.jeu.clone();
        let mut env = EnvLag {
            ctx,
            sys: sys.as_ref(),
            annuaire: &mut self.annuaire,
            lieux: &self.lieux,
            a_chercher: Vec::new(),
            lieux_a_chercher: Vec::new(),
        };
        let rec = s.finir(&mut env, maintenant, &date, &id_aleatoire());
        effets.ajouter(Effet::Rafraichir);
        let Some(rec) = rec else { return };
        let quick = valeur::vrai(valeur::champ(&rec, "Quick"));
        let niveau = valeur::texte(valeur::champ(valeur::champ(&rec, "Diag"), "Level"));
        let titre = valeur::texte(valeur::champ(valeur::champ(&rec, "Diag"), "Title"));
        if !quick && !valeur::vrai(valeur::champ(&rec, "Server")) && niveau == "ok" {
            ctx.journal(&format!("Lag: {jeu} sans serveur en ligne repéré et connexion stable, mesure non gardée"));
            return;
        }
        let id = valeur::texte(valeur::champ(&rec, "Id"));
        let lieu = lag::diag::lieu_serveur(valeur::champ(&rec, "Server"));
        ctx.journal(&format!(
            "Lag: {jeu}, {} min, {titre}{}",
            (valeur::num(valeur::champ(&rec, "Seconds")) / 60.0).round_ties_even() as i64,
            if !lieu.is_empty() {
                format!(
                    ", serveur {}situé à {lieu}",
                    if lag::diag::connexion_seule(&rec) { "de connexion " } else { "" }
                )
            } else {
                String::new()
            }
        ));
        self.sessions(ctx).push(rec);
        self.sauver_sessions(ctx);
        if quick {
            self.ouvrir_panneau(Detail::Lag(id), effets);
        } else {
            effets.ajouter(Effet::Signal {
                nom: signaux::NOTIFICATION.into(),
                donnees: json!({ "Titre": format!("Connexion pendant {jeu}"), "Texte": format!("{titre}. Clique pour voir le détail."), "Action": format!("lag:{id}"), "Valeur": null, "Ecran": "jeux" }),
            });
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // Annuaires (rdap.org, ipinfo.io), seulement si permis
    // -----------------------------------------------------------------------------------------------------------

    fn chercher_proprietaires(&mut self, ctx: &Contexte, ips: Vec<String>) {
        if !Self::annuaire_permis(ctx) {
            return;
        }
        let ips: Vec<String> = ips.into_iter().filter(|ip| self.annuaire.proprietaire(ip).is_none()).collect();
        if ips.is_empty() {
            return;
        }
        let sys = self.sys();
        let (tx, rx) = channel();
        let _ = std::thread::Builder::new().name("nevermind-annuaire".into()).spawn(move || {
            let mut r = Vec::new();
            for ip in ips {
                let ptr = sys.nom_inverse(&ip);
                let mut rep =
                    match sys.http(&format!("https://rdap.org/ip/{ip}"), "application/rdap+json, application/json") {
                        Ok(t) => match serde_json::from_str::<Value>(&t) {
                            Ok(o) => serveurs::lire_rdap(&ip, &o),
                            Err(_) => Reponse { ip: ip.clone(), ..Default::default() },
                        },
                        Err(_) => Reponse { ip: ip.clone(), ..Default::default() },
                    };
                rep.ptr = ptr;
                r.push(rep);
                std::thread::sleep(Duration::from_millis(400));
            }
            let _ = tx.send(r);
        });
        self.travaux.annuaire.push(rx);
    }

    fn recevoir_annuaire(&mut self, ctx: &Contexte) {
        let mut change = false;
        let mut garde = Vec::new();
        for rx in std::mem::take(&mut self.travaux.annuaire) {
            match rx.try_recv() {
                Ok(res) => {
                    self.travaux.annuaire_en_cours = false;
                    for r in res {
                        if !r.ptr.is_empty()
                            && let Some(rel) = self.releve.as_mut()
                            && !rel.dns.get(&r.ip).is_some_and(|x| !x.is_empty())
                        {
                            rel.dns.insert(r.ip.clone(), r.ptr.clone());
                        }
                        if r.ok && (!r.o.is_empty() || !r.n.is_empty()) && !r.s.is_empty() && !r.e.is_empty() {
                            self.annuaire.ajouter(Entree {
                                s: r.s,
                                e: r.e,
                                o: r.o,
                                c: r.c,
                                n: r.n,
                                d: chrono::Local::now().format("%Y-%m-%d").to_string(),
                            });
                            self.annuaire.manques.clear();
                            change = true;
                        } else if !r.ok {
                            // (annuaire injoignable ou trop sollicité : un seul nouvel essai)
                            let e = self.travaux.annuaire_essais.entry(r.ip.clone()).or_insert(0);
                            *e += 1;
                            if *e < 2 {
                                self.travaux.annuaire_fait.remove(&r.ip);
                            }
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => garde.push(rx),
                Err(_) => self.travaux.annuaire_en_cours = false,
            }
        }
        self.travaux.annuaire = garde;
        if change {
            self.sauver_annuaire(ctx);
        }
    }

    fn chercher_lieu(&mut self, ctx: &Contexte, ip: &str) {
        if self.lieux.contains_key(ip) || !Self::pays_permis(ctx) {
            return;
        }
        let sys = self.sys();
        let ip = ip.to_string();
        let (tx, rx) = channel();
        let _ = std::thread::Builder::new().name("nevermind-pays".into()).spawn(move || {
            let r = sys
                .http(&format!("https://ipinfo.io/{ip}/json"), "application/json")
                .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()))
                .map(|o| serveurs::lire_ipinfo(&o));
            let _ = tx.send((ip, r));
        });
        self.travaux.lieux.push(rx);
    }

    fn recevoir_lieux(&mut self, ctx: &Contexte) {
        let mut garde = Vec::new();
        for rx in std::mem::take(&mut self.travaux.lieux) {
            match rx.try_recv() {
                Ok((ip, Ok(l))) => {
                    if let Some(s) = self.lag.as_mut()
                        && let Some(srv) = s.serveur.as_mut()
                        && srv.ip == ip
                    {
                        srv.pays = l.pays.clone();
                        srv.ville = l.ville.clone();
                        if !l.pays.is_empty() {
                            let lieu = lag::diag::lieu_serveur(&json!({ "Pays": l.pays, "Ville": l.ville }));
                            ctx.journal(&format!("Lag: serveur {ip} situé à {lieu}"));
                        }
                    }
                    self.lieux.insert(ip, l);
                }
                Ok((_, Err(e))) => ctx.journal(&format!("Lag: pays du serveur inconnu ({e})")),
                Err(std::sync::mpsc::TryRecvError::Empty) => garde.push(rx),
                Err(_) => {}
            }
        }
        self.travaux.lieux = garde;
    }

    // -----------------------------------------------------------------------------------------------------------
    // Trafic
    // -----------------------------------------------------------------------------------------------------------

    /// `Start-TrafficWatch`
    fn demarrer_surveillance(&mut self, ctx: &Contexte, effets: &mut Effets) {
        if self.releve.is_none() {
            self.releve = Some(Releve::nouveau(std::process::id(), self.maintenant()));
        }
        self.surveillance = true;
        self.charger_annuaire(ctx);
        self.passer_trafic(ctx, effets);
        effets.ajouter(Effet::Annonce("Surveillance de ce qui sort du PC activée.".into()));
    }

    fn arreter_surveillance(&mut self, effets: &mut Effets) {
        self.surveillance = false;
        self.passage_trafic = None;
        effets.ajouter(Effet::Annonce("Surveillance arrêtée (les chiffres affichés sont gardés).".into()));
    }

    /// Les connexions TCP : par l'Aide (elle seule peut compter les octets), sinon lues ici (sans les octets)
    fn lignes_tcp(&mut self, ctx: &Contexte) -> Vec<String> {
        let maintenant = self.maintenant();
        if self.aide_tcp_ko.is_none_or(|t| maintenant - t > 60.0) {
            // (une lecture de la minuterie : jamais de demande de Windows, comme Get-TrafficTcp ; audit G5)
            match ctx.aide_de_fond("trafic.tcp", &json!({}), 15000) {
                Ok(r) => {
                    self.aide_tcp_ko = None;
                    self.compteurs_ok = valeur::vrai(valeur::champ(&r, "CompteursOk"));
                    return valeur::liste(valeur::champ(&r, "Lignes"))
                        .iter()
                        .filter(|x| valeur::vrai(x))
                        .map(valeur::texte)
                        .collect();
                }
                Err(_) => self.aide_tcp_ko = Some(maintenant),
            }
        }
        let (l, ok) = self.sys().tcp();
        self.compteurs_ok = ok;
        l
    }

    /// `Update-Traffic`
    fn passer_trafic(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let maintenant = self.maintenant();
        self.passage_trafic = Some(maintenant);
        let sys = self.sys();
        self.recevoir_signatures();
        self.recevoir_annuaire(ctx);
        // Cache DNS (toutes les 10 s, en arrière-plan ; aucune demande sur le réseau)
        if let Some(rx) = &self.travaux.dns
            && let Ok(l) = rx.try_recv()
        {
            self.travaux.dns = None;
            if let Some(r) = self.releve.as_mut() {
                for (ip, nom) in l {
                    r.dns.insert(ip, nom);
                }
            }
        }
        if self.travaux.dns.is_none() && self.travaux.dns_a.is_none_or(|t| maintenant - t >= 10.0) {
            self.travaux.dns_a = Some(maintenant);
            let (tx, rx) = channel();
            let s = sys.clone();
            let _ = std::thread::Builder::new().name("nevermind-dns".into()).spawn(move || {
                let _ = tx.send(s.cache_dns());
            });
            self.travaux.dns = Some(rx);
        }
        let lignes = self.lignes_tcp(ctx);
        self.lectures += 1;
        let Some(mut r) = self.releve.take() else { return };
        r.passages += 1;
        let procs = Procs { sys: sys.as_ref(), services: std::cell::RefCell::new(None) };
        // Processus fermés : Windows réutilise leurs numéros
        if r.passages % 15 == 0 {
            let vivants: HashSet<u32> = sys.processus().into_iter().map(|(p, _, _)| p).collect();
            r.oublier_morts(&vivants);
        }
        // Emplacements cachés (services, programmes en administrateur) : l'Aide les lit pour les nouveaux processus
        if self.aide_tcp_ko.is_none() {
            let mut v: Vec<u32> = lignes
                .iter()
                .filter_map(|l| l.split('|').next()?.trim().parse::<u32>().ok())
                .filter(|p| *p > 4 && !r.connu(*p) && !r.chemins_aide.contains_key(p))
                .collect();
            v.sort_unstable();
            v.dedup();
            if !v.is_empty() {
                for p in &v {
                    r.chemins_aide.insert(*p, String::new());
                }
                let pids: Vec<String> = v.iter().map(|p| p.to_string()).collect();
                if let Ok(x) = ctx.aide_de_fond("lecture.chemins", &json!({ "Pids": pids }), 15000)
                    && let Some(o) = x.as_object()
                {
                    for (k, c) in o {
                        r.chemins_aide.insert(k.parse().unwrap_or(0), valeur::texte(c));
                    }
                }
            }
        }
        r.passage(&lignes, &procs, maintenant);
        if r.passages % 30 == 0 {
            r.compresser(maintenant, 500, 30.0);
        }
        if r.passages % 5 == 1 {
            r.udp(&sys.udp(), &procs);
        }
        self.releve = Some(r);
        self.lancer_signatures();
        // Serveurs sans nom : à qui ils appartiennent (5 à la fois, si permis)
        if Self::annuaire_permis(ctx) && !self.travaux.annuaire_en_cours {
            let mut ips: Vec<String> = Vec::new();
            if let Some(r) = self.releve.as_ref() {
                'a: for a in r.apps.values() {
                    for d in a.dest.values() {
                        if d.privee
                            || r.dns.get(&d.distante).is_some_and(|x| !x.is_empty())
                            || self.travaux.annuaire_fait.contains(&d.distante)
                            || ips.contains(&d.distante)
                        {
                            continue;
                        }
                        if self.annuaire.proprietaire(&d.distante).is_some() {
                            continue;
                        }
                        ips.push(d.distante.clone());
                        if ips.len() >= 5 {
                            break 'a;
                        }
                    }
                }
            }
            if !ips.is_empty() {
                for ip in &ips {
                    self.travaux.annuaire_fait.insert(ip.clone());
                }
                self.travaux.annuaire_en_cours = true;
                self.chercher_proprietaires(ctx, ips);
            }
        }
        self.verifier_alertes(ctx, effets);
    }

    fn lancer_signatures(&mut self) {
        if self.travaux.signatures.is_some() {
            return;
        }
        let sys = self.sys();
        let Some(r) = self.releve.as_mut() else { return };
        let mut items = Vec::new();
        while items.len() < 6 {
            let Some(k) = r.a_verifier.pop_front() else { break };
            if let Some(a) = r.apps.get(&k)
                && !a.chemin.is_empty()
            {
                items.push((
                    a.cle.clone(),
                    a.chemin.clone(),
                    alertes::LOLBINS.is_match(&a.nom),
                    a.pids.iter().copied().collect::<Vec<u32>>(),
                ));
            }
        }
        if items.is_empty() {
            return;
        }
        let (tx, rx) = channel();
        let _ = std::thread::Builder::new().name("nevermind-signatures".into()).spawn(move || {
            let procs = if items.iter().any(|x| x.2) { sys.processus() } else { Vec::new() };
            let mut res = Vec::new();
            for (cle, chemin, lol, pids) in items {
                let (sig, editeur) = sys.signature(&chemin);
                let (mut parent, mut parent_sig, mut parent_pub) = (String::new(), String::new(), String::new());
                if lol {
                    for p in pids {
                        let Some(pp) = procs.iter().find(|x| x.0 == p).map(|x| x.1) else { continue };
                        let Some(par) = procs.iter().find(|x| x.0 == pp) else { continue };
                        let pc = sys.chemin(par.0);
                        if pc.is_empty() {
                            continue;
                        }
                        let (s, e) = sys.signature(&pc);
                        parent = format!("{}.exe", par.2);
                        parent_sig = s;
                        parent_pub = e;
                        if parent_sig != "Valid" {
                            break;
                        }
                    }
                }
                let desc = sys.description(&chemin);
                res.push(ResultatSig { cle, sig, editeur, parent, parent_sig, parent_pub, desc });
            }
            let _ = tx.send(res);
        });
        self.travaux.signatures = Some(rx);
    }

    fn recevoir_signatures(&mut self) {
        let Some(rx) = &self.travaux.signatures else { return };
        let Ok(res) = rx.try_recv() else { return };
        self.travaux.signatures = None;
        let Some(r) = self.releve.as_mut() else { return };
        for x in res {
            let Some(a) = r.apps.get_mut(&x.cle) else { continue };
            // Vérification ratée (fichier en cours de mise à jour...) : on réessaie, ce n'est pas « non signé »
            if x.sig == "Error" || x.sig == "UnknownError" {
                a.sig_essais += 1;
                if a.sig_essais < 3 {
                    r.a_verifier.push_back(x.cle.clone());
                } else {
                    a.sig = Some("Unknown".into());
                }
                continue;
            }
            if !x.desc.is_empty() && x.desc.chars().count() < 90 && a.titre == a.nom && a.services.is_empty() && !a.moi
            {
                a.titre = x.desc;
            }
            a.sig = Some(x.sig.clone());
            a.editeur = x.editeur;
            a.parent = x.parent;
            a.parent_sig = x.parent_sig;
            a.parent_pub = x.parent_pub;
            r.signatures.insert(x.cle, x.sig);
        }
    }

    /// `Test-TrafficAlerts` : prévient (une fois par programme) quand une alerte sérieuse apparaît
    fn verifier_alertes(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let confiance = alertes::marques(ctx.reglage("TrafficTrusted"));
        let analyses = alertes::marques(ctx.reglage("TrafficScanned"));
        let Some(r) = self.releve.as_mut() else { return };
        let l = alertes::alertes(r.apps.values(), &confiance, &analyses, &self.jeux_exes);
        for al in &l {
            if al.niveau == "info" || r.prevenus.contains(&al.cle) {
                continue;
            }
            r.prevenus.insert(al.cle.clone());
            let Some(a) = r.apps.get(&al.cle) else { continue };
            ctx.journal(&format!("Trafic anormal: {} ({}) : {}", a.nom, a.chemin, al.pourquoi.join(" ; ")));
            effets.ajouter(Effet::Signal {
                nom: signaux::NOTIFICATION.into(),
                donnees: json!({ "Titre": format!("Activité réseau à vérifier : {}", a.titre), "Texte": format!("{}. Clique pour voir.", al.pourquoi.first().cloned().unwrap_or_default()), "Action": "trafic-voir", "Valeur": al.cle, "Ecran": "trafic" }),
            });
        }
        self.alertes = l;
    }

    fn rafraichir_alertes(&mut self, ctx: &Contexte, effets: &mut Effets) {
        self.verifier_alertes(ctx, effets);
        effets.ajouter(Effet::Rafraichir);
    }

    /// `Set-TrafficMark`
    fn marquer(ctx: &Contexte, nom: &str, cle: &str, oui: bool) {
        let mut m = alertes::marques(ctx.reglage(nom));
        m.retain(|(k, _)| k.to_lowercase() != cle.to_lowercase());
        if oui {
            m.push((cle.to_string(), chrono::Local::now().format("%d/%m/%Y").to_string()));
        }
        let v: Vec<Value> = m.into_iter().map(|(k, d)| json!(format!("{k}|{d}"))).collect();
        let _ = ctx.ecrire_reglage(nom, Value::Array(v), false);
    }

    fn app(&self, cle: &str) -> Option<&trafic::etat::App> {
        self.releve.as_ref().and_then(|r| r.apps.get(cle))
    }

    fn question(&mut self, texte: &str, attente: Attente, effets: &mut Effets) {
        self.attente = Some(attente);
        effets.ajouter(Effet::Signal {
            nom: signaux::QUESTION.into(),
            donnees: json!({ "id": id_aleatoire(), "titre": "Nevermind", "texte": texte, "details": [], "boutons": [{ "id": "oui", "texte": "Oui" }, { "id": "non", "texte": "Non", "style": "second" }], "action": ACTION_QUESTION }),
        });
    }

    /// Une action d'un programme (`Get-TrafficAlertActions`) : 0 confiance, 1 analyse, 2 blocage, 3 emplacement
    fn action_app(&mut self, cle: &str, i: usize, effets: &mut Effets) {
        let Some(a) = self.app(cle) else { return };
        let (titre, chemin) = (a.titre.clone(), a.chemin.clone());
        match i {
            0 => {
                // (« Hide-TestPanel » d'abord : le panneau se ferme)
                self.fermer_panneau(effets);
                self.question(
                    &format!("Faire confiance à « {titre} » ?\n\nIl ne sera plus signalé. Tu pourras revenir sur ce choix depuis sa fiche."),
                    Attente::Confiance(cle.into()),
                    effets,
                );
            }
            1 => effets.ajouter(Effet::Signal {
                nom: signaux::ANALYSE.into(),
                donnees: json!({ "Sorte": "CustomScan", "Chemins": [chemin], "Titre": format!("Analyse de {titre}"), "Marque": { "Reglage": "TrafficScanned", "Cle": cle } }),
            }),
            2 => {
                if chemin.is_empty() {
                    effets.ajouter(Effet::Annonce("Impossible de bloquer ce programme : son emplacement est inconnu.".into()));
                    return;
                }
                self.question(
                    &format!("Bloquer l'accès à Internet pour « {titre} » ?\n\n{chemin}\n\nLe programme ne pourra plus rien envoyer ni recevoir. Tu pourras annuler depuis la page Sauvegarde (historique)."),
                    Attente::Bloquer(cle.into()),
                    effets,
                );
            }
            _ => {
                if !chemin.is_empty() {
                    let dossier = std::path::Path::new(&chemin).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                    effets.ajouter(Effet::OuvrirAdresse(dossier));
                }
            }
        }
    }

    /// `Block-TrafficApp` (après le « Oui ») : la règle du pare-feu par l'Aide, notée dans l'historique. L'Aide peut
    /// mettre du temps (sa tâche à réveiller, la demande de Windows) : dans un travail de fond (audit G3), un seul à
    /// la fois ; les messages arrivent à la fin, comme avant.
    fn bloquer(&mut self, ctx: &Contexte, cle: &str, effets: &mut Effets) {
        let Some(a) = self.app(cle) else { return };
        let (titre, chemin, services) = (a.titre.clone(), a.chemin.clone(), a.services.clone());
        if self.fond.occupe() {
            ctx.journal(&format!("Trafic : blocage de {titre} pas lancé, un geste est encore en cours"));
            // (dit en clair : le « Oui » n'est pas avalé sans un mot ; lot B)
            let en_cours = self.bloque_en_cours.clone().unwrap_or_default();
            effets.ajouter(Effet::Annonce(format!(
                "« {titre} » n'est pas bloqué : attends la fin du blocage de « {en_cours} », puis réessaie."
            )));
            return;
        }
        // (une règle du pare-feu : le verrou commun des gestes, sinon le clic est refusé ; lot A du second audit)
        let geste = match ctx.prendre_geste("Bloquer", "Trafic") {
            Ok(g) => g,
            Err(refus) => {
                effets.ajouter(Effet::Annonce(refus));
                return;
            }
        };
        self.bloque_en_cours = Some(titre.clone());
        self.fond.lancer_geste(ctx, "bloquer", geste, move |fond| {
            let ctx = fond.ctx;
            // (l'attente montrée tout de suite, retirée à la fin du travail ; lot B)
            fond.effet(nevermind_noyau::signaux::occupe(&format!("Blocage de « {titre} »...")));
            let mut effets = Vec::new();
            let nom = format!("Nevermind : bloque {titre}");
            let svc: Vec<String> = services.iter().map(|s| s.nom.clone()).collect();
            if let Err(e) =
                ctx.aide("pare-feu.bloquer", &json!({ "Nom": nom, "Programme": chemin, "Services": svc }), 15000)
            {
                effets.push(Effet::Annonce(e));
                return effets;
            }
            effets.push(Effet::Signal {
                nom: signaux::HISTORIQUE.into(),
                donnees: json!({ "titre": format!("Internet bloqué pour {titre}"), "details": [chemin], "log": [{ "Type": "fw", "Name": nom }] }),
            });
            let windir = std::env::var("windir").unwrap_or_else(|_| r"C:\Windows".into()).to_lowercase();
            let systeme = !services.is_empty() || chemin.to_lowercase().starts_with(&format!("{windir}\\"));
            if !systeme {
                // (la règle commune de fermeture est celle de l'Aide : jamais un service, Windows, la suite...)
                // (l'Aide vient de poser la règle du pare-feu : jamais une deuxième demande de Windows pour la fermeture)
                let _ = ctx.aide_sans_demande("processus.fermer", &json!({ "Chemin": chemin }), 15000);
            }
            effets.push(Effet::Annonce(format!(
                "« {titre} » est bloqué{}.\n\nPour annuler : page Sauvegarde, historique, « Annuler ».",
                if systeme { "" } else { " et a été fermé" }
            )));
            effets
        });
        self.relever_fond(effets);
    }

    /// Ce que les gestes longs ont rendu (sur le fil des blocs)
    fn relever_fond(&mut self, effets: &mut Effets) {
        for r in self.fond.relever() {
            match r {
                Rendu::Effet(e) => effets.ajouter(e),
                Rendu::Fini { resultat: Ok(l), .. } => l.into_iter().for_each(|e| effets.ajouter(e)),
                Rendu::Fini { resultat: Err(e), .. } => {
                    effets.ajouter(Effet::Annonce(format!("Oups, quelque chose s'est mal passé :\n\n{e}")))
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // Le panneau
    // -----------------------------------------------------------------------------------------------------------

    fn ouvrir_panneau(&mut self, d: Detail, effets: &mut Effets) {
        let de = match &self.panneau {
            Some(p) if self.ecran == "panneau" => p.de.clone(),
            _ => self.ecran.clone(),
        };
        self.panneau = Some(PanneauOuvert { detail: d, de, montre: false, boutons: Vec::new(), reglages: Vec::new() });
        effets.ajouter(Effet::Rafraichir);
        effets.ajouter(Effet::OuvrirEcran("panneau".into()));
    }

    fn fermer_panneau(&mut self, effets: &mut Effets) {
        if let Some(p) = self.panneau.take() {
            let de = if p.de.is_empty() || p.de == "panneau" || p.de == "fiche" { "accueil".to_string() } else { p.de };
            if self.ecran == "panneau" {
                effets.ajouter(Effet::OuvrirEcran(de));
            }
            effets.ajouter(Effet::Rafraichir);
        }
    }

    fn page_panneau(&mut self, ctx: &Contexte) -> Option<Value> {
        let d = self.panneau.as_ref()?.detail.clone();
        let de = self.panneau.as_ref().map(|p| p.de.clone()).unwrap_or_default();
        let sys = self.sys();
        let (pn, titre, sous, etat, texte_etat, icone, reglages) = match d {
            Detail::Lag(id) => {
                let s = self.sessions(ctx).iter().find(|s| valeur::texte(valeur::champ(s, "Id")) == id).cloned()?;
                let (p, t, so, e, te) = pages::panneau_lag(&s);
                (p, t, so, e, te, "ondes", Vec::new())
            }
            Detail::App(cle) => {
                let confiance = alertes::marques(ctx.reglage("TrafficTrusted"));
                let c = alertes::marque(&confiance, &cle).map(String::from);
                let r = self.releve.as_ref()?;
                let a = r.apps.get(&cle)?;
                let al = self.alertes.iter().find(|x| x.cle == cle);
                let (p, t, so, e, te) = pages::panneau_app(r, a, al, c.as_deref(), &mut self.annuaire);
                (p, t, so, e, te, "courbe", Vec::new())
            }
            Detail::Microsoft => {
                let (p, t, so, e, te, items) =
                    pages::panneau_microsoft(sys.as_ref(), self.releve.as_ref(), &mut self.annuaire);
                (p, t, so, e, te, "windows", items)
            }
        };
        let page = pn.page(&titre, &sous, icone, &etat, &texte_etat, Some(&de));
        if let Some(p) = self.panneau.as_mut() {
            p.boutons = pn.boutons;
            p.reglages = reglages;
        }
        Some(page)
    }

    fn bouton_panneau(&mut self, ctx: &Contexte, i: usize, effets: &mut Effets) {
        let Some(p) = self.panneau.as_ref() else { return };
        let Some(b) = p.boutons.get(i).cloned() else { return };
        let reglages = p.reglages.clone();
        match b {
            pages::BoutonPanneau::LimiterWindowsUpdate(titre) => {
                let cle = r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization";
                effets.ajouter(Effet::Signal {
                    nom: signaux::SAUVEGARDE.into(),
                    donnees: json!({ "Geste": "Limiter Windows Update", "Page": "Trafic", "Titre": format!("Limiter Windows Update ({titre})"), "Details": [],
                        "Ecritures": [
                            microsoft::Ecriture::Registre { chemin: cle.into(), nom: "DOPercentageMaxBackgroundBandwidth".into(), valeur: 10 }.json(),
                            microsoft::Ecriture::Registre { chemin: cle.into(), nom: "DODownloadMode".into(), valeur: 0 }.json(),
                        ],
                        "Message": "Windows Update est limité à 10 % de ta connexion en arrière-plan. Annulable (page Sauvegarde)." }),
                });
            }
            pages::BoutonPanneau::ActionApp(cle, n) => self.action_app(&cle, n, effets),
            pages::BoutonPanneau::PlusConfiance(cle) => {
                self.fermer_panneau(effets);
                Self::marquer(ctx, "TrafficTrusted", &cle, false);
                self.rafraichir_alertes(ctx, effets);
                let titre = self.app(&cle).map(|a| a.titre.clone()).unwrap_or_default();
                effets.ajouter(Effet::Annonce(format!("« {titre} » sera de nouveau surveillé.")));
            }
            pages::BoutonPanneau::ToutCouper(l) => self.couper(&reglages, &l, effets),
            pages::BoutonPanneau::Couper(k) => self.couper(&reglages, &[k], effets),
            pages::BoutonPanneau::OutilMicrosoft => {
                effets.ajouter(Effet::OuvrirAdresse("ms-settings:privacy-feedback".into()))
            }
        }
    }

    /// `Invoke-PrivacyOff` : les écritures passent par la sauvegarde (annulables), puis la page est refaite
    fn couper(&mut self, reglages: &[microsoft::Reglage], l: &[usize], effets: &mut Effets) {
        let items: Vec<&microsoft::Reglage> = l.iter().filter_map(|&k| reglages.get(k)).collect();
        if items.is_empty() {
            return;
        }
        let titre = if items.len() > 1 {
            "Envois à Microsoft réduits".to_string()
        } else {
            format!("Coupé : {}", items[0].titre)
        };
        let ecritures: Vec<Value> = items.iter().flat_map(|i| i.couper.iter().map(|e| e.json())).collect();
        effets.ajouter(Effet::Signal {
            nom: signaux::SAUVEGARDE.into(),
            donnees: json!({ "Geste": "Couper les envois", "Page": "Trafic", "Titre": titre, "Details": items.iter().map(|i| i.titre).collect::<Vec<_>>(), "Ecritures": ecritures, "Historique": [],
                "Message": format!("{} réglage(s) coupé(s). Annulable depuis la page Sauvegarde, historique.", items.len()) }),
        });
        effets.ajouter(Effet::Rafraichir);
    }

    /// Le délai avant le prochain passage de la surveillance du Trafic
    pub fn delai_trafic(&self) -> f64 {
        let detail_trafic = self.panneau.as_ref().is_some_and(|p| !matches!(p.detail, Detail::Lag(_)));
        if self.partie.is_some() {
            30.0
        } else if self.cache {
            10.0
        } else if self.ecran == "trafic" || (self.ecran == "panneau" && detail_trafic) {
            2.0
        } else {
            5.0
        }
    }
}

impl Bloc for BlocTrafic {
    fn nom(&self) -> &'static str {
        "trafic"
    }

    fn pages(&mut self, ctx: &Contexte, ecran: &str) -> Map<String, Value> {
        // Le panneau quitté autrement que par « Fermer » (onglet du haut, logo...) : fermé comme par Retour
        if let Some(p) = self.panneau.as_mut() {
            if ecran == "panneau" {
                p.montre = true;
            } else if p.montre {
                self.panneau = None;
            }
        }
        self.ecran = ecran.to_string();
        let mut m = Map::new();
        let en_cours = self.lag.is_some();
        let mesure = Self::mesure_lag(ctx);
        let onglet = pages::onglet_lag(self.sessions(ctx), mesure, en_cours);
        m.insert("jeux-lag".into(), onglet);
        self.kit_apps = self
            .releve
            .as_ref()
            .map(|r| pages::apps_montrees(r).iter().map(|a| a.cle.clone()).collect())
            .unwrap_or_default();
        self.kit_alertes = self.alertes.iter().filter(|a| a.niveau != "info").cloned().collect();
        let maintenant = self.maintenant();
        m.insert(
            "trafic".into(),
            pages::page_trafic(
                self.releve.as_ref(),
                self.surveillance,
                &self.alertes,
                Self::annuaire_permis(ctx),
                maintenant,
            ),
        );
        if let Some(p) = self.page_panneau(ctx) {
            m.insert("panneau".into(), p);
        }
        m
    }

    fn action(&mut self, ctx: &Contexte, id: &str, v: &Value, effets: &mut Effets) -> bool {
        match id {
            "lag-mesure" => {
                let on = valeur::vrai(v);
                let _ = ctx.ecrire_reglage("LagMeasure", json!(on), false);
                if !on && self.lag.as_ref().is_some_and(|s| s.secondes <= 0.0) {
                    self.arreter_lag(ctx, effets);
                }
                effets.ajouter(Effet::Rafraichir);
                return true;
            }
            "lag-test" => {
                if self.lag.is_none() {
                    self.demarrer_lag(ctx, "Test rapide", 0, 30.0, effets);
                }
                return true;
            }
            "lag-pays" => {
                let _ = ctx.ecrire_reglage("LagPays", json!(valeur::vrai(v)), false);
                effets.ajouter(Effet::Rafraichir);
                return true;
            }
            "trafic-annuaire" => {
                let _ = ctx.ecrire_reglage("TrafficLookup", json!(valeur::vrai(v)), false);
                effets.ajouter(Effet::Rafraichir);
                return true;
            }
            "trafic-microsoft" => {
                self.ouvrir_panneau(Detail::Microsoft, effets);
                return true;
            }
            "clic:BtnTraffic" => {
                if self.surveillance {
                    self.arreter_surveillance(effets);
                } else {
                    self.demarrer_surveillance(ctx, effets);
                }
                effets.ajouter(Effet::Rafraichir);
                return true;
            }
            "trafic-voir" => {
                let cle = valeur::texte(v);
                if self.app(&cle).is_some() {
                    self.ouvrir_panneau(Detail::App(cle), effets);
                }
                return true;
            }
            ACTION_QUESTION => {
                // ({ id, valeur } du signal commun « question » ; abandonnée : le dernier bouton, « Non »)
                let choix = if v.is_object() { valeur::champ(v, "valeur") } else { v };
                let oui = valeur::texte(choix) == "oui" || choix.as_bool() == Some(true);
                match self.attente.take() {
                    Some(Attente::Confiance(cle)) if oui => {
                        Self::marquer(ctx, "TrafficTrusted", &cle, true);
                        self.rafraichir_alertes(ctx, effets);
                        let titre = self.app(&cle).map(|a| a.titre.clone()).unwrap_or_default();
                        effets.ajouter(Effet::Annonce(format!("« {titre} » est approuvé.")));
                    }
                    Some(Attente::Bloquer(cle)) if oui => self.bloquer(ctx, &cle, effets),
                    _ => {}
                }
                return true;
            }
            "panneau:close" if self.panneau.is_some() => {
                self.fermer_panneau(effets);
                return true;
            }
            _ => {}
        }
        if let Some(n) = id.strip_prefix("panneau:b:")
            && self.panneau.is_some()
        {
            if let Ok(i) = n.parse::<usize>() {
                self.bouton_panneau(ctx, i, effets);
            }
            return true;
        }
        if let Some(x) = id.strip_prefix("lag:")
            && !x.is_empty()
            && x.len() <= 40
            && x.chars().all(|c| c.is_alphanumeric() || "_.:-".contains(c))
        {
            self.ouvrir_panneau(Detail::Lag(x.to_string()), effets);
            return true;
        }
        if let Some(k) = id.strip_prefix("trafic-app:").and_then(|x| x.parse::<usize>().ok()) {
            if let Some(cle) = self.kit_apps.get(k).cloned() {
                self.ouvrir_panneau(Detail::App(cle), effets);
            }
            return true;
        }
        if let Some(x) = id.strip_prefix("trafic-al:") {
            let mut it = x.split(':').map(|n| n.parse::<usize>().ok());
            if let (Some(Some(k)), Some(Some(j))) = (it.next(), it.next())
                && let Some(al) = self.kit_alertes.get(k).cloned()
            {
                self.action_app(&al.cle, j, effets);
            }
            return true;
        }
        false
    }

    fn travaux_finis(&mut self, _ctx: &Contexte, effets: &mut Effets) {
        self.relever_fond(effets);
    }

    fn tic(&mut self, ctx: &Contexte, cache: bool, effets: &mut Effets) -> Option<Duration> {
        self.relever_fond(effets);
        self.cache = cache;
        let maintenant = self.maintenant();
        let mut prochain = 60.0f64;
        if self.lag.is_some() {
            if self.passage_lag.is_none_or(|t| maintenant - t >= 1.95) {
                self.passage_lag = Some(maintenant);
                self.passer_lag(ctx, effets);
            }
            prochain = prochain.min(2.0);
        }
        if self.surveillance {
            let d = self.delai_trafic();
            if self.passage_trafic.is_none_or(|t| maintenant - t >= d - 0.05) {
                self.passer_trafic(ctx, effets);
                if !cache && (self.ecran == "trafic" || self.ecran == "panneau") {
                    effets.ajouter(Effet::Rafraichir);
                }
            }
            prochain = prochain.min(d);
        }
        Some(Duration::from_secs_f64(prochain))
    }

    fn signal(&mut self, ctx: &Contexte, nom: &str, d: &Value, effets: &mut Effets) {
        match nom {
            signaux::PARTIE_COMMENCEE => {
                let j = valeur::champ(d, "jeu");
                let jeu = valeur::texte(if j.is_null() { valeur::champ(d, "Jeu") } else { j });
                let p = valeur::champ(d, "pid");
                let pid = valeur::num(if p.is_null() { valeur::champ(d, "Pid") } else { p }) as u32;
                self.partie = Some((jeu.clone(), pid));
                if Self::mesure_lag(ctx) && self.lag.is_none() {
                    self.demarrer_lag(ctx, &jeu, pid, 0.0, effets);
                }
            }
            signaux::PARTIE_FINIE => {
                let p = valeur::champ(d, "pid");
                let pid = valeur::num(if p.is_null() { valeur::champ(d, "Pid") } else { p }) as u32;
                let fin = self.partie.as_ref().map(|p| p.1);
                self.partie = None;
                if self
                    .lag
                    .as_ref()
                    .is_some_and(|s| s.secondes <= 0.0 && (s.pid == pid || (pid == 0 && fin == Some(s.pid))))
                {
                    self.arreter_lag(ctx, effets);
                }
            }
            signaux::JEUX_INDEX => {
                self.jeux_exes =
                    valeur::liste(valeur::champ(d, "Exes")).iter().map(|x| valeur::texte(x).to_lowercase()).collect();
            }
            // (la recherche du bloc divers mène au panneau « Ce que Windows envoie à Microsoft », Open-SearchEntry)
            "trafic-microsoft" => self.ouvrir_panneau(Detail::Microsoft, effets),
            // (la fin de l'analyse demandée, bloc protection : `$script:LastScanResult` ; « clean » pose la marque,
            //  trafic.ps1:324)
            "analyse-finie" => {
                let m = valeur::champ(d, "Marque");
                let (reglage, cle) =
                    (valeur::texte(valeur::champ(m, "Reglage")), valeur::texte(valeur::champ(m, "Cle")));
                if valeur::texte(valeur::champ(d, "Resultat")) == "clean" && !reglage.is_empty() && !cle.is_empty() {
                    Self::marquer(ctx, &reglage, &cle, true);
                    effets.ajouter(Effet::Rafraichir);
                }
            }
            _ => {}
        }
    }

    fn fermer(&mut self, ctx: &Contexte) {
        let mut e = Effets::default();
        if self.lag.is_some() {
            self.arreter_lag(ctx, &mut e);
        }
        self.surveillance = false;
    }
}
