//! Les appels à Windows : cartes réseau (GetAdaptersAddresses), route par défaut (GetIpForwardTable2), voisins
//! (GetIpNetTable2), ping (IcmpSendEcho), demande d'adresse physique (SendARP), nom d'une adresse (GetNameInfoW),
//! registre en lecture, heure locale, texte OEM de netsh.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS, HANDLE};
use windows::Win32::Globalization::{CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS, MultiByteToWideChar};
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GAA_FLAG_INCLUDE_GATEWAYS, GetAdaptersAddresses, GetIpForwardTable2, GetIpNetTable2, ICMP_ECHO_REPLY,
    IF_TYPE_IEEE80211, IF_TYPE_SOFTWARE_LOOPBACK, IP_ADAPTER_ADDRESSES_LH, IcmpCloseHandle, IcmpCreateFile,
    IcmpSendEcho, MIB_IPFORWARD_TABLE2, MIB_IPNET_TABLE2, SendARP,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::{
    AF_INET, AF_INET6, AF_UNSPEC, GetNameInfoW, IN_ADDR, IN_ADDR_0, NI_NAMEREQD, SOCKADDR, SOCKADDR_IN, SOCKADDR_INET,
    SOCKET_ADDRESS, socklen_t,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_VALUE_TYPE, RRF_RT_ANY, RegCloseKey, RegEnumKeyExW, RegEnumValueW,
    RegGetValueW, RegOpenKeyExW,
};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::core::{HSTRING, PCWSTR, PWSTR};

/// Une carte réseau
pub struct Adaptateur {
    pub index: u32,
    pub guid: String,
    pub nom: String,
    pub desc: String,
    pub mac: String,
    pub wifi: bool,
    pub actif: bool,
    pub bouclage: bool,
    pub ipv4: Vec<(String, u8)>,
    pub dns: Vec<String>,
    pub rx: u64,
}

fn pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY : texte terminé par un zéro, donné par Windows
    unsafe { p.to_string().unwrap_or_default() }
}

fn adresse_socket(a: &SOCKET_ADDRESS) -> Option<IpAddr> {
    if a.lpSockaddr.is_null() {
        return None;
    }
    // SAFETY : adresse de la famille annoncée, dans la mémoire de GetAdaptersAddresses
    unsafe {
        let f = (*a.lpSockaddr).sa_family;
        if f == AF_INET {
            let s = &*(a.lpSockaddr as *const SOCKADDR_IN);
            Some(IpAddr::V4(Ipv4Addr::from(s.sin_addr.S_un.S_addr.to_ne_bytes())))
        } else if f == AF_INET6 {
            let s = &*(a.lpSockaddr as *const windows::Win32::Networking::WinSock::SOCKADDR_IN6);
            Some(IpAddr::V6(Ipv6Addr::from(s.sin6_addr.u.Byte)))
        } else {
            None
        }
    }
}

fn adresse_inet(a: &SOCKADDR_INET) -> Option<IpAddr> {
    // SAFETY : on lit le champ de la famille annoncée
    unsafe {
        match a.si_family {
            AF_INET => Some(IpAddr::V4(Ipv4Addr::from(a.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes()))),
            AF_INET6 => Some(IpAddr::V6(Ipv6Addr::from(a.Ipv6.sin6_addr.u.Byte))),
            _ => None,
        }
    }
}

fn mac(o: &[u8]) -> String {
    o.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join("-")
}

