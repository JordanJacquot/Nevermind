//! L'Organizer Dofus (organizer.ps1) : la règle qui le montre ou le cache (Test-OrgVisible, Set-OrgVisible), ses
//! réglages (organizer.json), la lecture du titre des fenêtres, l'ordre d'initiative des persos, le perso suivant ou
//! précédent, les touches, les couleurs des classes.
//!
//! La partie fenêtres est dans les sous-modules : `veille` (Update-OrgWatch, Register-OrgHotkeys, la barre et la vue,
//! sans Windows : les tests la rejouent contre la version PowerShell), `vue` (ce que montre la vue), `fil` (le fil à part
//! qui fait tourner tout cela, seulement quand l'Organizer est allumé ou sa vue ouverte) et `windows::organizer`
//! (Win32 : fenêtres du jeu, raccourcis, souris, barre flottante, vue).

pub mod fil;
pub mod touches;
pub mod veille;
pub mod vue;

use crate::json::{lire_json_sur, vrai};
use nevermind_noyau::Contexte;
use serde_json::{Map, Value, json};
use std::path::Path;

pub const MAX_TOUCHES: usize = 8;
/// Le nombre de « touches simulées » : un bouton de souris qui appuie sur une touche dans le jeu (la vue en garde 20
/// au plus : elles tiennent toutes sur un écran, sans défilement).
pub const MAX_SIMULEES: usize = 20;
/// Les identifiants des raccourcis ($OrgHotkeyBase) : 7100 suivant, 7101 précédent, 7111 à 7118 perso 1 à 8,
/// 7131 à 7150 touches simulées 1 à 20.
pub const BASE_TOUCHES: i32 = 7100;

/// Les actions, dans l'ordre de Register-OrgHotkeys (puis les touches simulées, propres à la version Rust).
pub fn actions() -> Vec<String> {
    let mut l = vec!["Next".to_string(), "Prev".to_string()];
    l.extend((1..=MAX_TOUCHES).map(|i| format!("P{i}")));
    l.extend((1..=MAX_SIMULEES).map(|i| format!("S{i}")));
    l
}

/// Le numéro d'une touche simulée (« S12 » : 12).
fn numero_simulee(a: &str) -> Option<usize> {
    a.strip_prefix('S').and_then(|n| n.parse().ok()).filter(|n| (1..=MAX_SIMULEES).contains(n))
}

/// Les touches simulées réglées (un bouton ou une touche choisis), dans l'ordre de leur numéro.
pub fn simulees(c: &Config) -> Vec<String> {
    (1..=MAX_SIMULEES).map(|i| format!("S{i}")).filter(|s| !c.touche(s).is_empty() || !c.envoi(s).is_empty()).collect()
}

/// La première touche simulée libre (`None` : les 20 sont prises).
pub fn simulee_libre(c: &Config) -> Option<String> {
    (1..=MAX_SIMULEES).map(|i| format!("S{i}")).find(|s| c.touche(s).is_empty() && c.envoi(s).is_empty())
}

/// L'identifiant du raccourci d'une action.
pub fn id_action(a: &str) -> i32 {
    match a {
        "Next" => BASE_TOUCHES,
        "Prev" => BASE_TOUCHES + 1,
        _ if simulee(a) => BASE_TOUCHES + 30 + a[1..].parse::<i32>().unwrap_or(0),
        _ => BASE_TOUCHES + 10 + a[1..].parse::<i32>().unwrap_or(0),
    }
}

/// Une touche simulée (« S1 » à « S20 ») : son raccourci appuie sur une touche dans le jeu au lieu de changer de perso.
pub fn simulee(a: &str) -> bool {
    numero_simulee(a).is_some()
}

/// La touche envoyée par une touche simulée (« E1 » à « E20 », choisie dans la vue) : seulement au clavier.
pub fn envoi(a: &str) -> bool {
    a.strip_prefix('E').and_then(|n| n.parse::<usize>().ok()).is_some_and(|n| (1..=MAX_SIMULEES).contains(&n))
}

/// Deux noms égaux pour PowerShell (-eq, -contains, clés d'une table) : sans tenir compte de la casse.
pub fn meme_nom(a: &str, b: &str) -> bool {
    a.chars().count() == b.chars().count()
        && a.chars().zip(b.chars()).all(|(x, y)| x == y || crate::texte::minuscule(x) == crate::texte::minuscule(y))
}

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
    /// Les touches : Next, Prev, P1 à P8 (et S1 à S3 : le raccourci des touches simulées).
    pub touches: Vec<(String, String)>,
    /// La touche que chaque touche simulée fait dans le jeu : S1 à S3 (« Sends » dans organizer.json).
    pub envois: Vec<(String, String)>,
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
            envois: Vec::new(),
        }
    }
}

