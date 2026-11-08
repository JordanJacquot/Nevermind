//! Le seul module qui parle à Windows (et le seul où le code « unsafe » est permis). Rien n'y écrit sur le PC :
//! des lectures sans droits (connexions TCP et UDP, processus, services, registre, signatures, cache DNS, route
//! par défaut), les pings (ICMP, comme `[LagMon]` de natif.cs) et les demandes aux annuaires publics (WinHTTP),
//! que la version PowerShell faisait aussi. Le comptage des octets TCP (SetPerTcpConnectionEStats) n'est accepté
//! par Windows qu'en administrateur : sans l'Aide, il est refusé et rien ne change.
//!
//! Aucun programme n'est lancé, sauf `netsh wlan show interfaces` (lecture du Wi-Fi), toujours sans fenêtre.

#![allow(unsafe_code)]

use crate::trafic::etat::Service;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::fmt::Write as _;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GAA_FLAG_INCLUDE_GATEWAYS, GetAdaptersAddresses, GetExtendedTcpTable, GetExtendedUdpTable,
    GetIpForwardTable2, GetPerTcp6ConnectionEStats, GetPerTcpConnectionEStats, ICMP_ECHO_REPLY,
    IP_ADAPTER_ADDRESSES_LH, IP_OPTION_INFORMATION, IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho,
    MIB_IPFORWARD_TABLE2, MIB_TCP_STATE, MIB_TCP6ROW, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_LH, MIB_TCPROW_LH_0,
    MIB_TCPROW_OWNER_PID, MIB_UDP6ROW_OWNER_PID, MIB_UDPROW_OWNER_PID, SetPerTcp6ConnectionEStats,
    SetPerTcpConnectionEStats, TCP_ESTATS_DATA_ROD_v0, TCP_TABLE_OWNER_PID_ALL, TcpConnectionEstatsData,
    UDP_TABLE_OWNER_PID,
};
use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6, IN6_ADDR, IN6_ADDR_0};
use windows::Win32::Security::Cryptography::Catalog::{
    CATALOG_INFO, CryptCATAdminAcquireContext2, CryptCATAdminCalcHashFromFileHandle2, CryptCATAdminEnumCatalogFromHash,
    CryptCATAdminReleaseCatalogContext, CryptCATAdminReleaseContext, CryptCATCatalogInfoFromContext,
};
use windows::Win32::Security::Cryptography::{CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_CATALOG_INFO, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CHOICE_CATALOG, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    GetFileVersionInfoSizeW, GetFileVersionInfoW, OPEN_EXISTING, VerQueryValueW,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Services::{
    CloseServiceHandle, ENUM_SERVICE_STATUS_PROCESSW, EnumServicesStatusExW, OpenSCManagerW, OpenServiceW,
    QUERY_SERVICE_CONFIGW, QueryServiceConfigW, QueryServiceStatus, SC_ENUM_PROCESS_INFO, SC_MANAGER_CONNECT,
    SC_MANAGER_ENUMERATE_SERVICE, SERVICE_ACTIVE, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_RUNNING,
    SERVICE_STATUS, SERVICE_WIN32,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

/// Un texte UTF-16 terminé par un zéro
fn texte16(b: &[u16]) -> String {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf16_lossy(&b[..n])
}

/// Un texte pointé par Windows
fn pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY : chaîne terminée par un zéro donnée par Windows
    unsafe { p.to_string().unwrap_or_default() }
}

// ---------------------------------------------------------------------------------------------------------------
// Connexions TCP et UDP (classe `TrafficMon` de natif.cs)
// ---------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct LigneTcp {
    v6: bool,
    etat: u32,
    locale: [u8; 16],
    distante: [u8; 16],
    port_local: u32,
    port_distant: u32,
    zone_locale: u32,
    zone_distante: u32,
    pid: u32,
}

fn table_tcp(v6: bool) -> Option<Vec<LigneTcp>> {
    let af = if v6 { AF_INET6.0 } else { AF_INET.0 } as u32;
    let mut taille = 0u32;
    // SAFETY : premier appel sans tampon, pour la taille
    unsafe {
        GetExtendedTcpTable(None, &mut taille, false, af, TCP_TABLE_OWNER_PID_ALL, 0);
    }
    for _ in 0..3 {
        taille += 4096;
        let mut b = vec![0u64; (taille as usize).div_ceil(8)];
        // SAFETY : tampon aligné d'au moins `taille` octets
        let r = unsafe {
            GetExtendedTcpTable(Some(b.as_mut_ptr().cast()), &mut taille, false, af, TCP_TABLE_OWNER_PID_ALL, 0)
        };
        if r != 0 {
            continue;
        }
        let n = b[0] as u32 as usize;
        let mut lignes = Vec::with_capacity(n);
        // SAFETY : Windows a écrit n lignes après le nombre (4 octets) ; lecture sans alignement
        unsafe {
            let debut = b.as_ptr().cast::<u8>().add(4);
            for i in 0..n {
                if v6 {
                    let l = std::ptr::read_unaligned(debut.cast::<MIB_TCP6ROW_OWNER_PID>().add(i));
                    lignes.push(LigneTcp {
                        v6: true,
                        etat: l.dwState,
                        locale: l.ucLocalAddr,
                        distante: l.ucRemoteAddr,
                        port_local: l.dwLocalPort,
                        port_distant: l.dwRemotePort,
                        zone_locale: l.dwLocalScopeId,
                        zone_distante: l.dwRemoteScopeId,
                        pid: l.dwOwningPid,
                    });
                } else {
                    let l = std::ptr::read_unaligned(debut.cast::<MIB_TCPROW_OWNER_PID>().add(i));
                    let mut a = [0u8; 16];
                    let mut d = [0u8; 16];
                    a[..4].copy_from_slice(&l.dwLocalAddr.to_ne_bytes());
                    d[..4].copy_from_slice(&l.dwRemoteAddr.to_ne_bytes());
                    lignes.push(LigneTcp {
                        v6: false,
                        etat: l.dwState,
                        locale: a,
                        distante: d,
                        port_local: l.dwLocalPort,
                        port_distant: l.dwRemotePort,
                        zone_locale: 0,
                        zone_distante: 0,
                        pid: l.dwOwningPid,
                    });
                }
            }
        }
        return Some(lignes);
    }
    None
}

