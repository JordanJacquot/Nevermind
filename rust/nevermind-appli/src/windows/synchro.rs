//! Les verrous d'instance unique (Nevermind.ps1:81-118) et le signal de Mindset.
//!
//! - `Test-InstanceRunning` : `Mutex.OpenExisting` sur chaque nom ; « accès refusé » compte comme ouvert (le verrou
//!   d'une copie lancée en administrateur) ;
//! - `Enter-Instance` : `New-Object Mutex($true, nom, [ref]$nouveau)` sur chaque nom, en attendant 5 s un verrou
//!   déjà tenu (un verrou abandonné est à nous) ; refus de Windows : « refuse » ;
//! - le signal `Local\Mindset-Afficher` (retour au menu de Mindset), levé seulement par l'exe livré.
//!
//! Les tests n'utilisent que des noms `Local\NevermindEssai-<pid>-...`, jamais ceux de Nevermind.

use std::time::Duration;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, GetLastError, WAIT_ABANDONED, WAIT_OBJECT_0,
};
use windows::Win32::System::Threading::{
    CreateMutexW, EVENT_MODIFY_STATE, OpenEventW, OpenMutexW, ReleaseMutex, SYNCHRONIZATION_SYNCHRONIZE, SetEvent,
    WaitForSingleObject,
};
use windows::core::HSTRING;

use super::handle;
use crate::vie::PriseVerrou;

/// Un verrou pris. Rendu et fermé quand il est lâché.
#[derive(Debug)]
pub struct Verrou {
    h: usize,
}

impl Drop for Verrou {
    fn drop(&mut self) {
        // SAFETY : handle rendu par CreateMutexW, pris par ce fil ; rendu puis fermé une seule fois (ici)
        unsafe {
            let _ = ReleaseMutex(handle(self.h));
            let _ = CloseHandle(handle(self.h));
        }
    }
}

/// Le verrou existe (`Mutex.OpenExisting`), ou Windows refuse de l'ouvrir (il existe, à une copie administrateur).
pub fn existe(nom: &str) -> bool {
    let nom = HSTRING::from(nom);
    // SAFETY : nom valide pendant l'appel ; le handle ouvert est fermé tout de suite ; GetLastError lu juste après
    unsafe {
        match OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &nom) {
            Ok(h) => {
                let _ = CloseHandle(h);
                true
            }
            Err(_) => GetLastError() == ERROR_ACCESS_DENIED,
        }
    }
}

/// Crée et prend le verrou, en attendant au plus `attente` s'il est tenu par une autre copie.
pub fn prendre(nom: &str, attente: Duration) -> (PriseVerrou, Option<Verrou>) {
    let nom = HSTRING::from(nom);
    // SAFETY : nom valide pendant l'appel ; le handle est gardé par Verrou (fermé une fois) ou fermé ici
    unsafe {
        let Ok(h) = CreateMutexW(None, true, &nom) else { return (PriseVerrou::Refuse, None) };
        if GetLastError() != ERROR_ALREADY_EXISTS {
            return (PriseVerrou::Pris, Some(Verrou { h: h.0 as usize }));
        }
        let ms = u32::try_from(attente.as_millis()).unwrap_or(u32::MAX - 1);
        let r = WaitForSingleObject(h, ms);
        if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED {
            return (PriseVerrou::Pris, Some(Verrou { h: h.0 as usize }));
        }
        let _ = CloseHandle(h);
        (PriseVerrou::Occupe, None)
    }
}

/// Ouvre le signal d'un autre programme et le lève : `false` s'il n'existe pas ou si Windows refuse.
pub fn lever(nom: &str) -> bool {
    let nom = HSTRING::from(nom);
    // SAFETY : nom valide pendant l'appel ; seul le droit de lever le signal est demandé ; handle fermé tout de suite
    unsafe {
        let Ok(h) = OpenEventW(EVENT_MODIFY_STATE, false, &nom) else { return false };
        let ok = SetEvent(h).is_ok();
        let _ = CloseHandle(h);
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn nom(quoi: &str) -> String {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let nom = format!(r"Local\NevermindEssai-{}-{n}-{quoi}", std::process::id());
        assert!(!nom.starts_with(r"Local\Nevermind-") && !nom.starts_with(r"Local\OptiGame-"));
        nom
    }

    #[test]
    fn pris_puis_tenu_puis_libre() {
        let n = nom("verrou");
        assert!(!existe(&n));
        let (r, v) = prendre(&n, Duration::from_millis(10));
        assert_eq!(r, PriseVerrou::Pris);
        assert!(existe(&n));
        let n2 = n.clone();
        let autre = std::thread::spawn(move || prendre(&n2, Duration::from_millis(50)).0).join().unwrap();
        assert_eq!(autre, PriseVerrou::Occupe, "tenu par une autre copie");
        drop(v);
        assert!(!existe(&n), "rendu et fermé : il n'existe plus");
    }

    #[test]
    fn abandonne_est_a_nous() {
        let n = nom("abandon");
        let n2 = n.clone();
        let fil = std::thread::spawn(move || {
            let (_, v) = prendre(&n2, Duration::ZERO);
            // (le fil s'arrête sans rendre le verrou : abandonné)
            std::mem::forget(v);
        });
        fil.join().unwrap();
        assert_eq!(prendre(&n, Duration::from_secs(2)).0, PriseVerrou::Pris);
    }

    #[test]
    fn signal_absent() {
        assert!(!lever(&nom("signal")));
    }
}
