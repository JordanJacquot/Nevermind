//! COM et WMI : ce que PowerShell lisait avec Get-CimInstance et les modules de Windows (Defender, Storage, SMB...).

use super::outils::message;
use serde_json::{Map, Value};
use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoInitializeSecurity,
    CoSetProxyBlanket, CoUninitialize, EOAC_DYNAMIC_CLOAKING, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_AUTHN_LEVEL_PKT_PRIVACY, RPC_C_IMP_LEVEL_IMPERSONATE, SAFEARRAY,
};
use windows::Win32::System::Ole::{
    SafeArrayCreateVector, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound, SafeArrayPutElement,
};
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Variant::{
    VARENUM, VARIANT, VT_ARRAY, VT_BOOL, VT_BSTR, VT_EMPTY, VT_I1, VT_I2, VT_I4, VT_I8, VT_INT, VT_NULL, VT_R4, VT_R8,
    VT_UI1, VT_UI2, VT_UI4, VT_UI8, VT_UINT, VariantClear,
};
use windows::Win32::System::Wmi::{
    IEnumWbemClassObject, IWbemClassObject, IWbemLocator, IWbemServices, WBEM_FLAG_FORWARD_ONLY,
    WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_FLAG_RETURN_WBEM_COMPLETE, WbemLocator,
};
use windows::core::{BSTR, HSTRING, PCWSTR};

/// COM prêt sur ce fil tant que la valeur vit
pub struct Com(bool);

impl Com {
    pub fn pret() -> Com {
        // SAFETY : initialisation de COM pour ce fil, défaite par Drop si elle a réussi
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Com(hr.is_ok())
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY : CoInitializeEx a réussi sur ce fil
            unsafe { CoUninitialize() };
        }
    }
}

/// La sécurité de COM pour tout le processus, une fois au démarrage (avant tout appel à COM). Le fil principal garde
/// COM prêt jusqu'à la fin (`Com` jamais relâché).
pub fn securite_du_processus() -> Com {
    let c = Com::pret();
    // SAFETY : appelé une seule fois, au démarrage, avant tout autre appel à COM
    unsafe {
        let _ = CoInitializeSecurity(
            None,
            -1,
            None,
            None,
            RPC_C_AUTHN_LEVEL_PKT_PRIVACY,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_DYNAMIC_CLOAKING,
            None,
        );
    }
    c
}

/// Une création d'objet COM (dans ce processus seulement)
pub fn creer<T: windows::core::Interface>(clsid: &windows::core::GUID) -> Result<T, String> {
    // SAFETY : création d'un objet COM du système
    unsafe { CoCreateInstance(clsid, None, CLSCTX_INPROC_SERVER) }.map_err(|e| message(&e))
}

/// Connexion à un espace de noms WMI de ce PC
fn wmi(espace: &str) -> Result<IWbemServices, String> {
    let loc: IWbemLocator = creer(&WbemLocator)?;
    let vide = BSTR::new();
    // SAFETY : appels COM ordinaires
    unsafe {
        let s = loc.ConnectServer(&BSTR::from(espace), &vide, &vide, &vide, 0, &vide, None).map_err(|e| message(&e))?;
        CoSetProxyBlanket(
            &s,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            PCWSTR::null(),
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )
        .map_err(|e| message(&e))?;
        Ok(s)
    }
}

/// Une valeur WMI (VARIANT) en JSON. Les entiers 64 bits arrivent en texte (règle de WMI).
///
/// # Safety
/// `v` doit être un VARIANT valable.
pub unsafe fn valeur(v: &VARIANT) -> Value {
    // SAFETY : on lit le champ qui correspond au type annoncé
    unsafe {
        let vt = v.Anonymous.Anonymous.vt;
        let u = &v.Anonymous.Anonymous.Anonymous;
        if vt.0 & VT_ARRAY.0 != 0 {
            return tableau(u.parray, VARENUM(vt.0 & !VT_ARRAY.0));
        }
        match vt {
            VT_EMPTY | VT_NULL => Value::Null,
            VT_BSTR => Value::String(u.bstrVal.to_string()),
            VT_BOOL => Value::Bool(u.boolVal.0 != 0),
            VT_I1 => Value::from(u.cVal),
            VT_UI1 => Value::from(u.bVal),
            VT_I2 => Value::from(u.iVal),
            VT_UI2 => Value::from(u.uiVal),
            VT_I4 | VT_INT => Value::from(u.lVal),
            VT_UI4 | VT_UINT => Value::from(u.ulVal),
            VT_I8 => Value::from(u.llVal),
            VT_UI8 => Value::from(u.ullVal),
            VT_R4 => Value::from(f64::from(u.fltVal)),
            VT_R8 => Value::from(u.dblVal),
            _ => Value::Null,
        }
    }
}

/// Un tableau WMI (textes ou entiers)
unsafe fn tableau(sa: *mut SAFEARRAY, vt: VARENUM) -> Value {
    if sa.is_null() {
        return Value::Null;
    }
    let mut r = Vec::new();
    // SAFETY : tableau à une dimension, lu élément par élément dans une variable du bon type
    unsafe {
        let (Ok(bas), Ok(haut)) = (SafeArrayGetLBound(sa, 1), SafeArrayGetUBound(sa, 1)) else { return Value::Null };
        for i in bas..=haut {
            let v = match vt {
                VT_BSTR => {
                    let mut b = BSTR::new();
                    if SafeArrayGetElement(sa, &i, (&mut b as *mut BSTR).cast()).is_err() {
                        continue;
                    }
                    Value::String(b.to_string())
                }
                VT_I4 | VT_INT | VT_UI4 | VT_UINT => {
                    let mut n = 0i32;
                    if SafeArrayGetElement(sa, &i, (&mut n as *mut i32).cast()).is_err() {
                        continue;
                    }
                    Value::from(n)
                }
                VT_UI1 => {
                    let mut n = 0u8;
                    if SafeArrayGetElement(sa, &i, (&mut n as *mut u8).cast()).is_err() {
                        continue;
                    }
                    Value::from(n)
                }
                _ => Value::Null,
            };
            r.push(v);
        }
    }
    Value::Array(r)
}

