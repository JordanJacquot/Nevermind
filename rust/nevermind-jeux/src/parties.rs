//! Ce que le bloc lit des autres parties de Nevermind, sur le disque comme la version PowerShell : le temps de jeu
//! (jeux.json, Get-PlayLog et Add-PlayTime), les parties mesurées (fps.json, Get-FpsSessions), les mesures de connexion
//! (lag.json, Get-LagSessions) et la date du dernier changement (historique.json, Get-LastChangeDate).

use crate::json::{lire_json_sur, nombre, texte, texte_de, vrai};
use crate::texte::{Moment, lire_date, n0};
use nevermind_noyau::Contexte;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::Path;

/// Le temps de jeu d'un jeu.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TempsJeu {
    pub last: String,
    pub secondes: f64,
    pub parties: i64,
}

/// Le journal des parties (jeux.json) : par nom de jeu, sans tenir compte de la casse (comme une table PowerShell).
#[derive(Debug, Clone, Default)]
pub struct JournalParties {
    /// Clé : nom en minuscules ; valeur : (nom d'origine, temps).
    pub jeux: Vec<(String, TempsJeu)>,
}

impl JournalParties {
    pub fn lire(ctx: Option<&Contexte>, donnees: &Path) -> Self {
        let mut j = JournalParties::default();
        if let (Some(Value::Object(o)), _) = lire_json_sur(ctx, &donnees.join("jeux.json")) {
            for (k, v) in o {
                let t = TempsJeu {
                    last: texte(&v, "Last"),
                    secondes: nombre(&v, "Seconds"),
                    parties: crate::texte::entier_ps(nombre(&v, "Count")),
                };
                j.mettre(&k, t);
            }
        }
        j
    }
    pub fn get(&self, nom: &str) -> Option<&TempsJeu> {
        let b = nom.to_lowercase();
        self.jeux.iter().find(|(k, _)| k.to_lowercase() == b).map(|x| &x.1)
    }
    pub fn contient(&self, nom: &str) -> bool {
        self.get(nom).is_some()
    }
    fn mettre(&mut self, nom: &str, t: TempsJeu) {
        let b = nom.to_lowercase();
        match self.jeux.iter_mut().find(|(k, _)| k.to_lowercase() == b) {
            Some(x) => x.1 = t,
            None => self.jeux.push((nom.to_string(), t)),
        }
    }
    /// Une partie finie (Add-PlayTime) : durée ajoutée, une partie de plus, date de la dernière ; écrit tout de suite.
    pub fn ajouter(&mut self, ctx: Option<&Contexte>, donnees: &Path, jeu: &str, debut: Moment, maintenant: Moment) {
        if jeu.is_empty() {
            return;
        }
        let mut e = self.get(jeu).cloned().unwrap_or_default();
        e.secondes += ((maintenant.0 - debut.0) as f64).max(0.0);
        e.parties += 1;
        e.last = maintenant.iso();
        self.mettre(jeu, e);
        let mut m = Map::new();
        for (k, t) in &self.jeux {
            let mut x = Map::new();
            x.insert("Last".into(), Value::String(t.last.clone()));
            x.insert("Seconds".into(), serde_json::json!(t.secondes));
            x.insert("Count".into(), serde_json::json!(t.parties));
            m.insert(k.clone(), Value::Object(x));
        }
        if let Err(e) = crate::json::ecrire_json_sur(&donnees.join("jeux.json"), &Value::Object(m), true)
            && let Some(c) = ctx
        {
            c.journal(&format!("Temps de jeu: {e}"));
        }
    }
}

/// Une partie mesurée (une ligne de fps.json).
#[derive(Debug, Clone, Default)]
pub struct PartieFps {
    pub id: String,
    pub date: String,
    pub moment: Option<Moment>,
    pub jeu: String,
    pub cle: String,
    pub avg: f64,
    pub low1: f64,
    pub secondes: f64,
    pub saccades: Option<f64>,
    /// La ligne entière de fps.json (courbe, 0,1 % bas, menus...).
    pub brut: Value,
}

