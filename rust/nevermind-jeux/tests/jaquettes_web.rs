//! Audit M10 (point 3) : « Jaquettes depuis Internet » ne cherchait plus rien (le réglage était seulement enregistré).
//! La version PowerShell (bibliotheque.ps1:147-279) : pour chaque jeu sans image, les images officielles connues,
//! sinon la boutique Steam (identifiant connu, ou recherche par nom), puis Wikipédia (en, puis fr, 400 ms avant
//! chaque demande) ; 15 s par image, 10 s par question à un site ; une image de 3 Ko ou moins n'est pas gardée ; pas
//! de réseau : réessayé plus tard. En plus en Rust : https seulement, une image de 15 Mo au plus, et seulement une
//! vraie image (JPEG ou PNG) est gardée.
//!
//! Faux Internet seulement : aucune demande ne part sur le vrai réseau.

use nevermind_jeux::BlocJeux;
use nevermind_jeux::jaquettes_web::{ATrouver, Internet, Trouvee, chercher};
use nevermind_jeux::systeme::SystemeEssai;
use nevermind_jeux::texte::Moment;
use nevermind_noyau::{Bloc, Contexte, Effet, Effets, dossier_essai};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const AGENT_IMAGES: &str = "Nevermind (jaquettes de la bibliotheque)";
const MAX: usize = 15 * 1024 * 1024;
const STEAM: &str = "https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps";
const CDN: &str = "https://cdn.cloudflare.steamstatic.com/steam/apps";

/// Une réponse du faux réseau : le code et le contenu, ou une erreur
type Reponse = Result<(u16, Vec<u8>), String>;

#[derive(Default)]
struct FauxInternet {
    reponses: Mutex<HashMap<String, Reponse>>,
    demandes: Mutex<Vec<(String, u32, String, usize)>>,
    pauses: Mutex<Vec<u64>>,
}

impl FauxInternet {
    fn repondre(&self, url: &str, code: u16, corps: Vec<u8>) {
        self.reponses.lock().unwrap().insert(url.into(), Ok((code, corps)));
    }
    fn json(&self, url: &str, v: Value) {
        self.repondre(url, 200, v.to_string().into_bytes());
    }
    fn echouer(&self, url: &str, e: &str) {
        self.reponses.lock().unwrap().insert(url.into(), Err(e.into()));
    }
    fn urls(&self) -> Vec<String> {
        self.demandes.lock().unwrap().iter().map(|d| d.0.clone()).collect()
    }
}

impl Internet for FauxInternet {
    fn obtenir(&self, url: &str, delai_s: u32, agent: &str, max: usize) -> Result<(u16, Vec<u8>), String> {
        self.demandes.lock().unwrap().push((url.into(), delai_s, agent.into(), max));
        match self.reponses.lock().unwrap().get(url) {
            Some(Ok((_, b))) if b.len() > max => Err("réponse trop grande".into()),
            Some(r) => r.clone(),
            None => Ok((404, b"introuvable".to_vec())),
        }
    }
    fn attendre(&self, ms: u64) {
        self.pauses.lock().unwrap().push(ms);
    }
}

fn jpeg(n: usize) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0];
    v.resize(n, 0x11);
    v
}

fn png(n: usize) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.resize(n, 0x22);
    v
}

fn wiki(net: &FauxInternet, langue: &str, titre: &str, source: &str, l: u32, h: u32) {
    net.json(
        &format!("https://{langue}.wikipedia.org/api/rest_v1/page/summary/{titre}"),
        json!({ "type": "standard", "description": "jeu vidéo", "extract": "Un jeu.",
            "originalimage": { "source": source, "width": l, "height": h } }),
    );
}