/// Une valeur d'une table PowerShell (@{}) : la clé garde sa première écriture, la casse ne compte pas.
pub fn mettre(l: &mut Vec<(String, String)>, k: &str, v: String) {
    match l.iter_mut().find(|(x, _)| meme_nom(x, k)) {
        Some(e) => e.1 = v,
        None => l.push((k.to_string(), v)),
    }
}

fn lire_table(l: &[(String, String)], k: &str) -> String {
    l.iter().find(|(x, _)| meme_nom(x, k)).map(|x| x.1.clone()).unwrap_or_default()
}

/// `[string]$x` de PowerShell pour une valeur lue dans un fichier JSON.
pub fn chaine_ps(v: &Value) -> String {
    fn element(v: &Value) -> String {
        match v {
            Value::Array(_) => "System.Object[]".into(),
            x => chaine_ps(x),
        }
    }
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Number(n) => n.to_string(),
        Value::Array(a) => a.iter().map(element).collect::<Vec<_>>().join(" "),
        Value::Object(m) => {
            format!("@{{{}}}", m.iter().map(|(k, x)| format!("{k}={}", element(x))).collect::<Vec<_>>().join("; "))
        }
    }
}

/// `[double]$x` de PowerShell : `Err` quand PowerShell lève une erreur.
fn double_ps(v: &Value) -> Result<f64, String> {
    match v {
        Value::Number(n) => n.as_f64().ok_or_else(|| "nombre illisible".to_string()),
        Value::Bool(b) => Ok(f64::from(u8::from(*b))),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                return Ok(0.0);
            }
            t.parse::<f64>()
                .ok()
                .filter(|x| x.is_finite() || t.to_lowercase().contains("inf"))
                .or_else(|| t.replace(',', "").parse::<f64>().ok())
                .ok_or_else(|| format!("Impossible de convertir la valeur « {s} » en type « System.Double »."))
        }
        _ => Err("Impossible de convertir la valeur en type « System.Double ».".into()),
    }
}

/// Les propriétés d'un objet lu (`$j.Hotkeys.PSObject.Properties`) : celles de l'objet JSON, ou la longueur d'un texte.
fn proprietes(v: &Value) -> Vec<(String, String)> {
    match v {
        Value::Object(m) => m.iter().map(|(k, x)| (k.clone(), chaine_ps(x))).collect(),
        Value::String(s) => vec![("Length".into(), s.encode_utf16().count().to_string())],
        _ => Vec::new(),
    }
}

impl Config {
    /// Les réglages (Get-OrgConfig).
    pub fn lire(ctx: Option<&Contexte>, donnees: &Path) -> Self {
        let (j, _) = lire_json_sur(ctx, &donnees.join("organizer.json"));
        let (c, erreur) = Self::depuis(j.as_ref());
        if let (Some(e), Some(ctx)) = (erreur, ctx) {
            ctx.journal(&format!("Organizer : réglages illisibles : {e}"));
        }
        c
    }

