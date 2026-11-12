//! Le verrou « une seule Aide à la fois » (lot K, audit du 10/11).
//!
//! Avant, le verrou était seulement le nom public `Global\Nevermind-Aide-<sid>-<suffixe>` : n'importe quel programme,
//! même sans droits, pouvait créer ce nom avant l'Aide, qui s'arrêtait alors sans rien noter (et Nevermind restait
//! sans Aide). Maintenant, avec les droits administrateur (en SYSTEM ou par la demande de Windows) :
//!
//! - le vrai verrou est dans un espace de noms privé (`CreatePrivateNamespaceW`). Sa frontière exige le groupe
//!   Administrateurs ET le niveau d'intégrité élevé : un programme sans droits (ou un administrateur sans la demande de
//!   Windows, au niveau moyen) ne peut ni créer cet espace ni y entrer, donc ni prendre ce verrou avant l'Aide. Ses
//!   droits (système et administrateurs seulement) empêchent aussi de l'ouvrir autrement. Une seconde Aide trouve le
//!   verrou pris et s'arrête, comme avant (lot E) ; l'espace de noms privé est commun à toutes les sessions, comme
//!   `Global\` (l'Aide en SYSTEM est dans la session 0, celle de secours dans la session de l'utilisateur) ;
//! - le nom public est encore pris, quand il est libre : les anciennes Aides (lanceur C#, PowerShell) le regardent, et
//!   l'essai réel vérifie que l'Aide en SYSTEM le tient. S'il est déjà pris par un autre programme, l'Aide continue
//!   avec son verrou protégé, et le note au journal.
//!
//! Sans droits administrateur (essais seulement), l'espace de noms privé ne peut pas être créé : seul le nom public
//! compte, comme avant ; s'il est pris, l'Aide s'arrête, en le disant.

use super::outils::{MemoireLocale, Poignee, message};
use crate::systeme::Verrouillage;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES};
use windows::Win32::System::Threading::{
    AddIntegrityLabelToBoundaryDescriptor, AddSIDToBoundaryDescriptor, ClosePrivateNamespace,
    CreateBoundaryDescriptorW, CreateMutexW, CreatePrivateNamespaceW, DeleteBoundaryDescriptor, OpenPrivateNamespaceW,
    ReleaseMutex,
};
use windows::core::HSTRING;

/// Le nom de la frontière de l'espace de noms privé de l'Aide
const FRONTIERE: &str = "Nevermind-Aide";
/// Le préfixe des noms dans cet espace (propre à ce processus)
const PREFIXE: &str = "NevermindAide";
/// Les SID exigés pour entrer dans l'espace : le groupe Administrateurs, et le niveau d'intégrité élevé
const ADMINISTRATEURS: &str = "S-1-5-32-544";
const INTEGRITE_ELEVEE: &str = "S-1-16-12288";
/// Les droits de l'espace : le système et les administrateurs seulement
const DROITS_ESPACE: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)";

/// L'espace de noms privé de l'Aide, ouvert tant que la valeur vit
struct Espace {
    espace: HANDLE,
    frontiere: HANDLE,
}

// SAFETY : des poignées de Windows, utilisables depuis n'importe quel fil
unsafe impl Send for Espace {}

impl Drop for Espace {
    fn drop(&mut self) {
        // SAFETY : nos poignées, refermées une seule fois, ici
        unsafe {
            if !self.espace.is_invalid() {
                let _ = ClosePrivateNamespace(self.espace, 0);
            }
            if !self.frontiere.is_invalid() {
                DeleteBoundaryDescriptor(self.frontiere);
            }
        }
    }
}

/// Un SID (texte) ajouté à la frontière
fn ajouter_sid(frontiere: &mut HANDLE, sid: &str, integrite: bool) -> Result<(), String> {
    let mut p = PSID::default();
    // SAFETY : SID créé par Windows, libéré par MemoireLocale après usage ; la frontière est à nous
    unsafe {
        ConvertStringSidToSidW(&HSTRING::from(sid), &mut p).map_err(|e| message(&e))?;
        let _m = MemoireLocale(p.0);
        let r = if integrite {
            AddIntegrityLabelToBoundaryDescriptor(frontiere, p)
        } else {
            AddSIDToBoundaryDescriptor(frontiere, p)
        };
        r.map_err(|e| message(&e))
    }
}

