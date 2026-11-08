//! Le calcul de la qualité du stream : des fonctions pures, sans rien lire ni écrire (voir la recherche du 08/11/2026,
//! partie 4 « Règle de choix »).
//!
//! - L'encodeur vient de la carte graphique : NVIDIA (NVENC), sinon AMD (AMF), sinon Intel (Quick Sync), sinon le
//!   processeur (x264, déconseillé pendant un jeu). H.264 partout (la seule chose que Twitch et Kick acceptent).
//! - Le débit vidéo : 70 % de l'envoi mesuré, moins le son, plafonné par la plateforme (Twitch 6 000 kb/s, Kick
//!   8 000, YouTube 12 000, Facebook 9 000, autre 6 000), arrondi à la centaine en dessous.
//! - Le palier : 480p30, 720p30, 720p60, 900p60, 1080p60 selon ce débit utile ; jamais plus haut que l'image de base
//!   (l'écran), jamais plus de 720p30 avec le processeur. Sous 1 500 kb/s utiles, le stream est déconseillé.
//! - Images clés toutes les 2 secondes (demandé par Kick, fixé par OBS lui-même pour Twitch et YouTube).

/// Le logiciel de stream
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Logiciel {
    Obs,
    Streamlabs,
}

impl Logiciel {
    /// L'identifiant (actions, fichiers de Nevermind)
    pub fn id(self) -> &'static str {
        match self {
            Logiciel::Obs => "obs",
            Logiciel::Streamlabs => "streamlabs",
        }
    }
    /// Le nom affiché
    pub fn nom(self) -> &'static str {
        match self {
            Logiciel::Obs => "OBS Studio",
            Logiciel::Streamlabs => "Streamlabs",
        }
    }
    /// Le nom court, dans les phrases (« Ferme OBS puis... »)
    pub fn court(self) -> &'static str {
        match self {
            Logiciel::Obs => "OBS",
            Logiciel::Streamlabs => "Streamlabs",
        }
    }
    /// « d'OBS », « de Streamlabs » (dans les phrases)
    pub fn de(self) -> &'static str {
        match self {
            Logiciel::Obs => "d'OBS",
            Logiciel::Streamlabs => "de Streamlabs",
        }
    }
    pub fn depuis_id(id: &str) -> Option<Logiciel> {
        match id {
            "obs" => Some(Logiciel::Obs),
            "streamlabs" => Some(Logiciel::Streamlabs),
            _ => None,
        }
    }
}

/// La plateforme où l'on streame
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Plateforme {
    Twitch,
    YouTube,
    Kick,
    Facebook,
    Autre,
}

impl Plateforme {
    pub const TOUTES: [Plateforme; 5] =
        [Plateforme::Twitch, Plateforme::YouTube, Plateforme::Kick, Plateforme::Facebook, Plateforme::Autre];

    pub fn id(self) -> &'static str {
        match self {
            Plateforme::Twitch => "twitch",
            Plateforme::YouTube => "youtube",
            Plateforme::Kick => "kick",
            Plateforme::Facebook => "facebook",
            Plateforme::Autre => "autre",
        }
    }
    pub fn nom(self) -> &'static str {
        match self {
            Plateforme::Twitch => "Twitch",
            Plateforme::YouTube => "YouTube",
            Plateforme::Kick => "Kick",
            Plateforme::Facebook => "Facebook",
            Plateforme::Autre => "Autre",
        }
    }
    pub fn depuis_id(id: &str) -> Option<Plateforme> {
        Plateforme::TOUTES.into_iter().find(|p| p.id() == id)
    }
    /// Le débit vidéo le plus haut accepté (kb/s), H.264
    pub fn plafond_video(self) -> u32 {
        match self {
            Plateforme::Twitch => 6000,
            Plateforme::YouTube => 12000,
            Plateforme::Kick => 8000,
            Plateforme::Facebook => 9000,
            Plateforme::Autre => 6000,
        }
    }
    /// Le débit du son (kb/s) : 160 (Twitch, Kick), 128 suffit pour YouTube et Facebook
    pub fn debit_son(self) -> u32 {
        match self {
            Plateforme::YouTube | Plateforme::Facebook => 128,
            _ => 160,
        }
    }
    /// Le service n'impose pas lui-même l'image clé à 2 secondes (serveur personnalisé) : Nevermind doit l'écrire
    pub fn image_cle_a_ecrire(self) -> bool {
        matches!(self, Plateforme::Kick)
    }
}

