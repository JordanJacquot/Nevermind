//! Les lectures du PC autres que les processus : la mémoire, les compteurs, les services, les antivirus inscrits, la
//! mémoire des pilotes et leurs fichiers, les informations de version d'un programme. Toutes en lecture seule.

use crate::donnees::{Pilote, Service};
use crate::texte::Date;
use aide_regles::dossiers::Dossiers;
use std::collections::HashMap;
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SYSTEM_INFORMATION_CLASS};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, CoTaskMemFree,
    CoUninitialize, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Performance::{
    PDH_HCOUNTER, PDH_HQUERY, PDH_RAW_COUNTER, PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData,
    PdhGetRawCounterValue, PdhOpenQueryW,
};
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Services::{
    CloseServiceHandle, ENUM_SERVICE_STATUS_PROCESSW, EnumServicesStatusExW, OpenSCManagerW, OpenServiceW,
    QUERY_SERVICE_CONFIGW, QueryServiceConfigW, QueryServiceStatus, SC_ENUM_PROCESS_INFO, SC_HANDLE,
    SC_MANAGER_CONNECT, SC_MANAGER_ENUMERATE_SERVICE, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_RUNNING,
    SERVICE_STATE_ALL, SERVICE_STATUS, SERVICE_WIN32,
};
use windows::Win32::System::SystemInformation::{
    ComputerNameNetBIOS, GetComputerNameExW, GetLocalTime, GetPhysicallyInstalledSystemMemory, GlobalMemoryStatusEx,
    MEMORYSTATUSEX,
};
use windows::Win32::System::Variant::{VARIANT, VT_BSTR, VariantClear};
use windows::Win32::System::Wmi::{
    IEnumWbemClassObject, IWbemLocator, IWbemServices, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY,
    WbemLocator,
};
use windows::Win32::UI::Shell::{KNOWN_FOLDER_FLAG, SHGetKnownFolderPath};
use windows::core::{BSTR, GUID, HSTRING, PCWSTR, PWSTR};

/// Un dossier connu de Windows (Windows, Program Files, Bureau...)
pub fn dossier_connu(id: &GUID) -> Option<String> {
    // SAFETY : le texte rendu est libéré par CoTaskMemFree
    unsafe {
        let p = SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG(0), None).ok()?;
        let t = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        t.filter(|x| !x.is_empty())
    }
}

/// Le nom du PC (`$env:COMPUTERNAME`)
pub fn nom_du_pc() -> String {
    let mut b = [0u16; 256];
    let mut n = b.len() as u32;
    // SAFETY : tampon et taille cohérents
    match unsafe { GetComputerNameExW(ComputerNameNetBIOS, Some(PWSTR(b.as_mut_ptr())), &mut n) } {
        Ok(()) => String::from_utf16_lossy(&b[..n as usize]),
        Err(_) => String::new(),
    }
}

/// L'heure locale
pub fn maintenant() -> Date {
    // SAFETY : simple lecture
    let t = unsafe { GetLocalTime() };
    Date {
        annee: u32::from(t.wYear),
        mois: u32::from(t.wMonth),
        jour: u32::from(t.wDay),
        heure: u32::from(t.wHour),
        minute: u32::from(t.wMinute),
        seconde: u32::from(t.wSecond),
    }
}

pub struct Memoire {
    pub visible: f64,
    pub disponible: f64,
    pub installee: f64,
}

/// La mémoire vue par Windows et la mémoire installée (`Win32_OperatingSystem`, `Win32_PhysicalMemory`)
pub fn memoire() -> Result<Memoire, String> {
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY : structure à la bonne taille
    unsafe { GlobalMemoryStatusEx(&mut m) }.map_err(|e| e.message())?;
    let mut ko = 0u64;
    // SAFETY : simple lecture
    let installee =
        if unsafe { GetPhysicallyInstalledSystemMemory(&mut ko) }.is_ok() { ko as f64 * 1024.0 } else { 0.0 };
    // (TotalVisibleMemorySize et FreePhysicalMemory sont en Ko entiers)
    Ok(Memoire {
        visible: (m.ullTotalPhys / 1024) as f64 * 1024.0,
        disponible: (m.ullAvailPhys / 1024) as f64 * 1024.0,
        installee,
    })
}

