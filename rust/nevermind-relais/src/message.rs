//! Le message de Windows quand Nevermind ne s'ouvre pas (MessageBox, comme relais.cs) : le seul code « unsafe » du
//! relais.

#![allow(unsafe_code)]

use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MessageBoxW};
use windows::core::HSTRING;

/// Montre `texte` sous le titre « Nevermind » : un avertissement, ou une erreur si `grave`.
pub fn montrer(texte: &str, grave: bool) {
    let icone = if grave { MB_ICONERROR } else { MB_ICONWARNING };
    // SAFETY : textes valables pendant l'appel, aucune fenêtre parente
    unsafe {
        let _ = MessageBoxW(None, &HSTRING::from(texte), &HSTRING::from("Nevermind"), MB_OK | icone | MB_SETFOREGROUND);
    }
}
