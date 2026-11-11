//! Le bloc Réseau : son état (`$script:NetList`, `$script:NetAudit`, `$script:Net`...), ses gestes et son travail
//! régulier. Les travaux longs (scan, audit, test du ping, fiche d'un appareil) tournent dans un fil à part et
//! rendent leur résultat par un canal, relu à chaque `tic` (comme Invoke-Async, sans figer la fenêtre).
//!
//! Signaux envoyés (D:\Projets\_plans\signaux-communs-nevermind-rust.md) : `question`, `occupe`, `historique`,
//! `sauvegarde-changee`, et deux propres au bloc : `bulle` {titre, texte, ecran} (Show-Notify : nouvel appareil,
//! carte bridée) et `test-debit` {} (le bouton « Mesurer mon débit » : le test « Connexion Internet » des Tests).
//! Signaux attendus : `partie-commencee`, `partie-finie`, `debit-mesure` {R:[..]} (dernier débit des Tests).

use crate::appareil::{Appareil, Fabricants};
use crate::audit::{self, Geste, Point};
use crate::fichiers::{ecrire_json, ecrire_texte, lire_json};
use crate::net::{Connexion, alerte_carte, bridage, format_vitesse};
use crate::pages::{self, DNS_CHOIX, EtatReseau, ResultatPing};
use crate::panneau::{self, BoutonPanneau, PingDirect, Services};
use crate::scan::{self, Autour, BrutApprofondi, BrutScan, Connus};
use crate::sonde::{Avancement, Sonde, SondeVide};
use nevermind_noyau::travaux::{Rendu, Travaux};
use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

/// Ce qu'un fil rend au bloc
enum Message {
    Scan(Option<Fabricants>, Result<BrutScan, String>),
    Approfondi(Result<BrutApprofondi, String>),
    Audit(Result<Value, String>),
    Ping(u64, Option<f64>),
    Ports(u64, Vec<i64>),
    Test(Option<Connexion>, Vec<ResultatPing>),
    CarteRevenue(Option<Connexion>),
    Surveillance(Connexion, Vec<String>, Result<BrutScan, String>),
    Connexion(Option<Connexion>),
}

/// Ce que rend un geste par l'Aide, fait dans un travail de fond (audit G3)
enum FinGeste {
    Audit { act: Box<crate::audit::ActionAudit>, titre: String, r: Result<Vec<Value>, String> },
    Dns { log: Value, serveurs: Vec<String>, r: Result<Option<Box<Connexion>>, String> },
    Carte { idx: i64, r: Result<(), String> },
}

/// La clé des gestes par l'Aide : un seul à la fois
const GESTE: &str = "geste";
/// La page, dans « Attends la fin de « Changer le DNS » (Réseau) »
const PAGE: &str = "Réseau";

/// Le panneau ouvert par le bloc
enum Panneau {
    Appareil { d: Box<Appareil>, ping: PingDirect, services: Services, jeton: u64, dernier: Instant, en_vol: bool },
    AuditEnCours,
    Audit,
    AuditEchec(String),
}

/// L'audit complet en mémoire (`$script:NetAudit`)
struct AuditComplet {
    points: Vec<Point>,
    score: i64,
    date: String,
    nombre: usize,
}

/// Un geste qui attend la réponse à sa question
enum Attente {
    Carte,
    ActionAudit(usize, usize),
}

pub struct BlocReseau {
    sonde: Arc<dyn Sonde>,
    tx: Sender<Message>,
    rx: Receiver<Message>,
    av: Arc<Avancement>,
    // Scan
    liste: Vec<Appareil>,
    premier: bool,
    connus: Connus,
    resume: String,
    conseil: String,
    scan: bool,
    approfondi: bool,
    scan_net: Option<(Connexion, Vec<String>)>,
    fab: Option<Fabricants>,
    filtre: String,
    audit_apres_scan: bool,
    /// L'écran d'où « Audit » a été cliqué avant le scan : l'audit ne s'ouvre à la fin du scan que si l'utilisateur y est
    /// encore (lot I)
    audit_ecran: String,
    // Audit
    audit_lu: bool,
    audit_sauve: Option<Value>,
    audit: Option<AuditComplet>,
    audit_en_cours: bool,
    // Connexion
    net: Option<Connexion>,
    net_lu: bool,
    pings: Vec<ResultatPing>,
    test_en_cours: bool,
    debit: Option<Vec<f64>>,
    carte_alertee: Option<String>,
    carte_vue: Instant,
    carte_en_vol: bool,
    en_partie: bool,
    // Surveillance
    surveillance: Option<bool>,
    prochaine_surveillance: Instant,
    vus: Connus,
    // Panneau et questions
    panneau: Option<Panneau>,
    boutons: Vec<BoutonPanneau>,
    jeton: u64,
    attente: Vec<(String, Attente)>,
    /// Les pastilles « en cours » montrées, chacune avec sa clé (smb1, test, carte) : la fin de l'une n'efface plus
    /// celle d'un geste qui tourne encore (lot I)
    occupes: Vec<(String, String)>,
    signal_connexion: Option<Value>,
    /// Les gestes par l'Aide (SMBv1 jusqu'à 15 min, DNS, carte, pare-feu...) : dans un fil à part (audit G3)
    travaux: Travaux<FinGeste>,
    /// Le nom du geste en cours (le dernier lancé), pour dire en clair pourquoi un autre clic est refusé (lot B)
    geste_nom: Option<String>,
    /// L'écran montré (le dernier demandé par l'hôte)
    ecran: String,
    /// Un geste de l'audit a fini pendant que l'utilisateur était ailleurs (ou en partie) : l'audit est refait quand il
    /// revient sur son panneau, jamais en le forçant à y aller (lot B)
    audit_a_refaire: bool,
}

impl Default for BlocReseau {
    fn default() -> Self {
        #[cfg(windows)]
        let s: Arc<dyn Sonde> = Arc::new(crate::windows::SondeWindows);
        #[cfg(not(windows))]
        let s: Arc<dyn Sonde> = Arc::new(SondeVide);
        Self::avec_sonde(s)
    }
}

const SANS_CONSEIL: &str = "Clique sur « Scanner le réseau » pour voir les appareils connectés.";

fn signal(effets: &mut Effets, nom: &str, donnees: Value) {
    effets.ajouter(Effet::Signal { nom: nom.into(), donnees });
}

