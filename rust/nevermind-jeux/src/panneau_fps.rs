//! Le détail d'une partie mesurée (Show-FpsSession de jeu.ps1, et « Mes FPS ne sont pas normaux », Invoke-FpsHelp),
//! en page « panneau » de l'écran. Le diagnostic (« D'où ça vient ») est fait par le bloc perf : demandé par le signal
//! « diagnostic-fps » {Id}, reçu par « diagnostic-fps-pret » {Id, diagnostic}.

use crate::BlocJeux;
use crate::json::{nombre, texte, vrai};
use crate::parties::{self, PartieFps};
use crate::texte::{duree_jeu, entier_ps, n0};
use nevermind_noyau::Contexte;
use serde_json::{Value, json};

/// Le verdict d'une partie (Get-FpsVerdict) : (statut, phrase).
pub fn verdict(s: &PartieFps) -> (&'static str, &'static str) {
    let ratio = if s.avg > 0.0 { s.low1 / s.avg } else { 0.0 };
    if s.avg < 30.0 {
        return ("bad", "Moins de 30 FPS en moyenne : le jeu saccade. Baisse la qualité graphique ou la résolution.");
    }
    if ratio < 0.5 {
        return (
            "warn",
            "Moyenne correcte, mais des chutes fréquentes : ce sont elles que tu ressens comme des saccades. Souvent des programmes en arrière-plan, un disque plein ou une surchauffe.",
        );
    }
    if s.avg < 60.0 {
        return ("warn", "Jouable, mais en dessous de 60 FPS la fluidité se ressent dans les jeux rapides.");
    }
    if ratio < 0.7 {
        return ("ok", "Bonne moyenne, avec quelques petites chutes par moments.");
    }
    ("ok", "Fluide et régulier : très bonne partie.")
}

fn ton_fps(f: f64) -> &'static str {
    if f >= 60.0 {
        "ok"
    } else if f >= 30.0 {
        "attention"
    } else {
        "urgent"
    }
}

fn couleur(statut: &str) -> &'static str {
    match statut {
        "ok" => "vert",
        "warn" => "orange",
        "bad" => "rouge",
        _ => "P",
    }
}

