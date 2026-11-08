//! Les images d'un thème personnalisé pour l'écran (`Get-PackVue`, vue.ps1:352-378, d'après `Get-ThemePack` et
//! `Get-ThemePackConsole`, themes.ps1:78-150), et les deux protocoles qui servent des fichiers à la page :
//!
//! - `pack` (`http://pack.localhost/<fichier>`) : les images et la police du pack choisi, lues par suite-commun
//!   (règle 6 de regles-tauri.md : nom simple, fichier du dossier du pack, type reconnu à ses premiers octets) ;
//!   à la place de `https://theme.local/` de la version PowerShell ;
//! - `fichier` (`http://fichier.localhost/<jeton>`) : une image ailleurs sur le PC (la jaquette d'un jeu), seulement
//!   si un bloc l'a déclarée (`Get-ImageVue`, vue.ps1:300-307), à la place de `https://fichier.local/<jeton>`.
//!
//! Les champs viennent de pack.json, lus comme le PowerShell (et non par la lecture de suite-commun, qui ne garde pas
//! tout : `Footer.Files`, la hauteur du dessin du bas au-delà de 48, l'effet par défaut du chargement de la console).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use suite_commun::fichier_pack::{self, Refus};
use suite_commun::themes::Pack;
use tauri::http::{Response, StatusCode, header};

/// L'adresse des fichiers du pack, vue de la page.
pub const ADRESSE_PACK: &str = "http://pack.localhost/";
/// L'adresse des images déclarées par les blocs.
pub const ADRESSE_FICHIER: &str = "http://fichier.localhost/";
/// L'origine des pages de l'appli (la 3D fait des textures des images : elle le demande)
const ORIGINE_PAGES: &str = "http://tauri.localhost";

/// L'adresse d'un fichier du pack : tout ce qui n'est pas une lettre ou un chiffre ASCII, `-`, `_` ou `.` est codé
/// (`[uri]::EscapeDataString`).
pub fn adresse(fichier: &str) -> String {
    let mut a = String::from(ADRESSE_PACK);
    for o in fichier.bytes() {
        if o.is_ascii_alphanumeric() || matches!(o, b'-' | b'_' | b'.') {
            a.push(o as char);
        } else {
            a.push_str(&format!("%{o:02X}"));
        }
    }
    a
}

fn champ<'a>(o: &'a Map<String, Value>, nom: &str) -> Option<&'a Value> {
    o.get(nom).or_else(|| o.iter().find(|(k, _)| k.eq_ignore_ascii_case(nom)).map(|(_, v)| v))
}
fn sous<'a>(v: Option<&'a Value>, nom: &str) -> Option<&'a Value> {
    match v {
        Some(Value::Object(o)) => champ(o, nom),
        _ => None,
    }
}
/// `[string]$x`
fn texte(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => if *b { "True" } else { "False" }.to_string(),
        _ => String::new(),
    }
}
/// `if ($x)` de PowerShell
fn vrai(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}
/// `[double]$x` (0 sinon)
fn nombre(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        _ => 0.0,
    }
}
/// `[int]$x` (arrondi au pair, comme .NET)
fn entier(v: Option<&Value>) -> i64 {
    let x = nombre(v);
    let r = x.round();
    (if (x - x.trunc()).abs() == 0.5 { 2.0 * (x / 2.0).round() } else { r }) as i64
}

/// Un nom de fichier du pack qui existe (`Test-Path (Join-Path $dir $nom)`).
fn existe(dossier: &Path, v: Option<&Value>) -> Option<String> {
    let n = texte(v);
    (!n.is_empty() && dossier.join(&n).exists()).then_some(n)
}
/// Une image du bloc Console (themes.ps1:140) : `^[\w .()-]{1,80}\.(png|webp|gif|jpe?g)$`, sans « .. ».
fn image_console(dossier: &Path, v: Option<&Value>) -> Option<String> {
    let Some(Value::String(n)) = v else { return None };
    let (base, ext) = n.rsplit_once('.')?;
    let nb = base.chars().count();
    let ok = (1..=80).contains(&nb)
        && base.chars().all(|c| c.is_alphanumeric() || c == '_' || " .()-".contains(c))
        && ["png", "webp", "gif", "jpg", "jpeg"].contains(&ext.to_ascii_lowercase().as_str())
        && !n.contains("..");
    (ok && dossier.join(n).exists()).then(|| n.clone())
}
/// Le nom du fichier seul (`[IO.Path]::GetFileName`)
fn nom_seul(n: &str) -> &str {
    n.rsplit(['\\', '/']).next().unwrap_or(n)
}
fn url(n: &str) -> Value {
    json!(adresse(nom_seul(n)))
}