fn ligne_v4(l: &LigneTcp) -> MIB_TCPROW_LH {
    let a = |o: &[u8; 16]| u32::from_ne_bytes([o[0], o[1], o[2], o[3]]);
    MIB_TCPROW_LH {
        Anonymous: MIB_TCPROW_LH_0 { dwState: l.etat },
        dwLocalAddr: a(&l.locale),
        dwLocalPort: l.port_local,
        dwRemoteAddr: a(&l.distante),
        dwRemotePort: l.port_distant,
    }
}

fn ligne_v6(l: &LigneTcp) -> MIB_TCP6ROW {
    MIB_TCP6ROW {
        State: MIB_TCP_STATE(l.etat as i32),
        LocalAddr: IN6_ADDR { u: IN6_ADDR_0 { Byte: l.locale } },
        dwLocalScopeId: l.zone_locale,
        dwLocalPort: l.port_local,
        RemoteAddr: IN6_ADDR { u: IN6_ADDR_0 { Byte: l.distante } },
        dwRemoteScopeId: l.zone_distante,
        dwRemotePort: l.port_distant,
    }
}

fn tcp_compter(l: &LigneTcp) -> bool {
    let rw = [1u8];
    // SAFETY : ligne locale, tampon d'un octet
    unsafe {
        if l.v6 {
            SetPerTcp6ConnectionEStats(&ligne_v6(l), TcpConnectionEstatsData, &rw, 0, 0) == 0
        } else {
            SetPerTcpConnectionEStats(&ligne_v4(l), TcpConnectionEstatsData, &rw, 0, 0) == 0
        }
    }
}

fn tcp_octets(l: &LigneTcp) -> Option<(u64, u64)> {
    let mut rod = TCP_ESTATS_DATA_ROD_v0::default();
    // SAFETY : la structure vue comme un tampon de sa taille exacte
    let o = unsafe { std::slice::from_raw_parts_mut((&raw mut rod).cast::<u8>(), size_of::<TCP_ESTATS_DATA_ROD_v0>()) };
    // SAFETY : ligne locale, tampon de la bonne taille
    let r = unsafe {
        if l.v6 {
            GetPerTcp6ConnectionEStats(&ligne_v6(l), TcpConnectionEstatsData, None, 0, None, 0, Some(o), 0)
        } else {
            GetPerTcpConnectionEStats(&ligne_v4(l), TcpConnectionEstatsData, None, 0, None, 0, Some(o), 0)
        }
    };
    (r == 0).then_some((rod.DataBytesOut, rod.DataBytesIn))
}

fn port(p: u32) -> u32 {
    ((p & 0xFF) << 8) | ((p >> 8) & 0xFF)
}

fn adresse(o: &[u8; 16], v6: bool) -> String {
    if !v6 {
        return std::net::Ipv4Addr::new(o[0], o[1], o[2], o[3]).to_string();
    }
    let a = std::net::Ipv6Addr::from(*o);
    match a.to_ipv4_mapped() {
        Some(v4) => v4.to_string(),
        None => a.to_string(),
    }
}

fn cle_tcp(l: &LigneTcp) -> String {
    let mut s = String::new();
    let _ =
        write!(s, "{}|{:?}:{}|{:?}:{}", if l.v6 { 6 } else { 4 }, l.locale, l.port_local, l.distante, l.port_distant);
    s
}

/// Les compteurs TCP (comptage demandé une fois par connexion établie, oublié quand elle se ferme)
#[derive(Default)]
pub struct CompteursTcp {
    comptees: HashSet<String>,
    pub ok: bool,
}

impl CompteursTcp {
    /// « pid|adresse distante|port distant|port local|état|octets envoyés|octets reçus » (`TrafficMon.Tcp`)
    pub fn lignes(&mut self) -> Vec<String> {
        let mut r = Vec::new();
        let mut vivantes = HashSet::new();
        for v6 in [false, true] {
            for l in table_tcp(v6).unwrap_or_default() {
                let vide = if v6 { l.distante.iter().all(|&x| x == 0) } else { l.distante[..4] == [0; 4] };
                if l.etat == 2 || vide {
                    continue;
                }
                let c = cle_tcp(&l);
                let (o, i) = if l.etat == 5 {
                    if !self.comptees.contains(&c) && tcp_compter(&l) {
                        self.comptees.insert(c.clone());
                        self.ok = true;
                    }
                    if self.comptees.contains(&c) { tcp_octets(&l).unwrap_or((0, 0)) } else { (0, 0) }
                } else {
                    (0, 0)
                };
                vivantes.insert(c);
                r.push(format!(
                    "{}|{}|{}|{}|{}|{o}|{i}",
                    l.pid,
                    adresse(&l.distante, v6),
                    port(l.port_distant),
                    port(l.port_local),
                    l.etat
                ));
            }
        }
        self.comptees.retain(|c| vivantes.contains(c));
        r
    }
}

/// Les programmes qui ont un port UDP ouvert : « pid|port » (`TrafficMon.Udp`)
pub fn udp() -> Vec<String> {
    let mut r = Vec::new();
    for v6 in [false, true] {
        let af = if v6 { AF_INET6.0 } else { AF_INET.0 } as u32;
        let mut taille = 0u32;
        // SAFETY : premier appel sans tampon, pour la taille
        unsafe {
            GetExtendedUdpTable(None, &mut taille, false, af, UDP_TABLE_OWNER_PID, 0);
        }
        for _ in 0..3 {
            taille += 4096;
            let mut b = vec![0u64; (taille as usize).div_ceil(8)];
            // SAFETY : tampon aligné d'au moins `taille` octets
            let x = unsafe {
                GetExtendedUdpTable(Some(b.as_mut_ptr().cast()), &mut taille, false, af, UDP_TABLE_OWNER_PID, 0)
            };
            if x != 0 {
                continue;
            }
            let n = b[0] as u32 as usize;
            // SAFETY : n lignes après le nombre ; lecture sans alignement
            unsafe {
                let debut = b.as_ptr().cast::<u8>().add(4);
                for i in 0..n {
                    let (pid, p) = if v6 {
                        let l = std::ptr::read_unaligned(debut.cast::<MIB_UDP6ROW_OWNER_PID>().add(i));
                        (l.dwOwningPid, l.dwLocalPort)
                    } else {
                        let l = std::ptr::read_unaligned(debut.cast::<MIB_UDPROW_OWNER_PID>().add(i));
                        (l.dwOwningPid, l.dwLocalPort)
                    };
                    r.push(format!("{pid}|{}", port(p)));
                }
            }
            break;
        }
    }
    r
}

