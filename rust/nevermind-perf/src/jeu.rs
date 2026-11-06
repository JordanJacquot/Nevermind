//! Le mode jeu et la partie en cours (jeu.ps1), sans Windows : les applis que le mode jeu sait fermer, la sélection
//! cochée, l'index des jeux et la détection d'une partie, ce qui est fermé au début d'une partie, la reprise d'un
//! fichier mode-jeu.json laissé par une partie interrompue, l'alerte de température. Les lectures et les gestes
//! (programmes ouverts, fermeture par l'Aide, relance) sont passés en fonctions : le bloc donne les vrais, les tests
//! des faux.

use crate::pages::{ch, liste, vrai};
use crate::texte::{egal, minuscules, ps_int};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Une appli que le mode jeu sait fermer ($GameModeApps) : seulement des applis qui ne perdent rien si on les ferme.
pub struct AppModeJeu {
    pub id: &'static str,
    pub nom: &'static str,
    /// Les noms exacts de ses programmes (la règle `^(a|b)$` de la version PowerShell)
    pub procs: &'static [&'static str],
    /// Cochée par défaut
    pub on: bool,
    /// Arguments de relance
    pub args: Option<&'static str>,
}

impl AppModeJeu {
    /// `$ProcessName -match $a.Proc` (sans tenir compte des majuscules)
    pub fn correspond(&self, nom: &str) -> bool {
        self.procs.iter().any(|p| egal(p, nom))
    }
}

pub const APPS_MODE_JEU: [AppModeJeu; 11] = [
    AppModeJeu { id: "onedrive", nom: "OneDrive", procs: &["OneDrive"], on: true, args: Some("/background") },
    AppModeJeu { id: "teams", nom: "Microsoft Teams", procs: &["ms-teams", "Teams"], on: true, args: None },
    AppModeJeu { id: "skype", nom: "Skype", procs: &["Skype"], on: true, args: None },
    AppModeJeu {
        id: "adobe",
        nom: "Adobe Creative Cloud",
        procs: &["Creative Cloud", "CCXProcess", "CCLibrary", "Adobe Desktop Service"],
        on: true,
        args: None,
    },
    AppModeJeu { id: "dropbox", nom: "Dropbox", procs: &["Dropbox"], on: true, args: None },
    AppModeJeu { id: "gdrive", nom: "Google Drive", procs: &["GoogleDriveFS"], on: true, args: None },
    AppModeJeu {
        id: "wallpaper",
        nom: "Wallpaper Engine",
        procs: &["wallpaper32", "wallpaper64"],
        on: false,
        args: None,
    },
    AppModeJeu { id: "spotify", nom: "Spotify", procs: &["Spotify"], on: false, args: None },
    AppModeJeu { id: "epic", nom: "Epic Games Launcher", procs: &["EpicGamesLauncher"], on: false, args: None },
    AppModeJeu { id: "ea", nom: "EA app", procs: &["EADesktop"], on: false, args: None },
    AppModeJeu { id: "ubisoft", nom: "Ubisoft Connect", procs: &["UbisoftConnect", "upc"], on: false, args: None },
];

/// L'appli de ce numéro (`-eq` : sans tenir compte des majuscules)
pub fn app(id: &str) -> Option<&'static AppModeJeu> {
    APPS_MODE_JEU.iter().find(|a| egal(a.id, id))
}

/// Get-GameModeSelection : le réglage GameModeApps, sinon les applis cochées par défaut
pub fn selection(reglage: Option<&Value>) -> Vec<String> {
    match reglage {
        None | Some(Value::Null) => APPS_MODE_JEU.iter().filter(|a| a.on).map(|a| a.id.to_string()).collect(),
        Some(v) => liste(Some(v)).iter().map(|x| ch(Some(x))).collect(),
    }
}

