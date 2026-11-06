//! La lecture de la protection, sans droits administrateur : `$SecDataWork` (Defender, autres antivirus, pare-feu,
//! exclusions, menaces, fichiers piégés, programmes cachés, démarrage, tâches planifiées), le contrôle des comptes,
//! le fichier hosts, le proxy et les programmes au démarrage (`Read-StartupItems`). Rien n'est écrit.

use super::com::{self, Com, requete};
use super::{DEFENDER, signature, taches, variable};
use crate::etat::{Demarrage, Detection, Environnement, EtatS, Menace, Mp, Tache};
use crate::ps::{self, Date};
use crate::regles::{self, ContexteTaches, DetectionLue};
use crate::systeme::Lecture;
use serde_json::{Map, Value};
use std::os::windows::fs::MetadataExt;
use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::System::Com::{IPersistFile, STGM_READ};
use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime};
use windows::Win32::UI::Shell::{
    FOLDERID_CommonStartup, FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Profile, FOLDERID_Startup, IShellLinkW,
    KNOWN_FOLDER_FLAG, SHGetKnownFolderPath, ShellLink,
};
use windows::core::{GUID, HSTRING, Interface};
use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE, Type};

/// 1er janvier 1601 (FILETIME) en ticks de .NET
const DEBUT_FILETIME: i64 = 504_911_232_000_000_000;

