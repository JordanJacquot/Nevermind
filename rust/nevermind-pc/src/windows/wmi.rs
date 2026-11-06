//! WMI (ce que lisent Get-CimInstance, Get-Disk, Get-Partition et Get-PhysicalDisk), par COM : une requête WQL et les
//! champs voulus de chaque objet. COM est préparé pour le fil le temps de la requête (multithread ; un fil déjà
//! préparé autrement est laissé tel quel), et la sécurité de l'appel est réglée sur la connexion seule
//! (`CoSetProxyBlanket`), sans toucher aux réglages du processus.

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, CoUninitialize,
    EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Ole::{SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound, SafeArrayGetVartype};
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Variant::{
    VARIANT, VT_ARRAY, VT_BOOL, VT_BSTR, VT_EMPTY, VT_I2, VT_I4, VT_NULL, VT_UI1, VT_UI2, VT_UI4, VariantClear,
};
use windows::Win32::System::Wmi::{
    IEnumWbemClassObject, IWbemClassObject, IWbemContext, IWbemLocator, IWbemServices, WBEM_FLAG_FORWARD_ONLY,
    WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_GENERIC_FLAG_TYPE, WBEM_INFINITE, WbemLocator,
};
use windows::core::{BSTR, HSTRING, PCWSTR};

type Mesure<T> = Result<T, String>;

/// L'espace de noms des classes de Windows (Win32_...).
pub const CIMV2: &str = r"ROOT\CIMV2";
/// L'espace de noms des disques (MSFT_Disk, MSFT_Partition, MSFT_PhysicalDisk).
pub const STOCKAGE: &str = r"ROOT\Microsoft\Windows\Storage";

/// Une valeur d'un champ WMI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Valeur {
    Rien,
    Texte(String),
    Entier(i64),
    /// Une liste de nombres (Win32_SystemEnclosure.ChassisTypes).
    Liste(Vec<i64>),
}

/// COM prêt pour ce fil tant que la valeur vit.
struct Com {
    a_liberer: bool,
}

impl Com {
    fn preparer() -> Mesure<Com> {
        // SAFETY : préparation de COM pour ce fil ; libérée par Drop si elle a réussi
        let r = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if r.is_ok() {
            Ok(Com { a_liberer: true })
        } else if r == RPC_E_CHANGED_MODE {
            // (ce fil a déjà COM, préparé autrement : on s'en sert tel quel)
            Ok(Com { a_liberer: false })
        } else {
            Err(r.message())
        }
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.a_liberer {
            // SAFETY : CoInitializeEx a réussi sur ce fil
            unsafe { CoUninitialize() };
        }
    }
}

/// La valeur d'un VARIANT rendu par WMI (les nombres de 64 bits arrivent en texte, char16 en entier).
fn valeur(v: &VARIANT) -> Valeur {
    // SAFETY : chaque champ de l'union n'est lu que pour le type indiqué par `vt`
    unsafe {
        let v0 = &v.Anonymous.Anonymous;
        match v0.vt {
            VT_EMPTY | VT_NULL => Valeur::Rien,
            VT_BSTR => Valeur::Texte(v0.Anonymous.bstrVal.to_string()),
            VT_I4 => Valeur::Entier(i64::from(v0.Anonymous.lVal)),
            // (char16 et uint16 arrivent en VT_I2 : lus sans signe)
            VT_I2 => Valeur::Entier(i64::from(v0.Anonymous.iVal as u16)),
            VT_UI1 => Valeur::Entier(i64::from(v0.Anonymous.bVal)),
            VT_BOOL => Valeur::Entier(i64::from(v0.Anonymous.boolVal.as_bool())),
            t if t.0 & VT_ARRAY.0 != 0 => liste(v0.Anonymous.parray),
            _ => Valeur::Rien,
        }
    }
}