/// Le fabricant d'une carte graphique (d'après le numéro du fabricant donné par Windows)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fabricant {
    Nvidia,
    Amd,
    Intel,
    Autre,
}

impl Fabricant {
    /// Le numéro PCI du fabricant (DXGI_ADAPTER_DESC1.VendorId)
    pub fn depuis_numero(n: u32) -> Fabricant {
        match n {
            0x10DE => Fabricant::Nvidia,
            0x1002 | 0x1022 => Fabricant::Amd,
            0x8086 => Fabricant::Intel,
            _ => Fabricant::Autre,
        }
    }
}

/// Une carte graphique du PC
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carte {
    pub nom: String,
    pub fabricant: Fabricant,
}

/// L'encodeur vidéo (H.264)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encodeur {
    /// NVIDIA NVENC
    Nvenc,
    /// AMD AMF
    Amf,
    /// Intel Quick Sync
    Qsv,
    /// Le processeur (x264)
    X264,
}

impl Encodeur {
    /// Qui encode, en mots simples (« encodé par ta carte NVIDIA »)
    pub fn par(self) -> &'static str {
        match self {
            Encodeur::Nvenc => "ta carte NVIDIA",
            Encodeur::Amf => "ta carte AMD",
            Encodeur::Qsv => "la puce graphique Intel",
            Encodeur::X264 => "le processeur",
        }
    }
    /// Le nom court (résumé)
    pub fn court(self) -> &'static str {
        match self {
            Encodeur::Nvenc => "Carte NVIDIA",
            Encodeur::Amf => "Carte AMD",
            Encodeur::Qsv => "Puce Intel",
            Encodeur::X264 => "Processeur",
        }
    }
    /// La valeur du mode Simple d'OBS et de Streamlabs (`[SimpleOutput] StreamEncoder`), la même de 28 à 33
    pub fn simple(self) -> &'static str {
        match self {
            Encodeur::Nvenc => "nvenc",
            Encodeur::Amf => "amd",
            Encodeur::Qsv => "qsv",
            Encodeur::X264 => "x264",
        }
    }
    /// L'identifiant réel du mode Avancé (`[AdvOut] Encoder`), selon la version d'OBS (inconnue : la plus récente)
    pub fn avance(self, version: Option<Version>) -> &'static str {
        let v = version.map(|v| v.majeure).unwrap_or(99);
        match self {
            Encodeur::Nvenc if v >= 31 => "obs_nvenc_h264_tex",
            Encodeur::Nvenc => "jim_nvenc",
            Encodeur::Qsv if v <= 28 => "obs_qsv11",
            Encodeur::Qsv => "obs_qsv11_v2",
            Encodeur::Amf => "h264_texture_amf",
            Encodeur::X264 => "obs_x264",
        }
    }
    /// La famille d'un identifiant d'encodeur lu dans un fichier (mode Simple ou Avancé), `None` : autre (HEVC, AV1...)
    pub fn famille(id: &str) -> Option<Encodeur> {
        let id = id.to_ascii_lowercase();
        if id.contains("hevc") || id.contains("av1") || id.contains("h265") {
            return None;
        }
        if id.contains("nvenc") {
            Some(Encodeur::Nvenc)
        } else if id.contains("qsv") {
            Some(Encodeur::Qsv)
        } else if id.contains("amd") || id.contains("amf") {
            Some(Encodeur::Amf)
        } else if id.contains("x264") {
            Some(Encodeur::X264)
        } else {
            None
        }
    }
    /// Le réglage de vitesse léger pour jouer et streamer en même temps : (clé du mode Simple, valeur). Quick Sync :
    /// rien (son réglage par défaut, équilibré, convient).
    pub fn preset_simple(self) -> Option<(&'static str, &'static str)> {
        match self {
            Encodeur::Nvenc => Some(("NVENCPreset2", "p5")),
            Encodeur::Amf => Some(("AMDPreset", "balanced")),
            Encodeur::Qsv => None,
            Encodeur::X264 => Some(("Preset", "veryfast")),
        }
    }
    /// Le même réglage dans streamEncoder.json (mode Avancé) : (clé, valeur). NVENC de 28 à 30 : « preset2 ».
    pub fn preset_avance(self, version: Option<Version>) -> Option<(&'static str, &'static str)> {
        let v = version.map(|v| v.majeure).unwrap_or(99);
        match self {
            Encodeur::Nvenc if v >= 31 => Some(("preset", "p5")),
            Encodeur::Nvenc => Some(("preset2", "p5")),
            Encodeur::Amf => Some(("preset", "balanced")),
            Encodeur::Qsv => None,
            Encodeur::X264 => Some(("preset", "veryfast")),
        }
    }
}

