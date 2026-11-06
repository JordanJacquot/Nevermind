//! Les pages de l'écran autour de « Ton PC » (ecran-pc.ps1, `Get-PagesPc`) : Ton PC et le Tableau de bord, avec les
//! mesures en direct (`Get-MesuresKit`).

use crate::modele::{Analyse, Constat, libelle_statut};
use crate::net::{self, taille};
use crate::score;
use serde_json::{Map, Value, json};

/// `$TonsStatut`
pub fn ton_statut(s: &str) -> Option<&'static str> {
    match s {
        "ok" => Some("ok"),
        "warn" => Some("attention"),
        "bad" => Some("urgent"),
        "info" => Some(""),
        _ => None,
    }
}

/// `Get-TonKit` : vert « ok », orange « attention », rouge « urgent », sinon rien
pub fn ton_couleur(hex: &str) -> &'static str {
    let h = hex.trim().trim_start_matches('#');
    let h = match h.len() {
        6 => h,
        8 => &h[2..],
        _ => return "",
    };
    let p = |i: usize| i64::from_str_radix(&h[i..i + 2], 16).ok();
    let (Some(r), Some(g), Some(b)) = (p(0), p(2), p(4)) else { return "" };
    if g > 150 && r < 120 && b < 170 {
        "ok"
    } else if r > 190 && g < 110 {
        "urgent"
    } else if r > 190 && (110..210).contains(&g) && b < 120 {
        "attention"
    } else {
        ""
    }
}

/// `Format-TexteKit` : « Désactivé: » devient « Désactivé : » (une lettre de lecteur, « D: », reste collée)
pub fn texte_kit(t: &str) -> String {
    let c: Vec<char> = t.chars().collect();
    let lettre = |x: char| x.is_ascii_lowercase() || ('\u{e0}'..='\u{ff}').contains(&x);
    let mut s = String::with_capacity(t.len() + 8);
    for (i, &x) in c.iter().enumerate() {
        if x == ':' && i >= 2 && lettre(c[i - 1]) && lettre(c[i - 2]) && c.get(i + 1).is_some_and(|n| n.is_whitespace())
        {
            s.push_str(" :");
        } else {
            s.push(x);
        }
    }
    s
}

/// `Get-IconeConstat`
pub fn icone_constat(id: &str) -> &'static str {
    let l = id.to_lowercase();
    if l.starts_with("tweak:") {
        "manette"
    } else if l.starts_with("startup") {
        "marche"
    } else if l.starts_with("disk") {
        "disque"
    } else if l.starts_with("gpu") {
        "puce"
    } else if l.starts_with("ram") {
        "memoire"
    } else if l.starts_with("ups") || l.starts_with("laptop-battery") {
        "eclair"
    } else if l.starts_with("bsod") || l.starts_with("whea") {
        "alerte"
    } else if l.starts_with("hvci") || l.starts_with("secureboot") {
        "bouclier"
    } else if l.starts_with("wifi") {
        "wifi"
    } else if l.starts_with("display") {
        "ecran"
    } else {
        "etoile"
    }
}

const SEUIL_TEMP: f64 = 87.0;

/// `Get-MesuresKit` : processeur, mémoire, carte graphique, température (`$Live` : Updated, Cpu, RamUsed, RamTotal,
/// Gpu, GpuTemp)
pub fn mesures(live: &Value, a: Option<&Analyse>) -> Vec<Value> {
    let info = |k: &str| a.map(|a| a.info(k)).unwrap_or_default();
    let pas = !net::champ_vrai(live, "Updated");
    let lit = |k: &str| if pas { None } else { live.get(k).filter(|v| !v.is_null()).map(net::nombre_de) };
    let mut l = Vec::new();
    let cpu = lit("Cpu");
    l.push(json!({ "id": "cpu", "nom": "Processeur", "note": info("Processeur"),
        "valeur": cpu.map(net::entier).unwrap_or(0), "texte": cpu.map(|c| format!("{} %", net::entier(c))).unwrap_or_else(|| "...".into()),
        "ton": if cpu.is_some_and(|c| c >= 90.0) { "attention" } else { "" } }));
    let total = net::champ_nombre(live, "RamTotal");
    if !pas && total != 0.0 {
        let u = net::champ_nombre(live, "RamUsed");
        let p = 100.0 * u / total;
        l.push(json!({ "id": "ram", "nom": "Mémoire vive", "note": format!("{} sur {}", taille(u), taille(total)), "valeur": net::entier(p),
            "texte": format!("{} %", net::entier(p)), "ton": if p >= 90.0 { "attention" } else { "" } }));
    } else {
        l.push(json!({ "id": "ram", "nom": "Mémoire vive", "note": "", "valeur": 0, "texte": "..." }));
    }
    let gpu = lit("Gpu");
    l.push(json!({ "id": "gpu", "nom": "Carte graphique", "note": info("Carte graphique"), "valeur": gpu.map(net::entier).unwrap_or(0),
        "texte": gpu.map(|g| format!("{} %", net::entier(g))).unwrap_or_else(|| "...".into()) }));
    if let Some(t) = lit("GpuTemp") {
        l.push(json!({ "id": "temp", "nom": "Température", "note": "carte graphique, alerte à 87°", "valeur": net::entier((100.0 * t / SEUIL_TEMP).min(100.0)),
            "texte": format!("{}°", net::entier(t)), "ton": if t >= SEUIL_TEMP { "urgent" } else if t >= 80.0 { "attention" } else { "" } }));
    }
    l
}

