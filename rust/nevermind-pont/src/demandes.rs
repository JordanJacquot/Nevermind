//! Les demandes de Nevermind à la DLL, en Rust ordinaire (aucun pointeur ici) : un JSON en entrée, un JSON en sortie.
//!
//! Chaque demande est un objet `{ "action": "...", ... }`. La réponse est toujours un objet avec `ok` : vrai et le
//! résultat, ou faux et `erreur` (le texte de l'erreur, pour le journal de Nevermind). Les noms des champs sont ceux du
//! corpus de cas (cas\aide\fermeture.json, canal.json) : Nevermind envoie ce que le corpus décrit.
//!
//! - `version` : `{ interface, version }` (Nevermind vérifie l'interface au chargement) ;
//! - `fermeture.refus` : `{ contexte, fiches: [...], elevePermis }` donne `{ refus: [texte ou null, ...] }`, un par
//!   fiche, dans le même ordre (null : fermable) ;
//! - `canal` : `{ dossier, sid }` donne `{ suffixe, canal, verrou }` (null si le dossier n'est pas un chemin « X:\ ») ;
//! - `dossiers.programmes` : `{ dossiers: { windows, programmes, programmesX86, donneesCommunes }, commandes: [...] }`
//!   donne `{ programmes: [chemin ou null, ...] }` ;
//! - `nettoyage.dossiers` : `{ windows }` donne `{ dossiers: { wu: [...], do: [...], dump: [...] } }`.

use aide_regles::dossiers::Dossiers;
use aide_regles::fermeture::{self, Contexte, Fiche, Parente, Reference};
use aide_regles::{canal, nettoyage};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// Numéro de l'interface : il change si une demande ou une réponse change de forme (pont.ps1 vérifie qu'il est le bon)
pub const INTERFACE: i64 = 1;

/// Une demande, sa réponse
pub fn traiter(entree: &str) -> Value {
    let d: Value = match serde_json::from_str(entree) {
        Ok(v) => v,
        Err(e) => return erreur(&format!("demande illisible ({e})")),
    };
    let resultat = match d["action"].as_str() {
        Some("version") => Ok(json!({ "interface": INTERFACE, "version": env!("CARGO_PKG_VERSION") })),
        Some("fermeture.refus") => fermeture_refus(&d),
        Some("canal") => Ok(nom_canal(&d)),
        Some("dossiers.programmes") => programmes(&d),
        Some("nettoyage.dossiers") => dossiers_nettoyage(&d),
        // Seulement dans la DLL d'essai (outils\tester-pont.ps1), jamais dans celle qui est livrée
        #[cfg(feature = "essai-panique")]
        Some("essai.panique") => panic!("panique volontaire (essai de tester-pont.ps1)"),
        Some(a) => Err(format!("demande inconnue : {a}")),
        None => Err("demande sans action".to_string()),
    };
    match resultat {
        Ok(Value::Object(mut o)) => {
            o.insert("ok".into(), Value::Bool(true));
            Value::Object(o)
        }
        Ok(autre) => json!({ "ok": true, "resultat": autre }),
        Err(e) => erreur(&e),
    }
}

/// Une réponse d'échec
pub fn erreur(texte: &str) -> Value {
    json!({ "ok": false, "erreur": texte })
}

/// Un texte, ou vide
fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// Une liste de textes (vide si absente)
fn textes(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().map(|x| s(x).to_string()).collect()).unwrap_or_default()
}

