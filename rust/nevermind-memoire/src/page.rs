//! La page RAM du kit (`Get-PageMemoireKit`, ecran-perf.ps1), au champ près.

use crate::constat::{Constat, part_cachee};
use crate::texte::{arrondi_pair, taille};
use serde_json::{Map, Value, json};

const MO: f64 = 1024.0 * 1024.0;

/// La page `memoire`. `en_cours` : « donnees » (lecture du PC), « pilotes » (recherche du pilote), vide sinon.
/// `fermeture` : une appli est en train de se fermer (plus aucun bouton « Fermer »).
pub fn page(c: Option<&Constat>, en_cours: &str, fermeture: bool) -> Value {
    let mut page = Map::new();
    page.insert("titre".into(), json!("RAM"));
    page.insert("parent".into(), json!("perf"));
    page.insert("sous".into(), json!("Ce qui prend la mémoire de ton PC, et ce qui peut être coupé sans risque."));
    page.insert("icone".into(), json!("memoire"));
    page.insert("couleur".into(), json!("P"));
    page.insert(
        "actions".into(),
        if en_cours.is_empty() {
            json!([{ "id": "memoire:analyser", "texte": "Actualiser", "icone": "actu" }])
        } else {
            json!([])
        },
    );
    let Some(c) = c else {
        page.insert(
            "module".into(),
            json!({ "icone": "memoire", "couleur": "P", "titre": "Je regarde ta mémoire",
                "texte": "Quelques secondes : je fais le tour de ce qui tourne sur ton PC.",
                "boutons": if en_cours.is_empty() { json!([{ "id": "memoire:analyser", "texte": "Regarder" }]) } else { json!([]) } }),
        );
        page.insert(
            "blocs".into(),
            json!([{ "type": "texte", "texte": "Nevermind regroupe la mémoire par appli et te dit ce qui est utile, ce qui ne sert pas au quotidien, et ce qui est à vérifier." }]),
        );
        return Value::Object(page);
    };
    let n = c.corrections.len();
    let mut boutons = Vec::new();
    if n > 0 {
        boutons.push(json!({ "id": "memoire:couper", "texte": format!("Couper ce qui est sûr ({n})") }));
    }
    boutons.push(json!({ "id": "memoire:rapport", "texte": "Créer un rapport", "style": if n > 0 { json!("second") } else { Value::Null } }));
    page.insert(
        "module".into(),
        json!({
            "anneau": { "valeur": c.pourcent, "sur": 100, "petit": "%", "ton": if c.pourcent >= 80 { "attention" } else { "" } },
            "titre": format!("{} utilisés sur {}", taille(c.utilisee), taille(c.visible)),
            "texte": if n == 1 {
                "1 chose ne sert pas au quotidien et peut être coupée sans risque.".to_string()
            } else if n > 0 {
                format!("{n} choses ne servent pas au quotidien et peuvent être coupées sans risque.")
            } else {
                "Rien à couper sans risque : ce qui reste se règle à la main (page Démarrage), ou est utile.".to_string()
            },
            "boutons": boutons,
            "resume": [
                { "nom": "Installée", "valeur": taille(c.installee) },
                { "nom": "En cache (ne compte pas)", "valeur": taille(c.cache) },
                { "nom": "Libre tout de suite", "valeur": taille(c.disponible) },
            ],
        }),
    );
    let alertes: Vec<Value> = c
        .alertes
        .iter()
        .map(|a| {
            let suite =
                if a.id == "pilotes" && en_cours == "pilotes" { " Je cherche encore quel pilote en est la cause..." } else { "" };
            let urgent = a.ton == "urgent";
            json!({ "type": "ligne", "icone": "alerte", "couleur": if urgent { "rouge" } else { "orange" }, "texte": a.titre,
                "note": format!("{}{suite} Que faire : {}", a.texte, a.quoi),
                "etiquette": if urgent { "Important" } else { "À surveiller" }, "ton": a.ton })
        })
        .collect();
    let max = c.applis.first().map_or(0.0, |a| a.octets).max(1.0);
    let n_grosses = c.applis.iter().filter(|a| a.octets >= 20.0 * MO).take(30).count();
    let grosses: Vec<usize> =
        c.applis.iter().enumerate().filter(|(_, a)| a.octets >= 20.0 * MO).take(n_grosses).map(|(i, _)| i).collect();
    let mut lignes: Vec<Value> = Vec::new();
    let mut petites = 0usize;
    let mut petites_octets = 0.0;
    for (i, g) in c.applis.iter().enumerate() {
        if !grosses.contains(&i) {
            petites += 1;
            petites_octets += g.octets;
        }
    }
    for &i in &grosses {
        let g = &c.applis[i];
        let (icone, couleur, ton) = match g.kind.as_str() {
            "windows" => ("pc", "doux", "ok"),
            "inutile" => ("balai", "orange", "attention"),
            _ => ("app", "P", ""),
        };
        let mut l = json!({ "type": "ligne", "icone": icone, "couleur": couleur,
            "texte": if g.nb > 1 { format!("{} ({} processus)", g.nom, g.nb) } else { g.nom.clone() },
            "note": g.note, "barre": arrondi_pair(100.0 * g.octets / max), "valeur": taille(g.octets),
            "etiquette": g.etiquette, "ton": ton });
        if g.fermable && !fermeture {
            l["bouton"] = json!({ "id": format!("memoire-fermer:{i}"), "texte": "Fermer", "style": "second" });
        }
        lignes.push(l);
    }
    if petites > 0 {
        lignes.push(json!({ "type": "ligne", "icone": "liste", "couleur": "doux", "texte": format!("{petites} petits programmes"),
            "note": "Chacun prend moins de 20 Mo.", "valeur": taille(petites_octets) }));
    }
    let cachee: Vec<Value> = part_cachee(c)
        .into_iter()
        .map(|x| {
            let mut l = json!({ "type": "ligne", "icone": "memoire", "couleur": "doux", "texte": x.nom, "note": x.note,
                "valeur": taille(x.octets) });
            if x.nom.starts_with("En cache") {
                l["ton"] = json!("ok");
                l["etiquette"] = json!("Ne compte pas");
            }
            l
        })
        .collect();
    let mut blocs = Vec::new();
    if !alertes.is_empty() {
        blocs.push(json!({ "type": "titre", "texte": "À regarder" }));
    }
    blocs.extend(alertes);
    blocs.push(json!({ "type": "titre", "texte": "Par appli", "droite": "Du plus gros au plus petit" }));
    blocs.extend(lignes);
    blocs.push(json!({ "type": "titre", "texte": "La part cachée", "droite": "Ce qui n'est à aucune appli" }));
    blocs.extend(cachee);
    blocs.push(json!({ "type": "note", "texte": "Nevermind ne ferme une appli que si tu cliques sur « Fermer », d'abord comme avec la croix pour te laisser enregistrer. Jamais ton jeu, Windows, un service, ton antivirus, les applis de Mindset ni ce qui appartient à un autre compte. « Couper ce qui est sûr » ne touche qu'aux mises à jour et aux logiciels du fabricant, rien de Windows, et se remet comme avant depuis la page Sauvegarde." }));
    page.insert("blocs".into(), Value::Array(blocs));
    Value::Object(page)
}