/// Une heure UTC (ticks) en heure locale, selon les règles d'heure d'été de cette date-là
fn ticks_local(utc: i64) -> i64 {
    let vers = |t: i64| {
        let f = (t - DEBUT_FILETIME).max(0) as u64;
        FILETIME { dwLowDateTime: f as u32, dwHighDateTime: (f >> 32) as u32 }
    };
    let depuis =
        |f: FILETIME| ((u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)) as i64 + DEBUT_FILETIME;
    let f = vers(utc);
    let (mut st, mut local, mut fl, mut ft) =
        (SYSTEMTIME::default(), SYSTEMTIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY : conversions de dates par Windows, structures locales
    unsafe {
        if FileTimeToSystemTime(&f, &mut st).is_err()
            || SystemTimeToTzSpecificLocalTime(None, &st, &mut local).is_err()
            || SystemTimeToFileTime(&local, &mut fl).is_err()
            || SystemTimeToFileTime(&st, &mut ft).is_err()
        {
            return utc;
        }
    }
    utc + (depuis(fl) - depuis(ft))
}

/// Une date de WMI (« 20261104210500.000000+060 ») en date locale, comme les `[datetime]` des modules de Defender
fn date_wmi(v: &Value) -> Option<Date> {
    let t = v.as_str()?;
    let n = |a: usize, b: usize| t.get(a..b)?.parse::<i64>().ok();
    let (an, mois, jour, h, m, s) = (n(0, 4)?, n(4, 6)?, n(6, 8)?, n(8, 10)?, n(10, 12)?, n(12, 14)?);
    let decalage = n(22, 25).unwrap_or(0) * if t.get(21..22) == Some("-") { -1 } else { 1 };
    let annonce =
        Date { annee: an, mois: mois as u32, jour: jour as u32, heure: h as u32, minute: m as u32, seconde: s as u32 };
    if !(1..=12).contains(&annonce.mois) {
        return None;
    }
    let utc = annonce.ticks() - decalage * 600_000_000;
    Some(Date::depuis_ticks(ticks_local(utc)))
}

/// Un entier de WMI (un uint32 arrive parfois en entier signé : 4294967295 devient -1)
fn non_signe(v: &Value) -> i64 {
    let n = ps::reel(v) as i64;
    if n < 0 { n + (1i64 << 32) } else { n }
}

fn texte(o: &Map<String, Value>, k: &str) -> String {
    o.get(k).map(ps::texte).unwrap_or_default()
}

/// Un dossier connu de Windows (Bureau, Documents...)
fn dossier_connu(id: &GUID) -> Option<String> {
    // SAFETY : chemin rendu par Windows, libéré par CoTaskMemFree
    unsafe {
        let p = SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG(0), None).ok()?;
        let s = p.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

/// Les fichiers d'un dossier et de ses sous-dossiers jusqu'à `profondeur`, dans l'ordre de Get-ChildItem -Recurse
/// -Force -File (ceux du dossier, puis chaque sous-dossier) ; les liens vers d'autres dossiers ne sont pas suivis
fn fichiers(dossier: &str, profondeur: u32, r: &mut Vec<(String, String)>) {
    let Ok(l) = std::fs::read_dir(dossier) else { return };
    let mut sous = Vec::new();
    for e in l.flatten() {
        let Ok(m) = e.metadata() else { continue };
        let nom = e.file_name().to_string_lossy().into_owned();
        let chemin = format!("{}\\{nom}", dossier.trim_end_matches('\\'));
        if m.file_attributes() & 0x10 != 0 {
            if m.file_attributes() & 0x400 == 0 {
                sous.push(chemin);
            }
        } else {
            r.push((chemin, nom));
        }
    }
    if profondeur > 0 {
        for s in sous {
            fichiers(&s, profondeur - 1, r);
        }
    }
}

/// `-match '\.(pdf|docx?|xlsx?|jpe?g|png|gif|txt|mp4|mp3|zip|rar)\s*\.(exe|scr|bat|cmd|com|pif|vbs|js|jse|hta|lnk)$'`
pub fn double_extension(nom: &str) -> bool {
    let b = nom.to_lowercase();
    let fins = ["exe", "scr", "bat", "cmd", "com", "pif", "vbs", "js", "jse", "hta", "lnk"];
    let debuts = ["pdf", "doc", "docx", "xls", "xlsx", "jpg", "jpeg", "png", "gif", "txt", "mp4", "mp3", "zip", "rar"];
    fins.iter().any(|f| {
        b.strip_suffix(&format!(".{f}")).is_some_and(|avant| {
            let avant = avant.trim_end_matches(char::is_whitespace);
            debuts.iter().any(|d| avant.ends_with(&format!(".{d}")))
        })
    })
}

/// L'extension comme `FileInfo.Extension` (« .js »), en minuscules
fn extension(nom: &str) -> String {
    nom.rfind('.').map(|i| nom[i..].to_lowercase()).unwrap_or_default()
}

/// Une valeur du registre comme `RegistryKey.GetValue` (texte à variables développé)
fn valeur_reg(k: &Key, nom: &str) -> Value {
    let Ok(v) = k.get_value(nom) else { return Value::Null };
    match v.ty() {
        Type::U32 => u32::try_from(v).map(Value::from).unwrap_or(Value::Null),
        Type::U64 => u64::try_from(v).map(Value::from).unwrap_or(Value::Null),
        Type::String => String::try_from(v).map(Value::String).unwrap_or(Value::Null),
        Type::ExpandString => String::try_from(v)
            .map(|s| Value::String(regles::developper(&s, &|n: &str| variable(n))))
            .unwrap_or(Value::Null),
        Type::MultiString => Vec::<String>::try_from(v)
            .map(|l| Value::Array(l.into_iter().map(Value::String).collect()))
            .unwrap_or(Value::Null),
        _ => Value::Array(v.iter().map(|b| Value::from(*b)).collect()),
    }
}

fn reg(racine: &Key, chemin: &str, nom: &str) -> Value {
    racine.open(chemin).map(|k| valeur_reg(&k, nom)).unwrap_or(Value::Null)
}

/// La cible d'un raccourci (.lnk), vide si illisible
fn cible_raccourci(lnk: &str) -> String {
    let Ok(l) = com::creer::<IShellLinkW>(&ShellLink) else { return String::new() };
    // SAFETY : appels COM ordinaires, tampon local
    unsafe {
        let Ok(p) = l.cast::<IPersistFile>() else { return String::new() };
        if p.Load(&HSTRING::from(lnk), STGM_READ).is_err() {
            return String::new();
        }
        let mut b = vec![0u16; 1024];
        if l.GetPath(&mut b, std::ptr::null_mut(), 0).is_err() {
            return String::new();
        }
        let n = b.iter().position(|c| *c == 0).unwrap_or(b.len());
        String::from_utf16_lossy(&b[..n])
    }
}

fn fichier_existe(p: &str) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file())
}

