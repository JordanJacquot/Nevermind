//! Le dessin de la barre flottante et de la vue de l'Organizer : une toile de pixels (couleurs prémultipliées, comme
//! UpdateLayeredWindow les veut), des formes aux bords adoucis (rectangles arrondis, cercles), les dégradés de la DA
//! (dont le bord « plasma » qui tourne), un flou pour la lueur, et le texte écrit par GDI (polices de Windows).

use std::cell::RefCell;
use std::collections::HashMap;
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS, CreateCompatibleDC,
    CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CALCRECT, DT_CENTER,
    DT_END_ELLIPSIS, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, DeleteDC, DeleteObject, DrawTextW,
    GetTextFaceW, HDC, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::core::HSTRING;

/// Une couleur : rouge, vert, bleu, opacité (de 0 à 1, pas prémultipliée).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Couleur(pub f32, pub f32, pub f32, pub f32);

impl Couleur {
    /// « #RRGGBB » ou « #AARRGGBB » (comme WPF).
    pub fn hex(t: &str) -> Couleur {
        let h = t.trim_start_matches('#');
        let v = u32::from_str_radix(h, 16).unwrap_or(0);
        let (a, r, g, b) = if h.len() == 8 {
            (v >> 24, (v >> 16) & 255, (v >> 8) & 255, v & 255)
        } else {
            (255, (v >> 16) & 255, (v >> 8) & 255, v & 255)
        };
        Couleur(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0)
    }

    pub fn opacite(self, o: f32) -> Couleur {
        Couleur(self.0, self.1, self.2, self.3 * o)
    }

    pub fn entre(a: Couleur, b: Couleur, t: f32) -> Couleur {
        let t = t.clamp(0.0, 1.0);
        Couleur(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t, a.3 + (b.3 - a.3) * t)
    }
}

/// Une façon de peindre : une couleur, ou un dégradé.
#[derive(Debug, Clone)]
pub enum Peinture {
    Unie(Couleur),
    /// Dégradé vertical de la boîte (haut, bas).
    Vertical(Couleur, Couleur),
    /// Dégradé en diagonale (de haut à gauche à bas à droite) aux couleurs données, tourné de l'angle (en degrés)
    /// autour du centre de la boîte (le bord « plasma »), avec son opacité.
    Plasma {
        couleurs: [Couleur; 4],
        angle: f32,
        opacite: f32,
    },
}

impl Peinture {
    /// La couleur au point (x, y) d'une boîte (bx, by, bw, bh).
    fn en(&self, x: f32, y: f32, boite: (f32, f32, f32, f32)) -> Couleur {
        match self {
            Peinture::Unie(c) => *c,
            Peinture::Vertical(a, b) => Couleur::entre(*a, *b, (y - boite.1) / boite.3.max(1.0)),
            Peinture::Plasma { couleurs, angle, opacite } => {
                // (coordonnées relatives de la boîte, tournées de -angle autour du centre, puis projetées sur la
                // diagonale (0,0) vers (1,1) : comme le LinearGradientBrush de WPF avec son RotateTransform relatif)
                let u = (x - boite.0) / boite.2.max(1.0) - 0.5;
                let v = (y - boite.1) / boite.3.max(1.0) - 0.5;
                let (s, c) = (-angle.to_radians()).sin_cos();
                let (ur, vr) = (u * c - v * s + 0.5, u * s + v * c + 0.5);
                let t = ((ur + vr) / 2.0).clamp(0.0, 1.0) * 3.0;
                let i = (t.floor() as usize).min(2);
                Couleur::entre(couleurs[i], couleurs[i + 1], t - i as f32).opacite(*opacite)
            }
        }
    }
}

/// Une toile de pixels (BGRA prémultipliés, ligne du haut d'abord).
#[derive(Clone)]
pub struct Toile {
    pub l: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

/// La distance (signée) d'un point au bord d'un rectangle arrondi centré en (0,0), demi-tailles (hx, hy), rayon r.
fn distance_arrondi(px: f32, py: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let r = r.min(hx).min(hy).max(0.0);
    let qx = px.abs() - (hx - r);
    let qy = py.abs() - (hy - r);
    let ext = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    ext + qx.max(qy).min(0.0) - r
}

impl Toile {
    pub fn nouvelle(l: i32, h: i32) -> Toile {
        Toile { l: l.max(1), h: h.max(1), px: vec![0; (l.max(1) * h.max(1)) as usize] }
    }

