//! Le relevé de la page Trafic (`New-TrafficState`, `Get-TrafficApp`, `Update-Traffic`, `Compress-TrafficDests`,
//! `Get-TrafficDestCount`) : les connexions TCP lues à chaque passage, regroupées par programme et par destination.
//! Les octets des connexions fermées restent comptés.

use crate::ip;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

/// Ports signalés (`$SusPorts`)
pub fn port_suspect(p: i64) -> Option<&'static str> {
    Some(match p {
        4444 | 1337 | 31337 | 5555 | 12345 => "port souvent utilisé par les logiciels espions",
        6666 | 6667 | 6697 => "discussion IRC (utilisée par des virus)",
        8333 => "réseau Bitcoin",
        3333 => "minage de cryptomonnaie",
        9001 | 9030 | 9050 | 9150 => "réseau Tor",
        23 => "Telnet (non chiffré)",
        _ => return None,
    })
}

/// Le nom d'un port connu (`$PortNames`)
pub fn nom_port(p: i64) -> Option<&'static str> {
    Some(match p {
        443 => "web sécurisé (HTTPS)",
        80 => "web (HTTP)",
        53 => "noms de domaine (DNS)",
        853 => "DNS chiffré",
        993 => "mails (IMAP)",
        995 => "mails (POP)",
        587 | 465 | 25 => "envoi de mails",
        22 => "accès à distance (SSH)",
        3389 => "bureau à distance",
        5228 => "notifications Google",
        5222 => "messagerie",
        3478 => "appels audio / vidéo",
        1194 | 51820 => "VPN",
        8080 => "web (autre port)",
        27015 => "jeux (Steam)",
        27036 => "Steam",
        5938 => "TeamViewer",
        7070 | 6568 => "AnyDesk",
        _ => return None,
    })
}

/// Une destination d'un programme (une adresse et un port)
#[derive(Clone, Debug, Default)]
pub struct Dest {
    pub distante: String,
    pub port: i64,
    pub out_ferme: f64,
    pub in_ferme: f64,
    pub out: f64,
    pub entrant: f64,
    /// Secondes (horloge du relevé) de la première et de la dernière fois où elle a été vue ouverte
    pub premier: f64,
    pub dernier: Option<f64>,
    pub vivante: bool,
    pub privee: bool,
}

/// Un service Windows (nom court, nom affiché)
#[derive(Clone, Debug, PartialEq)]
pub struct Service {
    pub nom: String,
    pub titre: String,
}

/// Un programme qui communique
#[derive(Clone, Debug, Default)]
pub struct App {
    pub cle: String,
    pub nom: String,
    pub chemin: String,
    pub services: Vec<Service>,
    pub titre: String,
    pub out_ferme: f64,
    pub in_ferme: f64,
    pub out: f64,
    pub entrant: f64,
    pub debit: f64,
    pub dernier_out: f64,
    pub dest: IndexMap<String, Dest>,
    pub udp: bool,
    pub pids: HashSet<u32>,
    /// « Valid », « NotSigned », « HashMismatch », « NoPath »... ; `None` : pas encore vérifiée
    pub sig: Option<String>,
    pub editeur: String,
    pub moi: bool,
    pub vivantes: i64,
    pub parent: String,
    pub parent_sig: String,
    pub parent_pub: String,
    pub sig_essais: i64,
    pub dest_anciennes: HashSet<String>,
    pub ips_anciennes: HashSet<String>,
}

impl App {
    /// Nombre de destinations sur Internet (réseau local à part), oubliées comprises (`Get-TrafficDestCount`)
    pub fn nb_destinations(&self) -> usize {
        self.dest.values().filter(|d| !d.privee).count() + self.dest_anciennes.len()
    }
}

/// Une connexion TCP vue au dernier passage
#[derive(Clone, Debug)]
struct Conn {
    app: String,
    distante: String,
    port: i64,
    out: f64,
    entrant: f64,
}

/// Ce que Windows dit d'un processus pour le regrouper
pub trait Processus {
    /// Les noms des processus en cours (sans « .exe »), par numéro
    fn noms(&self) -> HashMap<u32, String>;
    /// L'emplacement d'un processus (vide si Windows le cache)
    fn chemin(&self, pid: u32) -> String;
    /// Les services Windows d'un processus, triés par nom
    fn services(&self, pid: u32) -> Vec<Service>;
}

/// L'état de la surveillance (`$script:Traffic`)
#[derive(Default)]
pub struct Releve {
    pub apps: IndexMap<String, App>,
    pids: HashMap<u32, String>,
    conns: IndexMap<String, Conn>,
    pub dns: HashMap<String, String>,
    pub passages: u64,
    noms: HashMap<u32, String>,
    noms_passage: Option<u64>,
    /// Emplacements lus par l'Aide (processus lancés en administrateur)
    pub chemins_aide: HashMap<u32, String>,
    /// Programmes dont la signature est à vérifier, dans l'ordre
    pub a_verifier: std::collections::VecDeque<String>,
    pub signatures: HashMap<String, String>,
    /// Le numéro de Nevermind (`$PID`)
    pub moi: u32,
    /// Seconde (horloge du relevé) du début de la surveillance
    pub debut: f64,
    /// Seconde du dernier passage (pour le débit)
    dernier_passage: Option<f64>,
    /// Alertes déjà annoncées (une fois par programme)
    pub prevenus: HashSet<String>,
}