impl BlocReseau {
    /// Le bloc avec une sonde choisie (les tests lui donnent une fausse sonde)
    pub fn avec_sonde(sonde: Arc<dyn Sonde>) -> Self {
        let (tx, rx) = channel();
        let _ = SondeVide;
        BlocReseau {
            sonde,
            tx,
            rx,
            av: Arc::new(Avancement::default()),
            liste: Vec::new(),
            premier: false,
            connus: Connus::default(),
            resume: String::new(),
            conseil: SANS_CONSEIL.into(),
            scan: false,
            approfondi: false,
            scan_net: None,
            fab: None,
            filtre: String::new(),
            audit_apres_scan: false,
            audit_ecran: String::new(),
            audit_lu: false,
            audit_sauve: None,
            audit: None,
            audit_en_cours: false,
            net: None,
            net_lu: false,
            pings: Vec::new(),
            test_en_cours: false,
            debit: None,
            carte_alertee: None,
            carte_vue: Instant::now(),
            carte_en_vol: false,
            en_partie: false,
            surveillance: None,
            prochaine_surveillance: Instant::now() + Duration::from_secs(600),
            vus: Connus::default(),
            panneau: None,
            boutons: Vec::new(),
            jeton: 0,
            attente: Vec::new(),
            occupes: Vec::new(),
            signal_connexion: None,
            travaux: Travaux::default(),
            geste_nom: None,
            ecran: String::new(),
            audit_a_refaire: false,
        }
    }

    fn date_jour(&self) -> String {
        let [a, m, j, _, _] = self.sonde.maintenant();
        format!("{a:04}-{m:02}-{j:02}")
    }
    fn date_audit(&self) -> String {
        let [a, m, j, h, mi] = self.sonde.maintenant();
        format!("{j:02}/{m:02}/{a:04} à {h:02}:{mi:02}")
    }
    fn horodatage(&self) -> String {
        let [a, m, j, h, mi] = self.sonde.maintenant();
        format!("{a:04}{m:02}{j:02}-{h:02}{mi:02}00")
    }
    fn fichier(ctx: &Contexte, nom: &str) -> PathBuf {
        ctx.donnees.join(nom)
    }
    fn lire(&self, ctx: &Contexte, nom: &str) -> Option<Value> {
        lire_json(&Self::fichier(ctx, nom), &self.horodatage(), &|m| ctx.journal(m))
    }
    fn ecrire(ctx: &Contexte, nom: &str, v: &Value) {
        if let Err(e) = ecrire_json(&Self::fichier(ctx, nom), v) {
            ctx.journal(&format!("{nom} non enregistré : {e}"));
        }
    }

    /// Une fois, à la première page : audit.json, la surveillance, la connexion
    fn preparer(&mut self, ctx: &Contexte) {
        if !self.audit_lu {
            self.audit_lu = true;
            self.audit_sauve = self.lire(ctx, "audit.json");
        }
        if self.surveillance.is_none() {
            self.surveillance = Some(ctx.reglage("NetWatch").as_ref().is_some_and(|v| crate::texte::vrai(Some(v))));
        }
        if !self.net_lu {
            self.net_lu = true;
            self.net = self.sonde.connexion();
        }
    }

    /// Le panneau de l'audit est-il à l'écran, hors partie ? (le seul moment où l'audit peut être refait de lui-même)
    fn audit_regarde(&self) -> bool {
        !self.en_partie && self.ecran == "panneau" && matches!(self.panneau, Some(Panneau::Audit))
    }

    /// Un clic refusé parce qu'un geste de la page tourne encore : dit en clair (lot B ; avant, seulement le journal)
    fn refus_occupe(&self, effets: &mut Effets) {
        let nom = self.geste_nom.as_deref().unwrap_or("un geste du réseau");
        effets.ajouter(Effet::Annonce(nevermind_noyau::gestes::texte_refus(nom, PAGE)));
    }

    /// La pastille « en cours » d'un travail (`cle`), texte vide : retirée ; envoyée seulement si elle change
    fn occuper(&mut self, effets: &mut Effets, cle: &str, texte: &str) {
        if self.occupes.iter().any(|(c, t)| c == cle && t == texte)
            || (texte.is_empty() && !self.occupes.iter().any(|(c, _)| c == cle))
        {
            return;
        }
        self.occupes.retain(|(c, _)| c != cle);
        if !texte.is_empty() {
            self.occupes.push((cle.to_string(), texte.to_string()));
        }
        signal(effets, "occupe", json!({ "texte": texte, "travail": cle }));
    }

    fn question(&mut self, effets: &mut Effets, id: &str, texte: &str, attente: Attente) {
        self.attente.retain(|(i, _)| i != id);
        self.attente.push((id.to_string(), attente));
        signal(
            effets,
            "question",
            json!({ "id": id, "titre": "", "texte": texte,
            "boutons": [{ "id": "oui", "texte": "Oui" }, { "id": "non", "texte": "Non", "style": "second" }], "action": "reseau-question" }),
        );
    }

    fn textes(&mut self) {
        if self.liste.is_empty() {
            return;
        }
        let (r, c) = scan::textes_liste(&self.liste, self.premier, "");
        self.resume = r;
        self.conseil = c;
    }

    // -----------------------------------------------------------------------
    // Le scan (Invoke-NetworkScan, Invoke-NetDeepScan)
    // -----------------------------------------------------------------------
    fn lancer_scan(&mut self, ctx: &Contexte, effets: &mut Effets) {
        if self.scan {
            return;
        }
        let Some(net) = self.sonde.connexion() else {
            effets.ajouter(Effet::Annonce("Aucune connexion réseau détectée.".into()));
            return;
        };
        let Some(ip) = net.ip.clone().filter(|i| !i.is_empty()) else {
            effets.ajouter(Effet::Annonce("Impossible de lire l'adresse de ce PC.".into()));
            return;
        };
        let ips = scan::sous_reseau(&ip, net.prefix);
        self.scan = true;
        self.approfondi = false;
        self.av = Arc::new(Avancement::default());
        self.av.phase("");
        self.scan_net = Some((net.clone(), ips.clone()));
        let (s, tx, av) = (self.sonde.clone(), self.tx.clone(), self.av.clone());
        let charger = self.fab.is_none();
        let chemin = Self::fichier(ctx, "fabricants.txt");
        std::thread::spawn(move || {
            let r = s.scan(&ips, net.if_index, &av);
            let f = if charger { s.fabricants(&chemin) } else { None };
            let _ = tx.send(Message::Scan(f, r));
        });
        effets.ajouter(Effet::Rafraichir);
    }

