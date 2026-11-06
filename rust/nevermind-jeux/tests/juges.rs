//! Les juges exportés de la version PowerShell (outils\exporter-juges-jeux.ps1) : chaque cas refait le même faux PC
//! dans un dossier d'essai (jamais le vrai PC) et la version Rust doit donner exactement la même sortie.

use nevermind_jeux::systeme::{Ruche, SystemeEssai};
use nevermind_jeux::texte::{self, Moment};
use nevermind_jeux::{BlocJeux, biblio, lancement, organizer, steam};
use nevermind_noyau::{Bloc, Contexte, Effets};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn juge(nom: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("juges").join(format!("{nom}.json"));
    let t = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let mut v: Value = serde_json::from_str(t.trim_start_matches('\u{feff}')).unwrap();
    vides_en_null(&mut v);
    v
}

/// PowerShell 5.1 écrit « {} » pour une valeur vide ($(if ...) sans else) ; Send-OrdreVue la remet à null.
fn vides_en_null(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (_, x) in m.iter_mut() {
                if matches!(x, Value::Object(o) if o.is_empty()) {
                    *x = Value::Null;
                } else {
                    vides_en_null(x);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(vides_en_null),
        _ => {}
    }
}

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        x => x.to_string(),
    }
}

// ------------------------------------------------------------------ le faux PC

struct Bac {
    dir: PathBuf,
}