/// Toutes les cartes réseau
pub fn adaptateurs() -> Vec<Adaptateur> {
    let mut r = Vec::new();
    let mut taille: u32 = 32 * 1024;
    let mut tampon: Vec<u64> = Vec::new();
    for _ in 0..4 {
        tampon = vec![0u64; (taille as usize).div_ceil(8)];
        // SAFETY : tampon de la taille annoncée, aligné sur 8 octets
        let e = unsafe {
            GetAdaptersAddresses(
                u32::from(AF_UNSPEC.0),
                GAA_FLAG_INCLUDE_GATEWAYS,
                None,
                Some(tampon.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>()),
                &mut taille,
            )
        };
        if e == ERROR_BUFFER_OVERFLOW.0 {
            continue;
        }
        if e != ERROR_SUCCESS.0 {
            return r;
        }
        break;
    }
    let mut p = tampon.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    // SAFETY : liste chaînée remplie par Windows, lue tant que le tampon vit
    unsafe {
        while !p.is_null() {
            let a = &*p;
            let mut ipv4 = Vec::new();
            let mut u = a.FirstUnicastAddress;
            while !u.is_null() {
                if let Some(IpAddr::V4(v)) = adresse_socket(&(*u).Address) {
                    ipv4.push((v.to_string(), (*u).OnLinkPrefixLength));
                }
                u = (*u).Next;
            }
            let mut dns = Vec::new();
            let mut d = a.FirstDnsServerAddress;
            while !d.is_null() {
                if let Some(IpAddr::V4(v)) = adresse_socket(&(*d).Address) {
                    dns.push(v.to_string());
                }
                d = (*d).Next;
            }
            let n = (a.PhysicalAddressLength as usize).min(8);
            let guid =
                if a.AdapterName.is_null() { String::new() } else { a.AdapterName.to_string().unwrap_or_default() };
            r.push(Adaptateur {
                index: a.Anonymous1.Anonymous.IfIndex,
                guid,
                nom: pwstr(a.FriendlyName),
                desc: pwstr(a.Description),
                mac: if n == 6 { mac(&a.PhysicalAddress[..n]) } else { String::new() },
                wifi: a.IfType == IF_TYPE_IEEE80211,
                actif: a.OperStatus == IfOperStatusUp,
                bouclage: a.IfType == IF_TYPE_SOFTWARE_LOOPBACK,
                ipv4,
                dns,
                rx: a.ReceiveLinkSpeed,
            });
            p = a.Next;
        }
    }
    r
}

/// La route par défaut IPv4 que Windows prend vraiment : numéro de la carte et passerelle. Avant, la première au plus
/// petit coût de route : une carte Wi-Fi débranchée qui garde sa route au même coût que le câble pouvait gagner (PC
/// de Jordan, 08/11 : il n'était choisi juste que parce que Windows donnait le câble en premier).
pub fn route_defaut() -> Option<(u32, String)> {
    let mut t: *mut MIB_IPFORWARD_TABLE2 = std::ptr::null_mut();
    // SAFETY : table donnée par Windows, lue dans ses limites puis rendue
    let routes: Vec<(u32, u32, String)> = unsafe {
        if GetIpForwardTable2(AF_INET, &mut t).is_err() || t.is_null() {
            return None;
        }
        let lignes = std::slice::from_raw_parts((*t).Table.as_ptr(), (*t).NumEntries as usize);
        let r = lignes
            .iter()
            .filter(|l| l.DestinationPrefix.PrefixLength == 0)
            .map(|l| (l.InterfaceIndex, l.Metric, adresse_inet(&l.NextHop).map(|a| a.to_string()).unwrap_or_default()))
            .collect();
        FreeMibTable(t.cast());
        r
    };
    let (interfaces, cartes) = nevermind_noyau::windows::route::etats_ipv4();
    route_choisie(&routes, &interfaces, &cartes)
}

/// Le choix parmi les routes par défaut (carte, coût de la route, passerelle) lues dans la table de Windows (celle
/// en service : GetIpForwardTable2 ne donne que l'ActiveStore)
fn route_choisie(
    routes: &[(u32, u32, String)],
    interfaces: &[nevermind_noyau::route::InterfaceIp],
    cartes: &[nevermind_noyau::route::EtatCarte],
) -> Option<(u32, String)> {
    use nevermind_noyau::route::{RouteDefaut, choisir_route};
    let defauts: Vec<RouteDefaut> = routes
        .iter()
        .map(|(i, m, _)| RouteDefaut { index: i64::from(*i), metrique: i64::from(*m), active: true })
        .collect();
    let index = choisir_route(&defauts, interfaces, cartes)?;
    // (deux passerelles sur cette carte : la moins chère, celle que Windows prend, pas la première lue)
    routes.iter().filter(|(i, _, _)| i64::from(*i) == index).min_by_key(|(_, m, _)| *m).map(|(i, _, p)| (*i, p.clone()))
}

fn etat(e: i32) -> &'static str {
    match e {
        0 => "Unreachable",
        1 => "Incomplete",
        2 => "Probe",
        3 => "Delay",
        4 => "Stale",
        5 => "Reachable",
        6 => "Permanent",
        _ => "",
    }
}

