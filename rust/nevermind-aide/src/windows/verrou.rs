//! Le verrou « une seule Aide à la fois » (lot K, audit du 10/11 ; espace de noms propre à chaque verrou, 12/11).
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
//! Un espace de noms privé ne peut plus être ouvert dès que le processus qui l'a CRÉÉ le referme (ou s'arrête) : la
//! suivante en crée un neuf, vide (doc de `CreatePrivateNamespaceW`). D'où, depuis le 12/11 (run 43151526633) :
//!
//! - chaque verrou a son propre espace (le nom de la frontière et le préfixe portent une empreinte du nom du verrou) :
//!   avant, un seul espace servait à toutes les Aides du PC (autres comptes, copies d'essai), et l'arrêt de celle qui
//!   l'avait créé laissait passer une seconde Aide d'un autre compte ;
//! - une Aide ne garde le verrou que dans un espace qu'elle a créé elle-même. Si elle l'a seulement ouvert, elle le
//!   rend, referme, attend un peu et recommence (moins de 2 s en tout) : le créateur, s'il vit, prend le verrou et elle
//!   s'arrête ; s'il s'est arrêté (il avait vu le verrou pris par elle), elle crée un espace neuf et le garde. Au
//!   dernier essai, elle garde le verrou de l'espace ouvert (aucune Aide serait pire que deux), et le note au journal ;
//! - « pris » veut dire tenu par un autre (attente nulle sur le verrou), plus seulement « il existe » : une Aide qui
//!   vient d'être refusée et n'a pas encore refermé sa poignée ne fait plus s'arrêter les autres ;
//! - l'ancien espace commun (Nevermind 2.3.0 à 3.0) est seulement regardé, jamais créé : si une Aide d'avant tient
//!   le verrou de ce nom, celle-ci s'arrête.
//!
//! `PRIVATE_NAMESPACE_FLAG_DESTROY` n'est jamais utilisé : il détruirait l'espace d'une autre Aide.
//!
//! Sans droits administrateur (essais seulement), l'espace de noms privé ne peut pas être créé : seul le nom public
//! compte, comme avant ; s'il est pris, l'Aide s'arrête, en le disant.

use super::outils::{MemoireLocale, Poignee, message};
use crate::systeme::Verrouillage;
use std::time::Duration;
use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, GetLastError, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES};
use windows::Win32::System::Threading::{
    AddIntegrityLabelToBoundaryDescriptor, AddSIDToBoundaryDescriptor, ClosePrivateNamespace,
    CreateBoundaryDescriptorW, CreateMutexW, CreatePrivateNamespaceW, DeleteBoundaryDescriptor, OpenMutexW,
    OpenPrivateNamespaceW, ReleaseMutex, SYNCHRONIZATION_SYNCHRONIZE, WaitForSingleObject,
};
use windows::core::HSTRING;

/// Le début du nom de la frontière de l'espace de noms privé d'un verrou (suivi de son empreinte)
const FRONTIERE: &str = "Nevermind-Aide";
/// Le début du préfixe des noms dans cet espace (propre à ce processus, suivi de l'empreinte)
const PREFIXE: &str = "NevermindAide";
/// L'ancien espace commun à toutes les Aides (Nevermind 2.3.0 à 3.0) : seulement regardé
pub(super) const ANCIENNE_FRONTIERE: &str = "Nevermind-Aide";
pub(super) const ANCIEN_PREFIXE: &str = "NevermindAide";
/// Les SID exigés pour entrer dans l'espace : le groupe Administrateurs, et le niveau d'intégrité élevé
const ADMINISTRATEURS: &str = "S-1-5-32-544";
const INTEGRITE_ELEVEE: &str = "S-1-16-12288";
/// Les droits de l'espace et du verrou : le système et les administrateurs seulement
const DROITS: &str = "D:P(A;;GA;;;SY)(A;;GA;;;BA)";
/// Les attentes d'une Aide qui a seulement ouvert l'espace, entre deux essais (1,7 s en tout)
const ATTENTES_MS: [u64; 6] = [50, 100, 200, 300, 450, 600];

