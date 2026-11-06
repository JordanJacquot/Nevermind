//! Construction d'essai seulement (option cargo `essai`) : l'image de la fenêtre, comme Windows la dessine
//! (`PrintWindow` avec `PW_RENDERFULLCONTENT`, le contenu de WebView2 compris), même sur un bureau Windows à part.
//! L'exe livré ne contient pas ce module.

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDC, GetDIBits, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

use super::hwnd;

/// `PW_RENDERFULLCONTENT` : le contenu composé par DirectComposition (celui de WebView2) compris.
const PW_RENDERFULLCONTENT: u32 = 2;
/// Une image plus grande n'est pas faite (garde).
const COTE_MAX: i32 = 8000;

/// L'image de la fenêtre : largeur, hauteur et pixels RVBA (de haut en bas). `None` si elle ne se fait pas.
pub fn capturer(h: usize) -> Option<(u32, u32, Vec<u8>)> {
    let mut r = RECT::default();
    // SAFETY : lecture seule du cadre d'une fenêtre de ce programme
    unsafe { GetWindowRect(hwnd(h), &mut r) }.ok()?;
    let (l, ht) = (r.right - r.left, r.bottom - r.top);
    if l <= 0 || ht <= 0 || l > COTE_MAX || ht > COTE_MAX {
        return None;
    }
    let mut bgra = vec![0u8; (l * ht * 4) as usize];
    let mut bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: l,
            // (négatif : de haut en bas)
            biHeight: -ht,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY : contexte de l'écran du bureau de ce fil, rendu à la fin ; contexte et image en mémoire créés ici,
    // détruits à la fin ; l'image n'est plus choisie dans le contexte quand ses pixels sont lus (GetDIBits) ; le
    // tampon fait exactement largeur x hauteur x 4 octets, comme le décrit `bi`.
    let lignes = unsafe {
        let ecran = GetDC(None);
        let memoire = CreateCompatibleDC(Some(ecran));
        let image = CreateCompatibleBitmap(ecran, l, ht);
        let avant = SelectObject(memoire, HGDIOBJ::from(image));
        let _ = PrintWindow(hwnd(h), memoire, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT));
        SelectObject(memoire, avant);
        let n = GetDIBits(memoire, image, 0, ht as u32, Some(bgra.as_mut_ptr().cast()), &mut bi, DIB_RGB_COLORS);
        let _ = DeleteObject(HGDIOBJ::from(image));
        let _ = DeleteDC(memoire);
        ReleaseDC(None, ecran);
        n
    };
    if lignes != ht {
        return None;
    }
    // BGRA vers RVBA, opaque
    for p in bgra.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
        p[3] = 255;
    }
    Some((l as u32, ht as u32, bgra))
}
