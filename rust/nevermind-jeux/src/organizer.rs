//! L'Organizer Dofus (organizer.ps1) : la règle qui le montre ou le cache (Test-OrgVisible, Set-OrgVisible), ses
//! réglages (organizer.json), la lecture du titre des fenêtres, l'ordre d'initiative des persos, le perso suivant ou
//! précédent, et les touches. La barre flottante, la veille des fenêtres et les raccourcis pris à Windows ont besoin de
//! l'hôte (fenêtres, messages de Windows) : voir le rapport.

use crate::json::{lire_json_sur, texte_de, vrai};
use nevermind_noyau::Contexte;
use serde_json::{Map, Value, json};
use std::path::Path;

pub const MAX_TOUCHES: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub on: bool,
    pub ordre: Vec<String>,
    pub barre: bool,
    pub barre_toujours: bool,
    pub barre_x: Option<f64>,
    pub barre_y: Option<f64>,
    /// La classe retenue de chaque perso.
    pub classes: Vec<(String, String)>,
    /// Les touches : Next, Prev, P1 à P8.
    pub touches: Vec<(String, String)>,
}

impl Default for Config {
    fn default() -> Self {
        let mut touches = vec![("Next".to_string(), "Ctrl+Tab".to_string()), ("Prev".into(), "Ctrl+Maj+Tab".into())];
        for i in 1..=MAX_TOUCHES {
            touches.push((format!("P{i}"), format!("F{i}")));
        }
        Config {
            on: false,
            ordre: Vec::new(),
            barre: true,
            barre_toujours: false,
            barre_x: None,
            barre_y: None,
            classes: Vec::new(),
            touches,
        }
    }
}

fn mettre(l: &mut Vec<(String, String)>, k: &str, v: String) {
    match l.iter_mut().find(|(x, _)| x.eq_ignore_ascii_case(k)) {
        Some(e) => e.1 = v,
        None => l.push((k.to_string(), v)),
    }
}

impl Config {
    /// Les réglages (Get-OrgConfig).
    pub fn lire(ctx: Option<&Contexte>, donnees: &Path) -> Self {
        let mut c = Config::default();
        if let (Some(j), _) = lire_json_sur(ctx, &donnees.join("organizer.json"))
            && vrai(&j)
        {
            for (k, champ) in [("On", 0), ("Bar", 1), ("BarAlways", 2)] {
                if let Some(v) = j.get(k).filter(|v| !v.is_null()) {
                    let b = vrai(v);
                    match champ {
                        0 => c.on = b,
                        1 => c.barre = b,
                        _ => c.barre_toujours = b,
                    }
                }
            }
            if let Some(v) = j.get("BarX").filter(|v| !v.is_null()) {
                c.barre_x = Some(crate::json::nombre_de(v));
            }
            if let Some(v) = j.get("BarY").filter(|v| !v.is_null()) {
                c.barre_y = Some(crate::json::nombre_de(v));
            }
            c.ordre = crate::biblio::liste(j.get("Order")).iter().filter(|x| vrai(x)).map(texte_de).collect();
            if let Some(Value::Object(h)) = j.get("Hotkeys") {
                for (k, v) in h {
                    mettre(&mut c.touches, k, texte_de(v));
                }
            }
            if let Some(Value::Object(h)) = j.get("Classes") {
                for (k, v) in h {
                    mettre(&mut c.classes, k, texte_de(v));
                }
            }
        }
        c
    }

    pub fn vers_json(&self) -> Value {
        let mut h = Map::new();
        for (k, v) in &self.touches {
            h.insert(k.clone(), Value::String(v.clone()));
        }
        let mut cl = Map::new();
        for (k, v) in &self.classes {
            cl.insert(k.clone(), Value::String(v.clone()));
        }
        json!({ "On": self.on, "Order": self.ordre, "Bar": self.barre, "BarAlways": self.barre_toujours,
            "BarX": self.barre_x, "BarY": self.barre_y, "Classes": cl, "Hotkeys": h })
    }

