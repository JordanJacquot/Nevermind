//! Les jeux installés, tous launchers confondus (Get-InstalledGames d'optimisations.ps1), les jeux ajoutés à la main
//! (Get-CustomGames, Add-CustomGame, Remove-CustomGame de jeu.ps1), la liste de la bibliothèque (Get-LibraryGames) et
//! l'index des exécutables (Update-GameCache).

use crate::steam::{self, PAS_DES_JEUX, Reste};
use crate::systeme::{Ruche, Systeme};
use crate::texte::{self, joindre, nom_sans_extension};
use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;
use std::time::SystemTime;

/// Visuels fournis par le launcher lui-même (Ankama : jaquette, grande image, logo).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Art {
    pub cover: String,
    pub hero: String,
    pub logo: String,
}

/// Un jeu (même champs que la table de Get-InstalledGames).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Jeu {
    pub nom: String,
    pub exes: Vec<String>,
    /// Le launcher (« Steam », « Epic Games »...) ; vide = Steam.
    pub source: String,
    pub dossier: String,
    pub appid: String,
    /// Comment le lancer par son launcher (adresse, « exe|programme|arguments », « xbox:nom » ; vide = son exécutable).
    pub lancement: String,
    /// Comment le désinstaller (adresse, « launcher|adresse », ou la commande déclarée à Windows).
    pub desinstall: String,
    pub icone: String,
    pub art: Option<Art>,
    /// Ajouté à la main (« Retirer de la liste »).
    pub ajoute: bool,
}

impl Jeu {
    /// Le launcher affiché (`if ($g.Source) { $g.Source } else { 'Steam' }`).
    pub fn source_affichee(&self) -> &str {
        if self.source.is_empty() { "Steam" } else { &self.source }
    }
    pub fn premier_exe(&self) -> &str {
        self.exes.first().map(|s| s.as_str()).unwrap_or("")
    }
}

pub static MAUVAIS_EXE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)unins|setup|install|redist|dxsetup|directx|crash|report|easyanticheat|anticheat|eac_|beservice|battleye|_be$|update|helper|prereq|dotnet|webhelper|vcredist|python|java|browser|error|cleanup|touchup|repair|bootstrapper|resourcecompiler|^ui(32|64)$|diagnos|benchmark_?tool|ubisoftgamelauncher|uplay|^upc$|link2ea|socialclub|rockstarservice|cefsharp|leagueclient|riotclient|vanguard|^vgc$|blizzard ?error|agent$|gamelaunchhelper|launcher|snoretoast|notifier").unwrap()
});
static LANCEURS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(Battle\.net|Ubisoft Connect|Uplay|EA app|EA Desktop|Origin|Riot Client|Riot Vanguard|Rockstar Games Launcher|Rockstar Games Social Club|GOG GALAXY|GOG Galaxy|Amazon Games|Xbox|Epic Games Launcher)\s*$").unwrap()
});
/// Noms d'exécutables trop courants pour identifier un jeu ($GenericExe, jeu.ps1).
pub static EXE_GENERIQUE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(game|launcher|start|play|client|main|app|run|win64|win32|bin)$").unwrap());

/// `$norm` de Get-InstalledGames : barres obliques remises, espaces et guillemets enlevés, sans barre finale.
fn norm(p: &str) -> String {
    p.replace('/', "\\").trim().trim_matches('"').trim_end_matches('\\').to_string()
}

/// Un dossier trouvé par un launcher, avant la recherche de ses exécutables.
#[derive(Debug, Clone, Default)]
struct Trouve {
    nom: String,
    dossier: String,
    exe: String,
    source: String,
    appid: String,
    lancement: String,
    desinstall: String,
    art: Option<Art>,
    icone: String,
}

/// Le résultat de la recherche : les jeux et les restes de jeux Steam désinstallés.
#[derive(Debug, Default)]
pub struct Decouverte {
    pub jeux: Vec<Jeu>,
    pub restes: Vec<Reste>,
}

fn existe(p: &str) -> bool {
    !p.is_empty() && Path::new(p).exists()
}