/// Update-GameModeStatus : le texte d'état du mode jeu
pub fn etat_mode_jeu(mode_on: bool, session: Option<(String, i64)>, n_jeux: i64) -> String {
    if !mode_on {
        return "Désactivé.".into();
    }
    if let Some((jeu, n)) = session {
        let s = if n > 1 { "s" } else { "" };
        return format!("En jeu : {jeu}. {n} appli{s} fermée{s}.");
    }
    let x = if n_jeux > 1 { "x" } else { "" };
    let s = if n_jeux > 1 { "s" } else { "" };
    format!("Actif : en attente d'un jeu ({n_jeux} jeu{x} surveillé{s}).")
}

/// Noms d'exécutables trop courants pour reconnaître un jeu ($GenericExe)
pub fn exe_generique(base: &str) -> bool {
    ["game", "launcher", "start", "play", "client", "main", "app", "run", "win64", "win32", "bin"]
        .iter()
        .any(|g| egal(g, base))
}

/// `[IO.Path]::GetFileName`
pub fn nom_fichier(chemin: &str) -> &str {
    chemin.rsplit(['\\', '/']).next().unwrap_or(chemin)
}
/// `[IO.Path]::GetFileNameWithoutExtension`
pub fn nom_sans_extension(chemin: &str) -> &str {
    let n = nom_fichier(chemin);
    match n.rfind('.') {
        Some(i) => &n[..i],
        None => n,
    }
}
/// `[IO.Path]::GetExtension`
pub fn extension(chemin: &str) -> &str {
    let n = nom_fichier(chemin);
    match n.rfind('.') {
        Some(i) => &n[i..],
        None => "",
    }
}
/// `[IO.Path]::IsPathRooted`
pub fn enracine(chemin: &str) -> bool {
    let b = chemin.as_bytes();
    (!b.is_empty() && (b[0] == b'\\' || b[0] == b'/')) || (b.len() >= 2 && b[1] == b':')
}

