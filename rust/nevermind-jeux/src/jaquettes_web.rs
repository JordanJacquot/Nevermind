//! « Jaquettes depuis Internet » (bibliotheque.ps1:147-279, `$CoverWork`, `Start-CoverDownload`, `Receive-Covers`) :
//! pour les jeux sans image, les images officielles connues, sinon la boutique Steam (identifiant connu ou recherche
//! par nom), puis Wikipédia. Seul le nom du jeu est envoyé. Rien n'est exécuté : seules des images (JPEG ou PNG) sont
//! gardées, téléchargées en https seulement (15 Mo au plus).

use crate::texte::{lettre_ou_chiffre, minuscule, sans_marques};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;
use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::UnicodeNormalization;

/// Le réseau (les tests donnent un faux Internet)
pub trait Internet: Send + Sync {
    /// Une demande GET : le code de réponse et le contenu, au plus `max` octets et `delai_s` secondes (au-delà : une
    /// erreur). `agent` : le nom du programme envoyé (`-UserAgent`).
    fn obtenir(&self, url: &str, delai_s: u32, agent: &str, max: usize) -> Result<(u16, Vec<u8>), String>;
    /// Une pause entre deux demandes (Wikipédia refuse les demandes trop rapprochées)
    fn attendre(&self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

/// Le vrai réseau, par WinHTTP (proxy du PC, https)
pub struct InternetWindows;

impl Internet for InternetWindows {
    fn obtenir(&self, url: &str, delai_s: u32, agent: &str, max: usize) -> Result<(u16, Vec<u8>), String> {
        #[cfg(windows)]
        {
            nevermind_noyau::windows::http::obtenir_borne(url, delai_s, agent, max)
        }
        #[cfg(not(windows))]
        {
            let _ = (url, delai_s, agent, max);
            Err("pas de réseau sur ce système".into())
        }
    }
}

/// Un jeu à chercher
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ATrouver {
    pub nom: String,
    pub appid: String,
}

/// Ce qui a été trouvé pour un jeu (les chemins des images, vides si rien) ; `net` : faux si le réseau a manqué
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Trouvee {
    pub nom: String,
    pub cover: String,
    pub hero: String,
    pub logo: String,
    pub net: bool,
}

/// Le nom envoyé avec les demandes d'images et à Wikipédia
const AGENT: &str = "Nevermind (jaquettes de la bibliotheque)";
/// `-TimeoutSec` des images, et des questions aux sites (boutique Steam, Wikipédia)
const DELAI_IMAGE: u32 = 15;
const DELAI_SITE: u32 = 10;
/// Une image de 3 Ko ou moins est une page d'erreur (`Length -gt 3KB`) ; plus de 15 Mo : pas une jaquette (et l'écran
/// ne montre pas une image plus grande)
const MIN_IMAGE: usize = 3 * 1024;
pub const MAX_IMAGE: usize = 15 * 1024 * 1024;
/// Une réponse JSON d'un site : 2 Mo au plus
const MAX_SITE: usize = 2 * 1024 * 1024;

/// Les images officielles des jeux que Wikipédia ne montre que par leur logo (`$OfficialArt`) : (clé, jaquette,
/// bannière)
const OFFICIELLES: &[(&str, &str, &str)] = &[
    (
        "valorant",
        "https://cmsassets.rgpub.io/sanity/images/dsfx7636/news/015a083717e9687de8a741cfceddb836775b5f9f-616x822.png?w=616&fm=jpg&q=85",
        "",
    ),
    (
        "leagueoflegends",
        "https://ddragon.leagueoflegends.com/cdn/img/champion/loading/Jinx_0.jpg",
        "https://ddragon.leagueoflegends.com/cdn/img/champion/splash/Jinx_0.jpg",
    ),
    (
        "teamfighttactics",
        "https://cmsassets.rgpub.io/sanity/images/dsfx7636/news/3b0ebb24308884cb9a4c05ab433ec804ee401081-3840x1458.png?h=900&fm=jpg&q=85",
        "https://cmsassets.rgpub.io/sanity/images/dsfx7636/news/3b0ebb24308884cb9a4c05ab433ec804ee401081-3840x1458.png?w=1920&fm=jpg&q=85",
    ),
];

/// `Get-OfficialArtKey` : minuscules, sans ®™©, seulement a-z et 0-9
pub fn cle_officielle(nom: &str) -> String {
    sans_marques(nom).chars().map(minuscule).filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).collect()
}

/// Les images officielles connues de ce jeu
pub fn officielle(nom: &str) -> Option<(&'static str, &'static str)> {
    let k = cle_officielle(nom.trim());
    OFFICIELLES.iter().find(|o| o.0 == k).map(|o| (o.1, o.2))
}

/// `$norm` : minuscules, sans accents, seulement les lettres et les chiffres
fn norme(s: &str) -> String {
    let bas: String = s.chars().map(minuscule).collect();
    bas.nfd().filter(|c| get_general_category(*c) != GeneralCategory::NonspacingMark && lettre_ou_chiffre(*c)).collect()
}

/// `[uri]::EscapeDataString` : tout sauf A-Z a-z 0-9 - _ . ~ en %XX (UTF-8)
fn echapper(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            r.push(b as char);
        } else {
            r.push_str(&format!("%{b:02X}"));
        }
    }
    r
}

