//! Ce que le bloc demande à Windows : des LECTURES (registre, variables, disques, applis du menu Démarrer, heure) et
//! trois gestes de l'utilisateur (lancer un jeu, choisir un programme, lire le nom d'un programme). Tout passe par ce
//! trait : les tests donnent un faux Windows (`SystemeEssai`), jamais le vrai PC.

use crate::texte::Moment;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ruche {
    Utilisateur,
    Machine,
    Classes,
}

pub trait Systeme: Send + Sync {
    /// Une valeur texte du registre (REG_SZ, REG_EXPAND_SZ développée comme Get-ItemProperty), `None` si absente.
    fn reg_texte(&self, ruche: Ruche, cle: &str, nom: &str) -> Option<String>;
    /// Une valeur DWORD du registre.
    fn reg_nombre(&self, ruche: Ruche, cle: &str, nom: &str) -> Option<u32>;
    /// Les sous-clés d'une clé (vide si la clé n'existe pas).
    fn reg_sous_cles(&self, ruche: Ruche, cle: &str) -> Vec<String>;
    /// Une variable d'environnement (ProgramData, APPDATA, TEMP...).
    fn variable(&self, nom: &str) -> Option<String>;
    /// Les disques fixes prêts (« C:\ », « D:\ »).
    fn disques_fixes(&self) -> Vec<String>;
    /// Les applis du menu Démarrer : (nom, identifiant), comme Get-StartApps.
    fn applis_demarrer(&self) -> Vec<(String, String)>;
    /// L'heure locale.
    fn maintenant(&self) -> Moment;
    /// Lance un programme dans son dossier (un jeu ou un désinstalleur « exe ») ; s'il demande les droits, Windows
    /// pose sa question (et ce lancement attend la réponse : il est fait hors du fil des blocs). `Err` : le texte à
    /// montrer. Le vrai Windows n'est jamais appelé par les tests.
    fn lancer(&self, exe: &str, arguments: &str, dossier: &str) -> Result<(), String>;
    /// Un faux Windows (tests) : ses lancements sont faits même en mode test, puisqu'ils ne lancent rien.
    fn faux(&self) -> bool {
        false
    }
    /// Demande à l'utilisateur le programme d'un jeu (.exe) ; `None` s'il annule.
    fn choisir_exe(&self) -> Option<String>;
    /// Le nom de produit et la description d'un programme (FileVersionInfo).
    fn infos_programme(&self, exe: &str) -> (String, String);
}

/// Un faux Windows pour les tests : tout est donné à l'avance, rien ne touche le PC. Les gestes sont notés.
#[derive(Default)]
pub struct SystemeEssai {
    pub textes: HashMap<(Ruche, String, String), String>,
    pub nombres: HashMap<(Ruche, String, String), u32>,
    pub cles: HashMap<(Ruche, String), Vec<String>>,
    pub variables: HashMap<String, String>,
    pub disques: Vec<String>,
    pub applis: Vec<(String, String)>,
    pub heure: Option<Moment>,
    pub exe_choisi: Option<String>,
    pub infos: HashMap<String, (String, String)>,
    pub lances: Mutex<Vec<(String, String, String)>>,
    /// Un lancement refusé (par exemple « tu as répondu Non à la demande de Windows ») : le texte de l'erreur
    pub refus_lancement: Option<String>,
}

impl SystemeEssai {
    pub fn texte(&mut self, ruche: Ruche, cle: &str, nom: &str, valeur: &str) {
        self.textes.insert((ruche, cle.to_lowercase(), nom.to_lowercase()), valeur.to_string());
        self.ajouter_cle(ruche, cle);
    }
    pub fn nombre(&mut self, ruche: Ruche, cle: &str, nom: &str, valeur: u32) {
        self.nombres.insert((ruche, cle.to_lowercase(), nom.to_lowercase()), valeur);
        self.ajouter_cle(ruche, cle);
    }
    /// Déclare une clé (et la range dans les sous-clés de sa clé parente).
    pub fn ajouter_cle(&mut self, ruche: Ruche, cle: &str) {
        if let Some(i) = cle.rfind('\\') {
            let (parent, nom) = (&cle[..i], &cle[i + 1..]);
            let l = self.cles.entry((ruche, parent.to_lowercase())).or_default();
            if !l.iter().any(|x| x.eq_ignore_ascii_case(nom)) {
                l.push(nom.to_string());
            }
        }
    }
}