/// L'empreinte d'un nom de verrou (8 octets du SHA-256, en hexadécimal) : les noms de sa frontière et de son préfixe
fn empreinte(court: &str) -> String {
    aide_regles::sha256::empreinte(court.as_bytes())[..8].iter().map(|o| format!("{o:02x}")).collect()
}

/// L'espace de noms privé du verrou de ce nom (sans `Global\`) : le nom de sa frontière, son préfixe, et le nom
/// complet du verrou protégé
pub(super) fn noms_espace(court: &str) -> (String, String, String) {
    let h = empreinte(court);
    let prefixe = format!("{PREFIXE}-{h}");
    let verrou = format!("{prefixe}\\{court}");
    (format!("{FRONTIERE}-{h}"), prefixe, verrou)
}

/// Les droits « système et administrateurs seulement », prêts pour Windows
struct Droits {
    _sd: MemoireLocale,
    sa: SECURITY_ATTRIBUTES,
}

fn droits() -> Result<Droits, String> {
    let mut sd = PSECURITY_DESCRIPTOR::default();
    // SAFETY : descripteur créé par Windows, libéré par MemoireLocale quand Droits est jeté
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(&HSTRING::from(DROITS), SDDL_REVISION_1, &mut sd, None)
            .map_err(|e| message(&e))?;
    }
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: false.into(),
    };
    Ok(Droits { _sd: MemoireLocale(sd.0), sa })
}

/// Un espace de noms privé, ouvert tant que la valeur vit
pub(super) struct Espace {
    espace: HANDLE,
    frontiere: HANDLE,
}

// SAFETY : des poignées de Windows, utilisables depuis n'importe quel fil
unsafe impl Send for Espace {}

