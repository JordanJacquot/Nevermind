//! L'audit de sécurité du réseau (audit-reseau.ps1) : les points à partir de ce que l'audit a relevé
//! (Get-NetAuditChecks), la note, la carte de la page Réseau (Update-NetAuditCard) et le rapport exporté.

use crate::appareil::Appareil;
use crate::texte::{chaine, egal, entier, liste, part, parts_n, pluriel, re, vrai};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::sync::LazyLock;

/// Les ports vérifiés sur les appareils (`$AuditPorts`)
pub const PORTS: [i64; 9] = [21, 23, 80, 139, 443, 445, 554, 3389, 5900];
/// Les bons réflexes (`$AuditTips`)
pub const CONSEILS: [&str; 4] = [
    "Si beaucoup de monde connaît le mot de passe de ton Wi-Fi, change-le de temps en temps depuis la page de ta box.",
    "Crée un Wi-Fi invité (souvent proposé par la box) pour tes visiteurs et tes objets connectés.",
    "Change le mot de passe par défaut des caméras, NAS et imprimantes réseau.",
    "Laisse les mises à jour automatiques activées sur ta box, ton PC et tes appareils.",
];

/// La rubrique d'un point (`$AuditCats`)
pub fn rubrique(c: &str) -> &'static str {
    match c {
        "wifi" => "Wi-Fi",
        "box" => "Box",
        "dns" => "DNS",
        "devices" => "Appareils",
        "pc" => "Ce PC",
        _ => "",
    }
}

/// Ce que fait le bouton d'un point
#[derive(Clone, Debug, PartialEq)]
pub enum Geste {
    /// Ouvrir l'adresse de `arg` (la box, un routeur)
    Ouvrir,
    /// La page Connexion (pour changer de DNS)
    ChangerDns,
    /// Les Wi-Fi ouverts en connexion manuelle (Aide « wifi.manuel »)
    WifiManuel,
    /// Réactiver le pare-feu (Aide « pare-feu.activer »)
    PareFeu,
    /// Couper SMBv1 (Aide « smb1.couper »)
    Smb1,
    /// Couper le Bureau à distance (registre, par l'Aide)
    BureauDistance,
    /// Passer le réseau en public
    Public,
    /// Ouvrir la gestion des partages (fsmgmt.msc)
    Partages,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActionAudit {
    pub label: String,
    pub arg: Value,
    pub confirmer: String,
    pub sans_maj: bool,
    /// Relancer l'audit après le geste
    pub apres: bool,
    pub geste: Geste,
}

impl ActionAudit {
    fn new(label: &str, geste: Geste) -> Self {
        ActionAudit {
            label: label.into(),
            arg: Value::Null,
            confirmer: String::new(),
            sans_maj: false,
            apres: false,
            geste,
        }
    }
    pub fn json(&self) -> Value {
        json!({ "Label": self.label, "Arg": self.arg, "Confirm": self.confirmer, "NoRefresh": self.sans_maj, "After": self.apres })
    }
}

/// Un point de l'audit
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub statut: String,
    pub cat: String,
    pub titre: String,
    pub detail: String,
    pub elements: Vec<String>,
    pub actions: Vec<ActionAudit>,
}

impl Point {
    pub fn json(&self) -> Value {
        json!({ "Status": self.statut, "Cat": self.cat, "Title": self.titre, "Detail": self.detail, "Items": self.elements,
            "Actions": self.actions.iter().map(ActionAudit::json).collect::<Vec<_>>() })
    }
}

fn ajout(
    l: &mut Vec<Point>,
    statut: &str,
    cat: &str,
    titre: &str,
    detail: &str,
    el: Vec<String>,
    act: Vec<ActionAudit>,
) {
    l.push(Point {
        statut: statut.into(),
        cat: cat.into(),
        titre: titre.into(),
        detail: detail.into(),
        elements: el.into_iter().filter(|x| !x.is_empty()).collect(),
        actions: act,
    });
}

static R_OUVERT: LazyLock<Regex> = LazyLock::new(|| re("^(ouvrir|ouvert|open)"));
static R_WPA3: LazyLock<Regex> = LazyLock::new(|| re("WPA3"));
static R_WPA2: LazyLock<Regex> = LazyLock::new(|| re("WPA2"));
static R_WPA: LazyLock<Regex> = LazyLock::new(|| re("WPA"));
static R_WEP: LazyLock<Regex> = LazyLock::new(|| re("WEP"));
static R_TKIP: LazyLock<Regex> = LazyLock::new(|| re("TKIP"));
static R_WPS: LazyLock<Regex> = LazyLock::new(|| re("wifialliance-org:(device:WFADevice|service:WFAWLANConfig)"));
static R_NAT: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.|100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\.)"));
static R_DNS_LOCAL: LazyLock<Regex> = LazyLock::new(|| re(r"^(fe80:|192\.168\.|10\.|172\.)"));