const DESINSTALL: [(Ruche, &str); 3] = [
    (Ruche::Machine, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    (Ruche::Machine, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
    (Ruche::Utilisateur, r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
];

/// Les jeux installés (Get-InstalledGames). Lecture seule.
pub fn decouvrir(sys: &dyn Systeme) -> Decouverte {
    static ICONE_FIN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",\s*-?\d+$").unwrap());
    static ICO_EXE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\.(ico|exe)$").unwrap());
    static EXE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\.exe$").unwrap());
    static UPLAY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^Uplay Install (\d+)").unwrap());
    static RIOT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?i)^"?([^"]*RiotClientServices\.exe)"?.*--uninstall-product=(\S+)\s+--uninstall-patchline=(\S*)"#,
        )
        .unwrap()
    });
    static BNET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)--uid=(\S+)").unwrap());
    static ZAAP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)^"([^"]+\.exe)""#).unwrap());

    let mut r = Decouverte::default();
    let mut dirs: Vec<Trouve> = Vec::new();

    // Steam
    let mut steam_common: Vec<String> = Vec::new();
    if let Some(sp) = sys.reg_texte(Ruche::Utilisateur, r"Software\Valve\Steam", "SteamPath").filter(|s| !s.is_empty())
    {
        let s = steam::jeux_steam(&sp.replace('/', "\\"), SystemTime::now());
        r.restes.extend(s.restes);
        for d in s.dossiers {
            dirs.push(Trouve {
                lancement: format!("steam://rungameid/{}", d.appid),
                desinstall: format!("steam://uninstall/{}", d.appid),
                nom: d.nom,
                dossier: d.dossier,
                source: "Steam".into(),
                appid: d.appid,
                ..Default::default()
            });
        }
        steam_common = s.common;
    }

    // Epic Games
    if let Some(pd) = sys.variable("ProgramData") {
        let man = joindre(&pd, r"Epic\EpicGamesLauncher\Data\Manifests");
        for f in fichiers(&man, |n| n.to_lowercase().ends_with(".item")) {
            let Some(j) = lire_json(&f) else { continue };
            let loc = crate::json::texte(&j, "InstallLocation");
            if loc.is_empty() || !existe(&loc) {
                continue;
            }
            let le = crate::json::texte(&j, "LaunchExecutable");
            let exe = if le.is_empty() { String::new() } else { joindre(&loc, &le) };
            let (ns, item, app) = (
                crate::json::texte(&j, "CatalogNamespace"),
                crate::json::texte(&j, "CatalogItemId"),
                crate::json::texte(&j, "AppName"),
            );
            let lancement = if !ns.is_empty() && !item.is_empty() && !app.is_empty() {
                format!("com.epicgames.launcher://apps/{ns}%3A{item}%3A{app}?action=launch&silent=true")
            } else {
                String::new()
            };
            dirs.push(Trouve {
                nom: crate::json::texte(&j, "DisplayName"),
                dossier: loc,
                exe,
                source: "Epic Games".into(),
                lancement,
                desinstall: "launcher|com.epicgames.launcher://store/library".into(),
                ..Default::default()
            });
        }
    }

    // GOG Galaxy
    let gog = r"SOFTWARE\WOW6432Node\GOG.com\Games";
    for k in sys.reg_sous_cles(Ruche::Machine, gog) {
        let cle = format!("{gog}\\{k}");
        let nom = sys.reg_texte(Ruche::Machine, &cle, "gameName").unwrap_or_default();
        let dir = norm(&sys.reg_texte(Ruche::Machine, &cle, "path").unwrap_or_default());
        if !nom.is_empty() && existe(&dir) {
            let exe = norm(&sys.reg_texte(Ruche::Machine, &cle, "exe").unwrap_or_default());
            dirs.push(Trouve { nom, dossier: dir, exe, source: "GOG".into(), ..Default::default() });
        }
    }

    // Ubisoft Connect : jeux installés même sans entrée « Programmes et fonctionnalités »
    let mut ubi: Vec<(String, String)> = Vec::new();
    let ubi_cle = r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs";
    for k in sys.reg_sous_cles(Ruche::Machine, ubi_cle) {
        let dir = norm(&sys.reg_texte(Ruche::Machine, &format!("{ubi_cle}\\{k}"), "InstallDir").unwrap_or_default());
        if !dir.is_empty() {
            let bas = dir.to_lowercase();
            match ubi.iter_mut().find(|(d, _)| *d == bas) {
                Some(x) => x.1 = k,
                None => ubi.push((bas, k)),
            }
        }
    }

    // Programmes installés : les jeux des autres launchers s'y déclarent
    let sources: [(&str, &str, &str); 7] = [
        ("(?i)^Uplay Install", "(?i)Ubisoft", "Ubisoft Connect"),
        ("(?i)^Riot Game ", "(?i)Riot Games", "Riot"),
        ("", "(?i)Electronic Arts", "EA app"),
        ("", "(?i)Blizzard Entertainment", "Battle.net"),
        ("", "(?i)Rockstar Games", "Rockstar"),
        ("", r"(?i)GOG\.com|GOG Ltd", "GOG"),
        ("(?i)^AmazonGames/", "(?i)Amazon Games|Amazon Game Studios", "Amazon Games"),
    ];
    let sources: Vec<(Option<Regex>, Regex, &str)> = sources
        .iter()
        .map(|(re, p, s)| ((!re.is_empty()).then(|| Regex::new(re).unwrap()), Regex::new(p).unwrap(), *s))
        .collect();
    let mut desinst_par_dir: HashMap<String, String> = HashMap::new();
    let mut icone_par_dir: HashMap<String, String> = HashMap::new();
    for (ruche, racine) in DESINSTALL {
        for k in sys.reg_sous_cles(ruche, racine) {
            let cle = format!("{racine}\\{k}");
            let lire = |n: &str| sys.reg_texte(ruche, &cle, n).unwrap_or_default();
            let (loc, us, icone_brute, nom, editeur) = (
                lire("InstallLocation"),
                lire("UninstallString"),
                lire("DisplayIcon"),
                lire("DisplayName"),
                lire("Publisher"),
            );
            let ud = norm(&loc);
            if !ud.is_empty() {
                let b = ud.to_lowercase();
                if !us.is_empty() && !desinst_par_dir.contains_key(&b) {
                    desinst_par_dir.insert(b.clone(), us.clone());
                }
                let ico = norm(&ICONE_FIN.replace(&icone_brute, ""));
                if ICO_EXE.is_match(&ico) && !icone_par_dir.contains_key(&b) {
                    icone_par_dir.insert(b, ico);
                }
            }
            let mut src = None;
            for (re, p, s) in &sources {
                if re.as_ref().map(|re| re.is_match(&k)).unwrap_or(false)
                    || (!editeur.is_empty() && p.is_match(&editeur))
                {
                    src = Some(*s);
                    break;
                }
            }
            let Some(src) = src else { continue };
            if nom.is_empty() || LANCEURS.is_match(&nom) {
                continue;
            }
            let dir = norm(&loc);
            if !existe(&dir) {
                continue;
            }
            let bas = dir.to_lowercase();
            // Déjà trouvé par Steam (un jeu EA ou Blizzard acheté sur Steam)
            if steam_common.iter().any(|c| bas.starts_with(c.as_str())) {
                continue;
            }
            ubi.retain(|(d, _)| *d != bas);
            let icone = norm(&ICONE_FIN.replace(&icone_brute, ""));
            let exe =
                if EXE.is_match(&icone) && icone.to_lowercase().starts_with(&bas) { icone } else { String::new() };
            let mut lancement = String::new();
            if let Some(m) = UPLAY.captures(&k) {
                lancement = format!("uplay://launch/{}/0", &m[1]);
            } else if src == "Riot" {
                if let Some(m) = RIOT.captures(&us) {
                    lancement = format!("exe|{}|--launch-product={} --launch-patchline={}", &m[1], &m[2], &m[3]);
                }
            } else if src == "Battle.net"
                && let Some(m) = BNET.captures(&us)
            {
                let uid = m[1].trim_matches('"').to_lowercase();
                if let Some(c) = code_battlenet(&uid) {
                    lancement = format!("battlenet://{c}");
                }
            }
            dirs.push(Trouve {
                nom: nom.trim().to_string(),
                dossier: dir,
                exe,
                source: src.into(),
                lancement,
                ..Default::default()
            });
        }
    }
    for (d, id) in &ubi {
        if existe(d) {
            dirs.push(Trouve {
                nom: texte::nom_fichier(d.trim_end_matches('\\')).to_string(),
                dossier: d.clone(),
                source: "Ubisoft Connect".into(),
                lancement: format!("uplay://launch/{id}/0"),
                ..Default::default()
            });
        }
    }

    // Ankama Launcher (Dofus, Dofus Retro, Wakfu, Waven...) : rien n'est déclaré à Windows, les jeux sont dans ses fichiers
    let zaap = sys.reg_texte(Ruche::Classes, r"zaap\shell\open\command", "").unwrap_or_default();
    let ankama = ZAAP.captures(&zaap).map(|m| m[1].to_string()).unwrap_or_default();
    if let Some(ad) = sys.variable("APPDATA") {
        let prod = joindre(&ad, r"zaap\repositories\production");
        for d1 in dossiers(&prod) {
            for d2 in dossiers(&d1) {
                let rel = joindre(&d2, "release.json");
                if !Path::new(&rel).is_file() {
                    continue;
                }
                let Some(j) = lire_json(&rel) else { continue };
                let loc = match j.get("location") {
                    Some(Value::String(s)) if !s.is_empty() => s.clone(),
                    _ => continue,
                };
                if !existe(&loc) {
                    continue;
                }
                let (uid, nom, gnom) = (
                    crate::json::texte(&j, "gameUid"),
                    crate::json::texte(&j, "name"),
                    crate::json::texte(&j, "gameName"),
                );
                let nm = match (uid.to_lowercase().as_str(), nom.to_lowercase().as_str()) {
                    ("dofus", "dofus3") => "Dofus".to_string(),
                    ("dofus", "main") => "Dofus 2".to_string(),
                    ("retro", "main") => "Dofus Retro".to_string(),
                    _ => format!(
                        "{}{}",
                        titre_mots(&gnom),
                        if nom != "main" { format!(" ({nom})") } else { String::new() }
                    ),
                };
                let via = if ankama.is_empty() { String::new() } else { format!("exe|{ankama}|") };
                let data = joindre(&d2, "data");
                let pick = |n: &str| {
                    let p = joindre(&data, n);
                    if Path::new(&p).exists() { p } else { String::new() }
                };
                let home = pick("home_cover.jpg");
                let portrait =
                    !home.is_empty() && crate::jaquettes::taille_image(&home).map(|(w, h)| h > w).unwrap_or(false);
                let art = Art {
                    cover: if portrait { home.clone() } else { pick("cover.jpg") },
                    hero: home.clone(),
                    logo: pick("logo.png"),
                };
                dirs.push(Trouve {
                    nom: nm,
                    dossier: norm(&loc),
                    source: "Ankama".into(),
                    lancement: via,
                    desinstall: if ankama.is_empty() { String::new() } else { format!("launcher|{ankama}") },
                    art: Some(art),
                    icone: pick("icon.png"),
                    ..Default::default()
                });
            }
        }
    }

    // Xbox / Game Pass : dossiers « XboxGames » à la racine des disques
    for drv in sys.disques_fixes() {
        let xb = joindre(&drv, "XboxGames");
        if !Path::new(&xb).exists() {
            continue;
        }
        for g in dossiers(&xb) {
            let nom = texte::nom_fichier(&g).to_string();
            if PAS_DES_JEUX.is_match(&nom) {
                continue;
            }
            let content = joindre(&g, "Content");
            dirs.push(Trouve {
                dossier: if Path::new(&content).exists() { content } else { g.clone() },
                source: "Xbox".into(),
                lancement: format!("xbox:{nom}"),
                desinstall: "launcher|ms-settings:appsfeatures".into(),
                nom,
                ..Default::default()
            });
        }
    }

    let mut faits = HashSet::new();
    for d in dirs {
        if d.dossier.is_empty() || !faits.insert(d.dossier.to_lowercase()) {
            continue;
        }
        let mut exes = executables(&d.dossier);
        if !d.exe.is_empty()
            && Path::new(&d.exe).exists()
            && (!MAUVAIS_EXE.is_match(&nom_sans_extension(&d.exe)) || exes.is_empty())
        {
            exes.insert(0, d.exe.clone());
        }
        // (Select-Object -Unique)
        let mut vus = HashSet::new();
        exes.retain(|e| vus.insert(e.clone()));
        let bas = d.dossier.to_lowercase();
        let un = if !d.desinstall.is_empty() {
            d.desinstall.clone()
        } else {
            desinst_par_dir.get(&bas).cloned().unwrap_or_default()
        };
        let icone =
            if !d.icone.is_empty() { d.icone.clone() } else { icone_par_dir.get(&bas).cloned().unwrap_or_default() };
        if !exes.is_empty() {
            r.jeux.push(Jeu {
                nom: d.nom,
                exes,
                source: d.source,
                dossier: d.dossier,
                appid: d.appid,
                lancement: d.lancement,
                desinstall: un,
                icone,
                art: d.art,
                ajoute: false,
            });
        }
    }
    r
}

