//! La barre flottante de l'Organizer (Show-OrgBar, Update-OrgBar, Hide-OrgBar, Set-OrgBarOnTop, Reset-OrgBarPosition,
//! Test-OrgBarOnScreen) : un bouton par perso connecté, par-dessus le jeu, sans jamais lui prendre le focus ni
//! apparaître dans Alt+Tab. DA de la console : verre sombre, bord de plasma (cyan, violet, magenta du thème) qui tourne
//! lentement (un tour en 9 s, 20 images par seconde au plus), le perso devant mis en avant par un bord de plasma
//! lumineux et sa lueur. L'animation s'arrête dès que la barre est cachée : rien de lourd par-dessus le jeu.

use super::dessin::{self, Couleur, Mise, Peinture, Police, Toile};
use super::{Etat, Theme, hwnd};
use crate::organizer::veille::Pastille;
use crate::organizer::vue::{AUCUN_CONNECTE, GLISSER_BARRE};
use std::time::Instant;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, HDC, HGDIOBJ, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    TOOLTIPS_CLASSW, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_DELTOOLW, TTM_SETMAXTIPWIDTH, TTS_ALWAYSTIP, TTS_NOPREFIX,
    TTTOOLINFOW, WM_MOUSELEAVE,
};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, GetWindowRect, HWND_TOPMOST, IDC_ARROW, IDC_HAND,
    IDC_SIZEALL, KillTimer, LoadCursorW, MA_NOACTIVATE, RegisterClassExW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SPI_GETWORKAREA, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SendMessageW, SetCursor,
    SetTimer, SetWindowPos, ShowWindow, SystemParametersInfoW, ULW_ALPHA, UpdateLayeredWindow, WINDOW_STYLE,
    WM_DPICHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_SETCURSOR, WM_TIMER, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, w};

const ANIMATION: usize = 2;

/// La place de chaque morceau de la barre (en pixels).
#[derive(Debug, Clone, Default)]
struct Dispo {
    l: i32,
    h: i32,
    grip: (f32, f32, f32, f32),
    /// Chaque pastille : sa boîte (déjà grossie pour celle devant).
    pastilles: Vec<(f32, f32, f32, f32)>,
}

struct Dib {
    dc: HDC,
    bmp: HBITMAP,
    bits: *mut u32,
    l: i32,
    h: i32,
}

pub struct Barre {
    pub theme: Theme,
    h: isize,
    infobulle: isize,
    position: (Option<f64>, Option<f64>),
    pastilles: Vec<Pastille>,
    dispo: Dispo,
    statique: Toile,
    image: Toile,
    visible: bool,
    survol: Option<usize>,
    suivi: bool,
    appui: Option<usize>,
    glisse: Option<(POINT, POINT)>,
    debut: Instant,
    dib: Option<Dib>,
    echelle: f32,
    recentrer_apres: bool,
}

/// L'échelle des pixels indépendants (WPF) de tout le PC : celle des positions gardées dans organizer.json.
fn echelle_systeme() -> f64 {
    f64::from(unsafe { GetDpiForSystem() }.max(96)) / 96.0
}

fn zone_travail() -> RECT {
    let mut r = RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut r as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    r
}

/// Test-OrgBarOnScreen (en pixels indépendants) : la position est-elle encore sur un écran branché ?
pub fn sur_un_ecran(x: f64, y: f64) -> bool {
    let s = echelle_systeme();
    let (l, t, w, h) = unsafe {
        (
            f64::from(GetSystemMetrics(SM_XVIRTUALSCREEN)) / s,
            f64::from(GetSystemMetrics(SM_YVIRTUALSCREEN)) / s,
            f64::from(GetSystemMetrics(SM_CXVIRTUALSCREEN)) / s,
            f64::from(GetSystemMetrics(SM_CYVIRTUALSCREEN)) / s,
        )
    };
    x >= l - 20.0 && x <= l + w - 60.0 && y >= t - 10.0 && y <= t + h - 30.0
}

impl Barre {
    pub fn nouvelle(theme: Theme, x: Option<f64>, y: Option<f64>) -> Self {
        Barre {
            theme,
            h: 0,
            infobulle: 0,
            position: (x, y),
            pastilles: Vec::new(),
            dispo: Dispo::default(),
            statique: Toile::nouvelle(1, 1),
            image: Toile::nouvelle(1, 1),
            visible: false,
            survol: None,
            suivi: false,
            appui: None,
            glisse: None,
            debut: Instant::now(),
            dib: None,
            echelle: 1.0,
            recentrer_apres: false,
        }
    }

