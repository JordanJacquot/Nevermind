//! Les pings de la mesure du lag (classe `[LagMon]` de natif.cs) : un fil par cible, un ping toutes les 500 ms,
//! chaque mesure gardée [t, ms] (ms -1 = perdu). Une cible abandonnée (ancien serveur, box d'un autre joueur)
//! n'est plus pingée, ses mesures restent lisibles. Tout s'arrête avec la mesure : rien ne tourne en dehors.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Ce qui envoie un ping (Windows, ou un faux dans les tests)
pub trait Pingeur: Send + Sync {
    /// La durée en ms, `None` si perdu
    fn ping(&self, hote: &str) -> Option<f64>;
    /// Le chemin vers un hôte : « étape|adresse|ms »
    fn trace(&self, hote: &str, max: u8, delai_ms: u32) -> Vec<String>;
}

/// Les cibles pingées
pub trait Pings: Send {
    fn ajouter(&mut self, hote: &str) -> usize;
    fn retirer(&mut self, i: usize);
    /// [t0, ping0, t1, ping1...]
    fn serie(&self, i: usize) -> Vec<f64>;
    fn arreter(&mut self);
    /// L'hôte d'une cible
    fn hote(&self, i: usize) -> String;
}

struct Cible {
    hote: String,
    mesures: Arc<Mutex<Vec<f64>>>,
    off: Arc<AtomicBool>,
    fil: Option<std::thread::JoinHandle<()>>,
}

/// Les pings par Windows, un fil par cible
pub struct Moniteur {
    pingeur: Arc<dyn Pingeur>,
    cibles: Vec<Cible>,
    debut: Instant,
    marche: Arc<AtomicBool>,
    intervalle: Duration,
}

impl Moniteur {
    pub fn nouveau(pingeur: Arc<dyn Pingeur>, intervalle_ms: u64) -> Moniteur {
        Moniteur {
            pingeur,
            cibles: Vec::new(),
            debut: Instant::now(),
            marche: Arc::new(AtomicBool::new(true)),
            intervalle: Duration::from_millis(intervalle_ms),
        }
    }
}

impl Pings for Moniteur {
    fn ajouter(&mut self, hote: &str) -> usize {
        let mesures = Arc::new(Mutex::new(Vec::new()));
        let off = Arc::new(AtomicBool::new(false));
        let (m, o, marche, p, h, debut, iv) = (
            mesures.clone(),
            off.clone(),
            self.marche.clone(),
            self.pingeur.clone(),
            hote.to_string(),
            self.debut,
            self.intervalle,
        );
        let fil = std::thread::Builder::new()
            .name("nevermind-ping".into())
            .spawn(move || {
                while marche.load(Ordering::Relaxed) && !o.load(Ordering::Relaxed) {
                    let t0 = debut.elapsed();
                    let rtt = p.ping(&h).unwrap_or(-1.0);
                    if let Ok(mut v) = m.lock() {
                        v.push(t0.as_secs_f64());
                        v.push(rtt);
                    }
                    let passe = debut.elapsed().saturating_sub(t0);
                    if let Some(reste) = iv.checked_sub(passe) {
                        std::thread::sleep(reste);
                    }
                }
            })
            .ok();
        self.cibles.push(Cible { hote: hote.into(), mesures, off, fil });
        self.cibles.len() - 1
    }

    fn retirer(&mut self, i: usize) {
        if let Some(c) = self.cibles.get(i) {
            c.off.store(true, Ordering::Relaxed);
        }
    }

    fn serie(&self, i: usize) -> Vec<f64> {
        self.cibles.get(i).and_then(|c| c.mesures.lock().ok().map(|v| v.clone())).unwrap_or_default()
    }

    fn arreter(&mut self) {
        self.marche.store(false, Ordering::Relaxed);
        for c in &mut self.cibles {
            if let Some(f) = c.fil.take() {
                // (au plus un ping en cours, 1 s : le fil s'arrête de lui-même)
                let _ = f.join();
            }
        }
    }

    fn hote(&self, i: usize) -> String {
        self.cibles.get(i).map(|c| c.hote.clone()).unwrap_or_default()
    }
}

impl Drop for Moniteur {
    fn drop(&mut self) {
        self.marche.store(false, Ordering::Relaxed);
    }
}

/// Le pingeur de Windows (ICMP, 1 s au plus par ping)
pub struct PingWindows;

impl Pingeur for PingWindows {
    fn ping(&self, hote: &str) -> Option<f64> {
        crate::windows::ping(hote, 1000)
    }
    fn trace(&self, hote: &str, max: u8, delai_ms: u32) -> Vec<String> {
        crate::windows::trace(hote, max, delai_ms)
    }
}