fn sensible(port: i64) -> Option<&'static str> {
    Some(match port {
        21 => "FTP",
        22 => "SSH",
        23 => "Telnet",
        80 => "page web",
        139 | 445 => "partage Windows",
        554 => "caméra",
        3389 => "Bureau à distance",
        5900 => "VNC",
        8080 => "page web",
        _ => return None,
    })
}

fn dns_connu(s: &str) -> Option<&'static str> {
    Some(match s {
        "1.1.1.1" | "1.0.0.1" => "Cloudflare",
        "8.8.8.8" | "8.8.4.4" => "Google",
        "9.9.9.9" | "149.112.112.112" => "Quad9",
        "208.67.222.222" | "208.67.220.220" => "OpenDNS",
        "94.140.14.14" | "94.140.15.15" => "AdGuard",
        _ => return None,
    })
}

/// `Select-Object -Unique` (sensible aux majuscules), dans l'ordre
fn uniques(v: Vec<String>) -> Vec<String> {
    let mut r: Vec<String> = Vec::new();
    for x in v {
        if !r.contains(&x) {
            r.push(x);
        }
    }
    r
}

const CONFIRMER_DEFINITIF: &str = "Ce changement protège ton PC : « Tout restaurer » ne l'annulera pas (la façon de revenir en arrière est notée dans l'historique).";

