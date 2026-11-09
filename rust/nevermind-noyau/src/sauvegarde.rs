//! La sauvegarde des réglages d'origine (sauvegarde.json, modules\donnees.ps1) : `Import-Backup`, `Save-Backup`,
//! `Save-Original`, `Set-Reg`, `Clear-Reg`, `Get-TexteProblemeSauvegarde`, et les points ignorés (ignores.json).
//!
//! La sauvegarde ne dépend pas d'un seul fichier (issue #3) : sauvegarde.json, sa copie de secours (.bak, la version
//! d'avant), et les valeurs d'origine gardées dans le registre (réglages du PC : par l'Aide, dans HKLM ; réglages de
//! l'utilisateur : par Nevermind, dans HKCU). Un fichier abîmé n'est jamais remplacé en silence par une sauvegarde
//! vide : il est mis de côté, et `probleme` le dit (« Tout restaurer », désinstallation).
//!
//! Partagée par tous les blocs (un seul `$script:Backup`) : elle vit dans le `Contexte`.

use crate::Contexte;
use crate::aide::{texte_ps, vrai};
use crate::contexte::verrou;
use crate::moment;
use crate::registre;
use serde_json::{Map, Value, json};
use std::path::PathBuf;

/// `$script:Backup`
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sauvegarde {
    /// « chemin|nom » : `{ Path, Name, Existed, Value, Kind }`
    pub registre: Map<String, Value>,
    pub plan: Option<String>,
    pub plan_cree: Option<String>,
    pub curseur: Option<String>,
    pub dns: Map<String, Value>,
    pub ecrans: Map<String, Value>,
    /// Réglages déjà remis comme avant par « Tout restaurer » alors que d'autres restent à remettre
    pub oublies: Vec<String>,
    /// `$script:BackupProbleme` : « abimee », « absente », ou rien
    pub probleme: Option<String>,
}

impl Sauvegarde {
    /// Le contenu de sauvegarde.json (comme `$script:Backup | ConvertTo-Json -Depth 6`)
    pub fn en_json(&self) -> Value {
        let opt = |o: &Option<String>| o.clone().map_or(Value::Null, Value::String);
        json!({
            "Registry": self.registre,
            "PowerScheme": opt(&self.plan),
            "CreatedScheme": opt(&self.plan_cree),
            "Overlay": opt(&self.curseur),
            "Dns": self.dns,
            "Displays": self.ecrans,
            "Oublies": self.oublies,
        })
    }
}

/// Une entrée de la sauvegarde du registre (`@{ Path; Name; Existed; Value; Kind }`)
pub fn entree(chemin: &str, nom: &str, existait: bool, valeur: Value, kind: Value) -> Value {
    json!({ "Path": chemin, "Name": nom, "Existed": existait, "Value": valeur, "Kind": kind })
}

fn int_ps(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().unwrap_or_else(|| n.as_f64().map_or(0, |f| f.round() as i64)),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        Value::Bool(b) => i64::from(*b),
        _ => 0,
    }
}

