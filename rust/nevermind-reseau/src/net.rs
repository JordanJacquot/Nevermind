//! La connexion active (`Get-ActiveNet`, `$ActiveNetWork` d'interface.ps1) et la carte réseau bridée
//! (Get-CapaciteCarte, Get-BridageCarte, Format-VitesseCarte, Get-TexteBridage, Test-AlerteCarte de systeme.ps1).

use crate::texte::{arrondi_pair, chaine, entier, format_0_diese, liste, re, reel, vrai};
use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;

/// La connexion réseau active (la carte de la route par défaut)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Connexion {
    pub if_index: i64,
    pub gateway: String,
    pub nom: String,
    pub desc: String,
    /// La vitesse comme Windows l'écrit (« 1 Gbps »)
    pub speed: String,
    pub wifi: bool,
    pub guid: String,
    pub mac: String,
    pub ip: Option<String>,
    pub prefix: i64,
    pub dns: String,
    /// La vitesse de la liaison en bits/s
    pub bps: f64,
    /// Les vitesses que la carte sait faire (textes du réglage « Vitesse et duplex »)
    pub vitesses: Vec<String>,
    /// Les mêmes, en valeurs du registre
    pub vitesses_reg: Vec<String>,
}

impl Connexion {
    /// Depuis la table de PowerShell (juges) ; `None` pour `$null`
    pub fn depuis_json(v: &Value) -> Option<Self> {
        let o = v.as_object()?;
        let g = |k: &str| o.get(k);
        Some(Connexion {
            if_index: reel(g("IfIndex")) as i64,
            gateway: chaine(g("Gateway")),
            nom: chaine(g("Name")),
            desc: chaine(g("Desc")),
            speed: chaine(g("Speed")),
            wifi: vrai(g("Wifi")),
            guid: chaine(g("Guid")),
            mac: chaine(g("Mac")),
            ip: match g("Ip") {
                None | Some(Value::Null) => None,
                x => Some(chaine(x)),
            },
            prefix: match g("Prefix") {
                None | Some(Value::Null) => 24,
                x => reel(x) as i64,
            },
            dns: chaine(g("Dns")),
            bps: reel(g("Bps")),
            vitesses: liste(g("Vitesses")),
            vitesses_reg: liste(g("VitessesReg")),
        })
    }

    pub fn json(&self) -> Value {
        json!({ "IfIndex": self.if_index, "Gateway": self.gateway, "Name": self.nom, "Desc": self.desc, "Speed": self.speed,
            "Wifi": self.wifi, "Guid": self.guid, "Mac": self.mac, "Ip": self.ip, "Prefix": self.prefix, "Dns": self.dns,
            "Bps": self.bps, "Vitesses": self.vitesses, "VitessesReg": self.vitesses_reg })
    }
}

static R_VITESSE: LazyLock<Regex> = LazyLock::new(|| re(r"(\d+(?:[.,]\d+)?)\s*([MG])(?:bit|bps|b/s|b\b)"));

/// La plus grande vitesse que la carte sait faire, en bits/s (`Get-CapaciteCarte`, 0 si on ne sait pas)
pub fn capacite(n: &Connexion) -> f64 {
    let mut max = 0.0f64;
    for t in &n.vitesses {
        if let Some(c) = R_VITESSE.captures(t) {
            let v: f64 =
                c[1].replace(',', ".").parse().unwrap_or(0.0) * if c[2].eq_ignore_ascii_case("G") { 1e9 } else { 1e6 };
            if v > max {
                max = v;
            }
        }
    }
    if max > 0.0 {
        return max;
    }
    for r in &n.vitesses_reg {
        let Some(k) = entier(r) else { continue };
        let v = match k {
            1 | 2 => 1e7,
            3 | 4 => 1e8,
            5 | 6 => 1e9,
            7 => 1e10,
            k if k >= 1000 => k as f64 * 1e6,
            _ => 0.0,
        };
        if v > max {
            max = v;
        }
    }
    max
}

/// Une carte par câble branchée en dessous de 1 Gb/s alors qu'elle sait le faire (`Get-BridageCarte`)
#[derive(Clone, Debug, PartialEq)]
pub struct Bridage {
    pub bps: f64,
    pub capacite: f64,
    pub fois: i64,
}

pub fn bridage(n: Option<&Connexion>) -> Option<Bridage> {
    let n = n?;
    if n.wifi {
        return None;
    }
    if n.bps <= 0.0 || n.bps >= 1e9 {
        return None;
    }
    let cap = capacite(n);
    if cap < 1e9 {
        return None;
    }
    Some(Bridage { bps: n.bps, capacite: cap, fois: arrondi_pair(1e9 / n.bps) })
}

/// « 100 Mb/s », « 2,5 Gb/s » (`Format-VitesseCarte`)
pub fn format_vitesse(bps: f64) -> String {
    if bps >= 1e9 {
        format!("{} Gb/s", format_0_diese(bps / 1e9))
    } else {
        format!("{} Mb/s", format_0_diese(bps / 1e6))
    }
}

/// L'explication de la carte bridée (`Get-TexteBridage`)
pub fn texte_bridage(b: &Bridage) -> String {
    format!(
        "Ta carte réseau peut aller à 1 Gb/s mais elle est branchée à {} : ta connexion est {} plus lente qu'elle ne pourrait. Ça arrive souvent après une veille.",
        format_vitesse(b.bps),
        if b.fois >= 2 { format!("{} fois", b.fois) } else { "un peu".into() }
    )
}

/// La bulle de la carte bridée : une seule par épisode (`Test-AlerteCarte`). `alertee` : la carte déjà signalée.
/// Rend la ligne du journal et la bulle quand il faut la montrer.
pub fn alerte_carte(
    n: Option<&Connexion>,
    alertee: &mut Option<String>,
    en_partie: bool,
) -> Option<(String, crate::scan::Bulle)> {
    let Some(b) = bridage(n) else {
        *alertee = None;
        return None;
    };
    let n = n?;
    let cle = format!("{}|{}", n.guid, n.if_index);
    if alertee.as_deref() == Some(cle.as_str()) || en_partie {
        return None;
    }
    *alertee = Some(cle);
    Some((
        format!(
            "Carte réseau bridée : {} à {}, capable de {}",
            n.desc,
            format_vitesse(b.bps),
            format_vitesse(b.capacite)
        ),
        crate::scan::Bulle {
            titre: "Ta connexion par câble est ralentie".into(),
            texte: format!(
                "Ta carte réseau est branchée à {} au lieu de 1 Gb/s. Clique ici pour la remettre à pleine vitesse.",
                format_vitesse(b.bps)
            ),
        },
    ))
}
