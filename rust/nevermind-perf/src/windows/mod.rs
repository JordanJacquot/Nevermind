//! Le seul module qui parle à Windows (et le seul où le code « unsafe » est permis). Tout est lu sans droits
//! administrateur et sans lancer de programme ; rien n'est modifié (la seule exception : relancer une appli que le mode
//! jeu a fermée, comme la version PowerShell, et le compteur de FPS, dans `compteur`).
//!
//! Ce qui est ouvert une fois et gardé : la liste des cartes graphiques (DXGI) et leurs moteurs, nvml.dll (System32
//! seulement), la requête PDH. Chaque appel à Windows est compté (`lectures`).

#![allow(unsafe_code)]

pub mod compteur;

use crate::systeme::{Nvidia, Programme, Systeme};
use std::os::windows::process::CommandExt;
use std::time::Instant;

use windows::Wdk::Graphics::Direct3D::{
    D3DKMT_QUERYSTATISTICS, D3DKMT_QUERYSTATISTICS_ADAPTER, D3DKMT_QUERYSTATISTICS_NODE, D3DKMTQueryStatistics,
};
use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, HMODULE, LUID, WAIT_TIMEOUT};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1};
use windows::Win32::Graphics::Gdi::{
    DEVMODEW, DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE, EnumDisplayDevicesW,
    EnumDisplaySettingsW,
};
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW, PdhCollectQueryData,
    PdhGetFormattedCounterValue, PdhOpenQueryW,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegGetValueW,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{
    GetSystemTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    QueryFullProcessImageNameW, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};
use windows::core::{HSTRING, PCSTR, PCWSTR, PWSTR};

/// Un programme lancé sans console (CREATE_NO_WINDOW)
const SANS_FENETRE: u32 = 0x0800_0000;

fn ft(f: FILETIME) -> u64 {
    (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)
}

fn texte_large(b: &[u16]) -> String {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf16_lossy(&b[..n])
}

type Brute = unsafe extern "system" fn() -> isize;
type FnTemperature = unsafe extern "system" fn(*mut core::ffi::c_void, u32, *mut u32) -> i32;
type FnMemoire = unsafe extern "system" fn(*mut core::ffi::c_void, *mut [u64; 3]) -> i32;
type FnValeur = unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> i32;
type FnUtilisation = unsafe extern "system" fn(*mut core::ffi::c_void, *mut [u32; 2]) -> i32;

/// Les fonctions de nvml.dll dont le bloc se sert (chargées une fois)
struct Nvml {
    _module: HMODULE,
    appareil: *mut core::ffi::c_void,
    temperature: FnTemperature,
    memoire: FnMemoire,
    puissance: FnValeur,
    limite: FnValeur,
    utilisation: FnUtilisation,
}
// (le pointeur de l'appareil n'est utilisé que par le fil qui tient le bloc)
unsafe impl Send for Nvml {}