    fn fin_scan(&mut self, ctx: &Contexte, effets: &mut Effets, f: Option<Fabricants>, r: Result<BrutScan, String>) {
        if f.is_some() {
            self.fab = f;
        }
        let Some((net, ips)) = self.scan_net.clone() else { return };
        let brut = match r {
            Ok(b) => b,
            Err(e) => {
                self.scan = false;
                self.audit_apres_scan = false;
                effets.ajouter(Effet::Annonce(format!("Le scan n'a pas pu se faire : {e}")));
                effets.ajouter(Effet::Rafraichir);
                return;
            }
        };
        self.connus = Connus::depuis(self.lire(ctx, "appareils.json").as_ref());
        let locaux = self.sonde.locaux();
        let poste = self.sonde.poste();
        let jour = self.date_jour();
        let a = Autour { poste: &poste, aujourdhui: &jour, fab: self.fab.as_ref(), locaux: &locaux };
        let (l, premier) = scan::assembler(&net, &ips, &brut, &mut self.connus, &a);
        self.liste = l;
        self.premier = premier;
        Self::ecrire(ctx, "appareils.json", &self.connus.json());
        self.textes();
        // La recherche approfondie, tout de suite après
        self.approfondi = true;
        self.av.mettre(0.0);
        let trouves: Vec<String> = self.liste.iter().map(|d| d.ip.clone()).collect();
        let manquants = scan::manquants(&ips, &self.liste);
        let (s, tx, av) = (self.sonde.clone(), self.tx.clone(), self.av.clone());
        let ip = net.ip.clone().unwrap_or_default();
        let idx = net.if_index;
        std::thread::spawn(move || {
            let r = s.approfondi(&ip, idx, &manquants, &trouves, &av);
            let _ = tx.send(Message::Approfondi(r));
        });
        effets.ajouter(Effet::Rafraichir);
    }

    fn fin_approfondi(&mut self, ctx: &Contexte, effets: &mut Effets, r: Result<BrutApprofondi, String>) {
        self.scan = false;
        self.approfondi = false;
        let Some((net, ips)) = self.scan_net.clone() else { return };
        match r {
            Err(e) => ctx.journal(&format!("Recherche approfondie: {e}")),
            Ok(b) => {
                let locaux = self.sonde.locaux();
                let poste = self.sonde.poste();
                let jour = self.date_jour();
                let a = Autour { poste: &poste, aujourdhui: &jour, fab: self.fab.as_ref(), locaux: &locaux };
                let ajoutes = scan::fusionner(&mut self.liste, &b, Some(&net), &ips, &a);
                let _ = scan::apres_approfondi(&mut self.liste, &ajoutes, &mut self.connus, &jour);
                Self::ecrire(ctx, "appareils.json", &self.connus.json());
                self.textes();
            }
        }
        effets.ajouter(Effet::Rafraichir);
        if self.audit_apres_scan {
            self.audit_apres_scan = false;
            // (lot I : le scan a pu durer une minute ou plus ; l'audit ne s'ouvre que si l'utilisateur est encore sur la
            //  page d'où il l'a demandé (ou celles du réseau), hors partie. Sinon l'écran ne saute pas : « Audit » le relancera)
            let ici = self.ecran == self.audit_ecran || matches!(self.ecran.as_str(), "reseau" | "panneau");
            if !self.en_partie && ici {
                self.lancer_audit(ctx, effets);
            } else {
                self.audit_a_refaire = true;
                ctx.journal("Réseau : audit pas ouvert après le scan, l'utilisateur est parti ailleurs");
            }
        }
    }

    /// Le texte du scan en cours (`$script:Kit.ScanTexte`) et le nombre trouvé
    fn texte_scan(&self) -> (String, i64) {
        let p = self.av.phase_actuelle();
        let t = match p.as_str() {
            "ping" => "Recherche des appareils...",
            "arp" => "Recherche des appareils discrets...",
            "names" => "Récupération des noms...",
            "vendors" => "Identification des fabricants...",
            "deep-arp" => "Recherche des appareils qui ne répondent pas au ping...",
            "deep-listen" => "Écoute des appareils qui se présentent (TV, enceintes, imprimantes...)...",
            "deep-names" => "Recherche des noms Windows...",
            "deep-web" => "Lecture des pages de réglages...",
            _ if self.approfondi => "Recherche approfondie...",
            _ => "Préparation...",
        };
        (t.to_string(), if self.approfondi { self.liste.len() as i64 } else { self.av.trouves() })
    }

    // -----------------------------------------------------------------------
    // L'audit (Invoke-NetAudit)
    // -----------------------------------------------------------------------
    fn lancer_audit(&mut self, ctx: &Contexte, effets: &mut Effets) {
        if self.audit_en_cours {
            return;
        }
        if self.scan {
            effets.ajouter(Effet::Annonce("Attends la fin du scan du réseau, puis relance l'audit.".into()));
            return;
        }
        if self.liste.is_empty() {
            self.audit_apres_scan = true;
            self.audit_ecran = self.ecran.clone();
            self.lancer_scan(ctx, effets);
            if !self.scan {
                self.audit_apres_scan = false;
            }
            return;
        }
        let Some(net) = self.sonde.connexion().filter(|n| n.ip.as_deref().is_some_and(|i| !i.is_empty())) else {
            effets.ajouter(Effet::Annonce("Aucune connexion réseau détectée.".into()));
            return;
        };
        self.audit_a_refaire = false;
        self.audit_en_cours = true;
        self.av = Arc::new(Avancement::default());
        self.panneau = Some(Panneau::AuditEnCours);
        effets.ajouter(Effet::OuvrirEcran("panneau".into()));
        let ips: Vec<String> = self.liste.iter().filter(|d| !d.soi).map(|d| d.ip.clone()).collect();
        let (s, tx, av) = (self.sonde.clone(), self.tx.clone(), self.av.clone());
        self.scan_net = Some((net.clone(), Vec::new()));
        std::thread::spawn(move || {
            let r = s.audit(net.ip.as_deref().unwrap_or(""), &net.gateway, net.if_index, &ips, &audit::PORTS, &av);
            let _ = tx.send(Message::Audit(r));
        });
    }

    fn fin_audit(&mut self, ctx: &Contexte, effets: &mut Effets, r: Result<Value, String>) {
        self.audit_en_cours = false;
        let net = self.scan_net.as_ref().map(|x| x.0.clone()).unwrap_or_default();
        match r {
            Err(e) => {
                ctx.journal(&format!("Audit réseau: {e}"));
                self.panneau = Some(Panneau::AuditEchec(e));
            }
            Ok(r) => {
                let pts = audit::points(&r, &self.liste, &net.gateway, &json!(net.if_index));
                let (score, bad, warn) = audit::note(&pts);
                let date = self.date_audit();
                let nombre = self.liste.iter().filter(|d| !d.soi).count();
                let sauve = audit::sauvegarde(score, &date, bad, warn);
                Self::ecrire(ctx, "audit.json", &sauve);
                self.audit_sauve = Some(sauve);
                self.audit = Some(AuditComplet { points: pts, score, date, nombre });
                self.panneau = Some(Panneau::Audit);
            }
        }
        effets.ajouter(Effet::Rafraichir);
    }

