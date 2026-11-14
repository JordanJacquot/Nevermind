//! Audit G3 : la fenêtre « Choisis le programme du jeu (.exe) » attend l'utilisateur sans limite, et la recherche des
//! jeux parcourt le registre et les disques. Les deux partent dans un fil à part : le clic rend la main tout de suite,
//! une deuxième fenêtre ne s'ouvre pas, et la fin donne les mêmes messages qu'avant. Faux Windows : rien sur le vrai
//! PC, aucune vraie fenêtre.

use nevermind_jeux::BlocJeux;
use nevermind_jeux::systeme::{Ruche, Systeme, SystemeEssai};
use nevermind_jeux::texte::Moment;
use nevermind_noyau::{Bloc, Contexte, Effet, Effets, dossier_essai};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Le faux Windows, avec une fenêtre de choix qui attend le feu du test (60 s au plus)
struct Lent {
    sys: SystemeEssai,
    feu: Mutex<Option<Receiver<()>>>,
    fenetres: AtomicUsize,
}
impl Systeme for Lent {
    fn reg_texte(&self, r: Ruche, c: &str, n: &str) -> Option<String> {
        self.sys.reg_texte(r, c, n)
    }
    fn reg_nombre(&self, r: Ruche, c: &str, n: &str) -> Option<u32> {
        self.sys.reg_nombre(r, c, n)
    }
    fn reg_sous_cles(&self, r: Ruche, c: &str) -> Vec<String> {
        self.sys.reg_sous_cles(r, c)
    }
    fn variable(&self, n: &str) -> Option<String> {
        self.sys.variable(n)
    }
    fn disques_fixes(&self) -> Vec<String> {
        self.sys.disques_fixes()
    }
    fn applis_demarrer(&self) -> Vec<(String, String)> {
        self.sys.applis_demarrer()
    }
    fn maintenant(&self) -> Moment {
        self.sys.maintenant()
    }
    fn lancer(&self, e: &str, a: &str, d: &str) -> Result<(), String> {
        self.sys.lancer(e, a, d)
    }
    fn choisir_exe(&self, proprietaire: usize) -> Option<String> {
        self.fenetres.fetch_add(1, Ordering::SeqCst);
        if let Some(r) = self.feu.lock().unwrap().take() {
            let _ = r.recv_timeout(Duration::from_secs(60));
        }
        self.sys.choisir_exe(proprietaire)
    }
    fn infos_programme(&self, e: &str) -> (String, String) {
        self.sys.infos_programme(e)
    }
}

