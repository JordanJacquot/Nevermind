//! Services de Windows (Set-Service, Stop-Service -Force, Start-Service).

use super::outils::message;
use aide_regles::demandes::Demarrage;
use std::time::{Duration, Instant};
use windows::Win32::System::Services::{
    ChangeServiceConfigW, CloseServiceHandle, ControlService, ENUM_SERVICE_STATUSW, ENUM_SERVICE_TYPE,
    EnumDependentServicesW, OpenSCManagerW, OpenServiceW, QueryServiceStatusEx, SC_HANDLE, SC_MANAGER_CONNECT,
    SC_STATUS_PROCESS_INFO, SERVICE_ACTIVE, SERVICE_AUTO_START, SERVICE_CHANGE_CONFIG, SERVICE_CONTROL_STOP,
    SERVICE_DEMAND_START, SERVICE_DISABLED, SERVICE_ENUMERATE_DEPENDENTS, SERVICE_ERROR, SERVICE_NO_CHANGE,
    SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_START, SERVICE_STATUS, SERVICE_STATUS_CURRENT_STATE,
    SERVICE_STATUS_PROCESS, SERVICE_STOP, SERVICE_STOPPED, StartServiceW,
};
use windows::core::{HSTRING, PCWSTR};

/// Une poignée du gestionnaire de services, refermée toute seule
struct Sc(SC_HANDLE);

impl Drop for Sc {
    fn drop(&mut self) {
        // SAFETY : poignée ouverte par nous
        unsafe {
            let _ = CloseServiceHandle(self.0);
        }
    }
}

fn ouvrir(scm: &Sc, nom: &str, acces: u32) -> Result<Sc, String> {
    // SAFETY : ouverture d'un service par son nom
    unsafe { OpenServiceW(scm.0, &HSTRING::from(nom), acces) }.map(Sc).map_err(|e| {
        if e.code().0 as u32 == 0x8007_0424 {
            format!("Le service {nom} est introuvable sur ce PC.")
        } else {
            format!("Service {nom} : {}", message(&e))
        }
    })
}

fn etat(s: &Sc) -> Option<SERVICE_STATUS_CURRENT_STATE> {
    let mut b = vec![0u8; size_of::<SERVICE_STATUS_PROCESS>()];
    let mut n = 0u32;
    // SAFETY : tampon de la taille de SERVICE_STATUS_PROCESS
    unsafe {
        QueryServiceStatusEx(s.0, SC_STATUS_PROCESS_INFO, Some(&mut b), &mut n).ok()?;
        Some((*(b.as_ptr() as *const SERVICE_STATUS_PROCESS)).dwCurrentState)
    }
}

/// Attend que le service soit dans l'état voulu (60 s au plus)
fn attendre(s: &Sc, voulu: SERVICE_STATUS_CURRENT_STATE) -> bool {
    let fin = Instant::now() + Duration::from_secs(60);
    while Instant::now() < fin {
        if etat(s) == Some(voulu) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

/// Arrête le service, et d'abord ceux qui en dépendent (comme Stop-Service -Force)
fn arreter(scm: &Sc, nom: &str, profondeur: u32) -> Result<(), String> {
    let s = ouvrir(scm, nom, SERVICE_STOP | SERVICE_QUERY_STATUS | SERVICE_ENUMERATE_DEPENDENTS)?;
    if profondeur < 4 {
        for d in dependants(&s) {
            arreter(scm, &d, profondeur + 1)?;
        }
    }
    if etat(&s) == Some(SERVICE_STOPPED) {
        return Ok(());
    }
    let mut st = SERVICE_STATUS::default();
    // SAFETY : demande d'arrêt
    if let Err(e) = unsafe { ControlService(s.0, SERVICE_CONTROL_STOP, &mut st) }
        && e.code().0 as u32 != 0x8007_0426
    {
        return Err(format!("Le service {nom} ne s'arrête pas : {}", message(&e)));
    }
    if !attendre(&s, SERVICE_STOPPED) {
        return Err(format!("Le service {nom} ne s'est pas arrêté à temps."));
    }
    Ok(())
}

/// Les services en cours qui dépendent de celui-ci
fn dependants(s: &Sc) -> Vec<String> {
    let mut r = Vec::new();
    let mut besoin = 0u32;
    let mut n = 0u32;
    // SAFETY : premier appel pour la taille, second avec un tampon aligné de cette taille
    unsafe {
        let _ = EnumDependentServicesW(s.0, SERVICE_ACTIVE, None, 0, &mut besoin, &mut n);
        if besoin == 0 {
            return r;
        }
        let mut b = vec![0u64; (besoin as usize).div_ceil(8)];
        let p = b.as_mut_ptr() as *mut ENUM_SERVICE_STATUSW;
        if EnumDependentServicesW(s.0, SERVICE_ACTIVE, Some(p), besoin, &mut besoin, &mut n).is_err() {
            return r;
        }
        for e in std::slice::from_raw_parts(p, n as usize) {
            if let Ok(x) = e.lpServiceName.to_string() {
                r.push(x);
            }
        }
    }
    r
}

/// Règle le démarrage du service, puis l'arrête ou le lance si demandé
pub fn regler(nom: &str, demarrage: Demarrage, lancer: bool, arreter_aussi: bool) -> Result<(), String> {
    // SAFETY : connexion au gestionnaire de services de ce PC
    let scm =
        Sc(unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }.map_err(|e| message(&e))?);
    let s = ouvrir(&scm, nom, SERVICE_CHANGE_CONFIG | SERVICE_START | SERVICE_QUERY_STATUS)?;
    let type_demarrage = match demarrage {
        Demarrage::Desactive => SERVICE_DISABLED,
        Demarrage::Manuel => SERVICE_DEMAND_START,
        Demarrage::Automatique => SERVICE_AUTO_START,
    };
    // SAFETY : seul le type de démarrage change
    unsafe {
        ChangeServiceConfigW(
            s.0,
            ENUM_SERVICE_TYPE(SERVICE_NO_CHANGE),
            type_demarrage,
            SERVICE_ERROR(SERVICE_NO_CHANGE),
            PCWSTR::null(),
            PCWSTR::null(),
            None,
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
        )
    }
    .map_err(|e| format!("Service {nom} : {}", message(&e)))?;
    if arreter_aussi {
        arreter(&scm, nom, 0)?;
    } else if lancer {
        // SAFETY : démarrage sans argument
        if let Err(e) = unsafe { StartServiceW(s.0, None) }
            && e.code().0 as u32 != 0x8007_0420
        {
            return Err(format!("Le service {nom} ne démarre pas : {}", message(&e)));
        }
        if !attendre(&s, SERVICE_RUNNING) {
            return Err(format!("Le service {nom} n'a pas démarré à temps."));
        }
    }
    Ok(())
}