    fn geste_audit(&mut self, ctx: &Contexte, effets: &mut Effets, k: usize, j: usize) {
        let Some(a) = self.audit.as_ref() else { return };
        let Some(p) = a.points.get(k) else { return };
        let Some(act) = p.actions.get(j).cloned() else { return };
        let titre = p.titre.clone();
        let r: Result<(), String> = match act.geste {
            Geste::Ouvrir => {
                effets.ajouter(Effet::OuvrirAdresse(crate::texte::chaine(Some(&act.arg))));
                Ok(())
            }
            Geste::ChangerDns => {
                self.panneau = None;
                effets.ajouter(Effet::OuvrirEcran("connexion".into()));
                Ok(())
            }
            Geste::Partages => {
                effets.ajouter(Effet::OuvrirAdresse(nevermind_noyau::programme_windows("fsmgmt.msc")));
                Ok(())
            }
            // (par l'Aide, jusqu'à 15 min pour SMBv1, ou PowerShell : dans un travail de fond, audit G3)
            Geste::WifiManuel | Geste::PareFeu | Geste::Smb1 | Geste::BureauDistance | Geste::Public => {
                return self.geste_audit_fond(ctx, effets, act, titre);
            }
        };
        self.fin_geste_audit(ctx, effets, &act, &titre, r.map(|_| Vec::new()));
    }

    /// Un geste de l'audit qui attend l'Aide (ou PowerShell) : dans un travail de fond ; sa fin dans
    /// `fin_geste_audit`. Un autre geste en cours : rien n'est lancé (un double clic ne refait rien).
    fn geste_audit_fond(&mut self, ctx: &Contexte, effets: &mut Effets, act: crate::audit::ActionAudit, titre: String) {
        if self.travaux.occupe() {
            ctx.journal(&format!("Réseau : {} pas lancé, un geste est encore en cours", act.label));
            self.refus_occupe(effets);
            return;
        }
        // (un geste qui change le PC : le verrou commun des gestes, sinon le clic est refusé ; lot A du second audit)
        let nom = match act.geste {
            Geste::WifiManuel => "Wi-Fi en connexion manuelle",
            Geste::PareFeu => "Réactiver le pare-feu",
            Geste::Smb1 => "Désactiver SMBv1",
            Geste::BureauDistance => "Couper le Bureau à distance",
            Geste::Public => "Réseau public",
            Geste::Ouvrir | Geste::ChangerDns | Geste::Partages => act.label.as_str(),
        };
        let geste = match ctx.prendre_geste(nom, PAGE) {
            Ok(g) => g,
            Err(refus) => {
                effets.ajouter(Effet::Annonce(refus));
                return;
            }
        };
        if act.geste == Geste::Smb1 {
            self.occuper(effets, "smb1", "Désactivation de SMBv1...");
        }
        let sonde = self.sonde.clone();
        self.geste_nom = Some(nom.to_string());
        let attente = (act.geste != Geste::Smb1).then(|| format!("« {nom} » en cours..."));
        self.travaux.lancer_geste(ctx, GESTE, geste, move |fond| {
            let ctx = fond.ctx;
            // (l'attente des autres gestes, retirée à la fin du travail ; SMBv1 a la sienne ; lot B)
            if let Some(t) = &attente {
                fond.effet(nevermind_noyau::signaux::occupe(t));
            }
            let mut log: Vec<Value> = Vec::new();
            // (Add-ChangementDefinitif : Type, Quoi, Retour ; le titre sert seulement hors d'une action. Jusqu'au
            //  08/11, « Detail » au lieu de « Quoi » : l'historique montrait « (définitif) » sans rien devant)
            let definitif = |log: &mut Vec<Value>, _titre: &str, quoi: String, retour: &str| {
                log.push(json!({ "Type": "definitif", "Quoi": quoi, "Retour": retour }));
            };
            let r: Result<(), String> = match act.geste {
                Geste::WifiManuel => {
                    let noms = crate::texte::liste(Some(&act.arg));
                    ctx.exiger_geste().and_then(|p| ctx.aide_qui_change(&p, "wifi.manuel", &json!({ "Profils": noms }), 30_000)).map(|_| {
                        definitif(&mut log, "Wi-Fi ouverts en connexion manuelle", format!("Connexion manuelle : {}", noms.join(", ")),
                            "Paramètres de Windows, Réseau et Internet, Wi-Fi, « Gérer les réseaux connus » : choisir le réseau et cocher « Se connecter automatiquement ».");
                    })
                }
                Geste::PareFeu => ctx.exiger_geste().and_then(|p| ctx.aide_qui_change(&p, "pare-feu.activer", &json!({}), 30_000)).map(|_| {
                    definitif(&mut log, "Pare-feu de Windows réactivé", "Pare-feu réactivé sur tous les réseaux".into(),
                        "Sécurité Windows, « Pare-feu et protection du réseau » : choisir le réseau et désactiver le pare-feu.");
                }),
                Geste::Smb1 => {
                    ctx.exiger_geste().and_then(|p| ctx.aide_qui_change(&p, "smb1.couper", &json!({ "Client": crate::texte::vrai(Some(&act.arg)) }), 900_000)).map(|_| {
                        definitif(&mut log, "SMBv1 désactivé", "Ancien partage de fichiers SMBv1 désactivé".into(),
                            "Panneau de configuration, Programmes, « Activer ou désactiver des fonctionnalités Windows » : cocher « Support de partage de fichiers SMB 1.0/CIFS ».");
                    })
                }
                Geste::BureauDistance => {
                    // (Set-Reg, audit-reseau.ps1:302 : la valeur d'avant notée dans la sauvegarde AVANT le changement,
                    //  pour « Tout restaurer » et la désinstallation ; M9, audit du 08/11 : elle était écrite à la main,
                    //  « 0 » en dur, sans passer par la sauvegarde)
                    let cle = r"HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server";
                    let st = ctx.registre_etat(cle, "fDenyTSConnections");
                    log.push(json!({ "Type": "reg", "Path": cle, "Name": "fDenyTSConnections", "Existed": st["Existed"], "Value": st["Value"], "Kind": st["Kind"], "Nouveau": 1, "NouveauType": "DWord" }));
                    ctx.exiger_geste().and_then(|p| ctx.changer_registre(&p, cle, "fDenyTSConnections", &json!(1), "DWord"))
                }
                Geste::Public => sonde.passer_public(crate::texte::reel(Some(&act.arg)) as i64),
                Geste::Ouvrir | Geste::ChangerDns | Geste::Partages => Ok(()),
            };
            FinGeste::Audit { act: Box::new(act), titre, r: r.map(|_| log) }
        });
        self.relever_travaux(ctx, effets);
    }