    /// Les réglages d'après le contenu lu ; l'erreur arrête la lecture où elle est (comme le try de Get-OrgConfig).
    pub fn depuis(j: Option<&Value>) -> (Self, Option<String>) {
        let mut c = Config::default();
        let Some(j) = j.filter(|j| vrai(j)) else { return (c, None) };
        let r: Result<(), String> = (|| {
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
                c.barre_x = Some(double_ps(v)?);
            }
            if let Some(v) = j.get("BarY").filter(|v| !v.is_null()) {
                c.barre_y = Some(double_ps(v)?);
            }
            c.ordre = crate::biblio::liste(j.get("Order")).iter().filter(|x| vrai(x)).map(chaine_ps).collect();
            if let Some(h) = j.get("Hotkeys").filter(|h| vrai(h)) {
                for (k, v) in proprietes(h) {
                    mettre(&mut c.touches, &k, v);
                }
            }
            if let Some(h) = j.get("Classes").filter(|h| vrai(h)) {
                for (k, v) in proprietes(h) {
                    mettre(&mut c.classes, &k, v);
                }
            }
            if let Some(h) = j.get("Sends").filter(|h| vrai(h)) {
                for (k, v) in proprietes(h) {
                    mettre(&mut c.envois, &k, v);
                }
            }
            Ok(())
        })();
        (c, r.err())
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
        let mut j = json!({ "On": self.on, "Order": self.ordre, "Bar": self.barre, "BarAlways": self.barre_toujours,
            "BarX": self.barre_x, "BarY": self.barre_y, "Classes": cl, "Hotkeys": h });
        // (« Sends » seulement s'il y en a : le fichier reste celui de la version PowerShell sinon)
        if !self.envois.is_empty() {
            let mut e = Map::new();
            for (k, v) in &self.envois {
                e.insert(k.clone(), Value::String(v.clone()));
            }
            j["Sends"] = Value::Object(e);
        }
        j
    }

    /// Enregistre (Save-OrgConfig).
    pub fn ecrire(&self, ctx: Option<&Contexte>, donnees: &Path) {
        if let Err(e) = crate::json::ecrire_json_sur(&donnees.join("organizer.json"), &self.vers_json(), false)
            && let Some(c) = ctx
        {
            c.journal(&format!("Organizer : enregistrement impossible : {e}"));
        }
    }

    /// La classe retenue d'un perso (`[string]$c.Classes[$n]`).
    pub fn classe(&self, nom: &str) -> String {
        lire_table(&self.classes, nom)
    }

    /// La touche d'une action (`$c.Hotkeys[$a]`), « » si aucune.
    pub fn touche(&self, action: &str) -> String {
        lire_table(&self.touches, action)
    }

    /// La touche faite dans le jeu par une touche simulée (« S1 »), « » si aucune.
    pub fn envoi(&self, simulee: &str) -> String {
        lire_table(&self.envois, simulee)
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

/// Une fenêtre Dofus ouverte (Get-DofusWindows).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Fenetre {
    pub nom: String,
    pub classe: String,
    pub pid: u32,
    pub hwnd: isize,
    pub titre: String,
}

