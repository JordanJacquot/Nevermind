//! Essais de l'exe sur un faux PC (`cargo test` seulement) : la couche Windows est remplacée, rien n'est touché sur le
//! vrai PC. Le corpus cas\aide\environnement.json y passe, avec le reste de ce que font les demandes.

use crate::actions::{self, EtatAide};
use crate::serveur::{self, Journal};
use crate::systeme::*;
use aide_regles::demandes::{self, Carte, ContexteProgrammes, Demarrage, Pc, REGLE_ANCIENNE};
use aide_regles::demarrage::LecturesDemarrage;
use aide_regles::dossiers::Dossiers;
use aide_regles::fermeture::Fiche;
use aide_regles::registre::Ruche;
use aide_regles::texte;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

pub(crate) const ELODIE: &str = "S-1-5-21-1111111111-2222222222-3333333333-1001";
const BRUNO: &str = "S-1-5-21-1111111111-2222222222-3333333333-1002";
const RACINE: &str = "C:\\Program Files\\Mindset\\Programmes\\Nevermind";
/// Nevermind (Mindset.exe de la copie), celui qui fait les demandes
const NEVERMIND: i64 = 9000;
const MAINTENANT: i64 = 639_000_000_000_000_000;
const JOUR: i64 = 864_000_000_000;

/// Un faux processus
#[derive(Clone)]
pub(crate) struct FauxProcessus {
    pub(crate) pid: i64,
    pub(crate) chemin: String,
    compte: String,
    session: i64,
    protection: i64,
    eleve: i64,
}

#[derive(Default)]
struct Etat {
    registre: HashMap<(String, String), ValeurLue>,
    regles: Vec<String>,
    fermes: Vec<i64>,
    points: Vec<String>,
    effaces: Vec<String>,
    appels: Vec<String>,
}

/// Le faux PC
pub(crate) struct FauxSysteme {
    dossiers: Dossiers,
    compte: String,
    admin: bool,
    pub(crate) processus: Vec<FauxProcessus>,
    fichiers: HashMap<String, Vec<Entree>>,
    etat: Mutex<Etat>,
    // Pour le faux canal : les connexions qui arrivent, une par une
    connexions: Arc<Mutex<VecDeque<FausseConnexion>>>,
    reponses: Arc<Mutex<Vec<String>>>,
    /// Les fausses mesures (PresentMon, trace réseau, tables TCP) : rejouées par essais_mesures
    pub(crate) mesures: Arc<Mutex<FaussesMesures>>,
}

/// Ce que le faux PC répond pour les mesures continues
#[derive(Default)]
pub(crate) struct FaussesMesures {
    /// Refus de PresentMon (absent, mal signé) ; aucun : il est là et bien signé
    pub(crate) presentmon_refus: Option<String>,
    /// Ce qui est lancé, dans l'ordre : les arguments de chaque PresentMon, et ceux de la fermeture de sa session
    pub(crate) lancements: Vec<String>,
    /// Où parle le dernier PresentMon lancé
    pub(crate) rappel_pm: Option<RappelPm>,
    /// Ce que Windows répond au prochain démarrage de la trace (aucun : accepté)
    pub(crate) trace_refus: Option<RefusTrace>,
    /// Où arrivent les événements de la trace en cours, et si elle tourne
    pub(crate) rappel_trace: Option<RappelTrace>,
    pub(crate) trace_tourne: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) locales: Vec<std::net::IpAddr>,
    pub(crate) filetime: i64,
    /// Tables TCP (IPv4, IPv6 ; aucune : illisible), et pour chaque connexion (son index dans sa table) : comptage
    /// accepté, et octets lus
    pub(crate) tables: [Option<Vec<ConnexionFausse>>; 2],
}

/// Une connexion de la fausse table TCP : la ligne, comptage accepté, octets lus
pub(crate) type ConnexionFausse = (LigneTcp, bool, Option<(u64, u64)>);

/// Un faux PresentMon lancé (rien à arrêter : il ne parle que quand l'essai le fait parler)
struct FauxLance;

impl Lance for FauxLance {
    fn arreter(&mut self) {}
}

/// Une fausse trace
struct FausseTrace(Arc<std::sync::atomic::AtomicBool>);