/// Les parties mesurées, dans l'ordre du fichier (Get-FpsSessions).
pub fn parties_fps(ctx: Option<&Contexte>, donnees: &Path) -> Vec<PartieFps> {
    let (v, _) = lire_json_sur(ctx, &donnees.join("fps.json"));
    let mut l = Vec::new();
    for x in crate::biblio::liste(v.as_ref()) {
        if !vrai(&x) {
            continue;
        }
        let date = texte(&x, "Date");
        let id = texte(&x, "Id");
        let diag = x.get("Diag").filter(|d| vrai(d));
        l.push(PartieFps {
            id: if id.is_empty() { date.clone() } else { id },
            moment: lire_date(&date),
            date,
            jeu: texte(&x, "Game"),
            cle: texte(&x, "Key"),
            avg: nombre(&x, "Avg"),
            low1: nombre(&x, "Low1"),
            secondes: nombre(&x, "Seconds"),
            saccades: diag.map(|d| nombre(d, "Stutters")),
            brut: x.clone(),
        });
    }
    l
}

/// Partie avec un souci de FPS (Test-FpsProblem) : moins de 60 en moyenne, ou des chutes fortes et fréquentes.
pub fn probleme_fps(s: &PartieFps) -> bool {
    let par_min = match s.saccades {
        Some(n) => n / (s.secondes / 60.0).max(1.0),
        None => 0.0,
    };
    s.avg < 60.0 || s.low1 < 0.5 * s.avg || par_min > 6.0
}

/// Moyenne pondérée par la durée (Measure-FpsGroup) : FPS moyen, 1 % bas, secondes.
pub fn moyenne_groupe(l: &[&PartieFps]) -> Option<(f64, f64, f64)> {
    let sec: f64 = l.iter().map(|s| s.secondes).sum();
    if sec == 0.0 {
        return None;
    }
    let (mut a, mut b) = (0.0, 0.0);
    for s in l {
        a += s.avg * s.secondes;
        b += s.low1 * s.secondes;
    }
    Some((a / sec, b / sec, sec))
}

/// La date du dernier changement fait par Nevermind et pas annulé (Get-LastChangeDate).
pub fn dernier_changement(ctx: Option<&Contexte>, donnees: &Path) -> Option<Moment> {
    let (v, _) = lire_json_sur(ctx, &donnees.join("historique.json"));
    for h in crate::biblio::liste(v.as_ref()) {
        if !vrai(&h) || h.get("Annule").map(vrai).unwrap_or(false) {
            continue;
        }
        // « dd/MM/yyyy à HH:mm »
        let d = texte(&h, "Date");
        if let Some(m) = lire_date_historique(&d) {
            return Some(m);
        }
    }
    None
}

fn lire_date_historique(d: &str) -> Option<Moment> {
    static FORME: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"^(\d{2})/(\d{2})/(\d{4}) à (\d{2}):(\d{2})$").unwrap());
    let m = FORME.captures(d)?;
    lire_date(&format!("{}-{}-{}T{}:{}:00", &m[3], &m[2], &m[1], &m[4], &m[5]))
}

/// Une mesure de connexion (une ligne de lag.json).
#[derive(Debug, Clone, Default)]
pub struct MesureLag {
    pub id: String,
    pub date: String,
    pub moment: Option<Moment>,
    pub jeu: String,
    pub secondes: f64,
    pub niveau: String,
    pub titre: String,
    pub brut: Value,
}

pub fn mesures_lag(ctx: Option<&Contexte>, donnees: &Path) -> Vec<MesureLag> {
    let (v, _) = lire_json_sur(ctx, &donnees.join("lag.json"));
    let mut l = Vec::new();
    for x in crate::biblio::liste(v.as_ref()) {
        if !vrai(&x) {
            continue;
        }
        let diag = x.get("Diag").cloned().unwrap_or(Value::Null);
        let date = texte(&x, "Date");
        l.push(MesureLag {
            id: texte(&x, "Id"),
            moment: lire_date(&date),
            date,
            jeu: texte(&x, "Game"),
            secondes: nombre(&x, "Seconds"),
            niveau: texte(&diag, "Level"),
            titre: texte(&diag, "Title"),
            brut: x,
        });
    }
    l
}