impl Nvml {
    fn charger() -> Option<Nvml> {
        // SAFETY : nvml.dll de System32 seulement (jamais un fichier du dossier courant) ; les fonctions sont prises
        // par leur nom documenté et appelées avec leurs signatures C (nvml.h)
        unsafe {
            let m = LoadLibraryExW(&HSTRING::from("nvml.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32).ok()?;
            let f = |n: &[u8]| GetProcAddress(m, PCSTR(n.as_ptr()));
            let init = std::mem::transmute::<Brute, unsafe extern "system" fn() -> i32>(f(b"nvmlInit_v2\0")?);
            let par_index = std::mem::transmute::<
                Brute,
                unsafe extern "system" fn(u32, *mut *mut core::ffi::c_void) -> i32,
            >(f(b"nvmlDeviceGetHandleByIndex_v2\0")?);
            if init() != 0 {
                return None;
            }
            let mut appareil = std::ptr::null_mut();
            if par_index(0, &mut appareil) != 0 {
                return None;
            }
            Some(Nvml {
                _module: m,
                appareil,
                temperature: std::mem::transmute::<Brute, FnTemperature>(f(b"nvmlDeviceGetTemperature\0")?),
                memoire: std::mem::transmute::<Brute, FnMemoire>(f(b"nvmlDeviceGetMemoryInfo\0")?),
                puissance: std::mem::transmute::<Brute, FnValeur>(f(b"nvmlDeviceGetPowerUsage\0")?),
                limite: std::mem::transmute::<Brute, FnValeur>(f(b"nvmlDeviceGetPowerManagementLimit\0")?),
                utilisation: std::mem::transmute::<Brute, FnUtilisation>(f(b"nvmlDeviceGetUtilizationRates\0")?),
            })
        }
    }
}

struct Pdh {
    _requete: PDH_HQUERY,
    perf: PDH_HCOUNTER,
    disque: PDH_HCOUNTER,
}
unsafe impl Send for Pdh {}

/// Le vrai PC
pub struct WindowsReel {
    debut: Instant,
    n: u64,
    /// Les cartes graphiques (DXGI) : (nom, LUID, nombre de moteurs)
    cartes: Option<Vec<(String, LUID, u32)>>,
    nvml: Option<Option<Nvml>>,
    pdh: Option<Option<Pdh>>,
}

impl Default for WindowsReel {
    fn default() -> Self {
        Self { debut: Instant::now(), n: 0, cartes: None, nvml: None, pdh: None }
    }
}

impl WindowsReel {
    fn cartes_dxgi(&mut self) -> &Vec<(String, LUID, u32)> {
        if self.cartes.is_none() {
            let mut l = Vec::new();
            // SAFETY : fabrique DXGI créée et lâchée ici ; GetDesc1 remplit une structure à nous
            unsafe {
                if let Ok(f) = CreateDXGIFactory1::<IDXGIFactory1>() {
                    let mut i = 0;
                    while let Ok(a) = f.EnumAdapters1(i) {
                        i += 1;
                        self.n += 1;
                        let Ok(d) = a.GetDesc1() else { continue };
                        if (d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
                            continue;
                        }
                        let q = D3DKMT_QUERYSTATISTICS {
                            Type: D3DKMT_QUERYSTATISTICS_ADAPTER,
                            AdapterLuid: d.AdapterLuid,
                            ..Default::default()
                        };
                        let noeuds = if D3DKMTQueryStatistics(&q).is_ok() {
                            q.QueryResult.AdapterInformation.NodeCount
                        } else {
                            0
                        };
                        l.push((texte_large(&d.Description), d.AdapterLuid, noeuds));
                    }
                }
            }
            self.cartes = Some(l);
        }
        self.cartes.as_ref().unwrap_or_else(|| unreachable!())
    }
}

impl Systeme for WindowsReel {
    fn temps_processeur(&mut self) -> Option<(u64, u64)> {
        self.n += 1;
        let (mut i, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        // SAFETY : trois structures à nous
        unsafe { GetSystemTimes(Some(&mut i), Some(&mut k), Some(&mut u)).ok()? };
        Some((ft(i), ft(k) + ft(u)))
    }

    fn memoire(&mut self) -> Option<(f64, f64)> {
        self.n += 1;
        let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        // SAFETY : structure à nous, sa taille est donnée
        unsafe { GlobalMemoryStatusEx(&mut m).ok()? };
        Some((m.ullTotalPhys as f64, (m.ullTotalPhys - m.ullAvailPhys) as f64))
    }

    fn moteurs_gpu(&mut self) -> Option<(Vec<i64>, i64)> {
        let cartes: Vec<(LUID, u32)> = self.cartes_dxgi().iter().map(|c| (c.1, c.2)).collect();
        let mut v = Vec::new();
        for (luid, noeuds) in cartes {
            for id in 0..noeuds {
                self.n += 1;
                let mut q = D3DKMT_QUERYSTATISTICS {
                    Type: D3DKMT_QUERYSTATISTICS_NODE,
                    AdapterLuid: luid,
                    ..Default::default()
                };
                q.Anonymous.QueryNode.NodeId = id;
                // SAFETY : structure à nous, du type demandé ; le résultat est lu dans le champ de ce type
                unsafe {
                    if D3DKMTQueryStatistics(&q).is_ok() {
                        v.push(q.QueryResult.NodeInformation.GlobalInformation.RunningTime);
                    } else {
                        v.push(0);
                    }
                }
            }
        }
        if v.is_empty() {
            return None;
        }
        Some((v, (self.debut.elapsed().as_nanos() / 100) as i64))
    }

    fn nvidia(&mut self) -> Option<Nvidia> {
        if self.nvml.is_none() {
            self.n += 1;
            self.nvml = Some(Nvml::charger());
        }
        let n = self.nvml.as_ref()?.as_ref()?;
        self.n += 5;
        let mut r = Nvidia::default();
        // SAFETY : fonctions de nvml.dll, appelées avec l'appareil qu'elle a donné et des sorties à nous
        unsafe {
            let mut t = 0u32;
            if (n.temperature)(n.appareil, 0, &mut t) == 0 {
                r.temperature = Some(f64::from(t));
            }
            let mut m = [0u64; 3];
            if (n.memoire)(n.appareil, &mut m) == 0 {
                r.vram_totale = Some(m[0] as f64 / 1048576.0);
                r.vram_utilisee = Some(m[2] as f64 / 1048576.0);
            }
            let mut p = 0u32;
            if (n.puissance)(n.appareil, &mut p) == 0 {
                r.puissance = Some(f64::from(p) / 1000.0);
            }
            let mut l = 0u32;
            if (n.limite)(n.appareil, &mut l) == 0 {
                r.puissance_max = Some(f64::from(l) / 1000.0);
            }
            let mut u = [0u32; 2];
            if (n.utilisation)(n.appareil, &mut u) == 0 {
                r.utilisation = Some(f64::from(u[0]));
            }
        }
        Some(r)
    }

    fn compteurs(&mut self) -> (Option<f64>, Option<f64>) {
        if self.pdh.is_none() {
            self.n += 3;
            // SAFETY : requête PDH à nous, gardée pour toute la vie du bloc ; noms anglais (Windows en français)
            self.pdh = Some(unsafe {
                let mut q = PDH_HQUERY::default();
                if PdhOpenQueryW(PCWSTR::null(), 0, &mut q) != 0 {
                    None
                } else {
                    let mut perf = PDH_HCOUNTER::default();
                    let mut disque = PDH_HCOUNTER::default();
                    let a = PdhAddEnglishCounterW(
                        q,
                        &HSTRING::from(r"\Processor Information(_Total)\% Processor Performance"),
                        0,
                        &mut perf,
                    );
                    let b =
                        PdhAddEnglishCounterW(q, &HSTRING::from(r"\PhysicalDisk(_Total)\% Idle Time"), 0, &mut disque);
                    let _ = PdhCollectQueryData(q);
                    if a != 0 && b != 0 { None } else { Some(Pdh { _requete: q, perf, disque }) }
                }
            });
        }
        let Some(Some(p)) = &self.pdh else { return (None, None) };
        self.n += 1;
        // SAFETY : compteurs de notre requête, valeurs dans des structures à nous
        unsafe {
            if PdhCollectQueryData(p._requete) != 0 {
                return (None, None);
            }
            let lire = |c: PDH_HCOUNTER| -> Option<f64> {
                let mut v = PDH_FMT_COUNTERVALUE::default();
                (PdhGetFormattedCounterValue(c, PDH_FMT_DOUBLE, None, &mut v) == 0 && v.CStatus == 0)
                    .then_some(v.Anonymous.doubleValue)
            };
            (lire(p.perf), lire(p.disque).map(|idle| 100.0 - idle))
        }
    }

    fn programmes(&mut self) -> Vec<Programme> {
        self.n += 1;
        let mut taille: u32 = 512 * 1024;
        let mut buf: Vec<u64>;
        loop {
            buf = vec![0u64; taille as usize / 8];
            let mut rendu = 0u32;
            // SAFETY : tampon à nous, de la taille donnée ; Windows dit la taille qu'il faut s'il est trop petit
            let st = unsafe {
                NtQuerySystemInformation(SystemProcessInformation, buf.as_mut_ptr().cast(), taille, &mut rendu)
            };
            if st.is_ok() {
                break;
            }
            if st.0 as u32 == 0xC000_0004 && taille < 64 * 1024 * 1024 {
                taille = rendu.max(taille * 2) + 64 * 1024;
                continue;
            }
            return Vec::new();
        }
        let octets: &[u8] = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast(), buf.len() * 8) };
        let lu64 = |o: usize| -> u64 {
            octets.get(o..o + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8]))).unwrap_or(0)
        };
        let lu32 = |o: usize| -> u32 {
            octets.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap_or([0; 4]))).unwrap_or(0)
        };
        let mut out = Vec::new();
        let mut o = 0usize;
        loop {
            // (SYSTEM_PROCESS_INFORMATION en 64 bits : UserTime 0x28, KernelTime 0x30, ImageName 0x38, UniqueProcessId
            // 0x50, WorkingSetSize 0x90)
            let suivant = lu32(o) as usize;
            let pid = lu64(o + 0x50) as u32;
            let n_octets = (lu32(o + 0x38) & 0xffff) as usize;
            let ptr = lu64(o + 0x40) as usize;
            let mut nom = String::new();
            if ptr != 0 && n_octets > 0 {
                // SAFETY : le nom est dans le même tampon (Windows y met un pointeur vers sa copie)
                let debut = buf.as_ptr() as usize;
                if ptr >= debut && ptr + n_octets <= debut + buf.len() * 8 {
                    let s: &[u16] = unsafe { std::slice::from_raw_parts(ptr as *const u16, n_octets / 2) };
                    nom = String::from_utf16_lossy(s);
                }
            } else if pid == 0 {
                nom = "Idle".into();
            }
            if nom.len() > 4 && nom[nom.len() - 4..].eq_ignore_ascii_case(".exe") {
                nom.truncate(nom.len() - 4);
            }
            let cpu = (lu64(o + 0x28) + lu64(o + 0x30)) as f64 / 1e7;
            out.push(Programme { pid, nom, cpu, memoire: lu64(o + 0x90) as f64 });
            if suivant == 0 {
                break;
            }
            o += suivant;
            if o >= octets.len() {
                break;
            }
        }
        out
    }

    fn chemin(&mut self, pid: u32) -> String {
        self.n += 2;
        // SAFETY : droit de lecture minimal, poignée fermée ici ; tampon à nous
        unsafe {
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
            let mut b = [0u16; 1024];
            let mut n = b.len() as u32;
            let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(b.as_mut_ptr()), &mut n);
            let _ = CloseHandle(h);
            if r.is_ok() { String::from_utf16_lossy(&b[..n as usize]) } else { String::new() }
        }
    }

    fn vivant(&mut self, pid: u32) -> bool {
        self.n += 2;
        // SAFETY : poignée de synchronisation seule, fermée ici
        unsafe {
            let Ok(h): Result<HANDLE, _> =
                OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            else {
                return false;
            };
            let r = WaitForSingleObject(h, 0) == WAIT_TIMEOUT;
            let _ = CloseHandle(h);
            r
        }
    }

    fn devant(&mut self) -> (u32, String) {
        self.n += 3;
        // SAFETY : lectures sur la fenêtre au premier plan, tampon à nous
        unsafe {
            let h = GetForegroundWindow();
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            let mut b = [0u16; 256];
            let n = GetWindowTextW(h, &mut b);
            (pid, String::from_utf16_lossy(&b[..n.max(0) as usize]))
        }
    }

    fn lancer(&mut self, chemin: &str, args: Option<&str>) -> bool {
        let mut c = std::process::Command::new(chemin);
        if let Some(a) = args {
            c.raw_arg(a);
        }
        if let Some(d) = std::path::Path::new(chemin).parent() {
            c.current_dir(d);
        }
        c.creation_flags(SANS_FENETRE).spawn().is_ok()
    }

    fn sur_batterie(&mut self) -> bool {
        self.n += 1;
        let mut s = SYSTEM_POWER_STATUS::default();
        // SAFETY : structure à nous
        unsafe { GetSystemPowerStatus(&mut s).is_ok() && s.ACLineStatus == 0 }
    }

    fn batterie(&mut self) -> bool {
        self.n += 1;
        let mut s = SYSTEM_POWER_STATUS::default();
        // SAFETY : structure à nous
        unsafe { GetSystemPowerStatus(&mut s).is_ok() && s.BatteryFlag != 128 && s.BatteryFlag != 255 }
    }

    fn ecrans(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        // SAFETY : structures à nous, leur taille est donnée
        unsafe {
            let mut i = 0;
            loop {
                self.n += 1;
                let mut d = DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
                if !EnumDisplayDevicesW(PCWSTR::null(), i, &mut d, 0).as_bool() {
                    break;
                }
                i += 1;
                if (d.StateFlags.0 & 1) == 0 {
                    continue;
                }
                let nom = HSTRING::from(texte_large(&d.DeviceName));
                let mut cur = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
                if !EnumDisplaySettingsW(&nom, ENUM_CURRENT_SETTINGS, &mut cur).as_bool() {
                    continue;
                }
                let mut max = cur.dmDisplayFrequency;
                let mut k = 0;
                loop {
                    let mut m = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
                    if !EnumDisplaySettingsW(&nom, ENUM_DISPLAY_SETTINGS_MODE(k), &mut m).as_bool() {
                        break;
                    }
                    k += 1;
                    if m.dmPelsWidth == cur.dmPelsWidth
                        && m.dmPelsHeight == cur.dmPelsHeight
                        && m.dmDisplayFrequency > max
                    {
                        max = m.dmDisplayFrequency;
                    }
                }
                self.n += u64::from(k);
                out.push(format!(
                    "{}|{}|{}|{}|{}|{}|{}",
                    texte_large(&d.DeviceName),
                    texte_large(&d.DeviceString),
                    cur.dmPelsWidth,
                    cur.dmPelsHeight,
                    cur.dmDisplayFrequency,
                    max,
                    if (d.StateFlags.0 & 4) != 0 { "1" } else { "0" }
                ));
            }
        }
        out
    }

    fn cartes(&mut self) -> Vec<String> {
        self.cartes_dxgi().iter().map(|c| c.0.clone()).collect()
    }

    fn coeurs(&self) -> usize {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    }

    fn disque_systeme(&mut self) -> Option<(f64, f64, String)> {
        self.n += 1;
        let lettre = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let (mut libre, mut total) = (0u64, 0u64);
        // SAFETY : sorties à nous
        unsafe {
            GetDiskFreeSpaceExW(&HSTRING::from(format!("{lettre}\\")), Some(&mut libre), Some(&mut total), None).ok()?
        };
        Some((total as f64, libre as f64, lettre))
    }

    fn priorite(&mut self, exe: &str) -> bool {
        self.n += 1;
        let cle =
            format!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\{exe}\PerfOptions");
        let mut v = 0u32;
        let mut n = 4u32;
        // SAFETY : lecture d'une valeur DWORD dans une variable à nous
        unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                &HSTRING::from(cle),
                &HSTRING::from("CpuPriorityClass"),
                RRF_RT_REG_DWORD,
                None,
                Some((&mut v as *mut u32).cast()),
                Some(&mut n),
            )
            .is_ok()
                && v == 3
        }
    }

    fn preference_gpu(&mut self, exe: &str) -> String {
        self.n += 1;
        let mut b = [0u16; 512];
        let mut n = (b.len() * 2) as u32;
        // SAFETY : lecture d'un texte dans un tampon à nous, sa taille est donnée
        unsafe {
            if RegGetValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(r"Software\Microsoft\DirectX\UserGpuPreferences"),
                &HSTRING::from(exe),
                RRF_RT_REG_SZ,
                None,
                Some(b.as_mut_ptr().cast()),
                Some(&mut n),
            )
            .is_ok()
            {
                texte_large(&b)
            } else {
                String::new()
            }
        }
    }

    fn fichier_existe(&mut self, chemin: &str) -> bool {
        self.n += 1;
        std::path::Path::new(chemin).is_file()
    }

    fn dossier_existe(&mut self, chemin: &str) -> bool {
        self.n += 1;
        std::path::Path::new(chemin).is_dir()
    }

    fn lectures(&self) -> u64 {
        self.n
    }
}

/// Le temps de processeur utilisé par Nevermind lui-même (100 ns), pour mesurer la légèreté
pub fn temps_processeur_soi() -> u64 {
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let (mut a, mut b, mut k, mut u) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY : pseudo-poignée du processus courant, structures à nous
    unsafe {
        let _ = GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut k, &mut u);
    }
    ft(k) + ft(u)
}