    /// Un pixel peint par-dessus (couverture de 0 à 1).
    #[inline]
    pub fn poser(&mut self, x: i32, y: i32, c: Couleur, couverture: f32) {
        if x < 0 || y < 0 || x >= self.l || y >= self.h || couverture <= 0.0 {
            return;
        }
        let a = (c.3 * couverture).clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let i = (y * self.l + x) as usize;
        let d = self.px[i];
        let (da, dr, dg, db) = ((d >> 24) as f32, ((d >> 16) & 255) as f32, ((d >> 8) & 255) as f32, (d & 255) as f32);
        let k = 1.0 - a;
        let na = a * 255.0 + da * k;
        let nr = c.0 * a * 255.0 + dr * k;
        let ng = c.1 * a * 255.0 + dg * k;
        let nb = c.2 * a * 255.0 + db * k;
        let q = |x: f32| (x.round().clamp(0.0, 255.0)) as u32;
        self.px[i] = (q(na) << 24) | (q(nr) << 16) | (q(ng) << 8) | q(nb);
    }

    /// Un rectangle arrondi plein.
    pub fn rect_arrondi(&mut self, x: f32, y: f32, l: f32, h: f32, r: f32, p: &Peinture) {
        self.forme(
            x,
            y,
            l,
            h,
            |px, py| {
                let d = distance_arrondi(px - (x + l / 2.0), py - (y + h / 2.0), l / 2.0, h / 2.0, r);
                (0.5 - d).clamp(0.0, 1.0)
            },
            p,
        );
    }

    #[allow(clippy::too_many_arguments)]
    /// Le bord d'un rectangle arrondi (à l'intérieur de la boîte, comme le Border de WPF).
    pub fn contour_arrondi(&mut self, x: f32, y: f32, l: f32, h: f32, r: f32, e: f32, p: &Peinture) {
        self.forme(
            x,
            y,
            l,
            h,
            |px, py| {
                let d = distance_arrondi(px - (x + l / 2.0), py - (y + h / 2.0), l / 2.0, h / 2.0, r);
                let dehors = (0.5 - d).clamp(0.0, 1.0);
                let dedans = (0.5 - (d + e)).clamp(0.0, 1.0);
                (dehors - dedans).max(0.0)
            },
            p,
        );
    }

    /// Un disque.
    pub fn disque(&mut self, cx: f32, cy: f32, r: f32, p: &Peinture) {
        self.rect_arrondi(cx - r, cy - r, 2.0 * r, 2.0 * r, r, p);
    }

    /// Un cercle (son bord).
    pub fn cercle(&mut self, cx: f32, cy: f32, r: f32, e: f32, p: &Peinture) {
        self.contour_arrondi(cx - r, cy - r, 2.0 * r, 2.0 * r, r, e, p);
    }

    fn forme(&mut self, x: f32, y: f32, l: f32, h: f32, couv: impl Fn(f32, f32) -> f32, p: &Peinture) {
        let boite = (x, y, l, h);
        let (x0, y0) = ((x.floor() as i32 - 1).max(0), (y.floor() as i32 - 1).max(0));
        let (x1, y1) = (((x + l).ceil() as i32 + 1).min(self.l), ((y + h).ceil() as i32 + 1).min(self.h));
        for py in y0..y1 {
            for px in x0..x1 {
                let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                let c = couv(fx, fy);
                if c > 0.0 {
                    let col = p.en(fx, fy, boite);
                    self.poser(px, py, col, c);
                }
            }
        }
    }

    /// Une autre toile posée par-dessus (avec son opacité).
    pub fn coller(&mut self, t: &Toile, x: i32, y: i32, opacite: f32) {
        for ty in 0..t.h {
            for tx in 0..t.l {
                let s = t.px[(ty * t.l + tx) as usize];
                let a = (s >> 24) as f32 / 255.0;
                if a <= 0.0 {
                    continue;
                }
                let c = Couleur(
                    ((s >> 16) & 255) as f32 / 255.0 / a,
                    ((s >> 8) & 255) as f32 / 255.0 / a,
                    (s & 255) as f32 / 255.0 / a,
                    a,
                );
                self.poser(x + tx, y + ty, c, opacite);
            }
        }
    }

