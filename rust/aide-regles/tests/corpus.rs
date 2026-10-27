//! Le corpus de cas de l'Aide (cas\aide\*.json, commun avec la version PowerShell : outils\lire-cas.ps1 et
//! outils\tester-aide*.ps1), passé aux règles Rust. Un cas se change dans le fichier, jamais ici.
//!
//! Le dossier du corpus : `..\..\cas\aide` depuis ce crate, ou la variable NEVERMIND_CAS (tests seulement : pour la
//! contre-épreuve, un corpus copié où un cas faux a été glissé doit faire échouer le test).

use aide_regles::carte::{self, FicheCarte};
use aide_regles::demandes::{self, Carte, ContexteProgrammes, Pc};
use aide_regles::demarrage::LecturesDemarrage;
use aide_regles::dossiers::Dossiers;
use aide_regles::fermeture::{self, Contexte, Fiche, Parente, Reference};
use aide_regles::{canal, client, demarrage, restauration, serveur, sid, tache, taches_compte, texte, variables};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

// ---------------------------------------------------------------------------------------------------------------
// Lecture du corpus (comme Read-Cas, outils\lire-cas.ps1)
// ---------------------------------------------------------------------------------------------------------------

fn dossier_cas() -> PathBuf {
    match std::env::var_os("NEVERMIND_CAS") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("cas").join("aide"),
    }
}

/// Remplace les valeurs du PC d'essai ({windows}, {carte}...) : un texte égal à une valeur prend sa valeur telle quelle
/// (un nombre reste un nombre), un texte qui la contient la reçoit en texte. Une valeur citée mais non donnée arrête
/// le test.
fn remplacer(v: &Value, valeurs: &[(&str, Value)], fichier: &str) -> Value {
    match v {
        Value::String(s) => {
            if let Some((_, x)) = valeurs.iter().find(|(k, _)| k == s) {
                return x.clone();
            }
            let mut r = s.clone();
            for (k, x) in valeurs {
                let t = match x {
                    Value::String(t) => t.clone(),
                    autre => autre.to_string(),
                };
                r = r.replace(k, &t);
            }
            // (« {mot} » resté tel quel : `\{[a-z][a-z-]*\}`)
            for (i, _) in r.match_indices('{') {
                let apres = &r[i + 1..];
                let n = apres.find(|c: char| !(c.is_ascii_lowercase() || c == '-')).unwrap_or(apres.len());
                if n > 0 && apres.as_bytes()[0].is_ascii_lowercase() && apres[n..].starts_with('}') {
                    panic!("cas\\aide\\{fichier} : valeur {{{}}} non donnée par le test", &apres[..n]);
                }
            }
            Value::String(r)
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| remplacer(x, valeurs, fichier)).collect()),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, x)| (k.clone(), remplacer(x, valeurs, fichier))).collect::<Map<_, _>>())
        }
        autre => autre.clone(),
    }
}

/// Le fichier du corpus, ses cas (entrée remplacée), après les contrôles de Read-Cas
fn lire_cas(fichier: &str, valeurs: &[(&str, Value)], minimum: usize) -> (Value, Vec<Value>) {
    let chemin = dossier_cas().join(fichier);
    let texte = std::fs::read_to_string(&chemin).unwrap_or_else(|e| panic!("{} illisible : {e}", chemin.display()));
    let f: Value = serde_json::from_str(texte.trim_start_matches('\u{feff}')).expect("JSON du corpus");
    assert_eq!(f["format"], 1, "cas\\aide\\{fichier} : format inconnu");
    let cas = f["cas"].as_array().cloned().unwrap_or_default();
    assert!(cas.len() >= minimum.max(1), "cas\\aide\\{fichier} : {} cas, au moins {minimum} attendus", cas.len());
    let cas = cas
        .into_iter()
        .map(|mut c| {
            assert!(
                c["nom"].as_str().is_some_and(|n| !n.is_empty()) && !c["entree"].is_null() && !c["attendu"].is_null(),
                "cas\\aide\\{fichier} : un cas sans nom, entree ou attendu ({c})"
            );
            c["entree"] = remplacer(&c["entree"], valeurs, fichier);
            c
        })
        .collect();
    (f, cas)
}