fn dossier(nom: &str) -> PathBuf {
    let d = dossier_essai(nom);
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn jeu(nom: &str, appid: &str) -> ATrouver {
    ATrouver { nom: nom.into(), appid: appid.into() }
}

fn chemin(d: &Path, f: &str) -> String {
    d.join(f).to_string_lossy().into_owned()
}

#[test]
fn la_recherche_suit_la_version_powershell() {
    let d = dossier("jeux-jaquettes-recherche");
    let net = FauxInternet::default();
    // Steam, identifiant connu : la jaquette au premier essai, la bannière au second
    net.repondre(&format!("{STEAM}/123/library_600x900.jpg"), 200, jpeg(5000));
    net.repondre(&format!("{CDN}/123/header.jpg"), 200, jpeg(4000));
    // Les images officielles (League of Legends) : sans passer par Steam
    net.repondre("https://ddragon.leagueoflegends.com/cdn/img/champion/loading/Jinx_0.jpg", 200, jpeg(6000));
    net.repondre("https://ddragon.leagueoflegends.com/cdn/img/champion/splash/Jinx_0.jpg", 200, jpeg(7000));
    // Pas d'identifiant : la recherche de la boutique (même jeu : l'un contient l'autre, assez proche)
    net.json(
        "https://store.steampowered.com/api/storesearch/?term=Jeu%20Inconnu&l=french&cc=FR",
        json!({ "items": [{ "name": "Autre chose", "id": 5 }, { "name": "Jeu Inconnu 2", "id": 77 }] }),
    );
    net.repondre(&format!("{STEAM}/77/library_600x900.jpg"), 200, jpeg(5000));
    // Pas sur Steam : Wikipédia, une image plus haute que large (la jaquette)
    net.json("https://store.steampowered.com/api/storesearch/?term=Petit%20Jeu&l=french&cc=FR", json!({ "items": [] }));
    wiki(&net, "en", "Petit_Jeu", "https://upload.wikimedia.org/a/Petit.png?utm_source=x", 300, 450);
    net.repondre("https://upload.wikimedia.org/a/Petit.png?utm_source=x", 200, png(5000));
    // Pas de réseau pour la boutique, un logo sur Wikipédia en anglais, trop de demandes en français
    net.echouer("https://store.steampowered.com/api/storesearch/?term=Logo%20Jeu&l=french&cc=FR", "délai dépassé");
    wiki(&net, "en", "Logo_Jeu", "https://upload.wikimedia.org/Logo_Jeu_logo.svg.png", 800, 300);
    net.repondre("https://upload.wikimedia.org/Logo_Jeu_logo.svg.png", 200, png(4000));
    net.repondre("https://fr.wikipedia.org/api/rest_v1/page/summary/Logo_Jeu", 429, b"trop".to_vec());
    // Une image en http (jamais demandée), puis une image trop petite (pas gardée)
    net.json("https://store.steampowered.com/api/storesearch/?term=Faux%20Jeu&l=french&cc=FR", json!({ "items": [] }));
    wiki(&net, "en", "Faux_Jeu", "http://upload.wikimedia.org/Faux.jpg", 300, 450);
    wiki(&net, "fr", "Faux_Jeu", "https://upload.wikimedia.org/Petite.jpg", 300, 450);
    net.repondre("https://upload.wikimedia.org/Petite.jpg", 200, jpeg(3072));
    // Une page web à la place de l'image, puis une image trop grande : rien de gardé
    net.repondre(&format!("{STEAM}/9/library_600x900.jpg"), 200, b"<html>".repeat(1000));
    net.repondre(&format!("{CDN}/9/library_600x900.jpg"), 200, jpeg(MAX + 1));

    let jeux = [
        jeu("Mon Jeu Steam", "123"),
        jeu("League of Legends", ""),
        jeu("Jeu Inconnu", ""),
        jeu("Petit Jeu", ""),
        jeu("Logo Jeu", ""),
        jeu("Faux Jeu", ""),
        jeu("Page Web", "9"),
    ];
    let r = chercher(&net, &d, &jeux);
    let t = |nom: &str, cover: &str, hero: &str, logo: &str, n: bool| Trouvee {
        nom: nom.into(),
        cover: if cover.is_empty() { String::new() } else { chemin(&d, cover) },
        hero: if hero.is_empty() { String::new() } else { chemin(&d, hero) },
        logo: if logo.is_empty() { String::new() } else { chemin(&d, logo) },
        net: n,
    };
    assert_eq!(
        r,
        vec![
            t("Mon Jeu Steam", "monjeusteam.jpg", "monjeusteam-banniere.jpg", "", true),
            t("League of Legends", "leagueoflegends-officiel.jpg", "leagueoflegends-officiel-banniere.jpg", "", true),
            t("Jeu Inconnu", "jeuinconnu.jpg", "", "", true),
            t("Petit Jeu", "petitjeu-wiki.png", "", "", true),
            t("Logo Jeu", "", "", "logojeu-logo.png", false),
            t("Faux Jeu", "", "", "", true),
            t("Page Web", "", "", "", true),
        ]
    );
    for x in &r {
        for f in [&x.cover, &x.hero, &x.logo].into_iter().filter(|f| !f.is_empty()) {
            assert!(Path::new(f).exists(), "{f} n'est pas écrit");
        }
    }
    // Ce qui n'est pas gardé n'est pas laissé sur le disque
    for f in ["fauxjeu-wiki.jpg", "pageweb.jpg"] {
        assert!(!d.join(f).exists(), "{f} est resté");
    }
    // Les demandes : dans l'ordre de la version PowerShell, https seulement, avec ses délais
    let urls = net.urls();
    assert!(urls.iter().all(|u| u.starts_with("https://")), "une demande hors https : {urls:?}");
    assert!(!urls.iter().any(|u| u.contains("Faux.jpg")), "l'image en http a été demandée");
    let premier = |p: &str| urls.iter().position(|u| u.contains(p)).unwrap_or_else(|| panic!("{p} pas demandé"));
    assert_eq!(
        urls[..3],
        [
            format!("{STEAM}/123/library_600x900.jpg"),
            format!("{STEAM}/123/library_hero.jpg"),
            format!("{CDN}/123/header.jpg")
        ]
    );
    assert!(!urls.iter().any(|u| u.contains("term=League")), "les images officielles passent par Steam");
    assert!(premier("term=Jeu%20Inconnu") < premier("/77/library_600x900.jpg"));
    assert!(!urls.iter().any(|u| u.contains("fr.wikipedia.org/api/rest_v1/page/summary/Petit_Jeu")));
    assert!(
        premier("en.wikipedia.org/api/rest_v1/page/summary/Logo_Jeu")
            < premier("fr.wikipedia.org/api/rest_v1/page/summary/Logo_Jeu")
    );
    assert_eq!(net.pauses.lock().unwrap().clone(), vec![400; 7], "400 ms avant chaque demande à Wikipédia");
    for (u, delai, agent, max) in net.demandes.lock().unwrap().iter() {
        if u.contains("storesearch") {
            assert_eq!((*delai, agent.as_str()), (10, "Nevermind"), "{u}");
        } else if u.contains("wikipedia.org/api") {
            assert_eq!((*delai, agent.as_str()), (10, AGENT_IMAGES), "{u}");
        } else {
            assert_eq!((*delai, agent.as_str(), *max), (15, AGENT_IMAGES, MAX), "{u}");
        }
        assert!(*max <= MAX, "{u} : pas de limite de taille");
    }
    let _ = std::fs::remove_dir_all(&d);
}

fn annonces(e: &[Effet]) -> Vec<String> {
    e.iter()
        .filter_map(|x| match x {
            Effet::Annonce(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

/// Le réglage de la page Jeux : coupé, rien ne part ; allumé, les jeux sans image sont cherchés dans un travail de
/// fond, et index.json les range (Receive-Covers) ; un jeu déjà cherché sans succès n'est redemandé qu'après 7 jours
#[test]
fn le_reglage_cherche_et_range_les_jaquettes() {
    let d = dossier("jeux-jaquettes-bloc");
    let donnees = d.join("donnees");
    let jeux_dir = d.join("jeux");
    std::fs::create_dir_all(&jeux_dir).unwrap();
    let mut ajoutes = Vec::new();
    for nom in ["Petit Jeu", "Vieux Jeu", "Recent Jeu"] {
        let exe = jeux_dir.join(format!("{}.exe", nom.replace(' ', "")));
        std::fs::write(&exe, b"MZ").unwrap();
        ajoutes.push(json!({ "Name": nom, "Exe": exe.to_string_lossy() }));
    }
    let maintenant = Moment::depuis(2026, 11, 6, 15, 0, 0);
    let index = donnees.join("jaquettes").join("index.json");
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    std::fs::write(
        &index,
        json!({ "Vieux Jeu": { "Cover": "", "Hero": "", "Logo": "", "Date": "2026-10-27T15:00:00" },
            "Recent Jeu": { "Cover": "", "Hero": "", "Logo": "", "Date": "2026-11-03T15:00:00" } })
        .to_string(),
    )
    .unwrap();
    let ctx = Contexte::nouveau(d.join("appli"), donnees.clone(), false, true);
    ctx.ecrire_reglage("CustomGames", json!(ajoutes), false).unwrap();
    let net = Arc::new(FauxInternet::default());
    for nom in ["Petit%20Jeu", "Vieux%20Jeu"] {
        net.json(
            &format!("https://store.steampowered.com/api/storesearch/?term={nom}&l=french&cc=FR"),
            json!({ "items": [] }),
        );
    }
    wiki(&net, "en", "Petit_Jeu", "https://upload.wikimedia.org/a/Petit.jpg", 300, 450);
    net.repondre("https://upload.wikimedia.org/a/Petit.jpg", 200, jpeg(5000));
    let sys = SystemeEssai { heure: Some(maintenant), ..Default::default() };
    let mut b = BlocJeux::avec_internet(Arc::new(sys), net.clone());
    let mut e = Effets::default();
    b.rechercher_maintenant(&ctx, &mut e);
    b.travaux_finis(&ctx, &mut e);
    assert!(net.urls().is_empty(), "réglage coupé : des demandes sont parties : {:?}", net.urls());

    let mut e = Effets::default();
    assert!(b.action(&ctx, "coche:ChkLibCovers", &json!(true), &mut e));
    b.travaux_finis(&ctx, &mut e);
    let urls = net.urls();
    assert!(urls.iter().any(|u| u.contains("term=Petit%20Jeu")), "{urls:?}");
    assert!(urls.iter().any(|u| u.contains("term=Vieux%20Jeu")), "cherché sans succès il y a 10 jours : redemandé");
    assert!(!urls.iter().any(|u| u.contains("Recent")), "cherché sans succès il y a 3 jours : pas redemandé");
    let a = annonces(&e.0);
    assert!(a.contains(&"Recherche des jaquettes de 2 jeux...".to_string()), "{a:?}");
    assert!(a.contains(&"1 jaquette ajoutée à ta bibliothèque.".to_string()), "{a:?}");
    let v: Value =
        serde_json::from_str(std::fs::read_to_string(&index).unwrap().trim_start_matches('\u{feff}')).unwrap();
    let cover = donnees.join("jaquettes").join("petitjeu-wiki.jpg");
    assert_eq!(v["Petit Jeu"]["Cover"], json!(cover.to_string_lossy()));
    assert_eq!(v["Petit Jeu"]["Date"], json!(maintenant.iso()));
    assert_eq!(v["Vieux Jeu"]["Cover"], json!(""));
    assert_eq!(v["Vieux Jeu"]["Date"], json!(maintenant.iso()), "la date de la nouvelle recherche est notée");
    assert_eq!(v["Recent Jeu"]["Date"], json!("2026-11-03T15:00:00"), "les autres entrées restent");
    // La page montre la jaquette trouvée
    let p = serde_json::to_string(&b.pages(&ctx, "jeux")).unwrap();
    assert!(p.contains(&format!("fichier:{}", cover.to_string_lossy()).replace('\\', "\\\\")), "jaquette pas montrée");
    // Rien de plus à chercher : aucune nouvelle demande
    let n = net.urls().len();
    let mut e = Effets::default();
    assert!(b.action(&ctx, "coche:ChkLibCovers", &json!(true), &mut e));
    b.travaux_finis(&ctx, &mut e);
    assert_eq!(net.urls().len(), n, "un jeu qui a sa jaquette est cherché de nouveau");
    let _ = std::fs::remove_dir_all(&d);
}
