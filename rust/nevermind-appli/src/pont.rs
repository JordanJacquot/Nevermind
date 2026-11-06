//! Le pont entre le kit d'écrans (ui\kit.js) et l'appli, comme `Receive-MessageVue` de vue.ps1 : ce que la page
//! envoie par la commande `action` (un geste, l'écran montré, « Retour » depuis l'accueil, une réponse, une erreur),
//! vérifié champ par champ ; tout le reste est refusé. Repris de Mindcare en Rust (mindcare-appli\src\pont.rs).

use serde_json::Value;

/// Un message plus gros (en JSON) est refusé : un geste du kit fait quelques centaines d'octets, une recherche tapée
/// quelques milliers au plus.
pub const TAILLE_MAX: usize = 16 * 1024;
/// La profondeur la plus grande de `valeur` (le kit envoie un texte, un nombre, un oui ou non, ou `{ valeur, oui }`)
pub const PROFONDEUR_MAX: usize = 4;
/// Le texte d'une erreur de la page, coupé à cette longueur (le kit coupe aussi)
pub const ERREUR_MAX: usize = 300;

/// Un message du kit, vérifié (kit.js : « Messages envoyés à l'appli »).
#[derive(Debug, Clone, PartialEq)]
pub enum MessagePage {
    /// La page est chargée et le kit prêt (`{ type: 'pret' }`)
    Pret,
    /// Un geste : l'id d'une tuile, d'un bouton, d'une bascule, d'un champ... avec sa valeur, sur cet écran
    Action { id: String, valeur: Value, ecran: String },
    /// L'écran montré a changé (« accueil » ou l'id d'une page)
    Ecran { id: String },
    /// « Retour » depuis l'accueil : dans Mindset, retour à sa console
    Menu,
    /// Une erreur de la page (pour le journal)
    Erreur { texte: String },
    /// La réponse à une question posée par l'appli (`{ kit: 'question', id, ... }`) : l'id du bouton choisi
    Reponse { id: String, valeur: String },
}

/// Pourquoi un message est refusé (pour le journal ; la page n'en sait rien)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refus {
    /// Pas un objet JSON
    PasUnObjet,
    /// Plus de [`TAILLE_MAX`] octets
    TropGros,
    /// `type` absent ou inconnu
    Type,
    /// Un champ manque, ou n'a pas le bon type, ou est mal écrit
    Champ(&'static str),
    /// Un champ que ce type de message n'a pas
    ChampEnTrop,
    /// `valeur` plus profonde que [`PROFONDEUR_MAX`]
    TropProfond,
}

impl Refus {
    pub fn raison(self) -> String {
        match self {
            Refus::PasUnObjet => "pas un objet".into(),
            Refus::TropGros => "trop gros".into(),
            Refus::Type => "type inconnu".into(),
            Refus::Champ(c) => format!("champ {c} absent ou mal écrit"),
            Refus::ChampEnTrop => "champ en trop".into(),
            Refus::TropProfond => "valeur trop profonde".into(),
        }
    }
}

/// Un id du kit (tuile, bouton, page, question...) : 1 à 60 lettres, chiffres, `_`, `:`, `.` ou `-`, comme vue.ps1
/// (`^[\w:.-]{1,60}$`).
pub fn id_valable(id: &str) -> bool {
    let n = id.chars().count();
    (1..=60).contains(&n) && id.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | ':' | '.' | '-'))
}

fn profondeur(v: &Value) -> usize {
    match v {
        Value::Array(l) => 1 + l.iter().map(profondeur).max().unwrap_or(0),
        Value::Object(m) => 1 + m.values().map(profondeur).max().unwrap_or(0),
        _ => 0,
    }
}