    /// Un flou (trois passes de moyenne, proche du flou de WPF) de rayon r.
    pub fn flouter(&mut self, r: i32) {
        if r <= 0 {
            return;
        }
        let rr = (r / 2).max(1);
        for _ in 0..3 {
            self.passe_flou(rr, true);
            self.passe_flou(rr, false);
        }
    }

    fn passe_flou(&mut self, r: i32, horizontal: bool) {
        let (n, m) = if horizontal { (self.h, self.l) } else { (self.l, self.h) };
        let mut ligne = vec![[0u32; 4]; m as usize];
        for a in 0..n {
            for b in 0..m {
                let i = if horizontal { a * self.l + b } else { b * self.l + a } as usize;
                let p = self.px[i];
                ligne[b as usize] = [p >> 24, (p >> 16) & 255, (p >> 8) & 255, p & 255];
            }
            let mut somme = [0u32; 4];
            let largeur = (2 * r + 1) as u32;
            for k in -r..=r {
                let v = ligne[k.clamp(0, m - 1) as usize];
                for c in 0..4 {
                    somme[c] += v[c];
                }
            }
            for b in 0..m {
                let i = if horizontal { a * self.l + b } else { b * self.l + a } as usize;
                self.px[i] = ((somme[0] / largeur) << 24)
                    | ((somme[1] / largeur) << 16)
                    | ((somme[2] / largeur) << 8)
                    | (somme[3] / largeur);
                let sortant = ligne[(b - r).clamp(0, m - 1) as usize];
                let entrant = ligne[(b + r + 1).clamp(0, m - 1) as usize];
                for c in 0..4 {
                    somme[c] = somme[c] + entrant[c] - sortant[c];
                }
            }
        }
    }
}

// ------------------------------------------------------------------ le texte (GDI)

/// Une police : familles (la première qui existe), taille en pixels, graisse (400 normal, 600 demi-gras, 700 gras).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Police {
    pub familles: &'static str,
    pub taille: i32,
    pub graisse: i32,
}

thread_local! {
    static POLICES: RefCell<HashMap<Police, isize>> = RefCell::new(HashMap::new());
    static DC: RefCell<Option<isize>> = const { RefCell::new(None) };
}

fn dc_memoire() -> HDC {
    DC.with(|d| {
        let mut d = d.borrow_mut();
        if d.is_none() {
            *d = Some(unsafe { CreateCompatibleDC(None) }.0 as isize);
        }
        HDC(d.unwrap() as *mut _)
    })
}

/// La police de Windows (créée une fois par fil) : la première famille qui existe, sinon Segoe UI.
fn hfont(p: &Police) -> HFONT {
    POLICES.with(|m| {
        if let Some(h) = m.borrow().get(p) {
            return HFONT(*h as *mut _);
        }
        let dc = dc_memoire();
        let mut choisie = None;
        for f in p.familles.split(',').map(str::trim).chain(std::iter::once("Segoe UI")) {
            let h = unsafe {
                CreateFontW(
                    -p.taille.max(1),
                    0,
                    0,
                    0,
                    p.graisse,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET,
                    OUT_DEFAULT_PRECIS,
                    CLIP_DEFAULT_PRECIS,
                    ANTIALIASED_QUALITY,
                    DEFAULT_PITCH.0 as u32,
                    &HSTRING::from(f),
                )
            };
            if h.is_invalid() {
                continue;
            }
            // (Windows prend une autre police quand la famille n'existe pas : on vérifie son nom)
            let mut nom = [0u16; 64];
            let ancien = unsafe { SelectObject(dc, HGDIOBJ(h.0)) };
            let n = unsafe { GetTextFaceW(dc, Some(&mut nom)) };
            unsafe { SelectObject(dc, ancien) };
            let lu = String::from_utf16_lossy(&nom[..(n.max(1) as usize - 1).min(63)]);
            if lu.eq_ignore_ascii_case(f) || f == "Segoe UI" {
                choisie = Some(h);
                break;
            }
            unsafe {
                let _ = DeleteObject(HGDIOBJ(h.0));
            }
        }
        let h = choisie.unwrap_or_default();
        m.borrow_mut().insert(p.clone(), h.0 as isize);
        h
    })
}

