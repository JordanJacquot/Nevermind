//! Le seul module de Nevermind (l'appli) qui parle directement à Windows, et le seul où le code « unsafe » est permis
//! (chaque bloc a son commentaire SAFETY). Il remplit les traits de la vie de l'appli ([`systeme::SystemeWindows`]
//! pour `Systeme`, [`fenetre::FenetreTauri`] pour `Fenetre`) avec les vrais appels :
//!
//! - [`synchro`] : les verrous `Local\Nevermind-Instance` et `Local\OptiGame-Instance` (`CreateMutexW`, `OpenMutexW`),
//!   le signal de Mindset (`OpenEventW`, `SetEvent`) ;
//! - [`fenetre`] : accrochée dans Mindset, visible, devant, `Mindset.BarrePropre` (`SetPropW`), taille minimale gardée ;
//! - [`registre`] : `InstallLocation` de Mindset dans HKLM (lecture seule) ;
//! - [`webview2`] : les réglages de WebView2 que Tauri ne transmet pas, la veille de la page ;
//! - [`identite`] : l'identité de la barre des tâches ;
//! - [`reseau`] : câble, Wi-Fi ou rien (`GetAdaptersAddresses`, lecture seule) ;
//! - [`compte`] : le nom complet du compte (`NetUserGetInfo`, lecture seule) ;
//! - [`services`] : l'état d'un service (`QueryServiceConfigW`, lecture seule) ;
//! - [`message`] : les boîtes de message de Windows ;
//! - `capture` (construction d'essai seulement) : l'image de la fenêtre (`PrintWindow`).
//!
//! Rien ici n'écrit dans le registre, ne touche aux fenêtres d'un autre programme ni ne lance quoi que ce soit sans
//! qu'on le lui demande.

#![allow(unsafe_code)]

#[cfg(feature = "essai")]
pub mod capture;
pub mod compte;
pub mod fenetre;
pub mod identite;
pub mod message;
pub mod registre;
pub mod reseau;
pub mod services;
pub mod synchro;
pub mod systeme;
pub mod webview2;

use windows::Win32::Foundation::{HANDLE, HWND};

/// Un handle de Windows gardé comme un nombre.
pub(crate) fn handle(h: usize) -> HANDLE {
    HANDLE(h as *mut core::ffi::c_void)
}

/// Une fenêtre gardée comme un nombre (le `HWND` de Tauri).
pub(crate) fn hwnd(h: usize) -> HWND {
    HWND(h as *mut core::ffi::c_void)
}
