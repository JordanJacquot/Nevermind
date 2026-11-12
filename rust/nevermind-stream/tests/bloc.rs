//! Le bloc entier avec un faux PC : les logiciels « installés », les processus ouverts, les cartes et l'écran sont
//! donnés par le test ; les réglages d'OBS et de Streamlabs sont dans un dossier d'essai. Rien ne touche le vrai PC.

use nevermind_noyau::{Bloc, Contexte, Effet, Effets, dossier_essai};
use nevermind_stream::BlocStream;
use nevermind_stream::calcul::{Carte, Fabricant, Logiciel};
use nevermind_stream::systeme::{Installation, Processus, Systeme};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Pc {
    installations: Vec<Installation>,
    processus: Vec<Processus>,
    cartes: Vec<Carte>,
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
        self.0.lock().unwrap().cartes.clone()
    }
    fn ecran(&self) -> Option<(u32, u32)> {
        Some((1920, 1080))
    }
}

const SECRET: &str = "SECRET-CLE-DE-STREAM-1234";

/// Un OBS d'essai (réglages comme ceux créés par OBS 33) dans `racine`, avec un service.json qui contient une clé
fn obs(racine: &Path, service: &str) -> (Installation, PathBuf) {
    let reglages = racine.join("obs-studio");
    let p = reglages.join("basic").join("profiles").join("Sans_nom");
    std::fs::create_dir_all(&p).unwrap();
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("obs33");
    for n in ["global.ini", "user.ini"] {
        std::fs::copy(f.join(n), reglages.join(n)).unwrap();
    }
    std::fs::copy(f.join("basic.ini"), p.join("basic.ini")).unwrap();
    std::fs::write(p.join("service.json"), service).unwrap();
    let i = Installation {
        logiciel: Logiciel::Obs,
        dossier: racine.join("Program Files").join("obs-studio"),
        reglages,
        version: Some("33.0.1".into()),
    };
    (i, p)
}

