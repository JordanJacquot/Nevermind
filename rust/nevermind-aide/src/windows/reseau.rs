//! Cartes réseau et DNS (Get-NetAdapter, Get-NetRoute, Restart-NetAdapter, Set-DnsClientServerAddress,
//! Clear-DnsClientCache).

use super::com::methode_objet;
use super::outils::{fonction, guid_texte, texte};
use aide_regles::carte::FicheCarte;
use aide_regles::demandes::{Carte, ip_normale};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use windows::Win32::NetworkManagement::IpHelper::{
    ConvertInterfaceIndexToLuid, DNS_INTERFACE_SETTINGS, DNS_INTERFACE_SETTINGS_VERSION1, DNS_SETTING_IPV6,
    DNS_SETTING_NAMESERVER, FreeMibTable, GetIfEntry2, GetIpForwardTable2, IF_TYPE_SOFTWARE_LOOPBACK, MIB_IF_ROW2,
    MIB_IPFORWARD_TABLE2,
};
use windows::Win32::NetworkManagement::Ndis::NET_LUID_LH;
use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR_INET};
use windows::core::{GUID, PWSTR};

/// La ligne de Windows d'une carte réseau (`None` si elle n'existe pas, ou si c'est le bouclage local, qui n'est pas une
/// carte pour Get-NetAdapter)
fn ligne(index: i32) -> Option<MIB_IF_ROW2> {
    let index = u32::try_from(index).ok()?;
    let mut luid = NET_LUID_LH::default();
    // SAFETY : structures remplies par Windows
    unsafe {
        if ConvertInterfaceIndexToLuid(index, &mut luid).is_err() {
            return None;
        }
        let mut l = MIB_IF_ROW2 { InterfaceLuid: luid, ..Default::default() };
        if GetIfEntry2(&mut l).is_err() || l.Type == IF_TYPE_SOFTWARE_LOOPBACK {
            return None;
        }
        Some(l)
    }
}

/// La carte de ce numéro
pub fn carte(index: i32) -> Option<Carte> {
    ligne(index).map(|l| Carte { index, guid: guid_texte(&l.InterfaceGuid) })
}

/// Ce que Windows dit de la carte de ce numéro (`Read-AideFicheCarte`) : sa description, matérielle et avec une prise
/// (InterfaceAndOperStatusFlags : HardwareInterface, ConnectorPresent), son type, son support physique, et si elle porte
/// une route par défaut IPv4 en ce moment
pub fn fiche(index: i32) -> Option<FicheCarte> {
    let l = ligne(index)?;
    let drapeaux = l.InterfaceAndOperStatusFlags._bitfield;
    Some(FicheCarte {
        description: texte(&l.Description),
        materielle: drapeaux & 0b001 != 0,
        connecteur: drapeaux & 0b100 != 0,
        type_carte: l.Type,
        support: u32::try_from(l.PhysicalMediumType.0).unwrap_or(0),
        route_defaut: route_defaut(index),
    })
}

/// La carte porte-t-elle une route par défaut IPv4 (0.0.0.0/0, `Get-NetRoute -PolicyStore ActiveStore`) ?
fn route_defaut(index: i32) -> bool {
    let mut t: *mut MIB_IPFORWARD_TABLE2 = std::ptr::null_mut();
    // SAFETY : table donnée par Windows, lue dans ses limites puis rendue
    unsafe {
        if GetIpForwardTable2(AF_INET, &mut t).is_err() || t.is_null() {
            return false;
        }
        let n = (*t).NumEntries as usize;
        let lignes = std::slice::from_raw_parts((*t).Table.as_ptr(), n);
        let r = lignes
            .iter()
            .any(|l| i64::from(l.InterfaceIndex) == i64::from(index) && l.DestinationPrefix.PrefixLength == 0);
        FreeMibTable(t.cast());
        r
    }
}

/// Relance la carte (MSFT_NetAdapter.Restart, ce que fait Restart-NetAdapter : éteinte puis rallumée), et attend que
/// Windows l'ait fait
pub fn relancer(index: i32) -> Result<(), String> {
    let wql = format!("SELECT * FROM MSFT_NetAdapter WHERE InterfaceIndex = {index}");
    if !methode_objet("root\\StandardCimv2", &wql, "Restart")? {
        return Err("elle n'existe plus sur ce PC".into());
    }
    Ok(())
}

