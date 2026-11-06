//! Le type des données échangées avec un serveur, déduit de son nom, de son port, des volumes et de son
//! propriétaire (`$DataTypes`, `Get-DestType`, trafic.ps1). Le contenu, chiffré, n'est jamais lu.

use regex::Regex;
use std::sync::LazyLock;

#[derive(Clone, Debug, PartialEq)]
pub struct Sorte {
    pub id: &'static str,
    pub libelle: &'static str,
    pub couleur: &'static str,
    pub texte: &'static str,
}

const fn s(id: &'static str, libelle: &'static str, couleur: &'static str, texte: &'static str) -> Sorte {
    Sorte { id, libelle, couleur, texte }
}

/// Les types reconnus d'après le nom du serveur, dans l'ordre où ils sont essayés
static TYPES: LazyLock<Vec<(Regex, Sorte)>> = LazyLock::new(|| {
    let l: Vec<(&str, Sorte)> = vec![
        (
            r"doubleclick|googlesyndication|googleadservices|adservice|\bads?\.|adnxs|criteo|taboola|outbrain|scorecardresearch|connect\.facebook|facebook\.net|google-analytics|googletagmanager|analytics|segment\.(io|com)|mixpanel|amplitude|hotjar|appsflyer|adjust\.com|branch\.io|doubleverify|moatads|adsrvr|pubmatic|rubiconproject|quantserve",
            s(
                "ads",
                "Publicité et suivi",
                "#F5A524",
                "Ce que tu regardes et où tu cliques, pour afficher de la publicité et mesurer l'audience.",
            ),
        ),
        (
            r"telemetry|vortex|watson\.|events\.data|browser\.events|self\.events|settings-win|sentry\.io|bugsnag|crashlytics|datadoghq|newrelic|nr-data|app-measurement|firebaselogging|clientlogging|metrics|diagnostic|\.events\.|beacons?\.|stats\.|feedback",
            s(
                "telemetry",
                "Statistiques d'utilisation",
                "#B18CFF",
                "Rapports sur le fonctionnement du programme : plantages, performances, fonctions utilisées. Pas tes fichiers.",
            ),
        ),
        (
            r"smartscreen|wdcp\.|wd\.microsoft|defender|safebrowsing|malwarebytes|avast|avg\.com|kaspersky|bitdefender|eset\.|norton|mcafee",
            s(
                "security",
                "Protection",
                "#22D37A",
                "Vérifie des fichiers ou des adresses de sites auprès de l'éditeur de sécurité.",
            ),
        ),
        (
            r"ocsp|\bcrl|pki\.|digicert|sectigo|letsencrypt|lencr\.org|globalsign|verisign|usertrust|comodoca|entrust|godaddy\.com/repository|ctldl\.windowsupdate",
            s(
                "cert",
                "Vérification de certificats",
                "#A6A1BC",
                "Vérifie que les sites et les programmes sont authentiques. Très peu de données.",
            ),
        ),
        (
            r"(^|\.)login\.|\bauth|oauth|accounts\.|identity|\bsso\.|signin|msauth|passport",
            s("auth", "Connexion à ton compte", "#4EA8FF", "Identifiants chiffrés pour ouvrir ou garder ta session."),
        ),
        (
            r"anydesk|teamviewer|rustdesk|screenconnect|splashtop|logmein",
            s(
                "remote",
                "Prise en main à distance",
                "#F04438",
                "Images de ton écran, clavier et souris quand une session à distance est ouverte.",
            ),
        ),
        (
            r"anthropic|claude\.ai|openai|chatgpt|oaiusercontent|gemini|bard\.google|copilot|perplexity|mistral\.ai",
            s(
                "ai",
                "Assistant IA",
                "#FF5CC8",
                "Tes questions et le contexte que tu envoies à l'assistant (textes, fichiers ouverts).",
            ),
        ),
        (
            r"onedrive|sharepoint|dropbox|drive\.google|docs\.google|googleusercontent|icloud|box\.com|\bmega\.(nz|io)|backblaze|pcloud|nextcloud",
            s(
                "sync",
                "Synchronisation de fichiers",
                "#4EA8FF",
                "Tes fichiers envoyés vers (ou récupérés depuis) ton espace de stockage en ligne.",
            ),
        ),
        (
            r"discord|whatsapp|telegram|signal\.org|teams|skype|slack|zoom\.us|messenger|trouter|\.gateway\.",
            s(
                "chat",
                "Messagerie et appels",
                "#4EA8FF",
                "Tes messages, ta voix ou ta vidéo pendant les appels, et ta présence en ligne.",
            ),
        ),
        (
            r"googlevideo|youtube|ytimg|nflxvideo|netflix|twitch|ttvnw|jtvnw|primevideo|aiv-cdn|disney|dssott|spotify|scdn\.co|deezer|dzcdn|soundcloud|crunchyroll",
            s(
                "stream",
                "Vidéo ou musique",
                "#A6A1BC",
                "Tu reçois surtout de la vidéo ou du son. Ce qui part est minime (ce que tu regardes, ta position dans la vidéo).",
            ),
        ),
        (
            r"steamcommunity|steampowered|steamserver|valve\.net|riotgames|leagueoflegends|pvp\.net|epicgames|unrealengine|battle\.net|blizzard|ea\.com|origin\.com|ubisoft|ubi\.com|xboxlive|playstation|nintendo|netmarble|playfab|gamesparks|photonengine|faceit|easyanticheat|battleye",
            s(
                "game",
                "Jeu en ligne",
                "#22D37A",
                "Tes actions en jeu, le chat, ton compte et parfois les vérifications de l'anti-triche.",
            ),
        ),
        (
            r"windowsupdate|delivery\.mp\.microsoft|\bdl\.|download|update|steamcontent|epicgames-download|akamaized|akamai|cloudfront|fastly|cdn|edgesuite|edgekey|content",
            s(
                "update",
                "Mise à jour ou téléchargement",
                "#A6A1BC",
                "Le programme récupère des fichiers : mises à jour, jeux, images, pages.",
            ),
        ),
        (
            r"1e100\.net|amazonaws|azure|cloudapp|googleapis|gstatic|cloudflare|herokuapp|digitalocean|ovh\.|hetzner|linode|vultr",
            s(
                "cloud",
                "Serveur de l'éditeur",
                "#A6A1BC",
                "Échanges avec les serveurs du programme (hébergés dans un grand centre de données). Le contenu dépend du programme.",
            ),
        ),
    ];
    l.into_iter().map(|(rx, t)| (Regex::new(&format!("(?i){rx}")).expect("motif"), t)).collect()
});

