//! Pare-feu de Windows (New-NetFirewallRule, Remove-NetFirewallRule, Set-NetFirewallProfile), par son interface COM.
//! Le nom d'une règle COM est celui que Windows montre (DisplayName de PowerShell).

use super::com::creer;
use super::outils::message;
use windows::Win32::Foundation::VARIANT_TRUE;
use windows::Win32::NetworkManagement::WindowsFirewall::{
    INetFwPolicy2, INetFwRule, INetFwRules, NET_FW_ACTION_BLOCK, NET_FW_PROFILE2_ALL, NET_FW_PROFILE2_DOMAIN,
    NET_FW_PROFILE2_PRIVATE, NET_FW_PROFILE2_PUBLIC, NET_FW_RULE_DIR_IN, NET_FW_RULE_DIR_OUT, NetFwPolicy2, NetFwRule,
};
use windows::Win32::System::Com::IDispatch;
use windows::Win32::System::Ole::IEnumVARIANT;
use windows::Win32::System::Variant::{VARIANT, VT_DISPATCH, VariantClear};
use windows::core::{BSTR, Interface};

fn regles() -> Result<INetFwRules, String> {
    let p: INetFwPolicy2 = creer(&NetFwPolicy2)?;
    // SAFETY : appel COM ordinaire
    unsafe { p.Rules() }.map_err(|e| message(&e))
}

/// Toutes les règles (objets COM)
fn toutes() -> Result<Vec<INetFwRule>, String> {
    let r = regles()?;
    let mut l = Vec::new();
    // SAFETY : parcours de la collection ; chaque VARIANT est vidé après usage
    unsafe {
        let e: IEnumVARIANT = r._NewEnum().and_then(|u| u.cast()).map_err(|e| message(&e))?;
        loop {
            let mut v = [VARIANT::default()];
            let mut n = 0u32;
            let _ = e.Next(&mut v, &mut n);
            if n == 0 {
                break;
            }
            let a = &v[0].Anonymous.Anonymous;
            if a.vt == VT_DISPATCH {
                let d: &Option<IDispatch> = &a.Anonymous.pdispVal;
                if let Some(regle) = d.as_ref().and_then(|d| d.cast::<INetFwRule>().ok()) {
                    l.push(regle);
                }
            }
            let _ = VariantClear(&mut v[0]);
        }
    }
    Ok(l)
}

fn nom(r: &INetFwRule) -> String {
    // SAFETY : appel COM ordinaire
    unsafe { r.Name() }.map(|b| b.to_string()).unwrap_or_default()
}

/// Les noms de toutes les règles
pub fn noms() -> Result<Vec<String>, String> {
    Ok(toutes()?.iter().map(nom).collect())
}

/// Ajoute une règle qui bloque ce programme (et seulement ce service, si donné), dans un sens, pour tous les profils
pub fn ajouter(nom_regle: &str, programme: &str, service: Option<&str>, entrant: bool) -> Result<(), String> {
    let r: INetFwRule = creer(&NetFwRule)?;
    // SAFETY : appels COM ordinaires
    unsafe {
        r.SetName(&BSTR::from(nom_regle)).map_err(|e| message(&e))?;
        r.SetApplicationName(&BSTR::from(programme)).map_err(|e| message(&e))?;
        if let Some(s) = service {
            r.SetServiceName(&BSTR::from(s)).map_err(|e| message(&e))?;
        }
        r.SetDirection(if entrant { NET_FW_RULE_DIR_IN } else { NET_FW_RULE_DIR_OUT }).map_err(|e| message(&e))?;
        r.SetAction(NET_FW_ACTION_BLOCK).map_err(|e| message(&e))?;
        r.SetProfiles(NET_FW_PROFILE2_ALL.0).map_err(|e| message(&e))?;
        r.SetEnabled(VARIANT_TRUE).map_err(|e| message(&e))?;
        regles()?.Add(&r).map_err(|e| message(&e))
    }
}

/// Retire toutes les règles de ce nom exact
pub fn retirer(nom_regle: &str) -> Result<(), String> {
    let n = toutes()?.iter().filter(|r| nom(r) == nom_regle).count();
    let r = regles()?;
    for _ in 0..n {
        // SAFETY : appel COM ordinaire
        unsafe { r.Remove(&BSTR::from(nom_regle)) }.map_err(|e| message(&e))?;
    }
    Ok(())
}

/// Renomme les règles de ce nom
pub fn renommer(ancien: &str, nouveau: &str) -> Result<(), String> {
    for r in toutes()? {
        if nom(&r) == ancien {
            // SAFETY : appel COM ordinaire
            unsafe { r.SetName(&BSTR::from(nouveau)) }.map_err(|e| message(&e))?;
        }
    }
    Ok(())
}

/// Rallume le pare-feu pour les profils domaine, privé et public
pub fn activer() -> Result<(), String> {
    let p: INetFwPolicy2 = creer(&NetFwPolicy2)?;
    for profil in [NET_FW_PROFILE2_DOMAIN, NET_FW_PROFILE2_PRIVATE, NET_FW_PROFILE2_PUBLIC] {
        // SAFETY : appel COM ordinaire
        unsafe { p.put_FirewallEnabled(profil, VARIANT_TRUE) }.map_err(|e| message(&e))?;
    }
    Ok(())
}