pub struct Compteurs {
    pub disponible: f64,
    pub cache: f64,
    pub modifiee: f64,
    pub pool_np: f64,
    pub pool_p: f64,
}

/// Les compteurs de la mémoire (`Win32_PerfRawData_PerfOS_Memory`), lus par PDH
pub fn compteurs() -> Result<Compteurs, String> {
    let noms = [
        r"\Memory\Available Bytes",
        r"\Memory\Standby Cache Core Bytes",
        r"\Memory\Standby Cache Normal Priority Bytes",
        r"\Memory\Standby Cache Reserve Bytes",
        r"\Memory\Modified Page List Bytes",
        r"\Memory\Pool Nonpaged Bytes",
        r"\Memory\Pool Paged Bytes",
    ];
    let mut q = PDH_HQUERY::default();
    // SAFETY : requête PDH ouverte puis refermée ici
    unsafe {
        let r = PdhOpenQueryW(PCWSTR::null(), 0, &mut q);
        if r != 0 {
            return Err(format!("PDH 0x{r:08x}"));
        }
        let mut c = Vec::new();
        for n in noms {
            let mut h = PDH_HCOUNTER::default();
            let r = PdhAddEnglishCounterW(q, &HSTRING::from(n), 0, &mut h);
            if r != 0 {
                PdhCloseQuery(q);
                return Err(format!("PDH {n} 0x{r:08x}"));
            }
            c.push(h);
        }
        let r = PdhCollectQueryData(q);
        if r != 0 {
            PdhCloseQuery(q);
            return Err(format!("PDH 0x{r:08x}"));
        }
        let mut v = Vec::new();
        for h in c {
            let mut x = PDH_RAW_COUNTER::default();
            let r = PdhGetRawCounterValue(h, None, &mut x);
            v.push(if r == 0 { x.FirstValue as f64 } else { 0.0 });
        }
        PdhCloseQuery(q);
        Ok(Compteurs { disponible: v[0], cache: v[1] + v[2] + v[3], modifiee: v[4], pool_np: v[5], pool_p: v[6] })
    }
}

struct Gestionnaire(SC_HANDLE);

impl Drop for Gestionnaire {
    fn drop(&mut self) {
        // SAFETY : poignée ouverte ici, refermée une fois
        unsafe {
            let _ = CloseServiceHandle(self.0);
        }
    }
}

fn texte_pwstr(p: PWSTR) -> String {
    // SAFETY : texte terminé par un zéro, rendu par Windows
    if p.is_null() { String::new() } else { unsafe { p.to_string().unwrap_or_default() } }
}

/// Le type de démarrage d'un service, comme WMI (`StartMode` : « Auto », « Manual »...) et Get-Service
/// (`StartType` : « Automatic », « Manual »...)
fn demarrage(h: SC_HANDLE) -> Option<u32> {
    let mut n = 0u32;
    // SAFETY : premier appel pour la taille, second avec un tampon aligné de cette taille
    unsafe {
        let _ = QueryServiceConfigW(h, None, 0, &mut n);
        if n == 0 {
            return None;
        }
        let mut b = vec![0u64; (n as usize).div_ceil(8)];
        QueryServiceConfigW(h, Some(b.as_mut_ptr().cast()), n, &mut n).ok()?;
        Some((*(b.as_ptr() as *const QUERY_SERVICE_CONFIGW)).dwStartType.0)
    }
}

fn mode_wmi(t: u32) -> &'static str {
    match t {
        0 => "Boot",
        1 => "System",
        2 => "Auto",
        3 => "Manual",
        4 => "Disabled",
        _ => "Unknown",
    }
}

