//! Streamlabs Desktop réglé par son API, avec un faux Streamlabs qui répond comme le vrai (formes relevées sur
//! Streamlabs Desktop 1.21.9 et 1.22.4, essai réel du 12/11/2026, workflow streamlabs-reel.yml) : sa vidéo dans
//! `VideoSettingsService.state`, sa Sortie dans le formulaire de `SettingsService.fetchSettingsFromObs("Output")`, changée
//! par `setSettingValue`. Rien ne touche le vrai PC.

use nevermind_noyau::{Bloc, Contexte, Effet, Effets, dossier_essai};
use nevermind_stream::BlocStream;
use nevermind_stream::calcul::{Carte, Fabricant, Logiciel};
use nevermind_stream::systeme::{Installation, Processus, Systeme};
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Ce que le faux Streamlabs garde
struct Etat {
    ouvert: bool,
    /// L'API répond (elle peut être coupée dans ses Paramètres, ou Streamlabs finit de s'ouvrir)
    api: bool,
    enregistre: bool,
    video: Map<String, Value>,
    /// Les valeurs de la Sortie (des deux modes, comme basic.ini)
    sortie: Map<String, Value>,
    /// Les demandes reçues (méthode, ressource)
    demandes: Vec<String>,
    /// Une valeur que Streamlabs refuse de prendre (il la remet comme elle était)
    refuse: Option<String>,
    /// Le débit le plus haut de la plateforme de Streamlabs (il l'applique lui-même)
    limite: Option<u64>,
    /// Les valeurs du formulaire Avancé (débit qui s'adapte, reconnexion, priorité) et du formulaire Son
    avancees: Map<String, Value>,
    son: Map<String, Value>,
    /// Les sources des scènes (`SourcesService.getSources`)
    sources: Value,
}

#[derive(Clone)]
struct Faux(Arc<Mutex<Etat>>, PathBuf);

fn champ(nom: &str, v: &Value) -> Value {
    json!({ "name": nom, "value": v, "type": "OBS_PROPERTY_INT", "description": nom, "visible": true, "enabled": true })
}

fn liste(nom: &str, v: &Value, options: &[&str]) -> Value {
    let o: Vec<Value> = options.iter().map(|x| json!({ "description": x, "value": x })).collect();
    json!({ "name": nom, "value": v, "type": "OBS_PROPERTY_LIST", "options": o, "visible": true, "enabled": true })
}

impl Etat {
    fn formulaire(&self) -> Value {
        let s = &self.sortie;
        let v = |k: &str| s.get(k).cloned().unwrap_or(Value::Null);
        let avance = v("Mode") == json!("Advanced");
        let mut streaming = Vec::new();
        let mut groupes = vec![
            json!({ "nameSubCategory": "Untitled", "parameters": [liste("Mode", &v("Mode"), &["Simple", "Advanced"])] }),
        ];
        if avance {
            let enc = v("Encoder");
            streaming.push(liste("Encoder", &enc, &["obs_x264", "jim_nvenc"]));
            streaming.push(champ("TrackIndex", &v("TrackIndex")));
            streaming.push(champ("rate_control", &v("rate_control")));
            streaming.push(champ("bitrate", &v("bitrate")));
            streaming.push(champ("keyint_sec", &v("keyint_sec")));
            if enc == json!("jim_nvenc") {
                streaming.push(champ("preset2", &v("preset2")));
            } else {
                streaming.push(champ("preset", &v("preset")));
            }
            groupes.push(json!({ "nameSubCategory": "Streaming", "parameters": streaming }));
            groupes.push(
                json!({ "nameSubCategory": "Recording", "parameters": [champ("RecFilePath", &v("RecFilePath"))] }),
            );
            groupes.push(json!({ "nameSubCategory": "Audio - Track 1", "parameters": [champ("Track1Bitrate", &v("Track1Bitrate"))] }));
        } else {
            let enc = v("StreamEncoder");
            streaming.push(champ("VBitrate", &v("VBitrate")));
            streaming.push(liste("StreamEncoder", &enc, &["x264", "nvenc"]));
            streaming.push(champ("ABitrate", &v("ABitrate")));
            if enc == json!("nvenc") {
                streaming.push(champ("NVENCPreset2", &v("NVENCPreset2")));
            } else {
                streaming.push(champ("Preset", &v("Preset")));
            }
            groupes.push(json!({ "nameSubCategory": "Streaming", "parameters": streaming }));
            groupes.push(json!({ "nameSubCategory": "Recording", "parameters": [champ("FilePath", &v("FilePath"))] }));
        }
        Value::Array(groupes)
    }

