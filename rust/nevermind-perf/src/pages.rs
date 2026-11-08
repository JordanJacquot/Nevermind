//! Les pages de l'écran « Performances » (Get-PagesPerf, ecran-perf.ps1) : Performances, Optimisation gaming (Réglages
//! Windows, Mode jeu, Profils par jeu), Démarrage et Nettoyage. La page RAM est au bloc « memoire ».
//!
//! Tout part d'une seule entrée (un objet JSON, le même que les juges exportés de PowerShell) : ce que le bloc sait
//! lui-même (mode jeu, partie, dernier jeu, mémoire, disque, profils) et ce que les autres blocs lui envoient par
//! signal (réglages pour les jeux, démarrage, nettoyage, connexion, analyse). Les champs sont décrits dans
//! `DIVERGENCES.md` (section « Entrée des pages »).

use crate::jeu::{self, APPS_MODE_JEU};
use crate::texte::{egal, n0, ps_int, s, taille, texte_kit};
use serde_json::{Map, Value, json};

/// Vrai au sens de PowerShell (`if ($x)`)
pub fn vrai(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(t)) => !t.is_empty(),
        Some(Value::Array(a)) => match a.len() {
            0 => false,
            1 => vrai(a.first()),
            _ => true,
        },
        Some(Value::Object(_)) => true,
    }
}
pub fn nb(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        Some(Value::String(t)) => t.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}
/// `[string]$x`
pub fn ch(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(t)) => t.clone(),
        Some(Value::Bool(b)) => if *b { "True" } else { "False" }.into(),
        Some(Value::Number(n)) => n.to_string(),
        Some(x) => x.to_string(),
    }
}
pub fn liste(v: Option<&Value>) -> Vec<Value> {
    match v {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a.clone(),
        Some(x) => vec![x.clone()],
    }
}
/// Un nombre comme dans un texte PowerShell (`"$x"`, culture invariante) : 12 donne « 12 », 12.5 donne « 12.5 »
pub fn nombre_texte(v: Option<&Value>) -> String {
    match v {
        Some(Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else {
                let x = n.as_f64().unwrap_or(0.0);
                if x.fract() == 0.0 && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") }
            }
        }
        other => ch(other),
    }
}

/// `$t.Recommended -ne $false` (absent ou vrai : conseillé)
fn conseille(r: &Value) -> bool {
    !matches!(r.get("recommande"), Some(Value::Bool(false)))
}
fn facultatif(r: &Value) -> bool {
    matches!(r.get("recommande"), Some(Value::Bool(false)))
}

/// Les réglages pour les jeux comptés une seule fois (Get-ReglagesKit) : (total conseillés, faits, reste, facultatifs)
pub fn compter_reglages(lignes: &[Value]) -> (i64, i64, i64, i64) {
    let cons: Vec<&Value> = lignes.iter().filter(|r| conseille(r)).collect();
    let faits = cons.iter().filter(|r| !vrai(r.get("active"))).count() as i64;
    let fac = lignes.iter().filter(|r| facultatif(r) && vrai(r.get("active"))).count() as i64;
    let total = cons.len() as i64;
    (total, faits, total - faits, fac)
}

/// Get-TitreReglagesKit
pub fn titre_reglages(n_lignes: usize, reste: i64) -> String {
    if n_lignes == 0 {
        "Réglages pour les jeux".into()
    } else if reste > 0 {
        format!("{reste} réglage{} à faire", s(reste))
    } else {
        "Tout est optimisé".into()
    }
}

/// Les profils par jeu (Get-ProfilsKit) : (jeux triés avec leur note, priorité et carte puissante ; deux cartes ?)
pub struct Profil {
    pub nom: String,
    pub note: String,
    pub prio: bool,
    pub gpu: Option<bool>,
    /// Le jeu tel qu'il est arrivé (pour l'action profil:k)
    pub jeu: Value,
}