/// Lit pack.json du dossier du pack (BOM permis), un objet ; sinon rien.
fn lire_json(dossier: &Path) -> Option<Map<String, Value>> {
    let o = std::fs::read(dossier.join("pack.json")).ok()?;
    match serde_json::from_str::<Value>(String::from_utf8_lossy(&o).trim_start_matches('\u{feff}')) {
        Ok(Value::Object(m)) => Some(m),
        _ => None,
    }
}

/// `Get-PackVue` d'après pack.json (le pack est déjà choisi et lu par suite-commun).
pub fn pack_vue(p: &Pack) -> Value {
    let d = p.dossier.as_path();
    let j = lire_json(d).unwrap_or_default();
    pack_vue_json(d, &j)
}

/// `Get-PackVue` (vue.ps1:352-378) sur le contenu de pack.json.
pub fn pack_vue_json(d: &Path, j: &Map<String, Value>) -> Value {
    let mut p = Map::new();
    let c = match champ(j, "Console") {
        Some(c @ Value::Object(_)) => Some(c),
        _ => None,
    };
    let c_logo = c.and_then(|c| image_console(d, sous(Some(c), "Logo")));
    let c_fond = c.and_then(|c| image_console(d, sous(Some(c), "Fond")));
    let lueur = texte(champ(j, "LogoGlow"));
    let logo = existe(d, champ(j, "Logo"));
    if let Some(l) = c_logo {
        p.insert("logo".into(), json!({ "url": url(&l), "images": 1, "lueur": lueur }));
    } else if let Some(l) = logo {
        p.insert(
            "logo".into(),
            json!({ "url": url(&l), "images": entier(champ(j, "LogoFrames")).max(1),
                    "delai": entier(champ(j, "LogoDelay")).max(30), "lueur": lueur }),
        );
    }
    let fond_pack = existe(d, champ(j, "Background"));
    let opacite_pack =
        if vrai(champ(j, "BackgroundOpacity")) { nombre(champ(j, "BackgroundOpacity")).clamp(0.05, 1.0) } else { 0.35 };
    let al = texte(champ(j, "BackgroundAlign")).to_lowercase();
    if let Some(f) = c_fond.clone().or(fond_pack) {
        let fo = c.and_then(|c| sous(Some(c), "FondOpacite")).filter(|v| vrai(Some(v)));
        let op = match (&c_fond, fo) {
            (Some(_), Some(v)) => nombre(Some(v)).clamp(0.05, 1.0),
            _ => opacite_pack,
        };
        let x = if al.contains("droite") {
            json!("droite")
        } else if al.contains("gauche") {
            json!("gauche")
        } else {
            Value::Null
        };
        let y = if al.contains("bas") {
            json!("bas")
        } else if al.contains("haut") {
            json!("haut")
        } else {
            Value::Null
        };
        p.insert("fond".into(), json!({ "url": url(&f), "opacite": op, "x": x, "y": y }));
    }
    let footer = champ(j, "Footer");
    if let Some(c) = c {
        let mut tu = Map::new();
        if let Some(Value::Object(t)) = sous(Some(c), "Tuiles") {
            for (k, v) in t {
                let nom_ok = (1..=20).contains(&k.chars().count()) && k.chars().all(|x| x.is_ascii_alphabetic());
                if let Some(f) = image_console(d, Some(v)).filter(|_| nom_ok) {
                    tu.insert(k.clone(), url(&f));
                }
            }
        }
        let mut console = Map::new();
        console.insert("tuiles".into(), Value::Object(tu));
        if let Some(m) = image_console(d, sous(Some(c), "Mascotte")) {
            console.insert("mascotte".into(), json!({ "url": url(&m) }));
        }
        let ch = sous(Some(c), "Chargement");
        let (fichier, effet) = match ch {
            Some(Value::String(_)) => (ch, None),
            _ => (sous(ch, "File"), sous(ch, "Effet")),
        };
        if let Some(f) = image_console(d, fichier) {
            let e = if texte(effet).eq_ignore_ascii_case("aucun") { "aucun" } else { "tourne" };
            console.insert("chargement".into(), json!({ "url": url(&f), "effet": e }));
        }
        p.insert("console".into(), Value::Object(console));
    } else if let Some(Value::Array(fs)) =
        sous(footer, "Files").filter(|f| vrai(Some(f)) && f.as_array().is_some_and(|a| !a.is_empty()))
    {
        let fs: Vec<String> = fs.iter().filter_map(|f| existe(d, Some(f))).collect();
        if let Some(premier) = fs.first() {
            let h = entier(sous(footer, "Height")).max(16);
            p.insert("bas".into(), json!({ "url": url(premier), "images": 1, "hauteur": h }));
        }
    } else if let Some(f) = existe(d, sous(footer, "File")) {
        p.insert(
            "bas".into(),
            json!({ "url": url(&f), "images": entier(sous(footer, "Frames")).max(1),
                    "delai": entier(sous(footer, "Delay")).max(40), "hauteur": entier(sous(footer, "Height")).max(16) }),
        );
    }
    if let Some(f) = existe(d, champ(j, "Font")) {
        let pixel = match champ(j, "FontPixel") {
            None | Some(Value::Null) => true,
            v => vrai(v),
        };
        p.insert(
            "police".into(),
            json!({ "url": url(&f), "partout": texte(champ(j, "FontScope")).eq_ignore_ascii_case("tout"), "pixel": pixel }),
        );
    }
    Value::Object(p)
}