impl Drop for Espace {
    fn drop(&mut self) {
        // SAFETY : nos poignées, refermées une seule fois, ici (jamais avec PRIVATE_NAMESPACE_FLAG_DESTROY)
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

/// L'espace de noms privé de cette frontière et de ce préfixe : créé (si `creer`, et s'il n'existe pas) ou ouvert.
/// `Ok((espace, cree))` : `cree` si ce processus vient de le créer. `Err` : ni créé ni ouvert (sans les droits pour y
/// entrer, ou, sans `creer`, il n'existe pas ou plus).
pub(super) fn espace_prive(nom_frontiere: &str, prefixe: &str, creer: bool) -> Result<(Espace, bool), RefusEspace> {
    let refus = |e: &windows::core::Error, existait| RefusEspace { pourquoi: message(e), existait };
    // SAFETY : frontière et espace à nous, refermés par Espace
    unsafe {
        let frontiere = CreateBoundaryDescriptorW(&HSTRING::from(nom_frontiere), 0);
        if frontiere.is_invalid() {
            return Err(refus(&windows::core::Error::from_thread(), false));
        }
        let mut e = Espace { espace: HANDLE::default(), frontiere };
        let sans = |pourquoi| RefusEspace { pourquoi, existait: false };
        ajouter_sid(&mut e.frontiere, ADMINISTRATEURS, false).map_err(sans)?;
        ajouter_sid(&mut e.frontiere, INTEGRITE_ELEVEE, true).map_err(sans)?;
        let prefixe = HSTRING::from(prefixe);
        let mut creation = None;
        if creer {
            let d = droits().map_err(sans)?;
            e.espace = CreatePrivateNamespaceW(Some(&d.sa), e.frontiere.0, &prefixe);
            if !e.espace.is_invalid() {
                return Ok((e, true));
            }
            // (déjà créé, par une autre Aide : il est rejoint)
            creation = Some(windows::core::Error::from_thread());
        }
        e.espace = OpenPrivateNamespaceW(e.frontiere.0, &prefixe);
        if e.espace.is_invalid() {
            return Err(match creation {
                // (il existait quand la création a été refusée, et vient d'être refermé par son créateur)
                Some(c) => {
                    let existait = c.code() == ERROR_ALREADY_EXISTS.to_hresult();
                    refus(&c, existait)
                }
                None => refus(&windows::core::Error::from_thread(), false),
            });
        }
        Ok((e, false))
    }
}

/// Un espace de noms privé ni créé ni ouvert
pub(super) struct RefusEspace {
    /// Ce que dit Windows
    pourquoi: String,
    /// La création a été refusée parce qu'il existait (ce processus a donc les droits d'y entrer)
    existait: bool,
}

/// Un verrou nommé tenu par ce processus : relâché puis refermé quand il n'est plus utilisé
pub(super) struct Tenu(Poignee);

impl Drop for Tenu {
    fn drop(&mut self) {
        // SAFETY : verrou pris par ce fil (l'Aide le prend et le rend sur son fil principal)
        unsafe {
            let _ = ReleaseMutex(self.0.0);
        }
    }
}

/// Ce que donne la prise d'un verrou nommé
pub(super) enum Creation {
    /// Pris par ce processus
    Tenu(Tenu),
    /// Déjà là (le nom public : il existe ; le verrou protégé : tenu par un autre)
    Existe,
    /// Windows refuse (le nom est pris avec des droits qui nous en écartent, ou autre erreur)
    Refus(String),
}

/// Le nom public : pris s'il n'existait pas (les anciennes Aides et les essais réels ne regardent que son existence)
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

/// Le verrou protégé (dans l'espace de noms privé) : pris s'il n'est tenu par personne, qu'il existe déjà ou non
pub(super) fn prendre(nom: &str) -> Creation {
    let d = match droits() {
        Ok(d) => d,
        Err(x) => return Creation::Refus(x),
    };
    // SAFETY : création ou ouverture d'un verrou nommé, à nous ; attente nulle dessus
    unsafe {
        match CreateMutexW(Some(&d.sa), false, &HSTRING::from(nom)) {
            Ok(h) => {
                let p = Poignee(h);
                let r = WaitForSingleObject(h, 0);
                if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED {
                    // (abandonné : l'Aide qui le tenait est morte sans le rendre ; il est à nous maintenant)
                    Creation::Tenu(Tenu(p))
                } else if r == WAIT_TIMEOUT {
                    Creation::Existe
                } else {
                    Creation::Refus(message(&windows::core::Error::from_thread()))
                }
            }
            Err(e) => Creation::Refus(message(&e)),
        }
    }
}

/// Une Aide d'avant (Nevermind 2.3.0 à 3.0, espace commun) tient-elle le verrou de ce nom ? `Some(détail)` : oui, ou
/// sans doute (Windows refuse de l'ouvrir)
fn ancienne_aide(court: &str) -> Option<String> {
    let (_espace, _) = espace_prive(ANCIENNE_FRONTIERE, ANCIEN_PREFIXE, false).ok()?;
    // SAFETY : ouverture d'un verrou nommé (sans le créer), attente nulle, relâché aussitôt s'il est pris
    unsafe {
        match OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &HSTRING::from(format!("{ANCIEN_PREFIXE}\\{court}"))) {
            Ok(h) => {
                let _p = Poignee(h);
                let r = WaitForSingleObject(h, 0);
                if r == WAIT_TIMEOUT {
                    Some(String::from("tenu"))
                } else {
                    if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED {
                        let _ = ReleaseMutex(h);
                    }
                    None
                }
            }
            Err(e) if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() => None,
            Err(e) => Some(message(&e)),
        }
    }
}

/// Ce que donne le verrou protégé
enum Protege {
    /// Tenu, dans un espace gardé ouvert ; `remarque` : à noter au journal
    Tenu { espace: Espace, verrou: Tenu, remarque: Option<String> },
    /// Pas d'espace de noms privé (sans droits administrateur) : pourquoi
    SansDroits(String),
    /// Une autre Aide le tient : l'Aide s'arrête, pour cette raison
    Occupe(String),
}

/// Le verrou protégé de ce nom : voir le haut du module (gardé seulement dans un espace créé par cette Aide, sauf au
/// dernier essai)
fn verrou_protege(court: &str) -> Protege {
    let (frontiere, prefixe, verrou) = noms_espace(court);
    let mut essai = 0;
    loop {
        let dernier = essai == ATTENTES_MS.len();
        match espace_prive(&frontiere, &prefixe, true) {
            // (sans les droits d'y entrer : le nom public seul)
            Err(r) if !r.existait || dernier => return Protege::SansDroits(r.pourquoi),
            // (il existait, et son créateur vient de le refermer : on réessaie)
            Err(_) => {}
            Ok((espace, cree)) => match prendre(&verrou) {
                Creation::Tenu(t) if cree || dernier => {
                    let remarque = (!cree).then(|| {
                        format!(
                            "Verrou protégé {court} pris dans un espace de noms ouvert, pas créé par cette Aide (son \
                             créateur ne l'a pas pris en {} essais) : l'Aide le garde et continue",
                            essai + 1
                        )
                    });
                    return Protege::Tenu { espace, verrou: t, remarque };
                }
                Creation::Tenu(t) => {
                    // (relâché, puis l'espace refermé : le créateur, s'il vit, le prend pendant l'attente)
                    drop(t);
                    drop(espace);
                }
                Creation::Existe => {
                    return Protege::Occupe(format!(
                        "Une autre Aide tourne déjà pour ce compte (verrou protégé {court} tenu) : celle-ci s'arrête"
                    ));
                }
                Creation::Refus(x) => {
                    return Protege::Occupe(format!(
                        "Verrou protégé {court} impossible à prendre ({x}) : une autre Aide le tient sans doute, \
                         celle-ci s'arrête"
                    ));
                }
            },
        }
        std::thread::sleep(Duration::from_millis(ATTENTES_MS[essai]));
        essai += 1;
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
    // 0. Une Aide d'avant, dans l'ancien espace commun
    if let Some(x) = ancienne_aide(court) {
        return Verrouillage::Occupe(format!(
            "Une Aide d'une version précédente tourne déjà pour ce compte (verrou protégé {court} : {x}) : celle-ci \
             s'arrête"
        ));
    }
    // 1. Le vrai verrou, dans l'espace de noms privé de ce verrou (avec les droits administrateur seulement)
    let (espace, prive, mut remarques, sans_protege) = match verrou_protege(court) {
        Protege::Tenu { espace, verrou, remarque } => (Some(espace), Some(verrou), Vec::from_iter(remarque), None),
        Protege::Occupe(p) => return Verrouillage::Occupe(p),
        Protege::SansDroits(r) => (None, None, Vec::new(), Some(r)),
    };
    // 2. Le nom public : pris s'il est libre ; déjà pris, il ne compte que sans verrou protégé
    let (public, pris_par) = match creer(nom) {
        Creation::Tenu(t) => (Some(t), None),
        Creation::Existe => (None, Some(String::new())),
        Creation::Refus(x) => (None, Some(format!(" ({x})"))),
    };
    match (pris_par, prive.is_some()) {
        (None, _) => {}
        (Some(x), true) => remarques.push(format!(
            "Le nom {nom} est déjà pris par un autre programme{x} : l'Aide garde son verrou protégé et continue"
        )),
        (Some(x), false) => {
            return Verrouillage::Occupe(format!(
                "Le nom {nom} est déjà pris{x}, par une autre Aide ou par un autre programme : l'Aide s'arrête (pas de verrou \
                 protégé : {})",
                sans_protege.unwrap_or_default()
            ));
        }
    }
    let remarque = (!remarques.is_empty()).then(|| remarques.join(" ; "));
    Verrouillage::Pris { garde: Box::new(Garde { _public: public, _prive: prive, _espace: espace }), remarque }
}
