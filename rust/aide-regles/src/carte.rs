//! Relancer la carte réseau de la connexion (`carte.relancer`, `Get-AideRefusCarte` d'aide-actions.ps1) : elle
//! renégocie sa vitesse avec la box (une carte retombée à 100 Mb/s après une veille revient à 1 Gb/s).
//!
//! Seulement une vraie prise réseau du PC (pas une carte virtuelle : VirtualBox, Hyper-V, VPN, Wi-Fi Direct...),
//! filaire (Ethernet, 802.3 ; jamais le Wi-Fi) et qui porte la route par défaut (0.0.0.0/0) : la carte de la
//! connexion, jamais une autre.

use crate::texte;

/// Ce que Windows dit d'une carte réseau (`Get-NetAdapter -IncludeHidden`, `MIB_IF_ROW2` ; `Read-AideFicheCarte`)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FicheCarte {
    /// Sa description (InterfaceDescription : « Intel(R) Ethernet Controller I226-V »)
    pub description: String,
    /// Une carte matérielle (HardwareInterface), pas un logiciel
    pub materielle: bool,
    /// Avec une prise (ConnectorPresent)
    pub connecteur: bool,
    /// Le type de Windows (InterfaceType, IANA : 6 Ethernet, 71 Wi-Fi)
    pub type_carte: u32,
    /// Le support physique (NdisPhysicalMedium : 14 câble 802.3, 9 Wi-Fi, 10 Bluetooth, 0 aucun)
    pub support: u32,
    /// Elle porte une route par défaut IPv4 (0.0.0.0/0) en ce moment
    pub route_defaut: bool,
}

/// Type Ethernet (IF_TYPE_ETHERNET_CSMACD)
pub const TYPE_ETHERNET: u32 = 6;
/// Support câble Ethernet (NdisPhysicalMedium802_3)
pub const SUPPORT_802_3: u32 = 14;

/// Les mots d'une carte virtuelle dans sa description (sans tenir compte des majuscules) : machines virtuelles, VPN,
/// tunnels, cartes de Windows (miniports WAN, débogueur du noyau)
pub const MOTS_VIRTUELS: [&str; 13] = [
    "virtu",
    "vmware",
    "vbox",
    "tap-windows",
    "wintun",
    "wireguard",
    "tunnel",
    "vpn",
    "miniport",
    "debug",
    "loopback",
    "npcap",
    "zerotier",
];

pub const NON_VALABLE: &str = "Demande refusée : carte réseau non valable.";
pub const ABSENTE: &str = "Demande refusée : cette carte réseau n'existe pas sur ce PC.";
pub const VIRTUELLE: &str =
    "Demande refusée : cette carte réseau est virtuelle (VPN, machine virtuelle...), pas une vraie prise du PC.";
pub const PAS_FILAIRE: &str = "Demande refusée : seule une carte réseau filaire (câble Ethernet) peut être relancée.";
pub const SANS_ROUTE: &str = "Demande refusée : cette carte ne porte pas la connexion à Internet.";

/// Pourquoi cette carte ne peut pas être relancée (`None` : elle peut l'être). `fiche` : `None` si elle n'existe pas.
pub fn refus(fiche: Option<&FicheCarte>) -> Option<&'static str> {
    let Some(f) = fiche else { return Some(ABSENTE) };
    if !f.materielle || !f.connecteur || MOTS_VIRTUELS.iter().any(|m| texte::contient(&f.description, m)) {
        return Some(VIRTUELLE);
    }
    if f.type_carte != TYPE_ETHERNET || f.support != SUPPORT_802_3 {
        return Some(PAS_FILAIRE);
    }
    if !f.route_defaut {
        return Some(SANS_ROUTE);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_carte_de_la_connexion_seulement() {
        let bonne = FicheCarte {
            description: "Intel(R) Ethernet Controller I226-V".into(),
            materielle: true,
            connecteur: true,
            type_carte: 6,
            support: 14,
            route_defaut: true,
        };
        assert_eq!(refus(Some(&bonne)), None);
        assert_eq!(refus(None), Some(ABSENTE));
        let wifi = FicheCarte { type_carte: 71, support: 9, ..bonne.clone() };
        assert_eq!(refus(Some(&wifi)), Some(PAS_FILAIRE));
        let vbox = FicheCarte { description: "VirtualBox Host-Only Ethernet Adapter".into(), ..bonne.clone() };
        assert_eq!(refus(Some(&vbox)), Some(VIRTUELLE));
        assert_eq!(refus(Some(&FicheCarte { route_defaut: false, ..bonne })), Some(SANS_ROUTE));
    }
}
