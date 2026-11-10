//! Les juges exportés de la version PowerShell (outils\exporter-juges-desinstalleur.ps1) rejoués sur le désinstalleur
//! Rust : « tout optimiser puis désinstaller » doit donner la même suite de gestes (demandes à l'Aide, registre de
//! l'utilisateur, powercfg, écrans, tâches, programmes fermés, clé de l'Aide, raccourcis, commande qui supprime les
//! fichiers), la même question, le même message de fin, le même journal, les mêmes fichiers restants dans les données
//! et les mêmes réglages notés. Contre un faux PC et une fausse Aide qui se comportent comme ceux du script ; les
//! dossiers d'essai sont dans %TEMP%. Rien ne touche le vrai PC.
//!
//! `NEVERMIND_JUGES_DESINST=<fichier.json>` : rejoue un autre export (des cas jamais vus, avec une autre graine).

use nevermind_desinstalleur::choix::{Choix, choix_impose};
use nevermind_desinstalleur::travail::{self, Fin, Lieux, Suppression, Systeme};
use nevermind_pc::corrections::Sauvegarde;
use nevermind_pc::net;
use nevermind_pc::pc::{Pc, ResultatNettoyage, ValeurReg};
use nevermind_pc::temps::Moment;
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Le faux PC (celui de exporter-juges-pc.ps1, repris par exporter-juges-desinstalleur.ps1)
// ---------------------------------------------------------------------------
const BAT: [(&str, &str); 5] = nevermind_pc::analyse::REGLAGES_BATTERIE;

type Notes = Rc<RefCell<Vec<Value>>>;
type Lignes = Rc<RefCell<Vec<String>>>;

#[derive(Default)]
struct FauxPc {
    reg: Map<String, Value>,
    gestes: Notes,
    journal: Lignes,
    plans: Vec<(String, String)>,
    actif: String,
    curseur: String,
    ecrans: Vec<String>,
    bat: Vec<(String, [i64; 2])>,
    services: Map<String, Value>,
    reglages: Map<String, Value>,
    refuse: Vec<String>,
    regles_pare_feu: usize,
    cartes: Vec<(String, i64)>,
}