    /// La fin d'un geste de l'audit (sur le fil des blocs)
    fn fin_geste_audit(
        &mut self,
        ctx: &Contexte,
        effets: &mut Effets,
        act: &crate::audit::ActionAudit,
        titre: &str,
        r: Result<Vec<Value>, String>,
    ) {
        match act.geste {
            Geste::Smb1 => self.occuper(effets, "smb1", ""),
            Geste::BureauDistance => signal(effets, "sauvegarde-changee", json!({})),
            _ => {}
        }
        let log = match r {
            Ok(l) => l,
            Err(e) => {
                effets.ajouter(Effet::Annonce(format!("Oups, quelque chose s'est mal passé :\n\n{e}")));
                return;
            }
        };
        signal(effets, "historique", json!({ "titre": format!("{} ({titre})", act.label), "details": [], "log": log }));
        // (l'audit refait seulement si son panneau est encore à l'écran, et jamais pendant une partie : avant, la fin de
        //  SMBv1, jusqu'à 15 min plus tard, ramenait l'utilisateur sur le panneau de l'audit où qu'il soit ; lot B)
        if act.apres {
            if self.audit_regarde() {
                self.lancer_audit(ctx, effets);
            } else {
                self.audit_a_refaire = true;
            }
        }
    }

    fn exporter_audit(&self, effets: &mut Effets) {
        let Some(a) = &self.audit else { return };
        let poste = self.sonde.poste();
        let html = audit::rapport_html(&a.points, a.score, &a.date, a.nombre, &poste);
        let bureau = std::env::var_os("USERPROFILE").map(|p| Path::new(&p).join("Desktop")).unwrap_or_default();
        let chemin = bureau.join(format!("Audit réseau Nevermind {}.html", self.date_jour()));
        match ecrire_texte(&chemin, &format!("\u{feff}{html}")) {
            Ok(()) => {
                effets.ajouter(Effet::OuvrirAdresse(chemin.display().to_string()));
                effets.ajouter(Effet::Annonce("Rapport d'audit exporté.".into()));
            }
            Err(e) => effets.ajouter(Effet::Annonce(format!("Le rapport n'a pas pu être enregistré : {e}"))),
        }
    }

    // -----------------------------------------------------------------------
    // La fiche d'un appareil (Show-DeviceDetail)
    // -----------------------------------------------------------------------
    fn ouvrir_appareil(&mut self, effets: &mut Effets, k: usize) {
        if self.audit_en_cours {
            return;
        }
        let Some(d) = self.liste.get(k).cloned() else { return };
        self.jeton += 1;
        let jeton = self.jeton;
        let (s, tx, ip) = (self.sonde.clone(), self.tx.clone(), d.ip.clone());
        std::thread::spawn(move || {
            let ports: Vec<i64> = panneau::PORTS.iter().map(|p| p.0).collect();
            let mut r = s.ports(&ip, &ports, 600);
            r.sort();
            let _ = tx.send(Message::Ports(jeton, r));
        });
        self.panneau = Some(Panneau::Appareil {
            d: Box::new(d),
            ping: PingDirect::default(),
            services: Services::Recherche,
            jeton,
            dernier: Instant::now() - Duration::from_secs(1),
            en_vol: false,
        });
        effets.ajouter(Effet::OuvrirEcran("panneau".into()));
    }

    fn fermer_panneau(&mut self, effets: &mut Effets) {
        if self.audit_en_cours {
            return;
        }
        self.panneau = None;
        self.boutons.clear();
        effets.ajouter(Effet::OuvrirEcran("reseau".into()));
        effets.ajouter(Effet::Rafraichir);
    }

    fn page_panneau(&mut self) -> Option<Value> {
        let mut boutons = Vec::new();
        let p = match self.panneau.as_ref()? {
            Panneau::Appareil { d, ping, services, .. } => {
                let premiere = self.connus.date(&d.mac).map(str::to_string);
                panneau::fiche_appareil(d, premiere.as_deref(), ping, services, &mut boutons)
            }
            Panneau::AuditEnCours => {
                panneau::audit_en_cours(self.liste.iter().filter(|d| !d.soi).count(), self.av.progression())
            }
            Panneau::AuditEchec(e) => panneau::audit_echec(e),
            Panneau::Audit => {
                let a = self.audit.as_ref()?;
                panneau::audit_resultat(&a.points, a.score, &a.date, a.nombre, &mut boutons)
            }
        };
        self.boutons = boutons;
        Some(p)
    }

    // -----------------------------------------------------------------------
    // Connexion (Invoke-NetTest, Set-Dns, Restart-CarteReseau)
    // -----------------------------------------------------------------------
    fn tester_connexion(&mut self, effets: &mut Effets) {
        if self.test_en_cours {
            return;
        }
        self.test_en_cours = true;
        self.av = Arc::new(Avancement::default());
        let (s, tx, av) = (self.sonde.clone(), self.tx.clone(), self.av.clone());
        std::thread::spawn(move || {
            let net = s.connexion();
            let mut res = Vec::new();
            if let Some(n) = &net {
                for (lbl, hote, local) in pages::cibles(n) {
                    let mut t = Vec::new();
                    for i in 1..=20 {
                        av.phase(&format!("Test de {lbl}... ({i} sur 20)"));
                        t.push(s.ping(&hote, 1000));
                        std::thread::sleep(Duration::from_millis(80));
                    }
                    res.push(pages::verdict(&lbl, &hote, local, pages::mesure(&t)));
                }
            }
            let _ = tx.send(Message::Test(net, res));
        });
        self.occuper(effets, "test", "Test de la connexion...");
    }

