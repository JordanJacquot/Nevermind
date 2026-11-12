//! La vue de l'Organizer (Build-OrgPanel) dans sa propre fenêtre : « Mes personnages » (ordre d'initiative, glisser une
//! ligne pour la déplacer, Afficher, Retirer), « Raccourcis » (cliquer puis appuyer sur la touche ou le bouton de
//! souris voulu) et « Barre flottante ». Les textes viennent de `organizer::vue`, les mêmes que la version PowerShell.

use super::barre::bmp;
use super::dessin::{self, Couleur, Mise, Peinture, Police, Toile};
use super::{Etat, Theme, hwnd};
use crate::organizer::vue::{self as v, BoutonLigne, Modele};
use crate::organizer::{Capture, souris_capturee, touche_capturee};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, DIB_RGB_COLORS, EndPaint, InvalidateRect, PAINTSTRUCT, SRCCOPY,
    StretchDIBits,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    TOOLTIPS_CLASSW, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_DELTOOLW, TTM_SETMAXTIPWIDTH, TTS_ALWAYSTIP, TTS_NOPREFIX,
    TTTOOLINFOW,
};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, HICON, IDC_ARROW, IDC_HAND,
    IDC_SIZEALL, IDI_APPLICATION, IsIconic, LoadCursorW, LoadIconW, MINMAXINFO, RegisterClassExW, SW_RESTORE, SW_SHOW,
    SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW, SetCursor, SetForegroundWindow, SetWindowPos, ShowWindow,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CHAR, WM_CLOSE, WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETCURSOR, WM_SIZE,
    WM_SYSCHAR, WM_SYSKEYDOWN, WM_XBUTTONDOWN, WNDCLASSEXW, WS_OVERLAPPEDWINDOW, WS_POPUP,
};
use windows::core::{PCWSTR, PWSTR, w};

/// Une boîte (x, y, largeur, hauteur) en pixels.
type Boite = (f32, f32, f32, f32);

/// Ce qu'il y a sous la souris.
#[derive(Debug, Clone, PartialEq)]
enum Zone {
    Activer,
    Barre,
    Toujours,
    Remettre,
    Bouton(BoutonLigne),
    Touche(String),
    Poignee(String),
    Ligne(String),
}

pub struct FenetreVue {
    h: isize,
    infobulle: isize,
    theme: Theme,
    modele: Modele,
    echelle: f32,
    defile: f32,
    hauteur: f32,
    zones: Vec<((f32, f32, f32, f32), Zone)>,
    survol: Option<Zone>,
    appui: Option<Zone>,
    glisse: Option<String>,
    cible: Option<String>,
    toile: Toile,
}

fn xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 & 0xFFFF) as i16 as f32, ((lp.0 >> 16) & 0xFFFF) as i16 as f32)
}

fn dans(b: (f32, f32, f32, f32), x: f32, y: f32) -> bool {
    x >= b.0 && x < b.0 + b.2 && y >= b.1 && y < b.1 + b.3
}

/// Les modificateurs tenus (Keyboard.Modifiers de WPF) : 1 Alt, 2 Ctrl, 4 Maj, 8 Win.
fn modificateurs() -> u32 {
    let t = |vk: i32| (unsafe { GetKeyState(vk) } as u16 & 0x8000) != 0;
    u32::from(t(0x12)) | (u32::from(t(0x11)) << 1) | (u32::from(t(0x10)) << 2) | (u32::from(t(0x5B) || t(0x5C)) << 3)
}

