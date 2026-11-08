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
