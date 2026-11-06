//! Le thème de Nevermind (themes.ps1) : le réglage « Theme » (avec les anciens noms), celui de la suite pour une
//! copie gérée, le pack choisi ; les couleurs viennent de suite-commun (les mêmes thèmes intégrés et la même
//! traduction des couleurs que les applis PowerShell), les images d'un pack de [`crate::pack`].
//!
//! Et ce que vue.ps1 ajoute à chaque état (`Send-Vue`) : `theme` (`Get-ThemeVue`), `couleurs` (`Get-CouleursVue`),
//! `pack` (`Get-PackVue`) ; la connexion est ajoutée par l'hôte (elle change pendant que Nevermind tourne).

use std::path::Path;

use serde_json::{Map, Value, json};
use suite_commun::themes::{self, Theme};

use crate::vie::demarrage::Suite;

/// Le thème choisi au démarrage.
#[derive(Debug, Clone)]
pub struct Choix {
    /// `$ThemeSetting` (après la suite) : « neon », « pack:<id> »...
    pub reglage: String,
    /// Le thème chargé (couleurs, pack)
    pub theme: Theme,
    /// `Get-CurrentThemeKey` (parametres.ps1:317) : « pack:<id> » si le pack est là, sinon le thème de couleurs
    pub cle: String,
    /// Le réglage à écrire (`Set-Setting 'Theme'`), quand le thème de la suite est pris (themes.ps1:165)
    pub a_ecrire: Option<String>,
    /// Les lignes du journal
    pub journal: Vec<String>,
}

/// Les anciens noms de thèmes (themes.ps1:32, 1.0.63).
fn nom_actuel(t: &str) -> &str {
    match t {
        "retro" => "arcade",
        "dresseur" => "rubis",
        _ => t,
    }
}

/// Le thème au démarrage (themes.ps1:30-176). `reglage` : `Get-Setting 'Theme' 'neon'` ; `packs` : `$PacksDir`.
pub fn choisir(reglage: &str, gere: bool, suite: &Suite, packs: &Path) -> Choix {
    let mut journal = Vec::new();
    let mut reglage = reglage.to_string();
    let mut a_ecrire = None;
    let dossiers = [packs.to_path_buf()];
    // themes.ps1:160-169 : géré, le thème de la suite (s'il est là)
    if gere
        && let Some(t) = suite.theme.as_deref()
        && reglage != t
    {
        let absent = t.strip_prefix("pack:").is_some_and(|id| !crate::vie::surveillance::pack_present(packs, id));
        if absent {
            journal.push(format!(
                "Thème de la suite (Mindset) : {t} absent ou illisible dans {}, thème actuel gardé ({reglage})",
                packs.display()
            ));
        } else {
            reglage = t.to_string();
            a_ecrire = Some(reglage.clone());
            journal.push(format!("Thème de la suite (Mindset) : {reglage}"));
        }
    }
    let demande = nom_actuel(&reglage).to_string();
    let theme = if demande.starts_with("pack:") || themes::theme_integre(&demande).is_some() {
        themes::charger(Some(&demande), &dossiers)
    } else {
        // (un nom inconnu : Néon, comme themes.ps1:33)
        Theme::neon()
    };
    for r in &theme.raisons {
        journal.push(format!("Thème : {r}"));
    }
    let cle = match &theme.pack {
        Some(p) => format!("pack:{}", p.id),
        None => themes::theme_integre(&theme.id).map_or(themes::NEON, |t| t.id).to_string(),
    };
    Choix { reglage, theme, cle, a_ecrire, journal }
}

/// `Get-ThemeText 'Hello'` (themes.ps1:395) : la phrase d'un pack, sinon celle du thème.
fn bonjour(theme: &Theme) -> String {
    if let Some(p) = &theme.pack
        && !p.bonjour.is_empty()
    {
        return p.bonjour.clone();
    }
    if theme.id == "terminal" { "> salut {0}_".to_string() } else { "Salut {0}".to_string() }
}

/// `$texte -f $valeur` (String.Format de .NET) pour un seul argument : `{0}`, `{{` et `}}`. Toute autre accolade
/// est une erreur de .NET (`FormatException`) : `None`.
pub fn formater(modele: &str, valeur: &str) -> Option<String> {
    let mut s = String::new();
    let mut c = modele.chars().peekable();
    while let Some(x) = c.next() {
        match x {
            '{' if c.peek() == Some(&'{') => {
                c.next();
                s.push('{');
            }
            '}' if c.peek() == Some(&'}') => {
                c.next();
                s.push('}');
            }
            '{' => {
                let mut dedans = String::new();
                loop {
                    match c.next() {
                        Some('}') => break,
                        Some(y) => dedans.push(y),
                        None => return None,
                    }
                }
                if dedans.trim_end() != "0" {
                    return None;
                }
                s.push_str(valeur);
            }
            '}' => return None,
            _ => s.push(x),
        }
    }
    Some(s)
}