fn streamlabs(racine: &Path) -> Installation {
    let r = racine.join("slobs-client");
    std::fs::create_dir_all(&r).unwrap();
    std::fs::write(r.join("basic.ini"), "\u{feff}[SimpleOutput]\nStreamEncoder=x264\nVBitrate=2500\n").unwrap();
    std::fs::write(
        r.join("service.json"),
        format!(r#"{{"type":"rtmp_common","settings":{{"service":"YouTube - RTMPS","key":"{SECRET}"}}}}"#),
    )
    .unwrap();
    Installation {
        logiciel: Logiciel::Streamlabs,
        dossier: racine.join("Program Files").join("Streamlabs OBS"),
        reglages: r,
        version: None,
    }
}

fn contexte(nom: &str) -> Contexte {
    let d = dossier_essai(&format!("stream-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    Contexte::nouveau(d.clone(), d, false, true)
}

fn nvidia() -> Vec<Carte> {
    vec![Carte { nom: "NVIDIA GeForce RTX 3060".into(), fabricant: Fabricant::Nvidia }]
}

fn page(b: &mut BlocStream, ctx: &Contexte) -> Value {
    b.pages(ctx, "streameur")["streameur"].clone()
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

/// Tous les textes d'une valeur (pour y chercher un tiret long ou la clé)
fn textes(v: &Value, l: &mut Vec<String>) {
    match v {
        Value::String(s) => l.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| textes(x, l)),
        Value::Object(o) => o.values().for_each(|x| textes(x, l)),
        _ => {}
    }
}

fn sans_tiret_long(v: &Value) {
    let mut l = Vec::new();
    textes(v, &mut l);
    for t in l {
        assert!(!t.contains('\u{2014}') && !t.contains('\u{2013}'), "tiret long dans « {t} »");
    }
}

fn mesure(b: &mut BlocStream, ctx: &Contexte, envoi_mbps: f64) {
    let mut e = Effets::default();
    b.signal(ctx, "debit-mesure", &json!({ "R": [12.0, 300.0, envoi_mbps, 0.0] }), &mut e);
    assert!(e.0.contains(&Effet::Rafraichir));
}

#[test]
fn sans_logiciel_la_carte_est_grisee() {
    let ctx = contexte("sans");
    let mut b = BlocStream::avec_systeme(Box::new(FauxPc::default()));
    let p = page(&mut b, &ctx);
    assert_eq!(p["module"]["titre"], "Aucun logiciel de stream");
    assert_eq!(p["module"]["texte"], "Pour utiliser le mode streameur, il faut avoir installé OBS ou Streamlabs.");
    assert_eq!(p["parent"], "perf");
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    let s = signaux(&e, "streameur");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0]["disponible"], false);
    assert_eq!(s[0]["carte"]["inactif"], true);
    assert_eq!(s[0]["carte"]["note"], "Pour utiliser le mode streameur, il faut avoir installé OBS ou Streamlabs.");
    // pas de mesure lancée pour rien
    assert!(signaux(&e, "test-debit").is_empty());
    sans_tiret_long(&p);
}

#[test]
fn detection_refaite_a_chaque_ouverture() {
    let ctx = contexte("detection");
    let pc = FauxPc::default();
    let mut b = BlocStream::avec_systeme(Box::new(pc.clone()));
    assert_eq!(page(&mut b, &ctx)["module"]["titre"], "Aucun logiciel de stream");
    // OBS s'installe pendant que Nevermind est ouvert : vu à la prochaine ouverture de la page
    let (i, _) = obs(&ctx.donnees.join("pc"), "{}");
    pc.0.lock().unwrap().installations.push(i);
    let _ = b.pages(&ctx, "accueil");
    let p = b.pages(&ctx, "perf");
    assert!(p.contains_key("streameur"));
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    let s = signaux(&e, "streameur");
    assert_eq!(s.last().unwrap()["disponible"], true);
    assert_eq!(s.last().unwrap()["carte"]["id"], "streameur");
    assert!(s.last().unwrap()["carte"].get("inactif").is_none());
}

#[test]
fn mesure_automatique_une_seule_fois_et_gardee() {
    let ctx = contexte("mesure");
    let (i, _) = obs(&ctx.donnees.join("pc"), "{}");
    let pc = FauxPc::default();
    pc.0.lock().unwrap().installations.push(i);
    let mut b = BlocStream::avec_systeme(Box::new(pc.clone()));
    let p = page(&mut b, &ctx);
    assert_eq!(p["module"]["texte"], "Nevermind mesure d'abord ta connexion (environ 30 secondes).");
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    assert_eq!(signaux(&e, "test-debit").len(), 1, "la page lance la mesure toute seule");
    let _ = b.pages(&ctx, "perf");
    let _ = page(&mut b, &ctx);
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    assert!(signaux(&e, "test-debit").is_empty(), "une seule fois");
    mesure(&mut b, &ctx, 8.5);
    let p = page(&mut b, &ctx);
    assert_eq!(p["module"]["boutons"][0]["texte"], "Optimiser");
    // gardée pour la prochaine fois (streameur.json)
    let mut b2 = BlocStream::avec_systeme(Box::new(pc));
    let p = page(&mut b2, &ctx);
    let resume = p["module"]["resume"].as_array().unwrap();
    assert!(resume.iter().any(|r| r["nom"] == "Envoi" && r["valeur"] == "8,5 Mb/s"), "{resume:?}");
    let mut e = Effets::default();
    b2.tic(&ctx, false, &mut e);
    assert!(signaux(&e, "test-debit").is_empty(), "déjà mesurée : pas de nouvelle mesure");
    // une mesure ratée (serveurs injoignables : -1) ne change rien
    let mut e = Effets::default();
    b2.signal(&ctx, "debit-mesure", &json!({ "R": [-1, -1, -1] }), &mut e);
    let p = page(&mut b2, &ctx);
    assert!(p["module"]["resume"].as_array().unwrap().iter().any(|r| r["valeur"] == "8,5 Mb/s"));
}

#[test]
fn obs_ouvert_rien_n_est_change() {
    let ctx = contexte("ouvert");
    let (i, profil) = obs(&ctx.donnees.join("pc"), "{}");
    let pc = FauxPc::default();
    {
        let mut x = pc.0.lock().unwrap();
        x.installations.push(i);
        x.cartes = nvidia();
        x.processus.push(Processus {
            nom: "obs64.exe".into(),
            chemin: Some(ctx.donnees.join(r"pc\Program Files\obs-studio\bin\64bit\obs64.exe")),
        });
    }
    let avant = std::fs::read(profil.join("basic.ini")).unwrap();
    let mut b = BlocStream::avec_systeme(Box::new(pc.clone()));
    mesure(&mut b, &ctx, 20.0);
    let p = page(&mut b, &ctx);
    assert_eq!(p["blocs"][0]["texte"], "OBS est ouvert");
    let mut e = Effets::default();
    assert!(b.action(&ctx, "stream:optimiser:obs", &Value::Null, &mut e));
    assert_eq!(annonces(&e), ["Ferme OBS puis reclique sur Optimiser"]);
    assert_eq!(std::fs::read(profil.join("basic.ini")).unwrap(), avant);
    assert!(!ctx.donnees.join(r"streameur\obs\copie.json").exists());
    assert_eq!(page(&mut b, &ctx)["module"]["titre"], "Ferme OBS puis reclique sur Optimiser");
    // le moteur de Streamlabs (un autre obs64.exe) n'est pas OBS Studio
    pc.0.lock().unwrap().processus = vec![Processus {
        nom: "obs64.exe".into(),
        chemin: Some(PathBuf::from(
            r"C:\Program Files\Streamlabs OBS\resources\app.asar.unpacked\node_modules\obs-studio-node\obs64.exe",
        )),
    }];
    let mut e = Effets::default();
    b.action(&ctx, "stream:optimiser:obs", &Value::Null, &mut e);
    assert!(annonces(&e)[0].starts_with("OBS est réglé : "), "{:?}", annonces(&e));
}

#[test]
fn optimiser_puis_remettre_comme_avant() {
    let ctx = contexte("optimiser");
    let (i, profil) =
        obs(&ctx.donnees.join("pc"), r#"{"type":"rtmp_common","settings":{"service":"Twitch","server":"auto"}}"#);
    let pc = FauxPc::default();
    {
        let mut x = pc.0.lock().unwrap();
        x.installations.push(i);
        x.cartes = nvidia();
    }
    let avant = std::fs::read(profil.join("basic.ini")).unwrap();
    let mut b = BlocStream::avec_systeme(Box::new(pc));
    mesure(&mut b, &ctx, 20.0);
    let p = page(&mut b, &ctx);
    assert_eq!(p["module"]["titre"], "Prêt à régler OBS");
    assert_eq!(
        p["module"]["texte"],
        "Réglage conseillé : 1080p à 60 images par seconde, encodé par ta carte NVIDIA, 6\u{202f}000 kb/s."
    );
    let t = p.to_string();
    assert!(t.contains("Trouvée dans les réglages d'OBS : Twitch. Tu peux la changer."), "{t}");
    let mut e = Effets::default();
    assert!(b.action(&ctx, "stream:optimiser:obs", &Value::Null, &mut e));
    assert_eq!(
        annonces(&e),
        ["OBS est réglé : 1080p à 60 images par seconde, encodé par ta carte NVIDIA, 6\u{202f}000 kb/s."]
    );
    let carte = signaux(&e, "streameur");
    assert_ne!(std::fs::read(profil.join("basic.ini")).unwrap(), avant);
    let p = page(&mut b, &ctx);
    assert_eq!(p["module"]["titre"], "OBS est réglé");
    assert_eq!(p["module"]["boutons"][1]["texte"], "Remettre comme avant");
    let changes: Vec<&Value> = p["blocs"].as_array().unwrap().iter().filter(|x| x["type"] == "liste").collect();
    assert!(
        changes[0]["elements"].as_array().unwrap().iter().any(|l| l.as_str().unwrap().starts_with("Qualité : 1080p"))
    );
    sans_tiret_long(&p);
    // la carte de Performances dit « Réglé »
    assert_eq!(carte.last().unwrap()["carte"]["etiquette"], "Réglé");
    // Remettre comme avant : les octets d'avant
    let mut e = Effets::default();
    assert!(b.action(&ctx, "stream:remettre:obs", &Value::Null, &mut e));
    assert_eq!(annonces(&e), ["Les réglages d'avant sont revenus dans OBS."]);
    assert_eq!(std::fs::read(profil.join("basic.ini")).unwrap(), avant);
    let p = page(&mut b, &ctx);
    assert_eq!(p["module"]["titre"], "Prêt à régler OBS");
    assert_eq!(p["module"]["boutons"].as_array().unwrap().len(), 1);
}

#[test]
fn changer_de_plateforme() {
    let ctx = contexte("plateforme");
    let (i, profil) = obs(&ctx.donnees.join("pc"), "{}");
    let pc = FauxPc::default();
    {
        let mut x = pc.0.lock().unwrap();
        x.installations.push(i);
        x.cartes = nvidia();
    }
    let mut b = BlocStream::avec_systeme(Box::new(pc));
    mesure(&mut b, &ctx, 20.0);
    let p = page(&mut b, &ctx);
    assert!(p.to_string().contains("Pas trouvée dans les réglages d'OBS : choisis-la."));
    let mut e = Effets::default();
    assert!(b.action(&ctx, "stream:plateforme:obs", &json!("kick"), &mut e));
    let p = page(&mut b, &ctx);
    assert!(p["module"]["texte"].as_str().unwrap().ends_with("8\u{202f}000 kb/s."), "{}", p["module"]["texte"]);
    assert!(p.to_string().contains("Toutes les 2 secondes"));
    let mut e = Effets::default();
    b.action(&ctx, "stream:optimiser:obs", &Value::Null, &mut e);
    let j: Value = serde_json::from_slice(&std::fs::read(profil.join("streamEncoder.json")).unwrap()).unwrap();
    assert_eq!(j["keyint_sec"], 2);
    // le choix est gardé
    let p = page(
        &mut BlocStream::avec_systeme(Box::new(FauxPc(Arc::new(Mutex::new(Pc {
            installations: vec![obs(&ctx.donnees.join("pc2"), "{}").0],
            ..Default::default()
        }))))),
        &ctx,
    );
    assert!(p.to_string().contains("Choisie par toi."));
    // une plateforme inconnue n'est pas prise
    let mut e = Effets::default();
    b.action(&ctx, "stream:plateforme:obs", &json!("myspace"), &mut e);
    assert!(page(&mut b, &ctx)["module"]["texte"].as_str().unwrap().ends_with("8\u{202f}000 kb/s."));
}

#[test]
fn obs_et_streamlabs_deux_onglets() {
    let ctx = contexte("deux");
    let racine = ctx.donnees.join("pc");
    let (i, _) = obs(&racine, "{}");
    let s = streamlabs(&racine);
    // (le basic.ini d'un Streamlabs neuf, essai réel du 12/11/2026 : son [Video] est celui de la sortie verticale)
    let ini = s.reglages.join("basic.ini");
    std::fs::write(
        &ini,
        "\u{feff}[SimpleOutput]\nStreamEncoder=x264\nVBitrate=2500\n[Video]\nBaseCX=720\nBaseCY=1280\n",
    )
    .unwrap();
    let avant = std::fs::read(&ini).unwrap();
    let pc = FauxPc::default();
    {
        let mut x = pc.0.lock().unwrap();
        x.installations = vec![s.clone(), i];
        x.cartes = nvidia();
    }
    let mut b = BlocStream::avec_systeme(Box::new(pc));
    mesure(&mut b, &ctx, 20.0);
    let p = page(&mut b, &ctx);
    let o = p["onglets"].as_array().unwrap();
    assert_eq!((o[0]["nom"].as_str(), o[1]["nom"].as_str()), (Some("OBS Studio"), Some("Streamlabs")));
    let t = o[1].to_string();
    assert!(t.contains("Trouvée dans les réglages de Streamlabs : YouTube."), "{t}");
    // Nevermind règle lui-même la résolution et les images par seconde : plus rien à choisir soi-même ; et pas sur le
    // 720x1280 de basic.ini (l'écran, 1920x1080, tant que Streamlabs ne donne pas sa vraie image de base)
    assert!(!t.contains("À choisir toi-même"), "{t}");
    assert!(t.contains("1080p à 60 images par seconde"), "{t}");
    assert!(t.contains("Streamlabs est fermé") && t.contains("Ouvre-le avant de cliquer sur Optimiser"), "{t}");
    // Streamlabs fermé : il se règle ouvert, par son API ; ses fichiers ne sont plus touchés
    let mut e = Effets::default();
    b.action(&ctx, "stream:optimiser:streamlabs", &Value::Null, &mut e);
    assert_eq!(annonces(&e), ["Ouvre Streamlabs puis reclique sur Optimiser"]);
    assert_eq!(std::fs::read(&ini).unwrap(), avant);
    assert!(!ctx.donnees.join("streameur").join("streamlabs").join("copie.json").exists());
    sans_tiret_long(&page(&mut b, &ctx));
}

#[test]
fn la_cle_de_stream_n_est_jamais_divulguee() {
    let ctx = contexte("secret");
    let racine = ctx.donnees.join("pc");
    let service = format!(
        r#"{{"type":"rtmp_custom","settings":{{"server":"rtmp://moi:{SECRET}@fa723fc1b171.global-contribute.live-video.net/app/{SECRET}","key":"{SECRET}","use_auth":true,"username":"{SECRET}","password":"{SECRET}"}}}}"#
    );
    let (i, profil) = obs(&racine, &service);
    let s = streamlabs(&racine);
    let pc = FauxPc::default();
    {
        let mut x = pc.0.lock().unwrap();
        x.installations = vec![i, s];
        x.cartes = nvidia();
    }
    let mut b = BlocStream::avec_systeme(Box::new(pc));
    let mut vu = String::new();
    mesure(&mut b, &ctx, 20.0);
    vu.push_str(&page(&mut b, &ctx).to_string());
    for a in ["stream:optimiser:obs", "stream:optimiser:streamlabs", "stream:remettre:obs", "stream:optimiser:obs"] {
        let mut e = Effets::default();
        b.action(&ctx, a, &Value::Null, &mut e);
        b.tic(&ctx, false, &mut e);
        vu.push_str(&format!("{:?}", e.0));
        vu.push_str(&page(&mut b, &ctx).to_string());
        vu.push_str(&format!("{:?}", b.pages(&ctx, "perf")));
    }
    assert!(vu.contains("Kick"), "la plateforme est bien lue");
    assert!(!vu.contains("SECRET"), "la clé apparaît à l'écran ou dans un effet");
    // ni au journal, ni dans streameur.json, ni dans les copies des anciens réglages, ni dans les erreurs gardées
    fn fichiers(d: &Path, l: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() { fichiers(&p, l) } else { l.push(p) }
        }
    }
    let mut l = Vec::new();
    fichiers(&ctx.donnees, &mut l);
    let a_nous: Vec<&PathBuf> = l.iter().filter(|p| !p.starts_with(&racine)).collect();
    assert!(a_nous.iter().any(|p| p.ends_with("journal.txt")));
    assert!(a_nous.iter().any(|p| p.ends_with("streameur.json")));
    assert!(a_nous.iter().any(|p| p.ends_with("copie.json")));
    for p in a_nous {
        let t = String::from_utf8_lossy(&std::fs::read(p).unwrap()).to_string();
        assert!(!t.contains("SECRET"), "la clé est dans {}", p.display());
    }
    assert!(ctx.erreurs().iter().all(|e| !e.contains("SECRET")));
    // service.json n'est jamais réécrit
    assert_eq!(std::fs::read_to_string(profil.join("service.json")).unwrap(), service);
}

#[test]
fn actions_des_autres_blocs_laissees() {
    let ctx = contexte("autres");
    let mut b = BlocStream::avec_systeme(Box::new(FauxPc::default()));
    let mut e = Effets::default();
    assert!(!b.action(&ctx, "clic:BtnApply", &Value::Null, &mut e));
    assert!(!b.action(&ctx, "stream:inconnu:obs", &Value::Null, &mut e));
    assert!(!b.action(&ctx, "stream:optimiser:vlc", &Value::Null, &mut e));
    // sans logiciel, « Optimiser » le dit simplement
    assert!(b.action(&ctx, "stream:optimiser:obs", &Value::Null, &mut e));
    assert_eq!(annonces(&e), ["Pour utiliser le mode streameur, il faut avoir installé OBS ou Streamlabs."]);
}

#[test]
fn jamais_ouvert_et_reglages_abimes() {
    let ctx = contexte("jamais");
    let racine = ctx.donnees.join("pc");
    let (mut i, profil) = obs(&racine, "{}");
    let pc = FauxPc::default();
    i.reglages = racine.join("vide");
    pc.0.lock().unwrap().installations.push(i.clone());
    let mut b = BlocStream::avec_systeme(Box::new(pc.clone()));
    mesure(&mut b, &ctx, 20.0);
    let p = page(&mut b, &ctx);
    assert_eq!(p["module"]["titre"], "Ouvre OBS une première fois");
    assert!(p["module"]["boutons"].as_array().unwrap().is_empty());
    // basic.ini abîmé : Nevermind n'y touche pas
    std::fs::write(profil.join("basic.ini"), [0xC3, 0x28, 0x00]).unwrap();
    pc.0.lock().unwrap().installations = vec![Installation { reglages: racine.join("obs-studio"), ..i }];
    let _ = b.pages(&ctx, "perf");
    let p = page(&mut b, &ctx);
    assert_eq!(p["module"]["titre"], "Réglages d'OBS illisibles");
    let mut e = Effets::default();
    b.action(&ctx, "stream:optimiser:obs", &Value::Null, &mut e);
    assert_eq!(annonces(&e), ["Les réglages d'OBS ne se lisent pas : rien n'est changé."]);
    assert_eq!(std::fs::read(profil.join("basic.ini")).unwrap(), [0xC3, 0x28, 0x00]);
    sans_tiret_long(&p);
}

#[test]
fn connexion_lente_deconseille() {
    let ctx = contexte("lente");
    let (i, _) = obs(&ctx.donnees.join("pc"), "{}");
    let pc = FauxPc::default();
    pc.0.lock().unwrap().installations.push(i);
    let mut b = BlocStream::avec_systeme(Box::new(pc));
    mesure(&mut b, &ctx, 1.2);
    let p = page(&mut b, &ctx);
    assert_eq!(p["module"]["boutons"][0]["texte"], "Optimiser quand même");
    let t = p.to_string();
    assert!(t.contains("Connexion trop lente pour bien streamer"));
    assert!(t.contains("Ta connexion envoie 1,2 Mb/s."));
    // sans carte graphique : le processeur, déconseillé pendant un jeu
    assert!(t.contains("Aucune carte graphique de ce PC ne sait encoder"));
    assert!(t.contains("480p à 30 images par seconde"));
}
