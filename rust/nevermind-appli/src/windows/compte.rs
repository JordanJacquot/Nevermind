//! Le nom complet du compte Windows (`([adsi]"WinNT://$env:COMPUTERNAME/$env:USERNAME,user").FullName`,
//! navigation.ps1:19), pour le prénom de la phrase d'accueil. Lecture seule (`NetUserGetInfo`, niveau 10).

use windows::Win32::NetworkManagement::NetManagement::{NetApiBufferFree, NetUserGetInfo, USER_INFO_10};
use windows::core::{HSTRING, PCWSTR};

/// Le nom complet de `utilisateur` sur ce PC, s'il se lit.
pub fn nom_complet(utilisateur: &str) -> Option<String> {
    if utilisateur.is_empty() {
        return None;
    }
    let u = HSTRING::from(utilisateur);
    let mut p: *mut u8 = std::ptr::null_mut();
    // SAFETY : nom valide pendant l'appel ; Windows alloue le résultat, libéré par NetApiBufferFree ; lecture seule
    unsafe {
        if NetUserGetInfo(PCWSTR::null(), &u, 10, &mut p) != 0 || p.is_null() {
            return None;
        }
        let info = &*(p as *const USER_INFO_10);
        let n = if info.usri10_full_name.is_null() { None } else { info.usri10_full_name.to_string().ok() };
        let _ = NetApiBufferFree(Some(p.cast()));
        n
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn lecture_sans_erreur() {
        let u = std::env::var("USERNAME").unwrap_or_default();
        let _ = super::nom_complet(&u);
        assert_eq!(super::nom_complet(""), None);
        assert_eq!(super::nom_complet("NevermindEssaiCompteAbsent"), None);
    }
}