fn noms_pays() -> HashMap<&'static str, &'static str> {
    [
        ("US", "États-Unis"),
        ("FR", "France"),
        ("IE", "Irlande"),
        ("DE", "Allemagne"),
        ("NL", "Pays-Bas"),
        ("GB", "Royaume-Uni"),
        ("BE", "Belgique"),
        ("CH", "Suisse"),
        ("ES", "Espagne"),
        ("IT", "Italie"),
        ("SE", "Suède"),
        ("FI", "Finlande"),
        ("PL", "Pologne"),
        ("LU", "Luxembourg"),
        ("AT", "Autriche"),
        ("CA", "Canada"),
        ("JP", "Japon"),
        ("KR", "Corée du Sud"),
        ("CN", "Chine"),
        ("HK", "Hong Kong"),
        ("TW", "Taïwan"),
        ("SG", "Singapour"),
        ("IN", "Inde"),
        ("AU", "Australie"),
        ("BR", "Brésil"),
        ("RU", "Russie"),
        ("UA", "Ukraine"),
        ("IL", "Israël"),
        ("AE", "Émirats arabes unis"),
        ("ZA", "Afrique du Sud"),
        ("EU", "Europe"),
    ]
    .into_iter()
    .collect()
}

/// Le nom d'un pays (Format-Country, trafic.ps1).
pub fn pays(c: &str) -> String {
    if c.is_empty() {
        return String::new();
    }
    if c.chars().count() == 2 {
        let h = c.to_uppercase();
        return noms_pays().get(h.as_str()).map(|s| s.to_string()).unwrap_or(h);
    }
    let b = c.to_lowercase();
    if b.contains("united states") || b == "usa" {
        return "États-Unis".into();
    }
    crate::biblio::titre_mots(&b)
}

/// « 23 ms », « ? » (Format-Ms, lag.ps1).
fn ms(v: &Value) -> String {
    match v {
        Value::Null => "?".into(),
        x => {
            let n = crate::json::nombre_de(x);
            if n < 0.0 { "?".into() } else { format!("{} ms", n0(n)) }
        }
    }
}

/// La ligne d'une mesure (Get-LagSessionLine, lag.ps1).
pub fn ligne_lag(s: &MesureLag) -> String {
    let b = &s.brut;
    let serveur = b.get("Server").filter(|x| vrai(x));
    let etw = b.get("Etw").map(vrai).unwrap_or(false);
    let conn = serveur.map(|sv| texte(sv, "Proto") == "tcp").unwrap_or(false) && !etw;
    let mut p: Vec<String> = Vec::new();
    let stats = b.get("Stats").cloned().unwrap_or(Value::Null);
    for (k, nom) in [("gw", "Box"), ("ref", "Internet"), ("srv", if conn { "Serveur de connexion" } else { "Serveur" })]
    {
        if let Some(x) = stats.get(k).filter(|x| vrai(x))
            && !x.get("Dead").map(vrai).unwrap_or(false)
        {
            p.push(format!("{nom} {}", ms(x.get("Med").unwrap_or(&Value::Null))));
        }
    }
    if let Some(sv) = serveur {
        let code = texte(sv, "Pays");
        if !code.is_empty() {
            let ps = pays(&code);
            let ville = texte_de(sv.get("Ville").unwrap_or(&Value::Null));
            let lieu = if ville.is_empty() { ps } else { format!("{ville}, {ps}") };
            if !lieu.is_empty() {
                p.push(format!("serveur {}situé à {lieu}", if conn { "de connexion " } else { "" }));
            }
        }
    }
    p.join(", ")
}
