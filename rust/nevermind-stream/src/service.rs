//! La plateforme lue dans service.json (OBS : dans le dossier du profil ; Streamlabs : dans son dossier de réglages).
//!
//! RÈGLE : la clé de stream n'est JAMAIS lue dans une variable. Le fichier est lu dans une structure qui ne connaît que
//! trois champs (`type`, `settings.service`, `settings.server`) : la clé, le mot de passe et le nom d'utilisateur
//! sont sautés par la lecture elle-même (serde ignore les champs inconnus sans les garder). Du serveur, seul l'hôte
//! est gardé (certains serveurs mettent la clé dans le chemin de l'adresse). Rien n'est jamais écrit dans service.json.

use crate::calcul::Plateforme;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct Fichier {
    #[serde(rename = "type", default)]
    genre: String,
    #[serde(default)]
    settings: Option<Reglages>,
}

#[derive(Deserialize)]
struct Reglages {
    #[serde(default)]
    service: Option<String>,
    #[serde(default)]
    server: Option<String>,
}

/// Ce que Nevermind garde de service.json : rien d'autre
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Service {
    /// `rtmp_common`, `rtmp_custom`, `whip_custom`...
    pub genre: String,
    /// Le nom du service (« Twitch », « YouTube - RTMPS »), pour `rtmp_common`
    pub nom: String,
    /// L'hôte du serveur, sans le chemin ni le port (« live.twitch.tv »)
    pub hote: String,
}

/// L'hôte d'une adresse de serveur (« rtmps://a.b.net:443/app/xyz » : « a.b.net ») ; vide pour « auto »
pub fn hote(adresse: &str) -> String {
    let a = adresse.trim();
    let sans_protocole = a.split_once("://").map_or(a, |(_, r)| r);
    let avant_chemin = sans_protocole.split(['/', '?', '#']).next().unwrap_or("");
    // (un nom d'utilisateur et un mot de passe dans l'adresse ne sont jamais gardés)
    let sans_compte = avant_chemin.rsplit_once('@').map_or(avant_chemin, |(_, h)| h);
    let h = if let Some(r) = sans_compte.strip_prefix('[') {
        r.split(']').next().unwrap_or("")
    } else {
        sans_compte.split(':').next().unwrap_or("")
    };
    if h.eq_ignore_ascii_case("auto") || (!h.contains('.') && !h.eq_ignore_ascii_case("localhost")) {
        return String::new();
    }
    h.to_ascii_lowercase()
}

/// Lit le texte de service.json (ou de sa copie service.json.bak) : `None` s'il est absent ou abîmé
pub fn lire_texte(texte: &str) -> Option<Service> {
    let f: Fichier = serde_json::from_str(texte.trim_start_matches('\u{feff}')).ok()?;
    let r = f.settings;
    Some(Service {
        genre: f.genre,
        nom: r.as_ref().and_then(|r| r.service.clone()).unwrap_or_default(),
        hote: r.as_ref().and_then(|r| r.server.as_deref()).map(hote).unwrap_or_default(),
    })
}

/// service.json d'un dossier (repli sur service.json.bak, comme OBS)
pub fn lire(dossier: &Path) -> Option<Service> {
    for nom in ["service.json", "service.json.bak"] {
        if let Ok(o) = std::fs::read(dossier.join(nom))
            && let Some(s) = lire_texte(&String::from_utf8_lossy(&o))
        {
            return Some(s);
        }
    }
    None
}

