//! Les juges exportés de la version PowerShell (outils\exporter-juges-pc.ps1) rejoués sur le bloc Rust, avec un faux PC
//! qui se comporte exactement comme celui du script : mêmes réponses, mêmes gestes notés. Rien ne touche le vrai PC.

use nevermind_pc::analyse::{Entrees, Jeu};
use nevermind_pc::corrections::{self, Moteur, Sauvegarde};
use nevermind_pc::modele::{Analyse, Constat};
use nevermind_pc::pc::{Pc, ResultatNettoyage, ValeurReg};
use nevermind_pc::reglages::Demarrage;
use nevermind_pc::session::Session;
use nevermind_pc::temps::Moment;
use nevermind_pc::{fiche, net, pages};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Le faux PC (le même que celui de exporter-juges-pc.ps1)
// ---------------------------------------------------------------------------
const BAT: [(&str, &str); 5] = nevermind_pc::analyse::REGLAGES_BATTERIE;

#[derive(Default)]
struct FauxPc {
    reg: Map<String, Value>,
    gestes: Vec<Value>,
    plans: Vec<(String, String)>,
    actif: String,
    curseur: String,
    curseur_refuse: bool,
    ecrans: Vec<String>,
    bat: Vec<(String, [i64; 2])>,
    hiberfil: bool,
    services: Map<String, Value>,
    reglages: Map<String, Value>,
    bloques: bool,
    refuse: Vec<String>,
    reponse_question: bool,
    reponse_restauration: String,
    choix_ecran: String,
    statut: String,
    /// Le point de restauration attend ce feu (60 s au plus) : un PC lent (audit G3)
    restauration_lente: Option<std::sync::mpsc::Receiver<()>>,
    /// Les points de restauration demandés
    restaurations: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Ce que donne une nouvelle lecture du PC (« Analyser »), sans droits ; `None` : pas de relecture
    relecture: Option<nevermind_pc::lecture::Lu>,
    /// Ce que l'Aide lit (`lecture.pc` : Secure Boot, TPM, santé des disques) ; `None` : l'Aide répond « OK »
    lecture_aide: Option<Value>,
    /// La réponse de NVIDIA au dernier pilote « Game Ready » ; `None` : pas de réponse
    reponse_nvidia: Option<String>,
    /// Un lancement refusé (M8) : le texte de l'erreur
    refus_lancement: Option<String>,
}

impl FauxPc {
    fn geste(&mut self, g: &str, d: Value) {
        self.gestes.push(json!({ "geste": g, "donnees": d }));
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
            "lecture.pc" if self.lecture_aide.is_some() => return Ok(self.lecture_aide.clone().unwrap_or_default()),
            "restauration.creer" => {
                self.restaurations.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if let Some(r) = self.restauration_lente.take() {
                    let _ = r.recv_timeout(std::time::Duration::from_secs(60));
                }
                return Ok(json!(self.reponse_restauration));
            }
            "nettoyage.lancer" => {
                return Ok(
                    json!({ "Freed": 1048576.0, "Deleted": 3, "Skipped": 1, "Errors": 0, "Log": [format!("[SUPPRIMÉ] C:\\Faux\\{}\\a.tmp - 1,0 Mo", net::champ_texte(&p, "Id"))] }),
                );
            }
            _ => {}
        }
        Ok(json!("OK"))
    }
    fn relire(&mut self) -> Option<nevermind_pc::lecture::Lu> {
        self.relecture.clone()
    }
    fn reponse_pilote_nvidia(&mut self) -> Option<String> {
        self.reponse_nvidia.clone()
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
            "-duplicatescheme" => {
                let g = "c0ffee00-1111-2222-3333-444455556666";
                self.plans.push((g.into(), "Performances élevées".into()));
                return (0, format!("GUID du mode de gestion de l'alimentation : {g}  (Performances élevées)"));
            }
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
        if self.curseur_refuse {
            return 5;
        }
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
        match &self.refus_lancement {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
    fn ouvrir(&mut self, a: &str) {
        self.geste("ouvrir", json!(a));
    }
    fn commande(&mut self, exe: &str, _args: &[String]) -> i32 {
        self.geste("commande", json!(exe));
        0
    }
    fn dossier_temp(&self) -> String {
        "C:\\Faux\\Temp".into()
    }
    fn dossier_windows(&self) -> String {
        "C:\\Windows".into()
    }
    fn hiberfil(&self) -> bool {
        self.hiberfil
    }
    fn nettoyer_dossier(&mut self, id: &str, chemins: &[String], _exclus: &[String]) -> ResultatNettoyage {
        self.geste("nettoyage.dossier", json!(id));
        ResultatNettoyage {
            libere: 2048.0,
            supprimes: 1,
            laisses: 0,
            refuses: 0,
            journal: vec![format!("[SUPPRIMÉ] {}\\b.tmp - 2 Ko", chemins[0])],
        }
    }
    fn reglage(&self, nom: &str) -> Option<Value> {
        self.reglages.get(nom).cloned()
    }
    fn ecrire_reglage(&mut self, nom: &str, v: Value, exiger: bool) -> Result<(), String> {
        if exiger && self.bloques {
            return Err("Nevermind n'a pas pu noter l'état d'origine (son fichier de préférences est pris par un autre programme) : rien n'a été changé. Réessaie dans un instant.".into());
        }
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
    fn historique_ajouter(&mut self, titre: &str, details: &[String], journal: &[Value]) -> Option<String> {
        if journal.is_empty() {
            return None;
        }
        self.geste("historique", json!({ "Titre": titre, "Details": details, "Nombre": journal.len() }));
        Some("h1".into())
    }
    fn historique_annule(&mut self, id: &str) {
        self.geste("historique.annule", json!(id));
    }
    fn journal_nettoyage(&mut self, nom: &str, _l: &[String]) -> String {
        format!("DONNEES\\nettoyage\\{nom}")
    }
    fn message(&mut self, t: &str) {
        self.geste("message", json!(t));
    }
    fn question(&mut self, t: &str) -> Option<bool> {
        self.geste("question", json!(t));
        Some(self.reponse_question)
    }
    fn statut(&mut self, t: &str) {
        self.statut = t.into();
    }
    fn choix_ecran(&mut self) -> Option<String> {
        let c = self.choix_ecran.clone();
        self.geste("ecran.question", json!(c));
        Some(c)
    }
    fn maintenant(&self) -> Moment {
        Moment::lire("2026-11-06T12:00:00").unwrap()
    }
}

// ---------------------------------------------------------------------------
// Un PC simulé chargé (Use-PcSimule)
// ---------------------------------------------------------------------------
fn charger(sc: &Value) -> (FauxPc, Session, Value) {
    let mut pc = FauxPc { reponse_question: true, reponse_restauration: "OK".into(), ..Default::default() };
    if let Some(Value::Object(r)) = sc.get("registre") {
        pc.reg = r.clone();
    }
    pc.ecrans = net::champ_liste(sc, "ecrans").iter().map(net::texte).collect();
    pc.curseur = net::champ_texte(sc, "curseur");
    pc.plans = net::champ_liste(sc, "plans")
        .iter()
        .map(|p| (net::champ_texte(p, "Guid"), net::champ_texte(p, "Nom")))
        .collect();
    pc.actif = net::champ_texte(sc, "planActif");
    if let Some(Value::Object(b)) = sc.get("alimBatterie") {
        for (k, v) in b {
            pc.bat.push((k.clone(), [net::entier(net::nombre_de(&v[0])), net::entier(net::nombre_de(&v[1]))]));
        }
    }
    pc.hiberfil = net::champ_vrai(sc, "hiberfil");
    let demarrage: Vec<Demarrage> = net::champ_liste(sc, "demarrage")
        .iter()
        .map(|d| Demarrage {
            nom: net::champ_texte(d, "Nom"),
            source: net::champ_texte(d, "Source"),
            commande: net::champ_texte(d, "Commande"),
            approuve: net::champ_texte(d, "Approved"),
            nom_valeur: net::champ_texte(d, "ValueName"),
            actif: true,
            exe: net::champ_texte(d, "Exe"),
        })
        .collect();
    let smi = sc
        .get("smi")
        .and_then(|s| s.as_object())
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_f64())).collect::<HashMap<_, _>>());
    let vram = sc
        .get("vram")
        .and_then(|s| s.as_object())
        .map(|m| m.iter().map(|(k, v)| (k.clone(), net::nombre_de(v))).collect())
        .unwrap_or_default();
    let jeux = net::champ_liste(sc, "jeux")
        .iter()
        .map(|j| Jeu {
            nom: net::champ_texte(j, "Name"),
            exes: net::champ_liste(j, "Exes").iter().map(net::texte).collect(),
        })
        .collect();
    let entrees = Entrees {
        data: sc["data"].clone(),
        demarrage: Vec::new(),
        jeux,
        smi,
        vram,
        cim: sc["cim"].clone(),
        uefi: true, // (le script ne peut pas changer $env:firmware_type : la version PowerShell a lu celui du PC)
        ignores: Vec::new(),
        plan_cree: None,
    };
    let session = Session {
        entrees,
        demarrage,
        ignores: net::champ_liste(sc, "ignores").iter().filter(|x| net::vrai(x)).map(net::texte).collect(),
        sv: Sauvegarde {
            plan_cree: sc.get("creeParNevermind").filter(|v| !v.is_null()).map(net::texte),
            ..Default::default()
        },
        ..Default::default()
    };
    let live = sc.get("mesures").cloned().unwrap_or(Value::Null);
    (pc, session, live)
}

