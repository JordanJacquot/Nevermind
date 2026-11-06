//! Ce que produit l'analyse (analyse.ps1, tableau-de-bord.ps1) : les constats (`Add-Finding`), leur correction
//! (`New-Fix`) avec ce qu'elle change, déclaré à l'avance (`@{ Type = 'reg' ... }`), et les cartes des composants
//! (`New-Component`).

use crate::pc::ValeurReg;
use serde_json::{Map, Value, json};

/// Un changement déclaré à l'avance par une correction (voir `Set-Declarations`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Declaration {
    /// reg, pcfg, display, svc, power, texte, partiel
    pub sorte: String,
    pub chemin: String,
    pub nom: String,
    /// La valeur écrite (reg) ; `None` avec `efface`
    pub valeur: Option<ValeurReg>,
    /// Le type déclaré (« String », « Binary »...) ; vide : DWord
    pub type_reg: String,
    pub efface: bool,
    /// pcfg : le réglage (CritAction...) et sa valeur
    pub cle: String,
    pub nombre: i64,
    /// display : l'écran et sa fréquence
    pub ecran: String,
    pub hz: i64,
    /// svc : le démarrage voulu (Manual ou Disabled)
    pub demarrage: String,
    /// power : ce qui sera choisi, et son identifiant s'il existe déjà
    pub cible: String,
    pub guid: String,
    /// texte
    pub texte: String,
}

impl Declaration {
    pub fn reg(chemin: &str, nom: &str, valeur: ValeurReg, type_reg: &str) -> Self {
        Declaration {
            sorte: "reg".into(),
            chemin: chemin.into(),
            nom: nom.into(),
            valeur: Some(valeur),
            type_reg: type_reg.into(),
            ..Default::default()
        }
    }
    pub fn reg_efface(chemin: &str, nom: &str) -> Self {
        Declaration { sorte: "reg".into(), chemin: chemin.into(), nom: nom.into(), efface: true, ..Default::default() }
    }
    pub fn texte(t: &str) -> Self {
        Declaration { sorte: "texte".into(), texte: t.into(), ..Default::default() }
    }

    /// Une déclaration écrite en JSON (`@{ Type = 'reg'; Path; Name; Value; Kind }`, envoyée par un autre bloc)
    pub fn depuis_json(v: &Value) -> Self {
        use crate::net::{champ_entier, champ_texte, champ_vrai};
        let sorte = champ_texte(v, "Type");
        let kind = champ_texte(v, "Kind");
        let valeur = v
            .get("Value")
            .filter(|x| !x.is_null())
            .map(|x| ValeurReg::depuis(x, if kind.is_empty() { "DWord" } else { &kind }));
        Declaration {
            chemin: champ_texte(v, "Path"),
            nom: champ_texte(v, "Name"),
            valeur: if sorte == "reg" { valeur } else { None },
            type_reg: kind,
            efface: champ_vrai(v, "Efface"),
            cle: champ_texte(v, "Key"),
            nombre: champ_entier(v, "Value"),
            ecran: champ_texte(v, "Device"),
            hz: champ_entier(v, "Hz"),
            demarrage: champ_texte(v, "Demarrage"),
            cible: champ_texte(v, "Cible"),
            guid: champ_texte(v, "Guid"),
            texte: champ_texte(v, "Texte"),
            sorte,
        }
    }

    /// Comme la version PowerShell l'exporte (les clés présentes seulement, triées)
    pub fn json(&self) -> Value {
        let mut m = Map::new();
        match self.sorte.as_str() {
            "reg" => {
                if self.efface {
                    m.insert("Efface".into(), json!(true));
                }
                if !self.type_reg.is_empty() {
                    m.insert("Kind".into(), json!(self.type_reg));
                }
                m.insert("Name".into(), json!(self.nom));
                m.insert("Path".into(), json!(self.chemin));
                if let Some(v) = &self.valeur {
                    m.insert("Value".into(), v.json());
                }
            }
            "pcfg" => {
                m.insert("Key".into(), json!(self.cle));
                m.insert("Value".into(), json!(self.nombre));
            }
            "display" => {
                m.insert("Device".into(), json!(self.ecran));
                m.insert("Hz".into(), json!(self.hz));
            }
            "svc" => {
                if !self.demarrage.is_empty() {
                    m.insert("Demarrage".into(), json!(self.demarrage));
                }
                m.insert("Name".into(), json!(self.nom));
            }
            "power" => {
                m.insert("Cible".into(), json!(self.cible));
                if !self.guid.is_empty() {
                    m.insert("Guid".into(), json!(self.guid));
                }
            }
            "texte" => {
                m.insert("Texte".into(), json!(self.texte));
            }
            _ => {}
        }
        m.insert("Type".into(), json!(self.sorte));
        Value::Object(m)
    }
}

