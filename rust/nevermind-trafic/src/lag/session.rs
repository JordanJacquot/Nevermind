//! Une mesure du lag en cours (`Start-LagSession`, `Update-LagSession`, `Stop-LagSession`) : le serveur du jeu
//! repéré dans la trace réseau (UDP, par l'Aide) ou à défaut sa connexion TCP, les pings de chaque étape du
//! chemin, les téléchargements des autres programmes, le Wi-Fi ; à la fin, la mesure gardée avec son diagnostic.

use super::diag;
use super::pings::{Pingeur, Pings};
use super::stats;
use crate::format::arrondi;
use crate::ip::privee;
use crate::serveurs::Lieu;
use crate::valeur::n;
use regex::Regex;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, LazyLock};

/// Ce dont la mesure a besoin autour d'elle (le bloc, ou un faux dans les tests)
pub trait Env {
    /// La trace réseau (lignes « pid|proto|adresse|port|paquets envoyés|reçus|octets envoyés|reçus|?|trous|plus long|vu il y a »)
    fn flux(&mut self) -> Vec<String>;
    /// Les connexions TCP (comme la page Trafic)
    fn tcp(&mut self) -> Vec<String>;
    /// Le nom lisible d'un processus (`Get-LagProcName`)
    fn nom_processus(&mut self, pid: u32) -> String;
    /// Le propriétaire connu d'une adresse : (O, N, libellé)
    fn proprietaire(&mut self, ip: &str) -> Option<(String, String, String)>;
    /// Ce qu'ipinfo a dit d'une adresse (déjà reçu)
    fn lieu(&self, ip: &str) -> Option<Lieu>;
    /// Demande le propriétaire d'une adresse à l'annuaire (si permis, en arrière-plan)
    fn chercher_proprietaire(&mut self, ip: &str);
    /// Demande le pays d'une adresse (si permis, en arrière-plan)
    fn chercher_lieu(&mut self, ip: &str);
    /// Une ligne au journal
    fn journal(&mut self, m: &str);
    /// Le numéro de Nevermind
    fn moi(&self) -> u32;
}

#[derive(Clone, Debug, Default)]
pub struct Serveur {
    pub ip: String,
    pub port: i64,
    pub proto: String,
    pub ping: Option<bool>,
    pub proxy: String,
    pub pays: String,
    pub ville: String,
    pub controles: u32,
    pub via_proxy: bool,
}

struct Meilleur {
    ip: String,
    port: i64,
    proto: String,
    pk: f64,
    trous: i64,
    plus_long: i64,
}

/// La mesure en cours (`$script:LagSession`)
pub struct Session {
    pub jeu: String,
    pub pid: u32,
    /// Secondes au départ (horloge du bloc)
    pub debut: f64,
    pub secondes: f64,
    pub passerelle: String,
    pub wifi: bool,
    pub carte: String,
    pub etw: bool,
    pub cibles: HashMap<&'static str, usize>,
    pub isp: String,
    pub serveur: Option<Serveur>,
    /// La trace a repris en cours de partie (l'Aide dormait au début) : le serveur de connexion suivi jusque-là (TCP) est
    /// remplacé dès que la trace montre celui de la partie, sans attendre le contrôle des 15 passages (lot F)
    connexion_a_remplacer: bool,
    pub pairs: HashSet<String>,
    pub passages: u64,
    prev: HashMap<String, f64>,
    noms: HashMap<u32, String>,
    pub fond: Vec<Value>,
    pub signal: Vec<i64>,
    pub bande: String,
    pub canal: String,
    trace: Option<Receiver<Vec<String>>>,
    trace_srv: Option<Receiver<Vec<String>>>,
    wifi_job: Option<Receiver<Vec<String>>>,
    pub trous: i64,
    pub plus_long_trou: i64,
    base_trous: Option<i64>,
    pub pings: Box<dyn Pings>,
    pingeur: Arc<dyn Pingeur>,
    lire_wifi: Arc<dyn Fn() -> Vec<String> + Send + Sync>,
}

static VPN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)vpn|wireguard|openvpn|tap-windows|nord|proton|mullvad|surfshark|expressvpn|tunnel").expect("motif")
});
static SIGNAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^\s+Signal\s+:\s+(\d+)").expect("motif"));
static CANAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^\s+(Canal|Channel)\s+:\s+(\d+)").expect("motif"));
static BANDE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^\s+(Bande|Band)\s+:\s+(.+?)\s*$").expect("motif"));

