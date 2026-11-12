//! Tout ce que le bloc lit ou fait sur le PC et le réseau passe par une `Sonde` : la vraie (module `windows`) dans
//! l'appli, une fausse dans les tests (aucun ping, aucun scan, rien sur le vrai PC).

use crate::appareil::{Fabricants, Locaux};
use crate::net::Connexion;
use crate::scan::{BrutApprofondi, BrutScan};
use serde_json::Value;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

/// La progression d'un travail en cours (`[OGNative]::Progress`, `Phase`, `Found`, `Cancel`)
#[derive(Default)]
pub struct Avancement {
    progression: Mutex<f64>,
    phase: Mutex<String>,
    pub trouves: AtomicI64,
    pub annuler: AtomicBool,
}

impl Avancement {
    pub fn mettre(&self, p: f64) {
        *self.progression.lock().unwrap_or_else(|e| e.into_inner()) = p;
    }
    pub fn progression(&self) -> f64 {
        *self.progression.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn phase(&self, p: &str) {
        *self.phase.lock().unwrap_or_else(|e| e.into_inner()) = p.to_string();
    }
    pub fn phase_actuelle(&self) -> String {
        self.phase.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn trouves(&self) -> i64 {
        self.trouves.load(Ordering::Relaxed)
    }
}

/// Le PC et le réseau, vus par le bloc
pub trait Sonde: Send + Sync {
    /// La connexion active (`Get-ActiveNet`)
    fn connexion(&self) -> Option<Connexion>;
    /// Les adresses de ce PC sur toutes ses cartes (`Get-LocalInterfaces`)
    fn locaux(&self) -> Locaux;
    /// Le nom de ce PC (`$env:COMPUTERNAME`)
    fn poste(&self) -> String;
    /// Le scan rapide (`$NetScanWork`) : ping de toutes les adresses, voisins de Windows, noms
    fn scan(&self, ips: &[String], if_index: i64, av: &Avancement) -> Result<BrutScan, String>;
    /// La recherche approfondie (`$NetDeepWork`)
    fn approfondi(
        &self,
        ip: &str,
        if_index: i64,
        manquants: &[String],
        trouves: &[String],
        av: &Avancement,
    ) -> Result<BrutApprofondi, String>;
    /// L'audit (`$NetAuditWork`) : la table de ce qui a été relevé
    fn audit(
        &self,
        ip: &str,
        passerelle: &str,
        if_index: i64,
        ips: &[String],
        ports: &[i64],
        av: &Avancement,
    ) -> Result<Value, String>;
    /// Les ports TCP ouverts d'un appareil (`ScanPorts`)
    fn ports(&self, ip: &str, ports: &[i64], delai_ms: u32) -> Vec<i64>;
    /// Un ping (`None` : pas de réponse)
    fn ping(&self, hote: &str, delai_ms: u32) -> Option<f64>;
    /// Les serveurs DNS fixés à la main pour cette carte (registre, `NameServer`)
    fn dns_fixes(&self, guid: &str) -> String;
    /// Vider le cache DNS (`Clear-DnsClientCache`)
    fn vider_cache_dns(&self) -> Result<(), String>;
    /// Attendre que la carte revienne après son redémarrage (une vingtaine de secondes au plus)
    fn attendre_carte(&self, index: i64) -> bool;
    /// La liste des fabricants (fabricants.txt, téléchargée chez l'IEEE la première fois)
    fn fabricants(&self, chemin: &Path) -> Option<Fabricants>;
    /// Passer le réseau en public (`Set-NetConnectionProfile`)
    ///
    /// Lot J (suite, garde-fous) : sans le permis du verrou des gestes, ça ne compile pas :
    /// ```compile_fail
    /// fn public(s: &dyn nevermind_reseau::sonde::Sonde) {
    ///     let _ = s.passer_public(3);
    /// }
    /// ```
    fn passer_public(&self, index: i64) -> Result<(), String>;
    /// L'heure locale : année, mois, jour, heure, minute
    fn maintenant(&self) -> [u32; 5] {
        heure_utc()
    }
}

/// L'heure UTC (quand Windows ne donne pas l'heure locale)
pub fn heure_utc() -> [u32; 5] {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (j, r) = ((s / 86400) as i64, s % 86400);
    let z = j + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let a = (yoe + era * 400 + i64::from(m <= 2)) as u32;
    [a, m, d, (r / 3600) as u32, ((r % 3600) / 60) as u32]
}

/// Une sonde qui ne voit rien (hors Windows)
pub struct SondeVide;

impl Sonde for SondeVide {
    fn connexion(&self) -> Option<Connexion> {
        None
    }
    fn locaux(&self) -> Locaux {
        Vec::new()
    }
    fn poste(&self) -> String {
        std::env::var("COMPUTERNAME").unwrap_or_default()
    }
    fn scan(&self, _: &[String], _: i64, _: &Avancement) -> Result<BrutScan, String> {
        Err("pas de réseau".into())
    }
    fn approfondi(
        &self,
        _: &str,
        _: i64,
        _: &[String],
        _: &[String],
        _: &Avancement,
    ) -> Result<BrutApprofondi, String> {
        Err("pas de réseau".into())
    }
    fn audit(&self, _: &str, _: &str, _: i64, _: &[String], _: &[i64], _: &Avancement) -> Result<Value, String> {
        Err("pas de réseau".into())
    }
    fn ports(&self, _: &str, _: &[i64], _: u32) -> Vec<i64> {
        Vec::new()
    }
    fn ping(&self, _: &str, _: u32) -> Option<f64> {
        None
    }
    fn dns_fixes(&self, _: &str) -> String {
        String::new()
    }
    fn vider_cache_dns(&self) -> Result<(), String> {
        Ok(())
    }
    fn attendre_carte(&self, _: i64) -> bool {
        false
    }
    fn fabricants(&self, chemin: &Path) -> Option<Fabricants> {
        std::fs::read_to_string(chemin).ok().map(|t| crate::appareil::lire_fabricants(&t))
    }
    fn passer_public(&self, _: i64) -> Result<(), String> {
        Err("pas de réseau".into())
    }
}
