//! `Get-PagesProtection` : les pages Protection, Sécurité et Sauvegarde (Connexion : bloc réseau) envoyées au kit, au champ près.
//! (`null` là où PowerShell envoie `$null` ou une valeur vide, que vue.ps1 remet à `null`.)

use crate::etat::{Approuve, EtatS};
use crate::historique::{self, Entree, Sauvegarde};
use crate::regles::{self, Constat, MenaceRecente};
use serde_json::{Map, Value, json};

/// La dernière vérification (`$script:SecDonnees`)
#[derive(Debug, Clone, PartialEq)]
pub struct Verification {
    pub s: EtatS,
    pub constats: Vec<Constat>,
    pub note: i64,
    pub menaces: Vec<MenaceRecente>,
}

impl Verification {
    pub fn faire(s: EtatS, env: &crate::etat::Environnement, approuves: &[Approuve], hosts: &str) -> Verification {
        let constats = regles::constats(&s, env, approuves, hosts);
        let note = regles::note(&s, &constats);
        let menaces = regles::menaces_recentes(&s);
        Verification { s, constats, note, menaces }
    }
}

/// Ce que montrent les pages
pub struct EntreePages<'a> {
    pub verif: Option<&'a Verification>,
    /// `$ui.SecScanNote.Text` (vide avant la première vérification)
    pub note_analyses: &'a str,
    pub approuves: &'a [Approuve],
    pub sauvegarde: &'a Sauvegarde,
    pub historique: &'a [Entree],
}

fn ton_statut(s: &str) -> Value {
    match s {
        "ok" => json!("ok"),
        "warn" => json!("attention"),
        "bad" => json!("urgent"),
        "info" => json!(""),
        _ => Value::Null,
    }
}

fn pluriel(n: usize) -> &'static str {
    if n > 1 { "s" } else { "" }
}

/// Ce que la dalle Sécurité ajoute après « n points », en mots (second audit du 09/11 : avant, seulement la couleur
/// disait s'il fallait agir) : « à voir », « important(s) », ou « dont g important(s) » ; rien sans point à vérifier
fn jugement_securite(n: usize, graves: usize) -> Value {
    match (n, graves) {
        (0, _) => Value::Null,
        (_, 0) => json!("à voir"),
        (n, g) if g >= n => json!(format!("important{}", pluriel(n))),
        (_, g) => json!(format!("dont {g} important{}", pluriel(g))),
    }
}