    fn regler_dns(&mut self, ctx: &Contexte, effets: &mut Effets, k: usize) {
        let Some(net) = self.net.clone() else {
            effets.ajouter(Effet::Annonce("Aucune connexion détectée.".into()));
            return;
        };
        // (par l'Aide : dans un travail de fond, audit G3 ; un autre geste en cours : rien n'est lancé)
        if self.travaux.occupe() {
            ctx.journal("Réseau : DNS pas changé, un geste est encore en cours");
            self.refus_occupe(effets);
            return;
        }
        let geste = match ctx.prendre_geste("Changer le DNS", PAGE) {
            Ok(g) => g,
            Err(refus) => {
                effets.ajouter(Effet::Annonce(refus));
                return;
            }
        };
        let avant = self.sonde.dns_fixes(&net.guid);
        // (Set-Dns, onglets.ps1:655-661 : le DNS d'avant Nevermind noté dans la sauvegarde AVANT le changement, une
        //  fois, sous l'identifiant de la carte ; « Tout restaurer » et la désinstallation le remettent. M9, audit du
        //  08/11 : seul l'historique le notait, et le DNS n'était jamais remis.)
        let cle = if net.guid.is_empty() { net.if_index.to_string() } else { net.guid.clone() };
        let sv = ctx.sauvegarde();
        if !sv.dns.contains_key(&cle) && !sv.dns.contains_key(&net.if_index.to_string()) {
            ctx.changer_sauvegarde(|s| {
                s.dns.insert(cle.clone(), json!(avant));
            });
            if let Err(e) = ctx.exiger_geste().and_then(|p| {
                ctx.enregistrer_sauvegarde_ou_oublier(&p, move |s| {
                    s.dns.remove(&cle);
                })
            }) {
                effets.ajouter(Effet::Annonce(format!("Oups, quelque chose s'est mal passé :\n\n{e}")));
                return;
            }
        }
        let serveurs: Vec<String> = DNS_CHOIX[k].iter().map(|s| s.to_string()).collect();
        let log = json!([{ "Type": "dns", "IfIndex": net.if_index, "Guid": net.guid,
            "Servers": avant.split([',', ' ', '\t']).filter(|x| !x.is_empty()).collect::<Vec<_>>(), "Nouveau": serveurs }]);
        let sonde = self.sonde.clone();
        self.geste_nom = Some("Changer le DNS".into());
        self.travaux.lancer_geste(ctx, GESTE, geste, move |fond| {
            // (l'attente, retirée à la fin du travail ; lot B)
            fond.effet(nevermind_noyau::signaux::occupe("Changement du DNS..."));
            let r = fond
                .ctx
                .exiger_geste()
                .and_then(|p| {
                    fond.ctx.aide_qui_change(
                        &p,
                        "dns.regler",
                        &json!({ "Index": net.if_index, "Serveurs": serveurs }),
                        30_000,
                    )
                })
                .map(|_| sonde.connexion().map(Box::new));
            FinGeste::Dns { log, serveurs, r }
        });
        self.relever_travaux(ctx, effets);
    }

    fn fin_dns(
        &mut self,
        effets: &mut Effets,
        log: Value,
        serveurs: Vec<String>,
        r: Result<Option<Box<Connexion>>, String>,
    ) {
        match r {
            Err(e) => {
                effets.ajouter(Effet::Annonce(format!("Oups, quelque chose s'est mal passé :\n\n{e}")));
                return;
            }
            Ok(n) => self.net = n.map(|c| *c),
        }
        signal(effets, "sauvegarde-changee", json!({}));
        let titre = format!(
            "Serveur DNS : {}",
            if serveurs.is_empty() { "automatique".to_string() } else { serveurs.join(", ") }
        );
        signal(effets, "historique", json!({ "titre": titre, "details": [], "log": log }));
        effets.ajouter(Effet::Annonce("DNS modifié.".into()));
    }

    fn relancer_carte(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let Some(net) = self.net.clone().filter(|n| !n.wifi) else {
            effets.ajouter(Effet::Annonce("Aucune carte réseau par câble détectée.".into()));
            return;
        };
        // (par l'Aide, jusqu'à 60 s : dans un travail de fond, audit G3)
        if self.travaux.occupe() {
            ctx.journal("Réseau : carte pas redémarrée, un geste est encore en cours");
            self.refus_occupe(effets);
            return;
        }
        let geste = match ctx.prendre_geste("Redémarrer la carte réseau", PAGE) {
            Ok(g) => g,
            Err(refus) => {
                effets.ajouter(Effet::Annonce(refus));
                return;
            }
        };
        self.geste_nom = Some("Redémarrer la carte réseau".into());
        self.occuper(effets, "carte", "Redémarrage de la carte réseau...");
        ctx.journal(&format!("Carte réseau : redémarrage de {} (à {})", net.desc, format_vitesse(net.bps)));
        let idx = net.if_index;
        self.travaux.lancer_geste(ctx, GESTE, geste, move |fond| FinGeste::Carte {
            idx,
            r: fond
                .ctx
                .exiger_geste()
                .and_then(|p| fond.ctx.aide_qui_change(&p, "carte.relancer", &json!({ "Index": idx }), 60_000))
                .map(|_| ()),
        });
        self.relever_travaux(ctx, effets);
    }

    fn fin_relance(&mut self, effets: &mut Effets, idx: i64, r: Result<(), String>) {
        if let Err(e) = r {
            self.occuper(effets, "carte", "");
            effets.ajouter(Effet::Annonce(format!("Oups, quelque chose s'est mal passé :\n\n{e}")));
            return;
        }
        self.occuper(effets, "carte", "Carte réseau redémarrée, elle se reconnecte...");
        let (s, tx) = (self.sonde.clone(), self.tx.clone());
        std::thread::spawn(move || {
            s.attendre_carte(idx);
            let _ = tx.send(Message::CarteRevenue(s.connexion()));
        });
    }

    /// Ce que les gestes par l'Aide ont rendu (sur le fil des blocs)
    fn relever_travaux(&mut self, ctx: &Contexte, effets: &mut Effets) {
        for r in self.travaux.relever() {
            match r {
                Rendu::Effet(e) => effets.ajouter(e),
                Rendu::Fini { resultat: Ok(FinGeste::Audit { act, titre, r }), .. } => {
                    self.fin_geste_audit(ctx, effets, &act, &titre, r)
                }
                Rendu::Fini { resultat: Ok(FinGeste::Dns { log, serveurs, r }), .. } => {
                    self.fin_dns(effets, log, serveurs, r)
                }
                Rendu::Fini { resultat: Ok(FinGeste::Carte { idx, r }), .. } => self.fin_relance(effets, idx, r),
                Rendu::Fini { resultat: Err(e), .. } => {
                    self.occuper(effets, "smb1", "");
                    self.occuper(effets, "carte", "");
                    effets.ajouter(Effet::Annonce(format!("Oups, quelque chose s'est mal passé :\n\n{e}")));
                }
            }
        }
    }

    fn fin_carte(&mut self, ctx: &Contexte, effets: &mut Effets, n: Option<Connexion>) {
        if n.is_some() {
            self.net = n.clone();
        }
        self.carte_vue = Instant::now();
        let b = bridage(n.as_ref());
        self.carte_alertee = match (&b, &n) {
            (Some(_), Some(n)) => Some(format!("{}|{}", n.guid, n.if_index)),
            _ => None,
        };
        let texte = match (&n, &b) {
            (None, _) => "La carte réseau redémarre encore : Internet revient dans quelques secondes.".to_string(),
            (Some(_), Some(b)) => format!(
                "La carte est toujours à {}. Débranche le câble réseau 10 secondes puis rebranche-le. Si ça ne change rien, essaie un autre câble ou une autre prise de ta box.",
                format_vitesse(b.bps)
            ),
            (Some(n), None) => format!("Carte réseau redémarrée : elle est à {}.", format_vitesse(n.bps)),
        };
        ctx.journal(&format!("Carte réseau : {texte}"));
        self.occuper(effets, "carte", "");
        effets.ajouter(Effet::Annonce(texte));
        effets.ajouter(Effet::Rafraichir);
    }