/// Une version d'OBS (`[General] LastVersion` de global.ini : majeure << 24 | mineure << 16 | correctif)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub majeure: u32,
    pub mineure: u32,
    pub correctif: u32,
}

impl Version {
    pub fn depuis_entier(n: u64) -> Option<Version> {
        let v = Version {
            majeure: ((n >> 24) & 0xff) as u32,
            mineure: ((n >> 16) & 0xff) as u32,
            correctif: (n & 0xffff) as u32,
        };
        (20..100).contains(&v.majeure).then_some(v)
    }
    /// « 33.0.1 » ou l'entier de LastVersion
    pub fn lire(t: &str) -> Option<Version> {
        let t = t.trim();
        if let Ok(n) = t.parse::<u64>() {
            return Version::depuis_entier(n);
        }
        let mut it = t.split(['.', '-', ' ']).map(|x| x.trim().parse::<u32>());
        let majeure = it.next()?.ok()?;
        let mineure = it.next().and_then(Result::ok).unwrap_or(0);
        let correctif = it.next().and_then(Result::ok).unwrap_or(0);
        (20..100).contains(&majeure).then_some(Version { majeure, mineure, correctif })
    }
    pub fn texte(self) -> String {
        format!("{}.{}.{}", self.majeure, self.mineure, self.correctif)
    }
}

/// Le numéro de modèle qui suit `prefixe` dans le nom (« gtx 1060 » : 1060), `None` s'il n'y en a pas
fn modele(nom: &str, prefixe: &str) -> Option<u32> {
    let mut reste = nom;
    while let Some(i) = reste.find(prefixe) {
        let apres = &reste[i + prefixe.len()..];
        let chiffres: String = apres.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(n) = chiffres.parse() {
            return Some(n);
        }
        reste = apres;
    }
    None
}

/// La carte NVIDIA sait encoder pour OBS (NVENC) : les petites cartes (GT 710 à 1030, la plupart des MX) n'ont pas
/// d'encodeur ; les vieilles (Fermi : GTX 400 et 500) non plus ; OBS 31 et plus demande au moins une Maxwell
/// (GTX 750, GTX 900), donc plus de Kepler (GTX 600, GTX 760 à 780, GT 640...).
pub fn nvenc_possible(nom: &str, version: Option<Version>) -> bool {
    let n = nom.to_lowercase();
    let recent = version.map(|v| v.majeure).unwrap_or(99) >= 31;
    if let Some(m) = modele(&n, "mx") {
        return m == 570;
    }
    if let Some(m) = modele(&n, "gt ") {
        if [710, 720, 730, 1010, 1030].contains(&m) || m < 600 {
            return false;
        }
        // (GT 630 à 740 : Kepler)
        return !recent;
    }
    if let Some(m) = modele(&n, "gtx ") {
        if m < 600 {
            return false;
        }
        let kepler = (600..700).contains(&m) || (760..800).contains(&m);
        return !(kepler && recent);
    }
    true
}