/// Ce que change une correction : une liste fixe, ou calculée au moment de la montrer et de la faire (le
/// scriptblock `-Changes` de PowerShell, relu à chaque fois : un réglage changé juste avant compte).
#[derive(Debug, Clone, PartialEq)]
pub enum Changements {
    Aucun,
    Liste(Vec<Declaration>),
    /// `Get-ChangementAlimentation`
    Alimentation,
    /// `Get-ChangementsAccessHotkeys`
    Raccourcis,
    /// `Get-ChangementWindowedOptim`
    Fenetre,
    /// `Get-ChangementsNettoyage`
    Nettoyage,
}

/// Ce que fait le bouton Exécuter (`-Run`).
#[derive(Debug, Clone, PartialEq)]
pub enum Execution {
    /// Les changements déclarés (`Set-Declarations`)
    Declarations,
    /// Un réglage gaming (`Invoke-TweakApply`) : ses changements, puis ce qu'il fait en plus
    Reglage(String),
    /// `Invoke-CleanAll`
    Nettoyage,
    /// `Start-Process 'mdsched.exe'`
    TestMemoire,
    /// `$BiosRun` : redémarrer dans le BIOS
    Bios,
}

impl Execution {
    pub fn nom(&self) -> &'static str {
        match self {
            Execution::Declarations => "declarations",
            Execution::Reglage(_) => "tweak",
            Execution::Nettoyage => "nettoyage",
            Execution::TestMemoire => "mdsched",
            Execution::Bios => "bios",
        }
    }
}

/// Une correction (`New-Fix`).
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    pub auto: bool,
    pub quoi: Vec<String>,
    pub pourquoi: String,
    pub etapes: Vec<String>,
    pub execution: Option<Execution>,
    pub libelle: String,
    pub confirmer: Option<String>,
    pub fait: Option<String>,
    pub sans_analyse: bool,
    pub ouvrir: Option<String>,
    pub libelle_ouvrir: String,
    pub redemarrage: bool,
    pub restauration: bool,
    pub changements: Changements,
}

impl Default for Correction {
    fn default() -> Self {
        Correction {
            auto: false,
            quoi: Vec::new(),
            pourquoi: String::new(),
            etapes: Vec::new(),
            execution: None,
            libelle: "Exécuter".into(),
            confirmer: None,
            fait: None,
            sans_analyse: false,
            ouvrir: None,
            libelle_ouvrir: "Ouvrir".into(),
            redemarrage: false,
            restauration: false,
            changements: Changements::Aucun,
        }
    }
}

impl Correction {
    /// Des étapes à suivre soi-même (et un bouton pour ouvrir le bon endroit)
    pub fn conseil(pourquoi: &str, etapes: &[&str]) -> Self {
        Correction {
            pourquoi: pourquoi.into(),
            etapes: etapes.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }
    pub fn ouvre(mut self, cible: &str, libelle: &str) -> Self {
        self.ouvrir = Some(cible.into());
        self.libelle_ouvrir = libelle.into();
        self
    }
    /// Des changements déclarés, faits par Exécuter (`-Changes` sans `-Run`)
    pub fn changements(mut self, c: Changements) -> Self {
        if self.execution.is_none() {
            self.execution = Some(Execution::Declarations);
        }
        self.changements = c;
        self
    }
}

/// Un constat de l'analyse (`Add-Finding`) : ok, warn, bad ou info.
#[derive(Debug, Clone, PartialEq)]
pub struct Constat {
    pub id: String,
    pub statut: String,
    pub titre: String,
    pub detail: String,
    pub poids: i64,
    pub action: String,
    pub libelle_action: String,
    pub correction: Option<Correction>,
    /// Le gain montré (points), calculé par `Measure-Score`
    pub gain: f64,
}

impl Constat {
    pub fn nouveau(
        statut: &str,
        titre: &str,
        detail: &str,
        poids: i64,
        id: &str,
        correction: Option<Correction>,
    ) -> Self {
        Constat {
            id: if id.is_empty() { titre.into() } else { id.into() },
            statut: statut.into(),
            titre: titre.into(),
            detail: detail.into(),
            poids,
            action: String::new(),
            libelle_action: String::new(),
            correction,
            gain: 0.0,
        }
    }
    pub fn auto(&self) -> bool {
        self.correction.as_ref().is_some_and(|c| c.auto)
    }
}

/// Une jauge d'une carte (`Bars`).
#[derive(Debug, Clone, PartialEq)]
pub struct Barre {
    pub libelle: String,
    pub valeur: f64,
    pub texte: String,
    pub couleur: String,
}

/// Une ligne d'une carte (`Lines`) : le texte, et sa couleur quand elle est donnée.
#[derive(Debug, Clone, PartialEq)]
pub struct Ligne {
    pub cle: String,
    pub texte: String,
    pub couleur: Option<String>,
}

/// Une explication d'une carte (`Notes`).
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub statut: String,
    pub texte: String,
}