impl Releve {
    pub fn nouveau(moi: u32, debut: f64) -> Releve {
        Releve { moi, debut, ..Default::default() }
    }

    /// Les processus vus (pour les emplacements à demander à l'Aide)
    pub fn connu(&self, pid: u32) -> bool {
        self.pids.contains_key(&pid)
    }

    /// Le programme d'un processus (plusieurs processus du même programme sont regroupés)
    pub fn app_de(&mut self, pid: u32, sys: &dyn Processus) -> String {
        if let Some(k) = self.pids.get(&pid) {
            return k.clone();
        }
        // (noms des programmes : une seule lecture par passage pour tous les nouveaux)
        if self.noms_passage.is_none() || (!self.noms.contains_key(&pid) && self.noms_passage != Some(self.passages)) {
            self.noms = sys.noms();
            self.noms_passage = Some(self.passages);
        }
        let pname = self.noms.get(&pid).cloned().unwrap_or_default();
        let nom = if pid == 4 {
            "System".to_string()
        } else if !pname.is_empty() {
            pname
        } else {
            format!("Programme {pid}")
        };
        let mut chemin = if pid > 4 { sys.chemin(pid) } else { String::new() };
        if chemin.is_empty() {
            chemin = self.chemins_aide.get(&pid).cloned().unwrap_or_default();
        }
        let svc = if nom == "svchost" { sys.services(pid) } else { Vec::new() };
        let cle = if pid == self.moi {
            "nevermind".to_string()
        } else if !svc.is_empty() {
            format!("svc:{}", svc.iter().map(|s| s.nom.to_lowercase()).collect::<Vec<_>>().join(","))
        } else if !chemin.is_empty() {
            chemin.to_lowercase()
        } else {
            format!("nom:{}", nom.to_lowercase())
        };
        if !self.apps.contains_key(&cle) {
            let desc = if let Some(s0) = svc.first() {
                format!(
                    "Windows : {}{}",
                    s0.titre,
                    if svc.len() > 1 { format!(" (+{})", svc.len() - 1) } else { String::new() }
                )
            } else {
                String::new()
            };
            let moi = pid == self.moi;
            let titre = if moi {
                "Nevermind (cette app)".to_string()
            } else if !desc.is_empty() && desc.chars().count() < 90 {
                desc
            } else {
                nom.clone()
            };
            let mut a =
                App { cle: cle.clone(), nom, chemin: chemin.clone(), services: svc, titre, moi, ..Default::default() };
            if !chemin.is_empty() && !self.signatures.contains_key(&cle) {
                self.a_verifier.push_back(cle.clone());
            } else if chemin.is_empty() {
                a.sig = Some("NoPath".into());
            }
            self.apps.insert(cle.clone(), a);
        }
        if let Some(a) = self.apps.get_mut(&cle) {
            a.pids.insert(pid);
        }
        self.pids.insert(pid, cle.clone());
        cle
    }

    /// Oublie les processus fermés (Windows réutilise leurs numéros)
    pub fn oublier_morts(&mut self, vivants: &HashSet<u32>) {
        let morts: Vec<u32> = self.pids.keys().filter(|p| !vivants.contains(p)).copied().collect();
        for p in morts {
            if let Some(k) = self.pids.remove(&p)
                && let Some(a) = self.apps.get_mut(&k)
            {
                a.pids.remove(&p);
            }
            self.chemins_aide.remove(&p);
        }
    }