/// La carte AMD sait encoder (AMF) : depuis les Radeon HD 7000 (GCN)
pub fn amf_possible(nom: &str) -> bool {
    let n = nom.to_lowercase();
    match modele(&n, "hd ") {
        Some(m) => m >= 7000,
        None => true,
    }
}

/// La puce Intel sait encoder (Quick Sync) : toutes sauf les très vieilles (GMA)
pub fn qsv_possible(nom: &str) -> bool {
    !nom.to_lowercase().contains("gma")
}

/// L'encodeur choisi et la carte qui le donne
pub fn choisir_encodeur(cartes: &[Carte], version: Option<Version>) -> (Encodeur, Option<String>) {
    let trouve = |f: Fabricant, ok: &dyn Fn(&str) -> bool| cartes.iter().find(|c| c.fabricant == f && ok(&c.nom));
    if let Some(c) = trouve(Fabricant::Nvidia, &|n| nvenc_possible(n, version)) {
        return (Encodeur::Nvenc, Some(c.nom.clone()));
    }
    if let Some(c) = trouve(Fabricant::Amd, &amf_possible) {
        return (Encodeur::Amf, Some(c.nom.clone()));
    }
    if let Some(c) = trouve(Fabricant::Intel, &qsv_possible) {
        return (Encodeur::Qsv, Some(c.nom.clone()));
    }
    (Encodeur::X264, None)
}

/// Ce que le calcul reçoit
#[derive(Debug, Clone)]
pub struct Entree {
    /// L'envoi mesuré (kb/s)
    pub envoi_kbps: u32,
    pub cartes: Vec<Carte>,
    /// L'image de base (le canevas d'OBS, sinon l'écran)
    pub base: (u32, u32),
    pub plateforme: Plateforme,
    pub version: Option<Version>,
}

/// La qualité choisie
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub encodeur: Encodeur,
    /// La carte qui encode (son nom), `None` : le processeur
    pub carte: Option<String>,
    /// La taille de l'image envoyée
    pub sortie: (u32, u32),
    /// Images par seconde
    pub ips: u32,
    pub video_kbps: u32,
    pub son_kbps: u32,
    /// Images clés (secondes)
    pub image_cle_s: u32,
    pub plateforme: Plateforme,
    /// Débit utile trop faible : le stream est déconseillé
    pub deconseille: bool,
    /// Le débit vidéo est limité par la plateforme
    pub plafonne: bool,
    /// Le débit utile (70 % de l'envoi moins le son)
    pub utile_kbps: u32,
}

/// Les paliers (hauteur, images par seconde) et le débit utile à partir duquel on les prend
const PALIERS: [(u32, u32, u32); 5] =
    [(480, 30, 0), (720, 30, 2500), (720, 60, 3500), (900, 60, 5000), (1080, 60, 6000)];
/// Sous ce débit utile, le stream est déconseillé
pub const MINIMUM_KBPS: u32 = 1500;
/// Le débit vidéo le plus bas écrit, même pour une connexion très lente
pub const PLANCHER_KBPS: u32 = 400;

/// La largeur pour cette hauteur, à la forme de l'image de base, paire (les encodeurs le demandent)
pub fn largeur_pour(base: (u32, u32), h: u32) -> u32 {
    let (bw, bh) = (u64::from(base.0.max(2)), u64::from(base.1.max(2)));
    let w = ((bw * u64::from(h) + bh / 2) / bh) as u32;
    w + (w % 2)
}