    /// Le formulaire d'une catégorie (Output, Advanced, Audio)
    fn formulaire_de(&self, cat: &str) -> Value {
        match cat {
            "Advanced" => {
                let a = &self.avancees;
                let v = |k: &str| a.get(k).cloned().unwrap_or(Value::Null);
                json!([
                    { "nameSubCategory": "General", "parameters": [liste("ProcessPriority", &v("ProcessPriority"), &["High", "AboveNormal", "Normal", "BelowNormal", "Idle"])] },
                    { "nameSubCategory": "Automatically Reconnect", "parameters": [champ("Reconnect", &v("Reconnect")), champ("RetryDelay", &json!(2))] },
                    { "nameSubCategory": "Network", "parameters": [champ("DynamicBitrate", &v("DynamicBitrate"))] },
                ])
            }
            "Audio" => {
                json!([{ "nameSubCategory": "Untitled", "parameters": [liste("SampleRate", &self.son["SampleRate"], &["44100", "48000"])] }])
            }
            _ => self.formulaire(),
        }
    }

    /// Le réglage d'une catégorie, s'il existe dans son formulaire
    fn existe(&self, cat: &str, nom: &str) -> bool {
        self.formulaire_de(cat)
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["parameters"].as_array().unwrap().iter().any(|p| p["name"].as_str() == Some(nom)))
    }
}