    // -----------------------------------------------------------------------
    // Les messages des fils
    // -----------------------------------------------------------------------
    fn recevoir(&mut self, ctx: &Contexte, effets: &mut Effets) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Message::Scan(f, r) => self.fin_scan(ctx, effets, f, r),
                Message::Approfondi(r) => self.fin_approfondi(ctx, effets, r),
                Message::Audit(r) => self.fin_audit(ctx, effets, r),
                Message::Ping(j, rtt) => {
                    if let Some(Panneau::Appareil { ping, jeton, en_vol, .. }) = &mut self.panneau
                        && *jeton == j
                    {
                        ping.ajouter(rtt);
                        *en_vol = false;
                        effets.ajouter(Effet::Rafraichir);
                    }
                }
                Message::Ports(j, p) => {
                    if let Some(Panneau::Appareil { services, jeton, .. }) = &mut self.panneau
                        && *jeton == j
                    {
                        *services = Services::Trouves(p);
                        effets.ajouter(Effet::Rafraichir);
                    }
                }
                Message::Test(net, res) => {
                    self.test_en_cours = false;
                    self.net = net;
                    self.pings = res;
                    self.occuper(effets, "test", "");
                    effets.ajouter(Effet::Annonce(
                        if self.net.is_some() { "Test réseau terminé." } else { "Pas de connexion." }.into(),
                    ));
                    effets.ajouter(Effet::Rafraichir);
                }
                Message::CarteRevenue(n) => self.fin_carte(ctx, effets, n),
                Message::Surveillance(net, ips, r) => {
                    self.scan = false;
                    let Ok(b) = r else { continue };
                    let connus = Connus::depuis(self.lire(ctx, "appareils.json").as_ref());
                    let jour = self.date_jour();
                    if let Some((l, bulle)) =
                        scan::surveiller(&net, &ips, &b.arp, &connus, &mut self.vus, &jour, self.fab.as_ref())
                    {
                        ctx.journal(&l);
                        signal(
                            effets,
                            "bulle",
                            json!({ "titre": bulle.titre, "texte": bulle.texte, "ecran": "reseau" }),
                        );
                    }
                }
                Message::Connexion(n) => {
                    self.carte_en_vol = false;
                    if n.is_some() {
                        self.net = n.clone();
                    }
                    if let Some((l, b)) = alerte_carte(n.as_ref(), &mut self.carte_alertee, self.en_partie) {
                        ctx.journal(&l);
                        signal(effets, "bulle", json!({ "titre": b.titre, "texte": b.texte, "ecran": "connexion" }));
                    }
                }
            }
        }
        self.signaler_connexion(effets);
    }

    /// Le signal `connexion` pour la page Performances (le ping vers Internet du dernier test, la carte bridée),
    /// envoyé quand il change
    fn signaler_connexion(&mut self, effets: &mut Effets) {
        let internet = self
            .pings
            .iter()
            .find(|p| p.label.starts_with("Internet"))
            .map(|p| json!({ "moy": p.moyenne, "statut": p.statut }));
        let v = json!({ "internet": internet, "bridage": bridage(self.net.as_ref()).is_some() });
        if self.signal_connexion.as_ref() != Some(&v) {
            self.signal_connexion = Some(v.clone());
            signal(effets, "connexion", v);
        }
    }
}

