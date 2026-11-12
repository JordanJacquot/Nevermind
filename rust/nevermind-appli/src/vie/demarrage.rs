//! Le démarrage de Nevermind, dans l'ordre de Nevermind.ps1 : suite.json lu (`Read-Suite`), copie gérée ou non,
//! copie gérée lancée à la main (Mindset l'ouvre dans son onglet), dossier des données (`Resolve-DataDir`, avec le
//! renommage de l'ancien dossier OptiGame), copie bloquée (`Show-MindsetRequired`), une seule copie à la fois (les deux
//! verrous, `Send-ShowRequest`), vieilles demandes effacées, anciens programmes `.old` effacés.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use suite_commun::chemins::{chemin_complet, egal, feuille, joindre, parent};

use super::{PriseVerrou, Systeme};

/// Les deux verrous d'instance unique (Nevermind.ps1:84) : le sien, et celui de son ancien nom.
pub const VERROUS: [&str; 2] = [r"Local\Nevermind-Instance", r"Local\OptiGame-Instance"];
/// La demande d'affichage (Nevermind.ps1:284, `afficher_fichier` de apps.json).
pub const AFFICHER: &str = "afficher.demande";
/// La demande de fermeture de Mindset (Nevermind.ps1:286).
pub const QUITTER: &str = "quitter.demande";
/// La page de Nevermind dans Mindset (Nevermind.ps1:67).
pub const PAGE_NEVERMIND: &str = "appli:nevermind";
/// Le fichier où Mindset lit la page à ouvrir, à côté de suite.json.
pub const AFFICHER_PAGE: &str = "afficher-page.txt";
/// La page de Mindset, quand il n'est pas installé (Nevermind.ps1:171).
pub const MINDSET_PAGE: &str = "https://github.com/JordanJacquot/Mindset/releases/latest";
/// Les 5 thèmes de la suite (Nevermind.ps1:30).
pub const THEMES_SUITE: [&str; 5] = ["neon", "crepuscule", "terminal", "arcade", "rubis"];
/// L'attente d'une copie qui se ferme (Nevermind.ps1:303, :312, :99).
pub const ATTENTE: Duration = Duration::from_millis(5000);
/// Le pas des boucles d'attente (Nevermind.ps1:303 : 25 fois 200 ms).
pub const PAS_ATTENTE: Duration = Duration::from_millis(200);

/// Les noms des verrous (les vrais, ou ceux d'un essai : jamais les vrais dans un essai).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Noms {
    pub verrous: Vec<String>,
}

impl Noms {
    pub fn nevermind() -> Noms {
        Noms { verrous: VERROUS.iter().map(|s| s.to_string()).collect() }
    }
}

/// Les arguments (Nevermind.ps1:13). `-Demarrage` ou `/Demarrage`, sans tenir compte des majuscules ; `-Uninstall`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Arguments {
    pub demarrage: bool,
    pub desinstaller: bool,
}

fn option(a: &str, nom: &str) -> bool {
    a.strip_prefix('-').or_else(|| a.strip_prefix('/')).is_some_and(|n| n.eq_ignore_ascii_case(nom))
}

pub fn lire_arguments<I: IntoIterator<Item = String>>(arguments: I) -> Arguments {
    let mut r = Arguments::default();
    for a in arguments {
        r.demarrage |= option(&a, "Demarrage");
        r.desinstaller |= option(&a, "Uninstall");
    }
    r
}

/// Ce que Windows donne au démarrage.
#[derive(Debug, Clone, Default)]
pub struct Environnement {
    /// Le dossier du programme (celui de Mindset.exe : `Split-Path $PSScriptRoot -Parent`)
    pub racine: Option<String>,
    /// `%LOCALAPPDATA%`
    pub localappdata: String,
    /// `$env:ProgramFiles`, `$env:ProgramW6432`
    pub program_files: Option<String>,
    pub program_w6432: Option<String>,
    /// suite.json (`%LOCALAPPDATA%\Mindset\suite.json`)
    pub fichier_suite: PathBuf,
    /// La version (pour le journal)
    pub version: String,
    /// Le dossier des données d'un essai (construction d'essai seulement)
    pub donnees_essai: Option<PathBuf>,
    /// Les lettres des disques où chercher `<X>:\Mindset\Programmes\Mindset\Mindset.exe` (`Find-MindsetExe`)
    pub disques: Vec<char>,
}

/// Ce qu'on garde de suite.json (`Read-Suite`, Nevermind.ps1:41-50).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suite {
    /// Un des 5 thèmes (casse exacte, `-cin`), ou `pack:<id>` (`^pack:[a-z0-9-]{1,40}$`)
    pub theme: Option<String>,
    /// Le dossier noté pour Nevermind (`applis.nevermind`), en chemin complet sans « \ » final
    pub dossier: String,
}

