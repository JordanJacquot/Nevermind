//! Le scan du réseau, sans Windows : les adresses à essayer (Get-SubnetIps), l'assemblage des réponses
//! (Invoke-NetworkScan), la recherche approfondie (Merge-NetDeep, Invoke-NetDeepScan), les appareils déjà vus
//! (appareils.json), les textes de la liste (Show-NetDevices) et la surveillance (Invoke-NetWatch).

use crate::appareil::{Appareil, Fabricants, Locaux, ajouter, carte_locale, maj_identite, vendeur};
use crate::net::Connexion;
use crate::texte::{cle_version, contient, desechapper, egal, entier, part, parts, parts_n, re};
use regex::Regex;
use serde_json::{Map, Value};
use std::sync::LazyLock;

/// Les adresses du réseau de ce PC (`Get-SubnetIps`, au plus un /24)
pub fn sous_reseau(ip: &str, prefixe: i64) -> Vec<String> {
    let prefixe = prefixe.max(24);
    let Ok(a) = ip.trim().parse::<std::net::Ipv4Addr>() else { return Vec::new() };
    let n = f64::from(u32::from(a));
    let taille = 2f64.powi((32 - prefixe) as i32);
    let reseau = (n / taille).floor() * taille;
    let mut r = Vec::new();
    let mut i = 1.0;
    while i < taille - 1.0 {
        r.push(std::net::Ipv4Addr::from((reseau + i) as u32).to_string());
        i += 1.0;
    }
    r
}

/// Les réponses du scan rapide (`$NetScanWork`) : « ip|ms|ttl », « ip|mac|état », « ip|nom »
#[derive(Clone, Debug, Default)]
pub struct BrutScan {
    pub vivants: Vec<String>,
    pub arp: Vec<String>,
    pub noms: Vec<String>,
}

/// Les réponses de la recherche approfondie (`$NetDeepWork`)
#[derive(Clone, Debug, Default)]
pub struct BrutApprofondi {
    pub arp: Vec<String>,
    pub mdns: Vec<String>,
    pub wsd: Vec<String>,
    pub upnp: Vec<String>,
    pub v6: Vec<String>,
    pub netbios: Vec<String>,
    pub ports: Vec<String>,
    pub titres: Vec<String>,
}

/// Les appareils déjà vus (appareils.json : adresse physique, date de la première apparition)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Connus(pub Vec<(String, String)>);

impl Connus {
    /// Le contenu du fichier (un objet, ou une ancienne liste d'adresses, ou une adresse seule)
    pub fn depuis(v: Option<&Value>) -> Self {
        let mut c = Connus::default();
        match v {
            Some(Value::String(s)) => c.mettre(s, ""),
            Some(Value::Array(a)) => {
                for m in a {
                    c.mettre(&crate::texte::chaine(Some(m)), "");
                }
            }
            Some(Value::Object(o)) => {
                for (k, x) in o {
                    c.mettre(k, &crate::texte::chaine(Some(x)));
                }
            }
            _ => {}
        }
        c
    }
    pub fn contient(&self, mac: &str) -> bool {
        self.0.iter().any(|(k, _)| egal(k, mac))
    }
    pub fn date(&self, mac: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| egal(k, mac)).map(|(_, v)| v.as_str())
    }
    pub fn mettre(&mut self, mac: &str, date: &str) {
        if let Some(e) = self.0.iter_mut().find(|(k, _)| egal(k, mac)) {
            e.1 = date.to_string();
        } else {
            self.0.push((mac.to_string(), date.to_string()));
        }
    }
    pub fn est_vide(&self) -> bool {
        self.0.is_empty()
    }
    pub fn json(&self) -> Value {
        Value::Object(self.0.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect::<Map<_, _>>())
    }
}

/// Ce dont l'assemblage a besoin autour de lui
pub struct Autour<'a> {
    pub poste: &'a str,
    pub aujourdhui: &'a str,
    pub fab: Option<&'a Fabricants>,
    pub locaux: &'a Locaux,
}

static R_SUFFIXE: LazyLock<Regex> =
    LazyLock::new(|| re(r"\.(home|lan|local|localdomain|box|fritz\.box|station|bbox)$"));
const ETATS_VUS: [&str; 4] = ["Reachable", "Stale", "Delay", "Probe"];

fn trier(liste: &mut [Appareil]) {
    liste.sort_by_key(|d| {
        (
            if d.soi {
                0
            } else if d.gateway {
                1
            } else {
                2
            },
            cle_version(&d.ip),
        )
    });
}

