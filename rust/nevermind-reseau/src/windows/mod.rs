//! La vraie sonde : le seul module qui parle à Windows (et le seul où le code « unsafe » est permis).
//! Lectures sans droits (cartes, adresses, routes, voisins, registre en lecture), ping (ICMP de Windows), et le réseau
//! par les sockets de la bibliothèque standard (ports TCP, SSDP/UPnP, NetBIOS, pages web en http).
//! Les programmes lancés (netsh, curl) le sont sans fenêtre (CREATE_NO_WINDOW).

#![allow(unsafe_code)]

mod lectures;
mod reseau;

use crate::appareil::{Fabricants, InterfaceLocale, Locaux, lire_fabricants};
use crate::net::Connexion;
use crate::scan::{BrutApprofondi, BrutScan};
use crate::sonde::{Avancement, Sonde};
use serde_json::Value;
use std::path::Path;

pub struct SondeWindows;

/// « 1 Gbps », « 866.7 Mbps » (la vitesse comme Get-NetAdapter l'écrit)
fn texte_vitesse(bps: u64) -> String {
    let b = bps as f64;
    let (v, u) = if b >= 1e9 {
        (b / 1e9, "Gbps")
    } else if b >= 1e6 {
        (b / 1e6, "Mbps")
    } else {
        (b / 1e3, "Kbps")
    };
    let s = format!("{:.1}", v);
    format!("{} {u}", s.trim_end_matches('0').trim_end_matches('.'))
}

impl Sonde for SondeWindows {
    fn connexion(&self) -> Option<Connexion> {
        let (index, passerelle) = lectures::route_defaut()?;
        let a = lectures::adaptateurs().into_iter().find(|a| a.index == index)?;
        let wifi = a.wifi || crate::texte::re("Wi-?Fi|Wireless|WLAN").is_match(&a.desc);
        let ip = a.ipv4.iter().find(|(i, _)| !i.starts_with("169.254")).cloned();
        let (vitesses, reg) = if wifi { (Vec::new(), Vec::new()) } else { lectures::vitesses_carte(&a.guid) };
        Some(Connexion {
            if_index: i64::from(index),
            gateway: passerelle,
            nom: a.nom.clone(),
            desc: a.desc.clone(),
            speed: texte_vitesse(a.rx),
            wifi,
            guid: a.guid.clone(),
            mac: a.mac.clone(),
            prefix: ip.as_ref().map(|(_, p)| i64::from(*p)).unwrap_or(24),
            ip: ip.map(|(i, _)| i),
            dns: a.dns.join(", "),
            bps: a.rx as f64,
            vitesses,
            vitesses_reg: reg,
        })
    }

    fn locaux(&self) -> Locaux {
        let mut r = Vec::new();
        for a in lectures::adaptateurs() {
            if a.bouclage {
                continue;
            }
            for (ip, _) in &a.ipv4 {
                r.push((
                    ip.clone(),
                    InterfaceLocale {
                        mac: a.mac.clone(),
                        nom: a.nom.clone(),
                        desc: a.desc.clone(),
                        wifi: a.wifi,
                        active: a.actif,
                    },
                ));
            }
        }
        r
    }

    fn poste(&self) -> String {
        std::env::var("COMPUTERNAME").unwrap_or_default()
    }

