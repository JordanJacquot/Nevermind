//! Le compteur de FPS par-dessus le jeu (Show-FpsOverlay, New-FpsOverlayContent, Set-OverlayPosition de jeu.ps1 ;
//! MakeOverlay de natif.cs) : une petite fenêtre native, sans bordure, transparente pixel par pixel
//! (UpdateLayeredWindow), que la souris traverse (WS_EX_TRANSPARENT), qui ne prend jamais le focus (WS_EX_NOACTIVATE) et
//! n'apparaît ni dans la barre des tâches ni dans Alt+Tab (WS_EX_TOOLWINDOW). Elle vit dans son propre fil, créé à la
//! première demande seulement : sans partie ni raccourci, rien n'existe. Ce fil reçoit aussi le raccourci Ctrl+Maj+F.
//!
//! Le dessin reprend la DA du compteur : « complet » (verre sombre, liseré cyan vers magenta, coins arrondis, chiffre en
//! couleur, « FPS », puis le 1 % bas et la moyenne) ou « discret » (juste « 144 FPS », blanc à 70 %, petite ombre).

use crate::systeme::{Compteur, EtatCompteur};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DIB_RGB_COLORS,
    DT_NOPREFIX, DT_SINGLELINE, DeleteDC, DeleteObject, DrawTextW, FIXED_PITCH, FW_BOLD, FW_SEMIBOLD, GetDC,
    GetMonitorInfoW, GetTextExtentPoint32W, HDC, HFONT, HGDIOBJ, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
    MonitorFromWindow, OUT_DEFAULT_PRECIS, ReleaseDC, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW, HWND_TOPMOST, MSG,
    PostMessageW, RegisterClassW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    ShowWindow, TranslateMessage, ULW_ALPHA, UpdateLayeredWindow, WM_APP, WM_HOTKEY, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{PCWSTR, w};

/// Le numéro du raccourci ($FpsHotkeyId)
const RACCOURCI: i32 = 7001;
const WM_ORDRE: u32 = WM_APP + 1;

enum Ordre {
    Montrer(EtatCompteur),
    Cacher,
    Devant,
    Raccourci(bool, Sender<bool>),
}

/// Le compteur, vu du bloc
#[derive(Default)]
pub struct CompteurWindows {
    fil: Option<(Sender<Ordre>, Arc<AtomicIsize>)>,
    presse: Arc<AtomicBool>,
}