fn decrire(c: &Value) -> String {
    format!("« {} » {}", c["nom"].as_str().unwrap_or(""), c["entree"])
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// Les échecs d'un fichier, tous rapportés d'un coup
struct Bilan {
    echecs: Vec<String>,
    nb: usize,
}

impl Bilan {
    fn new() -> Self {
        Bilan { echecs: Vec::new(), nb: 0 }
    }
    fn verifier(&mut self, ok: bool, message: impl FnOnce() -> String) {
        self.nb += 1;
        if !ok {
            self.echecs.push(message());
        }
    }
    fn fin(self, sujet: &str) {
        assert!(
            self.echecs.is_empty(),
            "{sujet} : {} échec(s) sur {} :\n  - {}",
            self.echecs.len(),
            self.nb,
            self.echecs.join("\n  - ")
        );
        eprintln!("{sujet} : {} cas passés", self.nb);
    }
}

/// Les noms de comptes que Windows traduit (ceux du corpus)
fn traduire(nom: &str) -> Option<String> {
    let connus = [("SYSTEM", "S-1-5-18"), ("AUTORITE NT\\Système", "S-1-5-18"), ("NT AUTHORITY\\SYSTEM", "S-1-5-18")];
    connus.iter().find(|(n, _)| texte::egal(n, nom)).map(|(_, s)| s.to_string())
}

// ---------------------------------------------------------------------------------------------------------------
// Un faux PC pour les demandes (cas\aide\demandes.json) : rien n'est lu sur le vrai
// ---------------------------------------------------------------------------------------------------------------

const PID_ESSAI: i64 = 4321;
const CARTE_ESSAI: i32 = 12;

struct FauxPc {
    dossiers: Dossiers,
    fichiers: Vec<String>,
}

impl FauxPc {
    fn new() -> Self {
        let w = "C:\\Windows";
        let fichiers = [
            "System32\\notepad.exe",
            "explorer.exe",
            "System32\\cmd.exe",
            "System32\\svchost.exe",
            "System32\\lsass.exe",
            "System32\\MpSigStub.exe",
            "System32\\smartscreen.exe",
            "System32\\MRT.exe",
            "System32\\spoolsv.exe",
            "System32\\drivers\\etc\\hosts",
        ]
        .iter()
        .map(|f| format!("{w}\\{f}"))
        .collect();
        FauxPc {
            dossiers: Dossiers {
                windows: w.into(),
                programmes: "C:\\Program Files".into(),
                programmes_x86: "C:\\Program Files (x86)".into(),
                donnees_communes: "C:\\ProgramData".into(),
            },
            fichiers,
        }
    }

    fn valeurs(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("{windows}", Value::String(self.dossiers.windows.clone())),
            ("{bloc-notes}", Value::String(format!("{}\\System32\\notepad.exe", self.dossiers.windows))),
            ("{pid}", Value::String(PID_ESSAI.to_string())),
            ("{carte}", Value::from(CARTE_ESSAI)),
            ("{carte-connexion}", Value::from(CARTE_ESSAI)),
        ]
    }
}

impl LecturesDemarrage for FauxPc {
    fn commande_run(&self, _run32: bool, _nom: &str) -> Option<String> {
        None
    }
    fn antivirus(&self) -> Vec<String> {
        vec!["C:\\Program Files\\AntivirusTest\\avtest.exe".into()]
    }
}

