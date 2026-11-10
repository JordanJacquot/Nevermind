//! La lecture lente du PC (`$AnalysisDataWork`, interface.ps1, et `Read-StartupItems`, optimisations.ps1) : WMI, disques,
//! journaux d'événements, programmes au démarrage. Faite dans un fil à part, sans droits administrateur ; ce qui
//! demande les droits (Secure Boot, puce TPM, santé des disques) est complété ensuite par l'Aide (`lecture.pc`).
//! Les données ont les mêmes noms de champs que la version PowerShell (les juges les donnent telles quelles).

use crate::analyse::Entrees;
use crate::reglages::Demarrage;

/// Ce qui a été lu
#[derive(Debug, Clone, Default)]
pub struct Lu {
    pub entrees: Entrees,
    pub demarrage: Vec<Demarrage>,
}

/// Tout lire (sur un autre système que Windows : rien)
pub fn lire() -> Lu {
    #[cfg(windows)]
    {
        vrai::lire()
    }
    #[cfg(not(windows))]
    {
        Lu::default()
    }
}

pub use nevermind_noyau::route::{EtatCarte, InterfaceIp, RouteDefaut, choisir_route};

#[cfg(windows)]
mod vrai {
    use super::Lu;
    use crate::analyse::Entrees;
    use crate::reglages::{APPROUVE, Demarrage};
    use crate::windows::wmi::{self, CIMV2, STOCKAGE, Valeur};
    use serde_json::{Map, Value, json};
    use std::collections::HashMap;

    fn v(x: &Valeur) -> Value {
        match x {
            Valeur::Rien => Value::Null,
            Valeur::Texte(s) => json!(s),
            Valeur::Entier(n) => json!(n),
            Valeur::Liste(l) => json!(l),
        }
    }
    /// Un nombre (les nombres de 64 bits arrivent en texte)
    fn n(x: &Valeur) -> Value {
        match x {
            Valeur::Texte(s) => s.trim().parse::<f64>().map(|f| json!(f)).unwrap_or(Value::Null),
            Valeur::Entier(i) => json!(i),
            _ => Value::Null,
        }
    }
    /// Une date WMI (« 20261106120000.000000+060 ») en « 2026-11-06T12:00:00 »
    fn date(x: &Valeur) -> Value {
        let Valeur::Texte(s) = x else { return Value::Null };
        if s.len() < 14 || !s[..14].chars().all(|c| c.is_ascii_digit()) {
            return Value::Null;
        }
        json!(format!("{}-{}-{}T{}:{}:{}", &s[0..4], &s[4..6], &s[6..8], &s[8..10], &s[10..12], &s[12..14]))
    }
    fn objets(espace: &str, wql: &str, champs: &[&str], conv: &[fn(&Valeur) -> Value]) -> Vec<Value> {
        wmi::requete(espace, wql, champs)
            .unwrap_or_default()
            .iter()
            .map(|l| {
                let mut m = Map::new();
                for (i, c) in champs.iter().enumerate() {
                    let f = conv.get(i).copied().unwrap_or(v);
                    m.insert((*c).into(), l.get(i).map(f).unwrap_or(Value::Null));
                }
                Value::Object(m)
            })
            .collect()
    }