fn annonces(e: &[Effet]) -> Vec<String> {
    e.iter()
        .filter_map(|x| match x {
            Effet::Annonce(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn ajouter_un_jeu_ne_fige_pas_pendant_la_fenetre_de_choix() {
    let d = dossier_essai("jeux-fond");
    let _ = std::fs::remove_dir_all(&d);
    let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true).partage();
    let (feu, attente) = channel();
    let sys = SystemeEssai { exe_choisi: Some(r"C:\Jeux\Hades\Hades.exe".into()), ..Default::default() };
    let lent = Arc::new(Lent { sys, feu: Mutex::new(Some(attente)), fenetres: AtomicUsize::new(0) });
    let mut b = BlocJeux::avec(lent.clone());
    let _ = b.pages(&ctx, "jeux");
    // « Ajouter un jeu » : la main rendue tout de suite, la fenêtre attend dans son fil
    let mut e = Effets::default();
    let debut = Instant::now();
    assert!(b.action(&ctx, "clic:BtnLibAdd", &Value::Null, &mut e));
    assert!(
        debut.elapsed() < Duration::from_secs(2),
        "le clic a pris {:?} : le fil des blocs était figé",
        debut.elapsed()
    );
    let fin = Instant::now() + Duration::from_secs(10);
    while lent.fenetres.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < fin, "la fenêtre de choix ne s'ouvre pas");
        std::thread::sleep(Duration::from_millis(10));
    }
    // Un deuxième clic : pas de deuxième fenêtre ; les pages répondent
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnLibAdd", &Value::Null, &mut e));
    assert!(b.pages(&ctx, "jeux").contains_key("jeux"));
    feu.send(()).unwrap();
    let mut tout = Vec::new();
    while !annonces(&tout).iter().any(|t| t.contains("ajouté à tes jeux")) {
        assert!(Instant::now() < fin, "le jeu n'est pas ajouté : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(lent.fenetres.load(Ordering::SeqCst), 1, "une deuxième fenêtre de choix");
    assert_eq!(annonces(&tout), ["Hades ajouté à tes jeux : Nevermind le reconnaîtra à son lancement."]);
    assert!(tout.iter().any(|x| matches!(x, Effet::Signal { nom, .. } if nom == "jeux-liste")));
    // Actualiser : « Recherche de tes jeux... » tout de suite, le nombre à la fin
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnLibRefresh", &Value::Null, &mut e));
    let mut tout = e.0;
    assert_eq!(annonces(&tout).first().map(String::as_str), Some("Recherche de tes jeux..."));
    while !annonces(&tout).iter().any(|t| t.ends_with("jeux trouvés.")) {
        assert!(Instant::now() < fin, "la recherche ne finit pas : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    let n = b.bibliotheque().len();
    assert_eq!(annonces(&tout), ["Recherche de tes jeux...".to_string(), format!("{n} jeux trouvés.")]);
    let _ = std::fs::remove_dir_all(&d);
}

/// Lot I, point 6 : « Supprimer » des restes de jeux (plusieurs Go parfois) se fait dans un fil à part. Avant, rien
/// ne montrait l'attente, et un nouveau clic pendant la suppression était jeté sans un mot. Maintenant l'attente est
/// montrée (et retirée à la fin), et un clic pendant ce temps le dit en clair. Un dossier d'essai seulement.
#[test]
fn supprimer_des_restes_montre_son_attente_et_dit_le_clic_refuse() {
    let d = dossier_essai("jeux-restes-attente");
    let _ = std::fs::remove_dir_all(&d);
    let reste = d.join("steamapps").join("common").join("VieuxJeu");
    std::fs::create_dir_all(&reste).unwrap();
    std::fs::write(reste.join("donnees.bin"), b"essai").unwrap();
    let dossier = reste.to_string_lossy().into_owned();
    let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true).partage();
    let mut b = BlocJeux::avec(Arc::new(SystemeEssai::default()));
    let _ = b.essai_barre_restes(&[("Vieux Jeu".into(), dossier.clone(), 200.0 * 1024.0 * 1024.0, false)], false);
    let mut e = Effets::default();
    assert!(b.action(&ctx, "jeux-reste-supprimer:0", &Value::Null, &mut e));
    let q =
        e.0.iter()
            .find_map(|x| match x {
                Effet::Signal { nom, donnees } if nom == "question" => Some(donnees.clone()),
                _ => None,
            })
            .expect("la question avant de supprimer");
    let mut e = Effets::default();
    assert!(b.action(&ctx, "jeux-reponse", &serde_json::json!({ "id": q["id"], "valeur": "oui" }), &mut e));
    let mut tout = e.0;
    // Un deuxième clic pendant la suppression : dit en clair
    let mut e = Effets::default();
    b.action(&ctx, "jeux-restes-tout", &Value::Null, &mut e);
    assert!(
        annonces(&e.0).iter().any(|a| a.contains("suppression")),
        "le clic pendant la suppression est jeté sans un mot : {:?}",
        e.0
    );
    let fin = Instant::now() + Duration::from_secs(10);
    while !tout.iter().any(|x| matches!(x, Effet::Signal { nom, .. } if nom == "resultat")) {
        assert!(Instant::now() < fin, "la suppression ne finit pas : {tout:?}");
        let mut e = Effets::default();
        b.tic(&ctx, false, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(20));
    }
    let pastilles: Vec<Value> = tout
        .iter()
        .filter_map(|x| match x {
            Effet::Signal { nom, donnees } if nom == "occupe" => Some(donnees.clone()),
            _ => None,
        })
        .collect();
    assert!(
        pastilles.first().is_some_and(|p| p["texte"].as_str().is_some_and(|t| !t.is_empty())),
        "pas d'attente montrée pendant la suppression : {tout:?}"
    );
    assert_eq!(pastilles.last().map(|p| p["texte"].clone()), Some(serde_json::json!("")), "l'attente reste");
    let _ = std::fs::remove_dir_all(&d);
}

/// Restes du 10/11 : la fenêtre « Choisis le programme du jeu (.exe) » reçoit la fenêtre principale de Nevermind
/// comme propriétaire (avant : aucune, elle pouvait passer derrière Nevermind ou Mindset), comme le choix du dossier
/// de Protection. Le faux système note la propriétaire, sans rien montrer.
#[test]
fn la_fenetre_de_choix_d_un_jeu_a_nevermind_pour_proprietaire() {
    let d = dossier_essai("jeux-choix-proprietaire");
    let _ = std::fs::remove_dir_all(&d);
    let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true).partage();
    ctx.definir_fenetre(0x1234);
    let sys = Arc::new(SystemeEssai { exe_choisi: None, ..Default::default() });
    let mut b = BlocJeux::avec(sys.clone());
    let _ = b.pages(&ctx, "jeux");
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnLibAdd", &Value::Null, &mut e));
    let fin = Instant::now() + Duration::from_secs(10);
    while sys.proprietaire_choix.lock().unwrap().is_none() {
        assert!(Instant::now() < fin, "la fenêtre de choix n'est pas demandée");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(*sys.proprietaire_choix.lock().unwrap(), Some(0x1234));
    let _ = std::fs::remove_dir_all(&d);
}
