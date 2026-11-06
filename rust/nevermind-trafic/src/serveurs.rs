//! À qui appartient une adresse : l'annuaire gardé dans serveurs.json (`Get-ServerCache`, `Get-ServerOwner`,
//! `Get-OwnerLabel`), commun à la page Trafic et à Lag en ligne ; et la lecture des réponses de l'annuaire public
//! RDAP (rdap.org) et d'ipinfo.io. Ces deux annuaires ne sont consultés que si l'utilisateur l'a permis (réglages
//! « TrafficLookup » et « LagPays », coupés par défaut) ; seule l'adresse du serveur est envoyée.

use crate::format::pays;
use crate::ip;
use crate::valeur::{champ, texte};
use regex::Regex;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Une plage d'adresses et son propriétaire (S et E : début et fin en hexadécimal)
#[derive(Clone, Debug, PartialEq)]
pub struct Entree {
    pub s: String,
    pub e: String,
    pub o: String,
    pub c: String,
    pub n: String,
    pub d: String,
}

impl Entree {
    pub fn json(&self) -> Value {
        json!({ "S": self.s, "E": self.e, "O": self.o, "C": self.c, "N": self.n, "D": self.d })
    }
    /// « Valve Corporation (États-Unis) »
    pub fn libelle(&self) -> String {
        libelle(&self.o, &self.n, &self.c)
    }
}

/// `Get-OwnerLabel`
pub fn libelle(o: &str, n: &str, c: &str) -> String {
    let nom = if o.is_empty() { n } else { o };
    let p = pays(c);
    if p.is_empty() { nom.to_string() } else { format!("{nom} ({p})") }
}

/// L'annuaire gardé (serveurs.json), et les adresses déjà cherchées
#[derive(Default)]
pub struct Annuaire {
    pub entrees: Vec<Entree>,
    /// Propriétaire trouvé pour une adresse (index dans `entrees`)
    pub trouves: HashMap<String, usize>,
    /// Adresse sans propriétaire : taille de l'annuaire au moment de la recherche
    pub manques: HashMap<String, usize>,
    pub charge: bool,
}

/// Le nombre de jours entre deux dates « yyyy-MM-dd »
fn age_jours(d: &str, aujourd_hui: chrono::NaiveDate) -> f64 {
    match chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
        Ok(x) => (aujourd_hui - x).num_days() as f64,
        Err(_) => 999.0,
    }
}

impl Annuaire {
    /// Lit l'annuaire (les entrées de plus de 60 jours, ou sans nom, sont oubliées)
    pub fn depuis(v: &Value, aujourd_hui: chrono::NaiveDate) -> Annuaire {
        let mut a = Annuaire { charge: true, ..Default::default() };
        for e in crate::valeur::liste(v) {
            let (s, o, d) = (texte(champ(&e, "S")), texte(champ(&e, "O")), texte(champ(&e, "D")));
            if !s.is_empty() && !o.is_empty() && age_jours(&d, aujourd_hui) < 60.0 {
                a.entrees.push(Entree {
                    s,
                    e: texte(champ(&e, "E")),
                    o,
                    c: texte(champ(&e, "C")),
                    n: texte(champ(&e, "N")),
                    d,
                });
            }
        }
        a
    }

    /// Ajoute une entrée (au plus 3000 gardées, les plus anciennes partent)
    pub fn ajouter(&mut self, e: Entree) {
        self.entrees.push(e);
        // (les adresses sans propriétaire seront recherchées de nouveau : la taille a changé)
    }

    /// Pour serveurs.json
    pub fn json(&mut self) -> Value {
        while self.entrees.len() > 3000 {
            self.entrees.remove(0);
            self.trouves.clear();
        }
        Value::Array(self.entrees.iter().map(Entree::json).collect())
    }

    /// Le propriétaire d'une adresse (`Get-ServerOwner`), `None` si inconnu
    pub fn proprietaire(&mut self, adresse: &str) -> Option<&Entree> {
        if let Some(&i) = self.trouves.get(adresse) {
            return self.entrees.get(i);
        }
        if self.manques.get(adresse) == Some(&self.entrees.len()) {
            return None;
        }
        let h = ip::hex(adresse);
        if !h.is_empty() {
            for (i, e) in self.entrees.iter().enumerate() {
                if e.s.len() == h.len() && e.s.as_str() <= h.as_str() && h.as_str() <= e.e.as_str() {
                    self.trouves.insert(adresse.to_string(), i);
                    return self.entrees.get(i);
                }
            }
        }
        self.manques.insert(adresse.to_string(), self.entrees.len());
        None
    }
}

/// Ce que l'annuaire RDAP ou ipinfo a répondu pour une adresse
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reponse {
    pub ip: String,
    pub ptr: String,
    pub ok: bool,
    pub s: String,
    pub e: String,
    pub o: String,
    pub c: String,
    pub n: String,
}

