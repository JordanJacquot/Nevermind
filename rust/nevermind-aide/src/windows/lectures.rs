//! Lectures réservées aux administrateurs, par WMI (Get-CimInstance, Get-MpPreference, Get-MpThreat,
//! Get-PhysicalDisk, Get-StorageReliabilityCounter...) : seulement des textes et des nombres en retour.

use super::com::{Parametre, methode, requete};
use super::outils::fonction;
use super::processus::privilege;
use super::temps;
use aide_regles::demandes::ContexteProgrammes;
use aide_regles::dossiers::Dossiers;
use serde_json::{Map, Value, json};
use windows::Win32::System::WindowsProgramming::GetFirmwareEnvironmentVariableW;
use windows::core::HSTRING;

const DEFENDER: &str = "root\\Microsoft\\Windows\\Defender";

/// Santé d'un disque : les champs dont Nevermind se sert
const CHAMPS_DISQUE: [&str; 10] = [
    "Temperature",
    "TemperatureMax",
    "Wear",
    "PowerOnHours",
    "StartStopCycleCount",
    "ReadErrorsCorrected",
    "ReadErrorsUncorrected",
    "WriteErrorsUncorrected",
    "ReadLatencyMax",
    "WriteLatencyMax",
];

/// Un nombre WMI en nombre décimal (les entiers 64 bits arrivent en texte)
fn nombre(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Secure Boot (comme Confirm-SecureBootUEFI) : `None` si le PC n'est pas en UEFI ou si la variable est illisible
fn secure_boot() -> Option<bool> {
    privilege("SeSystemEnvironmentPrivilege");
    let mut b = [0u8; 1];
    // SAFETY : tampon d'un octet, taille donnée
    let n = unsafe {
        GetFirmwareEnvironmentVariableW(
            &HSTRING::from("SecureBoot"),
            &HSTRING::from("{8be4df61-93ca-11d2-aa0d-00e098032b8c}"),
            Some(b.as_mut_ptr().cast()),
            1,
        )
    };
    (n == 1).then_some(b[0] == 1)
}

/// Secure Boot, puce TPM, santé de chaque disque (températures, usure, erreurs)
pub fn pc() -> Value {
    let mut r = json!({ "SecureBoot": secure_boot(), "TpmOk": false, "Tpm": null, "Fiabilite": {} });
    if let Ok(t) =
        requete("root\\cimv2\\security\\microsofttpm", "SELECT SpecVersion FROM Win32_Tpm", &["SpecVersion"], 3)
    {
        r["TpmOk"] = json!(true);
        if let Some(premier) = t.first() {
            let v = match &premier["SpecVersion"] {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                autre => autre.to_string(),
            };
            r["Tpm"] = json!({ "SpecVersion": v });
        }
    }
    let espace = "root\\Microsoft\\Windows\\Storage";
    let mut fiabilite = Map::new();
    if let Ok(disques) = requete(espace, "SELECT * FROM MSFT_PhysicalDisk", &["DeviceId", "__RELPATH"], 30) {
        for d in disques {
            let (Some(id), Some(chemin)) = (d["DeviceId"].as_str(), d["__RELPATH"].as_str()) else { continue };
            let wql = format!("ASSOCIATORS OF {{{chemin}}} WHERE ResultClass = MSFT_StorageReliabilityCounter");
            let Ok(c) = requete(espace, &wql, &CHAMPS_DISQUE, 30) else { continue };
            let Some(c) = c.into_iter().next() else { continue };
            let mut champs = Map::new();
            for n in CHAMPS_DISQUE {
                if let Some(x) = nombre(&c[n]) {
                    champs.insert(n.to_string(), json!(x));
                }
            }
            fiabilite.insert(id.to_string(), Value::Object(champs));
        }
    }
    r["Fiabilite"] = Value::Object(fiabilite);
    r
}

/// Une date WMI (« 20261026120000.000000+060 ») en ticks .NET, heure locale (comme le [datetime] de PowerShell)
fn date_wmi(t: &str) -> Option<i64> {
    let n = |a: usize, b: usize| t.get(a..b)?.parse::<i64>().ok();
    let (an, mois, jour, h, m, s) = (n(0, 4)?, n(4, 6)?, n(6, 8)?, n(8, 10)?, n(10, 12)?, n(12, 14)?);
    let micro = n(15, 21).unwrap_or(0);
    let decalage = n(22, 25).unwrap_or(0) * if t.get(21..22) == Some("-") { -1 } else { 1 };
    let jours_avant = |a: i64| {
        let a = a - 1;
        a * 365 + a / 4 - a / 100 + a / 400
    };
    let bissextile = an % 4 == 0 && (an % 100 != 0 || an % 400 == 0);
    let cumul = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let mut j = jours_avant(an) + *cumul.get(usize::try_from(mois - 1).ok()?)? + jour - 1;
    if bissextile && mois > 2 {
        j += 1;
    }
    let local_annonce = (((j * 24 + h) * 60 + m) * 60 + s) * 10_000_000 + micro * 10;
    let utc = local_annonce - decalage * 600_000_000;
    Some(temps::ticks_local(utc))
}

/// Antivirus de Windows : nombre d'exclusions (jamais leurs chemins), menaces connues et dernières détections
pub fn securite() -> Value {
    // (DetectionsLues : vrai seulement si Defender a bien rendu ses détections ; sinon une liste vide ne veut pas dire
    // « aucune menace ». ExclusionsNombre -1 : pas lues)
    let mut exclusions = -1i64;
    if let Ok(p) = requete(
        DEFENDER,
        "SELECT * FROM MSFT_MpPreference",
        &["ExclusionPath", "ExclusionProcess", "ExclusionExtension"],
        30,
    ) && let Some(p) = p.first()
    {
        exclusions = ["ExclusionPath", "ExclusionProcess", "ExclusionExtension"]
            .iter()
            .flat_map(|k| p[*k].as_array().cloned().unwrap_or_default())
            .filter(|x| x.as_str().is_some_and(|s| !s.is_empty() && !aide_regles::texte::commence(s, "N/A")))
            .count() as i64;
    }
    let texte = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        autre => autre.to_string(),
    };
    let entier = |v: &Value| nombre(v).unwrap_or(0.0) as i64;
    let menaces: Vec<Value> =
        requete(DEFENDER, "SELECT * FROM MSFT_MpThreat", &["ThreatID", "ThreatName", "SeverityID", "IsActive"], 30)
            .unwrap_or_default()
            .iter()
            .map(|m| {
                json!({ "ThreatID": texte(&m["ThreatID"]), "ThreatName": texte(&m["ThreatName"]),
                    "SeverityID": entier(&m["SeverityID"]), "IsActive": m["IsActive"].as_bool().unwrap_or(false) })
            })
            .collect();
    let mut detections = Vec::new();
    let mut lues = false;
    let champs = ["ThreatID", "ThreatStatusID", "InitialDetectionTime", "Resources"];
    if let Ok(mut d) = requete(DEFENDER, "SELECT * FROM MSFT_MpThreatDetection", &champs, 30) {
        let quand = |x: &Map<String, Value>| x["InitialDetectionTime"].as_str().and_then(date_wmi).unwrap_or(0);
        d.sort_by_key(|x| std::cmp::Reverse(quand(x)));
        for x in d.iter().take(15) {
            let ressources: Vec<String> =
                x["Resources"].as_array().map(|a| a.iter().map(texte).collect()).unwrap_or_default();
            detections.push(json!({ "ThreatID": texte(&x["ThreatID"]), "ThreatStatusID": entier(&x["ThreatStatusID"]),
                                    "Quand": quand(x), "Resources": ressources }));
        }
        lues = true;
    }
    json!({ "ExclusionsNombre": exclusions, "Menaces": menaces, "Detections": detections, "DetectionsLues": lues })
}