    pub fn lire() -> Lu {
        let os = objets(
            CIMV2,
            "SELECT Caption, BuildNumber, TotalVisibleMemorySize, FreePhysicalMemory, LastBootUpTime FROM Win32_OperatingSystem",
            &["Caption", "BuildNumber", "TotalVisibleMemorySize", "FreePhysicalMemory", "LastBootUpTime"],
            &[v, v, n, n, date],
        );
        let batteries = objets(
            CIMV2,
            "SELECT Name, DeviceID, Description, Chemistry, BatteryStatus, EstimatedChargeRemaining, EstimatedRunTime FROM Win32_Battery",
            &[
                "Name",
                "DeviceID",
                "Description",
                "Chemistry",
                "BatteryStatus",
                "EstimatedChargeRemaining",
                "EstimatedRunTime",
            ],
            &[],
        );
        let mut d = Map::new();
        d.insert("OS".into(), os.into_iter().next().unwrap_or(Value::Null));
        let mut cim = Map::new();
        if !batteries.is_empty() {
            let w = |cl: &str, ch: &[&str]| {
                objets(r"ROOT\WMI", &format!("SELECT {} FROM {cl}", ch.join(", ")), ch, &[])
                    .into_iter()
                    .next()
                    .unwrap_or(Value::Null)
            };
            let s = w("BatteryStaticData", &["ManufactureName", "SerialNumber", "Chemistry", "DesignedCapacity"]);
            let f = w("BatteryFullChargedCapacity", &["FullChargedCapacity"]);
            let st = w("BatteryStatus", &["Voltage"]);
            let rt = w("BatteryRuntime", &["EstimatedRuntime"]);
            let c = w("BatteryCycleCount", &["CycleCount"]);
            let t = |o: &Value, k: &str| crate::net::champ_texte(o, k).trim().to_string();
            d.insert("BatWmi".into(), json!({ "Maker": t(&s, "ManufactureName"), "Serial": t(&s, "SerialNumber"), "Chem": s.get("Chemistry"),
                "Design": crate::net::champ_nombre(&s, "DesignedCapacity"), "Full": crate::net::champ_nombre(&f, "FullChargedCapacity"),
                "Volt": crate::net::champ_nombre(&st, "Voltage"), "Runtime": crate::net::champ_nombre(&rt, "EstimatedRuntime") }));
            cim.insert(
                "BatteryStaticData".into(),
                if s.is_null() { s.clone() } else { json!({ "DesignedCapacity": s.get("DesignedCapacity") }) },
            );
            cim.insert("BatteryFullChargedCapacity".into(), f);
            cim.insert("BatteryCycleCount".into(), c);
        }
        d.insert("Battery".into(), Value::Array(batteries));
        d.insert("UpsHints".into(), Value::Array(indices_onduleur()));
        let enc = objets(CIMV2, "SELECT ChassisTypes FROM Win32_SystemEnclosure", &["ChassisTypes"], &[]);
        d.insert("Chassis".into(), enc.first().and_then(|e| e.get("ChassisTypes").cloned()).unwrap_or(json!([])));
        let cs = objets(CIMV2, "SELECT PCSystemType FROM Win32_ComputerSystem", &["PCSystemType"], &[]);
        d.insert("PCType".into(), cs.first().and_then(|e| e.get("PCSystemType").cloned()).unwrap_or(json!(0)));
        d.insert("CPU".into(), objets(CIMV2, "SELECT Name, NumberOfCores, NumberOfLogicalProcessors, MaxClockSpeed, L3CacheSize FROM Win32_Processor",
            &["Name", "NumberOfCores", "NumberOfLogicalProcessors", "MaxClockSpeed", "L3CacheSize"], &[]).into_iter().next().unwrap_or(Value::Null));
        let gpus = objets(
            CIMV2,
            "SELECT Name, DriverVersion, DriverDate FROM Win32_VideoController",
            &["Name", "DriverVersion", "DriverDate"],
            &[v, v, date],
        );
        d.insert("GPUs".into(), Value::Array(gpus.clone()));
        d.insert("NvLatest".into(), Value::Null);
        d.insert("Mem".into(), Value::Array(objets(CIMV2, "SELECT Capacity, ConfiguredClockSpeed, Speed, SMBIOSMemoryType, Manufacturer, PartNumber FROM Win32_PhysicalMemory",
            &["Capacity", "ConfiguredClockSpeed", "Speed", "SMBIOSMemoryType", "Manufacturer", "PartNumber"], &[n, v, v, v, v, v])));
        d.insert("MemDiag".into(), crate::windows::journal::dernier_test_memoire());
        let lecteur = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let lettre = lecteur.trim_end_matches(':').to_string();
        let parts =
            objets(STOCKAGE, "SELECT DiskNumber, DriveLetter FROM MSFT_Partition", &["DiskNumber", "DriveLetter"], &[]);
        let lettre_de = |p: &Value| match p.get("DriveLetter") {
            Some(Value::Number(x)) => {
                char::from_u32(x.as_u64().unwrap_or(0) as u32).filter(|c| *c != '\0').map(|c| c.to_string())
            }
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };
        if let Some(p) = parts.iter().find(|p| lettre_de(p).is_some_and(|l| l.eq_ignore_ascii_case(&lettre))) {
            d.insert("SysDisk".into(), json!(crate::net::champ_texte(p, "DiskNumber")));
        }
        let mut disques = objets(
            STOCKAGE,
            "SELECT DeviceId, FriendlyName, MediaType, BusType, HealthStatus, Size FROM MSFT_PhysicalDisk",
            &["DeviceId", "FriendlyName", "MediaType", "BusType", "HealthStatus", "Size"],
            &[v, v, v, v, v, n],
        );
        disques.sort_by_key(|x| crate::net::champ_entier(x, "DeviceId"));
        let disques: Vec<Value> = disques
            .into_iter()
            .map(|mut x| {
                let media = match crate::net::champ_entier(&x, "MediaType") {
                    3 => "HDD",
                    4 => "SSD",
                    5 => "SCM",
                    _ => "Unspecified",
                };
                let bus = match crate::net::champ_entier(&x, "BusType") {
                    1 => "SCSI",
                    2 => "ATAPI",
                    3 => "ATA",
                    4 => "1394",
                    5 => "SSA",
                    6 => "Fibre Channel",
                    7 => "USB",
                    8 => "RAID",
                    9 => "iSCSI",
                    10 => "SAS",
                    11 => "SATA",
                    12 => "SD",
                    13 => "MMC",
                    15 => "File Backed Virtual",
                    16 => "Storage Spaces",
                    17 => "NVMe",
                    _ => "Unknown",
                };
                let sante = match crate::net::champ_entier(&x, "HealthStatus") {
                    0 => "Healthy",
                    1 => "Warning",
                    2 => "Unhealthy",
                    _ => "Unknown",
                };
                x["MediaType"] = json!(media);
                x["BusType"] = json!(bus);
                x["HealthStatus"] = json!(sante);
                let id = crate::net::champ_texte(&x, "DeviceId");
                let vols: Vec<Value> = parts
                    .iter()
                    .filter(|p| crate::net::champ_texte(p, "DiskNumber") == id)
                    .filter_map(&lettre_de)
                    .map(|l| json!({ "DriveLetter": l, "Vol": crate::windows::volume(&l) }))
                    .collect();
                let lettres: Vec<Value> = vols.iter().map(|v| v["DriveLetter"].clone()).collect();
                json!({ "Disk": x, "Vols": vols, "Rel": null, "Letters": lettres })
            })
            .collect();
        d.insert("Disks".into(), Value::Array(disques));
        d.insert(
            "LogicalC".into(),
            objets(
                CIMV2,
                &format!("SELECT Size, FreeSpace FROM Win32_LogicalDisk WHERE DeviceID='{lecteur}'"),
                &["Size", "FreeSpace"],
                &[n, n],
            )
            .into_iter()
            .next()
            .unwrap_or(Value::Null),
        );
        d.insert(
            "BaseBoard".into(),
            objets(CIMV2, "SELECT Manufacturer, Product FROM Win32_BaseBoard", &["Manufacturer", "Product"], &[])
                .into_iter()
                .next()
                .unwrap_or(Value::Null),
        );
        d.insert(
            "BIOS".into(),
            objets(
                CIMV2,
                "SELECT SMBIOSBIOSVersion, ReleaseDate FROM Win32_BIOS",
                &["SMBIOSBIOSVersion", "ReleaseDate"],
                &[v, date],
            )
            .into_iter()
            .next()
            .unwrap_or(Value::Null),
        );
        // (Secure Boot et TPM : seulement en administrateur, complétés par l'Aide)
        d.insert("SecureBoot".into(), Value::Null);
        d.insert("TpmOk".into(), json!(false));
        let j = crate::windows::journal::comptes_30_jours();
        for (k, x) in j {
            d.insert(k.into(), json!(x));
        }
        d.insert("Net".into(), reseau());
        let uefi = crate::windows::uefi();
        let mut vram = HashMap::new();
        for g in &gpus {
            let nom = crate::net::champ_texte(g, "Name");
            if let Some(x) = memoire_video(&nom) {
                vram.insert(nom, x);
            }
        }
        Lu {
            entrees: Entrees {
                data: Value::Object(d),
                demarrage: Vec::new(),
                jeux: Vec::new(),
                smi: smi(&gpus),
                vram,
                cim: Value::Object(cim),
                uefi,
                ignores: Vec::new(),
                plan_cree: None,
            },
            demarrage: demarrage(),
        }
    }