// ---------------------------------------------------------------------------------------------------------------
// Processus et services
// ---------------------------------------------------------------------------------------------------------------

/// Les processus : numéro, parent, nom (sans « .exe »)
pub fn processus() -> Vec<(u32, u32, String)> {
    let mut r = Vec::new();
    // SAFETY : instantané lu puis fermé ; structure locale de la bonne taille
    unsafe {
        let Ok(h) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return r };
        let mut e = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(h, &mut e).is_ok() {
            loop {
                let nom = texte16(&e.szExeFile);
                let nom = match nom.len().checked_sub(4) {
                    Some(i) if nom.is_char_boundary(i) && nom[i..].eq_ignore_ascii_case(".exe") => nom[..i].to_string(),
                    _ => nom,
                };
                r.push((e.th32ProcessID, e.th32ParentProcessID, nom));
                if Process32NextW(h, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(h);
    }
    r
}

/// L'emplacement d'un processus (droit de lecture minimal), vide si Windows le cache
pub fn chemin(pid: u32) -> String {
    // SAFETY : poignée ouverte puis fermée ; tampon local
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut b = vec![0u16; 1024];
        let mut n = b.len() as u32;
        let r = if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(b.as_mut_ptr()), &mut n).is_ok() {
            String::from_utf16_lossy(&b[..n as usize])
        } else {
            String::new()
        };
        let _ = CloseHandle(h);
        r
    }
}

struct Sc(windows::Win32::System::Services::SC_HANDLE);
impl Drop for Sc {
    fn drop(&mut self) {
        // SAFETY : poignée ouverte par OpenSCManagerW ou OpenServiceW, fermée une fois
        let _ = unsafe { CloseServiceHandle(self.0) };
    }
}

/// Les services en marche, par processus (`[SvcMap]::Pids()`)
pub fn services() -> HashMap<u32, Vec<Service>> {
    let mut r: HashMap<u32, Vec<Service>> = HashMap::new();
    // SAFETY : gestionnaire ouvert en lecture puis fermé ; le tampon garde les chaînes pointées pendant la lecture
    unsafe {
        let Ok(m) = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT | SC_MANAGER_ENUMERATE_SERVICE)
        else {
            return r;
        };
        let m = Sc(m);
        let mut besoin = 0u32;
        let mut nb = 0u32;
        let mut reprise = 0u32;
        let _ = EnumServicesStatusExW(
            m.0,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_ACTIVE,
            None,
            &mut besoin,
            &mut nb,
            Some(&mut reprise),
            PCWSTR::null(),
        );
        let mut b = vec![0u64; (besoin as usize + 4096).div_ceil(8)];
        let o = std::slice::from_raw_parts_mut(b.as_mut_ptr().cast::<u8>(), b.len() * 8);
        reprise = 0;
        if EnumServicesStatusExW(
            m.0,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_ACTIVE,
            Some(o),
            &mut besoin,
            &mut nb,
            Some(&mut reprise),
            PCWSTR::null(),
        )
        .is_ok()
        {
            let l = std::slice::from_raw_parts(b.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>(), nb as usize);
            for s in l {
                let pid = s.ServiceStatusProcess.dwProcessId;
                if pid == 0 {
                    continue;
                }
                r.entry(pid).or_default().push(Service { nom: pwstr(s.lpServiceName), titre: pwstr(s.lpDisplayName) });
            }
        }
    }
    for v in r.values_mut() {
        v.sort_by_key(|s| s.nom.to_lowercase());
    }
    r
}