impl Bloc for BlocReseau {
    fn nom(&self) -> &'static str {
        "reseau"
    }

    fn pages(&mut self, ctx: &Contexte, ecran: &str) -> Map<String, Value> {
        self.ecran = ecran.to_string();
        self.preparer(ctx);
        let (texte, trouves) = self.texte_scan();
        let etat = EtatReseau {
            liste: &self.liste,
            scan: self.scan,
            scan_texte: &texte,
            scan_trouves: trouves,
            progression: self.av.progression(),
            filtre: &self.filtre,
            audit: self.audit_sauve.as_ref(),
            audit_complet: self.audit.is_some(),
            surveillance: self.surveillance.unwrap_or(false),
            resume: &self.resume,
            conseil: &self.conseil,
        };
        let mut m = pages::page_reseau(&etat);
        m.insert("connexion".into(), pages::page_connexion(self.net.as_ref(), &self.pings, self.debit.as_deref()));
        if let Some(p) = self.page_panneau() {
            m.insert("panneau".into(), p);
        }
        // La tuile « Réseau » de l'accueil ($script:NetList, $script:NetScanning)
        m.insert("_accueil".into(), json!({ "reseau": { "appareils": self.liste.len(), "scan": self.scan } }));
        m
    }

    fn action(&mut self, ctx: &Contexte, id: &str, valeur: &Value, effets: &mut Effets) -> bool {
        self.preparer(ctx);
        let mien = self.panneau.is_some();
        match id {
            "clic:BtnNetScan" => self.lancer_scan(ctx, effets),
            "clic:BtnNetAudit" => self.lancer_audit(ctx, effets),
            "clic:BtnNetAuditView" => {
                // (un geste a fini pendant que l'utilisateur était ailleurs : l'audit est refait, pas montré périmé)
                if self.audit_a_refaire && !self.audit_en_cours && !self.en_partie {
                    self.lancer_audit(ctx, effets);
                    return true;
                }
                if self.audit.is_some() && !self.audit_en_cours {
                    self.panneau = Some(Panneau::Audit);
                    effets.ajouter(Effet::OuvrirEcran("panneau".into()));
                }
            }
            "coche:ChkNetWatch" => {
                let oui = crate::texte::vrai(Some(valeur));
                if let Err(e) = ctx.ecrire_reglage("NetWatch", json!(oui), false) {
                    ctx.journal(&e);
                }
                self.surveillance = Some(oui);
                self.prochaine_surveillance = Instant::now() + Duration::from_secs(600);
                effets.ajouter(Effet::Annonce(
                    if oui { "Surveillance du réseau activée." } else { "Surveillance du réseau désactivée." }.into(),
                ));
            }
            "net-filtre" => self.filtre = crate::texte::chaine(Some(valeur)),
            "clic:BtnPing" => self.tester_connexion(effets),
            "clic:BtnDnsFlush" => match self.sonde.vider_cache_dns() {
                Ok(()) => effets.ajouter(Effet::Annonce("Cache DNS vidé.".into())),
                Err(e) => effets.ajouter(Effet::Annonce(format!("Oups, quelque chose s'est mal passé :\n\n{e}"))),
            },
            "dns" => {
                let k = crate::texte::entier(&crate::texte::chaine(Some(valeur))).unwrap_or(-1);
                if (0..DNS_CHOIX.len() as i64).contains(&k) {
                    self.regler_dns(ctx, effets, k as usize);
                }
            }
            "carte-relancer" => self.question(
                effets,
                "carte",
                "Redémarrer la carte réseau ? Internet sera coupé 5 à 10 secondes : une partie en ligne, un appel ou un téléchargement en cours seront coupés.",
                Attente::Carte,
            ),
            "debit" => signal(effets, "test-debit", json!({})),
            "reseau-question" => {
                let qid = crate::texte::chaine(valeur.get("id"));
                let oui = crate::texte::chaine(valeur.get("valeur")) == "oui";
                let Some(i) = self.attente.iter().position(|(x, _)| *x == qid) else { return true };
                let (_, a) = self.attente.remove(i);
                if oui {
                    match a {
                        Attente::Carte => self.relancer_carte(ctx, effets),
                        Attente::ActionAudit(k, j) => self.geste_audit(ctx, effets, k, j),
                    }
                }
            }
            "panneau:close" if mien => self.fermer_panneau(effets),
            "retour" if mien => self.fermer_panneau(effets),
            "panneau:again" if mien => {
                if matches!(self.panneau, Some(Panneau::Audit) | Some(Panneau::AuditEchec(_))) {
                    self.lancer_audit(ctx, effets);
                }
            }
            _ => {
                if let Some(k) = id.strip_prefix("net-app:") {
                    if let Ok(k) = k.parse::<usize>() {
                        self.ouvrir_appareil(effets, k);
                    }
                    return true;
                }
                if mien && let Some(n) = id.strip_prefix("panneau:b:") {
                    let Some(b) = n.parse::<usize>().ok().and_then(|n| self.boutons.get(n).cloned()) else { return true };
                    match b {
                        BoutonPanneau::Ouvrir(u) => effets.ajouter(Effet::OuvrirAdresse(u)),
                        BoutonPanneau::Exporter => self.exporter_audit(effets),
                        BoutonPanneau::ActionAudit(k, j) => {
                            let confirmer = self
                                .audit
                                .as_ref()
                                .and_then(|a| a.points.get(k))
                                .and_then(|p| p.actions.get(j))
                                .map(|a| a.confirmer.clone())
                                .unwrap_or_default();
                            if confirmer.is_empty() {
                                self.geste_audit(ctx, effets, k, j);
                            } else {
                                self.question(effets, &format!("audit:{k}:{j}"), &confirmer, Attente::ActionAudit(k, j));
                            }
                        }
                    }
                    return true;
                }
                return false;
            }
        }
        true
    }

    fn tic(&mut self, ctx: &Contexte, cache: bool, effets: &mut Effets) -> Option<Duration> {
        self.recevoir(ctx, effets);
        self.relever_travaux(ctx, effets);
        // (l'utilisateur revient sur le panneau de l'audit après un geste fini ailleurs : l'audit est refait maintenant)
        if self.audit_a_refaire && !self.audit_en_cours && self.audit_regarde() {
            self.lancer_audit(ctx, effets);
        }
        if self.test_en_cours {
            let p = self.av.phase_actuelle();
            if !p.is_empty() {
                self.occuper(effets, "test", &p);
            }
        }
        // Le ping en direct de la fiche (toutes les 400 ms, seulement quand l'écran est regardé)
        if !cache
            && let Some(Panneau::Appareil { d, jeton, dernier, en_vol, .. }) = &mut self.panneau
            && !*en_vol
            && dernier.elapsed() >= Duration::from_millis(400)
        {
            *en_vol = true;
            *dernier = Instant::now();
            let (s, tx, ip, j) = (self.sonde.clone(), self.tx.clone(), d.ip.clone(), *jeton);
            std::thread::spawn(move || {
                let r = s.ping(&ip, 1000);
                let _ = tx.send(Message::Ping(j, r));
            });
        }
        // La carte bridée : la connexion relue toutes les minutes au plus, jamais pendant une partie (Watch-CarteReseau)
        if !self.en_partie && !self.carte_en_vol && self.carte_vue.elapsed() >= Duration::from_secs(60) {
            self.carte_vue = Instant::now();
            self.carte_en_vol = true;
            let (s, tx) = (self.sonde.clone(), self.tx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(Message::Connexion(s.connexion()));
            });
        }
        // La surveillance : un scan discret toutes les 10 minutes (Invoke-NetWatch)
        // (rien pendant une partie ni quand l'écran est caché : règle du coordinateur, voir DIVERGENCES.md)
        if self.surveillance == Some(true)
            && !cache
            && !self.en_partie
            && Instant::now() >= self.prochaine_surveillance
            && !self.scan
            && !self.audit_en_cours
        {
            self.prochaine_surveillance = Instant::now() + Duration::from_secs(600);
            if let Some(net) = self.sonde.connexion()
                && let Some(ip) = net.ip.clone().filter(|i| !i.is_empty())
            {
                let ips = scan::sous_reseau(&ip, net.prefix);
                self.scan = true;
                let (s, tx) = (self.sonde.clone(), self.tx.clone());
                let av = Arc::new(Avancement::default());
                std::thread::spawn(move || {
                    let r = s.scan(&ips, net.if_index, &av);
                    let _ = tx.send(Message::Surveillance(net, ips, r));
                });
            }
        }
        let actif = self.scan
            || self.audit_en_cours
            || self.test_en_cours
            || self.panneau.is_some()
            || self.carte_en_vol
            || self.travaux.occupe();
        if actif {
            if self.scan || self.audit_en_cours {
                effets.ajouter(Effet::Rafraichir);
            }
            return Some(Duration::from_millis(if cache { 1000 } else { 200 }));
        }
        let mut attente = Duration::from_secs(60).saturating_sub(self.carte_vue.elapsed());
        if self.surveillance == Some(true) {
            attente = attente.min(self.prochaine_surveillance.saturating_duration_since(Instant::now()));
        }
        Some(attente.max(Duration::from_millis(200)))
    }

    fn signal(&mut self, _ctx: &Contexte, nom: &str, donnees: &Value, _effets: &mut Effets) {
        match nom {
            "partie-commencee" => self.en_partie = true,
            "partie-finie" => self.en_partie = false,
            "debit-mesure" => {
                self.debit = donnees
                    .get("R")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().map(|x| crate::texte::reel(Some(x))).collect());
            }
            _ => {}
        }
    }

    fn travaux_finis(&mut self, ctx: &Contexte, effets: &mut Effets) {
        self.relever_travaux(ctx, effets);
    }

    fn fermer(&mut self, _ctx: &Contexte) {
        self.av.annuler.store(true, std::sync::atomic::Ordering::Relaxed);
        self.panneau = None;
    }
}