impl Pc for FauxPc {
    fn dossiers(&self) -> &Dossiers {
        &self.dossiers
    }
    fn fichier_existe(&self, chemin: &str) -> bool {
        self.fichiers.iter().any(|f| texte::egal(f, chemin))
    }
    fn processus_existe(&self, pid: i64) -> bool {
        pid == PID_ESSAI
    }
    fn carte(&self, index: i32) -> Option<Carte> {
        (index == CARTE_ESSAI).then(|| Carte { index, guid: "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0".into() })
    }
    fn fiche_carte(&self, index: i32) -> Option<FicheCarte> {
        // (la carte d'essai est celle de la connexion : une vraie prise Ethernet qui porte la route par défaut)
        (index == CARTE_ESSAI).then(|| FicheCarte {
            description: "Intel(R) Ethernet Controller I226-V".into(),
            materielle: true,
            connecteur: true,
            type_carte: carte::TYPE_ETHERNET,
            support: carte::SUPPORT_802_3,
            route_defaut: true,
        })
    }
    fn dns_locaux(&self, _carte: &Carte) -> Vec<String> {
        vec!["192.168.1.1".into()]
    }
    fn dns_origine(&self, _guid: &str) -> Option<String> {
        None
    }
    fn contexte(&self) -> Result<ContexteProgrammes, String> {
        Ok(ContexteProgrammes {
            antivirus: self.antivirus(),
            services_exe: vec![
                format!("{}\\System32\\svchost.exe", self.dossiers.windows),
                format!("{}\\System32\\spoolsv.exe", self.dossiers.windows),
            ],
            services_pid: [600].into_iter().collect(),
        })
    }
    fn processus_protege(&self, _chemin: &str) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Les fichiers du corpus
// ---------------------------------------------------------------------------------------------------------------

/// Toutes les demandes de l'Aide (aide.journal à part : elle est traitée par le canal, pas par la liste)
const TOUTES_LES_DEMANDES: [&str; 31] = [
    "aide.bonjour",
    "reg.ecrire",
    "reg.effacer",
    "service.regler",
    "dns.regler",
    "carte.relancer",
    "pare-feu.bloquer",
    "pare-feu.debloquer",
    "origines.oublier",
    "pare-feu.debloquer-tout",
    "pare-feu.activer",
    "restauration.creer",
    "antivirus.retirer-exclusions",
    "lecture.pc",
    "lecture.securite",
    "fps.demarrer",
    "fps.veiller",
    "fps.lire",
    "fps.arreter",
    "trafic.tcp",
    "flux.demarrer",
    "flux.veiller",
    "flux.lire",
    "flux.arreter",
    "lecture.taches",
    "lecture.chemins",
    "wifi.manuel",
    "smb1.couper",
    "nettoyage.lister",
    "nettoyage.lancer",
    "processus.fermer",
];

#[test]
fn demandes() {
    let pc = FauxPc::new();
    let (_, cas) = lire_cas("demandes.json", &pc.valeurs(), 185);
    let mut b = Bilan::new();
    let mut actions_acceptees = HashSet::new();
    let mut actions_refusees = HashSet::new();
    for c in &cas {
        let e = &c["entree"];
        let action = s(&e["action"]);
        let r = demandes::verifier(action, &e["parametres"], &pc);
        let accepte = c["attendu"]["accepte"].as_bool().unwrap_or(false);
        if accepte {
            actions_acceptees.insert(action.to_string());
        } else {
            actions_refusees.insert(action.to_string());
        }
        match (&r, accepte) {
            (Ok(_), true) => b.verifier(true, String::new),
            (Err(m), false) => b.verifier(demandes::est_un_refus(m), || {
                format!("Refusée, mais pas par la liste de l'Aide : {} ({m})", decrire(c))
            }),
            (Ok(_), false) => b.verifier(false, || format!("Acceptée à tort : {}", decrire(c))),
            (Err(m), true) => b.verifier(false, || format!("Refusée à tort : {} ({m})", decrire(c))),
        }
    }
    // Couverture (comme tester-aide.ps1) : chaque demande a un cas accepté ; chaque demande à paramètres, un refusé
    let sans_refus = [
        "aide.bonjour",
        "origines.oublier",
        "pare-feu.debloquer-tout",
        "pare-feu.activer",
        "lecture.pc",
        "lecture.securite",
        "lecture.taches",
        "fps.lire",
        "fps.arreter",
        "trafic.tcp",
        "flux.demarrer",
        "flux.veiller",
        "flux.lire",
        "flux.arreter",
        "smb1.couper",
    ];
    for a in TOUTES_LES_DEMANDES {
        b.verifier(actions_acceptees.contains(a), || format!("Couverture : « {a} » n'a aucun cas accepté"));
        if !sans_refus.contains(&a) {
            b.verifier(actions_refusees.contains(a), || format!("Couverture : « {a} » n'a aucun cas refusé"));
        }
    }
    for a in &actions_acceptees {
        b.verifier(TOUTES_LES_DEMANDES.contains(&a.as_str()), || {
            format!("Couverture : « {a} » acceptée, mais absente de la liste du test")
        });
    }
    b.fin("cas\\aide\\demandes.json");
}

#[test]
fn client() {
    let (_, cas) = lire_cas("client.json", &[], 17);
    let mut b = Bilan::new();
    for c in &cas {
        let r = client::accepte(s(&c["entree"]["chemin"]), s(&c["entree"]["dossier"]));
        b.verifier(Some(r) == c["attendu"]["accepte"].as_bool(), || {
            format!("Client de l'Aide : {} donne {r}", decrire(c))
        });
    }
    b.fin("cas\\aide\\client.json");
}

#[test]
fn canal() {
    let (_, cas) = lire_cas("canal.json", &[], 12);
    let mut b = Bilan::new();
    for c in &cas {
        let e = &c["entree"];
        let suf = canal::suffixe(s(&e["dossier"])).unwrap_or_default();
        let nom = canal::nom_canal(s(&e["sid"]), &suf);
        let verrou = canal::nom_verrou(s(&e["sid"]), &suf);
        let a = &c["attendu"];
        b.verifier(suf == s(&a["suffixe"]) && nom == s(&a["canal"]) && verrou == s(&a["verrou"]), || {
            format!("Canal de l'Aide : {} donne {suf}, {nom}, {verrou}", decrire(c))
        });
    }
    b.fin("cas\\aide\\canal.json");
}

/// Un nombre ou rien (null)
fn nombre(v: &Value) -> Option<i64> {
    v.as_i64()
}

#[test]
fn fermeture() {
    let (f, cas) = lire_cas("fermeture.json", &[], 47);
    let mut b = Bilan::new();
    for c in &cas {
        // Le contexte et la fiche du fichier ; ceux du cas remplacent les champs qu'ils citent
        let mut x = f["contexte"].as_object().cloned().unwrap_or_default();
        if let Some(o) = c["entree"]["contexte"].as_object() {
            x.extend(o.clone());
        }
        let mut fi = f["fiche"].as_object().cloned().unwrap_or_default();
        if let Some(o) = c["entree"]["fiche"].as_object() {
            fi.extend(o.clone());
        }
        let liste = |v: &Value| -> Vec<String> {
            v.as_array().map(|a| a.iter().map(|x| s(x).to_string()).collect()).unwrap_or_default()
        };
        let ctx = Contexte {
            windows: s(&x["windows"]).into(),
            programmes: s(&x["programmes"]).into(),
            antivirus: liste(&x["antivirus"]),
            services_pid: x["servicesPid"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()),
            reference: x["reference"].as_object().map(|r| Reference {
                id: r["id"].as_i64().unwrap_or(0),
                session: nombre(&r["session"]),
                compte: s(&r["compte"]).into(),
                eleve: nombre(&r["eleve"]),
            }),
            processus: x["processus"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|p| {
                            let parente =
                                Parente { parent: p["parent"].as_i64().unwrap_or(0), chemin: s(&p["chemin"]).into() };
                            (p["pid"].as_i64().unwrap_or(0), parente)
                        })
                        .collect::<HashMap<_, _>>()
                })
                .unwrap_or_default(),
        };
        let fiche = Fiche {
            id: fi["id"].as_i64().unwrap_or(0),
            chemin: s(&fi["chemin"]).into(),
            session: nombre(&fi["session"]),
            compte: s(&fi["compte"]).into(),
            protection: nombre(&fi["protection"]),
            eleve: nombre(&fi["eleve"]),
        };
        let r = fermeture::refus(&ctx, &fiche, c["entree"]["elevePermis"].as_bool().unwrap_or(false));
        let a = &c["attendu"];
        let attendu = if a["fermable"].as_bool() == Some(true) { None } else { a["raison"].as_str() };
        b.verifier(r == attendu, || format!("Règle « fermable » : {} donne {r:?} (attendu {attendu:?})", decrire(c)));
    }
    b.fin("cas\\aide\\fermeture.json");
}