/// Une réponse vide « introuvable » ou un fichier, avec les en-têtes de la suite.
fn reponse(lu: Result<(Vec<u8>, &'static str), ()>) -> Response<Vec<u8>> {
    let base = Response::builder()
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, ORIGINE_PAGES);
    let r = match lu {
        Ok((octets, t)) => base.status(StatusCode::OK).header(header::CONTENT_TYPE, t).body(octets),
        Err(()) => base.status(StatusCode::NOT_FOUND).body(Vec::new()),
    };
    r.unwrap_or_else(|_| Response::new(Vec::new()))
}

/// Le protocole `pack` : `chemin` est celui de l'adresse demandée (`/fond.png`).
pub fn repondre_pack(dossier_pack: Option<&Path>, chemin: &str) -> (Response<Vec<u8>>, Option<Refus>) {
    let lu = match (dossier_pack, fichier_pack::nom_depuis_adresse(chemin)) {
        (Some(dossier), Some(nom)) => fichier_pack::lire(dossier, &nom),
        (None, _) => Err(Refus::Dossier),
        (_, None) => Err(Refus::Nom),
    };
    match lu {
        Ok(f) => (reponse(Ok((f.octets, f.type_contenu))), None),
        Err(r) => (reponse(Err(())), Some(r)),
    }
}

/// Les images déclarées par les blocs (`$script:Vue.Images` : jeton vers chemin), partagées avec le protocole.
#[derive(Clone, Default)]
pub struct Images(Arc<Mutex<HashMap<String, PathBuf>>>);

/// Les extensions d'image servies (vue.ps1:133).
const EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
/// Une image plus grosse n'est pas servie.
const TAILLE_MAX: u64 = 15 * 1024 * 1024;

impl Images {
    /// `Get-ImageVue` (vue.ps1:300-307) : l'adresse d'une image n'importe où sur le PC, la même pour le même fichier.
    pub fn adresse(&self, chemin: &Path) -> String {
        let mut t = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let cle = chemin.to_string_lossy().to_lowercase();
        if let Some((j, _)) = t.iter().find(|(_, p)| p.to_string_lossy().to_lowercase() == cle) {
            return format!("{ADRESSE_FICHIER}{j}");
        }
        let ext = chemin.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let ext: String = ext.chars().filter(char::is_ascii_alphanumeric).take(5).collect();
        let jeton = format!("{}.{ext}", jeton_neuf(t.len()));
        t.insert(jeton.clone(), chemin.to_path_buf());
        format!("{ADRESSE_FICHIER}{jeton}")
    }

    /// Le protocole `fichier` : seulement un jeton connu, une image (extension et premiers octets), d'au plus 15 Mo.
    pub fn repondre(&self, chemin: &str) -> Response<Vec<u8>> {
        let jeton = chemin.trim_start_matches('/');
        let f = self.0.lock().unwrap_or_else(|e| e.into_inner()).get(jeton).cloned();
        let lu = f
            .filter(|f| {
                let e = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                EXTENSIONS.contains(&e.as_str())
                    && std::fs::metadata(f).is_ok_and(|m| m.is_file() && m.len() <= TAILLE_MAX)
            })
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|o| image_type(&o).map(|t| (o, t)));
        reponse(lu.ok_or(()))
    }
}