    fn scan(&self, ips: &[String], if_index: i64, av: &Avancement) -> Result<BrutScan, String> {
        av.phase("ping");
        let vivants = reseau::balayage(ips, 800, av);
        if av.annuler.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("annulé".into());
        }
        av.phase("arp");
        av.mettre(72.0);
        std::thread::sleep(std::time::Duration::from_millis(300));
        let arp: Vec<String> = lectures::voisins(false, if_index as u32)
            .into_iter()
            .filter(|(ip, mac, etat)| {
                ips.iter().any(|x| x == ip)
                    && !["Unreachable", "Incomplete", "Permanent"].contains(&etat.as_str())
                    && !mac.is_empty()
                    && !(mac == "00-00-00-00-00-00" || mac == "FF-FF-FF-FF-FF-FF" || mac.starts_with("01-00-5E"))
            })
            .map(|(ip, mac, etat)| format!("{ip}|{mac}|{etat}"))
            .collect();
        av.phase("names");
        av.mettre(80.0);
        let mut toutes: Vec<String> = vivants.iter().map(|l| l.split('|').next().unwrap_or("").to_string()).collect();
        for l in &arp {
            let ip = l.split('|').next().unwrap_or("").to_string();
            if !toutes.contains(&ip) {
                toutes.push(ip);
            }
        }
        let noms = reseau::noms(&toutes, 2500);
        av.phase("vendors");
        av.mettre(100.0);
        Ok(BrutScan { vivants, arp, noms })
    }

    fn approfondi(
        &self,
        ip: &str,
        if_index: i64,
        manquants: &[String],
        trouves: &[String],
        av: &Avancement,
    ) -> Result<BrutApprofondi, String> {
        av.phase("deep-arp");
        av.mettre(5.0);
        let arp = reseau::arp_direct(manquants, 64);
        av.phase("deep-listen");
        av.mettre(30.0);
        let upnp = reseau::upnp(ip);
        av.mettre(50.0);
        let v6: Vec<String> = lectures::voisins(true, if_index as u32)
            .into_iter()
            .filter(|(a, mac, etat)| {
                ["Reachable", "Stale", "Delay", "Probe"].contains(&etat.as_str())
                    && !mac.is_empty()
                    && mac != "00-00-00-00-00-00"
                    && mac != "FF-FF-FF-FF-FF-FF"
                    && !mac.starts_with("33-33-")
                    && !a.to_lowercase().starts_with("ff")
            })
            .map(|(a, mac, _)| format!("{a}|{mac}"))
            .collect();
        av.phase("deep-names");
        av.mettre(55.0);
        let mut toutes: Vec<String> = trouves.to_vec();
        for l in &arp {
            let a = l.split('|').next().unwrap_or("").to_string();
            if !a.is_empty() && !toutes.contains(&a) {
                toutes.push(a);
            }
        }
        let netbios = reseau::netbios(&toutes, 1500);
        av.phase("deep-web");
        let ports = reseau::ports_hotes(
            &toutes,
            &[80, 443, 8080, 8443, 554, 8000, 37777, 9100, 631, 5000],
            600,
            av,
            60.0,
            85.0,
        );
        let urls: Vec<String> = ports
            .iter()
            .filter_map(|l| {
                let (h, p) = l.split_once('|')?;
                matches!(p, "80" | "8080" | "5000").then(|| format!("http://{h}:{p}"))
            })
            .collect();
        let titres = reseau::titres(&urls, 3000);
        av.mettre(100.0);
        Ok(BrutApprofondi { arp, mdns: Vec::new(), wsd: Vec::new(), upnp, v6, netbios, ports, titres })
    }

    fn audit(
        &self,
        ip: &str,
        passerelle: &str,
        if_index: i64,
        ips: &[String],
        ports: &[i64],
        av: &Avancement,
    ) -> Result<Value, String> {
        Ok(reseau::audit(ip, passerelle, if_index, ips, ports, av))
    }

    fn ports(&self, ip: &str, ports: &[i64], delai_ms: u32) -> Vec<i64> {
        let l: Vec<String> = reseau::ports_hotes(&[ip.to_string()], ports, delai_ms, &Avancement::default(), 0.0, 0.0);
        l.iter().filter_map(|x| x.split_once('|').and_then(|(_, p)| p.parse().ok())).collect()
    }

    fn ping(&self, hote: &str, delai_ms: u32) -> Option<f64> {
        lectures::ping(hote, delai_ms).map(|(ms, _)| f64::from(ms))
    }

    fn dns_fixes(&self, guid: &str) -> String {
        lectures::registre_texte(
            &format!(r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{guid}"),
            "NameServer",
        )
        .unwrap_or_default()
    }

    fn vider_cache_dns(&self) -> Result<(), String> {
        lectures::vider_cache_dns();
        Ok(())
    }

    fn attendre_carte(&self, index: i64) -> bool {
        std::thread::sleep(std::time::Duration::from_secs(2));
        for _ in 0..20 {
            if let Some(a) = lectures::adaptateurs().into_iter().find(|a| i64::from(a.index) == index)
                && a.actif
                && a.rx > 0
                && lectures::route_defaut().is_some_and(|(i, _)| i64::from(i) == index)
            {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        false
    }

    fn fabricants(&self, chemin: &Path) -> Option<Fabricants> {
        // (un fichier vide ou incomplet, laissé par une ancienne version après une page d'erreur : retéléchargé)
        if !std::fs::read_to_string(chemin).is_ok_and(|t| crate::appareil::fabricants_complets(&t)) {
            reseau::telecharger_fabricants(chemin);
        }
        std::fs::read_to_string(chemin).ok().map(|t| lire_fabricants(&t))
    }

    fn passer_public(&self, index: i64) -> Result<(), String> {
        reseau::passer_public(index)
    }

    fn maintenant(&self) -> [u32; 5] {
        lectures::heure_locale()
    }
}