impl Fenetre {
    /// La fenêtre d'après son titre (le nom et la classe en sont tirés).
    pub fn depuis_titre(titre: &str, pid: u32, hwnd: isize) -> Self {
        let (nom, classe) = lire_titre(titre);
        Fenetre { nom, classe, pid, hwnd, titre: titre.to_string() }
    }
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
/// Renvoie aussi le nombre d'enregistrements de la config demandés (0, 1 ou 2, comme Save-OrgConfig).
pub fn liste(c: &mut Config, fenetres: &[Fenetre]) -> (Vec<Perso>, usize) {
    let mut enregistrements = 0;
    let mut ajoute = false;
    for w in fenetres {
        if !w.nom.is_empty() && !c.ordre.iter().any(|n| meme_nom(n, &w.nom)) {
            c.ordre.push(w.nom.clone());
            ajoute = true;
        }
    }
    if ajoute {
        enregistrements += 1;
    }
    let par_nom = |n: &str| fenetres.iter().find(|w| !w.nom.is_empty() && meme_nom(&w.nom, n));
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
    let mut vu = false;
    for w in fenetres {
        if !w.nom.is_empty() && !w.classe.is_empty() && !meme_nom(&c.classe(&w.nom), &w.classe) {
            mettre(&mut c.classes, &w.nom, w.classe.clone());
            vu = true;
        }
    }
    if vu {
        enregistrements += 1;
    }
    (l, enregistrements)
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
    c.ordre.remove(de);
    c.ordre.insert(vers, nom.to_string());
    true
}

/// Retire un perso de la liste (Remove-OrgChar).
pub fn retirer(c: &mut Config, nom: &str) {
    c.ordre.retain(|n| !meme_nom(n, nom));
}

/// Le perso à afficher (Get-OrgTarget) : « Next », « Prev » ou « P3 », d'après la fenêtre devant (hwnd ou pid).
pub fn cible(en_ligne: &[Perso], action: &str, devant_hwnd: isize, devant_pid: u32) -> Option<Perso> {
    if en_ligne.is_empty() {
        return None;
    }
    let mut cs = action.chars();
    if matches!(cs.next(), Some('p' | 'P')) && action.len() > 1 && action[1..].bytes().all(|b| b.is_ascii_digit()) {
        let n: usize = action[1..].parse().ok()?;
        // (« P0 » : $on[-1], le dernier, comme PowerShell)
        return if n == 0 {
            en_ligne.last().cloned()
        } else if n <= en_ligne.len() {
            Some(en_ligne[n - 1].clone())
        } else {
            None
        };
    }
    let mut i: Option<usize> = None;
    for (k, p) in en_ligne.iter().enumerate() {
        if p.hwnd == devant_hwnd || (devant_pid != 0 && p.pid == devant_pid) {
            i = Some(k);
        }
    }
    let Some(i) = i else { return Some(en_ligne[0].clone()) };
    let n = en_ligne.len();
    Some(if meme_nom(action, "Next") { en_ligne[(i + 1) % n].clone() } else { en_ligne[(i + n - 1) % n].clone() })
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

/// Un modificateur (Ctrl, Alt, Maj, Win) : 1 Alt, 2 Ctrl, 4 Maj, 8 Win.
fn modificateur(p: &str) -> Option<u32> {
    if meme_nom(p, "Ctrl") {
        Some(2)
    } else if meme_nom(p, "Alt") {
        Some(1)
    } else if meme_nom(p, "Maj") {
        Some(4)
    } else if meme_nom(p, "Win") {
        Some(8)
    } else {
        None
    }
}

/// Une touche de l'Organizer pour Windows (ConvertTo-OrgHotkey) : (modificateurs, code de touche).
/// Modificateurs : 1 Alt, 2 Ctrl, 4 Maj, 8 Win.
pub fn touche_windows(t: &str) -> Option<(u32, u32)> {
    if t.is_empty() {
        return None;
    }
    let mut mods = 0;
    let mut cle: Option<i64> = None;
    for p in t.split('+') {
        let p = p.trim();
        match modificateur(p) {
            Some(m) => mods |= m,
            None => {
                if let Some(v) = touches::cle_wpf(p) {
                    cle = Some(v);
                }
            }
        }
    }
    let cle = cle.filter(|&v| v != 0)?;
    Some((mods, touches::vk_de_cle(cle)))
}

/// Un bouton de souris (ConvertTo-OrgMouse) : (modificateurs, bouton 3 molette, 4 ou 5 latéraux).
pub fn souris(t: &str) -> Option<(u32, u32)> {
    if t.is_empty() {
        return None;
    }
    let (mut mods, mut b) = (0, 0);
    for p in t.split('+') {
        let p = p.trim();
        if let Some(m) = modificateur(p) {
            mods |= m;
        } else if meme_nom(p, "Souris4") {
            b = 4;
        } else if meme_nom(p, "Souris5") {
            b = 5;
        } else if meme_nom(p, "Molette") {
            b = 3;
        } else {
            return None;
        }
    }
    (b != 0).then_some((mods, b))
}

/// Le texte d'une touche avec les modificateurs tenus (Add-OrgModifiers) : « Ctrl+Alt+Maj+Win+Touche ».
pub fn avec_modificateurs(touche: &str, mods: u32) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if mods & 2 != 0 {
        parts.push("Ctrl");
    }
    if mods & 1 != 0 {
        parts.push("Alt");
    }
    if mods & 4 != 0 {
        parts.push("Maj");
    }
    if mods & 8 != 0 {
        parts.push("Win");
    }
    parts.push(touche);
    parts.join("+")
}

/// Ce que fait une touche appuyée pendant le choix d'un raccourci (Receive-OrgKey).
#[derive(Debug, Clone, PartialEq)]
pub enum Capture {
    /// Un modificateur seul : on attend la suite.
    Attendre,
    /// Échap : choix annulé.
    Annuler,
    /// Le raccourci choisi (« » : aucune touche, par Retour arrière ou Suppr).
    Choisir(String),
}

/// La touche appuyée (code Windows) avec les modificateurs tenus (1 Alt, 2 Ctrl, 4 Maj, 8 Win).
pub fn touche_capturee(vk: u32, mods: u32) -> Capture {
    let nom = touches::nom_de_vk(vk);
    match nom {
        "LeftCtrl" | "RightCtrl" | "LeftShift" | "RightShift" | "LeftAlt" | "RightAlt" | "LWin" | "RWin" => {
            Capture::Attendre
        }
        "Escape" => Capture::Annuler,
        "Back" | "Delete" => Capture::Choisir(String::new()),
        n => Capture::Choisir(avec_modificateurs(n, mods)),
    }
}

/// Un bouton de souris appuyé pendant le choix (Receive-OrgMouse) : 3 molette, 4 et 5 latéraux ; les autres : rien.
pub fn souris_capturee(bouton: u32, mods: u32) -> Option<String> {
    let n = match bouton {
        4 => "Souris4",
        5 => "Souris5",
        3 => "Molette",
        _ => return None,
    };
    Some(avec_modificateurs(n, mods))
}

/// La couleur de chaque classe ($OrgClassColors).
const COULEURS: &[(&str, &str)] = &[
    ("iop", "#E5484D"),
    ("cra", "#46A758"),
    ("eniripsa", "#E93D82"),
    ("sacrieur", "#B3261E"),
    ("ecaflip", "#F5C542"),
    ("enutrof", "#C08B3A"),
    ("sram", "#6E56CF"),
    ("xelor", "#3E63DD"),
    ("feca", "#2EB8D6"),
    ("osamodas", "#8D6E4A"),
    ("sadida", "#5BB98C"),
    ("pandawa", "#12A594"),
    ("roublard", "#A33A3A"),
    ("zobal", "#8E4EC6"),
    ("steamer", "#0D74CE"),
    ("eliotrope", "#00B3C7"),
    ("huppermage", "#7C66DC"),
    ("ouginak", "#D9822B"),
    ("forgelance", "#6B7F99"),
];

/// Le numéro de chaque classe ($OrgBreeds).
const CLASSES: &[(&str, u32)] = &[
    ("feca", 1),
    ("osamodas", 2),
    ("enutrof", 3),
    ("sram", 4),
    ("xelor", 5),
    ("ecaflip", 6),
    ("eniripsa", 7),
    ("iop", 8),
    ("cra", 9),
    ("sadida", 10),
    ("sacrieur", 11),
    ("pandawa", 12),
    ("roublard", 13),
    ("zobal", 14),
    ("steamer", 15),
    ("eliotrope", 16),
    ("huppermage", 17),
    ("ouginak", 18),
    ("forgelance", 20),
];

/// Le numéro d'une classe (Get-OrgBreedId), 0 si inconnue.
pub fn id_classe(classe: &str) -> u32 {
    let k = crate::texte::texte_recherche(classe);
    CLASSES.iter().find(|(n, _)| *n == k).map(|x| x.1).unwrap_or(0)
}

/// La teinte d'un nom (Get-GameHue, bibliotheque.ps1).
pub fn teinte(nom: &str) -> &'static str {
    const PALETTE: [&str; 8] = ["#4EA8FF", "#B18CFF", "#22D37A", "#F5A524", "#FF5CC8", "#2EC4D6", "#FF6B5B", "#8FA8FF"];
    let mut h: i64 = 0;
    for c in nom.encode_utf16() {
        h = (h * 31 + i64::from(c)) % 360;
    }
    PALETTE[(h % 8) as usize]
}

