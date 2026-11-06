//! Ce que les tests demandent à Windows : le ping (IcmpSendEcho, comme `System.Net.NetworkInformation.Ping`), les
//! flux du débit (WinHTTP, comme `HttpWebRequest` de natif.cs), la priorité basse des fils du test du processeur, la
//! santé d'un disque (MSFT_PhysicalDisk), nvidia-smi, la recherche d'erreurs (Repair-Volume, sans fenêtre) et la
//! fenêtre des couleurs en plein écran (`Start-PixelTest`). Le seul endroit des tests où le code `unsafe` est permis.
#![allow(unsafe_code)]

use super::mesures::{Compteur, Reseau};
use serde_json::{Value, json};
use std::ffi::c_void;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::Win32::Foundation::{COLORREF, HANDLE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DT_CENTER, DT_WORDBREAK, DeleteObject, DrawTextW, EndPaint,
    EnumDisplayMonitors, FillRect, GetMonitorInfoW, HDC, HGDIOBJ, HMONITOR, InvalidateRect, MONITORINFO,
    MONITORINFOEXW, PAINTSTRUCT, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::NetworkManagement::IpHelper::{ICMP_ECHO_REPLY, IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho};
use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPEN_REQUEST_FLAGS, WinHttpCloseHandle,
    WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts, WinHttpWriteData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, MSG,
    PostQuitMessage, RegisterClassW, SetCursor, SetForegroundWindow, TranslateMessage, WM_DESTROY, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_PAINT, WM_SETCURSOR, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::core::{BOOL, HSTRING, PCWSTR, w};

/// Les fils du test « tous les cœurs » passent en priorité basse (`ThreadPriority.BelowNormal`)
pub fn abaisser() {
    // SAFETY : le fil courant, toujours valable
    let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) };
}

/// Le vrai réseau
pub struct ReseauWindows;

struct P(*mut c_void);
impl Drop for P {
    fn drop(&mut self) {
        // SAFETY : une poignée WinHTTP ouverte ici, fermée une seule fois
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}
fn p(x: *mut c_void) -> Result<P, String> {
    if x.is_null() { Err(windows::core::Error::from_thread().message()) } else { Ok(P(x)) }
}

impl Reseau for ReseauWindows {
    fn ping(&self, hote: &str, delai_ms: u32) -> Option<f64> {
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
            (r.Status == 0).then_some(r.RoundTripTime as f64)
        }
    }