/// `Resolve-StartupExe`
fn programme_demarrage(commande: &str) -> Option<String> {
    if commande.is_empty() {
        return None;
    }
    let cmd = regles::developper(commande.trim(), &|n: &str| variable(n));
    let bas = cmd.to_ascii_lowercase();
    let mut exe = None;
    if let Some(r) = cmd.strip_prefix('"')
        && let Some(f) = r.find('"')
        && f > 0
    {
        exe = Some(r[..f].to_string());
    }
    if exe.is_none() {
        // (^(.+?\.(exe|lnk|bat|cmd|url))(\s|$) : le plus court début qui finit par une de ces extensions)
        for (k, _) in cmd.char_indices().skip(1).chain([(cmd.len(), ' ')]) {
            let debut = &bas[..k];
            let suite = cmd[k..].chars().next();
            if suite.is_none_or(char::is_whitespace)
                && [".exe", ".lnk", ".bat", ".cmd", ".url"].iter().any(|x| debut.len() > x.len() && debut.ends_with(x))
            {
                exe = Some(cmd[..k].to_string());
                break;
            }
        }
    }
    let mut exe = exe.unwrap_or(cmd.clone());
    if !exe.contains(['\\', '/']) {
        exe = format!("{}\\System32\\{exe}", variable("windir").unwrap_or_default());
    }
    if !fichier_existe(&exe) {
        return None;
    }
    if ps::finit(&exe, ".lnk") {
        let t = cible_raccourci(&exe);
        if !t.is_empty() && fichier_existe(&t) {
            return Some(t);
        }
    }
    Some(exe)
}