impl FauxPc {
    fn geste(&mut self, g: &str, d: Value) {
        self.gestes.borrow_mut().push(json!({ "geste": g, "donnees": d }));
    }
    fn texte_batterie(&self) -> String {
        if self.bat.is_empty() {
            return String::new();
        }
        let mut l = vec![
            format!("GUID du mode de gestion de l'alimentation : {}  (Plan actif)", self.actif),
            format!("  GUID du sous-groupe : {}  (Batterie)", nevermind_pc::analyse::SOUS_GROUPE_BATTERIE),
        ];
        for (k, g) in BAT {
            if let Some((_, v)) = self.bat.iter().find(|(n, _)| n == k) {
                l.push(format!("    GUID du paramètre d'alimentation : {g}  ({k})"));
                l.push(format!("      Index de paramètre de courant alternatif actuel : 0x{:08x}", v[0] as u32));
                l.push(format!("      Index de paramètre de courant continu actuel : 0x{:08x}", v[1] as u32));
            }
        }
        l.join("\n")
    }
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
        self.geste(
            "reg.utilisateur.ecrire",
            json!({ "Cle": chemin, "Nom": nom, "Type": v.sorte(), "Valeur": v.json() }),
        );
        Ok(())
    }
    fn reg_utilisateur_effacer(&mut self, chemin: &str, nom: &str) -> Result<(), String> {
        self.reg.remove(&format!("{chemin}|{nom}"));
        self.geste("reg.utilisateur.effacer", json!({ "Cle": chemin, "Nom": nom }));
        Ok(())
    }
    fn origine_utilisateur(&mut self, cle: &str, j: &str) {
        self.geste("origine.utilisateur", json!({ "Cle": cle, "Json": j }));
    }
    fn origine_utilisateur_oublier(&mut self, cle: &str) {
        self.geste("origine.utilisateur.oublier", json!(cle));
    }
    fn origines_utilisateur_effacer(&mut self) {
        self.geste("origines.utilisateur.effacer", Value::Null);
    }
    fn origines_machine_effacer(&mut self) -> Result<(), String> {
        self.geste("origines.machine.effacer", Value::Null);
        Ok(())
    }
    fn aide(&mut self, action: &str, p: Value, _delai: u64) -> Result<Value, String> {
        self.geste(&format!("aide:{action}"), p.clone());
        if self.refuse.iter().any(|r| r == action) {
            return Err(format!("L'Aide a refusé ({action})"));
        }
        match action {
            "reg.ecrire" => {
                self.reg.insert(
                    format!("{}|{}", net::champ_texte(&p, "Cle"), net::champ_texte(&p, "Nom")),
                    json!({ "Kind": p["Type"], "Value": p["Valeur"] }),
                );
            }
            "reg.effacer" => {
                self.reg.remove(&format!("{}|{}", net::champ_texte(&p, "Cle"), net::champ_texte(&p, "Nom")));
            }
            "service.regler" => {
                self.services.insert(
                    net::champ_texte(&p, "Nom"),
                    json!({ "Running": net::champ_vrai(&p, "Lancer"), "StartType": net::champ_texte(&p, "Demarrage") }),
                );
            }
            _ => {}
        }
        Ok(json!("OK"))
    }
    fn powercfg(&mut self, args: &[String]) -> (i32, String) {
        let a0 = args.first().map(|s| s.as_str()).unwrap_or("");
        match a0 {
            "/getactivescheme" => {
                let t = self
                    .plans
                    .iter()
                    .find(|(g, _)| *g == self.actif)
                    .map(|(g, n)| format!("GUID du mode de gestion de l'alimentation : {g}  ({n})"))
                    .unwrap_or_default();
                return (0, t);
            }
            "/list" => {
                let mut l = vec![
                    "Modes de gestion de l'alimentation existants (* Actif)".to_string(),
                    "-----------------------------------".into(),
                ];
                for (g, n) in &self.plans {
                    l.push(format!(
                        "GUID du mode de gestion de l'alimentation : {g}  ({n}){}",
                        if *g == self.actif { " *" } else { "" }
                    ));
                }
                return (0, l.join("\n"));
            }
            "/q" => return (0, self.texte_batterie()),
            _ => {}
        }
        self.geste("powercfg", json!(args));
        match a0 {
            "/setactive" => {
                if args[1] != "SCHEME_CURRENT" {
                    if self.plans.iter().any(|(g, _)| *g == args[1]) {
                        self.actif = args[1].clone();
                    } else {
                        return (1, String::new());
                    }
                }
            }
            "/delete" => self.plans.retain(|(g, _)| *g != args[1]),
            "/setacvalueindex" | "/setdcvalueindex" => {
                let i = usize::from(a0 == "/setdcvalueindex");
                for (k, g) in BAT {
                    if g == args[3]
                        && let Some(x) = self.bat.iter_mut().find(|(n, _)| n == k)
                    {
                        x.1[i] = args[4].parse().unwrap_or(0);
                    }
                }
            }
            _ => {}
        }
        (0, String::new())
    }
    fn curseur(&self) -> Option<String> {
        if self.curseur.is_empty() { None } else { Some(self.curseur.clone()) }
    }
    fn curseur_ecrire(&mut self, g: &str) -> i32 {
        self.geste("curseur", json!(g));
        self.curseur = g.into();
        0
    }
    fn ecrans(&self) -> Vec<String> {
        self.ecrans.clone()
    }
    fn ecran_frequence(&mut self, d: &str, hz: i64) -> i32 {
        self.geste("ecran.frequence", json!({ "Ecran": d, "Hz": hz }));
        for e in self.ecrans.iter_mut() {
            let mut p: Vec<String> = e.split('|').map(String::from).collect();
            if p[0] == d {
                p[4] = hz.to_string();
            }
            *e = p.join("|");
        }
        0
    }
    fn souris_relire(&mut self, t1: i64, t2: i64, v: i64) {
        self.geste("souris.relire", json!([t1, t2, v]));
    }
    fn service(&self, nom: &str) -> Option<(String, bool)> {
        self.services.get(nom).map(|s| (net::champ_texte(s, "StartType"), net::champ_vrai(s, "Running")))
    }
    fn lancer(&mut self, args: &[String]) -> Result<(), String> {
        self.geste("lancer", json!(args));
        Ok(())
    }
    fn ouvrir(&mut self, a: &str) {
        self.geste("ouvrir", json!(a));
    }
    fn commande(&mut self, exe: &str, _args: &[String]) -> i32 {
        self.geste("commande", json!(exe));
        0
    }
    fn regles_pare_feu(&self) -> usize {
        self.regles_pare_feu
    }
    fn index_carte(&self, guid: &str) -> Option<i64> {
        self.cartes.iter().find(|(g, _)| g.eq_ignore_ascii_case(guid.trim_matches(['{', '}']))).map(|(_, i)| *i)
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
        self.reglages.get(nom).cloned()
    }
    fn ecrire_reglage(&mut self, nom: &str, v: Value, _exiger: bool) -> Result<(), String> {
        self.reglages.insert(nom.into(), v.clone());
        self.geste("reglage", json!({ "Nom": nom, "Valeur": v }));
        Ok(())
    }
    fn sauvegarde_ecrire(&mut self, _s: &Value) -> Result<(), String> {
        Ok(())
    }
    fn sauvegarde_effacer(&mut self) {
        self.geste("fichier.effacer", json!(["sauvegarde.json", "sauvegarde.json.bak", "sauvegarde.json.tmp"]));
    }
    fn ignores_ecrire(&mut self, i: &[String]) {
        self.geste("ignores", json!(i));
    }
    fn historique_ajouter(&mut self, _t: &str, _d: &[String], _j: &[Value]) -> Option<String> {
        None
    }
    fn historique_annule(&mut self, id: &str) {
        self.geste("historique.annule", json!(id));
    }
    fn journal_nettoyage(&mut self, nom: &str, _l: &[String]) -> String {
        format!("DONNEES\\nettoyage\\{nom}")
    }
    fn journal(&mut self, ligne: &str) {
        self.journal.borrow_mut().push(ligne.into());
    }
    fn message(&mut self, t: &str) {
        self.geste("message", json!(t));
    }
    fn question(&mut self, t: &str) -> Option<bool> {
        self.geste("question", json!(t));
        Some(true)
    }
    fn maintenant(&self) -> Moment {
        Moment::lire("2026-11-06T12:00:00").unwrap()
    }
}