    fn police(&self, taille: f32, graisse: i32) -> Police {
        Police { familles: self.theme.police, taille: (taille * self.echelle).round() as i32, graisse }
    }

    fn creer(&mut self) {
        if self.h != 0 {
            return;
        }
        unsafe {
            let Ok(inst) = GetModuleHandleW(PCWSTR::null()) else { return };
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(proc_barre),
                hInstance: inst.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: w!("NevermindOrganizerBarre"),
                ..Default::default()
            };
            RegisterClassExW(&wc);
            // Position : celle gardée, sinon en haut au centre (comme Show-OrgBar)
            let s = echelle_systeme();
            let wa = zone_travail();
            let (wl, wt, ww) = (f64::from(wa.left) / s, f64::from(wa.top) / s, f64::from(wa.right - wa.left) / s);
            let x = self.position.0.unwrap_or(wl + ww / 2.0 - 150.0);
            let y = self.position.1.unwrap_or(wt + 6.0);
            let Ok(h) = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("NevermindOrganizerBarre"),
                w!("Nevermind Organizer"),
                WS_POPUP,
                (x * s).round() as i32,
                (y * s).round() as i32,
                10,
                10,
                None,
                None,
                Some(inst.into()),
                None,
            ) else {
                return;
            };
            self.h = h.0 as isize;
            self.echelle = GetDpiForWindow(h).max(96) as f32 / 96.0;
            // (l'infobulle de chaque pastille, comme le ToolTip de WPF)
            if let Ok(t) = CreateWindowExW(
                WS_EX_TOPMOST,
                TOOLTIPS_CLASSW,
                PCWSTR::null(),
                WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
                0,
                0,
                0,
                0,
                Some(h),
                None,
                Some(inst.into()),
                None,
            ) {
                self.infobulle = t.0 as isize;
                SendMessageW(t, TTM_SETMAXTIPWIDTH, None, Some(LPARAM(400)));
            }
            // Hors de l'écran (écran débranché) : retour en haut au centre
            if !sur_un_ecran(x, y) {
                self.position = (None, None);
                self.recentrer_apres = true;
            }
        }
    }

    /// Update-OrgBar.
    pub fn dessiner(&mut self, pastilles: &[Pastille]) {
        self.creer();
        if self.h == 0 {
            return;
        }
        self.pastilles = pastilles.to_vec();
        self.survol = None;
        self.disposer();
        self.peindre_statique();
        self.infobulles();
        if self.recentrer_apres {
            self.recentrer_apres = false;
            self.recentrer();
        }
        self.image_suivante();
    }

    fn disposer(&mut self) {
        let s = self.echelle;
        let icones = Police {
            familles: "Segoe Fluent Icons, Segoe MDL2 Assets",
            taille: (12.0 * s).round() as i32,
            graisse: 400,
        };
        let (gl, gh) = dessin::mesurer("\u{E76F}", &icones, None);
        let grip_l = gl as f32 + 8.0 * s;
        let nom = self.police(12.0, 600);
        let mut x = 1.0 + 5.0 * s + grip_l;
        let mut haut = gh as f32;
        let mut boites = Vec::new();
        let rangees: Vec<(f32, f32)> = self
            .pastilles
            .iter()
            .map(|p| {
                let (tl, th) = dessin::mesurer(&p.nom, &nom, None);
                ((tl as f32).min(110.0 * s), th as f32)
            })
            .collect();
        let ph = rangees.iter().map(|r| r.1).fold(22.0 * s, f32::max) + 6.0 * s;
        for (p, (tl, _)) in self.pastilles.iter().zip(&rangees) {
            x += 3.0 * s;
            let pl = 4.0 * s + 22.0 * s + 7.0 * s + tl + 12.0 * s;
            boites.push((x, 0.0, pl, ph, p.devant));
            x += pl + 3.0 * s;
        }
        if self.pastilles.is_empty() {
            let (tl, th) = dessin::mesurer(AUCUN_CONNECTE, &self.police(12.0, 400), None);
            boites.clear();
            x += 12.0 * s + tl as f32;
            haut = haut.max(th as f32 + 8.0 * s);
        } else {
            haut = haut.max(ph);
        }
        let l = (x + 8.0 * s + 1.0).ceil() as i32;
        let h = (1.0 + 6.0 * s + haut + 6.0 * s + 1.0).ceil() as i32;
        let y0 = 1.0 + 6.0 * s;
        self.dispo = Dispo {
            l,
            h,
            grip: (1.0 + 5.0 * s, y0, grip_l, haut),
            pastilles: boites
                .into_iter()
                .map(|(x, _, w, hh, devant)| {
                    let y = y0 + (haut - hh) / 2.0;
                    // (le perso devant, un peu plus grand : ScaleTransform 1.06 autour du centre)
                    if devant { (x - w * 0.03, y - hh * 0.03, w * 1.06, hh * 1.06) } else { (x, y, w, hh) }
                })
                .collect(),
        };
    }

    fn peindre_statique(&mut self) {
        let s = self.echelle;
        let d = self.dispo.clone();
        let mut t = Toile::nouvelle(d.l, d.h);
        let r = self.theme.rayon(24.0, 3.0) * s;
        t.rect_arrondi(
            0.0,
            0.0,
            d.l as f32,
            d.h as f32,
            r,
            &Peinture::Vertical(Couleur::hex("#F5100E22"), Couleur::hex("#F206050F")),
        );
        let icones = Police {
            familles: "Segoe Fluent Icons, Segoe MDL2 Assets",
            taille: (12.0 * s).round() as i32,
            graisse: 400,
        };
        let g = d.grip;
        dessin::ecrire(
            &mut t,
            "\u{E76F}",
            &icones,
            Couleur::hex("#6F6894"),
            g.0 as i32,
            g.1 as i32,
            g.2.ceil() as i32,
            g.3.ceil() as i32,
            Mise::Centre,
        );
        if self.pastilles.is_empty() {
            let x = g.0 + g.2 + 6.0 * s;
            dessin::ecrire(
                &mut t,
                AUCUN_CONNECTE,
                &self.police(12.0, 400),
                Couleur::hex("#A8A3C4"),
                x as i32,
                g.1 as i32,
                (d.l as f32 - x) as i32,
                g.3 as i32,
                Mise::Ligne,
            );
        }
        let rp = self.theme.rayon(16.0, 2.0) * s;
        for (p, b) in self.pastilles.iter().zip(&d.pastilles) {
            if p.devant {
                // Lueur : un bord flou fixe (ne tourne pas)
                let marge = (20.0 * s).ceil();
                let mut lueur = Toile::nouvelle((b.2 + 2.0 * marge) as i32, (b.3 + 2.0 * marge) as i32);
                lueur.contour_arrondi(marge, marge, b.2, b.3, rp, 3.0 * s, &self.theme.plasma(0.0, 1.0));
                lueur.flouter((10.0 * s).round() as i32);
                t.coller(&lueur, (b.0 - marge) as i32, (b.1 - marge) as i32, 1.0);
                t.rect_arrondi(
                    b.0,
                    b.1,
                    b.2,
                    b.3,
                    rp,
                    &Peinture::Vertical(Couleur::hex("#E61A1538"), Couleur::hex("#E60F0C24")),
                );
            } else {
                t.rect_arrondi(b.0, b.1, b.2, b.3, rp, &Peinture::Unie(Couleur::hex("#C8181433")));
                t.contour_arrondi(b.0, b.1, b.2, b.3, rp, 1.0 * s, &Peinture::Unie(Couleur::hex("#25223F")));
            }
            let k = b.3 / (22.0 * s + 6.0 * s).max(1.0);
            let k = if p.devant { k } else { 1.0 };
            // Pastille : l'initiale sur la couleur de la classe
            let pv = crate::organizer::vue::PastilleVue::de(&p.nom, &p.classe);
            let op = if p.devant { 1.0 } else { 0.75 };
            let rb = 11.0 * s * k;
            let (cx, cy) = (b.0 + 4.0 * s * k + rb, b.1 + b.3 / 2.0);
            let col = Couleur::hex(pv.couleur);
            t.disque(cx, cy, rb, &Peinture::Vertical(col.opacite(op), Couleur::hex("#33000000").opacite(op)));
            t.cercle(cx, cy, rb, 1.0 * s, &Peinture::Unie(Couleur::hex("#55FFFFFF").opacite(op)));
            let fi = Police { familles: self.theme.police, taille: (22.0 * 0.45 * s * k).round() as i32, graisse: 700 };
            dessin::ecrire(
                &mut t,
                &pv.initiale,
                &fi,
                Couleur::hex("#FFFFFF").opacite(op),
                (cx - rb) as i32,
                (cy - rb) as i32,
                (2.0 * rb).ceil() as i32,
                (2.0 * rb).ceil() as i32,
                Mise::Centre,
            );
            let tx = cx + rb + 7.0 * s * k;
            let tl = b.0 + b.2 - 12.0 * s * k - tx;
            let pol = Police { familles: self.theme.police, taille: (12.0 * s * k).round() as i32, graisse: 600 };
            let c = Couleur::hex(if p.devant { "#FFFFFF" } else { "#A8A3C4" });
            dessin::ecrire(
                &mut t,
                &p.nom,
                &pol,
                c,
                tx as i32,
                b.1 as i32,
                tl.ceil() as i32,
                b.3.ceil() as i32,
                Mise::Ligne,
            );
        }
        self.statique = t;
    }

    /// Une image de l'animation : le fond fixe, puis les bords de plasma qui tournent.
    fn image_suivante(&mut self) {
        if self.h == 0 {
            return;
        }
        let s = self.echelle;
        let angle = (self.debut.elapsed().as_secs_f32() % 9.0) / 9.0 * 360.0;
        let mut t = self.statique.clone();
        let d = &self.dispo;
        t.contour_arrondi(
            0.0,
            0.0,
            d.l as f32,
            d.h as f32,
            self.theme.rayon(24.0, 3.0) * s,
            1.0 * s,
            &self.theme.plasma(angle, 0.5),
        );
        let rp = self.theme.rayon(16.0, 2.0) * s;
        for (i, (p, b)) in self.pastilles.iter().zip(&d.pastilles).enumerate() {
            if p.devant {
                t.contour_arrondi(b.0, b.1, b.2, b.3, rp, 1.5 * s, &self.theme.plasma(angle, 1.0));
            } else if self.survol == Some(i) {
                // (survol : le bord de plasma s'allume à moitié)
                t.contour_arrondi(b.0, b.1, b.2, b.3, rp, 1.0 * s, &self.theme.plasma(angle, 0.6));
            }
        }
        self.poser(&t);
        self.image = t;
    }

    fn poser(&mut self, t: &Toile) {
        unsafe {
            if self.dib.as_ref().is_none_or(|d| d.l != t.l || d.h != t.h) {
                if let Some(d) = self.dib.take() {
                    let _ = DeleteObject(HGDIOBJ(d.bmp.0));
                    let _ = DeleteDC(d.dc);
                }
                let dc = CreateCompatibleDC(None);
                let bmi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: t.l,
                        biHeight: -t.h,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
                let Ok(bmp) = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
                    let _ = DeleteDC(dc);
                    return;
                };
                SelectObject(dc, HGDIOBJ(bmp.0));
                self.dib = Some(Dib { dc, bmp, bits: bits as *mut u32, l: t.l, h: t.h });
            }
            let Some(d) = &self.dib else { return };
            std::ptr::copy_nonoverlapping(t.px.as_ptr(), d.bits, t.px.len());
            let mut r = RECT::default();
            let _ = GetWindowRect(hwnd(self.h), &mut r);
            let pos = POINT { x: r.left, y: r.top };
            let taille = SIZE { cx: t.l, cy: t.h };
            let src = POINT::default();
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                hwnd(self.h),
                None,
                Some(&pos),
                Some(&taille),
                Some(d.dc),
                Some(&src),
                Default::default(),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }

    fn infobulles(&mut self) {
        if self.infobulle == 0 {
            return;
        }
        unsafe {
            let tip = hwnd(self.infobulle);
            for i in 0..=self.pastilles.len().max(16) {
                let mut ti = TTTOOLINFOW {
                    cbSize: size_of::<TTTOOLINFOW>() as u32,
                    hwnd: hwnd(self.h),
                    uId: i,
                    ..Default::default()
                };
                SendMessageW(tip, TTM_DELTOOLW, None, Some(LPARAM(&mut ti as *mut _ as isize)));
            }
            let ajouter = |id: usize, b: (f32, f32, f32, f32), texte: &str| {
                let mut t: Vec<u16> = texte.encode_utf16().chain(std::iter::once(0)).collect();
                let mut ti = TTTOOLINFOW {
                    cbSize: size_of::<TTTOOLINFOW>() as u32,
                    uFlags: TTF_SUBCLASS,
                    hwnd: hwnd(self.h),
                    uId: id,
                    rect: RECT {
                        left: b.0 as i32,
                        top: b.1 as i32,
                        right: (b.0 + b.2) as i32,
                        bottom: (b.1 + b.3) as i32,
                    },
                    lpszText: windows::core::PWSTR(t.as_mut_ptr()),
                    ..Default::default()
                };
                SendMessageW(tip, TTM_ADDTOOLW, None, Some(LPARAM(&mut ti as *mut _ as isize)));
            };
            ajouter(0, self.dispo.grip, GLISSER_BARRE);
            for (i, (p, b)) in self.pastilles.iter().zip(self.dispo.pastilles.clone()).enumerate() {
                ajouter(i + 1, b, &p.infobulle);
            }
        }
    }

    /// Montrée sans activer (le jeu garde le clavier) ; l'animation tourne.
    pub fn montrer(&mut self) {
        if self.h == 0 {
            return;
        }
        unsafe {
            let _ = ShowWindow(hwnd(self.h), SW_SHOWNOACTIVATE);
            SetTimer(Some(hwnd(self.h)), ANIMATION, 50, None);
        }
        self.visible = true;
    }

    /// Cachée : l'animation s'arrête.
    pub fn cacher(&mut self) {
        if self.h == 0 {
            return;
        }
        unsafe {
            let _ = KillTimer(Some(hwnd(self.h)), ANIMATION);
            let _ = ShowWindow(hwnd(self.h), SW_HIDE);
        }
        self.visible = false;
        self.survol = None;
    }

    /// Remise au-dessus de tout (un jeu « toujours devant » la recouvrirait sinon).
    pub fn devant(&mut self) {
        if self.h == 0 {
            return;
        }
        unsafe {
            let _ = SetWindowPos(
                hwnd(self.h),
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            );
        }
    }

    /// Essai : le temps de `n` images de l'animation (le fond fixe recopié, les bords de plasma qui tournent).
    pub fn cout_images(&mut self, n: u32) -> std::time::Duration {
        let t = Instant::now();
        for _ in 0..n {
            self.image_suivante();
        }
        t.elapsed()
    }

    pub fn hwnd(&self) -> isize {
        self.h
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn survolee(&self) -> bool {
        if self.h == 0 || !self.visible {
            return false;
        }
        let p = super::souris_ecran();
        let mut r = RECT::default();
        unsafe {
            let _ = GetWindowRect(hwnd(self.h), &mut r);
        }
        p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
    }

    /// Reset-OrgBarPosition (la fenêtre) : en haut au centre de l'écran principal.
    pub fn recentrer(&mut self) {
        self.position = (None, None);
        if self.h == 0 {
            return;
        }
        let wa = zone_travail();
        let x = wa.left + (wa.right - wa.left - self.dispo.l) / 2;
        let y = wa.top + (6.0 * echelle_systeme()).round() as i32;
        unsafe {
            let _ = SetWindowPos(hwnd(self.h), None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }

    pub fn detruire(&mut self) {
        unsafe {
            if self.h != 0 {
                let _ = KillTimer(Some(hwnd(self.h)), ANIMATION);
                let _ = DestroyWindow(hwnd(self.h));
            }
            if let Some(d) = self.dib.take() {
                let _ = DeleteObject(HGDIOBJ(d.bmp.0));
                let _ = DeleteDC(d.dc);
            }
        }
        self.h = 0;
        self.infobulle = 0;
    }

    fn a(&self, x: i32, y: i32) -> Option<usize> {
        let (x, y) = (x as f32, y as f32);
        self.dispo.pastilles.iter().position(|b| x >= b.0 && x < b.0 + b.2 && y >= b.1 && y < b.1 + b.3)
    }

    fn sur_poignee(&self, x: i32, y: i32) -> bool {
        let g = self.dispo.grip;
        let (x, y) = (x as f32, y as f32);
        x >= g.0 && x < g.0 + g.2 && y >= 0.0 && y < self.dispo.h as f32 && y >= g.1 - 6.0 * self.echelle
    }

    /// Essai : l'image de la barre en BMP.
    pub fn capturer(&self, f: &std::path::Path) -> bool {
        bmp(&self.image, f)
    }
}

/// Une toile en fichier BMP (32 bits, avec la transparence).
pub fn bmp(t: &Toile, f: &std::path::Path) -> bool {
    let mut o = Vec::with_capacity(54 + t.px.len() * 4);
    let taille = (54 + t.px.len() * 4) as u32;
    o.extend_from_slice(b"BM");
    o.extend_from_slice(&taille.to_le_bytes());
    o.extend_from_slice(&[0, 0, 0, 0]);
    o.extend_from_slice(&54u32.to_le_bytes());
    o.extend_from_slice(&40u32.to_le_bytes());
    o.extend_from_slice(&t.l.to_le_bytes());
    o.extend_from_slice(&(-t.h).to_le_bytes());
    o.extend_from_slice(&1u16.to_le_bytes());
    o.extend_from_slice(&32u16.to_le_bytes());
    o.extend_from_slice(&[0; 24]);
    for p in &t.px {
        o.extend_from_slice(&p.to_le_bytes());
    }
    std::fs::write(f, o).is_ok()
}

fn xy(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32)
}

/// Les gestes sur la barre : glisser la poignée, cliquer une pastille, survoler.
fn evenement(e: &mut Etat, m: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    let b = &mut e.bureau.barre;
    match m {
        WM_TIMER if wp.0 == ANIMATION => {
            if b.visible {
                b.image_suivante();
            }
            Some(LRESULT(0))
        }
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            if let Some((depart, fen)) = b.glisse {
                let p = super::souris_ecran();
                unsafe {
                    let _ = SetWindowPos(
                        hwnd(b.h),
                        None,
                        fen.x + p.x - depart.x,
                        fen.y + p.y - depart.y,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                return Some(LRESULT(0));
            }
            if !b.suivi {
                let mut t = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd(b.h),
                    dwHoverTime: 0,
                };
                unsafe {
                    let _ = TrackMouseEvent(&mut t);
                }
                b.suivi = true;
            }
            let s = b.a(x, y);
            if s != b.survol {
                b.survol = s;
                b.image_suivante();
            }
            Some(LRESULT(0))
        }
        WM_MOUSELEAVE => {
            b.suivi = false;
            if b.survol.take().is_some() {
                b.image_suivante();
            }
            Some(LRESULT(0))
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            if b.sur_poignee(x, y) {
                let mut r = RECT::default();
                unsafe {
                    let _ = GetWindowRect(hwnd(b.h), &mut r);
                    SetCapture(hwnd(b.h));
                }
                b.glisse = Some((super::souris_ecran(), POINT { x: r.left, y: r.top }));
            } else {
                b.appui = b.a(x, y);
            }
            Some(LRESULT(0))
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            if b.glisse.take().is_some() {
                unsafe {
                    let _ = ReleaseCapture();
                }
                let mut r = RECT::default();
                unsafe {
                    let _ = GetWindowRect(hwnd(b.h), &mut r);
                }
                let s = echelle_systeme();
                let (px, py) = (f64::from(r.left) / s, f64::from(r.top) / s);
                b.position = (Some(px), Some(py));
                let Etat { veille, bureau, .. } = e;
                veille.placer_barre(bureau, px, py);
                return Some(LRESULT(0));
            }
            let appui = b.appui.take();
            if let Some(i) = b.a(x, y).filter(|i| Some(*i) == appui) {
                let nom = b.pastilles[i].nom.clone();
                let Etat { veille, bureau, .. } = e;
                veille.clic_pastille(bureau, &nom);
            }
            Some(LRESULT(0))
        }
        WM_SETCURSOR => {
            let p = super::souris_ecran();
            let mut r = RECT::default();
            unsafe {
                let _ = GetWindowRect(hwnd(b.h), &mut r);
            }
            let (x, y) = (p.x - r.left, p.y - r.top);
            let c = if b.sur_poignee(x, y) {
                IDC_SIZEALL
            } else if b.a(x, y).is_some() {
                IDC_HAND
            } else {
                IDC_ARROW
            };
            unsafe {
                SetCursor(LoadCursorW(None, c).ok());
            }
            Some(LRESULT(1))
        }
        WM_DPICHANGED => {
            b.echelle = (wp.0 & 0xFFFF) as f32 / 96.0;
            let p = b.pastilles.clone();
            b.dessiner(&p);
            Some(LRESULT(0))
        }
        _ => None,
    }
}

unsafe extern "system" fn proc_barre(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // (la barre ne prend jamais le focus au jeu)
    if m == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    if matches!(
        m,
        WM_TIMER | WM_MOUSEMOVE | WM_MOUSELEAVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_SETCURSOR | WM_DPICHANGED
    ) && let Some(Some(r)) = super::avec(|e| evenement(e, m, wp, lp))
    {
        return r;
    }
    unsafe { DefWindowProcW(h, m, wp, lp) }
}