fn etat_wmi(s: u32) -> &'static str {
    match s {
        1 => "Stopped",
        2 => "Start Pending",
        3 => "Stop Pending",
        4 => "Running",
        5 => "Continue Pending",
        6 => "Pause Pending",
        7 => "Paused",
        _ => "Unknown",
    }
}

/// Les services (`Win32_Service`) : nom, titre, processus, état. Le type de démarrage n'est lu que pour les services
/// de la liste « inutile » (le seul endroit où il sert : les corrections).
pub fn services() -> Result<Vec<Service>, String> {
    // SAFETY : énumération en lecture seule, tampon de la taille demandée par Windows
    unsafe {
        let g = Gestionnaire(
            OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT | SC_MANAGER_ENUMERATE_SERVICE)
                .map_err(|e| e.message())?,
        );
        let mut besoin = 0u32;
        let mut n = 0u32;
        let mut reprise = 0u32;
        let mut l = Vec::new();
        let mut b: Vec<u64> = vec![0; 64 * 1024 / 8];
        loop {
            let tampon = std::slice::from_raw_parts_mut(b.as_mut_ptr() as *mut u8, b.len() * 8);
            let r = EnumServicesStatusExW(
                g.0,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                Some(tampon),
                &mut besoin,
                &mut n,
                Some(&mut reprise),
                PCWSTR::null(),
            );
            let encore = r.is_err() && r.as_ref().err().map(|e| e.code().0 as u32) == Some(0x800700EA); // ERROR_MORE_DATA
            if r.is_err() && !encore {
                return Err(r.err().map(|e| e.message()).unwrap_or_default());
            }
            let e = std::slice::from_raw_parts(b.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW, n as usize);
            for s in e {
                let nom = texte_pwstr(s.lpServiceName);
                let mode = if crate::listes::SERVICES_INUTILES.iter().any(|i| i.motif.is_match(&nom)) {
                    match OpenServiceW(g.0, &HSTRING::from(nom.as_str()), SERVICE_QUERY_CONFIG) {
                        Ok(h) => {
                            let h = Gestionnaire(h);
                            demarrage(h.0).map(mode_wmi).unwrap_or("").to_string()
                        }
                        Err(_) => String::new(),
                    }
                } else {
                    String::new()
                };
                l.push(Service {
                    titre: texte_pwstr(s.lpDisplayName),
                    pid: i64::from(s.ServiceStatusProcess.dwProcessId),
                    demarrage: mode,
                    etat: etat_wmi(s.ServiceStatusProcess.dwCurrentState.0).into(),
                    nom,
                });
            }
            if !encore {
                break;
            }
            if (besoin as usize) > b.len() * 8 {
                b = vec![0; (besoin as usize).div_ceil(8) + 1024];
            }
        }
        Ok(l)
    }
}

/// Un service (`Get-Service`) : son type de démarrage (« Automatic », « Manual »...) et s'il tourne
pub fn service(nom: &str) -> Option<(String, bool)> {
    // SAFETY : lectures seules
    unsafe {
        let g = Gestionnaire(OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT).ok()?);
        let h = Gestionnaire(OpenServiceW(g.0, &HSTRING::from(nom), SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS).ok()?);
        let t = demarrage(h.0)?;
        let mut st = SERVICE_STATUS::default();
        QueryServiceStatus(h.0, &mut st).ok()?;
        let depart = match t {
            0 => "Boot",
            1 => "System",
            2 => "Automatic",
            3 => "Manual",
            4 => "Disabled",
            _ => "Unknown",
        };
        Some((depart.into(), st.dwCurrentState == SERVICE_RUNNING))
    }
}