/// L'assemblage des appareils après le scan rapide (la fin d'`Invoke-NetworkScan`). Rend la liste et « premier scan ».
pub fn assembler(
    net: &Connexion,
    ips: &[String],
    r: &BrutScan,
    connus: &mut Connus,
    a: &Autour,
) -> (Vec<Appareil>, bool) {
    let dans = |ip: &str| ips.iter().any(|x| x == ip);
    let mut devs: Vec<Appareil> = Vec::new();
    let idx = |devs: &Vec<Appareil>, ip: &str| devs.iter().position(|d| egal(&d.ip, ip));
    for l in &r.vivants {
        let p = parts(l);
        let ip = part(&p, 0).to_string();
        let d = Appareil {
            ip: ip.clone(),
            ms: Some(entier(part(&p, 1)).unwrap_or(0)),
            ttl: if p.len() > 2 { entier(part(&p, 2)).unwrap_or(0) } else { 0 },
            ..Default::default()
        };
        match idx(&devs, &ip) {
            Some(i) => devs[i] = d,
            None => devs.push(d),
        }
    }
    for l in &r.arp {
        let p = parts(l);
        if !dans(part(&p, 0)) {
            continue;
        }
        if let Some(i) = idx(&devs, part(&p, 0)) {
            devs[i].mac = part(&p, 1).to_string();
        } else if ETATS_VUS.iter().any(|e| egal(e, part(&p, 2))) {
            devs.push(Appareil {
                ip: part(&p, 0).to_string(),
                ms: None,
                mac: part(&p, 1).to_string(),
                ..Default::default()
            });
        }
    }
    let soi = net.ip.clone().unwrap_or_default();
    let i = match idx(&devs, &soi) {
        Some(i) => i,
        None => {
            devs.push(Appareil { ip: soi.clone(), ms: Some(0), ..Default::default() });
            devs.len() - 1
        }
    };
    devs[i].mac = net.mac.clone();
    let mut noms: Vec<(String, String)> = Vec::new();
    for l in &r.noms {
        let p = parts_n(l, 2);
        if !part(&p, 1).is_empty() && !egal(part(&p, 1), part(&p, 0)) {
            let n = R_SUFFIXE.replace(part(&p, 1), "").into_owned();
            match noms.iter_mut().find(|(k, _)| egal(k, part(&p, 0))) {
                Some(e) => e.1 = n,
                None => noms.push((part(&p, 0).to_string(), n)),
            }
        }
    }
    let premier = connus.est_vide();
    for d in &mut devs {
        d.soi = egal(&d.ip, &soi);
        d.gateway = egal(&d.ip, &net.gateway);
        d.host = if d.soi {
            a.poste.to_string()
        } else {
            noms.iter().find(|(k, _)| egal(k, &d.ip)).map(|(_, v)| v.clone()).unwrap_or_default()
        };
        d.vendor = vendeur(&d.mac, a.fab);
        d.kind = crate::appareil::sorte(d).to_string();
        d.title = if d.soi {
            format!("{} (ce PC)", a.poste)
        } else if !d.host.is_empty() && !egal(&d.host, "lan") {
            d.host.clone()
        } else if d.gateway {
            "Box Internet".into()
        } else if !d.vendor.is_empty() && !egal(&d.vendor, "Adresse privée") {
            d.vendor.clone()
        } else {
            "Appareil inconnu".into()
        };
        d.new = !premier && !d.mac.is_empty() && !connus.contient(&d.mac) && !d.soi;
        if !egal(&d.ip, &soi) {
            carte_locale(d, a.locaux, a.poste, a.fab);
        }
    }
    trier(&mut devs);
    for d in &devs {
        if !d.mac.is_empty() && !connus.contient(&d.mac) {
            connus.mettre(&d.mac, a.aujourdhui);
        }
    }
    (devs, premier)
}

