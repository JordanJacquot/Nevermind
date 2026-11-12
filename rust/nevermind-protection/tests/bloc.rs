//! Le bloc avec un faux système (rien n'est lu ni touché sur le PC) : la vérification de fond, les questions avant
//! chaque geste, « Désactiver ces tâches » (outils\tester-securite.ps1, partie 1), les menaces après une analyse
//! (partie 2 : jamais « Aucune menace » sans résultat lu), et « C'est moi » / « Retirer ».

use nevermind_noyau::{Bloc, Contexte, Effet, Effets, dossier_essai};
use nevermind_protection::BlocProtection;
use nevermind_protection::etat::{EtatS, Tache};
use nevermind_protection::ps::Date;
use nevermind_protection::regles::{self, DetectionLue};
use nevermind_protection::systeme::{Faux, Lecture};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn contexte(nom: &str) -> Contexte {
    let d = dossier_essai(&format!("protection-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Contexte::nouveau(d.clone(), d, false, true)
}

/// Les tics jusqu'à la fin du travail de fond ; tous les effets reçus
fn attendre(b: &mut BlocProtection, ctx: &Contexte) -> Vec<Effet> {
    let mut tout = Vec::new();
    let debut = Instant::now();
    loop {
        let mut e = Effets::default();
        b.tic(ctx, false, &mut e);
        tout.extend(e.0);
        if !b.occupe() || debut.elapsed() > Duration::from_secs(10) {
            return tout;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn faux_avec(s: EtatS) -> Faux {
    let f = Faux::default();
    *f.lecture.lock().unwrap() = Lecture { s, env: Default::default() };
    f
}

fn defender() -> Value {
    json!({ "AMRunningMode": "Normal", "RealTimeProtectionEnabled": true, "AntivirusSignatureAge": 0, "QuickScanAge": 1,
        "QuickScanEndTime": "2026-11-05T10:00:00" })
}

fn tache(nom: &str, chemin: &str) -> Tache {
    Tache {
        nom: nom.into(),
        chemin: chemin.into(),
        cmd: format!("C:\\Users\\x\\AppData\\Local\\Temp\\{nom}.exe"),
        grave: false,
    }
}

fn annonces(e: &[Effet]) -> Vec<String> {
    e.iter().filter_map(|x| if let Effet::Annonce(t) = x { Some(t.clone()) } else { None }).collect()
}

fn signal<'a>(e: &'a [Effet], nom: &str) -> Option<&'a Value> {
    e.iter().find_map(|x| match x {
        Effet::Signal { nom: n, donnees } if n == nom => Some(donnees),
        _ => None,
    })
}

#[test]
fn verification_de_fond_puis_pages() {
    let ctx = contexte("fond");
    let s = EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap();
    let f = Arc::new(faux_avec(s));
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    let p = b.pages(&ctx, "protection");
    assert_eq!(p["protection"]["module"]["titre"], "Vérification en cours");
    let e = attendre(&mut b, &ctx);
    assert_eq!(b.note(), Some(100));
    assert_eq!(b.pages(&ctx, "protection")["_accueil"]["protection"]["score"], 100);
    assert!(e.contains(&Effet::Rafraichir));
    let p = b.pages(&ctx, "protection");
    assert_eq!(p["protection"]["module"]["titre"], "Bien protégé");
    assert_eq!(
        p["securite"]["onglets"][1]["blocs"].as_array().unwrap().last().unwrap()["texte"],
        "Les analyses utilisent Microsoft Defender, l'antivirus intégré à Windows."
    );
    // (une seule lecture : rien ne tourne en continu)
    let mut e = Effets::default();
    let delai = b.tic(&ctx, true, &mut e);
    assert_eq!(delai, Some(Duration::from_secs(60)));
    assert_eq!(f.gestes(), vec!["lire".to_string()]);
}

fn bloc_avec_taches(nom: &str, refus: &[&str], muettes: &[&str]) -> (BlocProtection, Arc<Faux>, Contexte) {
    let ctx = contexte(nom);
    let mut s = EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap();
    s.taches = vec![tache("Mise a jour", "\\"), tache("Lanceur cache", "\\Outils\\")];
    let mut f = faux_avec(s);
    f.refus = refus.iter().map(|x| x.to_string()).collect();
    f.muettes = muettes.iter().map(|x| x.to_string()).collect();
    let f = Arc::new(f);
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    attendre(&mut b, &ctx);
    let p = b.pages(&ctx, "securite");
    assert_eq!(p["securite"]["onglets"][0]["nom"], "Points à vérifier (1)");
    (b, f, ctx)
}

fn desactiver(b: &mut BlocProtection, ctx: &Contexte) -> Vec<Effet> {
    // (sans confirmation : la question d'abord, rien n'est fait)
    let mut e = Effets::default();
    assert!(b.action(ctx, "sec:0:0", &Value::Null, &mut e));
    let q = signal(&e.0, "question").expect("question posée");
    assert_eq!(
        q["texte"],
        "Désactiver ces tâches planifiées ? Tu pourras les réactiver dans le Planificateur de tâches."
    );
    let mut e = Effets::default();
    assert!(b.action(ctx, "sec:0:0", &json!({ "id": q["id"], "valeur": "oui" }), &mut e));
    e.0
}

#[test]
fn taches_toutes_desactivees() {
    let (mut b, f, ctx) = bloc_avec_taches("taches-ok", &[], &[]);
    let e = desactiver(&mut b, &ctx);
    assert!(annonces(&e).is_empty(), "{:?}", annonces(&e));
    assert!(f.gestes().contains(&"désactiver \\Mise a jour".to_string()));
}

#[test]
fn taches_refusees_jamais_comptees_comme_faites() {
    let (mut b, _f, ctx) = bloc_avec_taches("taches-refus", &["Mise a jour", "Lanceur cache"], &[]);
    let a = annonces(&desactiver(&mut b, &ctx));
    assert!(
        a.len() == 1
            && a[0].starts_with(
                "Oups, quelque chose s'est mal passé :

Je n'ai pas pu désactiver ces tâches : Mise a jour, Lanceur cache"
            )
            && a[0].contains("administrateur"),
        "{a:?}"
    );
    let j = std::fs::read_to_string(ctx.donnees.join("journal.txt")).unwrap_or_default();
    assert!(j.contains("Lanceur cache") && j.contains("non désactivée"), "{j}");
}

#[test]
fn tache_restee_active_sans_erreur() {
    let (mut b, _f, ctx) = bloc_avec_taches("taches-muette", &[], &["Mise a jour"]);
    let a = annonces(&desactiver(&mut b, &ctx));
    assert!(
        a.iter().any(|x| x.starts_with("1 tâche désactivée.")
            && x.contains("Je n'ai pas pu désactiver cette tâche : Mise a jour")),
        "{a:?}"
    );
}

#[test]
fn une_partie_seulement() {
    let (mut b, _f, ctx) = bloc_avec_taches("taches-partie", &["Lanceur cache"], &[]);
    let a = annonces(&desactiver(&mut b, &ctx));
    assert!(
        a.iter().any(|x| x.starts_with("1 tâche désactivée.") && x.contains("cette tâche : Lanceur cache")),
        "{a:?}"
    );
}

#[test]
fn c_est_moi_puis_retirer() {
    let (mut b, _f, ctx) = bloc_avec_taches("approuver", &[], &[]);
    let mut e = Effets::default();
    b.action(&ctx, "sec-moi:0", &Value::Null, &mut e);
    let q = signal(&e.0, "question").unwrap().clone();
    assert!(q["texte"].as_str().unwrap().starts_with("Marquer ces 2 éléments comme sûrs ?"));
    let mut e = Effets::default();
    b.action(&ctx, "sec-moi:0", &json!({ "id": q["id"], "valeur": "oui" }), &mut e);
    let p = b.pages(&ctx, "securite");
    assert_eq!(p["securite"]["onglets"][0]["nom"], "Points à vérifier (0)");
    assert_eq!(p["securite"]["onglets"][3]["nom"], "Approuvés par toi (2)");
    let reglages: Value =
        serde_json::from_str(&std::fs::read_to_string(ctx.donnees.join("parametres.json")).unwrap()).unwrap();
    assert_eq!(
        reglages["SecuriteApprouves"][0]["Key"],
        "tache|\\Mise a jour|C:\\Users\\x\\AppData\\Local\\Temp\\Mise a jour.exe"
    );
    let mut e = Effets::default();
    b.action(&ctx, "sec-retirer:0", &Value::Null, &mut e);
    let p = b.pages(&ctx, "securite");
    assert_eq!(p["securite"]["onglets"][0]["nom"], "Points à vérifier (1)");
    assert_eq!(p["securite"]["onglets"][3]["nom"], "Approuvés par toi (1)");
}

#[test]
fn analyse_sans_resultat_lu_jamais_aucune_menace() {
    let ctx = contexte("analyse");
    let f = Arc::new(faux_avec(EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap()));
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    attendre(&mut b, &ctx);
    let mut e = Effets::default();
    b.action(&ctx, "clic:BtnScanQuick", &Value::Null, &mut e);
    assert_eq!(b.panneau_donnees().unwrap()["etat"], "en-cours");
    let e = attendre(&mut b, &ctx);
    let fin = b.panneau_donnees().cloned().expect("fin de l'analyse");
    // (sans droits, sans Aide, une liste vide ne prouve rien)
    assert_eq!(fin["etat"], "inconnu");
    assert!(!annonces(&e).iter().any(|a| a.contains("Aucune menace")));
    assert!(f.gestes().iter().any(|g| g.starts_with("analyser Rapide")));
}

#[test]
fn analyse_en_administrateur_rien_trouve() {
    let ctx = contexte("analyse-admin");
    let mut f = faux_avec(EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap());
    f.admin = true;
    let f = Arc::new(f);
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    attendre(&mut b, &ctx);
    let mut e = Effets::default();
    b.action(&ctx, "clic:BtnScanQuick", &Value::Null, &mut e);
    attendre(&mut b, &ctx);
    let fin = b.panneau_donnees().cloned().unwrap();
    assert_eq!(fin["etat"], "aucune");
    assert_eq!(fin["lignes"][0], "Aucune menace trouvée. Analyse terminée en moins d'une minute.");
}

#[test]
fn analyses_refusees_sans_defender() {
    let ctx = contexte("sans-defender");
    let f = Arc::new(faux_avec(EtatS::depuis(&json!({ "OtherAv": ["Avast"], "Uac": true })).unwrap()));
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    attendre(&mut b, &ctx);
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnScanQuick", &Value::Null, &mut e));
    assert_eq!(annonces(&e.0), vec!["Un instant : Nevermind finit ce qu'il est en train de faire.".to_string()]);
    assert!(!f.gestes().iter().any(|g| g.starts_with("analyser")));
    let p = b.pages(&ctx, "securite");
    assert_eq!(
        p["securite"]["onglets"][1]["blocs"].as_array().unwrap().last().unwrap()["texte"],
        "Ton antivirus (Avast) remplace Microsoft Defender : lance les analyses depuis ton antivirus."
    );
}

#[test]
fn actions_inconnues_laissees_aux_autres_blocs() {
    let ctx = contexte("inconnues");
    let mut b = BlocProtection::avec_systeme(Some(Arc::new(Faux::default())));
    let mut e = Effets::default();
    for id in ["clic:BtnPing", "clic:BtnUndo", "jeu:1", "sec:x", "hist:XYZ"] {
        assert!(!b.action(&ctx, id, &Value::Null, &mut e), "{id}");
    }
    assert!(b.action(&ctx, "hist:0000074e21f9", &Value::Null, &mut e));
    assert_eq!(signal(&e.0, "historique-annuler"), Some(&json!({ "id": "0000074e21f9" })));
    for id in ["dns", "carte-relancer", "debit", "clic:BtnDnsFlush"] {
        assert!(!b.action(&ctx, id, &json!("2"), &mut e), "{id} : bloc réseau");
    }
}

// --- outils\tester-securite.ps1, partie 2 : les menaces après une analyse (Get-ScanThreats)

fn date(t: &str) -> Date {
    Date::lire(t).unwrap()
}

#[test]
fn menaces_apres_une_analyse() {
    let debut = date("2026-11-06T03:00:00");
    let apres = date("2026-11-06T03:00:30");
    let avant = date("2026-11-03T03:00:00");
    let rien = || (Ok(Vec::new()), Vec::new());
    // Sans droits, l'Aide répond : sa lecture fait foi
    let l = json!({ "Menaces": [{ "ThreatID": "9", "ThreatName": "Virus:Win32/Essai" }],
        "Detections": [{ "ThreatID": "9", "Quand": apres.ticks(), "Resources": ["file:_C:\\Users\\x\\Downloads\\a.exe"] },
                       { "ThreatID": "9", "Quand": avant.ticks(), "Resources": ["file:_C:\\vieux.exe"] }] });
    let (r, _) = regles::menaces_analyse(debut, false, Some(&l), &rien);
    assert!(
        r.lu && r.menaces.len() == 1
            && r.menaces[0].nom == "Virus:Win32/Essai"
            && r.menaces[0].fichier == "C:\\Users\\x\\Downloads\\a.exe"
    );
    let (r, _) = regles::menaces_analyse(
        debut,
        false,
        Some(&json!({ "Menaces": [], "Detections": [], "DetectionsLues": true })),
        &rien,
    );
    assert!(r.lu && r.menaces.is_empty());
    // L'Aide n'a pas pu lire les détections : une liste vide ne prouve rien
    let (r, _) = regles::menaces_analyse(
        debut,
        false,
        Some(&json!({ "Menaces": [], "Detections": [], "DetectionsLues": false })),
        &rien,
    );
    assert!(!r.lu);
    // Sans droits ni Aide : lecture vide ou en échec, résultat inconnu
    let (r, _) = regles::menaces_analyse(debut, false, None, &rien);
    assert!(!r.lu);
    let echec = || (Err("Lecture impossible.".to_string()), Vec::new());
    let (r, j) = regles::menaces_analyse(debut, false, None, &echec);
    assert!(!r.lu && j.is_some());
    let une = || {
        (
            Ok(vec![DetectionLue {
                id: "7".into(),
                quand: date("2026-11-06T03:00:30"),
                ressources: vec!["file:_C:\\b.exe".into()],
            }]),
            vec![("7".to_string(), "Trojan:Win32/Essai".to_string())],
        )
    };
    let (r, _) = regles::menaces_analyse(debut, false, None, &une);
    assert!(r.lu && r.menaces[0].nom == "Trojan:Win32/Essai" && r.menaces[0].fichier == "C:\\b.exe");
    // En administrateur : lecture sur place, sans l'Aide
    let (r, _) = regles::menaces_analyse(debut, true, Some(&l), &rien);
    assert!(r.lu && r.menaces.is_empty());
    let (r, j) = regles::menaces_analyse(debut, true, None, &echec);
    assert!(!r.lu && j.unwrap().contains("détections non lues"));
    // (5 secondes avant le début comptent encore, 6 non)
    let juste =
        || (Ok(vec![DetectionLue { id: "1".into(), quand: date("2026-11-06T02:59:55"), ressources: vec![] }]), vec![]);
    assert_eq!(regles::menaces_analyse(debut, true, None, &juste).0.menaces.len(), 1);
    let trop =
        || (Ok(vec![DetectionLue { id: "1".into(), quand: date("2026-11-06T02:59:54"), ressources: vec![] }]), vec![]);
    assert_eq!(regles::menaces_analyse(debut, true, None, &trop).0.menaces.len(), 0);
}

/// La lecture du vrai PC (lectures sans droits seulement), écrite en JSON pour la comparer à $SecDataWork :
/// `NEVERMIND_LECTURE_REELLE=<fichier> cargo test -- --ignored lecture_reelle`
#[cfg(windows)]
#[test]
#[ignore]
fn lecture_reelle() {
    let Ok(f) = std::env::var("NEVERMIND_LECTURE_REELLE") else { return };
    let sys = nevermind_protection::systeme_windows();
    let debut = Instant::now();
    let l = sys.lire();
    let duree = debut.elapsed().as_millis();
    let env = &l.env;
    let j = json!({ "S": l.s.json(), "Hosts": env.hosts, "Proxy": { "ProxyEnable": env.proxy_actif, "AutoConfigURL": env.proxy_pac,
        "ProxyServer": env.proxy_serveur }, "Demarrage": env.demarrage.iter().map(|d| json!({ "Nom": d.nom, "Exe": d.exe, "Enabled": d.actif })).collect::<Vec<_>>(),
        "Admin": sys.est_admin(), "Millisecondes": duree });
    std::fs::write(f, serde_json::to_string_pretty(&j).unwrap()).unwrap();
}

#[test]
fn reponse_non_rien_n_est_fait() {
    let (mut b, f, ctx) = bloc_avec_taches("non", &[], &[]);
    let mut e = Effets::default();
    assert!(b.action(&ctx, "sec:0:0", &json!({ "id": "sec:0:0", "valeur": "non" }), &mut e));
    assert!(e.0.is_empty(), "{:?}", e.0);
    assert!(!f.gestes().iter().any(|g| g.starts_with("désactiver")));
}

#[test]
fn analyse_demandee_par_le_trafic() {
    let ctx = contexte("trafic");
    let mut f = faux_avec(EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap());
    f.admin = true;
    let f = Arc::new(f);
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    attendre(&mut b, &ctx);
    let mut e = Effets::default();
    b.signal(
        &ctx,
        "analyse-demandee",
        &json!({ "Sorte": "CustomScan", "Chemins": ["C:/x/a.exe"], "Titre": "Analyse de A", "Marque": "cle-a" }),
        &mut e,
    );
    assert_eq!(b.panneau_donnees().unwrap()["titre"], "Analyse de A");
    let e = attendre(&mut b, &ctx);
    assert_eq!(signal(&e, "analyse-finie"), Some(&json!({ "Marque": "cle-a", "Resultat": "clean" })));
    assert!(f.gestes().iter().any(|g| g == "analyser Personnalisee C:/x/a.exe"));
}

/// La vérification de fond et la fin d'une analyse complètent leur lecture par l'Aide (sans droits), sans jamais
/// proposer la demande de Windows : ce sont des lectures, pas des clics (audit G5).
#[test]
fn lectures_par_l_aide_sans_demande_de_windows() {
    let ctx = contexte("sans-demande");
    let vues = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let v = vues.clone();
    ctx.fausse_aide(Arc::new(move |a: &str, _p: &Value| {
        v.lock().unwrap().push(a.to_string());
        Ok(match a {
            "lecture.taches" => json!([]),
            _ => json!({ "ExclusionsNombre": 0, "Menaces": [], "Detections": [] }),
        })
    }));
    let f = Arc::new(faux_avec(EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap()));
    assert!(!f.admin);
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    let _ = b.pages(&ctx, "protection");
    attendre(&mut b, &ctx);
    let mut e = Effets::default();
    b.signal(
        &ctx,
        "analyse-demandee",
        &json!({ "Sorte": "CustomScan", "Chemins": ["C:/x/a.exe"], "Titre": "Analyse de A", "Marque": "cle-a" }),
        &mut e,
    );
    attendre(&mut b, &ctx);
    let vues = vues.lock().unwrap().clone();
    assert!(vues.iter().filter(|a| *a == "lecture.securite").count() >= 2, "{vues:?}");
    assert!(vues.iter().any(|a| a == "lecture.taches"), "{vues:?}");
    assert_eq!(ctx.aide_demandes_relevees(), Vec::<String>::new(), "aucune demande de Windows pour une lecture");
}

/// « Analyser un dossier » : la racine d'un lecteur reste « C:\ » (« C: » serait le dossier courant du lecteur), la
/// barre finale d'un dossier est enlevée.
#[test]
fn analyser_un_dossier_garde_la_racine_du_lecteur() {
    for (choisi, analyse, titre) in [
        (r"C:\", r"C:\", r"Analyse de C:\"),
        (r"D:\", r"D:\", r"Analyse de D:\"),
        (r"d:\\", r"d:\", r"Analyse de d:\"),
        (r"C:\Jeux\", r"C:\Jeux", "Analyse de Jeux"),
        (r"C:\Jeux", r"C:\Jeux", "Analyse de Jeux"),
        (r" C:\Jeux\Steam\ ", r"C:\Jeux\Steam", "Analyse de Steam"),
    ] {
        let ctx = contexte("dossier");
        let mut f = faux_avec(EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap());
        f.admin = true;
        let f = Arc::new(f);
        let mut b = BlocProtection::avec_systeme(Some(f.clone()));
        attendre(&mut b, &ctx);
        let mut e = Effets::default();
        assert!(b.action(&ctx, "sec-analyser-dossier", &json!(choisi), &mut e));
        assert_eq!(b.panneau_donnees().unwrap()["titre"], titre, "{choisi}");
        attendre(&mut b, &ctx);
        let g = f.gestes();
        assert!(g.iter().any(|x| *x == format!("analyser Personnalisee {analyse}")), "{choisi} : {g:?}");
    }
}

/// Audit G3 : « Réactiver le pare-feu » attend l'Aide (jusqu'à 60 s) : dans un travail de fond. Le « Oui » rend la
/// main tout de suite, un deuxième « Oui » ne refait rien, et la fin note la même ligne d'historique qu'avant. Fausse
/// Aide : rien sur le vrai PC.
#[test]
fn reactiver_le_pare_feu_ne_fige_pas() {
    let ctx = contexte("pare-feu-fond").partage();
    let (feu, attente) = std::sync::mpsc::channel::<()>();
    let attente = std::sync::Mutex::new(Some(attente));
    let demandes = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let d = demandes.clone();
    ctx.fausse_aide(Arc::new(move |a: &str, _p: &Value| {
        if !a.starts_with("lecture.") {
            d.lock().unwrap().push(a.to_string());
            if let Some(r) = attente.lock().unwrap().take() {
                let _ = r.recv_timeout(Duration::from_secs(60));
            }
        }
        Ok(json!("ok"))
    }));
    let s = EtatS::depuis(&json!({ "Mp": defender(), "Uac": true, "FirewallOff": ["Public"] })).unwrap();
    let f = Arc::new(faux_avec(s));
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    attendre(&mut b, &ctx);
    let _ = b.pages(&ctx, "securite");
    let mut e = Effets::default();
    assert!(b.action(&ctx, "sec:0:0", &Value::Null, &mut e));
    let q = signal(&e.0, "question").expect("question posée").clone();
    assert!(q["texte"].as_str().unwrap().starts_with("Réactiver le pare-feu"), "{q}");
    let oui = json!({ "id": q["id"], "valeur": "oui" });
    let mut e = Effets::default();
    let debut = Instant::now();
    assert!(b.action(&ctx, "sec:0:0", &oui, &mut e));
    assert!(
        debut.elapsed() < Duration::from_secs(2),
        "le « Oui » a pris {:?} : le fil des blocs était figé",
        debut.elapsed()
    );
    let fin = Instant::now() + Duration::from_secs(10);
    while demandes.lock().unwrap().is_empty() {
        assert!(Instant::now() < fin, "l'Aide n'est pas appelée");
        std::thread::sleep(Duration::from_millis(10));
    }
    // Un deuxième « Oui » (la même question) pendant ce temps : rien
    let mut e = Effets::default();
    b.action(&ctx, "sec:0:0", &oui, &mut e);
    assert!(b.pages(&ctx, "securite").contains_key("securite"), "la page ne répond pas");
    feu.send(()).unwrap();
    let mut tout = Vec::new();
    while signal(&tout, "historique").is_none() {
        assert!(Instant::now() < fin, "le pare-feu ne finit pas : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(*demandes.lock().unwrap(), ["pare-feu.activer"], "le geste est reparti");
    let h = signal(&tout, "historique").unwrap();
    assert_eq!(h["titre"], "Réactiver le pare-feu (Pare-feu désactivé)");
    assert_eq!(h["log"][0]["Quoi"], "Pare-feu réactivé sur tous les réseaux");
}

/// M11 (audit du 08/11) : le « Oui » d'une question retrouve le point par ce qu'il est, pas par son numéro. Une
/// relecture pendant la question décale la liste (ici le pare-feu coupé passe devant) : le numéro 1, c'était les
/// tâches ; maintenant c'est le contrôle des comptes. Le « Oui » désactive les tâches, jamais l'autre geste.
#[test]
fn le_oui_d_une_question_retrouve_son_point_apres_une_relecture() {
    let ctx = contexte("question-decalee");
    let mut s = EtatS::depuis(&json!({ "Mp": defender(), "Uac": false })).unwrap();
    s.taches = vec![tache("Mise a jour", "\\")];
    let f = Arc::new(faux_avec(s.clone()));
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    attendre(&mut b, &ctx);
    b.pages(&ctx, "securite");
    let mut e = Effets::default();
    assert!(b.action(&ctx, "sec:1:0", &Value::Null, &mut e));
    let q = signal(&e.0, "question").expect("question posée").clone();
    assert!(q["texte"].as_str().unwrap().starts_with("Désactiver ces tâches planifiées ?"), "{q}");
    // Pendant la question : le pare-feu est coupé, la liste est relue
    s.pare_feu_coupe = vec!["Public".into()];
    f.lecture.lock().unwrap().s = s;
    let mut e = Effets::default();
    b.action(&ctx, "clic:BtnSecRefresh", &Value::Null, &mut e);
    attendre(&mut b, &ctx);
    b.pages(&ctx, "securite");
    let mut e = Effets::default();
    assert!(b.action(&ctx, "sec:1:0", &json!({ "id": q["id"], "valeur": "oui" }), &mut e));
    let mut tout = e.0;
    tout.extend(attendre(&mut b, &ctx));
    assert!(
        !tout.iter().any(|x| matches!(x, Effet::Signal { nom, donnees } if nom == "registre-ecrire" && donnees["Name"] == "EnableLUA")),
        "le Oui a fait le geste d'un autre point : {tout:?}"
    );
    assert!(f.gestes().contains(&r"désactiver \Mise a jour".to_string()), "{:?}", f.gestes());
}

/// M11 : le point de la question a disparu (déjà réglé ailleurs) : rien n'est fait, et c'est dit
#[test]
fn le_oui_d_une_question_dont_le_point_a_disparu_ne_fait_rien() {
    let ctx = contexte("question-disparue");
    let mut s = EtatS::depuis(&json!({ "Mp": defender(), "Uac": false })).unwrap();
    s.taches = vec![tache("Mise a jour", "\\")];
    let f = Arc::new(faux_avec(s.clone()));
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    attendre(&mut b, &ctx);
    b.pages(&ctx, "securite");
    let mut e = Effets::default();
    b.action(&ctx, "sec:1:0", &Value::Null, &mut e);
    let q = signal(&e.0, "question").expect("question posée").clone();
    s.taches.clear();
    s.pare_feu_coupe = vec!["Public".into()];
    f.lecture.lock().unwrap().s = s;
    b.action(&ctx, "clic:BtnSecRefresh", &Value::Null, &mut Effets::default());
    attendre(&mut b, &ctx);
    b.pages(&ctx, "securite");
    let mut e = Effets::default();
    b.action(&ctx, "sec:1:0", &json!({ "id": q["id"], "valeur": "oui" }), &mut e);
    let mut tout = e.0;
    tout.extend(attendre(&mut b, &ctx));
    assert!(!tout.iter().any(|x| matches!(x, Effet::Signal { nom, .. } if nom == "registre-ecrire")), "{tout:?}");
    assert!(!f.gestes().iter().any(|g| g.starts_with("désactiver")), "{:?}", f.gestes());
    assert!(annonces(&tout).iter().any(|a| a.contains("a changé")), "{:?}", annonces(&tout));
}

/// Lot B du second audit : « Analyser un dossier » n'attend plus la fenêtre de choix sur le fil des blocs (avant :
/// `choisir_dossier` faisait `.join()` sur la fenêtre, tout Nevermind figé tant que l'utilisateur parcourait ses
/// dossiers). Le clic rend la main tout de suite ; un deuxième clic pendant que la fenêtre est ouverte le dit en clair
/// (et n'en ouvre pas une deuxième) ; le dossier choisi est analysé au relevé.
#[test]
fn analyser_un_dossier_n_attend_pas_la_fenetre_de_choix() {
    let ctx = contexte("choix-dossier").partage();
    let mut f = faux_avec(EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap());
    f.admin = true;
    let (choisi, attente) = std::sync::mpsc::channel::<Option<String>>();
    *f.choix_dossier.lock().unwrap() = Some(attente);
    let f = Arc::new(f);
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    attendre(&mut b, &ctx);
    let mut e = Effets::default();
    let debut = Instant::now();
    assert!(b.action(&ctx, "clic:BtnScanFolder", &Value::Null, &mut e));
    assert!(
        debut.elapsed() < Duration::from_secs(2),
        "le clic a pris {:?} : le fil des blocs attendait la fenêtre",
        debut.elapsed()
    );
    // Un deuxième clic pendant que la fenêtre est ouverte : dit en clair, pas une deuxième fenêtre
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnScanFolder", &Value::Null, &mut e));
    assert_eq!(
        annonces(&e.0),
        ["La fenêtre pour choisir le dossier est déjà ouverte : choisis le dossier dedans, ou ferme-la."]
    );
    choisi.send(Some(r"C:\Jeux".into())).unwrap();
    let fin = Instant::now() + Duration::from_secs(10);
    while !f.gestes().iter().any(|g| g == r"analyser Personnalisee C:\Jeux") {
        assert!(Instant::now() < fin, "le dossier choisi n'est pas analysé : {:?}", f.gestes());
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        b.tic(&ctx, false, &mut e);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(b.panneau_donnees().unwrap()["titre"], "Analyse de Jeux");
    assert_eq!(f.gestes().iter().filter(|g| *g == "choisir un dossier").count(), 1, "deux fenêtres ouvertes");
}

/// Lot B du second audit : les lectures de l'Aide de la page Protection (`lecture.securite` puis `lecture.taches`,
/// jusqu'à 60 s chacune, au lancement, à chaque « Revérifier » et à la fin d'une analyse) ne sont plus attendues sur
/// le fil des blocs : aucun tic ne dure, même quand l'Aide met longtemps à répondre ; la lecture complétée arrive
/// ensuite comme avant.
#[test]
fn les_lectures_de_l_aide_ne_figent_pas_les_tics() {
    let ctx = contexte("lectures-fond").partage();
    let lentes = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let l = lentes.clone();
    ctx.fausse_aide(Arc::new(move |a: &str, _p: &Value| {
        l.lock().unwrap().push(a.to_string());
        // (l'Aide lente : 3 s par lecture)
        std::thread::sleep(Duration::from_secs(3));
        Ok(match a {
            "lecture.taches" => json!([]),
            _ => json!({ "ExclusionsNombre": 0, "Menaces": [], "Detections": [], "DetectionsLues": true }),
        })
    }));
    let f = Arc::new(faux_avec(EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap()));
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    // Les tics pendant la lecture du lancement, puis pendant une analyse et sa lecture de fin
    let tics = |b: &mut BlocProtection| {
        let fin = Instant::now() + Duration::from_secs(20);
        let mut tout = Vec::new();
        loop {
            let mut e = Effets::default();
            let d = Instant::now();
            b.tic(&ctx, false, &mut e);
            assert!(d.elapsed() < Duration::from_secs(1), "un tic a pris {:?} : il attendait l'Aide", d.elapsed());
            tout.extend(e.0);
            if !b.occupe() {
                return tout;
            }
            assert!(Instant::now() < fin, "la lecture ne finit pas");
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    tics(&mut b);
    assert!(b.note().is_some(), "la lecture n'est pas jugée");
    assert_eq!(*lentes.lock().unwrap(), ["lecture.securite", "lecture.taches"]);
    let mut e = Effets::default();
    b.signal(&ctx, "analyse-demandee", &json!({ "Sorte": "QuickScan", "Marque": "m" }), &mut e);
    let e = tics(&mut b);
    assert_eq!(signal(&e, "analyse-finie"), Some(&json!({ "Marque": "m", "Resultat": "clean" })));
    // (la fin de l'analyse a lu les menaces par l'Aide, puis la protection est relue et complétée)
    assert_eq!(lentes.lock().unwrap()[2], "lecture.securite");
    assert_eq!(ctx.aide_demandes_relevees(), Vec::<String>::new());
}

/// Lot B du second audit : pendant « Réactiver le pare-feu » (l'Aide jusqu'à 60 s), l'attente est montrée tout de
/// suite et retirée à la fin ; un deuxième « Oui » dans la même page le dit en clair (avant : seulement le journal).
#[test]
fn pare_feu_attente_montree_et_deuxieme_clic_dit() {
    let ctx = contexte("pare-feu-attente").partage();
    let (feu, attente) = std::sync::mpsc::channel::<()>();
    let attente = std::sync::Mutex::new(Some(attente));
    ctx.fausse_aide(Arc::new(move |a: &str, _p: &Value| {
        if !a.starts_with("lecture.")
            && let Some(r) = attente.lock().unwrap().take()
        {
            let _ = r.recv_timeout(Duration::from_secs(60));
        }
        Ok(json!("ok"))
    }));
    let s = EtatS::depuis(&json!({ "Mp": defender(), "Uac": true, "FirewallOff": ["Public"] })).unwrap();
    let mut b = BlocProtection::avec_systeme(Some(Arc::new(faux_avec(s))));
    attendre(&mut b, &ctx);
    let _ = b.pages(&ctx, "securite");
    let mut e = Effets::default();
    b.action(&ctx, "sec:0:0", &Value::Null, &mut e);
    let q = signal(&e.0, "question").expect("question posée").clone();
    let oui = json!({ "id": q["id"], "valeur": "oui" });
    let mut tout = Vec::new();
    let mut e = Effets::default();
    b.action(&ctx, "sec:0:0", &oui, &mut e);
    tout.extend(e.0);
    let fin = Instant::now() + Duration::from_secs(10);
    while !tout.iter().any(|x| matches!(x, Effet::Signal { nom, donnees } if nom == "occupe" && donnees["texte"] == "Réactivation du pare-feu...")) {
        assert!(Instant::now() < fin, "pas d'attente montrée : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut e = Effets::default();
    b.action(&ctx, "sec:0:0", &oui, &mut e);
    assert_eq!(annonces(&e.0), ["Attends la fin de « Réactiver le pare-feu » (Protection)"]);
    feu.send(()).unwrap();
    while signal(&tout, "historique").is_none() {
        assert!(Instant::now() < fin, "le pare-feu ne finit pas : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    let derniere = tout.iter().rev().find_map(|x| match x {
        // (la pastille du geste : la vérification qui suit montre la sienne, lot I)
        Effet::Signal { nom, donnees } if nom == "occupe" && donnees["travail"] == "geste" => {
            Some(donnees["texte"].clone())
        }
        _ => None,
    });
    assert_eq!(derniere, Some(json!("")), "l'attente reste à l'écran après la fin");
}

/// Lot F (reste du lot B) : « Supprimer les menaces » du panneau d'une analyse appelait Defender sur le fil des blocs
/// (tout Nevermind figé tant que Defender travaillait), hors du verrou des gestes une fois lancé, sans attente montrée.
/// Maintenant : un travail de fond, sous le verrou des gestes (un geste d'une autre page est refusé en le disant),
/// avec son attente, un deuxième clic refusé en le disant, puis « Menaces supprimées. » et la protection relue.
#[test]
fn supprimer_les_menaces_du_panneau_ne_fige_pas() {
    let ctx = contexte("supprimer-menaces").partage();
    let mut f = faux_avec(EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap());
    f.admin = true;
    let (feu, attente) = std::sync::mpsc::channel::<()>();
    *f.suppression.lock().unwrap() = Some(attente);
    let f = Arc::new(f);
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    attendre(&mut b, &ctx);
    let lectures = f.gestes().iter().filter(|g| *g == "lire").count();
    // La question d'abord
    let mut e = Effets::default();
    assert!(b.action(&ctx, "sec-supprimer-trouvees", &Value::Null, &mut e));
    let q = signal(&e.0, "question").expect("question posée").clone();
    assert!(q["texte"].as_str().unwrap().starts_with("Supprimer toutes les menaces"), "{q}");
    let oui = json!({ "id": "sec-supprimer-trouvees", "valeur": "oui" });
    let mut tout = Vec::new();
    let mut e = Effets::default();
    let debut = Instant::now();
    assert!(b.action(&ctx, "sec-supprimer-trouvees", &oui, &mut e));
    assert!(
        debut.elapsed() < Duration::from_secs(2),
        "le « Oui » a pris {:?} : le fil des blocs attendait Defender",
        debut.elapsed()
    );
    tout.extend(e.0);
    let fin = Instant::now() + Duration::from_secs(10);
    while !tout.iter().any(|x| matches!(x, Effet::Signal { nom, donnees } if nom == "occupe" && donnees["texte"] == "Suppression des menaces...")) {
        assert!(Instant::now() < fin, "pas d'attente montrée : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    // Pendant ce temps : un deuxième clic le dit, un geste d'une autre page est refusé (le verrou est tenu)
    let mut e = Effets::default();
    b.action(&ctx, "sec-supprimer-trouvees", &oui, &mut e);
    assert_eq!(annonces(&e.0), ["Attends la fin de « Supprimer les menaces » (Protection)"]);
    assert!(ctx.prendre_geste("Changer le DNS", "Réseau").is_err(), "le verrou des gestes n'est pas tenu");
    assert_eq!(f.gestes().iter().filter(|g| *g == "supprimer les menaces").count(), 1, "le geste est reparti");
    feu.send(()).unwrap();
    while !annonces(&tout).iter().any(|a| a == "Menaces supprimées.") {
        assert!(Instant::now() < fin, "la suppression ne finit pas : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    let derniere = tout.iter().rev().find_map(|x| match x {
        Effet::Signal { nom, donnees } if nom == "occupe" && donnees["travail"] == "geste" => {
            Some(donnees["texte"].clone())
        }
        _ => None,
    });
    assert_eq!(derniere, Some(json!("")), "l'attente reste à l'écran après la fin");
    assert!(ctx.prendre_geste("Changer le DNS", "Réseau").is_ok(), "le verrou des gestes n'est pas rendu");
    attendre(&mut b, &ctx);
    assert!(f.gestes().iter().filter(|g| *g == "lire").count() > lectures, "la protection n'est pas relue");
}

/// Lot F : la fenêtre « Choisis le dossier à analyser » reçoit la fenêtre principale de Nevermind comme propriétaire
/// (avant : aucune, elle pouvait passer derrière Nevermind). Le faux système note la propriétaire, sans rien montrer.
#[test]
fn la_fenetre_de_choix_a_nevermind_pour_proprietaire() {
    let ctx = contexte("choix-proprietaire").partage();
    ctx.definir_fenetre(0x1234);
    let mut f = faux_avec(EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap());
    f.admin = true;
    let f = Arc::new(f);
    let mut b = BlocProtection::avec_systeme(Some(f.clone()));
    attendre(&mut b, &ctx);
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnScanFolder", &Value::Null, &mut e));
    let fin = Instant::now() + Duration::from_secs(10);
    while f.proprietaire_choix.lock().unwrap().is_none() {
        assert!(Instant::now() < fin, "la fenêtre de choix n'est pas demandée");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(*f.proprietaire_choix.lock().unwrap(), Some(0x1234));
    attendre(&mut b, &ctx);
}

// ---------------------------------------------------------------------------------------------------------------
// Lot I : Nevermind ne se fige plus (fin des lots B et F)
// ---------------------------------------------------------------------------------------------------------------

/// Le faux système, avec ce que l'essai veut lent : la lecture des menaces par WMI (`detections`, jusqu'à 60 s en
/// vrai), et la lecture de la protection (`lire`), qui rend l'état du moment où elle COMMENCE puis attend le feu de
/// l'essai (comme la vraie, qui lit Defender, les tâches, l'Aide : un geste fini pendant ce temps n'y est pas)
struct Lent {
    f: Arc<Faux>,
    detections_attente: Duration,
    lecture_feu: std::sync::Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl Lent {
    fn nouveau(f: Arc<Faux>) -> Lent {
        Lent { f, detections_attente: Duration::ZERO, lecture_feu: std::sync::Mutex::new(None) }
    }
}

impl nevermind_protection::systeme::Systeme for Lent {
    fn lire(&self) -> Lecture {
        let l = self.f.lire();
        if let Some(r) = self.lecture_feu.lock().unwrap().take() {
            let _ = r.recv_timeout(Duration::from_secs(20));
        }
        l
    }
    fn est_admin(&self) -> bool {
        self.f.est_admin()
    }
    fn variable(&self, nom: &str) -> Option<String> {
        self.f.variable(nom)
    }
    fn existe(&self, chemin: &str) -> bool {
        self.f.existe(chemin)
    }
    fn script_protege(&self, chemin: &str) -> bool {
        self.f.script_protege(chemin)
    }
    fn temp(&self) -> String {
        self.f.temp()
    }
    fn public(&self) -> String {
        self.f.public()
    }
    fn hosts(&self) -> String {
        self.f.hosts()
    }
    fn maintenant(&self) -> Date {
        self.f.maintenant()
    }
    fn analyser(
        &self,
        sorte: nevermind_protection::systeme::TypeAnalyse,
        chemins: &[String],
        arret: &std::sync::atomic::AtomicBool,
    ) -> Result<(), String> {
        self.f.analyser(sorte, chemins, arret)
    }
    fn mettre_a_jour_base(&self) -> Result<(), String> {
        self.f.mettre_a_jour_base()
    }
    fn supprimer_menaces(&self, p: &nevermind_noyau::Permis) -> Result<(), String> {
        self.f.supprimer_menaces(p)
    }
    fn desactiver_tache(&self, p: &nevermind_noyau::Permis, nom: &str, chemin: &str) -> Result<bool, String> {
        self.f.desactiver_tache(p, nom, chemin)
    }
    fn detections(&self) -> regles::Detections {
        std::thread::sleep(self.detections_attente);
        self.f.detections()
    }
    fn bloc_notes(&self, chemin: &str) {
        self.f.bloc_notes(chemin)
    }
    fn montrer(&self, chemin: &str) {
        self.f.montrer(chemin)
    }
}

/// Les tics jusqu'à ce que le bloc n'ait plus rien en cours (20 s au plus) ; aucun ne doit durer
fn tics_sans_attente(b: &mut BlocProtection, ctx: &Contexte) -> Vec<Effet> {
    let fin = Instant::now() + Duration::from_secs(20);
    let mut tout = Vec::new();
    loop {
        let mut e = Effets::default();
        let d = Instant::now();
        b.tic(ctx, false, &mut e);
        b.travaux_finis(ctx, &mut e);
        assert!(d.elapsed() < Duration::from_secs(1), "un tic a pris {:?} : le fil des blocs attendait", d.elapsed());
        tout.extend(e.0);
        if !b.occupe() {
            return tout;
        }
        assert!(Instant::now() < fin, "le travail ne finit pas : {tout:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn pastilles(tout: &[Effet]) -> Vec<Value> {
    tout.iter()
        .filter_map(|x| match x {
            Effet::Signal { nom, donnees } if nom == "occupe" => Some(donnees.clone()),
            _ => None,
        })
        .collect()
}

/// Lot I, point 1 : à la fin d'une analyse de Defender en administrateur (et sans droits quand l'Aide n'a rien rendu),
/// les menaces sont lues sur place par WMI (deux requêtes, jusqu'à 60 s). Avant, cette lecture se faisait dans le tic,
/// sur le fil des blocs : tout Nevermind figé (FPS, parties). Maintenant elle se fait dans le fil de l'analyse, pendant
/// que le panneau dit encore « en cours » ; le verdict est le même.
#[test]
fn la_fin_d_une_analyse_ne_lit_pas_les_menaces_sur_le_fil_des_blocs() {
    for admin in [true, false] {
        let ctx = contexte(&format!("fin-analyse-{admin}")).partage();
        // (sans droits : une Aide qui ne rend rien, la lecture sur place prend le relais)
        ctx.fausse_aide(Arc::new(|_: &str, _: &Value| Err("l'Aide ne répond pas".into())));
        let mut f = faux_avec(EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap());
        f.admin = admin;
        let mut lent = Lent::nouveau(Arc::new(f));
        lent.detections_attente = Duration::from_secs(3);
        let mut b = BlocProtection::avec_systeme(Some(Arc::new(lent)));
        tics_sans_attente(&mut b, &ctx);
        let mut e = Effets::default();
        b.action(&ctx, "clic:BtnScanQuick", &Value::Null, &mut e);
        assert_eq!(b.panneau_donnees().unwrap()["etat"], "en-cours");
        tics_sans_attente(&mut b, &ctx);
        let fin = b.panneau_donnees().cloned().expect("fin de l'analyse");
        // (en administrateur, la lecture sur place fait foi ; sans droits, une liste vide ne prouve rien)
        assert_eq!(fin["etat"], if admin { "aucune" } else { "inconnu" }, "{fin}");
    }
}

/// Lot I, point 4 (M4) : une relecture demandée pendant une autre n'est plus perdue. La lecture du lancement est en
/// cours (elle a lu le pare-feu coupé) ; « Réactiver le pare-feu » finit pendant ce temps et demande une relecture.
/// Avant, elle était jetée : la page disait encore « pare-feu coupé » après « Pare-feu réactivé ». Maintenant elle est
/// faite juste après celle en cours, et la page dit le pare-feu actif.
#[test]
fn une_relecture_demandee_pendant_une_autre_est_faite_apres() {
    let ctx = contexte("relecture-gardee").partage();
    let coupe = EtatS::depuis(&json!({ "Mp": defender(), "Uac": true, "FirewallOff": ["Public"] })).unwrap();
    let actif = EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap();
    let mut f = faux_avec(coupe);
    f.admin = true;
    let f = Arc::new(f);
    let (feu, attente) = std::sync::mpsc::channel::<()>();
    let lent =
        Lent { f: f.clone(), detections_attente: Duration::ZERO, lecture_feu: std::sync::Mutex::new(Some(attente)) };
    let mut b = BlocProtection::avec_systeme(Some(Arc::new(lent)));
    // La lecture du lancement part (et a lu le pare-feu coupé)
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    let fin = Instant::now() + Duration::from_secs(5);
    while !f.gestes().iter().any(|g| g == "lire") {
        assert!(Instant::now() < fin, "la lecture ne part pas");
        std::thread::sleep(Duration::from_millis(10));
    }
    // Pendant ce temps, le pare-feu est réactivé, et une relecture est demandée (fin d'un geste, autre bloc...)
    f.lecture.lock().unwrap().s = actif;
    b.signal(&ctx, "protection-relire", &json!({}), &mut Effets::default());
    feu.send(()).unwrap();
    tics_sans_attente(&mut b, &ctx);
    assert_eq!(f.gestes().iter().filter(|g| *g == "lire").count(), 2, "la relecture demandée est perdue");
    let p = Value::Object(b.pages(&ctx, "securite")).to_string();
    assert!(!p.contains("Public"), "la page dit encore le pare-feu coupé : {p}");
}

/// Lot I, point 4 : « Revérifier » montre son attente pendant la lecture (avant : rien), et la retire à la fin. La
/// pastille porte sa clé : elle n'efface pas celle d'un geste qui tourne encore.
#[test]
fn reverifier_montre_son_attente() {
    let ctx = contexte("reverifier-attente").partage();
    let mut f = faux_avec(EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap());
    f.admin = true;
    let mut b = BlocProtection::avec_systeme(Some(Arc::new(Lent::nouveau(Arc::new(f)))));
    tics_sans_attente(&mut b, &ctx);
    let mut tout = Vec::new();
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnSecRefresh", &Value::Null, &mut e));
    tout.extend(e.0);
    tout.extend(tics_sans_attente(&mut b, &ctx));
    let p = pastilles(&tout);
    assert!(
        p.iter().any(|d| d["texte"] == "Vérification de la protection..."),
        "« Revérifier » ne montre pas d'attente : {tout:?}"
    );
    assert!(p.iter().all(|d| d["travail"].as_str().is_some_and(|t| !t.is_empty())), "{p:?}");
    assert_eq!(p.last().map(|d| d["texte"].clone()), Some(json!("")), "l'attente reste à l'écran");
}

/// Lot I, point 6 : la mise à jour de la base de virus envoyait sa pastille SANS clé ; sa fin (texte vide sans clé)
/// effaçait alors toutes les pastilles du bloc, même celle d'un geste encore en cours (« Réactivation du pare-feu... »).
/// Maintenant chaque pastille de la mise à jour porte sa clé.
#[test]
fn la_pastille_de_la_mise_a_jour_porte_sa_cle() {
    let ctx = contexte("pastille-base").partage();
    let mut f = faux_avec(EtatS::depuis(&json!({ "Mp": defender(), "Uac": true })).unwrap());
    f.admin = true;
    let mut b = BlocProtection::avec_systeme(Some(Arc::new(f)));
    tics_sans_attente(&mut b, &ctx);
    let mut tout = Vec::new();
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnScanUpdate", &Value::Null, &mut e));
    tout.extend(e.0);
    tout.extend(tics_sans_attente(&mut b, &ctx));
    let p = pastilles(&tout);
    assert!(p.iter().any(|d| d["texte"] == "Mise à jour de la base de virus..."), "{tout:?}");
    assert!(
        p.iter().all(|d| d["travail"].as_str().is_some_and(|t| !t.is_empty())),
        "une pastille sans clé efface celles des autres gestes : {p:?}"
    );
}
