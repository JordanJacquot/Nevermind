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
    occupe: String,
    signal_connexion: Option<Value>,
    /// Les gestes par l'Aide (SMBv1 jusqu'à 15 min, DNS, carte, pare-feu...) : dans un fil à part (audit G3)
    travaux: Travaux<FinGeste>,
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
            occupe: String::new(),
            signal_connexion: None,
            travaux: Travaux::default(),
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

    fn occuper(&mut self, effets: &mut Effets, texte: &str) {
        if self.occupe != texte {
            self.occupe = texte.to_string();
            signal(effets, "occupe", json!({ "texte": texte }));
        }
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
            self.lancer_audit(ctx, effets);
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
            return;
        }
        if act.geste == Geste::Smb1 {
            self.occuper(effets, "Désactivation de SMBv1...");
        }
        let sonde = self.sonde.clone();
        self.travaux.lancer(ctx, GESTE, move |fond| {
            let ctx = fond.ctx;
            let mut log: Vec<Value> = Vec::new();
            let definitif = |log: &mut Vec<Value>, t: &str, d: String, retour: &str| {
                log.push(json!({ "Type": "definitif", "Titre": t, "Detail": d, "Retour": retour }));
            };
            let r: Result<(), String> = match act.geste {
                Geste::WifiManuel => {
                    let noms = crate::texte::liste(Some(&act.arg));
                    ctx.aide("wifi.manuel", &json!({ "Profils": noms }), 30_000).map(|_| {
                        definitif(&mut log, "Wi-Fi ouverts en connexion manuelle", format!("Connexion manuelle : {}", noms.join(", ")),
                            "Paramètres de Windows, Réseau et Internet, Wi-Fi, « Gérer les réseaux connus » : choisir le réseau et cocher « Se connecter automatiquement ».");
                    })
                }
                Geste::PareFeu => ctx.aide("pare-feu.activer", &json!({}), 30_000).map(|_| {
                    definitif(&mut log, "Pare-feu de Windows réactivé", "Pare-feu réactivé sur tous les réseaux".into(),
                        "Sécurité Windows, « Pare-feu et protection du réseau » : choisir le réseau et désactiver le pare-feu.");
                }),
                Geste::Smb1 => {
                    ctx.aide("smb1.couper", &json!({ "Client": crate::texte::vrai(Some(&act.arg)) }), 900_000).map(|_| {
                        definitif(&mut log, "SMBv1 désactivé", "Ancien partage de fichiers SMBv1 désactivé".into(),
                            "Panneau de configuration, Programmes, « Activer ou désactiver des fonctionnalités Windows » : cocher « Support de partage de fichiers SMB 1.0/CIFS ».");
                    })
                }
                Geste::BureauDistance => {
                    let cle = r"HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server";
                    log.push(json!({ "Type": "reg", "Path": cle, "Name": "fDenyTSConnections", "Existed": true, "Value": 0, "Kind": "DWord", "Nouveau": 1, "NouveauType": "DWord" }));
                    ctx.aide("reg.ecrire", &json!({ "Cle": cle, "Nom": "fDenyTSConnections", "Valeur": 1, "Type": "DWord" }), 30_000).map(|_| ())
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
            Geste::Smb1 => self.occuper(effets, ""),
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
        if act.apres {
            self.lancer_audit(ctx, effets);
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
        self.occuper(effets, "Test de la connexion...");
    }

    fn regler_dns(&mut self, ctx: &Contexte, effets: &mut Effets, k: usize) {
        let Some(net) = self.net.clone() else {
            effets.ajouter(Effet::Annonce("Aucune connexion détectée.".into()));
            return;
        };
        // (par l'Aide : dans un travail de fond, audit G3 ; un autre geste en cours : rien n'est lancé)
        if self.travaux.occupe() {
            ctx.journal("Réseau : DNS pas changé, un geste est encore en cours");
            return;
        }
        let avant = self.sonde.dns_fixes(&net.guid);
        let serveurs: Vec<String> = DNS_CHOIX[k].iter().map(|s| s.to_string()).collect();
        let log = json!([{ "Type": "dns", "IfIndex": net.if_index, "Guid": net.guid,
            "Servers": avant.split([',', ' ', '\t']).filter(|x| !x.is_empty()).collect::<Vec<_>>(), "Nouveau": serveurs }]);
        let sonde = self.sonde.clone();
        self.travaux.lancer(ctx, GESTE, move |fond| {
            let r = fond
                .ctx
                .aide("dns.regler", &json!({ "Index": net.if_index, "Serveurs": serveurs }), 30_000)
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
            return;
        }
        self.occuper(effets, "Redémarrage de la carte réseau...");
        ctx.journal(&format!("Carte réseau : redémarrage de {} (à {})", net.desc, format_vitesse(net.bps)));
        let idx = net.if_index;
        self.travaux.lancer(ctx, GESTE, move |fond| FinGeste::Carte {
            idx,
            r: fond.ctx.aide("carte.relancer", &json!({ "Index": idx }), 60_000).map(|_| ()),
        });
        self.relever_travaux(ctx, effets);
    }

    fn fin_relance(&mut self, effets: &mut Effets, idx: i64, r: Result<(), String>) {
        if let Err(e) = r {
            self.occuper(effets, "");
            effets.ajouter(Effet::Annonce(format!("Oups, quelque chose s'est mal passé :\n\n{e}")));
            return;
        }
        self.occuper(effets, "Carte réseau redémarrée, elle se reconnecte...");
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
                    self.occuper(effets, "");
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
        self.occuper(effets, "");
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
                    self.occuper(effets, "");
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

    fn pages(&mut self, ctx: &Contexte, _ecran: &str) -> Map<String, Value> {
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
        if self.test_en_cours {
            let p = self.av.phase_actuelle();
            if !p.is_empty() {
                self.occuper(effets, &p);
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
