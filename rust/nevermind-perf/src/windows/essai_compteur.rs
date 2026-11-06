//! Essai à l'écran du compteur de FPS (test ignoré, jamais lancé par `cargo test`) : SEULEMENT sur un bureau Windows à
//! part (rust\nevermind-appli\outils\sur-bureau.ps1, bureau « MindcareEssai-<n> ») ; ailleurs il refuse. Une fenêtre
//! d'essai est mise au premier plan, puis le compteur est montré dans chaque style (complet, discret) et chaque coin
//! (hg, hd, bg, bd) ; l'écran du bureau est photographié à chaque fois (BMP), avec la place du compteur.
//!
//!   NEVERMIND_ESSAI_COMPTEUR=<dossier> cargo test -p nevermind-perf --lib essai_compteur_a_l_ecran -- --ignored

use crate::systeme::{Compteur, EtatCompteur};
use crate::windows::compteur::CompteurWindows;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, CreateSolidBrush, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, GetDC, HBRUSH, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::StationsAndDesktops::{GetThreadDesktop, GetUserObjectInformationW, UOI_NAME};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW, GetSystemMetrics, GetWindowRect,
    IsWindowVisible, MSG, PM_REMOVE, PeekMessageW, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW,
    SetForegroundWindow, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};
use windows::core::w;

extern "system" fn proc_essai(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY : traitement par défaut de Windows
    unsafe { DefWindowProcW(h, msg, w, l) }
}

/// Le nom du bureau de ce fil
fn bureau() -> String {
    // SAFETY : lecture du nom du bureau de ce fil dans un tampon de taille connue
    unsafe {
        let Ok(d) = GetThreadDesktop(GetCurrentThreadId()) else { return String::new() };
        let mut t = [0u16; 256];
        let mut n = 0u32;
        if GetUserObjectInformationW(
            windows::Win32::Foundation::HANDLE(d.0),
            UOI_NAME,
            Some(t.as_mut_ptr().cast()),
            (t.len() * 2) as u32,
            Some(&mut n),
        )
        .is_err()
        {
            return String::new();
        }
        let fin = t.iter().position(|c| *c == 0).unwrap_or(t.len());
        String::from_utf16_lossy(&t[..fin])
    }
}

