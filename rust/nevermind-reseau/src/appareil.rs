//! Un appareil du réseau (la table de `$script:NetList`), son fabricant, son type, son nom et ses indices « caméra »
//! (Get-Vendor, Get-DeviceKind, Get-OsGuess, Update-DeviceIdentity, Set-LocalDevice de reseau.ps1 et
//! reseau-avance.ps1).

use crate::texte::{contient, egal, re};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Un appareil, avec les mêmes champs que la version PowerShell (les noms des champs sont ceux des juges)
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase", default)]
pub struct Appareil {
    pub ip: String,
    pub mac: String,
    pub host: String,
    pub vendor: String,
    pub title: String,
    pub svc_text: String,
    pub mdns_host: String,
    pub mdns_name: String,
    pub upnp_name: String,
    pub upnp_maker: String,
    pub upnp_model: String,
    pub upnp_type: String,
    pub nb_name: String,
    pub nb_group: String,
    pub wsd_types: String,
    pub wsd_scopes: String,
    pub wsd_name: String,
    pub wsd_hardware: String,
    pub web_title: String,
    pub web_server: String,
    pub announced: String,
    pub model: String,
    pub maker: String,
    pub os_guess: String,
    pub adapter: String,
    pub ms: Option<i64>,
    pub ttl: i64,
    #[serde(rename = "Self")]
    pub soi: bool,
    pub gateway: bool,
    pub new: bool,
    pub hidden: bool,
    pub camera: bool,
    pub only6: bool,
    pub camera_why: Vec<String>,
    pub services: Vec<String>,
    pub service_labels: Vec<String>,
    pub ipv6: Vec<String>,
    pub found_by: Vec<String>,
    pub ports: Vec<i64>,
    /// `KindInfo.Kind`
    pub kind: String,
    /// Les informations annoncées en mDNS (clés sans majuscules, comme une table PowerShell)
    pub txt: Map<String, Value>,
    /// Le nom annoncé par service (`MdnsNames`), seulement pendant la recherche approfondie
    #[serde(skip)]
    pub mdns_names: Vec<(String, String)>,
}

impl Appareil {
    /// `$D.Txt.<clé>` (sans tenir compte des majuscules)
    pub fn txt(&self, cle: &str) -> &str {
        self.txt.iter().find(|(k, _)| egal(k, cle)).and_then(|(_, v)| v.as_str()).unwrap_or("")
    }
    pub fn txt_a(&self, cle: &str) -> bool {
        self.txt.keys().any(|k| egal(k, cle))
    }
    pub fn nom_mdns(&self, cle: &str) -> Option<&str> {
        self.mdns_names.iter().find(|(k, _)| egal(k, cle)).map(|(_, v)| v.as_str())
    }
}

/// Le fichier des fabricants (fabricants.txt : « 3 octets|nom »), clés en majuscules
pub type Fabricants = HashMap<String, String>;

/// Lit fabricants.txt (« F0B429|Apple, Inc. » par ligne)
pub fn lire_fabricants(texte: &str) -> Fabricants {
    let mut m = Fabricants::new();
    for l in texte.lines() {
        if let Some(i) = l.find('|')
            && i > 0
        {
            m.insert(l[..i].trim_matches('\u{feff}').to_uppercase(), l[i + 1..].to_string());
        }
    }
    m
}

static SOCIETE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"[,\s]+(inc|incorporated|co|ltd|corporation|corp|gmbh|s\.?a\.?s|sarl|s\.a|limited|llc|b\.v|ag|oy|ab)\b\.?")
});
static METIER: LazyLock<Regex> =
    LazyLock::new(|| re(r"\s+(technologies|technology|electronics|communications|broadband)\b"));

/// Le fabricant d'une adresse physique (`Get-Vendor`) : « Adresse privée » pour une adresse inventée par un téléphone
pub fn vendeur(mac: &str, fab: Option<&Fabricants>) -> String {
    if mac.is_empty() {
        return String::new();
    }
    let hex: Vec<char> = mac.chars().filter(|c| *c != '-' && *c != ':').flat_map(char::to_uppercase).collect();
    if hex.len() < 6 {
        return String::new();
    }
    if hex[1].to_digit(16).is_some_and(|d| d & 2 != 0) {
        return "Adresse privée".into();
    }
    let Some(fab) = fab else { return String::new() };
    let cle: String = hex[..6].iter().collect();
    let v = fab.get(&cle).map(String::as_str).unwrap_or("");
    let v = SOCIETE.replace_all(v, "");
    let v = METIER.replace_all(&v, "");
    v.trim_matches(|c| c == ' ' || c == ',' || c == '.').to_string()
}