const ARP_DIRECT: &str = "demande directe d'adresse physique (ARP)";
static R_TXT_NOM: LazyLock<Regex> = LazyLock::new(|| re(r"^(.+?)\.(_[^.]+\._(?:tcp|udp))\.local$"));
static R_LOCAL: LazyLock<Regex> = LazyLock::new(|| re(r"\.local$"));
static R_ID_HEX: LazyLock<Regex> = LazyLock::new(|| re(r"^[0-9A-F]{12}@"));
static R_CLES_MDNS: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(model|md|manufacturer|fn|ty|am|integrator|rpMd|usb_MFG|usb_MDL|product|vendor|name)$"));
static R_TYPE_UPNP: LazyLock<Regex> = LazyLock::new(|| re(r":device:([^:]+)"));
static R_WPS_NOM: LazyLock<Regex> = LazyLock::new(|| re(r"^(WPS Access Point|Wi-?Fi Protected Setup)"));
static R_WSD_NOM: LazyLock<Regex> = LazyLock::new(|| re(r"/name/(.+)$"));
static R_WSD_MAT: LazyLock<Regex> = LazyLock::new(|| re(r"/hardware/(.+)$"));
const PRIORITE_MDNS: [&str; 14] = [
    "_googlecast",
    "_airplay",
    "_companion-link",
    "_mediaremotetv",
    "_hap",
    "_homekit",
    "_ipp",
    "_ipps",
    "_printer",
    "_sonos",
    "_amzn-wplay",
    "_androidtvremote2",
    "_hue",
    "_raop",
];

fn nom_mdns(d: &mut Appareil, cle: &str, inst: &str) {
    if d.nom_mdns(cle).is_none() {
        d.mdns_names.push((cle.to_string(), R_ID_HEX.replace(inst, "").into_owned()));
    }
}

/// L'hôte d'une adresse web (`([uri]$x).Host`)
fn hote(url: &str) -> String {
    let reste = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let reste = reste.split(['/', '?', '#']).next().unwrap_or("");
    let reste = reste.rsplit_once('@').map(|(_, r)| r).unwrap_or(reste);
    if reste.starts_with('[') {
        return reste.split_once(']').map(|(h, _)| format!("{h}]")).unwrap_or_default();
    }
    reste.split(':').next().unwrap_or("").to_lowercase()
}