impl Service {
    /// La plateforme : par le nom du service (liste d'OBS), sinon par l'hôte du serveur. Kick n'est pas dans la
    /// liste d'OBS : il passe par un serveur personnalisé sur Amazon IVS (« fa723fc1b171 », ou « kick » dans le nom),
    /// le même domaine que Twitch (contribute.live-video.net).
    pub fn plateforme(&self) -> Option<Plateforme> {
        let n = self.nom.to_lowercase();
        let h = self.hote.as_str();
        if n.contains("kick") || h.contains("kick") || h.contains("fa723fc1b171") {
            return Some(Plateforme::Kick);
        }
        if n.contains("twitch") || h.contains("twitch") || h.ends_with("contribute.live-video.net") {
            return Some(Plateforme::Twitch);
        }
        if n.contains("youtube") || h.contains("youtube") {
            return Some(Plateforme::YouTube);
        }
        if n.contains("facebook") || h.contains("facebook") || h.contains("fbcdn") {
            return Some(Plateforme::Facebook);
        }
        if self.genre.is_empty() && n.is_empty() && h.is_empty() {
            return None;
        }
        Some(Plateforme::Autre)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotes() {
        assert_eq!(
            hote("rtmps://fa723fc1b171.global-contribute.live-video.net:443/app"),
            "fa723fc1b171.global-contribute.live-video.net"
        );
        assert_eq!(hote("rtmp://a.rtmp.youtube.com/live2"), "a.rtmp.youtube.com");
        assert_eq!(hote("auto"), "");
        assert_eq!(hote(""), "");
        assert_eq!(hote("rtmp://moi:motdepasse@serveur.exemple.fr:1935/live/CLE"), "serveur.exemple.fr");
        assert_eq!(hote("rtmp://[::1]:1935/x"), "");
        assert_eq!(hote("rtmp://localhost/x"), "localhost");
    }

    #[test]
    fn plateformes() {
        let s = |t: &str| lire_texte(t).and_then(|s| s.plateforme());
        assert_eq!(
            s(r#"{"type":"rtmp_common","settings":{"service":"Twitch","server":"auto","key":"live_123"}}"#),
            Some(Plateforme::Twitch)
        );
        assert_eq!(
            s(
                r#"{"type":"rtmp_common","settings":{"service":"YouTube - RTMPS","server":"rtmps://a.rtmps.youtube.com:443/live2"}}"#
            ),
            Some(Plateforme::YouTube)
        );
        assert_eq!(
            s(
                r#"{"type":"rtmp_custom","settings":{"server":"rtmps://fa723fc1b171.global-contribute.live-video.net:443/app","key":"sk_us-west"}}"#
            ),
            Some(Plateforme::Kick)
        );
        assert_eq!(
            s(r#"{"type":"rtmp_custom","settings":{"server":"rtmp://aps30.contribute.live-video.net/app/"}}"#),
            Some(Plateforme::Twitch)
        );
        assert_eq!(s(r#"{"type":"rtmp_common","settings":{"service":"Facebook Live"}}"#), Some(Plateforme::Facebook));
        assert_eq!(
            s(r#"{"type":"rtmp_custom","settings":{"server":"rtmp://live.exemple.fr/app"}}"#),
            Some(Plateforme::Autre)
        );
        assert_eq!(s(r#"{"type":"whip_custom","settings":{}}"#), Some(Plateforme::Autre));
        assert_eq!(s(r#"{}"#), None);
        assert_eq!(s("pas du json"), None);
        assert_eq!(
            s("\u{feff}{\"type\":\"rtmp_common\",\"settings\":{\"service\":\"Twitch\"}}"),
            Some(Plateforme::Twitch)
        );
    }

    #[test]
    fn la_cle_n_est_jamais_gardee() {
        let t = r#"{"type":"rtmp_custom","settings":{"server":"rtmp://moi:SECRET-MDP@srv.exemple.fr/live/SECRET-CHEMIN","key":"SECRET-CLE","password":"SECRET-MDP2","username":"SECRET-NOM","bearer_token":"SECRET-JETON","use_auth":true}}"#;
        let s = lire_texte(t).unwrap();
        let vu = format!("{s:?}");
        assert!(!vu.contains("SECRET"), "{vu}");
        assert_eq!(s.hote, "srv.exemple.fr");
    }
}
