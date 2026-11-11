//! Lot H, M2 : la sauvegarde des réglages d'origine, les réglages et l'historique sont relus APRÈS la fermeture de
//! l'Aide et de Nevermind, juste avant la remise en état. Un réglage changé dans Nevermind pendant que la question
//! était affichée est donc remis comme avant lui aussi (avant, la remise en état se faisait sur la copie lue au tout
//! début, et la valeur d'origine de ce réglage était perdue).
//!
//! Contre un faux PC et un faux Windows (rien ne touche le vrai PC), puis sur de vrais fichiers dans %TEMP% pour la
//! relecture elle-même (sans suivre de lien, comme au départ).

use nevermind_desinstalleur::choix::Choix;
use nevermind_desinstalleur::travail::{self, Fin, Lieux, Relue, Suppression, Systeme};
use nevermind_pc::corrections::Sauvegarde;
use nevermind_pc::net;
use nevermind_pc::pc::{Pc, ResultatNettoyage, ValeurReg};
use nevermind_pc::temps::Moment;
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

type Notes = Rc<RefCell<Vec<String>>>;
type Reglages = Rc<RefCell<Map<String, Value>>>;

/// Un faux PC : le registre et les services en mémoire ; chaque geste qui change le PC est noté
#[derive(Default)]
struct FauxPc {
    reg: Map<String, Value>,
    reglages: Reglages,
    gestes: Notes,
}