/// La couleur de la pastille d'un perso (Get-OrgColor) : celle de sa classe, sinon une tirée de son nom.
pub fn couleur(classe: &str, nom: &str) -> &'static str {
    let k = crate::texte::texte_recherche(classe);
    if let Some((_, c)) = COULEURS.iter().find(|(n, _)| !k.is_empty() && *n == k) {
        return c;
    }
    teinte(if nom.is_empty() { "Dofus" } else { nom })
}

/// L'initiale d'un perso sur sa pastille (`$Char.Name.Substring(0, 1).ToUpper()`, « ? » sans nom).
pub fn initiale(nom: &str) -> String {
    match nom.chars().next() {
        Some(c) => c.to_uppercase().collect(),
        None => "?".into(),
    }
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
        assert_eq!(touche_windows("A+"), None);
        assert_eq!(souris("Ctrl+Souris4"), Some((2, 4)));
        assert_eq!(souris("Ctrl+A"), None);
        assert_eq!(touche_capturee(0x70, 2), Capture::Choisir("Ctrl+F1".into()));
        assert_eq!(touche_capturee(0x10, 0), Capture::Attendre);
    }

    #[test]
    fn ordre() {
        let mut c = Config::default();
        let f = vec![
            Fenetre { nom: "A".into(), classe: "Iop".into(), pid: 1, hwnd: 10, ..Default::default() },
            Fenetre { nom: "B".into(), pid: 2, hwnd: 20, ..Default::default() },
            Fenetre { pid: 3, hwnd: 30, ..Default::default() },
        ];
        let (l, ch) = liste(&mut c, &f);
        assert_eq!(ch, 2);
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