    /// `Get-GpuVram`
    fn memoire_video(nom: &str) -> Option<f64> {
        let base = r"HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
        for i in 0..32 {
            let k = format!("{base}\\{i:04}");
            let Ok(Some(desc)) = crate::windows::reg_lire(&k, "DriverDesc") else { continue };
            if desc.texte() != nom {
                continue;
            }
            for c in ["HardwareInformation.qwMemorySize", "HardwareInformation.MemorySize"] {
                match crate::windows::reg_lire(&k, c) {
                    Ok(Some(crate::pc::ValeurReg::QWord(x))) if x != 0 => return Some(x as f64),
                    Ok(Some(crate::pc::ValeurReg::DWord(x))) if x != 0 => return Some(x as u32 as f64),
                    Ok(Some(crate::pc::ValeurReg::Octets(o))) if o.len() >= 4 => {
                        let x = u32::from_le_bytes([o[0], o[1], o[2], o[3]]) as f64;
                        if x != 0.0 {
                            return Some(x);
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// `Invoke-Smi` : nvidia-smi (sans fenêtre), seulement s'il y a une carte NVIDIA
    fn smi(gpus: &[Value]) -> Option<HashMap<String, Option<f64>>> {
        if !gpus.iter().any(|g| {
            let n = crate::net::champ_texte(g, "Name").to_lowercase();
            n.contains("nvidia") || n.contains("geforce")
        }) {
            return None;
        }
        let windir = std::env::var("windir").unwrap_or_else(|_| r"C:\Windows".into());
        let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
        let exe =
            [format!(r"{windir}\System32\nvidia-smi.exe"), format!(r"{pf}\NVIDIA Corporation\NVSMI\nvidia-smi.exe")]
                .into_iter()
                .find(|p| std::path::Path::new(p).exists())?;
        let champs = "temperature.gpu,fan.speed,power.draw,power.limit,pcie.link.width.current,pcie.link.width.max";
        let (_, o) =
            crate::windows::commande(&exe, &[format!("--query-gpu={champs}"), "--format=csv,noheader,nounits".into()]);
        let l = o.lines().next()?;
        let vals: Vec<&str> = l.split(',').collect();
        Some(
            champs
                .split(',')
                .enumerate()
                .map(|(i, c)| {
                    let x = vals.get(i).map(|s| s.trim()).unwrap_or("");
                    let ok = !x.is_empty() && x.chars().all(|c| c.is_ascii_digit() || c == '.');
                    (c.to_string(), if ok { x.parse().ok() } else { None })
                })
                .collect(),
        )
    }

    /// La connexion active (seulement ce que l'analyse montre : câble ou Wi-Fi, vitesse, nom de la carte)
    fn reseau() -> Value {
        use super::{EtatCarte, InterfaceIp, RouteDefaut};
        use crate::net::champ_entier;
        const STD: &str = r"ROOT\StandardCimv2";
        // La route de plus petit RouteMetric seule ne suffit pas (ni le « Sort-Object RouteMetric » du PowerShell) :
        // une carte Wi-Fi débranchée peut garder sa route, au même coût que le câble et lue avant lui (PC de Jordan,
        // 08/11) ; Windows, lui, écarte les cartes débranchées et ajoute le coût de l'interface.
        let routes: Vec<RouteDefaut> = objets(
            STD,
            "SELECT InterfaceIndex, RouteMetric, Store FROM MSFT_NetRoute WHERE DestinationPrefix='0.0.0.0/0'",
            &["InterfaceIndex", "RouteMetric", "Store"],
            &[],
        )
        .iter()
        .map(|r| RouteDefaut {
            index: champ_entier(r, "InterfaceIndex"),
            metrique: champ_entier(r, "RouteMetric"),
            active: r.get("Store").is_none_or(Value::is_null) || champ_entier(r, "Store") == 1,
        })
        .collect();
        let interfaces: Vec<InterfaceIp> = objets(
            STD,
            "SELECT InterfaceIndex, InterfaceMetric, ConnectionState FROM MSFT_NetIPInterface WHERE AddressFamily=2",
            &["InterfaceIndex", "InterfaceMetric", "ConnectionState"],
            &[],
        )
        .iter()
        .map(|i| InterfaceIp {
            index: champ_entier(i, "InterfaceIndex"),
            metrique: champ_entier(i, "InterfaceMetric"),
            connectee: champ_entier(i, "ConnectionState") == 1,
        })
        .collect();
        let cartes: Vec<EtatCarte> = objets(
            STD,
            "SELECT InterfaceIndex, State, MediaConnectState, HardwareInterface, InterfaceType FROM MSFT_NetAdapter",
            &["InterfaceIndex", "State", "MediaConnectState", "HardwareInterface", "InterfaceType"],
            &[],
        )
        .iter()
        .map(|c| EtatCarte {
            index: champ_entier(c, "InterfaceIndex"),
            connectee: champ_entier(c, "MediaConnectState") != 2 && champ_entier(c, "State") != 3,
            materielle: champ_entier(c, "HardwareInterface") != 0,
            tunnel: nevermind_noyau::route::est_tunnel(champ_entier(c, "InterfaceType"), 0),
        })
        .collect();
        let Some(idx) = super::choisir_route(&routes, &interfaces, &cartes) else {
            return Value::Null;
        };
        let ad = objets(
            STD,
            &format!(
                "SELECT InterfaceDescription, Speed, NdisPhysicalMedium, InterfaceGuid FROM MSFT_NetAdapter WHERE InterfaceIndex={idx}"
            ),
            &["InterfaceDescription", "Speed", "NdisPhysicalMedium", "InterfaceGuid"],
            &[v, n, v, v],
        );
        let Some(a) = ad.into_iter().next() else { return Value::Null };
        let desc = crate::net::champ_texte(&a, "InterfaceDescription");
        let dl = desc.to_lowercase();
        let wifi = crate::net::champ_entier(&a, "NdisPhysicalMedium") == 9
            || dl.contains("wi-fi")
            || dl.contains("wifi")
            || dl.contains("wireless")
            || dl.contains("wlan");
        let s = crate::net::champ_nombre(&a, "Speed");
        let vitesse = if s >= 1e9 {
            format!("{} Gbps", crate::net::texte_nombre(s / 1e9))
        } else if s >= 1e6 {
            format!("{} Mbps", crate::net::texte_nombre(s / 1e6))
        } else if s >= 1e3 {
            format!("{} Kbps", crate::net::texte_nombre(s / 1e3))
        } else {
            format!("{} bps", crate::net::texte_nombre(s))
        };
        json!({ "IfIndex": idx, "Desc": desc, "Speed": vitesse, "Wifi": wifi, "Guid": a.get("InterfaceGuid"), "Bps": s })
    }

    /// Les indices d'un onduleur que Windows ne lit pas comme une batterie (prise USB du fabricant, logiciel)
    fn indices_onduleur() -> Vec<Value> {
        let mut h = Vec::new();
        let pnp = objets(
            CIMV2,
            "SELECT DeviceID, Name, Status FROM Win32_PnPEntity WHERE DeviceID LIKE 'USB%' OR DeviceID LIKE 'HID%'",
            &["DeviceID", "Name", "Status"],
            &[],
        );
        let re_vid = regex::Regex::new(r"(?i)^(USB|HID)\\VID_(051D|0463|0764|10AF|06DA|0D9F)&|^(USB|HID)\\VID_0665&PID_5161|^(USB|HID)\\VID_0925&PID_1234").unwrap();
        let re_09ae = regex::Regex::new(r"(?i)^(USB|HID)\\VID_09AE&").unwrap();
        let re_pw = regex::Regex::new(r"(?i)ups|battery|batterie|power").unwrap();
        let re_nom = regex::Regex::new(r"(?i)\bups\b|onduleur|uninterruptible|back-?ups|smart-?ups").unwrap();
        let re_v = regex::Regex::new(r"(?i)VID_([0-9A-F]{4})").unwrap();
        for p in pnp {
            let id = crate::net::champ_texte(&p, "DeviceID");
            let nom = crate::net::champ_texte(&p, "Name");
            if re_vid.is_match(&id) || (re_09ae.is_match(&id) && re_pw.is_match(&nom)) || re_nom.is_match(&nom) {
                let vid = re_v.captures(&id).map(|c| c[1].to_uppercase()).unwrap_or_default();
                h.push(json!({ "Kind": "usb", "Name": nom, "Vid": vid, "Ok": crate::net::champ_texte(&p, "Status") == "OK" }));
            }
        }
        let logiciel = regex::Regex::new(r"(?i)powerchute|powerpanel|viewpower|winpower|upsilon|intelligent power protector|power ?shield|upsmon|power ?master|apc data service|pbeagent|eaton ipp|cyberpower|smartpower|upsmart").unwrap();
        for s in
            objets(CIMV2, "SELECT Name, DisplayName, State FROM Win32_Service", &["Name", "DisplayName", "State"], &[])
        {
            if logiciel.is_match(&format!(
                "{} {}",
                crate::net::champ_texte(&s, "Name"),
                crate::net::champ_texte(&s, "DisplayName")
            )) {
                h.push(json!({ "Kind": "soft", "Name": crate::net::champ_texte(&s, "DisplayName"), "Ok": crate::net::champ_texte(&s, "State") == "Running" }));
            }
        }
        for base in [
            r"HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            r"HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ] {
            for k in crate::windows::reg_sous_cles(base) {
                if let Ok(Some(n)) = crate::windows::reg_lire(&format!("{base}\\{k}"), "DisplayName")
                    && logiciel.is_match(&n.texte())
                {
                    h.push(json!({ "Kind": "app", "Name": n.texte(), "Ok": true }));
                }
            }
        }
        h
    }

    /// `Read-StartupItems` : les entrées Run et les dossiers Démarrage (activées ou pas : relu à chaque analyse)
    fn demarrage() -> Vec<Demarrage> {
        let dossier = |id: &str| std::env::var(id).ok();
        let appdata = dossier("APPDATA").map(|a| format!(r"{a}\Microsoft\Windows\Start Menu\Programs\Startup"));
        let commun = dossier("ProgramData").map(|a| format!(r"{a}\Microsoft\Windows\Start Menu\Programs\StartUp"));
        let sources: Vec<(bool, Option<String>, String, &str)> = vec![
            (
                true,
                Some(r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Run".into()),
                format!(r"HKCU:\{APPROUVE}\Run"),
                "Utilisateur",
            ),
            (
                true,
                Some(r"HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run".into()),
                format!(r"HKLM:\{APPROUVE}\Run"),
                "Tous les utilisateurs",
            ),
            (
                true,
                Some(r"HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run".into()),
                format!(r"HKLM:\{APPROUVE}\Run32"),
                "Tous les utilisateurs (32 bits)",
            ),
            (false, appdata, format!(r"HKCU:\{APPROUVE}\StartupFolder"), "Dossier Démarrage"),
            (false, commun, format!(r"HKLM:\{APPROUVE}\StartupFolder"), "Dossier Démarrage commun"),
        ];
        let mut l = Vec::new();
        for (reg, chemin, approuve, source) in sources {
            let Some(chemin) = chemin else { continue };
            let mut entrees: Vec<(String, String, String)> = Vec::new();
            if reg {
                for n in crate::windows::reg_noms(&chemin).unwrap_or_default().into_iter().filter(|n| !n.is_empty()) {
                    let c = crate::windows::reg_lire(&chemin, &n).ok().flatten().map(|v| v.texte()).unwrap_or_default();
                    entrees.push((n.clone(), n, c));
                }
            } else if let Ok(rd) = std::fs::read_dir(&chemin) {
                for e in rd.flatten() {
                    let p = e.path();
                    if !p.is_file() || e.file_name().to_string_lossy().eq_ignore_ascii_case("desktop.ini") {
                        continue;
                    }
                    let nom = e.file_name().to_string_lossy().to_string();
                    let base = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    entrees.push((nom, base, p.to_string_lossy().to_string()));
                }
            }
            for (nom, aff, cmd) in entrees {
                let Some(exe) = exe_de(&cmd) else { continue };
                l.push(Demarrage {
                    nom: aff,
                    source: source.into(),
                    commande: cmd,
                    approuve: approuve.clone(),
                    nom_valeur: nom,
                    actif: true,
                    exe,
                });
            }
        }
        l
    }

    /// `Resolve-StartupExe` (les raccourcis .lnk gardent leur propre chemin : leur cible n'est pas lue, voir DIVERGENCES.md)
    fn exe_de(commande: &str) -> Option<String> {
        if commande.trim().is_empty() {
            return None;
        }
        let mut cmd = commande.trim().to_string();
        for (k, val) in std::env::vars() {
            let motif = format!("%{k}%");
            if let Some(i) = cmd.to_lowercase().find(&motif.to_lowercase()) {
                cmd.replace_range(i..i + motif.len(), &val);
            }
        }
        let exe = if let Some(r) = cmd.strip_prefix('"') {
            r.split('"').next().unwrap_or("").to_string()
        } else {
            let re = regex::Regex::new(r"(?i)^(.+?\.(exe|lnk|bat|cmd|url))(\s|$)").unwrap();
            re.captures(&cmd).map(|c| c[1].to_string()).unwrap_or(cmd.clone())
        };
        let exe = if !exe.contains('\\') && !exe.contains('/') {
            format!(r"{}\System32\{exe}", std::env::var("windir").unwrap_or_default())
        } else {
            exe
        };
        std::path::Path::new(&exe).is_file().then_some(exe)
    }
}