impl Pc for FauxPc {
    fn reg_lire(&self, chemin: &str, nom: &str) -> Result<Option<ValeurReg>, String> {
        Ok(self
            .reg
            .get(&format!("{chemin}|{nom}"))
            .map(|e| ValeurReg::depuis(&e["Value"], &net::champ_texte(e, "Kind"))))
    }
    fn reg_utilisateur_ecrire(&mut self, chemin: &str, nom: &str, v: &ValeurReg) -> Result<(), String> {
        self.reg.insert(format!("{chemin}|{nom}"), json!({ "Kind": v.sorte(), "Value": v.json() }));
        self.gestes.borrow_mut().push(format!("écrit {chemin}|{nom}={}", v.json()));
        Ok(())
    }
    fn reg_utilisateur_effacer(&mut self, chemin: &str, nom: &str) -> Result<(), String> {
        self.reg.remove(&format!("{chemin}|{nom}"));
        self.gestes.borrow_mut().push(format!("effacé {chemin}|{nom}"));
        Ok(())
    }
    fn origine_utilisateur(&mut self, _cle: &str, _json: &str) {}
    fn origine_utilisateur_oublier(&mut self, _cle: &str) {}
    fn origines_utilisateur_effacer(&mut self) {}
    fn origines_machine_effacer(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn aide(&mut self, action: &str, p: Value, _delai: u64) -> Result<Value, String> {
        self.gestes.borrow_mut().push(format!("aide {action} {}", net::champ_texte(&p, "Nom")));
        Ok(json!("OK"))
    }
    fn powercfg(&mut self, _args: &[String]) -> (i32, String) {
        (0, String::new())
    }
    fn curseur(&self) -> Option<String> {
        None
    }
    fn curseur_ecrire(&mut self, _g: &str) -> i32 {
        0
    }
    fn ecrans(&self) -> Vec<String> {
        Vec::new()
    }
    fn ecran_frequence(&mut self, _d: &str, _hz: i64) -> i32 {
        0
    }
    fn souris_relire(&mut self, _a: i64, _b: i64, _c: i64) {}
    fn service(&self, _nom: &str) -> Option<(String, bool)> {
        None
    }
    fn lancer(&mut self, _args: &[String]) -> Result<(), String> {
        Ok(())
    }
    fn ouvrir(&mut self, _a: &str) {}
    fn commande(&mut self, _exe: &str, _args: &[String]) -> i32 {
        0
    }
    fn dossier_temp(&self) -> String {
        "C:\\Faux\\Temp".into()
    }
    fn dossier_windows(&self) -> String {
        "C:\\Windows".into()
    }
    fn hiberfil(&self) -> bool {
        false
    }
    fn nettoyer_dossier(&mut self, _id: &str, _c: &[String], _e: &[String]) -> ResultatNettoyage {
        ResultatNettoyage::default()
    }
    fn reglage(&self, nom: &str) -> Option<Value> {
        self.reglages.borrow().get(nom).cloned()
    }
    fn ecrire_reglage(&mut self, nom: &str, v: Value, _exiger: bool) -> Result<(), String> {
        self.reglages.borrow_mut().insert(nom.into(), v);
        Ok(())
    }
    fn sauvegarde_ecrire(&mut self, _s: &Value) -> Result<(), String> {
        Ok(())
    }
    fn sauvegarde_effacer(&mut self) {}
    fn ignores_ecrire(&mut self, _i: &[String]) {}
    fn historique_ajouter(&mut self, _t: &str, _d: &[String], _j: &[Value]) -> Option<String> {
        None
    }
    fn historique_annule(&mut self, _id: &str) {}
    fn journal_nettoyage(&mut self, _nom: &str, _l: &[String]) -> String {
        String::new()
    }
    fn message(&mut self, _t: &str) {}
    fn question(&mut self, _t: &str) -> Option<bool> {
        Some(true)
    }
    fn maintenant(&self) -> Moment {
        Moment::lire("2026-11-11T03:00:00").unwrap()
    }
}

/// Ce que contiennent les données de Nevermind « sur le disque » (la sauvegarde et les réglages), que Nevermind change
/// pendant que la question est affichée
#[derive(Default)]
struct Disque {
    sauvegarde: Value,
    reglages: Map<String, Value>,
}

/// Le faux Windows : Nevermind est ouvert depuis ce dossier ; il change un réglage pendant la question
struct FauxSysteme {
    racine: PathBuf,
    disque: Rc<RefCell<Disque>>,
    /// Les réglages du faux PC (ceux que la relecture remplace)
    reglages: Reglages,
    /// Ce que Nevermind écrit pendant la question
    pendant_la_question: Option<Disque>,
    nevermind_ouvert: bool,
    /// Quand les données ont été relues : Nevermind était-il encore ouvert ?
    relectures: Vec<bool>,
    /// La relecture répond (sinon : le faux Windows d'avant, sans relecture)
    relit: bool,
    messages: Vec<String>,
    question: Option<String>,
}

impl FauxSysteme {
    fn nouveau(racine: &Path, disque: Rc<RefCell<Disque>>, reglages: Reglages) -> FauxSysteme {
        FauxSysteme {
            racine: racine.to_path_buf(),
            disque,
            reglages,
            pendant_la_question: None,
            nevermind_ouvert: true,
            relectures: Vec::new(),
            relit: true,
            messages: Vec::new(),
            question: None,
        }
    }
}

impl Systeme for FauxSysteme {
    fn historique(&mut self) -> Vec<Value> {
        Vec::new()
    }
    fn demarrage_auto(&mut self) -> bool {
        false
    }
    fn raccourci_bureau(&mut self) -> bool {
        false
    }
    fn question(&mut self, texte: &str) -> Option<Choix> {
        self.question = Some(texte.into());
        // Pendant que la question est affichée, l'utilisateur change un réglage dans Nevermind (encore ouvert)
        if let Some(d) = self.pendant_la_question.take() {
            *self.disque.borrow_mut() = d;
        }
        Some(Choix::Garder)
    }
    fn message_fin(&mut self, texte: &str) {
        self.messages.push(texte.into());
    }
    fn journal(&mut self, _ligne: &str) {}
    fn arreter_tache_aide(&mut self) {}
    fn processus(&mut self) -> Vec<(u32, String)> {
        if self.nevermind_ouvert { vec![(10, format!("{}\\Mindset.exe", self.racine.display()))] } else { Vec::new() }
    }
    fn fermer_processus(&mut self, pid: u32, _chemin: &Path) {
        if pid == 10 {
            self.nevermind_ouvert = false;
        }
    }
    fn retirer_demarrage(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn tache_existe(&mut self, _nom: &str) -> bool {
        false
    }
    fn retirer_tache(&mut self, _nom: &str) -> Result<(), String> {
        Ok(())
    }
    fn retirer_tache_aide(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn effacer_cle_aide(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn effacer_raccourci_bureau(&mut self) {}
    fn retirer_raccourci_menu(&mut self) {}
    fn existe(&mut self, _chemin: &Path) -> bool {
        false
    }
    fn lancer_suppression(&mut self, _commande: &str, _s: &Suppression) {}
    fn relire_donnees(&mut self) -> Option<Relue> {
        if !self.relit {
            return None;
        }
        self.relectures.push(self.nevermind_ouvert);
        let d = self.disque.borrow();
        *self.reglages.borrow_mut() = d.reglages.clone();
        Some(Relue { sauvegarde: Sauvegarde::depuis(&d.sauvegarde), probleme: None })
    }
}

fn entree(cle: &str, nom: &str, existait: bool, valeur: Value) -> (String, Value) {
    (
        format!("HKCU:\\Software\\Essai{cle}|{nom}"),
        json!({ "Path": format!("HKCU:\\Software\\Essai{cle}"), "Name": nom, "Existed": existait, "Value": valeur,
                "Kind": if existait { json!("DWord") } else { Value::Null } }),
    )
}

fn sauvegarde(entrees: &[(String, Value)]) -> Value {
    let m: Map<String, Value> = entrees.iter().cloned().collect();
    json!({ "Registry": m })
}

struct Essai {
    racine: PathBuf,
    pc: FauxPc,
    sys: FauxSysteme,
    debut: Sauvegarde,
}

/// Au début : un réglage de Nevermind à remettre (A). Pendant la question, Nevermind en change un autre (B, qui
/// n'existait pas) et coupe un service (noté dans SvcOriginal).
fn essai(nom: &str) -> Essai {
    let racine = std::env::temp_dir().join(format!("nevermind-desinst-relecture-{nom}-{}", std::process::id()));
    let a = entree("A", "Valeur", true, json!(1));
    let b = entree("B", "Valeur", false, Value::Null);
    let disque =
        Rc::new(RefCell::new(Disque { sauvegarde: sauvegarde(std::slice::from_ref(&a)), reglages: Map::new() }));
    let reglages: Reglages = Default::default();
    let gestes: Notes = Default::default();
    let mut pc = FauxPc { reglages: reglages.clone(), gestes, ..Default::default() };
    pc.reg.insert(a.0.clone(), json!({ "Kind": "DWord", "Value": 0 }));
    pc.reg.insert(b.0.clone(), json!({ "Kind": "DWord", "Value": 5 }));
    let mut sys = FauxSysteme::nouveau(&racine, disque.clone(), reglages);
    let mut apres = Map::new();
    apres.insert("SvcOriginal".into(), json!(["DiagTrack|Automatic|True"]));
    apres.insert("Theme".into(), json!("neon"));
    sys.pendant_la_question = Some(Disque { sauvegarde: sauvegarde(&[a, b]), reglages: apres });
    let debut = Sauvegarde::depuis(&disque.borrow().sauvegarde);
    Essai { racine, pc, sys, debut }
}

fn lancer(e: &mut Essai, impose: Option<Choix>) -> Fin {
    let lieux = Lieux { racine: e.racine.clone(), donnees: e.racine.join("donnees"), suite: None, pid: 999_999 };
    travail::desinstaller(&mut e.pc, &mut e.sys, e.debut.clone(), None, &lieux, impose)
}

/// M2 : le réglage changé pendant la question (B) et le service coupé pendant la question sont remis comme avant, en
/// plus de celui du début (A) ; les données sont relues une fois, Nevermind fermé
#[test]
fn un_reglage_change_pendant_la_question_est_remis() {
    let mut e = essai("question");
    let fin = lancer(&mut e, None);
    let Fin::Faite { message, erreurs } = fin else { panic!("annulée") };
    assert!(erreurs.is_empty(), "{erreurs:?}");
    assert_eq!(e.sys.relectures, vec![false], "les données ne sont pas relues une fois, Nevermind fermé");
    let gestes = e.pc.gestes.borrow().clone();
    assert!(gestes.iter().any(|g| g == "écrit HKCU:\\Software\\EssaiA|Valeur=1"), "A n'est pas remis : {gestes:?}");
    assert!(
        gestes.iter().any(|g| g == "effacé HKCU:\\Software\\EssaiB|Valeur"),
        "le réglage changé pendant la question n'est pas remis : {gestes:?}"
    );
    assert!(
        gestes.iter().any(|g| g == "aide service.regler DiagTrack"),
        "le service coupé pendant la question n'est pas remis : {gestes:?}"
    );
    // (la question annonçait 1 réglage ; le message de fin dit ce qui a été fait)
    assert!(e.sys.question.as_deref().is_some_and(|q| q.contains("remettre les 1 réglage ")), "{:?}", e.sys.question);
    assert!(message.contains("3 réglages"), "le message de fin ne dit pas ce qui a été fait : {message}");
    assert!(message.contains(nevermind_desinstalleur::conseils::REDEMARRER), "{message}");
    assert_eq!(e.sys.messages, vec![message]);
    // L'état d'origine du service, remis, n'est plus noté ; la préférence écrite par Nevermind reste
    let r = e.pc.reglages.borrow();
    assert_eq!(r.get("SvcOriginal"), Some(&json!([])));
    assert_eq!(r.get("Theme"), Some(&json!("neon")));
}

/// Rien de changé au début, un réglage changé pendant la question : il est remis quand même (et il faut redémarrer)
#[test]
fn rien_au_debut_un_reglage_pendant_la_question() {
    let mut e = essai("rien-au-debut");
    e.debut = Sauvegarde::default();
    *e.sys.disque.borrow_mut() = Disque::default();
    e.pc.reg.remove("HKCU:\\Software\\EssaiA|Valeur");
    let b = entree("B", "Valeur", false, Value::Null);
    e.sys.pendant_la_question = Some(Disque { sauvegarde: sauvegarde(&[b]), reglages: Map::new() });
    let Fin::Faite { message, .. } = lancer(&mut e, None) else { panic!("annulée") };
    assert!(
        e.pc.gestes.borrow().iter().any(|g| g == "effacé HKCU:\\Software\\EssaiB|Valeur"),
        "le réglage changé pendant la question n'est pas remis : {:?}",
        e.pc.gestes.borrow()
    );
    assert!(message.contains(nevermind_desinstalleur::conseils::REDEMARRER), "{message}");
}

/// Piloté par Mindset (pas de question) : la relecture a lieu aussi, après la fermeture
#[test]
fn pilote_par_mindset_les_donnees_sont_relues_apres_la_fermeture() {
    let mut e = essai("pilote");
    // (changé juste avant la fermeture : Nevermind écrit encore, rien ne s'affiche)
    let d = e.sys.pendant_la_question.take().unwrap();
    *e.sys.disque.borrow_mut() = d;
    let Fin::Faite { message, .. } = lancer(&mut e, Some(Choix::Garder)) else { panic!("annulée") };
    assert_eq!(e.sys.relectures, vec![false]);
    assert!(e.pc.gestes.borrow().iter().any(|g| g == "effacé HKCU:\\Software\\EssaiB|Valeur"));
    // (pas de question : le nombre annoncé n'a pas été montré, le message n'en parle pas)
    assert!(!message.contains("au lieu de"), "{message}");
}

/// Sans relecture possible (le faux Windows des juges) : la copie du début sert, comme avant
#[test]
fn sans_relecture_la_copie_du_debut_sert() {
    let mut e = essai("sans-relecture");
    e.sys.relit = false;
    let _ = lancer(&mut e, None);
    let gestes = e.pc.gestes.borrow().clone();
    assert!(gestes.iter().any(|g| g == "écrit HKCU:\\Software\\EssaiA|Valeur=1"), "{gestes:?}");
    assert!(!gestes.iter().any(|g| g.contains("EssaiB")), "{gestes:?}");
}

// ---------------------------------------------------------------------------
// La relecture elle-même, en administrateur : sur de vrais fichiers, sans suivre de lien
// ---------------------------------------------------------------------------
#[cfg(windows)]
mod sur_le_disque {
    use nevermind_desinstalleur::os::{SystemeWindows, contexte_administrateur, ecrire_donnees};
    use nevermind_desinstalleur::sur_place::AideSurPlace;
    use nevermind_desinstalleur::travail::Systeme;
    use serde_json::{Value, json};
    use std::path::{Path, PathBuf};

    fn base(nom: &str) -> PathBuf {
        let b = std::env::temp_dir().join(format!("nevermind-desinst-relu-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&b);
        std::fs::create_dir_all(&b).unwrap();
        b
    }

    fn entree(nom: &str) -> String {
        format!(
            r#""HKCU:\\Software\\Essai|{nom}": {{ "Path": "HKCU:\\Software\\Essai", "Name": "{nom}", "Existed": true, "Value": 1, "Kind": "DWord" }}"#
        )
    }

    fn ecrire(d: &Path, nom: &str, texte: &str) {
        std::fs::write(d.join(nom), texte).unwrap();
    }

    /// Un point de jonction `lien` vers `cible` (sans droits : mklink /J, sans fenêtre)
    fn jonction(lien: &Path, cible: &Path) {
        use std::os::windows::process::CommandExt;
        let st = std::process::Command::new(format!("{}\\cmd.exe", nevermind_noyau::windows::dossier_systeme()))
            .raw_arg(format!("/d /c mklink /J \"{}\" \"{}\"", lien.display(), cible.display()))
            .creation_flags(0x0800_0000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(st.success());
    }

    /// Nevermind écrit la sauvegarde, ses réglages et l'historique après la lecture du début : la relecture les voit
    /// tous ; à la fin, parametres.json garde ce que Nevermind y a écrit, seuls les états d'origine changent
    #[test]
    fn la_relecture_voit_ce_que_nevermind_a_ecrit() {
        let b = base("vue");
        let donnees = b.join("Donnees").join("Nevermind");
        std::fs::create_dir_all(&donnees).unwrap();
        ecrire(&donnees, "sauvegarde.json", &format!("{{ \"Registry\": {{ {} }} }}", entree("A")));
        ecrire(&donnees, "parametres.json", r#"{ "Theme": "neon" }"#);
        let racine = b.join("Programmes").join("Nevermind");
        let (ctx, lus) = contexte_administrateur(&racine, &donnees, true);
        let _geste = ctx.prendre_geste("Désinstaller", "Désinstallation").unwrap();
        // (les valeurs d'origine gardées au registre de ce PC peuvent s'y ajouter : seules A et B sont regardées)
        let s = ctx.sauvegarde();
        assert!(
            s.registre.contains_key("HKCU:\\Software\\Essai|A") && !s.registre.contains_key("HKCU:\\Software\\Essai|B")
        );
        // Nevermind, pendant la question
        ecrire(&donnees, "sauvegarde.json", &format!("{{ \"Registry\": {{ {}, {} }} }}", entree("A"), entree("B")));
        ecrire(
            &donnees,
            "parametres.json",
            r#"{ "Theme": "clair", "SvcOriginal": ["DiagTrack|Automatic|True"], "Nouvelle": 1 }"#,
        );
        ecrire(
            &donnees,
            "historique.json",
            r#"[ { "Id": "1", "Log": [ { "Type": "definitif", "Quoi": "Q", "Retour": "R" } ] } ]"#,
        );
        let aide = AideSurPlace::nouvelle();
        let mut sys =
            SystemeWindows { ctx: &ctx, aide: &aide, racine: racine.clone(), muet: true, reglages_relus: None };
        let r = sys.relire_donnees().expect("rien de relu");
        assert_eq!(r.probleme, None);
        assert!(r.sauvegarde.registre.contains_key("HKCU:\\Software\\Essai|B"), "la sauvegarde n'est pas relue");
        assert_eq!(
            ctx.reglage("SvcOriginal"),
            Some(json!(["DiagTrack|Automatic|True"])),
            "les réglages ne sont pas relus"
        );
        assert_eq!(ctx.historique().len(), 1, "l'historique n'est pas relu");
        // La remise en état, puis la fin : ce qui a été relu sert de référence
        ctx.ecrire_reglage("SvcOriginal", json!([]), false).unwrap();
        let reference = sys.reglages_relus.clone().unwrap_or(lus);
        ecrire_donnees(&donnees, &ctx, reference.as_ref());
        let p: Value =
            serde_json::from_str(&std::fs::read_to_string(donnees.join("parametres.json")).unwrap()).unwrap();
        assert_eq!(p, json!({ "Theme": "clair", "SvcOriginal": [], "Nouvelle": 1 }));
        let _ = std::fs::remove_dir_all(&b);
    }

    /// La relecture se fait comme au départ : la copie de secours si sauvegarde.json a disparu, et jamais à travers
    /// une jonction posée entre-temps à la place du dossier des données
    #[test]
    fn la_relecture_prend_la_copie_de_secours_et_ne_suit_jamais_un_lien() {
        let b = base("secours");
        let donnees = b.join("Donnees").join("Nevermind");
        std::fs::create_dir_all(&donnees).unwrap();
        ecrire(&donnees, "sauvegarde.json", &format!("{{ \"Registry\": {{ {} }} }}", entree("A")));
        let racine = b.join("Programmes").join("Nevermind");
        let (ctx, _) = contexte_administrateur(&racine, &donnees, true);
        let aide = AideSurPlace::nouvelle();
        let mut sys =
            SystemeWindows { ctx: &ctx, aide: &aide, racine: racine.clone(), muet: true, reglages_relus: None };
        // sauvegarde.json remplacé (Nevermind l'écrit à côté puis le renomme) : seule la copie de secours est là
        std::fs::remove_file(donnees.join("sauvegarde.json")).unwrap();
        ecrire(&donnees, "sauvegarde.json.bak", &format!("{{ \"Registry\": {{ {}, {} }} }}", entree("A"), entree("C")));
        let r = sys.relire_donnees().unwrap();
        assert!(
            r.sauvegarde.registre.contains_key("HKCU:\\Software\\Essai|C"),
            "la copie de secours n'est pas reprise"
        );
        // Le dossier des données remplacé par une jonction vers un dossier piégé : rien n'y est lu
        let piege = b.join("piege");
        std::fs::create_dir_all(&piege).unwrap();
        ecrire(&piege, "sauvegarde.json", &format!("{{ \"Registry\": {{ {} }} }}", entree("Piege")));
        std::fs::rename(&donnees, b.join("parti")).unwrap();
        jonction(&donnees, &piege);
        let r = sys.relire_donnees().unwrap();
        assert!(!r.sauvegarde.registre.contains_key("HKCU:\\Software\\Essai|Piege"), "la jonction a été suivie");
        assert_eq!(r.probleme.as_deref(), Some("abimee"));
        let _ = std::fs::remove_dir(&donnees);
        let _ = std::fs::remove_dir_all(&b);
    }
}