/// cas\aide\carte.json : relancer une carte réseau, seulement la carte de la connexion
#[test]
fn carte() {
    let (f, cas) = lire_cas("carte.json", &[], 30);
    let mut b = Bilan::new();
    for c in &cas {
        // La fiche du fichier ; celle du cas remplace les champs qu'elle cite
        let mut x = f["fiche"].as_object().cloned().unwrap_or_default();
        if let Some(o) = c["entree"]["fiche"].as_object() {
            x.extend(o.clone());
        }
        let fiche = FicheCarte {
            description: s(&x["description"]).into(),
            materielle: x["materielle"].as_bool().unwrap_or(false),
            connecteur: x["connecteur"].as_bool().unwrap_or(false),
            type_carte: x["type"].as_u64().unwrap_or(0) as u32,
            support: x["support"].as_u64().unwrap_or(0) as u32,
            route_defaut: x["routeDefaut"].as_bool().unwrap_or(false),
        };
        let absente = c["entree"]["absente"].as_bool() == Some(true);
        let r = carte::refus(if absente { None } else { Some(&fiche) });
        let a = &c["attendu"];
        let attendu = if a["relancable"].as_bool() == Some(true) { None } else { a["raison"].as_str() };
        b.verifier(r == attendu, || format!("Relancer une carte : {} donne {r:?} (attendu {attendu:?})", decrire(c)));
    }
    b.fin("cas\\aide\\carte.json");
}