// ---------------------------------------------------------------------------
// La comparaison (au champ près ; seules différences tolérées : l'ordre des clés, 1 et 1.0, une liste d'un élément et
// cet élément, une liste vide et rien, que PowerShell confond en écrivant son JSON)
// ---------------------------------------------------------------------------
fn normal(v: &Value) -> Value {
    match v {
        Value::Array(a) if a.is_empty() => Value::Null,
        Value::Array(a) if a.len() == 1 => normal(&a[0]),
        Value::Array(a) => Value::Array(a.iter().map(normal).collect()),
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
                o.insert(k.clone(), normal(&x));
            }
            Value::Object(o)
        }
        Value::Number(n) => json!(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => {
            if let Some(i) = s.find("nevermind-juges-pc") {
                Value::String(format!("DONNEES{}", &s[i + "nevermind-juges-pc".len()..]))
            } else {
                v.clone()
            }
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
                    short(a),
                    short(b)
                ));
            }
            x.iter().zip(y).enumerate().find_map(|(i, (u, v))| ecart(u, v, &format!("{chemin}[{i}]")))
        }
        _ if a == b => None,
        _ => Some(format!("{chemin}\nPS   : {}\nRust : {}", short(a), short(b))),
    }
}
fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 900 { format!("{}...", &s[..s.char_indices().nth(900).map(|x| x.0).unwrap_or(s.len())]) } else { s }
}

fn compare(ps: &Value, rust: &Value, quoi: &str) -> Result<(), String> {
    match ecart(&normal(ps), &normal(rust), quoi) {
        None => Ok(()),
        Some(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// L'analyse dans la forme des juges (ConvertTo-JugeAnalyse)
// ---------------------------------------------------------------------------
fn constat_json(m: &mut Moteur, f: &Constat) -> Value {
    let fix = f.correction.as_ref().map(|c| {
        let ch: Vec<Value> = m.resoudre(&c.changements).unwrap_or_default().iter().map(|d| d.json()).collect();
        json!({ "Auto": c.auto, "What": c.quoi, "Why": c.pourquoi, "Steps": c.etapes, "Run": c.execution.as_ref().map(|e| e.nom()).unwrap_or(""), "RunLabel": c.libelle,
            "Confirm": c.confirmer.clone().unwrap_or_default(), "Done": c.fait.clone().unwrap_or_default(), "NoRescan": c.sans_analyse, "Open": c.ouvrir.clone().unwrap_or_default(),
            "OpenLabel": c.libelle_ouvrir, "Reboot": c.redemarrage, "Restore": c.restauration, "Changes": ch,
            "Tweak": match &c.execution { Some(nevermind_pc::modele::Execution::Reglage(id)) => id.clone(), _ => String::new() } })
    });
    json!({ "Id": f.id, "Status": f.statut, "Titre": f.titre, "Detail": f.detail, "Weight": f.poids, "Action": f.action, "ActionLabel": f.libelle_action, "Gain": f.gain, "Fix": fix })
}

fn analyse_json(m: &mut Moteur, a: &Analyse) -> Value {
    let mut info = Map::new();
    for (k, v) in &a.info {
        info.insert(k.clone(), json!(v));
    }
    let cartes: Vec<Value> = a
        .cartes
        .iter()
        .map(|c| {
            json!({ "Tag": c.tag, "Titre": c.titre, "Sous": c.sous, "Status": c.statut,
                "Bars": c.barres.iter().map(|b| json!({ "Label": b.libelle, "Value": b.valeur, "Text": b.texte, "Color": b.couleur })).collect::<Vec<_>>(),
                "Lines": c.lignes.iter().map(|l| json!({ "Cle": l.cle, "Texte": l.texte, "Couleur": l.couleur })).collect::<Vec<_>>(),
                "Notes": c.notes.iter().map(|n| json!({ "Status": n.statut, "Text": n.texte })).collect::<Vec<_>>(), "Action": c.action, "ActionLabel": c.libelle_action })
        })
        .collect();
    let actifs: Vec<Value> = a.actifs.iter().map(|f| constat_json(m, f)).collect();
    json!({ "Info": info, "Cards": cartes, "Active": actifs, "Score": a.score, "Potential": a.potentiel, "Label": a.appreciation, "Color": a.couleur, "Sante": a.sante,
        "Ignores": a.ignores.iter().map(|f| f.id.clone()).collect::<Vec<_>>(), "Portable": a.portable, "Onduleur": a.onduleur, "Build": a.build })
}

/// Lot F (reste du lot B) : pendant la lecture du démarrage de Ton PC (la lecture lente, puis ce que l'Aide y ajoute),
/// une attente est montrée dès le premier tic, puis retirée à la fin (avant : rien, seul le refus d'un clic
/// l'expliquait). La lecture est celle de l'essai, jamais celle du vrai PC.
#[test]
fn la_lecture_du_demarrage_montre_son_attente() {
    use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
    use std::time::{Duration, Instant};
    let cas = lire("corrections.json");
    let (pc, s, _) = charger(&cas[0]["entree"]);
    let lu = nevermind_pc::lecture::Lu { entrees: s.entrees.clone(), demarrage: s.demarrage.clone() };
    let (feu, attente) = std::sync::mpsc::channel::<()>();
    let mut b = nevermind_pc::BlocPc::sur_lecture_lente(Box::new(pc), move || {
        let _ = attente.recv_timeout(Duration::from_secs(10));
        lu
    });
    let d = nevermind_noyau::dossier_essai("pc-lecture-attente");
    let _ = std::fs::remove_dir_all(&d);
    let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true).partage();
    let pastilles = |e: &[Effet]| -> Vec<Value> {
        e.iter()
            .filter_map(|x| match x {
                Effet::Signal { nom, donnees } if nom == "occupe" => Some(donnees.clone()),
                _ => None,
            })
            .collect()
    };
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    assert_eq!(
        pastilles(&e.0),
        [json!({ "texte": "Lecture de ton PC...", "travail": "lecture" })],
        "pas d'attente montrée pendant la lecture"
    );
    feu.send(()).unwrap();
    let mut tout = Vec::new();
    let fin = Instant::now() + Duration::from_secs(10);
    while b.session().analyse.is_none() || b.travail_en_cours() {
        assert!(Instant::now() < fin, "la lecture ne finit pas : {tout:?}");
        std::thread::sleep(Duration::from_millis(10));
        let mut e = Effets::default();
        b.tic(&ctx, false, &mut e);
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
    }
    let lecture: Vec<Value> =
        pastilles(&tout).into_iter().filter(|p| p["travail"] == "lecture").map(|p| p["texte"].clone()).collect();
    assert_eq!(lecture, [json!("")], "l'attente n'est pas retirée une fois, à la fin de la lecture : {tout:?}");
    let _ = std::fs::remove_dir_all(&d);
}

fn lire(nom: &str) -> Vec<Value> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("juges").join(nom);
    let t = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{} : {e}", p.display()));
    serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).unwrap().as_array().cloned().unwrap()
}

