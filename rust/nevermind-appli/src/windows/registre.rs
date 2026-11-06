//! La lecture du registre (HKLM, lecture seule) : `InstallLocation` de Mindset (mindset.ps1:79), que seul un
//! administrateur peut écrire. Jamais d'écriture.

use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::core::HSTRING;

/// La clé de désinstallation de Mindset (mindset.ps1:79).
pub const CLE_MINDSET: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Mindset";
/// La valeur lue (mindset.ps1:79).
pub const VALEUR_EMPLACEMENT: &str = "InstallLocation";

/// Une valeur texte (REG_SZ, ou REG_EXPAND_SZ développée) de HKLM, `None` si elle n'existe pas, n'est pas du texte
/// ou ne se lit pas. Au plus 32 Ko.
pub fn lire_hklm(cle: &str, valeur: &str) -> Option<String> {
    let cle = HSTRING::from(cle);
    let valeur = HSTRING::from(valeur);
    let mut taille: u32 = 0;
    // SAFETY : noms valides pendant l'appel ; première lecture sans tampon, pour la taille en octets
    let r = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, &cle, &valeur, RRF_RT_REG_SZ, None, None, Some(&mut taille)) };
    if r != ERROR_SUCCESS || taille == 0 || taille > 32 * 1024 {
        return None;
    }
    let mut tampon = vec![0u16; (taille as usize).div_ceil(2) + 1];
    let mut taille = (tampon.len() * 2) as u32;
    // SAFETY : tampon de `taille` octets, qui vit pendant l'appel ; Windows y écrit au plus `taille` octets
    let r = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &cle,
            &valeur,
            RRF_RT_REG_SZ,
            None,
            Some(tampon.as_mut_ptr().cast()),
            Some(&mut taille),
        )
    };
    if r != ERROR_SUCCESS {
        return None;
    }
    let n = (taille as usize / 2).min(tampon.len());
    let texte = &tampon[..n];
    let fin = texte.iter().position(|c| *c == 0).unwrap_or(texte.len());
    Some(String::from_utf16_lossy(&texte[..fin]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une valeur qui existe sur tout Windows (lecture seule) ; une clé ou une valeur absente : rien.
    #[test]
    fn lecture_hklm() {
        let v = lire_hklm(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuild").expect("CurrentBuild");
        assert!(!v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()), "{v:?}");
        assert_eq!(lire_hklm(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "NevermindEssaiAbsente"), None);
        assert_eq!(lire_hklm(r"SOFTWARE\NevermindEssaiCleAbsente", "InstallLocation"), None);
        // (une valeur qui n'est pas du texte n'est pas rendue : CurrentMajorVersionNumber est un DWORD)
        assert_eq!(lire_hklm(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentMajorVersionNumber"), None);
    }
}
