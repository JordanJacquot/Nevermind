//! Les juges du noyau : les cas exportés de la version PowerShell (outils\exporter-juges-noyau.ps1, qui fait foi),
//! rejoués par la version Rust. Un cas se change en relançant l'export, jamais ici.
//!
//! Le dossier des juges : tests\juges, ou la variable NEVERMIND_JUGES (contre-épreuve : un juge copié où un octet a
//! été changé doit faire échouer le test).
//!
//! Rien ne touche le PC : chaque cas a son dossier temporaire ; l'Aide est une fausse Aide sur le canal de la copie
//! d'essai (un nom que la vraie Aide n'écoute jamais) ; le réseau est un faux transport ou un faux serveur sur
//! 127.0.0.1.

use nevermind_noyau::assistance::{self, Masque};
use nevermind_noyau::mises_a_jour::{self, FinInstallation, Transport};
use nevermind_noyau::moment::HeureLocale;
use nevermind_noyau::windows::canal::{FauxServeur, Repondre};
use nevermind_noyau::{Contexte, aide, json_ps};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ---------------------------------------------------------------------------------------------------------------
// Outils
// ---------------------------------------------------------------------------------------------------------------

fn juge(nom: &str) -> Value {
    let d = match std::env::var_os("NEVERMIND_JUGES") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("juges"),
    };
    let t = std::fs::read_to_string(d.join(nom)).unwrap_or_else(|e| panic!("juge {nom} illisible : {e}"));
    let v: Value = serde_json::from_str(t.trim_start_matches('\u{feff}')).expect("JSON du juge");
    assert_eq!(v["format"], 1, "{nom} : format inconnu");
    v
}

fn liste(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.clone(),
        Value::Null => Vec::new(),
        autre => vec![autre.clone()],
    }
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

fn b64(t: &str) -> Vec<u8> {
    let table = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let mut o = Vec::new();
    let (mut acc, mut n) = (0u32, 0);
    for c in t.bytes() {
        let Some(v) = table(c) else { continue };
        acc = (acc << 6) | v;
        n += 6;
        if n >= 8 {
            n -= 8;
            o.push((acc >> n) as u8);
            acc &= (1 << n) - 1;
        }
    }
    o
}

fn b64_vers(o: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut r = String::new();
    for c in o.chunks(3) {
        let n =
            (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                r.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                r.push('=');
            }
        }
    }
    r
}

/// Un dossier d'essai neuf
fn dossier(nom: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nevermind-juges-{}-{nom}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn poser_fichiers(d: &Path, f: &Value) {
    let _ = std::fs::remove_dir_all(d);
    std::fs::create_dir_all(d).unwrap();
    if let Value::Object(m) = f {
        for (k, v) in m {
            std::fs::write(d.join(k), b64(&s(v))).unwrap();
        }
    }
}

fn fichiers_de(d: &Path) -> Map<String, Value> {
    let mut l: Vec<(String, Vec<u8>)> = std::fs::read_dir(d)
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap()))
        .collect();
    l.sort_by_key(|x| x.0.to_lowercase());
    l.into_iter().map(|(k, v)| (k, json!(b64_vers(&v)))).collect()
}

const MOMENT: HeureLocale = HeureLocale { annee: 2026, mois: 11, jour: 6, heure: 21, minute: 5, seconde: 9 };

fn contexte(d: &Path) -> Contexte {
    let c = Contexte::nouveau(d.join("appli"), d.to_path_buf(), false, true);
    c.figer_horloge(MOMENT);
    c
}

/// Les lignes du journal (sans la date), le dossier remplacé par « <dossier> »
fn journal_de(d: &Path) -> Vec<String> {
    let Ok(t) = std::fs::read_to_string(d.join("journal.txt")) else { return Vec::new() };
    let dd = d.to_string_lossy().into_owned();
    t.lines().map(|l| l.split_once(' ').map_or(l, |x| x.1).replace(&dd, "<dossier>")).collect()
}

/// Une ligne du journal comparée à celle de PowerShell : le message d'erreur de .NET (ou de JSON) n'est pas repris
/// mot pour mot, seulement ce qui l'entoure
fn meme_ligne(rust: &str, ps: &str) -> bool {
    if rust == ps {
        return true;
    }
    if let (Some(r), Some(p)) = (rust.strip_prefix("Sauvegarde abîmée ("), ps.strip_prefix("Sauvegarde abîmée (")) {
        return r.ends_with(") : mise de côté") && p.ends_with(") : mise de côté");
    }
    for debut in ["Fichier abîmé (", "Fichier illisible pour l'instant (", "Fichier abîmé pas mis de côté ("] {
        if let (Some(r), Some(p)) = (rust.strip_prefix(debut), ps.strip_prefix(debut)) {
            let coupe = |t: &str| t.split(" : ").next().unwrap_or("").to_string();
            let fin = |t: &str| t.rsplit_once(", mis de côté : ").map(|x| x.1.to_string());
            return coupe(r) == coupe(p) && fin(r) == fin(p);
        }
    }
    if let (Some(_), Some(p)) = (
        rust.strip_prefix("Écriture des préférences impossible: "),
        ps.strip_prefix("Écriture des préférences impossible: "),
    ) {
        return p.starts_with("Exception lors de l'appel de");
    }
    false
}

fn memes_journaux(rust: &[String], ps: &[Value], cas: &str) {
    let ps: Vec<String> = ps.iter().map(s).collect();
    assert_eq!(rust.len(), ps.len(), "{cas} : journal\nRust : {rust:#?}\nPowerShell : {ps:#?}");
    for (r, p) in rust.iter().zip(&ps) {
        assert!(meme_ligne(r, p), "{cas} : ligne du journal\nRust : {r}\nPowerShell : {p}");
    }
}

/// `$v = <fonction>` en PowerShell : une liste d'un seul élément arrive comme cet élément, une liste vide comme rien
fn comme_affectation(v: Value) -> Value {
    match v {
        Value::Array(a) if a.len() == 1 => a.into_iter().next().unwrap(),
        Value::Array(a) if a.is_empty() => Value::Null,
        autre => autre,
    }
}

