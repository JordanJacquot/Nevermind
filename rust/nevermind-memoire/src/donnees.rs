//! Ce que la lecture du PC donne au constat (`Get-DonneesMemoire`), champ pour champ comme la version PowerShell.
//! Lu depuis le JSON des juges (mêmes noms de champs que la table de hachage de PowerShell) ou rempli par la lecture
//! native (module `windows`).

use crate::texte::Date;
use serde_json::{Map, Value, json};

/// Un processus (`Win32_Process`, plus ce que Windows dit de lui pour la règle « fermable »)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Processus {
    pub pid: i64,
    pub parent: i64,
    pub nom: String,
    pub chemin: String,
    pub ligne: String,
    pub octets: f64,
    pub produit: String,
    pub description: String,
    pub societe: String,
    /// Le chemin que Windows donne sur sa poignée (`Read-AideFiche`) ; `None` : pas de fiche
    pub image: Option<String>,
    pub session: Option<i64>,
    pub compte: String,
    pub protection: Option<i64>,
    pub eleve: Option<i64>,
    /// L'heure de démarrage lue sur la poignée gardée ; 0 : pas gardée (pas vérifiable)
    pub debut: i64,
}

impl Processus {
    /// `if ($p.Image) { $p.Image } else { $p.Chemin }`
    pub fn image_ou_chemin(&self) -> &str {
        match &self.image {
            Some(i) if !i.is_empty() => i,
            _ => &self.chemin,
        }
    }
}

/// Un service (`Win32_Service`)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Service {
    pub nom: String,
    pub titre: String,
    pub pid: i64,
    /// « Auto », « Manual », « Disabled »... (vide : pas lu)
    pub demarrage: String,
    pub etat: String,
}

/// Un pilote qui porte une étiquette de la mémoire des pilotes
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pilote {
    pub fichier: String,
    pub description: String,
    pub societe: String,
}

/// Une étiquette de la mémoire des pilotes (4 lettres), sa taille, ses pilotes probables
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Etiquette {
    pub tag: String,
    pub octets: f64,
    pub pilotes: Vec<Pilote>,
}

/// Celui pour qui on ferme (Nevermind) : `Get-AideReference`
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Moi {
    pub id: i64,
    pub session: Option<i64>,
    pub compte: String,
    pub eleve: Option<i64>,
}

/// Ce que Nevermind sait des jeux (`Get-JeuxMemoire`)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Jeux {
    /// Les programmes des jeux de la bibliothèque (chemins en minuscules)
    pub exes: Vec<String>,
    /// Les jeux appris par la mesure des FPS (noms sans .exe, en minuscules)
    pub noms: Vec<String>,
    /// Le jeu en cours (numéro de processus), 0 sinon
    pub en_cours: i64,
}

/// Toutes les données d'une lecture
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Donnees {
    pub pc: String,
    pub date: Option<Date>,
    pub windir: String,
    /// Program Files (pour reconnaître la suite Mindset dans la règle « fermable »)
    pub programmes: String,
    pub installee: f64,
    pub visible: f64,
    pub disponible: f64,
    pub cache: f64,
    pub modifiee: f64,
    pub pool_np: f64,
    pub pool_p: f64,
    pub processus: Vec<Processus>,
    pub services: Vec<Service>,
    pub demarrage: Vec<String>,
    pub etiquettes: Vec<Etiquette>,
    pub moi: Option<Moi>,
    pub antivirus: Vec<String>,
    pub jeux: Option<Jeux>,
}

// ---------------------------------------------------------------------------
// Lecture depuis le JSON de PowerShell (ConvertTo-Json d'une table de hachage)
// ---------------------------------------------------------------------------

fn texte(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => if *b { "True" } else { "False" }.into(),
        _ => String::new(),
    }
}

fn nombre(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        _ => 0.0,
    }
}

/// `[int]` ou `[long]` de PowerShell (arrondi au pair), 0 si absent
fn entier(v: Option<&Value>) -> i64 {
    crate::texte::arrondi_pair(nombre(v))
}

/// `ConvertTo-PontNombre` : `None` pour absent, null ou texte vide
fn entier_ou_rien(v: Option<&Value>) -> Option<i64> {
    match v {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => s.trim().parse::<f64>().ok().map(crate::texte::arrondi_pair),
        Some(Value::Bool(b)) => Some(i64::from(*b)),
        Some(x) => Some(entier(Some(x))),
    }
}

fn liste(v: Option<&Value>) -> Vec<&Value> {
    match v {
        Some(Value::Array(a)) => a.iter().filter(|x| !x.is_null()).collect(),
        Some(Value::Null) | None => Vec::new(),
        Some(x) => vec![x],
    }
}

fn textes(v: Option<&Value>) -> Vec<String> {
    liste(v).into_iter().map(|x| texte(Some(x))).collect()
}