/// Une adresse de Windows (IPv4 ou IPv6)
pub(super) fn adresse(a: &SOCKADDR_INET) -> Option<IpAddr> {
    // SAFETY : on lit le champ qui correspond à la famille annoncée
    unsafe {
        match a.si_family {
            AF_INET => Some(IpAddr::V4(Ipv4Addr::from(a.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes()))),
            AF_INET6 => Some(IpAddr::V6(Ipv6Addr::from(a.Ipv6.sin6_addr.u.Byte))),
            _ => None,
        }
    }
}

/// Les passerelles de cette carte (routes par défaut, 0.0.0.0/0 et ::/0)
pub fn passerelles(index: i32) -> Vec<String> {
    let mut r = Vec::new();
    let mut t: *mut MIB_IPFORWARD_TABLE2 = std::ptr::null_mut();
    // SAFETY : table donnée par Windows, lue dans ses limites puis rendue
    unsafe {
        if GetIpForwardTable2(AF_UNSPEC, &mut t).is_err() || t.is_null() {
            return r;
        }
        let n = (*t).NumEntries as usize;
        let lignes = std::slice::from_raw_parts((*t).Table.as_ptr(), n);
        for l in lignes {
            if i64::from(l.InterfaceIndex) != i64::from(index) || l.DestinationPrefix.PrefixLength != 0 {
                continue;
            }
            if let Some(ip) = adresse(&l.NextHop).and_then(|a| ip_normale(&a.to_string())) {
                r.push(ip);
            }
        }
        FreeMibTable(t.cast());
    }
    r
}

/// Met les serveurs DNS de la carte (aucun : ceux donnés par la box). Seulement les familles d'adresses demandées ;
/// les deux pour revenir à la box.
pub fn appliquer(carte: &Carte, serveurs: &[String]) -> Result<(), String> {
    let l = ligne(carte.index).ok_or("La carte réseau de ce réglage n'existe plus sur ce PC.")?;
    let guid: GUID = l.InterfaceGuid;
    let v4: Vec<&str> = serveurs.iter().filter(|s| s.parse::<Ipv4Addr>().is_ok()).map(String::as_str).collect();
    let v6: Vec<&str> = serveurs.iter().filter(|s| s.parse::<Ipv6Addr>().is_ok()).map(String::as_str).collect();
    let mut a_faire = Vec::new();
    if serveurs.is_empty() || !v4.is_empty() {
        a_faire.push((false, v4.join(",")));
    }
    if serveurs.is_empty() || !v6.is_empty() {
        a_faire.push((true, v6.join(",")));
    }
    // (SetInterfaceDnsSettings : Windows 10 2004 et plus récents, cherchée au moment de s'en servir)
    let f = fonction("iphlpapi.dll", b"SetInterfaceDnsSettings\0").ok_or("Windows trop ancien pour régler les DNS.")?;
    type Fonction = unsafe extern "system" fn(GUID, *const DNS_INTERFACE_SETTINGS) -> u32;
    // SAFETY : signature de SetInterfaceDnsSettings (netioapi.h)
    let regler: Fonction = unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, Fonction>(f) };
    for (ipv6, liste) in a_faire {
        let mut texte: Vec<u16> = liste.encode_utf16().chain(std::iter::once(0)).collect();
        let r = DNS_INTERFACE_SETTINGS {
            Version: DNS_INTERFACE_SETTINGS_VERSION1,
            Flags: u64::from(DNS_SETTING_NAMESERVER | if ipv6 { DNS_SETTING_IPV6 } else { 0 }),
            NameServer: PWSTR(texte.as_mut_ptr()),
            ..Default::default()
        };
        // SAFETY : structure et texte valables pendant l'appel
        let e = unsafe { regler(guid, &r) };
        if e != 0 {
            return Err(format!("Windows a refusé de changer les DNS (erreur {e})."));
        }
    }
    Ok(())
}

/// Vide le cache DNS de Windows (erreurs ignorées)
pub fn vider_cache() {
    if let Some(f) = fonction("dnsapi.dll", b"DnsFlushResolverCache\0") {
        type Fonction = unsafe extern "system" fn() -> i32;
        // SAFETY : signature de DnsFlushResolverCache (BOOL WINAPI, sans paramètre)
        unsafe {
            let vider: Fonction = std::mem::transmute::<unsafe extern "system" fn() -> isize, Fonction>(f);
            vider();
        }
    }
}