/// Un service : (en marche, type de démarrage avec les noms de .NET)
pub fn etat_service(nom: &str) -> Option<(bool, String)> {
    // SAFETY : poignées ouvertes en lecture puis fermées ; tampon de configuration aligné
    unsafe {
        let m = Sc(OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT).ok()?);
        let s = Sc(OpenServiceW(m.0, &HSTRING::from(nom), SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG).ok()?);
        let mut st = SERVICE_STATUS::default();
        QueryServiceStatus(s.0, &mut st).ok()?;
        let mut besoin = 0u32;
        let _ = QueryServiceConfigW(s.0, None, 0, &mut besoin);
        let mut b = vec![0u64; (besoin as usize).div_ceil(8).max(8)];
        let demarrage = if QueryServiceConfigW(
            s.0,
            Some(b.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>()),
            (b.len() * 8) as u32,
            &mut besoin,
        )
        .is_ok()
        {
            let c = &*b.as_ptr().cast::<QUERY_SERVICE_CONFIGW>();
            match c.dwStartType.0 {
                0 => "Boot",
                1 => "System",
                2 => "Automatic",
                3 => "Manual",
                4 => "Disabled",
                _ => "Unknown",
            }
        } else {
            "Unknown"
        };
        Some((st.dwCurrentState == SERVICE_RUNNING, demarrage.to_string()))
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Fichiers : description et signature (`FileVersionInfo`, `Get-AuthenticodeSignature`)
// ---------------------------------------------------------------------------------------------------------------

/// La description d'un programme (`FileVersionInfo.FileDescription`)
pub fn description(chemin: &str) -> String {
    let h = HSTRING::from(chemin);
    // SAFETY : tampon de la taille donnée par Windows ; les valeurs lues pointent dans ce tampon
    unsafe {
        let n = GetFileVersionInfoSizeW(&h, None);
        if n == 0 {
            return String::new();
        }
        let mut b = vec![0u8; n as usize];
        if GetFileVersionInfoW(&h, None, n, b.as_mut_ptr().cast()).is_err() {
            return String::new();
        }
        let mut p: *mut c_void = std::ptr::null_mut();
        let mut l = 0u32;
        let mut langues: Vec<(u16, u16)> = Vec::new();
        if VerQueryValueW(b.as_ptr().cast(), &HSTRING::from(r"\VarFileInfo\Translation"), &mut p, &mut l).as_bool()
            && !p.is_null()
        {
            let t = std::slice::from_raw_parts(p.cast::<u16>(), (l / 2) as usize);
            for c in t.chunks(2) {
                if c.len() == 2 {
                    langues.push((c[0], c[1]));
                }
            }
        }
        langues.extend([(0x0409, 0x04B0), (0x0409, 0x04E4), (0x040C, 0x04B0), (0, 0x04B0)]);
        for (lg, cp) in langues {
            let cle = format!("\\StringFileInfo\\{lg:04x}{cp:04x}\\FileDescription");
            let mut q: *mut c_void = std::ptr::null_mut();
            let mut ql = 0u32;
            if VerQueryValueW(b.as_ptr().cast(), &HSTRING::from(cle), &mut q, &mut ql).as_bool()
                && !q.is_null()
                && ql > 0
            {
                let s = std::slice::from_raw_parts(q.cast::<u16>(), ql as usize);
                return texte16(s).trim().to_string();
            }
        }
        String::new()
    }
}

/// L'état d'une signature avec les noms de PowerShell
fn etat_signature(code: i32) -> &'static str {
    match code as u32 {
        0 => "Valid",
        0x8009_0008 => "Incompatible",
        0x800B_0100 => "NotSigned",
        0x8009_6010 | 0x8009_200D => "HashMismatch",
        0x800B_0001 => "NotSupportedFileFormat",
        0x800B_0111 | 0x800B_0109 | 0x800B_010A => "NotTrusted",
        _ => "UnknownError",
    }
}

/// Vérifie `donnees` (fichier ou catalogue) ; rend le code et le nom de celui qui a signé (CN)
///
/// SAFETY : `donnees` décrit des structures vivantes pendant l'appel
unsafe fn verifier(donnees: &mut WINTRUST_DATA) -> (i32, String) {
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY : voir plus haut ; l'état gardé par Windows est rendu à la fin
    unsafe {
        let code = WinVerifyTrust(HWND::default(), &mut action, (donnees as *mut WINTRUST_DATA).cast());
        let mut sujet = String::new();
        let prov = WTHelperProvDataFromStateData(donnees.hWVTStateData);
        if !prov.is_null() {
            let signataire = WTHelperGetProvSignerFromChain(prov, 0, false, 0);
            if !signataire.is_null() {
                let cert = WTHelperGetProvCertFromChain(signataire, 0);
                if !cert.is_null() && !(*cert).pCert.is_null() {
                    let mut b = vec![0u16; 512];
                    let n = CertGetNameStringW((*cert).pCert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut b));
                    if n > 1 {
                        sujet = texte16(&b);
                    }
                }
            }
        }
        donnees.dwStateAction = WTD_STATEACTION_CLOSE;
        let _ = WinVerifyTrust(HWND::default(), &mut action, (donnees as *mut WINTRUST_DATA).cast());
        (code, sujet)
    }
}

/// La signature d'un fichier, celle du fichier ou celle d'un catalogue de Windows (comme `Get-AuthenticodeSignature`) :
/// (état, nom de l'éditeur)
pub fn signature(chemin: &str) -> (String, String) {
    let nom = HSTRING::from(chemin);
    let mut fichier = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(nom.as_ptr()),
        ..Default::default()
    };
    let mut donnees = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut fichier },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    // SAFETY : structures locales vivantes pendant l'appel
    let (code, sujet) = unsafe { verifier(&mut donnees) };
    let etat = etat_signature(code);
    if etat != "NotSigned" {
        return (etat.into(), sujet);
    }
    match signature_catalogue(chemin) {
        Some(x) => x,
        None => (etat.into(), sujet),
    }
}