/// `Read-StartupItems` : les programmes au démarrage (Nom, Exe, Enabled)
fn demarrage() -> Vec<Demarrage> {
    let racine = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved";
    let actif = |ruche: &Key, approuve: &str, nom: &str| match reg(ruche, &format!("{racine}\\{approuve}"), nom) {
        Value::Array(b) if !b.is_empty() => ps::entier(&b[0]).unwrap_or(0) & 1 == 0,
        _ => true,
    };
    let mut r = Vec::new();
    let cles: [(&Key, &str, &Key, &str); 3] = [
        (CURRENT_USER, "Software\\Microsoft\\Windows\\CurrentVersion\\Run", CURRENT_USER, "Run"),
        (LOCAL_MACHINE, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run", LOCAL_MACHINE, "Run"),
        (LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run", LOCAL_MACHINE, "Run32"),
    ];
    for (ruche, chemin, ruche_ok, approuve) in cles {
        let Ok(k) = ruche.open(chemin) else { continue };
        let Ok(valeurs) = k.values() else { continue };
        for (nom, _) in valeurs {
            if nom.is_empty() {
                continue;
            }
            let commande = ps::texte(&valeur_reg(&k, &nom));
            let Some(exe) = programme_demarrage(&commande) else { continue };
            r.push(Demarrage { actif: actif(ruche_ok, approuve, &nom), nom, exe });
        }
    }
    for (id, ruche) in [(&FOLDERID_Startup, CURRENT_USER), (&FOLDERID_CommonStartup, LOCAL_MACHINE)] {
        let Some(d) = dossier_connu(id) else { continue };
        let Ok(l) = std::fs::read_dir(&d) else { continue };
        for e in l.flatten() {
            let Ok(m) = e.metadata() else { continue };
            let nom = e.file_name().to_string_lossy().into_owned();
            if m.file_attributes() & 0x10 != 0 || nom.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            let chemin = format!("{}\\{nom}", d.trim_end_matches('\\'));
            let Some(exe) = programme_demarrage(&chemin) else { continue };
            let base = match nom.rfind('.') {
                Some(i) if i > 0 => nom[..i].to_string(),
                _ => nom.clone(),
            };
            r.push(Demarrage { actif: actif(ruche, "StartupFolder", &nom), nom: base, exe });
        }
    }
    r
}

/// Le fichier hosts comme `Get-Content` : UTF-8 ou UTF-16 avec BOM, sinon la page de code de Windows (1252)
fn lignes_texte(octets: &[u8]) -> Vec<String> {
    let t = if let Some(r) = octets.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(r).into_owned()
    } else if let Some(r) = octets.strip_prefix(&[0xFF, 0xFE]) {
        String::from_utf16_lossy(&r.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect::<Vec<_>>())
    } else {
        const HAUT: [u16; 32] = [
            0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x8D,
            0x017D, 0x8F, 0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A,
            0x0153, 0x9D, 0x017E, 0x0178,
        ];
        octets
            .iter()
            .map(|&b| {
                if (0x80..0xA0).contains(&b) {
                    char::from_u32(u32::from(HAUT[(b - 0x80) as usize])).unwrap_or('?')
                } else {
                    char::from(b)
                }
            })
            .collect()
    };
    let mut l: Vec<String> = t.split('\n').map(|x| x.strip_suffix('\r').unwrap_or(x).to_string()).collect();
    if l.last().is_some_and(String::is_empty) {
        l.pop();
    }
    l.into_iter().flat_map(|x| x.split('\r').map(str::to_string).collect::<Vec<_>>()).collect()
}

/// Les détections de Defender et les noms des menaces, lus sur place (Get-MpThreatDetection, Get-MpThreat)
pub fn detections_locales() -> regles::Detections {
    let d = requete(
        DEFENDER,
        "SELECT * FROM MSFT_MpThreatDetection",
        &["ThreatID", "InitialDetectionTime", "Resources"],
        30,
    )
    .map(|l| {
        l.iter()
            .map(|x| DetectionLue {
                id: texte(x, "ThreatID"),
                quand: x.get("InitialDetectionTime").and_then(date_wmi).unwrap_or(Date::depuis_ticks(0)),
                ressources: x.get("Resources").map(ps::liste).unwrap_or_default().iter().map(ps::texte).collect(),
            })
            .collect()
    });
    let noms = requete(DEFENDER, "SELECT * FROM MSFT_MpThreat", &["ThreatID", "ThreatName"], 30)
        .unwrap_or_default()
        .iter()
        .map(|m| (texte(m, "ThreatID"), texte(m, "ThreatName")))
        .collect();
    (d, noms)
}

/// `$SecDataWork` et le reste de `Get-ProtectionStatus`
pub fn lire() -> Lecture {
    let _com = Com::pret();
    let mut s = EtatS::default();
    if let Ok(l) = requete(
        DEFENDER,
        "SELECT * FROM MSFT_MpComputerStatus",
        &["AMRunningMode", "RealTimeProtectionEnabled", "AntivirusSignatureAge", "QuickScanAge", "QuickScanEndTime"],
        30,
    ) && let Some(m) = l.first()
    {
        s.mp = Some(Mp {
            mode: texte(m, "AMRunningMode"),
            temps_reel: m.get("RealTimeProtectionEnabled").is_some_and(ps::vrai),
            age_base: m.get("AntivirusSignatureAge").map(non_signe).unwrap_or(0),
            age_analyse: m.get("QuickScanAge").map(non_signe).unwrap_or(0),
            fin_analyse: m.get("QuickScanEndTime").and_then(date_wmi),
        });
    }
    if let Ok(l) = requete("root\\SecurityCenter2", "SELECT displayName FROM AntivirusProduct", &["displayName"], 30) {
        s.autres_av = l
            .iter()
            .map(|a| texte(a, "displayName"))
            .filter(|n| {
                let b = n.to_lowercase();
                !b.contains("windows defender") && !b.contains("microsoft defender")
            })
            .collect();
    }
    if let Ok(l) =
        requete("root\\StandardCimv2", "SELECT Name, Enabled FROM MSFT_NetFirewallProfile", &["Name", "Enabled"], 30)
    {
        // (Enabled : 0 non, 1 oui, 2 pas configuré)
        s.pare_feu_coupe = l
            .iter()
            .filter(|p| p.get("Enabled").map(ps::reel).unwrap_or(1.0) == 0.0)
            .map(|p| texte(p, "Name"))
            .collect();
    }
    if let Ok(l) = requete(
        DEFENDER,
        "SELECT * FROM MSFT_MpPreference",
        &["ExclusionPath", "ExclusionProcess", "ExclusionExtension"],
        30,
    ) && let Some(p) = l.first()
    {
        s.exclusions = ["ExclusionPath", "ExclusionProcess", "ExclusionExtension"]
            .iter()
            .flat_map(|k| p.get(*k).map(ps::liste).unwrap_or_default())
            .filter(ps::vrai)
            .map(|x| ps::texte(&x))
            .filter(|x| !ps::commence(x, "N/A"))
            .collect();
    }
    if let Ok(l) =
        requete(DEFENDER, "SELECT * FROM MSFT_MpThreat", &["ThreatID", "ThreatName", "SeverityID", "IsActive"], 30)
    {
        s.menaces = l
            .iter()
            .map(|m| Menace {
                id: texte(m, "ThreatID"),
                nom: texte(m, "ThreatName"),
                gravite: m.get("SeverityID").cloned().unwrap_or(Value::Null),
                active: m.get("IsActive").is_some_and(ps::vrai),
            })
            .collect();
    }
    if let Ok(mut l) = requete(
        DEFENDER,
        "SELECT * FROM MSFT_MpThreatDetection",
        &["ThreatID", "ThreatStatusID", "InitialDetectionTime", "Resources"],
        30,
    ) {
        let quand = |x: &Map<String, Value>| x.get("InitialDetectionTime").and_then(date_wmi);
        l.sort_by_key(|x| std::cmp::Reverse(quand(x)));
        s.detections = l
            .iter()
            .take(15)
            .map(|x| Detection {
                id: texte(x, "ThreatID"),
                statut: x.get("ThreatStatusID").cloned().unwrap_or(Value::Null),
                quand: quand(x),
                ressources: x.get("Resources").map(ps::liste).unwrap_or_default(),
            })
            .collect();
    }

    // Fichiers piégés dans les dossiers de l'utilisateur
    let profil = dossier_connu(&FOLDERID_Profile).unwrap_or_default();
    let telechargements = format!("{profil}\\Downloads");
    for d in [Some(telechargements), dossier_connu(&FOLDERID_Desktop), dossier_connu(&FOLDERID_Documents)]
        .into_iter()
        .flatten()
    {
        if !std::path::Path::new(&d).exists() {
            continue;
        }
        let dans_telechargements = ps::finit(&d, "Downloads");
        let mut l = Vec::new();
        fichiers(&d, 2, &mut l);
        for (chemin, nom) in l {
            if double_extension(&nom) {
                s.doubles.push(chemin);
            } else if dans_telechargements
                && ["scr", "pif", "vbs", "vbe", "js", "jse", "hta", "wsf"]
                    .iter()
                    .any(|x| extension(&nom) == format!(".{x}"))
            {
                s.scripts.push(chemin);
            }
        }
    }
    // Programmes non signés posés dans les dossiers à risque
    let temp = variable("TEMP").unwrap_or_default();
    let public = format!("{}\\Users\\Public", variable("SystemDrive").unwrap_or_default());
    let (appdata, local, programdata) = (
        variable("APPDATA").unwrap_or_default(),
        variable("LOCALAPPDATA").unwrap_or_default(),
        variable("ProgramData").unwrap_or_default(),
    );
    for (d, prof) in [(&temp, 0), (&appdata, 0), (&local, 0), (&public, 3), (&programdata, 0)] {
        if d.is_empty() || !std::path::Path::new(d).exists() {
            continue;
        }
        let mut l = Vec::new();
        fichiers(d, prof, &mut l);
        for (chemin, nom) in l {
            if [".exe", ".scr", ".com", ".pif"].contains(&extension(&nom).as_str()) && !signature::valide(&chemin) {
                s.caches.push(chemin);
            }
        }
    }
    // Programmes au démarrage dans un dossier à risque ou non signés
    let dem = demarrage();
    let risques = [&temp, &public, &programdata, &appdata, &local];
    for d in dem.iter().filter(|d| d.actif) {
        let dossier = d.exe.rsplit_once('\\').map(|x| x.0).unwrap_or("");
        let risque = risques.iter().any(|r| !r.is_empty() && ps::egal(dossier, r)) || ps::commence(dossier, &temp);
        if risque || !signature::valide(&d.exe) {
            s.demarrage_suspect.push(d.exe.clone());
        }
    }
    // Tâches planifiées suspectes
    let script = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| format!("{}\\fichiers\\Nevermind.ps1", p.display())))
        .unwrap_or_default();
    let var = |n: &str| variable(n);
    let prot = |c: &str| std::path::Path::new(c).exists() && super::acl::protege(c);
    let existe = |c: &str| std::path::Path::new(c).exists();
    let c = ContexteTaches {
        temp: &temp,
        public: &public,
        variable: &var,
        script_protege: &prot,
        existe: &existe,
        propre_tache: Some(("Nevermind (démarrage)", &script)),
    };
    for t in taches::liste().unwrap_or_default() {
        if t.desactivee || ps::commence(&t.dossier, "\\Microsoft\\") {
            continue;
        }
        for (exe, args) in &t.actions {
            if let Some(x) = regles::tache_suspecte(&t.nom, &t.dossier, exe, args, &c, false) {
                s.taches.push(Tache { ..x });
            }
        }
    }
    // Contrôle des comptes (absent : activé)
    s.uac = !matches!(
        reg(LOCAL_MACHINE, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System", "EnableLUA"),
        Value::Number(ref n) if n.as_f64() == Some(0.0)
    );

    // Fichier hosts, proxy
    let hosts =
        format!("{}\\System32\\drivers\\etc\\hosts", variable("windir").unwrap_or_else(|| "C:\\WINDOWS".into()));
    let inet = "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings";
    let env = Environnement {
        hosts: std::fs::read(&hosts).ok().map(|o| lignes_texte(&o)),
        proxy_actif: reg(CURRENT_USER, inet, "ProxyEnable"),
        proxy_pac: reg(CURRENT_USER, inet, "AutoConfigURL"),
        proxy_serveur: reg(CURRENT_USER, inet, "ProxyServer"),
        demarrage: dem,
    };
    Lecture { s, env }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fichiers_deguises() {
        assert!(double_extension("facture.pdf.exe"));
        assert!(double_extension("Photo.JPG  .scr"));
        assert!(double_extension("a.docx.lnk"));
        assert!(!double_extension("setup.exe"));
        assert!(!double_extension("pdf.exe"));
        assert_eq!(lignes_texte(b"a\r\nb\nc\r\n"), vec!["a", "b", "c"]);
        assert_eq!(lignes_texte(&[0x80, b'x']), vec!["\u{20ac}x"]);
    }
}