/// Ajoute aux appareils ce que la recherche approfondie a trouvé (`Merge-NetDeep`). Rend les adresses (positions dans
/// la liste triée) des appareils nouvellement découverts.
pub fn fusionner(
    liste: &mut Vec<Appareil>,
    r: &BrutApprofondi,
    net: Option<&Connexion>,
    ips: &[String],
    a: &Autour,
) -> Vec<String> {
    let dans = |ip: &str| ips.iter().any(|x| egal(x, ip));
    let par_ip = |l: &Vec<Appareil>, ip: &str| l.iter().rposition(|d| egal(&d.ip, ip));
    // (la table des adresses physiques n'est remplie qu'au début et pour les nouveaux, comme $byMac)
    let mut par_mac: Vec<(String, usize)> = Vec::new();
    for (i, d) in liste.iter_mut().enumerate() {
        if !d.mac.is_empty() {
            par_mac.push((d.mac.to_uppercase(), i));
        }
        d.found_by = vec![if d.ms.is_some() { "réponse au ping" } else { "liste des voisins de Windows" }.to_string()];
        d.hidden = d.ms.is_none() && !d.soi;
    }
    let mut ajoutes: Vec<usize> = Vec::new();
    let nouveau =
        |liste: &mut Vec<Appareil>, par_mac: &mut Vec<(String, usize)>, ip: &str, mac: &str, comment: &str| {
            liste.push(Appareil {
                ip: ip.to_string(),
                ms: None,
                mac: mac.to_string(),
                vendor: vendeur(mac, a.fab),
                hidden: true,
                found_by: vec![comment.to_string()],
                ..Default::default()
            });
            if !mac.is_empty() {
                par_mac.push((mac.to_uppercase(), liste.len() - 1));
            }
            liste.len() - 1
        };
    for l in &r.arp {
        let p = parts(l);
        if !dans(part(&p, 0)) {
            continue;
        }
        match par_ip(liste, part(&p, 0)) {
            None => {
                let i = nouveau(liste, &mut par_mac, part(&p, 0), part(&p, 1), ARP_DIRECT);
                ajoutes.push(i);
            }
            Some(i) => {
                let d = &mut liste[i];
                if d.mac.is_empty() {
                    d.mac = part(&p, 1).to_string();
                    d.vendor = vendeur(&d.mac, a.fab);
                }
                ajouter(&mut d.found_by, ARP_DIRECT);
            }
        }
    }
    for l in &r.mdns {
        let p = parts_n(l, 4);
        if p.len() < 4 {
            continue;
        }
        let sorte = p[1].to_lowercase();
        if sorte == "a" {
            if let Some(i) = par_ip(liste, &p[3]) {
                liste[i].mdns_host = R_LOCAL.replace(&p[2], "").into_owned();
            }
        } else if sorte == "ptr" {
            let Some(i) = par_ip(liste, &p[0]) else { continue };
            if egal(&p[2], "_services._dns-sd._udp.local") {
                continue;
            }
            let d = &mut liste[i];
            let ty = R_LOCAL.replace(&p[2], "").into_owned();
            ajouter(&mut d.services, &ty);
            let fin = re(&format!("{}$", regex::escape(&format!(".{}", p[2]))));
            let inst = fin.replace(&p[3], "").into_owned();
            if !inst.is_empty() && !egal(&inst, &p[3]) {
                let cle = ty.split('.').next().unwrap_or("").to_string();
                nom_mdns(d, &cle, &inst);
            }
        } else if sorte == "txt" {
            let Some(i) = par_ip(liste, &p[0]) else { continue };
            let d = &mut liste[i];
            if let Some(c) = R_TXT_NOM.captures(&p[2]) {
                let inst = c[1].to_string();
                let ty = c[2].to_string();
                let cle = ty.split('.').next().unwrap_or("").to_string();
                ajouter(&mut d.services, &ty);
                nom_mdns(d, &cle, &inst);
            }
            let kv = parts_n_egal(&p[3]);
            if kv.len() == 2 && R_CLES_MDNS.is_match(&kv[0]) && !kv[1].is_empty() && !d.txt_a(&kv[0]) {
                d.txt.insert(kv[0].clone(), Value::String(kv[1].clone()));
            }
        }
    }
    for d in liste.iter_mut() {
        if !d.mdns_names.is_empty()
            && let Some(n) = PRIORITE_MDNS.iter().find_map(|k| d.nom_mdns(k).map(str::to_string))
        {
            d.mdns_name = n;
        }
    }
    for l in &r.upnp {
        let p = parts(l);
        let Some(i) = par_ip(liste, part(&p, 0)) else { continue };
        let ty = R_TYPE_UPNP.captures(part(&p, 5)).map(|c| c[1].to_string()).unwrap_or_default();
        if egal(&ty, "WFADevice") || R_WPS_NOM.is_match(part(&p, 1)) {
            continue;
        }
        let d = &mut liste[i];
        if !d.upnp_name.is_empty() {
            continue;
        }
        d.upnp_name = part(&p, 1).to_string();
        d.upnp_maker = part(&p, 2).to_string();
        d.upnp_model =
            [part(&p, 3), part(&p, 4)].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(" ");
        d.upnp_type = ty;
    }
    for l in &r.netbios {
        let p = parts(l);
        let Some(i) = par_ip(liste, part(&p, 0)) else { continue };
        liste[i].nb_name = part(&p, 1).to_string();
        liste[i].nb_group = part(&p, 2).to_string();
    }
    for l in &r.wsd {
        let p = parts(l);
        let Some(i) = par_ip(liste, part(&p, 0)) else { continue };
        let d = &mut liste[i];
        d.wsd_types = part(&p, 1).to_string();
        d.wsd_scopes = part(&p, 2).to_string();
        for s in d.wsd_scopes.clone().split(|c: char| c.is_whitespace()) {
            if let Some(c) = R_WSD_NOM.captures(s) {
                d.wsd_name = desechapper(&c[1]);
            }
            if let Some(c) = R_WSD_MAT.captures(s) {
                d.wsd_hardware = desechapper(&c[1]);
            }
        }
    }
    for l in &r.v6 {
        let p = parts(l);
        let mac = part(&p, 1).to_uppercase();
        let i = match par_mac.iter().rev().find(|(m, _)| *m == mac).map(|(_, i)| *i) {
            Some(i) => i,
            None => {
                let i = nouveau(liste, &mut par_mac, part(&p, 0), part(&p, 1), "voisins IPv6");
                liste[i].only6 = true;
                ajoutes.push(i);
                i
            }
        };
        ajouter(&mut liste[i].ipv6, part(&p, 0));
    }
    for l in &r.ports {
        let p = parts(l);
        if let Some(i) = par_ip(liste, part(&p, 0)) {
            let port = entier(part(&p, 1)).unwrap_or(0);
            if !liste[i].ports.contains(&port) {
                liste[i].ports.push(port);
            }
        }
    }
    for l in &r.titres {
        let p = parts(l);
        let Some(i) = par_ip(liste, &hote(part(&p, 0))) else { continue };
        let d = &mut liste[i];
        if !part(&p, 1).is_empty() && d.web_title.is_empty() {
            d.web_title = part(&p, 1).to_string();
        }
        if !part(&p, 2).is_empty() && d.web_server.is_empty() {
            d.web_server = part(&p, 2).to_string();
        }
    }
    for d in liste.iter_mut() {
        // (`@($(if ...), $(if ...), ...) | Where-Object { $_ }` de PowerShell 5.1 : la liste s'arrête au deuxième
        // « rien ». Reproduit tel quel, voir DIVERGENCES.md)
        let comment = [
            (!d.services.is_empty()).then_some("annonces Bonjour (mDNS)"),
            (!d.upnp_name.is_empty()).then_some("annonces UPnP"),
            (!d.nb_name.is_empty()).then_some("nom Windows (NetBIOS)"),
            (!d.wsd_types.is_empty()).then_some("WS-Discovery"),
        ];
        let mut riens = 0;
        for c in comment {
            match c {
                Some(c) => ajouter(&mut d.found_by, c),
                None => {
                    riens += 1;
                    if riens >= 2 {
                        break;
                    }
                }
            }
        }
        maj_identite(d);
    }
    let primaire = net.and_then(|n| n.ip.clone()).unwrap_or_default();
    for d in liste.iter_mut() {
        if d.ip != primaire {
            carte_locale(d, a.locaux, a.poste, a.fab);
        }
    }
    // (les nouveaux sont retrouvés après le tri par leur place d'avant)
    let mut marques: Vec<(usize, Appareil)> = liste.drain(..).enumerate().collect();
    marques.sort_by_key(|(_, d)| {
        (
            if d.soi {
                0
            } else if d.gateway {
                1
            } else {
                2
            },
            cle_version(&d.ip),
        )
    });
    let mut places = Vec::new();
    for (j, (i, d)) in marques.into_iter().enumerate() {
        if ajoutes.contains(&i) {
            places.push((ajoutes.iter().position(|x| *x == i).unwrap_or(0), j));
        }
        liste.push(d);
    }
    places.sort();
    places.into_iter().map(|(_, j)| j.to_string()).collect()
}