/// Le juge principal de l'analyse : constats, cartes, score, pages Ton PC et Tableau de bord, fiches, Tout corriger
#[test]
fn analyses_pages_et_fiches() {
    let cas = lire("analyse.json");
    let mut echecs = Vec::new();
    let (mut n_fiches, mut n_cas) = (0, 0);
    for c in &cas {
        let num = net::champ_texte(&c["entree"], "numero");
        let (mut pc, mut s, live) = charger(&c["entree"]);
        s.analyser(&mut pc);
        let a = s.analyse.clone().unwrap();
        let attendu = &c["sortie"];
        let r = (|| -> Result<(), String> {
            let aj = {
                let mut m = Moteur::nouveau(&mut pc, &mut s.sv, a.portable);
                analyse_json(&mut m, &a)
            };
            compare(&attendu["analyse"], &aj, "analyse")?;
            let mut kit = pages::Kit::default();
            let p = pages::pages_pc(Some(&a), &live, &mut kit);
            compare(&attendu["pages"], &Value::Object(p), "pages")?;
            let (i, m) = nevermind_pc::score::a_regler(Some(&a));
            compare(
                &attendu["hub"],
                &json!({ "Important": i.iter().map(|x| x.id.clone()).collect::<Vec<_>>(), "Manual": m.iter().map(|x| x.id.clone()).collect::<Vec<_>>() }),
                "hub",
            )?;
            let fiches = net::champ_liste(attendu, "fiches");
            if attendu["fiches"].is_array() && !attendu["fiches"].as_array().unwrap().is_empty() {
                let fiches_ps = attendu["fiches"].as_array().unwrap();
                if fiches_ps.len() != kit.constats.len() {
                    return Err(format!(
                        "{} fiches en PowerShell, {} constats en Rust",
                        fiches_ps.len(),
                        kit.constats.len()
                    ));
                }
                for (k, f) in kit.constats.iter().enumerate() {
                    s.ouvrir(&mut pc, f);
                    let page = fiche::page(s.fiche.as_ref().unwrap(), None);
                    compare(&fiches_ps[k], &page, &format!("fiche {k} ({})", f.id))?;
                }
            }
            let _ = fiches;
            s.fiche = None;
            s.ouvrir_tout(&mut pc);
            let tout = s.fiche.as_ref().map(|f| fiche::page(f, None)).unwrap_or(Value::Null);
            compare(&attendu["toutCorriger"], &tout, "toutCorriger")?;
            compare(&attendu["gestes"], &json!(pc.gestes), "gestes de l'analyse (lecture seule)")?;
            Ok(())
        })();
        n_cas += 1;
        n_fiches += attendu["fiches"].as_array().map(|x| x.len()).unwrap_or(0);
        if let Err(e) = r {
            echecs.push(format!("PC simulé {num} : {e}"));
        }
    }
    if !echecs.is_empty() {
        panic!(
            "{} cas sur {} en échec. Premiers :\n{}",
            echecs.len(),
            n_cas,
            echecs.iter().take(4).cloned().collect::<Vec<_>>().join("\n\n")
        );
    }
    eprintln!("{n_cas} PC simulés, {n_fiches} fiches : tout pareil");
}

