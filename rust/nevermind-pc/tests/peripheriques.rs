//! Les périphériques de la page « Ton PC » (casque, souris, clavier) : la section, la page de détail, « Ouvrir G Hub »
//! et le rythme des lectures. Rien ne touche le vrai PC ni les vrais appareils : un faux PC, un contexte d'essai.

use nevermind_noyau::{Bloc, Contexte, Effets, Permis};
use nevermind_pc::lecture::Lu;
use nevermind_pc::pc::{Pc, ResultatNettoyage, ValeurReg};
use nevermind_pc::peripheriques::{Charge, Logiciel, Peripherique, Sorte};
use nevermind_pc::temps::Moment;
use serde_json::{Map, Value};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Etat {
    appareils: Vec<Peripherique>,
    g_hub: bool,
    lectures: usize,
    lances: Vec<Vec<String>>,
}

struct FauxPc(Arc<Mutex<Etat>>);

impl Pc for FauxPc {
    fn peripheriques(&mut self) -> Vec<Peripherique> {
        let mut e = self.0.lock().unwrap();
        e.lectures += 1;
        e.appareils.clone()
    }
    fn logiciel_marque(&self, marque: &str) -> Option<Logiciel> {
        (marque == "Logitech" && self.0.lock().unwrap().g_hub)
            .then(|| Logiciel { nom: "G Hub".into(), chemin: "C:\\Program Files\\LGHUB\\lghub.exe".into() })
    }
    fn lancer(&mut self, args: &[String]) -> Result<(), String> {
        self.0.lock().unwrap().lances.push(args.to_vec());
        Ok(())
    }
    fn exiger_geste(&self) -> Result<Permis, String> {
        Err("essai".into())
    }
    fn reg_lire(&self, _: &str, _: &str) -> Result<Option<ValeurReg>, String> {
        Ok(None)
    }
    fn reg_utilisateur_ecrire(&mut self, _: &Permis, _: &str, _: &str, _: &ValeurReg) -> Result<(), String> {
        Ok(())
    }
    fn reg_utilisateur_effacer(&mut self, _: &Permis, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn origine_utilisateur(&mut self, _: &Permis, _: &str, _: &str) {}
    fn origine_utilisateur_oublier(&mut self, _: &Permis, _: &str) {}
    fn origines_utilisateur_effacer(&mut self, _: &Permis) {}
    fn origines_machine_effacer(&mut self, _: &Permis) -> Result<(), String> {
        Ok(())
    }
    fn aide(&mut self, _: &str, _: Value, _: u64) -> Result<Value, String> {
        Err("pas d'Aide".into())
    }
    fn powercfg(&mut self, _: &[String]) -> (i32, String) {
        (1, String::new())
    }
    fn curseur(&self) -> Option<String> {
        None
    }
    fn curseur_ecrire(&mut self, _: &Permis, _: &str) -> i32 {
        1
    }
    fn ecrans(&self) -> Vec<String> {
        Vec::new()
    }
    fn ecran_frequence(&mut self, _: &Permis, _: &str, _: i64) -> i32 {
        1
    }
    fn souris_relire(&mut self, _: &Permis, _: i64, _: i64, _: i64) {}
    fn service(&self, _: &str) -> Option<(String, bool)> {
        None
    }
    fn ouvrir(&mut self, _: &str) {}
    fn commande(&mut self, _: &Permis, _: &str, _: &[String]) -> i32 {
        1
    }
    fn dossier_temp(&self) -> String {
        "C:\\Temp".into()
    }
    fn dossier_windows(&self) -> String {
        "C:\\Windows".into()
    }
    fn hiberfil(&self) -> bool {
        false
    }
    fn nettoyer_dossier(&mut self, _: &Permis, _: &str, _: &[String], _: &[String]) -> ResultatNettoyage {
        ResultatNettoyage::default()
    }
    fn reglage(&self, _: &str) -> Option<Value> {
        None
    }
    fn ecrire_reglage(&mut self, _: &str, _: Value, _: bool) -> Result<(), String> {
        Ok(())
    }
    fn sauvegarde_ecrire(&mut self, _: &Permis, _: &Value) -> Result<(), String> {
        Ok(())
    }
    fn sauvegarde_effacer(&mut self, _: &Permis) {}
    fn ignores_ecrire(&mut self, _: &[String]) {}
    fn historique_ajouter(&mut self, _: &str, _: &[String], _: &[Value]) -> Option<String> {
        None
    }
    fn historique_annule(&mut self, _: &str) {}
    fn journal_nettoyage(&mut self, _: &str, _: &[String]) -> String {
        String::new()
    }
    fn message(&mut self, _: &str) {}
    fn question(&mut self, _: &str) -> Option<bool> {
        Some(false)
    }
    fn maintenant(&self) -> Moment {
        Moment::lire("2026-11-12T12:00:00").unwrap()
    }
}

fn appareil(sorte: Sorte, nom: &str, sans_fil: bool, batterie: Option<u8>, charge: Charge) -> Peripherique {
    Peripherique { sorte, nom: nom.into(), marque: "Logitech".into(), sans_fil, batterie, charge, logiciel: None }
}

/// Les trois appareils lus sur le PC de l'essai réel (souris 58 % sur batterie, casque 96 % en charge, clavier filaire)
fn les_trois() -> Vec<Peripherique> {
    vec![
        appareil(Sorte::Casque, "PRO X 2 LIGHTSPEED", true, Some(96), Charge::EnCharge),
        appareil(Sorte::Souris, "PRO X SUPERLIGHT 2", true, Some(58), Charge::Batterie),
        appareil(Sorte::Clavier, "PRO X RAPID", false, None, Charge::Batterie),
    ]
}

fn bloc(nom: &str, appareils: Vec<Peripherique>, g_hub: bool) -> (nevermind_pc::BlocPc, Arc<Mutex<Etat>>, Contexte) {
    let e = Arc::new(Mutex::new(Etat { appareils, g_hub, ..Default::default() }));
    let b = nevermind_pc::BlocPc::sur(Box::new(FauxPc(e.clone())), Lu::default());
    let d = nevermind_noyau::dossier_essai(&format!("pc-perif-{nom}"));
    (b, e, Contexte::nouveau(d.clone(), d, false, true))
}

fn blocs_pc(p: &Map<String, Value>) -> Vec<Value> {
    p["pc"]["blocs"].as_array().unwrap().clone()
}

fn titre_perif(b: &[Value]) -> Option<usize> {
    b.iter().position(|x| x["type"] == "titre" && x["texte"] == "Mes périphériques")
}

#[test]
fn sans_appareil_aucune_section() {
    let (mut b, e, ctx) = bloc("aucun", Vec::new(), true);
    let p = b.pages(&ctx, "pc");
    assert_eq!(e.lock().unwrap().lectures, 1, "lus une fois, la page ouverte");
    assert!(titre_perif(&blocs_pc(&p)).is_none());
    assert!(!p.keys().any(|k| k.starts_with("perif:")));
}

#[test]
fn la_section_apres_ton_pc_maintenant() {
    let (mut b, _, ctx) = bloc("trois", les_trois(), true);
    let p = b.pages(&ctx, "pc");
    let bl = blocs_pc(&p);
    let i = titre_perif(&bl).expect("la section");
    assert_eq!(bl[i - 1]["type"], "mesures", "juste après les mesures de « Ton PC maintenant »");
    let casque = &bl[i + 1];
    assert_eq!(casque["texte"], "PRO X 2 LIGHTSPEED");
    assert_eq!(casque["icone"], "casque");
    assert_eq!(casque["note"], "Casque · En charge");
    assert_eq!(casque["valeur"], "96 %");
    assert_eq!(casque["barre"], 96);
    assert_eq!(casque["ton"], "ok");
    assert_eq!(casque["id"], "perif:casque-pro-x-2-lightspeed");
    let souris = &bl[i + 2];
    assert_eq!(souris["note"], "Souris · Sur batterie");
    assert_eq!(souris["valeur"], "58 %");
    assert_eq!(souris["ton"], "");
    let clavier = &bl[i + 3];
    assert_eq!(clavier["valeur"], "Filaire");
    assert_eq!(clavier["note"], "Clavier · Branché par câble");
    assert!(clavier.get("barre").is_none());
    // (« Ouvrir G Hub » dans chaque ligne, maquette B : la ligne elle-même mène au détail)
    for l in [casque, souris, clavier] {
        assert_eq!(l["bouton"]["id"], "perif-logiciel:Logitech");
        assert_eq!(l["bouton"]["texte"], "Ouvrir G Hub");
        assert_eq!(l["bouton"]["style"], "second");
    }
    assert!(!bl.iter().any(|x| x["type"] == "bouton"), "pas de bouton à part");
    // (la page de détail de chaque appareil, qui ramène à Ton PC)
    let d = &p["perif:casque-pro-x-2-lightspeed"];
    assert_eq!(d["parent"], "pc");
    assert_eq!(d["module"]["anneau"]["valeur"], 96);
    assert_eq!(d["module"]["titre"], "En charge");
    assert_eq!(d["actions"][0]["id"], "perif-logiciel:Logitech");
    let textes: Vec<&str> = d["blocs"].as_array().unwrap().iter().filter_map(|x| x["texte"].as_str()).collect();
    for t in ["Batterie", "Connexion", "Marque", "Logiciel"] {
        assert!(textes.contains(&t), "la ligne {t}");
    }
    let c = &p["perif:clavier-pro-x-rapid"];
    assert_eq!(c["module"]["titre"], "Branché par câble");
    assert!(!c["blocs"].as_array().unwrap().iter().any(|x| x["texte"] == "Batterie"));
}

#[test]
fn bouton_absent_sans_le_logiciel() {
    let (mut b, _, ctx) = bloc("sans-ghub", les_trois(), false);
    let p = b.pages(&ctx, "pc");
    let bl = blocs_pc(&p);
    assert!(titre_perif(&bl).is_some());
    assert!(!bl.iter().any(|x| x.get("bouton").is_some() || x["type"] == "bouton"));
    let d = &p["perif:souris-pro-x-superlight-2"];
    assert!(d["actions"].as_array().unwrap().is_empty());
    assert!(d["module"]["boutons"].as_array().unwrap().is_empty());
    assert!(!d["blocs"].as_array().unwrap().iter().any(|x| x["texte"] == "Logiciel"));
}

#[test]
fn batterie_faible() {
    let l = vec![
        appareil(Sorte::Souris, "Souris", true, Some(12), Charge::Batterie),
        appareil(Sorte::Casque, "Casque", true, Some(30), Charge::Batterie),
        appareil(Sorte::Clavier, "Clavier", true, Some(10), Charge::EnCharge),
    ];
    let (mut b, _, ctx) = bloc("faible", l, false);
    let bl = blocs_pc(&b.pages(&ctx, "pc"));
    let ligne = |n: &str| bl.iter().find(|x| x["texte"] == n).unwrap().clone();
    assert_eq!(ligne("Souris")["ton"], "urgent");
    assert_eq!(ligne("Souris")["couleur"], "rouge");
    assert_eq!(ligne("Souris")["note"], "Souris · Presque vide, pense à la brancher");
    assert_eq!(ligne("Casque")["ton"], "attention");
    assert_eq!(ligne("Clavier")["ton"], "ok", "en charge : vert même à 10 %");
}

#[test]
fn ouvrir_g_hub() {
    let (mut b, e, ctx) = bloc("ouvrir", les_trois(), true);
    let _ = b.pages(&ctx, "pc");
    let mut ef = Effets::default();
    assert!(b.action(&ctx, "perif-logiciel:Logitech", &Value::Null, &mut ef));
    assert_eq!(e.lock().unwrap().lances, vec![vec!["C:\\Program Files\\LGHUB\\lghub.exe".to_string()]]);
}

#[test]
fn lu_seulement_page_ouverte_et_pas_plus_d_une_fois_par_minute() {
    let (mut b, e, ctx) = bloc("rythme", les_trois(), false);
    let _ = b.pages(&ctx, "tableau");
    let mut ef = Effets::default();
    let _ = b.tic(&ctx, false, &mut ef);
    assert_eq!(e.lock().unwrap().lectures, 0, "page fermée : rien n'est lu");
    let _ = b.pages(&ctx, "pc");
    let _ = b.tic(&ctx, true, &mut ef);
    assert_eq!(e.lock().unwrap().lectures, 1);
    for _ in 0..5 {
        let _ = b.tic(&ctx, false, &mut ef);
        let _ = b.pages(&ctx, "pc");
    }
    assert_eq!(e.lock().unwrap().lectures, 1, "relu au plus une fois par minute");
}

#[test]
fn ecran_cache_rien_n_est_lu() {
    let (mut b, e, ctx) = bloc("cache", les_trois(), false);
    let _ = b.pages(&ctx, "accueil");
    let mut ef = Effets::default();
    let _ = b.tic(&ctx, true, &mut ef);
    assert_eq!(e.lock().unwrap().lectures, 0);
}