    /// Enregistre (Save-OrgConfig).
    pub fn ecrire(&self, ctx: Option<&Contexte>, donnees: &Path) {
        if let Err(e) = crate::json::ecrire_json_sur(&donnees.join("organizer.json"), &self.vers_json(), false)
            && let Some(c) = ctx
        {
            c.journal(&format!("Organizer : enregistrement impossible : {e}"));
        }
    }

    pub fn classe(&self, nom: &str) -> String {
        self.classes.iter().find(|(k, _)| k.eq_ignore_ascii_case(nom)).map(|x| x.1.clone()).unwrap_or_default()
    }
}

/// L'Organizer est-il proposé (Test-OrgVisible) ? Le réglage « OrganizerAffiche » de Paramètres, Jeux ; sans réglage :
/// seulement s'il est déjà allumé ou si Dofus est dans la bibliothèque.
pub fn visible(reglage: Option<&Value>, config_on: bool, noms_jeux: &[&str]) -> bool {
    static DOFUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)\bdofus\b").unwrap());
    if let Some(r) = reglage.filter(|r| !r.is_null()) {
        return vrai(r);
    }
    if config_on {
        return true;
    }
    noms_jeux.iter().any(|n| DOFUS.is_match(n))
}

/// Le nom du perso et sa classe dans le titre d'une fenêtre Dofus (ConvertFrom-DofusTitle).
/// Dofus 3 : « Perso - Classe - 3.x - Release » ; Dofus 2 : « Perso - Dofus 2.x » ; connexion : « Dofus ».
pub fn lire_titre(titre: &str) -> (String, String) {
    static DOFUS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)^Dofus\b").unwrap());
    static CLASSE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)^(Dofus|\d)").unwrap());
    let parts: Vec<String> = titre.split(" - ").map(|s| s.trim().to_string()).collect();
    if parts.len() < 2 || DOFUS.is_match(&parts[0]) || parts[0].is_empty() {
        return (String::new(), String::new());
    }
    let cls = if !CLASSE.is_match(&parts[1]) { parts[1].clone() } else { String::new() };
    (parts[0].clone(), cls)
}

/// Une fenêtre Dofus ouverte.
#[derive(Debug, Clone, PartialEq)]
pub struct Fenetre {
    pub nom: String,
    pub classe: String,
    pub pid: u32,
    pub hwnd: isize,
}

/// Un perso de la liste.
#[derive(Debug, Clone, PartialEq)]
pub struct Perso {
    pub nom: String,
    pub classe: String,
    pub hwnd: isize,
    pub pid: u32,
    pub en_ligne: bool,
}

/// La liste dans l'ordre d'initiative (Get-OrgList) : persos connus, puis nouveaux, puis fenêtres à la connexion.
/// Renvoie aussi si la config a changé (à enregistrer).
pub fn liste(c: &mut Config, fenetres: &[Fenetre]) -> (Vec<Perso>, bool) {
    let mut change = false;
    for w in fenetres {
        if !w.nom.is_empty() && !c.ordre.iter().any(|n| n.eq_ignore_ascii_case(&w.nom)) {
            c.ordre.push(w.nom.clone());
            change = true;
        }
    }
    let par_nom = |n: &str| fenetres.iter().find(|w| !w.nom.is_empty() && w.nom.eq_ignore_ascii_case(n));
    let mut l: Vec<Perso> = c
        .ordre
        .iter()
        .map(|n| match par_nom(n) {
            Some(w) => Perso {
                nom: n.clone(),
                classe: if w.classe.is_empty() { c.classe(n) } else { w.classe.clone() },
                hwnd: w.hwnd,
                pid: w.pid,
                en_ligne: true,
            },
            None => Perso { nom: n.clone(), classe: c.classe(n), hwnd: 0, pid: 0, en_ligne: false },
        })
        .collect();
    for w in fenetres {
        if w.nom.is_empty() {
            l.push(Perso { nom: String::new(), classe: String::new(), hwnd: w.hwnd, pid: w.pid, en_ligne: true });
        }
    }
    for w in fenetres {
        if !w.nom.is_empty() && !w.classe.is_empty() && c.classe(&w.nom) != w.classe {
            mettre(&mut c.classes, &w.nom, w.classe.clone());
            change = true;
        }
    }
    (l, change)
}