/// Retire un élément des exclusions de l'antivirus de Windows : chemin, programme et extension (erreurs ignorées)
pub fn retirer_exclusion(x: &str) {
    for champ in ["ExclusionPath", "ExclusionProcess", "ExclusionExtension"] {
        let _ = methode(DEFENDER, "MSFT_MpPreference", "Remove", &[(champ, Parametre::Textes(&[x]))]);
    }
}

/// Coupe SMB1 côté serveur (Set-SmbServerConfiguration -EnableSMB1Protocol $false ; erreurs ignorées)
pub fn smb1_serveur() {
    let _ = methode(
        "root\\Microsoft\\Windows\\SMB",
        "MSFT_SmbServerConfiguration",
        "SetConfiguration",
        &[("EnableSMB1Protocol", Parametre::Oui(false))],
    );
}

/// Les programmes des antivirus inscrits au Centre de sécurité de Windows (chemins complets ; aucun sur Windows
/// Server)
pub fn antivirus(d: &Dossiers) -> Vec<String> {
    let mut l = Vec::new();
    let champs = ["pathToSignedProductExe", "pathToSignedReportingExe"];
    if let Ok(av) = requete("root\\SecurityCenter2", "SELECT * FROM AntiVirusProduct", &champs, 5) {
        for a in av {
            for c in champs {
                if let Some(p) = a[c].as_str().and_then(|x| d.programme_de_commande(x)) {
                    l.push(p);
                }
            }
        }
    }
    l
}