static ID_TECHNIQUE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Z0-9]+(-[A-Z0-9]+)+$").expect("motif"));
static MNT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)-MNT$|^ORG-").expect("motif"));
static LIR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^local internet registry\s+").expect("motif"));
static ROLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s+(abuse|noc|contact|team|role)\b.*$").expect("motif"));

/// Les propriétés d'une vCard RDAP : (fn, kind, adresse)
fn vcard(en: &Value) -> (String, String, String) {
    let (mut fnom, mut kind, mut adr) = (String::new(), String::new(), String::new());
    let props = champ(en, "vcardArray").get(1).cloned().unwrap_or(Value::Null);
    for p in crate::valeur::liste(&props) {
        let nom = texte(p.get(0).unwrap_or(&Value::Null));
        match nom.as_str() {
            "fn" => fnom = texte(p.get(3).unwrap_or(&Value::Null)),
            "kind" => kind = texte(p.get(3).unwrap_or(&Value::Null)),
            "adr" => {
                let l = texte(champ(p.get(1).unwrap_or(&Value::Null), "label"));
                if !l.is_empty() {
                    adr = l;
                }
            }
            _ => {}
        }
    }
    (fnom, kind, adr)
}

/// Lit la réponse de rdap.org (`$ServerLookupWork`, trafic.ps1)
pub fn lire_rdap(adresse: &str, o: &Value) -> Reponse {
    let mut r = Reponse { ip: adresse.into(), ..Default::default() };
    r.n = texte(champ(o, "name"));
    r.c = texte(champ(o, "country")).to_uppercase();
    r.s = ip::hex(&texte(champ(o, "startAddress")));
    r.e = ip::hex(&texte(champ(o, "endAddress")));
    let entites = crate::valeur::liste(champ(o, "entities"));
    let (mut meilleur, mut adr) = (String::new(), String::new());
    for en in &entites {
        let roles = crate::valeur::liste(champ(en, "roles"));
        if !roles.iter().any(|x| texte(x) == "registrant") {
            continue;
        }
        let (f, kind, lab) = vcard(en);
        if f.is_empty() || MNT.is_match(&f) || ID_TECHNIQUE.is_match(&f) {
            continue;
        }
        if kind == "org" || meilleur.is_empty() {
            meilleur = f;
            adr = lab;
        }
        if kind == "org" {
            break;
        }
    }
    if meilleur.is_empty() {
        for en in &entites {
            let (f, kind, _) = vcard(en);
            if (kind != "org" && kind != "group") || f.is_empty() || ID_TECHNIQUE.is_match(&f) {
                continue;
            }
            let f = ROLE.replace(&LIR.replace(&f, ""), "").trim().to_string();
            if f.chars().count() >= 3 {
                meilleur = f;
                break;
            }
        }
    }
    r.o = meilleur;
    if r.c.is_empty() && !adr.is_empty() {
        r.c = adr.split('\n').next_back().unwrap_or("").trim().to_string();
    }
    r.ok = true;
    r
}

/// Où se trouve un serveur d'après ipinfo.io : (pays, ville, nom)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lieu {
    pub pays: String,
    pub ville: String,
    pub hote: String,
}

/// Lit la réponse d'ipinfo.io (`$ServerGeoWork`, lag.ps1)
pub fn lire_ipinfo(o: &Value) -> Lieu {
    if crate::valeur::vrai(champ(o, "bogon")) || !crate::valeur::vrai(champ(o, "country")) {
        return Lieu::default();
    }
    Lieu {
        pays: texte(champ(o, "country")).to_uppercase(),
        ville: texte(champ(o, "city")),
        hote: texte(champ(o, "hostname")),
    }
}

static PARTICULIER_PROPRIO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(^|[^a-z])(a?dsl|xdsl|ftth|fttx|bas|ppp|pppoe|dyn|dynamic|dial|dialup|residential|broadband|cable|subscribers?|abonnes)([^a-z]|$)|proxad|wanadoo").expect("motif")
});
static PARTICULIER_HOTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(^|[.-])(abo|subs|fbx|dyn|dynamic|dsl|adsl|xdsl|ftth|fttx|cable|cpe|pool|ppp|pppoe|dhcp|customer|residential|broadband)([.-]|\d|$)|\.(wanadoo|proxad|bbox|numericable|t-ipconnect)\.").expect("motif")
});

/// Adresse d'un particulier (la box d'un autre joueur, en pair à pair) et pas un serveur, d'après son propriétaire
/// ou son nom (`Test-AdresseParticulier`) ; `None` tant que ni l'un ni l'autre n'est connu
pub fn particulier(proprio: Option<(&str, &str)>, hote: Option<&str>) -> Option<bool> {
    let mut connu = false;
    if let Some((o, n)) = proprio {
        connu = true;
        if PARTICULIER_PROPRIO.is_match(&format!("{o} {n}")) {
            return Some(true);
        }
    }
    if let Some(h) = hote
        && !h.is_empty()
    {
        connu = true;
        if PARTICULIER_HOTE.is_match(h) {
            return Some(true);
        }
    }
    if connu { Some(false) } else { None }
}