/// Le contexte de la règle « fermable », tel que le décrit cas\aide\fermeture.json (null ou absent : illisible)
fn contexte(x: &Map<String, Value>) -> Contexte {
    let champ = |n: &str| x.get(n).unwrap_or(&Value::Null);
    Contexte {
        windows: s(champ("windows")).into(),
        programmes: s(champ("programmes")).into(),
        antivirus: textes(champ("antivirus")),
        services_pid: champ("servicesPid").as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()),
        reference: champ("reference").as_object().map(|r| Reference {
            id: r.get("id").and_then(Value::as_i64).unwrap_or(0),
            session: r.get("session").and_then(Value::as_i64),
            compte: r.get("compte").map(s).unwrap_or("").into(),
            eleve: r.get("eleve").and_then(Value::as_i64),
        }),
        processus: champ("processus")
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|p| {
                        let parente =
                            Parente { parent: p["parent"].as_i64().unwrap_or(0), chemin: s(&p["chemin"]).into() };
                        (p["pid"].as_i64().unwrap_or(0), parente)
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default(),
    }
}

/// Ce que Windows dit d'un processus (0 non, 1 oui, -1 illisible ; null ou absent : `None`)
fn fiche(f: &Map<String, Value>) -> Fiche {
    let champ = |n: &str| f.get(n).unwrap_or(&Value::Null);
    Fiche {
        id: champ("id").as_i64().unwrap_or(0),
        chemin: s(champ("chemin")).into(),
        session: champ("session").as_i64(),
        compte: s(champ("compte")).into(),
        protection: champ("protection").as_i64(),
        eleve: champ("eleve").as_i64(),
    }
}

/// La règle « fermable » sur une liste de processus (la page RAM en demande plusieurs centaines d'un coup)
fn fermeture_refus(d: &Value) -> Result<Value, String> {
    let ctx = contexte(d["contexte"].as_object().ok_or("contexte absent")?);
    let fiches = d["fiches"].as_array().ok_or("fiches absentes")?;
    let eleve_permis = d["elevePermis"].as_bool().unwrap_or(false);
    let mut refus = Vec::with_capacity(fiches.len());
    for f in fiches {
        // (une fiche qui n'est pas un objet : toute la demande échoue, Nevermind ne ferme alors rien)
        let f = f.as_object().ok_or("une fiche n'est pas un objet")?;
        refus.push(fermeture::refus(&ctx, &fiche(f), eleve_permis).map_or(Value::Null, |r| Value::String(r.into())));
    }
    Ok(json!({ "refus": refus }))
}

/// Le nom du canal et du verrou de l'Aide pour ce dossier et ce compte
fn nom_canal(d: &Value) -> Value {
    let suffixe = canal::suffixe(s(&d["dossier"]));
    let sid = s(&d["sid"]);
    let noms = |f: fn(&str, &str) -> String| match &suffixe {
        Some(x) if !sid.is_empty() => Value::String(f(sid, x)),
        _ => Value::Null,
    };
    json!({ "suffixe": suffixe, "canal": noms(canal::nom_canal), "verrou": noms(canal::nom_verrou) })
}

/// Le programme de chaque commande écrite par Windows (chemin complet, ou null)
fn programmes(d: &Value) -> Result<Value, String> {
    let x = d["dossiers"].as_object().ok_or("dossiers absents")?;
    let champ = |n: &str| x.get(n).map(s).unwrap_or("").to_string();
    let dossiers = Dossiers {
        windows: champ("windows"),
        programmes: champ("programmes"),
        programmes_x86: champ("programmesX86"),
        donnees_communes: champ("donneesCommunes"),
    };
    let commandes = d["commandes"].as_array().ok_or("commandes absentes")?;
    let r: Vec<Value> =
        commandes.iter().map(|c| dossiers.programme_de_commande(s(c)).map_or(Value::Null, Value::String)).collect();
    Ok(json!({ "programmes": r }))
}