/// Les pages, et les points tels que numérotés dans les boutons (`$script:Kit.SecChecks`)
pub fn pages(e: &EntreePages) -> (Map<String, Value>, Vec<Constat>) {
    let d = e.verif;
    let sec = d.map(|v| v.note);
    let etat = d.map(|v| regles::etat_resume(&v.s)).unwrap_or_default();
    let checks = d.map(|v| regles::points_page(&v.s, &v.constats)).unwrap_or_default();
    let n = checks.len();
    let defender_ok = d.is_some_and(|v| v.s.defender_ok());
    let titre_sec = match sec {
        None => "Vérification en cours",
        Some(x) if x >= 80 => "Bien protégé",
        Some(x) if x >= 50 => "Quelques points à voir",
        _ => "Vérifie ta protection",
    };
    let texte_sec = if n > 0 {
        format!("{n} point{} à vérifier.", pluriel(n))
    } else if d.is_some() {
        "Rien de suspect sur ton PC.".into()
    } else {
        "Antivirus, programmes cachés, réseau, sauvegarde.".into()
    };
    let analyses: Vec<Value> = if defender_ok {
        vec![
            json!({ "id": "clic:BtnScanQuick", "texte": "Analyse rapide" }),
            json!({ "id": "clic:BtnScanFull", "texte": "Analyse complète" }),
        ]
    } else {
        Vec::new()
    };
    let module = match sec {
        Some(x) => {
            let ton = if x < 50 {
                json!("urgent")
            } else if x < 80 {
                json!("attention")
            } else {
                Value::Null
            };
            json!({ "anneau": { "valeur": x, "sur": 100, "ton": ton }, "titre": titre_sec, "texte": texte_sec, "boutons": analyses })
        }
        None => json!({ "icone": "bouclier", "couleur": "T", "titre": titre_sec,
            "texte": "Nevermind vérifie ta protection, ça prend quelques secondes." }),
    };

    let nb = e.sauvegarde.nombre;
    let mut pages = Map::new();

    // ---- Protection
    let mut blocs = vec![json!({ "type": "outils", "elements": [
        { "id": "securite", "nom": "Sécurité", "resume": "Virus et menaces", "note": "Virus et programmes dangereux", "icone": "bouclier", "couleur": "T",
          "niveau": match sec { Some(x) => json!(x.clamp(0, 100)), None => Value::Null },
          "etiquette": if n > 0 { json!(format!("{n} point{}", pluriel(n))) } else if d.is_some() { json!("Tout est propre") } else { Value::Null },
          // (le jugement dit aussi en mots, pas seulement par la couleur : « 2 points à voir », « 1 point important »)
          "jugement": jugement_securite(n, checks.iter().filter(|c| c.grave()).count()),
          "ton": if checks.iter().any(Constat::grave) { "urgent" } else if n > 0 { "attention" } else { "ok" } },
        { "id": "sauvegarde", "nom": "Sauvegarde", "resume": "Retour en arrière", "note": "Revenir en arrière à tout moment", "icone": "sauve", "couleur": "S",
          "etiquette": if nb > 0 { format!("{nb} réglage{}", pluriel(nb)) } else { "Rien de modifié".into() }, "ton": "ok" }
    ] })];
    if n > 0 {
        blocs.push(json!({ "type": "titre", "texte": "Points à vérifier" }));
    }
    for c in checks.iter().take(4) {
        blocs.push(json!({ "type": "ligne", "id": "securite", "icone": "alerte", "couleur": if c.grave() { "rouge" } else { "orange" },
            "texte": c.titre, "note": c.detail, "etiquette": if c.grave() { "Important" } else { "À voir" }, "ton": ton_statut(c.statut) }));
    }
    if !etat.is_empty() {
        blocs.push(json!({ "type": "titre", "texte": "Ta protection" }));
    }
    for l in &etat {
        let couleur = match l.ton {
            "ok" => "vert",
            "urgent" => "rouge",
            "attention" => "orange",
            _ => "P",
        };
        blocs.push(json!({ "type": "ligne", "id": "securite", "icone": if l.ton == "ok" { "ok" } else { "alerte" }, "couleur": couleur,
            "texte": l.nom, "valeur": l.valeur, "ton": l.ton }));
    }
    pages.insert(
        "protection".into(),
        json!({ "titre": "Protection", "sous": "Antivirus, programmes cachés, sauvegarde.", "icone": "bouclier", "couleur": "T",
            "actions": [{ "id": "clic:BtnSecRefresh", "texte": "Revérifier", "icone": "actu" }], "module": module, "blocs": blocs }),
    );

    // ---- Sécurité
    let mut lignes = Vec::new();
    for (k, c) in checks.iter().enumerate() {
        lignes.push(json!({ "type": "ligne", "icone": "alerte", "couleur": if c.grave() { "rouge" } else { "orange" }, "texte": c.titre,
            "note": c.detail, "etiquette": if c.grave() { "Important" } else { "À voir" }, "ton": ton_statut(c.statut) }));
        if c.a_elements() {
            let tous = c.elements.as_deref().unwrap_or_default();
            let mut it: Vec<String> = tous.iter().take(6).cloned().collect();
            if tous.len() > 6 {
                let reste = tous.len() - 6;
                it.push(format!("et {reste} autre{}...", pluriel(reste)));
            }
            lignes.push(json!({ "type": "liste", "elements": it }));
        }
        for (j, a) in c.actions.iter().enumerate() {
            lignes.push(json!({ "type": "bouton", "id": format!("sec:{k}:{j}"), "texte": a.texte,
                "style": if j > 0 { json!("second") } else { Value::Null } }));
        }
        if !c.confiance.is_empty() {
            lignes.push(json!({ "type": "bouton", "id": format!("sec-moi:{k}"),
                "texte": if c.confiance.len() > 1 { "C'est moi, tout est OK" } else { "C'est moi, c'est OK" }, "style": "second" }));
        }
    }
    let blocs_points: Vec<Value> = if lignes.is_empty() {
        vec![
            json!({ "type": "pastilles", "elements": [{ "texte": "Rien de suspect", "ton": "ok" }] }),
            json!({ "type": "texte", "texte": "Pas de fichier déguisé, pas de programme caché, pas de tâche douteuse, pas de redirection." }),
        ]
    } else {
        let mut l = lignes;
        l.push(json!({ "type": "note", "texte": "Les points à vérifier sont des indices, pas des preuves : un programme non signé n'est pas forcément un virus. En cas de doute, lance une analyse." }));
        l
    };
    let mut blocs_etat = vec![
        json!({ "type": "reglages", "rangs": etat.iter().map(|l| json!({ "texte": l.nom, "valeur": l.valeur })).collect::<Vec<_>>() }),
    ];
    if defender_ok {
        blocs_etat.push(
            json!({ "type": "bouton", "id": "clic:BtnScanFolder", "texte": "Analyser un dossier", "style": "second" }),
        );
        blocs_etat.push(
            json!({ "type": "bouton", "id": "clic:BtnScanUpdate", "texte": "Mettre à jour la base de virus", "style": "second" }),
        );
    }
    blocs_etat.push(json!({ "type": "note", "texte": e.note_analyses }));
    let menaces = d.map(|v| v.menaces.as_slice()).unwrap_or_default();
    let blocs_menaces: Vec<Value> = if menaces.is_empty() {
        vec![json!({ "type": "texte", "texte": "Aucune menace trouvée sur ce PC récemment." })]
    } else {
        menaces
            .iter()
            .map(|h| {
                json!({ "type": "ligne", "icone": "alerte", "couleur": if h.active { "rouge" } else { "doux" }, "texte": h.nom,
                    "note": format!("{}   {}", h.quand.map(|q| q.complete()).unwrap_or_default(), h.fichier),
                    "etiquette": h.etat, "ton": if h.active { "urgent" } else { "ok" } })
            })
            .collect()
    };
    let mut onglets = vec![
        json!({ "id": "points", "nom": format!("Points à vérifier ({n})"), "blocs": blocs_points }),
        json!({ "id": "etat", "nom": "État", "blocs": blocs_etat }),
        json!({ "id": "menaces", "nom": format!("Menaces récentes ({})", menaces.len()), "blocs": blocs_menaces }),
    ];
    if !e.approuves.is_empty() {
        let mut b = vec![
            json!({ "type": "texte", "texte": "Tu as indiqué que c'est toi qui les as mis en place : ils ne baissent plus le niveau de protection. Retire-les si tu changes d'avis." }),
        ];
        for (k, t) in e.approuves.iter().enumerate() {
            b.push(json!({ "type": "ligne", "icone": "ok", "couleur": "vert", "texte": t.texte, "note": t.sorte,
                "bouton": { "id": format!("sec-retirer:{k}"), "texte": "Retirer", "style": "second" } }));
        }
        onglets.push(
            json!({ "id": "approuves", "nom": format!("Approuvés par toi ({})", e.approuves.len()), "blocs": b }),
        );
    }
    let module_sec = if sec.is_some() {
        json!({ "anneau": module["anneau"], "titre": titre_sec, "texte": texte_sec, "boutons": module["boutons"] })
    } else {
        module.clone()
    };
    pages.insert(
        "securite".into(),
        json!({ "titre": "Sécurité", "parent": "protection", "sous": "Recherche des virus et de tout ce qui pourrait être dangereux sur ton PC.",
            "icone": "bouclier", "couleur": "T", "actions": [{ "id": "clic:BtnSecRefresh", "texte": "Revérifier", "icone": "actu" }],
            "module": module_sec, "onglets": onglets }),
    );

    // (la page Connexion est donnée par le bloc réseau)
    pages.insert("sauvegarde".into(), page_sauvegarde(e));
    (pages, checks)
}