/// Les voisins de Windows sur une carte (Get-NetNeighbor) : adresse, adresse physique, état
pub fn voisins(v6: bool, index: u32) -> Vec<(String, String, String)> {
    let mut r = Vec::new();
    let mut t: *mut MIB_IPNET_TABLE2 = std::ptr::null_mut();
    // SAFETY : table donnée par Windows, lue dans ses limites puis rendue
    unsafe {
        if GetIpNetTable2(if v6 { AF_INET6 } else { AF_INET }, &mut t).is_err() || t.is_null() {
            return r;
        }
        let lignes = std::slice::from_raw_parts((*t).Table.as_ptr(), (*t).NumEntries as usize);
        for l in lignes {
            if l.InterfaceIndex != index {
                continue;
            }
            let Some(a) = adresse_inet(&l.Address) else { continue };
            let n = (l.PhysicalAddressLength as usize).min(32);
            r.push((
                a.to_string(),
                if n > 0 { mac(&l.PhysicalAddress[..n]) } else { String::new() },
                etat(l.State.0).to_string(),
            ));
        }
        FreeMibTable(t.cast());
    }
    r
}

/// Un ping IPv4 (IcmpSendEcho) : temps de réponse et TTL
pub fn ping(hote: &str, delai_ms: u32) -> Option<(u32, u8)> {
    let ip: Ipv4Addr = hote.trim().parse().ok()?;
    let dest = u32::from_ne_bytes(ip.octets());
    let donnees = [0x61u8; 32];
    let mut rep = vec![0u8; std::mem::size_of::<ICMP_ECHO_REPLY>() + 64];
    // SAFETY : poignée ICMP fermée après l'appel ; tampon de réponse assez grand
    unsafe {
        let h: HANDLE = IcmpCreateFile().ok()?;
        let n = IcmpSendEcho(
            h,
            dest,
            donnees.as_ptr().cast(),
            32,
            None,
            rep.as_mut_ptr().cast(),
            rep.len() as u32,
            delai_ms,
        );
        let _ = IcmpCloseHandle(h);
        if n == 0 {
            return None;
        }
        let r = &*(rep.as_ptr() as *const ICMP_ECHO_REPLY);
        (r.Status == 0).then_some((r.RoundTripTime, r.Options.Ttl))
    }
}

/// La demande directe d'adresse physique (SendARP)
pub fn arp(hote: &str) -> Option<String> {
    let ip: Ipv4Addr = hote.parse().ok()?;
    let mut m = [0u8; 8];
    let mut n: u32 = 8;
    // SAFETY : tampon de 8 octets annoncé
    let e = unsafe { SendARP(u32::from_ne_bytes(ip.octets()), 0, m.as_mut_ptr().cast(), &mut n) };
    (e == 0 && n == 6).then(|| mac(&m[..6]))
}

/// Le nom d'une adresse (Dns.GetHostEntry) : `None` sans nom
pub fn nom(hote: &str) -> Option<String> {
    let ip: Ipv4Addr = hote.parse().ok()?;
    let _ = std::net::UdpSocket::bind("127.0.0.1:0"); // (Winsock prêt)
    let s = SOCKADDR_IN {
        sin_family: AF_INET,
        sin_port: 0,
        sin_addr: IN_ADDR { S_un: IN_ADDR_0 { S_addr: u32::from_ne_bytes(ip.octets()) } },
        sin_zero: [0; 8],
    };
    let mut tampon = [0u16; 1025];
    // SAFETY : adresse et tampon valables pendant l'appel
    let e = unsafe {
        GetNameInfoW(
            (&raw const s).cast::<SOCKADDR>(),
            socklen_t(std::mem::size_of::<SOCKADDR_IN>() as i32),
            Some(&mut tampon),
            None,
            NI_NAMEREQD as i32,
        )
    };
    if e != 0 {
        return None;
    }
    let n = tampon.iter().position(|c| *c == 0).unwrap_or(0);
    Some(String::from_utf16_lossy(&tampon[..n]))
}

fn ouvrir(cle: &str) -> Option<HKEY> {
    let mut h = HKEY::default();
    // SAFETY : clé ouverte en lecture, fermée par l'appelant
    let e = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, &HSTRING::from(cle), None, KEY_READ, &mut h) };
    (e == ERROR_SUCCESS).then_some(h)
}

