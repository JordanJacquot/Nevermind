//! Les mesures en direct (le fil $LiveScript d'onglets.ps1), en bien plus léger.
//!
//! Avant (PowerShell, le 29/10) : toutes les secondes, cinq requêtes WMI (dont GPUEngine : près de 600 compteurs, 0,1 s
//! par lecture) et nvidia-smi.exe lancé à chaque tour, même pendant une partie. Ici :
//! - processeur : GetSystemTimes (un appel, les mêmes valeurs brutes que « % temps processeur ») ;
//! - mémoire : GlobalMemoryStatusEx (les mêmes nombres que Win32_OperatingSystem) ;
//! - carte graphique : le temps de travail de chaque moteur (D3DKMTQueryStatistics, ce que lit le Gestionnaire des
//!   tâches) au lieu des 600 compteurs par programme ; le moteur le plus occupé, comme avant ;
//! - NVIDIA : nvml.dll, chargée une fois (température, mémoire, puissance), au lieu de lancer nvidia-smi ;
//! - vitesse du processeur et disque : deux compteurs PDH ouverts une fois.
//!
//! Et seulement ce qui sert (`Besoin`) : l'écran regardé, tout chaque seconde ; l'écran caché, la température seule
//! toutes les 10 s (l'alerte), rien d'autre ; pendant une partie mesurée, un relevé complet toutes les 5 s quand le jeu
//! est devant (le diagnostic), rien quand il ne l'est pas.

use crate::systeme::Systeme;
use serde_json::{Value, json};

/// Ce qu'il faut lire à ce tour
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Besoin {
    pub processeur: bool,
    pub memoire: bool,
    pub moteurs_gpu: bool,
    pub nvidia: bool,
    pub compteurs: bool,
}

impl Besoin {
    pub const TOUT: Besoin =
        Besoin { processeur: true, memoire: true, moteurs_gpu: true, nvidia: true, compteurs: true };
    pub const TEMPERATURE: Besoin =
        Besoin { processeur: false, memoire: false, moteurs_gpu: false, nvidia: true, compteurs: false };
}

/// Les dernières mesures ($Live)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Direct {
    pub cpu: Option<f64>,
    pub cpu_perf: Option<f64>,
    pub ram_totale: Option<f64>,
    pub ram_utilisee: Option<f64>,
    pub gpu: Option<f64>,
    pub gpu_temp: Option<f64>,
    pub vram_utilisee: Option<f64>,
    pub vram_totale: Option<f64>,
    pub gpu_puissance: Option<f64>,
    pub gpu_puissance_max: Option<f64>,
    pub disque: Option<f64>,
    /// Au moins une mesure a été faite
    pub fait: bool,
}

impl Direct {
    pub fn json(&self) -> Value {
        json!({ "Cpu": self.cpu, "CpuPerf": self.cpu_perf, "RamTotal": self.ram_totale, "RamUsed": self.ram_utilisee, "Gpu": self.gpu, "GpuTemp": self.gpu_temp,
            "VramUsed": self.vram_utilisee, "VramTotal": self.vram_totale, "GpuPower": self.gpu_puissance, "GpuPowerLimit": self.gpu_puissance_max, "DiskBusy": self.disque, "Updated": self.fait })
    }
    /// Le relevé pour le diagnostic d'une partie (Add-FpsSysSample)
    pub fn releve(&self) -> crate::diag::Releve {
        let pct = |a: Option<f64>, b: Option<f64>| match (a, b) {
            (Some(a), Some(b)) if b != 0.0 => Some(100.0 * a / b),
            _ => None,
        };
        crate::diag::Releve {
            cpu: self.cpu,
            perf: self.cpu_perf,
            ram: pct(self.ram_utilisee, self.ram_totale),
            gpu: self.gpu,
            temp: self.gpu_temp,
            vram: pct(self.vram_utilisee, self.vram_totale),
            power: pct(self.gpu_puissance, self.gpu_puissance_max),
            disk: self.disque,
        }
    }
}

/// Le calcul entre deux lectures (les valeurs brutes, comme le Gestionnaire des tâches)
#[derive(Debug, Default)]
pub struct Mesures {
    cpu_avant: Option<(u64, u64)>,
    gpu_avant: Option<(Vec<i64>, i64)>,
    pub direct: Direct,
}

/// Le processeur occupé entre deux lectures de GetSystemTimes (0 à 100)
pub fn cpu_entre(avant: (u64, u64), apres: (u64, u64)) -> Option<f64> {
    let dt = apres.1 as f64 - avant.1 as f64;
    if dt <= 0.0 {
        return None;
    }
    let di = apres.0 as f64 - avant.0 as f64;
    Some((100.0 - 100.0 * di / dt).clamp(0.0, 100.0))
}

