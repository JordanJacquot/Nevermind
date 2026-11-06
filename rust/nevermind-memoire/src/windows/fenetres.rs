//! Les fenêtres d'un processus (`NmFermeture.Fenetres`, memoire.ps1) : la fermeture polie (WM_CLOSE, comme la
//! croix) et l'état d'une appli qui ne s'est pas fermée (ne répond plus, pose une question, rangée près de l'horloge).

use crate::constat::Cible;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetClassNameW, GetWindow, GetWindowTextLengthW, GetWindowThreadProcessId, IsHungAppWindow,
    IsWindowVisible, PostMessageW, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_CLOSE, WM_NULL,
};
use windows::core::BOOL;

const TECHNIQUES: &[&str] = &[
    "IME",
    "MSCTFIME UI",
    "tooltips_class32",
    "GDI+ Hook Window Class",
    "OleMainThreadWndClass",
    "CicMarshalWndClass",
    "SystemUserAdapterWindowClass",
];

struct Recherche {
    pid: u32,
    visibles: bool,
    trouvees: Vec<HWND>,
}

unsafe extern "system" fn rappel(w: HWND, l: LPARAM) -> BOOL {
    // SAFETY : `l` pointe sur la Recherche de `fenetres`, vivante pendant tout EnumWindows
    let r = unsafe { &mut *(l.0 as *mut Recherche) };
    let mut p = 0u32;
    // SAFETY : lectures sur une fenêtre donnée par Windows
    unsafe {
        GetWindowThreadProcessId(w, Some(&mut p));
        if p != r.pid {
            return BOOL(1);
        }
        let vue = IsWindowVisible(w).as_bool();
        if r.visibles {
            if vue {
                r.trouvees.push(w);
            }
            return BOOL(1);
        }
        // (une boîte d'une fenêtre à l'écran part avec elle ; un propriétaire caché ne compte pas)
        if let Ok(proprio) = GetWindow(w, GW_OWNER)
            && !proprio.is_invalid()
            && IsWindowVisible(proprio).as_bool()
        {
            return BOOL(1);
        }
        if vue {
            r.trouvees.push(w);
            return BOOL(1);
        }
        if GetWindowTextLengthW(w) == 0 {
            return BOOL(1);
        }
        let mut b = [0u16; 256];
        let n = GetClassNameW(w, &mut b);
        let cl = String::from_utf16_lossy(&b[..n.max(0) as usize]);
        if TECHNIQUES.contains(&cl.as_str()) || cl.starts_with(".NET-BroadcastEventWindow") {
            return BOOL(1);
        }
        r.trouvees.push(w);
    }
    BOOL(1)
}

/// Les fenêtres de premier niveau d'un processus. `visibles` : seulement celles à l'écran ; sinon celles à fermer.
fn fenetres(pid: i64, visibles: bool) -> Vec<HWND> {
    let Ok(pid) = u32::try_from(pid) else { return Vec::new() };
    let mut r = Recherche { pid, visibles, trouvees: Vec::new() };
    // SAFETY : le rappel ne vit que pendant l'appel, `r` aussi
    unsafe {
        let _ = EnumWindows(Some(rappel), LPARAM(&mut r as *mut Recherche as isize));
    }
    r.trouvees
}

/// WM_CLOSE à ses fenêtres (après `verifier`) : le nombre de fenêtres prévenues
pub fn fermer_poliment(pid: i64) -> i32 {
    let mut n = 0;
    for w in fenetres(pid, false) {
        // SAFETY : un message posté à une fenêtre de ce processus
        if unsafe { PostMessageW(Some(w), WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok() {
            n += 1;
        }
    }
    n
}

fn bloque(pid: i64) -> bool {
    // SAFETY : lecture sur des fenêtres données par Windows
    fenetres(pid, true).into_iter().any(|w| unsafe { IsHungAppWindow(w) }.as_bool())
}

/// `Process.Responding` : sa première fenêtre à l'écran répond-elle en 5 s ?
fn repond(pid: i64) -> bool {
    let Some(w) = fenetres(pid, true).into_iter().next() else { return true };
    let mut x = 0usize;
    // SAFETY : un message vide, attendu au plus 5 s, abandonné si la fenêtre est bloquée
    unsafe { SendMessageTimeoutW(w, WM_NULL, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 5000, Some(&mut x)) }.0 != 0
}

/// `Get-EtatFermeture` : « bloquee », « question » ou « rangee »
pub fn etat(restants: &[Cible]) -> &'static str {
    let mut visibles = 0;
    let mut bloquee = false;
    for c in restants {
        visibles += fenetres(c.pid, true).len();
        if bloque(c.pid) || (!fenetres(c.pid, true).is_empty() && !repond(c.pid)) {
            bloquee = true;
        }
    }
    if bloquee {
        "bloquee"
    } else if visibles > 0 {
        "question"
    } else {
        "rangee"
    }
}