/// Une valeur du registre (HKLM) en texte : chaîne, ou nombre
pub fn registre_texte(cle: &str, nom: &str) -> Option<String> {
    let mut ty = REG_VALUE_TYPE::default();
    let mut taille: u32 = 0;
    let (k, n) = (HSTRING::from(cle), HSTRING::from(nom));
    // SAFETY : tailles demandées puis lues dans un tampon de cette taille
    unsafe {
        if RegGetValueW(HKEY_LOCAL_MACHINE, &k, &n, RRF_RT_ANY, Some(&mut ty), None, Some(&mut taille)) != ERROR_SUCCESS
        {
            return None;
        }
        let mut t = vec![0u8; taille as usize + 2];
        if RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &k,
            &n,
            RRF_RT_ANY,
            Some(&mut ty),
            Some(t.as_mut_ptr().cast()),
            Some(&mut taille),
        ) != ERROR_SUCCESS
        {
            return None;
        }
        match ty.0 {
            4 => Some(u32::from_le_bytes([t[0], t[1], t[2], t[3]]).to_string()),
            1 | 2 | 7 => {
                let w: Vec<u16> = t[..taille as usize]
                    .chunks(2)
                    .map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)]))
                    .collect();
                let s = String::from_utf16_lossy(&w);
                Some(s.trim_end_matches('\0').replace('\0', "\n"))
            }
            _ => None,
        }
    }
}

/// Une clé du registre existe (HKLM)
pub fn registre_cle(cle: &str) -> bool {
    match ouvrir(cle) {
        Some(h) => {
            // SAFETY : clé ouverte juste avant
            unsafe {
                let _ = RegCloseKey(h);
            }
            true
        }
        None => false,
    }
}

fn sous_cles(cle: &str) -> Vec<String> {
    let mut r = Vec::new();
    let Some(h) = ouvrir(cle) else { return r };
    // SAFETY : clé ouverte, fermée à la fin
    unsafe {
        for i in 0..512 {
            let mut nom = [0u16; 256];
            let mut n: u32 = 256;
            if RegEnumKeyExW(h, i, Some(PWSTR(nom.as_mut_ptr())), &mut n, None, None, None, None) != ERROR_SUCCESS {
                break;
            }
            r.push(String::from_utf16_lossy(&nom[..n as usize]));
        }
        let _ = RegCloseKey(h);
    }
    r
}

fn valeurs(cle: &str) -> Vec<(String, String)> {
    let mut r = Vec::new();
    let Some(h) = ouvrir(cle) else { return r };
    // SAFETY : clé ouverte, fermée à la fin
    unsafe {
        for i in 0..64 {
            let mut nom = [0u16; 256];
            let mut n: u32 = 256;
            let mut donnees = [0u8; 1024];
            let mut t: u32 = 1024;
            let mut ty: u32 = 0;
            if RegEnumValueW(
                h,
                i,
                Some(PWSTR(nom.as_mut_ptr())),
                &mut n,
                None,
                Some(&mut ty),
                Some(donnees.as_mut_ptr()),
                Some(&mut t),
            ) != ERROR_SUCCESS
            {
                break;
            }
            if ty != 1 {
                continue;
            }
            let w: Vec<u16> =
                donnees[..t as usize].chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
            r.push((
                String::from_utf16_lossy(&nom[..n as usize]),
                String::from_utf16_lossy(&w).trim_end_matches('\0').to_string(),
            ));
        }
        let _ = RegCloseKey(h);
    }
    r
}

/// Toutes les valeurs d'une clé (HKLM) : nom, type et octets ; `None` si la clé ne s'ouvre pas (absente ou refusée)
pub fn valeurs_brutes(cle: &str) -> Option<Vec<(String, u32, Vec<u8>)>> {
    let mut r = Vec::new();
    let h = ouvrir(cle)?;
    let mut nom = vec![0u16; 16384];
    let mut donnees = vec![0u8; 65536];
    // SAFETY : clé ouverte, fermée à la fin ; tampons de la taille annoncée, remise à chaque valeur
    unsafe {
        for i in 0..512 {
            let mut n = nom.len() as u32;
            let mut t = donnees.len() as u32;
            let mut ty: u32 = 0;
            if RegEnumValueW(
                h,
                i,
                Some(PWSTR(nom.as_mut_ptr())),
                &mut n,
                None,
                Some(&mut ty),
                Some(donnees.as_mut_ptr()),
                Some(&mut t),
            ) != ERROR_SUCCESS
            {
                break;
            }
            let t = (t as usize).min(donnees.len());
            r.push((String::from_utf16_lossy(&nom[..n as usize]), ty, donnees[..t].to_vec()));
        }
        let _ = RegCloseKey(h);
    }
    Some(r)
}