/// Les points de l'audit (`Get-NetAuditChecks`). `r` : ce que l'audit a relevé (la table de `$NetAuditWork`).
pub fn points(r: &Value, devs: &[Appareil], passerelle: &str, if_index: &Value) -> Vec<Point> {
    let mut l = Vec::new();
    let gw = passerelle.to_string();
    let box_act = || ActionAudit {
        arg: json!(format!("http://{gw}")),
        sans_maj: true,
        ..ActionAudit::new("Ouvrir la box", Geste::Ouvrir)
    };
    let dns_act = || ActionAudit { sans_maj: true, ..ActionAudit::new("Changer de DNS", Geste::ChangerDns) };
    let par_ip = |ip: &str| devs.iter().rev().find(|d| egal(&d.ip, ip));
    let nom = |ip: &str| match par_ip(ip) {
        Some(d) => format!("{} ({ip})", d.title),
        None => ip.to_string(),
    };
    let g = |o: &Value, k: &str| o.get(k).cloned().unwrap_or(Value::Null);

    // Wi-Fi
    let w = r.get("Wifi").filter(|x| vrai(Some(x)));
    let mut auth = String::new();
    if let Some(w) = w {
        auth = chaine(w.get("Auth"));
        let ciph = chaine(w.get("Cipher"));
        let ssid = chaine(w.get("Ssid"));
        if R_WPA3.is_match(&auth) {
            ajout(
                &mut l,
                "ok",
                "wifi",
                &format!("Wi-Fi « {ssid} » protégé en WPA3"),
                "Le meilleur niveau de protection actuel.",
                vec![],
                vec![],
            );
        } else if R_WPA2.is_match(&auth) && R_TKIP.is_match(&ciph) {
            ajout(
                &mut l,
                "warn",
                "wifi",
                &format!("Wi-Fi « {ssid} » : chiffrement ancien (TKIP)"),
                "Le WPA2 est bon, mais le chiffrement TKIP est dépassé et ralentit le Wi-Fi. Choisis « WPA2 (AES) » ou « WPA2/WPA3 » dans les réglages Wi-Fi de ta box.",
                vec![],
                vec![box_act()],
            );
        } else if R_WPA2.is_match(&auth) {
            ajout(
                &mut l,
                "ok",
                "wifi",
                &format!("Wi-Fi « {ssid} » protégé en WPA2"),
                "Bonne protection. Si ta box propose le mode WPA2/WPA3, il est encore plus sûr.",
                vec![],
                vec![],
            );
        } else if R_WPA.is_match(&auth) {
            ajout(
                &mut l,
                "bad",
                "wifi",
                &format!("Wi-Fi « {ssid} » : protection dépassée (WPA)"),
                "Cette ancienne méthode se casse facilement. Passe en WPA2 ou WPA3 dans les réglages Wi-Fi de ta box.",
                vec![],
                vec![box_act()],
            );
        } else if R_WEP.is_match(&auth) || R_WEP.is_match(&ciph) {
            ajout(
                &mut l,
                "bad",
                "wifi",
                &format!("Wi-Fi « {ssid} » protégé en WEP"),
                "Le WEP se casse en quelques minutes : n'importe qui à portée peut entrer sur ton réseau. Passe en WPA2 ou WPA3 dans les réglages Wi-Fi de ta box.",
                vec![],
                vec![box_act()],
            );
        } else if R_OUVERT.is_match(&auth) {
            ajout(
                &mut l,
                "bad",
                "wifi",
                &format!("Wi-Fi « {ssid} » sans mot de passe"),
                "Tout le monde à portée peut se connecter et voir ce qui passe en clair. Si c'est ton Wi-Fi, mets un mot de passe WPA2 ou WPA3 depuis la page de ta box. Si c'est un Wi-Fi public, évite les sites sensibles.",
                vec![],
                vec![box_act()],
            );
        } else {
            ajout(
                &mut l,
                "info",
                "wifi",
                &format!("Wi-Fi « {ssid} » : protection non reconnue ({auth})"),
                "Vérifie dans la page de ta box que le Wi-Fi est en WPA2 ou WPA3.",
                vec![],
                vec![box_act()],
            );
        }
    } else {
        ajout(
            &mut l,
            "info",
            "wifi",
            "Ce PC est branché par câble",
            "La protection du Wi-Fi ne peut pas être lue depuis ce PC. Vérifie dans la page de ta box qu'il est en WPA2 ou WPA3.",
            vec![],
            vec![box_act()],
        );
    }
    let profils: Vec<(String, bool, bool)> = liste(r.get("Profiles"))
        .iter()
        .filter_map(|x| {
            let p: Vec<String> = x.split('|').map(str::to_string).collect();
            (p.len() >= 5).then(|| (p[0].clone(), p[3] == "1", p[4] == "1"))
        })
        .collect();
    let ouverts: Vec<String> = profils.iter().filter(|p| p.1 && p.2).map(|p| p.0.clone()).collect();
    if !ouverts.is_empty() {
        let a = ActionAudit {
            arg: json!(ouverts),
            apres: true,
            confirmer: format!(
                "Ces Wi-Fi ne se connecteront plus automatiquement. Tu pourras toujours t'y connecter en cliquant dessus. {CONFIRMER_DEFINITIF} Continuer ?"
            ),
            ..ActionAudit::new("Passer en connexion manuelle", Geste::WifiManuel)
        };
        ajout(
            &mut l,
            "warn",
            "wifi",
            &format!("Connexion automatique à {}", pluriel(ouverts.len(), "Wi-Fi ouvert", "Wi-Fi ouverts")),
            "Ton PC se reconnecte tout seul à ces réseaux sans protection. Un pirate peut créer un faux Wi-Fi avec le même nom pour espionner ta connexion.",
            ouverts.clone(),
            vec![a],
        );
    } else if !profils.is_empty() {
        ajout(
            &mut l,
            "ok",
            "wifi",
            "Aucun Wi-Fi ouvert en connexion automatique",
            &format!(
                "{} sur ce PC, tous protégés ou en connexion manuelle.",
                pluriel(profils.len(), "Wi-Fi mémorisé", "Wi-Fi mémorisés")
            ),
            vec![],
            vec![],
        );
    }

    // Box : WPS, UPnP, double NAT, page de réglages
    let ssdp: Vec<(String, String)> = liste(r.get("Ssdp"))
        .iter()
        .map(|x| {
            let p = parts_n(x, 3);
            (part(&p, 0).to_string(), part(&p, 1).to_string())
        })
        .collect();
    let wps = uniques(ssdp.iter().filter(|(_, st)| R_WPS.is_match(st)).map(|(ip, _)| ip.clone()).collect());
    if !wps.is_empty() {
        let titre = if wps.len() == 1 && egal(&wps[0], &gw) {
            "Le WPS semble activé sur ta box".to_string()
        } else {
            format!("Le WPS semble activé sur {}", pluriel(wps.len(), "appareil", "appareils"))
        };
        let actes: Vec<ActionAudit> = wps
            .iter()
            .take(3)
            .map(|ip| {
                if egal(ip, &gw) {
                    box_act()
                } else {
                    let qui = match par_ip(ip) {
                        Some(d) if !d.title.is_empty() && !egal(&d.title, "Appareil inconnu") => d.title.clone(),
                        _ => ip.clone(),
                    };
                    ActionAudit {
                        arg: json!(format!("http://{ip}")),
                        sans_maj: true,
                        ..ActionAudit::new(&format!("Ouvrir {qui}"), Geste::Ouvrir)
                    }
                }
            })
            .collect();
        ajout(
            &mut l,
            "warn",
            "box",
            &titre,
            "Le WPS permet de connecter un appareil en appuyant sur un bouton, mais son code PIN est une faille connue qui permet de trouver le mot de passe du Wi-Fi. Si tu ne t'en sers pas, désactive-le dans les réglages Wi-Fi de ta box et de tes répéteurs.",
            wps.iter().map(|ip| nom(ip)).collect(),
            actes,
        );
    } else {
        ajout(
            &mut l,
            "ok",
            "box",
            "WPS non annoncé sur le réseau",
            "Aucun appareil ne propose la connexion par code PIN WPS.",
            vec![],
            vec![],
        );
    }
    let igd = r.get("Igd").filter(|x| vrai(Some(x)));
    if let Some(igd) = igd {
        struct Regle {
            ext: String,
            proto: String,
            int: i64,
            client: String,
            desc: String,
        }
        let regles: Vec<Regle> = liste(igd.get("Maps"))
            .iter()
            .filter_map(|m| {
                let x = parts_n(m, 5);
                (x.len() >= 5).then(|| Regle {
                    ext: x[0].clone(),
                    proto: x[1].clone(),
                    int: entier(&x[2]).unwrap_or(0),
                    client: x[3].clone(),
                    desc: x[4].clone(),
                })
            })
            .collect();
        let texte_regle = |m: &Regle| {
            format!(
                "Port {} ({}) vers {}{}",
                m.ext,
                m.proto,
                nom(&m.client),
                if m.desc.is_empty() { String::new() } else { format!(" : {}", m.desc) }
            )
        };
        let danger: Vec<&Regle> = regles.iter().filter(|m| sensible(m.int).is_some()).collect();
        let sures: Vec<&Regle> = regles.iter().filter(|m| sensible(m.int).is_none()).collect();
        if !danger.is_empty() {
            ajout(
                &mut l,
                "bad",
                "box",
                &format!(
                    "{} sur Internet",
                    pluriel(danger.len(), "service sensible ouvert", "services sensibles ouverts")
                ),
                "Ces règles, créées automatiquement par un appareil (UPnP), rendent un service sensible joignable depuis tout Internet : n'importe qui peut essayer de s'y connecter. Supprime-les dans la page de ta box (rubrique NAT, PAT ou UPnP) si tu ne sais pas à quoi elles servent.",
                danger.iter().map(|m| format!("{} ({})", texte_regle(m), sensible(m.int).unwrap_or(""))).collect(),
                vec![box_act()],
            );
        }
        if !sures.is_empty() || regles.is_empty() {
            let titre = if !sures.is_empty() {
                format!("UPnP activé : {} automatiquement", pluriel(sures.len(), "port ouvert", "ports ouverts"))
            } else {
                "UPnP activé, aucun port ouvert".to_string()
            };
            ajout(
                &mut l,
                "ok",
                "box",
                &titre,
                "L'UPnP permet à tes jeux et consoles d'ouvrir les ports dont ils ont besoin (NAT ouvert). Rien de sensible dans ces règles.",
                sures.iter().map(|m| texte_regle(m)).collect(),
                vec![],
            );
        }
        let ext = chaine(igd.get("External"));
        if R_NAT.is_match(&ext) {
            ajout(
                &mut l,
                "info",
                "box",
                "Double NAT détecté",
                &format!(
                    "Ta box n'a pas directement une adresse Internet ({ext}) : elle est derrière un autre routeur, ou ton opérateur partage l'adresse entre plusieurs clients. En jeu, ça peut donner un « NAT strict » et des soucis pour héberger une partie."
                ),
                vec![],
                vec![],
            );
        }
    } else {
        ajout(
            &mut l,
            "ok",
            "box",
            "UPnP non détecté sur ta box",
            "Aucun appareil ne peut ouvrir de port vers Internet tout seul. Si tu as un « NAT strict » en jeu, activer l'UPnP dans ta box peut aider.",
            vec![],
            vec![],
        );
    }
    // Ports ouverts, par appareil (dans l'ordre où l'audit les a trouvés)
    let mut ouverts_par: Vec<(String, Vec<i64>)> = Vec::new();
    for x in liste(r.get("Ports")) {
        let p: Vec<String> = x.split('|').map(str::to_string).collect();
        let port = entier(part(&p, 1)).unwrap_or(0);
        match ouverts_par.iter_mut().find(|(ip, _)| egal(ip, part(&p, 0))) {
            Some(e) => e.1.push(port),
            None => ouverts_par.push((part(&p, 0).to_string(), vec![port])),
        }
    }
    let ports_gw: Vec<i64> =
        ouverts_par.iter().find(|(ip, _)| egal(ip, &gw)).map(|(_, v)| v.clone()).unwrap_or_default();
    if ports_gw.contains(&80) && !ports_gw.contains(&443) {
        ajout(
            &mut l,
            "info",
            "box",
            "Page de réglages de la box non chiffrée",
            "La page de ta box est en http simple : son mot de passe passe en clair sur ton réseau. Pas grave tant que seules des personnes de confiance sont connectées chez toi.",
            vec![],
            vec![box_act()],
        );
    }

    // DNS
    let dns = g(r, "Dns");
    let serveurs: Vec<String> = uniques(liste(dns.get("Servers")).into_iter().filter(|s| !s.is_empty()).collect())
        .into_iter()
        .map(|s| {
            let lbl = if egal(&s, &gw) {
                "ta box".to_string()
            } else if let Some(n) = dns_connu(&s.to_lowercase()) {
                n.to_string()
            } else if R_DNS_LOCAL.is_match(&s) {
                "réseau local".to_string()
            } else {
                "fournisseur d'accès ou autre".to_string()
            };
            format!("Serveur {s} ({lbl})")
        })
        .collect();
    let detours = liste(dns.get("Redirect"));
    if !detours.is_empty() {
        let mut el = detours.clone();
        el.extend(serveurs.clone());
        ajout(
            &mut l,
            "bad",
            "dns",
            "Tes recherches Internet sont détournées",
            "Des adresses connues ne donnent pas la bonne réponse : ton serveur DNS peut t'envoyer vers de faux sites. Si tu n'utilises pas de filtre (contrôle parental, bloqueur de pub), change de DNS et vérifie les réglages DNS de ta box.",
            el,
            vec![dns_act(), box_act()],
        );
    } else if !chaine(dns.get("NxHijack")).is_empty() {
        ajout(
            &mut l,
            "warn",
            "dns",
            "Ton DNS redirige les adresses qui n'existent pas",
            "Quand tu tapes une mauvaise adresse, tu arrives sur une page de pub au lieu d'une erreur. Pas dangereux, mais un DNS comme Cloudflare (1.1.1.1) évite ça et répond souvent plus vite.",
            serveurs.clone(),
            vec![dns_act()],
        );
    } else {
        ajout(
            &mut l,
            "ok",
            "dns",
            "DNS fiable",
            "Les réponses DNS sont correctes : pas de redirection vers de faux sites.",
            serveurs.clone(),
            vec![],
        );
    }

    // Appareils
    let (mut telnet, mut ftp, mut distance, mut cams) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (ip, ps) in &ouverts_par {
        let nm = nom(ip);
        if ps.contains(&23) {
            telnet.push(nm.clone());
        }
        if ps.contains(&21) {
            ftp.push(nm.clone());
        }
        if ps.contains(&3389) {
            distance.push(format!("{nm} : Bureau à distance"));
        }
        if ps.contains(&5900) {
            distance.push(format!("{nm} : VNC"));
        }
        if ps.contains(&554) {
            cams.push(nm.clone());
        }
    }
    let verifies = devs.iter().filter(|d| !d.soi).count();
    if !telnet.is_empty() {
        ajout(
            &mut l,
            "bad",
            "devices",
            &format!("Telnet ouvert sur {}", pluriel(telnet.len(), "appareil", "appareils")),
            "Telnet permet de prendre la main sur un appareil sans aucun chiffrement, et ces appareils ont souvent un mot de passe par défaut connu de tous. Désactive-le dans les réglages de l'appareil, ou mets à jour son logiciel.",
            telnet.clone(),
            vec![],
        );
    }
    if !ftp.is_empty() {
        ajout(
            &mut l,
            "warn",
            "devices",
            &format!("Transfert de fichiers FTP ouvert sur {}", pluriel(ftp.len(), "appareil", "appareils")),
            "Le FTP envoie les fichiers et les mots de passe en clair. Normal sur certains NAS ou box : si tu ne t'en sers pas, désactive-le dans les réglages de l'appareil.",
            ftp.clone(),
            vec![],
        );
    }
    if !distance.is_empty() {
        ajout(
            &mut l,
            "warn",
            "devices",
            "Prise de contrôle à distance ouverte",
            "Ces appareils acceptent qu'on prenne le contrôle de leur écran. Si ce n'est pas voulu, désactive-le dans leurs réglages, et utilise toujours un mot de passe fort.",
            distance.clone(),
            vec![],
        );
    }
    if !cams.is_empty() {
        ajout(
            &mut l,
            "info",
            "devices",
            &format!("Flux vidéo sur {}", pluriel(cams.len(), "appareil", "appareils")),
            "Souvent une caméra ou un décodeur TV. Vérifie que tes caméras sont protégées par un mot de passe que tu as choisi toi-même.",
            cams,
            vec![],
        );
    }
    if telnet.len() + ftp.len() + distance.len() == 0 {
        ajout(
            &mut l,
            "ok",
            "devices",
            &format!("Aucun service dangereux sur tes {}", pluriel(verifies, "appareil", "appareils")),
            "Pas de Telnet, de FTP ni de prise de contrôle à distance ouverts.",
            vec![],
            vec![],
        );
    }
    let texte_app = |d: &Appareil| {
        format!(
            "{} ({}){}",
            d.title,
            d.ip,
            if egal(&d.vendor, "Adresse privée") {
                ", adresse masquée".to_string()
            } else if !d.vendor.is_empty() {
                format!(", {}", d.vendor)
            } else {
                String::new()
            }
        )
    };
    let cameras: Vec<&Appareil> = devs.iter().filter(|d| d.camera).collect();
    if !cameras.is_empty() {
        ajout(
            &mut l,
            "warn",
            "devices",
            &format!("{} sur ton réseau", pluriel(cameras.len(), "caméra possible", "caméras possibles")),
            "Vérifie que tu sais à qui elles sont et où elles filment (utile aussi en location ou en colocation). Une caméra doit avoir un mot de passe que tu as choisi toi-même.",
            cameras.iter().map(|d| format!("{} : {}", texte_app(d), d.camera_why.join(", "))).collect(),
            vec![],
        );
    }
    let nouveaux: Vec<&Appareil> = devs.iter().filter(|d| d.new).collect();
    let inconnus: Vec<&Appareil> = devs.iter().filter(|d| egal(&d.title, "Appareil inconnu") && !d.new).collect();
    if !nouveaux.is_empty() {
        ajout(
            &mut l,
            "warn",
            "devices",
            &format!("{} depuis le dernier scan", pluriel(nouveaux.len(), "nouvel appareil", "nouveaux appareils")),
            "Si tu ne les reconnais pas, regarde la liste des appareils dans la page de ta box et change le mot de passe du Wi-Fi.",
            nouveaux.iter().map(|d| texte_app(d)).collect(),
            vec![box_act()],
        );
    }
    if !inconnus.is_empty() {
        ajout(
            &mut l,
            "info",
            "devices",
            &pluriel(inconnus.len(), "appareil non identifié", "appareils non identifiés"),
            "Leur nom et leur fabricant sont cachés : souvent des téléphones récents qui masquent leur adresse. Tu peux les retrouver dans la liste des appareils de ta box.",
            inconnus.iter().map(|d| texte_app(d)).collect(),
            vec![box_act()],
        );
    }
    if nouveaux.is_empty() && inconnus.is_empty() {
        ajout(
            &mut l,
            "ok",
            "devices",
            "Tous les appareils sont identifiés",
            "Aucun appareil inconnu ou nouveau sur ton réseau.",
            vec![],
            vec![],
        );
    }

    // Ce PC
    let pc = g(r, "Pc");
    let coupes = liste(pc.get("FirewallOff"));
    if !coupes.is_empty() {
        let a = ActionAudit {
            apres: true,
            confirmer: format!("Réactiver le pare-feu de Windows sur tous les réseaux ? {CONFIRMER_DEFINITIF}"),
            ..ActionAudit::new("Réactiver le pare-feu", Geste::PareFeu)
        };
        ajout(
            &mut l,
            "bad",
            "pc",
            "Pare-feu de Windows désactivé",
            &format!(
                "Il est coupé sur : {}. Ton PC accepte alors toutes les connexions venant du réseau.",
                coupes.join(", ")
            ),
            vec![],
            vec![a],
        );
    } else {
        ajout(
            &mut l,
            "ok",
            "pc",
            "Pare-feu de Windows activé",
            "Il bloque les connexions non voulues vers ce PC.",
            vec![],
            vec![],
        );
    }
    if vrai(pc.get("Smb1")) || vrai(pc.get("Smb1Client")) {
        let a = ActionAudit {
            arg: json!(vrai(pc.get("Smb1Client"))),
            apres: true,
            confirmer: format!(
                "Désactiver SMBv1 ? Un redémarrage peut être nécessaire. Seuls de très vieux appareils (NAS ou imprimantes d'avant 2010) pourraient ne plus accéder à ce PC. {CONFIRMER_DEFINITIF}"
            ),
            ..ActionAudit::new("Désactiver SMBv1", Geste::Smb1)
        };
        ajout(
            &mut l,
            "bad",
            "pc",
            "Ancien partage de fichiers SMBv1 activé",
            "Cette vieille version du partage de fichiers Windows est la faille utilisée par le virus WannaCry. Plus aucun appareil récent n'en a besoin.",
            vec![],
            vec![a],
        );
    } else {
        ajout(
            &mut l,
            "ok",
            "pc",
            "SMBv1 désactivé",
            "L'ancienne version du partage de fichiers (faille WannaCry) est coupée.",
            vec![],
            vec![],
        );
    }
    if vrai(pc.get("Rdp")) {
        let a = ActionAudit {
            apres: true,
            confirmer: "Désactiver le Bureau à distance ? Tu pourras revenir en arrière depuis la page Sauvegarde (dans Protection).".into(),
            ..ActionAudit::new("Désactiver", Geste::BureauDistance)
        };
        ajout(
            &mut l,
            "warn",
            "pc",
            "Bureau à distance activé",
            "N'importe qui sur ton réseau peut essayer de se connecter à ce PC avec ton mot de passe Windows. Si tu ne t'en sers pas, désactive-le.",
            vec![],
            vec![a],
        );
    } else {
        ajout(
            &mut l,
            "ok",
            "pc",
            "Bureau à distance désactivé",
            "Personne ne peut prendre le contrôle de ce PC à distance.",
            vec![],
            vec![],
        );
    }
    let cat = chaine(pc.get("Category"));
    if w.is_some() && R_OUVERT.is_match(&auth) && egal(&cat, "Private") {
        let a =
            ActionAudit { arg: if_index.clone(), apres: true, ..ActionAudit::new("Passer en public", Geste::Public) };
        ajout(
            &mut l,
            "bad",
            "pc",
            "Wi-Fi ouvert réglé en réseau privé",
            "Sur un Wi-Fi sans mot de passe, ton PC doit être en mode public pour rester invisible des autres appareils.",
            vec![],
            vec![a],
        );
    } else if egal(&cat, "Public") {
        ajout(
            &mut l,
            "ok",
            "pc",
            "Réseau en mode public",
            "Ton PC est invisible pour les autres appareils du réseau. C'est le réglage le plus sûr (le partage de fichiers entre PC est alors bloqué).",
            vec![],
            vec![],
        );
    } else if egal(&cat, "Private") {
        ajout(
            &mut l,
            "ok",
            "pc",
            "Réseau en mode privé",
            "Normal chez toi : les autres appareils peuvent voir ce PC. Sur un Wi-Fi d'hôtel ou de gare, choisis toujours « Public ».",
            vec![],
            vec![],
        );
    }
    let partages: Vec<(String, String, String)> = liste(pc.get("Shares"))
        .iter()
        .map(|x| {
            let p = parts_n(x, 3);
            (part(&p, 0).to_string(), part(&p, 1).to_string(), part(&p, 2).to_string())
        })
        .collect();
    let tous: Vec<&(String, String, String)> = partages.iter().filter(|s| !s.2.is_empty()).collect();
    let gerer = || ActionAudit { sans_maj: true, ..ActionAudit::new("Gérer les partages", Geste::Partages) };
    let droit = |x: &str| match x.to_lowercase().as_str() {
        "full" => "contrôle total".to_string(),
        "change" => "modification".to_string(),
        "read" => "lecture".to_string(),
        _ => x.to_string(),
    };
    if !tous.is_empty() {
        ajout(
            &mut l,
            "warn",
            "pc",
            &format!("{} avec tout le monde", pluriel(tous.len(), "dossier partagé", "dossiers partagés")),
            "N'importe quel appareil de ton réseau peut ouvrir ces dossiers. Vérifie que c'est voulu, et retire « Tout le monde » des autorisations sinon.",
            tous.iter()
                .map(|s| format!("{} ({}) : {}", s.0, s.1, s.2.split(',').map(droit).collect::<Vec<_>>().join(", ")))
                .collect(),
            vec![gerer()],
        );
    } else if !partages.is_empty() {
        ajout(
            &mut l,
            "info",
            "pc",
            &format!("{} sur le réseau", pluriel(partages.len(), "dossier partagé", "dossiers partagés")),
            "Seules les personnes autorisées peuvent y accéder.",
            partages.iter().map(|s| format!("{} ({})", s.0, s.1)).collect(),
            vec![gerer()],
        );
    } else {
        ajout(
            &mut l,
            "ok",
            "pc",
            "Aucun dossier partagé",
            "Tes fichiers ne sont pas accessibles depuis le réseau.",
            vec![],
            vec![],
        );
    }
    l
}