    /// Un passage : les connexions TCP « pid|adresse|port distant|port local|état|octets envoyés|octets reçus »
    /// (`Update-Traffic`, sans les lectures en arrière-plan). `maintenant` : secondes de l'horloge du relevé.
    pub fn passage(&mut self, lignes: &[String], sys: &dyn Processus, maintenant: f64) {
        let mut vues: HashSet<String> = HashSet::new();
        for l in lignes {
            let x: Vec<&str> = l.split('|').collect();
            if x.len() < 7 {
                continue;
            }
            let pid: u32 = x[0].trim().parse().unwrap_or(0);
            let distante = x[1].to_string();
            let port: i64 = x[2].trim().parse().unwrap_or(0);
            if pid == 0 {
                continue;
            }
            let ck = format!("{pid}|{distante}|{port}|{}", x[3]);
            vues.insert(ck.clone());
            let ak = self.app_de(pid, sys);
            let o: f64 = x[5].trim().parse().unwrap_or(0.0);
            let i: f64 = x[6].trim().parse().unwrap_or(0.0);
            match self.conns.get_mut(&ck) {
                Some(c) => {
                    c.out = c.out.max(o);
                    c.entrant = c.entrant.max(i);
                }
                None => {
                    self.conns.insert(ck, Conn { app: ak, distante, port, out: o, entrant: i });
                }
            }
        }
        // Connexions fermées : leurs octets restent comptés pour le programme et la destination
        let fermees: Vec<String> = self.conns.keys().filter(|k| !vues.contains(*k)).cloned().collect();
        for ck in fermees {
            if let Some(c) = self.conns.shift_remove(&ck)
                && let Some(a) = self.apps.get_mut(&c.app)
            {
                a.out_ferme += c.out;
                a.in_ferme += c.entrant;
                if let Some(d) = a.dest.get_mut(&format!("{}|{}", c.distante, c.port)) {
                    d.out_ferme += c.out;
                    d.in_ferme += c.entrant;
                }
            }
        }
        // Totaux par programme et par destination
        for a in self.apps.values_mut() {
            a.out = a.out_ferme;
            a.entrant = a.in_ferme;
            a.vivantes = 0;
            for d in a.dest.values_mut() {
                d.out = d.out_ferme;
                d.entrant = d.in_ferme;
                d.vivante = false;
            }
        }
        for c in self.conns.values() {
            let Some(a) = self.apps.get_mut(&c.app) else { continue };
            a.out += c.out;
            a.entrant += c.entrant;
            a.vivantes += 1;
            let dk = format!("{}|{}", c.distante, c.port);
            if !a.dest.contains_key(&dk) {
                a.dest.insert(
                    dk.clone(),
                    Dest {
                        distante: c.distante.clone(),
                        port: c.port,
                        premier: maintenant,
                        vivante: true,
                        privee: ip::privee(&c.distante),
                        ..Default::default()
                    },
                );
                a.dest_anciennes.remove(&dk);
            }
            if let Some(d) = a.dest.get_mut(&dk) {
                d.out += c.out;
                d.entrant += c.entrant;
                d.vivante = true;
                d.dernier = Some(maintenant);
            }
        }
        // Débit : octets envoyés depuis le passage d'avant, par seconde (2 s entre deux passages en PowerShell)
        let ecart = self.dernier_passage.map(|t| maintenant - t).filter(|e| *e > 0.5).unwrap_or(2.0);
        self.dernier_passage = Some(maintenant);
        for a in self.apps.values_mut() {
            a.debit = f64::max(0.0, (a.out - a.dernier_out) / ecart);
            a.dernier_out = a.out;
        }
    }

    /// UDP : seulement « ce programme en utilise » (Windows ne donne pas les destinations UDP)
    pub fn udp(&mut self, lignes: &[String], sys: &dyn Processus) {
        for l in lignes {
            let pid: u32 = l.split('|').next().and_then(|x| x.trim().parse().ok()).unwrap_or(0);
            if pid > 4 {
                let k = self.app_de(pid, sys);
                if let Some(a) = self.apps.get_mut(&k) {
                    a.udp = true;
                }
            }
        }
    }

    /// Destinations anciennes : au delà de `max` pour un programme, celles fermées depuis plus de `minutes` sont
    /// oubliées (les plus anciennes d'abord). Une destination sur un port signalé n'est jamais oubliée.
    pub fn compresser(&mut self, maintenant: f64, max: usize, minutes: f64) {
        let limite = maintenant - minutes * 60.0;
        for a in self.apps.values_mut() {
            if a.dest.len() <= max {
                continue;
            }
            let mut vieilles: Vec<(String, f64)> = Vec::new();
            for (k, d) in &a.dest {
                if d.vivante || port_suspect(d.port).is_some() {
                    continue;
                }
                let vu = d.dernier.unwrap_or(d.premier);
                if vu < limite {
                    vieilles.push((k.clone(), vu));
                }
            }
            let n = vieilles.len().min(a.dest.len() - max);
            if n == 0 {
                continue;
            }
            vieilles.sort_by(|x, y| x.1.total_cmp(&y.1));
            for (k, _) in vieilles.into_iter().take(n) {
                if let Some(d) = a.dest.shift_remove(&k)
                    && !d.privee
                {
                    a.dest_anciennes.insert(k);
                    a.ips_anciennes.insert(d.distante);
                }
            }
        }
    }

    /// Le nombre de connexions ouvertes vues au dernier passage
    pub fn nb_connexions(&self) -> usize {
        self.conns.len()
    }

    /// Le nom d'une destination (`Get-DestName`)
    pub fn nom_dest(&self, d: &Dest) -> String {
        if let Some(n) = self.dns.get(&d.distante)
            && !n.is_empty()
        {
            return n.clone();
        }
        if d.privee { "ton réseau local".into() } else { String::new() }
    }
}