/// Une vraie image (JPEG ou PNG), d'après ses premiers octets
fn est_image(o: &[u8]) -> bool {
    o.starts_with(&[0xFF, 0xD8, 0xFF]) || o.starts_with(b"\x89PNG\r\n\x1a\n")
}

/// `$get` : télécharge une image (https seulement) ; gardée seulement si c'est une vraie image de plus de 3 Ko
fn telecharger(net: &dyn Internet, url: &str, fichier: &Path) -> bool {
    if !url.starts_with("https://") {
        return false;
    }
    let ok = match net.obtenir(url, DELAI_IMAGE, AGENT, MAX_IMAGE) {
        Ok((code, corps)) if (200..300).contains(&code) && corps.len() > MIN_IMAGE && est_image(&corps) => {
            std::fs::write(fichier, &corps).is_ok()
        }
        _ => false,
    };
    if !ok && fichier.exists() {
        let _ = std::fs::remove_file(fichier);
    }
    ok
}

/// `Invoke-RestMethod` : le JSON d'un site (https seulement) ; `Err` avec le code si la réponse n'est pas 2xx
fn json_de(net: &dyn Internet, url: &str, agent: &str) -> Result<Value, String> {
    if !url.starts_with("https://") {
        return Err("adresse refusée".into());
    }
    let (code, corps) = net.obtenir(url, DELAI_SITE, agent, MAX_SITE)?;
    if !(200..300).contains(&code) {
        return Err(format!("Le serveur distant a retourné une erreur : ({code})."));
    }
    let t = String::from_utf8_lossy(&corps);
    serde_json::from_str(t.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())
}

fn texte(v: &Value, k: &str) -> String {
    crate::json::texte_de(v.get(k).unwrap_or(&Value::Null))
}

/// La clé des fichiers d'un jeu (`$key`) : son nom normalisé, ou « jeu » et un nombre tiré du nom
fn cle_fichiers(nom: &str) -> String {
    let k = norme(nom);
    if !k.is_empty() {
        return k;
    }
    // (FNV-1a : le même nombre pour le même nom, d'une fois à l'autre)
    let mut h: u32 = 0x811c_9dc5;
    for b in nom.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    format!("jeu{h}")
}

