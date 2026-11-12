//! La page « Mode streameur » (dans Performances) et sa carte dans la page Performances, construites pour le kit
//! (ui\kit.js) avec les mêmes blocs que les autres pages : module à gauche, lignes, choix en pastilles, notes.
//! Français simple, aucun tiret long.

use crate::calcul::{Carte, Encodeur, Logiciel, Plan, Plateforme, mbps};
use crate::reglages::{Etat, Probleme};
use serde_json::{Value, json};

/// Le message quand aucun logiciel de stream n'est installé (la carte est grisée)
pub const SANS_LOGICIEL: &str = "Pour utiliser le mode streameur, il faut avoir installé OBS ou Streamlabs.";

/// Le message quand le logiciel est ouvert au moment d'« Optimiser » (Nevermind ne le ferme jamais lui-même)
pub fn ferme_puis(l: Logiciel, bouton: &str) -> String {
    format!("Ferme {} puis reclique sur {bouton}", l.court())
}

/// Le message quand Streamlabs est fermé au moment d'« Optimiser » (il se règle ouvert, par son API)
pub fn ouvre_puis(l: Logiciel, bouton: &str) -> String {
    format!("Ouvre {} puis reclique sur {bouton}", l.court())
}

/// Streamlabs est ouvert mais son API ne répond pas (il finit de s'ouvrir, ou elle est coupée dans ses Paramètres)
pub const STREAMLABS_SANS_API: &str =
    "Streamlabs ne répond pas encore. Attends qu'il ait fini de s'ouvrir, puis reclique sur Optimiser.";

/// Streamlabs streame ou enregistre : ses réglages de sortie sont bloqués
pub const STREAMLABS_OCCUPE: &str =
    "Streamlabs est en train de streamer ou d'enregistrer. Arrête, puis reclique sur Optimiser.";

/// Ce qui a été changé la dernière fois (gardé par Nevermind)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changement {
    pub date: String,
    pub phrase: String,
    pub lignes: Vec<String>,
}

/// Un logiciel tel que la page le montre
#[derive(Debug, Clone)]
pub struct VueLogiciel {
    pub logiciel: Logiciel,
    pub etat: Result<Etat, Probleme>,
    pub ouvert: bool,
    pub plateforme: Plateforme,
    /// La plateforme a été trouvée dans ses réglages (sinon : choisie par l'utilisateur, ou Twitch par défaut)
    pub plateforme_lue: Option<Plateforme>,
    pub plateforme_choisie: bool,
    pub plan: Option<Plan>,
    /// La date de la copie des anciens réglages (« Remettre comme avant » possible)
    pub copie: Option<String>,
    pub changement: Option<Changement>,
    /// Un message de la dernière action (logiciel ouvert, erreur)
    pub alerte: Option<String>,
}

/// Toute la page
#[derive(Debug, Clone, Default)]
pub struct Vue {
    pub logiciels: Vec<VueLogiciel>,
    /// L'envoi mesuré (kb/s) et sa date
    pub envoi: Option<(u32, String)>,
    pub mesure_en_cours: bool,
    pub cartes: Vec<Carte>,
    pub ecran: Option<(u32, u32)>,
}

