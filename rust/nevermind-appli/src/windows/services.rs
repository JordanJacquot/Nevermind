//! L'état d'un service de Windows (`Get-Service` : `StartType` et `Status`), pour `Set-ServiceOff` : lecture seule.
//! Le changement lui-même passe par l'Aide (`service.regler`).

use windows::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, QUERY_SERVICE_CONFIGW, QueryServiceConfigW, QueryServiceStatus,
    SC_MANAGER_CONNECT, SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS,
};
use windows::core::{HSTRING, PCWSTR};

/// Le démarrage comme `ServiceStartMode` de .NET (« Boot », « System », « Automatic », « Manual », « Disabled ») et
/// s'il tourne ; `None` si le service n'existe pas ou ne se lit pas.
pub fn etat(nom: &str) -> Option<(String, bool)> {
    if nom.is_empty() {
        return None;
    }
    let n = HSTRING::from(nom);
    // SAFETY : lectures seules ; les poignées ouvertes sont fermées avant de sortir ; le tampon de la configuration
    // vit pendant l'appel et fait la taille annoncée
    unsafe {
        let sc = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT).ok()?;
        let s = OpenServiceW(sc, &n, SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS);
        let r = s.as_ref().ok().and_then(|s| {
            let mut besoin = 0u32;
            let _ = QueryServiceConfigW(*s, None, 0, &mut besoin);
            let mut tampon = vec![0u64; (besoin as usize).div_ceil(8).max(8)];
            let p = tampon.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW;
            QueryServiceConfigW(*s, Some(p), (tampon.len() * 8) as u32, &mut besoin).ok()?;
            let debut = match (*p).dwStartType.0 {
                0 => "Boot",
                1 => "System",
                2 => "Automatic",
                3 => "Manual",
                _ => "Disabled",
            };
            let mut st = SERVICE_STATUS::default();
            QueryServiceStatus(*s, &mut st).ok()?;
            Some((debut.to_string(), st.dwCurrentState == SERVICE_RUNNING))
        });
        if let Ok(s) = s {
            let _ = CloseServiceHandle(s);
        }
        let _ = CloseServiceHandle(sc);
        r
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn un_service_qui_existe_partout() {
        let (d, _) = super::etat("EventLog").expect("le journal d'événements existe sur tout Windows");
        assert!(["Automatic", "Manual"].contains(&d.as_str()), "{d}");
        assert_eq!(super::etat("NevermindEssaiServiceAbsent"), None);
    }
}
