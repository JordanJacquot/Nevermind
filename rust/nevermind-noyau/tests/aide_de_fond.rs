//! Lot B du second audit : les demandes de fond à l'Aide (`aide_de_fond`, faites depuis les tics) ne réveillent jamais
//! l'Aide sur le fil qui les fait, et renoncent tout de suite pendant une pause après une demande restée sans réponse.
//! Une Aide qui répond mais refuse n'est pas un échec. Fausse Aide sur le canal de la copie d'essai (jamais la vraie),
//! tâche simulée (jamais schtasks).

use nevermind_noyau::windows::canal::{FauxServeur, Repondre};
use nevermind_noyau::{Contexte, aide, dossier_essai, fausse_aide};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn contexte(nom: &str) -> (Contexte, std::path::PathBuf) {
    let d = dossier_essai(&format!("aide-de-fond-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let ctx = Contexte::nouveau(d.join("appli"), d.clone(), false, true);
    let moi = std::env::current_exe().unwrap().to_string_lossy().into_owned();
    ctx.regler_aide_essai(Some(moi), Some((Duration::from_secs(3), Duration::from_secs(1))));
    (ctx, d)
}

#[test]
fn un_refus_n_est_pas_un_echec_une_absence_de_reponse_si() {
    let (ctx, d) = contexte("refus");
    let ctx = ctx.partage();
    let serveur = fausse_aide::lancer(&ctx, |a, _| {
        Some(if a == "trafic.tcp" { fausse_aide::refus("Pas maintenant.") } else { fausse_aide::ok(json!("ok")) })
    })
    .unwrap();
    // Un refus : dit tel quel, et la demande suivante part (pas de pause)
    assert_eq!(ctx.aide_de_fond("trafic.tcp", &json!({}), 2000), Err("Pas maintenant.".into()));
    assert_eq!(ctx.aide_de_fond("flux.lire", &json!({}), 2000), Ok(json!("ok")));
    assert_eq!(serveur.demandes().len(), 2);
    // L'Aide s'arrête : la demande échoue sans attendre la tâche, puis les suivantes renoncent tout de suite
    let reveils = Arc::new(AtomicUsize::new(0));
    let r = reveils.clone();
    ctx.tache_aide_essai(Arc::new(move || {
        r.fetch_add(1, Ordering::SeqCst);
        true
    }))
    .unwrap();
    drop(serveur);
    let t = Instant::now();
    assert!(ctx.aide_de_fond("flux.lire", &json!({}), 2000).is_err());
    assert_eq!(ctx.aide_de_fond("flux.lire", &json!({}), 2000), Err(aide::ERREUR_PAUSE.into()));
    assert!(t.elapsed() < Duration::from_secs(1), "la demande de fond a attendu le réveil ({:?})", t.elapsed());
    // (le réveil part dans un fil à part : la tâche est lancée une fois)
    let fin = Instant::now() + Duration::from_secs(5);
    while reveils.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < fin, "le réveil n'est pas lancé");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(ctx.aide_demandes_relevees(), Vec::<String>::new());
    let _ = std::fs::remove_dir_all(&d);
}

/// Le réveil réussi lève la pause : l'Aide relancée par sa tâche répond de nouveau aux demandes de fond
#[test]
fn le_reveil_reussi_leve_la_pause() {
    let (ctx, d) = contexte("reveil");
    let ctx = ctx.partage();
    let canal = ctx.aide_config().canal.clone();
    let recues = Arc::new(Mutex::new(Vec::<String>::new()));
    let r = recues.clone();
    let repondre: Repondre = Arc::new(move |l: &str| {
        let v: Value = serde_json::from_str(l).unwrap_or(Value::Null);
        r.lock().unwrap().push(v["a"].as_str().unwrap_or("").to_string());
        Some(fausse_aide::ok(json!([])).to_string())
    });
    let relancee = Arc::new(Mutex::new(None::<FauxServeur>));
    let (rl, c, rep) = (relancee.clone(), canal.clone(), repondre.clone());
    ctx.tache_aide_essai(Arc::new(move || {
        *rl.lock().unwrap() = Some(FauxServeur::lancer(&c, rep.clone()).unwrap());
        true
    }))
    .unwrap();
    assert!(ctx.aide_de_fond("flux.lire", &json!({}), 2000).is_err(), "l'Aide n'écoute pas encore");
    let fin = Instant::now() + Duration::from_secs(10);
    while ctx.aide_de_fond("flux.lire", &json!({}), 2000).is_err() {
        assert!(Instant::now() < fin, "la pause n'est pas levée après le réveil");
        std::thread::sleep(Duration::from_millis(50));
    }
    // (lot G : la sonde rapide par le canal, toutes les 5 s pendant la pause, peut s'ajouter à celle du réveil)
    let r = recues.lock().unwrap().clone();
    assert!(
        r.len() >= 2
            && r.last().is_some_and(|x| x == "flux.lire")
            && r[..r.len() - 1].iter().all(|x| x == "aide.bonjour"),
        "{r:?}"
    );
    drop(relancee.lock().unwrap().take());
    let _ = std::fs::remove_dir_all(&d);
}

/// Un réveil demandé pendant qu'un autre est en cours (et va échouer) n'est plus perdu : il est refait à la fin du
/// premier. Avant, la demande était jetée et la pause de 60 s notée : rien ne relançait l'Aide pendant une minute (la
/// mesure des FPS d'une partie qui commence ne reprenait pas).
#[test]
fn un_reveil_demande_pendant_un_autre_n_est_pas_perdu() {
    let (ctx, d) = contexte("redemande");
    let ctx = ctx.partage();
    // (sans droits, comme Nevermind : en administrateur `mesures_aide` ne réveille jamais l'Aide)
    ctx.regler_admin_essai(false).unwrap();
    let canal = ctx.aide_config().canal.clone();
    let repondre: Repondre = Arc::new(|_: &str| Some(fausse_aide::ok(json!([])).to_string()));
    let relancee = Arc::new(Mutex::new(None::<FauxServeur>));
    let reveils = Arc::new(AtomicUsize::new(0));
    let (rl, rv, c, rep) = (relancee.clone(), reveils.clone(), canal.clone(), repondre.clone());
    // (la tâche : son premier lancement ne donne rien ; le suivant relance l'Aide)
    ctx.tache_aide_essai(Arc::new(move || {
        if rv.fetch_add(1, Ordering::SeqCst) >= 1 {
            *rl.lock().unwrap() = Some(FauxServeur::lancer(&c, rep.clone()).unwrap());
        }
        true
    }))
    .unwrap();
    // Un premier réveil, qui va échouer (3 s d'attente de l'Aide)
    assert!(ctx.aide_de_fond("flux.lire", &json!({}), 2000).is_err());
    let fin = Instant::now() + Duration::from_secs(5);
    while reveils.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < fin, "le premier réveil n'est pas lancé");
        std::thread::sleep(Duration::from_millis(20));
    }
    // Pendant ce temps, une partie commence : la mesure demande un réveil
    assert!(!ctx.mesures_aide(), "l'Aide n'écoute pas encore");
    let fin = Instant::now() + Duration::from_secs(15);
    while !ctx.aide_joignable(1) || !ctx.mesures_aide() {
        assert!(
            Instant::now() < fin,
            "la demande faite pendant le premier réveil est perdue ({} réveil(s))",
            reveils.load(Ordering::SeqCst)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(reveils.load(Ordering::SeqCst), 2);
    drop(relancee.lock().unwrap().take());
    let _ = std::fs::remove_dir_all(&d);
}

/// Lot F : une Aide qui accepte la connexion mais ne répond jamais. Avant, après chaque pause (60 s), la demande de
/// fond suivante l'attendait de nouveau en entier sur le fil des blocs (15 s par minute), et les demandes texte
/// (`fps.lire`, la veille des FPS) ou la mesure (`mesures_aide`, vraie puisque l'Aide est joignable) l'attendaient à
/// chaque fois. Maintenant elle n'est jamais attendue en entier : ensuite tout renonce tout de suite, la pause finie
/// elle est sondée dans un fil à part, et la sonde réussie lève la pause quand l'Aide répond de nouveau.
///
/// Lot I : même la PREMIÈRE demande d'un épisode ne l'attend plus en entier (avant : son délai entier, 15 à 20 s, une
/// fois par épisode, souvent au début d'une partie). Elle passe d'abord par la sonde, avec un délai court ; une Aide
/// muette est donc reconnue en moins de 2 s, et la vraie demande ne lui est jamais envoyée.
#[test]
fn une_aide_muette_n_est_attendue_qu_une_fois() {
    use std::sync::atomic::AtomicBool;
    let (ctx, d) = contexte("muette");
    let ctx = ctx.partage();
    // (sans droits, comme Nevermind ; une pause courte pour l'essai, 60 s en vrai)
    ctx.regler_admin_essai(false).unwrap();
    ctx.regler_pause_essai(Duration::from_millis(500)).unwrap();
    let parle = Arc::new(AtomicBool::new(false));
    let recues = Arc::new(Mutex::new(Vec::<String>::new()));
    let (p, r) = (parle.clone(), recues.clone());
    let _serveur = fausse_aide::lancer(&ctx, move |a, _| {
        r.lock().unwrap().push(a.to_string());
        p.load(Ordering::SeqCst).then(|| fausse_aide::ok(json!([])))
    })
    .unwrap();
    // La première demande n'attend plus son délai (15 s, comme `processus.fermer` ou `fps.demarrer`) : la sonde, au
    // délai court, voit que l'Aide est muette (lot I)
    let t = Instant::now();
    assert!(ctx.aide_de_fond("flux.lire", &json!({}), 15000).is_err());
    assert!(
        t.elapsed() < Duration::from_millis(2500),
        "la première demande a attendu l'Aide muette ({:?})",
        t.elapsed()
    );
    // Puis 4 s de demandes, comme les tics (bien plus que la pause) : aucune n'attend l'Aide
    let fin = Instant::now() + Duration::from_secs(4);
    while Instant::now() < fin {
        let t = Instant::now();
        assert!(ctx.aide_de_fond("flux.lire", &json!({}), 1500).is_err());
        assert!(ctx.aide_texte("fps.lire", "{}", 1500).is_err());
        assert!(!ctx.mesures_aide(), "une Aide muette prise pour prête à mesurer");
        assert!(!ctx.aide_prete());
        assert!(t.elapsed() < Duration::from_millis(400), "une demande a attendu l'Aide muette ({:?})", t.elapsed());
        std::thread::sleep(Duration::from_millis(50));
    }
    let n = |a: &str| recues.lock().unwrap().iter().filter(|x| *x == a).count();
    assert_eq!(n("flux.lire"), 0, "la vraie demande est partie vers l'Aide muette : {:?}", recues.lock().unwrap());
    assert_eq!(n("fps.lire"), 0);
    assert!(n("aide.bonjour") >= 1, "la sonde n'est pas partie : {:?}", recues.lock().unwrap());
    // L'Aide répond de nouveau : la sonde lève la pause, les demandes et la mesure reprennent
    parle.store(true, Ordering::SeqCst);
    let fin = Instant::now() + Duration::from_secs(30);
    while ctx.aide_de_fond("flux.lire", &json!({}), 1500).is_err() {
        assert!(Instant::now() < fin, "la pause n'est pas levée : {:?}", recues.lock().unwrap());
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(ctx.mesures_aide());
    assert_eq!(ctx.aide_texte("fps.lire", "{}", 1500), Ok(String::new()));
    // Un deuxième épisode : l'Aide a répondu, puis se tait de nouveau. La première demande texte (le départ d'une
    // mesure, 15 s de délai) ne l'attend pas non plus en entier (lot I)
    parle.store(false, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(1500));
    let t = Instant::now();
    assert!(ctx.aide_texte("fps.demarrer", "{\"Pid\":1}", 15000).is_err());
    assert!(t.elapsed() < Duration::from_millis(2500), "le départ a attendu l'Aide muette ({:?})", t.elapsed());
    assert_eq!(n("fps.demarrer"), 0, "le départ est parti vers l'Aide muette : {:?}", recues.lock().unwrap());
    assert_eq!(ctx.aide_demandes_relevees(), Vec::<String>::new());
    let _ = std::fs::remove_dir_all(&d);
}

/// Lot I (note de l'avis du lot G) : un départ de mesure (`fps.demarrer`) demandé pendant qu'un arrêt est en cours
/// d'envoi (`fps.arreter`, jusqu'à 8 s quand PresentMon tarde) n'attend plus cet envoi sur le fil qui l'appelle : il
/// renonce tout de suite (« nouvel essai dans un moment »), et l'ordre reste juste, l'arrêt arrive toujours avant le
/// départ. Une fois l'arrêt reçu, le départ passe.
#[test]
fn un_depart_n_attend_pas_un_arret_en_cours_d_envoi() {
    let (ctx, d) = contexte("depart-pendant-arret");
    let ctx = ctx.partage();
    ctx.regler_admin_essai(false).unwrap();
    let _serveur = fausse_aide::lancer(&ctx, |a, _| {
        if a == "fps.arreter" {
            // (PresentMon met 3 s à s'arrêter)
            std::thread::sleep(Duration::from_secs(3));
        }
        Some(fausse_aide::ok(json!("")))
    })
    .unwrap();
    let canal_recues = _serveur.recues.clone();
    let recues = move || -> Vec<String> {
        canal_recues
            .lock()
            .unwrap()
            .iter()
            .map(|l| serde_json::from_str::<Value>(l).unwrap_or(Value::Null)["a"].as_str().unwrap_or("").to_string())
            .collect()
    };
    // L'arrêt part dans son fil ; on attend qu'il soit chez l'Aide
    ctx.arreter_mesure(aide::Mesure::Fps);
    let fin = Instant::now() + Duration::from_secs(5);
    while !recues().iter().any(|a| a == "fps.arreter") {
        assert!(Instant::now() < fin, "l'arrêt n'est pas envoyé : {:?}", recues());
        std::thread::sleep(Duration::from_millis(10));
    }
    // Le départ, pendant que l'Aide traite l'arrêt : il ne l'attend pas
    let t = Instant::now();
    let r = ctx.aide_texte("fps.demarrer", "{\"Pid\":1}", 15000);
    assert!(t.elapsed() < Duration::from_secs(1), "le départ a attendu l'envoi de l'arrêt ({:?})", t.elapsed());
    assert_eq!(r, Err(aide::ERREUR_PAUSE.into()));
    // L'arrêt reçu, le départ passe, et il arrive après l'arrêt
    let fin = Instant::now() + Duration::from_secs(10);
    while ctx.aide_texte("fps.demarrer", "{\"Pid\":1}", 15000).is_err() {
        assert!(Instant::now() < fin, "le départ ne passe plus : {:?}", recues());
        std::thread::sleep(Duration::from_millis(100));
    }
    let l = recues();
    let i_arret = l.iter().position(|a| a == "fps.arreter").unwrap();
    let i_depart = l.iter().position(|a| a == "fps.demarrer").unwrap();
    assert!(i_arret < i_depart, "le départ est arrivé avant l'arrêt : {l:?}");
    let _ = std::fs::remove_dir_all(&d);
}