pub fn panneau(b: &BlocJeux, ctx: &Contexte, s: &PartieFps, toutes: &[PartieFps]) -> Value {
    let d = s.moment.unwrap_or_default();
    let nom = b.nom_partie(s);
    let sous = format!("Partie du {} à {}, {} mesurées", d.jj_mm(), d.hh_mm(), duree_jeu(s.secondes));
    let (mut statut, phrase) = verdict(s);
    let mut etat = match statut {
        "ok" => "Bonne partie",
        "warn" => "À surveiller",
        _ => "Saccades",
    }
    .to_string();
    let x = &s.brut;
    let mut blocs = vec![json!({ "type": "titre", "texte": "RÉSULTAT" })];
    let low01 = nombre(x, "Low01");
    blocs.push(json!({ "type": "chiffres", "elements": [
        { "titre": "FPS moyen", "valeur": n0(s.avg), "unite": "FPS", "ton": ton_fps(s.avg) },
        { "titre": "1 % bas", "valeur": n0(s.low1), "unite": "FPS", "ton": ton_fps(s.low1) },
        { "titre": "0,1 % bas", "valeur": n0(low01), "unite": "FPS", "ton": ton_fps(low01) }
    ] }));
    blocs.push(json!({ "type": "ligne", "icone": if statut == "ok" { "ok" } else { "alerte" }, "couleur": couleur(statut), "texte": phrase }));
    let menu_sec = entier_ps(nombre(x, "MenuSec"));
    let menu_fps = entier_ps(nombre(x, "MenuFps"));
    if menu_sec >= 5 {
        blocs.push(json!({ "type": "texte", "texte": format!("Menus, chargements ou cinématiques bloqués à {menu_fps} FPS pendant {} : mis à part, ils ne comptent ni dans ces chiffres ni comme des chutes (les creux de la courbe).", duree_jeu(menu_sec as f64)) }));
    }
    // D'où ça vient : le diagnostic du bloc perf
    blocs.push(json!({ "type": "titre", "texte": "D'OÙ ÇA VIENT" }));
    match b.diagnostics.get(&s.id) {
        Some(dg) => {
            let st = texte(dg, "statut");
            if vrai(dg.get("probleme").unwrap_or(&Value::Null)) {
                statut = if st == "bad" { "bad" } else { "warn" };
                etat = if st == "bad" { "Problème trouvé".into() } else { "À améliorer".into() };
            }
            blocs.push(
                json!({ "type": "ligne", "icone": if st == "ok" { "ok" } else { "alerte" }, "couleur": couleur(&st),
                "texte": texte(dg, "titre"), "note": texte(dg, "texte") }),
            );
            // (chaque cause comme New-SecurityCard, securite.ps1:289-345 : toutes ses lignes de conseil, puis un bouton
            //  par action, le premier en avant ; le clic part en « fps-diag:<cause>:<bouton> », audit M10)
            for (k, e) in crate::biblio::liste(dg.get("elements")).iter().enumerate() {
                let es = texte(e, "statut");
                blocs.push(
                    json!({ "type": "ligne", "icone": if es == "ok" { "ok" } else { "alerte" }, "couleur": couleur(&es),
                    "texte": texte(e, "titre"), "note": texte(e, "detail") }),
                );
                let lignes: Vec<String> = crate::biblio::liste(e.get("lignes"))
                    .iter()
                    .map(crate::json::texte_de)
                    .filter(|l| !l.is_empty())
                    .collect();
                if !lignes.is_empty() {
                    blocs.push(json!({ "type": "liste", "elements": lignes }));
                }
                for (j, a) in crate::biblio::liste(e.get("actions")).iter().enumerate() {
                    blocs.push(json!({ "type": "bouton", "id": format!("fps-diag:{k}:{j}"), "texte": texte(a, "texte"),
                        "style": if j > 0 { json!("second") } else { Value::Null } }));
                }
            }
        }
        None => blocs.push(json!({ "type": "texte", "texte": "Analyse de la partie..." })),
    }
    // La courbe, étirée sur 160 points
    let serie: Vec<f64> =
        crate::biblio::liste(x.get("Series")).iter().filter(|v| !v.is_null()).map(crate::json::nombre_de).collect();
    if serie.len() >= 2 {
        blocs.push(json!({ "type": "titre", "texte": "FPS PENDANT LA PARTIE" }));
        let pts: Vec<f64> = (0..160).map(|k| serie[k * serie.len() / 160].round_ties_even()).collect();
        blocs.push(json!({ "type": "courbe", "points": pts, "unite": "FPS", "titre": format!("Moyenne {} FPS, à la fin", n0(s.avg)),
            "etiquettes": ["Début de la partie", "Fin de la partie"] }));
    }
    // Avant / après les derniers réglages (même jeu)
    let mut meme: Vec<&PartieFps> = toutes.iter().filter(|p| p.cle.to_lowercase() == s.cle.to_lowercase()).collect();
    crate::texte::trier_ps(&mut meme, |a, c| a.moment.cmp(&c.moment));
    if let Some(lc) = parties::dernier_changement(Some(ctx), &ctx.donnees) {
        let av: Vec<&PartieFps> = meme.iter().filter(|p| p.moment.map(|m| m < lc).unwrap_or(false)).copied().collect();
        let ap: Vec<&PartieFps> = meme.iter().filter(|p| p.moment.map(|m| m >= lc).unwrap_or(false)).copied().collect();
        let av = &av[av.len().saturating_sub(3)..];
        let ap = &ap[ap.len().saturating_sub(3)..];
        if let (Some(bb), Some(aa)) = (parties::moyenne_groupe(av), parties::moyenne_groupe(ap)) {
            blocs.push(json!({ "type": "titre", "texte": "AVANT / APRÈS TES DERNIERS RÉGLAGES" }));
            blocs.push(json!({ "type": "chiffres", "elements": [
                { "titre": format!("Avant le {}", lc.jj_mm()), "valeur": n0(bb.0), "unite": "FPS" },
                { "titre": "Après", "valeur": n0(aa.0), "unite": "FPS" }
            ] }));
        }
    }
    // Jeu mesuré avec Ctrl + Maj + F et inconnu : proposer de l'ajouter
    let chemin = x.get("Diag").map(|d| texte(d, "Path")).unwrap_or_default();
    if !chemin.is_empty()
        && std::path::Path::new(&chemin).exists()
        && !b.jeux.iter().flatten().any(|g| g.exes.iter().any(|e| e.eq_ignore_ascii_case(&chemin)))
    {
        blocs.push(json!({ "type": "ligne", "icone": "plus", "couleur": "P",
            "texte": "Nevermind ne connaît pas encore ce jeu : ajoute-le pour qu'il soit reconnu tout seul la prochaine fois (mode jeu, FPS, lag).",
            "bouton": { "id": "fps-ajouter", "texte": "Ajouter à mes jeux" } }));
    }
    let duree = format!(
        "{} de jeu{}",
        duree_jeu(s.secondes),
        if menu_sec >= 5 {
            format!(" + {} de menus bloqués à {menu_fps} FPS", duree_jeu(menu_sec as f64))
        } else {
            String::new()
        }
    );
    let exclusif = vrai(x.get("Exclusive").unwrap_or(&Value::Null));
    blocs.push(json!({ "type": "details", "titre": "Détails techniques", "lignes": [
        format!("Durée mesurée : {duree}"),
        format!("Images affichées : {}", n0(nombre(x, "Frames"))),
        format!("Mode d'affichage : {}", if exclusif { "Plein écran (le compteur ne peut pas s'afficher par-dessus)" } else { "Fenêtré ou plein écran fenêtré" }),
        "Mesure : PresentMon (Intel). Les moments où le jeu n'était pas au premier plan ne comptent pas.".to_string()
    ] }));
    json!({ "titre": nom, "sous": sous, "icone": "jauge", "couleur": "P", "retour": "appli", "parent": "jeux",
        "module": { "icone": if statut == "ok" { "ok" } else { "alerte" }, "couleur": couleur(statut), "titre": etat, "texte": sous,
            "boutons": [{ "id": "panneau-fps-fermer", "texte": "Fermer", "style": "second" }] },
        "blocs": blocs })
}