fn code_battlenet(uid: &str) -> Option<&'static str> {
    Some(match uid {
        "prometheus" => "Pro",
        "hs_beta" => "WTCG",
        "fenris" => "Fen",
        "diablo3" => "D3",
        "wow" => "WoW",
        "s2" => "S2",
        "heroes" => "Hero",
        "osi" => "OSI",
        "w3" => "W3",
        "odin" => "ODIN",
        "auks" => "AUKS",
        "lazarus" => "LAZR",
        "fore" => "FORE",
        "zeus" => "ZEUS",
        "viper" => "VIPR",
        "rtro" => "RTRO",
        "anbs" => "ANBS",
        _ => return None,
    })
}

/// `(Get-Culture).TextInfo.ToTitleCase` : première lettre de chaque mot en majuscule, le reste en minuscules (sauf un
/// mot tout en majuscules, gardé tel quel).
pub fn titre_mots(s: &str) -> String {
    let mut sortie = String::new();
    let mut mot = String::new();
    let vider = |mot: &mut String, sortie: &mut String| {
        if mot.is_empty() {
            return;
        }
        let tout_maj = mot.chars().all(|c| !c.is_lowercase());
        if tout_maj && mot.chars().filter(|c| c.is_alphabetic()).count() > 1 {
            sortie.push_str(mot);
        } else {
            let mut it = mot.chars();
            if let Some(p) = it.next() {
                sortie.extend(p.to_uppercase());
                sortie.extend(it.flat_map(|c| c.to_lowercase()));
            }
        }
        mot.clear();
    };
    for c in s.chars() {
        if c.is_alphanumeric() || c == '\'' {
            mot.push(c);
        } else {
            vider(&mut mot, &mut sortie);
            sortie.push(c);
        }
    }
    vider(&mut mot, &mut sortie);
    sortie
}