static R_IMPRIMANTE_SVC: LazyLock<Regex> =
    LazyLock::new(|| re(r"_ipp|_printer|_pdl-datastream|PrintDevice|\bPrinter\b"));
static R_ROUTEUR: LazyLock<Regex> = LazyLock::new(|| {
    re(r"\brt-|router|routeur|archer|\bdeco\b|orbi|mesh|access.?point|repeater|répéteur|ubiquiti|unifi")
});
static R_ORDI_SVC: LazyLock<Regex> = LazyLock::new(|| re(r"pub:Computer|_workstation|_smb\b"));
static R_APPLE_SVC: LazyLock<Regex> = LazyLock::new(|| re(r"_companion-link"));
static R_TV_SVC: LazyLock<Regex> =
    LazyLock::new(|| re(r"_googlecast|_amzn-wplay|_androidtvremote2|MediaRenderer|_mediaremotetv|_airplay"));
static R_AUDIO_SVC: LazyLock<Regex> = LazyLock::new(|| re(r"_sonos|_raop"));
static R_IOT_SVC: LazyLock<Regex> = LazyLock::new(|| re(r"_hap|_homekit|_matter|_hue|_alexa"));
static R_TEL: LazyLock<Regex> =
    LazyLock::new(|| re(r"iphone|ipad|android|galaxy|pixel|redmi|oneplus|oppo|honor|phone|motorola|poco"));
static R_CONSOLE: LazyLock<Regex> =
    LazyLock::new(|| re(r"playstation|\bps[345]\b|sony interactive|nintendo|xbox|switch"));
static R_TV: LazyLock<Regex> =
    LazyLock::new(|| re(r"webos|\btv\b|tizen|bravia|androidtv|chromecast|roku|fire.?tv|lg innotek|hisense|\btcl\b"));
static R_IMPRIMANTE: LazyLock<Regex> =
    LazyLock::new(|| re(r"printer|imprimante|hewlett|\bhp\b|canon|epson|brother|lexmark|kyocera"));
static R_BOX: LazyLock<Regex> =
    LazyLock::new(|| re(r"sagemcom|sercomm|arcadyan|technicolor|freebox|livebox|bbox|decodeur|décodeur"));
static R_IOT: LazyLock<Regex> = LazyLock::new(|| {
    re(r"espressif|tuya|shelly|sonoff|signify|philips lighting|amazon|google|nest|ring|meross|netatmo|tapo|xiaomi")
});
static R_APPLE: LazyLock<Regex> = LazyLock::new(|| re(r"\bapple\b"));
static R_ORDI: LazyLock<Regex> =
    LazyLock::new(|| re(r"desktop|laptop|\bpc|asustek|micro-star|gigabyte|dell|lenovo|acer|intel|realtek|killer"));

/// Le type d'un appareil (`Get-DeviceKind`, `.Kind`)
pub fn sorte(d: &Appareil) -> &'static str {
    let t = format!(
        "{} {} {} {} {} {} {} {}",
        d.host, d.vendor, d.mdns_host, d.announced, d.maker, d.model, d.upnp_name, d.web_title
    );
    let svc = &d.svc_text;
    if d.soi {
        return "Ce PC";
    }
    if d.gateway {
        return "Box Internet";
    }
    if d.camera {
        return "Caméra";
    }
    if R_IMPRIMANTE_SVC.is_match(svc) {
        return "Imprimante";
    }
    if R_ROUTEUR.is_match(&t) {
        return "Routeur ou répéteur Wi-Fi";
    }
    if !d.nb_name.is_empty() || R_ORDI_SVC.is_match(svc) {
        return "Ordinateur";
    }
    if R_APPLE_SVC.is_match(svc) {
        return "Appareil Apple";
    }
    if R_TV_SVC.is_match(svc) {
        return "TV ou multimédia";
    }
    if R_AUDIO_SVC.is_match(svc) {
        return "Enceinte ou audio";
    }
    if R_IOT_SVC.is_match(svc) {
        return "Objet connecté";
    }
    if R_TEL.is_match(&t) {
        return "Téléphone ou tablette";
    }
    if R_CONSOLE.is_match(&t) {
        return "Console de jeu";
    }
    if R_TV.is_match(&t) {
        return "TV ou multimédia";
    }
    if R_IMPRIMANTE.is_match(&t) {
        return "Imprimante";
    }
    if R_BOX.is_match(&t) {
        return "Box ou décodeur TV";
    }
    if R_IOT.is_match(&t) {
        return "Objet connecté";
    }
    if R_APPLE.is_match(&t) {
        return "Appareil Apple";
    }
    if R_ORDI.is_match(&t) {
        return "Ordinateur";
    }
    if egal(&d.vendor, "Adresse privée") {
        return "Téléphone probable";
    }
    "Appareil"
}