impl Trace for FausseTrace {
    fn tourne(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn arreter(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

pub(crate) fn p(pid: i64, chemin: &str, compte: &str) -> FauxProcessus {
    FauxProcessus { pid, chemin: chemin.into(), compte: compte.into(), session: 1, protection: 0, eleve: 0 }
}

impl FauxSysteme {
    pub(crate) fn new() -> FauxSysteme {
        let w = "C:\\Windows";
        let entree = |chemin: &str, dossier: bool, lien: bool, age_jours: i64| Entree {
            chemin: chemin.into(),
            dossier,
            lien,
            lecture_seule: false,
            taille: 1000,
            ecrit: MAINTENANT - age_jours * JOUR,
        };
        let mut fichiers = HashMap::new();
        fichiers.insert(
            format!("{w}\\Minidump"),
            vec![
                entree(&format!("{w}\\Minidump\\ancien.dmp"), false, false, 3),
                entree(&format!("{w}\\Minidump\\recent.dmp"), false, false, 0),
                entree(&format!("{w}\\Minidump\\sous"), true, false, 3),
                entree(&format!("{w}\\Minidump\\lien"), true, true, 3),
            ],
        );
        fichiers.insert(
            format!("{w}\\Minidump\\sous"),
            vec![entree(&format!("{w}\\Minidump\\sous\\vieux.dmp"), false, false, 9)],
        );
        fichiers.insert(
            format!("{w}\\Minidump\\lien"),
            vec![entree("C:\\Users\\x\\Documents\\nevermind-piege.txt", false, false, 9)],
        );
        FauxSysteme {
            dossiers: Dossiers {
                windows: w.into(),
                programmes: "C:\\Program Files".into(),
                programmes_x86: "C:\\Program Files (x86)".into(),
                donnees_communes: "C:\\ProgramData".into(),
            },
            compte: "S-1-5-18".into(),
            admin: true,
            processus: vec![
                p(NEVERMIND, &format!("{RACINE}\\Mindset.exe"), ELODIE),
                p(800, "C:\\Jeux\\chrome.exe", ELODIE),
                p(801, "C:\\Jeux\\chrome.exe", BRUNO),
                p(802, "C:\\Windows\\chrome.exe", ELODIE),
                FauxProcessus { protection: 1, ..p(803, "C:\\Jeux\\chrome.exe", ELODIE) },
                p(804, "C:\\Autre\\chrome.exe", ELODIE),
                p(805, "C:\\Users\\x\\Downloads\\Mindset.exe", ELODIE),
            ],
            fichiers,
            etat: Mutex::default(),
            connexions: Arc::default(),
            reponses: Arc::default(),
            mesures: Arc::default(),
        }
    }

    fn etat(&self) -> std::sync::MutexGuard<'_, Etat> {
        self.etat.lock().unwrap()
    }
}

impl LecturesDemarrage for FauxSysteme {
    fn commande_run(&self, _run32: bool, _nom: &str) -> Option<String> {
        None
    }
    fn antivirus(&self) -> Vec<String> {
        Vec::new()
    }
}

impl Pc for FauxSysteme {
    fn dossiers(&self) -> &Dossiers {
        &self.dossiers
    }
    fn fichier_existe(&self, chemin: &str) -> bool {
        texte::finit(chemin, ".exe")
    }
    fn processus_existe(&self, pid: i64) -> bool {
        self.processus.iter().any(|x| x.pid == pid)
    }
    fn carte(&self, index: i32) -> Option<Carte> {
        (index == 12).then(|| Carte { index, guid: "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0".into() })
    }
    fn dns_locaux(&self, _carte: &Carte) -> Vec<String> {
        vec!["192.168.1.1".into()]
    }
    fn dns_origine(&self, _guid: &str) -> Option<String> {
        None
    }
    fn contexte(&self) -> Result<ContexteProgrammes, String> {
        Ok(ContexteProgrammes { services_pid: [600].into_iter().collect(), ..Default::default() })
    }
    fn processus_protege(&self, _chemin: &str) -> bool {
        false
    }
}

impl Systeme for FauxSysteme {
    fn identite(&self) -> Identite {
        Identite { compte: self.compte.clone(), admin: self.admin, pid: 4242 }
    }
    fn racine(&self) -> String {
        RACINE.into()
    }
    fn heure_locale(&self) -> String {
        "2026-10-26T12:00:00".into()
    }
    fn ticks_utc(&self) -> i64 {
        MAINTENANT
    }
    fn ticks_local(&self, utc: i64) -> i64 {
        utc + 36_000_000_000
    }
    fn reg_lire(&self, _ruche: Ruche, cle: &str, nom: &str) -> Option<ValeurLue> {
        self.etat().registre.get(&(cle.to_string(), nom.to_string())).cloned()
    }
    fn reg_ecrire(&self, _ruche: Ruche, cle: &str, nom: &str, valeur: &ValeurLue) -> Result<(), String> {
        self.etat().registre.insert((cle.into(), nom.into()), valeur.clone());
        Ok(())
    }
    fn reg_effacer(&self, _ruche: Ruche, cle: &str, nom: &str) -> Result<(), String> {
        self.etat().registre.remove(&(cle.to_string(), nom.to_string()));
        Ok(())
    }
    fn reg_effacer_arbre(&self, _ruche: Ruche, cle: &str) -> Result<(), String> {
        self.etat().registre.retain(|(c, _), _| !c.starts_with(cle));
        Ok(())
    }
    fn service_regler(&self, nom: &str, demarrage: Demarrage, lancer: bool, arreter: bool) -> Result<(), String> {
        self.etat().appels.push(format!("service {nom} {demarrage:?} {lancer} {arreter}"));
        Ok(())
    }
    fn dns_appliquer(&self, _carte: &Carte, serveurs: &[String]) -> Result<(), String> {
        self.etat().appels.push(format!("dns {}", serveurs.join(",")));
        Ok(())
    }
    fn dns_vider_cache(&self) {}
    fn pare_feu_regles(&self) -> Result<Vec<String>, String> {
        Ok(self.etat().regles.clone())
    }
    fn pare_feu_ajouter(&self, nom: &str, programme: &str, service: Option<&str>, entrant: bool) -> Result<(), String> {
        self.etat().appels.push(format!("bloque {nom} {programme} {service:?} {entrant}"));
        self.etat().regles.push(nom.into());
        Ok(())
    }
    fn pare_feu_retirer(&self, nom: &str) -> Result<(), String> {
        self.etat().regles.retain(|r| r != nom);
        Ok(())
    }
    fn pare_feu_renommer(&self, ancien: &str, nouveau: &str) -> Result<(), String> {
        for r in self.etat().regles.iter_mut() {
            if r == ancien {
                *r = nouveau.into();
            }
        }
        Ok(())
    }
    fn pare_feu_activer(&self) -> Result<(), String> {
        Ok(())
    }
    fn point_restauration(&self, description: &str) -> Result<(), String> {
        self.etat().points.push(description.into());
        Ok(())
    }
    fn retirer_exclusion(&self, _element: &str) {}
    fn wifi_manuel(&self, _profil: &str) {}
    fn smb1_couper(&self, _client: bool) {}
    fn lecture_pc(&self) -> Value {
        json!({ "SecureBoot": null, "TpmOk": false, "Tpm": null, "Fiabilite": {} })
    }
    fn lecture_securite(&self) -> Value {
        json!({ "ExclusionsNombre": -1, "Menaces": [], "Detections": [], "DetectionsLues": false })
    }
    fn taches(&self) -> Vec<TacheVue> {
        let t = |nom: &str, dossier: &str, compte: &str, desactivee: bool| TacheVue {
            nom: nom.into(),
            dossier: dossier.into(),
            desactivee,
            compte: compte.into(),
            groupe: String::new(),
            actions: vec![("C:\\x.exe".into(), "-a".into())],
        };
        vec![
            t("A moi", "\\", ELODIE, false),
            t("Au système", "\\Mindset\\", "SYSTEM", false),
            t("A Bruno", "\\", BRUNO, false),
            t("Coupée", "\\", ELODIE, true),
            t("Microsoft", "\\Microsoft\\Windows\\", "SYSTEM", false),
        ]
    }
    fn traduire_compte(&self, nom: &str) -> Option<String> {
        texte::egal(nom, "SYSTEM").then(|| "S-1-5-18".into())
    }
    fn chemins_processus(&self) -> Vec<(i64, String)> {
        self.processus.iter().map(|x| (x.pid, x.chemin.clone())).collect()
    }
    fn entrees(&self, dossier: &str) -> Vec<Entree> {
        self.fichiers.get(dossier).cloned().unwrap_or_default()
    }
    fn dossier_existe(&self, dossier: &str) -> bool {
        self.fichiers.contains_key(dossier)
    }
    fn effacer_fichier(&self, chemin: &str, _lecture_seule: bool) -> Result<(), ErreurEffacement> {
        self.etat().effaces.push(chemin.into());
        Ok(())
    }
    fn effacer_dossier_vide(&self, dossier: &str) {
        self.etat().effaces.push(format!("{dossier}\\"));
    }
    fn processus(&self) -> Vec<ProcessusVu> {
        self.processus
            .iter()
            .map(|x| ProcessusVu { pid: x.pid, nom: texte::nom_fichier(&x.chemin).to_string() })
            .collect()
    }
    fn chemin_processus(&self, pid: i64) -> String {
        self.processus.iter().find(|x| x.pid == pid).map(|x| x.chemin.clone()).unwrap_or_default()
    }
    fn fiche(&self, pid: i64, chemin: &str) -> Fiche {
        match self.processus.iter().find(|x| x.pid == pid) {
            Some(x) => Fiche {
                id: pid,
                chemin: x.chemin.clone(),
                session: Some(x.session),
                compte: x.compte.clone(),
                protection: Some(x.protection),
                eleve: Some(x.eleve),
            },
            None => Fiche { id: pid, chemin: chemin.into(), ..Default::default() },
        }
    }
    fn fermer_processus(&self, pid: i64) -> bool {
        self.etat().fermes.push(pid);
        true
    }

    fn presentmon(&self) -> Result<String, String> {
        match &self.mesures.lock().unwrap().presentmon_refus {
            Some(e) => Err(e.clone()),
            None => Ok(format!("{RACINE}\\fichiers\\outils-tiers\\PresentMon.exe")),
        }
    }
    fn lancer_presentmon(&self, _chemin: &str, arguments: &str, rappel: RappelPm) -> Result<Box<dyn Lance>, String> {
        let mut m = self.mesures.lock().unwrap();
        m.lancements.push(arguments.into());
        m.rappel_pm = Some(rappel);
        Ok(Box::new(FauxLance))
    }
    fn fermer_session_presentmon(&self, _chemin: &str) {
        self.mesures.lock().unwrap().lancements.push("--terminate_existing_session --session_name Nevermind".into());
    }
    fn demarrer_trace(&self, rappel: RappelTrace) -> Result<Box<dyn Trace>, RefusTrace> {
        let mut m = self.mesures.lock().unwrap();
        if let Some(r) = m.trace_refus.take() {
            return Err(r);
        }
        m.rappel_trace = Some(rappel);
        m.trace_tourne = Arc::new(std::sync::atomic::AtomicBool::new(true));
        Ok(Box::new(FausseTrace(Arc::clone(&m.trace_tourne))))
    }
    fn adresses_locales(&self) -> Vec<std::net::IpAddr> {
        self.mesures.lock().unwrap().locales.clone()
    }
    fn filetime(&self) -> i64 {
        self.mesures.lock().unwrap().filetime
    }
    fn table_tcp(&self, v6: bool) -> Option<Vec<LigneTcp>> {
        let m = self.mesures.lock().unwrap();
        m.tables[usize::from(v6)].as_ref().map(|t| t.iter().map(|x| x.0.clone()).collect())
    }
    fn tcp_compter(&self, l: &LigneTcp) -> bool {
        let m = self.mesures.lock().unwrap();
        m.tables[usize::from(l.v6)].as_ref().and_then(|t| t.iter().find(|x| &x.0 == l)).is_some_and(|x| x.1)
    }
    fn tcp_octets(&self, l: &LigneTcp) -> Option<(u64, u64)> {
        let m = self.mesures.lock().unwrap();
        m.tables[usize::from(l.v6)].as_ref().and_then(|t| t.iter().find(|x| &x.0 == l)).and_then(|x| x.2)
    }
}

// --- Un faux canal

pub(crate) struct FausseConnexion {
    client: u32,
    morceaux: VecDeque<Option<Vec<u8>>>,
    reponses: Arc<Mutex<Vec<String>>>,
}

impl Connexion for FausseConnexion {
    fn client(&self) -> u32 {
        self.client
    }
    fn lire(&mut self, tampon: &mut [u8], _ms: u32) -> Result<Option<usize>, String> {
        match self.morceaux.pop_front() {
            None => Ok(Some(0)),
            Some(None) => Ok(None),
            Some(Some(o)) => {
                let n = o.len().min(tampon.len());
                tampon[..n].copy_from_slice(&o[..n]);
                if n < o.len() {
                    self.morceaux.push_front(Some(o[n..].to_vec()));
                }
                Ok(Some(n))
            }
        }
    }
    fn ecrire(&mut self, octets: &[u8]) -> Result<(), String> {
        self.reponses.lock().unwrap().push(String::from_utf8_lossy(octets).into_owned());
        Ok(())
    }
}

fn connexion(client: u32, morceaux: &[Option<&[u8]>], reponses: &Arc<Mutex<Vec<String>>>) -> FausseConnexion {
    FausseConnexion {
        client,
        morceaux: morceaux.iter().map(|m| m.map(<[u8]>::to_vec)).collect(),
        reponses: Arc::clone(reponses),
    }
}

pub(crate) struct FausseEcoute(Option<FausseConnexion>, Arc<Mutex<VecDeque<FausseConnexion>>>);

impl Ecoute for FausseEcoute {
    type C = FausseConnexion;
    fn attendre(&mut self, _ms: u32) -> Result<bool, String> {
        if self.0.is_some() {
            return Ok(true);
        }
        match self.1.lock().unwrap().pop_front() {
            Some(c) => {
                self.0 = Some(c);
                Ok(true)
            }
            // (plus de connexion : la boucle s'arrête)
            None => Err("fin de l'essai".into()),
        }
    }
    fn connexion(self) -> FausseConnexion {
        self.0.expect("connexion")
    }
}

impl Hote for FauxSysteme {
    type E = FausseEcoute;
    fn verrou(&self, _nom: &str) -> Option<Box<dyn Send>> {
        Some(Box::new(()))
    }
    fn preparer_aide_temp(&self, _dossier: &str, _admin: bool) -> Result<(), String> {
        Ok(())
    }
    fn canal(
        &self,
        nom: &str,
        sddl: &str,
        premier: bool,
        proprietaire: Option<&str>,
    ) -> Result<FausseEcoute, ErreurCanal> {
        self.etat().appels.push(format!("canal {nom} {sddl} {premier} {proprietaire:?}"));
        Ok(FausseEcoute(None, Arc::clone(&self.connexions)))
    }
}

pub(crate) fn etat_aide() -> EtatAide {
    EtatAide {
        admin: true,
        compte: "S-1-5-18".into(),
        pid: 4242,
        temp: format!("{RACINE}\\aide-temp\\"),
        mesures: Default::default(),
    }
}

/// Une demande, comme sur le canal ; rend la réponse en JSON
fn demander(s: &FauxSysteme, client: u32, ligne: &str) -> Value {
    let (r, _) = serveur::repondre(ligne, s, client, &etat_aide(), &Journal::default());
    serde_json::from_str(&r).unwrap()
}

// ---------------------------------------------------------------------------------------------------------------

/// cas\aide\environnement.json : de vraies demandes, en lecture seule, à une Aide dont l'environnement est piégé.
/// Ici, l'Aide ne lit aucune variable d'environnement : les dossiers viennent du faux PC (C:\Windows).
#[test]
fn corpus_environnement() {
    let dossier = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("cas").join("aide");
    let dossier = std::env::var_os("NEVERMIND_CAS").map(std::path::PathBuf::from).unwrap_or(dossier);
    let f: Value = serde_json::from_str(
        std::fs::read_to_string(dossier.join("environnement.json")).unwrap().trim_start_matches('\u{feff}'),
    )
    .unwrap();
    assert_eq!(f["format"], 1);
    let cas = f["cas"].as_array().unwrap();
    assert!(cas.len() >= 8);
    let s = FauxSysteme::new();
    let mut echecs = Vec::new();
    for c in cas {
        let mut e = c["entree"].to_string();
        e = e.replace("{pid}", &NEVERMIND.to_string());
        let e: Value = serde_json::from_str(&e).unwrap();
        let ligne = json!({ "a": e["action"], "p": e["parametres"] }).to_string();
        let r = demander(&s, NEVERMIND as u32, &ligne);
        let a = &c["attendu"];
        let texte = r.to_string();
        let ok = r["ok"] == a["ok"]
            && a["contient"].as_str().is_none_or(|x| texte.contains(x))
            && a["absent"].as_str().is_none_or(|x| !texte.contains(x));
        if !ok {
            echecs.push(format!("{} : {texte}", c["nom"]));
        }
    }
    assert!(echecs.is_empty(), "environnement.json :\n{}", echecs.join("\n"));
}

#[test]
fn bonjour_et_demandes_inconnues() {
    let s = FauxSysteme::new();
    let r = demander(&s, 1, r#"{"a":"aide.bonjour","p":{}}"#);
    assert_eq!(r["r"]["Version"], 5);
    assert_eq!(r["r"]["Compte"], "S-1-5-18");
    assert_eq!(r["r"]["Temp"], format!("{RACINE}\\aide-temp\\"));
    assert_eq!(demander(&s, 1, r#"{"a":"reg.lire","p":{}}"#)["e"], "L'Aide ne connaît pas la demande « reg.lire ».");
    assert_eq!(demander(&s, 1, "pas du json")["ok"], false);
    assert_eq!(demander(&s, 1, r#"{"a":"fps.lire"}"#)["r"], "");
    assert_eq!(demander(&s, 1, r#"{"a":"flux.lire"}"#)["e"], "La trace réseau ne tourne pas.");
}

#[test]
fn registre_valeur_d_origine_gardee_une_fois() {
    let s = FauxSysteme::new();
    let cle = "HKLM:\\SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers";
    s.reg_ecrire(
        Ruche::Machine,
        "SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers",
        "HwSchMode",
        &ValeurLue::DWord(1),
    )
    .unwrap();
    let r = demander(
        &s,
        1,
        &json!({"a":"reg.ecrire","p":{"Cle":cle,"Nom":"HwSchMode","Valeur":2,"Type":"DWord"}}).to_string(),
    );
    assert_eq!(r["r"], "ok");
    let origine =
        s.reg_lire(Ruche::Machine, "SOFTWARE\\Mindset\\Nevermind\\Aide\\Origines", &format!("{cle}|HwSchMode"));
    let attendu =
        format!("{{\"Path\":{},\"Name\":\"HwSchMode\",\"Existed\":true,\"Value\":1,\"Kind\":\"DWord\"}}", json!(cle));
    assert_eq!(origine, Some(ValeurLue::Texte(attendu.clone())));
    // Une seconde fois : la toute première valeur reste
    demander(&s, 1, &json!({"a":"reg.ecrire","p":{"Cle":cle,"Nom":"HwSchMode","Valeur":1,"Type":"DWord"}}).to_string());
    let origine =
        s.reg_lire(Ruche::Machine, "SOFTWARE\\Mindset\\Nevermind\\Aide\\Origines", &format!("{cle}|HwSchMode"));
    assert_eq!(origine, Some(ValeurLue::Texte(attendu)));
    assert_eq!(
        s.reg_lire(Ruche::Machine, "SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers", "HwSchMode"),
        Some(ValeurLue::DWord(1))
    );
    // Une valeur absente avant : notée absente
    demander(&s, 1, &json!({"a":"reg.effacer","p":{"Cle":"HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\DataCollection","Nom":"AllowTelemetry"}}).to_string());
    let o = s.reg_lire(
        Ruche::Machine,
        "SOFTWARE\\Mindset\\Nevermind\\Aide\\Origines",
        "HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\DataCollection|AllowTelemetry",
    );
    assert_eq!(
        o,
        Some(ValeurLue::Texte(
            "{\"Path\":\"HKLM:\\\\SOFTWARE\\\\Policies\\\\Microsoft\\\\Windows\\\\DataCollection\",\"Name\":\"AllowTelemetry\",\"Existed\":false,\"Value\":null,\"Kind\":null}".into()
        ))
    );
    // Oublier les origines
    demander(&s, 1, r#"{"a":"origines.oublier"}"#);
    assert!(s.etat().registre.keys().all(|(c, _)| !c.contains("Origines")));
}

#[test]
fn restauration_une_par_jour() {
    let s = FauxSysteme::new();
    let d = r#"{"a":"restauration.creer","p":{"Description":"Nevermind"}}"#;
    assert_eq!(demander(&s, 1, d)["r"], "ok");
    assert_eq!(s.etat().points, vec!["Nevermind".to_string()]);
    assert_eq!(
        s.reg_lire(Ruche::Machine, "SOFTWARE\\Mindset\\Nevermind\\Aide", "DerniereRestauration"),
        Some(ValeurLue::Texte(MAINTENANT.to_string()))
    );
    // Un point de moins de 24 h : c'est lui qui sert
    assert_eq!(demander(&s, 1, d)["r"], "recent");
    assert_eq!(s.etat().points.len(), 1);
}

#[test]
fn fermer_seulement_ce_que_la_regle_permet() {
    let s = FauxSysteme::new();
    let r = demander(&s, NEVERMIND as u32, r#"{"a":"processus.fermer","p":{"Nom":"chrome"}}"#);
    // 800 seulement : 801 (autre compte), 802 (dossier de Windows), 803 (protégé) restent ; 804 aussi est fermé
    assert_eq!(r["r"], 2);
    assert_eq!(s.etat().fermes, vec![800, 804]);
    // Par chemin : seulement ce programme-là
    let s = FauxSysteme::new();
    let r = demander(&s, NEVERMIND as u32, r#"{"a":"processus.fermer","p":{"Chemin":"C:\\Autre\\chrome.exe"}}"#);
    assert_eq!(r["r"], 1);
    assert_eq!(s.etat().fermes, vec![804]);
    // Sans savoir qui demande : refusé
    let r = demander(&s, 0, r#"{"a":"processus.fermer","p":{"Nom":"chrome"}}"#);
    assert_eq!(r["e"], "Demande refusée : programme qui fait la demande inconnu.");
    // Un Mindset.exe : jamais par son nom
    let r = demander(&s, NEVERMIND as u32, r#"{"a":"processus.fermer","p":{"Nom":"Mindset"}}"#);
    assert_eq!(r["ok"], false);
}

#[test]
fn pare_feu_regles_de_nevermind_seulement() {
    let s = FauxSysteme::new();
    s.etat().regles = vec![
        "Nevermind : bloque Truc".into(),
        format!("{REGLE_ANCIENNE}Truc"),
        "Nevermind : bloque Machin".into(),
        "Core Networking - DNS (UDP-Out)".into(),
    ];
    demander(&s, 1, r#"{"a":"pare-feu.debloquer","p":{"Nom":"Nevermind : bloque Truc"}}"#);
    assert_eq!(
        s.etat().regles,
        vec!["Nevermind : bloque Machin".to_string(), "Core Networking - DNS (UDP-Out)".to_string()]
    );
    demander(&s, 1, r#"{"a":"pare-feu.debloquer-tout"}"#);
    assert_eq!(s.etat().regles, vec!["Core Networking - DNS (UDP-Out)".to_string()]);
    // Anciennes règles renommées au démarrage
    s.etat().regles.push(format!("{REGLE_ANCIENNE}Vieux"));
    actions::renommer_anciennes_regles(&s).unwrap();
    assert!(s.etat().regles.contains(&"Nevermind : bloque Vieux".to_string()));
}

#[test]
fn nettoyage_sans_suivre_les_liens() {
    let s = FauxSysteme::new();
    let r = demander(&s, 1, r#"{"a":"nettoyage.lister","p":{"Id":"dump","Top":10}}"#);
    assert_eq!(r["r"]["Count"], 2, "{r}");
    assert_eq!(r["r"]["Size"], 2000.0);
    assert!(!r.to_string().contains("nevermind-piege"));
    // (dates en heure locale)
    assert_eq!(r["r"]["Top"][0][2], MAINTENANT - 3 * JOUR + 36_000_000_000);
    let r = demander(&s, 1, r#"{"a":"nettoyage.lancer","p":{"Id":"dump"}}"#);
    assert_eq!(r["r"]["Deleted"], 2);
    assert_eq!(r["r"]["Log"][0], "[SUPPRIMÉ] C:\\Windows\\Minidump\\ancien.dmp - 1 Ko");
    let effaces = s.etat().effaces.clone();
    assert!(!effaces.iter().any(|e| e.contains("recent") || e.contains("piege") || e.contains("lien")), "{effaces:?}");
    assert!(effaces.contains(&"C:\\Windows\\Minidump\\sous\\".to_string()));
}

#[test]
fn taches_du_compte_seulement() {
    let s = FauxSysteme::new();
    let r = demander(&s, NEVERMIND as u32, r#"{"a":"lecture.taches"}"#);
    let noms: Vec<&str> = r["r"].as_array().unwrap().iter().map(|t| t["Nom"].as_str().unwrap()).collect();
    assert_eq!(noms, vec!["A moi", "Au système"]);
    assert_eq!(demander(&s, 0, r#"{"a":"lecture.taches"}"#)["ok"], false);
}

#[test]
fn demande_trop_longue_trop_lente_ou_vide() {
    let s = FauxSysteme::new();
    let rep = Arc::new(Mutex::new(Vec::new()));
    let j = Journal::default();
    let gros = vec![b'x'; 70_000];
    serveur::traiter(connexion(1, &[Some(&gros)], &rep), &s, &etat_aide(), &j);
    serveur::traiter(connexion(1, &[Some(b"{\"a\":"), None], &rep), &s, &etat_aide(), &j);
    serveur::traiter(connexion(1, &[Some(b"\r\n")], &rep), &s, &etat_aide(), &j);
    assert!(rep.lock().unwrap().is_empty());
    let l = j.lignes();
    assert!(l[0].ends_with("Échange interrompu : demande trop longue (plus de 64 Ko), connexion coupée"), "{l:?}");
    assert!(l[1].ends_with("Échange interrompu : demande trop lente (plus de 5 s), connexion coupée"), "{l:?}");
    assert_eq!(l.len(), 2);
    // Une demande en deux morceaux, finie par une fin de ligne : une réponse, finie par \r\n
    serveur::traiter(
        connexion(1, &[Some(b"{\"a\":\"aide."), Some(b"bonjour\"}\r\nreste")], &rep),
        &s,
        &etat_aide(),
        &j,
    );
    let r = rep.lock().unwrap();
    assert_eq!(r.len(), 1);
    assert!(r[0].starts_with("{\"ok\":true") && r[0].ends_with("}\r\n"));
    assert!(j.lignes()[2].ends_with("aide.bonjour : fait"));
}

/// En SYSTEM, avec les droits administrateur : seulement Mindset.exe de ce dossier, sous le compte servi
#[test]
fn canal_client_et_compte_verifies() {
    let f = Arc::new(FauxSysteme::new());
    let rep = Arc::clone(&f.reponses);
    let ligne: &[u8] = b"{\"a\":\"aide.bonjour\"}\n";
    {
        let mut c = f.connexions.lock().unwrap();
        c.push_back(connexion(NEVERMIND as u32, &[Some(ligne)], &rep)); // Nevermind : répond
        c.push_back(connexion(805, &[Some(ligne)], &rep)); // un Mindset.exe ailleurs : refusé
        c.push_back(connexion(806, &[Some(ligne)], &rep)); // Mindset.exe de ce dossier, mais d'un autre compte : refusé
    }
    serveur::lancer(Arc::clone(&f), Some(ELODIE));
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(rep.lock().unwrap().len(), 1);
    let appels = f.etat().appels.clone();
    let suffixe = aide_regles::canal::suffixe(RACINE).unwrap();
    assert!(
        appels[0].starts_with(&format!("canal Nevermind-Aide-{ELODIE}-{suffixe} O:SYD:P(D;;0x1f019f;;;NU)")),
        "{appels:?}"
    );
    assert!(appels[0].ends_with(" true None"), "{appels:?}");
    assert!(appels[1].ends_with(" false Some(\"S-1-5-18\")"), "{appels:?}");
}

/// Pour qui l'Aide démarre : jamais sans -Sid en SYSTEM, jamais pour un autre compte en secours
#[test]
fn demarrage_refuse_sans_le_bon_sid() {
    for (compte, sid) in [("S-1-5-18", None), ("S-1-5-18", Some("pas-un-sid")), (ELODIE, Some(BRUNO))] {
        let mut f = FauxSysteme::new();
        f.compte = compte.into();
        let f = Arc::new(f);
        serveur::lancer(Arc::clone(&f), sid);
        assert!(f.etat().appels.is_empty(), "{compte} {sid:?} : l'Aide a ouvert son canal");
    }
    // Contre-épreuve : le bon SID, le canal est ouvert
    let mut f = FauxSysteme::new();
    f.compte = ELODIE.into();
    let f = Arc::new(f);
    serveur::lancer(Arc::clone(&f), Some(ELODIE));
    assert!(!f.etat().appels.is_empty());
}

#[test]
fn verification_avant_action() {
    // Une demande refusée ne touche à rien
    let s = FauxSysteme::new();
    let r = demander(&s, 1, r#"{"a":"service.regler","p":{"Nom":"WinDefend","Demarrage":"Disabled"}}"#);
    assert!(demandes::est_un_refus(r["e"].as_str().unwrap()));
    assert!(s.etat().appels.is_empty());
    let r = demander(&s, 1, r#"{"a":"service.regler","p":{"Nom":"DiagTrack","Demarrage":"disabled","Arreter":true}}"#);
    assert_eq!(r["r"], "ok");
    assert_eq!(s.etat().appels, vec!["service DiagTrack Desactive false true".to_string()]);
    // DNS : seulement après vérification, et l'origine est retenue la première fois
    let r = demander(&s, 1, r#"{"a":"dns.regler","p":{"Index":12,"Serveurs":["1.1.1.1","1.0.0.1"]}}"#);
    assert_eq!(r["r"], "ok");
    assert!(
        s.reg_lire(Ruche::Machine, "SOFTWARE\\Mindset\\Nevermind\\Aide\\Dns", "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0")
            .is_some()
    );
}

/// « Tout restaurer » remet le DNS automatique avec une liste vide, que PowerShell 5.1 envoie parfois comme un objet
/// vide (« "Serveurs":{} », une liste vide sortie d'un « if ») : acceptée comme dans la version PowerShell (essai réel
/// du 27/10 sur GitHub : refusée, le DNS restait sur 1.1.1.1)
#[test]
fn dns_automatique_avec_une_liste_vide() {
    for serveurs in ["[]", "{}", "null", "\"\"", "[\"\"]"] {
        let s = FauxSysteme::new();
        let ligne = format!("{{\"a\":\"dns.regler\",\"p\":{{\"Index\":12,\"Serveurs\":{serveurs}}}}}");
        let r = demander(&s, NEVERMIND as u32, &ligne);
        assert_eq!(r["ok"], true, "Serveurs = {serveurs} : {r}");
        assert!(s.etat().appels.iter().any(|a| a == "dns "), "Serveurs = {serveurs} : {:?}", s.etat().appels);
    }
    // (un objet non vide n'est pas une liste de serveurs : refusé, comme en PowerShell)
    let s = FauxSysteme::new();
    let r = demander(&s, NEVERMIND as u32, r#"{"a":"dns.regler","p":{"Index":12,"Serveurs":{"a":"1.1.1.1"}}}"#);
    assert_eq!(r["ok"], false);
}