/// Une date du corpus (« 2026-10-26T12:00:00Z ») en ticks UTC de .NET ; rien : 0
fn ticks(v: &Value) -> i64 {
    let Some(t) = v.as_str() else { return 0 };
    let n = |a: usize, b: usize| t[a..b].parse::<i64>().expect("date du corpus");
    let (an, mois, jour, h, m, sec) = (n(0, 4), n(5, 7), n(8, 10), n(11, 13), n(14, 16), n(17, 19));
    // Jours depuis le 1er janvier de l'an 1 (calendrier grégorien)
    let jours_avant = |a: i64| {
        let a = a - 1;
        a * 365 + a / 4 - a / 100 + a / 400
    };
    let bissextile = an % 4 == 0 && (an % 100 != 0 || an % 400 == 0);
    let cumul = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let mut j = jours_avant(an) + cumul[(mois - 1) as usize] + jour - 1;
    if bissextile && mois > 2 {
        j += 1;
    }
    ((j * 24 + h) * 60 + m) * 60 * 10_000_000 + sec * 10_000_000
}

#[test]
fn restauration() {
    let (_, cas) = lire_cas("restauration.json", &[], 8);
    let mut b = Bilan::new();
    for c in &cas {
        let r = restauration::recente(ticks(&c["entree"]["derniere"]), ticks(&c["entree"]["maintenant"]));
        b.verifier(Some(r) == c["attendu"]["recente"].as_bool(), || format!("Restauration : {} donne {r}", decrire(c)));
    }
    b.fin("cas\\aide\\restauration.json");
}

#[test]
fn taches_du_compte() {
    let (_, cas) = lire_cas("taches-compte.json", &[], 13);
    let mut b = Bilan::new();
    for c in &cas {
        let e = &c["entree"];
        let r = taches_compte::montree(s(&e["userId"]), s(&e["groupId"]), s(&e["sid"]), &traduire);
        b.verifier(Some(r) == c["attendu"]["montree"].as_bool(), || {
            format!("Tâches du compte : {} donne {r}", decrire(c))
        });
    }
    b.fin("cas\\aide\\taches-compte.json");
}