impl FenetreVue {
    pub fn creer(theme: Theme) -> Option<FenetreVue> {
        unsafe {
            let inst = GetModuleHandleW(PCWSTR::null()).ok()?;
            // (l'icône de Nevermind si l'appli en a une, sinon celle de Windows)
            #[allow(clippy::manual_dangling_ptr)]
            let icone: HICON = LoadIconW(Some(inst.into()), PCWSTR(32512 as *const u16))
                .or_else(|_| LoadIconW(Some(inst.into()), PCWSTR(1 as *const u16)))
                .or_else(|_| LoadIconW(None, IDI_APPLICATION))
                .unwrap_or_default();
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(proc_vue),
                hInstance: inst.into(),
                hIcon: icone,
                hIconSm: icone,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: w!("NevermindOrganizerVue"),
                ..Default::default()
            };
            RegisterClassExW(&wc);
            let s = GetDpiForSystem().max(96) as f32 / 96.0;
            let h = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("NevermindOrganizerVue"),
                w!("Organizer Dofus"),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                (1060.0 * s) as i32,
                (800.0 * s) as i32,
                None,
                None,
                Some(inst.into()),
                None,
            )
            .ok()?;
            let sombre: u32 = 1;
            let _ = DwmSetWindowAttribute(h, DWMWA_USE_IMMERSIVE_DARK_MODE, &sombre as *const _ as *const _, 4);
            let t = CreateWindowExW(
                WINDOW_EX_STYLE(0x8),
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
            )
            .map(|t| t.0 as isize)
            .unwrap_or(0);
            if t != 0 {
                SendMessageW(hwnd(t), TTM_SETMAXTIPWIDTH, None, Some(LPARAM(360)));
            }
            let mut f = FenetreVue {
                h: h.0 as isize,
                infobulle: t,
                theme,
                modele: Modele::default(),
                echelle: GetDpiForWindow(h).max(96) as f32 / 96.0,
                defile: 0.0,
                hauteur: 0.0,
                zones: Vec::new(),
                survol: None,
                appui: None,
                glisse: None,
                cible: None,
                toile: Toile::nouvelle(1, 1),
            };
            let _ = ShowWindow(h, SW_SHOW);
            let _ = SetForegroundWindow(h);
            f.rendre();
            Some(f)
        }
    }

    pub fn hwnd(&self) -> isize {
        self.h
    }

    pub fn devant(&self) {
        unsafe {
            if IsIconic(hwnd(self.h)).as_bool() {
                let _ = ShowWindow(hwnd(self.h), SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd(self.h));
        }
    }

    pub fn detruire(self) {
        unsafe {
            let _ = DestroyWindow(hwnd(self.h));
        }
    }

    /// Le contenu change (Build-OrgPanel).
    pub fn modele(&mut self, m: Modele) {
        self.modele = m;
        self.rendre();
    }

    fn police(&self, taille: f32, graisse: i32) -> Police {
        Police { familles: self.theme.police, taille: (taille * self.echelle).round() as i32, graisse }
    }

    /// Tout est redessiné dans la toile, puis montré.
    fn rendre(&mut self) {
        let mut r = RECT::default();
        unsafe {
            let _ = GetClientRect(hwnd(self.h), &mut r);
        }
        let (l, h) = ((r.right - r.left).max(1), (r.bottom - r.top).max(1));
        let mut t = Toile::nouvelle(l, h);
        t.rect_arrondi(
            -50.0,
            -50.0,
            l as f32 + 100.0,
            h as f32 + 100.0,
            0.0,
            &Peinture::Vertical(Couleur::hex("#0C0920"), Couleur::hex("#06050F")),
        );
        self.zones.clear();
        let mut infobulles: Vec<(Boite, &'static str)> = Vec::new();
        let s = self.echelle;
        let m = self.modele.clone();
        let p = self.theme.p;
        let dy = -self.defile;
        let blanc = Couleur::hex("#FFFFFF");
        let gris = Couleur::hex("#8E88A8");
        // En-tête
        let x0 = 24.0 * s;
        let mut y = 20.0 * s + dy;
        let ft = self.police(26.0, 700);
        let (_, th) = dessin::mesurer(v::TITRE, &ft, None);
        dessin::ecrire(&mut t, v::TITRE, &ft, blanc, x0 as i32, y as i32, l, th, Mise::Ligne);
        y += th as f32 + 4.0 * s;
        let fs = self.police(13.0, 400);
        let (_, sh) = dessin::mesurer(v::SOUS_TITRE, &fs, None);
        dessin::ecrire(&mut t, v::SOUS_TITRE, &fs, Couleur::hex("#A6A1BC"), x0 as i32, y as i32, l, sh, Mise::Ligne);
        y += sh as f32 + 18.0 * s;
        let haut = y;
        let droite_l = 330.0 * s;
        let gauche_l = (l as f32 - 2.0 * x0 - 16.0 * s - droite_l).max(360.0 * s);
        let xd = x0 + gauche_l + 16.0 * s;

        // ------------------------------------------------ Mes personnages
        let (cx, cl) = (x0 + 20.0 * s, gauche_l - 40.0 * s);
        let mut cy = haut + 18.0 * s;
        let debut_carte = haut;
        let mut corps = Vec::new();
        cy = self.entete_carte(&mut corps, cx, cy, cl, v::PERSOS_TITRE, v::PERSOS_NOTE, p);
        cy = self.interrupteur(&mut corps, cx, cy, cl, v::ACTIVER, v::ACTIVER_NOTE, m.on, Zone::Activer);
        if m.vide {
            let ft = self.police(12.5, 400);
            let (_, hh) = dessin::mesurer(v::AUCUNE_FENETRE, &ft, Some((cl - 32.0 * s) as i32));
            let bh = hh as f32 + 36.0 * s;
            corps.push(Dessin::Rect(cx, cy, cl, bh, 14.0 * s, Peinture::Unie(Couleur::hex("#0AFFFFFF"))));
            corps.push(Dessin::Texte(
                v::AUCUNE_FENETRE.into(),
                ft,
                Couleur::hex("#A6A1BC"),
                cx + 16.0 * s,
                cy + 18.0 * s,
                cl - 32.0 * s,
                hh as f32,
                Mise::Lignes,
            ));
            cy += bh;
        }
        for li in &m.lignes {
            let rh = 46.0 * s;
            let fond = Couleur::hex(if li.en_ligne { "#12FFFFFF" } else { "#06FFFFFF" });
            corps.push(Dessin::Rect(cx, cy, cl, rh, 12.0 * s, Peinture::Unie(fond)));
            let bord = if !li.nom.is_empty() && self.glisse.is_some() && self.cible.as_deref() == Some(li.nom.as_str())
            {
                p
            } else {
                Couleur::hex("#14FFFFFF")
            };
            corps.push(Dessin::Bord(cx, cy, cl, rh, 12.0 * s, 1.0 * s, Peinture::Unie(bord)));
            let mut x = cx + 10.0 * s;
            let icones = Police {
                familles: "Segoe Fluent Icons, Segoe MDL2 Assets",
                taille: (13.0 * s).round() as i32,
                graisse: 400,
            };
            if !li.nom.is_empty() {
                corps.push(Dessin::Texte(
                    "\u{E76F}".into(),
                    icones,
                    Couleur::hex("#655E7E"),
                    x,
                    cy,
                    14.0 * s,
                    rh,
                    Mise::Centre,
                ));
                self.zones.push(((x - 4.0 * s, cy, 22.0 * s, rh), Zone::Poignee(li.nom.clone())));
                self.zones.push(((cx, cy, cl, rh), Zone::Ligne(li.nom.clone())));
            }
            x += 14.0 * s + 10.0 * s;
            corps.push(Dessin::Texte(li.numero.clone(), self.police(13.0, 700), p, x, cy, 18.0 * s, rh, Mise::Ligne));
            x += 18.0 * s;
            corps.push(Dessin::Pastille(
                li.pastille.initiale.clone(),
                li.pastille.couleur,
                x + 15.0 * s,
                cy + rh / 2.0,
                15.0 * s,
                if li.en_ligne { 1.0 } else { 0.4 },
            ));
            x += 30.0 * s + 10.0 * s;
            let fb = self.police(12.5, 600);
            let (bl, _) = match &li.bouton {
                BoutonLigne::Afficher(_) => dessin::mesurer("Afficher", &fb, None),
                BoutonLigne::Retirer(_) => dessin::mesurer("Retirer", &fb, None),
                BoutonLigne::Aucun => (0, 0),
            };
            let bw = if bl > 0 { bl as f32 + 28.0 * s } else { 0.0 };
            let info_l = cl - (x - cx) - bw - 20.0 * s;
            let fnom = self.police(13.5, 600);
            corps.push(Dessin::Texte(
                li.titre.clone(),
                fnom,
                if li.en_ligne { blanc } else { gris },
                x,
                cy + 6.0 * s,
                info_l,
                18.0 * s,
                Mise::Ligne,
            ));
            let fe = self.police(11.5, 400);
            corps.push(Dessin::Texte(
                li.etat.clone(),
                fe,
                if li.etat_ok { Couleur::hex("#22D37A") } else { gris },
                x,
                cy + 24.0 * s,
                info_l,
                16.0 * s,
                Mise::Ligne,
            ));
            if bw > 0.0 {
                let bx = cx + cl - 10.0 * s - bw;
                let bb = (bx, cy + (rh - 30.0 * s) / 2.0, bw, 30.0 * s);
                let texte = if matches!(li.bouton, BoutonLigne::Afficher(_)) { "Afficher" } else { "Retirer" };
                let z = Zone::Bouton(li.bouton.clone());
                corps.push(Dessin::Bouton(texte.into(), bb, self.survol.as_ref() == Some(&z)));
                if matches!(li.bouton, BoutonLigne::Retirer(_)) {
                    infobulles.push((bb, v::RETIRER_AIDE));
                }
                self.zones.push((bb, z));
            }
            cy += rh + 6.0 * s;
        }
        let fin_gauche = cy + 18.0 * s - 6.0 * s;
        self.carte(&mut t, x0, debut_carte, gauche_l, fin_gauche - debut_carte);
        self.peindre(&mut t, corps);

        // ------------------------------------------------ Raccourcis
        let (cx, cl) = (xd + 20.0 * s, droite_l - 40.0 * s);
        let mut cy = haut + 18.0 * s;
        let mut corps = Vec::new();
        cy = self.entete_carte(&mut corps, cx, cy, cl, v::RACCOURCIS_TITRE, v::RACCOURCIS_NOTE, self.theme.s);
        for k in &m.touches {
            if let Some((titre, note)) = k.titre_avant {
                let f = self.police(12.0, 600);
                cy += 8.0 * s;
                corps.push(Dessin::Texte(titre.into(), f, gris, cx, cy, cl, 16.0 * s, Mise::Ligne));
                cy += 16.0 * s + 8.0 * s;
                if let Some(note) = note {
                    let fnote = self.police(11.5, 400);
                    let (_, nh) = dessin::mesurer(note, &fnote, Some(cl as i32));
                    corps.push(Dessin::Texte(note.into(), fnote, gris, cx, cy, cl, nh as f32, Mise::Lignes));
                    cy += nh as f32 + 8.0 * s;
                }
            }
            let fm = Police { familles: "Cascadia Code, Consolas", taille: (12.0 * s).round() as i32, graisse: 700 };
            let (tl, th) = dessin::mesurer(&k.bouton, &fm, None);
            // (une touche très longue : le bouton ne prend pas plus de 70 % de la ligne, son texte finit par « ... »)
            let bw = (tl as f32 + 20.0 * s).max(70.0 * s).min(cl * 0.7);
            let bh = th as f32 + 8.0 * s + 3.0 * s;
            let ll = cl - bw - 8.0 * s;
            let fr = self.police(11.5, 400);
            let hr = if k.refusee { dessin::mesurer(v::DEJA_PRISE, &fr, Some(ll as i32)).1 as f32 } else { 0.0 };
            let lh = if k.refusee { 21.0 * s + hr } else { 20.0 * s };
            let rh = bh.max(lh);
            let fl = self.police(13.0, 600);
            corps.push(Dessin::Texte(
                k.libelle.clone(),
                fl,
                Couleur::hex("#EEEBF7"),
                cx,
                cy + (rh - lh) / 2.0,
                ll,
                20.0 * s,
                Mise::Ligne,
            ));
            if k.refusee {
                corps.push(Dessin::Texte(
                    v::DEJA_PRISE.into(),
                    fr,
                    Couleur::hex("#F04438"),
                    cx,
                    cy + (rh - lh) / 2.0 + 21.0 * s,
                    ll,
                    hr,
                    Mise::Lignes,
                ));
            }
            let bb = (cx + cl - bw, cy + (rh - bh) / 2.0, bw, bh);
            corps.push(Dessin::Touche(k.bouton.clone(), bb, k.en_choix, k.vide));
            infobulles.push((bb, k.aide));
            self.zones.push((bb, Zone::Touche(k.action.clone())));
            cy += rh + 8.0 * s;
        }
        let fin_r = cy + 18.0 * s - 8.0 * s;
        self.carte(&mut t, xd, haut, droite_l, fin_r - haut);
        self.peindre(&mut t, corps);

        // ------------------------------------------------ Barre flottante
        let haut_b = fin_r + 16.0 * s;
        let mut cy = haut_b + 18.0 * s;
        let mut corps = Vec::new();
        cy = self.entete_carte(&mut corps, cx, cy, cl, v::BARRE_TITRE, v::BARRE_NOTE, self.theme.t);
        cy = self.interrupteur(&mut corps, cx, cy, cl, v::BARRE, v::BARRE_SOUS, m.barre, Zone::Barre);
        cy = self.interrupteur(&mut corps, cx, cy, cl, v::TOUJOURS, v::TOUJOURS_SOUS, m.toujours, Zone::Toujours);
        let f = self.police(12.0, 400);
        let (_, hh) = dessin::mesurer(v::PLEIN_ECRAN, &f, Some(cl as i32));
        corps.push(Dessin::Texte(v::PLEIN_ECRAN.into(), f, gris, cx, cy, cl, hh as f32, Mise::Lignes));
        cy += hh as f32 + 8.0 * s + 4.0 * s;
        let fb = self.police(12.5, 600);
        let (bl, _) = dessin::mesurer(v::REMETTRE, &fb, None);
        let bb = (cx, cy, (bl as f32 + 28.0 * s).min(cl), 32.0 * s);
        corps.push(Dessin::Bouton(v::REMETTRE.into(), bb, self.survol == Some(Zone::Remettre)));
        self.zones.push((bb, Zone::Remettre));
        cy += 32.0 * s;
        let fin_b = cy + 18.0 * s;
        self.carte(&mut t, xd, haut_b, droite_l, fin_b - haut_b);
        self.peindre(&mut t, corps);

        // ------------------------------------------------ La note du bas
        let bas = fin_gauche.max(fin_b) + 16.0 * s;
        let f = self.police(12.0, 400);
        let (_, nh) = dessin::mesurer(v::NOTE_BAS, &f, Some((l as f32 - 2.0 * x0 - 4.0 * s) as i32));
        dessin::ecrire(
            &mut t,
            v::NOTE_BAS,
            &f,
            Couleur::hex("#655E7E"),
            (x0 + 4.0 * s) as i32,
            bas as i32,
            (l as f32 - 2.0 * x0 - 4.0 * s) as i32,
            nh,
            Mise::Lignes,
        );
        self.hauteur = bas + nh as f32 + 8.0 * s + 16.0 * s - dy;
        self.toile = t;
        self.infobulles(&infobulles);
        unsafe {
            let _ = InvalidateRect(Some(hwnd(self.h)), None, false);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn entete_carte(
        &self,
        corps: &mut Vec<Dessin>,
        x: f32,
        y: f32,
        l: f32,
        titre: &str,
        note: &str,
        accent: Couleur,
    ) -> f32 {
        let s = self.echelle;
        let ft = self.police(15.0, 700);
        let (_, th) = dessin::mesurer(titre, &ft, None);
        corps.push(Dessin::Point(x + 4.5 * s, y + th as f32 / 2.0 + 1.0 * s, accent));
        corps.push(Dessin::Texte(
            titre.into(),
            ft,
            Couleur::hex("#FFFFFF"),
            x + 19.0 * s,
            y,
            l - 19.0 * s,
            th as f32,
            Mise::Ligne,
        ));
        let mut y = y + th as f32;
        let fn_ = self.police(12.0, 400);
        let (_, nh) = dessin::mesurer(note, &fn_, Some((l - 19.0 * s) as i32));
        y += 2.0 * s;
        corps.push(Dessin::Texte(
            note.into(),
            fn_,
            Couleur::hex("#8E88A8"),
            x + 19.0 * s,
            y,
            l - 19.0 * s,
            nh as f32,
            Mise::Lignes,
        ));
        y + nh as f32 + 14.0 * s
    }

    #[allow(clippy::too_many_arguments)]
    fn interrupteur(
        &mut self,
        corps: &mut Vec<Dessin>,
        x: f32,
        y: f32,
        l: f32,
        titre: &str,
        texte: &str,
        on: bool,
        z: Zone,
    ) -> f32 {
        let s = self.echelle;
        let (sl, sh) = (40.0 * s, 22.0 * s);
        let tl = l - sl - 16.0 * s;
        let f1 = self.police(14.0, 600);
        let (_, h1) = dessin::mesurer(titre, &f1, None);
        let f2 = self.police(12.0, 400);
        let (_, h2) = dessin::mesurer(texte, &f2, Some(tl as i32));
        corps.push(Dessin::Texte(titre.into(), f1, Couleur::hex("#FFFFFF"), x, y, tl, h1 as f32, Mise::Ligne));
        corps.push(Dessin::Texte(
            texte.into(),
            f2,
            Couleur::hex("#A6A1BC"),
            x,
            y + h1 as f32 + 2.0 * s,
            tl,
            h2 as f32,
            Mise::Lignes,
        ));
        let rh = (h1 + h2) as f32 + 2.0 * s;
        let b = (x + l - sl, y + (rh - sh) / 2.0, sl, sh);
        corps.push(Dessin::Interrupteur(b, on));
        self.zones.push(((x, y, l, rh), z));
        y + rh + 10.0 * s
    }

    fn carte(&self, t: &mut Toile, x: f32, y: f32, l: f32, h: f32) {
        let s = self.echelle;
        t.rect_arrondi(x, y, l, h, 20.0 * s, &Peinture::Vertical(Couleur::hex("#D8171336"), Couleur::hex("#D0100D26")));
        t.contour_arrondi(
            x,
            y,
            l,
            h,
            20.0 * s,
            1.0 * s,
            &Peinture::Vertical(Couleur::hex("#FF342C62"), Couleur::hex("#FF231E46")),
        );
    }

    fn peindre(&self, t: &mut Toile, corps: Vec<Dessin>) {
        let s = self.echelle;
        let p = self.theme.p;
        for d in corps {
            match d {
                Dessin::Rect(x, y, l, h, r, pe) => t.rect_arrondi(x, y, l, h, r, &pe),
                Dessin::Bord(x, y, l, h, r, e, pe) => t.contour_arrondi(x, y, l, h, r, e, &pe),
                Dessin::Texte(tx, f, c, x, y, l, h, mise) => {
                    dessin::ecrire(t, &tx, &f, c, x as i32, y as i32, l.ceil() as i32, h.ceil() as i32, mise)
                }
                Dessin::Point(x, y, c) => t.disque(x, y, 4.5 * s, &Peinture::Unie(c)),
                Dessin::Pastille(ini, col, x, y, r, op) => {
                    t.disque(
                        x,
                        y,
                        r,
                        &Peinture::Vertical(Couleur::hex(col).opacite(op), Couleur::hex("#33000000").opacite(op)),
                    );
                    t.cercle(x, y, r, 1.0 * s, &Peinture::Unie(Couleur::hex("#55FFFFFF").opacite(op)));
                    let f =
                        Police { familles: self.theme.police, taille: (2.0 * r * 0.45).round() as i32, graisse: 700 };
                    dessin::ecrire(
                        t,
                        &ini,
                        &f,
                        Couleur::hex("#FFFFFF").opacite(op),
                        (x - r) as i32,
                        (y - r) as i32,
                        (2.0 * r).ceil() as i32,
                        (2.0 * r).ceil() as i32,
                        Mise::Centre,
                    );
                }
                Dessin::Bouton(tx, b, survol) => {
                    let r = self.theme.rayon(10.0, 2.0) * s;
                    t.rect_arrondi(b.0, b.1, b.2, b.3, r, &Peinture::Unie(Couleur::hex("#16FFFFFF")));
                    let bord = if survol { p.opacite(0.5) } else { Couleur::hex("#26FFFFFF") };
                    t.contour_arrondi(b.0, b.1, b.2, b.3, r, 1.0 * s, &Peinture::Unie(bord));
                    dessin::ecrire(
                        t,
                        &tx,
                        &self.police(12.5, 600),
                        Couleur::hex("#F2EFFA"),
                        b.0 as i32,
                        b.1 as i32,
                        b.2 as i32,
                        b.3 as i32,
                        Mise::Centre,
                    );
                }
                Dessin::Touche(tx, b, choix, vide) => {
                    let r = self.theme.rayon(8.0, 2.0) * s;
                    let fond = if choix { p.opacite(0.2) } else { Couleur::hex("#1AFFFFFF") };
                    let bord = if choix { p } else { Couleur::hex("#33FFFFFF") };
                    t.rect_arrondi(b.0, b.1, b.2, b.3, r, &Peinture::Unie(bord));
                    t.rect_arrondi(
                        b.0 + s,
                        b.1 + s,
                        b.2 - 2.0 * s,
                        b.3 - 4.0 * s,
                        (r - s).max(0.0),
                        &Peinture::Unie(Couleur::hex("#160F2A")),
                    );
                    t.rect_arrondi(
                        b.0 + s,
                        b.1 + s,
                        b.2 - 2.0 * s,
                        b.3 - 4.0 * s,
                        (r - s).max(0.0),
                        &Peinture::Unie(fond),
                    );
                    let f =
                        Police { familles: "Cascadia Code, Consolas", taille: (12.0 * s).round() as i32, graisse: 700 };
                    let c = if vide { Couleur::hex("#8E88A8") } else { Couleur::hex("#FFFFFF") };
                    dessin::ecrire(
                        t,
                        &tx,
                        &f,
                        c,
                        (b.0 + 8.0 * s) as i32,
                        b.1 as i32,
                        (b.2 - 16.0 * s) as i32,
                        (b.3 - 2.0 * s) as i32,
                        Mise::Centre,
                    );
                }
                Dessin::Interrupteur(b, on) => {
                    let fond = if on { Peinture::Unie(p) } else { Peinture::Unie(Couleur::hex("#33FFFFFF")) };
                    t.rect_arrondi(b.0, b.1, b.2, b.3, b.3 / 2.0, &fond);
                    let kr = b.3 / 2.0 - 3.0 * s;
                    let kx = if on { b.0 + b.2 - b.3 / 2.0 } else { b.0 + b.3 / 2.0 };
                    t.disque(
                        kx,
                        b.1 + b.3 / 2.0,
                        kr,
                        &Peinture::Unie(Couleur::hex(if on { "#08060F" } else { "#FFFFFF" })),
                    );
                }
            }
        }
    }

    fn infobulles(&self, l: &[(Boite, &'static str)]) {
        if self.infobulle == 0 {
            return;
        }
        unsafe {
            let tip = hwnd(self.infobulle);
            for i in 0..40usize {
                let mut ti = TTTOOLINFOW {
                    cbSize: size_of::<TTTOOLINFOW>() as u32,
                    hwnd: hwnd(self.h),
                    uId: i,
                    ..Default::default()
                };
                SendMessageW(tip, TTM_DELTOOLW, None, Some(LPARAM(&mut ti as *mut _ as isize)));
            }
            for (i, (b, texte)) in l.iter().enumerate().take(40) {
                let mut tx: Vec<u16> = texte.encode_utf16().chain(std::iter::once(0)).collect();
                let mut ti = TTTOOLINFOW {
                    cbSize: size_of::<TTTOOLINFOW>() as u32,
                    uFlags: TTF_SUBCLASS,
                    hwnd: hwnd(self.h),
                    uId: i,
                    rect: RECT {
                        left: b.0 as i32,
                        top: b.1 as i32,
                        right: (b.0 + b.2) as i32,
                        bottom: (b.1 + b.3) as i32,
                    },
                    lpszText: PWSTR(tx.as_mut_ptr()),
                    ..Default::default()
                };
                SendMessageW(tip, TTM_ADDTOOLW, None, Some(LPARAM(&mut ti as *mut _ as isize)));
            }
        }
    }

    fn zone(&self, x: f32, y: f32) -> Option<Zone> {
        // (les boutons d'abord, puis la poignée, puis la ligne entière)
        let mut l: Vec<&(Boite, Zone)> = self.zones.iter().filter(|(b, _)| dans(*b, x, y)).collect();
        l.sort_by_key(|(_, z)| match z {
            Zone::Ligne(_) => 2,
            Zone::Poignee(_) => 1,
            _ => 0,
        });
        l.first().map(|x| x.1.clone())
    }

    fn peindre_fenetre(&self) {
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd(self.h), &mut ps);
            let t = &self.toile;
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
            StretchDIBits(
                dc,
                0,
                0,
                t.l,
                t.h,
                0,
                0,
                t.l,
                t.h,
                Some(t.px.as_ptr() as *const _),
                &bmi,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
            let _ = EndPaint(hwnd(self.h), &ps);
        }
    }

    /// Essai : l'image de la vue en BMP.
    pub fn capturer(&mut self, f: &std::path::Path) -> bool {
        bmp(&self.toile, f)
    }
}

/// Ce qu'il y a à peindre dans une carte (après son fond).
enum Dessin {
    Rect(f32, f32, f32, f32, f32, Peinture),
    Bord(f32, f32, f32, f32, f32, f32, Peinture),
    Texte(String, Police, Couleur, f32, f32, f32, f32, Mise),
    Point(f32, f32, Couleur),
    Pastille(String, &'static str, f32, f32, f32, f32),
    Bouton(String, (f32, f32, f32, f32), bool),
    Touche(String, (f32, f32, f32, f32), bool, bool),
    Interrupteur((f32, f32, f32, f32), bool),
}

/// Un geste dans la vue.
fn evenement(e: &mut Etat, m: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    let fv = e.bureau.vue.as_mut()?;
    match m {
        WM_PAINT => {
            fv.peindre_fenetre();
            Some(LRESULT(0))
        }
        WM_SIZE => {
            fv.rendre();
            Some(LRESULT(0))
        }
        WM_DPICHANGED => {
            fv.echelle = (wp.0 & 0xFFFF) as f32 / 96.0;
            let r = unsafe { &*(lp.0 as *const RECT) };
            unsafe {
                let _ = SetWindowPos(
                    hwnd(fv.h),
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            fv.rendre();
            Some(LRESULT(0))
        }
        WM_MOUSEWHEEL => {
            let d = ((wp.0 >> 16) & 0xFFFF) as i16 as f32;
            let mut r = RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd(fv.h), &mut r);
            }
            let max = (fv.hauteur - (r.bottom - r.top) as f32).max(0.0);
            fv.defile = (fv.defile - d / 120.0 * 60.0 * fv.echelle).clamp(0.0, max);
            fv.rendre();
            Some(LRESULT(0))
        }
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            let z = fv.zone(x, y);
            if fv.glisse.is_some() {
                let c = match &z {
                    Some(Zone::Ligne(n) | Zone::Poignee(n)) => Some(n.clone()),
                    _ => None,
                };
                if c != fv.cible {
                    fv.cible = c;
                    fv.rendre();
                }
            } else if z != fv.survol {
                fv.survol = z;
                fv.rendre();
            }
            Some(LRESULT(0))
        }
        WM_SETCURSOR if (lp.0 & 0xFFFF) as u32 == 1 => {
            let p = super::souris_ecran();
            let mut pt = p;
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd(fv.h), &mut pt);
            }
            let c = match fv.zone(pt.x as f32, pt.y as f32) {
                Some(Zone::Poignee(_)) => IDC_SIZEALL,
                Some(Zone::Ligne(_)) | None => IDC_ARROW,
                Some(_) => IDC_HAND,
            };
            unsafe {
                SetCursor(LoadCursorW(None, c).ok());
            }
            Some(LRESULT(1))
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            match fv.zone(x, y) {
                Some(Zone::Poignee(n)) => {
                    unsafe { SetCapture(hwnd(fv.h)) };
                    fv.glisse = Some(n);
                    fv.cible = None;
                    e.veille.glisse = true;
                }
                z => fv.appui = z,
            }
            Some(LRESULT(0))
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            if let Some(de) = fv.glisse.take() {
                unsafe {
                    let _ = ReleaseCapture();
                }
                let vers = fv.cible.take();
                e.veille.glisse = false;
                let Etat { veille, bureau, .. } = e;
                match vers {
                    Some(v) if v != de => veille.deplacer(bureau, &de, &v),
                    _ => veille.montrer_vue(bureau, true),
                }
                return Some(LRESULT(0));
            }
            let z = fv.zone(x, y);
            let appui = fv.appui.take();
            if z.is_none() || z != appui {
                return Some(LRESULT(0));
            }
            let h = fv.h;
            let Etat { veille, bureau, .. } = e;
            match z.unwrap() {
                Zone::Activer => {
                    let on = !veille.config.on;
                    veille.allumer(bureau, on);
                }
                Zone::Barre => {
                    let b = !veille.config.barre;
                    veille.regler_barre(bureau, false, b);
                }
                Zone::Toujours => {
                    let b = !veille.config.barre_toujours;
                    veille.regler_barre(bureau, true, b);
                }
                Zone::Remettre => {
                    veille.oublier_position(bureau);
                    bureau.barre.recentrer();
                    bureau.partage_statut(v::REMISE);
                }
                Zone::Bouton(BoutonLigne::Afficher(hw)) => veille.afficher_fenetre(bureau, hw),
                Zone::Bouton(BoutonLigne::Retirer(n)) => veille.retirer(bureau, &n),
                Zone::Touche(a) => {
                    veille.capture = Some(a);
                    veille.montrer_vue(bureau, true);
                    unsafe {
                        let _ = SetFocus(Some(hwnd(h)));
                    }
                }
                _ => {}
            }
            Some(LRESULT(0))
        }
        WM_KEYDOWN | WM_SYSKEYDOWN if e.veille.capture.is_some() => {
            let Etat { veille, bureau, .. } = e;
            match touche_capturee(wp.0 as u32, modificateurs()) {
                Capture::Attendre => {}
                Capture::Annuler => veille.annuler_choix(bureau),
                Capture::Choisir(t) => veille.choisir_touche(bureau, &t),
            }
            Some(LRESULT(0))
        }
        WM_CHAR | WM_SYSCHAR if e.veille.capture.is_some() => Some(LRESULT(0)),
        WM_MBUTTONDOWN | WM_XBUTTONDOWN if e.veille.capture.is_some() => {
            let b = if m == WM_MBUTTONDOWN {
                3
            } else if (wp.0 >> 16) & 0xFFFF == 1 {
                4
            } else {
                5
            };
            // (la touche faite dans le jeu par une touche simulée se choisit au clavier : le bouton ne compte pas)
            if e.veille.capture_souris()
                && let Some(t) = souris_capturee(b, modificateurs())
            {
                let Etat { veille, bureau, .. } = e;
                veille.choisir_touche(bureau, &t);
            }
            Some(LRESULT(if m == WM_XBUTTONDOWN { 1 } else { 0 }))
        }
        WM_CLOSE => {
            e.fermer_vue();
            Some(LRESULT(0))
        }
        _ => None,
    }
}

unsafe extern "system" fn proc_vue(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_ERASEBKGND => return LRESULT(1),
        WM_GETMINMAXINFO => {
            let i = unsafe { &mut *(lp.0 as *mut MINMAXINFO) };
            let s = unsafe { GetDpiForWindow(h) }.max(96) as i32;
            i.ptMinTrackSize.x = 760 * s / 96;
            i.ptMinTrackSize.y = 480 * s / 96;
            return LRESULT(0);
        }
        _ => {}
    }
    if let Some(Some(r)) = super::avec(|e| evenement(e, m, wp, lp)) {
        return r;
    }
    unsafe { DefWindowProcW(h, m, wp, lp) }
}