/// Les propriétés `noms` d'un objet WMI
fn proprietes(o: &IWbemClassObject, noms: &[&str]) -> Map<String, Value> {
    let mut m = Map::new();
    for n in noms {
        let mut v = VARIANT::default();
        // SAFETY : lecture d'une propriété dans un VARIANT, vidé ensuite
        let x = unsafe {
            if o.Get(&HSTRING::from(*n), 0, &mut v, None, None).is_ok() {
                let x = valeur(&v);
                let _ = VariantClear(&mut v);
                x
            } else {
                Value::Null
            }
        };
        m.insert((*n).to_string(), x);
    }
    m
}

/// Une requête WMI : les propriétés `noms` de chaque objet trouvé. `delai_s` : temps maximal pour chaque objet.
pub fn requete(espace: &str, wql: &str, noms: &[&str], delai_s: i32) -> Result<Vec<Map<String, Value>>, String> {
    let s = wmi(espace)?;
    // SAFETY : appels COM ordinaires
    let e: IEnumWbemClassObject = unsafe {
        s.ExecQuery(&BSTR::from("WQL"), &BSTR::from(wql), WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY, None)
    }
    .map_err(|e| message(&e))?;
    let mut r = Vec::new();
    loop {
        let mut objets = [None];
        let mut n = 0u32;
        // SAFETY : un objet à la fois
        let hr = unsafe { e.Next(delai_s.saturating_mul(1000), &mut objets, &mut n) };
        if n == 0 {
            if hr.is_err() {
                return Err(message(&windows::core::Error::from(hr)));
            }
            if hr.0 == 0x40004 {
                return Err("WMI ne répond pas à temps".into()); // WBEM_S_TIMEDOUT
            }
            break;
        }
        if let Some(o) = objets[0].take() {
            r.push(proprietes(&o, noms));
        }
    }
    Ok(r)
}

/// Un VARIANT contenant une liste de textes
fn variant_textes(l: &[&str]) -> VARIANT {
    let mut v = VARIANT::default();
    // SAFETY : tableau créé par Windows, rempli élément par élément (copiés), confié au VARIANT qui le libère
    unsafe {
        let sa = SafeArrayCreateVector(VT_BSTR, 0, l.len() as u32);
        for (i, x) in l.iter().enumerate() {
            let b = BSTR::from(*x);
            let _ = SafeArrayPutElement(sa, &(i as i32), b.as_ptr().cast());
        }
        let a = &mut *v.Anonymous.Anonymous;
        a.vt = VARENUM(VT_ARRAY.0 | VT_BSTR.0);
        a.Anonymous.parray = sa;
    }
    v
}

/// Un VARIANT oui/non
fn variant_bool(b: bool) -> VARIANT {
    let mut v = VARIANT::default();
    // SAFETY : écriture du type et de la valeur correspondante
    unsafe {
        let a = &mut *v.Anonymous.Anonymous;
        a.vt = VT_BOOL;
        a.Anonymous.boolVal = VARIANT_BOOL(if b { -1 } else { 0 });
    }
    v
}

/// Un paramètre d'une méthode WMI
pub enum Parametre<'a> {
    Textes(&'a [&'a str]),
    Oui(bool),
}

/// Appelle la méthode statique `methode` de la classe WMI `classe`
pub fn methode(espace: &str, classe: &str, methode: &str, params: &[(&str, Parametre)]) -> Result<(), String> {
    let s = wmi(espace)?;
    // SAFETY : appels COM ordinaires ; chaque VARIANT est vidé après usage
    unsafe {
        let mut c = None;
        s.GetObject(&BSTR::from(classe), WBEM_FLAG_RETURN_WBEM_COMPLETE, None, Some(&mut c), None)
            .map_err(|e| message(&e))?;
        let c: IWbemClassObject = c.ok_or("classe WMI introuvable")?;
        let mut entree = None;
        c.GetMethod(&HSTRING::from(methode), 0, &mut entree, std::ptr::null_mut()).map_err(|e| message(&e))?;
        let entree: IWbemClassObject = entree.ok_or("méthode WMI introuvable")?;
        let inst = entree.SpawnInstance(0).map_err(|e| message(&e))?;
        for (nom, p) in params {
            let mut v = match p {
                Parametre::Textes(l) => variant_textes(l),
                Parametre::Oui(b) => variant_bool(*b),
            };
            let r = inst.Put(&HSTRING::from(*nom), 0, &v, 0);
            let _ = VariantClear(&mut v);
            r.map_err(|e| message(&e))?;
        }
        let mut sortie = None;
        s.ExecMethod(
            &BSTR::from(classe),
            &BSTR::from(methode),
            WBEM_FLAG_RETURN_WBEM_COMPLETE,
            None,
            &inst,
            Some(&mut sortie),
            None,
        )
        .map_err(|e| message(&e))?;
        if let Some(o) = sortie {
            let m = proprietes(&o, &["ReturnValue"]);
            if let Some(n) = m.get("ReturnValue").and_then(Value::as_i64)
                && n != 0
            {
                return Err(format!("WMI : {classe}.{methode} a répondu {n}"));
            }
        }
        Ok(())
    }
}
