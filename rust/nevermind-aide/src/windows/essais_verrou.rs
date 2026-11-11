//! Lot K (audit du 10/11) : le verrou « une seule Aide à la fois », sur le vrai Windows. Seulement des noms d'essai
//! (un compte « S-1-5-21-0-0-0-<processus> » qui n'existe pas) : jamais celui de la vraie Aide. Sans droits
//! administrateur (le PC de développement), l'espace de noms privé ne peut pas être créé : l'Aide garde alors le nom
//! public, comme avant. Avec les droits (la machine de GitHub), le verrou protégé est pris.

use super::verrou_aide;
use crate::systeme::Verrouillage;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::HSTRING;

fn nom(essai: &str) -> String {
    format!(r"Global\Nevermind-Aide-S-1-5-21-0-0-0-{}-{essai}", std::process::id())
}

/// Lot E : une seconde Aide (même compte, même copie) s'arrête au verrou ; une fois la première arrêtée, la suivante
/// passe
#[test]
fn une_seconde_aide_s_arrete_au_verrou() {
    let n = nom("seconde");
    let premiere = match verrou_aide(&n) {
        Verrouillage::Pris { garde, remarque } => {
            assert!(remarque.is_none(), "rien ne tenait ce nom : {remarque:?}");
            garde
        }
        Verrouillage::Occupe(p) => panic!("la première Aide est refusée : {p}"),
    };
    assert!(matches!(verrou_aide(&n), Verrouillage::Occupe(_)), "la seconde Aide passe le verrou");
    drop(premiere);
    assert!(matches!(verrou_aide(&n), Verrouillage::Pris { .. }), "après la première, la suivante est refusée");
}

/// Un programme sans droits prend le nom public avant l'Aide. Avec les droits administrateur : l'Aide continue avec
/// son verrou protégé et le note. Sans (secours d'essai) : elle s'arrête, en disant pourquoi. Jamais en silence.
#[test]
fn le_nom_public_pris_avant_l_aide() {
    let n = nom("squat");
    // SAFETY : un verrou nommé d'essai, refermé à la fin
    let squat = unsafe { CreateMutexW(None, false, &HSTRING::from(n.as_str())) }.expect("verrou d'essai");
    match verrou_aide(&n) {
        Verrouillage::Pris { garde, remarque } => {
            let r = remarque.expect("le nom pris n'est pas noté");
            assert!(r.contains("déjà pris par un autre programme"), "{r}");
            // et une seconde Aide s'arrête toujours, au verrou protégé
            assert!(matches!(verrou_aide(&n), Verrouillage::Occupe(_)), "la seconde Aide passe le verrou protégé");
            drop(garde);
        }
        Verrouillage::Occupe(p) => assert!(p.contains("déjà pris"), "l'Aide s'arrête sans dire pourquoi : {p}"),
    }
    // SAFETY : notre poignée
    unsafe {
        let _ = CloseHandle(squat);
    }
}