/// Le calcul : la qualité à régler pour cette connexion, cette carte, cet écran et cette plateforme
pub fn calculer(e: &Entree) -> Plan {
    let (encodeur, carte) = choisir_encodeur(&e.cartes, e.version);
    let son = e.plateforme.debit_son();
    let utile = (u64::from(e.envoi_kbps) * 7 / 10).saturating_sub(u64::from(son)) as u32;
    let deconseille = utile < MINIMUM_KBPS;
    // Le palier d'après le débit utile, puis les limites : le processeur (720p30 au plus), l'image de base
    let mut k = PALIERS.iter().rposition(|p| utile >= p.2).unwrap_or(0);
    if encodeur == Encodeur::X264 {
        k = k.min(1);
    }
    let base = (e.base.0.max(2), e.base.1.max(2));
    while k > 0 && PALIERS[k].0 > base.1 {
        k -= 1;
    }
    let (h, ips, _) = PALIERS[k];
    let h = h.min(base.1) & !1;
    let sortie = if h == base.1 { (base.0 & !1, h) } else { (largeur_pour(base, h), h) };
    let plafond = e.plateforme.plafond_video();
    let video = utile.min(plafond);
    let video = (video / 100 * 100).max(PLANCHER_KBPS);
    Plan {
        encodeur,
        carte,
        sortie,
        ips,
        video_kbps: video,
        son_kbps: son,
        image_cle_s: 2,
        plateforme: e.plateforme,
        deconseille,
        plafonne: utile > plafond,
        utile_kbps: utile,
    }
}

/// Un nombre en français (« 6 000 »), avec l'espace fine insécable de Nevermind
pub fn nombre(n: u32) -> String {
    let t = n.to_string();
    let mut r = String::new();
    for (i, c) in t.chars().enumerate() {
        if i > 0 && (t.len() - i).is_multiple_of(3) {
            r.push('\u{202f}');
        }
        r.push(c);
    }
    r
}

/// Un débit en Mb/s (« 12 Mb/s », « 2,5 Mb/s »)
pub fn mbps(kbps: u32) -> String {
    if kbps >= 10_000 {
        format!("{} Mb/s", nombre((kbps + 500) / 1000))
    } else {
        let d = (kbps + 50) / 100;
        if d.is_multiple_of(10) { format!("{} Mb/s", d / 10) } else { format!("{},{} Mb/s", d / 10, d % 10) }
    }
}

