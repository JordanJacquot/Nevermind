//! Le bloc entier sur un faux PC (rien n'est lu ni fait sur le vrai) : ce qu'il lit selon que l'écran est regardé ou
//! non, la partie détectée et ses signaux, la reprise d'un mode-jeu.json, les actions.

use nevermind_noyau::{Bloc, Contexte, Effet, Effets, dossier_essai};
use nevermind_perf::BlocPerf;
use nevermind_perf::systeme::{FauxCompteur, FauxSysteme, Nvidia, Programme};
use serde_json::{Value, json};

fn contexte(nom: &str) -> Contexte {
    let d = dossier_essai(&format!("perf-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Contexte::nouveau(d.join("appli"), d.join("donnees"), false, true)
}

fn pc() -> FauxSysteme {
    FauxSysteme {
        cpu: Some((100, 1000)),
        ram: Some((16e9, 8e9)),
        gpu: Some((vec![0, 0], 0)),
        nv: Some(Nvidia { temperature: Some(60.0), utilisation: Some(10.0), ..Default::default() }),
        perf: Some(100.0),
        disque: Some(3.0),
        progs: vec![Programme { pid: 10, nom: "explorer".into(), ..Default::default() }],
        ..Default::default()
    }
}

fn signaux(e: &Effets) -> Vec<(String, Value)> {
    e.0.iter()
        .filter_map(|x| match x {
            Effet::Signal { nom, donnees } => Some((nom.clone(), donnees.clone())),
            _ => None,
        })
        .collect()
}

/// Avance l'horloge du bloc et fait les passages prévus pendant `secondes`
fn faire(b: &mut BlocPerf, ctx: &Contexte, cache: bool, secondes: f64, e: &mut Effets) {
    let fin = b.horloge.unwrap_or(0.0) + secondes;
    while b.horloge.unwrap_or(0.0) < fin {
        let d = b.tic(ctx, cache, e).map(|d| d.as_secs_f64()).unwrap_or(1.0);
        b.horloge = Some(b.horloge.unwrap_or(0.0) + d);
    }
}

#[test]
fn ecran_regarde_puis_cache() {
    let ctx = contexte("ecran");
    let mut b = BlocPerf::avec(Box::new(pc()), Box::new(FauxCompteur::default()));
    let mut e = Effets::default();
    faire(&mut b, &ctx, false, 60.0, &mut e);
    let vu = b.lectures();
    // Regardé : 5 lectures par seconde (processeur, mémoire, moteurs, NVIDIA, compteurs) + les programmes toutes les 5 s
    assert!((60 * 5..=60 * 5 + 20).contains(&vu), "{vu} lectures en une minute, écran regardé");
    assert!(signaux(&e).iter().any(|(n, _)| n == "mesures"));
    assert_eq!(b.direct().ram_totale, Some(16e9));
    let mut e = Effets::default();
    faire(&mut b, &ctx, true, 60.0, &mut e);
    let cache = b.lectures() - vu;
    // Caché : la température toutes les 10 s, et les programmes toutes les 5 s (la détection d'une partie)
    assert!(cache <= 6 + 12 + 1, "{cache} lectures en une minute, écran caché");
    assert!(!signaux(&e).iter().any(|(n, _)| n == "mesures"), "rien n'est envoyé quand l'écran est caché");
}

#[test]
fn partie_detectee_et_signaux() {
    let ctx = contexte("partie");
    let mut s = pc();
    s.progs.push(Programme { pid: 4242, nom: "RocketLeague".into(), ..Default::default() });
    s.chemins.insert(4242, r"C:\Jeux\RocketLeague\Binaries\Win64\RocketLeague.exe".into());
    let mut b = BlocPerf::avec(Box::new(s), Box::new(FauxCompteur::default()));
    let mut e = Effets::default();
    b.signal(&ctx, "jeux-liste", &json!([{ "nom": "Rocket League", "exes": [r"C:\Jeux\RocketLeague\Binaries\Win64\RocketLeague.exe"], "source": "Epic" }]), &mut e);
    faire(&mut b, &ctx, true, 6.0, &mut e);
    let sg = signaux(&e);
    let debut = sg.iter().find(|(n, _)| n == "partie-commencee").expect("la partie doit être vue");
    assert_eq!(debut.1["jeu"], "Rocket League");
    assert_eq!(debut.1["pid"], 4242);
    assert_eq!(debut.1["exe"], "RocketLeague");
    // La page dit que le mode jeu attend (désactivé ici)
    let p = b.pages(&ctx, "optimisation");
    assert_eq!(p["optimisation"]["onglets"][1]["module"]["titre"], "Mode jeu désactivé");
    assert_eq!(p["optimisation"]["onglets"][2]["blocs"][1]["texte"], "1 jeu dont Nevermind connaît le programme");
}

#[test]
fn reprise_du_mode_jeu() {
    let ctx = contexte("reprise");
    std::fs::create_dir_all(&ctx.donnees).unwrap();
    let f = ctx.donnees.join("mode-jeu.json");
    std::fs::write(
        &f,
        json!({ "Game": "bf6", "Pid": 999, "Proc": "bf6", "Start": "2026-11-05T20:00:00", "Closed": [
            { "Id": "onedrive", "Name": "OneDrive", "Path": r"C:\Users\x\AppData\Local\Microsoft\OneDrive\OneDrive.exe" },
            { "Id": "pirate", "Name": "Pirate", "Path": r"C:\pirate.exe" } ] })
        .to_string(),
    )
    .unwrap();
    let mut b = BlocPerf::avec(Box::new(pc()), Box::new(FauxCompteur::default()));
    let mut e = Effets::default();
    b.tic(&ctx, true, &mut e);
    assert!(!f.exists(), "le fichier est effacé une fois les applis relancées");
    let n = signaux(&e).into_iter().find(|(n, _)| n == "notification").expect("une bulle");
    assert_eq!(n.1["titre"], "Fin du mode jeu");
    assert_eq!(n.1["texte"], "Les applis fermées ont été relancées.");
}

#[test]
fn actions_du_mode_jeu() {
    let ctx = contexte("actions");
    let mut b = BlocPerf::avec(Box::new(pc()), Box::new(FauxCompteur::default()));
    let mut e = Effets::default();
    assert!(b.action(&ctx, "modejeu", &json!(true), &mut e));
    assert_eq!(ctx.reglage("GameMode"), Some(json!(true)));
    assert!(b.action(&ctx, "modejeu-app", &json!({ "valeur": "spotify", "oui": true }), &mut e));
    let sel = ctx.reglage("GameModeApps").unwrap();
    assert!(sel.as_array().unwrap().iter().any(|x| x == "spotify"));
    assert!(sel.as_array().unwrap().iter().any(|x| x == "onedrive"), "les applis cochées par défaut restent");
    assert!(b.action(&ctx, "modejeu-app", &json!({ "valeur": "onedrive", "oui": false }), &mut e));
    assert!(!ctx.reglage("GameModeApps").unwrap().as_array().unwrap().iter().any(|x| x == "onedrive"));
    // (les réglages, le démarrage et le nettoyage sont aux blocs qui ont leurs données)
    assert!(!b.action(&ctx, "tweak", &json!({ "valeur": "0", "oui": true }), &mut e));
    assert!(!b.action(&ctx, "dem:0", &json!(true), &mut e));
}

/// Le lancement d'un jeu ferme les applis du mode jeu par l'Aide, sans jamais proposer la demande de Windows : la
/// partie commence, l'utilisateur n'a rien cliqué (audit G5).
#[test]
fn lancement_d_un_jeu_sans_demande_de_windows() {
    let ctx = contexte("jeu-sans-demande");
    let vues = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let v = vues.clone();
    ctx.fausse_aide(std::sync::Arc::new(move |a: &str, _p: &Value| {
        v.lock().unwrap().push(a.to_string());
        Ok(json!(1))
    }));
    let mut s = pc();
    s.progs.push(Programme { pid: 77, nom: "OneDrive".into(), ..Default::default() });
    s.progs.push(Programme { pid: 4242, nom: "RocketLeague".into(), ..Default::default() });
    s.chemins.insert(4242, r"C:\Jeux\RocketLeague\Binaries\Win64\RocketLeague.exe".into());
    let mut b = BlocPerf::avec(Box::new(s), Box::new(FauxCompteur::default()));
    let mut e = Effets::default();
    assert!(b.action(&ctx, "modejeu", &json!(true), &mut e));
    b.signal(&ctx, "jeux-liste", &json!([{ "nom": "Rocket League", "exes": [r"C:\Jeux\RocketLeague\Binaries\Win64\RocketLeague.exe"], "source": "Epic" }]), &mut e);
    faire(&mut b, &ctx, true, 6.0, &mut e);
    assert!(signaux(&e).iter().any(|(n, _)| n == "partie-commencee"), "la partie doit être vue");
    assert!(vues.lock().unwrap().iter().any(|a| a == "processus.fermer"), "OneDrive est fermé par l'Aide");
    assert_eq!(ctx.aide_demandes_relevees(), Vec::<String>::new(), "aucune demande de Windows au lancement d'un jeu");
}

/// La carte « Mode streameur » (signal « streameur » du bloc stream) : sous « Pour streamer », grisée sans logiciel ;
/// sans le signal, la page est celle d'avant
#[test]
fn carte_du_mode_streameur() {
    let ctx = contexte("streameur");
    let mut b = BlocPerf::avec(Box::new(pc()), Box::new(FauxCompteur::default()));
    let blocs = |b: &mut BlocPerf| b.pages(&ctx, "perf")["perf"]["blocs"].as_array().cloned().unwrap_or_default();
    assert!(!blocs(&mut b).iter().any(|x| x["texte"] == "Pour streamer"));
    let mut e = Effets::default();
    let carte = json!({ "id": "streameur", "nom": "Mode streameur", "note": "Pour utiliser le mode streameur, il faut avoir installé OBS ou Streamlabs.", "inactif": true });
    b.signal(&ctx, "streameur", &json!({ "disponible": false, "regle": false, "carte": carte }), &mut e);
    let l = blocs(&mut b);
    let k = l.iter().position(|x| x["texte"] == "Pour streamer").expect("titre « Pour streamer »");
    assert_eq!(l[k + 1]["type"], "outils");
    assert_eq!(l[k + 1]["elements"][0], carte);
}

/// M5 (audit du 08/11) : la carte graphique puissante d'un jeu (HKCU) est dans la liste fermée du noyau, sinon « Tout
/// restaurer » et le désinstalleur refuseraient de la remettre comme avant
#[test]
fn le_profil_gpu_ecrit_dans_la_liste_fermee() {
    for on in [true, false] {
        let l = nevermind_perf::changements_profil(&json!({ "exes": [r"C:\Jeux\Jeu\jeu.exe"] }), "gpu", on);
        assert_eq!(l.len(), 1);
        let (p, n) = (l[0]["Path"].as_str().unwrap(), l[0]["Name"].as_str().unwrap());
        assert!(nevermind_noyau::registre::utilisateur(p));
        assert!(nevermind_noyau::registre::utilisateur_permis(p, n), "{p}|{n} hors de la liste");
    }
}

/// Petit de l'audit du 08/11 : Mindset arrête l'appli 6 s après lui avoir demandé de quitter (contrat, partie 5).
/// Pendant une partie mesurée, la fermeture du bloc attendait l'arrêt des FPS par l'Aide (jusqu'à 8 s), puis le
/// bilan (jusqu'à 10 s), et seulement ensuite relançait les applis fermées pour la partie : Mindset tuait Nevermind
/// avant. Maintenant les applis sont relancées d'abord, et l'arrêt des FPS a un temps compté.
#[test]
fn la_fermeture_pendant_une_partie_mesuree_tient_dans_le_delai_de_mindset() {
    use std::time::{Duration, Instant};
    let ctx = contexte("fermeture").partage();
    ctx.fausse_aide(std::sync::Arc::new(|a: &str, _p: &Value| {
        if a == "fps.arreter" {
            // (une Aide lente : PresentMon met longtemps à s'arrêter)
            std::thread::sleep(Duration::from_secs(8));
        }
        Ok(if a == "fps.demarrer" { json!("nouvelle") } else { json!(1) })
    }));
    let _ = ctx.ecrire_reglage("FpsMeasure", json!(true), false);
    let mut s = pc();
    s.progs.push(Programme { pid: 77, nom: "OneDrive".into(), ..Default::default() });
    s.chemins.insert(77, r"C:\Users\x\AppData\Local\Microsoft\OneDrive\OneDrive.exe".into());
    s.progs.push(Programme { pid: 4242, nom: "RocketLeague".into(), ..Default::default() });
    s.chemins.insert(4242, r"C:\Jeux\RocketLeague\Binaries\Win64\RocketLeague.exe".into());
    s.fichiers.push(ctx.appli.join("outils-tiers").join("PresentMon.exe").to_string_lossy().to_string());
    let mut b = BlocPerf::avec(Box::new(s), Box::new(FauxCompteur::default()));
    let mut e = Effets::default();
    assert!(b.action(&ctx, "modejeu", &json!(true), &mut e));
    b.signal(&ctx, "jeux-liste", &json!([{ "nom": "Rocket League", "exes": [r"C:\Jeux\RocketLeague\Binaries\Win64\RocketLeague.exe"], "source": "Epic" }]), &mut e);
    faire(&mut b, &ctx, true, 6.0, &mut e);
    assert!(signaux(&e).iter().any(|(n, _)| n == "partie-commencee"), "la partie doit être vue");
    let j = std::fs::read_to_string(ctx.donnees.join("journal.txt")).unwrap_or_default();
    assert!(j.contains("Mesure des FPS: Rocket League"), "la mesure doit avoir commencé : {j}");
    let mode_jeu = ctx.donnees.join("mode-jeu.json");
    // (lot I : les applis sont fermées dans un travail de fond, qui écrit mode-jeu.json lui-même ; sur une machine
    //  chargée, il peut finir après les tics simulés : attendu 10 s au plus)
    let attente = Instant::now();
    while !mode_jeu.exists() && attente.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(mode_jeu.exists(), "les applis fermées pour la partie sont notées");
    // (un tic de plus : le bloc reçoit le résultat du travail de fond, comme en vrai avant une fermeture)
    faire(&mut b, &ctx, true, 1.0, &mut e);
    let debut = Instant::now();
    b.fermer(&ctx);
    let duree = debut.elapsed();
    assert!(!mode_jeu.exists(), "la partie n'est pas finie : les applis fermées ne sont pas relancées");
    assert!(duree < Duration::from_secs(3), "la fermeture du bloc a pris {duree:?} (Mindset arrête l'appli à 6 s)");
}

/// Lot I, point 3 : au début d'une partie, le mode jeu ferme les applis une par une par l'Aide (chaque demande lit le
/// contexte de l'Aide par WMI). Avant, c'était dans le tic, sur le fil des blocs : Nevermind figé pendant que le jeu
/// charge. Maintenant c'est un travail de fond, toujours sous le verrou des gestes (prioritaire : jamais refusé), avec
/// son attente affichée ; la fin donne le même journal, le même fichier et la même bulle qu'avant.
#[test]
fn le_mode_jeu_ferme_les_applis_en_fond() {
    use std::time::{Duration, Instant};
    let ctx = contexte("mode-jeu-fond").partage();
    let (feu, attente) = std::sync::mpsc::channel::<()>();
    let attente = std::sync::Mutex::new(Some(attente));
    ctx.fausse_aide(std::sync::Arc::new(move |a: &str, _p: &Value| {
        if a == "processus.fermer" {
            // (l'Aide lente : elle lit son contexte par WMI pour chaque appli ; ici, le feu de l'essai)
            if let Some(r) = attente.lock().unwrap().take() {
                let _ = r.recv_timeout(Duration::from_secs(20));
            }
        }
        Ok(json!(1))
    }));
    let mut s = pc();
    s.progs.push(Programme { pid: 77, nom: "OneDrive".into(), ..Default::default() });
    s.chemins.insert(77, r"C:\Users\x\AppData\Local\Microsoft\OneDrive\OneDrive.exe".into());
    s.progs.push(Programme { pid: 4242, nom: "RocketLeague".into(), ..Default::default() });
    s.chemins.insert(4242, r"C:\Jeux\RocketLeague\Binaries\Win64\RocketLeague.exe".into());
    let mut b = BlocPerf::avec(Box::new(s), Box::new(FauxCompteur::default()));
    let mut e = Effets::default();
    assert!(b.action(&ctx, "modejeu", &json!(true), &mut e));
    b.signal(&ctx, "jeux-liste", &json!([{ "nom": "Rocket League", "exes": [r"C:\Jeux\RocketLeague\Binaries\Win64\RocketLeague.exe"], "source": "Epic" }]), &mut e);
    // Les tics jusqu'au début de la partie : aucun n'attend l'Aide
    let mut tout = Vec::new();
    let pas = |b: &mut BlocPerf, tout: &mut Vec<Effet>| {
        let mut e = Effets::default();
        let d = Instant::now();
        b.tic(&ctx, true, &mut e);
        b.travaux_finis(&ctx, &mut e);
        assert!(d.elapsed() < Duration::from_secs(1), "un tic a pris {:?} : le mode jeu attendait l'Aide", d.elapsed());
        b.horloge = Some(b.horloge.unwrap_or(0.0) + 0.5);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(20));
    };
    let fin = Instant::now() + Duration::from_secs(10);
    while !tout.iter().any(|x| matches!(x, Effet::Signal { nom, .. } if nom == "partie-commencee")) {
        assert!(Instant::now() < fin, "la partie n'est pas vue");
        pas(&mut b, &mut tout);
    }
    // Pendant la fermeture : l'attente est montrée, le verrou des gestes est tenu (un clic est refusé)
    let fin = Instant::now() + Duration::from_secs(10);
    let attente_montree = |tout: &[Effet]| {
        tout.iter().any(|x| matches!(x, Effet::Signal { nom, donnees } if nom == "occupe" && donnees["texte"] == "Fermeture des applis pour ta partie..."))
    };
    while !attente_montree(&tout) {
        assert!(Instant::now() < fin, "l'attente du mode jeu n'est pas montrée : {tout:?}");
        pas(&mut b, &mut tout);
    }
    assert_eq!(
        ctx.prendre_geste("Essai", "Essai").unwrap_err(),
        "Attends la fin de « Début de partie » (Mode jeu)",
        "le mode jeu ne tient pas le verrou des gestes"
    );
    feu.send(()).unwrap();
    let journal = || std::fs::read_to_string(ctx.donnees.join("journal.txt")).unwrap_or_default();
    let fin = Instant::now() + Duration::from_secs(10);
    while !journal().contains("Mode jeu: Rocket League lancé, applis fermées: OneDrive") {
        assert!(Instant::now() < fin, "la fermeture ne finit pas : {}", journal());
        pas(&mut b, &mut tout);
    }
    for _ in 0..3 {
        pas(&mut b, &mut tout);
    }
    assert!(ctx.donnees.join("mode-jeu.json").exists(), "les applis fermées ne sont pas notées");
    assert!(
        signaux(&Effets(tout.clone())).iter().any(|(n, d)| n == "notification" && d["titre"] == "Mode jeu activé"),
        "pas de bulle du mode jeu"
    );
    let derniere = tout.iter().rev().find_map(|x| match x {
        Effet::Signal { nom, donnees } if nom == "occupe" => Some(donnees["texte"].clone()),
        _ => None,
    });
    assert_eq!(derniere, Some(json!("")), "l'attente reste à l'écran");
    assert!(ctx.prendre_geste("Essai", "Essai").is_ok(), "le verrou des gestes n'est pas rendu");
    assert_eq!(ctx.aide_demandes_relevees(), Vec::<String>::new(), "aucune demande de Windows au lancement d'un jeu");
}