// ---------------------------------------------------------------------------
// Le faux Windows de la désinstallation (les fausses versions de exporter-juges-desinstalleur.ps1)
// ---------------------------------------------------------------------------
#[derive(Default)]
struct FauxSysteme {
    gestes: Notes,
    journal: Lignes,
    historique: Vec<Value>,
    demarrage_auto: bool,
    raccourci_bureau: bool,
    reponse: Option<Choix>,
    processus: Vec<(u32, String)>,
    anciennes: Vec<String>,
    demarrage_refuse: bool,
    /// La tâche et la clé de l'Aide refusent de partir
    aide_restee: bool,
    question: Option<String>,
    messages: Vec<String>,
}

impl FauxSysteme {
    fn geste(&mut self, g: &str, d: Value) {
        self.gestes.borrow_mut().push(json!({ "geste": g, "donnees": d }));
    }
}

impl Systeme for FauxSysteme {
    fn historique(&mut self) -> Vec<Value> {
        self.historique.clone()
    }
    fn demarrage_auto(&mut self) -> bool {
        self.demarrage_auto
    }
    fn raccourci_bureau(&mut self) -> bool {
        self.raccourci_bureau
    }
    fn question(&mut self, texte: &str) -> Option<Choix> {
        self.question = Some(texte.into());
        self.reponse
    }
    fn message_fin(&mut self, texte: &str) {
        self.messages.push(texte.into());
    }
    fn journal(&mut self, ligne: &str) {
        self.journal.borrow_mut().push(ligne.into());
    }
    fn arreter_tache_aide(&mut self) {
        self.geste(
            "tache.arreter",
            json!({ "TaskName": travail::TACHE_AIDE_NOM, "TaskPath": travail::TACHE_AIDE_DOSSIER }),
        );
    }
    fn processus(&mut self) -> Vec<(u32, String)> {
        self.processus.clone()
    }
    fn fermer_processus(&mut self, pid: u32, _chemin: &Path) {
        self.geste("processus.fermer", json!(pid));
    }
    fn retirer_demarrage(&mut self) -> Result<(), String> {
        self.geste("demarrage.retirer", Value::Null);
        if self.demarrage_refuse { Err("Accès refusé.".into()) } else { Ok(()) }
    }
    fn tache_existe(&mut self, nom: &str) -> bool {
        self.anciennes.iter().any(|t| t == nom)
    }
    fn retirer_tache(&mut self, nom: &str) -> Result<(), String> {
        self.geste("tache.supprimer", json!({ "TaskName": nom, "TaskPath": null }));
        Ok(())
    }
    fn retirer_tache_aide(&mut self) -> Result<(), String> {
        self.geste(
            "tache.supprimer",
            json!({ "TaskName": travail::TACHE_AIDE_NOM, "TaskPath": travail::TACHE_AIDE_DOSSIER }),
        );
        if self.aide_restee { Err("Accès refusé.".into()) } else { Ok(()) }
    }
    fn effacer_cle_aide(&mut self) -> Result<(), String> {
        self.geste("registre.effacer.cle", json!(travail::CLE_AIDE));
        if self.aide_restee { Err("Accès refusé.".into()) } else { Ok(()) }
    }
    fn effacer_raccourci_bureau(&mut self) {
        self.geste("raccourci.bureau.effacer", Value::Null);
    }
    fn retirer_raccourci_menu(&mut self) {
        self.geste("raccourci.menu.retirer", Value::Null);
    }
    fn lancer_suppression(&mut self, commande: &str, _s: &Suppression) {
        self.geste("suppression", json!(commande));
    }
}

// ---------------------------------------------------------------------------
// La comparaison (comme les juges du bloc pc : listes d'un élément et nombres comme PowerShell les écrit)
// ---------------------------------------------------------------------------
fn normal(v: &Value, chemins: &[(String, &str)]) -> Value {
    match v {
        Value::Array(a) if a.is_empty() => Value::Null,
        Value::Array(a) if a.len() == 1 => normal(&a[0], chemins),
        Value::Array(a) => Value::Array(a.iter().map(|x| normal(x, chemins)).collect()),
        Value::Object(m) if m.is_empty() => Value::Null,
        Value::Object(m) => {
            let mut o = Map::new();
            let mut cles: Vec<&String> = m.keys().collect();
            cles.sort();
            for k in cles {
                let x = &m[k];
                let x = if k == "Json" && x.is_string() {
                    serde_json::from_str(x.as_str().unwrap_or("")).unwrap_or(x.clone())
                } else {
                    x.clone()
                };
                o.insert(k.clone(), normal(&x, chemins));
            }
            Value::Object(o)
        }
        Value::Number(n) => json!(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => {
            let mut t = s.clone();
            for (avant, apres) in chemins {
                t = t.replace(avant.as_str(), apres);
            }
            Value::String(t)
        }
        _ => v.clone(),
    }
}

fn ecart(a: &Value, b: &Value, chemin: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                let (u, v) = (x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null));
                if let Some(e) = ecart(u, v, &format!("{chemin}.{k}")) {
                    return Some(e);
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!(
                    "{chemin} : {} éléments en PowerShell, {} en Rust\nPS   : {}\nRust : {}",
                    x.len(),
                    y.len(),
                    court(a),
                    court(b)
                ));
            }
            x.iter().zip(y).enumerate().find_map(|(i, (u, v))| ecart(u, v, &format!("{chemin}[{i}]")))
        }
        _ if a == b => None,
        _ => Some(format!("{chemin}\nPS   : {}\nRust : {}", court(a), court(b))),
    }
}