/// Un texte multiple du registre (REG_MULTI_SZ) : ses lignes
pub fn lignes_multiples(o: &[u8]) -> Vec<String> {
    let w: Vec<u16> = o.chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
    String::from_utf16_lossy(&w).split('\0').filter(|l| !l.is_empty()).map(str::to_string).collect()
}

/// La catégorie du réseau de la carte (`Get-NetConnectionProfile -InterfaceIndex` : NetworkCategory), par le
/// gestionnaire des réseaux de Windows (lecture sans droits) : 0 public, 1 privé, 2 domaine ; `None` si illisible
pub fn categorie_reseau(guid_carte: &str) -> Option<i32> {
    use windows::Win32::Networking::NetworkListManager::{INetworkListManager, NetworkListManager};
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    };
    let voulu = guid_carte.trim().trim_start_matches('{').trim_end_matches('}').to_ascii_uppercase();
    if voulu.is_empty() {
        return None;
    }
    // SAFETY : COM ouvert sur ce fil (et refermé seulement s'il l'a été ici) ; les objets sont lâchés avant
    unsafe {
        let init = CoInitializeEx(None, COINIT_MULTITHREADED);
        let r = (|| -> Option<i32> {
            let nlm: INetworkListManager = CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL).ok()?;
            let connexions = nlm.GetNetworkConnections().ok()?;
            for _ in 0..64 {
                let mut c = [None];
                let mut n: u32 = 0;
                if connexions.Next(&mut c, Some(&mut n)).is_err() || n == 0 {
                    return None;
                }
                let c = c[0].take()?;
                let Ok(g) = c.GetAdapterId() else { continue };
                let d = g.data4;
                let texte = format!(
                    "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
                    g.data1, g.data2, g.data3, d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]
                );
                if texte == voulu {
                    return c.GetNetwork().ok()?.GetCategory().ok().map(|x| x.0);
                }
            }
            None
        })();
        if init.is_ok() {
            CoUninitialize();
        }
        r
    }
}

/// Les vitesses que la carte sait faire (réglage « *SpeedDuplex » de son pilote) : textes et valeurs
pub fn vitesses_carte(guid: &str) -> (Vec<String>, Vec<String>) {
    let base = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}";
    for s in sous_cles(base) {
        let cle = format!(r"{base}\{s}");
        if registre_texte(&cle, "NetCfgInstanceId").is_some_and(|g| g.eq_ignore_ascii_case(guid)) {
            let mut v = valeurs(&format!(r"{cle}\Ndi\Params\*SpeedDuplex\enum"));
            v.sort_by_key(|(k, _)| k.parse::<i64>().unwrap_or(i64::MAX));
            let textes = v.iter().filter(|(_, t)| !t.starts_with('@')).map(|(_, t)| t.clone()).collect();
            let regs = v.into_iter().map(|(k, _)| k).collect();
            return (textes, regs);
        }
    }
    (Vec::new(), Vec::new())
}

/// L'heure locale
pub fn heure_locale() -> [u32; 5] {
    // SAFETY : sans paramètre
    let t = unsafe { GetLocalTime() };
    [u32::from(t.wYear), u32::from(t.wMonth), u32::from(t.wDay), u32::from(t.wHour), u32::from(t.wMinute)]
}

/// Le texte d'un programme de la console (netsh) : page de code OEM
pub fn texte_oem(o: &[u8]) -> String {
    if o.is_empty() {
        return String::new();
    }
    // SAFETY : tampons de tailles annoncées
    unsafe {
        let n = MultiByteToWideChar(CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), o, None);
        if n <= 0 {
            return String::from_utf8_lossy(o).into_owned();
        }
        let mut w = vec![0u16; n as usize];
        MultiByteToWideChar(CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), o, Some(&mut w));
        String::from_utf16_lossy(&w)
    }
}