impl Systeme for Faux {
    fn installations(&self) -> Vec<Installation> {
        vec![Installation {
            logiciel: Logiciel::Streamlabs,
            dossier: PathBuf::from(r"C:\Program Files\Streamlabs OBS"),
            reglages: self.1.clone(),
            version: Some("1.22.4".into()),
        }]
    }
    fn processus(&self) -> Vec<Processus> {
        if self.0.lock().unwrap().ouvert {
            vec![Processus {
                nom: "Streamlabs OBS.exe".into(),
                chemin: Some(PathBuf::from(r"C:\Program Files\Streamlabs OBS\Streamlabs OBS.exe")),
            }]
        } else {
            Vec::new()
        }
    }
    fn cartes(&self) -> Vec<Carte> {
        vec![Carte { nom: "NVIDIA GeForce RTX 3060".into(), fabricant: Fabricant::Nvidia }]
    }
    fn ecran(&self) -> Option<(u32, u32)> {
        Some((2560, 1440))
    }
    fn streamlabs(&self, ressource: &str, methode: &str, args: &[Value]) -> Result<Value, String> {
        let mut e = self.0.lock().unwrap();
        if !e.ouvert {
            return Err("Streamlabs n'est pas ouvert".into());
        }
        if !e.api {
            return Err("Streamlabs ne répond pas".into());
        }
        e.demandes.push(format!("{ressource}.{methode}"));
        match (ressource, methode) {
            ("StreamingService", "getModel") => {
                let r = if e.enregistre { "recording" } else { "offline" };
                Ok(json!({ "streamingStatus": "offline", "recordingStatus": r, "replayBufferStatus": "offline" }))
            }
            ("VideoSettingsService", "state") => Ok(json!({ "horizontal": e.video, "vertical": { "baseWidth": 720 } })),
            ("VideoSettingsService", "setSettings") => {
                assert_eq!(args.get(1), Some(&json!("horizontal")));
                for (k, v) in args[0].as_object().unwrap() {
                    if e.refuse.as_deref() != Some(&format!("Video/{k}")) {
                        e.video.insert(k.clone(), v.clone());
                    }
                }
                Ok(Value::Null)
            }
            ("SettingsService", "fetchSettingsFromObs") => {
                let cat = args[0].as_str().unwrap_or_default();
                assert!(
                    ["Output", "Advanced", "Audio"].contains(&cat),
                    "seulement Sortie, Avancé et Son (jamais la catégorie du stream, où est la clé) : {cat}"
                );
                Ok(json!({ "type": 0, "formData": e.formulaire_de(cat) }))
            }
            ("SourcesService", "getSources") => Ok(e.sources.clone()),
            ("SettingsService", "setSettingValue") if args[0] != json!("Output") => {
                let cat = args[0].as_str().unwrap().to_string();
                assert!(cat == "Advanced" || cat == "Audio", "catégorie inattendue : {cat}");
                let nom = args[1].as_str().unwrap().to_string();
                if e.existe(&cat, &nom) && e.refuse.as_deref() != Some(&format!("{cat}/{nom}")) {
                    let m = if cat == "Advanced" { &mut e.avancees } else { &mut e.son };
                    m.insert(nom, args[2].clone());
                }
                Ok(Value::Null)
            }
            ("SettingsService", "setSettingValue") => {
                assert_eq!(args[0], json!("Output"));
                let nom = args[1].as_str().unwrap().to_string();
                // (comme Streamlabs : un champ qui n'est pas dans le formulaire du mode actuel n'est pas changé)
                let existe = e.formulaire().as_array().unwrap().iter().any(|g| {
                    g["parameters"].as_array().unwrap().iter().any(|p| p["name"].as_str() == Some(nom.as_str()))
                });
                if existe && e.refuse.as_deref() != Some(&format!("Output/{nom}")) {
                    let mut v = args[2].clone();
                    // (« appliquer les réglages du service » : le débit limité à celui de sa plateforme)
                    if let Some(l) =
                        e.limite.filter(|l| (nom == "bitrate" || nom == "VBitrate") && v.as_u64() > Some(*l))
                    {
                        v = json!(l);
                    }
                    e.sortie.insert(nom, v);
                }
                Ok(Value::Null)
            }
            _ => Err(format!("METHOD_NOT_FOUND {methode}")),
        }
    }
}

/// Un Streamlabs comme celui de l'essai réel après son premier lancement, l'utilisateur à 60 images par seconde
fn faux(ctx: &Contexte) -> Faux {
    let r = ctx.donnees.join("pc").join("slobs-client");
    std::fs::create_dir_all(&r).unwrap();
    // (le basic.ini d'un Streamlabs neuf : son [Video] est celui de la sortie verticale)
    std::fs::write(
        r.join("basic.ini"),
        "[Output]\nMode=Simple\n[SimpleOutput]\nVBitrate=4500\nStreamEncoder=x264\n[Video]\nBaseCX=720\nBaseCY=1280\nOutputCX=720\nOutputCY=1280\n",
    )
    .unwrap();
    std::fs::write(
        r.join("service.json"),
        r#"{"type":"rtmp_common","settings":{"service":"Twitch","key":"SECRET-1234"}}"#,
    )
    .unwrap();
    let video = json!({ "fpsNum": 60, "fpsDen": 1, "baseWidth": 1920, "baseHeight": 1080, "outputWidth": 1920,
        "outputHeight": 1080, "outputFormat": 2, "colorspace": 2, "range": 1, "scaleType": 3, "fpsType": 1 });
    let sortie = json!({ "Mode": "Simple", "VBitrate": 4500, "StreamEncoder": "x264", "ABitrate": 160, "Preset": "veryfast",
        "NVENCPreset2": "p4", "FilePath": "D:\\Mes vidéos", "Encoder": "obs_x264", "TrackIndex": 1, "rate_control": "CBR",
        "bitrate": 2500, "keyint_sec": 0, "preset": "veryfast", "preset2": "p4", "RecFilePath": "C:\\Users\\Joueur\\Videos",
        "Track1Bitrate": 160 });
    Faux(
        Arc::new(Mutex::new(Etat {
            ouvert: true,
            api: true,
            enregistre: false,
            video: video.as_object().unwrap().clone(),
            sortie: sortie.as_object().unwrap().clone(),
            demandes: Vec::new(),
            refuse: None,
            limite: None,
            avancees: json!({ "ProcessPriority": "Normal", "Reconnect": true, "DynamicBitrate": false })
                .as_object()
                .unwrap()
                .clone(),
            son: json!({ "SampleRate": "48000" }).as_object().unwrap().clone(),
            sources: json!([{ "id": "game_capture_1", "type": "game_capture" }]),
        })),
        r,
    )
}