/// L'espace de noms privé de l'Aide, créé ou rejoint. `Err` : ce processus n'a pas les droits pour y entrer.
fn espace_prive() -> Result<Espace, String> {
    // SAFETY : frontière et espace à nous, refermés par Espace ; descripteur de sécurité libéré par MemoireLocale
    unsafe {
        let frontiere = CreateBoundaryDescriptorW(&HSTRING::from(FRONTIERE), 0);
        if frontiere.is_invalid() {
            return Err(message(&windows::core::Error::from_thread()));
        }
        let mut e = Espace { espace: HANDLE::default(), frontiere };
        ajouter_sid(&mut e.frontiere, ADMINISTRATEURS, false)?;
        ajouter_sid(&mut e.frontiere, INTEGRITE_ELEVEE, true)?;
        let mut sd = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            &HSTRING::from(DROITS_ESPACE),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )
        .map_err(|e| message(&e))?;
        let _sd = MemoireLocale(sd.0);
        let sa = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: false.into(),
        };
        let prefixe = HSTRING::from(PREFIXE);
        e.espace = CreatePrivateNamespaceW(Some(&sa), e.frontiere.0, &prefixe);
        if e.espace.is_invalid() {
            // (déjà créé, par une autre Aide ou plus tôt par celle-ci : il est rejoint)
            let creation = windows::core::Error::from_thread();
            e.espace = OpenPrivateNamespaceW(e.frontiere.0, &prefixe);
            if e.espace.is_invalid() {
                return Err(message(&creation));
            }
        }
        Ok(e)
    }
}

/// Un verrou nommé tenu par ce processus : relâché puis refermé quand il n'est plus utilisé
struct Tenu(Poignee);

impl Drop for Tenu {
    fn drop(&mut self) {
        // SAFETY : verrou pris par ce fil (l'Aide le prend et le rend sur son fil principal)
        unsafe {
            let _ = ReleaseMutex(self.0.0);
        }
    }
}

/// Ce que donne la création d'un verrou nommé
enum Creation {
    /// Créé et pris par ce processus
    Tenu(Tenu),
    /// Il existait déjà (une autre Aide, ou un autre programme)
    Existe,
    /// Windows refuse (le nom est pris avec des droits qui nous en écartent, ou autre erreur)
    Refus(String),
}

fn creer(nom: &str) -> Creation {
    // SAFETY : création d'un verrou nommé ; GetLastError lu juste après
    unsafe {
        match CreateMutexW(None, true, &HSTRING::from(nom)) {
            Ok(h) => {
                let deja = GetLastError() == ERROR_ALREADY_EXISTS;
                let p = Poignee(h);
                if deja { Creation::Existe } else { Creation::Tenu(Tenu(p)) }
            }
            Err(e) => Creation::Refus(message(&e)),
        }
    }
}

/// Le verrou de l'Aide, gardé tant que la valeur vit (le nom public, le verrou protégé, puis l'espace de noms)
struct Garde {
    _public: Option<Tenu>,
    _prive: Option<Tenu>,
    _espace: Option<Espace>,
}

/// Prend le verrou « une seule Aide à la fois » de ce nom (`Global\Nevermind-Aide-<sid>-<suffixe>`) : voir le haut du
/// module
pub fn verrou_aide(nom: &str) -> Verrouillage {
    let court = nom.strip_prefix("Global\\").unwrap_or(nom);
    // 1. Le vrai verrou, dans l'espace de noms privé (avec les droits administrateur seulement)
    let (espace, prive, sans_protege) = match espace_prive() {
        Ok(e) => match creer(&format!("{PREFIXE}\\{court}")) {
            Creation::Tenu(t) => (Some(e), Some(t), None),
            Creation::Existe => {
                return Verrouillage::Occupe(format!(
                    "Une autre Aide tourne déjà pour ce compte (verrou protégé {court} tenu) : celle-ci s'arrête"
                ));
            }
            Creation::Refus(x) => {
                return Verrouillage::Occupe(format!(
                    "Verrou protégé {court} impossible à prendre ({x}) : une autre Aide le tient sans doute, celle-ci s'arrête"
                ));
            }
        },
        Err(r) => (None, None, Some(r)),
    };
    // 2. Le nom public : pris s'il est libre ; déjà pris, il ne compte que sans verrou protégé
    let (public, pris_par) = match creer(nom) {
        Creation::Tenu(t) => (Some(t), None),
        Creation::Existe => (None, Some(String::new())),
        Creation::Refus(x) => (None, Some(format!(" ({x})"))),
    };
    let remarque = match (pris_par, prive.is_some()) {
        (None, _) => None,
        (Some(x), true) => Some(format!(
            "Le nom {nom} est déjà pris par un autre programme{x} : l'Aide garde son verrou protégé et continue"
        )),
        (Some(x), false) => {
            return Verrouillage::Occupe(format!(
                "Le nom {nom} est déjà pris{x}, par une autre Aide ou par un autre programme : l'Aide s'arrête (pas de verrou \r
                 protégé : {})",
                sans_protege.unwrap_or_default()
            ));
        }
    };
    Verrouillage::Pris { garde: Box::new(Garde { _public: public, _prive: prive, _espace: espace }), remarque }
}