struct LecturesVides;
impl LecturesDemarrage for LecturesVides {
    fn commande_run(&self, _run32: bool, _nom: &str) -> Option<String> {
        panic!("le corpus donne toujours la commande : rien n'est lu dans le registre")
    }
    fn antivirus(&self) -> Vec<String> {
        panic!("le corpus donne toujours les antivirus")
    }
}

#[test]
fn demarrage_securite() {
    let (_, cas) = lire_cas("demarrage-securite.json", &[], 11);
    let d = FauxPc::new().dossiers;
    let mut b = Bilan::new();
    for c in &cas {
        let e = &c["entree"];
        let av: Vec<String> =
            e["antivirus"].as_array().map(|a| a.iter().map(|x| s(x).to_string()).collect()).unwrap_or_default();
        let r = demarrage::securite(s(&e["cle"]), s(&e["nom"]), Some(s(&e["commande"])), Some(&av), &d, &LecturesVides);
        b.verifier(Some(r) == c["attendu"]["securite"].as_bool(), || format!("Démarrage : {} donne {r}", decrire(c)));
    }
    b.fin("cas\\aide\\demarrage-securite.json");
}

#[test]
fn tache() {
    let (_, cas) = lire_cas("tache.json", &[], 14);
    let mut b = Bilan::new();
    for c in &cas {
        let e = &c["entree"];
        let r = tache::etat(s(&e["xml"]), s(&e["exe"]), s(&e["sid"]), &traduire);
        b.verifier(r == s(&c["attendu"]["etat"]), || format!("Tâche de l'Aide : {} donne « {r} »", decrire(c)));
    }
    b.fin("cas\\aide\\tache.json");
}

#[test]
fn variables() {
    let (_, cas) = lire_cas("variables.json", &[], 19);
    let mut b = Bilan::new();
    for c in &cas {
        let r = variables::dangereuse(s(&c["entree"]["nom"]));
        b.verifier(Some(r) == c["attendu"]["dangereuse"].as_bool(), || format!("Variables : {} donne {r}", decrire(c)));
    }
    b.fin("cas\\aide\\variables.json");
}

#[test]
fn serveur() {
    let (_, cas) = lire_cas("serveur.json", &[], 8);
    let mut b = Bilan::new();
    for c in &cas {
        let e = &c["entree"];
        let r = serveur::accepte(s(&e["chemin"]), s(&e["proprietaire"]), s(&e["exeAttendu"]));
        b.verifier(Some(r) == c["attendu"]["accepte"].as_bool(), || {
            format!("Serveur de l'Aide : {} donne {r}", decrire(c))
        });
    }
    b.fin("cas\\aide\\serveur.json");
}

/// Aucun fichier du corpus n'est oublié : chacun est lu par un test d'ici (environnement.json, par l'exe)
#[test]
fn tous_les_fichiers_sont_lus() {
    let lus = [
        "canal.json",
        "carte.json",
        "client.json",
        "demandes.json",
        "demarrage-securite.json",
        "environnement.json",
        "fermeture.json",
        "restauration.json",
        "serveur.json",
        "tache.json",
        "taches-compte.json",
        "variables.json",
    ];
    for f in std::fs::read_dir(dossier_cas()).expect("dossier du corpus") {
        let n = f.expect("fichier").file_name().to_string_lossy().to_string();
        if n.ends_with(".json") {
            assert!(lus.contains(&n.as_str()), "cas\\aide\\{n} n'est lu par aucun test Rust");
        }
    }
    // (le SID de la tâche, utilisé par plusieurs fichiers, a bien la forme attendue)
    assert!(sid::sid_utilisateur("S-1-5-21-1111111111-2222222222-3333333333-1001"));
}