/// La note sur 100 (20 points par point à corriger, 7 par point à surveiller)
pub fn note(points: &[Point]) -> (i64, usize, usize) {
    let bad = points.iter().filter(|c| c.statut == "bad").count();
    let warn = points.iter().filter(|c| c.statut == "warn").count();
    ((100 - 20 * bad as i64 - 7 * warn as i64).max(0), bad, warn)
}

/// `Get-AuditLabel`
pub fn libelle(score: i64) -> &'static str {
    if score >= 85 {
        "Réseau bien protégé"
    } else if score >= 60 {
        "Quelques points à améliorer"
    } else {
        "Réseau à risque"
    }
}

/// La carte de l'audit sur la page Réseau (`Update-NetAuditCard`) : son texte et le texte de son bouton
pub fn carte(sauve: Option<&Value>) -> Option<(String, String)> {
    let s = sauve?;
    let score = entier_ps(s.get("Score"));
    let mut parties = Vec::new();
    if entier_ps(s.get("Bad")) != 0 {
        parties.push(format!("{} à corriger", chaine(s.get("Bad"))));
    }
    if entier_ps(s.get("Warn")) != 0 {
        parties.push(format!("{} à surveiller", chaine(s.get("Warn"))));
    }
    let detail = if parties.is_empty() { "rien à corriger".to_string() } else { parties.join(", ") };
    Some((
        format!("{}. Dernier audit le {} : {detail}.", libelle(score), chaine(s.get("Date"))),
        "Relancer l'audit".into(),
    ))
}