/// Les dossiers de Windows que l'Aide vide, par catégorie
fn dossiers_nettoyage(d: &Value) -> Result<Value, String> {
    let windows = s(&d["windows"]);
    let b = windows.as_bytes();
    if b.len() < 3 || !b[0].is_ascii_alphabetic() || b[1] != b':' || b[2] != b'\\' {
        return Err(format!("dossier de Windows non valable : {windows}"));
    }
    let mut o = Map::new();
    for id in ["wu", "do", "dump"] {
        let c = nettoyage::categorie(id).ok_or("catégorie inconnue")?;
        o.insert(id.into(), json!(nettoyage::dossiers(c, windows)));
    }
    Ok(json!({ "dossiers": o }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_et_erreurs() {
        let v = traiter(r#"{"action":"version"}"#);
        assert_eq!(v["ok"], true);
        assert_eq!(v["interface"], INTERFACE);
        assert_eq!(traiter("{pas du json")["ok"], false);
        assert_eq!(traiter(r#"{"action":"inconnue"}"#)["ok"], false);
        assert_eq!(traiter(r#"{"action":"fermeture.refus","fiches":[]}"#)["ok"], false);
        assert_eq!(traiter(r#"[1,2]"#)["ok"], false);
        // La demande d'essai n'existe pas dans la DLL livrée
        #[cfg(not(feature = "essai-panique"))]
        assert_eq!(traiter(r#"{"action":"essai.panique"}"#)["erreur"], "demande inconnue : essai.panique");
    }

    #[test]
    fn fermeture_par_liste() {
        let d = r#"{"action":"fermeture.refus","contexte":{"windows":"C:\\Windows","programmes":"C:\\Program Files",
            "antivirus":[],"servicesPid":[600],"reference":{"id":9000,"session":1,"compte":"S-1-5-21-1","eleve":0},
            "processus":[]},"fiches":[
            {"id":800,"chemin":"C:\\Jeux\\a.exe","session":1,"compte":"S-1-5-21-1","protection":0,"eleve":0},
            {"id":600,"chemin":"C:\\Jeux\\a.exe","session":1,"compte":"S-1-5-21-1","protection":0,"eleve":0},
            {"id":801,"chemin":"C:\\Jeux\\a.exe","session":1,"compte":"S-1-5-21-1","protection":0,"eleve":null}],
            "elevePermis":false}"#;
        let r = traiter(d);
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["refus"], json!([null, "est un service", "tourne en administrateur"]));
        // Une fiche qui n'est pas un objet : rien n'est fermable
        let r = traiter(&d.replace(r#""fiches":["#, r#""fiches":[3,"#));
        assert_eq!(r["ok"], false);
    }

    #[test]
    fn canal_et_dossiers() {
        let r = traiter(
            r#"{"action":"canal","dossier":"C:\\Program Files\\Mindset\\Programmes\\Nevermind","sid":"S-1-5-21-1"}"#,
        );
        assert_eq!(r["suffixe"], "4a8522ca5e0c");
        assert_eq!(r["canal"], "Nevermind-Aide-S-1-5-21-1-4a8522ca5e0c");
        assert_eq!(r["verrou"], "Local\\Nevermind-Aide-S-1-5-21-1-4a8522ca5e0c");
        let r = traiter(r#"{"action":"canal","dossier":"\\\\serveur\\partage","sid":"S-1-5-21-1"}"#);
        assert_eq!(r["ok"], true);
        assert!(r["suffixe"].is_null() && r["canal"].is_null());
        let r = traiter(
            r#"{"action":"dossiers.programmes","dossiers":{"windows":"C:\\Windows","programmes":"C:\\Program Files",
            "programmesX86":"C:\\Program Files (x86)","donneesCommunes":"C:\\ProgramData"},
            "commandes":["%ProgramFiles%\\AVX\\avx.exe","windowsdefender://","\"C:\\A\\b.exe\" -x"]}"#,
        );
        assert_eq!(r["programmes"], json!(["C:\\Program Files\\AVX\\avx.exe", null, "C:\\A\\b.exe"]));
        let r = traiter(r#"{"action":"nettoyage.dossiers","windows":"C:\\Windows"}"#);
        assert_eq!(r["dossiers"]["wu"], json!(["C:\\Windows\\SoftwareDistribution\\Download"]));
        assert_eq!(r["dossiers"]["dump"], json!(["C:\\Windows\\Minidump"]));
        assert_eq!(traiter(r#"{"action":"nettoyage.dossiers","windows":""}"#)["ok"], false);
    }
}
