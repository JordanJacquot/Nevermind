//! Le registre de Windows, comme Nevermind le lit (modules\donnees.ps1) : en lecture seule, par l'API de Windows
//! (`Split-RegChemin`, `Get-RegValue`, `Get-RegState`, `Get-CleRegNormale`, `Test-RegUtilisateur`,
//! `Read-OriginesRegistre`). Les valeurs sont rendues comme PowerShell les donne : DWord en entier signé (Int32),
//! QWord en Int64, Binary en liste d'octets, MultiString en liste de textes ; le type (`Kind`) a le nom de .NET.
//!
//! Écriture : seulement les réglages de l'utilisateur (HKCU), que Nevermind écrit lui-même (`Write-RegValue`,
//! `Remove-RegValue`) ; ceux du PC passent par l'Aide (voir `Contexte::ecrire_registre`). En mode test, rien n'est
//! jamais écrit dans le vrai registre (voir le faux registre du `Contexte`).

use serde_json::{Value, json};
use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE, Type, USERS};

/// Une ruche de registre lisible par Nevermind
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ruche {
    Utilisateur,
    Machine,
    Comptes,
}

impl Ruche {
    fn cle(self) -> &'static Key {
        match self {
            Ruche::Utilisateur => CURRENT_USER,
            Ruche::Machine => LOCAL_MACHINE,
            Ruche::Comptes => USERS,
        }
    }
}

/// Retire `debut` au début de `t` sans tenir compte des majuscules (comme `-match` / `-replace` de PowerShell)
fn sans_debut<'a>(t: &'a str, debut: &str) -> Option<&'a str> {
    if t.len() >= debut.len() && t.is_char_boundary(debut.len()) && t[..debut.len()].eq_ignore_ascii_case(debut) {
        Some(&t[debut.len()..])
    } else {
        None
    }
}

/// `Split-RegChemin` : la ruche et la sous-clé d'un chemin PowerShell (HKCU:\..., HKLM:\..., Registry::HKEY_USERS\...),
/// `None` pour les autres.
pub fn decouper(chemin: &str) -> Option<(Ruche, String)> {
    let p = sans_debut(chemin, "Registry::").unwrap_or(chemin);
    let reste = |r: &str| r.strip_prefix('\\').unwrap_or(r).to_string();
    for (debut, ruche) in [
        ("HKCU:", Ruche::Utilisateur),
        ("HKEY_CURRENT_USER", Ruche::Utilisateur),
        ("HKLM:", Ruche::Machine),
        ("HKEY_LOCAL_MACHINE", Ruche::Machine),
    ] {
        if let Some(r) = sans_debut(p, debut) {
            return Some((ruche, reste(r)));
        }
    }
    sans_debut(p, "HKEY_USERS").map(|r| (Ruche::Comptes, reste(r)))
}

/// `Get-CleRegNormale` : HKEY_LOCAL_MACHINE\ ou HKLM\ deviennent « HKLM:\ » (de même HKCU, HKU), « Registry:: » retiré.
pub fn cle_normale(chemin: &str) -> String {
    let t = chemin.trim();
    let c = sans_debut(t, "Registry::").unwrap_or(t);
    for (debuts, nouveau) in [
        (&["HKEY_LOCAL_MACHINE\\", "HKLM:\\", "HKLM\\"][..], "HKLM:\\"),
        (&["HKEY_USERS\\", "HKU:\\", "HKU\\"][..], "HKU:\\"),
        (&["HKEY_CURRENT_USER\\", "HKCU:\\", "HKCU\\"][..], "HKCU:\\"),
    ] {
        for d in debuts {
            if let Some(r) = sans_debut(c, d) {
                return format!("{nouveau}{r}");
            }
        }
    }
    c.to_string()
}

/// `Test-RegUtilisateur` : un réglage de l'utilisateur (HKCU), que Nevermind écrit lui-même
pub fn utilisateur(chemin: &str) -> bool {
    let p = sans_debut(chemin, "Registry::").unwrap_or(chemin);
    sans_debut(p, "HKCU:").is_some() || sans_debut(p, "HKEY_CURRENT_USER").is_some()
}