/// `Get-FullDir` (Nevermind.ps1:33-36) : chemin complet sans « \ » final, vide s'il n'est pas valable.
pub fn dossier_complet(chemin: &str) -> String {
    let enracine = chemin.starts_with(['\\', '/']) || (chemin.len() >= 2 && chemin.as_bytes()[1] == b':');
    if chemin.is_empty() || !enracine {
        return String::new();
    }
    chemin_complet(chemin).map(|c| c.trim_end_matches('\\').to_string()).unwrap_or_default()
}

/// Une propriété d'un objet JSON sans tenir compte des majuscules (`$j.theme` de PowerShell).
fn champ<'a>(o: &'a serde_json::Map<String, Value>, nom: &str) -> Option<&'a Value> {
    o.get(nom).or_else(|| o.iter().find(|(k, _)| k.eq_ignore_ascii_case(nom)).map(|(_, v)| v))
}

/// Un thème de la suite valable : un des 5 (casse exacte) ou `pack:<id>`.
pub fn theme_suite_valable(t: &str) -> bool {
    if THEMES_SUITE.contains(&t) {
        return true;
    }
    t.strip_prefix("pack:").is_some_and(|id| {
        (1..=40).contains(&id.len()) && id.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'))
    })
}

/// `Read-Suite` : toute erreur (absent, pris, abîmé) donne une suite vide.
pub fn lire_suite(fichier: &Path) -> Suite {
    let mut s = Suite::default();
    let Ok(octets) = std::fs::read(fichier) else { return s };
    let texte = String::from_utf8_lossy(&octets);
    let Ok(Value::Object(j)) = serde_json::from_str::<Value>(texte.trim_start_matches('\u{feff}')) else { return s };
    if let Some(Value::String(t)) = champ(&j, "theme")
        && theme_suite_valable(t)
    {
        s.theme = Some(t.clone());
    }
    if let Some(Value::Object(a)) = champ(&j, "applis")
        && let Some(Value::String(d)) = champ(a, "nevermind")
    {
        s.dossier = dossier_complet(d);
    }
    s
}

/// Ce que le démarrage a trouvé, quand l'appli continue.
#[derive(Debug, Clone)]
pub struct Lancement {
    pub arguments: Arguments,
    /// Gérée par Mindset (Nevermind.ps1:54)
    pub gere: bool,
    /// Le dossier de cette copie (celui de Mindset.exe)
    pub racine: Option<String>,
    pub suite: Suite,
    pub fichier_suite: PathBuf,
    /// `%LOCALAPPDATA%\Mindset` (le dossier de suite.json : afficher-page.txt)
    pub dossier_mindset: PathBuf,
    /// `$DataDir`
    pub dossier_donnees: PathBuf,
    /// La ligne du journal sur le dossier des données (`$DataDirNote`), s'il y en a une
    pub note_donnees: Option<String>,
    /// Le programme qui relance Nevermind (Mindset.exe du dossier, sinon Nevermind.exe)
    pub programme: Option<PathBuf>,
}

/// Comment le démarrage finit.
#[derive(Debug, Clone)]
pub enum Depart {
    /// Copie que Mindset ne gère pas, ni dossier de travail : le message, puis rien d'autre
    Bloquee,
    /// Copie gérée lancée à la main : Mindset l'ouvre dans son onglet
    OuverteDansMindset,
    /// Une autre copie tourne : elle a reçu la demande d'affichage (ou c'était un lancement caché)
    DejaOuverte,
    /// `-Uninstall` : la désinstallation n'est pas faite par ce programme (Désinstaller Nevermind.exe)
    Desinstallation,
    /// L'appli démarre : verrous pris, dossier des données prêt
    Lancer(Box<Lancement>),
}

/// Le journal d'avant le chargement (`Write-LaunchNote`, Nevermind.ps1:77-79).
pub type NoteLancement<'a> = &'a mut dyn FnMut(&Path, &str);