/// Le moteur de carte graphique le plus occupé entre deux lectures (0 à 100) ; `None` si les moteurs ont changé
pub fn gpu_entre(avant: &(Vec<i64>, i64), apres: &(Vec<i64>, i64)) -> Option<f64> {
    let dt = (apres.1 - avant.1) as f64;
    if dt <= 0.0 || avant.0.len() != apres.0.len() || apres.0.is_empty() {
        return None;
    }
    let mut haut: f64 = 0.0;
    for (a, b) in avant.0.iter().zip(&apres.0) {
        let x = 100.0 * (b - a) as f64 / dt;
        if x > haut {
            haut = x;
        }
    }
    Some(haut.min(100.0))
}

impl Mesures {
    /// Une lecture de ce qui est demandé ; ce qui n'est pas lu garde sa dernière valeur (comme $Live)
    pub fn lire(&mut self, s: &mut dyn Systeme, b: Besoin) {
        if b.processeur
            && let Some(t) = s.temps_processeur()
        {
            if let Some(a) = self.cpu_avant
                && let Some(c) = cpu_entre(a, t)
            {
                self.direct.cpu = Some(c);
            }
            self.cpu_avant = Some(t);
        }
        if b.memoire
            && let Some((tot, used)) = s.memoire()
        {
            self.direct.ram_totale = Some(tot);
            self.direct.ram_utilisee = Some(used);
        }
        let mut gpu_moteurs = false;
        if b.moteurs_gpu {
            if let Some(m) = s.moteurs_gpu() {
                if let Some(a) = &self.gpu_avant
                    && let Some(g) = gpu_entre(a, &m)
                {
                    self.direct.gpu = Some(g);
                    gpu_moteurs = true;
                }
                self.gpu_avant = Some(m);
            }
        } else {
            // (comme la version PowerShell : la mesure d'après repart de zéro)
            self.gpu_avant = None;
        }
        if b.nvidia
            && let Some(n) = s.nvidia()
        {
            if !gpu_moteurs && b.moteurs_gpu && n.utilisation.is_some() {
                self.direct.gpu = n.utilisation;
            }
            self.direct.gpu_temp = n.temperature;
            if b.moteurs_gpu || b.memoire {
                self.direct.vram_utilisee = n.vram_utilisee;
                self.direct.vram_totale = n.vram_totale;
                self.direct.gpu_puissance = n.puissance;
                self.direct.gpu_puissance_max = n.puissance_max;
            }
        }
        if b.compteurs {
            let (perf, disque) = s.compteurs();
            if perf.is_some() {
                self.direct.cpu_perf = perf;
            }
            if let Some(d) = disque {
                self.direct.disque = Some(d.clamp(0.0, 100.0));
            }
        }
        if b != Besoin::default() {
            self.direct.fait = true;
        }
    }
}

#[cfg(test)]
mod essais {
    use super::*;
    use crate::systeme::{FauxSysteme, Nvidia};

    #[test]
    fn processeur_et_carte_comme_le_gestionnaire() {
        assert_eq!(cpu_entre((100, 1000), (150, 2000)), Some(95.0));
        assert_eq!(cpu_entre((100, 1000), (1100, 2000)), Some(0.0));
        assert_eq!(cpu_entre((100, 1000), (100, 1000)), None);
        assert_eq!(gpu_entre(&(vec![0, 10], 0), &(vec![50, 20], 100)), Some(50.0));
        assert_eq!(gpu_entre(&(vec![0], 0), &(vec![500], 100)), Some(100.0));
        assert_eq!(gpu_entre(&(vec![0], 0), &(vec![5, 6], 100)), None);
    }

    #[test]
    fn ecran_cache_seule_la_temperature() {
        let mut s = FauxSysteme {
            cpu: Some((1, 2)),
            ram: Some((16.0, 8.0)),
            nv: Some(Nvidia { temperature: Some(70.0), ..Default::default() }),
            ..Default::default()
        };
        let mut m = Mesures::default();
        m.lire(&mut s, Besoin::TEMPERATURE);
        assert_eq!(s.n, 1, "une seule lecture : la carte NVIDIA");
        assert_eq!(m.direct.gpu_temp, Some(70.0));
        assert_eq!(m.direct.ram_totale, None);
        m.lire(&mut s, Besoin::TOUT);
        assert_eq!(s.n, 1 + 5);
    }
}