/// Lit un message de la page : un objet `{ type, ... }` avec seulement les champs de son type, chacun du bon type.
pub fn lire_message(message: &Value) -> Result<MessagePage, Refus> {
    let Value::Object(m) = message else { return Err(Refus::PasUnObjet) };
    // (un objet se réécrit toujours en JSON)
    if serde_json::to_vec(message).map_or(usize::MAX, |o| o.len()) > TAILLE_MAX {
        return Err(Refus::TropGros);
    }
    let type_ = m.get("type").and_then(Value::as_str).ok_or(Refus::Type)?;
    let permis: &[&str] = match type_ {
        "pret" | "menu" => &["type"],
        "action" => &["type", "id", "valeur", "ecran"],
        "ecran" => &["type", "id"],
        "erreur" => &["type", "texte"],
        "reponse" => &["type", "id", "valeur"],
        _ => return Err(Refus::Type),
    };
    if m.keys().any(|k| !permis.contains(&k.as_str())) {
        return Err(Refus::ChampEnTrop);
    }
    let id = |champ: &'static str| -> Result<String, Refus> {
        m.get(champ).and_then(Value::as_str).filter(|i| id_valable(i)).map(str::to_string).ok_or(Refus::Champ(champ))
    };
    Ok(match type_ {
        "pret" => MessagePage::Pret,
        "menu" => MessagePage::Menu,
        "ecran" => MessagePage::Ecran { id: id("id")? },
        "reponse" => MessagePage::Reponse { id: id("id")?, valeur: id("valeur")? },
        "erreur" => {
            let t = m.get("texte").and_then(Value::as_str).ok_or(Refus::Champ("texte"))?;
            MessagePage::Erreur { texte: t.chars().take(ERREUR_MAX).collect() }
        }
        _ => {
            let valeur = m.get("valeur").cloned().unwrap_or(Value::Null);
            if profondeur(&valeur) > PROFONDEUR_MAX {
                return Err(Refus::TropProfond);
            }
            // (le kit envoie toujours l'écran d'où vient le geste)
            MessagePage::Action { id: id("id")?, valeur, ecran: id("ecran")? }
        }
    })
}

/// Un texte venu de la page, coupé pour le journal
fn court(t: &str) -> String {
    t.chars().take(120).collect()
}