/// Les antivirus et les services : programme et processus (`Get-AideContexte`)
pub fn contexte(d: &Dossiers) -> Result<ContexteProgrammes, String> {
    let mut c = ContexteProgrammes { antivirus: antivirus(d), ..Default::default() };
    for s in requete("root\\cimv2", "SELECT PathName, ProcessId FROM Win32_Service", &["PathName", "ProcessId"], 10)? {
        if let Some(p) = s["PathName"].as_str().and_then(|x| d.programme_de_commande(x)) {
            c.services_exe.push(p);
        }
        if let Some(pid) = s["ProcessId"].as_i64().filter(|&n| n > 0) {
            c.services_pid.insert(pid);
        }
    }
    Ok(c)
}

/// L'emplacement des programmes en cours (Win32_Process)
pub fn chemins_processus() -> Vec<(i64, String)> {
    requete("root\\cimv2", "SELECT ProcessId, ExecutablePath FROM Win32_Process", &["ProcessId", "ExecutablePath"], 30)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| Some((p["ProcessId"].as_i64()?, p["ExecutablePath"].as_str()?.to_string())))
        .collect()
}

/// Un point de restauration (Checkpoint-Computer -RestorePointType MODIFY_SETTINGS)
pub fn point_restauration(description: &str) -> Result<(), String> {
    use windows::Win32::System::Restore::{
        BEGIN_SYSTEM_CHANGE, END_SYSTEM_CHANGE, MODIFY_SETTINGS, RESTOREPOINTINFOW, STATEMGRSTATUS,
    };
    // (srclient.dll : absente des Windows Server ; cherchée au moment de s'en servir)
    let f = fonction("srclient.dll", b"SRSetRestorePointW\0")
        .ok_or("La restauration du système n'existe pas sur ce PC.")?;
    type Fonction = unsafe extern "system" fn(*const RESTOREPOINTINFOW, *mut STATEMGRSTATUS) -> i32;
    // SAFETY : signature de SRSetRestorePointW (srrestoreptapi.h)
    let creer: Fonction = unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, Fonction>(f) };
    let mut info =
        RESTOREPOINTINFOW { dwEventType: BEGIN_SYSTEM_CHANGE, dwRestorePtType: MODIFY_SETTINGS, ..Default::default() };
    for (i, c) in description.encode_utf16().take(255).enumerate() {
        info.szDescription[i] = c;
    }
    let mut etat = STATEMGRSTATUS::default();
    // SAFETY : structures valables pendant l'appel
    if unsafe { creer(&info, &mut etat) } == 0 {
        return Err(format!("Windows n'a pas créé le point de restauration (erreur {}).", { etat.nStatus }.0));
    }
    info.dwEventType = END_SYSTEM_CHANGE;
    info.llSequenceNumber = etat.llSequenceNumber;
    // SAFETY : comme plus haut
    unsafe { creer(&info, &mut etat) };
    Ok(())
}
