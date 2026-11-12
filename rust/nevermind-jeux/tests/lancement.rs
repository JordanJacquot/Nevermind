//! M8 (audit du 08/11) : un jeu ou un désinstalleur qui demande les droits administrateur ne se lançait pas
//! (`std::process::Command` ne sait pas demander les droits), l'échec n'allait qu'au journal, et l'écran disait
//! quand même « Lancement de X... » ou « Désinstallation de X : suis les instructions... » (avec 15 min de
//! surveillance pour rien). Maintenant le lancement passe par ShellExecute quand il le faut, dans un travail de fond,
//! et un échec est dit. Et un double clic ne lance pas le jeu deux fois. Faux Windows : rien n'est lancé pour de vrai.

use nevermind_jeux::BlocJeux;
use nevermind_jeux::systeme::{Ruche, SystemeEssai};
use nevermind_noyau::{Bloc, Contexte, Effet, Effets, dossier_essai};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const NON: &str = "tu as répondu « Non » à la demande de Windows";

fn annonces(e: &[Effet]) -> Vec<String> {
    e.iter()
        .filter_map(|x| match x {
            Effet::Annonce(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

/// Un dossier d'essai avec un faux programme (il existe, mais n'est jamais lancé)
fn dossier(nom: &str, programme: &str) -> (PathBuf, String) {
    let d = dossier_essai(&format!("jeux-lancement-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    let jeu = d.join("Jeux").join("Hades");
    std::fs::create_dir_all(&jeu).unwrap();
    let exe = jeu.join(programme);
    std::fs::write(&exe, b"pas un vrai programme").unwrap();
    (d, exe.display().to_string())
}

/// Les effets jusqu'à ce que `fini` les accepte (10 s au plus)
fn attendre(b: &mut BlocJeux, ctx: &Contexte, mut tout: Vec<Effet>, fini: impl Fn(&[Effet]) -> bool) -> Vec<Effet> {
    let fin = Instant::now() + Duration::from_secs(10);
    while !fini(&tout) && Instant::now() < fin {
        let mut e = Effets::default();
        b.travaux_finis(ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    tout
}

/// Le bloc avec Hades ajouté à la main (« Ajouter un jeu »), choisi dans la bibliothèque
fn avec_hades(ctx: &Contexte, exe: &str, refus: Option<&str>) -> (BlocJeux, Arc<SystemeEssai>) {
    let sys = Arc::new(SystemeEssai {
        exe_choisi: Some(exe.to_string()),
        refus_lancement: refus.map(str::to_string),
        ..Default::default()
    });
    let mut b = BlocJeux::avec(sys.clone());
    let _ = b.pages(ctx, "jeux");
    let mut e = Effets::default();
    b.action(ctx, "clic:BtnLibAdd", &Value::Null, &mut e);
    let tout = attendre(&mut b, ctx, e.0, |t| annonces(t).iter().any(|a| a.contains("ajouté à tes jeux")));
    assert!(annonces(&tout).iter().any(|a| a.contains("ajouté à tes jeux")), "{tout:?}");
    for _ in 0..20 {
        let mut e = Effets::default();
        b.travaux_finis(ctx, &mut e);
        std::thread::sleep(Duration::from_millis(10));
    }
    b.choisir("Hades");
    (b, sys)
}

#[test]
fn un_jeu_refuse_par_windows_le_dit() {
    for partage in [false, true] {
        let (d, exe) = dossier(&format!("refus-{partage}"), "Hades.exe");
        let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true);
        let ctx = if partage { ctx.partage() } else { Arc::new(ctx) };
        let (mut b, sys) = avec_hades(&ctx, &exe, Some(NON));
        let mut e = Effets::default();
        let debut = Instant::now();
        assert!(b.action(&ctx, "jeu-lancer", &Value::Null, &mut e));
        assert!(debut.elapsed() < Duration::from_secs(2), "le clic a figé le fil des blocs");
        let tout = attendre(&mut b, &ctx, e.0, |t| annonces(t).iter().any(|a| a.contains("ne s'est pas lancé")));
        assert_eq!(sys.lances.lock().unwrap().len(), 1, "le jeu est demandé une fois");
        assert!(
            annonces(&tout).contains(&format!("« Hades » ne s'est pas lancé : {NON}.")),
            "l'échec n'est pas dit : {:?}",
            annonces(&tout)
        );
        // Refusé : on peut réessayer tout de suite
        let mut e = Effets::default();
        b.action(&ctx, "jeu-lancer", &Value::Null, &mut e);
        let _ = attendre(&mut b, &ctx, e.0, |t| annonces(t).iter().any(|a| a.contains("ne s'est pas lancé")));
        assert_eq!(sys.lances.lock().unwrap().len(), 2, "après un refus, le jeu se relance");
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn un_double_clic_ne_lance_pas_le_jeu_deux_fois() {
    let (d, exe) = dossier("double", "Hades.exe");
    let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true);
    let (mut b, sys) = avec_hades(&ctx, &exe, None);
    for _ in 0..3 {
        let mut e = Effets::default();
        assert!(b.action(&ctx, "jeu-lancer", &Value::Null, &mut e));
        let _ = attendre(&mut b, &ctx, e.0, |_| true);
    }
    let lances = sys.lances.lock().unwrap().clone();
    assert_eq!(lances.len(), 1, "le jeu n'est pas lancé une seule fois : {lances:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// Un jeu GOG et son désinstalleur (qui demande les droits) : refusé, rien n'est surveillé ni dit « en cours »
#[test]
fn un_desinstalleur_refuse_par_windows_le_dit() {
    let (d, desinstalleur) = dossier("desinstalleur", "unins000.exe");
    let jeu = d.join("Jeux").join("Hades");
    let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true).partage();
    let mut sys = SystemeEssai { refus_lancement: Some(NON.into()), ..Default::default() };
    let cle = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\1207658924_is1";
    sys.texte(Ruche::Machine, cle, "DisplayName", "Hades");
    sys.texte(Ruche::Machine, cle, "Publisher", "GOG.com");
    sys.texte(Ruche::Machine, cle, "InstallLocation", &jeu.display().to_string());
    sys.texte(Ruche::Machine, cle, "UninstallString", &format!("\"{desinstalleur}\""));
    sys.texte(Ruche::Machine, cle, "DisplayIcon", &jeu.join("Hades.exe").display().to_string());
    let sys = Arc::new(sys);
    let mut b = BlocJeux::avec(sys.clone());
    std::fs::write(jeu.join("Hades.exe"), b"pas un vrai jeu").unwrap();
    let _ = b.pages(&ctx, "jeux");
    let mut e = Effets::default();
    b.action(&ctx, "clic:BtnLibRefresh", &Value::Null, &mut e);
    let _ = attendre(&mut b, &ctx, e.0, |t| annonces(t).iter().any(|a| a.ends_with("jeux trouvés.")));
    assert!(b.bibliotheque().iter().any(|g| g.nom == "Hades"), "le jeu GOG n'est pas trouvé : {:?}", b.bibliotheque());
    b.choisir("Hades");
    let mut e = Effets::default();
    b.action(&ctx, "jeu-desinstaller", &Value::Null, &mut e);
    let q =
        e.0.iter()
            .find_map(|x| match x {
                Effet::Signal { nom, donnees } if nom == "question" => Some(donnees.clone()),
                _ => None,
            })
            .expect("la question avant de désinstaller");
    let mut e = Effets::default();
    b.action(&ctx, "jeux-reponse", &json!({ "id": q["id"], "valeur": "oui" }), &mut e);
    let tout = attendre(&mut b, &ctx, e.0, |t| annonces(t).iter().any(|a| a.contains("ne s'est pas ouvert")));
    assert_eq!(sys.lances.lock().unwrap().first().map(|x| x.0.clone()), Some(desinstalleur.clone()));
    let a = annonces(&tout);
    assert!(
        a.contains(&format!("Le désinstalleur de « Hades » ne s'est pas ouvert : {NON}.\n\nRien n'a été désinstallé.")),
        "l'échec n'est pas dit : {a:?}"
    );
    assert!(!a.iter().any(|x| x.starts_with("Désinstallation de Hades")), "un faux « en cours » : {a:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// Le faux Windows dont la liste des applis du menu Démarrer est lente (tout AppsFolder parcouru par COM en vrai)
struct AppsLentes {
    sys: SystemeEssai,
    lecture: Duration,
    lectures: std::sync::atomic::AtomicUsize,
}
impl nevermind_jeux::systeme::Systeme for AppsLentes {
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
        self.lectures.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::thread::sleep(self.lecture);
        self.sys.applis_demarrer()
    }
    fn maintenant(&self) -> nevermind_jeux::texte::Moment {
        self.sys.maintenant()
    }
    fn lancer(&self, e: &str, a: &str, d: &str) -> Result<(), String> {
        self.sys.lancer(e, a, d)
    }
    fn faux(&self) -> bool {
        true
    }
    fn choisir_exe(&self) -> Option<String> {
        None
    }
    fn infos_programme(&self, e: &str) -> (String, String) {
        self.sys.infos_programme(e)
    }
}

/// Gels (audit du 10/11, M3) : un jeu Xbox ou Game Pass se lance par son appli du menu Démarrer, cherchée dans toute la
/// liste des applis de Windows (tout AppsFolder par COM, lent quand il y a beaucoup d'applis). Cette liste était lue
/// sur le fil des blocs à la première fiche d'un jeu Xbox et au lancement, et oubliée à chaque recherche des jeux.
/// Maintenant elle est lue dans un travail de fond : la fiche s'affiche et « Lancer » rend la main tout de suite, puis
/// le jeu se lance par son appli. Faux Windows à la lecture lente (3 s) : rien n'est lancé.
#[test]
fn un_jeu_xbox_ne_fige_pas_nevermind() {
    let d = dossier_essai("jeux-xbox-lent");
    let _ = std::fs::remove_dir_all(&d);
    let contenu = d.join("XboxGames").join("Forza Horizon 5").join("Content");
    std::fs::create_dir_all(&contenu).unwrap();
    // (pas un vrai programme, jamais lancé ; plus de 200 Ko, comme le programme d'un jeu)
    std::fs::write(contenu.join("ForzaHorizon5.exe"), vec![0u8; 300 * 1024]).unwrap();
    let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true).partage();
    let sys = Arc::new(AppsLentes {
        sys: SystemeEssai {
            disques: vec![format!("{}\\", d.display())],
            applis: vec![(
                "Forza Horizon 5".into(),
                "Microsoft.SunriseBaseGame_8wekyb3d8bbwe!SunriseReleaseFinal".into(),
            )],
            ..Default::default()
        },
        lecture: Duration::from_secs(3),
        lectures: Default::default(),
    });
    let mut b = BlocJeux::avec(sys.clone());
    // La recherche des jeux trouve le jeu Xbox
    let mut e = Effets::default();
    b.rechercher_maintenant(&ctx, &mut e);
    let vu = |t: &[Effet]| {
        t.iter().any(|x| {
            matches!(x, Effet::Signal { nom, donnees } if nom == "jeux-liste"
                && donnees.to_string().contains("Forza Horizon 5"))
        })
    };
    let tout = attendre(&mut b, &ctx, e.0, vu);
    assert!(vu(&tout), "le jeu Xbox n'est pas trouvé : {tout:?}");
    b.choisir("Forza Horizon 5");
    // Sa fiche s'affiche sans attendre la liste des applis
    let t = Instant::now();
    let p = b.pages(&ctx, "jeu");
    let duree = t.elapsed();
    assert!(p.contains_key("jeu"), "{:?}", p.keys().collect::<Vec<_>>());
    assert!(
        duree < Duration::from_secs(1),
        "la fiche du jeu a pris {duree:?} : la liste des applis lue sur le fil des blocs"
    );
    // « Lancer » rend la main tout de suite, puis le jeu se lance par son appli
    let mut e = Effets::default();
    let t = Instant::now();
    assert!(b.action(&ctx, "jeu-lancer", &Value::Null, &mut e));
    let duree = t.elapsed();
    assert!(
        duree < Duration::from_secs(1),
        "« Lancer » a pris {duree:?} : la liste des applis lue sur le fil des blocs"
    );
    let adresse = r"shell:AppsFolder\Microsoft.SunriseBaseGame_8wekyb3d8bbwe!SunriseReleaseFinal";
    let lance = |t: &[Effet]| t.iter().any(|x| matches!(x, Effet::OuvrirAdresse(a) if a == adresse));
    let tout = attendre(&mut b, &ctx, e.0, lance);
    assert!(lance(&tout), "le jeu ne se lance pas par son appli : {tout:?}");
    // (l'attente dite au clic, puis le lancement)
    let dites = annonces(&tout);
    let i = dites.iter().position(|a| a == "Préparation du lancement de Forza Horizon 5...");
    let j = dites.iter().position(|a| a == "Lancement de Forza Horizon 5 par Xbox...");
    assert!(i.is_some() && j.is_some() && i < j, "{dites:?}");
    // Un deuxième clic ne le relance pas (20 s)
    let mut e = Effets::default();
    assert!(b.action(&ctx, "jeu-lancer", &Value::Null, &mut e));
    let tout = attendre(&mut b, &ctx, e.0, |_| true);
    assert!(!lance(&tout), "relancé : {tout:?}");
    assert!(sys.lectures.load(std::sync::atomic::Ordering::SeqCst) <= 2, "la liste des applis est relue sans cesse");
    let _ = std::fs::remove_dir_all(&d);
}