/// Une liste de nombres d'un VARIANT (VT_ARRAY) ; une liste d'autre chose : rien.
///
/// # Safety
/// `psa` est nul ou un SAFEARRAY valide (celui d'un VARIANT rendu par WMI, pas encore vidé).
unsafe fn liste(psa: *mut SAFEARRAY) -> Valeur {
    if psa.is_null() {
        return Valeur::Rien;
    }
    // SAFETY : psa est un SAFEARRAY valide (voir plus haut) ; chaque élément est lu dans une variable du bon type
    unsafe {
        let (Ok(genre), Ok(bas), Ok(haut)) =
            (SafeArrayGetVartype(psa), SafeArrayGetLBound(psa, 1), SafeArrayGetUBound(psa, 1))
        else {
            return Valeur::Rien;
        };
        let mut l = Vec::new();
        for i in bas..=haut.min(bas.saturating_add(255)) {
            let n = match genre {
                VT_I4 | VT_UI4 => {
                    let mut x = 0i32;
                    SafeArrayGetElement(psa, &i, (&raw mut x).cast()).map(|_| i64::from(x))
                }
                VT_I2 | VT_UI2 => {
                    let mut x = 0u16;
                    SafeArrayGetElement(psa, &i, (&raw mut x).cast()).map(|_| i64::from(x))
                }
                VT_UI1 => {
                    let mut x = 0u8;
                    SafeArrayGetElement(psa, &i, (&raw mut x).cast()).map(|_| i64::from(x))
                }
                _ => return Valeur::Rien,
            };
            match n {
                Ok(n) => l.push(n),
                Err(_) => return Valeur::Rien,
            }
        }
        Valeur::Liste(l)
    }
}

/// Une requête WQL dans un espace de noms : pour chaque objet, les champs demandés, dans l'ordre.
pub fn requete(espace: &str, wql: &str, champs: &[&str]) -> Mesure<Vec<Vec<Valeur>>> {
    requetes(espace, &[(wql, champs)]).into_iter().next().unwrap_or_else(|| Err("aucune requête".to_string()))
}

/// Plusieurs requêtes WQL dans un même espace de noms, sur une seule connexion (plus rapide) : le résultat de chacune,
/// dans l'ordre. La connexion ratée : toutes ratées.
pub fn requetes(espace: &str, liste: &[(&str, &[&str])]) -> Vec<Mesure<Vec<Vec<Valeur>>>> {
    let tout_rate = |e: String| liste.iter().map(|_| Err(e.clone())).collect();
    let _com = match Com::preparer() {
        Ok(c) => c,
        Err(e) => return tout_rate(e),
    };
    let m = |e: windows::core::Error| e.message();
    // SAFETY : appels COM documentés, sur des interfaces valides rendues par COM
    let services = unsafe {
        (|| -> Mesure<IWbemServices> {
            let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).map_err(m)?;
            let vide = BSTR::new();
            let services = locator
                .ConnectServer(&BSTR::from(espace), &vide, &vide, &vide, 0, &vide, None::<&IWbemContext>)
                .map_err(m)?;
            CoSetProxyBlanket(
                &services,
                RPC_C_AUTHN_WINNT,
                RPC_C_AUTHZ_NONE,
                PCWSTR::null(),
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )
            .map_err(m)?;
            Ok(services)
        })()
    };
    let services = match services {
        Ok(s) => s,
        Err(e) => return tout_rate(e),
    };
    liste.iter().map(|(wql, champs)| executer(&services, wql, champs)).collect()
}

/// Une requête sur une connexion ouverte.
fn executer(services: &IWbemServices, wql: &str, champs: &[&str]) -> Mesure<Vec<Vec<Valeur>>> {
    let m = |e: windows::core::Error| e.message();
    // SAFETY : appels COM documentés, sur des interfaces valides rendues par COM ; chaque VARIANT est vidé par
    // VariantClear après lecture
    unsafe {
        let drapeaux = WBEM_GENERIC_FLAG_TYPE(WBEM_FLAG_FORWARD_ONLY.0 | WBEM_FLAG_RETURN_IMMEDIATELY.0);
        let objets: IEnumWbemClassObject =
            services.ExecQuery(&BSTR::from("WQL"), &BSTR::from(wql), drapeaux, None::<&IWbemContext>).map_err(m)?;
        let noms: Vec<HSTRING> = champs.iter().map(|c| HSTRING::from(*c)).collect();
        let mut l = Vec::new();
        loop {
            let mut lot: [Option<IWbemClassObject>; 1] = [None];
            let mut rendus = 0u32;
            let r = objets.Next(WBEM_INFINITE, &mut lot, &mut rendus);
            if r.is_err() {
                return Err(r.message());
            }
            let Some(o) = lot[0].take().filter(|_| rendus > 0) else { break };
            let mut ligne = Vec::with_capacity(noms.len());
            for n in &noms {
                let mut v = VARIANT::default();
                let lu = o.Get(n, 0, &mut v, None, None);
                ligne.push(if lu.is_ok() { valeur(&v) } else { Valeur::Rien });
                let _ = VariantClear(&mut v);
            }
            l.push(ligne);
        }
        Ok(l)
    }
}