fn lire_json(f: &str) -> Option<Value> {
    let o = std::fs::read(f).ok()?;
    let t = String::from_utf8_lossy(&o);
    serde_json::from_str(t.trim_start_matches('\u{feff}')).ok()
}

fn fichiers(d: &str, garde: impl Fn(&str) -> bool) -> Vec<String> {
    let d = &texte::lettre_majuscule(d);
    let mut v: Vec<String> = std::fs::read_dir(d)
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| garde(n))
                .map(|n| joindre(d, &n))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|a| a.to_lowercase());
    v
}

fn dossiers(d: &str) -> Vec<String> {
    let d = &texte::lettre_majuscule(d);
    let mut v: Vec<String> = std::fs::read_dir(d)
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .map(|e| joindre(d, &e.file_name().to_string_lossy()))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|a| a.to_lowercase());
    v
}

/// Les .exe d'un dossier jusqu'à `profondeur` niveaux de sous-dossiers : (chemin, taille, profondeur).
fn exe_du_dossier(racine: &str, profondeur: usize) -> Vec<(String, u64, usize)> {
    let mut v = Vec::new();
    // (Get-ChildItem rend la lettre du disque en majuscule, même si le dossier est écrit en minuscules)
    let mut r = racine.to_string();
    if r.as_bytes().get(1) == Some(&b':') {
        r = format!("{}{}", r[..1].to_uppercase(), &r[1..]);
    }
    let mut pile = vec![(r, 0usize)];
    while let Some((d, niv)) = pile.pop() {
        let Ok(it) = std::fs::read_dir(&d) else { continue };
        let mut sous = Vec::new();
        for e in it.flatten() {
            let Ok(m) = e.metadata() else { continue };
            let n = e.file_name().to_string_lossy().to_string();
            let p = joindre(&d, &n);
            if m.is_dir() {
                if niv < profondeur {
                    sous.push((p, niv + 1));
                }
            } else if n.to_lowercase().ends_with(".exe") {
                v.push((p, m.len(), niv));
            }
        }
        sous.sort();
        pile.extend(sous.into_iter().rev());
    }
    v
}

