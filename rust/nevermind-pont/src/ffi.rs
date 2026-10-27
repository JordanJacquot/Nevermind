//! La frontière C de NevermindPont.dll : le seul endroit de la DLL avec du code « unsafe » (des pointeurs reçus de
//! Nevermind), petit et commenté.
//!
//!   nevermind_pont_appel(texte JSON en UTF-8, terminé par un zéro) -> texte JSON en UTF-8 (à rendre par
//!   nevermind_pont_liberer)
//!   nevermind_pont_liberer(texte rendu par nevermind_pont_appel)
//!
//! Toute panique Rust est rattrapée ici (catch_unwind, avec `panic = "unwind"` dans Cargo.toml) et revient comme
//! `{ "ok": false, "erreur": ... }` : sans ce filet, une erreur Rust fermerait Nevermind d'un coup, sans message
//! (essai 4 de l'étape 0).

#![allow(unsafe_code)]

use crate::demandes;
use serde_json::Value;
use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Once;

/// Une panique est rapportée dans la réponse, jamais écrite sur la console de Nevermind
fn sans_message_console() {
    static FAIT: Once = Once::new();
    FAIT.call_once(|| std::panic::set_hook(Box::new(|_| {})));
}

/// La réponse, en texte C à rendre par `nevermind_pont_liberer` (nul seulement si le texte contenait un zéro, ce que
/// serde_json n'écrit jamais : Nevermind traite alors la réponse comme un échec)
fn vers_c(v: &Value) -> *mut c_char {
    CString::new(v.to_string()).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// Le texte d'une panique rattrapée
fn texte_panique(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "inconnue".into())
}

/// Lit le texte reçu de Nevermind.
///
/// # Safety
/// `entree` est nul, ou un texte terminé par un zéro, valable pendant tout l'appel.
unsafe fn lire(entree: *const c_char) -> Result<String, &'static str> {
    if entree.is_null() {
        return Err("demande vide");
    }
    // SAFETY : garanti par l'appelant (voir plus haut).
    let c = unsafe { CStr::from_ptr(entree) };
    c.to_str().map(str::to_owned).map_err(|_| "demande pas en UTF-8")
}

/// Une demande de Nevermind (voir le module `demandes`), sa réponse.
///
/// # Safety
/// `entree` : texte UTF-8 terminé par un zéro (ou nul), valable pendant l'appel. Le résultat (nul ou un texte terminé
/// par un zéro) doit être rendu, une seule fois, par `nevermind_pont_liberer`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nevermind_pont_appel(entree: *const c_char) -> *mut c_char {
    let r = catch_unwind(AssertUnwindSafe(|| {
        sans_message_console();
        // SAFETY : contrat de nevermind_pont_appel (plus haut).
        match unsafe { lire(entree) } {
            Ok(t) => demandes::traiter(&t),
            Err(e) => demandes::erreur(e),
        }
    }));
    let v = match r {
        Ok(v) => v,
        Err(p) => demandes::erreur(&format!("erreur interne de la DLL (panique : {})", texte_panique(p.as_ref()))),
    };
    // (le texte lui-même ne peut pas paniquer ; rattrapé quand même, par prudence)
    catch_unwind(|| vers_c(&v)).unwrap_or(std::ptr::null_mut())
}

/// Rend la mémoire d'une réponse.
///
/// # Safety
/// `texte` : nul, ou rendu par `nevermind_pont_appel`, et pas encore rendu.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nevermind_pont_liberer(texte: *mut c_char) {
    if texte.is_null() {
        return;
    }
    let _ = catch_unwind(|| {
        // SAFETY : ce texte vient de CString::into_raw (contrat de nevermind_pont_liberer).
        drop(unsafe { CString::from_raw(texte) });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un appel complet par l'interface C, comme le fait Nevermind
    fn appel(t: &str) -> Value {
        let c = CString::new(t).unwrap();
        // SAFETY : texte terminé par un zéro, valable pendant l'appel ; la réponse est rendue une fois.
        unsafe {
            let r = nevermind_pont_appel(c.as_ptr());
            assert!(!r.is_null());
            let v = serde_json::from_str(CStr::from_ptr(r).to_str().unwrap()).unwrap();
            nevermind_pont_liberer(r);
            v
        }
    }

    #[test]
    fn interface_c() {
        assert_eq!(appel(r#"{"action":"version"}"#)["ok"], true);
        assert_eq!(appel("Évènement « ok » ✓")["ok"], false);
        // SAFETY : nul est permis.
        unsafe {
            let r = nevermind_pont_appel(std::ptr::null());
            assert!(CStr::from_ptr(r).to_str().unwrap().contains("demande vide"));
            nevermind_pont_liberer(r);
            nevermind_pont_liberer(std::ptr::null_mut());
        }
    }

    #[cfg(feature = "essai-panique")]
    #[test]
    fn panique_rattrapee() {
        let v = appel(r#"{"action":"essai.panique"}"#);
        assert_eq!(v["ok"], false);
        assert!(v["erreur"].as_str().unwrap().contains("panique volontaire"));
        assert_eq!(appel(r#"{"action":"version"}"#)["ok"], true);
    }
}