/// La signature par un catalogue de Windows (les programmes de Windows sont signés ainsi)
fn signature_catalogue(chemin: &str) -> Option<(String, String)> {
    let nom = HSTRING::from(chemin);
    // SAFETY : poignées ouvertes puis rendues ; tampons locaux ; structures vivantes pendant les appels
    unsafe {
        let f = CreateFileW(
            &nom,
            0x8000_0000,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
        .ok()?;
        let mut resultat = None;
        for algo in ["SHA256", "SHA1"] {
            let mut admin: isize = 0;
            let algo_h = HSTRING::from(algo);
            if CryptCATAdminAcquireContext2(&mut admin, None, &algo_h, None, None).is_err() {
                continue;
            }
            let mut n = 0u32;
            let _ = CryptCATAdminCalcHashFromFileHandle2(admin, f, &mut n, None, None);
            let mut empreinte = vec![0u8; n as usize];
            if n == 0
                || CryptCATAdminCalcHashFromFileHandle2(admin, f, &mut n, Some(empreinte.as_mut_ptr()), None).is_err()
            {
                let _ = CryptCATAdminReleaseContext(admin, 0);
                continue;
            }
            let cat = CryptCATAdminEnumCatalogFromHash(admin, &empreinte, None, None);
            if cat != 0 {
                let mut info = CATALOG_INFO { cbStruct: size_of::<CATALOG_INFO>() as u32, ..Default::default() };
                if CryptCATCatalogInfoFromContext(cat, &mut info, 0).is_ok() {
                    let membre: String = empreinte.iter().fold(String::new(), |mut s, b| {
                        let _ = write!(s, "{b:02X}");
                        s
                    });
                    let membre_h = HSTRING::from(membre);
                    let mut ci = WINTRUST_CATALOG_INFO {
                        cbStruct: size_of::<WINTRUST_CATALOG_INFO>() as u32,
                        pcwszCatalogFilePath: PCWSTR(info.wszCatalogFile.as_ptr()),
                        pcwszMemberTag: PCWSTR(membre_h.as_ptr()),
                        pcwszMemberFilePath: PCWSTR(nom.as_ptr()),
                        hMemberFile: f,
                        pbCalculatedFileHash: empreinte.as_mut_ptr(),
                        cbCalculatedFileHash: empreinte.len() as u32,
                        hCatAdmin: admin,
                        ..Default::default()
                    };
                    let mut donnees = WINTRUST_DATA {
                        cbStruct: size_of::<WINTRUST_DATA>() as u32,
                        dwUIChoice: WTD_UI_NONE,
                        fdwRevocationChecks: WTD_REVOKE_NONE,
                        dwUnionChoice: WTD_CHOICE_CATALOG,
                        Anonymous: WINTRUST_DATA_0 { pCatalog: &mut ci },
                        dwStateAction: WTD_STATEACTION_VERIFY,
                        ..Default::default()
                    };
                    let (code, sujet) = verifier(&mut donnees);
                    resultat = Some((etat_signature(code).to_string(), sujet));
                }
                let _ = CryptCATAdminReleaseCatalogContext(admin, cat, 0);
            }
            let _ = CryptCATAdminReleaseContext(admin, 0);
            if resultat.is_some() {
                break;
            }
        }
        let _ = CloseHandle(f);
        resultat
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Réseau : la connexion active, le Wi-Fi, les pings
// ---------------------------------------------------------------------------------------------------------------

/// La connexion vers Internet (`Get-ActiveNet`) : passerelle, Wi-Fi, nom et description de la carte
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reseau {
    pub passerelle: String,
    pub wifi: bool,
    pub nom: String,
    pub desc: String,
}

/// Le choix parmi les routes par défaut (carte, coût de la route, passerelle) de la table en service
/// (GetIpForwardTable2 ne donne que l'ActiveStore)
fn route_choisie(
    routes: &[(u32, u32, [u8; 4])],
    interfaces: &[nevermind_noyau::route::InterfaceIp],
    cartes: &[nevermind_noyau::route::EtatCarte],
) -> Option<(u32, [u8; 4])> {
    use nevermind_noyau::route::{RouteDefaut, choisir_route};
    let defauts: Vec<RouteDefaut> = routes
        .iter()
        .map(|(i, m, _)| RouteDefaut { index: i64::from(*i), metrique: i64::from(*m), active: true })
        .collect();
    let index = choisir_route(&defauts, interfaces, cartes)?;
    routes.iter().find(|(i, _, _)| i64::from(*i) == index).map(|(i, _, p)| (*i, *p))
}

/// La route par défaut que Windows prend vraiment et sa carte. Avant, la première au plus petit coût de route : une
/// carte Wi-Fi débranchée qui garde sa route au même coût que le câble pouvait gagner (PC de Jordan, 08/11 : il
/// n'était choisi juste que parce que Windows donnait le câble en premier).
pub fn reseau_actif() -> Option<Reseau> {
    // SAFETY : tables données par Windows, lues dans leurs limites puis rendues
    unsafe {
        let mut t: *mut MIB_IPFORWARD_TABLE2 = std::ptr::null_mut();
        if GetIpForwardTable2(AF_INET, &mut t).is_err() || t.is_null() {
            return None;
        }
        let lignes = std::slice::from_raw_parts((*t).Table.as_ptr(), (*t).NumEntries as usize);
        let routes: Vec<(u32, u32, [u8; 4])> = lignes
            .iter()
            .filter(|l| l.DestinationPrefix.PrefixLength == 0)
            .map(|l| (l.InterfaceIndex, l.Metric, l.NextHop.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes()))
            .collect();
        FreeMibTable(t.cast());
        let (interfaces, cartes) = nevermind_noyau::windows::route::etats_ipv4();
        let (index, pa) = route_choisie(&routes, &interfaces, &cartes)?;
        let mut taille = 16 * 1024u32;
        let mut b = vec![0u64; taille as usize / 8];
        let mut r = GetAdaptersAddresses(
            AF_INET.0 as u32,
            GAA_FLAG_INCLUDE_GATEWAYS,
            None,
            Some(b.as_mut_ptr().cast()),
            &mut taille,
        );
        if r == 111 {
            b = vec![0u64; (taille as usize).div_ceil(8)];
            r = GetAdaptersAddresses(
                AF_INET.0 as u32,
                GAA_FLAG_INCLUDE_GATEWAYS,
                None,
                Some(b.as_mut_ptr().cast()),
                &mut taille,
            );
        }
        if r != 0 {
            return None;
        }
        let mut p = b.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        while !p.is_null() {
            let a = &*p;
            if a.Anonymous1.Anonymous.IfIndex == index {
                let nom = pwstr(a.FriendlyName);
                let desc = pwstr(a.Description);
                let wifi = a.IfType == 71 || {
                    let d = desc.to_lowercase();
                    d.contains("wi-fi") || d.contains("wifi") || d.contains("wireless") || d.contains("wlan")
                };
                return Some(Reseau { passerelle: std::net::Ipv4Addr::from(pa).to_string(), wifi, nom, desc });
            }
            p = a.Next;
        }
        None
    }
}

/// Les lignes de `netsh wlan show interfaces` (force du signal, bande, canal), lancé sans fenêtre
pub fn wifi() -> Vec<String> {
    use std::os::windows::process::CommandExt;
    let systeme = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let sortie = std::process::Command::new(format!(r"{systeme}\System32\netsh.exe"))
        .args(["wlan", "show", "interfaces"])
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output();
    match sortie {
        // (netsh écrit dans la page de code de la console : les chiffres et les mots-clés suffisent)
        Ok(o) => String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect(),
        Err(_) => Vec::new(),
    }
}

/// Une poignée ICMP
struct Icmp(HANDLE);
impl Drop for Icmp {
    fn drop(&mut self) {
        // SAFETY : poignée ouverte par IcmpCreateFile, fermée une fois
        let _ = unsafe { IcmpCloseHandle(self.0) };
    }
}

/// Un ping (IPv4) : (statut, adresse qui a répondu, durée en ms) ; `ttl` : nombre d'étapes au plus
fn echo(hote: &str, delai_ms: u32, ttl: Option<u8>) -> Option<(u32, String, f64)> {
    let a: std::net::Ipv4Addr = hote.parse().ok()?;
    let dest = u32::from_ne_bytes(a.octets());
    let donnees = [0u8; 32];
    let mut reponse = vec![0u8; size_of::<ICMP_ECHO_REPLY>() + 32 + 8 + 64];
    let options = ttl.map(|t| IP_OPTION_INFORMATION { Ttl: t, ..Default::default() });
    let debut = std::time::Instant::now();
    // SAFETY : poignée ouverte puis fermée ; tampons locaux de la taille donnée
    unsafe {
        let h = Icmp(IcmpCreateFile().ok()?);
        let n = IcmpSendEcho(
            h.0,
            dest,
            donnees.as_ptr().cast(),
            donnees.len() as u16,
            options.as_ref().map(|o| o as *const IP_OPTION_INFORMATION),
            reponse.as_mut_ptr().cast(),
            reponse.len() as u32,
            delai_ms,
        );
        if n == 0 {
            return None;
        }
        let r = std::ptr::read_unaligned(reponse.as_ptr().cast::<ICMP_ECHO_REPLY>());
        let qui = std::net::Ipv4Addr::from(r.Address.to_ne_bytes()).to_string();
        let ms = if r.Status == 0 { r.RoundTripTime as f64 } else { debut.elapsed().as_secs_f64() * 1000.0 };
        Some((r.Status, qui, ms))
    }
}

/// Un ping : la durée en ms (au moins 0,5), `None` s'il est perdu
pub fn ping(hote: &str, delai_ms: u32) -> Option<f64> {
    match echo(hote, delai_ms, None) {
        Some((0, _, ms)) => Some(f64::max(0.5, ms)),
        _ => None,
    }
}

/// Le chemin vers un hôte : « étape|adresse|ms » (adresse vide si le routeur ne répond pas), comme `[LagMon]::TraceRoute`
pub fn trace(hote: &str, max: u8, delai_ms: u32) -> Vec<String> {
    let mut r = Vec::new();
    for ttl in 1..=max {
        let (mut qui, mut ms, mut fini) = (String::new(), -1.0, false);
        for _ in 0..2 {
            // 11013 : IP_TTL_EXPIRED_TRANSIT ; 0 : arrivé
            if let Some((st, a, t)) = echo(hote, delai_ms, Some(ttl))
                && (st == 0 || st == 11013)
            {
                qui = a;
                ms = t;
                fini = st == 0;
                break;
            }
        }
        r.push(format!("{ttl}|{qui}|{}", ms as i64));
        if fini {
            break;
        }
    }
    r
}

// ---------------------------------------------------------------------------------------------------------------
// Le registre (lecture seulement) et le compte de la session
// ---------------------------------------------------------------------------------------------------------------

fn cle_registre(chemin: &str) -> Option<windows_registry::Key> {
    let (ruche, reste) = chemin.split_once(":\\")?;
    let r = match ruche.to_ascii_uppercase().as_str() {
        "HKLM" => windows_registry::LOCAL_MACHINE,
        "HKCU" => windows_registry::CURRENT_USER,
        _ => return None,
    };
    r.options().read().open(reste).ok()
}

/// Un nombre du registre (`Get-RegNum`)
pub fn reg_nombre(chemin: &str, nom: &str) -> Option<i64> {
    let k = cle_registre(chemin)?;
    if let Ok(v) = k.get_u32(nom) {
        return Some(v as i32 as i64);
    }
    if let Ok(v) = k.get_u64(nom) {
        return i32::try_from(v as i64).ok().map(i64::from);
    }
    if let Ok(s) = k.get_string(nom) {
        return s.trim().parse::<i32>().ok().map(i64::from);
    }
    None
}

/// Un texte du registre
pub fn reg_texte(chemin: &str, nom: &str) -> String {
    cle_registre(chemin).and_then(|k| k.get_string(nom).ok()).unwrap_or_default()
}

windows::core::link!("sechost.dll" "system" fn LsaLookupUserAccountType(sid: *mut c_void, compte: *mut i32) -> i32);

/// Le compte de la session est un compte Microsoft (`Get-LocalUser`, `PrincipalSource`)
pub fn compte_microsoft() -> bool {
    use windows::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    // SAFETY : jeton du processus ouvert en lecture puis fermé ; tampon aligné
    unsafe {
        let mut jeton = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut jeton).is_err() {
            return false;
        }
        let mut n = 0u32;
        let _ = GetTokenInformation(jeton, TokenUser, None, 0, &mut n);
        let mut b = vec![0u64; (n as usize).div_ceil(8).max(8)];
        let ok =
            GetTokenInformation(jeton, TokenUser, Some(b.as_mut_ptr().cast()), (b.len() * 8) as u32, &mut n).is_ok();
        let _ = CloseHandle(jeton);
        if !ok {
            return false;
        }
        let u = &*b.as_ptr().cast::<TOKEN_USER>();
        let mut sorte = 0i32;
        // 4 : LocalConnected, 6 : Internet, 7 : MSA (compte Microsoft)
        LsaLookupUserAccountType(u.User.Sid.0, &mut sorte) == 0 && matches!(sorte, 4 | 6 | 7)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Les noms des serveurs : le cache DNS de Windows (lu sans aucune demande sur le réseau) et la recherche inverse
// ---------------------------------------------------------------------------------------------------------------

#[repr(C)]
struct EntreeCacheDns {
    suivant: *mut EntreeCacheDns,
    nom: PWSTR,
    sorte: u16,
    donnees: u16,
    drapeaux: u32,
}

windows::core::link!("dnsapi.dll" "system" fn DnsGetCacheDataTable(table: *mut *mut EntreeCacheDns) -> i32);

/// Les adresses connues du cache DNS : (adresse, nom), comme `Get-DnsClientCache` (types A et AAAA)
pub fn cache_dns() -> Vec<(String, String)> {
    use windows::Win32::NetworkManagement::Dns::{
        DNS_QUERY_NO_WIRE_QUERY, DNS_RECORDA, DNS_TYPE_A, DNS_TYPE_AAAA, DnsFree, DnsFreeRecordList, DnsQuery_W,
    };
    let mut r = Vec::new();
    // SAFETY : la liste donnée par Windows est lue puis rendue ; chaque réponse est rendue ; aucune demande ne part
    // sur le réseau (DNS_QUERY_NO_WIRE_QUERY : le cache seulement)
    unsafe {
        let mut t: *mut EntreeCacheDns = std::ptr::null_mut();
        if DnsGetCacheDataTable(&mut t) == 0 {
            return r;
        }
        let mut noms: Vec<(String, u16)> = Vec::new();
        let mut p = t;
        while !p.is_null() {
            let e = &*p;
            let nom = pwstr(e.nom);
            if e.sorte == DNS_TYPE_A.0 || e.sorte == DNS_TYPE_AAAA.0 {
                noms.push((nom, e.sorte));
            }
            let suivant = e.suivant;
            if !e.nom.is_null() {
                DnsFree(Some(e.nom.0 as *const c_void), windows::Win32::NetworkManagement::Dns::DnsFreeFlat);
            }
            DnsFree(Some(p as *const c_void), windows::Win32::NetworkManagement::Dns::DnsFreeFlat);
            p = suivant;
        }
        for (nom, sorte) in noms {
            let mut rep: *mut DNS_RECORDA = std::ptr::null_mut();
            let h = HSTRING::from(nom.as_str());
            if DnsQuery_W(
                &h,
                windows::Win32::NetworkManagement::Dns::DNS_TYPE(sorte),
                DNS_QUERY_NO_WIRE_QUERY,
                None,
                &mut rep as *mut _ as *mut _,
                None,
            )
            .0 != 0
            {
                continue;
            }
            let mut x = rep;
            while !x.is_null() {
                let e = &*x;
                if e.wType == DNS_TYPE_A.0 {
                    r.push((
                        std::net::Ipv4Addr::from(e.Data.A.IpAddress.to_ne_bytes()).to_string(),
                        nom.trim_end_matches('.').to_string(),
                    ));
                } else if e.wType == DNS_TYPE_AAAA.0 {
                    let o: [u8; 16] = std::mem::transmute(e.Data.AAAA.Ip6Address);
                    r.push((std::net::Ipv6Addr::from(o).to_string(), nom.trim_end_matches('.').to_string()));
                }
                x = e.pNext;
            }
            DnsFree(Some(rep as *const c_void), DnsFreeRecordList);
        }
    }
    r
}

/// Le nom officiel d'une adresse (recherche inverse), vide sinon. Envoie une demande au DNS : seulement si
/// l'utilisateur a permis l'annuaire (comme `$ServerLookupWork`)
pub fn nom_inverse(adresse: &str) -> String {
    // (std ne fait pas de recherche inverse : WinSock GetNameInfoW)
    use windows::Win32::Networking::WinSock::{GetNameInfoW, NI_NAMEREQD, SOCKADDR, SOCKADDR_IN, WSADATA, WSAStartup};
    let Ok(a) = adresse.parse::<std::net::Ipv4Addr>() else { return String::new() };
    // SAFETY : structures locales ; WinSock démarré (compté par Windows)
    unsafe {
        let mut w = WSADATA::default();
        let _ = WSAStartup(0x0202, &mut w);
        let mut sa = SOCKADDR_IN { sin_family: AF_INET, ..Default::default() };
        sa.sin_addr.S_un.S_addr = u32::from_ne_bytes(a.octets());
        let mut hote = vec![0u16; 1025];
        let r = GetNameInfoW(
            &sa as *const SOCKADDR_IN as *const SOCKADDR,
            windows::Win32::Networking::WinSock::socklen_t(size_of::<SOCKADDR_IN>() as i32),
            Some(&mut hote),
            None,
            NI_NAMEREQD as i32,
        );
        if r == 0 { texte16(&hote).trim_end_matches('.').to_string() } else { String::new() }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// WinHTTP : une demande GET (annuaires publics rdap.org et ipinfo.io, seulement si l'utilisateur l'a permis)
// ---------------------------------------------------------------------------------------------------------------

struct Hinternet(*mut c_void);
impl Drop for Hinternet {
    fn drop(&mut self) {
        // SAFETY : poignée WinHTTP ouverte, fermée une fois
        let _ = unsafe { windows::Win32::Networking::WinHttp::WinHttpCloseHandle(self.0) };
    }
}

/// Une adresse découpée : https ?, l'hôte, le port, le chemin
pub fn decouper(url: &str) -> Option<(bool, String, u16, String)> {
    let (securise, reste) =
        if let Some(r) = url.strip_prefix("https://") { (true, r) } else { (false, url.strip_prefix("http://")?) };
    let (hp, chemin) = match reste.find('/') {
        Some(i) => (&reste[..i], &reste[i..]),
        None => (reste, "/"),
    };
    let (hote, port) = match hp.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().ok()?),
        None => (hp, if securise { 443 } else { 80 }),
    };
    (!hote.is_empty()).then(|| (securise, hote.to_string(), port, chemin.to_string()))
}

/// GET `url` (8 secondes au plus, comme la version PowerShell) : le texte de la réponse
pub fn http_get(url: &str, accepte: &str, sans_proxy: bool) -> Result<String, String> {
    use windows::Win32::Networking::WinHttp::*;
    let (securise, hote, port, chemin) = decouper(url).ok_or_else(|| format!("adresse inattendue : {url}"))?;
    let err = |e: windows::core::Error| e.message();
    let ouvrir = |p: *mut c_void| {
        if p.is_null() { Err(windows::core::Error::from_thread().message()) } else { Ok(Hinternet(p)) }
    };
    // SAFETY : poignées ouvertes puis fermées dans l'ordre ; chaînes et tampons vivants pendant les appels
    unsafe {
        let acces = if sans_proxy { WINHTTP_ACCESS_TYPE_NO_PROXY } else { WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY };
        let s = ouvrir(WinHttpOpen(&HSTRING::from("Nevermind"), acces, PCWSTR::null(), PCWSTR::null(), 0))?;
        WinHttpSetTimeouts(s.0, 8000, 8000, 8000, 8000).map_err(err)?;
        let c = ouvrir(WinHttpConnect(s.0, &HSTRING::from(hote), port, 0))?;
        let drapeaux = if securise { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
        let d = ouvrir(WinHttpOpenRequest(
            c.0,
            &HSTRING::from("GET"),
            &HSTRING::from(chemin),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            drapeaux,
        ))?;
        let entetes: Vec<u16> = format!("Accept: {accepte}\r\n").encode_utf16().collect();
        WinHttpSendRequest(d.0, Some(&entetes), None, 0, 0, 0).map_err(err)?;
        WinHttpReceiveResponse(d.0, std::ptr::null_mut()).map_err(err)?;
        let mut statut = 0u32;
        let mut l = 4u32;
        WinHttpQueryHeaders(
            d.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut statut as *mut u32 as *mut c_void),
            &mut l,
            std::ptr::null_mut(),
        )
        .map_err(err)?;
        if statut != 200 {
            return Err(format!("réponse {statut} du serveur"));
        }
        let mut corps = Vec::new();
        let mut b = vec![0u8; 16 * 1024];
        loop {
            let mut lu = 0u32;
            WinHttpReadData(d.0, b.as_mut_ptr().cast(), b.len() as u32, &mut lu).map_err(err)?;
            if lu == 0 || corps.len() > 2 * 1024 * 1024 {
                break;
            }
            corps.extend_from_slice(&b[..lu as usize]);
        }
        Ok(String::from_utf8_lossy(&corps).into_owned())
    }
}

// Essais sur le vrai Windows, en lecture seulement : rien n'est changé, aucun paquet ne part sur Internet
// (le ping vise 127.0.0.1 seulement).
#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn lectures_sans_droits() {
        let mut c = CompteursTcp::default();
        let l = c.lignes();
        assert!(l.iter().all(|x| x.split('|').count() == 7), "{l:?}");
        assert!(udp().iter().all(|x| x.split('|').count() == 2));
        let p = processus();
        let moi = std::process::id();
        assert!(p.iter().any(|(pid, _, _)| *pid == moi));
        assert!(chemin(moi).to_lowercase().ends_with(".exe"));
        // (les programmes de Windows sont signés par catalogue)
        let systeme = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let (etat, editeur) = signature(&format!(r"{systeme}\System32\svchost.exe"));
        assert_eq!(etat, "Valid", "{editeur}");
        assert!(editeur.contains("Microsoft"), "{editeur}");
        let (etat, _) = signature(&std::env::current_exe().unwrap().to_string_lossy());
        assert_eq!(etat, "NotSigned");
        assert!(!description(&format!(r"{systeme}\System32\svchost.exe")).is_empty());
        assert!(etat_service("Dnscache").is_some());
        assert!(etat_service("service-qui-n-existe-pas").is_none());
        let _ = services();
        let _ = reg_nombre(r"HKCU:\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo", "Enabled");
        let _ = compte_microsoft();
        let _ = cache_dns();
        let _ = reseau_actif();
    }

    /// Lecture seule de ce PC (lancé à la main : `cargo test -p nevermind-trafic reseau_de_ce_pc -- --ignored
    /// --nocapture`) : la connexion vers Internet choisie, à comparer avec Get-NetRoute et Get-NetAdapter.
    #[test]
    #[ignore]
    fn reseau_de_ce_pc() {
        eprintln!("reseau : {:?}", reseau_actif());
    }

    /// Le PC de Jordan le 08/11, le Wi-Fi débranché (12) lu avant le câble (6), au même coût de route (0) : avant, la
    /// première route au plus petit coût gagnait (« Wi-Fi ») ; la bonne n'était prise que si Windows donnait le câble
    /// en premier.
    #[test]
    fn jordan_cable_et_wifi_debranche() {
        use nevermind_noyau::route::{EtatCarte, InterfaceIp};
        let ip = |index, metrique, connectee| InterfaceIp { index, metrique, connectee };
        let carte = |index, connectee, materielle| EtatCarte { index, connectee, materielle };
        let interfaces = [ip(13, 25, true), ip(6, 25, true), ip(12, 65, false), ip(1, 75, true)];
        let cartes = [carte(13, true, false), carte(12, false, true), carte(6, true, true)];
        let pa = [192, 168, 1, 1];
        assert_eq!(route_choisie(&[(12, 0, pa), (6, 0, pa)], &interfaces, &cartes), Some((6, pa)));
        assert_eq!(route_choisie(&[(6, 0, pa), (12, 0, pa)], &interfaces, &cartes), Some((6, pa)));
        assert_eq!(route_choisie(&[], &interfaces, &cartes), None);
    }

    #[test]
    fn ping_local() {
        assert!(ping("127.0.0.1", 1000).is_some());
        assert!(ping("pas-une-adresse", 100).is_none());
        let t = trace("127.0.0.1", 3, 500);
        assert_eq!(t.len(), 1, "{t:?}");
        assert!(t[0].starts_with("1|127.0.0.1|"));
    }

    #[test]
    fn adresses_decoupees() {
        assert_eq!(decouper("https://rdap.org/ip/1.2.3.4"), Some((true, "rdap.org".into(), 443, "/ip/1.2.3.4".into())));
        assert_eq!(decouper("http://127.0.0.1:8123/x"), Some((false, "127.0.0.1".into(), 8123, "/x".into())));
        assert_eq!(decouper("ftp://x"), None);
    }
}