/// Ce que le juge a noté d'une valeur (`ConvertTo-Json -Compress`, ou `{}` pour rien)
fn valeur_juge(v: &Value) -> Value {
    match v {
        Value::String(t) => serde_json::from_str(t).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// 1. ConvertTo-Json
// ---------------------------------------------------------------------------------------------------------------

#[test]
fn json_comme_convertto_json() {
    let j = juge("json.json");
    let cas = liste(&j["cas"]);
    assert!(cas.len() >= 300);
    for c in &cas {
        // (« null » relu par ConvertFrom-Json ne donne rien du tout : ConvertTo-Json n'écrit alors rien)
        if s(&c["entree"]) == "null" {
            continue;
        }
        let v: Value = serde_json::from_str(&s(&c["entree"])).unwrap();
        assert_eq!(json_ps::lisible(&v), s(&c["lisible"]), "forme lisible de {}", s(&c["entree"]));
        assert_eq!(json_ps::compact(&v), s(&c["compact"]), "forme compacte de {}", s(&c["entree"]));
    }
}

// ---------------------------------------------------------------------------------------------------------------
// 2. Fichiers sûrs
// ---------------------------------------------------------------------------------------------------------------

fn prendre(p: &Path) -> std::fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).share_mode(0).open(p).unwrap()
}