/// La phrase d'accueil (`app.phrase`, accueil-console.ps1:37) : le prénom (en minuscules pour Terminal), vide si la
/// phrase du pack est mal écrite.
pub fn phrase(theme: &Theme, prenom: &str) -> String {
    let p = if theme.id == "terminal" { prenom.to_lowercase() } else { prenom.to_string() };
    formater(&bonjour(theme), &p).unwrap_or_default()
}

/// Le prénom pour la phrase (`Get-FirstName`, navigation.ps1:15-26) : le premier mot du nom complet du compte, sinon le
/// nom d'utilisateur, avec une majuscule.
pub fn prenom(nom_complet: Option<&str>, utilisateur: &str) -> String {
    let n = nom_complet.map(str::trim).filter(|n| !n.is_empty()).map(|n| n.split(' ').next().unwrap_or("").to_string());
    let n = n.filter(|n| !n.is_empty()).unwrap_or_else(|| utilisateur.to_string());
    let mut c = n.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Une couleur que la page accepte : « #RRGGBB ».
pub fn couleur_valable(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Ce que `Send-Vue` ajoute de lui-même à l'état : le thème, les couleurs d'un pack, ses images.
#[derive(Debug, Clone, PartialEq)]
pub struct Habillage {
    pub theme: &'static str,
    pub couleurs: Option<Map<String, Value>>,
    pub pack: Option<Value>,
}

impl Habillage {
    pub fn depuis(theme: &Theme) -> Habillage {
        let couleurs = theme.couleurs_ecran().map(|v| {
            let mut m = Map::new();
            for (n, c) in v {
                if couleur_valable(&c) {
                    m.insert(n.to_string(), json!(c));
                }
            }
            m
        });
        Habillage { theme: theme.theme_ecran(), couleurs, pack: theme.pack.as_ref().map(crate::pack::pack_vue) }
    }

    /// Ajoute à l'état ce qui y manque (comme `Send-Vue`).
    pub fn poser(&self, etat: &mut Map<String, Value>) {
        etat.entry("theme").or_insert_with(|| json!(self.theme));
        if let Some(c) = &self.couleurs {
            etat.entry("couleurs").or_insert_with(|| Value::Object(c.clone()));
        }
        if let Some(p) = &self.pack {
            etat.entry("pack").or_insert_with(|| p.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_comme_dotnet() {
        assert_eq!(formater("Salut {0}", "Léa").as_deref(), Some("Salut Léa"));
        assert_eq!(formater("> salut {0}_", "léa").as_deref(), Some("> salut léa_"));
        assert_eq!(formater("{{x}} {0}", "a").as_deref(), Some("{x} a"));
        assert_eq!(formater("Bonjour", "a").as_deref(), Some("Bonjour"));
        for faux in ["{1}", "{", "}", "{x}", "a {0} {1}"] {
            assert_eq!(formater(faux, "a"), None, "{faux}");
        }
    }

    #[test]
    fn prenom_comme_get_firstname() {
        assert_eq!(prenom(Some("jordan Jacquot"), "jj"), "Jordan");
        assert_eq!(prenom(Some("  "), "élodie"), "Élodie");
        assert_eq!(prenom(None, "pc"), "Pc");
        assert_eq!(prenom(None, ""), "");
    }

    #[test]
    fn anciens_noms_et_inconnus() {
        let d = std::env::temp_dir();
        for (r, cle) in
            [("retro", "arcade"), ("dresseur", "rubis"), ("terminal", "terminal"), ("bidule", "neon"), ("", "neon")]
        {
            assert_eq!(choisir(r, false, &Suite::default(), &d).cle, cle, "{r}");
        }
        // Géré : le thème de la suite l'emporte, et s'écrit
        let s = Suite { theme: Some("crepuscule".into()), dossier: String::new() };
        let c = choisir("neon", true, &s, &d);
        assert_eq!((c.cle.as_str(), c.a_ecrire.as_deref()), ("crepuscule", Some("crepuscule")));
        // Pas géré : la suite ne compte pas
        assert_eq!(choisir("neon", false, &s, &d).cle, "neon");
        // Un pack de la suite absent : le thème actuel reste, noté
        let s = Suite { theme: Some("pack:absent-ici".into()), dossier: String::new() };
        let c = choisir("rubis", true, &s, &d.join("nevermind-aucun-pack"));
        assert_eq!((c.cle.as_str(), c.a_ecrire.is_none()), ("rubis", true));
        assert!(c.journal[0].contains("absent ou illisible"));
    }

    #[test]
    fn phrase_du_terminal_en_minuscules() {
        let t = themes::charger(Some("terminal"), &[]);
        assert_eq!(phrase(&t, "Jordan"), "> salut jordan_");
        assert_eq!(phrase(&Theme::neon(), "Jordan"), "Salut Jordan");
    }
}