/// Ce qui rend la réponse d'une image (le `UriSchemeResponder` de Tauri)
type Rendre = Box<dyn FnOnce(Response<Vec<u8>>) + Send>;

/// Le protocole `fichier` hors du fil de la fenêtre (petit de l'audit du 08/11) : Tauri appelle un protocole
/// synchrone sur le fil de la fenêtre, et chaque jaquette (jusqu'à 15 Mo) y était lue en entier, ce qui faisait
/// saccader la fenêtre sur un disque lent. Les images sont lues ici sur un fil à elles, une à la fois (un disque dur
/// lit mieux dans l'ordre), et la réponse est rendue quand elle est prête.
#[derive(Clone)]
pub struct LectureImages {
    images: Images,
    envoi: std::sync::mpsc::Sender<(String, Rendre)>,
}

impl LectureImages {
    pub fn nouvelle(images: Images) -> LectureImages {
        let (envoi, reception) = std::sync::mpsc::channel::<(String, Rendre)>();
        let pour_fil = images.clone();
        // (si le fil ne démarre pas, `demander` lit sur place, comme avant)
        let _ = std::thread::Builder::new().name("nevermind-images".into()).spawn(move || {
            for (chemin, rendre) in reception {
                rendre(pour_fil.repondre(&chemin));
            }
        });
        LectureImages { images, envoi }
    }

    /// L'image de l'adresse `chemin`, rendue par `rendre` depuis le fil des images (sans attendre ici)
    pub fn demander(&self, chemin: &str, rendre: impl FnOnce(Response<Vec<u8>>) + Send + 'static) {
        if let Err(std::sync::mpsc::SendError((c, r))) = self.envoi.send((chemin.to_string(), Box::new(rendre))) {
            r(self.images.repondre(&c));
        }
    }
}

/// Le type d'une image à ses premiers octets (bmp en plus de ceux de suite-commun).
fn image_type(o: &[u8]) -> Option<&'static str> {
    fichier_pack::type_image(&o[..o.len().min(64)]).or_else(|| o.starts_with(b"BM").then_some("image/bmp"))
}

