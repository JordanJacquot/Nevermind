//! La vue de l'Organizer dans sa propre fenêtre, sur un seul écran en trois colonnes (maquette « E1 » du 12/11) :
//! « Mes personnages » (les connectés, leur logo, leur touche ; glisser une ligne pour l'ordre d'initiative, un clic
//! pour mettre sa fenêtre devant), « Raccourcis » et « Barre flottante », « Boutons de souris » (les touches simulées en
//! pastilles, la création en deux grandes cases). Les textes viennent de `organizer::vue`.

use super::barre::bmp;
use super::dessin::{self, Couleur, Mise, Peinture, Police, Toile};
use super::{Etat, Theme, hwnd};
use crate::organizer::vue::{self as v, Modele};
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
    Touche(String),
    Poignee(String),
    Ligne(String),
    /// Un clic sur la ligne d'un perso : sa fenêtre devant.
    Afficher(isize),
    /// Une pastille de bouton de souris : changer son bouton, sa touche, l'enlever.
    PuceBouton(String),
    PuceTouche(String),
    Enlever(String),
    Ajouter,
    Annuler,
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
                (1180.0 * s) as i32,
                (720.0 * s) as i32,
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
        let icones = |taille: f32| Police {
            familles: "Segoe Fluent Icons, Segoe MDL2 Assets",
            taille: (taille * s).round() as i32,
            graisse: 400,
        };
        let mono = |taille: f32| Police {
            familles: "Cascadia Code, Consolas",
            taille: (taille * s).round() as i32,
            graisse: 700,
        };

        // ------------------------------------------------ En-tête : le titre, et l'interrupteur à droite
        let x0 = 22.0 * s;
        let largeur = l as f32 - 2.0 * x0;
        let y = 16.0 * s + dy;
        let ft = self.police(23.0, 700);
        let (_, th) = dessin::mesurer(v::TITRE, &ft, None);
        dessin::ecrire(&mut t, v::TITRE, &ft, blanc, x0 as i32, y as i32, l, th, Mise::Ligne);
        let fs = self.police(12.5, 400);
        let (_, sh) = dessin::mesurer(v::SOUS_TITRE, &fs, None);
        let ys = y + th as f32 + 2.0 * s;
        dessin::ecrire(&mut t, v::SOUS_TITRE, &fs, Couleur::hex("#A6A1BC"), x0 as i32, ys as i32, l, sh, Mise::Ligne);
        let mut corps = Vec::new();
        let il = 250.0 * s;
        self.interrupteur(
            &mut corps,
            x0 + largeur - il,
            y + 4.0 * s,
            il,
            v::ACTIVER,
            v::ACTIVER_NOTE,
            m.on,
            Zone::Activer,
        );
        self.peindre(&mut t, corps);
        let haut = ys + sh as f32 + 14.0 * s;

        // ------------------------------------------------ Les trois colonnes (1 ; 0,95 ; 1,25)
        let ecart = 14.0 * s;
        let utile = largeur - 2.0 * ecart;
        let (l1, l2) = (utile * 1.0 / 3.2, utile * 0.95 / 3.2);
        let l3 = utile - l1 - l2;
        let (xa, xb, xc) = (x0, x0 + l1 + ecart, x0 + l1 + l2 + 2.0 * ecart);
        let marge = 15.0 * s;

        // ------------------------------------------------ Mes personnages
        let (cx, cl) = (xa + marge, l1 - 2.0 * marge);
        let mut cy = haut + 14.0 * s;
        let mut corps = Vec::new();
        cy = self.entete_carte(&mut corps, cx, cy, cl, v::PERSOS_TITRE, v::PERSOS_NOTE, p);
        if m.vide {
            let ft = self.police(12.5, 400);
            let (_, hh) = dessin::mesurer(v::AUCUNE_FENETRE, &ft, Some((cl - 28.0 * s) as i32));
            let bh = hh as f32 + 28.0 * s;
            corps.push(Dessin::Rect(cx, cy, cl, bh, 12.0 * s, Peinture::Unie(Couleur::hex("#0AFFFFFF"))));
            corps.push(Dessin::Texte(
                v::AUCUNE_FENETRE.into(),
                ft,
                Couleur::hex("#A6A1BC"),
                cx + 14.0 * s,
                cy + 14.0 * s,
                cl - 28.0 * s,
                hh as f32,
                Mise::Lignes,
            ));
            cy += bh;
        }
        for li in &m.lignes {
            let rh = 42.0 * s;
            let fond = Couleur::hex(if li.devant { "#1400E5FF" } else { "#10FFFFFF" });
            corps.push(Dessin::Rect(cx, cy, cl, rh, 11.0 * s, Peinture::Unie(fond)));
            let cible = !li.nom.is_empty() && self.glisse.is_some() && self.cible.as_deref() == Some(li.nom.as_str());
            let bord =
                if cible || li.devant { p.opacite(if cible { 1.0 } else { 0.55 }) } else { Couleur::hex("#14FFFFFF") };
            corps.push(Dessin::Bord(cx, cy, cl, rh, 11.0 * s, 1.0 * s, Peinture::Unie(bord)));
            let mut x = cx + 9.0 * s;
            if !li.nom.is_empty() {
                corps.push(Dessin::Texte(
                    "\u{E76F}".into(),
                    icones(12.0),
                    Couleur::hex("#655E7E"),
                    x,
                    cy,
                    13.0 * s,
                    rh,
                    Mise::Centre,
                ));
                self.zones.push(((x - 4.0 * s, cy, 21.0 * s, rh), Zone::Poignee(li.nom.clone())));
                self.zones.push(((cx, cy, cl, rh), Zone::Ligne(li.nom.clone())));
            }
            self.zones.push(((cx, cy, cl, rh), Zone::Afficher(li.hwnd)));
            x += 13.0 * s + 8.0 * s;
            corps.push(Dessin::Texte(li.numero.clone(), self.police(12.5, 700), p, x, cy, 14.0 * s, rh, Mise::Ligne));
            x += 14.0 * s + 4.0 * s;
            corps.push(Dessin::Pastille(
                li.pastille.initiale.clone(),
                li.pastille.couleur,
                li.pastille.id_classe,
                x + 15.0 * s,
                cy + rh / 2.0,
                15.0 * s,
                1.0,
            ));
            x += 30.0 * s + 9.0 * s;
            // La touche du perso, à droite
            let mut bw = 0.0;
            if let Some(k) = &li.touche {
                let (tl, tth) = dessin::mesurer(&k.bouton, &mono(11.5), None);
                bw = (tl as f32 + 18.0 * s).max(46.0 * s).min(cl * 0.45);
                let bh = tth as f32 + 9.0 * s;
                let bb = (cx + cl - 9.0 * s - bw, cy + (rh - bh) / 2.0, bw, bh);
                corps.push(Dessin::Touche(k.bouton.clone(), bb, k.en_choix, k.vide));
                infobulles.push((bb, if k.refusee { v::DEJA_PRISE } else { v::TOUCHE_AIDE }));
                self.zones.push((bb, Zone::Touche(k.action.clone())));
            }
            let info_l = cl - (x - cx) - bw - 18.0 * s;
            corps.push(Dessin::Texte(
                li.titre.clone(),
                self.police(13.0, 600),
                blanc,
                x,
                cy + 5.0 * s,
                info_l,
                17.0 * s,
                Mise::Ligne,
            ));
            let refus = li.touche.as_ref().is_some_and(|k| k.refusee);
            let (etat, couleur_etat) = if refus {
                ("Touche déjà prise par un autre programme".to_string(), Couleur::hex("#F04438"))
            } else if li.devant {
                (format!("{}  ·  devant", li.etat), Couleur::hex("#22D37A"))
            } else {
                (li.etat.clone(), Couleur::hex("#22D37A"))
            };
            corps.push(Dessin::Texte(
                etat,
                self.police(11.0, 400),
                couleur_etat,
                x,
                cy + 22.0 * s,
                info_l,
                15.0 * s,
                Mise::Ligne,
            ));
            cy += rh + 5.0 * s;
        }
        let fin_1 = cy + 15.0 * s - 5.0 * s;
        self.carte(&mut t, xa, haut, l1, fin_1 - haut);
        self.peindre(&mut t, corps);

        // ------------------------------------------------ Raccourcis (perso suivant, précédent)
        let (cx, cl) = (xb + marge, l2 - 2.0 * marge);
        let mut cy = haut + 14.0 * s;
        let mut corps = Vec::new();
        cy = self.entete_carte(&mut corps, cx, cy, cl, v::RACCOURCIS_TITRE, v::RACCOURCIS_NOTE, self.theme.s);
        for k in &m.touches {
            let (tl, tth) = dessin::mesurer(&k.bouton, &mono(11.5), None);
            let bw = (tl as f32 + 18.0 * s).max(60.0 * s).min(cl * 0.55);
            let bh = tth as f32 + 9.0 * s;
            let ll = cl - bw - 8.0 * s;
            let fr = self.police(11.0, 400);
            let hr = if k.refusee { dessin::mesurer(v::DEJA_PRISE, &fr, Some(ll as i32)).1 as f32 } else { 0.0 };
            let lh = if k.refusee { 19.0 * s + hr } else { 18.0 * s };
            let rh = bh.max(lh);
            corps.push(Dessin::Texte(
                k.libelle.clone(),
                self.police(12.5, 600),
                Couleur::hex("#EEEBF7"),
                cx,
                cy + (rh - lh) / 2.0,
                ll,
                18.0 * s,
                Mise::Ligne,
            ));
            if k.refusee {
                corps.push(Dessin::Texte(
                    v::DEJA_PRISE.into(),
                    fr,
                    Couleur::hex("#F04438"),
                    cx,
                    cy + (rh - lh) / 2.0 + 19.0 * s,
                    ll,
                    hr,
                    Mise::Lignes,
                ));
            }
            let bb = (cx + cl - bw, cy + (rh - bh) / 2.0, bw, bh);
            corps.push(Dessin::Touche(k.bouton.clone(), bb, k.en_choix, k.vide));
            infobulles.push((bb, v::TOUCHE_AIDE));
            self.zones.push((bb, Zone::Touche(k.action.clone())));
            cy += rh + 8.0 * s;
        }
        let fin_r = cy + 15.0 * s - 8.0 * s;
        self.carte(&mut t, xb, haut, l2, fin_r - haut);
        self.peindre(&mut t, corps);

        // ------------------------------------------------ Barre flottante, sous les raccourcis
        let haut_b = fin_r + ecart;
        let mut cy = haut_b + 14.0 * s;
        let mut corps = Vec::new();
        cy = self.entete_carte(&mut corps, cx, cy, cl, v::BARRE_TITRE, v::BARRE_NOTE, self.theme.p);
        // (l'aperçu de la barre : les logos des persos connectés, celui devant mis en avant)
        let nommes: Vec<&v::Ligne> = m.lignes.iter().filter(|x| !x.nom.is_empty()).collect();
        if !nommes.is_empty() {
            let r = 11.0 * s;
            let pas = 2.0 * r + 3.0 * s;
            let par_ligne = (((cl - 14.0 * s) / pas).floor() as usize).max(1);
            let lignes_b = nommes.len().div_ceil(par_ligne);
            let bl = (nommes.len().min(par_ligne) as f32) * pas + 11.0 * s;
            let bh = lignes_b as f32 * pas + 7.0 * s;
            corps.push(Dessin::Rect(
                cx,
                cy,
                bl,
                bh,
                bh.min(2.0 * r + 10.0 * s) / 2.0,
                Peinture::Unie(Couleur::hex("#EB0C0920")),
            ));
            corps.push(Dessin::Bord(
                cx,
                cy,
                bl,
                bh,
                bh.min(2.0 * r + 10.0 * s) / 2.0,
                1.0 * s,
                Peinture::Unie(p.opacite(0.35)),
            ));
            for (i, li) in nommes.iter().enumerate() {
                let (col, lig) = (i % par_ligne, i / par_ligne);
                let px = cx + 7.0 * s + col as f32 * pas + r;
                let py = cy + 5.0 * s + lig as f32 * pas + r;
                corps.push(Dessin::Pastille(
                    li.pastille.initiale.clone(),
                    li.pastille.couleur,
                    li.pastille.id_classe,
                    px,
                    py,
                    r,
                    if li.devant { 1.0 } else { 0.75 },
                ));
                if li.devant {
                    corps.push(Dessin::Anneau(px, py, r + 1.5 * s, p));
                }
            }
            cy += bh + 10.0 * s;
        }
        cy = self.interrupteur(&mut corps, cx, cy, cl, v::BARRE, v::BARRE_SOUS, m.barre, Zone::Barre);
        cy = self.interrupteur(&mut corps, cx, cy, cl, v::TOUJOURS, v::TOUJOURS_SOUS, m.toujours, Zone::Toujours);
        let fb = self.police(12.0, 600);
        let (bl, _) = dessin::mesurer(v::REMETTRE, &fb, None);
        let bb = (cx, cy, (bl as f32 + 24.0 * s).min(cl), 30.0 * s);
        corps.push(Dessin::Bouton(v::REMETTRE.into(), bb, self.survol == Some(Zone::Remettre)));
        infobulles.push((bb, v::PLEIN_ECRAN));
        self.zones.push((bb, Zone::Remettre));
        cy += 30.0 * s;
        let fin_b = cy + 15.0 * s;
        self.carte(&mut t, xb, haut_b, l2, fin_b - haut_b);
        self.peindre(&mut t, corps);

        // ------------------------------------------------ Boutons de souris
        let (cx, cl) = (xc + marge, l3 - 2.0 * marge);
        let mut cy = haut + 14.0 * s;
        let mut corps = Vec::new();
        cy = self.entete_carte(&mut corps, cx, cy, cl, v::SOURIS_TITRE, v::SOURIS_NOTE, self.theme.t);
        if let Some(c) = &m.creation {
            // Les deux grandes cases : le bouton de souris, puis la touche du jeu
            let fl = 26.0 * s;
            let tl = (cl - fl) / 2.0;
            let th2 = 104.0 * s;
            for (i, (titre, glyphe, valeur, active)) in [
                (
                    v::CASE_SOURIS,
                    "\u{E962}",
                    if c.etape == 1 { v::EN_ATTENTE.to_string() } else { c.bouton.clone() },
                    c.etape == 1,
                ),
                (
                    v::CASE_TOUCHE,
                    "\u{E765}",
                    if c.etape == 2 { v::EN_ATTENTE.to_string() } else { "?".to_string() },
                    c.etape == 2,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let x = cx + i as f32 * (tl + fl);
                let fond = if active { p.opacite(0.08) } else { Couleur::hex("#08FFFFFF") };
                let bord = if active { p } else { Couleur::hex("#17FFFFFF") };
                corps.push(Dessin::Rect(x, cy, tl, th2, 12.0 * s, Peinture::Unie(fond)));
                corps.push(Dessin::Bord(x, cy, tl, th2, 12.0 * s, 1.0 * s, Peinture::Unie(bord)));
                let op = if active || i == 0 { 1.0 } else { 0.45 };
                corps.push(Dessin::Texte(
                    titre.to_uppercase(),
                    self.police(10.5, 700),
                    gris.opacite(op),
                    x,
                    cy + 9.0 * s,
                    tl,
                    14.0 * s,
                    Mise::Centre,
                ));
                corps.push(Dessin::Texte(
                    glyphe.into(),
                    icones(24.0),
                    blanc.opacite(op),
                    x,
                    cy + 26.0 * s,
                    tl,
                    34.0 * s,
                    Mise::Centre,
                ));
                let (vl, vh) = dessin::mesurer(&valeur, &mono(11.5), None);
                let bw = (vl as f32 + 18.0 * s).min(tl - 12.0 * s);
                let bh = vh as f32 + 9.0 * s;
                let bb = (x + (tl - bw) / 2.0, cy + 68.0 * s, bw, bh);
                corps.push(Dessin::Touche(valeur, bb, active, !active && i == 1));
            }
            corps.push(Dessin::Texte(
                "→".into(),
                self.police(18.0, 700),
                self.theme.s,
                cx + tl,
                cy,
                fl,
                th2,
                Mise::Centre,
            ));
            cy += th2 + 10.0 * s;
            let consigne = if c.etape == 1 { v::ETAPE_SOURIS } else { v::ETAPE_TOUCHE };
            let fc = self.police(12.5, 600);
            let (_, ch) = dessin::mesurer(consigne, &fc, Some(cl as i32));
            corps.push(Dessin::Texte(
                consigne.into(),
                fc,
                Couleur::hex("#EEEBF7"),
                cx,
                cy,
                cl,
                ch as f32,
                Mise::Lignes,
            ));
            cy += ch as f32 + 10.0 * s;
            let (al, _) = dessin::mesurer(v::ANNULER, &self.police(12.0, 600), None);
            let ab = (cx + cl - al as f32 - 24.0 * s, cy, al as f32 + 24.0 * s, 30.0 * s);
            corps.push(Dessin::Bouton(v::ANNULER.into(), ab, self.survol == Some(Zone::Annuler)));
            self.zones.push((ab, Zone::Annuler));
            cy += 30.0 * s;
        } else {
            // Les pastilles, deux par ligne : « bouton → touche ✕ »
            let ecart_p = 6.0 * s;
            let pl = (cl - ecart_p) / 2.0;
            let ph = 28.0 * s;
            for (i, pu) in m.puces.iter().enumerate() {
                let x = cx + (i % 2) as f32 * (pl + ecart_p);
                let y = cy + (i / 2) as f32 * (ph + ecart_p);
                corps.push(Dessin::Rect(x, y, pl, ph, 9.0 * s, Peinture::Unie(Couleur::hex("#0CFFFFFF"))));
                corps.push(Dessin::Bord(x, y, pl, ph, 9.0 * s, 1.0 * s, Peinture::Unie(Couleur::hex("#14FFFFFF"))));
                let xl = 20.0 * s;
                let (kl, kh) = dessin::mesurer(&pu.touche, &mono(11.0), None);
                let kw = (kl as f32 + 22.0 * s).max(30.0 * s).min(pl * 0.45);
                let kb = (x + pl - xl - 4.0 * s - kw, y + (ph - kh as f32 - 6.0 * s) / 2.0, kw, kh as f32 + 6.0 * s);
                let bx = x + 6.0 * s;
                corps.push(Dessin::Texte("\u{E962}".into(), icones(11.0), gris, bx, y, 13.0 * s, ph, Mise::Centre));
                let bl_ = kb.0 - bx - 16.0 * s - 18.0 * s;
                let couleur_b = if pu.choix_bouton { p } else { Couleur::hex("#EEEBF7") };
                corps.push(Dessin::Texte(
                    pu.bouton.clone(),
                    self.police(11.5, 600),
                    couleur_b,
                    bx + 16.0 * s,
                    y,
                    bl_,
                    ph,
                    Mise::Ligne,
                ));
                self.zones.push(((bx, y, bl_ + 16.0 * s, ph), Zone::PuceBouton(pu.id.clone())));
                corps.push(Dessin::Texte(
                    "→".into(),
                    self.police(12.0, 700),
                    self.theme.s,
                    kb.0 - 18.0 * s,
                    y,
                    16.0 * s,
                    ph,
                    Mise::Centre,
                ));
                corps.push(Dessin::Touche(pu.touche.clone(), kb, pu.choix_touche, false));
                self.zones.push((kb, Zone::PuceTouche(pu.id.clone())));
                let xb_ = (x + pl - xl, y, xl, ph);
                let couleur_x =
                    if self.survol == Some(Zone::Enlever(pu.id.clone())) { Couleur::hex("#F04438") } else { gris };
                corps.push(Dessin::Texte(
                    "\u{E711}".into(),
                    icones(9.0),
                    couleur_x,
                    xb_.0,
                    y,
                    xl - 4.0 * s,
                    ph,
                    Mise::Centre,
                ));
                self.zones.push((xb_, Zone::Enlever(pu.id.clone())));
                infobulles.push(((x, y, pl - xl, ph), v::PUCE_AIDE));
                infobulles.push((xb_, v::ENLEVER_AIDE));
            }
            if !m.puces.is_empty() {
                cy += m.puces.len().div_ceil(2) as f32 * (ph + ecart_p) + 4.0 * s;
            }
            if m.plein {
                let fp = self.police(11.5, 400);
                let (_, hp) = dessin::mesurer(v::PLEIN, &fp, Some(cl as i32));
                corps.push(Dessin::Texte(v::PLEIN.into(), fp, gris, cx, cy, cl, hp as f32, Mise::Lignes));
                cy += hp as f32;
            } else {
                let fa = self.police(12.0, 600);
                let (al, _) = dessin::mesurer(v::AJOUTER, &fa, None);
                let ab = (cx, cy, al as f32 + 26.0 * s, 30.0 * s);
                corps.push(Dessin::Ajouter(v::AJOUTER.into(), ab, self.survol == Some(Zone::Ajouter)));
                self.zones.push((ab, Zone::Ajouter));
                cy += 30.0 * s;
            }
        }
        let fin_3 = cy + 15.0 * s;
        self.carte(&mut t, xc, haut, l3, fin_3 - haut);
        self.peindre(&mut t, corps);

        // ------------------------------------------------ La note du bas
        let bas = fin_1.max(fin_b).max(fin_3) + 12.0 * s;
        let f = self.police(11.5, 400);
        let (_, nh) = dessin::mesurer(v::NOTE_BAS, &f, Some((largeur - 4.0 * s) as i32));
        dessin::ecrire(
            &mut t,
            v::NOTE_BAS,
            &f,
            Couleur::hex("#655E7E"),
            (x0 + 4.0 * s) as i32,
            bas as i32,
            (largeur - 4.0 * s) as i32,
            nh,
            Mise::Lignes,
        );
        self.hauteur = bas + nh as f32 + 14.0 * s - dy;
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
                Dessin::Anneau(x, y, r, c) => t.cercle(x, y, r, 1.5 * s, &Peinture::Unie(c)),
                Dessin::Ajouter(tx, b, survol) => {
                    let r = self.theme.rayon(10.0, 2.0) * s;
                    t.rect_arrondi(b.0, b.1, b.2, b.3, r, &Peinture::Unie(p.opacite(if survol { 0.16 } else { 0.08 })));
                    t.contour_arrondi(b.0, b.1, b.2, b.3, r, 1.0 * s, &Peinture::Unie(p.opacite(0.5)));
                    dessin::ecrire(
                        t,
                        &tx,
                        &self.police(12.0, 600),
                        p,
                        b.0 as i32,
                        b.1 as i32,
                        b.2 as i32,
                        b.3 as i32,
                        Mise::Centre,
                    );
                }
                // (le logo de la classe s'il est là, sinon l'initiale sur la couleur de la classe)
                Dessin::Pastille(_, _, id, x, y, r, op) if super::logos::logo(id).is_some() => {
                    if let Some(lg) = super::logos::logo(id) {
                        t.coller_echelle(&lg, x - r, y - r, 2.0 * r, 2.0 * r, op);
                    }
                }
                Dessin::Pastille(ini, col, _, x, y, r, op) => {
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
            Zone::Afficher(_) => 3,
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
    /// Initiale, couleur, numéro de la classe (pour son logo), centre, rayon, opacité.
    Pastille(String, &'static str, u32, f32, f32, f32, f32),
    /// Un anneau (le perso devant, dans l'aperçu de la barre).
    Anneau(f32, f32, f32, Couleur),
    /// « + Ajouter un bouton ».
    Ajouter(String, (f32, f32, f32, f32), bool),
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
                None => IDC_ARROW,
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
            let lignes: Vec<(String, isize)> = fv.modele.lignes.iter().map(|l| (l.nom.clone(), l.hwnd)).collect();
            let Etat { veille, bureau, .. } = e;
            let choisir = |veille: &mut crate::organizer::veille::Veille, bureau: &mut super::BureauWin, a: String| {
                veille.capture = Some(a);
                veille.montrer_vue(bureau, true);
                unsafe {
                    let _ = SetFocus(Some(hwnd(h)));
                }
            };
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
                Zone::Afficher(hw) => veille.afficher_fenetre(bureau, hw),
                Zone::Ligne(n) => {
                    if let Some((_, hw)) = lignes.iter().find(|(x, _)| *x == n) {
                        veille.afficher_fenetre(bureau, *hw);
                    }
                }
                Zone::Touche(a) => choisir(veille, bureau, a),
                Zone::PuceBouton(sid) => choisir(veille, bureau, sid),
                Zone::PuceTouche(sid) => choisir(veille, bureau, format!("E{}", &sid[1..])),
                Zone::Enlever(sid) => veille.supprimer_simulee(bureau, &sid),
                Zone::Ajouter => {
                    veille.commencer_creation(bureau);
                    unsafe {
                        let _ = SetFocus(Some(hwnd(h)));
                    }
                }
                Zone::Annuler => veille.annuler_choix(bureau),
                Zone::Poignee(_) => {}
            }
            Some(LRESULT(0))
        }
        WM_KEYDOWN | WM_SYSKEYDOWN if e.veille.capture.is_some() => {
            let Etat { veille, bureau, .. } = e;
            // (le bouton d'une touche simulée se choisit à la souris : au clavier, seul Échap compte)
            if !veille.capture_clavier() {
                if wp.0 as u32 == 0x1B {
                    veille.annuler_choix(bureau);
                }
                return Some(LRESULT(0));
            }
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
            i.ptMinTrackSize.x = 1000 * s / 96;
            i.ptMinTrackSize.y = 560 * s / 96;
            return LRESULT(0);
        }
        _ => {}
    }
    if let Some(Some(r)) = super::avec(|e| evenement(e, m, wp, lp)) {
        return r;
    }
    unsafe { DefWindowProcW(h, m, wp, lp) }
}