/// `Open-DansMindset` (Nevermind.ps1:60-71) : `afficher-page.txt` puis Mindset lancé par l'Explorateur. `false` si
/// Mindset est introuvable ou si quelque chose a échoué.
pub fn ouvrir_dans_mindset(dossier_mindset: &Path, systeme: &mut dyn Systeme) -> bool {
    let Some(brut) = systeme.emplacement_mindset() else { return false };
    let d = brut.trim().trim_matches('"').to_string();
    if d.is_empty() || !(d.starts_with('\\') || (d.len() >= 2 && d.as_bytes()[1] == b':')) {
        return false;
    }
    let exe = PathBuf::from(joindre(&d, "Mindset.exe"));
    if !exe.is_file() {
        return false;
    }
    if std::fs::create_dir_all(dossier_mindset).is_err() {
        return false;
    }
    if std::fs::write(dossier_mindset.join(AFFICHER_PAGE), PAGE_NEVERMIND.as_bytes()).is_err() {
        return false;
    }
    systeme.ouvrir_par_explorateur(&exe.to_string_lossy()).is_ok()
}

/// `Find-MindsetExe` (Nevermind.ps1:174-190) : celui de son installation, sinon Program Files, sinon
/// `X:\Mindset\Programmes\Mindset\Mindset.exe` sur un disque. `None` s'il n'existe nulle part.
pub fn trouver_mindset(env: &Environnement, systeme: &mut dyn Systeme) -> Option<PathBuf> {
    if let Some(d) = systeme.emplacement_mindset().filter(|d| !d.is_empty()) {
        let e = PathBuf::from(joindre(&d, "Mindset.exe"));
        if e.exists() {
            return Some(e);
        }
    }
    let pf = env.program_w6432.clone().or_else(|| env.program_files.clone()).unwrap_or_default();
    let e = PathBuf::from(joindre(&pf, r"Mindset\Mindset\Mindset.exe"));
    if !pf.is_empty() && e.exists() {
        return Some(e);
    }
    env.disques
        .iter()
        .map(|l| PathBuf::from(format!(r"{l}:\Mindset\Programmes\Mindset\Mindset.exe")))
        .find(|e| e.is_file())
}

/// `Test-WorkCopy` (Nevermind.ps1:193-197) : un `.git` dans le dossier de la copie ou juste au-dessus.
fn copie_de_travail(racine: Option<&str>) -> bool {
    let Some(r) = racine else { return false };
    [r, parent(r)].iter().any(|d| !d.is_empty() && Path::new(&joindre(d, ".git")).exists())
}

/// `Get-ManagedDataDir` (Nevermind.ps1:128-133) : `<X>:\Mindset\Programmes\Nevermind` donne
/// `<X>:\Mindset\Donnees\Nevermind`, sinon `%LOCALAPPDATA%\Mindset\Nevermind`.
pub fn dossier_donnees_gere(racine: Option<&str>, localappdata: &str) -> PathBuf {
    if let Some(r) = racine {
        let p = parent(r);
        let m = parent(p);
        if !m.is_empty() && egal(feuille(p), "Programmes") && egal(feuille(m), "Mindset") {
            return PathBuf::from(joindre(m, r"Donnees\Nevermind"));
        }
    }
    PathBuf::from(joindre(localappdata, r"Mindset\Nevermind"))
}

/// La racine d'un chemin (`[IO.Path]::GetPathRoot`), en majuscules pour comparer.
fn racine_disque(p: &Path) -> String {
    p.components().next().map(|c| c.as_os_str().to_string_lossy().to_uppercase()).unwrap_or_default()
}

/// `Test-InstanceRunning` (Nevermind.ps1:85-94).
pub fn instance_ouverte(noms: &Noms, systeme: &mut dyn Systeme) -> bool {
    noms.verrous.iter().any(|n| systeme.verrou_existe(n))
}

/// `Resolve-DataDir` (Nevermind.ps1:134-154) : rend le dossier et la ligne du journal.
fn resoudre_donnees(
    gere: bool,
    desinstaller: bool,
    env: &Environnement,
    noms: &Noms,
    systeme: &mut dyn Systeme,
) -> (PathBuf, Option<String>) {
    let ancien = PathBuf::from(joindre(&env.localappdata, "OptiGame"));
    if !gere {
        return (ancien, None);
    }
    let nouveau = dossier_donnees_gere(env.racine.as_deref(), &env.localappdata);
    if nouveau.is_dir() {
        return (nouveau, None);
    }
    // (12/11, essai réel cas 8) avant de regarder si l'ancien dossier existe : une copie ouverte sans nouveau dossier
    // se sert de l'ancien, même si elle ne l'a pas encore créé (Nevermind 1.11.0 tout juste lancé). Le nouveau créé
    // ici par la note d'un lancement abandonné empêcherait le déplacement des données au lancement suivant.
    if desinstaller || instance_ouverte(noms, systeme) {
        return (ancien, None);
    }
    if !ancien.is_dir() {
        return (nouveau, None);
    }
    let (a, n) = (ancien.display(), nouveau.display());
    if racine_disque(&nouveau) != racine_disque(&ancien) {
        let note = format!(
            "Données : {a} est sur un autre disque que {n}, rien n'est copié (Mindset s'en charge), l'ancien dossier est gardé pour cette fois"
        );
        return (ancien, Some(note));
    }
    let r = nouveau.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::rename(&ancien, &nouveau));
    match r {
        Ok(()) => (nouveau.clone(), Some(format!("Données déplacées : {a} -> {n} (Nevermind est géré par Mindset)"))),
        Err(e) => (
            ancien.clone(),
            Some(format!(
                "Données : {a} n'a pas pu être renommé en {n}, l'ancien dossier est gardé pour cette fois ({e})"
            )),
        ),
    }
}