/// Le nom .NET d'un type de valeur (`GetValueKind().ToString()`)
pub fn nom_type(t: Type) -> &'static str {
    match t {
        Type::U32 => "DWord",
        Type::U64 => "QWord",
        Type::String => "String",
        Type::ExpandString => "ExpandString",
        Type::MultiString => "MultiString",
        Type::Bytes => "Binary",
        Type::Other(0) => "None",
        Type::Other(_) => "Unknown",
    }
}

fn texte_reg(o: &[u8]) -> String {
    let l: Vec<u16> = o.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    let fin = l.iter().position(|&c| c == 0).unwrap_or(l.len());
    String::from_utf16_lossy(&l[..fin])
}

/// Une valeur lue, comme PowerShell la donne (`GetValue`), et son type.
fn valeur_ps(v: &windows_registry::Value, etendre: bool) -> Value {
    let o: &[u8] = v;
    match v.ty() {
        Type::U32 => o.get(..4).map_or(Value::Null, |b| json!(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))),
        Type::U64 => o.get(..8).map_or(Value::Null, |b| json!(i64::from_le_bytes(b.try_into().unwrap_or([0; 8])))),
        Type::String => json!(texte_reg(o)),
        Type::ExpandString => {
            let t = texte_reg(o);
            json!(if etendre { crate::windows::etendre_variables(&t) } else { t })
        }
        Type::MultiString => {
            let l: Vec<u16> = o.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
            // (comme .NET : coupé au premier double zéro ; les textes vides du milieu sont gardés)
            let mut textes: Vec<String> = l.split(|&c| c == 0).map(String::from_utf16_lossy).collect();
            while textes.last().is_some_and(|t| t.is_empty()) {
                textes.pop();
            }
            json!(textes)
        }
        Type::Bytes | Type::Other(_) => json!(o.to_vec()),
    }
}

fn ouvrir(chemin: &str) -> Option<Key> {
    let (ruche, cle) = decouper(chemin)?;
    ruche.cle().open(cle).ok()
}

/// Les noms des valeurs d'une clé (`GetValueNames`), vide si la clé n'existe pas
pub fn noms_valeurs(chemin: &str) -> Vec<String> {
    let Some(k) = ouvrir(chemin) else { return Vec::new() };
    k.values().map(|it| it.map(|(n, _)| n).collect()).unwrap_or_default()
}

/// `Get-RegValue` : la valeur (variables « %...% » remplacées), `None` si elle n'existe pas ou n'est pas lisible
pub fn valeur(chemin: &str, nom: &str) -> Option<Value> {
    let k = ouvrir(chemin)?;
    let v = k.get_value(nom).ok()?;
    Some(valeur_ps(&v, true))
}

/// `Get-RegState` : `{ Existed, Value, Kind }`, la valeur telle qu'écrite (sans remplacer les « %...% »)
pub fn etat(chemin: &str, nom: &str) -> Value {
    let mut s = json!({ "Existed": false, "Value": null, "Kind": null });
    if let Some(k) = ouvrir(chemin)
        && let Ok(v) = k.get_value(nom)
    {
        s["Existed"] = json!(true);
        s["Value"] = valeur_ps(&v, false);
        s["Kind"] = json!(nom_type(v.ty()));
    }
    s
}

/// `Read-OriginesRegistre` : les valeurs d'origine gardées dans le registre (HKLM par l'Aide, HKCU par Nevermind),
/// chacune en JSON. Une valeur illisible est passée.
pub fn origines(machine: &str, utilisateur: &str) -> Vec<Value> {
    let mut r = Vec::new();
    for (ruche, cle) in [(Ruche::Machine, machine), (Ruche::Utilisateur, utilisateur)] {
        let Ok(k) = ruche.cle().open(cle) else { continue };
        let Ok(it) = k.values() else { continue };
        for (n, _) in it {
            let Ok(v) = k.get_value(&n) else { continue };
            let t = match valeur_ps(&v, true) {
                Value::String(s) => s,
                autre => autre.to_string(),
            };
            if let Ok(j) = serde_json::from_str::<Value>(&t) {
                r.push(j);
            }
        }
    }
    r
}

