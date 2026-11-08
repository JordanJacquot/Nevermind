//! Audit M10 (point 2) : le détail d'une partie mesurée avait perdu les conseils et les boutons « Régler » de son
//! diagnostic. La version PowerShell montre chaque cause (`Add-DiagItem` : titre, détail, Items, Actions ;
//! diagnostic-fps.ps1:130-279) avec `New-SecurityCard` (securite.ps1:289-345) : toutes les lignes de conseil, puis un
//! bouton par action (le premier en avant), qui fait le geste après sa question (`Confirm`).
//!
//! Les diagnostics viennent du juge PowerShell du bloc perf (nevermind-perf/tests/juges/diagnostic.json, sorties de
//! Get-FpsDiagnosis) : chaque conseil et chaque bouton doivent être dans le panneau, dans l'ordre. Puis chaque sorte de
//! bouton fait son geste. Faux Windows, fausse Aide, dossier d'essai : rien ne touche le vrai PC.

use nevermind_jeux::BlocJeux;
use nevermind_jeux::systeme::SystemeEssai;
use nevermind_noyau::{Bloc, Contexte, Effet, Effets, dossier_essai};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const ECRAN: &str = r"\\.\DISPLAY1";
const EXE: &str = r"C:\Jeux\Mon jeu\monjeu.exe";

/// Les diagnostics de Get-FpsDiagnosis (sans les relevés), avec le geste de chaque bouton (« quoi ») que le juge ne
/// garde pas : retrouvé par son texte
fn diagnostics_powershell() -> Vec<Value> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../nevermind-perf/tests/juges/diagnostic.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let mut l: Vec<Value> = v["cas"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["entree"].get("releves").is_none())
        .map(|c| c["attendu"].clone())
        .collect();
    for d in &mut l {
        for e in d["elements"].as_array_mut().into_iter().flatten() {
            for a in e["actions"].as_array_mut().into_iter().flatten() {
                let t = a["texte"].as_str().unwrap_or("");
                let quoi = match t {
                    "Utiliser la carte puissante" => "gpu",
                    "Mettre en mode performances" => "alimentation",
                    "Activer le mode jeu" => "modejeu",
                    "Régler l'écran" => "ecran",
                    "Donner la priorité au jeu" => "priorite",
                    "Page du pilote" => "pilote",
                    _ if t.starts_with("Fermer ") => "fermer",
                    _ => panic!("bouton inconnu : {t}"),
                };
                a["quoi"] = json!(quoi);
            }
        }
    }
    assert!(l.len() >= 350, "le juge du diagnostic a été vidé ou coupé ({})", l.len());
    l
}

struct Essai {
    b: BlocJeux,
    ctx: Arc<Contexte>,
    dossier: PathBuf,
    aide: Arc<Mutex<Vec<(String, Value)>>>,
}

impl Essai {
    fn nouveau(nom: &str) -> Essai {
        let dossier = dossier_essai(nom);
        let _ = std::fs::remove_dir_all(&dossier);
        let donnees = dossier.join("donnees");
        std::fs::create_dir_all(&donnees).unwrap();
        let partie = json!([{ "Id": "p1", "Date": "2026-11-06T14:00:00", "Game": "Mon jeu", "Key": "monjeu",
            "Avg": 42.0, "Low1": 18.0, "Low01": 12.0, "Seconds": 900, "Frames": 37800,
            "Series": [40, 44, 38, 45, 41], "Diag": { "Path": EXE, "Display": ECRAN } }]);
        std::fs::write(donnees.join("fps.json"), partie.to_string()).unwrap();
        let ctx = Arc::new(Contexte::nouveau(dossier.join("appli"), donnees, false, true));
        let aide = Arc::new(Mutex::new(Vec::new()));
        let a = aide.clone();
        ctx.fausse_aide(Arc::new(move |action: &str, p: &Value| {
            a.lock().unwrap().push((action.to_string(), p.clone()));
            Ok(json!(1))
        }));
        let mut b = BlocJeux::avec(Arc::new(SystemeEssai::default()));
        let mut e = Effets::default();
        assert!(b.action(&ctx, "fps:p1", &Value::Null, &mut e), "le détail de la partie ne s'ouvre pas");
        Essai { b, ctx, dossier, aide }
    }

    fn diagnostic(&mut self, d: &Value) {
        let mut e = Effets::default();
        self.b.signal(&self.ctx, "diagnostic-fps-pret", &json!({ "Id": "p1", "diagnostic": d }), &mut e);
    }