/// Le réseau au départ de la mesure
pub struct Depart {
    pub passerelle: String,
    pub wifi: bool,
    pub carte: String,
}

impl Session {
    /// `Start-LagSession` (la trace réseau est déjà demandée : `etw`)
    #[allow(clippy::too_many_arguments)]
    pub fn demarrer(
        jeu: &str,
        pid: u32,
        secondes: f64,
        maintenant: f64,
        reseau: Option<Depart>,
        etw: bool,
        mut pings: Box<dyn Pings>,
        pingeur: Arc<dyn Pingeur>,
        lire_wifi: Arc<dyn Fn() -> Vec<String> + Send + Sync>,
    ) -> Session {
        let gw = reseau
            .as_ref()
            .map(|r| r.passerelle.clone())
            .filter(|g| !g.is_empty() && g != "0.0.0.0")
            .unwrap_or_default();
        let mut cibles = HashMap::new();
        if !gw.is_empty() {
            cibles.insert("gw", pings.ajouter(&gw));
        }
        cibles.insert("ref", pings.ajouter("1.1.1.1"));
        // Chemin vers Internet en arrière-plan : le premier routeur public est celui du fournisseur
        let (tx, rx) = channel();
        let p = pingeur.clone();
        let _ = std::thread::Builder::new().name("nevermind-trace".into()).spawn(move || {
            let _ = tx.send(p.trace("1.1.1.1", 10, 800));
        });
        Session {
            jeu: jeu.into(),
            pid,
            debut: maintenant,
            secondes,
            passerelle: gw,
            wifi: reseau.as_ref().is_some_and(|r| r.wifi),
            carte: reseau.map(|r| r.carte).unwrap_or_default(),
            etw,
            cibles,
            isp: String::new(),
            serveur: None,
            connexion_a_remplacer: false,
            pairs: HashSet::new(),
            passages: 0,
            prev: HashMap::new(),
            noms: HashMap::new(),
            fond: Vec::new(),
            signal: Vec::new(),
            bande: String::new(),
            canal: String::new(),
            trace: Some(rx),
            trace_srv: None,
            wifi_job: None,
            trous: 0,
            plus_long_trou: 0,
            base_trous: None,
            pings,
            pingeur,
            lire_wifi,
        }
    }

    /// La trace réseau de l'Aide a démarré en cours de partie (l'Aide dormait au début, lot F) : le serveur de la
    /// partie est cherché par la trace (UDP) dès le prochain passage, à la place du serveur de connexion (TCP)
    pub fn reprendre_trace(&mut self) {
        self.etw = true;
        self.connexion_a_remplacer = self.serveur.is_some();
    }

    /// `Test-AdresseParticulier`
    fn particulier(env: &mut dyn Env, ip: &str) -> Option<bool> {
        let p = env.proprietaire(ip);
        let h = env.lieu(ip).map(|l| l.hote);
        crate::serveurs::particulier(p.as_ref().map(|(o, n, _)| (o.as_str(), n.as_str())), h.as_deref())
    }

    /// Une adresse d'un autre joueur : écartée pour le reste de la partie
    fn ajouter_pair(&mut self, env: &mut dyn Env, ip: &str) {
        if self.pairs.insert(ip.to_string()) {
            env.journal(&format!(
                "Lag: {ip} écarté, c'est la box d'un autre joueur (pair à pair) et pas le serveur du jeu"
            ));
        }
    }

    /// Le serveur suivi est abandonné : il n'est plus pingé
    fn retirer_serveur(&mut self) {
        for k in ["srv", "proxy"] {
            if let Some(i) = self.cibles.remove(k) {
                self.pings.retirer(i);
            }
        }
        self.trace_srv = None;
        self.serveur = None;
    }