/// `Get-LigneConstat`
pub fn ligne_constat(f: &Constat, id: &str) -> Map<String, Value> {
    let ton = ton_statut(&f.statut);
    let ok = f.statut == "ok";
    let mut m = Map::new();
    m.insert("type".into(), json!("ligne"));
    m.insert("id".into(), json!(id));
    m.insert("icone".into(), json!(icone_constat(&f.id)));
    m.insert(
        "couleur".into(),
        json!(if ton == Some("urgent") {
            "rouge"
        } else if ok {
            "vert"
        } else {
            "P"
        }),
    );
    m.insert("texte".into(), json!(texte_kit(&f.titre)));
    let note = if ok {
        texte_kit(&f.detail)
    } else if f.auto() {
        "Nevermind le fait pour toi".into()
    } else if f.correction.is_some() {
        "À faire toi-même, Nevermind explique comment".into()
    } else {
        texte_kit(&f.detail)
    };
    m.insert("note".into(), json!(note));
    let g = f.gain as i64;
    m.insert(
        "valeur".into(),
        json!(if f.gain > 0.0 {
            format!("+{g} point{}", if f.gain > 1.0 { "s" } else { "" })
        } else if ok {
            String::new()
        } else {
            "Voir".into()
        }),
    );
    m.insert(
        "ton".into(),
        json!(if ton == Some("urgent") {
            "urgent"
        } else if ok {
            "ok"
        } else {
            ""
        }),
    );
    m
}

/// Ce que les pages gardent pour les gestes (`$script:Kit` : Fiches, Constats, ActionsComposants)
#[derive(Debug, Clone, Default)]
pub struct Kit {
    pub fiches: Vec<Constat>,
    pub constats: Vec<Constat>,
    pub actions_composants: Vec<String>,
}

fn pluriel_points(n: usize) -> String {
    if n == 1 { "1 point".into() } else { format!("{n} points") }
}