/// `$p[3] -split '=', 2`
fn parts_n_egal(s: &str) -> Vec<String> {
    s.splitn(2, '=').map(str::to_string).collect()
}

/// Après la recherche approfondie (`Invoke-NetDeepScan`) : les nouveaux mémorisés, et signalés s'ils n'étaient jamais
/// apparus. Rend le texte de la barre d'état.
pub fn apres_approfondi(liste: &mut [Appareil], ajoutes: &[String], connus: &mut Connus, aujourdhui: &str) -> String {
    let premier = connus.est_vide();
    for j in ajoutes {
        let Ok(j) = j.parse::<usize>() else { continue };
        let d = &mut liste[j];
        if !d.mac.is_empty() && !connus.contient(&d.mac) {
            d.new = !premier;
            connus.mettre(&d.mac, aujourdhui);
        }
    }
    let discrets = liste.iter().filter(|d| d.hidden).count();
    let cams = liste.iter().filter(|d| d.camera).count();
    let mut parties = Vec::new();
    if discrets > 0 {
        parties.push(format!("{discrets} discret{}", if discrets > 1 { "s" } else { "" }));
    }
    if cams > 0 {
        let s = if cams > 1 { "s" } else { "" };
        parties.push(format!("{cams} caméra{s} possible{s}"));
    }
    format!(
        "Scan terminé : {} appareils{}.",
        liste.len(),
        if parties.is_empty() { String::new() } else { format!(", dont {}", parties.join(", ")) }
    )
}

/// Les filtres de la liste (`$NetFilters`)
pub const FILTRES: [(&str, &str); 4] =
    [("all", "Tous"), ("hidden", "Discrets"), ("new", "Nouveaux"), ("cam", "Caméras possibles")];

pub fn passe_filtre(id: &str, d: &Appareil) -> bool {
    match id {
        "hidden" => d.hidden,
        "new" => d.new,
        "cam" => d.camera,
        _ => true,
    }
}