    /// Un passage (toutes les 2 s) ; `true` quand la mesure est finie (test rapide arrivé au bout)
    pub fn passage(&mut self, env: &mut dyn Env, maintenant: f64) -> bool {
        self.passages += 1;
        let ecoule = maintenant - self.debut;
        // Premier routeur du fournisseur
        if let Some(rx) = &self.trace
            && let Ok(l) = rx.try_recv()
        {
            self.trace = None;
            let hop =
                l.iter().filter_map(|x| x.split('|').nth(1).map(String::from)).find(|a| !a.is_empty() && !privee(a));
            if let Some(h) = hop
                && h != "1.1.1.1"
            {
                self.isp = h.clone();
                self.cibles.insert("isp", self.pings.ajouter(&h));
                env.chercher_proprietaire(&h);
            }
        }
        // Le serveur suivi est la box d'un autre joueur (connu quelques secondes après, par l'annuaire) : écarté
        if let Some(s) = self.serveur.as_mut()
            && s.controles < 30
        {
            s.controles += 1;
            let ip = s.ip.clone();
            if Session::particulier(env, &ip) == Some(true) {
                self.ajouter_pair(env, &ip);
                self.retirer_serveur();
            }
        }
        // Réseau par programme (trace de l'Aide) : serveur du jeu et téléchargements en arrière-plan
        let flux = if self.etw { env.flux() } else { Vec::new() };
        let mut par_pid: Vec<(u32, f64)> = Vec::new();
        let mut best: Option<Meilleur> = None;
        let moi = env.moi();
        for l in &flux {
            let x: Vec<&str> = l.split('|').collect();
            if x.len() < 12 || privee(x[2]) {
                continue;
            }
            let f = |i: usize| x[i].trim().parse::<f64>().unwrap_or(0.0);
            let fpid = f(0) as u32;
            let octets = f(6) + f(7);
            let k = format!("{}|{}|{}|{}", x[0], x[1], x[2], x[3]);
            let prev = self.prev.get(&k).copied().unwrap_or(0.0);
            self.prev.insert(k, octets);
            if fpid == self.pid {
                let pk = f(4) + f(5);
                if f(5) > 50.0
                    && (f(11) as i64) < 5000
                    && !self.pairs.contains(x[2])
                    && best.as_ref().is_none_or(|b| pk > b.pk)
                {
                    best = Some(Meilleur {
                        ip: x[2].into(),
                        port: f(3) as i64,
                        proto: x[1].into(),
                        pk,
                        trous: f(9) as i64,
                        plus_long: f(10) as i64,
                    });
                }
            } else if fpid != moi && fpid > 4 {
                match par_pid.iter_mut().find(|(p, _)| *p == fpid) {
                    Some(e) => e.1 += octets - prev,
                    None => par_pid.push((fpid, octets - prev)),
                }
            }
        }
        // Sans la trace : la connexion TCP du jeu (serveur de connexion, souvent dans la même région)
        if !self.etw && self.pid != 0 && self.serveur.is_none() && self.passages >= 3 {
            for l in env.tcp() {
                let x: Vec<&str> = l.split('|').collect();
                if x.len() >= 3
                    && x[0].trim().parse::<u32>().ok() == Some(self.pid)
                    && !privee(x[1])
                    && !self.pairs.contains(x[1])
                {
                    best = Some(Meilleur {
                        ip: x[1].into(),
                        port: x[2].trim().parse().unwrap_or(0),
                        proto: "tcp".into(),
                        pk: 0.0,
                        trous: 0,
                        plus_long: 0,
                    });
                    break;
                }
            }
        }
        let quinze = self.passages.is_multiple_of(15);
        // (la trace vient de reprendre : le serveur de connexion suivi jusque-là laisse la place au premier serveur
        //  qu'elle montre, même à la même adresse, pour suivre ses pertes ; lot F)
        let remplacer = self.connexion_a_remplacer && best.is_some();
        let nouveau =
            |s: &Option<Serveur>, b: &Meilleur| remplacer || s.as_ref().is_none_or(|s| s.ip != b.ip && quinze);
        let change = best.as_ref().is_some_and(|b| nouveau(&self.serveur, b));
        if change && let Some(b) = &best {
            let ip = b.ip.clone();
            if Session::particulier(env, &ip) == Some(true) {
                self.ajouter_pair(env, &ip);
                best = None;
            }
        }
        if let Some(b) = best {
            if nouveau(&self.serveur, &b) {
                self.connexion_a_remplacer = false;
                self.retirer_serveur();
                self.serveur =
                    Some(Serveur { ip: b.ip.clone(), port: b.port, proto: b.proto.clone(), ..Default::default() });
                self.cibles.insert("srv", self.pings.ajouter(&b.ip));
                self.base_trous = Some(b.trous);
                env.chercher_proprietaire(&b.ip);
                // (`Start-LagGeoLookup` : le lieu déjà connu est pris tout de suite)
                if let Some(l) = env.lieu(&b.ip)
                    && let Some(s) = self.serveur.as_mut()
                {
                    s.pays = l.pays;
                    s.ville = l.ville;
                } else {
                    env.chercher_lieu(&b.ip);
                }
            }
            if self.serveur.as_ref().is_some_and(|s| s.ip == b.ip)
                && let Some(base) = self.base_trous
            {
                self.trous = b.trous - base;
                self.plus_long_trou = self.plus_long_trou.max(b.plus_long);
            }
        }
        // Le serveur ne répond pas au ping : on mesure le dernier routeur avant lui
        if let Some(s) = self.serveur.as_mut()
            && s.ping.is_none()
            && let Some(&i) = self.cibles.get("srv")
        {
            let a = self.pings.serie(i);
            if a.len() >= 16 {
                let ok = a.iter().skip(1).step_by(2).filter(|&&r| r >= 0.0).count();
                s.ping = Some(ok > 0);
                if ok == 0 {
                    let (tx, rx) = channel();
                    let (p, ip) = (self.pingeur.clone(), s.ip.clone());
                    let _ = std::thread::Builder::new().name("nevermind-trace".into()).spawn(move || {
                        let _ = tx.send(p.trace(&ip, 20, 800));
                    });
                    self.trace_srv = Some(rx);
                }
            }
        }
        if let Some(rx) = &self.trace_srv
            && let Ok(l) = rx.try_recv()
        {
            self.trace_srv = None;
            let dernier = l
                .iter()
                .filter_map(|x| x.split('|').nth(1).map(String::from))
                .rfind(|a| !a.is_empty() && !privee(a) && *a != self.isp);
            if let (Some(d), Some(s)) = (dernier, self.serveur.as_mut())
                && d != s.ip
            {
                s.proxy = d.clone();
                self.cibles.insert("proxy", self.pings.ajouter(&d));
            }
        }
        // Téléchargements des autres programmes (octets par seconde)
        let mut top: Option<(u32, f64)> = None;
        let mut total = 0.0;
        for (p, o) in &par_pid {
            let r = o / 2.0;
            total += r;
            if top.is_none_or(|t| r > t.1) {
                top = Some((*p, r));
            }
        }
        let nom = match top {
            Some((p, r)) if r > 200.0 * 1024.0 => match self.noms.get(&p) {
                Some(x) => x.clone(),
                None => {
                    let x = env.nom_processus(p);
                    self.noms.insert(p, x.clone());
                    x
                }
            },
            _ => String::new(),
        };
        self.fond.push(json!({ "T": n(arrondi(ecoule, 1)), "Rate": n(arrondi(total, 0)), "Name": nom, "NameRate": n(top.map(|t| arrondi(t.1, 0)).unwrap_or(0.0)) }));
        // Wi-Fi : force du signal et bande, lues en arrière-plan
        if let Some(rx) = &self.wifi_job
            && let Ok(lignes) = rx.try_recv()
        {
            self.wifi_job = None;
            for ln in lignes {
                if let Some(c) = SIGNAL.captures(&ln) {
                    self.signal.push(c[1].parse().unwrap_or(0));
                } else if let Some(c) = CANAL.captures(&ln) {
                    self.canal = c[2].to_string();
                } else if let Some(c) = BANDE.captures(&ln) {
                    self.bande = c[2].to_string();
                }
            }
        }
        if self.wifi && self.passages % 5 == 1 && self.wifi_job.is_none() {
            let (tx, rx) = channel();
            let f = self.lire_wifi.clone();
            let _ = std::thread::Builder::new().name("nevermind-wifi".into()).spawn(move || {
                let _ = tx.send(f());
            });
            self.wifi_job = Some(rx);
        }
        self.secondes > 0.0 && ecoule >= self.secondes
    }

