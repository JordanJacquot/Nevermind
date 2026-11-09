//! Une fausse Aide sur le vrai canal, pour les tests (du noyau et des blocs) : le test lance un faux serveur au nom du
//! canal de SA copie d'essai (un dossier temporaire : la vraie Aide n'écoute jamais ce nom), et Nevermind lui parle
//! exactement comme à la vraie (connexion, contrôle du programme qui répond, une ligne JSON dans chaque sens).
//!
//! Refusée hors du mode test (`Contexte::essai`) : jamais un canal au nom de la vraie Aide.

use crate::Contexte;
use crate::windows::canal::{FauxServeur, Repondre};
use serde_json::{Value, json};
use std::sync::Arc;

/// Lance une fausse Aide pour ce contexte de test. `repondre` reçoit l'action et les paramètres de chaque demande et
/// rend la réponse entière (`{"ok":true,"r":...}`), `Some(Value::Null)` pour couper l'échange sans répondre, ou
/// `None` pour ne pas répondre (l'Aide en retard). Le test lui-même est le programme attendu au bout du canal.
pub fn lancer(
    ctx: &Contexte,
    repondre: impl Fn(&str, &Value) -> Option<Value> + Send + Sync + 'static,
) -> Result<FauxServeur, String> {
    if !ctx.essai {
        return Err("une fausse Aide seulement en mode test".into());
    }
    let moi = std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().into_owned();
    // (seulement le programme attendu : les attentes réglées par le test, tâche et demande de Windows, sont gardées)
    *ctx.aide.exe_attendu.lock().unwrap_or_else(|e| e.into_inner()) = Some(moi);
    let canal = ctx.aide_config().canal.clone();
    let r: Repondre = Arc::new(move |ligne: &str| {
        let d: Value = serde_json::from_str(ligne).unwrap_or(Value::Null);
        let a = d["a"].as_str().unwrap_or("");
        repondre(a, &d["p"]).map(|v| if v.is_null() { String::new() } else { v.to_string() })
    });
    FauxServeur::lancer(&canal, r)
}

/// Une réponse « réussie » de l'Aide
pub fn ok(r: Value) -> Value {
    json!({ "ok": true, "r": r })
}

/// Une réponse « refusée » de l'Aide
pub fn refus(e: &str) -> Value {
    json!({ "ok": false, "e": e })
}
