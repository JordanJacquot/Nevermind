//! Ce que le choix de la connexion active ([`crate::route::choisir_route`]) demande à Windows, par l'IP Helper (sans
//! WMI, sans droits, rien n'est changé) : le coût et l'état de chaque interface IPv4, l'état de chaque carte.

use crate::route::{EtatCarte, InterfaceIp};
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetIfTable2, GetIpInterfaceTable, MIB_IF_TABLE2, MIB_IPINTERFACE_TABLE,
};
use windows::Win32::NetworkManagement::Ndis::{MediaConnectStateDisconnected, NET_IF_ADMIN_STATUS_DOWN};
use windows::Win32::Networking::WinSock::AF_INET;

/// Les interfaces IPv4 (GetIpInterfaceTable : InterfaceMetric, Connected) et les cartes (GetIfTable2 : débranchée,
/// désactivée, vraie carte du PC). Une table illisible donne une liste vide : rien n'est alors écarté.
pub fn etats_ipv4() -> (Vec<InterfaceIp>, Vec<EtatCarte>) {
    let mut interfaces = Vec::new();
    let mut cartes = Vec::new();
    // SAFETY : tables données par Windows, lues dans leurs limites puis rendues
    unsafe {
        let mut t: *mut MIB_IPINTERFACE_TABLE = std::ptr::null_mut();
        if GetIpInterfaceTable(AF_INET, &mut t).is_ok() && !t.is_null() {
            for l in std::slice::from_raw_parts((*t).Table.as_ptr(), (*t).NumEntries as usize) {
                interfaces.push(InterfaceIp {
                    index: i64::from(l.InterfaceIndex),
                    metrique: i64::from(l.Metric),
                    connectee: l.Connected,
                });
            }
            FreeMibTable(t.cast());
        }
        let mut t: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut t).is_ok() && !t.is_null() {
            for l in std::slice::from_raw_parts((*t).Table.as_ptr(), (*t).NumEntries as usize) {
                cartes.push(EtatCarte {
                    index: i64::from(l.InterfaceIndex),
                    connectee: l.MediaConnectState != MediaConnectStateDisconnected
                        && l.AdminStatus != NET_IF_ADMIN_STATUS_DOWN,
                    // (premier bit d'InterfaceAndOperStatusFlags : HardwareInterface)
                    materielle: l.InterfaceAndOperStatusFlags._bitfield & 1 != 0,
                });
            }
            FreeMibTable(t.cast());
        }
    }
    (interfaces, cartes)
}
