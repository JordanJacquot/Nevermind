//! Câble, Wi-Fi ou rien (`Get-ConnexionVue`, vue.ps1:382-394) : une carte réseau active, ni boucle locale ni
//! tunnel, avec une passerelle ; le câble l'emporte sur le Wi-Fi. Lecture seule (`GetAdaptersAddresses`).

use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR};
use windows::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
    GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR_IN, SOCKADDR_IN6};

/// `NetworkInterfaceType` de .NET : Wireless80211, Loopback, Tunnel.
const WIFI: u32 = 71;
const BOUCLE: u32 = 24;
const TUNNEL: u32 = 131;

/// « cable », « wifi » ou « aucune ».
pub fn connexion() -> String {
    let drapeaux =
        GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut taille: u32 = 16 * 1024;
    let mut tampon: Vec<u64> = Vec::new();
    for _ in 0..4 {
        tampon = vec![0u64; (taille as usize).div_ceil(8)];
        // SAFETY : tampon de `taille` octets (aligné sur 8), qui vit pendant l'appel ; lecture seule de Windows
        let r = unsafe {
            GetAdaptersAddresses(AF_UNSPEC.0 as u32, drapeaux, None, Some(tampon.as_mut_ptr().cast()), &mut taille)
        };
        if r == NO_ERROR.0 {
            break;
        }
        if r != ERROR_BUFFER_OVERFLOW.0 {
            return "aucune".into();
        }
        tampon.clear();
    }
    if tampon.is_empty() {
        return "aucune".into();
    }
    let mut v = "aucune";
    let mut a = tampon.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !a.is_null() {
        // SAFETY : la liste chaînée écrite par Windows dans `tampon`, qui vit jusqu'à la fin de la fonction
        let c = unsafe { &*a };
        a = c.Next;
        if c.OperStatus != IfOperStatusUp || c.IfType == BOUCLE || c.IfType == TUNNEL {
            continue;
        }
        let mut passerelle = false;
        let mut g = c.FirstGatewayAddress;
        while !g.is_null() {
            // SAFETY : élément de la liste des passerelles, dans `tampon`
            let p = unsafe { &*g };
            g = p.Next;
            let s = p.Address.lpSockaddr;
            if s.is_null() {
                continue;
            }
            // SAFETY : une adresse de socket de Windows, de la famille indiquée
            let vraie = unsafe {
                match (*s).sa_family {
                    AF_INET => (*(s as *const SOCKADDR_IN)).sin_addr.S_un.S_addr != 0,
                    AF_INET6 => (*(s as *const SOCKADDR_IN6)).sin6_addr.u.Byte.iter().any(|o| *o != 0),
                    _ => false,
                }
            };
            passerelle |= vraie;
        }
        if !passerelle {
            continue;
        }
        if c.IfType == WIFI {
            v = "wifi";
        } else {
            return "cable".into();
        }
    }
    v.into()
}

#[cfg(test)]
mod tests {
    #[test]
    fn une_des_trois_reponses() {
        let c = super::connexion();
        assert!(["cable", "wifi", "aucune"].contains(&c.as_str()), "{c}");
    }
}