/// `[int]@($x)[0]`
pub fn entier_ps(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::Array(a)) => entier_ps(a.first()),
        Some(Value::Number(n)) => n.as_f64().map(crate::texte::arrondi_pair).unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse::<f64>().map(crate::texte::arrondi_pair).unwrap_or(0),
        Some(Value::Bool(b)) => i64::from(*b),
        _ => 0,
    }
}

/// Ce qu'on garde de l'audit dans audit.json
pub fn sauvegarde(score: i64, date: &str, bad: usize, warn: usize) -> Value {
    let mut m = Map::new();
    m.insert("Score".into(), json!(score));
    m.insert("Date".into(), json!(date));
    m.insert("Bad".into(), json!(bad));
    m.insert("Warn".into(), json!(warn));
    Value::Object(m)
}

/// La couleur de la note (`Get-AuditColor`)
pub fn couleur(score: i64) -> &'static str {
    if score >= 85 {
        "#22D37A"
    } else if score >= 60 {
        "#F5A524"
    } else {
        "#F04438"
    }
}

/// `[System.Net.WebUtility]::HtmlEncode` (.NET Framework : les lettres de 160 à 255 et celles hors du plan de base
/// en entités numériques)
fn html(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => r.push_str("&amp;"),
            '<' => r.push_str("&lt;"),
            '>' => r.push_str("&gt;"),
            '"' => r.push_str("&quot;"),
            '\'' => r.push_str("&#39;"),
            c if ('\u{a0}'..='\u{ff}').contains(&c) || u32::from(c) > 0xFFFF => {
                r.push_str(&format!("&#{};", u32::from(c)))
            }
            c => r.push(c),
        }
    }
    r
}