/// La ligne du journal d'un message (le receveur par défaut, en attendant celui du programme)
pub fn ligne_journal(m: &MessagePage) -> String {
    match m {
        MessagePage::Pret => "Écran prêt".into(),
        // (le texte de la recherche n'est jamais écrit au journal, seulement sa longueur : c'est la phrase donnée à
        // l'assistant, ou dictée, comme dans le PowerShell)
        MessagePage::Action { id, valeur: Value::String(t), ecran } if id == "recherche" => {
            format!("Écran : action {id} (écran {ecran}, {} caractères)", t.encode_utf16().count())
        }
        MessagePage::Action { id, valeur, ecran } => {
            format!("Écran : action {id} (écran {ecran}, valeur {})", court(&valeur.to_string()))
        }
        MessagePage::Ecran { id } => format!("Écran : montre {id}"),
        MessagePage::Menu => "Écran : retour au menu de Mindset demandé".into(),
        MessagePage::Erreur { texte } => format!("Écran : erreur ({})", court(texte)),
        MessagePage::Reponse { id, valeur } => format!("Écran : réponse {valeur} à la question {id}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn la_phrase_de_la_recherche_jamais_au_journal() {
        let m =
            MessagePage::Action { id: "recherche".into(), valeur: json!("mon pc s'éteint"), ecran: "raconte".into() };
        assert_eq!(ligne_journal(&m), "Écran : action recherche (écran raconte, 15 caractères)");
        let m = MessagePage::Action { id: "idee".into(), valeur: json!("1"), ecran: "pourEtreSur".into() };
        assert_eq!(ligne_journal(&m), "Écran : action idee (écran pourEtreSur, valeur \"1\")");
    }

    #[test]
    fn messages_du_kit_lus() {
        assert_eq!(lire_message(&json!({ "type": "pret" })), Ok(MessagePage::Pret));
        assert_eq!(lire_message(&json!({ "type": "menu" })), Ok(MessagePage::Menu));
        assert_eq!(
            lire_message(&json!({ "type": "action", "id": "famille:plantages", "ecran": "accueil" })),
            Ok(MessagePage::Action { id: "famille:plantages".into(), valeur: Value::Null, ecran: "accueil".into() })
        );
        assert_eq!(
            lire_message(
                &json!({ "type": "action", "id": "geste", "valeur": { "valeur": "a", "oui": true }, "ecran": "enquete" })
            ),
            Ok(MessagePage::Action {
                id: "geste".into(),
                valeur: json!({ "valeur": "a", "oui": true }),
                ecran: "enquete".into()
            })
        );
        assert_eq!(
            lire_message(&json!({ "type": "action", "id": "recherche", "valeur": "écran bleu", "ecran": "raconte" })),
            Ok(MessagePage::Action { id: "recherche".into(), valeur: json!("écran bleu"), ecran: "raconte".into() })
        );
        assert_eq!(
            lire_message(&json!({ "type": "ecran", "id": "raconte" })),
            Ok(MessagePage::Ecran { id: "raconte".into() })
        );
        assert_eq!(
            lire_message(&json!({ "type": "reponse", "id": "a1b2c3d4e5", "valeur": "oui" })),
            Ok(MessagePage::Reponse { id: "a1b2c3d4e5".into(), valeur: "oui".into() })
        );
        let long = "x".repeat(500);
        assert_eq!(
            lire_message(&json!({ "type": "erreur", "texte": long })),
            Ok(MessagePage::Erreur { texte: "x".repeat(ERREUR_MAX) })
        );
    }

    /// Contre-épreuve : chaque message mal formé est refusé, avec sa raison.
    #[test]
    fn messages_mal_formes_refuses() {
        for (m, refus) in [
            (json!("pret"), Refus::PasUnObjet),
            (json!(["pret"]), Refus::PasUnObjet),
            (json!({}), Refus::Type),
            (json!({ "type": 3 }), Refus::Type),
            (json!({ "type": "ouvrir" }), Refus::Type),
            (json!({ "type": "pret", "fichier": "C:\\Windows" }), Refus::ChampEnTrop),
            (json!({ "type": "action", "ecran": "accueil" }), Refus::Champ("id")),
            (json!({ "type": "action", "id": "", "ecran": "accueil" }), Refus::Champ("id")),
            (json!({ "type": "action", "id": "<script>", "ecran": "accueil" }), Refus::Champ("id")),
            (json!({ "type": "action", "id": "a b", "ecran": "accueil" }), Refus::Champ("id")),
            (json!({ "type": "action", "id": "x".repeat(61), "ecran": "accueil" }), Refus::Champ("id")),
            (json!({ "type": "action", "id": 7, "ecran": "accueil" }), Refus::Champ("id")),
            (json!({ "type": "action", "id": "ok" }), Refus::Champ("ecran")),
            (json!({ "type": "action", "id": "ok", "ecran": "../x" }), Refus::Champ("ecran")),
            (json!({ "type": "action", "id": "ok", "ecran": "a", "valeur": [[[[[1]]]]] }), Refus::TropProfond),
            (json!({ "type": "action", "id": "ok", "ecran": "a", "valeur": "x".repeat(TAILLE_MAX) }), Refus::TropGros),
            (json!({ "type": "ecran" }), Refus::Champ("id")),
            (json!({ "type": "erreur", "texte": 1 }), Refus::Champ("texte")),
            (json!({ "type": "reponse", "id": "q", "valeur": true }), Refus::Champ("valeur")),
        ] {
            assert_eq!(lire_message(&m), Err(refus), "{m}");
        }
        // (à la limite : 4 niveaux passent)
        assert!(lire_message(&json!({ "type": "action", "id": "ok", "ecran": "a", "valeur": [[[[1]]]] })).is_ok());
    }
}