impl Drop for Bac {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Bac {
    fn neuf(nom: &str) -> Bac {
        let n = std::sync::atomic::AtomicU64::new(0);
        let _ = n;
        let dir = std::env::temp_dir().join(format!(
            "nevermind-essai-jeux-{nom}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Bac { dir }
    }
    fn txt(&self) -> String {
        self.dir.to_string_lossy().to_string()
    }
    fn sub(&self, t: &str) -> String {
        t.replace("{BAC2}", &self.txt().replace('\\', "\\\\")).replace("{BAC}", &self.txt())
    }
    /// Remplace le dossier du bac par {BAC}, sans tenir compte de la casse (Ubisoft garde les dossiers en minuscules).
    fn norm(&self, t: &str) -> String {
        let bas = t.to_lowercase();
        let cle = self.txt().to_lowercase();
        let mut sortie = String::new();
        let mut i = 0;
        while let Some(j) = bas[i..].find(&cle) {
            sortie.push_str(&t[i..i + j]);
            sortie.push_str("{BAC}");
            i += j + cle.len();
        }
        sortie.push_str(&t[i..]);
        sortie
    }
    fn norm_json(&self, v: &Value) -> Value {
        match v {
            Value::String(x) => Value::String(self.norm(x)),
            Value::Array(a) => Value::Array(a.iter().map(|x| self.norm_json(x)).collect()),
            Value::Object(m) => Value::Object(m.iter().map(|(k, x)| (k.clone(), self.norm_json(x))).collect()),
            x => x.clone(),
        }
    }
}

fn b64(s: &str) -> Vec<u8> {
    let t = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut v = Vec::new();
    let (mut acc, mut n) = (0u32, 0);
    for c in s.bytes() {
        let Some(x) = t.iter().position(|y| *y == c) else { continue };
        acc = (acc << 6) | x as u32;
        n += 6;
        if n >= 8 {
            n -= 8;
            v.push((acc >> n) as u8);
        }
    }
    v
}

fn dater(p: &Path, jours: f64) {
    use std::os::windows::fs::OpenOptionsExt;
    let t = std::time::SystemTime::now() - std::time::Duration::from_secs_f64(jours * 86400.0);
    let f = std::fs::OpenOptions::new().access_mode(0x100).custom_flags(0x0200_0000).open(p).unwrap();
    f.set_modified(t).unwrap();
}

/// Fabrique le faux PC déclaré (New-FauxPc de l'exporteur) et le faux Windows qui va avec.
fn faux_pc(bac: &Bac, pc: &Value) -> SystemeEssai {
    let d = &bac.dir;
    let fichiers = pc["Fichiers"].as_array().cloned().unwrap_or_default();
    for f in fichiers.iter().filter(|f| f.is_object()) {
        let p = d.join(s(&f["C"]));
        if f.get("Dossier").map(|x| x == &json!(true)).unwrap_or(false) {
            std::fs::create_dir_all(&p).unwrap();
            continue;
        }
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        if let Some(t) = f.get("T").filter(|x| !x.is_null()) {
            std::fs::write(&p, bac.sub(&s(t))).unwrap();
        } else if let Some(x) = f.get("B64").filter(|x| !x.is_null()) {
            std::fs::write(&p, b64(&s(x))).unwrap();
        } else {
            std::fs::write(&p, vec![0u8; f["Taille"].as_f64().unwrap_or(0.0) as usize]).unwrap();
        }
    }
    let mut ages: Vec<&Value> =
        fichiers.iter().filter(|f| f.get("Age").map(|a| !a.is_null()).unwrap_or(false)).collect();
    ages.sort_by_key(|f| std::cmp::Reverse(s(&f["C"]).len()));
    for f in ages {
        dater(&d.join(s(&f["C"])), f["Age"].as_f64().unwrap());
    }
    let mut sys = SystemeEssai::default();
    for r in pc["Registre"].as_array().cloned().unwrap_or_default() {
        if !r.is_object() {
            continue;
        }
        let ruche = match s(&r["Ruche"]).as_str() {
            "HKCU" => Ruche::Utilisateur,
            "HKLM" => Ruche::Machine,
            _ => Ruche::Classes,
        };
        let cle = s(&r["Cle"]);
        sys.ajouter_cle(ruche, &cle);
        let mut parties: Vec<&str> = cle.split('\\').collect();
        while parties.len() > 1 {
            parties.pop();
            sys.ajouter_cle(ruche, &parties.join("\\"));
        }
        if let Some(Value::Object(m)) = r.get("Valeurs") {
            for (k, v) in m {
                match v {
                    Value::Number(n) => sys.nombre(ruche, &cle, &bac.sub(k), n.as_u64().unwrap() as u32),
                    _ => sys.texte(ruche, &cle, &bac.sub(k), &bac.sub(&s(v))),
                }
            }
        }
    }
    // (les sous-clés dans l'ordre de déclaration : ajouter_cle range chaque clé sous son parent)
    for d2 in pc["Disques"].as_array().cloned().unwrap_or_default() {
        if !d2.is_null() {
            sys.disques.push(bac.sub(&s(&d2)));
        }
    }
    for n in ["ProgramData", "APPDATA"] {
        let v = pc.get("Env").and_then(|e| e.get(n)).map(s).unwrap_or_else(|| format!("{{BAC}}\\vide-{n}"));
        sys.variables.insert(n.to_lowercase(), bac.sub(&v));
    }
    sys.heure = Some(Moment::depuis(2026, 11, 6, 15, 0, 0));
    sys
}

fn contexte(bac: &Bac) -> Contexte {
    Contexte::nouveau(bac.dir.join("appli"), bac.dir.join("donnees"), false, true)
}

// ------------------------------------------------------------------ les juges

#[test]
fn texte() {
    let j = &juge("texte")[0];
    let mut faux = Vec::new();
    for c in j["recherche"].as_array().unwrap() {
        if texte::texte_recherche(&s(&c["e"])) != s(&c["s"]) {
            faux.push(format!(
                "recherche {:?}: {:?} au lieu de {:?}",
                c["e"],
                texte::texte_recherche(&s(&c["e"])),
                c["s"]
            ));
        }
    }
    for c in j["duree"].as_array().unwrap() {
        let r = texte::duree_jeu(c["e"].as_f64().unwrap());
        if r != s(&c["s"]) {
            faux.push(format!("durée {}: {r} au lieu de {}", c["e"], c["s"]));
        }
    }
    let n = Moment::depuis(2026, 11, 6, 15, 0, 0);
    for c in j["derniere"].as_array().unwrap() {
        let r = texte::derniere_partie(&s(&c["e"]), n);
        if r != s(&c["s"]) {
            faux.push(format!("dernière {}: {r} au lieu de {}", c["e"], c["s"]));
        }
    }
    for c in j["n0"].as_array().unwrap() {
        let x = c["e"].as_f64().unwrap();
        if texte::n0(x) != s(&c["s"]) || texte::n1(x) != s(&c["s1"]) {
            faux.push(format!("N0 {x}: {} / {} au lieu de {} / {}", texte::n0(x), texte::n1(x), c["s"], c["s1"]));
        }
    }
    for c in j["taille"].as_array().unwrap() {
        let r = nevermind_jeux::taille_lisible(c["e"].as_f64().unwrap());
        if r != s(&c["s"]) {
            faux.push(format!("taille {}: {r} au lieu de {}", c["e"], c["s"]));
        }
    }
    for c in j["entier"].as_array().unwrap() {
        if texte::entier_ps(c["e"].as_f64().unwrap()) != c["s"].as_i64().unwrap() {
            faux.push(format!("[int] {}", c["e"]));
        }
    }
    for c in j["titre"].as_array().unwrap() {
        let r = biblio::titre_mots(&s(&c["e"]));
        if r != s(&c["s"]) {
            faux.push(format!("titre {}: {r} au lieu de {}", c["e"], c["s"]));
        }
    }
    for c in j["tri"].as_array().unwrap() {
        let mut l: Vec<String> = c["e"].as_array().unwrap().iter().map(s).collect();
        texte::trier_ps(&mut l, |a, b| texte::comparer_culture(a, b));
        let attendu: Vec<String> = c["s"].as_array().unwrap().iter().map(s).collect();
        if l != attendu {
            faux.push(format!("tri : {l:?} au lieu de {attendu:?}"));
        }
    }
    println!(
        "texte : {} cas vérifiés",
        j.as_object().unwrap().values().map(|v| v.as_array().map(|a| a.len()).unwrap_or(0)).sum::<usize>()
    );
    assert!(faux.is_empty(), "{} écarts :\n{}", faux.len(), faux.join("\n"));
}

#[test]
fn organizer() {
    let j = &juge("organizer")[0];
    let mut faux = Vec::new();
    for c in j["titres"].as_array().unwrap() {
        let (n, cl) = organizer::lire_titre(&s(&c["e"]));
        if n != s(&c["nom"]) || cl != s(&c["classe"]) {
            faux.push(format!("titre {}: {n}/{cl}", c["e"]));
        }
    }
    for c in j["touches"].as_array().unwrap() {
        let r = organizer::nom_touche(&s(&c["e"]));
        if r != s(&c["s"]) {
            faux.push(format!("touche {}: {r} au lieu de {}", c["e"], c["s"]));
        }
    }
    for c in j["visible"].as_array().unwrap() {
        let noms: Vec<String> = c["jeux"].as_array().unwrap().iter().map(s).collect();
        let r: Vec<&str> = noms.iter().map(|x| x.as_str()).collect();
        let reg = if c["reglage"].is_null() { None } else { Some(&c["reglage"]) };
        if organizer::visible(reg, c["on"].as_bool().unwrap(), &r) != c["s"].as_bool().unwrap() {
            faux.push(format!("visible {c}"));
        }
    }
    assert!(faux.is_empty(), "{} écarts :\n{}", faux.len(), faux.join("\n"));
}

#[test]
fn lancement() {
    let j = &juge("lancement")[0];
    let applis: Vec<(String, String)> =
        j["applis"].as_array().unwrap().iter().map(|a| (s(&a["nom"]), s(&a["id"]))).collect();
    let mut faux = Vec::new();
    for c in j["cas"].as_array().unwrap() {
        let g = &c["jeu"];
        let jeu = biblio::Jeu {
            nom: s(&g["Name"]),
            exes: biblio::liste(g.get("Exes")).iter().map(s).collect(),
            lancement: s(&g["Launch"]),
            desinstall: s(&g["Uninstall"]),
            source: s(&g["Source"]),
            ..Default::default()
        };
        let l = lancement::lancement(&jeu, &|| applis.clone());
        let (k, p, a) = match l {
            lancement::Lancement::Adresse(x) => ("url".to_string(), x, String::new()),
            lancement::Lancement::Exe { chemin, arguments } => ("exe".to_string(), chemin, arguments),
        };
        let att = &c["lancer"];
        if k != s(&att["kind"]) || p != s(&att["path"]) || a != s(&att["args"]) {
            faux.push(format!("lancer {}: {k} {p} {a} au lieu de {att}", jeu.nom));
        }
        let (k, p, a) = match lancement::desinstallation(&jeu) {
            lancement::Desinstallation::Aucune => ("none".to_string(), String::new(), String::new()),
            lancement::Desinstallation::Launcher(x) => ("launcher".into(), x, String::new()),
            lancement::Desinstallation::Adresse(x) => ("url".into(), x, String::new()),
            lancement::Desinstallation::Exe { chemin, arguments } => ("exe".into(), chemin, arguments),
        };
        let att = &c["desinstaller"];
        if k != s(&att["kind"]) || p != s(&att["path"]) || a != s(&att["args"]) {
            faux.push(format!("désinstaller {}: {k} {p} {a} au lieu de {att}", jeu.nom));
        }
    }
    assert!(faux.is_empty(), "{} écarts :\n{}", faux.len(), faux.join("\n"));
}

fn jeu_en_json(g: &biblio::Jeu) -> Value {
    json!({ "Name": g.nom, "Exes": g.exes, "Source": g.source, "Dir": g.dossier, "AppId": g.appid, "Launch": g.lancement,
        "Uninstall": g.desinstall, "Icon": g.icone, "Leftover": false, "Incomplet": false,
        "Art": g.art.as_ref().map(|a| json!({ "Cover": a.cover, "Hero": a.hero, "Logo": a.logo })) })
}

#[test]
fn bibliotheques_simulees() {
    let mut faux = Vec::new();
    for (i, c) in juge("biblio").as_array().unwrap().iter().enumerate() {
        let bac = Bac::neuf(&format!("biblio{i}"));
        let sys = faux_pc(&bac, &c["pc"]);
        let d = biblio::decouvrir(&sys);
        let mut v: Vec<Value> = d
            .restes
            .iter()
            .map(|r| {
                json!({ "Name": r.nom, "Exes": [], "Source": "Steam", "Dir": r.dossier, "AppId": "", "Launch": "", "Uninstall": "",
                    "Icon": "", "Leftover": true, "Incomplet": r.incomplet, "Art": null })
            })
            .collect();
        v.extend(d.jeux.iter().map(jeu_en_json));
        let obtenu = bac.norm_json(&Value::Array(v));
        let attendu = bac.norm_json(&c["jeux"]);
        let (a, o) = (attendu.as_array().unwrap(), obtenu.as_array().unwrap());
        for k in 0..a.len().max(o.len()) {
            if a.get(k) != o.get(k) {
                faux.push(format!(
                    "PC {i}, jeu {k} :\n  attendu {}\n  obtenu  {}",
                    a.get(k).unwrap_or(&Value::Null),
                    o.get(k).unwrap_or(&Value::Null)
                ));
            }
        }
    }
    assert!(faux.is_empty(), "{} écarts :\n{}", faux.len(), faux.join("\n"));
}

#[test]
fn barre_des_restes() {
    let bac = Bac::neuf("restes");
    for x in ["a", "b", "c", "d", "e"] {
        std::fs::create_dir_all(bac.dir.join(x)).unwrap();
    }
    let mut faux = Vec::new();
    let mut b = BlocJeux::avec(Arc::new(SystemeEssai::default()));
    let cas = juge("restes");
    for c in cas.as_array().unwrap() {
        let l: Vec<(String, String, f64, bool)> = c["restes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    s(&r["nom"]),
                    bac.sub(&s(&r["dossier"])),
                    r["mo"].as_f64().unwrap() * 1048576.0,
                    r["recent"].as_bool().unwrap(),
                )
            })
            .collect();
        let r = b.essai_barre_restes(&l, c["incomplet"].as_bool().unwrap()).map(|t| bac.norm(&t));
        let att = if c["texte"].is_null() { None } else { Some(s(&c["texte"])) };
        if r != att {
            faux.push(format!("{c}\n  obtenu {r:?}"));
        }
    }
    assert!(faux.is_empty(), "{} écarts :\n{}", faux.len(), faux.join("\n"));
}

/// Remplace les adresses d'images (jeton) par celle de la version PowerShell de l'essai (« https://fichier.local/<chemin> »).
fn images(b: &BlocJeux, v: &Value) -> Value {
    match v {
        Value::String(x) if x.starts_with("https://fichier.local/") => {
            let jeton = &x["https://fichier.local/".len()..];
            Value::String(format!(
                "https://fichier.local/{}",
                b.image(jeton).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| jeton.into())
            ))
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| images(b, x)).collect()),
        Value::Object(m) => Value::Object(m.iter().map(|(k, x)| (k.clone(), images(b, x))).collect()),
        x => x.clone(),
    }
}