pub fn profils(jeux: &[Value], gpus: &[Value]) -> (Vec<Profil>, bool) {
    let mut avec: Vec<&Value> =
        jeux.iter().filter(|g| !jeu::noms_exes_profil(&liste(g.get("exes"))).is_empty()).collect();
    avec.sort_by(|a, b| crate::texte::comparer_noms(&ch(a.get("nom")), &ch(b.get("nom"))));
    let deux = jeu::gpus_reelles(gpus).len() >= 2;
    let l = avec
        .into_iter()
        .map(|g| {
            let exes = liste(g.get("exes"));
            let src = if vrai(g.get("source")) { ch(g.get("source")) } else { "Steam".into() };
            Profil {
                nom: ch(g.get("nom")),
                note: format!("{src} · {}", jeu::noms_exes_profil(&exes).join(", ")),
                prio: vrai(g.get("prio")),
                gpu: if deux { Some(jeu::preference_puissante(&ch(g.get("gpu")))) } else { None },
                jeu: g.clone(),
            }
        })
        .collect();
    (l, deux)
}

/// Le texte d'état du mode jeu (Update-GameModeStatus)
pub fn etat_mode_jeu(e: &Value) -> String {
    let session = e.get("session").filter(|x| !x.is_null());
    let n_jeux = e.get("jeux").filter(|x| !x.is_null()).map(|x| liste(Some(x)).len() as i64).unwrap_or(0);
    jeu::etat_mode_jeu(
        vrai(e.get("modeJeu")),
        session.map(|x| (ch(x.get("jeu")), liste(x.get("fermees")).len() as i64)),
        n_jeux,
    )
}