/// Les messages de ce fil pendant `ms`
fn pomper(ms: u64) {
    let fin = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    while std::time::Instant::now() < fin {
        // SAFETY : boucle de messages ordinaire du fil
        unsafe {
            let mut m = MSG::default();
            while PeekMessageW(&mut m, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&m);
                DispatchMessageW(&m);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
}

/// L'image d'une fenêtre (PrintWindow, PW_RENDERFULLCONTENT) en BGRA, avec sa place à l'écran
fn imprimer(h: HWND) -> Option<(RECT, Vec<u8>)> {
    // SAFETY : contexte et image en mémoire créés ici, détruits à la fin ; l'image fait largeur x hauteur x 4 octets
    unsafe {
        let mut r = RECT::default();
        GetWindowRect(h, &mut r).ok()?;
        let (l, ht) = (r.right - r.left, r.bottom - r.top);
        if l <= 0 || ht <= 0 {
            return None;
        }
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: l,
                biHeight: -ht,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let ecran = GetDC(None);
        let mem = CreateCompatibleDC(Some(ecran));
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let img = CreateDIBSection(Some(mem), &bi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let avant = SelectObject(mem, HGDIOBJ::from(img));
        let _ = PrintWindow(h, mem, PRINT_WINDOW_FLAGS(2));
        let px = std::slice::from_raw_parts(bits as *const u8, (l * ht * 4) as usize).to_vec();
        SelectObject(mem, avant);
        let _ = DeleteObject(HGDIOBJ::from(img));
        let _ = DeleteDC(mem);
        ReleaseDC(None, ecran);
        Some((r, px))
    }
}

/// L'écran du bureau tel que Windows le composerait (le bureau d'essai n'étant pas affiché, Windows n'y compose rien :
/// sa photo directe est noire) : un fond gris, la fenêtre d'essai, puis le compteur par-dessus avec sa transparence
/// (pixels prémultipliés d'UpdateLayeredWindow). En BMP 24 bits. Rend le nombre de pixels non vides du compteur.
fn composer(fichier: &std::path::Path, essai: HWND, compteur: HWND) -> usize {
    // SAFETY : simple lecture de la taille de l'écran
    let (l, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    let mut toile = vec![0x30u8; (l * h * 4) as usize];
    let mut plein = 0;
    for (k, f) in [essai, compteur].into_iter().enumerate() {
        let Some((r, px)) = imprimer(f) else { continue };
        let lf = r.right - r.left;
        let transparent = k == 1 && px.as_chunks::<4>().0.iter().all(|p| p[3] == 0);
        for y in 0..(r.bottom - r.top) {
            for x in 0..lf {
                let (sx, sy) = (r.left + x, r.top + y);
                if sx < 0 || sy < 0 || sx >= l || sy >= h {
                    continue;
                }
                let s = ((y * lf + x) * 4) as usize;
                let d = ((sy * l + sx) * 4) as usize;
                let a = if k == 0 {
                    255
                } else if transparent {
                    if px[s] | px[s + 1] | px[s + 2] != 0 { 255 } else { 0 }
                } else {
                    px[s + 3]
                };
                if k == 1 && a > 0 {
                    plein += 1;
                }
                for c in 0..3 {
                    let v = if k == 0 {
                        px[s + c] as u32
                    } else {
                        // (prémultiplié : source + fond x (1 - alpha))
                        (px[s + c] as u32 + toile[d + c] as u32 * (255 - a as u32) / 255).min(255)
                    };
                    toile[d + c] = v as u8;
                }
            }
        }
    }
    // BMP 24 bits, de bas en haut
    let ligne = ((l * 3 + 3) / 4 * 4) as usize;
    let taille = 54 + ligne * h as usize;
    let mut o = Vec::with_capacity(taille);
    o.extend_from_slice(b"BM");
    o.extend_from_slice(&(taille as u32).to_le_bytes());
    o.extend_from_slice(&[0; 4]);
    o.extend_from_slice(&54u32.to_le_bytes());
    o.extend_from_slice(&40u32.to_le_bytes());
    o.extend_from_slice(&l.to_le_bytes());
    o.extend_from_slice(&h.to_le_bytes());
    o.extend_from_slice(&1u16.to_le_bytes());
    o.extend_from_slice(&24u16.to_le_bytes());
    o.extend_from_slice(&[0; 24]);
    for y in (0..h as usize).rev() {
        let debut = o.len();
        for x in 0..l as usize {
            let i = (y * l as usize + x) * 4;
            o.extend_from_slice(&toile[i..i + 3]);
        }
        o.resize(debut + ligne, 0);
    }
    let _ = std::fs::write(fichier, o);
    plein
}

#[test]
#[ignore]
fn essai_compteur_a_l_ecran() {
    let Ok(dossier) = std::env::var("NEVERMIND_ESSAI_COMPTEUR") else { return };
    let b = bureau();
    assert!(b.starts_with("MindcareEssai-"), "refusé : seulement sur un bureau à part (bureau « {b} »)");
    let dossier = std::path::PathBuf::from(dossier);
    std::fs::create_dir_all(&dossier).unwrap();
    let mut notes = Vec::new();
    // SAFETY : une fenêtre d'essai de ce fil, détruite à la fin
    let fenetre = unsafe {
        let module = GetModuleHandleW(None).unwrap();
        let classe = WNDCLASSW {
            lpfnWndProc: Some(proc_essai),
            hInstance: module.into(),
            lpszClassName: w!("NevermindEssaiJeu"),
            hbrBackground: HBRUSH(CreateSolidBrush(windows::Win32::Foundation::COLORREF(0x0050_3020)).0),
            ..Default::default()
        };
        RegisterClassW(&classe);
        let f = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("NevermindEssaiJeu"),
            w!("Jeu d'essai"),
            WS_OVERLAPPEDWINDOW,
            120,
            80,
            1100,
            700,
            None,
            None,
            Some(module.into()),
            None,
        )
        .unwrap();
        let _ = ShowWindow(f, SW_SHOW);
        let _ = SetForegroundWindow(f);
        f
    };
    pomper(800);
    let mut c = CompteurWindows::default();
    for style in ["complet", "discret"] {
        for coin in ["hg", "hd", "bg", "bd"] {
            c.montrer(&EtatCompteur {
                style: style.into(),
                coin: coin.into(),
                chiffre: "144".into(),
                sous: "1 % bas 98 · moy. 141".into(),
                couleur: 0x00_FF87,
            });
            pomper(900);
            // SAFETY : lecture de la place de la fenêtre du compteur
            let (vu, r, h) = unsafe {
                let h = FindWindowW(w!("NevermindCompteurFps"), None).unwrap_or_default();
                let mut r = RECT::default();
                let _ = GetWindowRect(h, &mut r);
                (IsWindowVisible(h).as_bool(), r, h)
            };
            let plein = composer(&dossier.join(format!("overlay-{style}-{coin}.bmp")), fenetre, h);
            notes.push(format!(
                "{style} {coin} : visible {vu}, place ({}, {}) à ({}, {}), {plein} pixels dessinés",
                r.left, r.top, r.right, r.bottom
            ));
        }
    }
    c.cacher();
    pomper(300);
    // SAFETY : la fenêtre d'essai de ce fil
    unsafe {
        let _ = DestroyWindow(fenetre);
    }
    std::fs::write(dossier.join("overlay.txt"), notes.join("\r\n")).unwrap();
}