fn court(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 1500 { format!("{}...", s.chars().take(1500).collect::<String>()) } else { s }
}

fn comparer(ps: &Value, rust: &Value, quoi: &str, chemins: &[(String, &str)]) -> Result<(), String> {
    match ecart(&normal(ps, chemins), &normal(rust, chemins), quoi) {
        None => Ok(()),
        Some(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Un cas rejoué
// ---------------------------------------------------------------------------
fn creer(dossier: &Path, noms: &[Value]) {
    for n in noms {
        let p = dossier.join(net::texte(n));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "{}").unwrap();
    }
}

/// Les fichiers du dossier (chemins relatifs, triés sans tenir compte des majuscules), `None` s'il n'existe pas
fn fichiers(dossier: &Path) -> Option<Vec<String>> {
    fn parcourir(d: &Path, base: &Path, l: &mut Vec<String>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                parcourir(&p, base, l);
            } else {
                l.push(p.strip_prefix(base).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    if !dossier.is_dir() {
        return None;
    }
    let mut l = Vec::new();
    parcourir(dossier, dossier, &mut l);
    l.sort_by_key(|a| a.to_lowercase());
    Some(l)
}

fn paires(v: &Value) -> Map<String, Value> {
    let mut m = Map::new();
    for p in net::liste(v) {
        if let Value::Array(a) = &p
            && a.len() == 2
        {
            m.insert(net::texte(&a[0]), a[1].clone());
        }
    }
    m
}

/// Les chemins écrits {APPLI} et {DONNEES} dans les juges, remis à ceux du dossier d'essai
fn avec_chemins(v: &Value, racine: &Path, donnees: &Path) -> Value {
    match v {
        Value::String(s) => Value::String(
            s.replace("{APPLI}", &racine.display().to_string()).replace("{DONNEES}", &donnees.display().to_string()),
        ),
        Value::Array(a) => Value::Array(a.iter().map(|x| avec_chemins(x, racine, donnees)).collect()),
        Value::Object(m) => {
            Value::Object(m.iter().map(|(k, x)| (k.clone(), avec_chemins(x, racine, donnees))).collect())
        }
        _ => v.clone(),
    }
}

fn rejouer(c: &Value, base: &Path) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(base);
    let racine = base.join("appli").join("Nevermind");
    let donnees = base.join("donnees");
    let entree = avec_chemins(&c["entree"], &racine, &donnees);
    let e = &entree;
    let s = &c["sortie"];
    creer(&racine, &net::champ_liste(e, "fichiersAppli"));
    std::fs::create_dir_all(racine.join("fichiers")).unwrap();
    if !e["fichiersDonnees"].is_null() {
        std::fs::create_dir_all(&donnees).unwrap();
        creer(&donnees, &net::champ_liste(e, "fichiersDonnees"));
    }
    let gestes: Notes = Default::default();
    let journal: Lignes = Default::default();
    let etat = &e["etat"];
    let mut pc = FauxPc {
        reg: etat["Registre"].as_object().cloned().unwrap_or_default(),
        gestes: gestes.clone(),
        journal: journal.clone(),
        plans: net::champ_liste(e, "plans")
            .iter()
            .map(|p| (net::champ_texte(p, "Guid"), net::champ_texte(p, "Nom")))
            .collect(),
        actif: net::champ_texte(etat, "PlanActif"),
        curseur: net::champ_texte(etat, "Curseur"),
        ecrans: net::champ_liste(etat, "Ecrans").iter().map(net::texte).collect(),
        bat: etat["Batterie"]
            .as_object()
            .map(|b| {
                b.iter()
                    .map(|(k, v)| (k.clone(), [net::entier(net::nombre_de(&v[0])), net::entier(net::nombre_de(&v[1]))]))
                    .collect()
            })
            .unwrap_or_default(),
        services: etat["Services"].as_object().cloned().unwrap_or_default(),
        reglages: etat["Settings"].as_object().cloned().unwrap_or_default(),
        refuse: net::champ_liste(e, "refuse").iter().map(net::texte).collect(),
        regles_pare_feu: net::champ_entier(e, "reglesPareFeu") as usize,
        cartes: net::champ_liste(e, "cartes")
            .iter()
            .map(|x| (net::champ_texte(x, "Guid"), net::champ_entier(x, "Index")))
            .collect(),
    };
    let reponse = match net::champ_texte(e, "reponse").as_str() {
        "garder" => Some(Choix::Garder),
        "supprimer" => Some(Choix::Supprimer),
        _ => None,
    };
    let mut sys = FauxSysteme {
        gestes: gestes.clone(),
        journal: journal.clone(),
        historique: net::champ_liste(e, "historique"),
        demarrage_auto: net::champ_vrai(e, "autoStart"),
        raccourci_bureau: net::champ_vrai(e, "raccourciBureau"),
        reponse,
        processus: net::champ_liste(e, "processus")
            .iter()
            .map(|p| (net::champ_entier(p, "Id") as u32, net::champ_texte(p, "Path")))
            .collect(),
        anciennes: net::champ_liste(e, "anciennesTaches").iter().map(net::texte).collect(),
        demarrage_refuse: net::champ_vrai(e, "demarrageRefuse"),
        ..Default::default()
    };
    let sv_ps = &e["sauvegarde"];
    let sv = Sauvegarde::depuis(&json!({
        "Registry": paires(&sv_ps["Registry"]), "Dns": paires(&sv_ps["Dns"]), "Displays": paires(&sv_ps["Displays"]),
        "PowerScheme": sv_ps["PowerScheme"], "CreatedScheme": sv_ps["CreatedScheme"], "Overlay": sv_ps["Overlay"],
        "Oublies": sv_ps["Oublies"],
    }));
    let probleme = e["probleme"].as_str().map(String::from);
    let options: Vec<String> = net::champ_liste(e, "options").iter().map(net::texte).collect();
    let lieux = Lieux { racine: racine.clone(), donnees: donnees.clone(), suite: None, pid: 999_999 };
    let fin = travail::desinstaller(&mut pc, &mut sys, sv, probleme.as_deref(), &lieux, choix_impose(&options));

    let chemins = [(racine.display().to_string(), "{APPLI}"), (donnees.display().to_string(), "{DONNEES}")];
    let gestes_rust = json!(*gestes.borrow());
    comparer(&s["gestes"], &gestes_rust, "gestes", &chemins)?;
    comparer(&s["question"], &json!(sys.question), "question", &chemins)?;
    comparer(&s["messages"], &json!(sys.messages), "messages", &chemins)?;
    comparer(&s["journal"], &json!(*journal.borrow()), "journal", &chemins)?;
    let mut d_ps: Option<Vec<String>> =
        if s["donnees"].is_null() { None } else { Some(net::liste(&s["donnees"]).iter().map(net::texte).collect()) };
    if let Some(l) = d_ps.as_mut() {
        l.sort_by_key(|a| a.to_lowercase());
    }
    comparer(&json!(d_ps), &json!(fichiers(&donnees)), "donnees", &chemins)?;
    comparer(&s["reglages"], &json!(pc.reglages), "reglages", &chemins)?;
    comparer(&s["registre"], &json!(pc.reg), "registre", &chemins)?;
    // (annulée : rien n'a été fait, aucun message)
    if matches!(fin, Fin::Annulee) != net::liste(&s["messages"]).is_empty() {
        return Err("annulée d'un côté seulement".into());
    }
    Ok(())
}

fn lire_juges() -> Vec<Value> {
    let f = std::env::var("NEVERMIND_JUGES_DESINST").map(PathBuf::from).unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("juges").join("desinstallation.json")
    });
    let t = std::fs::read_to_string(&f).unwrap_or_else(|e| panic!("{} illisible : {e}", f.display()));
    let v: Value = serde_json::from_str(t.trim_start_matches('\u{feff}')).expect("juges illisibles");
    net::liste(&v)
}

fn dossier_essai(nom: &str) -> PathBuf {
    std::env::temp_dir().join(format!("nevermind-desinst-{nom}-{}", std::process::id()))
}

/// « Tout optimiser puis désinstaller » : la même chose que la version PowerShell, cas par cas
#[test]
fn tout_optimiser_puis_desinstaller() {
    let cas = lire_juges();
    assert!(cas.len() >= 10, "trop peu de juges ({})", cas.len());
    let base = dossier_essai("juges");
    let mut echecs = Vec::new();
    let mut n_gestes = 0usize;
    for c in &cas {
        n_gestes += net::champ_liste(&c["sortie"], "gestes").len();
        if let Err(e) = rejouer(c, &base) {
            echecs.push(format!("cas {} : {e}", net::champ_entier(&c["entree"], "numero")));
        }
    }
    let _ = std::fs::remove_dir_all(&base);
    if !echecs.is_empty() {
        panic!(
            "{} cas sur {} diffèrent de la version PowerShell :\n\n{}",
            echecs.len(),
            cas.len(),
            echecs.iter().take(4).cloned().collect::<Vec<_>>().join("\n\n")
        );
    }
    eprintln!("{} cas, {n_gestes} gestes : tout pareil", cas.len());
}

/// La contre-épreuve : un juge changé d'un rien doit faire échouer le cas
#[test]
fn contre_epreuve() {
    let cas = lire_juges();
    let base = dossier_essai("contre");
    let mut changes = 0;
    for (i, c) in cas.iter().enumerate().take(40) {
        let mut variantes: Vec<Value> = Vec::new();
        // un geste de plus ou de moins, un mot du message, un fichier restant, une option
        let mut v = c.clone();
        if let Some(Value::Array(g)) = v["sortie"].get_mut("gestes")
            && !g.is_empty()
        {
            g.remove(i % g.len());
            variantes.push(v);
        }
        let mut v = c.clone();
        if let Some(Value::Array(m)) = v["sortie"].get_mut("messages")
            && let Some(Value::String(t)) = m.first_mut()
        {
            t.push('.');
            variantes.push(v);
        }
        let mut v = c.clone();
        let opts = net::champ_liste(&v["entree"], "options");
        let opts: Vec<String> = opts.iter().map(net::texte).collect();
        // (imposé : l'autre choix ; question posée, même avec les deux options : un choix imposé)
        v["entree"]["options"] = match choix_impose(&opts) {
            Some(Choix::Garder) => json!(["-SupprimerDonnees"]),
            _ => json!(["-GarderDonnees"]),
        };
        variantes.push(v);
        let mut v = c.clone();
        if let Some(Value::Array(l)) = v["sortie"].get_mut("donnees") {
            l.push(json!("en-trop.txt"));
            variantes.push(v);
        }
        for v in variantes {
            changes += 1;
            assert!(rejouer(&v, &base).is_err(), "cas {i} : un juge changé passe quand même");
        }
    }
    let _ = std::fs::remove_dir_all(&base);
    assert!(changes > 60, "trop peu de contre-épreuves ({changes})");
}

/// Ce que les juges ne montrent pas : Nevermind ouvert depuis ce dossier est fermé avec l'Aide (DIVERGENCES.md, point
/// 3) ; jamais ce programme lui-même, jamais un programme d'un autre dossier
#[test]
fn nevermind_ouvert_est_ferme_avec_l_aide() {
    let base = dossier_essai("ouvert");
    let _ = std::fs::remove_dir_all(&base);
    let racine = base.join("Nevermind");
    std::fs::create_dir_all(&racine).unwrap();
    let r = racine.display().to_string();
    let gestes: Notes = Default::default();
    let mut pc = FauxPc { gestes: gestes.clone(), ..Default::default() };
    let mut sys = FauxSysteme {
        gestes: gestes.clone(),
        processus: vec![
            (10, format!("{r}\\Mindset.exe")),
            (11, format!("{}\\NEVERMIND.EXE", r.to_uppercase())),
            (12, "C:\\Program Files\\Mindset\\Mindset\\Mindset.exe".into()),
            (13, format!("{r}\\NevermindAide.exe")),
            (14, format!("{r}\\Désinstaller Nevermind.exe")),
            (15, format!("{r}\\Mindset.exe")),
        ],
        ..Default::default()
    };
    let lieux = Lieux { racine: racine.clone(), donnees: base.join("donnees"), suite: None, pid: 15 };
    let _ = travail::desinstaller(&mut pc, &mut sys, Sauvegarde::default(), None, &lieux, Some(Choix::Garder));
    let fermes: Vec<i64> = gestes
        .borrow()
        .iter()
        .filter(|g| g["geste"] == "processus.fermer")
        .map(|g| g["donnees"].as_i64().unwrap())
        .collect();
    // (l'Aide d'abord, comme la version PowerShell, puis Nevermind)
    assert_eq!(fermes, vec![13, 10, 11]);
    let _ = std::fs::remove_dir_all(&base);
}

// ---------------------------------------------------------------------------
// Les conseils passés à Mindset (DIVERGENCES.md, point 15) : piloté par une option seulement, dans le dossier des
// données de la suite d'une copie gérée. Les juges n'en ont pas (le texte de fin, lui, ne change pas).
// ---------------------------------------------------------------------------
use nevermind_desinstalleur::conseils;

/// Une désinstallation d'essai : `<base>\Mindset\Programmes\Nevermind`, ses données `<base>\Mindset\Donnees\Nevermind`
struct Essai {
    base: PathBuf,
    suite: PathBuf,
    donnees: PathBuf,
    /// Des réglages à remettre comme avant (« Redémarre le PC »)
    reglages: bool,
    /// Le registre (par l'Aide) et le démarrage automatique refusés : des éléments non restaurés
    erreurs: bool,
    probleme: Option<&'static str>,
    historique: Vec<Value>,
    /// La tâche et la clé de l'Aide refusent de partir
    aide_restee: bool,
    /// Le journal de Nevermind (ce que le désinstalleur y écrit)
    journal: Lignes,
    /// Copie gérée par Mindset (sinon, pas de dossier de la suite)
    geree: bool,
}

impl Essai {
    fn nouveau(nom: &str) -> Essai {
        let base = dossier_essai(nom);
        let _ = std::fs::remove_dir_all(&base);
        let suite = base.join("Mindset").join("Donnees");
        let donnees = suite.join("Nevermind");
        std::fs::create_dir_all(&donnees).unwrap();
        std::fs::write(donnees.join("jeux.json"), "{}").unwrap();
        Essai {
            base,
            suite,
            donnees,
            reglages: false,
            erreurs: false,
            probleme: None,
            historique: Vec::new(),
            aide_restee: false,
            journal: Lignes::default(),
            geree: true,
        }
    }

    fn fichier(&self) -> PathBuf {
        self.suite.join(conseils::NOM)
    }

    fn tmp(&self) -> PathBuf {
        self.suite.join(format!("{}.tmp", conseils::NOM))
    }

    /// Les lignes du fichier des conseils (`None` : pas de fichier) ; il est en UTF-8 sans BOM
    fn lignes(&self) -> Option<Vec<String>> {
        let o = std::fs::read(self.fichier()).ok()?;
        assert!(!o.starts_with(&[0xEF, 0xBB, 0xBF]), "le fichier des conseils a un BOM");
        assert!(!self.tmp().exists(), "le fichier d'écriture est resté");
        Some(String::from_utf8(o).expect("pas de l'UTF-8").lines().map(String::from).collect())
    }

    fn lancer(&self, impose: Option<Choix>) -> (Fin, Vec<String>) {
        let racine = self.base.join("Mindset").join("Programmes").join("Nevermind");
        std::fs::create_dir_all(&racine).unwrap();
        let refuse = if self.erreurs { vec!["reg.ecrire".to_string()] } else { Vec::new() };
        let mut pc = FauxPc { refuse, ..Default::default() };
        let mut sys = FauxSysteme {
            reponse: Some(Choix::Garder),
            demarrage_refuse: self.erreurs,
            aide_restee: self.aide_restee,
            journal: self.journal.clone(),
            historique: self.historique.clone(),
            ..Default::default()
        };
        let sv = if self.reglages {
            Sauvegarde::depuis(&json!({ "Registry": { "HKLM:\\SOFTWARE\\Essai|Valeur": {
                "Path": "HKLM:\\SOFTWARE\\Essai", "Name": "Valeur", "Existed": true, "Value": 1, "Kind": "DWord" } } }))
        } else {
            Sauvegarde::default()
        };
        let lieux = Lieux {
            racine,
            donnees: self.donnees.clone(),
            suite: self.geree.then(|| self.suite.clone()),
            pid: 999_999,
        };
        let fin = travail::desinstaller(&mut pc, &mut sys, sv, self.probleme, &lieux, impose);
        (fin, sys.messages)
    }
}

impl Drop for Essai {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn definitif(quoi: &str, retour: &str) -> Value {
    json!({ "Log": [ { "Type": "definitif", "Quoi": quoi, "Retour": retour } ] })
}

/// Piloté, avec des réglages remis (redémarrer), des éléments non restaurés, une sauvegarde abîmée et un changement de
/// sécurité qui reste : une ligne par conseil, dans l'ordre de la fenêtre de fin
#[test]
fn conseils_pilote_avec_redemarrage_et_erreurs() {
    let mut e = Essai::nouveau("conseils-erreurs");
    e.reglages = true;
    e.erreurs = true;
    e.probleme = Some("abimee");
    e.historique = vec![definitif("Pare-feu renforcé", "page Sécurité")];
    let (fin, messages) = e.lancer(Some(Choix::Garder));
    let Fin::Faite { erreurs, message } = fin else { panic!("annulée") };
    assert!(erreurs.len() >= 2, "trop peu d'erreurs : {erreurs:?}");
    assert!(erreurs.iter().any(|x| x == "Démarrage automatique: Accès refusé."));
    // (le message de fin ne change pas, et il n'est donné qu'une fois)
    assert_eq!(messages, vec![message.clone()]);
    assert!(message.starts_with("Nevermind est désinstallé."));
    let mut attendu = vec![conseils::REDEMARRER.to_string()];
    attendu.extend(erreurs.iter().map(|x| format!("Certains éléments n'ont pas pu être restaurés : {x}")));
    attendu.push(format!("Attention : {}", nevermind_noyau::sauvegarde::texte_probleme(Some("abimee"))));
    attendu.push(
        "Ce changement protège ton PC et reste en place : Pare-feu renforcé (pour revenir en arrière : page Sécurité)"
            .into(),
    );
    assert_eq!(e.lignes(), Some(conseils::preparer(&attendu)));
    // (fins de ligne de Windows, une ligne par conseil)
    let t = std::fs::read_to_string(e.fichier()).unwrap();
    assert_eq!(t.matches("\r\n").count(), e.lignes().unwrap().len());
}

/// Piloté, rien à dire : pas de fichier, et un ancien fichier laissé là est effacé
#[test]
fn conseils_pilote_sans_rien_a_dire() {
    let e = Essai::nouveau("conseils-rien");
    std::fs::write(e.fichier(), "ancien conseil\r\n").unwrap();
    std::fs::write(e.tmp(), "reste").unwrap();
    let (fin, messages) = e.lancer(Some(Choix::Garder));
    assert!(matches!(fin, Fin::Faite { .. }));
    assert_eq!(messages.len(), 1);
    assert_eq!(e.lignes(), None);
    assert!(!e.tmp().exists());
    // (le dossier de la suite reste)
    assert!(e.suite.is_dir());
}

/// Sans option (la question posée, la fenêtre de fin montrée) : jamais de fichier, même avec des choses à dire, et un
/// fichier déjà là n'est pas touché (il n'est pas de cette désinstallation)
#[test]
fn conseils_jamais_sans_option() {
    let mut e = Essai::nouveau("conseils-sans-option");
    e.reglages = true;
    e.erreurs = true;
    let (_, messages) = e.lancer(None);
    assert_eq!(messages.len(), 1);
    assert_eq!(e.lignes(), None);
    std::fs::write(e.fichier(), "d'un autre\r\n").unwrap();
    let _ = e.lancer(None);
    assert_eq!(e.lignes(), Some(vec!["d'un autre".to_string()]));
}

/// Copie non gérée par Mindset : rien n'est écrit nulle part
#[test]
fn conseils_copie_non_geree() {
    let mut e = Essai::nouveau("conseils-non-geree");
    e.reglages = true;
    e.geree = false;
    let _ = e.lancer(Some(Choix::Garder));
    assert_eq!(e.lignes(), None);
    let _ = e.lancer(Some(Choix::Supprimer));
    assert_eq!(e.lignes(), None);
}

/// Trop de lignes, des lignes trop longues ou sur plusieurs lignes : au plus 10 lignes de 300 caractères
#[test]
fn conseils_coupes() {
    let mut e = Essai::nouveau("conseils-coupes");
    e.reglages = true;
    let long = "é".repeat(500);
    e.historique = (0..15).map(|i| definitif(&format!("Changement {i}\r\nsur deux lignes {long}"), "x")).collect();
    let _ = e.lancer(Some(Choix::Garder));
    let l = e.lignes().expect("pas de fichier");
    assert_eq!(l.len(), conseils::MAX_LIGNES);
    assert_eq!(l[0], conseils::REDEMARRER);
    assert!(l.iter().all(|x| x.chars().count() <= conseils::MAX_CARACTERES && !x.is_empty()));
    assert!(l[1].starts_with("Ce changement protège ton PC et reste en place : Changement 0 sur deux lignes éé"));
    assert_eq!(l[1].chars().count(), conseils::MAX_CARACTERES);
    assert!(l[9].contains("Changement 8 "));
}

/// Données supprimées : le fichier est quand même là (il est dans le dossier de la suite)
#[test]
fn conseils_donnees_supprimees() {
    let mut e = Essai::nouveau("conseils-supprimees");
    e.reglages = true;
    let (fin, _) = e.lancer(Some(Choix::Supprimer));
    assert!(matches!(fin, Fin::Faite { ref erreurs, .. } if erreurs.is_empty()));
    assert!(!e.donnees.exists(), "les données sont restées");
    assert_eq!(e.lignes(), Some(vec![conseils::REDEMARRER.to_string()]));
}

/// Un lien posé à la place du fichier des conseils est remplacé : ce qu'il vise n'est pas touché
#[cfg(windows)]
#[test]
fn conseils_un_lien_n_est_jamais_suivi() {
    let mut e = Essai::nouveau("conseils-lien");
    e.reglages = true;
    let cible = e.base.join("precieux.txt");
    std::fs::write(&cible, "ne pas toucher").unwrap();
    // (un lien symbolique de fichier demande des droits ou le mode développeur : sans, l'essai n'a pas lieu)
    if std::os::windows::fs::symlink_file(&cible, e.fichier()).is_err() {
        return;
    }
    let _ = e.lancer(Some(Choix::Garder));
    assert_eq!(std::fs::read_to_string(&cible).unwrap(), "ne pas toucher");
    assert_eq!(e.lignes(), Some(vec![conseils::REDEMARRER.to_string()]));
    assert!(!std::fs::symlink_metadata(e.fichier()).unwrap().file_type().is_symlink());
}

/// La contre-épreuve : chaque chose changée d'un rien change le fichier attendu
#[test]
fn conseils_contre_epreuve() {
    fn prete(nom: &str) -> Essai {
        let mut e = Essai::nouveau(nom);
        e.reglages = true;
        e.erreurs = true;
        e.historique = vec![definitif("A", "B")];
        e
    }
    let reference = {
        let e = prete("conseils-contre-ref");
        let _ = e.lancer(Some(Choix::Garder));
        e.lignes().expect("pas de fichier")
    };
    let mut changes = 0;
    for v in 0..5 {
        let mut e = prete(&format!("conseils-contre-{v}"));
        match v {
            0 => e.reglages = false,
            1 => e.erreurs = false,
            2 => e.historique.clear(),
            3 => e.probleme = Some("absente"),
            _ => e.geree = false,
        }
        let _ = e.lancer(Some(Choix::Garder));
        assert_ne!(e.lignes().as_ref(), Some(&reference), "variante {v} : le même fichier");
        changes += 1;
    }
    assert_eq!(changes, 5);
    // (et sans rien changer d'important : le même fichier, même avec les données supprimées)
    let e = prete("conseils-contre-pareil");
    let _ = e.lancer(Some(Choix::Supprimer));
    assert_eq!(e.lignes(), Some(reference));
}

/// Petit défaut de l'audit du 08/11 : la tâche SYSTEM de l'Aide (ou sa clé HKLM) qui n'a pas pu être retirée n'est plus
/// avalée : elle est dite à la fin (et dans les conseils passés à Mindset) et notée au journal, au lieu d'un
/// « Nevermind est désinstallé. » sans rien de plus
#[test]
fn la_tache_de_l_aide_restee_est_dite() {
    let mut e = Essai::nouveau("aide-restee");
    e.aide_restee = true;
    let (fin, messages) = e.lancer(Some(Choix::Garder));
    let Fin::Faite { erreurs, message } = fin else { panic!("annulée") };
    assert!(erreurs.iter().any(|x| x == "Tâche de l'Aide: Accès refusé."), "{erreurs:?}");
    assert!(erreurs.iter().any(|x| x == "Réglages de l'Aide: Accès refusé."), "{erreurs:?}");
    assert_eq!(messages, vec![message.clone()]);
    assert!(message.contains("Tâche de l'Aide: Accès refusé."), "{message}");
    let j = e.journal.borrow().join("\n");
    assert!(j.contains("Désinstallation : Tâche de l'Aide: Accès refusé."), "{j}");
    let l = e.lignes().unwrap_or_default();
    assert!(l.iter().any(|x| x.contains("Tâche de l'Aide: Accès refusé.")), "{l:?}");
}