/// Écrit une valeur de l'utilisateur (HKCU), noms spéciaux compris. `kind` : le nom .NET du type.
pub fn ecrire_utilisateur(chemin: &str, nom: &str, valeur: &Value, kind: &str) -> Result<(), String> {
    let (ruche, cle) = decouper(chemin).ok_or_else(|| format!("Chemin de registre non pris en charge: {chemin}"))?;
    if ruche != Ruche::Utilisateur {
        return Err(format!("Chemin de registre non pris en charge: {chemin}"));
    }
    let k = CURRENT_USER.create(cle).map_err(|e| e.message())?;
    let texte = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        autre => autre.to_string(),
    };
    let r = match kind.to_ascii_lowercase().as_str() {
        "dword" => k.set_u32(nom, valeur.as_i64().unwrap_or(0) as u32),
        "qword" => k.set_u64(nom, valeur.as_i64().unwrap_or(0) as u64),
        "expandstring" => k.set_expand_string(nom, texte(valeur)),
        "multistring" => {
            let l: Vec<String> = valeur.as_array().map(|a| a.iter().map(texte).collect()).unwrap_or_default();
            k.set_multi_string(nom, &l.iter().map(String::as_str).collect::<Vec<_>>())
        }
        "binary" => {
            let o: Vec<u8> = valeur
                .as_array()
                .map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0) as u8).collect())
                .unwrap_or_default();
            k.set_bytes(nom, Type::Bytes, &o)
        }
        _ => k.set_string(nom, texte(valeur)),
    };
    r.map_err(|e| e.message())
}

/// Efface une valeur de l'utilisateur (HKCU) ; rien si elle n'existe pas
pub fn effacer_utilisateur(chemin: &str, nom: &str) -> Result<(), String> {
    let (ruche, cle) = decouper(chemin).ok_or_else(|| format!("Chemin de registre non pris en charge: {chemin}"))?;
    if ruche != Ruche::Utilisateur {
        return Err(format!("Chemin de registre non pris en charge: {chemin}"));
    }
    let Ok(k) = CURRENT_USER.options().read().write().open(cle) else { return Ok(()) };
    let _ = k.remove_value(nom);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemins() {
        assert_eq!(decouper("HKCU:\\Software\\X"), Some((Ruche::Utilisateur, "Software\\X".into())));
        assert_eq!(decouper("Registry::HKEY_LOCAL_MACHINE\\SOFTWARE"), Some((Ruche::Machine, "SOFTWARE".into())));
        assert_eq!(decouper("hklm:\\a"), Some((Ruche::Machine, "a".into())));
        assert_eq!(decouper("HKEY_USERS\\S-1"), Some((Ruche::Comptes, "S-1".into())));
        assert_eq!(decouper("HKU:\\S-1"), None);
        assert_eq!(cle_normale(" Registry::HKEY_LOCAL_MACHINE\\A "), "HKLM:\\A");
        assert_eq!(cle_normale("HKLM\\A"), "HKLM:\\A");
        assert_eq!(cle_normale("hkcu:\\A"), "HKCU:\\A");
        assert!(utilisateur("Registry::HKEY_CURRENT_USER\\x"));
        assert!(!utilisateur("HKLM:\\x"));
    }

    #[test]
    fn lecture_reelle_sans_droits() {
        // (lecture seule, permise : une clé que tout Windows a)
        let v = valeur("HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion", "CurrentBuildNumber");
        assert!(v.is_some_and(|v| v.is_string()));
        let e = etat("HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion", "nom-qui-n-existe-pas");
        assert_eq!(e["Existed"], json!(false));
    }
}
