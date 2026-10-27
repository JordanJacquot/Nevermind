//! Les échanges réseau par programme, en temps réel (classe `NetFlow` de natif.cs) : la trace de Windows
//! (fournisseur « Kernel-Network », UDP et TCP) donne les vraies destinations UDP, que Windows ne liste nulle part
//! ailleurs. Pendant « Lag en ligne », Nevermind y cherche le vrai serveur de la partie. Demande les droits
//! administrateur.
//!
//! Les dates des événements sont en heure système (FILETIME, 100 ns) : Windows les convertit pour le lecteur en
//! temps réel. L'âge du dernier paquet se compte donc dans la même horloge (`Systeme::filetime`), jamais avec un
//! compteur QPC (bug du 15/10 : des âges de -13 milliards de ms, et le serveur du jeu jamais retenu).

use super::ip;
use crate::systeme::{EvenementReseau, RefusTrace, Systeme, Trace};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::{Arc, Mutex, MutexGuard};

/// Échanges gardés au plus (au delà, plus aucun n'est ajouté)
const ECHANGES_MAX: usize = 20000;
/// Au delà de 3 s sans paquet reçu, c'est une pause (menu, chargement), pas un à-coup en pleine partie
const PAUSE_MS: f64 = 3000.0;

/// Un échange : un programme, une autre machine, un port
#[derive(Clone, Debug, Default)]
struct Echange {
    pid: i32,
    proto: &'static str,
    ip: String,
    port: i32,
    paquets_envoyes: i64,
    paquets_recus: i64,
    octets_envoyes: i64,
    octets_recus: i64,
    dernier_recu: i64,
    dernier: i64,
    trous_100: i32,
    trous_250: i32,
    plus_long_trou: f64,
}

/// Une table qui rend ses éléments dans l'ordre d'un `Dictionary` de .NET Framework : l'ordre d'ajout, sauf qu'une
/// place libérée (élément retiré) est reprise par le prochain ajout, la dernière libérée d'abord. Nevermind garde le
/// premier serveur trouvé à égalité : le même ordre que la version PowerShell donne le même serveur.
#[derive(Default)]
struct TableDotNet {
    places: Vec<Option<(String, Echange)>>,
    libres: Vec<usize>,
    index: HashMap<String, usize>,
}

impl TableDotNet {
    fn len(&self) -> usize {
        self.index.len()
    }

    fn vider(&mut self) {
        *self = TableDotNet::default();
    }

    fn get_mut(&mut self, cle: &str) -> Option<&mut Echange> {
        let i = *self.index.get(cle)?;
        self.places[i].as_mut().map(|x| &mut x.1)
    }

    fn ajouter(&mut self, cle: String, e: Echange) -> &mut Echange {
        let i = match self.libres.pop() {
            Some(i) => i,
            None => {
                self.places.push(None);
                self.places.len() - 1
            }
        };
        self.index.insert(cle.clone(), i);
        &mut self.places[i].insert((cle, e)).1
    }

    /// Retire les éléments qui ne passent pas `garder` (dans l'ordre des places, comme .NET)
    fn retirer_si(&mut self, garder: impl Fn(&Echange) -> bool) {
        for i in 0..self.places.len() {
            let retirer = matches!(&self.places[i], Some((_, e)) if !garder(e));
            if retirer && let Some((cle, _)) = self.places[i].take() {
                self.index.remove(&cle);
                self.libres.push(i);
            }
        }
    }

    fn valeurs(&self) -> impl Iterator<Item = &Echange> {
        self.places.iter().filter_map(|x| x.as_ref().map(|x| &x.1))
    }
}

#[derive(Default)]
struct Echanges {
    table: TableDotNet,
    /// Les adresses de ce PC, écrites comme celles des événements
    locales: HashSet<String>,
}

/// La trace réseau de l'Aide (une seule à la fois)
#[derive(Clone, Default)]
pub struct Flux {
    trace: Arc<Mutex<Option<Box<dyn Trace>>>>,
    echanges: Arc<Mutex<Echanges>>,
}

/// La raison d'un refus, telle que la version PowerShell l'écrit (`NetFlow.LastError`)
pub fn raison(r: &RefusTrace) -> String {
    match r {
        RefusTrace::Session(5) => "accès refusé (droits administrateur, ou anti-triche d'un jeu en cours)".into(),
        RefusTrace::Session(e) => format!("StartTrace {e}"),
        RefusTrace::Fournisseur(e) => format!("EnableTrace {e}"),
        RefusTrace::Lecture(e) => format!("OpenTrace {e}"),
    }
}

/// Une adresse IP en texte, comme .NET l'écrit (sans zone)
pub fn adresse(a: &IpAddr) -> String {
    match a {
        IpAddr::V4(x) => ip::ipv4(&x.octets()),
        IpAddr::V6(x) => ip::ipv6(&x.octets()),
    }
}

fn entier(d: &[u8], i: usize) -> i32 {
    i32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]])
}

fn port(d: &[u8], i: usize) -> i32 {
    (i32::from(d[i]) << 8) | i32::from(d[i + 1])
}

