//! Les pages « jeux » et « jeu » (Get-PagesJeux d'ecran-jeux.ps1), au champ près.

use crate::biblio::Jeu;

use crate::lancement::Lancement;
use crate::parties::{self, JournalParties, PartieFps};
use crate::texte::{self, derniere_partie, duree_jeu, entier_ps, n0, texte_recherche};
use crate::{BlocJeux, optim};
use nevermind_noyau::Contexte;
use serde_json::{Map, Value, json};

fn pluriel(n: usize, s: &str) -> &str {
    if n > 1 { s } else { "" }
}

fn coin(c: &str) -> &'static str {
    match c {
        "hd" => "En haut à droite",
        "bg" => "En bas à gauche",
        "bd" => "En bas à droite",
        _ => "En haut à gauche",
    }
}

pub fn pages(b: &mut BlocJeux, ctx: &Contexte, ecran: &str) -> Map<String, Value> {
    let mut pages = Map::new();
    let tous: Vec<Jeu> = if b.jeux.is_some() { b.bibliotheque() } else { Vec::new() };
    let log = JournalParties::lire(Some(ctx), &ctx.donnees);
    let fps_brut = parties::parties_fps(Some(ctx), &ctx.donnees);
    let last = b.dernier_jeu(ctx, &log, &fps_brut);
    let lourd = ecran == "jeux" || ecran == "jeu";
    let maintenant = b.maintenant();

    // ---- Bibliothèque
    let filtre = if b.filtre.is_empty() { "Tous".to_string() } else { b.filtre.clone() };
    let q = b.recherche.trim().to_string();
    let qn = texte_recherche(&q);
    // Les launchers, du plus fourni au moins fourni
    let mut sources: Vec<(String, usize)> = Vec::new();
    for g in &tous {
        let s = g.source_affichee().to_string();
        match sources.iter_mut().find(|(x, _)| x.to_lowercase() == s.to_lowercase()) {
            Some(x) => x.1 += 1,
            None => sources.push((s, 1)),
        }
    }
    crate::texte::trier_ps(&mut sources, |a, b| b.1.cmp(&a.1));
    let recents = tous.iter().filter(|g| log.contient(&g.nom)).count();
    let mut montres: Vec<Jeu> = tous
        .iter()
        .filter(|g| {
            let src = g.source_affichee();
            (filtre == "Tous" || (filtre == "Récents" && log.contient(&g.nom)) || src.eq_ignore_ascii_case(&filtre))
                && (q.is_empty() || texte_recherche(&g.nom).contains(&qn))
        })
        .cloned()
        .collect();
    let derniere = |g: &Jeu| log.get(&g.nom).map(|p| p.last.clone()).unwrap_or_default();
    crate::texte::trier_ps(&mut montres, |a, c| {
        texte::comparer_culture(&derniere(c), &derniere(a)).then_with(|| texte::comparer_culture(&a.nom, &c.nom))
    });
    b.montres = montres.clone();
    let en_jeu = b.en_jeu.clone();
    let mut grille = Vec::new();
    if lourd {
        for (k, g) in montres.iter().take(120).enumerate() {
            let sous = match log.get(&g.nom) {
                Some(pl) => format!("Joué {}", derniere_partie(&pl.last, maintenant)),
                None => g.source_affichee().to_string(),
            };
            let image = b.adresse_jaquette(ctx, g, &tous);
            let marque = if en_jeu.as_deref() == Some(g.nom.as_str()) { json!("En jeu") } else { Value::Null };
            grille.push(json!({ "id": format!("jeu:{k}"), "titre": g.nom, "sous": sous, "image": image,
                "couleur": (["P", "S", "T"][k % 3]), "marque": marque }));
        }
    }
    let mut choix = vec![json!({ "id": "Tous", "texte": format!("Tous ({})", tous.len()) })];
    if recents > 0 {
        choix.push(json!({ "id": "Récents", "texte": format!("Récents ({recents})") }));
    }
    for (n, c) in &sources {
        choix.push(json!({ "id": n, "texte": format!("{n} ({c})") }));
    }
    let mut blocs_lib = vec![
        json!({ "type": "champ", "id": "jeux-cherche", "recherche": true, "valeur": q, "indice": "Chercher un jeu..." }),
    ];
    if !tous.is_empty() {
        blocs_lib.push(json!({ "type": "choix", "id": "jeux-filtre", "valeur": filtre, "options": choix }));
    }
    blocs_lib.push(json!({ "type": "titre", "texte": "Ta bibliothèque", "droite": format!("{} jeu{}", montres.len(), pluriel(montres.len(), "x")) }));
    if b.jeux.is_none() {
        blocs_lib.push(json!({ "type": "texte", "texte": "Recherche de tes jeux..." }));
    } else if montres.is_empty() {
        let t = if !tous.is_empty() {
            "Aucun jeu ne correspond."
        } else {
            "Aucun jeu trouvé sur ce PC. Clique sur « Ajouter un jeu » pour ajouter le tien."
        };
        blocs_lib.push(json!({ "type": "texte", "texte": t }));
    } else if !grille.is_empty() {
        blocs_lib.push(json!({ "type": "grille", "elements": grille }));
    } else {
        blocs_lib.push(json!({ "type": "texte", "texte": "Chargement des jaquettes..." }));
    }
    if let Some(t) = b.barre_restes() {
        blocs_lib.push(json!({ "type": "ligne", "icone": "poubelle", "couleur": "orange", "texte": t,
            "bouton": { "id": "clic:BtnLibLeftovers", "texte": "Voir", "style": "second" } }));
    }
    let couv = BlocJeux::reglage_bool(ctx, "LibCoversOnline", false);
    blocs_lib.push(json!({ "type": "reglages", "rangs": [{ "texte": "Jaquettes depuis Internet",
        "note": "Les images des jeux qui n'en ont pas sont cherchées sur la boutique Steam puis Wikipédia (seul le nom du jeu est envoyé).",
        "bascule": { "id": "coche:ChkLibCovers", "oui": couv } }] }));

    // ---- Perf. In Game : FPS
    let mut sessions: Vec<PartieFps> = fps_brut.clone();
    crate::texte::trier_ps(&mut sessions, |a, c| c.moment.cmp(&a.moment));
    let max12 = sessions.iter().take(12).map(|s| s.avg).fold(0.0f64, f64::max);
    let max_fps = 60.0f64.max(max12);
    let mesure = BlocJeux::mesure_fps(ctx);
    let overlay = BlocJeux::reglage_bool(ctx, "FpsOverlay", false);
    let style = match ctx.reglage("FpsOverlayStyle") {
        Some(Value::String(s)) if s == "discret" => "discret",
        _ => "complet",
    };
    let coin_id = match ctx.reglage("FpsOverlayCorner") {
        Some(Value::String(s)) if ["hg", "hd", "bg", "bd"].contains(&s.as_str()) => s,
        _ => "hg".to_string(),
    };
    let texte_compteur = if overlay {
        format!("affiché, style {style}, {}", coin(&coin_id).to_lowercase())
    } else {
        "masqué".to_string()
    };
    let mut blocs_fps = vec![json!({ "type": "reglages", "rangs": [
        { "texte": "Mesurer mes FPS quand je joue", "note": "Automatique pour tes jeux (Steam, Epic, Ubisoft, EA, Battle.net, Riot, GOG, Xbox...). Pour un autre jeu : Ctrl + Maj + F pendant la partie.", "bascule": { "id": "fps-mesure", "oui": mesure } },
        { "texte": "Le compteur par-dessus le jeu", "note": format!("Compteur à l'écran : {texte_compteur}."), "lien": { "id": "overlay", "texte": "Régler le compteur" } }
    ] })];
    if !ctx.appli.join("outils-tiers").join("PresentMon.exe").exists() {
        blocs_fps.push(
            json!({ "type": "note", "texte": "PresentMon est absent du dossier de Nevermind : réinstalle Nevermind." }),
        );
    }
    if let Some(s0) = sessions.first() {
        let mut cmp: Vec<&PartieFps> =
            fps_brut.iter().filter(|s| s.cle.to_lowercase() == s0.cle.to_lowercase()).collect();
        crate::texte::trier_ps(&mut cmp, |a, c| a.moment.cmp(&c.moment));
        if let Some(lc) = parties::dernier_changement(Some(ctx), &ctx.donnees) {
            let av: Vec<&PartieFps> =
                cmp.iter().filter(|s| s.moment.map(|m| m < lc).unwrap_or(false)).copied().collect();
            let ap: Vec<&PartieFps> =
                cmp.iter().filter(|s| s.moment.map(|m| m >= lc).unwrap_or(false)).copied().collect();
            let av = &av[av.len().saturating_sub(3)..];
            let ap = &ap[ap.len().saturating_sub(3)..];
            if !av.is_empty() && !ap.is_empty() {
                let bb = parties::moyenne_groupe(av);
                let aa = parties::moyenne_groupe(ap);
                let (b0, a0) = (bb.map(|x| x.0), aa.map(|x| x.0));
                let pct = match (b0, a0) {
                    (Some(b0), a0) if b0 > 0.0 => 100.0 * (a0.unwrap_or(0.0) - b0) / b0,
                    _ => 0.0,
                };
                let fmt = |x: Option<f64>| x.map(n0).unwrap_or_default();
                blocs_fps.push(json!({ "type": "titre", "texte": format!("{} : avant / après tes derniers réglages", b.nom_partie(s0)) }));
                let (val, ton) = if pct.abs() < 3.0 {
                    ("Pareil".to_string(), "")
                } else {
                    (
                        format!("{}{} %", if pct > 0.0 { "+" } else { "" }, n0(pct)),
                        if pct > 0.0 { "ok" } else { "attention" },
                    )
                };
                blocs_fps.push(json!({ "type": "chiffres", "elements": [
                    { "titre": format!("Avant le {}", lc.jj_mm()), "valeur": fmt(b0), "unite": "FPS" },
                    { "titre": "Après", "valeur": fmt(a0), "unite": "FPS" },
                    { "titre": "Différence", "valeur": val, "ton": ton }
                ] }));
            }
        }
        blocs_fps.push(
            json!({ "type": "bouton", "id": "fps-aide", "texte": "Mes FPS ne sont pas normaux : trouver pourquoi" }),
        );
        blocs_fps.push(json!({ "type": "titre", "texte": "Dernières parties", "droite": "Clique pour le détail" }));
        for s in sessions.iter().take(12) {
            let d = s.moment.unwrap_or_default();
            let pb = parties::probleme_fps(s);
            let ton = if s.avg < 30.0 {
                "urgent"
            } else if s.avg < 60.0 || pb {
                "attention"
            } else {
                ""
            };
            blocs_fps.push(json!({ "type": "ligne", "id": format!("fps:{}", s.id), "quand": [d.jj_mm(), d.hh_mm()],
                "texte": b.nom_partie(s),
                "note": format!("{} · 1 % bas {}{}", duree_jeu(s.secondes), n0(s.low1), if pb { ", à vérifier" } else { "" }),
                "barre": entier_ps(100f64.min(100.0 * s.avg / max_fps)), "valeur": format!("{} FPS", n0(s.avg)), "ton": ton }));
        }
    } else {
        blocs_fps.push(json!({ "type": "texte", "texte": "Aucune partie mesurée pour l'instant. Joue au moins 30 secondes : tes FPS moyens, tes chutes et la courbe de la partie apparaîtront ici." }));
    }

    // ---- Perf. In Game : Lag en ligne : la page « jeux-lag » du bloc trafic ({ module, blocs }), mise à sa place
    //      (jeux.onglets[perf].onglets[lag]) par l'hôte à l'assemblage.

    // ---- La page Jeux : le module montre le dernier jeu joué (« Reprendre »)
    let jeu_m = last.as_ref().map(|l| l.jeu.clone()).or_else(|| montres.first().cloned());
    b.jeu_module = jeu_m.clone();
    let module = match &jeu_m {
        Some(g) => {
            let texte_m = match log.get(&g.nom) {
                Some(pl) => {
                    let fps = match last.as_ref().and_then(|l| l.derniere.as_ref()) {
                        Some(d) => format!(", {} FPS en moyenne", n0(d.avg)),
                        None => String::new(),
                    };
                    format!("Dernière partie {}{fps}.", derniere_partie(&pl.last, maintenant))
                }
                None => "Pas encore joué depuis Nevermind.".into(),
            };
            let bouton = if en_jeu.as_deref() == Some(g.nom.as_str()) { "En cours de jeu" } else { "Jouer" };
            json!({ "image": b.adresse_jaquette(ctx, g, &tous), "legende": g.nom,
                "titre": if last.is_some() { "Reprendre".to_string() } else { g.nom.clone() },
                "texte": texte_m,
                "boutons": [{ "id": "jeu-module-lancer", "texte": bouton }, { "id": "jeu-module-fiche", "texte": "Voir le jeu", "style": "second" }],
                "resume": [{ "nom": "Lanceur", "valeur": g.source_affichee() },
                    { "nom": "Bibliothèque", "valeur": format!("{} jeu{}", tous.len(), pluriel(tous.len(), "x")) }] })
        }
        None => {
            json!({ "icone": "manette", "couleur": "S", "titre": "Tes jeux", "texte": "Tous tes jeux, quel que soit le lanceur (Steam, Epic...)." })
        }
    };

    // ---- Les modules de Perf. In Game
    let mod_fps = match sessions.first() {
        Some(s0) => {
            let d0 = s0.moment.unwrap_or_default();
            let couleur = if s0.avg < 30.0 {
                "rouge"
            } else if s0.avg < 60.0 {
                "orange"
            } else {
                "vert"
            };
            json!({ "icone": "jauge", "couleur": couleur, "titre": format!("{} FPS en moyenne", n0(s0.avg)),
                "texte": format!("Dernière partie mesurée : {}, le {} à {}, pendant {}.", b.nom_partie(s0), d0.jj_mm(), d0.hh_mm(), duree_jeu(s0.secondes)),
                "boutons": [{ "id": format!("fps:{}", s0.id), "texte": "Voir le détail" }],
                "resume": [{ "nom": "Jeu", "valeur": b.nom_partie(s0) }, { "nom": "FPS moyens", "valeur": n0(s0.avg) },
                    { "nom": "1 % bas", "valeur": n0(s0.low1) }, { "nom": "Parties mesurées", "valeur": sessions.len().to_string() }] })
        }
        None => {
            json!({ "icone": "jauge", "couleur": "doux", "titre": "Aucune partie mesurée", "texte": "Joue au moins 30 secondes : tes FPS moyens et tes chutes apparaîtront ici." })
        }
    };
    let mut actions = Vec::new();
    if b.organizer_visible(ctx) {
        actions.push(json!({ "id": "jeux-organizer", "texte": "Organizer Dofus", "style": "second" }));
    }
    actions.push(json!({ "id": "clic:BtnLibRefresh", "texte": "Actualiser", "icone": "actu" }));
    actions.push(json!({ "id": "clic:BtnLibAdd", "texte": "Ajouter un jeu", "icone": "plus" }));
    pages.insert(
        "jeux".into(),
        json!({ "titre": "Jeux", "sous": "Tous tes jeux, quel que soit le lanceur (Steam, Epic...).", "icone": "manette", "couleur": "S",
            "actions": actions, "module": module,
            "onglets": [
                { "id": "lib", "nom": "Bibliothèque", "module": module, "blocs": blocs_lib },
                { "id": "perf", "nom": "Perf. In Game", "texte": "Tes FPS et ta connexion pendant chaque partie : ce qui ralentit, et comment le corriger.",
                    "onglets": [{ "id": "fps", "nom": "FPS", "module": mod_fps, "blocs": blocs_fps },
                        { "id": "lag", "nom": "Lag en ligne" }] }
            ] }),
    );

    // ---- La fiche d'un jeu
    if let Some(g) = b.choisi.clone()
        && lourd
    {
        let pl = log.get(&g.nom).cloned();
        let bases: Vec<String> = g.exes.iter().map(|e| texte::nom_sans_extension(e).to_lowercase()).collect();
        let fsess: Vec<&PartieFps> = sessions
            .iter()
            .filter(|s| b.nom_partie(s).to_lowercase() == g.nom.to_lowercase() || bases.contains(&s.cle.to_lowercase()))
            .collect();
        let dern = fsess.first().copied();
        let en_cours = en_jeu.as_deref() == Some(g.nom.as_str());
        let comment = b.comment_lancer(&g);
        let sys = b.sys.clone();
        let opt = optim::points(sys.as_ref(), &b.connu, &g, mesure, BlocJeux::reglage_bool(ctx, "GameMode", false));
        b.optims = opt.clone();
        let afaire = optim::a_faire(&opt).len();
        let hero = b.jaquettes.banniere(sys.as_ref(), &ctx.donnees, &g);
        let mut lignes_opt = Vec::new();
        for (k, it) in opt.iter().enumerate() {
            let couleur = if it.ok {
                "vert"
            } else if it.facultatif {
                "doux"
            } else {
                "orange"
            };
            let mut l = json!({ "type": "ligne", "icone": if it.ok { "ok" } else { "alerte" }, "couleur": couleur, "texte": it.titre, "note": it.texte });
            if it.interrupteur {
                l["bascule"] = json!({ "id": format!("jeu-opt:{k}"), "oui": it.ok });
            } else if it.correction.as_deref() == Some("tweaks") {
                l["bouton"] = json!({ "id": format!("jeu-opt:{k}"), "texte": "Appliquer", "style": "second" });
            }
            lignes_opt.push(l);
        }
        let mut blocs = Vec::new();
        if let Some(h) = &hero {
            let sous = match &pl {
                Some(p) => format!("{} de jeu", duree_jeu(p.secondes)),
                None => String::new(),
            };
            blocs.push(json!({ "type": "banniere", "image": b.jaquettes.adresse(h), "titre": g.nom, "sous": sous }));
        }
        if let Some(d) = dern {
            blocs.push(json!({ "type": "ligne", "id": format!("fps:{}", d.id), "icone": "jauge", "couleur": "vert",
                "texte": format!("Dernière partie mesurée : {} FPS", n0(d.avg)), "note": format!("1 % bas {}. Voir le détail", n0(d.low1)), "valeur": "" }));
        }
        let droite = if afaire > 0 {
            format!("{afaire} point{} à améliorer", pluriel(afaire, "s"))
        } else {
            "Ce jeu est optimisé".into()
        };
        blocs.push(json!({ "type": "titre", "texte": "Optimisation du jeu", "droite": droite }));
        if afaire > 0 {
            blocs.push(json!({ "type": "bouton", "id": "jeu-optimiser", "texte": "Tout optimiser" }));
        }
        blocs.extend(lignes_opt);
        blocs.push(json!({ "type": "titre", "texte": "Aussi" }));
        blocs.push(json!({ "type": "bouton", "id": "jeu-dossier", "texte": "Ouvrir le dossier", "style": "second" }));
        blocs.push(json!({ "type": "bouton", "id": "optimisation", "texte": "Profils par jeu", "style": "second" }));
        blocs.push(if g.ajoute {
            json!({ "type": "bouton", "id": "jeu-retirer", "texte": "Retirer de la liste", "style": "second" })
        } else {
            json!({ "type": "bouton", "id": "jeu-desinstaller", "texte": "Désinstaller", "style": "danger" })
        });
        let texte_l = if matches!(comment, Lancement::Adresse(_)) && !g.source.is_empty() {
            format!("Lancé par {} (connexion, mises à jour et anti-triche comme d'habitude).", g.source)
        } else {
            "Lancé directement depuis son dossier.".into()
        };
        let boutons: Vec<Value> =
            if en_cours { Vec::new() } else { vec![json!({ "id": "jeu-lancer", "texte": "Lancer" })] };
        let image = b.adresse_jaquette(ctx, &g, &tous);
        pages.insert(
            "jeu".into(),
            json!({ "titre": g.nom, "parent": "jeux",
                "sous": format!("{} · dernière partie : {}", g.source_affichee(), derniere_partie(pl.as_ref().map(|p| p.last.as_str()).unwrap_or(""), maintenant)),
                "icone": "manette", "couleur": "S",
                "module": { "image": image, "legende": g.nom, "titre": if en_cours { "En cours de jeu" } else { "Prêt à jouer" },
                    "texte": texte_l, "boutons": boutons,
                    "resume": [{ "nom": "Temps de jeu", "valeur": pl.as_ref().map(|p| duree_jeu(p.secondes)).unwrap_or_else(|| "-".into()) },
                        { "nom": "Parties", "valeur": pl.as_ref().map(|p| p.parties.to_string()).unwrap_or_else(|| "0".into()) },
                        { "nom": "FPS moyens", "valeur": dern.map(|d| n0(d.avg)).unwrap_or_else(|| "-".into()) }] },
                "blocs": blocs }),
        );
    }

    // ---- Overlay (le compteur de FPS par-dessus le jeu)
    let coin_kit = |c: &str| match c {
        "hd" => "haut-droite",
        "bg" => "bas-gauche",
        "bd" => "bas-droite",
        _ => "haut-gauche",
    };
    let detail = match sessions.first() {
        Some(s0) => json!(format!("1 % bas {} · moy. {}", n0(s0.low1), n0(s0.avg))),
        None => Value::Null,
    };
    let apercu = sessions.first().map(|s0| entier_ps(s0.avg)).unwrap_or(144);
    let boutons_ov: Vec<Value> = if mesure {
        Vec::new()
    } else {
        vec![json!({ "id": "fps-mesure", "valeur": "oui", "texte": "Activer la mesure" })]
    };
    let options_coins: Vec<Value> =
        ["hg", "hd", "bg", "bd"].iter().map(|k| json!({ "id": coin_kit(k), "texte": coin(k) })).collect();
    pages.insert(
        "overlay".into(),
        json!({ "titre": "Overlay", "sous": "Ton compteur de FPS par-dessus le jeu : un coup d'œil suffit, sans quitter la partie.", "icone": "jauge", "couleur": "vert",
            "module": { "icone": "jauge", "couleur": if overlay && mesure { "vert" } else { "doux" },
                "titre": if !mesure { "Mesure coupée" } else if overlay { "Prêt" } else { "Compteur masqué" },
                "texte": if !mesure { "La mesure des FPS est coupée : le compteur ne peut pas s'afficher." } else if overlay { "Il apparaîtra dès que tu lances un jeu." } else { "Active le compteur pour le voir pendant tes parties." },
                "boutons": boutons_ov },
            "blocs": [
                { "type": "apercu", "id": "overlay-coin", "coin": coin_kit(&coin_id), "style": style, "fps": apercu, "detail": detail,
                    "legende": "Aperçu. Clique sur un coin de l'écran pour y placer le compteur." },
                { "type": "reglages", "rangs": [
                    { "texte": "Afficher le compteur pendant la partie", "note": "Par-dessus le jeu, en fenêtré ou plein écran fenêtré.", "bascule": { "id": "overlay-on", "oui": overlay } },
                    { "texte": "Raccourci pour le montrer ou le cacher", "note": "Jeu non reconnu ? Appuie dessus pendant la partie pour lancer ou arrêter la mesure et le compteur.", "touches": ["Ctrl", "Maj", "F"] }
                ] },
                { "type": "titre", "texte": "Style du compteur" },
                { "type": "cartes", "id": "overlay-style", "valeur": style, "options": [
                    { "id": "complet", "titre": "Complet", "texte": "Le chiffre, le 1 % bas (les pires moments de la partie) et la moyenne." },
                    { "id": "discret", "titre": "Discret", "texte": "Juste le chiffre, petit et semi-transparent." }
                ] },
                { "type": "titre", "texte": "Position du compteur" },
                { "type": "choix", "id": "overlay-coin", "valeur": coin_kit(&coin_id), "options": options_coins },
                { "type": "note", "texte": "Il reste collé au bord, même quand le chiffre change de taille." }
            ] }),
    );

    // ---- Le détail d'une partie mesurée, quand il est ouvert
    if let Some(id) = b.partie_ouverte.clone()
        && let Some(s) = fps_brut.iter().find(|s| s.id == id)
    {
        let p = crate::panneau_fps::panneau(b, ctx, s, &fps_brut);
        pages.insert("panneau".into(), p);
    }

    // ---- Les restes de jeux désinstallés, quand ils sont ouverts
    if b.restes_ouverts {
        pages.insert("panneau".into(), b.panneau_restes());
    }

    pages
}