    /// Les blocs de « D'OÙ ÇA VIENT » (jusqu'au titre suivant)
    fn causes(&mut self) -> Vec<Value> {
        let p = self.b.pages(&self.ctx, "panneau");
        let blocs = p["panneau"]["blocs"].as_array().cloned().expect("le panneau de la partie");
        let i = blocs.iter().position(|b| b["texte"] == "D'OÙ ÇA VIENT").expect("le titre D'OÙ ÇA VIENT");
        blocs[i + 1..].iter().take_while(|b| b["type"] != "titre").cloned().collect()
    }

    fn action(&mut self, id: &str, valeur: Value) -> Vec<Effet> {
        let mut e = Effets::default();
        assert!(self.b.action(&self.ctx, id, &valeur, &mut e), "{id} n'est pas pris");
        self.b.travaux_finis(&self.ctx, &mut e);
        e.0
    }
}

impl Drop for Essai {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dossier);
    }
}

fn signal<'a>(e: &'a [Effet], nom: &str) -> Option<&'a Value> {
    e.iter().find_map(|x| match x {
        Effet::Signal { nom: n, donnees } if n == nom => Some(donnees),
        _ => None,
    })
}

fn annonces(e: &[Effet]) -> Vec<String> {
    e.iter()
        .filter_map(|x| match x {
            Effet::Annonce(t) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn le_panneau_montre_les_conseils_et_les_boutons_du_diagnostic() {
    let mut s = Essai::nouveau("jeux-diagnostic-panneau");
    let (mut lignes, mut boutons) = (0, 0);
    for (n, d) in diagnostics_powershell().iter().enumerate() {
        s.diagnostic(d);
        let vus = s.causes();
        // Ce que New-SecurityCard montre pour chaque cause : le titre et le détail, les conseils, les boutons
        let mut attendu = vec![("ligne".to_string(), d["titre"].as_str().unwrap_or("").to_string())];
        for (k, e) in d["elements"].as_array().into_iter().flatten().enumerate() {
            attendu
                .push(("ligne".into(), format!("{}|{}", e["titre"].as_str().unwrap(), e["detail"].as_str().unwrap())));
            let l: Vec<&str> = e["lignes"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            if !l.is_empty() {
                lignes += l.len();
                attendu.push(("liste".into(), l.join("\n")));
            }
            for (j, a) in e["actions"].as_array().into_iter().flatten().enumerate() {
                boutons += 1;
                let style = if j > 0 { "second" } else { "" };
                attendu.push(("bouton".into(), format!("fps-diag:{k}:{j}|{}|{style}", a["texte"].as_str().unwrap())));
            }
        }
        let vu: Vec<(String, String)> = vus
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let t = b["type"].as_str().unwrap_or("").to_string();
                let s = |k: &str| b[k].as_str().unwrap_or("").to_string();
                let x = match t.as_str() {
                    "ligne" if i == 0 => s("texte"),
                    "ligne" => format!("{}|{}", s("texte"), s("note")),
                    "liste" => b["elements"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|x| x.as_str().unwrap_or("").to_string())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    "bouton" => format!("{}|{}|{}", s("id"), s("texte"), s("style")),
                    _ => b.to_string(),
                };
                (t, x)
            })
            .collect();
        assert_eq!(vu, attendu, "diagnostic n° {n} : le panneau ne montre pas ce que montre la version PowerShell");
    }
    assert!(lignes > 500 && boutons > 300, "trop peu de conseils ({lignes}) ou de boutons ({boutons}) essayés");
}

/// Un diagnostic qui a ce bouton, et sa place (élément, bouton)
fn avec_bouton(quoi: &str) -> (Value, String, Value) {
    for d in diagnostics_powershell() {
        for (k, e) in d["elements"].as_array().into_iter().flatten().enumerate() {
            for (j, a) in e["actions"].as_array().into_iter().flatten().enumerate() {
                if a["quoi"] == quoi {
                    return (d.clone(), format!("fps-diag:{k}:{j}"), a.clone());
                }
            }
        }
    }
    panic!("aucun diagnostic avec le bouton {quoi}");
}

#[test]
fn chaque_bouton_du_diagnostic_fait_son_geste() {
    // « Utiliser la carte puissante » et « Donner la priorité au jeu » : le profil du jeu (Set-GameProfile), puis le
    // message de la version PowerShell
    for (quoi, message) in [
        ("gpu", "C'est réglé. Relance Mon jeu pour que ce soit pris en compte."),
        ("priorite", "C'est réglé : Mon jeu passera avant les autres programmes à son prochain lancement."),
    ] {
        let mut s = Essai::nouveau(&format!("jeux-diagnostic-{quoi}"));
        let (d, id, _) = avec_bouton(quoi);
        s.diagnostic(&d);
        let e = s.action(&id, Value::Null);
        let decl = signal(&e, "declarations").unwrap_or_else(|| panic!("{quoi} : le profil du jeu n'est pas changé"));
        let attendu = serde_json::to_string(&decl["changements"]).unwrap();
        assert!(attendu.contains("monjeu.exe"), "{quoi} : le profil ne vise pas le programme du jeu : {attendu}");
        assert!(decl["titre"].as_str().unwrap().starts_with("Profil de Mon jeu"), "{quoi} : {decl}");
        assert!(annonces(&e).contains(&message.to_string()), "{quoi} : pas le message attendu : {:?}", annonces(&e));
    }

    // « Mettre en mode performances » : Invoke-TweakFix @('power'), par le bloc pc
    let mut s = Essai::nouveau("jeux-diagnostic-alimentation");
    let (d, id, _) = avec_bouton("alimentation");
    s.diagnostic(&d);
    let e = s.action(&id, Value::Null);
    assert_eq!(signal(&e, "appliquer-reglages-windows"), Some(&json!({ "ids": ["power"] })));

    // « Régler l'écran » : la fiche du point de l'écran du jeu (Open-Sheet), par le bloc pc
    let mut s = Essai::nouveau("jeux-diagnostic-ecran");
    let (d, id, _) = avec_bouton("ecran");
    s.diagnostic(&d);
    let e = s.action(&id, Value::Null);
    assert_eq!(signal(&e, "ouvrir-fiche"), Some(&json!({ "id": format!("display:{ECRAN}") })));

    // « Page du pilote » : l'adresse du pilote (Open-Url)
    let mut s = Essai::nouveau("jeux-diagnostic-pilote");
    let (d, id, a) = avec_bouton("pilote");
    s.diagnostic(&d);
    let e = s.action(&id, Value::Null);
    let url = a["arg"].as_str().unwrap().to_string();
    assert!(url.starts_with("https://"), "{url}");
    assert!(e.contains(&Effet::OuvrirAdresse(url)), "la page du pilote ne s'ouvre pas : {e:?}");

    // « Activer le mode jeu » : le réglage GameMode
    let mut s = Essai::nouveau("jeux-diagnostic-modejeu");
    let (d, id, _) = avec_bouton("modejeu");
    s.diagnostic(&d);
    let e = s.action(&id, Value::Null);
    assert_eq!(s.ctx.reglage("GameMode"), Some(json!(true)), "le mode jeu n'est pas activé");
    assert!(annonces(&e).iter().any(|t| t.starts_with("Mode jeu automatique activé")), "{:?}", annonces(&e));

    // « Fermer X » : la question d'abord (Confirm) ; « Non » ne ferme rien, « Oui » ferme par l'Aide
    let mut s = Essai::nouveau("jeux-diagnostic-fermer");
    let (d, id, a) = avec_bouton("fermer");
    let nom = a["arg"].as_str().unwrap().to_string();
    s.diagnostic(&d);
    for reponse in ["non", "oui"] {
        let e = s.action(&id, Value::Null);
        let q = signal(&e, "question").expect("pas de question avant de fermer").clone();
        assert_eq!(q["texte"], a["confirmer"], "pas la question de la version PowerShell");
        assert!(s.aide.lock().unwrap().is_empty(), "un programme est fermé avant la réponse");
        let e = s.action("jeux-reponse", json!({ "id": q["id"], "valeur": reponse }));
        if reponse == "non" {
            assert!(s.aide.lock().unwrap().is_empty(), "« Non » a fermé {nom}");
        } else {
            assert_eq!(
                s.aide.lock().unwrap().clone(),
                vec![("processus.fermer".to_string(), json!({ "Nom": nom }))],
                "le programme n'est pas fermé par l'Aide"
            );
            assert!(annonces(&e).contains(&format!("{nom} fermé.")), "{:?}", annonces(&e));
        }
    }
}