/// Les informations de version d'un programme (`[Diagnostics.FileVersionInfo]::GetVersionInfo`) : produit,
/// description, société (vides si illisibles)
pub fn version(chemin: &str) -> (String, String, String) {
    let f = HSTRING::from(chemin);
    // SAFETY : tampon de la taille donnée par Windows ; les valeurs lues pointent dans ce tampon
    unsafe {
        let n = GetFileVersionInfoSizeW(&f, None);
        if n == 0 {
            return Default::default();
        }
        let mut b = vec![0u8; n as usize];
        if GetFileVersionInfoW(&f, None, n, b.as_mut_ptr().cast()).is_err() {
            return Default::default();
        }
        let valeur = |chemin: &str| -> Option<String> {
            let mut p: *mut core::ffi::c_void = std::ptr::null_mut();
            let mut l = 0u32;
            if !VerQueryValueW(b.as_ptr().cast(), &HSTRING::from(chemin), &mut p, &mut l).as_bool() || p.is_null() {
                return None;
            }
            Some((p, l)).map(|(p, l)| {
                let t = std::slice::from_raw_parts(p as *const u16, l as usize);
                let fin = t.iter().position(|&c| c == 0).unwrap_or(t.len());
                String::from_utf16_lossy(&t[..fin])
            })
        };
        // La langue : la première traduction déclarée, sinon celles que .NET essaie
        let mut langues = Vec::new();
        {
            let mut p: *mut core::ffi::c_void = std::ptr::null_mut();
            let mut l = 0u32;
            if VerQueryValueW(b.as_ptr().cast(), &HSTRING::from(r"\VarFileInfo\Translation"), &mut p, &mut l).as_bool()
                && !p.is_null()
                && l >= 4
            {
                let t = std::slice::from_raw_parts(p as *const u16, 2);
                langues.push(format!("{:04x}{:04x}", t[0], t[1]));
            }
        }
        langues.extend(["040904b0".to_string(), "040904e4".to_string(), "04090000".to_string()]);
        for lg in langues {
            let base = format!(r"\StringFileInfo\{lg}\");
            let (p, d, s) = (
                valeur(&format!("{base}ProductName")),
                valeur(&format!("{base}FileDescription")),
                valeur(&format!("{base}CompanyName")),
            );
            if p.is_some() || d.is_some() || s.is_some() {
                return (
                    p.unwrap_or_default().trim().to_string(),
                    d.unwrap_or_default().trim().to_string(),
                    s.unwrap_or_default().trim().to_string(),
                );
            }
        }
        Default::default()
    }
}

/// Les plus grosses étiquettes de la mémoire des pilotes (pool non paginé, SystemPoolTagInformation, lisible sans
/// droits administrateur) : (étiquette, octets)
pub fn etiquettes_pool(nombre: usize) -> Result<Vec<(String, f64)>, String> {
    const MISMATCH: i32 = 0xC000_0004_u32 as i32;
    let mut taille = 0x10_0000usize;
    for _ in 0..6 {
        let mut b = vec![0u64; taille / 8];
        let mut rendu = 0u32;
        // SAFETY : tampon de la taille annoncée
        let st = unsafe {
            NtQuerySystemInformation(
                SYSTEM_INFORMATION_CLASS(22),
                b.as_mut_ptr().cast(),
                (b.len() * 8) as u32,
                &mut rendu,
            )
        };
        if st.0 == MISMATCH {
            taille = (taille * 2).max(rendu as usize + 0x1000);
            continue;
        }
        if st.0 != 0 {
            return Err(format!("NtQuerySystemInformation 0x{:08x}", st.0));
        }
        let o = |i: usize, n: usize| -> &[u8] {
            // SAFETY : lecture dans le tampon rempli par Windows (bornes vérifiées ci-dessous)
            unsafe { std::slice::from_raw_parts((b.as_ptr() as *const u8).add(i), n) }
        };
        let n = u32::from_le_bytes(o(0, 4).try_into().unwrap_or([0; 4])) as usize;
        let mut l = Vec::with_capacity(n);
        for i in 0..n {
            let e = 8 + i * 40;
            if e + 40 > b.len() * 8 {
                break;
            }
            let tag = String::from_utf8_lossy(o(e, 4)).into_owned();
            let np = u64::from_le_bytes(o(e + 32, 8).try_into().unwrap_or([0; 8])) as f64;
            l.push((tag, np));
        }
        l.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        l.truncate(nombre);
        return Ok(l);
    }
    Err("mémoire des pilotes illisible".into())
}

/// Le pilote qui porte une étiquette : les fichiers .sys de Windows qui contiennent ces 4 lettres (lecture seule).
/// Une étiquette trouvée dans trop de fichiers (plus de 3) ne désigne personne : liste vide.
pub fn pilotes_etiquettes(windows: &str, tags: &[String]) -> HashMap<String, Vec<Pilote>> {
    let mut r = HashMap::new();
    let mut a_chercher: Vec<&String> = Vec::new();
    for t in tags.iter().filter(|t| !t.is_empty()) {
        if !a_chercher.contains(&t) {
            a_chercher.push(t);
        }
    }
    if a_chercher.is_empty() || windows.is_empty() {
        return r;
    }
    let mut trouves: HashMap<&String, Vec<std::path::PathBuf>> = a_chercher.iter().map(|t| (*t, Vec::new())).collect();
    let dossier = std::path::Path::new(windows).join("System32").join("drivers");
    if let Ok(lu) = std::fs::read_dir(&dossier) {
        for f in lu.flatten() {
            let p = f.path();
            let sys = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sys"));
            if !sys || !f.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let Ok(o) = std::fs::read(&p) else { continue };
            for t in &a_chercher {
                if o.windows(t.len()).any(|w| w == t.as_bytes())
                    && let Some(l) = trouves.get_mut(t)
                {
                    l.push(p.clone());
                }
            }
        }
    }
    for t in a_chercher {
        let l = trouves.remove(t).unwrap_or_default();
        let pilotes = if !l.is_empty() && l.len() <= 3 {
            l.iter()
                .map(|c| {
                    let (_, d, s) = version(&c.to_string_lossy());
                    Pilote {
                        fichier: c.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default(),
                        description: d,
                        societe: s,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        r.insert(t.clone(), pilotes);
    }
    r
}

/// COM prêt sur ce fil tant que la valeur vit
struct Com(bool);

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY : CoInitializeEx a réussi sur ce fil
            unsafe { CoUninitialize() };
        }
    }
}

/// Les programmes des antivirus inscrits au Centre de sécurité de Windows (`Get-AideAntivirus`, chemins complets)
pub fn antivirus(d: &Dossiers) -> Vec<String> {
    let mut l = Vec::new();
    // SAFETY : COM pour ce fil, requête WMI en lecture seule, VARIANT vidés après lecture
    unsafe {
        let _com = Com(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let Ok(loc) = CoCreateInstance::<_, IWbemLocator>(&WbemLocator, None, CLSCTX_INPROC_SERVER) else { return l };
        let vide = BSTR::new();
        let Ok(s): Result<IWbemServices, _> =
            loc.ConnectServer(&BSTR::from(r"root\SecurityCenter2"), &vide, &vide, &vide, 0, &vide, None)
        else {
            return l;
        };
        let _ = CoSetProxyBlanket(
            &s,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            PCWSTR::null(),
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        );
        let Ok(e): Result<IEnumWbemClassObject, _> = s.ExecQuery(
            &BSTR::from("WQL"),
            &BSTR::from("SELECT * FROM AntiVirusProduct"),
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None,
        ) else {
            return l;
        };
        let mut ecrits = Vec::new();
        loop {
            let mut objets = [None];
            let mut n = 0u32;
            let _ = e.Next(5000, &mut objets, &mut n);
            if n == 0 {
                break;
            }
            let Some(o) = objets[0].take() else { break };
            for c in ["pathToSignedProductExe", "pathToSignedReportingExe"] {
                let mut v = VARIANT::default();
                if o.Get(&HSTRING::from(c), 0, &mut v, None, None).is_ok() {
                    if v.Anonymous.Anonymous.vt == VT_BSTR {
                        ecrits.push(v.Anonymous.Anonymous.Anonymous.bstrVal.to_string());
                    }
                    let _ = VariantClear(&mut v);
                }
            }
        }
        for x in ecrits {
            if let Some(p) = d.programme_de_commande(&x) {
                l.push(p);
            }
        }
    }
    l
}
