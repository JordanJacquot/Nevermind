//! Registre : les seules valeurs que l'Aide écrit ou efface (`$AideRegles`, `ConvertTo-AideCle`, `Get-AideRegle`,
//! `ConvertTo-AideValeur`, aide-actions.ps1).

use crate::{exes, ps, texte};
use serde_json::Value;

/// Une ruche du registre
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ruche {
    /// HKEY_LOCAL_MACHINE
    Machine,
    /// HKEY_USERS
    Utilisateurs,
}

/// Le type d'une valeur
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    DWord,
    Binary,
}

impl Type {
    pub fn nom(self) -> &'static str {
        match self {
            Type::DWord => "DWord",
            Type::Binary => "Binary",
        }
    }
}

/// Une valeur à écrire
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Valeur {
    DWord(i32),
    Binaire(Vec<u8>),
}

/// La clé visée par une règle
enum CleRegle {
    Exacte(&'static str),
    /// Image File Execution Options\<programme>.exe\PerfOptions
    PerfOptions,
    /// Explorer\StartupApproved\(Run|Run32|StartupFolder)
    Demarrage,
}

/// Une règle : la clé, le nom de la valeur (`None` : tout nom de 1 à 260 caractères sans caractère de contrôle), le
/// type, les valeurs permises, si l'Aide peut l'effacer, et pourquoi seule une valeur sûre passe (`garde`)
pub struct Regle {
    cle: CleRegle,
    nom: Option<&'static str>,
    pub type_valeur: Type,
    valeurs: &'static [i64],
    min_max: Option<(i64, i64)>,
    premiers: &'static [u8],
    pub effacer: bool,
    pub garde: Option<&'static str>,
}

impl Regle {
    /// Les programmes lancés avec Windows (StartupApproved)
    pub fn demarrage(&self) -> bool {
        matches!(self.cle, CleRegle::Demarrage)
    }
}

const IFEO: &str = "HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options\\";
const STARTUP: &str = "HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\";

const fn dword(cle: &'static str, nom: &'static str, valeurs: &'static [i64], min_max: Option<(i64, i64)>) -> Regle {
    Regle {
        cle: CleRegle::Exacte(cle),
        nom: Some(nom),
        type_valeur: Type::DWord,
        valeurs,
        min_max,
        premiers: &[],
        effacer: true,
        garde: None,
    }
}

/// La liste, dans l'ordre où elle est essayée
static REGLES: [Regle; 11] = [
    dword("HKLM:\\SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers", "HwSchMode", &[1, 2], None),
    dword(
        "HKU:\\S-1-5-20\\Software\\Microsoft\\Windows\\CurrentVersion\\DeliveryOptimization\\Settings",
        "DownloadMode",
        &[0, 1, 2, 3],
        None,
    ),
    dword(
        "HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\DeliveryOptimization",
        "DOPercentageMaxBackgroundBandwidth",
        &[],
        Some((0, 100)),
    ),
    dword(
        "HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\DeliveryOptimization",
        "DODownloadMode",
        &[0, 1, 2, 3, 99, 100],
        None,
    ),
    dword(
        "HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\DataCollection",
        "AllowTelemetry",
        &[],
        Some((0, 3)),
    ),
    dword("HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\DataCollection", "AllowTelemetry", &[], Some((0, 3))),
    dword(
        "HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\SystemRestore",
        "SystemRestorePointCreationFrequency",
        &[],
        Some((0, 100000)),
    ),
    Regle {
        cle: CleRegle::Exacte("HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Terminal Server"),
        nom: Some("fDenyTSConnections"),
        type_valeur: Type::DWord,
        valeurs: &[1],
        min_max: None,
        premiers: &[],
        effacer: false,
        garde: Some("le Bureau à distance reste coupé : l'Aide ne le rallume jamais"),
    },
    Regle {
        cle: CleRegle::Exacte("HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System"),
        nom: Some("EnableLUA"),
        type_valeur: Type::DWord,
        valeurs: &[1],
        min_max: None,
        premiers: &[],
        effacer: false,
        garde: Some("le contrôle des comptes reste allumé : l'Aide ne le coupe jamais"),
    },
    // Profils par jeu : priorité du processeur (PerfOptions seulement). Jamais 4 : « temps réel », qui peut figer le PC.
    Regle {
        cle: CleRegle::PerfOptions,
        nom: Some("CpuPriorityClass"),
        type_valeur: Type::DWord,
        valeurs: &[1, 2, 3, 5, 6],
        min_max: None,
        premiers: &[],
        effacer: true,
        garde: None,
    },
    // Programmes lancés avec Windows pour tous les utilisateurs : seulement les marquer actifs ou coupés
    Regle {
        cle: CleRegle::Demarrage,
        nom: None,
        type_valeur: Type::Binary,
        valeurs: &[],
        min_max: None,
        premiers: &[2, 3, 6, 7],
        effacer: true,
        garde: None,
    },
];

/// Le chemin de clé sous une seule forme : « HKLM:\... » ou « HKU:\... » (`ConvertTo-AideCle`)
pub fn cle_normale(chemin: &str) -> Result<String, String> {
    let refus = || format!("Demande refusée : clé de registre non valable ({chemin}).");
    let mut c = texte::sans_espaces(chemin);
    if let Some(r) = texte::reste_apres(c, "Registry::") {
        c = r;
    }
    let mut n = None;
    for (debuts, ruche) in
        [(["HKEY_LOCAL_MACHINE\\", "HKLM:\\", "HKLM\\"], "HKLM:\\"), (["HKEY_USERS\\", "HKU:\\", "HKU\\"], "HKU:\\")]
    {
        for d in debuts {
            if let Some(r) = texte::reste_apres(c, d) {
                n = Some(format!("{ruche}{r}"));
                break;
            }
        }
        if n.is_some() {
            break;
        }
    }
    let n = n.ok_or_else(refus)?;
    if n.contains("\\\\") || n.ends_with('\\') || n.chars().any(|x| (x as u32) < 0x20 || x == '*' || x == '?') {
        return Err(refus());
    }
    Ok(n)
}

/// La ruche et la sous-clé d'un chemin déjà normalisé
pub fn decouper(cle: &str) -> Option<(Ruche, &str)> {
    if let Some(r) = cle.strip_prefix("HKLM:\\") {
        Some((Ruche::Machine, r))
    } else {
        cle.strip_prefix("HKU:\\").map(|r| (Ruche::Utilisateurs, r))
    }
}

/// Le programme d'une clé PerfOptions (`[\w .()+\-]{1,100}\.exe`), ou `None`
fn programme_perf(cle: &str) -> Option<&str> {
    let r = texte::reste_apres(cle, IFEO)?;
    if !texte::finit(r, "\\PerfOptions") {
        return None;
    }
    let exe = &r[..r.len() - "\\PerfOptions".len()];
    if !texte::finit(exe, ".exe") {
        return None;
    }
    let debut = &exe[..exe.len() - 4];
    let n = debut.chars().count();
    let permis = |c: char| texte::lettre_de_mot(c) || " .()+-".contains(c);
    ((1..=100).contains(&n) && debut.chars().all(permis)).then_some(exe)
}

/// La règle qui couvre cette valeur (`Get-AideRegle`), ou le refus
pub fn regle(cle: &str, nom: &str) -> Result<&'static Regle, String> {
    for r in REGLES.iter() {
        let mut exe = None;
        let cle_ok = match r.cle {
            CleRegle::Exacte(c) => texte::egal(cle, c),
            CleRegle::PerfOptions => {
                exe = programme_perf(cle);
                exe.is_some()
            }
            CleRegle::Demarrage => texte::reste_apres(cle, STARTUP)
                .is_some_and(|f| ["Run", "Run32", "StartupFolder"].iter().any(|x| texte::egal(f, x))),
        };
        if !cle_ok {
            continue;
        }
        let nom_ok = match r.nom {
            Some(n) => texte::egal(nom, n),
            None => (1..=260).contains(&nom.chars().count()) && !texte::a_controle(nom),
        };
        if !nom_ok {
            continue;
        }
        if let Some(e) = exe
            && exes::protege(e)
        {
            return Err(format!("Demande refusée : {e} est un programme de Windows ou de l'antivirus."));
        }
        return Ok(r);
    }
    Err(format!("Demande refusée : {cle}\\{nom} n'est pas dans la liste de l'Aide."))
}