/// « Tout corriger » puis « Revenir en arrière », et « Tout corriger » puis « Tout restaurer » : la suite exacte des
/// demandes à l'Aide et des gestes sur le PC, et ce qui est noté (sauvegarde, réglages), contre une fausse Aide
#[test]
fn tout_corriger_revenir_et_tout_restaurer() {
    let cas = lire("corrections.json");
    let mut echecs = Vec::new();
    let mut n_gestes = 0usize;
    for c in &cas {
        let v = net::champ_entier(c, "variante");
        let attendu = &c["sortie"];
        let variante = |pc: &mut FauxPc| {
            pc.choix_ecran = ["keep", "revert", "timeout"][(v % 3) as usize].into();
            match v % 7 {
                1 => pc.reponse_restauration = "recent".into(),
                2 => {
                    pc.refuse = vec!["restauration.creer".into()];
                    pc.reponse_question = false;
                }
                3 => pc.refuse = vec!["restauration.creer".into()],
                4 => pc.refuse = vec!["reg.ecrire".into()],
                5 => pc.curseur_refuse = true,
                6 => pc.bloques = true,
                _ => {}
            }
        };
        let etat = |pc: &FauxPc, s: &Session| {
            json!({ "Backup": s.sv.json(), "Settings": pc.reglages, "Ignores": s.ignores, "Registre": pc.reg, "PlanActif": pc.actif, "Curseur": pc.curseur, "Ecrans": pc.ecrans,
                "Batterie": pc.bat.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<Map<_, _>>(), "Services": pc.services })
        };
        let r = (|| -> Result<(), String> {
            // a. Tout corriger, puis Revenir en arrière
            let (mut pc, mut s, _) = charger(&c["entree"]);
            variante(&mut pc);
            s.analyser(&mut pc);
            pc.gestes.clear();
            s.ouvrir_tout(&mut pc);
            compare(
                &attendu["ficheTout"],
                &s.fiche.as_ref().map(|f| fiche::page(f, None)).unwrap_or(Value::Null),
                "ficheTout",
            )?;
            compare(
                &attendu["items"],
                &json!(
                    s.fiche
                        .as_ref()
                        .map(|f| f.items.iter().map(|x| x.id.clone()).collect::<Vec<_>>())
                        .unwrap_or_default()
                ),
                "items",
            )?;
            if s.fiche.is_some() {
                s.executer(&mut pc);
                let co = &attendu["corriger"];
                n_gestes += pc.gestes.len();
                compare(&co["gestes"], &json!(pc.gestes), "corriger.gestes")?;
                pc.gestes.clear();
                let resultat = s.fiche.as_ref().filter(|f| f.mode == "result").is_some();
                if resultat {
                    compare(
                        &co["journal"],
                        &json!(s.journal_resultat.clone().unwrap_or_default()),
                        "corriger.journal",
                    )?;
                }
                compare(&co["etat"], &etat(&pc, &s), "corriger.etat")?;
                compare(&co["score"], &json!(s.analyse.as_ref().map(|a| a.score)), "corriger.score")?;
                compare(
                    &co["fiche"],
                    &s.fiche.as_ref().map(|f| fiche::page(f, None)).unwrap_or(Value::Null),
                    "corriger.fiche",
                )?;
                compare(&co["statut"], &json!(pc.statut), "corriger.statut")?;
                if resultat {
                    s.bouton_ignorer(&mut pc);
                    let an = &attendu["annuler"];
                    n_gestes += pc.gestes.len();
                    compare(&an["gestes"], &json!(pc.gestes), "annuler.gestes")?;
                    compare(&an["etat"], &etat(&pc, &s), "annuler.etat")?;
                    compare(&an["score"], &json!(s.analyse.as_ref().map(|a| a.score)), "annuler.score")?;
                    compare(
                        &an["fiche"],
                        &s.fiche.as_ref().map(|f| fiche::page(f, None)).unwrap_or(Value::Null),
                        "annuler.fiche",
                    )?;
                } else if !attendu["annuler"].is_null() {
                    return Err("PowerShell a pu revenir en arrière, pas Rust".into());
                }
            }
            // b. Tout corriger, puis Tout restaurer
            let (mut pc, mut s, _) = charger(&c["entree"]);
            variante(&mut pc);
            s.analyser(&mut pc);
            s.ouvrir_tout(&mut pc);
            if s.fiche.is_some() {
                s.executer(&mut pc);
                pc.gestes.clear();
                pc.refuse = if v % 5 == 2 {
                    vec!["service.regler".into()]
                } else if v % 5 == 3 {
                    vec!["origines.oublier".into()]
                } else {
                    vec![]
                };
                let mut erreurs = {
                    let portable = s.analyse.as_ref().is_some_and(|a| a.portable);
                    let mut m = Moteur::nouveau(&mut pc, &mut s.sv, portable);
                    corrections::tout_restaurer(&mut m)
                };
                let re = &attendu["restaurer"];
                n_gestes += pc.gestes.len();
                // (l'ordre de la sauvegarde : celui d'une table de hachage de .NET en PowerShell, sans ordre voulu ; ici
                // celui où les réglages ont été notés. Les gestes sont comparés sans leur ordre.)
                let trie = |v: &Value| {
                    let mut l: Vec<String> = net::liste(v).iter().map(|x| normal(x).to_string()).collect();
                    l.sort();
                    json!(l)
                };
                compare(&trie(&re["gestes"]), &trie(&json!(pc.gestes)), "restaurer.gestes")?;
                let mut e_ps: Vec<String> = net::liste(&re["erreurs"])
                    .iter()
                    .flat_map(net::liste)
                    .map(|x| net::texte(&x))
                    .filter(|x| !x.is_empty())
                    .collect();
                e_ps.sort();
                erreurs.sort();
                compare(&json!(e_ps), &json!(erreurs), "restaurer.erreurs")?;
                compare(&re["etat"], &etat(&pc, &s), "restaurer.etat")?;
                compare(&re["compte"], &json!(corrections::compte_total(&s.sv, &pc)), "restaurer.compte")?;
            }
            Ok(())
        })();
        if let Err(e) = r {
            echecs.push(format!("PC simulé {} (variante {v}) : {e}", net::champ_texte(&c["entree"], "numero")));
        }
    }
    if !echecs.is_empty() {
        panic!(
            "{} cas sur {} en échec. Premiers :\n{}",
            echecs.len(),
            cas.len(),
            echecs.iter().take(4).cloned().collect::<Vec<_>>().join("\n\n")
        );
    }
    eprintln!("{} PC simulés, {n_gestes} gestes : tout pareil", cas.len());
}

/// Le bloc entier sur un faux PC : pages, « Tout corriger », Exécuter, puis « Revenir en arrière » ; jamais le vrai PC
#[test]
fn le_bloc_sur_un_faux_pc() {
    use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
    let cas = lire("corrections.json");
    let c = cas.iter().find(|c| !c["sortie"]["annuler"].is_null()).expect("un cas avec retour en arrière");
    let (pc, s, _) = charger(&c["entree"]);
    let lu = nevermind_pc::lecture::Lu { entrees: s.entrees.clone(), demarrage: s.demarrage.clone() };
    let mut b = nevermind_pc::BlocPc::sur(Box::new(pc), lu);
    let d = nevermind_noyau::dossier_essai("pc-bloc");
    let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true);
    let p = b.pages(&ctx, "pc");
    assert!(p.contains_key("pc") && p.contains_key("tableau") && p.contains_key("tests"));
    let mut e = Effets::default();
    assert!(b.action(&ctx, "toutcorriger", &Value::Null, &mut e));
    assert!(e.0.contains(&Effet::OuvrirEcran("fiche".into())), "la fiche s'ouvre");
    assert!(
        e.0.iter().any(|x| matches!(x, Effet::Signal { nom, .. } if nom == "reglages-jeu")),
        "les réglages sont envoyés au bloc perf"
    );
    let _ = b.pages(&ctx, "fiche");
    let mut e = Effets::default();
    assert!(b.action(&ctx, "fiche:run", &Value::Null, &mut e));
    assert_eq!(b.session().fiche.as_ref().map(|f| f.mode.as_str()), Some("result"));
    let mut e = Effets::default();
    assert!(b.action(&ctx, "fiche:ignore", &Value::Null, &mut e));
    assert!(b.session().journal_resultat.is_none(), "revenu en arrière");
    let _ = std::fs::remove_dir_all(&d);
}

