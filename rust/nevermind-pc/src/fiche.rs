//! La fiche (tableau-de-bord.ps1 et ecran-pc.ps1) : un point à régler (`Open-Sheet`), « Tout corriger », le résultat
//! avec « Revenir en arrière » (`Show-ResultSheet`), la vérification de l'écran après un changement de fréquence
//! (`Confirm-DisplayChange`), et une question. En PowerShell, la fiche est faite d'éléments WPF que `Get-FicheKit`
//! relit pour l'écran ; ici les mêmes éléments sont gardés tels quels ([`El`]) et relus de la même façon.

use crate::analyse::{ACTIONS_BATTERIE, REGLAGES_BATTERIE};
use crate::corrections::Moteur;
use crate::modele::{Constat, Declaration, couleurs};
use crate::net;
use crate::pages::ton_couleur;
use serde_json::{Map, Value, json};

/// Un élément de la fiche (ce que `Get-FicheKit` lit dans `$ui.SheetBody`)
#[derive(Debug, Clone, PartialEq)]
pub enum El {
    /// L'en-tête : la pastille (sa couleur) et le titre
    Tete { couleur: String, titre: String },
    /// Les étiquettes (« +4 points au score »...) : texte et couleur
    Etiquettes(Vec<(String, String)>),
    /// Un texte : gras (titre), demi-gras (titre de section) ou normal
    Texte { texte: String, poids: Poids },
    /// Un encadré (`Add-SheetInfo`)
    Info(String),
    /// « Détails techniques » (`Add-SheetDetails`)
    Details { titre: String, lignes: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Poids {
    Normal,
    Demi,
    Gras,
}

/// Les quatre boutons de la fiche : leur texte quand ils sont montrés
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Boutons {
    pub run: Option<String>,
    pub open: Option<String>,
    pub ignore: Option<String>,
    pub close: Option<String>,
}

/// La fiche ouverte
#[derive(Debug, Clone, PartialEq)]
pub struct Fiche {
    /// fix, result, display, question
    pub mode: String,
    pub els: Vec<El>,
    pub boutons: Boutons,
    /// Le point (ou les points) de la fiche
    pub items: Vec<Constat>,
    /// Ce que fait « Ouvrir » (`$ui.SheetOpen.Tag`)
    pub ouvrir: Option<String>,
}

fn section(els: &mut Vec<El>, titre: &str, lignes: &[String], prefixe: Prefixe) {
    els.push(El::Texte { texte: titre.into(), poids: Poids::Demi });
    for (i, l) in lignes.iter().enumerate() {
        let p = match prefixe {
            Prefixe::Numero => format!("{}.  ", i + 1),
            Prefixe::Puce => "•  ".into(),
            Prefixe::Rien => String::new(),
        };
        els.push(El::Texte { texte: format!("{p}{l}"), poids: Poids::Normal });
    }
}
#[derive(Clone, Copy)]
enum Prefixe {
    Numero,
    Puce,
    Rien,
}

fn details(els: &mut Vec<El>, titre: &str, lignes: Vec<String>) {
    let l: Vec<String> = lignes.into_iter().filter(|x| !x.is_empty()).collect();
    if !l.is_empty() {
        els.push(El::Details { titre: titre.into(), lignes: l });
    }
}

/// `Format-ValeurTech`
pub fn valeur_tech(v: &Value) -> String {
    match v {
        Value::Null => "vide".into(),
        Value::Array(a) => {
            let mut t = a.iter().take(16).map(net::texte).collect::<Vec<_>>().join(",");
            if a.len() > 16 {
                t.push_str("...");
            }
            t
        }
        x => {
            let t = net::texte(x);
            let n: Vec<u16> = t.encode_utf16().collect();
            if n.len() > 120 { format!("{}...", String::from_utf16_lossy(&n[..117])) } else { t }
        }
    }
}

/// `Format-DemarrageService`
pub fn demarrage_service(t: &str) -> String {
    match t.to_lowercase().as_str() {
        "automatic" => "automatique".into(),
        "manual" => "manuel".into(),
        "disabled" => "désactivé".into(),
        _ => t.into(),
    }
}

fn nom_batterie(k: &str) -> &'static str {
    match k {
        "CritAction" => "action à batterie critique",
        "CritLevel" => "niveau de batterie critique",
        "LowLevel" => "niveau de batterie faible",
        "LowNotify" => "alerte de batterie faible",
        "LowAction" => "action à batterie faible",
        _ => "",
    }
}

/// `Format-ValeurBat`
pub fn valeur_batterie(k: &str, v: Option<i64>) -> String {
    let Some(v) = v else { return "inconnu".into() };
    if k.to_lowercase().ends_with("action") {
        let j = if v < 0 { ACTIONS_BATTERIE.len() as i64 + v } else { v };
        return usize::try_from(j)
            .ok()
            .and_then(|j| ACTIONS_BATTERIE.get(j))
            .map(|s| s.to_string())
            .unwrap_or_else(|| v.to_string());
    }
    if k == "LowNotify" {
        return if v == 1 { "activée" } else { "coupée" }.into();
    }
    format!("{v} %")
}

/// `Format-Declaration` : une ligne lisible, avec la valeur actuelle (lue sans rien changer) et celle d'après
fn ligne_declaration(m: &mut Moteur, d: &Declaration, cache: &mut Option<crate::analyse::Batterie>) -> String {
    match d.sorte.as_str() {
        "reg" => {
            let st = m.pc.reg_lire(&d.chemin, &d.nom);
            let maint = match &st {
                Err(_) => "illisible sans droits administrateur".to_string(),
                Ok(Some(v)) => valeur_tech(&v.json()),
                Ok(None) => "absente".into(),
            };
            let apres = if d.efface {
                "effacée".to_string()
            } else {
                valeur_tech(&d.valeur.as_ref().map(|v| v.json()).unwrap_or(Value::Null))
            };
            let pareil = match &st {
                Err(_) => false,
                Ok(x) => {
                    if d.efface {
                        x.is_none()
                    } else {
                        x.is_some() && maint.to_lowercase() == apres.to_lowercase()
                    }
                }
            };
            format!(
                "Registre {} : valeur « {} », maintenant {maint}, après {apres}{}",
                d.chemin.strip_prefix("Registry::").unwrap_or(&d.chemin),
                d.nom,
                if pareil { " (déjà comme ça)" } else { "" }
            )
        }
        "pcfg" => {
            if cache.is_none() {
                // (déjà lu en fond pour la fiche, sinon lu à l'instant ; gels)
                *cache = Some(crate::analyse::reglages_batterie_lus(
                    &m.lire_powercfg(crate::corrections::LecturePowercfg::Batterie),
                ));
            }
            let cur = cache.as_ref().and_then(|c| crate::analyse::batterie_get(c, &d.cle));
            let maint = match cur {
                Some(c) => {
                    format!("secteur {}, batterie {}", valeur_batterie(&d.cle, c[0]), valeur_batterie(&d.cle, c[1]))
                }
                None => "inconnu".into(),
            };
            let guid = REGLAGES_BATTERIE.iter().find(|(k, _)| *k == d.cle).map(|(_, g)| *g).unwrap_or("");
            format!(
                "Alimentation, {} (réglage {guid}) : maintenant {maint}, après {} (secteur et batterie)",
                nom_batterie(&d.cle),
                valeur_batterie(&d.cle, Some(d.nombre))
            )
        }
        "display" => {
            let hz =
                m.pc.ecrans()
                    .into_iter()
                    .find(|e| e.split('|').next() == Some(d.ecran.as_str()))
                    .map(|e| e.split('|').nth(4).and_then(|x| x.trim().parse::<i64>().ok()).unwrap_or(0));
            format!(
                "Écran {} : fréquence maintenant {}, après {} Hz (même résolution)",
                d.ecran,
                hz.map(|h| format!("{h} Hz")).unwrap_or_else(|| "inconnue".into()),
                d.hz
            )
        }
        "power" => {
            let s = crate::reglages::plan_actif_lu(&m.lire_powercfg(crate::corrections::LecturePowercfg::PlanActif));
            let pareil = s.as_ref().is_some_and(|(g, _)| !d.guid.is_empty() && g.eq_ignore_ascii_case(&d.guid));
            format!(
                "Plan d'alimentation : maintenant « {} », après « {} »{}",
                s.as_ref().map(|(_, n)| n.clone()).unwrap_or_else(|| "inconnu".into()),
                d.cible,
                if pareil { " (déjà comme ça)" } else { "" }
            )
        }
        "svc" => {
            let dem = if d.demarrage.is_empty() { "Disabled" } else { d.demarrage.as_str() };
            let apres = demarrage_service(dem);
            match m.pc.service(&d.nom) {
                None => format!("Service {} : absent de ce PC, rien à faire", d.nom),
                Some((debut, tourne)) => format!(
                    "Service {} : démarrage maintenant {}{}, après {apres}{}{}",
                    d.nom,
                    demarrage_service(&debut),
                    if tourne { ", il tourne" } else { "" },
                    if tourne { ", arrêté" } else { "" },
                    if debut.eq_ignore_ascii_case(&d.demarrage) { " (déjà comme ça)" } else { "" }
                ),
            }
        }
        "texte" => d.texte.clone(),
        "partiel" => {
            "Et d'autres changements décidés pendant l'exécution, visibles ensuite dans l'historique (page Sauvegarde)."
                .into()
        }
        x => format!("Changement non décrit ({x})"),
    }
}

/// `Get-DetailsAvant` : les détails techniques d'une correction avant de la faire (lecture seule)
pub fn details_avant(m: &mut Moteur, c: &crate::modele::Changements) -> Vec<String> {
    let liste = match m.resoudre(c) {
        Ok(l) => l,
        Err(e) => return vec![format!("Détails impossibles à lire : {e}")],
    };
    let mut cache = None;
    liste.iter().map(|d| ligne_declaration(m, d, &mut cache)).collect()
}

/// `Get-FixKind` : (texte, couleur) du badge
fn genre(f: &Constat) -> (&'static str, &'static str) {
    if f.auto() { ("L'app s'en charge", couleurs::OK) } else { ("À faire toi-même", couleurs::INFO) }
}

/// `Open-Sheet` d'un seul point
pub fn ouvrir(m: &mut Moteur, f: &Constat, ignores: &[String]) -> Fiche {
    let mut els = Vec::new();
    let ignore = ignores.iter().any(|x| x.eq_ignore_ascii_case(&f.id));
    let fix = f.correction.as_ref();
    els.push(El::Tete { couleur: couleurs::de(&f.statut).into(), titre: f.titre.clone() });
    let mut et = Vec::new();
    if f.gain > 0.0 {
        et.push((format!("+{} points au score", f.gain as i64), couleurs::OK.to_string()));
    }
    if fix.is_some() {
        let (t, c) = genre(f);
        et.push((t.into(), c.into()));
    }
    if fix.is_some_and(|x| x.redemarrage) {
        et.push(("Redémarrage requis".into(), "#A6A1BC".into()));
    }
    els.push(El::Etiquettes(et));
    if !f.detail.is_empty() {
        section(&mut els, "Ce qu'on a trouvé", std::slice::from_ref(&f.detail), Prefixe::Rien);
    }
    if let Some(x) = fix.filter(|x| !x.pourquoi.is_empty()) {
        section(&mut els, "Pourquoi ça compte", std::slice::from_ref(&x.pourquoi), Prefixe::Rien);
    }
    if let Some(x) = fix {
        if x.auto {
            section(&mut els, "Ce que Nevermind va faire quand tu cliques sur Exécuter", &x.quoi, Prefixe::Puce);
            if x.execution.is_some() {
                let l = details_avant(m, &x.changements);
                details(&mut els, "Détails techniques", l);
            }
            let mut sur =
                "Tu peux revenir en arrière à tout moment depuis la page Sauvegarde (dans Protection).".to_string();
            if x.restauration {
                sur = format!("Un point de restauration Windows est créé avant. {sur}");
            }
            els.push(El::Info(sur));
        } else {
            if !x.etapes.is_empty() {
                section(&mut els, "Ce que tu dois faire", &x.etapes, Prefixe::Numero);
            }
            if !x.quoi.is_empty() {
                section(&mut els, "Ce que Nevermind peut faire pour t'aider", &x.quoi, Prefixe::Puce);
            }
            if x.execution.is_some() {
                let l = details_avant(m, &x.changements);
                details(&mut els, "Détails techniques", l);
            }
        }
    }
    if ignore {
        els.push(El::Info("Ce point est ignoré : il ne compte plus dans ton score.".into()));
    }
    let ouvrir =
        fix.and_then(|x| x.ouvrir.clone()).or_else(|| if f.action.is_empty() { None } else { Some(f.action.clone()) });
    let libelle_ouvrir = if let Some(x) = fix.filter(|x| x.ouvrir.is_some()) {
        x.libelle_ouvrir.clone()
    } else if !f.libelle_action.is_empty() {
        f.libelle_action.clone()
    } else {
        "Ouvrir".into()
    };
    let boutons = Boutons {
        run: fix.filter(|x| x.execution.is_some() && !ignore).map(|x| x.libelle.clone()),
        open: ouvrir.as_ref().map(|_| libelle_ouvrir),
        ignore: (f.statut != "ok")
            .then(|| if ignore { "Ne plus ignorer".to_string() } else { "Ignorer (c'est voulu)".to_string() }),
        close: Some("Fermer".into()),
    };
    Fiche { mode: "fix".into(), els, boutons, items: vec![f.clone()], ouvrir }
}

/// `Open-Sheet` de plusieurs points : « Tout corriger en un clic »
pub fn tout_corriger(m: &mut Moteur, items: &[Constat]) -> Fiche {
    let mut els = Vec::new();
    let n = items.len();
    let somme: f64 = items.iter().map(|f| f.gain).sum();
    els.push(El::Texte { texte: "Tout corriger en un clic".into(), poids: Poids::Gras });
    els.push(El::Texte {
        texte: format!(
            "L'app va appliquer {n} correction{}, pour environ +{} points :",
            if n > 1 { "s" } else { "" },
            net::texte_nombre(somme)
        ),
        poids: Poids::Normal,
    });
    for f in items {
        let quoi = f.correction.as_ref().map(|c| c.quoi.clone()).unwrap_or_default();
        section(&mut els, &format!("{}   (+{} pts)", f.titre, f.gain as i64), &quoi, Prefixe::Puce);
    }
    for f in items {
        let l = f.correction.as_ref().map(|c| details_avant(m, &c.changements)).unwrap_or_default();
        details(&mut els, &format!("Détails techniques : {}", f.titre), l);
    }
    let mut sur = "Un point de restauration Windows est créé avant. Tu peux annuler ces corrections depuis la page Sauvegarde (dans Protection).".to_string();
    if items.iter().any(|f| f.correction.as_ref().is_some_and(|c| c.redemarrage)) {
        sur.push_str(" Certains réglages demandent un redémarrage.");
    }
    els.push(El::Info(sur));
    Fiche {
        mode: "fix".into(),
        els,
        boutons: Boutons {
            run: Some(format!("Exécuter les {n} corrections")),
            open: None,
            ignore: None,
            close: Some("Fermer".into()),
        },
        items: items.to_vec(),
        ouvrir: None,
    }
}

/// `Show-ResultSheet`
pub fn resultat(titre: &str, lignes: &[String], peut_annuler: bool, note: Option<&str>) -> Fiche {
    let mut els = vec![El::Tete { couleur: couleurs::OK.into(), titre: titre.into() }];
    for l in lignes {
        els.push(El::Texte { texte: l.clone(), poids: Poids::Normal });
    }
    if let Some(n) = note.filter(|n| !n.is_empty()) {
        els.push(El::Info(n.into()));
    }
    if peut_annuler {
        els.push(El::Info(
            "Si quelque chose ne va pas, « Revenir en arrière » annule exactement ces changements.".into(),
        ));
    }
    Fiche {
        mode: "result".into(),
        els,
        boutons: Boutons {
            run: None,
            open: None,
            ignore: peut_annuler.then(|| "Revenir en arrière".to_string()),
            close: Some("OK".into()),
        },
        items: Vec::new(),
        ouvrir: None,
    }
}

/// `Confirm-DisplayChange` : « Tes écrans s'affichent bien ? », avec le compte à rebours
pub fn ecran(restant: i64) -> Fiche {
    let els = vec![
        El::Texte { texte: "Tes écrans s'affichent bien ?".into(), poids: Poids::Gras },
        El::Texte { texte: "La fréquence de l'écran vient d'être changée. Si un écran est resté noir ou affiche un message d'erreur, ne touche à rien : Nevermind revient tout seul à l'ancien réglage.".into(), poids: Poids::Normal },
        El::Info("Sans réponse, retour automatique à l'ancien réglage.".into()),
    ];
    Fiche {
        mode: "display".into(),
        els,
        boutons: Boutons {
            run: Some("Oui, garder".into()),
            open: None,
            ignore: Some(format!("Revenir en arrière ({restant})")),
            close: None,
        },
        items: Vec::new(),
        ouvrir: None,
    }
}

/// Une question Oui / Non posée dans la fiche (`Confirm-Action` : dans l'écran de la version PowerShell, une question
/// par-dessus la page ; voir DIVERGENCES.md)
pub fn question(texte: &str) -> Fiche {
    Fiche {
        mode: "question".into(),
        els: vec![El::Texte { texte: texte.into(), poids: Poids::Normal }],
        boutons: Boutons { run: Some("Oui".into()), open: None, ignore: None, close: Some("Non".into()) },
        items: Vec::new(),
        ouvrir: None,
    }
}

/// Un texte de la fiche : « 1.  étape » ou « •  puce » (le reste du texte, sans retour à la ligne au milieu)
fn motif(t: &str, numero: bool) -> Option<String> {
    let corps = t.strip_suffix('\n').unwrap_or(t);
    if corps.contains('\n') {
        return None;
    }
    if numero {
        let n = corps.chars().take_while(|c| c.is_ascii_digit()).count();
        if n == 0 {
            return None;
        }
        let r = corps[n..].strip_prefix('.')?;
        let espaces = r.chars().take_while(|c| c.is_whitespace()).count();
        if espaces == 0 {
            return None;
        }
        Some(r.chars().skip(espaces).collect())
    } else {
        let r = corps.strip_prefix('•')?;
        let espaces = r.chars().take_while(|c| c.is_whitespace()).count();
        if espaces == 0 {
            return None;
        }
        Some(r.chars().skip(espaces).collect())
    }
}

/// `Get-FicheKit` : la fiche pour l'écran
pub fn page(f: &Fiche, de: Option<&str>) -> Value {
    let mut titre = String::new();
    let mut avant = String::new();
    let mut ton = String::new();
    let mut etiq: Vec<(String, String)> = Vec::new();
    let mut blocs: Vec<Value> = Vec::new();
    let mut liste: Option<Vec<String>> = None;
    let mut sorte = "";
    let finir = |blocs: &mut Vec<Value>, liste: &mut Option<Vec<String>>, sorte: &str| {
        if let Some(l) = liste.take()
            && !l.is_empty()
        {
            if sorte == "etapes" {
                blocs.push(json!({ "type": "etapes", "elements": l.iter().map(|t| json!({ "texte": t })).collect::<Vec<_>>() }));
            } else {
                blocs.push(json!({ "type": "liste", "elements": l }));
            }
        }
    };
    for el in &f.els {
        match el {
            El::Details { titre: t, lignes } => {
                finir(&mut blocs, &mut liste, sorte);
                let q = json!({ "question": t, "lignes": lignes, "reponse": lignes.join(" ; "), "icone": "liste" });
                if let Some(der) = blocs.last_mut()
                    && der["type"] == "faq"
                {
                    if let Some(a) = der["elements"].as_array_mut() {
                        a.push(q);
                    }
                    continue;
                }
                blocs.push(json!({ "type": "faq", "elements": [q] }));
            }
            El::Tete { couleur, titre: t } => {
                if titre.is_empty() {
                    titre = t.clone();
                }
                ton = ton_couleur(couleur).into();
            }
            El::Etiquettes(l) => {
                for (t, c) in l {
                    etiq.push((t.clone(), ton_couleur(c).into()));
                }
            }
            El::Info(t) => {
                finir(&mut blocs, &mut liste, sorte);
                blocs.push(json!({ "type": "note", "texte": t }));
            }
            El::Texte { texte: t, poids } => {
                if titre.is_empty() {
                    if *poids == Poids::Gras {
                        titre = t.clone();
                    } else {
                        avant = t.clone();
                    }
                    continue;
                }
                if let Some(x) = motif(t, true) {
                    if sorte != "etapes" && liste.is_some() {
                        finir(&mut blocs, &mut liste, sorte);
                    }
                    if liste.is_none() {
                        liste = Some(Vec::new());
                        sorte = "etapes";
                    }
                    if let Some(l) = liste.as_mut() {
                        l.push(x);
                    }
                    continue;
                }
                if let Some(x) = motif(t, false) {
                    if sorte != "liste" && liste.is_some() {
                        finir(&mut blocs, &mut liste, sorte);
                    }
                    if liste.is_none() {
                        liste = Some(Vec::new());
                        sorte = "liste";
                    }
                    if let Some(l) = liste.as_mut() {
                        l.push(x);
                    }
                    continue;
                }
                finir(&mut blocs, &mut liste, sorte);
                if *poids == Poids::Normal {
                    blocs.push(json!({ "type": "texte", "texte": t }));
                } else {
                    blocs.push(json!({ "type": "titre", "texte": t }));
                }
            }
        }
    }
    finir(&mut blocs, &mut liste, sorte);
    let ouvrir = |t: &str| t.strip_prefix("Onglet ").map(|r| format!("Ouvrir {r}")).unwrap_or_else(|| t.to_string());
    let (mut ic, mut coul) = match ton.as_str() {
        "ok" => ("ok", "vert"),
        "attention" => ("alerte", "orange"),
        "urgent" => ("alerte", "rouge"),
        _ => ("aide", "P"),
    };
    if f.mode == "display" {
        ic = "ecran";
        coul = "orange";
    }
    let mut boutons_m = Vec::new();
    if let Some(t) = &f.boutons.run {
        boutons_m.push(json!({ "id": "fiche:run", "texte": ouvrir(t) }));
    }
    if let Some(t) = &f.boutons.open {
        boutons_m.push(json!({ "id": "fiche:open", "texte": ouvrir(t), "style": "second" }));
    }
    if let Some(t) = &f.boutons.ignore {
        blocs.push(json!({ "type": "bouton", "id": "fiche:ignore", "texte": ouvrir(t), "style": "second" }));
    }
    if let Some(t) = &f.boutons.close {
        blocs.push(json!({ "type": "bouton", "id": "fiche:close", "texte": ouvrir(t), "style": "second" }));
    }
    let mt = etiq.first().map(|e| e.0.clone()).unwrap_or_default();
    let mx = etiq.iter().skip(1).map(|e| e.0.clone()).collect::<Vec<_>>().join(" · ");
    let mut module = Map::new();
    module.insert("icone".into(), json!(ic));
    module.insert("couleur".into(), json!(coul));
    module.insert("titre".into(), json!(mt));
    module.insert("texte".into(), json!(mx));
    module.insert("boutons".into(), Value::Array(boutons_m));
    json!({
        "titre": if titre.is_empty() { "Nevermind".to_string() } else { titre },
        "sous": avant, "icone": ic, "couleur": coul, "retour": "appli",
        "parent": de.filter(|d| !d.is_empty() && *d != "accueil" && *d != "fiche"),
        "module": Value::Object(module),
        "blocs": blocs
    })
}