/// Les pages, comme Get-PagesPerf (sans la page RAM)
pub fn pages(e: &Value) -> Map<String, Value> {
    let lignes_reg = liste(e.get("reglages"));
    let (total, ok_n, reste, facultatifs) = compter_reglages(&lignes_reg);
    let inter = e.get("internet").filter(|x| vrai(Some(x)));
    let bridage = vrai(e.get("bridage"));

    let mut resume: Vec<Value> = Vec::new();
    if let Some(last) = e.get("dernier").filter(|x| !x.is_null()) {
        if vrai(last.get("moy")) {
            resume.push(json!({ "nom": "Dernière partie", "valeur": format!("{} FPS", n0(nb(last.get("moy")))) }));
        }
        resume.push(json!({ "nom": "Jeu", "valeur": ch(last.get("jeu")) }));
    }
    let anneau =
        if total > 0 { Some(json!({ "valeur": ok_n, "sur": total, "petit": format!("/{total}") })) } else { None };
    let titre_reg = titre_reglages(lignes_reg.len(), reste);
    let texte_reg = if total > 0 {
        let mut t = format!("{ok_n} réglage{p} conseillé{p} sur {total} déjà fait{p}.", p = s(ok_n));
        if facultatifs > 0 {
            t.push_str(&format!(" {facultatifs} réglage{p} facultatif{p} : à toi de voir.", p = s(facultatifs)));
        }
        t
    } else {
        "Les réglages se chargent.".into()
    };

    // Démarrage
    let entrees = liste(e.get("demarrage"));
    let on = entrees.iter().filter(|x| vrai(x.get("actif"))).count() as i64;
    let conseil = entrees.iter().filter(|x| egal(&ch(x.get("genre")), "safe") && vrai(x.get("actif"))).count() as i64;
    let a_voir = entrees.iter().filter(|x| egal(&ch(x.get("genre")), "choice") && vrai(x.get("actif"))).count() as i64;
    // Nettoyage
    let c_rows = liste(e.get("nettoyage"));
    let mut tout = 0.0;
    let mut choisi = 0.0;
    for r in &c_rows {
        tout += nb(r.get("taille"));
        if vrai(r.get("coche")) {
            choisi += nb(r.get("taille"));
        }
    }
    // RAM : le constat s'il est fait, sinon la mesure en direct
    let mem = e.get("memoire").filter(|x| !x.is_null());
    let ram = e.get("ram").filter(|x| !x.is_null());
    let ram_p: Option<i64> = if let Some(m) = mem {
        Some(ps_int(nb(m.get("pourcent"))))
    } else {
        ram.filter(|r| vrai(r.get("totale"))).map(|r| ps_int(100.0 * nb(r.get("utilisee")) / nb(r.get("totale"))))
    };
    let ram_alerte = mem.is_some_and(|m| nb(m.get("alertes")) > 0.0);

    // Conseillé pour toi
    let conseils_src: Vec<Value> = liste(e.get("analyse"))
        .into_iter()
        .filter(|a| ch(a.get("id")).to_lowercase().starts_with("tweak:") && egal(&ch(a.get("statut")), "warn"))
        .collect();
    let conseils: Vec<Value> = conseils_src
        .iter()
        .enumerate()
        .map(|(k, f)| {
            json!({ "type": "ligne", "id": format!("conseil:{k}"), "icone": "manette", "texte": texte_kit(&ch(f.get("titre"))),
                "note": texte_kit(&ch(f.get("detail"))), "etiquette": "Conseillé", "ton": "attention" })
        })
        .collect();

    let mut pages = Map::new();
    let module_perf = if let Some(a) = &anneau {
        json!({ "anneau": a, "titre": titre_reg, "texte": texte_reg, "boutons": [{ "id": "optimisation", "texte": "Voir les réglages" }], "resume": resume })
    } else {
        json!({ "icone": "eclair", "titre": titre_reg, "texte": texte_reg, "resume": resume })
    };
    let ton_statut = |st: &str| -> Value {
        match st.to_lowercase().as_str() {
            "ok" => json!("ok"),
            "warn" => json!("attention"),
            "bad" => json!("urgent"),
            "info" => json!(""),
            _ => Value::Null,
        }
    };
    let elements = json!([
        { "id": "optimisation", "nom": "Optimisation gaming", "note": "Réglages, Mode jeu, profils par jeu", "icone": "manette", "couleur": "P",
          "etiquette": if reste > 0 { json!(format!("{reste} à faire")) } else if total > 0 { json!("Optimisé") } else { Value::Null },
          "ton": if reste > 0 { "attention" } else if total > 0 { "ok" } else { "" } },
        { "id": "demarrage", "nom": "Démarrage", "note": "Ce qui se lance avec le PC", "icone": "marche", "couleur": "S",
          "etiquette": if !entrees.is_empty() { json!(format!("{on} programme{}", s(on))) } else { Value::Null },
          "ton": if conseil > 0 { "attention" } else { "" } },
        { "id": "nettoyage", "nom": "Nettoyage", "note": "Les fichiers inutiles", "icone": "balai", "couleur": "T",
          "etiquette": if !c_rows.is_empty() { json!(taille(tout)) } else { Value::Null },
          "ton": if tout > 1073741824.0 { "attention" } else if !c_rows.is_empty() { "ok" } else { "" } },
        { "id": "memoire", "nom": "RAM", "note": "Ce qui prend la mémoire", "icone": "memoire", "couleur": "P",
          "etiquette": match ram_p { Some(p) => json!(format!("{p} %")), None => Value::Null },
          "ton": if ram_alerte || ram_p.is_some_and(|p| p >= 80) { "attention" } else { "" } },
        { "id": "connexion", "nom": "Connexion", "note": "Temps de réponse (ping), stabilité, DNS", "icone": "ondes", "couleur": "P",
          "etiquette": if bridage { json!("Carte trop lente") } else if let Some(i) = inter { json!(format!("{} ms", nombre_texte(i.get("moy")))) } else { json!("À tester") },
          "ton": if bridage { json!("attention") } else if let Some(i) = inter { ton_statut(&ch(i.get("statut"))) } else { json!("") } }
    ]);
    let mut blocs_perf = vec![
        json!({ "type": "titre", "texte": "Pour gagner des FPS" }),
        json!({ "type": "outils", "elements": elements }),
    ];
    // La carte du mode streameur (bloc stream, signal « streameur ») : grisée sans OBS ni Streamlabs
    if let Some(c) = e.get("streameur").filter(|c| c.is_object()) {
        blocs_perf.push(json!({ "type": "titre", "texte": "Pour streamer" }));
        blocs_perf.push(json!({ "type": "outils", "elements": [c] }));
    }
    if !conseils.is_empty() {
        blocs_perf.push(json!({ "type": "titre", "texte": "Conseillé pour toi" }));
    }
    blocs_perf.extend(conseils);
    pages.insert(
        "perf".into(),
        json!({ "titre": "Performances", "sous": "Pour gagner des FPS (images par seconde : plus il y en a, plus le jeu est fluide). Les réglages de Windows, le démarrage, la place sur le disque.",
            "icone": "eclair", "couleur": "P", "module": module_perf, "blocs": blocs_perf }),
    );

    // ---- Optimisation gaming
    let lignes: Vec<Value> = lignes_reg
        .iter()
        .enumerate()
        .map(|(k, r)| {
            let fait = !vrai(r.get("active"));
            let fac = facultatif(r);
            let impact = ch(r.get("impact"));
            let etq = if fait {
                "Optimisé".to_string()
            } else if fac {
                "Facultatif".to_string()
            } else {
                format!("Impact {}", impact.to_lowercase())
            };
            let ton = if fait {
                "ok"
            } else if fac {
                ""
            } else if egal(&impact, "Important") {
                "urgent"
            } else if egal(&impact, "Moyen") {
                "attention"
            } else {
                ""
            };
            let mut l = json!({ "type": "ligne", "texte": texte_kit(&ch(r.get("titre"))), "note": texte_kit(&ch(r.get("description"))), "etiquette": etq, "ton": ton });
            if !fait {
                l["case"] = json!({ "id": "tweak", "valeur": k.to_string(), "oui": vrai(r.get("coche")) });
            }
            if vrai(r.get("redemarrage")) && !fait {
                let n = ch(l.get("note"));
                l["note"] = json!(format!("{n} Il faut redémarrer le PC ensuite."));
            }
            l
        })
        .collect();
    let coches = lignes_reg.iter().filter(|r| vrai(r.get("active")) && vrai(r.get("coche"))).count() as i64;
    let mut mod_reg = json!({ "titre": titre_reg, "texte": texte_reg });
    if anneau.is_some() {
        mod_reg["anneau"] = json!({ "valeur": ok_n, "sur": total, "petit": format!("/{total}"), "ton": if reste > 0 { "attention" } else { "" } });
    } else {
        mod_reg["icone"] = json!("reglages");
        mod_reg["couleur"] = json!("P");
    }
    let mut boutons = Vec::new();
    if coches > 0 {
        boutons.push(json!({ "id": "clic:BtnApply", "texte": if coches > 1 { format!("Appliquer ({coches})") } else { "Appliquer".into() } }));
    }
    if reste > 0 {
        boutons.push(json!({ "id": "opti:recommande", "texte": "Cocher le recommandé", "style": if coches > 0 { json!("second") } else { Value::Null } }));
    }
    mod_reg["boutons"] = Value::Array(boutons);
    if coches > 0 {
        mod_reg["case"] = json!({ "id": "coche:ChkRestore", "texte": "Créer un point de restauration avant (conseillé)", "oui": vrai(e.get("restauration")) });
    }
    let sel = jeu::selection(e.get("appsModeJeu"));
    let mode_on = vrai(e.get("modeJeu"));
    let etat = etat_mode_jeu(e);
    let etat_mode = if mode_on && !etat.is_empty() { format!(" {etat}") } else { String::new() };
    let presentes = presentes(e);
    let apps_la: Vec<&jeu::AppModeJeu> = APPS_MODE_JEU.iter().filter(|a| presentes.iter().any(|p| p == a.id)).collect();
    let autres: Vec<&str> =
        APPS_MODE_JEU.iter().filter(|a| !presentes.iter().any(|p| p == a.id)).map(|a| a.nom).collect();
    let mod_mode = json!({ "icone": "manette", "couleur": if mode_on { "vert" } else { "doux" }, "titre": format!("Mode jeu {}", if mode_on { "activé" } else { "désactivé" }),
        "texte": format!("Quand un de tes jeux démarre, les applis cochées sont fermées, puis relancées quand tu quittes le jeu.{etat_mode}"),
        "case": { "id": "modejeu", "texte": "Fermer des applis pendant que je joue", "oui": mode_on },
        "resume": [ { "nom": "Applis cochées", "valeur": sel.len().to_string() }, { "nom": "Ouvertes en ce moment", "valeur": apps_la.len().to_string() } ] });
    let mut mode_jeu = vec![json!({ "type": "titre", "texte": "Applis à fermer", "droite": "Ouvertes en ce moment" })];
    if apps_la.is_empty() {
        mode_jeu.push(json!({ "type": "texte", "texte": "Aucune des applis que Nevermind sait fermer n'est ouverte en ce moment." }));
    }
    for a in &apps_la {
        mode_jeu.push(json!({ "type": "case", "id": "modejeu-app", "valeur": a.id, "texte": a.nom, "oui": sel.iter().any(|x| egal(x, a.id)) }));
    }
    if !autres.is_empty() {
        mode_jeu.push(json!({ "type": "note", "texte": format!("Nevermind sait aussi fermer {} : elles ne sont pas ouvertes en ce moment, elles apparaîtront ici quand elles le seront.", autres.join(", ")) }));
    }
    mode_jeu.push(json!({ "type": "note", "texte": "Ne coche pas le lanceur du jeu auquel tu joues (Epic, EA, Ubisoft). Marche tant que Nevermind est ouvert, même réduit." }));

    let bouton_ajout = json!({ "id": "ajouterjeu", "texte": "Ajouter un jeu" });
    let mut mod_profils = json!({ "icone": "manette", "couleur": "S", "titre": "Profils par jeu", "texte": "Lecture des profils de tes jeux...", "boutons": [bouton_ajout] });
    let mut blocs_profils: Vec<Value> = Vec::new();
    let jeux = e.get("jeux").filter(|x| !x.is_null());
    let ecran = ch(e.get("ecran"));
    let profils_lus = vrai(e.get("profilsLus")) && jeux.is_some();
    let mut resume_profils = json!("...");
    if jeux.is_none() {
        blocs_profils.push(json!({ "type": "texte", "texte": "Recherche des jeux installés..." }));
        mod_profils["texte"] = json!("Recherche des jeux installés...");
    } else if !egal(&ecran, "optimisation") && !profils_lus {
        blocs_profils.push(json!({ "type": "texte", "texte": "Lecture des profils..." }));
    } else {
        let (pr, deux) = profils(&liste(jeux), &liste(e.get("gpus")));
        blocs_profils.push(json!({ "type": "texte", "texte": "Ces réglages sont appliqués par Windows à chaque lancement du jeu, même Nevermind fermé. « Priorité haute » : le jeu passe avant les autres programmes." }));
        let n_p = pr.len() as i64;
        let n_prio = pr.iter().filter(|x| x.prio).count() as i64;
        resume_profils = json!(format!("{n_prio} en priorité haute"));
        mod_profils = if n_p > 0 {
            let x = if n_p > 1 { "x" } else { "" };
            let mut r = vec![
                json!({ "nom": "Jeux reconnus", "valeur": n_p.to_string() }),
                json!({ "nom": "Priorité haute", "valeur": n_prio.to_string() }),
            ];
            if deux {
                r.push(json!({ "nom": "Carte puissante", "valeur": pr.iter().filter(|p| p.gpu == Some(true)).count().to_string() }));
            }
            json!({ "anneau": { "valeur": n_prio, "sur": n_p, "petit": format!("/{n_p}") }, "titre": format!("{n_prio} jeu{} en priorité haute", if n_prio > 1 { "x" } else { "" }),
                "texte": format!("Sur {n_p} jeu{x} dont Nevermind connaît le programme."), "boutons": [bouton_ajout], "resume": r })
        } else {
            json!({ "icone": "manette", "couleur": "S", "titre": "Aucun jeu reconnu", "texte": "Nevermind ne connaît le programme d'aucun de tes jeux pour l'instant. Ajoute ton jeu pour lui faire un profil.", "boutons": [bouton_ajout] })
        };
        blocs_profils.push(json!({ "type": "titre", "texte": format!("{n_p} jeu{} dont Nevermind connaît le programme", if n_p > 1 { "x" } else { "" }) }));
        let n_lib = nb(e.get("nLib")) as i64;
        if n_lib > n_p {
            let d = n_lib - n_p;
            let qui = if d > 1 { format!("{d} d'entre eux") } else { "l'un d'eux".into() };
            blocs_profils.push(json!({ "type": "texte", "texte": format!("Ta bibliothèque compte {n_lib} jeux : pour {qui}, Nevermind ne connaît pas le programme du jeu, il ne peut donc pas lui faire de profil.") }));
        }
        for (k, x) in pr.iter().enumerate() {
            let mut b = vec![json!({ "id": format!("profil:{k}:priority"), "texte": "Priorité haute", "oui": x.prio })];
            if deux {
                b.push(
                    json!({ "id": format!("profil:{k}:gpu"), "texte": "Carte puissante", "oui": x.gpu == Some(true) }),
                );
            }
            blocs_profils.push(json!({ "type": "ligne", "icone": "manette", "couleur": "S", "texte": x.nom, "note": x.note, "bascules": b }));
        }
    }
    let mut blocs_reg = vec![json!({ "type": "titre", "texte": "Les réglages", "droite": "Chacun s'annule" })];
    blocs_reg.extend(lignes);
    blocs_reg.push(json!({ "type": "note", "texte": "Seulement des réglages à l'effet reconnu : pas de « réglages miracles ». Coche ceux que tu veux, puis « Appliquer ». Chacun s'annule depuis la page Sauvegarde." }));
    pages.insert(
        "optimisation".into(),
        json!({ "titre": "Optimisation gaming", "parent": "perf", "sous": "Règle Windows pour tes jeux : réglages, Mode jeu et profils par jeu.", "icone": "manette", "couleur": "P",
            "module": { "icone": "manette", "couleur": "P", "titre": "Pour tes jeux", "texte": "Trois façons de régler Windows pour tes jeux : les réglages, le Mode jeu et les profils par jeu.",
                "resume": [
                    { "nom": "Réglages conseillés", "valeur": if total > 0 { format!("{ok_n} sur {total} faits") } else { "...".into() }, "ton": if reste > 0 { "attention" } else if total > 0 { "ok" } else { "" } },
                    { "nom": "Mode jeu", "valeur": if mode_on { "Activé" } else { "Désactivé" }, "ton": if mode_on { "ok" } else { "" } },
                    { "nom": "Profils par jeu", "valeur": resume_profils }
                ] },
            "onglets": [
                { "id": "reglages", "nom": "Réglages Windows", "module": mod_reg, "blocs": blocs_reg },
                { "id": "mode", "nom": "Mode jeu", "module": mod_mode, "blocs": mode_jeu },
                { "id": "profils", "nom": "Profils par jeu", "module": mod_profils, "blocs": blocs_profils }
            ] }),
    );

    // ---- Démarrage
    let lignes_dem: Vec<Value> = entrees
        .iter()
        .enumerate()
        .map(|(k, x)| {
            let actif = vrai(x.get("actif"));
            let genre = ch(x.get("genre")).to_lowercase();
            let (mut etq, mut why, mut ton) = (ch(x.get("etiquette")), texte_kit(&ch(x.get("pourquoi"))), match genre.as_str() {
                "safe" => "ok",
                "keep" => "attention",
                _ => "",
            });
            if !actif {
                if genre == "keep" {
                    etq = "Coupé".into();
                    why = "Pilote ou protection de ton PC : mieux vaut le remettre.".into();
                    ton = "attention";
                } else {
                    etq = "Coupé au démarrage".into();
                    why = "Il ne se lance plus avec le PC. Il s'ouvre toujours quand tu cliques dessus.".into();
                    ton = "";
                }
            }
            let image = x.get("image").filter(|i| vrai(Some(i))).map(|i| json!(ch(Some(i)))).unwrap_or(Value::Null);
            json!({ "type": "ligne", "image": image, "icone": "app", "couleur": "doux", "texte": ch(x.get("nom")), "note": why, "etiquette": etq, "ton": ton,
                "bascule": { "id": format!("dem:{k}"), "oui": actif }, "fait": !actif })
        })
        .collect();
    let module_dem = if !entrees.is_empty() && on > 0 && conseil > 0 {
        let fin = if conseil == 1 {
            "1 ne sert pas dès l'allumage. Le couper rend l'allumage plus rapide, sans rien désinstaller.".to_string()
        } else {
            format!(
                "{conseil} ne servent pas dès l'allumage. Les couper rend l'allumage plus rapide, sans rien désinstaller."
            )
        };
        json!({ "anneau": { "valeur": conseil, "sur": on, "petit": format!("/{on}") }, "titre": format!("{conseil} à couper"),
            "texte": format!("Sur {on} programme{} au démarrage, {fin}", s(on)), "boutons": [{ "id": "clic:BtnDisableStartup", "texte": format!("Couper ce qui est conseillé ({conseil})") }] })
    } else if on > 0 && a_voir > 0 {
        let dep = if a_voir == 1 { "1 dépend".to_string() } else { format!("{a_voir} dépendent") };
        json!({ "icone": "marche", "couleur": "S", "titre": "À toi de voir",
            "texte": format!("Sur {on} programme{} au démarrage, aucun n'est inutile à coup sûr. {dep} de ton usage : regarde ceux marqués « À toi de voir ». Rien n'est supprimé si tu en coupes un.", s(on)) })
    } else {
        let titre = if on > 0 {
            "Rien à couper"
        } else if !entrees.is_empty() {
            "Rien ne se lance"
        } else {
            "Lecture en cours"
        };
        let texte = if on == 1 {
            "1 programme se lance avec le PC, et il est utile. Tu peux quand même le couper, rien n'est supprimé."
                .to_string()
        } else if on > 0 {
            format!(
                "{on} programmes se lancent avec le PC, et tous sont utiles. Tu peux quand même en couper un, rien n'est supprimé."
            )
        } else {
            ch(e.get("texteDemarrage"))
        };
        json!({ "icone": "marche", "couleur": "S", "titre": titre, "texte": texte })
    };
    let mut blocs_dem =
        vec![json!({ "type": "titre", "texte": "Au démarrage", "droite": "Activé : se lance avec le PC" })];
    if lignes_dem.is_empty() {
        blocs_dem.push(json!({ "type": "texte", "texte": "Aucun programme ne se lance avec Windows." }));
    } else {
        blocs_dem.extend(lignes_dem);
    }
    blocs_dem.push(json!({ "type": "note", "texte": "Rien n'est supprimé : un programme coupé s'ouvre toujours quand tu cliques dessus, et tu peux le remettre quand tu veux." }));
    pages.insert(
        "demarrage".into(),
        json!({ "titre": "Démarrage", "parent": "perf", "sous": "Les programmes qui se lancent tout seuls quand tu allumes ton PC.", "icone": "marche", "couleur": "S",
            "actions": [{ "id": "clic:BtnRefreshStartup", "texte": "Actualiser", "icone": "actu" }], "module": module_dem, "blocs": blocs_dem }),
    );

    // ---- Nettoyage
    let lignes_net: Vec<Value> = c_rows
        .iter()
        .enumerate()
        .map(|(k, r)| {
            let vide = !vrai(r.get("active"));
            let t = nb(r.get("taille"));
            let mut l = json!({ "type": "ligne", "texte": ch(r.get("titre")), "note": ch(r.get("desc")), "valeur": if vide { "0".to_string() } else { taille(t) },
                "ton": if t > 524288000.0 { "attention" } else { "" } });
            if vide {
                l["etiquette"] = json!("Déjà propre");
                l["ton"] = json!("ok");
                l["fait"] = json!(true);
            } else {
                l["case"] = json!({ "id": "nett", "valeur": k.to_string(), "oui": vrai(r.get("coche")) });
                l["bouton"] = json!({ "id": format!("nett-voir:{k}"), "texte": "Voir", "style": "second" });
            }
            l
        })
        .collect();
    let lecteur = {
        let l = ch(e.get("lecteur"));
        if l.is_empty() { "C".to_string() } else { l.trim_end_matches(':').to_string() }
    };
    let disque = e.get("disque").filter(|d| !d.is_null() && vrai(d.get("total")));
    let module_net = if let Some(d) = disque {
        let tot = nb(d.get("total"));
        let libre = nb(d.get("libre"));
        let p = ps_int(100.0 * (tot - libre) / tot);
        let suite = if !c_rows.is_empty() {
            format!(" {} choisis pour le nettoyage.", taille(choisi))
        } else {
            " Clique sur Analyser pour voir ce qui peut partir.".into()
        };
        json!({ "anneau": { "valeur": p, "sur": 100, "petit": "%", "ton": if p >= 90 { json!("attention") } else { Value::Null } }, "titre": format!("Disque {lecteur} rempli à {p} %"),
            "texte": format!("{} libres sur {}.{suite}", taille(libre), taille(tot)),
            "boutons": [ if !c_rows.is_empty() { json!({ "id": "clic:BtnClean", "texte": ch(e.get("boutonNettoyer")) }) } else { json!({ "id": "clic:BtnCleanScan", "texte": "Analyser" }) } ] })
    } else {
        json!({ "icone": "balai", "couleur": "T", "titre": "Nettoyage", "texte": ch(e.get("texteNettoyage")), "boutons": [{ "id": "clic:BtnCleanScan", "texte": "Analyser" }] })
    };
    let mut blocs_net =
        vec![json!({ "type": "titre", "texte": "Ce qui peut partir", "droite": "Ceux de moins de 24 h sont gardés" })];
    if c_rows.is_empty() {
        blocs_net
            .push(json!({ "type": "texte", "texte": "Clique sur « Analyser » pour voir ce qui peut être libéré." }));
    }
    blocs_net.extend(lignes_net);
    if let Some(d) = e.get("dernierNettoyage").filter(|d| !d.is_null() && vrai(d.get("date")))
        && let Some(date) = crate::jeu::date_iso(&ch(d.get("date")))
    {
        let sup = nb(d.get("supprimes")) as i64;
        let lai = nb(d.get("laisses")) as i64;
        blocs_net.push(json!({ "type": "ligne", "icone": "horloge", "texte": format!("Dernier nettoyage le {:02}/{:02} à {:02}:{:02}", date.2, date.1, date.3, date.4),
            "note": format!("{} libérés, {} fichier{p} supprimé{p}, {} laissé{q} car utilisé{q}", taille(nb(d.get("libere"))), nombre_texte(d.get("supprimes")), nombre_texte(d.get("laisses")), p = s(sup), q = s(lai)),
            "bouton": if vrai(d.get("fichier")) { json!({ "id": "nett-journal", "texte": "Ouvrir le journal", "style": "second" }) } else { Value::Null } }));
    }
    blocs_net.push(json!({ "type": "note", "texte": "Nevermind ne touche jamais à tes documents, à la corbeille ni aux fichiers que les jeux préparent pour l'affichage (caches de shaders) : les vider provoquerait des saccades le temps qu'ils se reconstruisent." }));
    pages.insert(
        "nettoyage".into(),
        json!({ "titre": "Nettoyage", "parent": "perf", "sous": "Libère de la place en supprimant les fichiers inutiles.", "icone": "balai", "couleur": "T",
            "actions": if c_rows.is_empty() { json!([]) } else { json!([{ "id": "clic:BtnCleanScan", "texte": "Analyser de nouveau", "icone": "actu" }]) },
            "module": module_net, "blocs": blocs_net }),
    );
    pages
}

/// Les applis du Mode jeu présentes en ce moment (Get-AppsModeJeuKit) : ouvertes, ou fermées pendant la partie
pub fn presentes(e: &Value) -> Vec<String> {
    let noms: Vec<String> = liste(e.get("processus")).iter().map(|p| ch(Some(p))).collect();
    let fermees: Vec<String> = e
        .get("session")
        .filter(|x| !x.is_null())
        .map(|x| liste(x.get("fermees")).iter().map(|n| ch(Some(n))).collect())
        .unwrap_or_default();
    APPS_MODE_JEU
        .iter()
        .filter(|a| noms.iter().any(|n| a.correspond(n)) || fermees.iter().any(|f| egal(f, a.nom)))
        .map(|a| a.id.to_string())
        .collect()
}