/// Le premier écart entre deux JSON (chemin, attendu, obtenu).
fn ecart(chemin: &str, a: &Value, o: &Value, sortie: &mut Vec<String>) {
    match (a, o) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                ecart(
                    &format!("{chemin}.{k}"),
                    x.get(k).unwrap_or(&Value::Null),
                    y.get(k).unwrap_or(&Value::Null),
                    sortie,
                );
            }
            for k in y.keys() {
                if !x.contains_key(k) {
                    sortie.push(format!("{chemin}.{k} : en trop"));
                }
            }
            for k in x.keys() {
                if !y.contains_key(k) && !x[k].is_null() {
                    sortie.push(format!("{chemin}.{k} : absent"));
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                sortie.push(format!("{chemin} : {} éléments au lieu de {}", y.len(), x.len()));
            }
            for k in 0..x.len().min(y.len()) {
                ecart(&format!("{chemin}[{k}]"), &x[k], &y[k], sortie);
            }
        }
        (Value::Number(x), Value::Number(y)) if x.as_f64() == y.as_f64() => {}
        _ => {
            if a != o {
                sortie.push(format!("{chemin} : attendu {a}, obtenu {o}"));
            }
        }
    }
}

fn pages_juges(cas: &Value, sortie: &mut Vec<String>) -> usize {
    let mut n = 0;
    for (i, c) in cas.as_array().unwrap().iter().enumerate() {
        let bac = Bac::neuf(&format!("pages{i}"));
        let mut sys = faux_pc(&bac, &c["pc"]);
        sys.applis = c["applis"].as_array().unwrap().iter().map(|a| (s(&a["nom"]), s(&a["id"]))).collect();
        let ctx = contexte(&bac);
        let mut b = BlocJeux::avec(Arc::new(sys));
        b.heure = Some(Moment::depuis(2026, 11, 6, 15, 0, 0));
        let mut ef = Effets::default();
        if !c["jeuxNull"].as_bool().unwrap_or(false) {
            b.rechercher_maintenant(&ctx, &mut ef);
        }
        b.action(&ctx, "jeux-filtre", &c["filtre"], &mut ef);
        b.action(&ctx, "jeux-cherche", &c["recherche"], &mut ef);
        let analyse = c["analyse"].clone();
        b.signal(&ctx, "analyse", &analyse, &mut ef);
        b.signal(&ctx, "reglages-windows", &json!({ "reglages": c["reglages"] }), &mut ef);
        if !s(&c["enJeu"]).is_empty() {
            b.signal(&ctx, "partie-commencee", &json!({ "jeu": c["enJeu"] }), &mut ef);
        }
        if !s(&c["choisi"]).is_empty() {
            b.choisir(&s(&c["choisi"]));
        }
        let p = b.pages(&ctx, &s(&c["ecran"]));
        let mut obtenu = Map::new();
        for k in ["jeux", "jeu", "overlay"] {
            if let Some(v) = p.get(k) {
                obtenu.insert(k.into(), bac.norm_json(&images(&b, v)));
            }
        }
        let mut attendu = bac.norm_json(&c["pages"]);
        // L'onglet Lag en ligne est rempli par le bloc trafic (page « jeux-lag », mise en place par l'hôte)
        if let Some(lag) = attendu.pointer_mut("/jeux/onglets/1/onglets/1").and_then(|v| v.as_object_mut()) {
            lag.remove("module");
            lag.remove("blocs");
        }
        // Divergence voulue (DIVERGENCES.md, n° 1) : la version PowerShell ne lit jamais la priorité haute d'un jeu à un
        // seul programme (« $names[0] » prend la première lettre du nom) ; Rust la lit.
        if s(&c["nom"]) == "fiche rocket" {
            attendu["jeu"]["blocs"][2]["droite"] = json!("3 points à améliorer");
            attendu["jeu"]["blocs"][4]["bascule"]["oui"] = json!(true);
            attendu["jeu"]["blocs"][4]["couleur"] = json!("vert");
            attendu["jeu"]["blocs"][4]["icone"] = json!("ok");
        }
        let avant = sortie.len();
        ecart(&format!("[{}]", s(&c["nom"])), &attendu, &Value::Object(obtenu), sortie);
        if sortie.len() == avant {
            n += 1;
        }
    }
    n
}