/// Le résumé et le conseil sous la liste (`$ui.NetDevSummary`, `$ui.NetDevHint` de Show-NetDevices)
pub fn textes_liste(liste: &[Appareil], premier: bool, filtre: &str) -> (String, String) {
    let n = liste.len();
    let nouveaux = liste.iter().filter(|d| d.new).count();
    let discrets = liste.iter().filter(|d| d.hidden).count();
    let mut cur = if filtre.is_empty() { "all".to_string() } else { filtre.to_string() };
    for (id, _) in FILTRES {
        if id != "all" && !liste.iter().any(|d| passe_filtre(id, d)) && egal(&cur, id) {
            cur = "all".into();
        }
    }
    let resume = format!(
        "{n} appareil{}{}{}",
        if n > 1 { "s" } else { "" },
        if nouveaux > 0 {
            format!(", {nouveaux} nouveau{}", if nouveaux > 1 { "x" } else { "" })
        } else {
            String::new()
        },
        if discrets > 0 {
            format!(", {discrets} discret{}", if discrets > 1 { "s" } else { "" })
        } else {
            String::new()
        }
    );
    let conseil = if egal(&cur, "hidden") {
        "Les appareils discrets ne répondent pas au ping : Nevermind les a trouvés autrement (la table de ta box, leurs annonces sur le réseau). C'est normal pour beaucoup de téléphones, de PC protégés par un pare-feu et d'objets connectés en veille. Clique dessus pour voir ce qui a été trouvé."
    } else if premier {
        "Premier scan : ces appareils sont mémorisés, Nevermind te signalera tout nouvel appareil au prochain scan. Clique sur un appareil pour voir ses détails."
    } else if nouveaux > 0 {
        "Un appareil « Nouveau » n'était pas là au scan précédent. Si tu ne le reconnais pas, change le mot de passe de ton Wi-Fi depuis la page de ta box. Attention : les téléphones récents changent parfois d'adresse et peuvent apparaître comme nouveaux."
    } else {
        "Aucun nouvel appareil depuis le dernier scan. Clique sur un appareil pour voir ses détails, son ping en direct et ses services ouverts."
    };
    (resume, conseil.to_string())
}

/// Les adresses du réseau qui n'ont pas répondu (`$missing` d'Invoke-NetDeepScan)
pub fn manquants(sous: &[String], liste: &[Appareil]) -> Vec<String> {
    let trouves: Vec<String> = liste.iter().map(|d| d.ip.clone()).collect();
    sous.iter().filter(|x| !contient(&trouves, x)).cloned().collect()
}

/// Une bulle de Windows (`Show-Notify`)
#[derive(Clone, Debug, PartialEq)]
pub struct Bulle {
    pub titre: String,
    pub texte: String,
}

/// La surveillance (`Invoke-NetWatch`) : les adresses physiques jamais vues signalées une fois. `vus` : déjà signalés
/// pendant cette session. Rend la ligne du journal et la bulle.
pub fn surveiller(
    net: &Connexion,
    ips: &[String],
    arp: &[String],
    connus: &Connus,
    vus: &mut Connus,
    aujourdhui: &str,
    fab: Option<&Fabricants>,
) -> Option<(String, Bulle)> {
    if connus.est_vide() {
        return None;
    }
    let soi = net.ip.clone().unwrap_or_default();
    let mut nouveaux = Vec::new();
    for l in arp {
        let p = parts(l);
        if !ips.iter().any(|x| egal(x, part(&p, 0))) || part(&p, 1).is_empty() || egal(part(&p, 0), &soi) {
            continue;
        }
        if !ETATS_VUS.iter().any(|e| egal(e, part(&p, 2))) {
            continue;
        }
        if connus.contient(part(&p, 1)) || vus.contient(part(&p, 1)) {
            continue;
        }
        vus.mettre(part(&p, 1), aujourdhui);
        let v = vendeur(part(&p, 1), fab);
        let nom = if !v.is_empty() && !egal(&v, "Adresse privée") { v } else { "Appareil inconnu".into() };
        nouveaux.push(format!("{nom} ({})", part(&p, 0)));
    }
    if nouveaux.is_empty() {
        return None;
    }
    let liste = nouveaux.join(", ");
    let titre = if nouveaux.len() > 1 {
        format!("{} nouveaux appareils sur ton réseau", nouveaux.len())
    } else {
        "Nouvel appareil sur ton réseau".to_string()
    };
    Some((
        format!("Nouvel appareil sur le réseau: {liste}"),
        Bulle { titre, texte: format!("{liste}. Si tu ne le reconnais pas, ouvre la section Réseau de Nevermind.") },
    ))
}