/// Les exécutables probables du jeu : les plus gros, les moins enfouis d'abord (6 au plus) ; sinon un petit lanceur
/// (Dofus 2 : 184 Ko).
pub fn executables(dossier: &str) -> Vec<String> {
    let mut gros: Vec<(String, u64, usize)> = exe_du_dossier(dossier, 3)
        .into_iter()
        .filter(|(p, t, _)| *t > 200 * 1024 && !MAUVAIS_EXE.is_match(&nom_sans_extension(p)))
        .collect();
    // (profondeur : nombre de morceaux du chemin après le dossier du jeu, comme la version PowerShell)
    crate::texte::trier_ps(&mut gros, |a, b| a.2.cmp(&b.2).then(b.1.cmp(&a.1)));
    let mut exes: Vec<String> = gros.into_iter().take(6).map(|x| x.0).collect();
    if exes.is_empty() {
        let mut petits: Vec<(String, u64, usize)> = exe_du_dossier(dossier, 1)
            .into_iter()
            .filter(|(p, t, _)| *t > 50 * 1024 && !MAUVAIS_EXE.is_match(&nom_sans_extension(p)))
            .collect();
        crate::texte::trier_ps(&mut petits, |a, b| b.1.cmp(&a.1));
        exes = petits.into_iter().take(3).map(|x| x.0).collect();
    }
    exes
}