/// `Send-ShowRequest` (Nevermind.ps1:287-295) : le numéro de ce processus dans afficher.demande.
fn demander_affichage(dossier: &Path, systeme: &mut dyn Systeme) {
    let _ = std::fs::create_dir_all(dossier);
    let _ = std::fs::write(dossier.join(AFFICHER), std::process::id().to_string());
    systeme.permettre_premier_plan();
}

/// Le démarrage de Nevermind (Nevermind.ps1:52-370). `note` : le journal d'avant le chargement (`Write-LaunchNote`).
pub fn demarrer(
    arguments: Arguments,
    env: &Environnement,
    noms: &Noms,
    systeme: &mut dyn Systeme,
    note: NoteLancement,
) -> Depart {
    let demarrage = arguments.demarrage;
    // Nevermind.ps1:53-54
    let suite = lire_suite(&env.fichier_suite);
    let racine = env.racine.as_deref().map(dossier_complet).filter(|r| !r.is_empty());
    let gere = !suite.dossier.is_empty() && racine.as_deref().is_some_and(|r| egal(r, &suite.dossier));
    let dossier_mindset = env.fichier_suite.parent().map(Path::to_path_buf).unwrap_or_default();
    // Nevermind.ps1:72 (la construction d'essai ne lit pas HKLM : Mindset y est toujours introuvable)
    if gere && !demarrage && !arguments.desinstaller && ouvrir_dans_mindset(&dossier_mindset, systeme) {
        return Depart::OuverteDansMindset;
    }
    // Nevermind.ps1:155
    let (dossier, note_donnees) = match &env.donnees_essai {
        Some(d) => (d.clone(), None),
        None => resoudre_donnees(gere, arguments.desinstaller, env, noms, systeme),
    };
    // Nevermind.ps1:274-277
    if !gere && !arguments.desinstaller && !copie_de_travail(racine.as_deref()) {
        let exe = trouver_mindset(env, systeme);
        if systeme.demander_mindset(exe.is_some()) {
            let cible = exe.map_or(MINDSET_PAGE.to_string(), |e| e.to_string_lossy().into_owned());
            let _ = systeme.ouvrir_par_explorateur(&cible);
        }
        return Depart::Bloquee;
    }
    if arguments.desinstaller {
        return Depart::Desinstallation;
    }
    let afficher = dossier.join(AFFICHER);
    // Nevermind.ps1:296-319
    if instance_ouverte(noms, systeme) {
        if demarrage {
            if !attendre(systeme, || false, noms) {
                note(&dossier, "Lancement caché abandonné : un autre Nevermind est encore ouvert après 5 s");
                return Depart::DejaOuverte;
            }
        } else {
            demander_affichage(&dossier, systeme);
            let a = afficher.clone();
            if !attendre(systeme, move || !a.is_file(), noms) {
                return Depart::DejaOuverte;
            }
            let _ = std::fs::remove_file(&afficher);
        }
    }
    // Nevermind.ps1:355-366
    for nom in &noms.verrous {
        match systeme.prendre_verrou(nom, ATTENTE) {
            PriseVerrou::Pris => {}
            PriseVerrou::Refuse => {
                systeme.liberer_verrous();
                if !demarrage {
                    demander_affichage(&dossier, systeme);
                }
                return Depart::DejaOuverte;
            }
            PriseVerrou::Occupe => {
                systeme.liberer_verrous();
                if !demarrage {
                    demander_affichage(&dossier, systeme);
                } else {
                    note(&dossier, "Lancement caché abandonné : la place n'a pas été libérée en 5 s");
                }
                return Depart::DejaOuverte;
            }
        }
    }
    let _ = std::fs::create_dir_all(&dossier);
    let _ = std::fs::remove_file(&afficher);
    let _ = std::fs::remove_file(dossier.join(QUITTER));
    // Nevermind.ps1:370 : les anciens programmes mis de côté par la dernière mise à jour
    if let Some(r) = racine.as_deref()
        && let Ok(l) = std::fs::read_dir(r)
    {
        for f in l.flatten() {
            let n = f.file_name().to_string_lossy().to_lowercase();
            if (n.ends_with(".exe.old") || n.ends_with(".dll.old")) && f.path().is_file() {
                let _ = std::fs::remove_file(f.path());
            }
        }
    }
    // Nevermind.ps1:374-379 : la marque « téléchargé depuis Internet » retirée des programmes de Nevermind
    if let Some(r) = racine.as_deref() {
        for n in ["Mindset.exe", "Nevermind.exe", "NevermindAide.exe", "Désinstaller Nevermind.exe", "LISEZMOI.txt"] {
            let _ = std::fs::remove_file(format!("{}:Zone.Identifier", joindre(r, n)));
        }
    }
    let programme = racine.as_deref().map(|r| {
        let e = PathBuf::from(joindre(r, "Mindset.exe"));
        if e.is_file() { e } else { PathBuf::from(joindre(r, "Nevermind.exe")) }
    });
    Depart::Lancer(Box::new(Lancement {
        arguments,
        gere,
        racine,
        suite,
        fichier_suite: env.fichier_suite.clone(),
        dossier_mindset,
        dossier_donnees: dossier,
        note_donnees,
        programme,
    }))
}

