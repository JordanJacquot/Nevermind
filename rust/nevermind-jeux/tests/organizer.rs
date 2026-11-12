//! Les juges de l'Organizer exportés de la version PowerShell (outils\exporter-juges-organizer.ps1) : touches, souris,
//! touches capturées, couleurs, titres, réglages (organizer.json), et des centaines de scénarios de la veille (fenêtres
//! du jeu qui s'ouvrent et se ferment, fenêtre devant, raccourcis pris ou refusés, barre flottante, gestes de la vue).
//! Chaque scénario est rejoué contre un faux Windows : les mêmes événements doivent sortir, dans le même ordre.

use nevermind_jeux::organizer::veille::{Bureau, Pastille, Veille};
use nevermind_jeux::organizer::{self, Capture, Config, Fenetre, Perso};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

fn juge(nom: &str) -> Value {
    // (JUGES_ORGANIZER : un autre dossier de juges, par exemple tirés avec une autre graine : des cas jamais vus)
    let d = std::env::var("JUGES_ORGANIZER")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("juges"));
    let p = d.join(format!("{nom}.json"));
    let t = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    serde_json::from_str(t.trim_start_matches('\u{feff}')).unwrap()
}

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        x => x.to_string(),
    }
}

fn paire(v: &Value) -> Option<(u32, u32)> {
    v.as_array().map(|a| (a[0].as_u64().unwrap() as u32, a[1].as_u64().unwrap() as u32))
}

#[test]
fn touches_souris_noms() {
    let mut faux = Vec::new();
    let j = juge("organizer-touches");
    for c in j.as_array().unwrap() {
        let e = s(&c["e"]);
        let hk = organizer::touche_windows(&e);
        if hk != paire(&c["hk"]) {
            faux.push(format!("touche {e:?} : {hk:?} au lieu de {}", c["hk"]));
        }
        let ms = organizer::souris(&e);
        if ms != paire(&c["souris"]) {
            faux.push(format!("souris {e:?} : {ms:?} au lieu de {}", c["souris"]));
        }
        let n = organizer::nom_touche(&e);
        if n != s(&c["nom"]) {
            faux.push(format!("nom {e:?} : {n:?} au lieu de {}", c["nom"]));
        }
    }
    assert!(j.as_array().unwrap().len() > 2000);
    assert!(faux.is_empty(), "{} écarts :\n{}", faux.len(), faux[..faux.len().min(60)].join("\n"));
}

#[test]
fn touches_capturees() {
    let mut faux = Vec::new();
    for c in juge("organizer-captures").as_array().unwrap() {
        let mods = c["mods"].as_u64().unwrap() as u32;
        let (r, garde) = if let Some(vk) = c["vk"].as_u64() {
            match organizer::touche_capturee(vk as u32, mods) {
                Capture::Attendre => ("<rien>".to_string(), true),
                Capture::Annuler => ("<annule>".to_string(), false),
                Capture::Choisir(t) => (t, false),
            }
        } else {
            let b = match s(&c["bouton"]).as_str() {
                "XButton1" => 4,
                "XButton2" => 5,
                "Middle" => 3,
                _ => 1,
            };
            (
                organizer::souris_capturee(b, mods).unwrap_or_else(|| "<rien>".into()),
                c["garde"].as_bool().unwrap_or(false),
            )
        };
        if r != s(&c["r"]) || garde != c["garde"].as_bool().unwrap_or(false) {
            faux.push(format!("{c} : {r:?} {garde}"));
        }
    }
    assert!(faux.is_empty(), "{} écarts :\n{}", faux.len(), faux[..faux.len().min(60)].join("\n"));
}

#[test]
fn couleurs_et_titres() {
    let mut faux = Vec::new();
    for c in juge("organizer-couleurs").as_array().unwrap() {
        // (le nom arrive en liste d'un seul élément : PowerShell le lit comme son texte)
        let n = match &c["nom"] {
            Value::Array(a) => a.first().map(s).unwrap_or_default(),
            x => s(x),
        };
        let cl = s(&c["classe"]);
        let col = organizer::couleur(&cl, &n);
        if col != s(&c["couleur"]) {
            faux.push(format!("couleur {cl:?} {n:?} : {col} au lieu de {}", c["couleur"]));
        }
        if c["classe"] != "Inconnu" && u64::from(organizer::id_classe(&cl)) != c["id"].as_u64().unwrap() {
            faux.push(format!("classe {cl:?} : {}", organizer::id_classe(&cl)));
        }
    }
    for c in juge("organizer-titres").as_array().unwrap() {
        let (n, cl) = organizer::lire_titre(&s(&c["e"]));
        if n != s(&c["nom"]) || cl != s(&c["classe"]) {
            faux.push(format!("titre {:?} : {n:?} {cl:?}", c["e"]));
        }
    }
    assert!(faux.is_empty(), "{} écarts :\n{}", faux.len(), faux.join("\n"));
}