static R_OS_WIN: LazyLock<Regex> = LazyLock::new(|| re(r"pub:Computer|_workstation"));
static R_OS_APPLE: LazyLock<Regex> = LazyLock::new(|| re(r"_companion-link|_mediaremotetv"));
static R_OS_APPLE_MODELE: LazyLock<Regex> = LazyLock::new(|| re(r"^(iPhone|iPad|Mac|AppleTV)"));

/// Le système probable (`Get-OsGuess`)
pub fn systeme(d: &Appareil) -> &'static str {
    let t = d.ttl;
    if !d.nb_name.is_empty() || R_OS_WIN.is_match(&d.svc_text) {
        return "Windows";
    }
    if R_OS_APPLE.is_match(&d.svc_text) || R_OS_APPLE_MODELE.is_match(&d.model) {
        return "Apple (iPhone, iPad, Mac ou Apple TV)";
    }
    if t > 128 {
        return "Système d'équipement réseau (box, routeur, imprimante...)";
    }
    if t > 64 {
        return "Windows";
    }
    if t > 0 {
        return "Linux, Android, iPhone ou Mac";
    }
    ""
}

/// Les services annoncés, en clair (`$ServiceLabels`, dans l'ordre)
pub const SERVICES: &[(&str, &str)] = &[
    ("_googlecast", "Chromecast / Google Cast"),
    ("_airplay", "AirPlay"),
    ("_raop", "Enceinte AirPlay"),
    ("_companion-link", "Appareil Apple"),
    ("_ipp", "Impression"),
    ("_ipps", "Impression"),
    ("_printer", "Impression"),
    ("_pdl-datastream", "Impression"),
    ("_scanner", "Scanner"),
    ("_uscan", "Scanner"),
    ("_hap", "Maison connectée (HomeKit)"),
    ("_homekit", "Maison connectée (HomeKit)"),
    ("_matter", "Maison connectée (Matter)"),
    ("_matterc", "Maison connectée (Matter)"),
    ("_hue", "Philips Hue"),
    ("_spotify-connect", "Spotify Connect"),
    ("_sonos", "Sonos"),
    ("_amzn-wplay", "Amazon Fire TV"),
    ("_androidtvremote2", "Android TV"),
    ("_mediaremotetv", "Apple TV"),
    ("_smb", "Partage de fichiers"),
    ("_workstation", "Ordinateur"),
    ("_nvstream", "NVIDIA GameStream"),
    ("_rtsp", "Flux vidéo"),
    ("_http", "Page web"),
    ("_alexa", "Amazon Alexa"),
];

static R_CAM_FAB: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"hikvision|dahua|ezviz|reolink|wyze|arlo|amcrest|axis comm|foscam|uniview|zhejiang uniview|imou|annke|lorex|swann|vivotek|hanwha|eufy|blink|yi technology|shenzhen reecam|sricam|vstarcam|hisilicon|ubiquiti.*protect",
    )
});
static R_CAM_MOTS: LazyLock<Regex> =
    LazyLock::new(|| re(r"camera|caméra|\bipc\b|\bnvr\b|\bdvr\b|webcam|ip ?cam|network ?video|onvif|video ?encoder"));
static R_ONVIF_TYPE: LazyLock<Regex> = LazyLock::new(|| re("NetworkVideoTransmitter"));
static R_ONVIF: LazyLock<Regex> = LazyLock::new(|| re("onvif"));
static R_RTSP: LazyLock<Regex> = LazyLock::new(|| re("^_rtsp"));
static R_CAST: LazyLock<Regex> = LazyLock::new(|| re("^_googlecast"));

fn premier<'a>(v: &[&'a str]) -> &'a str {
    v.iter().find(|x| !x.is_empty()).copied().unwrap_or("")
}