/// `^(\d{2}/\d{2})/\d{4} à (\d{2}:\d{2})$`
fn jour_heure(dt: &str) -> (String, String) {
    let c: Vec<char> = dt.chars().collect();
    let chiffre = |i: usize| c.get(i).is_some_and(|x| x.is_ascii_digit());
    let ok = c.len() == 18
        && [0, 1, 3, 4, 6, 7, 8, 9, 13, 14, 16, 17].iter().all(|i| chiffre(*i))
        && c[2] == '/'
        && c[5] == '/'
        && c[10] == ' '
        && c[11] == 'à'
        && c[12] == ' '
        && c[15] == ':';
    if ok { (c[0..5].iter().collect(), c[13..18].iter().collect()) } else { (dt.to_string(), String::new()) }
}

fn page_sauvegarde(e: &EntreePages) -> Value {
    let nb = e.sauvegarde.nombre;
    let mut lignes = Vec::new();
    for h in e.historique.iter().take(20) {
        let (jour, heure) = jour_heure(&h.date);
        let details = h.details.join(", ");
        let mut l =
            json!({ "type": "ligne", "quand": [jour, heure], "texte": h.titre, "note": details, "fait": h.annule });
        if h.annule {
            l["etiquette"] = json!("Annulé");
        } else if h.note() {
            l["etiquette"] = json!("Noté");
        } else if h.definitif() {
            l["etiquette"] = json!("Définitif");
            let morceaux: Vec<String> = [details.clone(), format!("Pour revenir en arrière : {}", h.retour())]
                .into_iter()
                .filter(|x| !x.is_empty())
                .collect();
            l["note"] = json!(morceaux.join(". "));
        } else {
            l["bouton"] = json!({ "id": format!("hist:{}", h.id), "texte": "Annuler", "style": "second" });
        }
        lignes.push(l);
        let tech = historique::details_techniques(h, &e.sauvegarde.plan_cree);
        if !tech.is_empty() {
            lignes.push(json!({ "type": "faq", "elements": [{ "question": "Détails techniques", "reponse": tech.join(" ; "), "icone": "liste" }] }));
        }
    }
    let mut blocs = vec![
        json!({ "type": "titre", "texte": "Historique des changements", "droite": "Du plus récent au plus ancien" }),
    ];
    if lignes.is_empty() {
        blocs.push(json!({ "type": "texte", "texte": "Aucun changement pour le moment." }));
    } else {
        blocs.extend(lignes);
    }
    blocs.push(json!({ "type": "titre", "texte": "Aussi" }));
    blocs.push(json!({ "type": "ligne", "icone": "bouclier", "couleur": "T", "texte": "Point de restauration Windows",
        "note": "Une photo complète des réglages de Windows. En cas de souci, la restauration du système te ramène à cet état.",
        "bouton": { "id": "clic:BtnRestorePoint", "texte": "Créer", "style": "second" } }));
    blocs.push(json!({ "type": "ligne", "icone": "sauve", "couleur": "S", "texte": "Restauration du système",
        "note": "Revenir à un point de restauration (outil de Windows).", "bouton": { "id": "clic:BtnOpenRestore", "texte": "Ouvrir", "style": "second" } }));
    blocs.push(json!({ "type": "ligne", "icone": "rapport", "couleur": "P", "texte": "Rapport de ton PC",
        "note": "Une page web avec ta configuration, ton score et les points à améliorer.", "bouton": { "id": "clic:BtnExport", "texte": "Exporter", "style": "second" } }));
    let titre = if nb > 0 {
        format!("{nb} réglage{} modifié{}", pluriel(nb), pluriel(nb))
    } else {
        "Rien de modifié".into()
    };
    let texte = if nb > 0 {
        "Tout ce que Nevermind a changé est sauvegardé avant d'être fait. « Tout restaurer » remet tout comme avant, sauf les protections du PC marquées « Définitif » dans l'historique, qui restent en place."
    } else {
        "Nevermind n'a encore rien modifié sur ce PC."
    };
    let boutons: Vec<Value> =
        if nb > 0 { vec![json!({ "id": "clic:BtnUndo", "texte": "Tout restaurer" })] } else { Vec::new() };
    json!({ "titre": "Sauvegarde et rapport", "parent": "protection", "sous": "Reviens en arrière à tout moment, ou partage l'état de ton PC.",
        "icone": "sauve", "couleur": "S", "module": { "icone": "sauve", "couleur": "S", "titre": titre, "texte": texte, "boutons": boutons },
        "blocs": blocs })
}
