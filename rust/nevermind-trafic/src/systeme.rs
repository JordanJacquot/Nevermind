//! Ce que le bloc demande à Windows, derrière un trait : le vrai Windows (`SystemeWindows`, module `windows`), ou un
//! faux dans les tests (jamais le vrai réseau, jamais le vrai PC).

use crate::lag::pings::{PingWindows, Pingeur};
use crate::lag::session::Depart;
use crate::microsoft::LecturesWindows;
use crate::trafic::etat::Service;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub trait Systeme: LecturesWindows + Send + Sync {
    /// Les connexions TCP (« pid|adresse|port distant|port local|état|octets envoyés|octets reçus ») et si Windows
    /// compte les octets
    fn tcp(&self) -> (Vec<String>, bool);
    /// Les ports UDP ouverts (« pid|port »)
    fn udp(&self) -> Vec<String>;
    /// Les processus : numéro, parent, nom sans « .exe »
    fn processus(&self) -> Vec<(u32, u32, String)>;
    fn chemin(&self, pid: u32) -> String;
    /// Les services en marche, par processus
    fn services(&self) -> HashMap<u32, Vec<Service>>;
    /// Le cache DNS de Windows : (adresse, nom)
    fn cache_dns(&self) -> Vec<(String, String)>;
    /// (état, éditeur)
    fn signature(&self, chemin: &str) -> (String, String);
    fn description(&self, chemin: &str) -> String;
    fn reseau(&self) -> Option<Depart>;
    fn pingeur(&self) -> Arc<dyn Pingeur>;
    fn wifi(&self) -> Vec<String>;
    /// GET (annuaires publics, seulement si l'utilisateur l'a permis)
    fn http(&self, url: &str, accepte: &str) -> Result<String, String>;
    fn nom_inverse(&self, ip: &str) -> String;
}

/// Le vrai Windows (lectures sans droits)
#[derive(Default)]
pub struct SystemeWindows {
    tcp: Mutex<crate::windows::CompteursTcp>,
}

impl LecturesWindows for SystemeWindows {
    fn reg_nombre(&self, chemin: &str, nom: &str) -> Option<i64> {
        crate::windows::reg_nombre(chemin, nom)
    }
    fn reg_texte(&self, chemin: &str, nom: &str) -> String {
        crate::windows::reg_texte(chemin, nom)
    }
    fn service(&self, nom: &str) -> Option<(bool, String)> {
        crate::windows::etat_service(nom)
    }
    fn compte_microsoft(&self) -> bool {
        crate::windows::compte_microsoft()
    }
}

impl Systeme for SystemeWindows {
    fn tcp(&self) -> (Vec<String>, bool) {
        let mut c = self.tcp.lock().unwrap_or_else(|e| e.into_inner());
        let l = c.lignes();
        (l, c.ok)
    }
    fn udp(&self) -> Vec<String> {
        crate::windows::udp()
    }
    fn processus(&self) -> Vec<(u32, u32, String)> {
        crate::windows::processus()
    }
    fn chemin(&self, pid: u32) -> String {
        crate::windows::chemin(pid)
    }
    fn services(&self) -> HashMap<u32, Vec<Service>> {
        crate::windows::services()
    }
    fn cache_dns(&self) -> Vec<(String, String)> {
        crate::windows::cache_dns()
    }
    fn signature(&self, chemin: &str) -> (String, String) {
        crate::windows::signature(chemin)
    }
    fn description(&self, chemin: &str) -> String {
        crate::windows::description(chemin)
    }
    fn reseau(&self) -> Option<Depart> {
        crate::windows::reseau_actif().map(|r| Depart {
            passerelle: r.passerelle,
            wifi: r.wifi,
            carte: format!("{} {}", r.nom, r.desc),
        })
    }
    fn pingeur(&self) -> Arc<dyn Pingeur> {
        Arc::new(PingWindows)
    }
    fn wifi(&self) -> Vec<String> {
        crate::windows::wifi()
    }
    fn http(&self, url: &str, accepte: &str) -> Result<String, String> {
        crate::windows::http_get(url, accepte, false)
    }
    fn nom_inverse(&self, ip: &str) -> String {
        crate::windows::nom_inverse(ip)
    }
}