/// Un octet d'une valeur binaire (`[byte]` de PowerShell), ou `None`
fn octet(v: &Value) -> Option<u8> {
    let f = match v {
        Value::Null => return Some(0),
        Value::Bool(b) => return Some(u8::from(*b)),
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => {
            let t = texte::sans_espaces(s);
            if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                return u8::from_str_radix(h, 16).ok();
            }
            return u8::try_from(ps::lire_entier(t)?).ok();
        }
        _ => return None,
    };
    let r = f.round();
    let r = if (f - f.trunc()).abs() == 0.5 && r % 2.0 != 0.0 { r - f.signum() } else { r };
    (0.0..=255.0).contains(&r).then_some(r as u8)
}

/// La valeur demandée, si la règle la permet (`ConvertTo-AideValeur`)
pub fn valeur(r: &Regle, v: Option<&Value>, type_demande: &str) -> Result<Valeur, String> {
    if !texte::egal(type_demande, r.type_valeur.nom()) {
        return Err(format!("Demande refusée : type {type_demande} au lieu de {}.", r.type_valeur.nom()));
    }
    let refus = || match r.garde {
        Some(g) => format!("L'Aide garde ce réglage : {g}."),
        None => "Demande refusée : valeur hors des valeurs permises.".to_string(),
    };
    match r.type_valeur {
        Type::DWord => {
            let n = match v {
                None | Some(Value::Null) | Some(Value::Array(_)) => return Err(refus()),
                Some(x) => ps::lire_entier(&ps::chaine(Some(x))).ok_or_else(refus)?,
            };
            if !r.valeurs.is_empty() && !r.valeurs.contains(&n) {
                return Err(refus());
            }
            if let Some((min, max)) = r.min_max
                && (n < min || n > max)
            {
                return Err(refus());
            }
            Ok(Valeur::DWord(i32::try_from(n).map_err(|_| refus())?))
        }
        Type::Binary => {
            let elements: Vec<&Value> = match v {
                Some(Value::Array(a)) => a.iter().collect(),
                Some(x) => vec![x],
                None => vec![&Value::Null],
            };
            let o: Option<Vec<u8>> = elements.into_iter().map(octet).collect();
            let o = o.ok_or_else(refus)?;
            if o.len() != 12 || !r.premiers.contains(&o[0]) {
                return Err(refus());
            }
            Ok(Valeur::Binaire(o))
        }
    }
}
