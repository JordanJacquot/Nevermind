//! Les réglages d'OBS de plusieurs versions, sur de vrais fichiers : ceux créés par OBS Studio 33.0.1 (portable,
//! tests\obs33, essai réel du 08/11/2026), et ceux d'OBS 28, 30 et 31 fabriqués d'après la recherche (emplacement du
//! profil actif, identifiants d'encodeur). Profils multiples, fichiers abîmés, « Remettre comme avant » octet pour
//! octet. Tout se passe dans un dossier d'essai, jamais dans les vrais réglages.

use nevermind_stream::calcul::{Carte, Encodeur, Entree, Fabricant, Logiciel, Plan, Plateforme, Version, calculer};
use nevermind_stream::ini::Ini;
use nevermind_stream::reglages::{self, Probleme};
use nevermind_stream::sauvegarde::Copie;
use std::path::{Path, PathBuf};

const BOM: &str = "\u{feff}";

fn dossier(nom: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nevermind-stream-fichiers-{nom}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn ecrire(p: &Path, t: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, t).unwrap();
}

fn profil(racine: &Path, dossier: &str, basic: &str) -> PathBuf {
    let d = racine.join("basic").join("profiles").join(dossier);
    ecrire(&d.join("basic.ini"), basic);
    d
}

fn nvidia() -> Vec<Carte> {
    vec![Carte { nom: "NVIDIA GeForce RTX 3060".into(), fabricant: Fabricant::Nvidia }]
}

fn plan(e: &reglages::Etat, cartes: Vec<Carte>, p: Plateforme, envoi: u32) -> Plan {
    calculer(&Entree {
        envoi_kbps: envoi,
        cartes,
        base: reglages::base(Some(e), Some((1920, 1080))),
        plateforme: p,
        version: e.version,
    })
}

fn version(t: &str) -> u64 {
    let v = Version::lire(t).unwrap();
    (u64::from(v.majeure) << 24) | (u64::from(v.mineure) << 16) | u64::from(v.correctif)
}