/// Audit G3 : « Tout corriger » commence par un point de restauration, qui peut attendre l'Aide jusqu'à 10 min. Le
/// geste part dans un travail de fond : le clic rend la main tout de suite, les pages répondent, un deuxième clic ne
/// refait rien, et à la fin tout est exactement comme quand le geste était fait sur place (mêmes effets, même fiche,
/// même sauvegarde, mêmes gestes sur le PC).
#[test]
fn tout_corriger_ne_fige_pas_pendant_le_point_de_restauration() {
    use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
    use std::time::{Duration, Instant};
    let cas = lire("corrections.json");
    let c = cas.iter().find(|c| !c["sortie"]["annuler"].is_null()).expect("un cas avec retour en arrière");
    // Le geste joué de bout en bout ; `partage` : le contexte de Nevermind (travaux de fond), sinon sur place
    let jouer = |partage: bool| {
        let (mut pc, s, _) = charger(&c["entree"]);
        let (feu, attente) = std::sync::mpsc::channel::<()>();
        let restaurations = pc.restaurations.clone();
        if partage {
            pc.restauration_lente = Some(attente);
        }
        let lu = nevermind_pc::lecture::Lu { entrees: s.entrees.clone(), demarrage: s.demarrage.clone() };
        let mut b = nevermind_pc::BlocPc::sur(Box::new(pc), lu);
        let d = nevermind_noyau::dossier_essai(&format!("pc-fond-{partage}"));
        let _ = std::fs::remove_dir_all(&d);
        let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true);
        let ctx = if partage { ctx.partage() } else { std::sync::Arc::new(ctx) };
        let _ = b.pages(&ctx, "pc");
        let mut tout = Vec::new();
        let mut e = Effets::default();
        assert!(b.action(&ctx, "toutcorriger", &Value::Null, &mut e));
        tout.extend(e.0);
        let _ = b.pages(&ctx, "fiche");
        let mut e = Effets::default();
        let debut = Instant::now();
        assert!(b.action(&ctx, "fiche:run", &Value::Null, &mut e));
        let duree = debut.elapsed();
        tout.extend(e.0);
        if partage {
            assert!(duree < Duration::from_secs(2), "« Exécuter » a pris {duree:?} : le fil des blocs était figé");
            assert!(b.travail_en_cours(), "le geste n'est pas parti en fond");
            // Pendant le point de restauration : les pages répondent, un deuxième clic ne refait rien et le dit (lot B ;
            // avant, il était avalé sans un mot)
            let fin = Instant::now() + Duration::from_secs(10);
            while restaurations.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                assert!(Instant::now() < fin, "le point de restauration n'est pas demandé");
                std::thread::sleep(Duration::from_millis(10));
            }
            let debut = Instant::now();
            let p = b.pages(&ctx, "fiche");
            assert!(p.contains_key("fiche") && debut.elapsed() < Duration::from_secs(1), "la page ne répond pas");
            let mut e = Effets::default();
            assert!(b.action(&ctx, "fiche:run", &Value::Null, &mut e), "le deuxième clic est pris (sans rien faire)");
            assert_eq!(
                e.0,
                [Effet::Annonce("Attends la fin de « Tout corriger » (Ton PC)".into()), Effet::Rafraichir],
                "le deuxième clic n'est pas dit en clair"
            );
            feu.send(()).unwrap();
            while b.travail_en_cours() {
                assert!(Instant::now() < fin, "le geste ne finit pas");
                std::thread::sleep(Duration::from_millis(10));
                let mut e = Effets::default();
                b.travaux_finis(&ctx, &mut e);
                tout.extend(e.0);
            }
        }
        assert_eq!(restaurations.load(std::sync::atomic::Ordering::SeqCst), 1, "un seul point de restauration");
        let r = (
            tout.into_iter().filter(|x| !matches!(x, Effet::Rafraichir)).collect::<Vec<_>>(),
            b.session().fiche.clone(),
            b.session().sv.clone(),
            b.session().journal_resultat.clone(),
        );
        let _ = std::fs::remove_dir_all(&d);
        r
    };
    let sur_place = jouer(false);
    let en_fond = jouer(true);
    assert_eq!(sur_place.1.as_ref().map(|f| f.mode.as_str()), Some("result"));
    assert_eq!(en_fond.0, sur_place.0, "pas les mêmes effets");
    assert_eq!(en_fond.1, sur_place.1, "pas la même fiche");
    assert_eq!(en_fond.2, sur_place.2, "pas la même sauvegarde");
    assert_eq!(en_fond.3, sur_place.3, "pas le même journal du résultat");
}

/// Audit G3 : dans un travail de fond, le vrai PC envoie son texte d'attente tout de suite (« Création du point de
/// restauration... » avant la réponse de l'Aide) ; la fin du geste rend le reste comme avant (pastille retirée, dernier
/// texte en message). Avec une fausse Aide (mode test) : rien ne touche le vrai PC.
#[test]
fn le_texte_d_attente_du_vrai_pc_part_tout_de_suite() {
    use nevermind_noyau::{Contexte, Effet};
    let d = nevermind_noyau::dossier_essai("pc-direct");
    let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true);
    let vus = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let v = vus.clone();
    let envoyes = std::sync::Mutex::new(Vec::<Effet>::new());
    ctx.fausse_aide(std::sync::Arc::new(move |a: &str, _p: &Value| {
        v.lock().unwrap().push(a.to_string());
        Ok(json!("OK"))
    }));
    let envoi = |e: Effet| envoyes.lock().unwrap().push(e);
    // (lot A : « Appliquer » tient le verrou des gestes ; il le rend pendant le point de restauration)
    let _geste = ctx.prendre_geste("Appliquer", "Ton PC").unwrap();
    let mut p = nevermind_pc::systeme::PcWindows::nouveau(&ctx).en_direct(&envoi);
    let mut sv = Sauvegarde::default();
    let r = {
        let mut m = Moteur::nouveau(&mut p, &mut sv, false);
        m.point_de_restauration()
    };
    assert_eq!(r, Ok(Some(true)));
    assert!(vus.lock().unwrap().contains(&"restauration.creer".to_string()), "la fausse Aide est appelée");
    let occupe = |t: &str| Effet::Signal { nom: "occupe".into(), donnees: json!({ "texte": t }) };
    assert_eq!(
        envoyes.lock().unwrap().clone(),
        vec![
            occupe("Création du point de restauration (ça peut prendre une minute)..."),
            occupe("Point de restauration créé.")
        ],
        "les textes d'attente partent tout de suite, dans l'ordre"
    );
    let a = p.annonces();
    assert!(a.contains(&occupe("")), "la pastille est retirée à la fin : {a:?}");
    assert!(a.contains(&Effet::Annonce("Point de restauration créé.".into())));
    assert!(!a.iter().any(|x| *x == occupe("Point de restauration créé.")), "un texte d'attente en double : {a:?}");
    let _ = std::fs::remove_dir_all(&d);
}