    /// Le texte de la mesure en cours (`Update-LagLive`)
    pub fn en_direct(&self, maintenant: f64) -> String {
        let mut parts = Vec::new();
        for (k, nom) in [("gw", "box"), ("isp", "fournisseur"), ("ref", "Internet"), ("srv", "serveur")] {
            let Some(&i) = self.cibles.get(k) else { continue };
            let a = self.pings.serie(i);
            if a.len() < 2 {
                continue;
            }
            let v = a[a.len() - 1];
            if v < 0.0 && (k == "isp" || k == "srv") {
                continue;
            }
            parts.push(format!(
                "{nom} {}",
                if v < 0.0 { "perdu".into() } else { format!("{} ms", crate::format::nombre_n(v, 0)) }
            ));
        }
        let el = crate::format::entier(maintenant - self.debut);
        let quoi = if self.secondes > 0.0 {
            format!(" ({el} / {} s)", self.secondes as i64)
        } else {
            format!(" pendant {} ({} min)", self.jeu, crate::format::entier(el as f64 / 60.0))
        };
        format!("Mesure en cours{quoi} : {}", parts.join(", "))
    }

    /// `Stop-LagSession` : la mesure gardée (avec son diagnostic), ou `None` si trop courte. `date` : maintenant en
    /// heure locale (format « o »), `id` : 10 caractères.
    pub fn finir(mut self, env: &mut dyn Env, maintenant: f64, date: &str, id: &str) -> Option<Value> {
        self.pings.arreter();
        let dur = maintenant - self.debut;
        if dur < if self.secondes > 0.0 { 6.0 } else { 60.0 } {
            return None;
        }
        let pas = f64::max(1.0, (dur / 240.0).ceil());
        let mut st: HashMap<&str, stats::Stats> = HashMap::new();
        let mut series: HashMap<&str, Vec<f64>> = HashMap::new();
        let mut cles: Vec<(&'static str, usize)> = self.cibles.iter().map(|(k, v)| (*k, *v)).collect();
        cles.sort_by_key(|(k, _)| *k);
        for (k, i) in cles {
            let a = self.pings.serie(i);
            if let Some(s) = stats::ping(&a) {
                st.insert(k, s);
                series.insert(k, stats::reduire(&a, pas));
            }
        }
        // Le serveur ne répond pas au ping : le dernier routeur avant lui le remplace
        if self.serveur.is_some() && st.get("srv").is_some_and(|s| s.mort) && st.get("proxy").is_some_and(|s| !s.mort) {
            if let Some(p) = st.get("proxy").cloned() {
                st.insert("srv", p);
            }
            if let Some(p) = series.get("proxy").cloned() {
                series.insert("srv", p);
            }
            if let Some(s) = self.serveur.as_mut() {
                s.via_proxy = true;
            }
        }
        st.remove("proxy");
        series.remove("proxy");
        let main = match st.get("srv") {
            Some(s) if !s.mort => Some(s.clone()),
            _ => st.get("ref").cloned(),
        };
        let liens = stats::liens(
            main.as_ref().map(|m| m.pics.as_slice()).unwrap_or(&[]),
            st.get("gw").map(|g| g.pics.as_slice()).unwrap_or(&[]),
            &self.fond,
        );
        let srv_owner = self.serveur.as_ref().and_then(|s| env.proprietaire(&s.ip)).map(|x| x.2).unwrap_or_default();
        let isp_owner = if self.isp.is_empty() {
            String::new()
        } else {
            env.proprietaire(&self.isp.clone()).map(|x| x.2).unwrap_or_default()
        };
        let mut noms = liens.noms.clone();
        noms.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut stats_json = Map::new();
        for (k, s) in &st {
            stats_json.insert(k.to_string(), s.json_garde());
        }
        let mut series_json = Map::new();
        for (k, s) in &series {
            series_json.insert(k.to_string(), json!(s.iter().map(|x| n(*x)).collect::<Vec<_>>()));
        }
        let signal = if self.signal.is_empty() { Value::Null } else { json!(self.signal.iter().min()) };
        let signal_moy = if self.signal.is_empty() {
            Value::Null
        } else {
            n(arrondi(self.signal.iter().sum::<i64>() as f64 / self.signal.len() as f64, 0))
        };
        let mut rec = json!({
            "Id": id, "Date": date, "Game": self.jeu, "Quick": self.secondes > 0.0, "Seconds": crate::format::entier(dur),
            "Wifi": self.wifi, "Vpn": VPN.is_match(&self.carte),
            "Signal": signal, "SignalAvg": signal_moy, "Band": self.bande, "Channel": self.canal, "Etw": self.etw,
            "Server": match &self.serveur {
                Some(s) => json!({ "Ip": s.ip, "Port": s.port, "Proto": s.proto, "ViaProxy": s.via_proxy, "Owner": srv_owner, "Pays": s.pays, "Ville": s.ville }),
                None => Value::Null,
            },
            "Isp": isp_owner,
            "Stats": stats_json, "Step": n(pas), "Series": series_json,
            "Spikes": main.as_ref().map(|m| m.pics.len()).unwrap_or(0), "LocalSpikes": liens.local, "BgSpikes": liens.fond,
            "BgApps": noms.iter().take(3).map(|(k, v)| json!({ "Name": k, "Rate": n(*v) })).collect::<Vec<_>>(),
            "Gaps": self.trous, "MaxGap": self.plus_long_trou,
        });
        let d = diag::diagnostic(&rec);
        rec["Diag"] = d.json();
        Some(rec)
    }
}
