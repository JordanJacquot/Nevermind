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
    fn choisir_exe(&self) -> Option<String> {
        self.fenetres.fetch_add(1, Ordering::SeqCst);
        if let Some(r) = self.feu.lock().unwrap().take() {
            let _ = r.recv_timeout(Duration::from_secs(60));
        }
        self.sys.choisir_exe()
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