/// `Get-PagesPc` : les pages « pc » et « tableau »
pub fn pages_pc(a: Option<&Analyse>, live: &Value, kit: &mut Kit) -> Map<String, Value> {
    let (imp, man) = score::a_regler(a);
    let a_regler: Vec<Constat> = imp.into_iter().chain(man).collect();
    let todo = a_regler.len();
    kit.fiches = a_regler.iter().take(8).cloned().collect();
    let auto: Vec<&Constat> = a
        .map(|a| a.actifs.iter().filter(|f| f.statut != "ok" && f.gain > 0.0 && f.auto()).collect())
        .unwrap_or_default();
    let gain_auto = a.map(|a| 0.max(a.potentiel - a.score)).unwrap_or(0);
    let boutons_tout: Vec<Value> = if auto.is_empty() {
        vec![]
    } else {
        vec![
            json!({ "id": "toutcorriger", "texte": if gain_auto != 0 { format!("Tout corriger (+{gain_auto} point{})", if gain_auto > 1 { "s" } else { "" }) } else { "Tout corriger".into() } }),
        ]
    };
    let analyser = json!({ "id": "analyser", "texte": "Relancer l'analyse", "icone": "actu" });
    let ton_module = if todo > 0 { "attention" } else { "" };
    let module = if let Some(a) = a {
        let texte = if todo > 0 {
            format!(
                "{} à régler pour gagner des points{}",
                pluriel_points(todo),
                if auto.len() == todo {
                    ", Nevermind s'en charge.".to_string()
                } else if !auto.is_empty() {
                    format!(", dont {} que Nevermind fait pour toi.", auto.len())
                } else {
                    ".".into()
                }
            )
        } else {
            "Ton PC est au top pour jouer. Rien à corriger.".into()
        };
        json!({ "anneau": { "valeur": a.score, "sur": 100, "ton": ton_module }, "titre": a.appreciation, "texte": texte, "boutons": boutons_tout })
    } else {
        json!({ "icone": "pc", "titre": "Analyse en cours", "texte": "Je regarde ton PC, ça prend quelques secondes." })
    };
    let lignes: Vec<Value> =
        kit.fiches.iter().enumerate().map(|(k, f)| Value::Object(ligne_constat(f, &format!("fiche:{k}")))).collect();
    let cartes = a.map(|a| a.cartes.clone()).unwrap_or_default();
    let mut blocs: Vec<Value> = vec![
        json!({ "type": "titre", "texte": "Ton PC maintenant", "direct": true }),
        json!({ "type": "mesures", "elements": mesures(live, a) }),
    ];
    if !lignes.is_empty() {
        blocs.push(json!({ "type": "titre", "texte": "Pour gagner des points", "droite": if todo > lignes.len() { format!("les {} premiers sur {todo}", lignes.len()) } else { format!("{todo} à régler") } }));
    }
    blocs.extend(lignes);
    blocs.push(json!({ "type": "titre", "texte": "Outils" }));
    blocs.push(json!({ "type": "raccourcis", "elements": [
        { "id": "tableau", "nom": "Tableau de bord", "icone": "jauge", "couleur": "P" },
        { "id": "tests", "nom": "Tests des composants", "icone": "test", "couleur": "S" } ] }));
    if !cartes.is_empty() {
        blocs.push(json!({ "type": "titre", "texte": "Santé des composants", "droite": a.map(|a| a.sante.clone()).unwrap_or_default() }));
        blocs.push(json!({ "type": "chiffres", "elements": cartes.iter().map(|c| json!({ "titre": c.titre, "valeur": libelle_statut(&c.statut), "ton": ton_statut(&c.statut) })).collect::<Vec<_>>() }));
    }

    // Le Tableau de bord : tout le détail de l'analyse
    let tous: Vec<Constat> = a.map(|a| a.actifs.clone()).unwrap_or_default();
    let dans = |f: &Constat| a_regler.iter().any(|x| x.id == f.id);
    let pour_info: Vec<Constat> = tous.iter().filter(|f| f.statut != "ok" && !dans(f)).cloned().collect();
    let deja_ok: Vec<Constat> = tous.iter().filter(|f| f.statut == "ok").cloned().collect();
    let ignores: Vec<Constat> = a.map(|a| a.ignores.clone()).unwrap_or_default();
    kit.constats = a_regler.iter().chain(&pour_info).chain(&deja_ok).chain(&ignores).cloned().collect();
    let mut k = 0usize;
    let mut suivant = || {
        let x = k;
        k += 1;
        x
    };
    let lr: Vec<Value> = a_regler
        .iter()
        .map(|f| {
            let mut l = ligne_constat(f, &format!("constat:{}", suivant()));
            l.insert("etiquette".into(), json!(libelle_statut(&f.statut)));
            l.insert("ton".into(), json!(ton_statut(&f.statut)));
            Value::Object(l)
        })
        .collect();
    let lp: Vec<Value> = pour_info
        .iter()
        .map(|f| {
            let mut l = ligne_constat(f, &format!("constat:{}", suivant()));
            l.insert("etiquette".into(), json!("Pour info"));
            l.insert("ton".into(), json!(""));
            l.insert("valeur".into(), json!("Voir"));
            Value::Object(l)
        })
        .collect();
    let lo: Vec<Value> =
        deja_ok.iter().map(|f| Value::Object(ligne_constat(f, &format!("constat:{}", suivant())))).collect();
    let li: Vec<Value> = ignores
        .iter()
        .map(|f| {
            let mut l = ligne_constat(f, &format!("constat:{}", suivant()));
            l.insert("etiquette".into(), json!("Ignoré"));
            l.insert("note".into(), json!("Ne compte plus dans ton score"));
            Value::Object(l)
        })
        .collect();
    let mut onglets = vec![json!({ "id": "regarder", "nom": format!("À régler ({})", a_regler.len()), "blocs":
        if !lr.is_empty() {
            let mut b = vec![json!({ "type": "texte", "texte": "Les points qui font gagner des points. Clique sur un point pour voir ce qu'il faut faire." })];
            b.extend(lr);
            b
        } else { vec![json!({ "type": "texte", "texte": "Rien à corriger : ton PC est bien réglé pour le jeu." })] } })];
    if !lp.is_empty() {
        let n = lp.len();
        let mut b = vec![
            json!({ "type": "texte", "texte": "Ces points ne comptent pas dans ton score : à toi de voir s'ils te concernent." }),
        ];
        b.extend(lp);
        onglets.push(json!({ "id": "info", "nom": format!("Pour info ({n})"), "blocs": b }));
    }
    onglets.push(json!({ "id": "ok", "nom": format!("Déjà OK ({})", lo.len()), "blocs": lo }));
    if !li.is_empty() {
        onglets.push(json!({ "id": "ignores", "nom": format!("Ignorés ({})", li.len()), "blocs": li }));
    }
    let mut comp = Vec::new();
    kit.actions_composants.clear();
    for c in &cartes {
        comp.push(json!({ "type": "titre", "texte": c.titre, "droite": libelle_statut(&c.statut) }));
        for b in &c.barres {
            comp.push(json!({ "type": "ligne", "texte": b.libelle, "barre": crate::net::entier(b.valeur.clamp(0.0, 100.0)), "valeur": b.texte, "ton": ton_couleur(&b.couleur) }));
        }
        let rangs: Vec<Value> = c.lignes.iter().map(|l| json!({ "texte": l.cle, "valeur": l.texte })).collect();
        if !rangs.is_empty() {
            comp.push(json!({ "type": "reglages", "rangs": rangs }));
        }
        for n in &c.notes {
            comp.push(json!({ "type": "note", "texte": n.texte }));
        }
        if let Some(x) = c.action.as_ref().filter(|x| !x.is_empty()) {
            kit.actions_composants.push(x.clone());
            comp.push(json!({ "type": "bouton", "id": format!("composant:{}", kit.actions_composants.len() - 1), "texte": c.libelle_action.clone().unwrap_or_default(), "style": "second" }));
        }
    }
    if !comp.is_empty() {
        onglets.push(json!({ "id": "composants", "nom": "Composants", "blocs": comp }));
    }
    let module_tableau = if let Some(a) = a {
        json!({ "anneau": { "valeur": a.score, "sur": 100, "ton": ton_module }, "titre": a.appreciation,
            "texte": if gain_auto != 0 && !auto.is_empty() { format!("Jusqu'à {} en un clic.", a.potentiel) } else if todo > 0 { format!("{} à régler.", pluriel_points(todo)) } else { "Rien à corriger.".into() },
            "boutons": boutons_tout })
    } else {
        module.clone()
    };
    let mut p = Map::new();
    p.insert("pc".into(), json!({ "titre": "Ton PC", "sous": "La santé de chaque composant de ton PC et ce qui freine tes jeux.", "icone": "pc", "couleur": "P", "actions": [analyser.clone()], "module": module, "blocs": blocs }));
    p.insert("tableau".into(), json!({ "titre": "Tableau de bord", "parent": "pc", "sous": "Tout le détail de l'analyse : ce qui freine tes jeux, et la santé de chaque composant.", "icone": "jauge", "couleur": "P", "actions": [analyser], "module": module_tableau, "onglets": onglets }));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tons_et_textes() {
        assert_eq!(ton_couleur("#22D37A"), "ok");
        assert_eq!(ton_couleur("#F5A524"), "attention");
        assert_eq!(ton_couleur("#F04438"), "urgent");
        assert_eq!(ton_couleur("#4EA8FF"), "");
        assert_eq!(texte_kit("Désactivé: la Game Bar"), "Désactivé : la Game Bar");
        assert_eq!(texte_kit("Le lecteur D: est plein"), "Le lecteur D: est plein");
        assert_eq!(texte_kit("ok:fin"), "ok:fin");
    }
}