fn contexte(nom: &str) -> Contexte {
    let d = dossier_essai(&format!("stream-slapi-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    Contexte::nouveau(d.clone(), d, false, true)
}

fn annonces(e: &Effets) -> Vec<String> {
    e.0.iter().filter_map(|x| if let Effet::Annonce(t) = x { Some(t.clone()) } else { None }).collect()
}

fn bloc(ctx: &Contexte, f: &Faux, envoi_mbps: f64) -> BlocStream {
    let mut b = BlocStream::avec_systeme(Box::new(f.clone()));
    let mut e = Effets::default();
    b.signal(ctx, "debit-mesure", &json!({ "R": [10.0, 100.0, envoi_mbps, 0.0] }), &mut e);
    let _ = b.pages(ctx, "streameur");
    b
}

fn agir(b: &mut BlocStream, ctx: &Contexte, id: &str, v: Value) -> Vec<String> {
    let mut e = Effets::default();
    b.action(ctx, id, &v, &mut e);
    annonces(&e)
}

fn video(f: &Faux) -> (Value, Value, Value, Value) {
    let e = f.0.lock().unwrap();
    (
        e.video["outputWidth"].clone(),
        e.video["outputHeight"].clone(),
        e.video["fpsNum"].clone(),
        e.video["fpsDen"].clone(),
    )
}

fn sortie(f: &Faux, k: &str) -> Value {
    f.0.lock().unwrap().sortie.get(k).cloned().unwrap_or(Value::Null)
}

fn page(b: &mut BlocStream, ctx: &Contexte) -> String {
    b.pages(ctx, "streameur")["streameur"].to_string()
}

#[test]
fn optimiser_regle_la_resolution_et_les_images_par_seconde() {
    let ctx = contexte("regle");
    let f = faux(&ctx);
    let mut b = bloc(&ctx, &f, 20.0);
    // (la page ne demande plus de choisir soi-même la résolution)
    let p = page(&mut b, &ctx);
    assert!(!p.contains("À choisir toi-même"), "{p}");
    let a = agir(&mut b, &ctx, "stream:optimiser:streamlabs", Value::Null);
    assert_eq!(
        a,
        ["Streamlabs est réglé : 1080p à 60 images par seconde, encodé par ta carte NVIDIA, 6\u{202f}000 kb/s."]
    );
    // 1080p60 sur l'image de base de Streamlabs (1920x1080, donnée par son API ; pas le 720x1280 de son basic.ini)
    assert_eq!(video(&f), (json!(1920), json!(1080), json!(60), json!(1)));
    assert_eq!(
        (sortie(&f, "StreamEncoder"), sortie(&f, "VBitrate"), sortie(&f, "ABitrate")),
        (json!("nvenc"), json!(6000), json!(160))
    );
    // (le préréglage du nouvel encodeur, dont le champ n'existe qu'une fois l'encodeur changé)
    assert_eq!(sortie(&f, "NVENCPreset2"), json!("p5"));
    // la clé de stream n'est jamais demandée : seulement la Sortie
    assert!(f.0.lock().unwrap().demandes.iter().all(|d| !d.contains("Stream\"") && !d.contains("getSettingsFormData")));
    let p = page(&mut b, &ctx);
    assert!(p.contains("Qualité : 1080p à 60 images par seconde (c'était déjà le bon réglage)"), "{p}");
    // les réglages en plus : débit qui s'adapte et priorité (la reconnexion et le son étaient déjà bons ; l'image
    // n'est pas réduite : pas de filtre)
    {
        let e = f.0.lock().unwrap();
        assert_eq!(
            (&e.avancees["DynamicBitrate"], &e.avancees["ProcessPriority"]),
            (&json!(true), &json!("AboveNormal"))
        );
        assert_eq!(e.son["SampleRate"], json!("48000"));
        assert_eq!(e.video["scaleType"], json!(3));
    }
    assert!(p.contains("Débit qui baisse tout seul") && p.contains("passe avant le jeu"), "{p}");
    // (dans « Ce qui a changé » : pas les réglages qui étaient déjà bons)
    assert!(
        !p.contains("Reconnexion automatique si")
            && !p.contains("48 kHz")
            && !p.contains("quand elle est réduite (filtre"),
        "{p}"
    );
    assert!(p.contains("Nos conseils") && p.contains("Laisse de la marge") && !p.contains("Capture de tout l'écran"));
    assert!(p.contains("Remettre comme avant"), "{p}");
    assert!(!p.contains('\u{2014}') && !p.contains('\u{2013}'));
}

#[test]
fn optimiser_puis_remettre_comme_avant() {
    let ctx = contexte("remettre");
    let f = faux(&ctx);
    let mut b = bloc(&ctx, &f, 3.0);
    let avant = {
        let e = f.0.lock().unwrap();
        (e.video.clone(), e.sortie.clone())
    };
    let avancees = f.0.lock().unwrap().avancees.clone();
    let a = agir(&mut b, &ctx, "stream:optimiser:streamlabs", Value::Null);
    assert!(a[0].starts_with("Streamlabs est réglé : 480p à 30 images par seconde"), "{a:?}");
    assert_eq!(video(&f), (json!(852), json!(480), json!(30), json!(1)));
    // (l'image est réduite : le filtre net ; et les réglages Avancés)
    assert_eq!(f.0.lock().unwrap().video["scaleType"], json!(4));
    assert_eq!(f.0.lock().unwrap().avancees["DynamicBitrate"], json!(true));
    // l'utilisateur change lui-même son débit vidéo depuis : il est gardé
    f.0.lock().unwrap().sortie.insert("VBitrate".into(), json!(2000));
    // Streamlabs fermé : « Remettre comme avant » attend qu'il soit ouvert, la copie est gardée
    f.0.lock().unwrap().ouvert = false;
    let a = agir(&mut b, &ctx, "stream:remettre:streamlabs", Value::Null);
    assert_eq!(a, ["Ouvre Streamlabs puis reclique sur Remettre comme avant"]);
    assert!(page(&mut b, &ctx).contains("Remettre comme avant"));
    f.0.lock().unwrap().ouvert = true;
    let a = agir(&mut b, &ctx, "stream:remettre:streamlabs", Value::Null);
    assert_eq!(
        a,
        ["Les réglages d'avant sont revenus dans Streamlabs. Ce que tu as changé toi-même depuis est gardé."]
    );
    let e = f.0.lock().unwrap();
    assert_eq!(e.video, avant.0);
    assert_eq!(e.avancees, avancees, "le débit qui s'adapte et la priorité reviennent aussi");
    let mut attendu = avant.1.clone();
    attendu.insert("VBitrate".into(), json!(2000));
    assert_eq!(e.sortie, attendu);
    drop(e);
    let p = page(&mut b, &ctx);
    assert!(!p.contains("stream:remettre:streamlabs"), "{p}");
}

#[test]
fn streamlabs_ferme_ou_occupe_rien_n_est_change() {
    let ctx = contexte("ferme");
    let f = faux(&ctx);
    let mut b = bloc(&ctx, &f, 20.0);
    let basic = std::fs::read(f.1.join("basic.ini")).unwrap();
    f.0.lock().unwrap().ouvert = false;
    let a = agir(&mut b, &ctx, "stream:optimiser:streamlabs", Value::Null);
    assert_eq!(a, ["Ouvre Streamlabs puis reclique sur Optimiser"]);
    assert_eq!(std::fs::read(f.1.join("basic.ini")).unwrap(), basic, "basic.ini n'est plus touché");
    let p = page(&mut b, &ctx);
    assert!(p.contains("Streamlabs est fermé") && p.contains("À ouvrir"), "{p}");
    // ouvert mais en train d'enregistrer
    f.0.lock().unwrap().ouvert = true;
    f.0.lock().unwrap().enregistre = true;
    let a = agir(&mut b, &ctx, "stream:optimiser:streamlabs", Value::Null);
    assert_eq!(a, ["Streamlabs est en train de streamer ou d'enregistrer. Arrête, puis reclique sur Optimiser."]);
    // ouvert, mais son API ne répond pas encore
    f.0.lock().unwrap().enregistre = false;
    f.0.lock().unwrap().api = false;
    let a = agir(&mut b, &ctx, "stream:optimiser:streamlabs", Value::Null);
    assert_eq!(
        a,
        ["Streamlabs ne répond pas encore. Attends qu'il ait fini de s'ouvrir, puis reclique sur Optimiser."]
    );
    let e = f.0.lock().unwrap();
    assert_eq!((e.video["fpsNum"].clone(), e.sortie["VBitrate"].clone()), (json!(60), json!(4500)));
    drop(e);
    assert!(!ctx.donnees.join("streameur").join("streamlabs").join("copie.json").exists());
}

#[test]
fn un_reglage_pas_pris_tout_revient() {
    let ctx = contexte("pas-pris");
    let f = faux(&ctx);
    let mut b = bloc(&ctx, &f, 3.0);
    f.0.lock().unwrap().refuse = Some("Output/VBitrate".into());
    let a = agir(&mut b, &ctx, "stream:optimiser:streamlabs", Value::Null);
    assert_eq!(a, ["Nevermind n'a pas pu changer les réglages de Streamlabs : rien n'a été changé."]);
    // la vidéo, déjà changée, est revenue ; pas de copie
    assert_eq!(video(&f), (json!(1920), json!(1080), json!(60), json!(1)));
    assert!(!ctx.donnees.join("streameur").join("streamlabs").join("copie.json").exists());
}

#[test]
fn kick_passe_en_mode_avance_avec_l_image_cle() {
    let ctx = contexte("kick");
    let f = faux(&ctx);
    let mut b = bloc(&ctx, &f, 20.0);
    let avant = f.0.lock().unwrap().sortie.clone();
    let p = page(&mut b, &ctx);
    assert!(!p.contains("passe en mode Avancé et mets 2"), "{p}");
    agir(&mut b, &ctx, "stream:plateforme:streamlabs", json!("kick"));
    let p = page(&mut b, &ctx);
    assert!(p.contains("Pour pouvoir le régler, Nevermind passe la sortie de Streamlabs en mode Avancé."), "{p}");
    let a = agir(&mut b, &ctx, "stream:optimiser:streamlabs", Value::Null);
    assert!(a[0].starts_with("Streamlabs est réglé"), "{a:?}");
    assert_eq!(sortie(&f, "Mode"), json!("Advanced"));
    assert_eq!(
        (
            sortie(&f, "Encoder"),
            sortie(&f, "bitrate"),
            sortie(&f, "keyint_sec"),
            sortie(&f, "rate_control"),
            sortie(&f, "preset2")
        ),
        (json!("jim_nvenc"), json!(8000), json!(2), json!("CBR"), json!("p5"))
    );
    // les enregistrements restent dans le dossier choisi en mode Simple
    assert_eq!(sortie(&f, "RecFilePath"), json!("D:\\Mes vidéos"));
    let p = page(&mut b, &ctx);
    assert!(
        p.contains("Sortie de Streamlabs en mode Avancé (Kick demande une image clé toutes les 2 secondes)"),
        "{p}"
    );
    let a = agir(&mut b, &ctx, "stream:remettre:streamlabs", Value::Null);
    assert_eq!(a, ["Les réglages d'avant sont revenus dans Streamlabs."]);
    assert_eq!(f.0.lock().unwrap().sortie, avant);
}

/// Une copie faite par Nevermind 2.3.0 (basic.ini de Streamlabs changé) : remise Streamlabs fermé, comme avant
#[test]
fn ancienne_copie_de_fichiers_remise_streamlabs_ferme() {
    let ctx = contexte("ancienne");
    let f = faux(&ctx);
    let basic = f.1.join("basic.ini");
    let d = ctx.donnees.join("streameur").join("streamlabs");
    std::fs::create_dir_all(&d).unwrap();
    let copie = nevermind_stream::sauvegarde::Copie::nouvelle(d);
    let cle = nevermind_stream::sauvegarde::CleIni {
        section: "SimpleOutput".into(),
        cle: "VBitrate".into(),
        avant: Some("4500".into()),
        apres: Some("12000".into()),
    };
    copie
        .garder(
            &[nevermind_stream::sauvegarde::Modif::Ini { chemin: basic.clone(), cles: vec![cle], sections: vec![] }],
            "11/11/2026 à 20:00",
            "",
        )
        .unwrap();
    std::fs::write(&basic, "[Output]\nMode=Simple\n[SimpleOutput]\nVBitrate=12000\nStreamEncoder=nvenc\n").unwrap();
    let mut b = bloc(&ctx, &f, 20.0);
    // ouvert : ses fichiers ne se remettent pas (il les réécrirait)
    let a = agir(&mut b, &ctx, "stream:remettre:streamlabs", Value::Null);
    assert_eq!(a, ["Ferme Streamlabs puis reclique sur Remettre comme avant"]);
    f.0.lock().unwrap().ouvert = false;
    let a = agir(&mut b, &ctx, "stream:remettre:streamlabs", Value::Null);
    assert_eq!(a, ["Les réglages d'avant sont revenus dans Streamlabs."]);
    assert!(std::fs::read_to_string(&basic).unwrap().contains("VBitrate=4500"));
}

/// Streamlabs limite lui-même le débit à celui de sa plateforme (« appliquer les réglages du service ») : le débit qu'il
/// a pris est gardé, dit tel quel, et « Remettre comme avant » le reconnaît comme celui de Nevermind
#[test]
fn le_debit_limite_par_la_plateforme_est_accepte() {
    let ctx = contexte("limite");
    let f = faux(&ctx);
    f.0.lock().unwrap().limite = Some(6000);
    let mut b = bloc(&ctx, &f, 20.0);
    agir(&mut b, &ctx, "stream:plateforme:streamlabs", json!("kick"));
    let a = agir(&mut b, &ctx, "stream:optimiser:streamlabs", Value::Null);
    assert!(a[0].starts_with("Streamlabs est réglé") && a[0].ends_with("6\u{202f}000 kb/s."), "{a:?}");
    assert_eq!(
        (sortie(&f, "Mode"), sortie(&f, "bitrate"), sortie(&f, "keyint_sec")),
        (json!("Advanced"), json!(6000), json!(2))
    );
    let a = agir(&mut b, &ctx, "stream:remettre:streamlabs", Value::Null);
    assert_eq!(a, ["Les réglages d'avant sont revenus dans Streamlabs."]);
    assert_eq!((sortie(&f, "Mode"), sortie(&f, "bitrate")), (json!("Simple"), json!(2500)));
}