/// Propriétaire connu : type de données le plus probable (`$OwnerTypes`)
static PROPRIOS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    [
        ("game", r"valve|riot games|epic games|blizzard|activision|electronic arts|ubisoft|netmarble|nexon|krafton|tencent|bandai|square enix|nintendo|sony interactive|take-two|rockstar|psyonix|mihoyo|cognosphere|garena|ncsoft|wargaming|bungie|faceit|embark|i3d\.net|multiplay"),
        ("chat", r"discord|telegram|whatsapp|zoom video|slack"),
        ("ai", r"anthropic|openai"),
        ("remote", r"teamviewer|anydesk|philandro"),
        ("stream", r"netflix|spotify|twitch|deezer"),
        ("ads", r"criteo|taboola|outbrain|pubmatic|rubicon|the trade desk|xandr|appnexus"),
    ]
    .into_iter()
    .map(|(id, rx)| (id, Regex::new(&format!("(?i){rx}")).expect("motif")))
    .collect()
});

pub const DNS: Sorte =
    s("dns", "Recherche d'adresses", "#A6A1BC", "Traduit les noms de sites en adresses. Très peu de données.");
pub const UPLOAD: Sorte = s(
    "upload",
    "Envoi important",
    "#F5A524",
    "Le programme envoie bien plus qu'il ne reçoit : fichiers, vidéo ou sauvegarde. À vérifier si tu ne sais pas pourquoi.",
);
pub const DOWNLOAD: Sorte =
    s("download", "Téléchargement", "#A6A1BC", "Le programme reçoit surtout des données (fichiers, contenus).");
pub const OWNED: Sorte = s(
    "owned",
    "Serveur d'une entreprise connue",
    "#A6A1BC",
    "On sait à qui appartient le serveur (indiqué sous son adresse), mais pas précisément ce qui est échangé.",
);
pub const UNKNOWN: Sorte = s(
    "unknown",
    "Serveur non identifié",
    "#655E7E",
    "Adresse sans nom ni propriétaire connu : impossible de savoir à quoi elle sert.",
);
pub const OTHER: Sorte =
    s("other", "Échanges avec le serveur", "#655E7E", "Le nom du serveur ne dit pas précisément ce qui est échangé.");
pub const LOCAL: Sorte = s(
    "local",
    "Appareils de ton réseau",
    "#A6A1BC",
    "Échanges avec des appareils de chez toi (box, imprimante, TV...) : rien ne sort sur Internet.",
);

fn par_id(id: &str) -> Sorte {
    TYPES.iter().find(|(_, t)| t.id == id).map(|(_, t)| t.clone()).unwrap_or(OTHER)
}

const MO: f64 = 1024.0 * 1024.0;

/// `Get-DestType` : `proprio` = (O, N) du propriétaire connu ; `outil_distance` : le programme est un logiciel de
/// prise en main à distance (`$App.Name -match $RemoteTools`)
pub fn sorte(
    nom: &str,
    port: i64,
    sortant: f64,
    entrant: f64,
    outil_distance: bool,
    proprio: Option<(&str, &str)>,
) -> Sorte {
    let n = nom.to_lowercase();
    if port == 53 || port == 853 {
        return DNS;
    }
    if !n.is_empty() {
        for (rx, t) in TYPES.iter() {
            if rx.is_match(&n) {
                return t.clone();
            }
        }
    }
    let own = proprio.map(|(o, nn)| format!("{o} {nn}").to_lowercase()).unwrap_or_default();
    if !own.trim().is_empty() {
        for (id, rx) in PROPRIOS.iter() {
            if rx.is_match(&own) {
                return par_id(id);
            }
        }
    }
    if outil_distance {
        return par_id("remote");
    }
    if sortant > 10.0 * MO && sortant > 3.0 * entrant {
        return UPLOAD;
    }
    if entrant > 3.0 * f64::max(1.0, sortant) && entrant > MO {
        return DOWNLOAD;
    }
    if n.is_empty() && !own.trim().is_empty() {
        return OWNED;
    }
    if n.is_empty() {
        return UNKNOWN;
    }
    OTHER
}