/// Les jeux ajoutés à la main (réglage CustomGames) dont le programme existe encore (Get-CustomGames).
pub fn jeux_ajoutes(reglage: Option<&Value>) -> Vec<Jeu> {
    let mut v = Vec::new();
    for x in liste(reglage) {
        let exe = crate::json::texte(&x, "Exe");
        if exe.is_empty() || !Path::new(&exe).exists() {
            continue;
        }
        v.push(Jeu {
            nom: crate::json::texte(&x, "Name"),
            exes: vec![exe],
            source: "Ajouté".into(),
            ajoute: true,
            ..Default::default()
        });
    }
    v
}

/// `@(Get-Setting 'X' @())` : une liste (un seul élément devient une liste d'un élément).
pub fn liste(v: Option<&Value>) -> Vec<Value> {
    match v {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a.iter().filter(|x| !x.is_null()).cloned().collect(),
        Some(x) => vec![x.clone()],
    }
}

/// La liste complète : jeux trouvés, plus ceux ajoutés à la main qui ne sont pas déjà dedans (Update-GameCache).
pub fn fusionner(trouves: Vec<Jeu>, ajoutes: Vec<Jeu>) -> Vec<Jeu> {
    let mut tous = trouves;
    let deja: Vec<String> = tous.iter().flat_map(|j| j.exes.iter().map(|e| e.to_lowercase())).collect();
    for a in ajoutes {
        if !deja.contains(&a.premier_exe().to_lowercase()) {
            tous.push(a);
        }
    }
    tous
}

/// La bibliothèque : jeux avec un nom, triés par nom, un seul par nom (Get-LibraryGames).
pub fn bibliotheque(jeux: &[Jeu]) -> Vec<Jeu> {
    let mut v: Vec<&Jeu> = jeux.iter().filter(|j| !j.nom.is_empty()).collect();
    crate::texte::trier_ps(&mut v, |a, b| texte::comparer_culture(&a.nom, &b.nom));
    let mut vus = HashSet::new();
    v.into_iter().filter(|j| vus.insert(j.nom.to_lowercase())).cloned().collect()
}

/// Une entrée de l'index des exécutables (nom du programme en minuscules, sans .exe).
#[derive(Debug, Clone, Default)]
pub struct Indexe {
    pub jeu: String,
    pub exes: Vec<String>,
    pub source: String,
    pub par_chemin: HashMap<String, String>,
}

/// L'index des programmes des jeux ($script:GameIndex), qui reconnaît un jeu à son programme.
pub fn index(jeux: &[Jeu]) -> HashMap<String, Indexe> {
    let mut idx: HashMap<String, Indexe> = HashMap::new();
    for g in jeux {
        for e in &g.exes {
            let base = nom_sans_extension(e).to_lowercase();
            if base.chars().count() < 3 || EXE_GENERIQUE.is_match(&base) {
                continue;
            }
            let x = idx.entry(base).or_insert_with(|| Indexe {
                jeu: g.nom.clone(),
                source: g.source.clone(),
                ..Default::default()
            });
            x.exes.push(e.to_lowercase());
            x.par_chemin.insert(e.to_lowercase(), g.nom.clone());
        }
    }
    idx
}

/// Les noms de programmes d'un jeu qui servent pour son profil (Get-GameExeNames) : pas trop courts, pas génériques.
pub fn noms_exe(g: &Jeu) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for e in &g.exes {
        let n = texte::nom_fichier(e).to_string();
        if !EXE_GENERIQUE.is_match(&nom_sans_extension(&n)) && n.chars().count() >= 8 && !v.contains(&n) {
            v.push(n);
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titres() {
        assert_eq!(titre_mots("waven"), "Waven");
        assert_eq!(titre_mots("wAKFU online"), "Wakfu Online");
        assert_eq!(titre_mots("DOFUS touch"), "DOFUS Touch");
    }
}