    fn flux(&self, url: &str, envoi: bool, c: &Compteur) -> Result<(), String> {
        let (securise, hote, port, chemin) = nevermind_noyau::windows::http::decouper(url)?;
        let m = |e: windows::core::Error| e.message();
        // SAFETY : des chaînes terminées par un zéro, vivantes pendant chaque appel ; les poignées sont refermées par P
        unsafe {
            let s = p(WinHttpOpen(
                &HSTRING::from("Nevermind"),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ))?;
            WinHttpSetTimeouts(s.0, 10000, 10000, 10000, 10000).map_err(m)?;
            let cx = p(WinHttpConnect(s.0, &HSTRING::from(hote), port, 0))?;
            let drapeaux = if securise { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
            let verbe = if envoi { "POST" } else { "GET" };
            let d = p(WinHttpOpenRequest(
                cx.0,
                &HSTRING::from(verbe),
                &HSTRING::from(chemin),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                drapeaux,
            ))?;
            if envoi {
                // (400 morceaux de 64 Ko au plus par demande, comme natif.cs)
                const MORCEAU: usize = 65536;
                let mut donnees = vec![0u8; MORCEAU];
                let mut x = 3u64;
                for b in donnees.iter_mut() {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    *b = x as u8;
                }
                let entetes: Vec<u16> = "Content-Type: application/octet-stream".encode_utf16().collect();
                WinHttpSendRequest(d.0, Some(&entetes), None, 0, (400 * MORCEAU) as u32, 0).map_err(m)?;
                for _ in 0..400 {
                    if !c.encore() {
                        // (le temps est fini : la demande est abandonnée, ce qui est passé est compté)
                        return Ok(());
                    }
                    let mut ecrit = 0u32;
                    WinHttpWriteData(d.0, Some(donnees.as_ptr().cast()), MORCEAU as u32, &mut ecrit).map_err(m)?;
                    c.ajouter(ecrit as u64);
                }
                let _ = WinHttpReceiveResponse(d.0, std::ptr::null_mut());
            } else {
                WinHttpSendRequest(d.0, None, None, 0, 0, 0).map_err(m)?;
                WinHttpReceiveResponse(d.0, std::ptr::null_mut()).map_err(m)?;
                let mut tampon = vec![0u8; 65536];
                loop {
                    let mut lu = 0u32;
                    WinHttpReadData(d.0, tampon.as_mut_ptr().cast(), tampon.len() as u32, &mut lu).map_err(m)?;
                    if lu == 0 {
                        break;
                    }
                    c.ajouter(lu as u64);
                    if !c.encore() {
                        break;
                    }
                }
            }
            Ok(())
        }
    }
}

/// `$SmiPath` : nvidia-smi, s'il est là
pub fn chemin_smi() -> Option<String> {
    let windir = std::env::var("windir").unwrap_or_else(|_| r"C:\Windows".into());
    let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
    [format!(r"{windir}\System32\nvidia-smi.exe"), format!(r"{pf}\NVIDIA Corporation\NVSMI\nvidia-smi.exe")]
        .into_iter()
        .find(|p| std::path::Path::new(p).exists())
}

/// La première ligne de nvidia-smi pour la carte graphique en direct (sans fenêtre)
pub fn smi() -> Option<String> {
    let exe = chemin_smi()?;
    let (_, o) = crate::windows::commande(
        &exe,
        &[format!("--query-gpu={}", super::rendus::CHAMPS_SMI), "--format=csv,noheader,nounits".into()],
    );
    o.lines().next().map(str::to_string)
}

/// Un disque relu (`$DiskInfoWork` : Get-PhysicalDisk), rien s'il est introuvable
pub fn disque(id: &str) -> Option<Value> {
    use crate::windows::wmi::{self, STOCKAGE, Valeur};
    let champs =
        ["DeviceId", "FriendlyName", "MediaType", "BusType", "HealthStatus", "Size", "SerialNumber", "FirmwareVersion"];
    let l = wmi::requete(STOCKAGE, &format!("SELECT {} FROM MSFT_PhysicalDisk", champs.join(", ")), &champs).ok()?;
    let t = |x: &Valeur| match x {
        Valeur::Texte(s) => s.clone(),
        Valeur::Entier(n) => n.to_string(),
        _ => String::new(),
    };
    let n = |x: &Valeur| match x {
        Valeur::Texte(s) => s.trim().parse::<i64>().unwrap_or(0),
        Valeur::Entier(i) => *i,
        _ => 0,
    };
    let x = l.into_iter().find(|x| x.first().map(t).as_deref() == Some(id))?;
    let media = match x.get(2).map(n).unwrap_or(0) {
        3 => "HDD",
        4 => "SSD",
        5 => "SCM",
        _ => "Unspecified",
    };
    let bus = match x.get(3).map(n).unwrap_or(0) {
        1 => "SCSI",
        2 => "ATAPI",
        3 => "ATA",
        4 => "1394",
        5 => "SSA",
        6 => "Fibre Channel",
        7 => "USB",
        8 => "RAID",
        9 => "iSCSI",
        10 => "SAS",
        11 => "SATA",
        12 => "SD",
        13 => "MMC",
        15 => "File Backed Virtual",
        16 => "Storage Spaces",
        17 => "NVMe",
        _ => "Unknown",
    };
    let sante = match x.get(4).map(n).unwrap_or(-1) {
        0 => "Healthy",
        1 => "Warning",
        2 => "Unhealthy",
        _ => "Unknown",
    };
    let taille = match x.get(5) {
        Some(Valeur::Texte(s)) => s.trim().parse::<f64>().unwrap_or(0.0),
        Some(Valeur::Entier(i)) => *i as f64,
        _ => 0.0,
    };
    Some(
        json!({ "DeviceId": id, "FriendlyName": x.get(1).map(t).unwrap_or_default(), "MediaType": media, "BusType": bus,
        "HealthStatus": sante, "Size": taille, "SerialNumber": x.get(6).map(t).unwrap_or_default(), "FirmwareVersion": x.get(7).map(t).unwrap_or_default() }),
    )
}

/// La recherche d'erreurs d'un lecteur (`Repair-Volume -Scan`, sans fenêtre) : son résultat, ou le message d'erreur
pub fn verifier_volume(lettre: &str) -> Result<String, String> {
    if !lettre.chars().all(|c| c.is_ascii_alphabetic()) || lettre.len() != 1 {
        return Err("Lecteur inattendu.".into());
    }
    let windir = std::env::var("windir").unwrap_or_else(|_| r"C:\Windows".into());
    let ps = format!(r"{windir}\System32\WindowsPowerShell\v1.0\powershell.exe");
    let script = format!(
        "try {{ [string](Repair-Volume -DriveLetter {lettre} -Scan -ErrorAction Stop) }} catch {{ 'ERR ' + $_.Exception.Message }}"
    );
    let (_, o) = crate::windows::commande(
        &ps,
        &[
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-Command".into(),
            script,
        ],
    );
    let o = o.trim().to_string();
    match o.strip_prefix("ERR ") {
        Some(e) => Err(e.to_string()),
        None if o.is_empty() => Err("Pas de réponse de Windows.".into()),
        None => Ok(o),
    }
}

// ---------------------------------------------------------------------------
// Les couleurs en plein écran (Start-PixelTest)
// ---------------------------------------------------------------------------

const COULEURS: [u32; 6] = [0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0x808080];
static INDEX: AtomicUsize = AtomicUsize::new(0);

fn colorref(rgb: u32) -> COLORREF {
    COLORREF(((rgb >> 16) & 0xFF) | (rgb & 0xFF00) | ((rgb & 0xFF) << 16))
}

unsafe extern "system" fn procedure(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let suivant = |h: HWND| {
        let i = INDEX.fetch_add(1, Ordering::Relaxed) + 1;
        // SAFETY : la fenêtre de ce fil
        unsafe {
            if i >= COULEURS.len() {
                let _ = DestroyWindow(h);
            } else {
                let _ = InvalidateRect(Some(h), None, true);
            }
        }
    };
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            // SAFETY : dessin dans la fenêtre de ce fil, entre BeginPaint et EndPaint ; objets GDI rendus ensuite
            unsafe {
                let hdc: HDC = BeginPaint(h, &mut ps);
                let i = INDEX.load(Ordering::Relaxed).min(COULEURS.len() - 1);
                let pinceau = CreateSolidBrush(colorref(COULEURS[i]));
                FillRect(hdc, &ps.rcPaint, pinceau);
                let _ = DeleteObject(HGDIOBJ(pinceau.0));
                if i == 0 {
                    let police = CreateFontW(
                        -27,
                        0,
                        0,
                        0,
                        400,
                        0,
                        0,
                        0,
                        Default::default(),
                        Default::default(),
                        Default::default(),
                        Default::default(),
                        0,
                        w!("Segoe UI"),
                    );
                    let ancienne = SelectObject(hdc, HGDIOBJ(police.0));
                    SetBkMode(hdc, TRANSPARENT);
                    SetTextColor(hdc, colorref(0xA6A1BC));
                    let mut r = RECT::default();
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(h, &mut r);
                    r.top = r.bottom / 2 - 60;
                    let mut t: Vec<u16> = "Cherche les points qui ne sont pas de la bonne couleur.\n\nClic ou Espace : couleur suivante        Échap : quitter"
                        .encode_utf16()
                        .collect();
                    DrawTextW(hdc, &mut t, &mut r, DT_CENTER | DT_WORDBREAK);
                    SelectObject(hdc, ancienne);
                    let _ = DeleteObject(HGDIOBJ(police.0));
                }
                let _ = EndPaint(h, &ps);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            suivant(h);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            match wp.0 {
                // (Échap)
                0x1B => {
                    // SAFETY : la fenêtre de ce fil
                    let _ = unsafe { DestroyWindow(h) };
                }
                // (Espace, Entrée, flèche droite)
                0x20 | 0x0D | 0x27 => suivant(h),
                _ => {}
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            // SAFETY : pas de curseur au-dessus de la fenêtre
            unsafe { SetCursor(None) };
            LRESULT(1)
        }
        WM_DESTROY => {
            // SAFETY : fin de la boucle de messages de ce fil
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY : le traitement par défaut de Windows
        _ => unsafe { DefWindowProcW(h, msg, wp, lp) },
    }
}

unsafe extern "system" fn un_ecran(m: HMONITOR, _: HDC, _: *mut RECT, donnees: LPARAM) -> BOOL {
    // SAFETY : `donnees` est l'adresse de la liste passée à EnumDisplayMonitors, vivante pendant l'appel
    let l = unsafe { &mut *(donnees.0 as *mut Vec<(String, RECT)>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY : structure initialisée avec sa taille
    if unsafe { GetMonitorInfoW(m, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) }.as_bool() {
        let n = info.szDevice.iter().position(|c| *c == 0).unwrap_or(info.szDevice.len());
        l.push((String::from_utf16_lossy(&info.szDevice[..n]), info.monitorInfo.rcMonitor));
    }
    BOOL(1)
}

/// Les couleurs unies en plein écran sur l'écran `appareil` (« \\.\DISPLAY1 »), dans un fil à part : clic ou Espace
/// pour la couleur suivante, Échap pour quitter
pub fn pixels(appareil: String) {
    std::thread::spawn(move || {
        let mut ecrans: Vec<(String, RECT)> = Vec::new();
        // SAFETY : la liste vit pendant l'appel ; la fenêtre et sa boucle de messages sont celles de ce fil
        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(un_ecran), LPARAM(&mut ecrans as *mut _ as isize));
            let Some(r) =
                ecrans.iter().find(|(n, _)| n.eq_ignore_ascii_case(&appareil)).or(ecrans.first()).map(|x| x.1)
            else {
                return;
            };
            INDEX.store(0, Ordering::Relaxed);
            let instance = GetModuleHandleW(None).unwrap_or_default();
            let classe = w!("NevermindPixels");
            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: classe,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let Ok(h) = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                classe,
                w!("Nevermind"),
                WS_POPUP | WS_VISIBLE,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                None,
                None,
                Some(instance.into()),
                None,
            ) else {
                return;
            };
            let _ = SetForegroundWindow(h);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
}