/// Recalcule le nom, le type, le modèle et les indices « caméra » (`Update-DeviceIdentity`)
pub fn maj_identite(d: &mut Appareil) {
    let mut labels: Vec<String> = Vec::new();
    for s in &d.services {
        let k = s.split('.').next().unwrap_or("");
        if let Some((_, l)) = SERVICES.iter().find(|(c, _)| egal(c, k))
            && !labels.iter().any(|x| x == l)
        {
            labels.push((*l).to_string());
        }
    }
    d.svc_text = format!("{} {} {} {}", d.services.join(" "), d.upnp_type, d.wsd_types, labels.join(" "));
    d.service_labels = labels;
    let model = premier(&[
        d.txt("md"),
        d.txt("model"),
        d.txt("am"),
        d.txt("rpMd"),
        d.txt("ty"),
        d.txt("usb_MDL"),
        d.txt("product"),
        &d.upnp_model,
        &d.wsd_hardware,
    ])
    .to_string();
    let maker =
        premier(&[d.txt("manufacturer"), d.txt("usb_MFG"), d.txt("integrator"), d.txt("vendor"), &d.upnp_maker])
            .to_string();
    d.model = model;
    d.maker = maker;
    d.os_guess = systeme(d).to_string();
    let mut fort = Vec::new();
    let mut faible = Vec::new();
    if R_CAM_FAB.is_match(&d.vendor) || R_CAM_FAB.is_match(&d.maker) {
        let qui = if R_CAM_FAB.is_match(&d.maker) { &d.maker } else { &d.vendor };
        fort.push(format!("fabricant de caméras ({qui})"));
    }
    if R_ONVIF_TYPE.is_match(&d.wsd_types) || R_ONVIF.is_match(&d.wsd_scopes) {
        fort.push("se déclare comme caméra (norme ONVIF)".to_string());
    }
    if R_CAM_MOTS.is_match(&format!("{} {} {} {} {}", d.model, d.upnp_name, d.upnp_type, d.web_title, d.mdns_name)) {
        fort.push("son nom ou sa page web parle de caméra".to_string());
    }
    if d.ports.contains(&554) || d.services.iter().any(|s| R_RTSP.is_match(s)) {
        faible.push("diffuse un flux vidéo (RTSP)".to_string());
    }
    if d.ports.contains(&8000) || d.ports.contains(&37777) {
        faible.push("port typique des enregistreurs vidéo".to_string());
    }
    d.camera = !fort.is_empty() || faible.len() >= 2;
    fort.extend(faible);
    d.camera_why = fort;
    let fn_ = d.txt("fn").to_string();
    let annonce = if !fn_.is_empty() && d.services.iter().any(|s| R_CAST.is_match(s)) {
        fn_
    } else {
        premier(&[&d.mdns_name, &d.upnp_name, &fn_, &d.wsd_name, &d.nb_name]).to_string()
    };
    d.announced = annonce;
    if !d.soi && !d.gateway {
        d.title = if !d.announced.is_empty() {
            d.announced.clone()
        } else if !d.host.is_empty() && !egal(&d.host, "lan") {
            d.host.clone()
        } else if !d.mdns_host.is_empty() {
            d.mdns_host.clone()
        } else if !d.model.is_empty() {
            format!("{}{}", if d.maker.is_empty() { String::new() } else { format!("{} ", d.maker) }, d.model)
        } else if !d.vendor.is_empty() && !egal(&d.vendor, "Adresse privée") {
            d.vendor.clone()
        } else {
            "Appareil inconnu".to_string()
        };
    }
    d.kind = sorte(d).to_string();
}

/// Une carte réseau de ce PC (`Get-LocalInterfaces`)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InterfaceLocale {
    pub mac: String,
    pub nom: String,
    pub desc: String,
    pub wifi: bool,
    pub active: bool,
}

/// Les cartes de ce PC, par adresse
pub type Locaux = Vec<(String, InterfaceLocale)>;

/// Un appareil du scan qui est une carte réseau de ce PC (`Set-LocalDevice`)
pub fn carte_locale(d: &mut Appareil, locaux: &Locaux, poste: &str, fab: Option<&Fabricants>) {
    let Some((_, li)) = locaux.iter().find(|(ip, _)| egal(ip, &d.ip)) else { return };
    d.soi = true;
    if !li.mac.is_empty() {
        d.mac = li.mac.clone();
        d.vendor = vendeur(&li.mac, fab);
    }
    d.host = poste.to_string();
    let comment = if li.wifi { "Wi-Fi" } else { "câble" };
    d.title = if li.active {
        format!("{poste} (ce PC, {comment})")
    } else {
        format!("{poste} (ce PC, ancienne adresse {comment})")
    };
    d.adapter = format!(
        "{} : {}{}",
        li.nom,
        li.desc,
        if li.active { "" } else { ", déconnectée : Windows répond lui-même à son ancienne adresse" }
    );
    d.hidden = false;
    d.new = false;
    d.kind = sorte(d).to_string();
}

/// `-notcontains` puis ajout (les listes de l'appareil)
pub fn ajouter(liste: &mut Vec<String>, x: &str) {
    if !contient(liste, x) {
        liste.push(x.to_string());
    }
}
