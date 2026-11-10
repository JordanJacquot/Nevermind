//! « Remettre comme avant » et la copie des anciens réglages, sur de faux dossiers OBS (dossier d'essai, jamais le vrai
//! PC) : ce que l'utilisateur change dans OBS après « Optimiser » est gardé, un profil renommé ou supprimé est dit en
//! clair, un « Optimiser » raté ne laisse pas de copie, une copie abîmée n'est jamais remise, les anciennes copies
//! (format d'avant le 10/11/2026) se relisent, et un OBS portable est trouvé quand il est ouvert.

use nevermind_noyau::{Bloc, Contexte, Effet, Effets, dossier_essai};
use nevermind_stream::BlocStream;
use nevermind_stream::calcul::{Carte, Fabricant, Logiciel};
use nevermind_stream::ini::Ini;
use nevermind_stream::systeme::{Installation, Processus, Systeme};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Pc {
    installations: Vec<Installation>,
    processus: Vec<Processus>,
}

#[derive(Clone, Default)]
struct FauxPc(Arc<Mutex<Pc>>);

impl Systeme for FauxPc {
    fn installations(&self) -> Vec<Installation> {
        self.0.lock().unwrap().installations.clone()
    }
    fn processus(&self) -> Vec<Processus> {
        self.0.lock().unwrap().processus.clone()
    }
    fn cartes(&self) -> Vec<Carte> {
        vec![Carte { nom: "NVIDIA GeForce RTX 3060".into(), fabricant: Fabricant::Nvidia }]
    }
    fn ecran(&self) -> Option<(u32, u32)> {
        Some((1920, 1080))
    }
}

const TWITCH: &str = r#"{"type":"rtmp_common","settings":{"service":"Twitch","server":"auto"}}"#;

fn obs33() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("obs33")
}

/// Les réglages d'un OBS d'essai (ceux créés par OBS 33) dans `reglages`, profil « Sans_nom »
fn reglages_obs(reglages: &Path) -> PathBuf {
    let p = reglages.join("basic").join("profiles").join("Sans_nom");
    std::fs::create_dir_all(&p).unwrap();
    for n in ["global.ini", "user.ini"] {
        std::fs::copy(obs33().join(n), reglages.join(n)).unwrap();
    }
    std::fs::copy(obs33().join("basic.ini"), p.join("basic.ini")).unwrap();
    std::fs::write(p.join("service.json"), TWITCH).unwrap();
    p
}

/// Un OBS installé d'essai, et son faux PC
fn obs(ctx: &Contexte) -> (FauxPc, PathBuf) {
    let racine = ctx.donnees.join("pc");
    let reglages = racine.join("obs-studio");
    let p = reglages_obs(&reglages);
    let pc = FauxPc::default();
    pc.0.lock().unwrap().installations.push(Installation {
        logiciel: Logiciel::Obs,
        dossier: racine.join("Program Files").join("obs-studio"),
        reglages,
        version: Some("33.0.1".into()),
    });
    (pc, p)
}