/// Un jeton difficile à deviner (32 chiffres hexadécimaux, comme un guid sans tirets).
fn jeton_neuf(n: usize) -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_usize(n);
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let a = h.finish();
    let mut h2 = std::collections::hash_map::RandomState::new().build_hasher();
    h2.write_u64(a);
    format!("{a:016x}{:016x}", h2.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dossier(PathBuf);
    impl Drop for Dossier {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn dossier(nom: &str, fichiers: &[&str]) -> Dossier {
        let d = nevermind_noyau::dossier_essai(&format!("hote-pack-{nom}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for f in fichiers {
            std::fs::write(d.join(f), b"\x89PNG\r\n\x1a\n0000").unwrap();
        }
        Dossier(d)
    }

    #[test]
    fn adresses_codees() {
        assert_eq!(adresse("fond.png"), "http://pack.localhost/fond.png");
        assert_eq!(adresse("mon fond (2).png"), "http://pack.localhost/mon%20fond%20%282%29.png");
        assert_eq!(adresse("été.png"), "http://pack.localhost/%C3%A9t%C3%A9.png");
    }

    #[test]
    fn pack_sans_console() {
        let d = dossier("simple", &["logo.png", "fond.jpg", "bas.png", "police.ttf"]);
        let j = json!({ "Logo": "logo.png", "LogoFrames": 4, "LogoDelay": 10, "LogoGlow": "#FF00FF",
            "Background": "fond.jpg", "BackgroundOpacity": 2, "BackgroundAlign": "bas-droite",
            "Footer": { "File": "bas.png", "Frames": 3, "Delay": 20, "Height": 60 },
            "Font": "police.ttf", "FontScope": "Tout", "FontPixel": false });
        let v = pack_vue_json(&d.0, j.as_object().unwrap());
        assert_eq!(
            v,
            json!({
                "logo": { "url": "http://pack.localhost/logo.png", "images": 4, "delai": 30, "lueur": "#FF00FF" },
                "fond": { "url": "http://pack.localhost/fond.jpg", "opacite": 1.0, "x": "droite", "y": "bas" },
                "bas": { "url": "http://pack.localhost/bas.png", "images": 3, "delai": 40, "hauteur": 60 },
                "police": { "url": "http://pack.localhost/police.ttf", "partout": true, "pixel": false },
            })
        );
        // Des fichiers absents : rien
        let v = pack_vue_json(
            &d.0,
            json!({ "Logo": "absent.png", "Footer": { "Files": ["x.png", "bas.png"] } }).as_object().unwrap(),
        );
        assert_eq!(v, json!({ "bas": { "url": "http://pack.localhost/bas.png", "images": 1, "hauteur": 16 } }));
    }

    #[test]
    fn pack_avec_console() {
        let d = dossier("console", &["c.png", "t.webp", "m.gif", "ch.png", "f.png", "bas.png"]);
        let j = json!({ "Logo": "c.png", "Footer": { "File": "bas.png" },
            "Console": { "Logo": "c.png", "Tuiles": { "pc": "t.webp", "Jeux": "t.webp", "a1": "t.webp", "x": "../t.webp" },
                         "Mascotte": "m.gif", "Chargement": { "File": "ch.png" }, "Fond": "f.png", "FondOpacite": 0.01 } });
        let v = pack_vue_json(&d.0, j.as_object().unwrap());
        assert_eq!(
            v,
            json!({
                "logo": { "url": "http://pack.localhost/c.png", "images": 1, "lueur": "" },
                "fond": { "url": "http://pack.localhost/f.png", "opacite": 0.05, "x": null, "y": null },
                "console": { "tuiles": { "pc": "http://pack.localhost/t.webp", "Jeux": "http://pack.localhost/t.webp" },
                             "mascotte": { "url": "http://pack.localhost/m.gif" },
                             "chargement": { "url": "http://pack.localhost/ch.png", "effet": "tourne" } },
            })
        );
    }

    #[test]
    fn images_declarees_seulement() {
        let d = dossier("images", &["jaquette.png"]);
        let i = Images::default();
        let a = i.adresse(&d.0.join("jaquette.png"));
        assert!(a.starts_with(ADRESSE_FICHIER) && a.ends_with(".png"), "{a}");
        assert_eq!(i.adresse(&d.0.join("JAQUETTE.png")), a, "la même pour le même fichier");
        let j = &a[ADRESSE_FICHIER.len()..];
        assert_eq!(i.repondre(&format!("/{j}")).status(), StatusCode::OK);
        assert_eq!(i.repondre("/inconnu.png").status(), StatusCode::NOT_FOUND);
        // Pas une image (au premier octet) : refusée
        std::fs::write(d.0.join("faux.png"), b"MZ pas une image").unwrap();
        let a = i.adresse(&d.0.join("faux.png"));
        assert_eq!(i.repondre(&format!("/{}", &a[ADRESSE_FICHIER.len()..])).status(), StatusCode::NOT_FOUND);
        let a = i.adresse(Path::new(r"C:\Windows\win.ini"));
        assert_eq!(i.repondre(&format!("/{}", &a[ADRESSE_FICHIER.len()..])).status(), StatusCode::NOT_FOUND);
    }

    /// Petit de l'audit du 08/11 : les jaquettes sont lues sur un fil à elles, jamais sur celui qui les demande (le
    /// fil de la fenêtre) ; la réponse est la même qu'avant.
    #[test]
    fn les_images_sont_lues_hors_du_fil_qui_les_demande() {
        let d = dossier("images-fil", &["jaquette.png"]);
        let i = Images::default();
        let a = i.adresse(&d.0.join("jaquette.png"));
        let j = format!("/{}", &a[ADRESSE_FICHIER.len()..]);
        let l = LectureImages::nouvelle(i.clone());
        let (envoi, reception) = std::sync::mpsc::channel();
        for c in [j.clone(), "/inconnu.png".to_string()] {
            let e = envoi.clone();
            l.demander(&c, move |r| {
                let _ = e.send((std::thread::current().id(), r.status(), r.body().len()));
            });
        }
        let ici = std::thread::current().id();
        let attente = std::time::Duration::from_secs(10);
        let (fil, statut, taille) = reception.recv_timeout(attente).unwrap();
        assert_ne!(fil, ici, "l'image est lue sur le fil qui la demande");
        assert_eq!((statut, taille), (StatusCode::OK, i.repondre(&j).body().len()));
        let (fil, statut, _) = reception.recv_timeout(attente).unwrap();
        assert_ne!(fil, ici);
        assert_eq!(statut, StatusCode::NOT_FOUND);
    }
}
