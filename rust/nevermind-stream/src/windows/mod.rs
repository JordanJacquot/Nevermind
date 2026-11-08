//! Le seul module qui parle à Windows (et le seul où le code « unsafe » est permis). Des lectures seulement, sans
//! droits administrateur : la liste des processus et le chemin de leur exe, les cartes graphiques (DXGI), la taille
//! de l'écran principal, deux dossiers connus. Rien n'est lancé, rien n'est écrit.

#![allow(unsafe_code)]

use crate::calcul::{Carte, Fabricant};
use std::path::PathBuf;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1};
use windows::Win32::Graphics::Gdi::{DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Shell::{FOLDERID_ProgramFiles, FOLDERID_RoamingAppData};
use windows::core::{PCWSTR, PWSTR};

/// Une poignée refermée quand elle n'est plus utilisée
struct Poignee(HANDLE);

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

fn texte_large(b: &[u16]) -> String {
    let fin = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf16_lossy(&b[..fin])
}

/// Le chemin de l'exe d'un processus (droit de lecture limité : celui de tout utilisateur sur ses propres processus)
fn chemin(pid: u32) -> Option<String> {
    // SAFETY : simple ouverture en lecture limitée, refermée par Poignee
    let h = Poignee(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?);
    let mut b = vec![0u16; 2048];
    let mut n = b.len() as u32;
    // SAFETY : tampon et taille cohérents
    unsafe { QueryFullProcessImageNameW(h.0, PROCESS_NAME_WIN32, PWSTR(b.as_mut_ptr()), &mut n) }.ok()?;
    Some(String::from_utf16_lossy(&b[..n as usize]))
}

/// Les processus dont le nom est dans `noms` (sans tenir compte des majuscules) : (nom, chemin s'il se lit)
pub fn processus(noms: &[&str]) -> Vec<(String, Option<String>)> {
    let mut r = Vec::new();
    // SAFETY : instantané refermé par Poignee ; la structure est à nous, sa taille posée avant le premier appel
    unsafe {
        let Ok(s) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return r };
        let s = Poignee(s);
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(s.0, &mut e).is_err() {
            return r;
        }
        loop {
            let nom = texte_large(&e.szExeFile);
            if noms.iter().any(|n| n.eq_ignore_ascii_case(&nom)) {
                r.push((nom, chemin(e.th32ProcessID)));
            }
            if Process32NextW(s.0, &mut e).is_err() {
                break;
            }
        }
    }
    r
}

/// Les cartes graphiques (sans les cartes logicielles, comme « Microsoft Basic Render Driver »)
pub fn cartes() -> Vec<Carte> {
    let mut l = Vec::new();
    // SAFETY : fabrique DXGI créée et lâchée ici ; GetDesc1 remplit une structure à nous
    unsafe {
        let Ok(f) = CreateDXGIFactory1::<IDXGIFactory1>() else { return l };
        let mut i = 0;
        while let Ok(a) = f.EnumAdapters1(i) {
            i += 1;
            let Ok(d) = a.GetDesc1() else { continue };
            if (d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
                continue;
            }
            let c = Carte { nom: texte_large(&d.Description), fabricant: Fabricant::depuis_numero(d.VendorId) };
            if !l.contains(&c) {
                l.push(c);
            }
        }
    }
    l
}

/// La taille de l'écran principal, en vrais pixels (sans la mise à l'échelle de Windows)
pub fn ecran() -> Option<(u32, u32)> {
    let mut m = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
    // SAFETY : structure à nous, sa taille posée ; nom nul = l'écran principal
    let ok = unsafe { EnumDisplaySettingsW(PCWSTR::null(), ENUM_CURRENT_SETTINGS, &mut m) }.as_bool();
    (ok && m.dmPelsWidth > 0 && m.dmPelsHeight > 0).then_some((m.dmPelsWidth, m.dmPelsHeight))
}

fn dossier(id: &windows::core::GUID) -> Option<PathBuf> {
    let d = nevermind_noyau::windows::dossier_connu(id);
    (!d.is_empty()).then(|| PathBuf::from(d))
}

/// %APPDATA% (le dossier « Roaming » du compte), lu par Windows et jamais dans les variables d'environnement
pub fn appdata() -> Option<PathBuf> {
    dossier(&FOLDERID_RoamingAppData)
}

/// Program Files (64 bits)
pub fn program_files() -> Option<PathBuf> {
    dossier(&FOLDERID_ProgramFiles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lectures_reelles_sans_droits() {
        // (lecture seule : ce processus de test lui-même, l'écran, les cartes)
        let moi = std::env::current_exe().unwrap();
        let nom = moi.file_name().unwrap().to_string_lossy().to_string();
        let l = processus(&[&nom]);
        assert!(
            l.iter().any(|(_, c)| c.as_deref().is_some_and(|c| c.eq_ignore_ascii_case(&moi.to_string_lossy()))),
            "{l:?}"
        );
        let _ = cartes();
        let _ = ecran();
        assert!(appdata().is_some_and(|p| p.is_dir()));
    }
}
