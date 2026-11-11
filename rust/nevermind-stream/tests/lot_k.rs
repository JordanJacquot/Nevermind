//! Lot K (petits restants de l'audit du 10/11) pour le mode streameur : une copie des anciens réglages tenue un
//! instant par l'antivirus n'est plus prise pour abîmée (ni effacée), et un OBS portable n'est plus ignoré quand un
//! OBS installé existe aussi.
//!
//! Rien ne touche le vrai PC : faux dossiers OBS dans un dossier d'essai, faux PC (processus et installations donnés
//! par l'essai), jamais la vraie installation d'OBS.

use nevermind_noyau::{Bloc, Contexte, Effet, Effets, dossier_essai};
use nevermind_stream::BlocStream;
use nevermind_stream::calcul::{Carte, Fabricant, Logiciel};
use nevermind_stream::ini::Ini;
use nevermind_stream::sauvegarde::Copie;
use nevermind_stream::systeme::{Installation, Processus, Systeme};
use serde_json::{Value, json};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

fn contexte(nom: &str) -> Contexte {
    let d = dossier_essai(&format!("stream-lot-k-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    Contexte::nouveau(d.clone(), d, false, true)
}

fn annonces(e: &Effets) -> Vec<String> {
    e.0.iter().filter_map(|x| if let Effet::Annonce(t) = x { Some(t.clone()) } else { None }).collect()
}

fn page(b: &mut BlocStream, ctx: &Contexte) -> Value {
    b.pages(ctx, "streameur")["streameur"].clone()
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

fn sortie(f: &Path) -> Option<String> {
    Ini::lire(&std::fs::read(f).unwrap()).unwrap().valeur("Video", "OutputCX").map(str::to_string)
}

/// Une copie des anciens réglages, bonne, dans un dossier d'essai
fn copie_bonne(nom: &str) -> (Copie, PathBuf) {
    let d = dossier_essai(&format!("stream-lot-k-copie-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    let c = Copie::nouvelle(d.join("obs"));
    c.garder(&[], "11/11/2026 à 01:00", "Sans_nom").unwrap();
    assert!(c.existe());
    (c, d)
}

/// copie.json ouvert sans partage (comme un antivirus qui l'analyse) pendant `duree`, sur un autre fil
fn tenir(f: PathBuf, duree: Duration) -> std::thread::JoinHandle<()> {
    let (pret, attendre) = std::sync::mpsc::channel();
    let h = std::thread::spawn(move || {
        let _f = std::fs::OpenOptions::new().read(true).share_mode(0).open(&f).unwrap();
        pret.send(()).unwrap();
        std::thread::sleep(duree);
    });
    attendre.recv().unwrap();
    h
}

/// Audit du 10/11 (lot D, petits) : copie.json tenue un instant par l'antivirus (erreur de partage). Avant : la
/// lecture ratée valait « abîmée », et « Remettre comme avant » effaçait toute la copie. Maintenant la lecture est
/// réessayée : la copie est bien relue et remise.
#[test]
fn une_copie_tenue_un_instant_est_relue() {
    let (c, d) = copie_bonne("instant");
    let h = tenir(c.dossier.join("copie.json"), Duration::from_millis(400));
    let r = c.remettre();
    h.join().unwrap();
    let b = r.expect("la copie tenue un instant n'a pas été remise");
    assert_eq!(b.abimes, 0, "la copie tenue un instant a été prise pour abîmée");
    let _ = std::fs::remove_dir_all(&d);
}

/// copie.json tenue longtemps : on ne conclut jamais qu'elle est abîmée, rien n'est effacé ni remplacé, et le clic le
/// dit (une erreur, la copie reste pour réessayer)
#[test]
fn une_copie_tenue_longtemps_n_est_jamais_effacee() {
    let (c, d) = copie_bonne("longtemps");
    let fiche = c.dossier.join("copie.json");
    let avant = std::fs::read(&fiche).unwrap();
    let h = tenir(fiche.clone(), Duration::from_secs(3));
    let r = c.remettre();
    assert!(r.is_err(), "une copie illisible a été traitée comme abîmée : {r:?}");
    // « Optimiser » pendant ce temps : la copie d'origine n'est pas remplacée par l'état d'aujourd'hui
    let g = c.garder(&[], "11/11/2026 à 02:00", "Sans_nom");
    assert!(g.is_err(), "la copie illisible a été remplacée");
    h.join().unwrap();
    assert_eq!(std::fs::read(&fiche).unwrap(), avant, "la copie a changé");
    assert!(c.existe());
    let _ = std::fs::remove_dir_all(&d);
}

/// Un OBS installé et un OBS portable sur le même PC (audit du 10/11, lot D, petits). Avant : le premier trouvé,
/// l'installé, était toujours pris, même quand l'utilisateur se servait du portable, et « Optimiser » réglait le mauvais
/// OBS. Maintenant : celui qui est ouvert ; sinon le dernier vu ouvert (gardé dans streameur.json).
#[test]
fn le_bon_obs_quand_il_y_en_a_deux() {
    let ctx = contexte("deux-obs");
    let racine = ctx.donnees.join("pc");
    // l'OBS installé (Program Files, réglages dans AppData)
    let installe = racine.join("Program Files").join("obs-studio");
    let exe_installe = installe.join("bin").join("64bit").join("obs64.exe");
    std::fs::create_dir_all(exe_installe.parent().unwrap()).unwrap();
    std::fs::write(&exe_installe, b"MZ").unwrap();
    let profil_installe = reglages_obs(&racine.join("AppData").join("obs-studio"));
    // l'OBS portable (dézippé ailleurs, réglages dans son dossier)
    let portable = racine.join("OBS portable");
    let exe_portable = portable.join("bin").join("64bit").join("obs64.exe");
    std::fs::create_dir_all(exe_portable.parent().unwrap()).unwrap();
    std::fs::write(&exe_portable, b"MZ").unwrap();
    std::fs::write(portable.join("portable_mode.txt"), b"").unwrap();
    let profil_portable = reglages_obs(&portable.join("config").join("obs-studio"));
    let avant_installe = sortie(&profil_installe.join("basic.ini"));
    assert_ne!(avant_installe.as_deref(), Some("1920"), "l'essai suppose un OBS pas encore réglé");

    let pc = FauxPc::default();
    pc.0.lock().unwrap().installations.push(Installation {
        logiciel: Logiciel::Obs,
        dossier: installe.clone(),
        reglages: racine.join("AppData").join("obs-studio"),
        version: Some("33.0.1".into()),
    });
    // 1. L'utilisateur ouvre son OBS portable (Nevermind le voit), puis le ferme pour régler : c'est lui qui est réglé
    pc.0.lock().unwrap().processus.push(Processus { nom: "obs64.exe".into(), chemin: Some(exe_portable.clone()) });
    let mut b = bloc(&pc, &ctx);
    let _ = page(&mut b, &ctx);
    pc.0.lock().unwrap().processus.clear();
    let mut b = bloc(&pc, &ctx);
    assert!(action(&mut b, &ctx, "stream:optimiser:obs")[0].starts_with("OBS est réglé"));
    assert_eq!(sortie(&profil_portable.join("basic.ini")).as_deref(), Some("1920"), "le portable utilisé n'est pas réglé");
    assert_eq!(sortie(&profil_installe.join("basic.ini")), avant_installe, "l'OBS installé, pas utilisé, a été réglé");
    // 2. Au prochain démarrage de Nevermind, c'est toujours lui : « Remettre comme avant » remet le portable
    let mut b = bloc(&pc, &ctx);
    let _ = action(&mut b, &ctx, "stream:remettre:obs");
    assert_ne!(sortie(&profil_portable.join("basic.ini")).as_deref(), Some("1920"), "le portable n'est pas remis");
    // 3. L'utilisateur ouvre ensuite l'OBS installé : c'est lui qui compte, et il le reste une fois fermé
    pc.0.lock().unwrap().processus.push(Processus { nom: "obs64.exe".into(), chemin: Some(exe_installe.clone()) });
    let mut b = bloc(&pc, &ctx);
    let _ = page(&mut b, &ctx);
    pc.0.lock().unwrap().processus.clear();
    let mut b = bloc(&pc, &ctx);
    assert!(action(&mut b, &ctx, "stream:optimiser:obs")[0].starts_with("OBS est réglé"));
    assert_eq!(sortie(&profil_installe.join("basic.ini")).as_deref(), Some("1920"), "le dernier OBS utilisé n'est pas réglé");
    assert_ne!(sortie(&profil_portable.join("basic.ini")).as_deref(), Some("1920"), "le portable a été réglé");
}