impl Plan {
    /// « 1080p à 60 images par seconde »
    pub fn qualite(&self) -> String {
        format!("{}p à {} images par seconde", self.sortie.1, self.ips)
    }
    /// « 6 000 kb/s »
    pub fn debit(&self) -> String {
        format!("{} kb/s", nombre(self.video_kbps))
    }
    /// La phrase simple : « 1080p à 60 images par seconde, encodé par ta carte NVIDIA, 6 000 kb/s »
    pub fn phrase(&self) -> String {
        format!("{}, encodé par {}, {}", self.qualite(), self.encodeur.par(), self.debit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nv(n: &str) -> Carte {
        Carte { nom: n.into(), fabricant: Fabricant::Nvidia }
    }
    fn amd(n: &str) -> Carte {
        Carte { nom: n.into(), fabricant: Fabricant::Amd }
    }
    fn intel(n: &str) -> Carte {
        Carte { nom: n.into(), fabricant: Fabricant::Intel }
    }
    fn entree(envoi: u32, cartes: Vec<Carte>, p: Plateforme) -> Entree {
        Entree { envoi_kbps: envoi, cartes, base: (1920, 1080), plateforme: p, version: Version::lire("33.0.1") }
    }

    #[test]
    fn versions() {
        assert_eq!(Version::lire("553648129"), Some(Version { majeure: 33, mineure: 0, correctif: 1 }));
        assert_eq!(Version::lire(&((30u64 << 24) | (2 << 16) | 3).to_string()).unwrap().texte(), "30.2.3");
        assert_eq!(Version::lire("28.0.0").unwrap().majeure, 28);
        assert_eq!(Version::lire("31.1").unwrap().mineure, 1);
        assert_eq!(Version::lire(""), None);
        assert_eq!(Version::lire("abc"), None);
        assert_eq!(Version::lire("5"), None);
    }

    #[test]
    fn encodeurs_selon_la_version() {
        let v28 = Version::lire("28.1.2");
        let v30 = Version::lire("30.2.3");
        let v31 = Version::lire("31.0.0");
        assert_eq!(Encodeur::Nvenc.avance(v28), "jim_nvenc");
        assert_eq!(Encodeur::Nvenc.avance(v30), "jim_nvenc");
        assert_eq!(Encodeur::Nvenc.avance(v31), "obs_nvenc_h264_tex");
        assert_eq!(Encodeur::Nvenc.avance(None), "obs_nvenc_h264_tex");
        assert_eq!(Encodeur::Qsv.avance(v28), "obs_qsv11");
        assert_eq!(Encodeur::Qsv.avance(v30), "obs_qsv11_v2");
        assert_eq!(Encodeur::Amf.avance(v28), "h264_texture_amf");
        assert_eq!(Encodeur::X264.avance(v31), "obs_x264");
        assert_eq!(Encodeur::Nvenc.preset_avance(v30), Some(("preset2", "p5")));
        assert_eq!(Encodeur::Nvenc.preset_avance(v31), Some(("preset", "p5")));
        assert_eq!(Encodeur::famille("jim_nvenc"), Some(Encodeur::Nvenc));
        assert_eq!(Encodeur::famille("obs_nvenc_hevc_tex"), None);
        assert_eq!(Encodeur::famille("h264_texture_amf"), Some(Encodeur::Amf));
        assert_eq!(Encodeur::famille("amd"), Some(Encodeur::Amf));
        assert_eq!(Encodeur::famille("obs_qsv11_v2"), Some(Encodeur::Qsv));
        assert_eq!(Encodeur::famille("x264"), Some(Encodeur::X264));
    }

    #[test]
    fn cartes_nvidia() {
        let v33 = Version::lire("33.0.1");
        let v30 = Version::lire("30.2.3");
        assert!(nvenc_possible("NVIDIA GeForce RTX 4070", v33));
        assert!(nvenc_possible("NVIDIA GeForce GTX 1060 6GB", v33));
        assert!(nvenc_possible("NVIDIA GeForce GTX 750 Ti", v33));
        assert!(nvenc_possible("NVIDIA GeForce GTX 970", v33));
        assert!(!nvenc_possible("NVIDIA GeForce GT 1030", v33));
        assert!(!nvenc_possible("NVIDIA GeForce GT 710", v30));
        assert!(!nvenc_possible("NVIDIA GeForce MX450", v33));
        assert!(nvenc_possible("NVIDIA GeForce MX570", v33));
        assert!(!nvenc_possible("NVIDIA GeForce GTX 580", v30));
        assert!(!nvenc_possible("NVIDIA GeForce GTX 770", v33));
        assert!(nvenc_possible("NVIDIA GeForce GTX 770", v30));
        assert!(!nvenc_possible("NVIDIA GeForce GTX 660", None));
        assert!(nvenc_possible("NVIDIA Quadro P2000", v33));
        assert!(nvenc_possible("NVIDIA RTX A2000", v33));
        assert!(amf_possible("AMD Radeon RX 6600"));
        assert!(amf_possible("AMD Radeon(TM) Graphics"));
        assert!(amf_possible("AMD Radeon HD 7850"));
        assert!(!amf_possible("ATI Radeon HD 5450"));
        assert!(qsv_possible("Intel(R) UHD Graphics 630"));
        assert!(!qsv_possible("Mobile Intel(R) 4 Series Express Chipset Family (GMA 4500)"));
    }

    #[test]
    fn ordre_des_cartes() {
        let v = Version::lire("33.0.1");
        let (e, c) = choisir_encodeur(&[intel("Intel(R) UHD Graphics 770"), nv("NVIDIA GeForce RTX 3060")], v);
        assert_eq!((e, c.as_deref()), (Encodeur::Nvenc, Some("NVIDIA GeForce RTX 3060")));
        let (e, _) = choisir_encodeur(&[intel("Intel(R) UHD Graphics 630"), amd("AMD Radeon RX 580")], v);
        assert_eq!(e, Encodeur::Amf);
        // (une GT 1030 sans encodeur, à côté de la puce Intel : Quick Sync)
        let (e, c) = choisir_encodeur(&[nv("NVIDIA GeForce GT 1030"), intel("Intel(R) HD Graphics 530")], v);
        assert_eq!((e, c.as_deref()), (Encodeur::Qsv, Some("Intel(R) HD Graphics 530")));
        assert_eq!(choisir_encodeur(&[], v), (Encodeur::X264, None));
        assert_eq!(
            choisir_encodeur(&[Carte { nom: "Microsoft Basic Render Driver".into(), fabricant: Fabricant::Autre }], v)
                .0,
            Encodeur::X264
        );
        assert_eq!(Fabricant::depuis_numero(0x10de), Fabricant::Nvidia);
        assert_eq!(Fabricant::depuis_numero(0x1002), Fabricant::Amd);
        assert_eq!(Fabricant::depuis_numero(0x8086), Fabricant::Intel);
        assert_eq!(Fabricant::depuis_numero(0x1414), Fabricant::Autre);
    }

    #[test]
    fn paliers_twitch() {
        let c = || vec![nv("NVIDIA GeForce RTX 3060")];
        // (débit utile = 0,7 x envoi - 160)
        let p = calculer(&entree(20_000, c(), Plateforme::Twitch));
        assert_eq!((p.sortie, p.ips, p.video_kbps, p.plafonne), ((1920, 1080), 60, 6000, true));
        assert_eq!(p.phrase(), "1080p à 60 images par seconde, encodé par ta carte NVIDIA, 6\u{202f}000 kb/s");
        let p = calculer(&entree(8_000, c(), Plateforme::Twitch)); // utile 5440
        assert_eq!((p.sortie, p.ips, p.video_kbps), ((1600, 900), 60, 5400));
        let p = calculer(&entree(6_000, c(), Plateforme::Twitch)); // utile 4040
        assert_eq!((p.sortie, p.ips, p.video_kbps), ((1280, 720), 60, 4000));
        let p = calculer(&entree(4_000, c(), Plateforme::Twitch)); // utile 2640
        assert_eq!((p.sortie, p.ips, p.video_kbps), ((1280, 720), 30, 2600));
        let p = calculer(&entree(3_000, c(), Plateforme::Twitch)); // utile 1940
        assert_eq!((p.sortie, p.ips, p.video_kbps, p.deconseille), ((854, 480), 30, 1900, false));
        assert_eq!(p.son_kbps, 160);
        assert_eq!(p.image_cle_s, 2);
    }

    #[test]
    fn connexion_lente() {
        // 1,5 Mb/s en envoi : utile 890, déconseillé, 480p30 au plus bas
        let p = calculer(&entree(1_500, vec![nv("NVIDIA GeForce RTX 3060")], Plateforme::Twitch));
        assert!(p.deconseille);
        assert_eq!((p.sortie, p.ips, p.video_kbps), ((854, 480), 30, 800));
        // une connexion presque nulle : jamais sous le plancher
        let p = calculer(&entree(300, vec![], Plateforme::Kick));
        assert!(p.deconseille);
        assert_eq!(p.video_kbps, PLANCHER_KBPS);
        assert_eq!(calculer(&entree(0, vec![], Plateforme::Twitch)).video_kbps, PLANCHER_KBPS);
    }

    #[test]
    fn plateformes() {
        let c = || vec![nv("NVIDIA GeForce RTX 4080")];
        let p = calculer(&entree(50_000, c(), Plateforme::Kick));
        assert_eq!((p.sortie.1, p.ips, p.video_kbps), (1080, 60, 8000));
        let p = calculer(&entree(50_000, c(), Plateforme::YouTube));
        assert_eq!((p.video_kbps, p.son_kbps), (12000, 128));
        let p = calculer(&entree(12_000, c(), Plateforme::YouTube)); // utile 8272
        assert_eq!((p.video_kbps, p.plafonne), (8200, false));
        let p = calculer(&entree(50_000, c(), Plateforme::Facebook));
        assert_eq!(p.video_kbps, 9000);
        let p = calculer(&entree(50_000, c(), Plateforme::Autre));
        assert_eq!(p.video_kbps, 6000);
        assert!(Plateforme::Kick.image_cle_a_ecrire());
        assert!(!Plateforme::Twitch.image_cle_a_ecrire());
        for p in Plateforme::TOUTES {
            assert_eq!(Plateforme::depuis_id(p.id()), Some(p));
        }
    }

    #[test]
    fn vieux_pc_sans_carte_graphique() {
        // Le processeur encode : 720p30 au plus, même avec une très bonne connexion
        let p = calculer(&entree(100_000, vec![], Plateforme::Twitch));
        assert_eq!((p.encodeur, p.sortie, p.ips), (Encodeur::X264, (1280, 720), 30));
        assert_eq!(p.video_kbps, 6000);
        assert_eq!(p.phrase(), "720p à 30 images par seconde, encodé par le processeur, 6\u{202f}000 kb/s");
        // un vieil écran 1366 x 768 et une vieille carte Intel
        let mut e = entree(10_000, vec![intel("Intel(R) HD Graphics 4000")], Plateforme::Twitch);
        e.base = (1366, 768);
        let p = calculer(&e);
        assert_eq!((p.encodeur, p.sortie, p.ips), (Encodeur::Qsv, (1282, 720), 60));
        // un écran plus petit que 720p
        e.base = (1024, 600);
        let p = calculer(&e);
        assert_eq!((p.sortie, p.ips), ((820, 480), 30));
    }

    #[test]
    fn ecrans_non_16_9() {
        let mut e = entree(20_000, vec![nv("NVIDIA GeForce RTX 3070")], Plateforme::Twitch);
        e.base = (2560, 1440);
        assert_eq!(calculer(&e).sortie, (1920, 1080));
        e.base = (3440, 1440); // ultra large
        assert_eq!(calculer(&e).sortie, (2580, 1080));
        e.base = (1920, 1200); // 16:10
        assert_eq!(calculer(&e).sortie, (1728, 1080));
        e.base = (1280, 1024);
        let p = calculer(&e);
        assert_eq!(p.sortie, (1126, 900));
        assert_eq!(largeur_pour((1920, 1080), 480), 854);
        assert_eq!(largeur_pour((1920, 1080), 720), 1280);
        assert_eq!(largeur_pour((0, 0), 480), 480);
    }

    #[test]
    fn textes() {
        assert_eq!(nombre(6000), "6\u{202f}000");
        assert_eq!(nombre(12500), "12\u{202f}500");
        assert_eq!(nombre(800), "800");
        assert_eq!(mbps(12_340), "12 Mb/s");
        assert_eq!(mbps(2_500), "2,5 Mb/s");
        assert_eq!(mbps(3_000), "3 Mb/s");
        assert_eq!(mbps(940), "0,9 Mb/s");
        assert_eq!(Encodeur::Qsv.par(), "la puce graphique Intel");
        assert_eq!(Logiciel::depuis_id("streamlabs"), Some(Logiciel::Streamlabs));
    }
}