/// Vider le cache DNS de Windows (DnsFlushResolverCache, sans droits)
pub fn vider_cache_dns() {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    // SAFETY : fonction cherchée dans dnsapi.dll, signature BOOL WINAPI sans paramètre
    unsafe {
        let Ok(m) = LoadLibraryW(&HSTRING::from("dnsapi.dll")) else { return };
        if let Some(f) = GetProcAddress(m, windows::core::s!("DnsFlushResolverCache")) {
            type Fonction = unsafe extern "system" fn() -> i32;
            let vider: Fonction = std::mem::transmute::<unsafe extern "system" fn() -> isize, Fonction>(f);
            vider();
        }
    }
}

#[allow(dead_code)]
fn _pcwstr(_: PCWSTR) {}

#[cfg(test)]
mod essais {
    /// Lecture seule de ce PC (lancé à la main : `cargo test -p nevermind-reseau route_de_ce_pc -- --ignored
    /// --nocapture`) : la route par défaut choisie et sa carte, à comparer avec Get-NetRoute et Get-NetAdapter.
    #[test]
    #[ignore]
    fn route_de_ce_pc() {
        let r = super::route_defaut();
        let carte = r.as_ref().and_then(|(i, _)| super::adaptateurs().into_iter().find(|a| a.index == *i));
        eprintln!("route : {r:?} ; carte : {:?}", carte.map(|a| (a.nom, a.desc, a.wifi)));
        let (interfaces, cartes) = nevermind_noyau::windows::route::etats_ipv4();
        eprintln!("interfaces : {interfaces:?}\ncartes : {cartes:?}");
    }

    /// Le PC de Jordan le 08/11, le Wi-Fi débranché (12) lu avant le câble (6), au même coût de route (0) : avant, la
    /// première route au plus petit coût gagnait ; la bonne n'était prise que si Windows donnait le câble en premier.
    #[test]
    fn jordan_cable_et_wifi_debranche() {
        use nevermind_noyau::route::{EtatCarte, InterfaceIp};
        let ip = |index, metrique, connectee| InterfaceIp { index, metrique, connectee };
        let carte = |index, connectee, materielle| EtatCarte { index, connectee, materielle, tunnel: false };
        let interfaces = [ip(13, 25, true), ip(6, 25, true), ip(12, 65, false), ip(1, 75, true)];
        let cartes = [carte(13, true, false), carte(12, false, true), carte(6, true, true)];
        let routes = [(12, 0, "192.168.1.1".to_string()), (6, 0, "192.168.1.1".to_string())];
        assert_eq!(super::route_choisie(&routes, &interfaces, &cartes), Some((6, "192.168.1.1".into())));
        // (dans l'autre ordre aussi, et sans route : rien)
        let routes = [(6, 0, "192.168.1.1".to_string()), (12, 0, "192.168.1.1".to_string())];
        assert_eq!(super::route_choisie(&routes, &interfaces, &cartes), Some((6, "192.168.1.1".into())));
        assert_eq!(super::route_choisie(&[], &interfaces, &cartes), None);
    }

    /// Second audit du 09/11 : deux passerelles sur la carte de la connexion, un ancien routeur (coût 100) lu avant la
    /// box (coût 0). Windows sort par la box : c'est elle que Connexion affiche et teste, pas la première lue.
    #[test]
    fn deux_passerelles_sur_la_meme_carte() {
        use nevermind_noyau::route::{EtatCarte, InterfaceIp};
        let interfaces = [InterfaceIp { index: 6, metrique: 25, connectee: true }];
        let cartes = [EtatCarte { index: 6, connectee: true, materielle: true, tunnel: false }];
        let routes = [(6, 100, "192.168.1.1".to_string()), (6, 0, "192.168.1.254".to_string())];
        assert_eq!(super::route_choisie(&routes, &interfaces, &cartes), Some((6, "192.168.1.254".into())));
        let routes = [(6, 0, "192.168.1.254".to_string()), (6, 100, "192.168.1.1".to_string())];
        assert_eq!(super::route_choisie(&routes, &interfaces, &cartes), Some((6, "192.168.1.254".into())));
    }
}