/// Le nom court d'une carte graphique, pour le résumé (« NVIDIA GeForce RTX 4080 » : « RTX 4080 »)
pub fn carte_courte(nom: &str) -> String {
    let mut t = nom.replace("(R)", "").replace("(TM)", "").replace("(tm)", "");
    for debut in ["NVIDIA ", "GeForce ", "AMD ", "ATI ", "Intel "] {
        if let Some(r) = t.strip_prefix(debut)
            && !r.trim().is_empty()
        {
            t = r.to_string();
        }
    }
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn nom_carte(v: &Vue, plan: Option<&Plan>) -> String {
    match plan.and_then(|p| p.carte.clone()) {
        Some(c) => carte_courte(&c),
        None if v.cartes.is_empty() => "Aucune trouvée".into(),
        None => "Aucune qui encode".into(),
    }
}

fn module(v: &Vue, l: &VueLogiciel) -> Value {
    let n = l.logiciel.court();
    let mut boutons = Vec::new();
    let (titre, texte) = match &l.etat {
        Err(Probleme::JamaisOuvert) => (
            format!("Ouvre {n} une première fois"),
            format!(
                "{n} n'a pas encore créé ses réglages. Ouvre-le une fois, ferme-le, puis reviens ici : Nevermind pourra le régler."
            ),
        ),
        Err(Probleme::Abime(f)) => (
            format!("Réglages {} illisibles", l.logiciel.de()),
            format!(
                "Le fichier {f} {} semble abîmé : Nevermind n'y touche pas. Ouvre {n}, vérifie tes réglages et ferme-le, puis réessaie.",
                l.logiciel.de()
            ),
        ),
        Ok(_) => {
            let titre = if let Some(a) = &l.alerte {
                a.clone()
            } else if l.copie.is_some() && l.changement.is_some() {
                format!("{n} est réglé")
            } else {
                format!("Prêt à régler {n}")
            };
            let texte = match &l.plan {
                Some(p) => format!("Réglage conseillé : {}.", p.phrase()),
                None if v.mesure_en_cours => "Nevermind mesure d'abord ta connexion (environ 30 secondes).".into(),
                None => {
                    "Nevermind a besoin de connaître la vitesse d'envoi de ta connexion pour choisir la qualité.".into()
                }
            };
            if let Some(p) = &l.plan {
                let t = if p.deconseille { "Optimiser quand même" } else { "Optimiser" };
                boutons.push(json!({ "id": format!("stream:optimiser:{}", l.logiciel.id()), "texte": t }));
            } else if !v.mesure_en_cours {
                boutons.push(json!({ "id": "stream:mesurer", "texte": "Mesurer ma connexion" }));
            }
            (titre, texte)
        }
    };
    if l.copie.is_some() {
        boutons.push(json!({ "id": format!("stream:remettre:{}", l.logiciel.id()), "texte": "Remettre comme avant", "style": "second" }));
    }
    let mut resume = vec![json!({ "nom": "Logiciel", "valeur": match l.etat.as_ref().ok().and_then(|e| e.version) {
        Some(ver) => format!("{} {}", l.logiciel.court(), ver.texte()),
        None => l.logiciel.nom().to_string(),
    } })];
    resume.push(json!({ "nom": "Envoi", "valeur": match &v.envoi {
        Some((k, _)) => mbps(*k),
        None if v.mesure_en_cours => "Mesure en cours".into(),
        None => "Pas mesuré".into(),
    } }));
    resume.push(json!({ "nom": "Carte graphique", "valeur": nom_carte(v, l.plan.as_ref()) }));
    if let Some((w, h)) = v.ecran {
        resume.push(json!({ "nom": "Écran", "valeur": format!("{w} x {h}") }));
    }
    resume.push(json!({ "nom": "Plateforme", "valeur": l.plateforme.nom() }));
    let ton = if l.alerte.is_some() || l.etat.is_err() { "attention" } else { "" };
    json!({ "icone": "camera", "couleur": if ton.is_empty() { "T" } else { "orange" }, "titre": titre, "texte": texte, "boutons": boutons, "resume": resume })
}

fn blocs(v: &Vue, l: &VueLogiciel) -> Vec<Value> {
    let n = l.logiciel.court();
    let mut b = Vec::new();
    // (OBS se règle fermé, par ses fichiers ; Streamlabs se règle ouvert, par sa porte officielle)
    let a_ouvrir = l.logiciel == Logiciel::Streamlabs;
    if l.ouvert && !a_ouvrir {
        b.push(json!({ "type": "ligne", "icone": "alerte", "couleur": "orange", "texte": format!("{n} est ouvert"),
            "note": "Ferme-le avant de cliquer sur Optimiser : pendant qu'il est ouvert, il ne relit pas ses réglages et remettrait les siens. Nevermind ne le ferme jamais lui-même.",
            "etiquette": "À fermer", "ton": "attention" }));
    }
    if !l.ouvert && a_ouvrir && l.etat.is_ok() {
        b.push(json!({ "type": "ligne", "icone": "alerte", "couleur": "orange", "texte": format!("{n} est fermé"),
            "note": "Ouvre-le avant de cliquer sur Optimiser : Nevermind le règle pendant qu'il est ouvert, et tu vois tout de suite le changement dans ses Paramètres.",
            "etiquette": "À ouvrir", "ton": "attention" }));
    }
    if let Some(a) = &l.alerte
        && l.ouvert == a_ouvrir
    {
        b.push(json!({ "type": "ligne", "icone": "alerte", "couleur": "orange", "texte": a, "ton": "attention" }));
    }
    if l.etat.is_err() {
        b.push(
            json!({ "type": "note", "texte": "Nevermind ne change rien tant que les réglages ne se lisent pas bien." }),
        );
        return b;
    }
    // La plateforme
    b.push(json!({ "type": "titre", "texte": "Ta plateforme" }));
    let d = match (l.plateforme_lue, l.plateforme_choisie) {
        (_, true) => "Choisie par toi. Nevermind règle la qualité selon ses limites.".to_string(),
        (Some(p), false) => format!("Trouvée dans les réglages {} : {}. Tu peux la changer.", l.logiciel.de(), p.nom()),
        (None, false) => format!("Pas trouvée dans les réglages {} : choisis-la.", l.logiciel.de()),
    };
    b.push(json!({ "type": "texte", "texte": d }));
    let options: Vec<Value> = Plateforme::TOUTES.iter().map(|p| json!({ "id": p.id(), "texte": p.nom() })).collect();
    b.push(json!({ "type": "choix", "id": format!("stream:plateforme:{}", l.logiciel.id()), "valeur": l.plateforme.id(), "options": options }));
    // Ce qui sera réglé
    if let Some(p) = &l.plan {
        if p.deconseille {
            b.push(json!({ "type": "ligne", "icone": "alerte", "couleur": "rouge", "texte": "Connexion trop lente pour bien streamer",
                "note": format!("Ta connexion envoie {}. Pour un stream correct, il faut au moins 2,5 Mb/s en envoi. Essaie avec un câble, ou quand personne d'autre n'utilise Internet à la maison.", v.envoi.as_ref().map(|e| mbps(e.0)).unwrap_or_default()),
                "etiquette": "Déconseillé", "ton": "urgent" }));
        }
        b.push(json!({ "type": "titre", "texte": "Ce que Nevermind va régler" }));
        b.push(json!({ "type": "ligne", "icone": "ecran", "couleur": "T", "texte": "Qualité de l'image", "note": "La taille de l'image envoyée et sa fluidité.", "valeur": p.qualite() }));
        let limite =
            if p.plafonne { format!(", sans dépasser la limite de {}", p.plateforme.nom()) } else { String::new() };
        b.push(json!({ "type": "ligne", "icone": "ondes", "couleur": "T", "texte": "Débit vidéo", "valeur": p.debit(),
            "note": format!("Environ 70 % de ton envoi, moins le son{limite}. Le reste garde de la marge pour ton jeu en ligne.") }));
        let note_enc = match p.encodeur {
            Encodeur::X264 => "Aucune carte graphique de ce PC ne sait encoder : c'est le processeur qui le fait. C'est lourd pendant un jeu, alors Nevermind garde une qualité modeste.".to_string(),
            _ => format!("{} a une puce faite pour ça : ton jeu garde ses FPS.", p.carte.clone().unwrap_or_else(|| p.encodeur.court().into())),
        };
        let ton_enc = if p.encodeur == Encodeur::X264 { "attention" } else { "" };
        b.push(json!({ "type": "ligne", "icone": "puce", "couleur": "T", "texte": "Encodage", "valeur": p.encodeur.court(), "note": note_enc, "ton": ton_enc }));
        b.push(json!({ "type": "ligne", "icone": "son", "couleur": "T", "texte": "Son", "valeur": format!("{} kb/s", p.son_kbps) }));
        if p.plateforme == Plateforme::Kick {
            let note = if l.logiciel == Logiciel::Obs {
                "Kick le demande. Pour pouvoir le régler, Nevermind passe la sortie d'OBS en mode Avancé. Tes enregistrements restent dans le même dossier.".to_string()
            } else if l.etat.as_ref().is_ok_and(|e| e.actuel.avance) {
                "Kick le demande.".to_string()
            } else {
                "Kick le demande. Pour pouvoir le régler, Nevermind passe la sortie de Streamlabs en mode Avancé. Tes enregistrements restent dans le même dossier.".to_string()
            };
            b.push(json!({ "type": "ligne", "icone": "horloge", "couleur": "T", "texte": "Image clé", "valeur": "Toutes les 2 secondes", "note": note }));
        }
    }
    // La connexion
    b.push(json!({ "type": "titre", "texte": "Ta connexion" }));
    match &v.envoi {
        Some((k, date)) => b.push(json!({ "type": "ligne", "icone": "internet", "couleur": "P", "texte": "Vitesse d'envoi", "valeur": mbps(*k),
            "note": format!("Mesurée le {date} par le test « Connexion Internet ». Mesure à nouveau si tu changes de box ou de Wi-Fi."),
            "bouton": if v.mesure_en_cours { Value::Null } else { json!({ "id": "stream:mesurer", "texte": "Mesurer à nouveau", "style": "second" }) } })),
        None => b.push(json!({ "type": "ligne", "icone": "internet", "couleur": "P", "texte": "Vitesse d'envoi",
            "valeur": if v.mesure_en_cours { "Mesure en cours" } else { "Pas mesurée" },
            "note": "Le test « Connexion Internet » mesure ce que ta connexion peut envoyer : c'est ce qui compte pour streamer." })),
    }
    // Ce qui a changé
    if let Some(c) = &l.changement
        && l.copie.is_some()
    {
        b.push(json!({ "type": "titre", "texte": "Ce qui a changé", "droite": c.date }));
        b.push(json!({ "type": "texte", "texte": format!("{}.", c.phrase) }));
        b.push(json!({ "type": "liste", "elements": c.lignes, "ton": "ok" }));
    }
    let profil = match &l.etat {
        Ok(e) if l.logiciel == Logiciel::Obs && e.nb_profils > 1 => {
            format!(" Profil réglé : {} (celui qu'OBS ouvre).", e.profil)
        }
        _ => String::new(),
    };
    b.push(json!({ "type": "note", "texte": format!("Avant de changer quoi que ce soit, Nevermind note tes anciens réglages : « Remettre comme avant » remet ce que Nevermind a changé, et garde ce que tu as changé toi-même dans {n} depuis. Tes scènes, tes sources et ta clé de stream ne sont jamais touchées.{profil}") }));
    b
}

/// La page « streameur »
pub fn page(v: &Vue) -> Value {
    let mut p = json!({ "titre": "Mode streameur", "parent": "perf", "icone": "camera", "couleur": "T",
        "sous": "Nevermind règle ton logiciel de stream pour un direct fluide, selon ta connexion, ta carte graphique et ta plateforme." });
    match v.logiciels.as_slice() {
        [] => {
            p["module"] = json!({ "icone": "camera", "couleur": "doux", "titre": "Aucun logiciel de stream", "texte": SANS_LOGICIEL });
            p["blocs"] = json!([{ "type": "note", "texte": "Nevermind cherche OBS Studio et Streamlabs à chaque fois que tu ouvres cette page. Un OBS qui n'est pas installé (une version portable) est trouvé dès que tu l'ouvres." }]);
        }
        [l] => {
            p["module"] = module(v, l);
            p["blocs"] = Value::Array(blocs(v, l));
        }
        l => {
            let onglets: Vec<Value> = l
                .iter()
                .map(|x| json!({ "id": x.logiciel.id(), "nom": x.logiciel.nom(), "module": module(v, x), "blocs": blocs(v, x) }))
                .collect();
            p["module"] = module(v, &l[0]);
            p["onglets"] = Value::Array(onglets);
        }
    }
    p
}

/// La carte « Mode streameur » de la page Performances (signal `streameur` au bloc perf) : grisée sans logiciel
pub fn carte_perf(disponible: bool, regle: bool) -> Value {
    if !disponible {
        return json!({ "id": "streameur", "nom": "Mode streameur", "note": SANS_LOGICIEL, "icone": "camera", "couleur": "doux", "inactif": true });
    }
    json!({ "id": "streameur", "nom": "Mode streameur", "resume": "Réglages pour streamer", "note": "Règle OBS ou Streamlabs pour streamer sans saccades", "icone": "camera", "couleur": "T",
        "etiquette": if regle { json!("Réglé") } else { Value::Null }, "ton": if regle { "ok" } else { "" } })
}

/// La phrase de l'annonce après « Remettre comme avant »
pub fn annonce_remise(l: Logiciel, b: &crate::sauvegarde::Bilan) -> String {
    let n = l.court();
    if b.remis == 0 && b.abimes > 0 {
        return format!(
            "La copie des anciens réglages est abîmée : Nevermind n'y touche pas, tes réglages {} restent comme ils sont.",
            l.de()
        );
    }
    if b.remis == 0 && b.disparus > 0 {
        return match l {
            Logiciel::Obs => "Le profil d'OBS réglé par Nevermind n'existe plus (il a été renommé ou supprimé) : il n'y a plus rien à remettre.".into(),
            Logiciel::Streamlabs => "Les réglages de Streamlabs changés par Nevermind n'existent plus : il n'y a plus rien à remettre.".into(),
        };
    }
    let mut t = if b.abimes > 0 {
        format!(
            "Une partie des réglages d'avant est revenue dans {n}. Le reste de la copie était abîmé : Nevermind n'y a pas touché."
        )
    } else {
        format!("Les réglages d'avant sont revenus dans {n}.")
    };
    if b.gardes > 0 {
        t.push_str(" Ce que tu as changé toi-même depuis est gardé.");
    }
    t
}

/// La phrase de l'annonce après « Optimiser »
pub fn annonce_reglee(l: Logiciel, plan: &Plan) -> String {
    match l {
        Logiciel::Obs => format!("OBS est réglé : {}.", plan.phrase()),
        Logiciel::Streamlabs => format!("Streamlabs est réglé : {}.", plan.phrase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms_courts_des_cartes() {
        assert_eq!(carte_courte("NVIDIA GeForce RTX 4080"), "RTX 4080");
        assert_eq!(carte_courte("NVIDIA RTX A2000"), "RTX A2000");
        assert_eq!(carte_courte("AMD Radeon RX 6600"), "Radeon RX 6600");
        assert_eq!(carte_courte("AMD Radeon(TM) Graphics"), "Radeon Graphics");
        assert_eq!(carte_courte("Intel(R) UHD Graphics 630"), "UHD Graphics 630");
        assert_eq!(carte_courte("NVIDIA"), "NVIDIA");
    }

    #[test]
    fn carte_de_performances() {
        let c = carte_perf(false, false);
        assert_eq!((c["inactif"].as_bool(), c["note"].as_str()), (Some(true), Some(SANS_LOGICIEL)));
        let c = carte_perf(true, true);
        assert_eq!((c.get("inactif"), c["etiquette"].as_str(), c["ton"].as_str()), (None, Some("Réglé"), Some("ok")));
        assert_eq!(ferme_puis(Logiciel::Obs, "Optimiser"), "Ferme OBS puis reclique sur Optimiser");
        assert_eq!(ferme_puis(Logiciel::Streamlabs, "Optimiser"), "Ferme Streamlabs puis reclique sur Optimiser");
    }
}
