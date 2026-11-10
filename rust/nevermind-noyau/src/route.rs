//! La connexion active : quelle route par défaut IPv4 Windows prend vraiment pour aller sur Internet. Le choix seul,
//! sans lecture (testé partout) ; les lectures viennent de WMI (Ton PC) ou de l'IP Helper ([`crate::windows::route`],
//! Connexion et Trafic). Avant, chaque endroit prenait la première route au plus petit RouteMetric : une carte Wi-Fi
//! débranchée qui garde sa route, au même coût que le câble, pouvait passer pour la connexion (PC de Jordan, 08/11).

/// Une route par défaut IPv4, 0.0.0.0/0 (MSFT_NetRoute ou GetIpForwardTable2)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteDefaut {
    pub index: i64,
    pub metrique: i64,
    /// Dans la table en service (Store 1, ActiveStore) ; faux pour une route seulement enregistrée (PersistentStore)
    pub active: bool,
}

/// Une interface IPv4 (MSFT_NetIPInterface AddressFamily 2, ou GetIpInterfaceTable)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceIp {
    pub index: i64,
    /// Le coût de l'interface (InterfaceMetric), ajouté à celui de la route comme le fait Windows
    pub metrique: i64,
    /// ConnectionState 1 (Connected)
    pub connectee: bool,
}

/// Une carte réseau (MSFT_NetAdapter, ou GetIfTable2)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EtatCarte {
    pub index: i64,
    /// Ni débranchée (MediaConnectState 2), ni désactivée (State 3)
    pub connectee: bool,
    /// Une vraie carte du PC (HardwareInterface) ; faux pour un VPN, VirtualBox, Hyper-V
    pub materielle: bool,
    /// Un tunnel (VPN, PPP) : voir [`est_tunnel`]
    pub tunnel: bool,
}

/// Un tunnel d'après Windows : son type (IfType, ou InterfaceType de MSFT_NetAdapter) est PPP (23), « virtuel
/// propriétaire » (53 : WireGuard, la plupart des VPN récents) ou tunnel (131), ou il a un type de tunnel (TunnelType).
/// Une carte virtuelle Ethernet (Hyper-V vEthernet, Pont réseau, VirtualBox) n'en est pas un.
pub fn est_tunnel(if_type: i64, tunnel_type: i64) -> bool {
    matches!(if_type, 23 | 53 | 131) || tunnel_type != 0
}

/// La carte de la connexion active : parmi les routes par défaut en service dont la carte est connectée, pas un tunnel
/// d'abord (un VPN seulement s'il n'y a que lui), puis le plus petit coût, route + interface, comme Windows ; à égalité,
/// une vraie carte du PC, puis le plus petit numéro de carte. Une carte virtuelle qui n'est pas un tunnel passe devant
/// une vraie carte plus chère : c'est elle que Windows prend (Hyper-V vEthernet d'un commutateur externe ou Pont réseau
/// sur le câble, à côté d'un Wi-Fi connecté : second audit du 09/11). Une interface ou une carte que Windows n'a pas
/// décrite n'est pas écartée (lecture ratée) : seul un « débranché » lu l'écarte, et une carte non décrite passe après
/// les autres. `None` : aucune route utilisable.
pub fn choisir_route(routes: &[RouteDefaut], interfaces: &[InterfaceIp], cartes: &[EtatCarte]) -> Option<i64> {
    routes
        .iter()
        .filter(|r| r.active)
        .filter_map(|r| {
            let ip = interfaces.iter().find(|i| i.index == r.index);
            let carte = cartes.iter().find(|c| c.index == r.index);
            if ip.is_some_and(|i| !i.connectee) || carte.is_some_and(|c| !c.connectee) {
                return None;
            }
            let tunnel = carte.is_none_or(|c| c.tunnel);
            let virtuelle = !carte.is_some_and(|c| c.materielle);
            Some((tunnel, r.metrique.saturating_add(ip.map_or(0, |i| i.metrique)), virtuelle, r.index))
        })
        .min()
        .map(|(_, _, _, index)| index)
}

#[cfg(test)]
mod tests {
    use super::{EtatCarte, InterfaceIp, RouteDefaut, choisir_route, est_tunnel};

    fn route(index: i64, metrique: i64) -> RouteDefaut {
        RouteDefaut { index, metrique, active: true }
    }
    fn ip(index: i64, metrique: i64, connectee: bool) -> InterfaceIp {
        InterfaceIp { index, metrique, connectee }
    }
    fn carte(index: i64, connectee: bool, materielle: bool) -> EtatCarte {
        EtatCarte { index, connectee, materielle, tunnel: false }
    }
    /// Un VPN connecté (WireGuard : IfType 53)
    fn vpn(index: i64) -> EtatCarte {
        EtatCarte { index, connectee: true, materielle: false, tunnel: est_tunnel(53, 0) }
    }

    /// Le PC de Jordan le 08/11 : Wi-Fi (12) débranché, lu en premier, route au coût 0 comme le câble (6). Avant, la
    /// première route au plus petit RouteMetric gagnait : « Connexion en Wi-Fi » alors qu'il est en câble.
    #[test]
    fn jordan_cable_et_wifi_debranche() {
        let routes = [route(12, 0), route(6, 0)];
        let ips = [
            ip(13, 25, true),
            ip(21, 65, false),
            ip(10, 25, false),
            ip(6, 25, true),
            ip(18, 25, false),
            ip(12, 65, false),
            ip(1, 75, true),
        ];
        let cartes = [carte(21, false, false), carte(13, true, false), carte(12, false, true), carte(6, true, true)];
        assert_eq!(choisir_route(&routes, &ips, &cartes), Some(6));
    }