fn tries(l: &[(String, String)]) -> Vec<(String, String)> {
    let mut v = l.to_vec();
    v.sort_by_key(|x| x.0.to_lowercase());
    v
}

fn paires(v: &Value) -> Vec<(String, String)> {
    // (PowerShell les range comme la langue de Windows : ici, le même ordre des deux côtés)
    tries(&v.as_array().map(|a| a.iter().map(|p| (s(&p[0]), s(&p[1]))).collect::<Vec<_>>()).unwrap_or_default())
}

#[test]
fn reglages() {
    let dir = nevermind_noyau::dossier_essai("organizer-reglages");
    let mut faux = Vec::new();
    for c in juge("organizer-config").as_array().unwrap() {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if let Value::String(t) = &c["e"] {
            std::fs::write(dir.join("organizer.json"), t.as_bytes()).unwrap();
        }
        let (j, _) = nevermind_jeux::json::lire_json_sur(None, &dir.join("organizer.json"));
        let (cf, err) = Config::depuis(j.as_ref());
        let f = |v: &Value| v.as_f64();
        let ok = cf.on == c["On"].as_bool().unwrap()
            && cf.barre == c["Bar"].as_bool().unwrap()
            && cf.barre_toujours == c["BarAlways"].as_bool().unwrap()
            && cf.barre_x == f(&c["BarX"])
            && cf.barre_y == f(&c["BarY"])
            && cf.ordre == c["Order"].as_array().unwrap().iter().map(s).collect::<Vec<_>>()
            && tries(&cf.classes) == paires(&c["Classes"])
            && tries(&cf.touches) == paires(&c["Hotkeys"])
            && err.is_some() == c["illisible"].as_bool().unwrap();
        if !ok {
            faux.push(format!("{} : {cf:?} {err:?}", c["e"]));
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(faux.is_empty(), "{} écarts :\n{}", faux.len(), faux.join("\n"));
}

// ------------------------------------------------------------------ la veille : un faux Windows

#[derive(Default)]
struct Faux {
    ev: Vec<String>,
    fenetres: Vec<Fenetre>,
    fg: isize,
    pids: HashMap<isize, u32>,
    focus_ok: bool,
    prises: HashSet<String>,
    souris_ok: bool,
    survol: bool,
}

impl Bureau for Faux {
    fn fenetres(&mut self) -> Vec<Fenetre> {
        self.fenetres.clone()
    }
    fn devant(&mut self) -> isize {
        self.fg
    }
    fn pid(&mut self, h: isize) -> u32 {
        self.pids.get(&h).copied().unwrap_or(0)
    }
    fn afficher(&mut self, h: isize) -> bool {
        let ok = self.focus_ok && h != 0;
        self.ev.push(format!("afficher {h} {}", if ok { "oui" } else { "non" }));
        if ok {
            self.fg = h;
        }
        ok
    }
    fn prendre_touche(&mut self, id: i32, mods: u32, vk: u32) -> bool {
        let ok = !self.prises.contains(&format!("{mods}:{vk}"));
        self.ev.push(format!("prendre {id} {mods} {vk} {}", if ok { "oui" } else { "non" }));
        ok
    }
    fn rendre_touche(&mut self, id: i32) {
        self.ev.push(format!("rendre {id}"));
    }
    fn souris_vider(&mut self) {
        self.ev.push("souris-vider".into());
    }
    fn souris_lier(&mut self, mods: u32, bouton: u32, id: i32) {
        self.ev.push(format!("souris-lier {mods} {bouton} {id}"));
    }
    fn souris_demarrer(&mut self) -> bool {
        self.ev.push(format!("souris-demarrer {}", if self.souris_ok { "oui" } else { "non" }));
        self.souris_ok
    }
    fn souris_arreter(&mut self) {
        self.ev.push("souris-arreter".into());
    }
    fn envoyer_touche(&mut self, mods: u32, vk: u32) -> bool {
        self.ev.push(format!("envoyer {mods} {vk}"));
        true
    }
    fn survol_barre(&mut self) -> bool {
        self.survol
    }
    fn barre_dessiner(&mut self, p: &[Pastille]) {
        let t = if p.is_empty() {
            "Aucun perso connecté".to_string()
        } else {
            p.iter()
                .map(|x| format!("{}{}", if x.devant { ">" } else { "" }, x.infobulle))
                .collect::<Vec<_>>()
                .join(" ; ")
        };
        self.ev.push(format!("barre {t}"));
    }
    fn barre_montrer(&mut self) {
        self.ev.push("montrer".into());
    }
    fn barre_cacher(&mut self) {
        self.ev.push("cacher".into());
    }
    fn barre_devant(&mut self) {
        self.ev.push("devant-barre".into());
    }
    fn vue_redessiner(&mut self, _l: &[Perso]) {
        self.ev.push("vue".into());
    }
    fn enregistrer(&mut self, _c: &Config) {
        self.ev.push("enregistrer".into());
    }
    fn journal(&mut self, m: &str) {
        self.ev.push(format!("journal {m}"));
    }
    fn statut(&mut self, m: &str) {
        self.ev.push(format!("statut {m}"));
    }
}

/// L'ordre des « rendre » qui se suivent ne compte pas (PowerShell les rend dans l'ordre d'une table).
fn canon(v: &[String]) -> Vec<String> {
    let mut o: Vec<String> = Vec::new();
    let mut i = 0;
    while i < v.len() {
        if v[i].starts_with("rendre ") {
            let mut j = i;
            while j < v.len() && v[j].starts_with("rendre ") {
                j += 1;
            }
            let mut r = v[i..j].to_vec();
            r.sort();
            o.extend(r);
            i = j;
        } else {
            o.push(v[i].clone());
            i += 1;
        }
    }
    o
}

fn config_depart(c: &Value) -> Config {
    let mut cf = Config::default();
    if let Some(b) = c.get("On").and_then(Value::as_bool) {
        cf.on = b;
    }
    if let Some(b) = c.get("Bar").and_then(Value::as_bool) {
        cf.barre = b;
    }
    if let Some(b) = c.get("BarAlways").and_then(Value::as_bool) {
        cf.barre_toujours = b;
    }
    if let Some(o) = c.get("Order") {
        cf.ordre = nevermind_jeux::biblio::liste(Some(o)).iter().filter(|x| !s(x).is_empty()).map(s).collect();
    }
    if let Some(Value::Object(h)) = c.get("Hotkeys") {
        for (k, v) in h {
            organizer::mettre(&mut cf.touches, k, s(v));
        }
    }
    if let Some(Value::Object(h)) = c.get("Classes") {
        for (k, v) in h {
            organizer::mettre(&mut cf.classes, k, s(v));
        }
    }
    cf
}

fn rejouer(sc: &Value) -> Result<(), String> {
    let mut b = Faux { focus_ok: true, souris_ok: true, ..Default::default() };
    let mut v = Veille::nouvelle(config_depart(&sc["config"]));
    let sorties = sc["sorties"].as_array().unwrap();
    let attendu =
        |i: usize| -> Vec<String> { sorties[i].as_array().map(|a| a.iter().map(s).collect()).unwrap_or_default() };
    if v.config.on {
        v.demarrer(&mut b);
    }
    let comparer = |b: &mut Faux, i: usize, quoi: &str| -> Result<(), String> {
        let a = canon(&attendu(i));
        let r = canon(&std::mem::take(&mut b.ev));
        if a != r {
            return Err(format!("étape {i} ({quoi}) :\n  PowerShell : {a:?}\n  Rust :       {r:?}"));
        }
        Ok(())
    };
    comparer(&mut b, 0, "démarrage")?;
    for (i, e) in sc["etapes"].as_array().unwrap().iter().enumerate() {
        let o = e.as_object().unwrap();
        if let Some(f) = o.get("fenetres") {
            b.fenetres = f
                .as_array()
                .unwrap()
                .iter()
                .map(|w| {
                    let (p, h) = (w["Pid"].as_u64().unwrap() as u32, w["Hwnd"].as_i64().unwrap() as isize);
                    b.pids.insert(h, p);
                    Fenetre::depuis_titre(&s(&w["Title"]), p, h)
                })
                .collect();
        } else if let Some(d) = o.get("devant") {
            let h = d.as_i64().unwrap() as isize;
            b.fg = h;
            match o.get("pid").and_then(Value::as_u64).filter(|p| *p != 0) {
                Some(p) => {
                    b.pids.insert(h, p as u32);
                }
                None => {
                    b.pids.remove(&h);
                }
            }
        } else if let Some(n) = o.get("tic") {
            for _ in 0..n.as_u64().unwrap() {
                if v.active {
                    v.passe(&mut b, false);
                }
            }
        } else if let Some(id) = o.get("touche") {
            v.touche(&mut b, id.as_i64().unwrap() as i32);
        } else if let Some(p) = o.get("prises") {
            b.prises = p.as_array().map(|a| a.iter().map(s).collect()).unwrap_or_default();
        } else if let Some(a) = o.get("capture") {
            v.capture = Some(s(a));
            v.choisir_touche(&mut b, &s(&o["texte"]));
        } else if let Some(x) = o.get("souris_ok") {
            b.souris_ok = x.as_bool().unwrap();
        } else if let Some(x) = o.get("focus_ok") {
            b.focus_ok = x.as_bool().unwrap();
        } else if let Some(x) = o.get("survol") {
            b.survol = x.as_bool().unwrap();
        } else if let Some(x) = o.get("vue") {
            v.montrer_vue(&mut b, x.as_bool().unwrap());
        } else if let Some(x) = o.get("on") {
            v.allumer(&mut b, x.as_bool().unwrap());
        } else if let Some(x) = o.get("barre") {
            v.regler_barre(&mut b, false, x.as_bool().unwrap());
        } else if let Some(x) = o.get("toujours") {
            v.regler_barre(&mut b, true, x.as_bool().unwrap());
        } else if let Some(x) = o.get("deplacer") {
            v.deplacer(&mut b, &s(x), &s(&o["vers"]));
        } else if let Some(x) = o.get("retirer") {
            v.retirer(&mut b, &s(x));
        } else if let Some(x) = o.get("clic") {
            v.clic_pastille(&mut b, &s(x));
        } else if let Some(x) = o.get("afficher") {
            v.afficher_fenetre(&mut b, x.as_i64().unwrap() as isize);
        }
        comparer(&mut b, i + 1, &e.to_string())?;
    }
    let f = &sc["fin"];
    let c = &v.config;
    let ok = c.on == f["On"].as_bool().unwrap()
        && c.barre == f["Bar"].as_bool().unwrap()
        && c.barre_toujours == f["BarAlways"].as_bool().unwrap()
        && c.ordre == f["Order"].as_array().unwrap().iter().map(s).collect::<Vec<_>>()
        && tries(&c.classes) == paires(&f["Classes"])
        && tries(&c.touches) == paires(&f["Hotkeys"]);
    if !ok {
        return Err(format!("config finale : {c:?}\n  attendu {f}"));
    }
    Ok(())
}

#[test]
fn veille() {
    let j = juge("organizer-veille");
    let l = j.as_array().unwrap();
    let mut faux = Vec::new();
    let mut etapes = 0;
    for (i, sc) in l.iter().enumerate() {
        etapes += sc["etapes"].as_array().unwrap().len();
        if let Err(e) = rejouer(sc) {
            faux.push(format!("scénario {i} : {e}"));
        }
    }
    assert!(l.len() >= 300, "{} scénarios seulement", l.len());
    assert!(
        faux.is_empty(),
        "{} scénarios faux sur {} ({etapes} étapes) :\n{}",
        faux.len(),
        l.len(),
        faux[..faux.len().min(5)].join("\n")
    );
}

/// Les touches simulées (propres à la version Rust) : le bouton choisi appuie sur la touche choisie, seulement quand le
/// jeu est devant, sans changer de perso ; la touche se choisit au clavier, pas à la souris.
#[test]
fn touche_simulee() {
    let mut b = Faux { focus_ok: true, souris_ok: true, ..Default::default() };
    b.fenetres = vec![Fenetre::depuis_titre("Bob - Iop - 3.0 - Release", 1, 10)];
    b.pids.insert(10, 1);
    b.fg = 10;
    let mut v = Veille::nouvelle(Config::default());
    v.config.on = true;
    // (le bouton : clic latéral avant ; la touche : A)
    v.capture = Some("S1".into());
    assert!(v.capture_souris());
    v.choisir_touche(&mut b, "Souris5");
    v.capture = Some("E1".into());
    assert!(!v.capture_souris(), "la touche faite dans le jeu se choisit au clavier");
    v.choisir_touche(&mut b, "A");
    assert_eq!(v.config.envoi("S1"), "A");
    assert_eq!(v.config.touche("S1"), "Souris5");
    // (une autre action ne perd pas sa touche : « A » n'est pas un raccourci)
    assert_eq!(v.config.touche("P1"), "F1");
    v.demarrer(&mut b);
    assert!(b.ev.iter().any(|e| e == "souris-lier 0 5 7131"), "{:?}", b.ev);
    b.ev.clear();
    v.touche(&mut b, 7131);
    assert_eq!(b.ev, vec!["envoyer 0 65".to_string()], "A, et rien d'autre (pas de changement de perso)");
    // (enregistré, puis relu)
    let (relu, e) = Config::depuis(Some(&v.config.vers_json()));
    assert!(e.is_none());
    assert_eq!(relu.envoi("S1"), "A");
    // (une touche que l'organizer prend lui-même n'est pas envoyée)
    v.config.envois.clear();
    v.config.envois.push(("S1".into(), "F1".into()));
    b.ev.clear();
    v.touche(&mut b, 7131);
    assert!(!b.ev.iter().any(|e| e.starts_with("envoyer")), "{:?}", b.ev);
    // (jeu plus devant : les raccourcis sont rendus, plus rien n'est écouté)
    b.fg = 99;
    v.passe(&mut b, false);
    b.ev.clear();
    v.touche(&mut b, 7131);
    assert!(b.ev.is_empty(), "{:?}", b.ev);
}

/// La création guidée (maquette D) : « + Ajouter », le bouton de souris (le clavier ne compte pas), puis la touche du
/// jeu ; Annuler oublie une création pas finie ; la croix enlève un bouton ; 20 au plus.
#[test]
fn creation_guidee_des_boutons_de_souris() {
    let mut b = Faux::default();
    let mut v = Veille::nouvelle(Config::default());
    v.commencer_creation(&mut b);
    assert_eq!((v.creation.as_deref(), v.capture.as_deref()), (Some("S1"), Some("S1")));
    assert!(v.capture_souris() && !v.capture_clavier(), "étape 1 : seulement la souris");
    v.choisir_touche(&mut b, "Souris5");
    assert_eq!(v.capture.as_deref(), Some("E1"), "étape 2 : la touche du jeu");
    assert!(!v.capture_souris() && v.capture_clavier(), "étape 2 : seulement le clavier");
    v.choisir_touche(&mut b, "A");
    assert_eq!((v.creation.as_deref(), v.capture.as_deref()), (None, None));
    assert_eq!((v.config.touche("S1"), v.config.envoi("S1")), ("Souris5".to_string(), "A".to_string()));
    assert!(b.ev.iter().any(|e| e == "statut C'est prêt : Souris 5 (avant) fait A dans le jeu."), "{:?}", b.ev);
    // Une deuxième, annulée après le bouton : rien n'est gardé
    v.commencer_creation(&mut b);
    assert_eq!(v.creation.as_deref(), Some("S2"));
    v.choisir_touche(&mut b, "Molette");
    v.annuler_choix(&mut b);
    assert!(v.config.touche("S2").is_empty() && v.creation.is_none());
    assert_eq!(nevermind_jeux::organizer::simulees(&v.config), ["S1"]);
    // La croix
    v.supprimer_simulee(&mut b, "S1");
    assert!(nevermind_jeux::organizer::simulees(&v.config).is_empty());
    // 20 au plus
    for i in 1..=20 {
        v.config.touches.push((format!("S{i}"), "Souris4".into()));
        v.config.envois.push((format!("S{i}"), "B".into()));
    }
    v.commencer_creation(&mut b);
    assert!(v.creation.is_none(), "les 20 sont prises");
}

/// Sans touche simulée, le fichier des réglages reste celui de la version PowerShell (pas de « Sends »).
#[test]
fn reglages_sans_touche_simulee() {
    assert!(Config::default().vers_json().get("Sends").is_none());
}

/// La contre-épreuve : un événement changé dans un scénario doit être vu.
#[test]
fn contre_epreuve() {
    let j = juge("organizer-veille");
    let mut sc = j[0].clone();
    let sorties = sc["sorties"].as_array_mut().unwrap();
    let i =
        sorties.iter().position(|x| x.as_array().is_some_and(|a| !a.is_empty())).expect("un scénario sans événement");
    sorties[i].as_array_mut().unwrap()[0] = Value::String("journal faux".into());
    assert!(rejouer(&sc).is_err());
    assert!(rejouer(&j[0]).is_ok());
}