/// `$CoverWork` : cherche les images de chaque jeu, rangées dans `dossier`
pub fn chercher(net: &dyn Internet, dossier: &Path, jeux: &[ATrouver]) -> Vec<Trouvee> {
    let _ = std::fs::create_dir_all(dossier);
    let fichier = |nom: &str| dossier.join(nom);
    let chemin = |p: &Path| p.to_string_lossy().into_owned();
    let mut sortie = Vec::new();
    for g in jeux {
        let key = cle_fichiers(&g.nom);
        let mut res = Trouvee { nom: g.nom.clone(), net: true, ..Default::default() };
        let propre = sans_marques(&g.nom).trim().to_string();
        let mut id = g.appid.trim().to_string();
        // Les images officielles d'abord
        if let Some((cover, hero)) = officielle(&propre) {
            let f = fichier(&format!("{key}-officiel.jpg"));
            if !cover.is_empty() && telecharger(net, cover, &f) {
                res.cover = chemin(&f);
            }
            let h = fichier(&format!("{key}-officiel-banniere.jpg"));
            if !hero.is_empty() && telecharger(net, hero, &h) {
                res.hero = chemin(&h);
            }
            if !res.cover.is_empty() {
                sortie.push(res);
                continue;
            }
        }
        // La boutique Steam : par son identifiant, sinon par son nom
        if id.is_empty() {
            let url =
                format!("https://store.steampowered.com/api/storesearch/?term={}&l=french&cc=FR", echapper(&propre));
            match json_de(net, &url, "Nevermind") {
                Ok(j) => {
                    let b = norme(&propre);
                    for it in crate::biblio::liste(j.get("items")) {
                        let n = norme(&texte(&it, "name"));
                        let (ln, lb) = (n.chars().count() as f64, b.chars().count() as f64);
                        // (même jeu : même nom, ou l'un contient l'autre, « Overwatch » et « Overwatch 2 »)
                        let proche =
                            n == b || ((n.contains(&b) || b.contains(&n)) && ln.min(lb) / 1f64.max(ln.max(lb)) >= 0.6);
                        if proche {
                            id = texte(&it, "id");
                            break;
                        }
                    }
                }
                Err(_) => res.net = false,
            }
        }
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
            let f = fichier(&format!("{key}.jpg"));
            for u in [
                format!(
                    "https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{id}/library_600x900.jpg"
                ),
                format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{id}/library_600x900.jpg"),
            ] {
                if telecharger(net, &u, &f) {
                    res.cover = chemin(&f);
                    break;
                }
            }
            let h = fichier(&format!("{key}-banniere.jpg"));
            for u in [
                format!("https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{id}/library_hero.jpg"),
                format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{id}/header.jpg"),
            ] {
                if telecharger(net, &u, &h) {
                    res.hero = chemin(&h);
                    break;
                }
            }
        }
        // Pas sur Steam : l'image de la page Wikipédia du jeu. Plus haute que large : la jaquette ; sinon son logo.
        if res.cover.is_empty() {
            for langue in ["en", "fr"] {
                net.attendre(400);
                let url = format!(
                    "https://{langue}.wikipedia.org/api/rest_v1/page/summary/{}",
                    echapper(&propre.replace(' ', "_"))
                );
                let s = match json_de(net, &url, AGENT) {
                    Ok(s) => s,
                    Err(e) => {
                        if e.contains("429") {
                            res.net = false;
                        }
                        continue;
                    }
                };
                let img = s.get("originalimage").filter(|i| i.is_object());
                let sujet = format!("{} {}", texte(&s, "description"), texte(&s, "extract")).to_lowercase();
                let Some(img) = img else { continue };
                if texte(&s, "type") != "standard" || !(sujet.contains("jeu") || sujet.contains("game")) {
                    continue;
                }
                let src = texte(img, "source");
                let sans_question = src.split('?').next().unwrap_or("");
                let ext = Path::new(sans_question)
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                    .unwrap_or_default();
                if !matches!(ext.as_str(), ".jpg" | ".jpeg" | ".png") {
                    continue;
                }
                let (l, h) = (crate::json::nombre(img, "width"), crate::json::nombre(img, "height"));
                let bas = src.to_lowercase();
                if h > 1.15 * l {
                    let f = fichier(&format!("{key}-wiki{ext}"));
                    if telecharger(net, &src, &f) {
                        res.cover = chemin(&f);
                        break;
                    }
                } else if ext == ".png" && (bas.contains("logo") || bas.contains(".svg.png")) && res.logo.is_empty() {
                    let f = fichier(&format!("{key}-logo.png"));
                    if telecharger(net, &src, &f) {
                        res.logo = chemin(&f);
                    }
                }
            }
        }
        sortie.push(res);
    }
    sortie
}

/// Le dossier des jaquettes téléchargées
pub fn dossier(donnees: &Path) -> PathBuf {
    donnees.join("jaquettes")
}

/// `Receive-Covers` : range ce qui a été trouvé dans index.json (un jeu sans réseau ni image est laissé pour plus
/// tard) ; le nombre de jeux qui ont maintenant une image
pub fn ranger(
    ctx: &nevermind_noyau::Contexte,
    donnees: &Path,
    trouvees: &[Trouvee],
    maintenant: &str,
) -> Result<usize, String> {
    let f = dossier(donnees).join("index.json");
    let mut idx = match crate::json::lire_json_sur(Some(ctx), &f).0 {
        Some(Value::Object(o)) => o,
        _ => serde_json::Map::new(),
    };
    let mut n = 0;
    for r in trouvees {
        if !r.net && r.cover.is_empty() && r.logo.is_empty() {
            continue;
        }
        // (la table de PowerShell ne distingue pas les majuscules : une seule entrée par jeu)
        let anciennes: Vec<String> = idx.keys().filter(|k| k.to_lowercase() == r.nom.to_lowercase()).cloned().collect();
        for k in anciennes {
            idx.remove(&k);
        }
        idx.insert(
            r.nom.clone(),
            serde_json::json!({ "Cover": r.cover, "Hero": r.hero, "Logo": r.logo, "Date": maintenant }),
        );
        if !r.cover.is_empty() || !r.logo.is_empty() {
            n += 1;
        }
    }
    ctx.ecrire_json(&f, &Value::Object(idx), true, false)?;
    Ok(n)
}