impl Flux {
    fn trace(&self) -> MutexGuard<'_, Option<Box<dyn Trace>>> {
        self.trace.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn echanges(&self) -> MutexGuard<'_, Echanges> {
        self.echanges.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// La trace tourne-t-elle ?
    pub fn tourne(&self) -> bool {
        self.trace().as_ref().is_some_and(|t| t.tourne())
    }

    /// Démarre la trace (rien à faire si elle tourne déjà) ; sinon, la raison du refus de Windows
    pub fn demarrer(&self, s: &dyn Systeme) -> Result<(), String> {
        let mut t = self.trace();
        if t.as_ref().is_some_and(|x| x.tourne()) {
            return Ok(());
        }
        // (une trace arrêtée toute seule : sa lecture est refermée avant d'en ouvrir une autre)
        if let Some(mut vieille) = t.take() {
            vieille.arreter();
        }
        {
            let mut x = self.echanges();
            x.table.vider();
            x.locales = s.adresses_locales().iter().map(adresse).collect();
        }
        let flux = self.clone();
        *t = Some(s.demarrer_trace(Arc::new(move |ev: &EvenementReseau| flux.evenement(ev))).map_err(|r| raison(&r))?);
        Ok(())
    }

    /// Arrête la trace
    pub fn arreter(&self) {
        let t = self.trace().take();
        if let Some(mut t) = t {
            t.arreter();
        }
    }

    /// Un événement de la trace
    pub fn evenement(&self, ev: &EvenementReseau) {
        let (v6, envoi, udp) = match ev.id {
            10 => (false, true, false),
            11 => (false, false, false),
            26 => (true, true, false),
            27 => (true, false, false),
            42 => (false, true, true),
            43 => (false, false, true),
            58 => (true, true, true),
            59 => (true, false, true),
            _ => return,
        };
        let Some(d) = ev.donnees else { return };
        if d.len() < if v6 { 44 } else { 20 } {
            return;
        }
        let pid = entier(d, 0);
        let taille = entier(d, 4);
        // daddr, saddr, dport, sport : l'autre machine est celle dont l'adresse n'est pas à ce PC
        let lire_v6 = |i: usize| {
            let mut o = [0u8; 16];
            o.copy_from_slice(&d[i..i + 16]);
            ip::ipv6(&o)
        };
        let (da, sa) = if v6 { (lire_v6(8), lire_v6(24)) } else { (ip::ipv4(&d[8..12]), ip::ipv4(&d[12..16])) };
        let (dp, sp) = if v6 { (port(d, 40), port(d, 42)) } else { (port(d, 16), port(d, 18)) };
        let mut x = self.echanges();
        let d_locale = x.locales.contains(&da);
        let (ip, port) = if d_locale { (sa, sp) } else { (da, dp) };
        let proto = if udp { "udp" } else { "tcp" };
        let cle = format!("{pid}|{proto}|{ip}|{port}");
        let ts = ev.date;
        let plein = x.table.len() > ECHANGES_MAX;
        let f = match x.table.get_mut(&cle) {
            Some(f) => f,
            None if plein => return,
            None => x.table.ajouter(cle, Echange { pid, proto, ip, port, ..Default::default() }),
        };
        if envoi {
            f.paquets_envoyes += 1;
            f.octets_envoyes += i64::from(taille);
        } else {
            if f.dernier_recu != 0 {
                // (dates en heure système, FILETIME : 100 ns)
                let trou = ts.wrapping_sub(f.dernier_recu) as f64 / 10000.0;
                if trou < PAUSE_MS {
                    if trou > 100.0 {
                        f.trous_100 += 1;
                    }
                    if trou > 250.0 {
                        f.trous_250 += 1;
                    }
                    if trou > f.plus_long_trou {
                        f.plus_long_trou = trou;
                    }
                }
            }
            f.paquets_recus += 1;
            f.octets_recus += i64::from(taille);
            f.dernier_recu = ts;
        }
        f.dernier = ts;
    }

    /// Veille avant un jeu (« flux.veiller ») : la trace tourne longtemps avant la partie, les échanges sans paquet
    /// depuis `age_max_s` secondes sont oubliés
    pub fn oublier(&self, s: &dyn Systeme, age_max_s: i64) {
        let limite = s.filetime() - age_max_s * 10_000_000;
        self.echanges().table.retirer_si(|e| e.dernier >= limite);
    }

    /// « pid|proto|ip|port|paquets envoyés|paquets reçus|octets envoyés|octets reçus|trous>100ms|trous>250ms|plus long
    /// trou ms|ms depuis le dernier paquet »
    pub fn lignes(&self, s: &dyn Systeme) -> Vec<String> {
        let maintenant = s.filetime();
        let f = 1.0 / 10000.0;
        let x = self.echanges();
        x.table
            .valeurs()
            .map(|e| {
                format!(
                    "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                    e.pid,
                    e.proto,
                    e.ip,
                    e.port,
                    e.paquets_envoyes,
                    e.paquets_recus,
                    e.octets_envoyes,
                    e.octets_recus,
                    e.trous_100,
                    e.trous_250,
                    e.plus_long_trou as i32,
                    (maintenant.wrapping_sub(e.dernier) as f64 * f) as i64
                )
            })
            .collect()
    }
}