    /// Le Wi-Fi débranché seulement vu par la carte (l'interface IP se dit encore connectée) : écarté quand même
    #[test]
    fn carte_debranchee_ecartee_meme_si_interface_connectee() {
        let routes = [route(12, 0), route(6, 0)];
        let ips = [ip(12, 65, true), ip(6, 25, true)];
        let cartes = [carte(12, false, true), carte(6, true, true)];
        assert_eq!(choisir_route(&routes, &ips, &cartes), Some(6));
    }

    #[test]
    fn une_seule_carte() {
        assert_eq!(choisir_route(&[route(6, 0)], &[ip(6, 25, true)], &[carte(6, true, true)]), Some(6));
    }

    /// Câble et Wi-Fi branchés tous les deux : le plus petit coût route + interface, comme Windows
    #[test]
    fn deux_cartes_connectees() {
        let routes = [route(12, 0), route(6, 0)];
        let ips = [ip(12, 65, true), ip(6, 25, true)];
        let cartes = [carte(12, true, true), carte(6, true, true)];
        assert_eq!(choisir_route(&routes, &ips, &cartes), Some(6));
        // Le coût de la route compte aussi : 0 + 65 contre 50 + 25
        let routes = [route(12, 0), route(6, 50)];
        assert_eq!(choisir_route(&routes, &ips, &cartes), Some(12));
    }

    /// Même coût total : le plus petit numéro de carte, quel que soit l'ordre de lecture
    #[test]
    fn egalite_departagee_sans_dependre_de_l_ordre() {
        let ips = [ip(7, 25, true), ip(4, 25, true)];
        let cartes = [carte(7, true, true), carte(4, true, true)];
        assert_eq!(choisir_route(&[route(7, 0), route(4, 0)], &ips, &cartes), Some(4));
        assert_eq!(choisir_route(&[route(4, 0), route(7, 0)], &ips, &cartes), Some(4));
    }

    #[test]
    fn aucune_route() {
        assert_eq!(choisir_route(&[], &[ip(6, 25, true)], &[carte(6, true, true)]), None);
    }

    /// Toutes les cartes à routes débranchées : pas de connexion (plutôt qu'une carte au hasard)
    #[test]
    fn toutes_debranchees() {
        assert_eq!(choisir_route(&[route(12, 0)], &[ip(12, 65, false)], &[carte(12, false, true)]), None);
    }

    #[test]
    fn wifi_seul() {
        let routes = [route(12, 0)];
        let ips = [ip(12, 50, true), ip(6, 25, false)];
        let cartes = [carte(12, true, true), carte(6, false, true)];
        assert_eq!(choisir_route(&routes, &ips, &cartes), Some(12));
    }

    /// Un VPN (un tunnel), ou une carte que Windows n'a pas décrite, avec une route moins chère ne passe pas devant la
    /// vraie carte ; s'il n'y a que lui, il reste la connexion
    #[test]
    fn vpn_ne_gagne_pas() {
        let routes = [route(30, 0), route(13, 0), route(6, 0)];
        let ips = [ip(30, 1, true), ip(13, 5, true), ip(6, 25, true)];
        let cartes = [vpn(13), carte(6, true, true)];
        assert_eq!(choisir_route(&routes, &ips, &cartes), Some(6));
        assert_eq!(choisir_route(&[route(13, 0)], &ips, &cartes), Some(13));
        // (les types de Windows : PPP, virtuel propriétaire, tunnel, ou un type de tunnel ; pas Ethernet ni Wi-Fi)
        assert!(est_tunnel(23, 0) && est_tunnel(53, 0) && est_tunnel(131, 0) && est_tunnel(6, 2));
        assert!(!est_tunnel(6, 0) && !est_tunnel(71, 0));
    }

    /// Second audit du 09/11 : un commutateur externe Hyper-V (ou un Pont réseau) sur le câble, et le Wi-Fi connecté.
    /// Windows sort par la carte vEthernet (coût 15 contre 55) : c'est la connexion, même si elle n'est pas une carte
    /// matérielle. Avant, toute carte non matérielle passait après le Wi-Fi : « Connexion en Wi-Fi ».
    #[test]
    fn carte_virtuelle_qui_porte_la_connexion() {
        let routes = [route(12, 0), route(25, 0)];
        let ips = [ip(12, 55, true), ip(25, 15, true)];
        let cartes = [carte(12, true, true), carte(25, true, false)];
        assert_eq!(choisir_route(&routes, &ips, &cartes), Some(25));
        // À coût égal, la vraie carte du PC
        let ips = [ip(12, 15, true), ip(25, 15, true)];
        assert_eq!(choisir_route(&routes, &ips, &cartes), Some(12));
    }

    /// Une route seulement enregistrée (PersistentStore) ne compte pas ; une carte ou une interface non décrite
    /// (lecture ratée) n'est pas écartée
    #[test]
    fn route_enregistree_et_lectures_manquantes() {
        let routes = [RouteDefaut { index: 6, metrique: 0, active: false }, route(12, 0)];
        let cartes = [carte(6, true, true), carte(12, true, true)];
        assert_eq!(choisir_route(&routes, &[], &cartes), Some(12));
        assert_eq!(choisir_route(&[route(6, 0)], &[], &[]), Some(6));
    }
}