/// La place d'un texte : sur une ligne, ou coupé en lignes dans la largeur donnée.
pub fn mesurer(texte: &str, p: &Police, largeur: Option<i32>) -> (i32, i32) {
    let dc = dc_memoire();
    let ancien = unsafe { SelectObject(dc, HGDIOBJ(hfont(p).0)) };
    let mut r = RECT { left: 0, top: 0, right: largeur.unwrap_or(100_000), bottom: 0 };
    let mut w: Vec<u16> = texte.encode_utf16().collect();
    let f = if largeur.is_some() {
        DT_WORDBREAK | DT_NOPREFIX | DT_CALCRECT
    } else {
        DT_SINGLELINE | DT_NOPREFIX | DT_CALCRECT
    };
    if w.is_empty() {
        w.push(u16::from(b' '));
    }
    unsafe { DrawTextW(dc, &mut w, &mut r, f) };
    unsafe { SelectObject(dc, ancien) };
    (r.right - r.left, r.bottom - r.top)
}

/// Comment poser un texte dans sa boîte.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mise {
    /// Sur une ligne, centré en hauteur, coupé par « ... » s'il est trop long.
    Ligne,
    /// Sur une ligne, centré.
    Centre,
    /// Coupé en lignes.
    Lignes,
}

#[allow(clippy::too_many_arguments)]
/// Écrit un texte dans la boîte (x, y, l, h) de la toile.
pub fn ecrire(t: &mut Toile, texte: &str, p: &Police, c: Couleur, x: i32, y: i32, l: i32, h: i32, mise: Mise) {
    if l <= 0 || h <= 0 || texte.is_empty() {
        return;
    }
    unsafe {
        let dc = dc_memoire();
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
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
        let Ok(bmp) = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else { return };
        if bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            return;
        }
        let ancien_bmp = SelectObject(dc, HGDIOBJ(bmp.0));
        let ancienne = SelectObject(dc, HGDIOBJ(hfont(p).0));
        SetTextColor(dc, COLORREF(0x00FF_FFFF));
        SetBkMode(dc, TRANSPARENT);
        let mut r = RECT { left: 0, top: 0, right: l, bottom: h };
        let mut w: Vec<u16> = texte.encode_utf16().collect();
        let f = match mise {
            Mise::Ligne => DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
            Mise::Centre => DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
            Mise::Lignes => DT_WORDBREAK | DT_NOPREFIX,
        };
        DrawTextW(dc, &mut w, &mut r, f);
        let n = (l * h) as usize;
        let s = std::slice::from_raw_parts(bits as *const u32, n);
        for yy in 0..h {
            for xx in 0..l {
                let v = s[(yy * l + xx) as usize];
                let couv = ((v >> 8) & 255).max((v >> 16) & 255).max(v & 255) as f32 / 255.0;
                if couv > 0.0 {
                    t.poser(x + xx, y + yy, c, couv);
                }
            }
        }
        SelectObject(dc, ancienne);
        SelectObject(dc, ancien_bmp);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
    }
}

/// Libère les polices et le DC de ce fil (à la fin du fil de l'Organizer).
pub fn liberer() {
    POLICES.with(|m| {
        for (_, h) in m.borrow_mut().drain() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(h as *mut _));
            }
        }
    });
    DC.with(|d| {
        if let Some(h) = d.borrow_mut().take() {
            unsafe {
                let _ = DeleteDC(HDC(h as *mut _));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formes() {
        let mut t = Toile::nouvelle(20, 10);
        t.rect_arrondi(0.0, 0.0, 20.0, 10.0, 5.0, &Peinture::Unie(Couleur::hex("#FF0000")));
        // (le centre plein, le coin vide)
        assert_eq!(t.px[(5 * 20 + 10) as usize] >> 24, 255);
        assert!(t.px[0] >> 24 < 80);
        let p = Peinture::Plasma { couleurs: [Couleur::hex("#00E5FF"); 4], angle: 30.0, opacite: 0.5 };
        assert!((p.en(3.0, 3.0, (0.0, 0.0, 10.0, 10.0)).3 - 0.5).abs() < 1e-6);
        t.flouter(4);
    }
}
