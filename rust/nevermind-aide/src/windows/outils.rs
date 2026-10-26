//! Petits outils pour parler à Windows : textes en UTF-16, poignées refermées toutes seules, messages d'erreur.

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, HMODULE, LocalFree};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows::core::{HSTRING, PCSTR};

/// Un texte Windows (UTF-16, terminé par un zéro) en texte Rust
pub fn texte(b: &[u16]) -> String {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf16_lossy(&b[..n])
}

/// Une poignée de Windows, refermée quand elle n'est plus utilisée
pub struct Poignee(pub HANDLE);

impl Drop for Poignee {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY : la poignée nous appartient et n'est refermée qu'ici
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

// SAFETY : une poignée de Windows peut servir depuis n'importe quel fil
unsafe impl Send for Poignee {}
unsafe impl Sync for Poignee {}

/// Une mémoire donnée par Windows (LocalAlloc), rendue quand elle n'est plus utilisée
pub struct MemoireLocale(pub *mut core::ffi::c_void);

impl Drop for MemoireLocale {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY : mémoire allouée par Windows avec LocalAlloc, rendue une seule fois
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0)));
            }
        }
    }
}

/// Le message d'une erreur de Windows, en clair (dans la langue de Windows)
pub fn message(e: &windows::core::Error) -> String {
    let m = e.message();
    let m = m.trim();
    if m.is_empty() { format!("erreur 0x{:08x}", e.code().0) } else { m.to_string() }
}

/// Une fonction d'une DLL de System32, cherchée au moment de s'en servir (absente des Windows trop anciens ou des
/// Windows Server : l'Aide démarre quand même, seule la demande qui s'en sert échoue)
pub fn fonction(dll: &str, nom: &[u8]) -> Option<unsafe extern "system" fn() -> isize> {
    // SAFETY : chargement par le dossier System32 seulement ; `nom` est terminé par un zéro
    unsafe {
        let m: HMODULE = LoadLibraryExW(&HSTRING::from(dll), None, LOAD_LIBRARY_SEARCH_SYSTEM32).ok()?;
        debug_assert!(nom.ends_with(&[0]));
        GetProcAddress(m, PCSTR(nom.as_ptr()))
    }
}

/// Un GUID en texte, en minuscules sans accolades (« 0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0 »)
pub fn guid_texte(g: &windows::core::GUID) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7]
    )
}

/// Décalage entre les dates de Windows (FILETIME, depuis 1601) et les ticks de .NET (depuis l'an 1)
pub const DEBUT_FILETIME: i64 = 504_911_232_000_000_000;