impl CompteurWindows {
    fn envoyer(&mut self, o: Ordre) {
        if self.fil.is_none() {
            let (tx, rx) = channel();
            let hwnd = Arc::new(AtomicIsize::new(0));
            let h = hwnd.clone();
            let presse = self.presse.clone();
            let pret = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
            let p2 = pret.clone();
            let ok = std::thread::Builder::new()
                .name("nevermind-compteur".into())
                .spawn(move || fil(rx, h, presse, p2))
                .is_ok();
            if !ok {
                return;
            }
            // (on attend que la fenêtre existe : un ordre envoyé avant serait perdu)
            let (m, c) = &*pret;
            let mut fait = m.lock().unwrap_or_else(|e| e.into_inner());
            while !*fait {
                let (f, t) = c.wait_timeout(fait, std::time::Duration::from_secs(2)).unwrap_or_else(|e| e.into_inner());
                fait = f;
                if t.timed_out() {
                    break;
                }
            }
            self.fil = Some((tx, hwnd));
        }
        if let Some((tx, h)) = &self.fil {
            let _ = tx.send(o);
            let hwnd = HWND(h.load(Ordering::SeqCst) as *mut _);
            // SAFETY : simple message à notre fenêtre (le fil la réveille pour lire l'ordre)
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_ORDRE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

impl Compteur for CompteurWindows {
    fn montrer(&mut self, e: &EtatCompteur) {
        self.envoyer(Ordre::Montrer(e.clone()));
    }
    fn cacher(&mut self) {
        if self.fil.is_some() {
            self.envoyer(Ordre::Cacher);
        }
    }
    fn devant(&mut self) {
        if self.fil.is_some() {
            self.envoyer(Ordre::Devant);
        }
    }
    fn raccourci(&mut self, actif: bool) -> bool {
        if !actif && self.fil.is_none() {
            return true;
        }
        let (tx, rx) = channel();
        self.envoyer(Ordre::Raccourci(actif, tx));
        rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or(false)
    }
    fn raccourci_presse(&mut self) -> bool {
        self.presse.swap(false, Ordering::SeqCst)
    }
}

static PRESSE: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

extern "system" fn proc_fenetre(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_HOTKEY && w.0 as i32 == RACCOURCI {
        if let Some(p) = PRESSE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            p.store(true, Ordering::SeqCst);
        }
        return LRESULT(0);
    }
    // SAFETY : traitement par défaut de Windows
    unsafe { DefWindowProcW(h, msg, w, l) }
}

fn fil(
    rx: Receiver<Ordre>,
    h: Arc<AtomicIsize>,
    presse: Arc<AtomicBool>,
    pret: Arc<(Mutex<bool>, std::sync::Condvar)>,
) {
    *PRESSE.lock().unwrap_or_else(|e| e.into_inner()) = Some(presse);
    // SAFETY : classe et fenêtre à nous, dans ce fil ; la fenêtre n'est montrée que par Montrer
    let hwnd = unsafe {
        let inst = GetModuleHandleW(None).unwrap_or_default();
        let c = WNDCLASSW {
            lpfnWndProc: Some(proc_fenetre),
            hInstance: inst.into(),
            lpszClassName: w!("NevermindCompteurFps"),
            ..Default::default()
        };
        RegisterClassW(&c);
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            w!("NevermindCompteurFps"),
            w!("Nevermind FPS"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(inst.into()),
            None,
        )
    };
    let hwnd = hwnd.unwrap_or_default();
    h.store(hwnd.0 as isize, Ordering::SeqCst);
    {
        let (m, c) = &*pret;
        *m.lock().unwrap_or_else(|e| e.into_inner()) = true;
        c.notify_all();
    }
    let mut visible = false;
    let mut raccourci = false;
    let mut msg = MSG::default();
    // SAFETY : boucle des messages de ce fil
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_ORDRE {
                while let Ok(o) = rx.try_recv() {
                    match o {
                        Ordre::Montrer(e) => {
                            dessiner(hwnd, &e);
                            if !visible {
                                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                                visible = true;
                            }
                        }
                        Ordre::Cacher => {
                            if visible {
                                let _ = ShowWindow(hwnd, SW_HIDE);
                                visible = false;
                            }
                        }
                        Ordre::Devant => {
                            if visible {
                                let _ = SetWindowPos(
                                    hwnd,
                                    Some(HWND_TOPMOST),
                                    0,
                                    0,
                                    0,
                                    0,
                                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                                );
                            }
                        }
                        Ordre::Raccourci(actif, rep) => {
                            let ok = if actif && !raccourci {
                                raccourci =
                                    RegisterHotKey(Some(hwnd), RACCOURCI, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, 0x46)
                                        .is_ok();
                                raccourci
                            } else if !actif && raccourci {
                                let _ = UnregisterHotKey(Some(hwnd), RACCOURCI);
                                raccourci = false;
                                true
                            } else {
                                true
                            };
                            let _ = rep.send(ok);
                        }
                    }
                }
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Une image du compteur, en BGRA prémultiplié
struct Image {
    l: i32,
    h: i32,
    px: Vec<[f32; 4]>,
}

impl Image {
    /// Pose une couleur (r, g, b en 0..1, alpha a) avec la couverture c
    fn poser(&mut self, x: i32, y: i32, rgb: [f32; 3], a: f32) {
        if x < 0 || y < 0 || x >= self.l || y >= self.h || a <= 0.0 {
            return;
        }
        let p = &mut self.px[(y * self.l + x) as usize];
        let k = 1.0 - a;
        p[0] = rgb[2] * a + p[0] * k;
        p[1] = rgb[1] * a + p[1] * k;
        p[2] = rgb[0] * a + p[2] * k;
        p[3] = a + p[3] * k;
    }
}

fn rgb(c: u32) -> [f32; 3] {
    [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0]
}

/// Un texte : sa police, sa taille, sa place
struct Morceau {
    texte: Vec<u16>,
    police: HFONT,
    x: i32,
    y: i32,
    l: i32,
    h: i32,
    couleur: [f32; 3],
    opacite: f32,
}

unsafe fn police(px: i32, gras: bool) -> HFONT {
    // SAFETY : police créée ici, effacée par l'appelant
    unsafe {
        CreateFontW(
            -px,
            0,
            0,
            0,
            if gras { FW_BOLD.0 as i32 } else { FW_SEMIBOLD.0 as i32 },
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            FIXED_PITCH.0 as u32,
            w!("Consolas"),
        )
    }
}

unsafe fn mesurer(dc: HDC, f: HFONT, t: &[u16]) -> (i32, i32) {
    // SAFETY : DC et police valides
    unsafe {
        let old = SelectObject(dc, HGDIOBJ(f.0));
        let mut s = SIZE::default();
        let _ = GetTextExtentPoint32W(dc, t, &mut s);
        SelectObject(dc, old);
        (s.cx, s.cy)
    }
}

/// Dessine le texte en blanc sur noir dans un DIB à part, puis le pose en couleur avec sa couverture
unsafe fn poser_texte(img: &mut Image, dc: HDC, m: &Morceau, decalage: (i32, i32), couleur: [f32; 3], opacite: f32) {
    // SAFETY : DIB et DC créés et effacés ici
    unsafe {
        let (l, h) = (m.l.max(1), m.h.max(1));
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: l,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let Ok(bmp) = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else { return };
        let old = SelectObject(dc, HGDIOBJ(bmp.0));
        let oldf = SelectObject(dc, HGDIOBJ(m.police.0));
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0x00ff_ffff));
        let mut r = RECT { left: 0, top: 0, right: l, bottom: h };
        let mut t = m.texte.clone();
        DrawTextW(dc, &mut t, &mut r, DT_SINGLELINE | DT_NOPREFIX);
        let px = std::slice::from_raw_parts(bits as *const u8, (l * h * 4) as usize);
        for y in 0..h {
            for x in 0..l {
                let i = ((y * l + x) * 4) as usize;
                let cov = f32::from(px[i].max(px[i + 1]).max(px[i + 2])) / 255.0;
                img.poser(m.x + x + decalage.0, m.y + y + decalage.1, couleur, cov * opacite);
            }
        }
        SelectObject(dc, oldf);
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
    }
}

fn dessiner(hwnd: HWND, e: &EtatCompteur) {
    // SAFETY : tous les objets GDI sont créés et effacés ici ; la fenêtre est la nôtre
    unsafe {
        let devant = GetForegroundWindow();
        let mon = MonitorFromWindow(devant, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        let k = dx as f32 / 96.0;
        let p = |v: f32| (v * k).round() as i32;
        let ecran = GetDC(None);
        let dc = CreateCompatibleDC(Some(ecran));
        let discret = e.style == "discret";
        let chiffre: Vec<u16> = e.chiffre.encode_utf16().collect();
        let unite: Vec<u16> = "FPS".encode_utf16().collect();
        let sous: Vec<u16> = e.sous.encode_utf16().collect();
        let (f_chiffre, f_unite, f_sous) = if discret {
            (police(p(15.0), false), police(p(10.0), false), police(p(10.0), false))
        } else {
            (police(p(26.0), true), police(p(12.0), false), police(p(11.5), false))
        };
        let (lc, hc) = mesurer(dc, f_chiffre, &chiffre);
        let (lu, hu) = mesurer(dc, f_unite, &unite);
        let (ls, hs) = if discret || sous.is_empty() { (0, 0) } else { mesurer(dc, f_sous, &sous) };
        let (pg, ph, pd, pb) =
            if discret { (p(4.0), p(2.0), p(4.0), p(2.0)) } else { (p(12.0), p(5.0), p(14.0), p(7.0)) };
        let ecart = if discret { p(3.0) } else { p(6.0) };
        let ligne_l = lc + ecart + lu;
        let l = pg + ligne_l.max(ls) + pd + 2;
        let h = ph + hc + hs + pb + 2;
        let mut img = Image { l, h, px: vec![[0.0; 4]; (l * h) as usize] };
        if !discret {
            // Verre sombre (#D90B0820), liseré cyan vers magenta (#B000E5FF, #B0FF2EB5), coins arrondis
            let rayon = p(12.0) as f32;
            let fond = rgb(0x0B0820);
            for y in 0..h {
                for x in 0..l {
                    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                    let cx = fx.clamp(rayon, l as f32 - rayon);
                    let cy = fy.clamp(rayon, h as f32 - rayon);
                    let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                    let dedans = (rayon - d + 0.5).clamp(0.0, 1.0);
                    if dedans <= 0.0 {
                        continue;
                    }
                    let bord = (fx.min(l as f32 - fx).min(fy.min(h as f32 - fy)) < 1.0 + k) || d > rayon - 1.0 - k;
                    if bord {
                        let t = ((x as f32 / l as f32) + (y as f32 / h as f32)) / 2.0;
                        let (a, b) = (rgb(0x00E5FF), rgb(0xFF2EB5));
                        let c = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
                        img.poser(x, y, fond, 0.85 * dedans);
                        img.poser(x, y, c, 0.69 * dedans);
                    } else {
                        img.poser(x, y, fond, 0.85 * dedans);
                    }
                }
            }
        }
        let base = ph + hc;
        let m_chiffre = Morceau {
            texte: chiffre,
            police: f_chiffre,
            x: pg,
            y: ph,
            l: lc,
            h: hc,
            couleur: if discret { [1.0; 3] } else { rgb(e.couleur) },
            opacite: if discret { 0.7 } else { 1.0 },
        };
        let m_unite = Morceau {
            texte: unite,
            police: f_unite,
            x: pg + lc + ecart,
            y: base - hu - if discret { p(2.0) } else { p(5.0) } + p(2.0),
            l: lu,
            h: hu,
            couleur: if discret { [1.0; 3] } else { rgb(0xA6A1BC) },
            opacite: if discret { 0.7 } else { 1.0 },
        };
        let morceaux: Vec<Morceau> = if discret || ls == 0 {
            vec![m_chiffre, m_unite]
        } else {
            vec![
                m_chiffre,
                m_unite,
                Morceau {
                    texte: sous,
                    police: f_sous,
                    x: pg,
                    y: base,
                    l: ls,
                    h: hs,
                    couleur: rgb(0xD3CDE3),
                    opacite: 1.0,
                },
            ]
        };
        for m in &morceaux {
            if discret {
                // (petite ombre : le chiffre reste lisible sur un décor clair)
                poser_texte(&mut img, dc, m, (1, 1), [0.0; 3], 0.6);
            }
            poser_texte(&mut img, dc, m, (0, 0), m.couleur, m.opacite);
        }
        // L'image dans un DIB, puis à l'écran
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: l,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        if let Ok(bmp) = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0) {
            let out = std::slice::from_raw_parts_mut(bits as *mut u8, (l * h * 4) as usize);
            for (i, p) in img.px.iter().enumerate() {
                for c in 0..4 {
                    out[i * 4 + c] = (p[c].clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
            let old = SelectObject(dc, HGDIOBJ(bmp.0));
            let b = mi.rcMonitor;
            let marge = 16;
            let x = if e.coin.ends_with('d') { b.right - marge - l } else { b.left + marge };
            let y = if e.coin.starts_with('b') { b.bottom - marge - h } else { b.top + marge };
            let pos = POINT { x, y };
            let taille = SIZE { cx: l, cy: h };
            let src = POINT { x: 0, y: 0 };
            let bf = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                hwnd,
                Some(ecran),
                Some(&pos),
                Some(&taille),
                Some(dc),
                Some(&src),
                COLORREF(0),
                Some(&bf),
                ULW_ALPHA,
            );
            SelectObject(dc, old);
            let _ = DeleteObject(HGDIOBJ(bmp.0));
        }
        for f in [f_chiffre, f_unite, f_sous] {
            let _ = DeleteObject(HGDIOBJ(f.0));
        }
        let _ = DeleteDC(dc);
        ReleaseDC(None, ecran);
        let _ = PCWSTR::null();
    }
}
