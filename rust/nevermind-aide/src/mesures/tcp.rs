//! Les connexions TCP avec les octets envoyés et reçus de chacune (`TrafficMon.Tcp`, natif.cs) : Windows ne compte
//! qu'à la demande d'un administrateur, d'où « trafic.tcp » dans l'Aide. Le comptage est demandé une fois par
//! connexion, et oublié quand elle se ferme.

use super::ip;
use crate::systeme::{LigneTcp, Systeme};
use std::collections::HashSet;
use std::fmt::Write;

/// État d'une connexion en écoute, et d'une connexion établie
const EN_ECOUTE: u32 = 2;
const ETABLIE: u32 = 5;

/// Les compteurs de l'Aide
#[derive(Default)]
pub struct CompteursTcp {
    /// Connexions dont Windows compte les octets
    comptees: HashSet<String>,
    /// Windows a déjà accepté de compter (faux tant qu'il refuse : l'Aide sans droits administrateur). Ne redevient
    /// jamais faux, comme dans la version PowerShell.
    pub ok: bool,
}

/// Un port tel que Windows le range (octets du réseau dans un entier) en nombre
pub fn port(p: u32) -> u32 {
    ((p & 0xFF) << 8) | ((p >> 8) & 0xFF)
}

/// Les octets en « 0A-1B-... » (`BitConverter.ToString`)
fn octets_texte(o: &[u8]) -> String {
    let mut s = String::new();
    for (i, x) in o.iter().enumerate() {
        if i > 0 {
            s.push('-');
        }
        let _ = write!(s, "{x:02X}");
    }
    s
}

/// L'identité d'une connexion pour le comptage (adresses et ports des deux côtés)
fn cle(l: &LigneTcp) -> String {
    if l.v6 {
        format!(
            "6|{}:{}|{}:{}",
            octets_texte(&l.adresse_locale),
            l.port_local,
            octets_texte(&l.adresse_distante),
            l.port_distant
        )
    } else {
        let a = |o: &[u8; 16]| u32::from_le_bytes([o[0], o[1], o[2], o[3]]);
        format!("4|{}:{}|{}:{}", a(&l.adresse_locale), l.port_local, a(&l.adresse_distante), l.port_distant)
    }
}

impl CompteursTcp {
    /// Octets envoyés et reçus d'une connexion établie, depuis que le comptage a été demandé pour elle
    fn octets(&mut self, s: &dyn Systeme, cle: &str, l: &LigneTcp) -> (u64, u64) {
        if !self.comptees.contains(cle) && s.tcp_compter(l) {
            self.comptees.insert(cle.to_string());
            self.ok = true;
        }
        if self.comptees.contains(cle) { s.tcp_octets(l).unwrap_or((0, 0)) } else { (0, 0) }
    }

    /// Connexions TCP ouvertes : « pid|adresse distante|port distant|port local|état|octets envoyés|octets reçus »
    pub fn lignes(&mut self, s: &dyn Systeme) -> Vec<String> {
        let mut r = Vec::new();
        let mut vivantes = HashSet::new();
        for v6 in [false, true] {
            for l in s.table_tcp(v6).unwrap_or_default() {
                // (en écoute, ou sans destination)
                let vide =
                    if v6 { l.adresse_distante.iter().all(|&x| x == 0) } else { l.adresse_distante[..4] == [0; 4] };
                if l.etat == EN_ECOUTE || vide {
                    continue;
                }
                let distante = if !v6 {
                    ip::ipv4(&l.adresse_distante)
                } else if ip::ipv4_dans_ipv6(&l.adresse_distante) {
                    ip::ipv4(&l.adresse_distante[12..16])
                } else {
                    ip::ipv6(&l.adresse_distante)
                };
                let c = cle(&l);
                let (o, i) = if l.etat == ETABLIE { self.octets(s, &c, &l) } else { (0, 0) };
                vivantes.insert(c);
                r.push(format!(
                    "{}|{distante}|{}|{}|{}|{o}|{i}",
                    l.pid,
                    port(l.port_distant),
                    port(l.port_local),
                    l.etat
                ));
            }
        }
        // (oublie les connexions fermées)
        self.comptees.retain(|c| vivantes.contains(c));
        r
    }
}