fn contexte(nom: &str) -> Contexte {
    let d = dossier_essai(&format!("stream-remettre-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    Contexte::nouveau(d.clone(), d, false, true)
}

fn annonces(e: &Effets) -> Vec<String> {
    e.0.iter().filter_map(|x| if let Effet::Annonce(t) = x { Some(t.clone()) } else { None }).collect()
}

fn signaux(e: &Effets, nom: &str) -> Vec<Value> {
    e.0.iter()
        .filter_map(|x| match x {
            Effet::Signal { nom: n, donnees } if n == nom => Some(donnees.clone()),
            _ => None,
        })
        .collect()
}

fn page(b: &mut BlocStream, ctx: &Contexte) -> Value {
    b.pages(ctx, "streameur")["streameur"].clone()
}

fn boutons(p: &Value) -> Vec<String> {
    p["module"]["boutons"].as_array().unwrap().iter().map(|b| b["texte"].as_str().unwrap().to_string()).collect()
}

fn action(b: &mut BlocStream, ctx: &Contexte, id: &str) -> Vec<String> {
    let mut e = Effets::default();
    assert!(b.action(ctx, id, &Value::Null, &mut e));
    annonces(&e)
}

/// Un bloc prêt à régler (connexion mesurée, page ouverte)
fn bloc(pc: &FauxPc, ctx: &Contexte) -> BlocStream {
    let mut b = BlocStream::avec_systeme(Box::new(pc.clone()));
    let mut e = Effets::default();
    b.signal(ctx, "debit-mesure", &json!({ "R": [12.0, 300.0, 20.0, 0.0] }), &mut e);
    let _ = page(&mut b, ctx);
    b
}

fn lire(f: &Path) -> Ini {
    Ini::lire(&std::fs::read(f).unwrap()).unwrap()
}

/// OBS réécrit basic.ini (l'utilisateur a changé ses réglages) : les valeurs données changent, le reste est gardé
fn changer_dans_obs(f: &Path, changes: &[(&str, &str, &str)]) {
    let mut i = lire(f);
    for (s, k, v) in changes {
        i.mettre(s, k, v);
    }
    std::fs::write(f, i.octets()).unwrap();
}

const DOSSIER_PERSO: &str = r"D:\\Mes enregistrements";

#[test]
fn ce_que_l_utilisateur_a_change_depuis_est_garde() {
    let ctx = contexte("depuis");
    let (pc, profil) = obs(&ctx);
    let f = profil.join("basic.ini");
    let mut b = bloc(&pc, &ctx);
    assert!(action(&mut b, &ctx, "stream:optimiser:obs")[0].starts_with("OBS est réglé"));
    assert_eq!(lire(&f).valeur("Video", "OutputCX"), Some("1920"));
    // Des semaines plus tard, dans OBS : un autre dossier d'enregistrement, un compte connecté, et une taille de sortie
    // que l'utilisateur a choisie lui-même (une clé que Nevermind avait changée)
    changer_dans_obs(
        &f,
        &[
            ("SimpleOutput", "FilePath", DOSSIER_PERSO),
            ("Twitch", "Token", "jeton"),
            ("Video", "OutputCX", "1600"),
            ("Video", "OutputCY", "900"),
        ],
    );
    assert_eq!(
        action(&mut b, &ctx, "stream:remettre:obs"),
        ["Les réglages d'avant sont revenus dans OBS. Ce que tu as changé toi-même depuis est gardé."]
    );
    let i = lire(&f);
    assert_eq!(i.valeur("SimpleOutput", "FilePath"), Some(DOSSIER_PERSO), "le dossier choisi depuis est gardé");
    assert_eq!(i.valeur("Twitch", "Token"), Some("jeton"), "le compte connecté depuis est gardé");
    assert_eq!((i.valeur("Video", "OutputCX"), i.valeur("Video", "OutputCY")), (Some("1600"), Some("900")));
    // ce que Nevermind avait changé, et que l'utilisateur n'a pas retouché, revient comme avant
    assert_eq!(i.valeur("Video", "FPSCommon"), Some("30"));
    assert_eq!(i.valeur("SimpleOutput", "StreamEncoder"), None, "une clé qui n'existait pas est retirée");
    assert!(!ctx.donnees.join(r"streameur\obs").exists(), "la copie est effacée");
    assert_eq!(boutons(&page(&mut b, &ctx)), ["Optimiser"]);
}

#[test]
fn mode_avance_streamencoder_json_garde_les_changements_faits_depuis() {
    let ctx = contexte("json");
    let (pc, profil) = obs(&ctx);
    let f = profil.join("basic.ini");
    let fe = profil.join("streamEncoder.json");
    changer_dans_obs(&f, &[("Output", "Mode", "Advanced"), ("AdvOut", "Encoder", "obs_nvenc_h264_tex")]);
    let avant_json = br#"{"bitrate":2500,"multipass":"qres"}"#;
    std::fs::write(&fe, avant_json).unwrap();
    let avant_ini = std::fs::read(&f).unwrap();
    let mut b = bloc(&pc, &ctx);
    action(&mut b, &ctx, "stream:optimiser:obs");
    let j: Value = serde_json::from_slice(&std::fs::read(&fe).unwrap()).unwrap();
    assert_eq!((j["bitrate"].as_u64(), j["keyint_sec"].as_u64()), (Some(6000), Some(2)));
    // rien n'a changé depuis : les mêmes octets qu'avant reviennent
    action(&mut b, &ctx, "stream:remettre:obs");
    assert_eq!(std::fs::read(&fe).unwrap(), avant_json);
    assert_eq!(std::fs::read(&f).unwrap(), avant_ini);
    // cette fois, l'utilisateur change un réglage de l'encodeur dans OBS après « Optimiser »
    action(&mut b, &ctx, "stream:optimiser:obs");
    let mut j: Value = serde_json::from_slice(&std::fs::read(&fe).unwrap()).unwrap();
    j["multipass"] = json!("fullres");
    std::fs::write(&fe, serde_json::to_vec_pretty(&j).unwrap()).unwrap();
    action(&mut b, &ctx, "stream:remettre:obs");
    let j: Value = serde_json::from_slice(&std::fs::read(&fe).unwrap()).unwrap();
    assert_eq!(j, json!({ "bitrate": 2500, "multipass": "fullres" }), "seul ce que Nevermind a changé revient");
}

#[test]
fn profil_renomme_ou_supprime_dit_en_clair() {
    let ctx = contexte("renomme");
    let (pc, profil) = obs(&ctx);
    let mut b = bloc(&pc, &ctx);
    action(&mut b, &ctx, "stream:optimiser:obs");
    // l'utilisateur renomme son profil dans OBS (OBS renomme le dossier)
    std::fs::rename(&profil, profil.with_file_name("Mon_profil")).unwrap();
    assert_eq!(
        action(&mut b, &ctx, "stream:remettre:obs"),
        [
            "Le profil d'OBS réglé par Nevermind n'existe plus (il a été renommé ou supprimé) : il n'y a plus rien à remettre."
        ]
    );
    assert!(!ctx.donnees.join(r"streameur\obs\copie.json").exists(), "la copie est effacée");
    let p = page(&mut b, &ctx);
    assert!(!boutons(&p).contains(&"Remettre comme avant".to_string()), "{p}");
    // le renommage ne bloque rien : le nouveau profil se règle et se remet normalement
    let neuf = profil.with_file_name("Mon_profil").join("basic.ini");
    let avant = std::fs::read(&neuf).unwrap();
    action(&mut b, &ctx, "stream:optimiser:obs");
    assert_eq!(action(&mut b, &ctx, "stream:remettre:obs"), ["Les réglages d'avant sont revenus dans OBS."]);
    let i = lire(&neuf);
    assert_eq!(i.valeur("Video", "OutputCX"), Ini::lire(&avant).unwrap().valeur("Video", "OutputCX"));
}

#[test]
fn optimiser_rate_ne_laisse_pas_de_copie() {
    let ctx = contexte("rate");
    let (pc, profil) = obs(&ctx);
    let f = profil.join("basic.ini");
    let lecture_seule = |oui: bool| {
        let mut p = std::fs::metadata(&f).unwrap().permissions();
        p.set_readonly(oui);
        std::fs::set_permissions(&f, p).unwrap();
    };
    lecture_seule(true);
    let mut b = bloc(&pc, &ctx);
    assert_eq!(
        action(&mut b, &ctx, "stream:optimiser:obs"),
        ["Nevermind n'a pas pu changer les réglages d'OBS : rien n'a été changé."]
    );
    assert!(!ctx.donnees.join(r"streameur\obs\copie.json").exists());
    assert_eq!(boutons(&page(&mut b, &ctx)), ["Optimiser"], "pas de « Remettre comme avant » : rien n'a changé");
    // une copie d'avant (un « Optimiser » réussi) reste telle qu'elle était après un second « Optimiser » raté
    lecture_seule(false);
    action(&mut b, &ctx, "stream:optimiser:obs");
    let copie = std::fs::read(ctx.donnees.join(r"streameur\obs\copie.json")).unwrap();
    let mut e = Effets::default();
    b.action(&ctx, "stream:plateforme:obs", &json!("kick"), &mut e);
    lecture_seule(true);
    action(&mut b, &ctx, "stream:optimiser:obs");
    assert_eq!(std::fs::read(ctx.donnees.join(r"streameur\obs\copie.json")).unwrap(), copie);
    lecture_seule(false);
}

/// Une copie faite par les versions d'avant le 10/11/2026 : copie.json avec la liste, et le fichier entier dans
/// `1.avant`
fn ancienne_copie(ctx: &Contexte, basic: &Path, avant: &[u8]) {
    let d = ctx.donnees.join(r"streameur\obs");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("1.avant"), avant).unwrap();
    let fiche = json!({ "date": "08/11/2026 à 23:00", "profil": "Sans nom",
        "fichiers": [{ "chemin": basic.to_string_lossy(), "existait": true, "copie": "1.avant" }] });
    std::fs::write(d.join("copie.json"), serde_json::to_vec_pretty(&fiche).unwrap()).unwrap();
}

#[test]
fn ancienne_copie_relue_sans_rien_perdre() {
    let ctx = contexte("ancienne");
    let (pc, profil) = obs(&ctx);
    let f = profil.join("basic.ini");
    let origine = std::fs::read(&f).unwrap();
    // l'état laissé par une ancienne version : OBS réglé, la copie d'avant au vieux format
    changer_dans_obs(
        &f,
        &[("Video", "OutputCX", "1920"), ("Video", "OutputCY", "1080"), ("SimpleOutput", "StreamEncoder", "nvenc")],
    );
    ancienne_copie(&ctx, &f, &origine);
    // puis l'utilisateur change son dossier d'enregistrement dans OBS
    changer_dans_obs(&f, &[("SimpleOutput", "FilePath", DOSSIER_PERSO)]);
    let mut b = bloc(&pc, &ctx);
    assert!(boutons(&page(&mut b, &ctx)).contains(&"Remettre comme avant".to_string()));
    assert_eq!(action(&mut b, &ctx, "stream:remettre:obs"), ["Les réglages d'avant sont revenus dans OBS."]);
    let i = lire(&f);
    assert_eq!((i.valeur("Video", "OutputCX"), i.valeur("Video", "OutputCY")), (Some("1280"), Some("720")));
    assert_eq!(i.valeur("SimpleOutput", "StreamEncoder"), None);
    assert_eq!(i.valeur("SimpleOutput", "FilePath"), Some(DOSSIER_PERSO), "le dossier choisi depuis est gardé");
    assert!(!ctx.donnees.join(r"streameur\obs").exists());
}

#[test]
fn ancienne_copie_reprise_par_un_nouvel_optimiser() {
    let ctx = contexte("ancienne-reprise");
    let (pc, profil) = obs(&ctx);
    let f = profil.join("basic.ini");
    let origine = std::fs::read(&f).unwrap();
    changer_dans_obs(&f, &[("Video", "OutputCX", "1920"), ("Video", "OutputCY", "1080")]);
    ancienne_copie(&ctx, &f, &origine);
    let mut b = bloc(&pc, &ctx);
    action(&mut b, &ctx, "stream:optimiser:obs");
    changer_dans_obs(&f, &[("SimpleOutput", "FilePath", DOSSIER_PERSO)]);
    action(&mut b, &ctx, "stream:remettre:obs");
    let i = lire(&f);
    // l'état d'avant le PREMIER « Optimiser » (celui de l'ancienne copie), le dossier choisi depuis gardé
    assert_eq!((i.valeur("Video", "OutputCX"), i.valeur("Video", "FPSCommon")), (Some("1280"), Some("30")));
    assert_eq!(i.valeur("SimpleOutput", "FilePath"), Some(DOSSIER_PERSO));
}

#[test]
fn ancienne_copie_abimee_jamais_remise() {
    let ctx = contexte("abimee");
    let (pc, profil) = obs(&ctx);
    let f = profil.join("basic.ini");
    let mut b = bloc(&pc, &ctx);
    // la copie d'avant a été vidée (coupure de courant, antivirus)
    ancienne_copie(&ctx, &f, b"");
    let maintenant = std::fs::read(&f).unwrap();
    assert_eq!(
        action(&mut b, &ctx, "stream:remettre:obs"),
        [
            "La copie des anciens réglages est abîmée : Nevermind n'y touche pas, tes réglages d'OBS restent comme ils sont."
        ]
    );
    assert_eq!(std::fs::read(&f).unwrap(), maintenant, "basic.ini n'est pas vidé");
    assert!(!ctx.donnees.join(r"streameur\obs").exists(), "la copie inutilisable est effacée");
    // pareil avec des octets nuls
    ancienne_copie(&ctx, &f, &[0u8; 64]);
    action(&mut b, &ctx, "stream:remettre:obs");
    assert_eq!(std::fs::read(&f).unwrap(), maintenant);
}

#[test]
fn copie_abimee_jamais_remise() {
    let ctx = contexte("abimee2");
    let (pc, profil) = obs(&ctx);
    let f = profil.join("basic.ini");
    let mut b = bloc(&pc, &ctx);
    action(&mut b, &ctx, "stream:optimiser:obs");
    let fiche = ctx.donnees.join(r"streameur\obs\copie.json");
    let bonne = std::fs::read(&fiche).unwrap();
    let maintenant = std::fs::read(&f).unwrap();
    let abimee = "La copie des anciens réglages est abîmée : Nevermind n'y touche pas, tes réglages d'OBS restent comme ils sont.";
    // une valeur changée dans la copie (le contrôle ne correspond plus)
    let t = String::from_utf8(bonne.clone()).unwrap().replace("\"1280\"", "\"9999\"");
    assert!(t.as_bytes() != &bonne[..], "la valeur d'avant est bien dans la copie");
    std::fs::write(&fiche, t).unwrap();
    assert_eq!(action(&mut b, &ctx, "stream:remettre:obs"), [abimee]);
    assert_eq!(std::fs::read(&f).unwrap(), maintenant);
    // une copie coupée en deux : le bouton reste, et le clic le dit au lieu de ne rien faire
    std::fs::create_dir_all(fiche.parent().unwrap()).unwrap();
    std::fs::write(&fiche, &bonne[..bonne.len() / 2]).unwrap();
    assert!(boutons(&page(&mut b, &ctx)).contains(&"Remettre comme avant".to_string()));
    assert_eq!(action(&mut b, &ctx, "stream:remettre:obs"), [abimee]);
    assert_eq!(std::fs::read(&f).unwrap(), maintenant);
}

#[test]
fn obs_portable_trouve_quand_il_est_ouvert() {
    let ctx = contexte("portable");
    // un OBS dézippé, en mode portable, que rien n'a installé (ni registre, ni Program Files)
    let d = ctx.donnees.join("pc").join("OBS portable");
    let exe = d.join("bin").join("64bit").join("obs64.exe");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, b"MZ").unwrap();
    std::fs::write(d.join("portable_mode.txt"), b"").unwrap();
    let profil = reglages_obs(&d.join("config").join("obs-studio"));
    let pc = FauxPc::default();
    let mut b = BlocStream::avec_systeme(Box::new(pc.clone()));
    assert_eq!(page(&mut b, &ctx)["module"]["titre"], "Aucun logiciel de stream");
    // l'utilisateur ouvre son OBS : Nevermind le voit en revenant sur Performances
    pc.0.lock().unwrap().processus.push(Processus { nom: "obs64.exe".into(), chemin: Some(exe.clone()) });
    let _ = b.pages(&ctx, "accueil");
    let _ = b.pages(&ctx, "perf");
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    assert_eq!(signaux(&e, "streameur").last().unwrap()["disponible"], true);
    // il le ferme ; au prochain démarrage de Nevermind, il est toujours connu
    pc.0.lock().unwrap().processus.clear();
    let mut b = bloc(&pc, &ctx);
    let p = page(&mut b, &ctx);
    assert_eq!(p["module"]["titre"], "Prêt à régler OBS");
    assert!(action(&mut b, &ctx, "stream:optimiser:obs")[0].starts_with("OBS est réglé"));
    assert_eq!(lire(&profil.join("basic.ini")).valeur("Video", "OutputCX"), Some("1920"));
    // son dossier n'existe plus : il est oublié
    std::fs::remove_dir_all(&d).unwrap();
    let mut b = bloc(&pc, &ctx);
    assert_eq!(page(&mut b, &ctx)["module"]["titre"], "Aucun logiciel de stream");
}

#[test]
fn obs_portable_vu_sur_la_page_performances_deja_ouverte() {
    let ctx = contexte("portable-perf");
    let d = ctx.donnees.join("pc").join("OBS");
    let exe = d.join("bin").join("64bit").join("obs64.exe");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, b"MZ").unwrap();
    std::fs::write(d.join("portable_mode"), b"").unwrap();
    reglages_obs(&d.join("config").join("obs-studio"));
    let pc = FauxPc::default();
    let mut b = BlocStream::avec_systeme(Box::new(pc.clone()));
    let _ = b.pages(&ctx, "perf");
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    assert_eq!(signaux(&e, "streameur").last().unwrap()["disponible"], false);
    // OBS s'ouvre pendant que la page Performances est affichée : la carte se dégrise d'elle-même
    pc.0.lock().unwrap().processus.push(Processus { nom: "obs64.exe".into(), chemin: Some(exe) });
    std::thread::sleep(std::time::Duration::from_millis(3100));
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    assert_eq!(signaux(&e, "streameur").last().map(|s| s["disponible"].clone()), Some(json!(true)));
}