impl Systeme for SystemeEssai {
    fn reg_texte(&self, ruche: Ruche, cle: &str, nom: &str) -> Option<String> {
        self.textes.get(&(ruche, cle.to_lowercase(), nom.to_lowercase())).cloned()
    }
    fn reg_nombre(&self, ruche: Ruche, cle: &str, nom: &str) -> Option<u32> {
        self.nombres.get(&(ruche, cle.to_lowercase(), nom.to_lowercase())).copied()
    }
    fn reg_sous_cles(&self, ruche: Ruche, cle: &str) -> Vec<String> {
        self.cles.get(&(ruche, cle.to_lowercase())).cloned().unwrap_or_default()
    }
    fn variable(&self, nom: &str) -> Option<String> {
        self.variables.get(&nom.to_lowercase()).cloned()
    }
    fn disques_fixes(&self) -> Vec<String> {
        self.disques.clone()
    }
    fn applis_demarrer(&self) -> Vec<(String, String)> {
        self.applis.clone()
    }
    fn maintenant(&self) -> Moment {
        self.heure.unwrap_or(Moment::depuis(2026, 11, 6, 15, 0, 0))
    }
    fn lancer(&self, exe: &str, arguments: &str, dossier: &str) -> Result<(), String> {
        self.lances.lock().unwrap_or_else(|e| e.into_inner()).push((exe.into(), arguments.into(), dossier.into()));
        match &self.refus_lancement {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
    fn faux(&self) -> bool {
        true
    }
    fn choisir_exe(&self) -> Option<String> {
        self.exe_choisi.clone()
    }
    fn infos_programme(&self, exe: &str) -> (String, String) {
        self.infos.get(&exe.to_lowercase()).cloned().unwrap_or_default()
    }
}

/// Le vrai Windows (lectures seulement, et les trois gestes de l'utilisateur).
#[derive(Default)]
pub struct SystemeWindows;

#[cfg(windows)]
impl Systeme for SystemeWindows {
    fn reg_texte(&self, ruche: Ruche, cle: &str, nom: &str) -> Option<String> {
        crate::windows::reg_texte(ruche, cle, nom)
    }
    fn reg_nombre(&self, ruche: Ruche, cle: &str, nom: &str) -> Option<u32> {
        crate::windows::reg_nombre(ruche, cle, nom)
    }
    fn reg_sous_cles(&self, ruche: Ruche, cle: &str) -> Vec<String> {
        crate::windows::reg_sous_cles(ruche, cle)
    }
    fn variable(&self, nom: &str) -> Option<String> {
        std::env::var(nom).ok().filter(|v| !v.is_empty())
    }
    fn disques_fixes(&self) -> Vec<String> {
        crate::windows::disques_fixes()
    }
    fn applis_demarrer(&self) -> Vec<(String, String)> {
        crate::windows::applis_demarrer()
    }
    fn maintenant(&self) -> Moment {
        crate::windows::heure_locale()
    }
    fn lancer(&self, exe: &str, arguments: &str, dossier: &str) -> Result<(), String> {
        crate::windows::lancer(exe, arguments, dossier)
    }
    fn choisir_exe(&self) -> Option<String> {
        crate::windows::choisir_exe()
    }
    fn infos_programme(&self, exe: &str) -> (String, String) {
        crate::windows::infos_programme(exe)
    }
}

#[cfg(not(windows))]
impl Systeme for SystemeWindows {
    fn reg_texte(&self, _: Ruche, _: &str, _: &str) -> Option<String> {
        None
    }
    fn reg_nombre(&self, _: Ruche, _: &str, _: &str) -> Option<u32> {
        None
    }
    fn reg_sous_cles(&self, _: Ruche, _: &str) -> Vec<String> {
        Vec::new()
    }
    fn variable(&self, nom: &str) -> Option<String> {
        std::env::var(nom).ok()
    }
    fn disques_fixes(&self) -> Vec<String> {
        Vec::new()
    }
    fn applis_demarrer(&self) -> Vec<(String, String)> {
        Vec::new()
    }
    fn maintenant(&self) -> Moment {
        let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        Moment(s as i64)
    }
    fn lancer(&self, _: &str, _: &str, _: &str) -> Result<(), String> {
        Err("pas sous Windows".into())
    }
    fn choisir_exe(&self) -> Option<String> {
        None
    }
    fn infos_programme(&self, _: &str) -> (String, String) {
        Default::default()
    }
}