/// Get-GameExeNames : les noms des programmes d'un jeu qui servent aux profils (8 caractères au moins, pas génériques)
pub fn noms_exes_profil(exes: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in exes {
        let c = ch(Some(e));
        let n = nom_fichier(&c).to_string();
        if exe_generique(nom_sans_extension(&n)) || n.encode_utf16().count() < 8 {
            continue;
        }
        // (Select-Object -Unique : tient compte des majuscules)
        if !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// Les vraies cartes graphiques (sans les cartes virtuelles ou d'écran à distance)
pub fn gpus_reelles(gpus: &[Value]) -> Vec<String> {
    let exclues = ["remote", "virtual", "parsec", "mirage", "displaylink", "citrix", "meta", "microsoft basic"];
    gpus.iter()
        .map(|g| ch(Some(g)))
        .filter(|n| {
            let l = n.to_lowercase();
            !exclues.iter().any(|x| l.contains(x))
        })
        .collect()
}

/// `(Get-GpuPreference $exe) -match 'GpuPreference=2'`
pub fn preference_puissante(v: &str) -> bool {
    v.to_lowercase().contains("gpupreference=2")
}

/// (année, mois, jour, heure, minute, seconde)
pub type Date = (i64, u32, u32, u32, u32, u32);

/// Une date `aaaa-mm-jjThh:mm:ss` (heure locale)
pub fn date_iso(t: &str) -> Option<Date> {
    let t = t.trim();
    let (d, h) = t.split_once('T').unwrap_or((t, "00:00:00"));
    let mut dp = d.split('-');
    let a: i64 = dp.next()?.parse().ok()?;
    let m: u32 = dp.next()?.parse().ok()?;
    let j: u32 = dp.next()?.parse().ok()?;
    let h = h.split(['+', 'Z']).next().unwrap_or("");
    let mut hp = h.split(':');
    let hh: u32 = hp.next().unwrap_or("0").parse().ok()?;
    let mi: u32 = hp.next().unwrap_or("0").parse().ok()?;
    let se: u32 = hp.next().unwrap_or("0").split('.').next().unwrap_or("0").parse().ok()?;
    Some((a, m, j, hh, mi, se))
}

// ---------------------------------------------------------------------------
// Index des jeux et détection d'une partie
// ---------------------------------------------------------------------------

/// Un nom de programme connu ($script:GameIndex) : le jeu, ses chemins, et le jeu de chaque chemin (deux jeux au même
/// nom de programme, comme Dofus et Dofus 2, se reconnaissent par leur dossier)
#[derive(Clone, Debug, Default)]
pub struct EntreeIndex {
    pub jeu: String,
    pub exes: Vec<String>,
    pub par_chemin: HashMap<String, String>,
}

/// Index des jeux (Update-GameCache) depuis la liste des jeux `[{ nom, exes }]`
pub fn index_jeux(jeux: &[Value]) -> HashMap<String, EntreeIndex> {
    let mut idx: HashMap<String, EntreeIndex> = HashMap::new();
    for g in jeux {
        let nom = ch(g.get("nom"));
        for e in liste(g.get("exes")) {
            let e = ch(Some(&e));
            let base = minuscules(nom_sans_extension(&e));
            if base.encode_utf16().count() < 3 || exe_generique(&base) {
                continue;
            }
            let x = idx.entry(base).or_insert_with(|| EntreeIndex { jeu: nom.clone(), ..Default::default() });
            x.exes.push(minuscules(&e));
            x.par_chemin.insert(minuscules(&e), nom.clone());
        }
    }
    idx
}

/// Un programme ouvert : (numéro, nom sans .exe, chemin ; vide s'il n'a pas pu être lu)
pub struct Ouvert {
    pub pid: u32,
    pub nom: String,
    pub chemin: String,
}

/// Test-GameRunning : le premier programme ouvert qui est un jeu connu (même nom ET même chemin) : (jeu, numéro)
pub fn detecter(idx: &HashMap<String, EntreeIndex>, ouverts: &[Ouvert]) -> Option<(String, u32)> {
    for p in ouverts {
        let Some(info) = idx.get(&minuscules(&p.nom)) else { continue };
        let c = minuscules(&p.chemin);
        if !c.is_empty() && info.exes.contains(&c) {
            let nom = info.par_chemin.get(&c).filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| info.jeu.clone());
            return Some((nom, p.pid));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Début et fin d'une partie : les applis fermées, puis relancées
// ---------------------------------------------------------------------------

/// Une appli fermée pendant la partie
#[derive(Clone, Debug, PartialEq)]
pub struct Fermee {
    pub id: String,
    pub nom: String,
    pub chemin: String,
    pub args: Option<String>,
}

impl Fermee {
    pub fn json(&self) -> Value {
        // (chemin inconnu : null, comme la version PowerShell)
        json!({ "id": self.id, "nom": self.nom, "chemin": if self.chemin.is_empty() { Value::Null } else { json!(self.chemin) }, "args": self.args })
    }
}

/// Start-GameSession, partie « applis » : pour chaque appli cochée (mode jeu activé) qui a un programme ouvert, ses
/// programmes sont fermés avec la règle commune (`fermer` rend le nombre fermé) ; elle est retenue si l'un l'a été.
pub fn fermer_pour_partie(
    mode_on: bool,
    sel: &[String],
    ouverts: &[(String, String)],
    mut fermer: impl FnMut(&AppModeJeu, &[&(String, String)]) -> usize,
) -> Vec<Fermee> {
    let mut out = Vec::new();
    if !mode_on {
        return out;
    }
    for a in APPS_MODE_JEU.iter().filter(|a| sel.iter().any(|s| egal(s, a.id))) {
        let procs: Vec<&(String, String)> = ouverts.iter().filter(|p| a.correspond(&p.0)).collect();
        if procs.is_empty() {
            continue;
        }
        let chemin = procs.iter().map(|p| p.1.clone()).find(|c| !c.is_empty()).unwrap_or_default();
        if fermer(a, &procs) > 0 {
            out.push(Fermee { id: a.id.into(), nom: a.nom.into(), chemin, args: a.args.map(str::to_string) });
        }
    }
    out
}

/// La bulle du début de partie (aucune si rien n'a été fermé)
pub fn bulle_debut(jeu: &str, fermees: &[Fermee]) -> Option<(String, String)> {
    if fermees.is_empty() {
        return None;
    }
    let noms: Vec<&str> = fermees.iter().map(|f| f.nom.as_str()).collect();
    let s = if fermees.len() > 1 { "s" } else { "" };
    Some(("Mode jeu activé".into(), format!("{jeu} : {} fermé{s} pendant que tu joues.", noms.join(", "))))
}

/// Le fichier mode-jeu.json d'une partie (Save-GameModePending) ; `None` : rien à garder (le fichier est effacé)
pub fn fichier_partie(jeu: &str, pid: u32, proc_nom: &str, debut_o: &str, fermees: &[Fermee]) -> Option<Value> {
    if fermees.is_empty() {
        return None;
    }
    let closed: Vec<Value> = fermees.iter().map(|f| json!({ "Id": f.id, "Name": f.nom, "Path": f.chemin })).collect();
    Some(json!({ "Game": jeu, "Pid": pid, "Proc": proc_nom, "Start": debut_o, "Closed": closed }))
}

/// Restart-GameModeApps : relance les applis fermées (celles déjà rouvertes sont laissées) ; rend les noms de celles à
/// relancer à la main. `ouvert(nom)` : un programme de ce nom tourne ; `lancer(chemin, args)` : relance.
pub fn relancer(
    fermees: &[Fermee],
    ouvert: impl Fn(&str) -> bool,
    mut lancer: impl FnMut(&str, Option<&str>) -> bool,
) -> Vec<String> {
    let mut rates = Vec::new();
    for c in fermees {
        if c.chemin.is_empty() {
            rates.push(c.nom.clone());
            continue;
        }
        if ouvert(nom_sans_extension(&c.chemin)) {
            continue;
        }
        if !lancer(&c.chemin, c.args.as_deref()) {
            rates.push(c.nom.clone());
        }
    }
    rates
}

/// La bulle de fin du mode jeu
pub fn bulle_fin(rates: &[String]) -> (String, String) {
    (
        "Fin du mode jeu".into(),
        if rates.is_empty() {
            "Les applis fermées ont été relancées.".into()
        } else {
            format!("Relance toi-même : {}.", rates.join(", "))
        },
    )
}

/// Resume-GameModePending, partie « lecture » : les applis d'un fichier mode-jeu.json, seulement celles de la liste du
/// mode jeu, au nom attendu (le fichier n'ajoute rien à lancer) ; leurs arguments viennent de la liste.
pub fn reprise(j: &Value) -> Vec<Fermee> {
    let mut out = Vec::new();
    for e in liste(j.get("Closed")) {
        let Some(a) = app(&ch(e.get("Id"))) else { continue };
        let mut chemin = ch(e.get("Path"));
        if !chemin.is_empty()
            && !(enracine(&chemin) && egal(extension(&chemin), ".exe") && a.correspond(nom_sans_extension(&chemin)))
        {
            chemin.clear();
        }
        out.push(Fermee { id: a.id.into(), nom: a.nom.into(), chemin, args: a.args.map(str::to_string) });
    }
    out
}

// ---------------------------------------------------------------------------
// Alerte de température (Test-TempAlert)
// ---------------------------------------------------------------------------

/// La carte graphique très chaude : au moins 87 °C pendant 15 s, une alerte toutes les 10 minutes au plus
#[derive(Default, Debug)]
pub struct AlerteChaleur {
    depuis: Option<f64>,
    alertee: Option<f64>,
}

impl AlerteChaleur {
    /// Une mesure (`None` : pas de mesure, rien ne change) à l'instant `t` (en secondes) ; rend le texte de l'alerte
    pub fn tester(&mut self, temp: Option<f64>, t: f64) -> Option<String> {
        let temp = temp?;
        if temp < 87.0 {
            self.depuis = None;
            return None;
        }
        let Some(d) = self.depuis else {
            self.depuis = Some(t);
            return None;
        };
        if t - d < 15.0 {
            return None;
        }
        if self.alertee.is_some_and(|a| (t - a) / 60.0 < 10.0) {
            return None;
        }
        self.alertee = Some(t);
        Some(format!(
            "Elle est à {} °C depuis un moment. Vérifie que les ventilateurs tournent et que le PC respire (poussière, grilles bouchées).",
            ps_int(temp)
        ))
    }
    /// La carte chauffe : une mesure rapprochée est utile (alerte à l'heure)
    pub fn chauffe(&self) -> bool {
        self.depuis.is_some()
    }
}

/// Les noms des jeux à anti-triche EA à guetter avant leur lancement (Get-JeuxAntiTricheEA) ; `ea(exe)` : des fichiers
/// EAAntiCheat sont à côté du programme
pub fn jeux_anti_triche(jeux: &[Value], mut ea: impl FnMut(&str) -> bool) -> Vec<String> {
    let nom_fc = |b: &str| -> bool {
        let l = b.to_lowercase();
        let chiffres = |s: &str| s.len() == 2 && s.bytes().all(|c| c.is_ascii_digit());
        (l.starts_with("fc") && chiffres(&l[2..])) || (l.starts_with("fifa") && chiffres(&l[4..]))
    };
    let mut noms: Vec<String> = Vec::new();
    for g in jeux {
        let exes: Vec<String> = liste(g.get("exes")).iter().map(|e| ch(Some(e))).filter(|e| !e.is_empty()).collect();
        if exes.is_empty() {
            continue;
        }
        for e in &exes {
            if nom_fc(nom_sans_extension(e)) {
                noms.push(nom_fichier(e).to_lowercase());
            }
        }
        let base = nom_sans_extension(&exes[0]);
        if base.encode_utf16().count() >= 3 && !exe_generique(base) && (nom_fc(base) || ea(&exes[0])) {
            noms.push(nom_fichier(&exes[0]).to_lowercase());
        }
    }
    let mut out: Vec<String> = Vec::new();
    for n in noms {
        if nom_jeu_valable(&n) && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// `^[a-z0-9][a-z0-9 ._()-]{0,60}\.exe$` (la forme que l'Aide accepte)
pub fn nom_jeu_valable(n: &str) -> bool {
    let Some(d) = n.strip_suffix(".exe") else { return false };
    let mut c = d.chars();
    c.next().is_some_and(|x| x.is_ascii_lowercase() || x.is_ascii_digit())
        && d.chars().count() <= 61
        && c.all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || " ._()-".contains(x))
}

/// Refus de Windows (anti-triche) : message de PresentMon ou de l'Aide (Test-FpsRefus)
pub fn refus_mesure(err: &str) -> bool {
    let l = err.to_lowercase();
    l.contains("access denied") || l.contains("accès refusé") || l.contains("performance log users")
}

/// Le jeu de ce nom est-il connu par l'index ? (pour le raccourci Ctrl+Maj+F)
pub fn jeu_connu<'a>(idx: &'a HashMap<String, EntreeIndex>, nom: &str) -> Option<&'a str> {
    idx.get(&minuscules(nom)).map(|e| e.jeu.as_str())
}

/// Valeur vraie d'un réglage (`[bool](Get-Setting ...)`)
pub fn reglage_vrai(v: Option<&Value>, defaut: bool) -> bool {
    match v {
        None | Some(Value::Null) => defaut,
        Some(x) => vrai(Some(x)),
    }
}