/// Les boucles d'attente du démarrage (Nevermind.ps1:303, :312-315) : 25 fois 200 ms au plus. `true` si l'autre
/// copie a disparu ; `fini` arrête l'attente plus tôt (la demande d'affichage prise par l'autre copie : `false`).
fn attendre(systeme: &mut dyn Systeme, fini: impl Fn() -> bool, noms: &Noms) -> bool {
    for _ in 0..(ATTENTE.as_millis() / PAS_ATTENTE.as_millis()) {
        if fini() {
            return false;
        }
        systeme.dormir(PAS_ATTENTE);
        if !instance_ouverte(noms, systeme) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::io;

    /// Un faux Windows : des verrous tenus ou non, l'emplacement de Mindset, et ce qui a été lancé.
    #[derive(Default)]
    struct Faux {
        tenus: HashSet<String>,
        refuses: HashSet<String>,
        pris: Vec<String>,
        /// Après tant d'attentes, l'autre copie a disparu
        disparait_apres: Option<u32>,
        attentes: u32,
        mindset: Option<String>,
        lances: Vec<String>,
        premier_plan: u32,
        demande: bool,
        reponse: bool,
        ouvertures: HashMap<String, u32>,
    }
    impl Systeme for Faux {
        fn verrou_existe(&mut self, nom: &str) -> bool {
            self.tenus.contains(nom) || self.refuses.contains(nom)
        }
        fn prendre_verrou(&mut self, nom: &str, _: Duration) -> PriseVerrou {
            if self.refuses.contains(nom) {
                PriseVerrou::Refuse
            } else if self.tenus.contains(nom) {
                PriseVerrou::Occupe
            } else {
                self.pris.push(nom.into());
                PriseVerrou::Pris
            }
        }
        fn liberer_verrous(&mut self) {
            self.pris.clear();
        }
        fn emplacement_mindset(&mut self) -> Option<String> {
            self.mindset.clone()
        }
        fn lancer(&mut self, exe: &Path, _: &[&str], _: Option<&Path>) -> io::Result<()> {
            self.lances.push(exe.display().to_string());
            Ok(())
        }
        fn ouvrir_par_explorateur(&mut self, cible: &str) -> io::Result<()> {
            *self.ouvertures.entry(cible.into()).or_default() += 1;
            Ok(())
        }
        fn permettre_premier_plan(&mut self) {
            self.premier_plan += 1;
        }
        fn lever_signal(&mut self, _: &str) {
            panic!("aucun signal au démarrage");
        }
        fn dormir(&mut self, _: Duration) {
            self.attentes += 1;
            if self.disparait_apres.is_some_and(|n| self.attentes >= n) {
                self.tenus.clear();
            }
        }
        fn demander_mindset(&mut self, _: bool) -> bool {
            self.demande = true;
            self.reponse
        }
    }

    /// Un dossier d'essai : <essai>\Local (le faux %LOCALAPPDATA%) et <essai>\Mindset\Programmes\Nevermind.
    struct Essai {
        racine: PathBuf,
    }
    impl Essai {
        fn nouveau(nom: &str) -> Essai {
            let racine = nevermind_noyau::dossier_essai(&format!("hote-demarrage-{nom}"));
            let _ = std::fs::remove_dir_all(&racine);
            std::fs::create_dir_all(racine.join(r"Local\Mindset")).unwrap();
            std::fs::create_dir_all(racine.join(r"Mindset\Programmes\Nevermind")).unwrap();
            Essai { racine }
        }
        fn programme(&self) -> String {
            self.racine.join(r"Mindset\Programmes\Nevermind").display().to_string()
        }
        fn env(&self) -> Environnement {
            Environnement {
                racine: Some(self.programme()),
                localappdata: self.racine.join("Local").display().to_string(),
                fichier_suite: self.racine.join(r"Local\Mindset\suite.json"),
                version: "1.15.6".into(),
                ..Default::default()
            }
        }
        fn gerer(&self, theme: &str) {
            let j = serde_json::json!({ "theme": theme, "applis": { "nevermind": format!("{}\\", self.programme()) } });
            std::fs::write(self.racine.join(r"Local\Mindset\suite.json"), j.to_string()).unwrap();
        }
    }
    impl Drop for Essai {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.racine);
        }
    }

    fn sans_note() -> impl FnMut(&Path, &str) {
        |_: &Path, t: &str| panic!("note inattendue : {t}")
    }

    #[test]
    fn arguments_comme_le_lanceur() {
        let a = |l: &[&str]| lire_arguments(l.iter().map(|s| s.to_string()));
        assert_eq!(a(&["-Demarrage"]), Arguments { demarrage: true, desinstaller: false });
        assert!(a(&["/demarrage"]).demarrage);
        assert_eq!(a(&["-DEMARRAGE", "-Uninstall"]), Arguments { demarrage: true, desinstaller: true });
        assert_eq!(a(&["Demarrage", "--Demarrage", "-Demarrage2"]), Arguments::default());
    }

    #[test]
    fn suite_json_lu_comme_read_suite() {
        let e = Essai::nouveau("suite");
        let f = e.racine.join("s.json");
        assert_eq!(lire_suite(&f), Suite::default(), "absent");
        for (texte, theme) in [
            (r#"{"theme":"rubis"}"#, Some("rubis")),
            (r#"{"Theme":"terminal"}"#, Some("terminal")),
            (r#"{"theme":"Rubis"}"#, None),
            (r#"{"theme":"pack:mon-pack-2"}"#, Some("pack:mon-pack-2")),
            (r#"{"theme":"pack:Mon"}"#, None),
            (r#"{"theme":"pack:../x"}"#, None),
            (r#"{"theme":3}"#, None),
            ("\u{feff}{\"theme\":\"arcade\"}", Some("arcade")),
            ("{abîmé", None),
        ] {
            std::fs::write(&f, texte).unwrap();
            assert_eq!(lire_suite(&f).theme.as_deref(), theme, "{texte}");
        }
        std::fs::write(&f, r#"{"applis":{"nevermind":"C:\\Mindset\\Programmes\\Nevermind\\"}}"#).unwrap();
        assert_eq!(lire_suite(&f).dossier, r"C:\Mindset\Programmes\Nevermind");
        std::fs::write(&f, r#"{"applis":{"nevermind":"relatif\\x"}}"#).unwrap();
        assert_eq!(lire_suite(&f).dossier, "");
    }

    #[test]
    fn dossier_des_donnees_d_une_copie_geree() {
        assert_eq!(
            dossier_donnees_gere(Some(r"D:\Mindset\Programmes\Nevermind"), r"C:\U\AppData\Local"),
            PathBuf::from(r"D:\Mindset\Donnees\Nevermind")
        );
        assert_eq!(
            dossier_donnees_gere(Some(r"C:\Program Files\Mindset\Nevermind"), r"C:\U\AppData\Local"),
            PathBuf::from(r"C:\U\AppData\Local\Mindset\Nevermind")
        );
        assert_eq!(dossier_donnees_gere(None, r"C:\L"), PathBuf::from(r"C:\L\Mindset\Nevermind"));
    }

    #[test]
    fn copie_bloquee_sans_mindset_ni_git() {
        let e = Essai::nouveau("bloquee");
        let mut s = Faux { reponse: true, ..Default::default() };
        let d = demarrer(Arguments::default(), &e.env(), &Noms::nevermind(), &mut s, &mut sans_note());
        assert!(matches!(d, Depart::Bloquee));
        assert!(s.demande && s.pris.is_empty(), "le message, et aucun verrou");
        assert_eq!(s.ouvertures.get(MINDSET_PAGE), Some(&1), "Télécharger Mindset : sa page");
    }

    #[test]
    fn copie_de_travail_demarre_avec_l_ancien_dossier() {
        let e = Essai::nouveau("travail");
        std::fs::create_dir_all(e.racine.join(r"Mindset\Programmes\.git")).unwrap();
        let mut s = Faux::default();
        let Depart::Lancer(l) = demarrer(Arguments::default(), &e.env(), &Noms::nevermind(), &mut s, &mut sans_note())
        else {
            panic!("la copie de travail démarre")
        };
        assert!(!l.gere);
        assert_eq!(l.dossier_donnees, e.racine.join(r"Local\OptiGame"));
        assert_eq!(s.pris, VERROUS.to_vec());
    }

    #[test]
    fn copie_geree_lancee_a_la_main_s_ouvre_dans_mindset() {
        let e = Essai::nouveau("a-la-main");
        e.gerer("rubis");
        let ms = e.racine.join("MindsetInstalle");
        std::fs::create_dir_all(&ms).unwrap();
        std::fs::write(ms.join("Mindset.exe"), "x").unwrap();
        let mut s = Faux { mindset: Some(format!("\"{}\" ", ms.display())), ..Default::default() };
        let d = demarrer(Arguments::default(), &e.env(), &Noms::nevermind(), &mut s, &mut sans_note());
        assert!(matches!(d, Depart::OuverteDansMindset));
        assert_eq!(
            std::fs::read_to_string(e.racine.join(r"Local\Mindset\afficher-page.txt")).unwrap(),
            "appli:nevermind"
        );
        assert_eq!(s.ouvertures.get(&ms.join("Mindset.exe").display().to_string()), Some(&1));
        // Lancée cachée (par Mindset) : elle démarre, données dans <X>:\Mindset\Donnees\Nevermind
        let mut s = Faux { mindset: Some(ms.display().to_string()), ..Default::default() };
        let Depart::Lancer(l) = demarrer(
            Arguments { demarrage: true, ..Default::default() },
            &e.env(),
            &Noms::nevermind(),
            &mut s,
            &mut sans_note(),
        ) else {
            panic!("lancée cachée, elle démarre")
        };
        assert!(l.gere);
        assert_eq!(l.suite.theme.as_deref(), Some("rubis"));
        assert_eq!(l.dossier_donnees, e.racine.join(r"Mindset\Donnees\Nevermind"));
        assert!(s.ouvertures.is_empty());
    }

    #[test]
    fn ancien_dossier_renomme_une_fois() {
        let e = Essai::nouveau("renomme");
        e.gerer("neon");
        let ancien = e.racine.join(r"Local\OptiGame");
        std::fs::create_dir_all(&ancien).unwrap();
        std::fs::write(ancien.join("parametres.json"), "{}").unwrap();
        let mut s = Faux::default();
        let Depart::Lancer(l) = demarrer(
            Arguments { demarrage: true, ..Default::default() },
            &e.env(),
            &Noms::nevermind(),
            &mut s,
            &mut sans_note(),
        ) else {
            panic!()
        };
        assert_eq!(l.dossier_donnees, e.racine.join(r"Mindset\Donnees\Nevermind"));
        assert!(l.dossier_donnees.join("parametres.json").is_file() && !ancien.exists());
        assert!(l.note_donnees.unwrap().starts_with("Données déplacées : "));
    }

    /// (12/11, essai réel cas 8) Nevermind 1.11.0 tout juste lancé : il tient son verrou mais n'a pas encore créé
    /// %LOCALAPPDATA%\OptiGame. Le lancement caché abandonné note dans l'ancien dossier (celui de la copie ouverte),
    /// sans créer le nouveau : sinon, au lancement suivant, le nouveau dossier existe et les données d'OptiGame ne
    /// sont jamais déplacées.
    #[test]
    fn copie_ouverte_sans_dossier_encore_la_note_va_dans_l_ancien() {
        let e = Essai::nouveau("ouverte-sans-dossier");
        e.gerer("neon");
        let ancien = e.racine.join(r"Local\OptiGame");
        let nouveau = e.racine.join(r"Mindset\Donnees\Nevermind");
        let mut s = Faux::default();
        s.tenus.insert(VERROUS[1].into());
        // (comme note_lancement de programme.rs : le dossier créé, puis la ligne ajoutée)
        let mut note = |d: &Path, t: &str| {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("journal.txt"), t).unwrap();
        };
        let d = demarrer(
            Arguments { demarrage: true, ..Default::default() },
            &e.env(),
            &Noms::nevermind(),
            &mut s,
            &mut note,
        );
        assert!(matches!(d, Depart::DejaOuverte));
        assert!(ancien.join("journal.txt").is_file(), "la note est dans l'ancien dossier");
        assert!(!nouveau.exists(), "le nouveau dossier n'est pas créé");
        // 1.11.0 écrit ses données puis se ferme : le lancement suivant les déplace
        std::fs::write(ancien.join("parametres.json"), "{}").unwrap();
        let mut s = Faux::default();
        let Depart::Lancer(l) = demarrer(
            Arguments { demarrage: true, ..Default::default() },
            &e.env(),
            &Noms::nevermind(),
            &mut s,
            &mut sans_note(),
        ) else {
            panic!()
        };
        assert_eq!(l.dossier_donnees, nouveau);
        assert!(nouveau.join("parametres.json").is_file() && !ancien.exists(), "données déplacées");
    }

    #[test]
    fn second_lancement_demande_l_affichage() {
        let e = Essai::nouveau("second");
        std::fs::create_dir_all(e.racine.join(r"Mindset\Programmes\.git")).unwrap();
        let mut s = Faux::default();
        s.tenus.insert(VERROUS[0].into());
        let d = demarrer(Arguments::default(), &e.env(), &Noms::nevermind(), &mut s, &mut sans_note());
        assert!(matches!(d, Depart::DejaOuverte));
        let demande = e.racine.join(r"Local\OptiGame\afficher.demande");
        assert_eq!(std::fs::read_to_string(&demande).unwrap(), std::process::id().to_string());
        assert_eq!((s.premier_plan, s.attentes), (1, 25), "5 s d'attente, la demande n'est pas prise");
        // L'ancienne version disparaît pendant l'attente : ce lancement continue, la demande effacée
        let mut s = Faux { disparait_apres: Some(3), ..Default::default() };
        s.tenus.insert(VERROUS[1].into());
        let d = demarrer(Arguments::default(), &e.env(), &Noms::nevermind(), &mut s, &mut sans_note());
        assert!(matches!(d, Depart::Lancer(_)));
        assert!(!demande.exists());
    }

    #[test]
    fn lancement_cache_abandonne_si_la_place_reste_prise() {
        let e = Essai::nouveau("cache");
        std::fs::create_dir_all(e.racine.join(r"Mindset\Programmes\.git")).unwrap();
        let mut s = Faux::default();
        s.tenus.insert(VERROUS[0].into());
        let mut notes = Vec::new();
        let d = demarrer(
            Arguments { demarrage: true, ..Default::default() },
            &e.env(),
            &Noms::nevermind(),
            &mut s,
            &mut |_: &Path, t: &str| notes.push(t.to_string()),
        );
        assert!(matches!(d, Depart::DejaOuverte));
        assert_eq!(
            notes,
            vec!["Lancement caché abandonné : un autre Nevermind est encore ouvert après 5 s".to_string()]
        );
        assert!(!e.racine.join(r"Local\OptiGame\afficher.demande").exists(), "caché : aucune demande d'affichage");
        // Verrou d'une copie administrateur : refus, une demande d'affichage sans lancement caché
        let mut s = Faux::default();
        s.refuses.insert(VERROUS[1].into());
        s.disparait_apres = Some(0);
        let d = demarrer(
            Arguments::default(),
            &e.env(),
            &Noms { verrous: vec!["a".into(), VERROUS[1].into()] },
            &mut s,
            &mut sans_note(),
        );
        assert!(matches!(d, Depart::DejaOuverte));
        assert!(s.pris.is_empty(), "aucun verrou gardé");
    }

    #[test]
    fn vieilles_demandes_et_anciens_programmes_effaces() {
        let e = Essai::nouveau("menage");
        std::fs::create_dir_all(e.racine.join(r"Mindset\Programmes\.git")).unwrap();
        let d = e.racine.join(r"Local\OptiGame");
        std::fs::create_dir_all(&d).unwrap();
        for f in [AFFICHER, QUITTER] {
            std::fs::write(d.join(f), "1").unwrap();
        }
        let p = PathBuf::from(e.programme());
        for f in ["Mindset.exe.old", "NevermindPont.dll.old", "garder.txt"] {
            std::fs::write(p.join(f), "x").unwrap();
        }
        let mut s = Faux::default();
        assert!(matches!(
            demarrer(Arguments::default(), &e.env(), &Noms::nevermind(), &mut s, &mut sans_note()),
            Depart::Lancer(_)
        ));
        assert!(!d.join(AFFICHER).exists() && !d.join(QUITTER).exists());
        assert!(!p.join("Mindset.exe.old").exists() && !p.join("NevermindPont.dll.old").exists());
        assert!(p.join("garder.txt").exists());
    }
}