/// Fin de la lecture du PC (le fil de lecture a fini, aucun clic) : Secure Boot, TPM et santé des disques sont
/// complétés par l'Aide sans jamais proposer la demande de Windows (audit G5, `Get-AideLecture`).
#[test]
fn lecture_du_pc_completee_sans_demande_de_windows() {
    use nevermind_noyau::Contexte;
    let d = nevermind_noyau::dossier_essai("pc-lecture-aide");
    let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true);
    ctx.fausse_aide(std::sync::Arc::new(|a: &str, _p: &Value| {
        assert_eq!(a, "lecture.pc");
        Ok(json!({ "SecureBoot": true, "TpmOk": false, "Fiabilite": {} }))
    }));
    let mut p = nevermind_pc::systeme::PcWindows::nouveau(&ctx);
    let mut e = Entrees { data: json!({ "SecureBoot": null, "Disks": [] }), ..Default::default() };
    nevermind_pc::systeme::completer_par_aide(&mut p, &mut e);
    assert_eq!(e.data["SecureBoot"], json!(true), "la lecture de l'Aide est prise");
    assert_eq!(ctx.aide_demandes_relevees(), Vec::<String>::new(), "aucune demande de Windows pour une lecture");
}

/// Audit M10 (point 4) : la version PowerShell complète la lecture par l'Aide à chaque analyse (`Invoke-Analysis`
/// appelle `Add-AideLecturePc`, analyse.ps1:195). « Analyser » relit le PC : Secure Boot, TPM et santé des disques
/// (lus seulement par l'Aide) doivent rester, et l'analyse doit être la même quand rien n'a changé sur le PC.
#[test]
fn analyser_garde_secure_boot_tpm_et_sante_des_disques() {
    use nevermind_noyau::{Bloc, Contexte, Effets};
    let cas = lire("corrections.json");
    let c = cas
        .iter()
        .find(|c| {
            let d = &c["entree"]["data"];
            !d["SecureBoot"].is_null() && net::liste(&d["Disks"]).iter().any(|x| net::champ_vrai(x, "Rel"))
        })
        .expect("un cas avec Secure Boot et la santé d'un disque");
    let (mut pc, s, _) = charger(&c["entree"]);
    let complet = s.entrees.data.clone();
    // Ce que l'Aide lit : Secure Boot, la puce TPM et la santé de chaque disque
    let mut fiabilite = Map::new();
    for dd in net::liste(&complet["Disks"]) {
        if net::champ_vrai(&dd, "Rel") {
            fiabilite.insert(net::champ_texte(&dd["Disk"], "DeviceId"), dd["Rel"].clone());
        }
    }
    pc.lecture_aide = Some(json!({ "SecureBoot": complet["SecureBoot"], "TpmOk": true,
        "Tpm": { "SpecVersion": "2.0, 0, 1.59" }, "Fiabilite": fiabilite }));
    // Ce que le PC donne sans droits (la lecture lente, au début et à chaque « Analyser ») : rien de tout ça
    let mut sans_droits = s.entrees.clone();
    sans_droits.data["SecureBoot"] = Value::Null;
    sans_droits.data["TpmOk"] = json!(false);
    sans_droits.data["Tpm"] = Value::Null;
    if let Some(Value::Array(l)) = sans_droits.data.get_mut("Disks") {
        for dd in l {
            dd["Rel"] = Value::Null;
        }
    }
    pc.relecture = Some(nevermind_pc::lecture::Lu { entrees: sans_droits.clone(), demarrage: s.demarrage.clone() });
    // La première lecture, complétée par l'Aide comme au démarrage de Nevermind
    let mut premiere = sans_droits.clone();
    nevermind_pc::systeme::completer_par_aide(&mut pc, &mut premiere);
    assert!(!premiere.data["SecureBoot"].is_null(), "la fausse Aide ne complète pas la première lecture");
    let mut b = nevermind_pc::BlocPc::sur(
        Box::new(pc),
        nevermind_pc::lecture::Lu { entrees: premiere.clone(), demarrage: s.demarrage.clone() },
    );
    let d = nevermind_noyau::dossier_essai("pc-relecture-aide");
    let _ = std::fs::remove_dir_all(&d);
    let ctx = std::sync::Arc::new(Contexte::nouveau(d.clone(), d.clone(), false, true));
    let _ = b.pages(&ctx, "pc");
    let avant = b.session().analyse.clone().expect("la première analyse");
    for _ in 0..2 {
        let mut e = Effets::default();
        assert!(b.action(&ctx, "analyser", &Value::Null, &mut e));
        let fin = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while b.travail_en_cours() {
            assert!(std::time::Instant::now() < fin, "l'analyse ne finit pas");
            std::thread::sleep(std::time::Duration::from_millis(10));
            b.travaux_finis(&ctx, &mut Effets::default());
        }
        let _ = b.pages(&ctx, "pc");
        let data = &b.session().entrees.data;
        assert_eq!(data["SecureBoot"], premiere.data["SecureBoot"], "Secure Boot a disparu après « Analyser »");
        assert_eq!(data["TpmOk"], json!(true), "la puce TPM a disparu après « Analyser »");
        assert_eq!(data["Disks"], premiere.data["Disks"], "la santé des disques a disparu après « Analyser »");
        let apres = b.session().analyse.clone().expect("l'analyse refaite");
        assert_eq!(apres.score, avant.score, "le score change alors que rien n'a changé sur le PC");
        assert_eq!(apres, avant, "l'analyse change alors que rien n'a changé sur le PC");
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// M5 (audit du 08/11) : sauvegarde.json est modifiable sans droits, et « Tout restaurer » tourne en administrateur
/// dans le désinstalleur. Une sauvegarde trafiquée ne fait écrire ni effacer dans HKCU que les réglages que Nevermind
/// change lui-même (liste fermée) : jamais une stratégie (Policies), jamais une autre valeur. Contre le faux PC.
#[test]
fn une_sauvegarde_trafiquee_ne_pose_aucune_strategie() {
    let entree = |p: &str, n: &str, existait: bool, v: Value, k: &str| {
        (
            format!("{p}|{n}"),
            json!({ "Path": p, "Name": n, "Existed": existait, "Value": v, "Kind": if existait { json!(k) } else { Value::Null } }),
        )
    };
    let mut reg = Map::new();
    for (k, v) in [
        // Le seul vrai réglage de Nevermind
        entree(r"HKCU:\Software\Microsoft\GameBar", "AutoGameModeEnabled", true, json!(0), "DWord"),
        // Ajoutés par un programme sans droits
        entree(
            r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Policies\System",
            "DisableTaskMgr",
            true,
            json!(1),
            "DWord",
        ),
        entree(
            r"Registry::HKEY_CURRENT_USER\Software\Policies\Microsoft\Windows\System",
            "DisableCMD",
            true,
            json!(1),
            "DWord",
        ),
        entree(r"HKCU:\Software\Policies\Microsoft\Windows\Explorer", "NoWinKeys", true, json!(1), "DWord"),
        entree(r"HKCU:\\Software\Policies\Microsoft\Windows Defender", "DisableAntiSpyware", true, json!(1), "DWord"),
        entree(r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Run", "Piege", true, json!(r"C:\x.exe"), "String"),
        entree(r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Hidden", false, Value::Null, ""),
    ] {
        reg.insert(k, v);
    }
    let mut sv = Sauvegarde::depuis(&json!({ "Registry": reg }));
    let mut pc = FauxPc::default();
    let erreurs = {
        let mut m = Moteur::nouveau(&mut pc, &mut sv, false);
        corrections::tout_restaurer(&mut m)
    };
    let ecrits: Vec<String> = pc
        .gestes
        .iter()
        .filter(|g| net::champ_texte(g, "geste").starts_with("reg.utilisateur"))
        .map(|g| format!("{}|{}", net::champ_texte(&g["donnees"], "Cle"), net::champ_texte(&g["donnees"], "Nom")))
        .collect();
    assert_eq!(ecrits, vec![r"HKCU:\Software\Microsoft\GameBar|AutoGameModeEnabled".to_string()], "{:?}", pc.gestes);
    assert!(pc.reg.keys().all(|k| !k.to_ascii_lowercase().contains("policies")), "{:?}", pc.reg);
    assert_eq!(erreurs.len(), 6, "{erreurs:?}");
    // Le vrai réglage est oublié (remis), les autres restent notés et refusés
    assert_eq!(sv.registre.len(), 6);
}

/// M8 (audit du 08/11) : « Test complet Windows » (mdsched.exe, qui demande les droits administrateur) ne se lançait
/// pas, l'erreur était jetée et rien n'était dit. Maintenant il passe par ShellExecute (la question de Windows), dans
/// un travail de fond, et un refus est dit. Contre le faux PC : rien n'est lancé pour de vrai.
#[test]
fn un_programme_de_windows_qui_ne_s_ouvre_pas_le_dit() {
    use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
    use std::time::{Duration, Instant};
    fn bouton(v: &Value, texte: &str) -> Option<String> {
        match v {
            Value::Object(o) => {
                if o.get("texte").and_then(Value::as_str) == Some(texte)
                    && let Some(id) = o.get("id").and_then(Value::as_str)
                {
                    return Some(id.to_string());
                }
                o.values().find_map(|x| bouton(x, texte))
            }
            Value::Array(a) => a.iter().find_map(|x| bouton(x, texte)),
            _ => None,
        }
    }
    let cas = lire("corrections.json");
    for partage in [false, true] {
        let (mut pc, s, _) = charger(&cas[0]["entree"]);
        pc.refus_lancement = Some("tu as répondu « Non » à la demande de Windows".into());
        let lu = nevermind_pc::lecture::Lu { entrees: s.entrees.clone(), demarrage: s.demarrage.clone() };
        let mut b = nevermind_pc::BlocPc::sur(Box::new(pc), lu);
        let d = nevermind_noyau::dossier_essai(&format!("pc-mdsched-{partage}"));
        let _ = std::fs::remove_dir_all(&d);
        let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true);
        let ctx = if partage { ctx.partage() } else { std::sync::Arc::new(ctx) };
        let p = b.pages(&ctx, "tests");
        let id = bouton(&p["tests"], "Test complet Windows").expect("le bouton « Test complet Windows »");
        let mut e = Effets::default();
        assert!(b.action(&ctx, &id, &Value::Null, &mut e));
        let mut tout = e.0;
        let fin = Instant::now() + Duration::from_secs(10);
        let attendu = Effet::Annonce(
            "Le test de la mémoire de Windows ne s'est pas ouvert : tu as répondu « Non » à la demande de Windows."
                .into(),
        );
        while !tout.contains(&attendu) && Instant::now() < fin {
            let mut e = Effets::default();
            b.travaux_finis(&ctx, &mut e);
            tout.extend(e.0);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(tout.contains(&attendu), "le refus n'est pas dit : {tout:?}");
        let pc = b.faux_pc().expect("le faux PC");
        let _ = pc;
        let _ = std::fs::remove_dir_all(&d);
    }
    // « Tester la mémoire » d'une correction (« Tout corriger », Exécuter) : l'échec remonte, pas de faux « réussi »
    let mut pc = FauxPc { refus_lancement: Some("Accès refusé.".into()), ..Default::default() };
    let mut sv = Sauvegarde::default();
    let r = {
        let mut m = Moteur::nouveau(&mut pc, &mut sv, false);
        nevermind_pc::session::executer_correction(
            &mut m,
            Some(&nevermind_pc::modele::Execution::TestMemoire),
            &nevermind_pc::modele::Changements::Aucun,
        )
    };
    assert_eq!(r, Err("Le test de la mémoire de Windows ne s'est pas ouvert : Accès refusé.".to_string()));
    assert_eq!(pc.gestes, vec![json!({ "geste": "lancer", "donnees": ["mdsched.exe"] })]);
}

/// M9 (audit du 08/11) : une sauvegarde abîmée était lue sans rien dire, et « Tout restaurer » répondait « Il n'y a
/// aucun changement à annuler ». Comme Get-TexteProblemeSauvegarde (onglets.ps1:870-873) : le problème est dit, avec
/// le point de restauration de Windows ; et la question de « Tout restaurer » prévient aussi.
#[test]
fn une_sauvegarde_abimee_est_dite_comme_telle() {
    use nevermind_noyau::Contexte;
    let d = nevermind_noyau::dossier_essai("pc-sauvegarde-abimee");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("sauvegarde.json"), "\u{feff}{ \"Registry\": { \"HKLM:\\SOFT").unwrap();
    let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true);
    let sv = nevermind_pc::systeme::lire_sauvegarde(&ctx);
    assert_eq!(sv.probleme.as_deref(), Some("abimee"), "le problème de la sauvegarde n'est pas noté");
    let texte = nevermind_noyau::sauvegarde::texte_probleme(Some("abimee"));
    // Rien à remettre : le problème est dit, pas « rien à annuler »
    let mut pc = FauxPc::default();
    let mut s =
        Session { sv: Sauvegarde { probleme: Some("abimee".into()), ..Default::default() }, ..Default::default() };
    s.restaurer_tout(&mut pc, &[]);
    assert_eq!(pc.gestes, vec![json!({ "geste": "message", "donnees": texte })]);
    // Des réglages à remettre : la question prévient
    let mut pc = FauxPc::default();
    let mut s = Session {
        sv: Sauvegarde { probleme: Some("abimee".into()), plan: Some("guid".into()), ..Default::default() },
        ..Default::default()
    };
    s.restaurer_tout(&mut pc, &[]);
    let q = pc.gestes.iter().find(|g| g["geste"] == "question").expect("la question");
    assert!(q["donnees"].as_str().unwrap().ends_with(&format!("\n\nAttention : {texte}")), "{q}");
    // Une sauvegarde lisible : rien de dit en plus
    let mut pc = FauxPc::default();
    let mut s = Session::default();
    s.restaurer_tout(&mut pc, &[]);
    assert_eq!(pc.gestes, vec![json!({ "geste": "message", "donnees": "Il n'y a aucun changement à annuler." })]);
    let _ = std::fs::remove_dir_all(&d);
}

/// La réponse de NVIDIA le 08/11/2026 (raccourcie) : le dernier pilote « Game Ready » est le 621.14
const REPONSE_NVIDIA: &str = r#"{ "Success" : "1", "IDS" : [{ "downloadInfo": { "Success" : "1", "ID" : "283968", "Name" : "GeForce%20Game%20Ready%20Driver", "Release" : "620", "Version" : "621.14", "IsBeta" : "0", "IsWHQL" : "1", "ReleaseDateTime" : "Wed Nov 04, 2026", "DetailsURL" : "https://www.nvidia.com/en-us/drivers/details/283968/", "DownloadURL" : "https://us.download.nvidia.com/Windows/621.14/621.14-desktop-win10-win11-64bit-international-dch-whql.exe" } }] }"#;

/// La réponse de NVIDIA lue comme `$r.NvLatest` ; une réponse illisible ou sans numéro n'en donne pas
#[test]
fn reponse_nvidia_lue() {
    use nevermind_pc::systeme::pilote_nvidia_depuis_reponse as lire_reponse;
    assert_eq!(
        lire_reponse(REPONSE_NVIDIA),
        Some(
            json!({ "Version": "621.14", "Date": "Wed Nov 04, 2026", "Url": "https://www.nvidia.com/en-us/drivers/details/283968/" })
        )
    );
    assert_eq!(lire_reponse(""), None);
    assert_eq!(lire_reponse("<html>Connexion au Wi-Fi de la gare</html>"), None);
    assert_eq!(lire_reponse(r#"{ "Success" : "0", "IDS" : [] }"#), None);
    assert_eq!(lire_reponse(r#"{ "IDS" : [{ "downloadInfo": { "Version" : "" } }] }"#), None);
    // (une adresse qui n'est pas chez NVIDIA n'est jamais ouverte : la page des pilotes à la place)
    let autre =
        REPONSE_NVIDIA.replace("https://www.nvidia.com/en-us/drivers/details/283968/", "https://ailleurs.example/");
    assert_eq!(lire_reponse(&autre).unwrap()["Url"], json!("https://www.nvidia.com/fr-fr/drivers/"));
}

/// Le PC de Jordan le 08/11/2026 : GeForce RTX 4080, pilote 617.42 du 30/09/2026 (moins de 6 mois), le 621.14 sorti
/// le 04/11. La version PowerShell demandait le dernier pilote à NVIDIA (réglage « Vérifier le pilote NVIDIA au
/// lancement », mis par défaut) ; la version Rust ne le demandait jamais et disait « Pilote graphique à jour ».
#[test]
fn pilote_nvidia_ancien_alors_que_recent() {
    use nevermind_noyau::{Bloc, Contexte, Effets};
    let data = json!({ "GPUs": [{ "Name": "NVIDIA GeForce RTX 4080", "DriverVersion": "32.0.16.1742", "DriverDate": "2026-09-30T00:00:00" }], "NvLatest": null });
    let titres = |reglage: Option<bool>| {
        let lu = nevermind_pc::lecture::Lu {
            entrees: Entrees { data: data.clone(), ..Default::default() },
            demarrage: Vec::new(),
        };
        let mut pc = FauxPc {
            reponse_question: true,
            reponse_nvidia: Some(REPONSE_NVIDIA.into()),
            relecture: Some(lu.clone()),
            ..Default::default()
        };
        if let Some(r) = reglage {
            pc.reglages.insert("PiloteNvidia".into(), json!(r));
        }
        let mut b = nevermind_pc::BlocPc::sur(Box::new(pc), lu);
        let d = nevermind_noyau::dossier_essai("pc-pilote-nvidia");
        let _ = std::fs::remove_dir_all(&d);
        let ctx = std::sync::Arc::new(Contexte::nouveau(d.clone(), d.clone(), false, true));
        let _ = b.pages(&ctx, "pc");
        let mut e = Effets::default();
        assert!(b.action(&ctx, "analyser", &Value::Null, &mut e));
        let fin = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while b.travail_en_cours() {
            assert!(std::time::Instant::now() < fin, "l'analyse ne finit pas");
            std::thread::sleep(std::time::Duration::from_millis(10));
            b.travaux_finis(&ctx, &mut Effets::default());
        }
        let _ = b.pages(&ctx, "pc");
        let a = b.session().analyse.clone().expect("l'analyse");
        let _ = std::fs::remove_dir_all(&d);
        a.actifs.iter().filter(|c| c.id.starts_with("gpu-driver")).map(|c| c.titre.clone()).collect::<Vec<_>>()
    };
    let attendu = vec!["Nouveau pilote NVIDIA disponible (621.14)".to_string()];
    assert_eq!(titres(None), attendu, "réglage par défaut : NVIDIA est demandé");
    assert_eq!(titres(Some(true)), attendu);
    // Réglage coupé : seulement l'âge du pilote (moins de 6 mois : à jour), comme la version PowerShell
    assert_eq!(titres(Some(false)), vec!["Pilote graphique à jour (NVIDIA GeForce RTX 4080)".to_string()]);
}

/// Lot B du second audit : « Analyser » du Nettoyage passe par l'Aide (jusqu'à 2 min par catégorie) dans un travail de
/// fond ; son attente est montrée pendant le travail, puis retirée (avant : aucun texte d'attente).
#[test]
fn analyser_le_nettoyage_montre_son_attente() {
    use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
    use std::time::{Duration, Instant};
    let cas = lire("corrections.json");
    let (pc, s, _) = charger(&cas[0]["entree"]);
    let lu = nevermind_pc::lecture::Lu { entrees: s.entrees.clone(), demarrage: s.demarrage.clone() };
    let mut b = nevermind_pc::BlocPc::sur(Box::new(pc), lu);
    let d = nevermind_noyau::dossier_essai("pc-nettoyage-attente");
    let _ = std::fs::remove_dir_all(&d);
    let ctx = Contexte::nouveau(d.clone(), d.clone(), false, true).partage();
    let _ = b.pages(&ctx, "pc");
    let mut tout = Vec::new();
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnCleanScan", &Value::Null, &mut e));
    tout.extend(e.0);
    let fin = Instant::now() + Duration::from_secs(10);
    while b.travail_en_cours() {
        assert!(Instant::now() < fin, "l'analyse du nettoyage ne finit pas");
        std::thread::sleep(Duration::from_millis(10));
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
    }
    let pastilles: Vec<Value> = tout
        .iter()
        .filter_map(|x| match x {
            Effet::Signal { nom, donnees } if nom == "occupe" => Some(donnees["texte"].clone()),
            _ => None,
        })
        .collect();
    assert_eq!(pastilles.first(), Some(&json!("Recherche des fichiers à nettoyer...")), "{pastilles:?}");
    assert_eq!(pastilles.last(), Some(&json!("")), "l'attente reste à l'écran : {pastilles:?}");
    let _ = std::fs::remove_dir_all(&d);
}