/// La carte d'un composant (`New-Component`).
#[derive(Debug, Clone, PartialEq)]
pub struct Carte {
    pub tag: String,
    pub titre: String,
    pub sous: String,
    pub statut: String,
    pub barres: Vec<Barre>,
    pub lignes: Vec<Ligne>,
    pub notes: Vec<Note>,
    pub action: Option<String>,
    pub libelle_action: Option<String>,
}

impl Carte {
    pub fn nouvelle(tag: &str, titre: &str, sous: &str) -> Self {
        Carte {
            tag: tag.into(),
            titre: titre.into(),
            sous: sous.into(),
            statut: "ok".into(),
            barres: Vec::new(),
            lignes: Vec::new(),
            notes: Vec::new(),
            action: None,
            libelle_action: None,
        }
    }
    /// `$c.Lines[cle] = texte` (une clé déjà là garde sa place)
    pub fn ligne(&mut self, cle: &str, texte: &str, couleur: Option<&str>) {
        let l = Ligne { cle: cle.into(), texte: texte.into(), couleur: couleur.map(String::from) };
        if let Some(x) = self.lignes.iter_mut().find(|x| x.cle == cle) {
            *x = l;
        } else {
            self.lignes.push(l);
        }
    }
    /// `Set-Worse`
    pub fn pire(&mut self, statut: &str) {
        let rang = |s: &str| match s {
            "warn" => 1,
            "bad" => 2,
            _ => 0,
        };
        if rang(statut) > rang(&self.statut) {
            self.statut = statut.into();
        }
    }
    /// `Add-Note`
    pub fn note(&mut self, statut: &str, texte: &str) {
        self.notes.push(Note { statut: statut.into(), texte: texte.into() });
        self.pire(statut);
    }
}

/// L'analyse complète (`$script:LastAnalysis`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Analyse {
    pub info: Vec<(String, String)>,
    pub cartes: Vec<Carte>,
    /// Les constats qui comptent (pas ignorés), triés
    pub actifs: Vec<Constat>,
    /// Les points à régler mis de côté (`$script:IgnoredFindings`)
    pub ignores: Vec<Constat>,
    pub score: i64,
    pub potentiel: i64,
    pub appreciation: String,
    pub couleur: String,
    /// « 5 en bon état, 1 à surveiller » (`Show-HealthCards`)
    pub sante: String,
    pub portable: bool,
    pub onduleur: bool,
    pub build: i64,
}

impl Analyse {
    pub fn info(&self, cle: &str) -> String {
        self.info.iter().find(|(k, _)| k == cle).map(|(_, v)| v.clone()).unwrap_or_default()
    }
}

/// Les couleurs de Nevermind (`$Colors`, interface.ps1)
pub mod couleurs {
    pub const OK: &str = "#22D37A";
    pub const WARN: &str = "#F5A524";
    pub const BAD: &str = "#F04438";
    pub const INFO: &str = "#4EA8FF";
    pub const MUTED: &str = "#655E7E";
    pub const TEXTE: &str = "#EEEBF7";
    pub fn de(statut: &str) -> &'static str {
        match statut {
            "ok" => OK,
            "warn" => WARN,
            "bad" => BAD,
            "info" => INFO,
            _ => "",
        }
    }
}

/// `$StatusLabels`
pub fn libelle_statut(s: &str) -> &'static str {
    match s {
        "ok" => "Bon état",
        "warn" => "À surveiller",
        "bad" => "Problème",
        "info" => "Info",
        _ => "",
    }
}