impl Donnees {
    /// Les données telles que PowerShell les écrit en JSON
    pub fn depuis_json(v: &Value) -> Donnees {
        let o = v.as_object().cloned().unwrap_or_default();
        let g = |k: &str| o.get(k);
        let processus = liste(g("Processus"))
            .into_iter()
            .filter_map(Value::as_object)
            .map(|p| {
                let c = |k: &str| p.get(k);
                Processus {
                    pid: entier(c("Pid")),
                    parent: entier(c("Parent")),
                    nom: texte(c("Nom")),
                    chemin: texte(c("Chemin")),
                    ligne: texte(c("Ligne")),
                    octets: nombre(c("Octets")),
                    produit: texte(c("Produit")),
                    description: texte(c("Description")),
                    societe: texte(c("Societe")),
                    image: c("Image").filter(|x| !x.is_null()).map(|x| texte(Some(x))),
                    session: entier_ou_rien(c("Session")),
                    compte: texte(c("Compte")),
                    protection: entier_ou_rien(c("Protection")),
                    eleve: entier_ou_rien(c("Eleve")),
                    debut: entier(c("Debut")),
                }
            })
            .collect();
        let services = liste(g("Services"))
            .into_iter()
            .filter_map(Value::as_object)
            .map(|s| Service {
                nom: texte(s.get("Nom")),
                titre: texte(s.get("Titre")),
                pid: entier(s.get("Pid")),
                demarrage: texte(s.get("Demarrage")),
                etat: texte(s.get("Etat")),
            })
            .collect();
        let etiquettes = liste(g("Etiquettes"))
            .into_iter()
            .filter_map(Value::as_object)
            .map(|e| Etiquette {
                tag: texte(e.get("Tag")),
                octets: nombre(e.get("Octets")),
                pilotes: liste(e.get("Pilotes")).into_iter().filter_map(Value::as_object).map(pilote_json).collect(),
            })
            .collect();
        let moi = g("Moi").and_then(Value::as_object).map(|m| Moi {
            id: entier(m.get("Id")),
            session: entier_ou_rien(m.get("Session")),
            compte: texte(m.get("Compte")),
            eleve: entier_ou_rien(m.get("Eleve")),
        });
        let jeux = g("Jeux").and_then(Value::as_object).map(|j| Jeux {
            exes: textes(j.get("Exes")),
            noms: textes(j.get("Noms")),
            en_cours: entier(j.get("EnCours")),
        });
        Donnees {
            pc: texte(g("Pc")),
            date: g("Date").and_then(Value::as_str).and_then(Date::lire),
            windir: texte(g("Windir")),
            programmes: texte(g("Programmes")),
            installee: nombre(g("Installee")),
            visible: nombre(g("Visible")),
            disponible: nombre(g("Disponible")),
            cache: nombre(g("Cache")),
            modifiee: nombre(g("Modifiee")),
            pool_np: nombre(g("PoolNP")),
            pool_p: nombre(g("PoolP")),
            processus,
            services,
            demarrage: textes(g("Demarrage")).into_iter().filter(|x| !x.is_empty()).collect(),
            etiquettes,
            moi,
            antivirus: textes(g("Antivirus")),
            jeux,
        }
    }

    /// Les mêmes données en JSON (mêmes champs que PowerShell) : pour comparer avec la version PowerShell
    pub fn en_json(&self) -> Value {
        let mut o = Map::new();
        o.insert("Pc".into(), json!(self.pc));
        o.insert("Date".into(), json!(self.date.map(|d| d.texte())));
        o.insert("Windir".into(), json!(self.windir));
        o.insert("Programmes".into(), json!(self.programmes));
        for (k, v) in [
            ("Installee", self.installee),
            ("Visible", self.visible),
            ("Disponible", self.disponible),
            ("Cache", self.cache),
            ("Modifiee", self.modifiee),
            ("PoolNP", self.pool_np),
            ("PoolP", self.pool_p),
        ] {
            o.insert(k.into(), json!(v));
        }
        o.insert(
            "Processus".into(),
            Value::Array(
                self.processus
                    .iter()
                    .map(|p| {
                        json!({ "Pid": p.pid, "Parent": p.parent, "Nom": p.nom, "Chemin": p.chemin, "Ligne": p.ligne,
                            "Octets": p.octets, "Produit": p.produit, "Description": p.description, "Societe": p.societe,
                            "Image": p.image, "Session": p.session, "Compte": p.compte, "Protection": p.protection,
                            "Eleve": p.eleve, "Debut": p.debut })
                    })
                    .collect(),
            ),
        );
        o.insert(
            "Services".into(),
            Value::Array(
                self.services
                    .iter()
                    .map(|s| json!({ "Nom": s.nom, "Titre": s.titre, "Pid": s.pid, "Demarrage": s.demarrage, "Etat": s.etat }))
                    .collect(),
            ),
        );
        o.insert("Demarrage".into(), json!(self.demarrage));
        o.insert(
            "Etiquettes".into(),
            Value::Array(
                self.etiquettes
                    .iter()
                    .map(|e| json!({ "Tag": e.tag, "Octets": e.octets, "Pilotes": e.pilotes.iter().map(pilote_en_json).collect::<Vec<_>>() }))
                    .collect(),
            ),
        );
        if let Some(m) = &self.moi {
            o.insert("Moi".into(), json!({ "Id": m.id, "Session": m.session, "Compte": m.compte, "Eleve": m.eleve }));
        }
        o.insert("Antivirus".into(), json!(self.antivirus));
        if let Some(j) = &self.jeux {
            o.insert("Jeux".into(), json!({ "Exes": j.exes, "Noms": j.noms, "EnCours": j.en_cours }));
        }
        Value::Object(o)
    }
}

pub fn pilote_json(p: &Map<String, Value>) -> Pilote {
    Pilote {
        fichier: texte(p.get("Fichier")),
        description: texte(p.get("Description")),
        societe: texte(p.get("Societe")),
    }
}

pub fn pilote_en_json(p: &Pilote) -> Value {
    json!({ "Fichier": p.fichier, "Description": p.description, "Societe": p.societe })
}