#[test]
fn obs_33_vrais_fichiers() {
    let r = dossier("33");
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("obs33");
    std::fs::copy(f.join("global.ini"), r.join("global.ini")).unwrap();
    std::fs::copy(f.join("user.ini"), r.join("user.ini")).unwrap();
    let p = r.join("basic").join("profiles").join("Sans_nom");
    std::fs::create_dir_all(&p).unwrap();
    std::fs::copy(f.join("basic.ini"), p.join("basic.ini")).unwrap();
    let origine = std::fs::read(p.join("basic.ini")).unwrap();

    let e = reglages::lire(Logiciel::Obs, &r).unwrap();
    assert_eq!(e.version.map(Version::texte).as_deref(), Some("33.0.1"));
    assert_eq!(e.profil, "Sans nom");
    assert_eq!(e.dossier, p);
    assert_eq!(e.base, Some((1920, 1080)));
    assert!(!e.actuel.avance);
    // (OBS 33 n'écrit pas l'encodeur du mode Simple tant qu'on ne l'a pas changé : c'est sa valeur par défaut)
    assert_eq!(e.actuel.encodeur, None);
    assert_eq!(e.actuel.video_kbps, Some(6000));
    assert_eq!(e.actuel.sortie, Some((1280, 720)));
    assert_eq!(e.actuel.ips.as_deref(), Some("30"));
    assert!(e.service.is_none());

    let pl = plan(&e, nvidia(), Plateforme::Twitch, 20_000);
    let copie = Copie::nouvelle(r.join("copie"));
    let n = reglages::appliquer(&e, &pl, &copie, "08/11/2026 à 04:30").unwrap();
    assert!(n.encodeur.is_none());
    let apres = std::fs::read(p.join("basic.ini")).unwrap();
    // Exactement ce que l'essai réel a changé (4 lignes changées ou ajoutées), plus le débit qui s'adapte (12/11), rien
    // d'autre ; et la priorité d'OBS dans global.ini
    let avant_l: Vec<&str> = std::str::from_utf8(&origine).unwrap().lines().collect();
    let apres_l: Vec<&str> = std::str::from_utf8(&apres).unwrap().lines().collect();
    let ajoutees: Vec<&&str> = apres_l.iter().filter(|l| !avant_l.contains(l)).collect();
    let retirees: Vec<&&str> = avant_l.iter().filter(|l| !apres_l.contains(l)).collect();
    assert_eq!(
        ajoutees,
        [&"DynamicBitrate=true", &"StreamEncoder=nvenc", &"OutputCX=1920", &"OutputCY=1080", &"FPSCommon=60"]
    );
    let global_origine = std::fs::read(f.join("global.ini")).unwrap();
    let global = std::fs::read_to_string(r.join("global.ini")).unwrap();
    assert!(global.contains("ProcessPriority=AboveNormal"), "{global}");
    assert_eq!(n.cles_global.len(), 1);
    assert_eq!(retirees, [&"OutputCX=1280", &"OutputCY=720", &"FPSCommon=30"]);
    assert_eq!(&apres[..3], &[0xEF, 0xBB, 0xBF]);
    let lignes = reglages::changements(&e, &pl, &n);
    assert_eq!(lignes[0], "Qualité : 1080p à 60 images par seconde (avant : 720p à 30 images par seconde)");
    assert!(lignes.iter().any(|l| l == "Encodé par ta carte NVIDIA"));
    assert!(lignes.iter().any(|l| l == reglages::PLUS_DEBIT));
    assert!(lignes.iter().any(|l| l == reglages::PLUS_PRIORITE));
    assert!(lignes.iter().any(|l| l == &format!("Débit vidéo : 6\u{202f}000 kb/s{}", reglages::DEJA)), "{lignes:?}");
    // Remettre comme avant : les octets d'origine, des deux fichiers
    copie.remettre().unwrap();
    assert_eq!(std::fs::read(p.join("basic.ini")).unwrap(), origine);
    assert_eq!(std::fs::read(r.join("global.ini")).unwrap(), global_origine);
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn obs_28_profil_dans_global_ini_et_mode_avance() {
    let r = dossier("28");
    ecrire(
        &r.join("global.ini"),
        &format!(
            "{BOM}[General]\nPre19Defaults=false\nLastVersion={}\n\n[Basic]\nProfile=Untitled\nProfileDir=Untitled\nSceneCollection=Untitled\nSceneCollectionFile=Untitled\n",
            version("28.1.2")
        ),
    );
    let p = profil(
        &r,
        "Untitled",
        &format!(
            "{BOM}[General]\nName=Untitled\n\n[Output]\nMode=Advanced\n\n[AdvOut]\nEncoder=obs_x264\nTrackIndex=1\nTrack1Bitrate=128\nRecEncoder=obs_x264\n\n[Video]\nBaseCX=2560\nBaseCY=1440\nOutputCX=1280\nOutputCY=720\nFPSType=1\nFPSInt=30\n"
        ),
    );
    ecrire(
        &p.join("streamEncoder.json"),
        r#"{"bitrate":2500,"keyint_sec":0,"preset":"veryfast","tune":"zerolatency"}"#,
    );
    let e = reglages::lire(Logiciel::Obs, &r).unwrap();
    assert_eq!(e.version.unwrap().majeure, 28);
    assert_eq!(e.profil, "Untitled");
    assert!(e.actuel.avance);
    assert_eq!(e.actuel.encodeur, Some(Encodeur::X264));
    assert_eq!(e.actuel.video_kbps, Some(2500));
    assert_eq!(e.actuel.ips.as_deref(), Some("30"));
    assert_eq!(e.base, Some((2560, 1440)));
    let pl = plan(&e, nvidia(), Plateforme::Twitch, 20_000);
    assert_eq!(pl.sortie, (1920, 1080));
    let copie = Copie::nouvelle(r.join("copie"));
    let avant_ini = std::fs::read(p.join("basic.ini")).unwrap();
    let avant_json = std::fs::read(p.join("streamEncoder.json")).unwrap();
    reglages::appliquer(&e, &pl, &copie, "d").unwrap();
    let i = Ini::lire(&std::fs::read(p.join("basic.ini")).unwrap()).unwrap();
    // OBS 28 : l'ancien NVENC (jim_nvenc), son réglage « preset2 » ; la base n'est jamais changée
    assert_eq!(i.valeur("AdvOut", "Encoder"), Some("jim_nvenc"));
    assert_eq!(i.valeur("AdvOut", "RecEncoder"), Some("obs_x264"));
    assert_eq!(i.valeur("AdvOut", "Track1Bitrate"), Some("160"));
    assert_eq!((i.valeur("Video", "BaseCX"), i.valeur("Video", "OutputCX")), (Some("2560"), Some("1920")));
    assert_eq!((i.valeur("Video", "FPSType"), i.valeur("Video", "FPSCommon")), (Some("0"), Some("60")));
    let j: serde_json::Value = serde_json::from_slice(&std::fs::read(p.join("streamEncoder.json")).unwrap()).unwrap();
    assert_eq!(j, serde_json::json!({ "rate_control": "CBR", "bitrate": 6000, "keyint_sec": 2, "preset2": "p5" }));
    copie.remettre().unwrap();
    assert_eq!(std::fs::read(p.join("basic.ini")).unwrap(), avant_ini);
    assert_eq!(std::fs::read(p.join("streamEncoder.json")).unwrap(), avant_json);
    // OBS 28 et une puce Intel seule : l'ancien Quick Sync
    let e = reglages::lire(Logiciel::Obs, &r).unwrap();
    let intel = vec![Carte { nom: "Intel(R) UHD Graphics 630".into(), fabricant: Fabricant::Intel }];
    let pl = plan(&e, intel, Plateforme::YouTube, 20_000);
    let n = reglages::preparer(Logiciel::Obs, &avant_ini, Some(&avant_json), &pl, e.version).unwrap();
    assert_eq!(Ini::lire(&n.basic).unwrap().valeur("AdvOut", "Encoder"), Some("obs_qsv11"));
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn obs_30_plusieurs_profils() {
    let r = dossier("30");
    ecrire(
        &r.join("global.ini"),
        &format!("{BOM}[General]\nLastVersion={}\n\n[Basic]\nProfile=Stream\nProfileDir=Stream\n", version("30.2.3")),
    );
    profil(&r, "Enregistrement", &format!("{BOM}[General]\nName=Enregistrement\n\n[SimpleOutput]\nVBitrate=40000\n"));
    let p = profil(
        &r,
        "Stream",
        &format!("{BOM}[General]\nName=Stream\n\n[SimpleOutput]\nStreamEncoder=x264\nVBitrate=2500\n"),
    );
    let e = reglages::lire(Logiciel::Obs, &r).unwrap();
    assert_eq!((e.profil.as_str(), e.nb_profils), ("Stream", 2));
    assert_eq!(e.dossier, p);
    assert_eq!(e.actuel.encodeur, Some(Encodeur::X264));
    let pl = plan(&e, nvidia(), Plateforme::Twitch, 6_000);
    reglages::appliquer(&e, &pl, &Copie::nouvelle(r.join("copie")), "d").unwrap();
    let i = Ini::lire(&std::fs::read(p.join("basic.ini")).unwrap()).unwrap();
    assert_eq!(i.valeur("SimpleOutput", "StreamEncoder"), Some("nvenc"));
    assert_eq!(i.valeur("SimpleOutput", "VBitrate"), Some("4000"));
    // l'autre profil n'est pas touché
    let autre = std::fs::read_to_string(r.join("basic/profiles/Enregistrement/basic.ini")).unwrap();
    assert!(autre.contains("VBitrate=40000"));
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn obs_31_user_ini_et_ancien_global_ini() {
    let r = dossier("31");
    // global.ini garde l'ancien profil (copié vers user.ini à la migration) : seul user.ini compte
    ecrire(
        &r.join("global.ini"),
        &format!(
            "{BOM}[General]\nLastVersion={}\nPre31Migrated=true\n\n[Basic]\nProfile=Ancien\nProfileDir=Ancien\n",
            version("31.0.0")
        ),
    );
    ecrire(&r.join("user.ini"), &format!("{BOM}[Basic]\nProfile=Jeux\nProfileDir=Jeux_1\n"));
    profil(&r, "Ancien", &format!("{BOM}[General]\nName=Ancien\n"));
    // le dossier écrit ne porte pas le bon nom : OBS compare les NOMS (Name du basic.ini)
    profil(&r, "Jeux_1", &format!("{BOM}[General]\nName=Autre chose\n"));
    let p = profil(&r, "Jeux", &format!("{BOM}[General]\nName=Jeux\n\n[Output]\nMode=Simple\n"));
    let e = reglages::lire(Logiciel::Obs, &r).unwrap();
    assert_eq!((e.profil.as_str(), e.nb_profils), ("Jeux", 3));
    assert_eq!(e.dossier, p);
    // Kick sur OBS 31 : mode Avancé, nouvel NVENC, « preset »
    let pl = plan(&e, nvidia(), Plateforme::Kick, 20_000);
    let n = reglages::appliquer(&e, &pl, &Copie::nouvelle(r.join("copie")), "d").unwrap();
    assert!(n.passe_en_avance);
    let i = Ini::lire(&std::fs::read(p.join("basic.ini")).unwrap()).unwrap();
    assert_eq!(i.valeur("Output", "Mode"), Some("Advanced"));
    assert_eq!(i.valeur("AdvOut", "Encoder"), Some("obs_nvenc_h264_tex"));
    let j: serde_json::Value = serde_json::from_slice(&std::fs::read(p.join("streamEncoder.json")).unwrap()).unwrap();
    assert_eq!(
        (j["keyint_sec"].as_u64(), j["preset"].as_str(), j["bitrate"].as_u64()),
        (Some(2), Some("p5"), Some(8000))
    );
    // sans aucun profil correspondant, parmi plusieurs : Nevermind ne devine pas
    ecrire(&r.join("user.ini"), &format!("{BOM}[Basic]\nProfile=Inconnu\nProfileDir=Inconnu\n"));
    assert!(matches!(reglages::lire(Logiciel::Obs, &r), Err(Probleme::JamaisOuvert)));
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn obs_31_profils_deplaces_par_locations() {
    let r = dossier("loc");
    let ailleurs = r.join("ailleurs");
    std::fs::create_dir_all(ailleurs.join("obs-studio")).unwrap();
    ecrire(
        &r.join("global.ini"),
        &format!(
            "{BOM}[General]\nLastVersion={}\n\n[Locations]\nConfiguration={}\nProfiles={}\n",
            version("32.0.2"),
            ailleurs.display(),
            ailleurs.display()
        ),
    );
    ecrire(&ailleurs.join("obs-studio").join("user.ini"), &format!("{BOM}[Basic]\nProfile=Loin\nProfileDir=Loin\n"));
    profil(&ailleurs.join("obs-studio"), "Loin", &format!("{BOM}[General]\nName=Loin\n"));
    let e = reglages::lire(Logiciel::Obs, &r).unwrap();
    assert_eq!(e.profil, "Loin");
    assert!(e.dossier.starts_with(&ailleurs));
    // une valeur relative (mode portable : « ../../config ») est ignorée : les profils sont à leur place habituelle
    let r2 = dossier("loc2");
    ecrire(&r2.join("global.ini"), &format!("{BOM}[Locations]\nProfiles=../../config\n"));
    profil(&r2, "Seul", &format!("{BOM}[General]\nName=Seul\n"));
    assert_eq!(reglages::lire(Logiciel::Obs, &r2).unwrap().profil, "Seul");
    let _ = std::fs::remove_dir_all(&r);
    let _ = std::fs::remove_dir_all(&r2);
}

#[test]
fn fichiers_abimes_et_jamais_ouvert() {
    let r = dossier("abimes");
    assert_eq!(reglages::lire(Logiciel::Obs, &r).unwrap_err(), Probleme::JamaisOuvert);
    // un seul profil, aucun global.ini ni user.ini : il est pris
    let p = profil(&r, "Seul", "[General]\nName=Seul\n");
    assert_eq!(reglages::lire(Logiciel::Obs, &r).unwrap().profil, "Seul");
    // basic.ini abîmé : rien n'est changé, le fichier n'est pas touché
    std::fs::write(p.join("basic.ini"), [0xFF, 0xFE, 0x00, 0x5B, 0xC3]).unwrap();
    let e = reglages::lire(Logiciel::Obs, &r);
    assert!(matches!(e, Err(Probleme::Abime(ref f)) if f == "basic.ini"), "{e:?}");
    // global.ini abîmé
    std::fs::write(p.join("basic.ini"), "[General]\nName=Seul\n").unwrap();
    std::fs::write(r.join("global.ini"), [0x00, 0x00, 0xC3]).unwrap();
    assert!(matches!(reglages::lire(Logiciel::Obs, &r), Err(Probleme::Abime(ref f)) if f == "global.ini"));
    std::fs::remove_file(r.join("global.ini")).unwrap();
    // service.json abîmé : la plateforme n'est pas connue, sa copie .bak est reprise si elle est bonne
    std::fs::write(p.join("service.json"), "{abime").unwrap();
    assert!(reglages::lire(Logiciel::Obs, &r).unwrap().service.is_none());
    std::fs::write(p.join("service.json.bak"), r#"{"type":"rtmp_common","settings":{"service":"YouTube - RTMPS"}}"#)
        .unwrap();
    let s = reglages::lire(Logiciel::Obs, &r).unwrap().service.unwrap();
    assert_eq!(s.plateforme(), Some(Plateforme::YouTube));
    // un basic.ini abîmé au moment d'Optimiser : erreur, aucune copie, rien d'écrit
    let e = reglages::lire(Logiciel::Obs, &r).unwrap();
    std::fs::write(p.join("basic.ini"), [0xC3, 0x28]).unwrap();
    let copie = Copie::nouvelle(r.join("copie"));
    let pl = plan(&e, nvidia(), Plateforme::Twitch, 20_000);
    assert!(reglages::appliquer(&e, &pl, &copie, "d").is_err());
    assert!(!copie.existe());
    assert_eq!(std::fs::read(p.join("basic.ini")).unwrap(), [0xC3, 0x28]);
    let _ = std::fs::remove_dir_all(&r);
}

#[test]
fn streamlabs_sans_profils() {
    let r = dossier("slobs");
    ecrire(
        &r.join("basic.ini"),
        &format!(
            "{BOM}[Output]\nMode=Simple\n\n[SimpleOutput]\nStreamEncoder=jim_nvenc\nVBitrate=2500\n\n[Video]\nBaseCX=1920\nBaseCY=1080\nOutputCX=1280\nOutputCY=720\nFPSCommon=30\n"
        ),
    );
    ecrire(
        &r.join("service.json"),
        r#"{"type":"rtmp_common","settings":{"service":"Twitch","server":"auto","key":"live_SECRET"}}"#,
    );
    let e = reglages::lire(Logiciel::Streamlabs, &r).unwrap();
    assert_eq!(e.service.as_ref().and_then(|s| s.plateforme()), Some(Plateforme::Twitch));
    assert_eq!(e.actuel.encodeur, Some(Encodeur::Nvenc));
    let pl = plan(&e, nvidia(), Plateforme::Twitch, 20_000);
    let copie = Copie::nouvelle(r.join("copie"));
    let avant = std::fs::read(r.join("basic.ini")).unwrap();
    let avant_service = std::fs::read(r.join("service.json")).unwrap();
    reglages::appliquer(&e, &pl, &copie, "d").unwrap();
    let i = Ini::lire(&std::fs::read(r.join("basic.ini")).unwrap()).unwrap();
    assert_eq!(i.valeur("SimpleOutput", "StreamEncoder"), Some("nvenc"));
    assert_eq!(i.valeur("SimpleOutput", "VBitrate"), Some("6000"));
    // la vidéo de Streamlabs n'est pas dans ses fichiers : pas touchée
    assert_eq!((i.valeur("Video", "OutputCX"), i.valeur("Video", "FPSCommon")), (Some("1280"), Some("30")));
    assert_eq!(std::fs::read(r.join("service.json")).unwrap(), avant_service, "service.json n'est jamais réécrit");
    copie.remettre().unwrap();
    assert_eq!(std::fs::read(r.join("basic.ini")).unwrap(), avant);
    let _ = std::fs::remove_dir_all(&r);
}