#[test]
fn pages_jeux_et_fiche() {
    let mut faux = Vec::new();
    let cas = juge("pages");
    let n = pages_juges(&cas, &mut faux);
    println!("pages : {n} cas identiques sur {}", cas.as_array().unwrap().len());
    assert!(faux.is_empty(), "{} écarts :\n{}", faux.len(), faux.join("\n"));
}

/// Contre-épreuve : un octet changé dans un juge doit faire échouer la comparaison.
#[test]
fn contre_epreuve() {
    let mut cas = juge("pages");
    let c = &mut cas.as_array_mut().unwrap()[2];
    let t = c["pages"]["jeux"]["titre"].as_str().unwrap().to_string();
    c["pages"]["jeux"]["titre"] = Value::String(t.replace('J', "K"));
    let mut faux = Vec::new();
    pages_juges(&Value::Array(vec![c.clone()]), &mut faux);
    assert!(!faux.is_empty(), "le juge modifié devait échouer");
}

#[test]
fn suppression_des_restes_dans_un_bac() {
    // (comme tester-bibliotheque.ps1 : vrai reste supprimé ; cité par une fiche, récent ou hors steamapps : gardé)
    let bac = Bac::neuf("suppr");
    let sa = bac.dir.join("Lib").join("steamapps");
    let common = sa.join("common");
    for d in ["Vieux", "Cite", "Recent"] {
        std::fs::create_dir_all(common.join(d).join("Sauvegardes")).unwrap();
        std::fs::write(common.join(d).join("Sauvegardes").join("p.sav"), [0u8; 1024]).unwrap();
    }
    std::fs::write(
        sa.join("appmanifest_3.acf"),
        "\"AppState\"\n{\n\t\"appid\"\t\t\"3\"\n\t\"installdir\"\t\t\"Cite\"\n}\n",
    )
    .unwrap();
    for d in ["Vieux", "Cite"] {
        for p in [common.join(d).join("Sauvegardes").join("p.sav"), common.join(d).join("Sauvegardes"), common.join(d)]
        {
            dater(&p, 30.0);
        }
    }
    let ds: Vec<String> =
        ["Vieux", "Cite", "Recent"].iter().map(|d| common.join(d).to_string_lossy().to_string()).collect();
    let mut tout = ds.clone();
    tout.push(sa.to_string_lossy().to_string());
    let r = steam::supprimer_restes(&tout);
    assert_eq!(r[0].raison, "");
    assert!(!common.join("Vieux").exists());
    assert!(r[1].garde && r[1].raison.contains("Steam s'en sert de nouveau"));
    assert!(r[2].garde && r[2].raison.contains("moins de 7 jours"));
    assert!(r[3].raison.contains("pas un dossier de jeu"));
    assert!(common.join("Cite").exists() && common.join("Recent").exists() && sa.exists());
    let (t, recent) = steam::mesurer_reste(&ds[2], 7);
    assert_eq!(t, 1024.0);
    assert!(recent);
}

/// Cas jamais vus : la bibliothèque du vrai PC, LUE seulement (registre, dossiers des launchers), écrite dans le fichier
/// donné par NEVERMIND_PC_REEL pour la comparer avec Get-InstalledGames. Lancé à la main seulement (--ignored).
#[test]
#[ignore]
fn pc_reel_en_lecture() {
    let Ok(f) = std::env::var("NEVERMIND_PC_REEL") else { return };
    let d = biblio::decouvrir(&nevermind_jeux::systeme::SystemeWindows);
    let mut v: Vec<Value> = d.restes.iter().map(|r| json!({ "Name": r.nom, "Dir": r.dossier, "Source": "Steam", "Exes": [], "Launch": "", "Uninstall": "", "Icon": "", "AppId": "", "Leftover": true, "Incomplet": r.incomplet })).collect();
    v.extend(d.jeux.iter().map(jeu_en_json));
    std::fs::write(f, serde_json::to_string_pretty(&v).unwrap()).unwrap();
}
