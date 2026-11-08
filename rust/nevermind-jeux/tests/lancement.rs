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