/// Les persos connectés, numérotés dans l'ordre (Get-OrgOnline).
pub fn en_ligne(l: &[Perso]) -> Vec<Perso> {
    l.iter().filter(|p| p.en_ligne && !p.nom.is_empty()).cloned().collect()
}

/// Un perso lâché sur un autre prend sa place (Move-OrgChar). `true` si l'ordre a changé.
pub fn deplacer(c: &mut Config, nom: &str, cible: &str) -> bool {
    let (Some(de), Some(vers)) = (c.ordre.iter().position(|n| n == nom), c.ordre.iter().position(|n| n == cible))
    else {
        return false;
    };
    if de == vers {
        return false;
    }
    let x = c.ordre.remove(de);
    c.ordre.insert(vers, x);
    true
}

/// Retire un perso de la liste (Remove-OrgChar).
pub fn retirer(c: &mut Config, nom: &str) {
    c.ordre.retain(|n| !n.eq_ignore_ascii_case(nom));
}

/// Le perso à afficher (Get-OrgTarget) : « Next », « Prev » ou « P3 », d'après la fenêtre devant (hwnd ou pid).
pub fn cible(en_ligne: &[Perso], action: &str, devant_hwnd: isize, devant_pid: u32) -> Option<Perso> {
    if en_ligne.is_empty() {
        return None;
    }
    let a = action.to_lowercase();
    if a.len() > 1 && a.starts_with('p') && a[1..].chars().all(|c| c.is_ascii_digit()) {
        let n: usize = a[1..].parse().ok()?;
        return if n >= 1 && n <= en_ligne.len() { Some(en_ligne[n - 1].clone()) } else { None };
    }
    let mut i: Option<usize> = None;
    for (k, p) in en_ligne.iter().enumerate() {
        if p.hwnd == devant_hwnd || (devant_pid != 0 && p.pid == devant_pid) {
            i = Some(k);
        }
    }
    let Some(i) = i else { return Some(en_ligne[0].clone()) };
    let n = en_ligne.len();
    Some(if a == "next" { en_ligne[(i + 1) % n].clone() } else { en_ligne[(i + n - 1) % n].clone() })
}