#[test]
fn fichiers_surs() {
    let j = juge("fichiers.json");
    let d = dossier("fichiers");
    for c in liste(&j["cas"]) {
        let nom = s(&c["nom"]);
        poser_fichiers(&d, &c["avant"]);
        let ctx = contexte(&d.join("..").join(format!("nevermind-juges-{}-fichiers-ctx", std::process::id())));
        let p = d.join("x.json");
        if s(&c["operation"]) == "lire" {
            let verrou = c["pris"].as_bool().unwrap_or(false).then(|| prendre(&p));
            let r = ctx.lire_json(&p);
            let ecrit = verrou.is_some().then(|| ctx.ecrire_json(&p, &json!({ "a": 1 }), false, false));
            drop(verrou);
            let attendu = valeur_juge(&c["resultat"]);
            let r = r
                .map(|v| if c["liste"].as_bool() == Some(true) { Value::Array(liste(&v)) } else { v })
                .unwrap_or(Value::Null);
            assert_eq!(r, attendu, "{nom} : valeur lue");
            if let Some(e) = ecrit {
                match c["ecriture"].as_str() {
                    Some("ok") | None => assert!(e.is_ok(), "{nom} : écriture"),
                    Some(m) if m.contains("n'a pas pu être lu") => {
                        assert_eq!(e, Err(m.to_string()), "{nom} : écriture")
                    }
                    Some(_) => assert!(e.is_err(), "{nom} : écriture refusée"),
                }
            }
        } else {
            let v: Value = serde_json::from_str(&s(&c["valeur"])).unwrap();
            ctx.ecrire_json(&p, &v, c["compact"].as_bool().unwrap_or(false), c["bom"].as_bool().unwrap_or(false))
                .unwrap();
        }
        assert_eq!(Value::Object(fichiers_de(&d)), c["apres"], "{nom} : fichiers après");
        let journal: Vec<String> = journal_de(ctx.donnees.as_path())
            .iter()
            .map(|l| l.replace(&d.to_string_lossy().into_owned(), "<dossier>"))
            .collect();
        memes_journaux(&journal, &liste(&c["journal"]), &nom);
        let _ = std::fs::remove_dir_all(&ctx.donnees);
    }
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------------------------------------------
// 3. Réglages
// ---------------------------------------------------------------------------------------------------------------

#[test]
fn reglages() {
    let j = juge("reglages.json");
    let d = dossier("reglages");
    for c in liste(&j["cas"]) {
        let nom = s(&c["nom"]);
        poser_fichiers(&d, &c["avant"]);
        let ctx = contexte(&d);
        let f = d.join("parametres.json");
        let mut verrou = None;
        for o in liste(&c["operations"]) {
            match s(&o["op"]).as_str() {
                "lire" => {
                    let defaut: Value = serde_json::from_str(&s(&o["defaut"])).unwrap();
                    let v = comme_affectation(ctx.reglage_ou(&s(&o["nom"]), defaut));
                    assert_eq!(v, valeur_juge(&o["valeur"]), "{nom} : lecture de {}", s(&o["nom"]));
                }
                "ecrire" => {
                    let v: Value = serde_json::from_str(&s(&o["valeur"])).unwrap();
                    let r = ctx.ecrire_reglage(&s(&o["nom"]), v, o["exiger"].as_bool().unwrap_or(false));
                    assert_eq!(
                        r.err(),
                        o["erreur"].as_str().map(str::to_string),
                        "{nom} : écriture de {}",
                        s(&o["nom"])
                    );
                }
                "prendre" => verrou = Some(prendre(&f)),
                _ => verrou = None,
            }
        }
        drop(verrou);
        let texte = std::fs::read_to_string(&f).ok();
        match c["parametres"].as_str() {
            None => assert!(texte.is_none(), "{nom} : parametres.json ne devrait pas exister"),
            Some(p) => {
                let t = texte.unwrap_or_default();
                let (vr, vp): (Value, Value) = (serde_json::from_str(&t).unwrap(), serde_json::from_str(p).unwrap());
                // (le contenu est le même ; l'ordre des clés d'une table PowerShell est celui de ses cases de hachage)
                assert_eq!(vr, vp, "{nom} : parametres.json\nRust : {t}\nPowerShell : {p}");
                if t != p {
                    assert_eq!(t, json_ps::lisible(&vr), "{nom} : forme de parametres.json");
                }
            }
        }
        let mut noms: Vec<String> = fichiers_de(&d).keys().filter(|k| *k != "journal.txt").cloned().collect();
        noms.sort();
        let mut attendus: Vec<String> = liste(&c["fichiers"]).iter().map(s).collect();
        attendus.sort();
        assert_eq!(noms, attendus, "{nom} : fichiers");
        memes_journaux(&journal_de(&d), &liste(&c["journal"]), &nom);
    }
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------------------------------------------
// 4. Journal
// ---------------------------------------------------------------------------------------------------------------

fn empreinte(p: &Path) -> Option<String> {
    let o = std::fs::read(p).ok()?;
    Some(aide_regles::sha256::empreinte(&o).iter().map(|b| format!("{b:02X}")).collect())
}

#[test]
fn journal_raccourci_et_logerror() {
    let j = juge("journal.json");
    let d = dossier("journal");
    for c in liste(&j["cas"]) {
        let (n, t) = (c["lignes"].as_u64().unwrap() as usize, c["longueur"].as_u64().unwrap() as usize);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut texte = String::new();
        for i in 0..n {
            texte.push_str(&format!("2026-11-06T21:05:09 ligne {i} é {}\r\n", "x".repeat(t - 30)));
        }
        std::fs::write(d.join("journal.txt"), &texte).unwrap();
        assert_eq!(texte.len() as u64, c["taille"].as_u64().unwrap(), "{} : taille", s(&c["nom"]));
        let _ctx = Contexte::nouveau(d.join("appli"), d.clone(), false, true);
        for f in ["journal.txt", "journal-ancien.txt"] {
            assert_eq!(
                empreinte(&d.join(f)),
                c["empreintes"][f].as_str().map(str::to_string),
                "{} : {f}",
                s(&c["nom"])
            );
        }
    }
    // LogError : un journal qui ne peut pas être écrit, noté une seule fois
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("journal.txt")).unwrap();
    let ctx = contexte(&d);
    ctx.journal("première");
    let le1 = s(&ctx.reglage("LogError").unwrap_or_default());
    ctx.journal("deuxième");
    ctx.oublier_reglages();
    let le2 = s(&ctx.reglage("LogError").unwrap_or_default());
    let attendu = s(&j["logError"]["LogError"]);
    let debut = "2026-11-06T21:05:09 MethodInvocationException: ";
    assert!(attendu.starts_with(debut) && le1.starts_with(debut), "LogError : {le1} / {attendu}");
    assert_eq!(le1, le2, "LogError n'est noté qu'une fois");
    assert_eq!(s(&j["logError"]["apresDeuxieme"]), attendu);
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------------------------------------------
// 5. L'Aide
// ---------------------------------------------------------------------------------------------------------------

#[test]
fn demandes_sur_le_canal() {
    let j = juge("aide.json");
    for c in liste(&j["demandes"]) {
        let p: Value = serde_json::from_str(&s(&c["parametres"])).unwrap();
        assert_eq!(aide::demande(&s(&c["action"]), &p), s(&c["demande"]));
    }
    for c in liste(&j["variables"]) {
        let noms: Vec<String> = liste(&c["noms"]).iter().map(s).collect();
        let attendu: Vec<String> = liste(&c["dangereuses"]).iter().map(s).collect();
        assert_eq!(aide::variables_dangereuses_de(&noms), attendu, "variables {noms:?}");
    }
    for c in liste(&j["refusFps"]) {
        assert_eq!(aide::fps_refus(&s(&c["texte"])), c["refus"].as_bool().unwrap(), "refus FPS « {} »", s(&c["texte"]));
    }
}

#[test]
fn veille_des_fps() {
    let j = juge("aide.json");
    let v = &j["veille"];
    let d = dossier("veille");
    std::fs::write(d.join("parametres.json"), r#"{"FpsVeille":["Jeu.EXE","bf6.exe",""]}"#).unwrap();
    let ctx = contexte(&d);
    let avant = ctx.fps_veille_jeux();
    ctx.ajouter_fps_veille("Autre");
    ctx.ajouter_fps_veille("JEU.exe");
    assert_eq!(json!(avant), v["avant"]);
    assert_eq!(json!(ctx.fps_veille_jeux()), v["apres"]);
    assert_eq!(json_ps::compact(&ctx.reglage("FpsVeille").unwrap()), s(&v["reglage"]));
    memes_journaux(&journal_de(&d), &liste(&v["journal"]), "veille des FPS");
    let _ = std::fs::remove_dir_all(&d);
}

/// Invoke-Aide contre une fausse Aide sur le vrai canal (celui de la copie d'essai)
#[test]
fn invoke_aide_fausse_aide() {
    let j = juge("aide.json");
    let moi = std::env::current_exe().unwrap().to_string_lossy().into_owned();
    for (i, c) in liste(&j["cas"]).iter().enumerate() {
        let nom = s(&c["nom"]);
        let d = dossier(&format!("aide-{i}"));
        let ctx = contexte(&d);
        ctx.regler_aide_essai(Some(moi.clone()), None);
        let canal = ctx.aide_config().canal.clone();
        let recues = Arc::new(std::sync::Mutex::new(Vec::new()));
        for e in liste(&c["etapes"]) {
            let serveur = if e["ecoute"].as_bool() == Some(true) {
                let rep = s(&e["reponse"]);
                let err = s(&e["erreurAide"]);
                let r: Repondre = Arc::new(move |_l: &str| match err.as_str() {
                    "" => Some(rep.clone()),
                    m if m.contains("à temps") => None,
                    _ => Some(String::new()),
                });
                Some(FauxServeur::lancer(&canal, r).unwrap())
            } else {
                None
            };
            let p: Value = serde_json::from_str(&s(&e["parametres"])).unwrap();
            let r = ctx.aide(&s(&e["action"]), &p, 300);
            if let Some(sv) = serveur {
                recues.lock().unwrap().extend(sv.demandes());
            }
            match (&r, e["erreur"].as_str()) {
                (Ok(v), None) => {
                    assert_eq!(comme_affectation(v.clone()), valeur_juge(&e["resultat"]), "{nom} : réponse")
                }
                (Err(m), Some(a)) => assert_eq!(m, a, "{nom} : erreur"),
                _ => panic!("{nom} : {r:?} au lieu de {:?}", e["erreur"]),
            }
        }
        assert_eq!(json!(*recues.lock().unwrap()), c["recues"], "{nom} : demandes reçues par l'Aide");
        memes_journaux(&journal_de(&d), &liste(&c["journal"]), &nom);
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// La fausse Aide de haut niveau (pour les blocs) : jamais un canal hors du mode test
#[test]
fn fausse_aide_seulement_en_test() {
    let d = dossier("vraie");
    let ctx = Contexte::nouveau(d.join("appli"), d.clone(), false, false);
    assert!(nevermind_noyau::fausse_aide::lancer(&ctx, |_, _| None).is_err());
    let ctx = contexte(&d);
    ctx.fausse_aide(Arc::new(|a, p| if a == "x" { Ok(json!({ "vu": p })) } else { Err("non".into()) }));
    assert_eq!(ctx.aide("x", &json!(1), 10).unwrap(), json!({ "vu": 1 }));
    assert_eq!(ctx.aide("y", &json!(1), 10), Err("non".into()));
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------------------------------------------
// 6. Mises à jour
// ---------------------------------------------------------------------------------------------------------------

struct FauxReseau(HashMap<String, Vec<u8>>);

impl Transport for FauxReseau {
    fn obtenir(&self, url: &str, _delai_s: u32) -> Result<(u16, Vec<u8>), String> {
        Ok(match self.0.get(url) {
            Some(o) => (200, o.clone()),
            None => (404, Vec::new()),
        })
    }
}

fn reseau(reponses: &Value) -> FauxReseau {
    let mut m = HashMap::new();
    if let Value::Object(o) = reponses {
        for (k, v) in o {
            m.insert(k.clone(), s(v).into_bytes());
        }
    }
    FauxReseau(m)
}

#[test]
fn versions_et_publications() {
    let j = juge("maj.json");
    let comp = liste(&j["comparaisons"]);
    assert!(comp.len() >= 400);
    for c in &comp {
        assert_eq!(
            mises_a_jour::plus_recente(&s(&c["distante"]), &s(&c["locale"])),
            c["plus"].as_bool().unwrap(),
            "[version]'{}' -gt [version]'{}'",
            s(&c["distante"]),
            s(&c["locale"])
        );
    }
    for p in liste(&j["publications"]) {
        let t = reseau(&p["reponses"]);
        let r = mises_a_jour::chercher_version(
            &t,
            mises_a_jour::API,
            mises_a_jour::DEPOT,
            p["beta"].as_bool().unwrap(),
            p["ancien"].as_bool().unwrap(),
        );
        let a = &p["resultat"];
        match r {
            Err(e) => {
                let attendu = s(&a["Error"]);
                assert!(!attendu.is_empty(), "{} : erreur {e}", s(&p["nom"]));
                if !attendu.starts_with("Le serveur distant") {
                    assert_eq!(e, attendu, "{}", s(&p["nom"]));
                }
            }
            Ok(v) => {
                assert_eq!(v.version, s(&a["Version"]), "{}", s(&p["nom"]));
                assert_eq!(v.url, s(&a["Url"]), "{}", s(&p["nom"]));
                assert_eq!(v.sig_url, s(&a["SigUrl"]), "{}", s(&p["nom"]));
                assert_eq!(v.beta, a["Beta"].as_bool().unwrap(), "{}", s(&p["nom"]));
                assert_eq!(v.notes, s(&a["Notes"]), "{}", s(&p["nom"]));
            }
        }
    }
}

#[test]
fn verification_des_mises_a_jour() {
    let j = juge("maj.json");
    for (i, c) in liste(&j["cas"]).iter().enumerate() {
        let nom = s(&c["nom"]);
        let d = dossier(&format!("maj-{i}"));
        std::fs::write(d.join("parametres.json"), s(&c["reglages"])).unwrap();
        let ctx = Contexte::nouveau(d.join("appli"), d.clone(), c["geree"].as_bool().unwrap(), true);
        ctx.figer_horloge(MOMENT);
        ctx.definir_version(&s(&c["version"]));
        let t = reseau(&c["reponses"]);
        let r = ctx.verifier_mise_a_jour_de(c["manuel"].as_bool().unwrap(), &t, mises_a_jour::API, mises_a_jour::DEPOT);
        let e = ctx.etat_mise_a_jour();
        assert_eq!(e.statut, s(&c["statut"]), "{nom} : statut");
        assert_eq!(e.bandeau, s(&c["bandeau"]), "{nom} : bandeau");
        assert_eq!(
            if e.mettre_a_jour { "Mettre à jour" } else { "Vérifier les mises à jour" },
            s(&c["bouton"]),
            "{nom} : bouton"
        );
        assert_eq!(e.en_attente.map(|p| p.version), c["enAttente"].as_str().map(str::to_string), "{nom} : en attente");
        let mut messages = Vec::new();
        if let Some((m, avert)) = r.message {
            messages.push(format!("{}|{m}", if avert { "Warning" } else { "" }));
        }
        if let Some(st) = r.statut {
            messages.push(format!("statut|{st}"));
        }
        assert_eq!(json!(messages), c["messages"], "{nom} : messages");
        let jr = journal_de(&d);
        let jp: Vec<String> = liste(&c["journal"]).iter().map(s).collect();
        assert_eq!(jr.len(), jp.len(), "{nom} : journal {jr:?} / {jp:?}");
        for (a, b) in jr.iter().zip(&jp) {
            assert!(a.starts_with("Mise à jour: ") && b.starts_with("Mise à jour: "), "{nom} : {a} / {b}");
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}

/// Un vrai serveur HTTP sur 127.0.0.1 (le test seulement), pour passer par WinHTTP
fn serveur_local(
    fichiers: HashMap<String, Vec<u8>>,
) -> (String, std::thread::JoinHandle<()>, Arc<std::sync::atomic::AtomicBool>) {
    use std::io::{Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.set_nonblocking(true).unwrap();
    let adresse = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    let fin = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let f2 = fin.clone();
    let fil = std::thread::spawn(move || {
        while !f2.load(std::sync::atomic::Ordering::SeqCst) {
            let Ok((mut c, _)) = l.accept() else {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            };
            c.set_nonblocking(false).unwrap();
            let mut b = [0u8; 4096];
            let n = c.read(&mut b).unwrap_or(0);
            let t = String::from_utf8_lossy(&b[..n]);
            let chemin = t.split_whitespace().nth(1).unwrap_or("/").to_string();
            let (code, corps) = match fichiers.get(&chemin) {
                Some(o) => ("200 OK", o.clone()),
                None => ("404 Not Found", Vec::new()),
            };
            let _ = write!(c, "HTTP/1.1 {code}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", corps.len());
            let _ = c.write_all(&corps);
        }
    });
    (adresse, fil, fin)
}

#[test]
fn installation_signee() {
    let j = juge("maj.json");
    let ins = &j["installation"];
    let zip = b64(&s(&ins["zip"]));
    let sig = b64(&s(&ins["signature"]));
    let d = dossier("installation");
    let cles = d.join("cles");
    std::fs::create_dir_all(&cles).unwrap();
    std::fs::write(cles.join("essai.xml"), s(&ins["cle"])).unwrap();
    let mut abime = zip.clone();
    let n = abime.len();
    abime[n / 2] ^= 0x55;
    let mut fichiers = HashMap::new();
    fichiers.insert("/dl/Nevermind.zip".to_string(), zip.clone());
    fichiers.insert("/dl/Nevermind.zip.sig".to_string(), sig.clone());
    fichiers.insert("/dl/abime.zip".to_string(), abime);
    fichiers.insert("/dl/abime.zip.sig".to_string(), sig.clone());
    let (adresse, fil, fin) = serveur_local(fichiers);
    let essai = |nom: &str, zipu: &str, sigu: &str| -> (FinInstallation, PathBuf, Vec<String>) {
        let racine = d.join(nom);
        std::fs::create_dir_all(racine.join("fichiers")).unwrap();
        std::fs::write(racine.join("fichiers").join("ancien.ps1"), "# ancien").unwrap();
        std::fs::write(racine.join("Nevermind.exe"), "vieil exe").unwrap();
        let donnees = racine.join("donnees");
        let ctx = Contexte::nouveau(racine.join("fichiers"), donnees.clone(), false, true);
        ctx.figer_horloge(MOMENT);
        ctx.definir_version("1.15.6");
        let mut rep = HashMap::new();
        let sigj = if sigu.is_empty() {
            String::new()
        } else {
            format!(r#",{{"name":"Nevermind.zip.sig","browser_download_url":"{adresse}{sigu}"}}"#)
        };
        rep.insert(
            format!("{}/repos/{}/releases/latest", mises_a_jour::API, mises_a_jour::DEPOT),
            format!(r#"{{"tag_name":"v1.16.0","assets":[{{"name":"Nevermind.zip","browser_download_url":"{adresse}{zipu}"}}{sigj}]}}"#).into_bytes(),
        );
        ctx.verifier_mise_a_jour(true, &FauxReseau(rep));
        assert_eq!(ctx.etat_mise_a_jour().en_attente.map(|p| p.version), Some("1.16.0".into()));
        let f = ctx.installer_mise_a_jour(&mises_a_jour::WinHttp, &cles);
        (f, racine, journal_de(&donnees))
    };
    // Signée et entière : installée
    let (f, racine, journal) = essai("bonne", "/dl/Nevermind.zip", "/dl/Nevermind.zip.sig");
    assert_eq!(f, FinInstallation::Relancer(racine.join("fichiers").join("Nevermind.ps1")));
    assert_eq!(
        std::fs::read_to_string(racine.join("fichiers").join("Nevermind.ps1")).unwrap(),
        "$AppVersion = '1.16.0'"
    );
    assert_eq!(std::fs::read_to_string(racine.join("Nevermind.exe")).unwrap(), "exe");
    assert_eq!(std::fs::read_to_string(racine.join("Nevermind.exe.old")).unwrap(), "vieil exe");
    assert!(!racine.join("fichiers").join("ancien.ps1").exists());
    assert!(journal.contains(&"Fichier obsolète supprimé: fichiers\\ancien.ps1".to_string()), "{journal:?}");
    assert_eq!(journal.last().unwrap(), "Mise à jour installée: 1.15.6 -> 1.16.0");
    // Un octet changé : la signature ne correspond plus, rien n'est installé
    let (f, racine, journal) = essai("abime", "/dl/abime.zip", "/dl/abime.zip.sig");
    assert_eq!(
        f,
        FinInstallation::Refus {
            message: mises_a_jour::message_signature("1.16.0", "fausse ou illisible"),
            statut: Some("Mise à jour annulée.".into())
        }
    );
    assert!(racine.join("fichiers").join("ancien.ps1").exists());
    assert_eq!(journal.last().unwrap(), "Mise à jour 1.16.0 refusée : signature invalide");
    // Pas de signature publiée : refusée sans rien télécharger
    let (f, _, journal) = essai("sans-signature", "/dl/Nevermind.zip", "");
    assert_eq!(
        f,
        FinInstallation::Refus { message: mises_a_jour::message_signature("1.16.0", "absente"), statut: None }
    );
    assert_eq!(
        journal.last().unwrap(),
        "Mise à jour 1.16.0 refusée : pas de signature (Nevermind.zip.sig) dans la version publiée"
    );
    fin.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = fil.join();
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn zip_abime_ou_hors_du_dossier() {
    // (des zips faits à la main : un CRC faux, un chemin qui sort du dossier)
    let d = dossier("zips");
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    w.start_file("Nevermind/a.txt", o).unwrap();
    std::io::Write::write_all(&mut w, b"bonjour").unwrap();
    let bon = w.finish().unwrap().into_inner();
    assert!(mises_a_jour::dezipper_verifie(&bon, &d.join("x")).is_ok());
    let mut faux = bon.clone();
    let i = faux.windows(7).position(|x| x == b"bonjour").unwrap();
    faux[i] = b'B';
    assert_eq!(
        mises_a_jour::dezipper_verifie(&faux, &d.join("y")),
        Err("Le zip est abîmé : rien n'a été installé.".into())
    );
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    w.start_file("../dehors.txt", o).unwrap();
    std::io::Write::write_all(&mut w, b"x").unwrap();
    let dehors = w.finish().unwrap().into_inner();
    assert_eq!(
        mises_a_jour::dezipper_verifie(&dehors, &d.join("z")),
        Err("Chemin refusé dans le zip : ../dehors.txt".into())
    );
    assert!(!d.join("dehors.txt").exists());
    assert_eq!(
        mises_a_jour::dezipper_verifie(b"pas un zip", &d.join("w")),
        Err("Le zip est abîmé : rien n'a été installé.".into())
    );
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------------------------------------------
// 7. Assistance
// ---------------------------------------------------------------------------------------------------------------

#[test]
fn historique_et_details() {
    let j = juge("assistance.json");
    let cas = liste(&j["cas"]);
    assert!(cas.len() >= 30);
    for c in &cas {
        let e = assistance::entree_historique(&serde_json::from_str(&s(&c["entree"])).unwrap());
        let attendu: Value = serde_json::from_str(&s(&c["convertie"])).unwrap();
        assert_eq!(e, attendu, "ConvertFrom-HistoryEntry {}", s(&c["entree"]));
        let d = assistance::details_techniques(&json!({ "Log": [e] }), Some("11111111-2222-3333-4444-555555555555"));
        assert_eq!(json!(d), c["details"], "Get-DetailsTechniques {}", s(&c["entree"]));
    }
    for q in liste(&j["qualites"]) {
        let log: Vec<Value> = liste(&q["log"])
            .iter()
            .map(|x| assistance::entree_historique(&serde_json::from_str(&s(x)).unwrap()))
            .collect();
        let h = json!({ "Log": log });
        assert_eq!(assistance::historique_definitif(&h), q["definitif"].as_bool().unwrap(), "définitif {:?}", q["log"]);
        assert_eq!(assistance::historique_note(&h), q["note"].as_bool().unwrap(), "noté {:?}", q["log"]);
        assert_eq!(
            assistance::historique_part_definitive(&h),
            q["partDefinitive"].as_bool().unwrap(),
            "en partie {:?}",
            q["log"]
        );
        assert_eq!(assistance::retour_definitif(&h), s(&q["retour"]), "retour {:?}", q["log"]);
    }
    for t in liste(&j["titres"]) {
        let l: Vec<String> = liste(&t["lignes"]).iter().map(s).collect();
        assert_eq!(assistance::titre_historique(&s(&t["titre"]), &l), s(&t["resultat"]), "titre de {l:?}");
        assert_eq!(json!(assistance::elements_historique(&l)), t["elements"], "éléments de {l:?}");
    }
    // Import-History
    let d = dossier("historique");
    std::fs::write(d.join("historique.json"), b64(&s(&j["historique"]["fichier"]))).unwrap();
    let ctx = contexte(&d);
    let lu: Value = serde_json::from_str(&s(&j["historique"]["lu"])).unwrap();
    assert_eq!(json!(ctx.historique()), lu, "Import-History");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn nouveautes_et_masque() {
    let j = juge("assistance.json");
    for n in liste(&j["nouveautes"]) {
        let mut attendu: Vec<String> = liste(&n["lignes"]).iter().map(s).collect();
        if attendu.is_empty() {
            attendu.push("Corrections et améliorations.".into());
        }
        assert_eq!(assistance::lignes_nouveautes(&s(&n["notes"]), true), attendu, "notes {:?}", s(&n["notes"]));
    }
    let masques = liste(&j["masques"]);
    assert!(masques.len() >= 50);
    for m in masques {
        let profil = s(&m["profil"]);
        let masque = Masque {
            profil: profil.trim_end_matches('\\').rsplit('\\').next().unwrap_or("").to_string(),
            compte: s(&m["compte"]),
            pc: s(&m["pc"]),
        };
        assert_eq!(masque.appliquer(&s(&m["texte"])), s(&m["masque"]), "masque de « {} » ({profil})", s(&m["texte"]));
    }
}

// ---------------------------------------------------------------------------------------------------------------
// 8. Sauvegarde
// ---------------------------------------------------------------------------------------------------------------

#[test]
fn import_de_la_sauvegarde() {
    let j = juge("sauvegarde.json");
    let d = dossier("sauvegarde");
    for c in liste(&j["cas"]) {
        let nom = s(&c["nom"]);
        poser_fichiers(&d, &c["avant"]);
        let ctx = contexte(&d);
        ctx.origines_factices(liste(&c["origines"]).iter().map(|o| serde_json::from_str(&s(o)).unwrap()).collect());
        ctx.importer_sauvegarde();
        let sv = ctx.sauvegarde();
        let attendu: Value = serde_json::from_str(&s(&c["sauvegarde"])).unwrap();
        assert_eq!(sv.en_json(), attendu, "{nom} : sauvegarde");
        assert_eq!(sv.probleme, c["probleme"].as_str().map(str::to_string), "{nom} : problème");
        assert_eq!(ctx.texte_probleme_sauvegarde(), s(&c["texte"]), "{nom} : texte");
        let mut noms: Vec<String> = fichiers_de(&d).keys().filter(|k| *k != "journal.txt").cloned().collect();
        noms.sort();
        let mut attendus: Vec<String> = liste(&c["apres"]).iter().map(s).collect();
        attendus.sort();
        assert_eq!(noms, attendus, "{nom} : fichiers");
        memes_journaux(&journal_de(&d), &liste(&c["journal"]), &nom);
    }
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------------------------------------------------------
// 9. Les pages de l'assistance (fiche et panneau), et les règles propres au noyau
// ---------------------------------------------------------------------------------------------------------------

/// `{}` dans un juge : une valeur vide de PowerShell (`$(if ...)` sans rien), écrite `null` par l'écran
fn vide_en_null(v: Value) -> Value {
    match v {
        Value::Object(m) if m.is_empty() => Value::Null,
        Value::Object(m) => Value::Object(m.into_iter().map(|(k, x)| (k, vide_en_null(x))).collect()),
        Value::Array(a) => Value::Array(a.into_iter().map(vide_en_null).collect()),
        autre => autre,
    }
}

#[test]
fn pages_fiche_et_panneau() {
    let j = juge("pages.json");
    let cas = liste(&j["cas"]);
    assert!(cas.len() >= 21);
    for c in cas {
        let attendu = vide_en_null(serde_json::from_str(&s(&c["page"])).unwrap());
        let de = Some(s(&c["de"]));
        let page = match s(&c["quoi"]).as_str() {
            "visite" => assistance::page_fiche(&assistance::Fiche::Visite(c["etape"].as_u64().unwrap() as usize), &de),
            "resultat" => assistance::page_fiche(
                &assistance::Fiche::Resultat {
                    titre: s(&c["titre"]),
                    lignes: liste(&c["lignes"]).iter().map(s).collect(),
                    note: s(&c["note"]),
                },
                &de,
            ),
            _ => assistance::page_signalement(&de),
        };
        assert_eq!(page, attendu, "{} depuis {}", s(&c["quoi"]), s(&c["de"]));
    }
}

/// La visite guidée de bout en bout : ouverte, suivie, fermée (retour à la page d'où elle a été ouverte)
#[test]
fn visite_de_bout_en_bout() {
    use nevermind_noyau::{Effet, Effets};
    let d = dossier("visite");
    let ctx = contexte(&d);
    assert!(nevermind_noyau::pages_assistance(&ctx, "parametres").is_empty());
    let mut e = Effets::default();
    let infos = assistance::InfosSignalement::default();
    assert!(assistance::action(&ctx, "visite", &Value::Null, &infos, &mut e));
    assert!(e.0.contains(&Effet::OuvrirEcran("fiche".into())));
    assert_eq!(ctx.reglage("TourDone"), Some(json!(true)));
    let p = nevermind_noyau::pages_assistance(&ctx, "fiche");
    assert_eq!(p["fiche"]["sous"], json!("Étape 1 sur 3"));
    assert_eq!(p["fiche"]["parent"], json!("parametres"));
    for _ in 0..2 {
        assert!(assistance::action(&ctx, "fiche:run", &Value::Null, &infos, &mut e));
    }
    assert_eq!(nevermind_noyau::pages_assistance(&ctx, "fiche")["fiche"]["sous"], json!("Étape 3 sur 3"));
    let mut e = Effets::default();
    assert!(assistance::action(&ctx, "fiche:run", &Value::Null, &infos, &mut e));
    assert_eq!(e.0[0], Effet::OuvrirEcran("parametres".into()));
    assert!(nevermind_noyau::pages_assistance(&ctx, "parametres").is_empty());
    // (sans fiche ouverte, les boutons de fiche sont à un autre bloc)
    assert!(!assistance::action(&ctx, "fiche:close", &Value::Null, &infos, &mut e));
    let _ = std::fs::remove_dir_all(&d);
}

/// Bienvenue : la visite la première fois ; un ancien utilisateur (plus d'un démarrage au journal) a les nouveautés
#[test]
fn bienvenue() {
    use nevermind_noyau::Effets;
    let d = dossier("bienvenue");
    let ctx = contexte(&d);
    ctx.definir_version("1.16.0");
    let rien = FauxReseau(HashMap::new());
    let mut e = Effets::default();
    ctx.verifier_bienvenue(&rien, &mut e);
    assert_eq!(ctx.fiche_assistance(), Some(assistance::Fiche::Visite(0)));
    assert_eq!(ctx.reglage("LastVersion"), Some(json!("1.16.0")));
    let d2 = dossier("bienvenue-ancien");
    std::fs::write(
        d2.join("journal.txt"),
        "2026-01-01T00:00:00 Démarrage Nevermind 1.0\r\n2026-01-02T00:00:00 démarrage OptiGame 1.0.50\r\n",
    )
    .unwrap();
    let ctx = contexte(&d2);
    ctx.definir_version("1.16.0");
    let mut rep = HashMap::new();
    rep.insert(
        format!("{}/repos/{}/releases/tags/v1.16.0", mises_a_jour::API, mises_a_jour::DEPOT),
        br#"{"body":"- un\r\n- deux"}"#.to_vec(),
    );
    ctx.verifier_bienvenue(&FauxReseau(rep), &mut e);
    assert_eq!(
        ctx.fiche_assistance(),
        Some(assistance::Fiche::Resultat {
            titre: "Quoi de neuf dans la version 1.16.0".into(),
            lignes: vec!["•  un".into(), "•  deux".into()],
            note: "Nevermind vient d'être mis à jour.".into()
        })
    );
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::remove_dir_all(&d2);
}

/// L'historique partagé : ajouté, annulé, « Tout restaurer » (seules les entrées entièrement remises)
#[test]
fn historique_partage() {
    let d = dossier("historique-partage");
    let ctx = contexte(&d);
    assert_eq!(ctx.ajouter_historique("Rien", &[], vec![]), None);
    let a = ctx
        .ajouter_historique(
            "Souris",
            &["a".into(), String::new()],
            vec![json!({ "Type": "reg", "Path": "HKCU:\\A", "Name": "b", "Existed": false })],
        )
        .unwrap();
    let b = ctx
        .ajouter_historique("Protection", &[], vec![json!({ "Type": "definitif", "Quoi": "q", "Retour": "r" })])
        .unwrap();
    assert_eq!(a.len(), 12);
    let h = ctx.historique();
    assert_eq!(h[0]["Id"], json!(b));
    assert_eq!(h[1]["Details"], json!(["a"]));
    assert_eq!(h[1]["Date"], json!("06/11/2026 à 21:05"));
    // « Tout restaurer » : la souris est remise (plus rien dans la sauvegarde), la protection reste « Définitif »
    ctx.marquer_historique_tout_annule(false);
    let h = ctx.historique();
    assert_eq!((h[0]["Annule"].clone(), h[1]["Annule"].clone()), (json!(false), json!(true)));
    // relu du disque (comme au lancement suivant)
    let ctx2 = contexte(&d);
    assert_eq!(ctx2.historique(), h);
    // 100 entrées au plus
    for i in 0..105 {
        ctx2.ajouter_historique(&format!("n{i}"), &[], vec![json!({ "Type": "note", "Quoi": "x" })]);
    }
    assert_eq!(ctx2.historique().len(), 100);
    let _ = std::fs::remove_dir_all(&d);
}

/// Set-Reg et Clear-Reg en mode test : l'origine notée dans sauvegarde.json, le changement noté, le faux registre
/// écrit pour l'utilisateur ; un réglage du PC passe par l'Aide
#[test]
fn reglages_du_registre() {
    let d = dossier("registre");
    let ctx = contexte(&d);
    let demandes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let dm = demandes.clone();
    ctx.fausse_aide(Arc::new(move |a, p| {
        dm.lock().unwrap().push(format!("{a} {p}"));
        Ok(json!(true))
    }));
    ctx.commencer_changements();
    ctx.changer_registre("HKCU:\\Software\\NevermindEssai", "Valeur", &json!(5), "DWord").unwrap();
    ctx.changer_registre("HKLM:\\SOFTWARE\\NevermindEssai", "N", &json!("t"), "String").unwrap();
    let ch = ctx.finir_changements();
    assert_eq!(ch.len(), 2);
    assert_eq!(ch[0]["Nouveau"], json!(5));
    assert_eq!(ctx.registre_valeur("HKCU:\\Software\\NevermindEssai", "Valeur"), Some(json!(5)));
    assert_eq!(
        demandes.lock().unwrap()[0],
        r#"reg.ecrire {"Cle":"HKLM:\\SOFTWARE\\NevermindEssai","Nom":"N","Valeur":"t","Type":"String"}"#
    );
    let sv = ctx.sauvegarde();
    assert_eq!(sv.registre["HKCU:\\Software\\NevermindEssai|Valeur"]["Existed"], json!(false));
    // relue du disque (sauvegarde.json avec BOM, comme Save-Backup)
    let o = std::fs::read(d.join("sauvegarde.json")).unwrap();
    assert_eq!(&o[..3], &[0xEF, 0xBB, 0xBF]);
    let ctx2 = contexte(&d);
    assert_eq!(ctx2.sauvegarde().registre, sv.registre);
    // Clear-Reg : la valeur (du faux registre) est effacée
    ctx.vider_registre("HKCU:\\Software\\NevermindEssai", "Valeur").unwrap();
    assert_eq!(ctx.registre_valeur("HKCU:\\Software\\NevermindEssai", "Valeur"), None);
    let _ = std::fs::remove_dir_all(&d);
}

/// Signaler un problème : le zip est créé dans le dossier donné (jamais le bureau dans un test), sans le nom du compte
#[test]
fn signaler_un_probleme() {
    use std::io::Read;
    let d = dossier("signalement");
    let ctx = contexte(&d);
    let compte = std::env::var("USERNAME").unwrap_or_default();
    ctx.ecrire_reglage("Chemin", json!(format!("C:\\Users\\{compte}\\x")), false).unwrap();
    ctx.noter_erreur("une erreur");
    let infos = assistance::InfosSignalement { score: Some("72".into()), protection: Some(90), ..Default::default() };
    let zip = ctx.signaler_probleme(Some(&d), "ça rame", &infos).unwrap();
    assert_eq!(zip, d.join("Nevermind problème 2026-11-06 21.05.zip"));
    let mut a = zip::ZipArchive::new(std::fs::File::open(&zip).unwrap()).unwrap();
    let mut noms: Vec<String> = a.file_names().map(|n| n.unwrap().into_owned()).collect();
    noms.sort();
    assert_eq!(noms, ["description.txt", "erreurs.txt", "infos.txt", "journal.txt", "parametres.json"]);
    let mut t = String::new();
    a.by_name("parametres.json").unwrap().read_to_string(&mut t).unwrap();
    assert!(compte.len() < 3 || !t.contains(&compte), "le nom du compte est masqué : {t}");
    let mut i = String::new();
    a.by_name("infos.txt").unwrap().read_to_string(&mut i).unwrap();
    assert!(
        i.starts_with('\u{feff}') && i.contains("Score d'optimisation : 72") && i.contains("Protection : 90 sur 100"),
        "{i}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// Les bulles pour Mindset : une à la fois dans notification.demande, la suivante quand Mindset a pris la précédente
#[test]
fn bulles_pour_mindset() {
    let d = dossier("bulles");
    let ctx = contexte(&d);
    let id = ctx.notifier("Partie finie", "144 FPS en moyenne");
    let f = d.join("notification.demande");
    let t = std::fs::read_to_string(&f).unwrap();
    assert_eq!(t, format!(r#"{{"titre":"Partie finie","texte":"144 FPS en moyenne","id":"{id}"}}"#));
    ctx.notifier("Deuxième", "attend");
    assert!(ctx.bulles_en_attente());
    std::fs::remove_file(&f).unwrap();
    ctx.envoyer_notification_suivante();
    // (moins de 6 secondes après la première : elle attend encore)
    assert!(!f.exists() && ctx.bulles_en_attente());
    let _ = std::fs::remove_dir_all(&d);
}

/// Get-AppRoot : le parent du dossier de l'appli s'il s'appelle « fichiers »
#[test]
fn racine_de_la_copie() {
    let d = dossier("racine");
    let c = Contexte::nouveau(d.join("Nevermind").join("fichiers"), d.clone(), false, true);
    assert_eq!(c.racine(), d.join("Nevermind"));
    let c = Contexte::nouveau(d.join("Nevermind"), d.clone(), false, true);
    assert_eq!(c.racine(), d.join("Nevermind"));
    c.definir_racine(d.join("autre"));
    assert_eq!(c.racine(), d.join("autre"));
    let _ = std::fs::remove_dir_all(&d);
}