/// Le rapport de l'audit en page web (`Export-NetAudit`)
pub fn rapport_html(points: &[Point], score: i64, date: &str, nombre: usize, poste: &str) -> String {
    let couleurs = |s: &str| match s {
        "ok" => "#22D37A",
        "warn" => "#F5A524",
        "bad" => "#F04438",
        _ => "#4EA8FF",
    };
    let titres = [("bad", "À corriger"), ("warn", "À surveiller"), ("info", "Bon à savoir"), ("ok", "Tout va bien")];
    let mut sections = Vec::new();
    for (st, lbl) in titres {
        let items: Vec<&Point> = points.iter().filter(|c| c.statut == st).collect();
        if items.is_empty() {
            continue;
        }
        let cartes: Vec<String> = items
            .iter()
            .map(|c| {
                let li = if c.elements.is_empty() {
                    String::new()
                } else {
                    format!("<ul>{}</ul>", c.elements.iter().map(|x| format!("<li>{}</li>", html(x))).collect::<String>())
                };
                format!(
                    "<div class='f'><span class='dot' style='background:{}'></span><div><span class='cat'>{}</span><b>{}</b><p>{}</p>{li}</div></div>",
                    couleurs(st),
                    html(rubrique(&c.cat)),
                    html(&c.titre),
                    html(&c.detail)
                )
            })
            .collect();
        sections.push(format!("<h2 style='color:{}'>{}</h2>\n{}", couleurs(st), html(lbl), cartes.join("\n")));
    }
    let conseils: String = CONSEILS.iter().map(|t| format!("<li>{}</li>", html(t))).collect();
    let col = couleur(score);
    format!(
        "<!DOCTYPE html>\n<html lang=\"fr\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>Audit réseau Nevermind</title>\n<style>\nbody{{margin:0;background:#0D0B14;color:#EEEBF7;font:15px/1.5 'Segoe UI',system-ui,sans-serif}}\nmain{{max-width:860px;margin:0 auto;padding:32px 16px}}\nh1{{margin:0;font-size:28px}}h1 span{{color:#00D9F5}}\nh2{{margin:32px 0 12px;font-size:18px}}\n.sub{{color:#A6A1BC}}\n.score{{display:flex;align-items:center;gap:20px;background:#18151F;border:1px solid #231E33;border-radius:12px;padding:20px;margin-top:24px}}\n.score b{{font-size:48px;color:{col}}}\n.f{{display:flex;gap:14px;background:#18151F;border:1px solid #231E33;border-radius:12px;padding:12px 16px;margin-bottom:8px}}\n.f p{{margin:2px 0 0;color:#A6A1BC;font-size:14px}}\n.f ul{{margin:6px 0 0;padding-left:18px;color:#D3CDE3;font-size:13px}}\n.cat{{display:inline-block;margin-right:10px;color:#655E7E;font-size:12px;font-weight:600;text-transform:uppercase}}\n.dot{{flex:none;width:12px;height:12px;border-radius:50%;margin-top:6px}}\n.tips{{background:#18151F;border:1px solid #231E33;border-radius:12px;padding:12px 16px 12px 34px;color:#D3CDE3}}\n</style></head><body><main>\n<h1>Never<span>mind</span></h1>\n<div class=\"sub\">Audit de sécurité du réseau, depuis {}, le {date}</div>\n<div class=\"score\"><b>{score}</b><div><div style=\"font-size:20px;font-weight:600\">{}</div><div class=\"sub\">Note de sécurité sur 100, {nombre} appareils vérifiés</div></div></div>\n{}\n<h2>Bons réflexes</h2>\n<ul class=\"tips\">{conseils}</ul>\n</main></body></html>\n",
        html(poste),
        html(libelle(score)),
        sections.join("\n")
    )
}