/// Le nom lisible d'une touche (Format-OrgKey) : « D1 » donne « 1 », « NumPad1 » donne « Pavé 1 ».
pub fn nom_touche(t: &str) -> String {
    if t.is_empty() {
        return "Aucune".into();
    }
    t.split('+')
        .map(|p| {
            let p = p.trim();
            let b = p.to_lowercase();
            if b.len() == 2 && b.starts_with('d') && b.as_bytes()[1].is_ascii_digit() {
                return p[1..].to_string();
            }
            if b.len() == 7 && b.starts_with("numpad") && b.as_bytes()[6].is_ascii_digit() {
                return format!("Pavé {}", &p[6..]);
            }
            match b.as_str() {
                "souris4" => "Souris 4 (arrière)".into(),
                "souris5" => "Souris 5 (avant)".into(),
                "molette" => "Clic molette".into(),
                "space" => "Espace".into(),
                "return" => "Entrée".into(),
                "escape" => "Échap".into(),
                "back" => "Retour".into(),
                "oem7" => "²".into(),
                "oem3" => "ù".into(),
                _ => p.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Une touche de l'Organizer pour Windows (ConvertTo-OrgHotkey) : (modificateurs, code de touche).
/// Modificateurs : 1 Alt, 2 Ctrl, 4 Maj, 8 Win.
pub fn touche_windows(t: &str) -> Option<(u32, u32)> {
    if t.is_empty() {
        return None;
    }
    let mut mods = 0;
    let mut vk = None;
    for p in t.split('+') {
        match p.trim().to_lowercase().as_str() {
            "ctrl" => mods |= 2,
            "alt" => mods |= 1,
            "maj" => mods |= 4,
            "win" => mods |= 8,
            autre => {
                if let Some(v) = code_touche(autre) {
                    vk = Some(v);
                }
            }
        }
    }
    vk.map(|v| (mods, v))
}

/// Un bouton de souris (ConvertTo-OrgMouse) : (modificateurs, bouton 3 molette, 4 ou 5 latéraux).
pub fn souris(t: &str) -> Option<(u32, u32)> {
    if t.is_empty() {
        return None;
    }
    let (mut mods, mut b) = (0, 0);
    for p in t.split('+') {
        match p.trim().to_lowercase().as_str() {
            "ctrl" => mods |= 2,
            "alt" => mods |= 1,
            "maj" => mods |= 4,
            "win" => mods |= 8,
            "souris4" => b = 4,
            "souris5" => b = 5,
            "molette" => b = 3,
            _ => return None,
        }
    }
    (b != 0).then_some((mods, b))
}

/// Le code Windows d'une touche, d'après son nom WPF (System.Windows.Input.Key).
fn code_touche(n: &str) -> Option<u32> {
    let b = n.as_bytes();
    if b.len() == 1 && b[0].is_ascii_lowercase() {
        return Some(b[0].to_ascii_uppercase() as u32);
    }
    if b.len() == 2 && b[0] == b'd' && b[1].is_ascii_digit() {
        return Some(b[1] as u32);
    }
    if let Some(x) = n.strip_prefix("numpad")
        && let Ok(d) = x.parse::<u32>()
        && d <= 9
    {
        return Some(0x60 + d);
    }
    if let Some(x) = n.strip_prefix('f')
        && let Ok(d) = x.parse::<u32>()
        && (1..=24).contains(&d)
    {
        return Some(0x6F + d);
    }
    Some(match n {
        "tab" => 0x09,
        "space" => 0x20,
        "return" | "enter" => 0x0D,
        "escape" => 0x1B,
        "back" => 0x08,
        "oem3" => 0xC0,
        "oem7" => 0xDE,
        "oem1" => 0xBA,
        "oem2" => 0xBF,
        "oem4" => 0xDB,
        "oem5" => 0xDC,
        "oem6" => 0xDD,
        "oemplus" => 0xBB,
        "oemminus" => 0xBD,
        "oemcomma" => 0xBC,
        "oemperiod" => 0xBE,
        "home" => 0x24,
        "end" => 0x23,
        "insert" => 0x2D,
        "delete" => 0x2E,
        "pageup" | "prior" => 0x21,
        "pagedown" | "next" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titres() {
        assert_eq!(lire_titre("Bob - Iop - 3.0.12 - Release"), ("Bob".into(), "Iop".into()));
        assert_eq!(lire_titre("Bob - Dofus 2.70"), ("Bob".into(), String::new()));
        assert_eq!(lire_titre("Dofus"), (String::new(), String::new()));
        assert_eq!(lire_titre("Dofus - 3.0"), (String::new(), String::new()));
    }

    #[test]
    fn touches() {
        assert_eq!(nom_touche("Ctrl+Maj+D1"), "Ctrl + Maj + 1");
        assert_eq!(nom_touche("NumPad3"), "Pavé 3");
        assert_eq!(nom_touche(""), "Aucune");
        assert_eq!(touche_windows("Ctrl+Tab"), Some((2, 9)));
        assert_eq!(touche_windows("F1"), Some((0, 0x70)));
        assert_eq!(souris("Ctrl+Souris4"), Some((2, 4)));
        assert_eq!(souris("Ctrl+A"), None);
    }

    #[test]
    fn ordre() {
        let mut c = Config::default();
        let f = vec![
            Fenetre { nom: "A".into(), classe: "Iop".into(), pid: 1, hwnd: 10 },
            Fenetre { nom: "B".into(), classe: String::new(), pid: 2, hwnd: 20 },
            Fenetre { nom: String::new(), classe: String::new(), pid: 3, hwnd: 30 },
        ];
        let (l, ch) = liste(&mut c, &f);
        assert!(ch);
        assert_eq!(l.len(), 3);
        let on = en_ligne(&l);
        assert_eq!(cible(&on, "Next", 10, 0).unwrap().nom, "B");
        assert_eq!(cible(&on, "Prev", 10, 0).unwrap().nom, "B");
        assert_eq!(cible(&on, "P1", 0, 0).unwrap().nom, "A");
        assert!(cible(&on, "P3", 0, 0).is_none());
        assert!(deplacer(&mut c, "B", "A"));
        assert_eq!(c.ordre, vec!["B", "A"]);
    }
}