/// `Read-FichierSauvegarde` : `None` s'il n'existe pas ; une erreur s'il est abîmé ou n'a pas « Registry »
fn lire_fichier(f: &std::path::Path) -> Result<Option<Map<String, Value>>, String> {
    if !f.exists() {
        return Ok(None);
    }
    let o = std::fs::read(f).map_err(|e| e.to_string())?;
    let t = suite_commun::json_sur::texte_du_fichier(&o);
    let j: Value = serde_json::from_str(t.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    match j {
        Value::Object(m) if m.contains_key("Registry") => Ok(Some(m)),
        _ => Err("contenu inattendu".into()),
    }
}

impl Contexte {
    /// Le fichier de la sauvegarde
    pub fn fichier_sauvegarde(&self) -> PathBuf {
        self.donnees.join("sauvegarde.json")
    }

    /// `Import-Backup` : relit la sauvegarde (fichier, copie de secours, valeurs d'origine du registre)
    pub fn importer_sauvegarde(&self) {
        let mut s = Sauvegarde::default();
        let fichier = self.fichier_sauvegarde();
        let mut notes: Vec<String> = Vec::new();
        let mut j = match lire_fichier(&fichier) {
            Ok(j) => j,
            Err(e) => {
                notes.push(format!("Sauvegarde abîmée ({e}) : mise de côté"));
                s.probleme = Some("abimee".into());
                let dest =
                    self.donnees.join(format!("sauvegarde-abimee-{}.json", moment::compacte(&self.maintenant())));
                let _ = std::fs::rename(&fichier, dest);
                None
            }
        };
        if j.is_none()
            && let Ok(Some(b)) = lire_fichier(&crate::fichiers::a_cote(&fichier, ".bak"))
        {
            notes.push("Sauvegarde reprise de sa copie de secours".into());
            j = Some(b);
        }
        if let Some(j) = &j {
            if let Some(Value::Object(r)) = j.get("Registry") {
                for (k, v) in r {
                    s.registre.insert(
                        k.clone(),
                        entree(
                            &texte_ps(&v["Path"]),
                            &texte_ps(&v["Name"]),
                            vrai(&v["Existed"]),
                            v["Value"].clone(),
                            v["Kind"].clone(),
                        ),
                    );
                }
            }
            let texte = |n: &str| j.get(n).filter(|v| vrai(v)).map(texte_ps);
            s.plan = texte("PowerScheme");
            s.plan_cree = texte("CreatedScheme");
            if let Some(Value::Object(d)) = j.get("Dns") {
                for (k, v) in d {
                    s.dns.insert(k.clone(), json!(texte_ps(v)));
                }
            }
            if let Some(Value::Object(d)) = j.get("Displays") {
                for (k, v) in d {
                    s.ecrans.insert(k.clone(), json!(int_ps(v)));
                }
            }
            s.curseur = texte("Overlay");
            if let Some(o) = j.get("Oublies") {
                let l: Vec<Value> = match o {
                    Value::Array(a) => a.clone(),
                    Value::Null => Vec::new(),
                    autre => vec![autre.clone()],
                };
                s.oublies = l.iter().filter(|x| vrai(x)).map(texte_ps).collect();
            }
        }
        // Les valeurs d'origine du registre complètent ce que le fichier n'a plus (fichier effacé, abîmé, ou plus ancien)
        let mut connues: std::collections::HashSet<String> = s
            .registre
            .values()
            .map(|e| format!("{}|{}", registre::cle_normale(&texte_ps(&e["Path"])), texte_ps(&e["Name"])))
            .collect();
        connues.extend(s.oublies.iter().cloned());
        let mut reprises = 0;
        for o in self.origines_registre() {
            if !vrai(&o) || !vrai(&o["Path"]) || !vrai(&o["Name"]) {
                continue;
            }
            let (p, n) = (texte_ps(&o["Path"]), texte_ps(&o["Name"]));
            let cle = format!("{}|{n}", registre::cle_normale(&p));
            if connues.contains(&cle) {
                continue;
            }
            s.registre
                .insert(format!("{p}|{n}"), entree(&p, &n, vrai(&o["Existed"]), o["Value"].clone(), o["Kind"].clone()));
            connues.insert(cle);
            reprises += 1;
        }
        if reprises > 0 {
            notes.push(format!("Sauvegarde : {reprises} valeur(s) d'origine reprise(s) du registre"));
        }
        // Plus rien alors que l'historique montre des changements pas annulés : la sauvegarde a disparu
        if s.probleme.is_none() && s.registre.is_empty() {
            let h = self.historique();
            let reste = h.iter().any(|e| {
                vrai(e)
                    && !vrai(&e["Annule"])
                    && e["Log"].as_array().is_some_and(|l| l.iter().any(|x| x["Type"].as_str() == Some("reg")))
            });
            if reste {
                s.probleme = Some("absente".into());
            }
        }
        *verrou(&self.sauvegarde) = Some(s);
        for n in notes {
            self.journal(&n);
        }
    }

    /// `$script:Backup` (importée au premier appel)
    pub fn sauvegarde(&self) -> Sauvegarde {
        if verrou(&self.sauvegarde).is_none() {
            self.importer_sauvegarde();
        }
        verrou(&self.sauvegarde).clone().unwrap_or_default()
    }

    /// Change la sauvegarde en mémoire (sans l'écrire : voir `enregistrer_sauvegarde`)
    pub fn changer_sauvegarde<R>(&self, f: impl FnOnce(&mut Sauvegarde) -> R) -> R {
        if verrou(&self.sauvegarde).is_none() {
            self.importer_sauvegarde();
        }
        let mut g = verrou(&self.sauvegarde);
        f(g.get_or_insert_with(Sauvegarde::default))
    }

    /// `Save-Backup` : écrit sauvegarde.json (UTF-8 avec BOM ; l'ancien en copie de secours)
    pub fn enregistrer_sauvegarde(&self) -> Result<(), String> {
        self.exiger_geste()?;
        let j = self.sauvegarde().en_json();
        crate::fichiers::ecrire_texte_sur(&self.fichier_sauvegarde(), &crate::json_ps::lisible(&j), true)
            .map_err(|e| e.to_string())
    }

    /// `Save-BackupOuOublier` : l'état d'origine noté AVANT le changement ; s'il n'a pas pu être écrit, il est
    /// oublié (`oublier`) et l'erreur remonte, sans rien changer.
    pub fn enregistrer_sauvegarde_ou_oublier(&self, oublier: impl FnOnce(&mut Sauvegarde)) -> Result<(), String> {
        self.exiger_geste()?;
        match self.enregistrer_sauvegarde() {
            Ok(()) => Ok(()),
            Err(e) => {
                self.changer_sauvegarde(oublier);
                Err(format!(
                    "Nevermind n'a pas pu noter l'état d'origine (sauvegarde.json : {e}) : rien n'a été changé."
                ))
            }
        }
    }

    /// `Get-TexteProblemeSauvegarde` : ce que « Tout restaurer » et la désinstallation disent quand la sauvegarde a
    /// un problème (vide sinon)
    pub fn texte_probleme_sauvegarde(&self) -> String {
        texte_probleme(self.sauvegarde().probleme.as_deref())
    }

    /// `Save-Original` : mémorise la valeur d'origine avant la première modification (et une copie dans le
    /// registre de l'utilisateur pour un réglage de l'utilisateur ; jamais en mode test).
    pub fn noter_origine(&self, chemin: &str, nom: &str) -> Result<(), String> {
        self.exiger_geste()?;
        let cle = format!("{chemin}|{nom}");
        if self.sauvegarde().registre.contains_key(&cle) {
            return Ok(());
        }
        let st = self.registre_etat(chemin, nom);
        let e = entree(chemin, nom, vrai(&st["Existed"]), st["Value"].clone(), st["Kind"].clone());
        self.changer_sauvegarde(|s| {
            s.registre.insert(cle.clone(), e.clone());
        });
        let k = cle.clone();
        self.enregistrer_sauvegarde_ou_oublier(move |s| {
            s.registre.remove(&k);
        })?;
        if registre::utilisateur(chemin) && !self.essai {
            let copie = format!("HKCU:\\{}", self.cle_origines_utilisateur());
            if crate::registre::valeur(&copie, &cle).is_none() {
                let _ =
                    crate::registre::ecrire_utilisateur(&copie, &cle, &json!(crate::json_ps::compact(&e)), "String");
            }
        }
        Ok(())
    }

    /// `Set-Reg` : note l'origine, le changement (action en cours), puis écrit (l'Aide pour un réglage du PC)
    pub fn changer_registre(&self, chemin: &str, nom: &str, valeur: &Value, kind: &str) -> Result<(), String> {
        self.exiger_geste()?;
        self.noter_origine(chemin, nom)?;
        if self.changements_en_cours() {
            let st = self.registre_etat(chemin, nom);
            self.noter_changement(json!({ "Type": "reg", "Path": chemin, "Name": nom, "Existed": st["Existed"], "Value": st["Value"], "Kind": st["Kind"], "Nouveau": valeur, "NouveauType": kind }));
        }
        self.ecrire_registre(chemin, nom, valeur, kind)
    }

    /// `Clear-Reg` : une valeur qui existe est effacée (origine et changement notés)
    pub fn vider_registre(&self, chemin: &str, nom: &str) -> Result<(), String> {
        self.exiger_geste()?;
        let st = self.registre_etat(chemin, nom);
        if !vrai(&st["Existed"]) {
            return Ok(());
        }
        self.noter_origine(chemin, nom)?;
        self.noter_changement(json!({ "Type": "reg", "Path": chemin, "Name": nom, "Existed": true, "Value": st["Value"], "Kind": st["Kind"], "Efface": true }));
        self.effacer_registre(chemin, nom)
    }

    // -----------------------------------------------------------------------------------------------------------
    // Les points ignorés (ignores.json)
    // -----------------------------------------------------------------------------------------------------------

    /// `Import-Ignored` / `$script:Ignored` : les points que l'utilisateur a choisi d'ignorer
    pub fn ignores(&self) -> Vec<String> {
        if let Some(l) = verrou(&self.ignores).clone() {
            return l;
        }
        let l = match self.lire_json(&self.donnees.join("ignores.json")) {
            Some(Value::Array(a)) => a.iter().filter(|x| vrai(x)).map(texte_ps).collect(),
            Some(v) if vrai(&v) => vec![texte_ps(&v)],
            _ => Vec::new(),
        };
        *verrou(&self.ignores) = Some(l.clone());
        l
    }

    /// `Save-Ignored` : enregistre la liste des points ignorés
    pub fn enregistrer_ignores(&self, l: Vec<String>) -> Result<(), String> {
        *verrou(&self.ignores) = Some(l.clone());
        self.ecrire_json(&self.donnees.join("ignores.json"), &json!(l), false, false)
    }
}

/// Le texte de `Get-TexteProblemeSauvegarde` pour ce problème
pub fn texte_probleme(probleme: Option<&str>) -> String {
    match probleme {
        None | Some("") => String::new(),
        Some(p) => format!(
            "{} : certains réglages changés par Nevermind ne pourront peut-être pas être remis comme avant. Pour tout remettre, utilise le point de restauration de Windows créé par Nevermind (page Sauvegarde, « Restauration du système »).",
            if p == "abimee" {
                "Le fichier qui garde les réglages d'origine est abîmé"
            } else {
                "Le fichier qui garde les réglages d'origine a disparu"
            }
        ),
    }
}
