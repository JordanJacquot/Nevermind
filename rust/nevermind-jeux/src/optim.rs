//! L'optimisation d'un jeu (Get-GameOptimizations, Get-ChangementsProfil, Test-GamePriority, Get-GpuPreference de
//! bibliotheque.ps1, jeu.ps1 et optimisations.ps1). Les changements de Windows ne sont jamais faits ici : ils sont
//! décrits (déclarations, comme Set-Declarations) et envoyés à la sauvegarde, qui les fait par l'Aide.

use crate::biblio::{Jeu, noms_exe};
use crate::systeme::{Ruche, Systeme};
use serde_json::{Value, json};

pub const IFEO: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options";
pub const DX: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";

/// Un point d'optimisation d'un jeu.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Point {
    pub id: String,
    pub titre: String,
    pub texte: String,
    pub ok: bool,
    pub interrupteur: bool,
    /// « tweaks » : un bouton « Appliquer » (réglages Windows).
    pub correction: Option<String>,
    pub ids: Vec<String>,
    pub facultatif: bool,
}

/// Ce que le bloc sait du PC (envoyé par le bloc « pc », comme $script:AnalysisData) et des réglages Windows pas encore
/// faits (envoyés par le bloc « perf », comme $script:LibTweaks).
#[derive(Debug, Clone, Default)]
pub struct Connu {
    pub cartes: Vec<String>,
    /// Les disques : (lettres, type « HDD », « SSD »...).
    pub disques: Vec<(Vec<String>, String)>,
    /// Les réglages Windows conseillés pas encore faits : (id, titre).
    pub reglages: Vec<(String, String)>,
}

impl Connu {
    /// Lit l'analyse du PC envoyée par le bloc « pc » (mêmes champs que $script:AnalysisData : GPUs, Disks).
    pub fn lire_analyse(&mut self, v: &Value) {
        self.cartes = crate::biblio::liste(v.get("GPUs")).iter().map(|g| crate::json::texte(g, "Name")).collect();
        self.disques = crate::biblio::liste(v.get("Disks"))
            .iter()
            .map(|d| {
                let l = crate::biblio::liste(d.get("Letters")).iter().map(crate::json::texte_de).collect();
                (l, crate::json::texte(d.get("Disk").unwrap_or(&Value::Null), "MediaType"))
            })
            .collect();
    }
    /// Lit la liste des réglages Windows à faire envoyée par le bloc « perf » : [{ Id, Titre }].
    pub fn lire_reglages(&mut self, v: &Value) {
        let src = v.get("reglages").or(v.get("liste")).unwrap_or(v);
        self.reglages = crate::biblio::liste(Some(src))
            .iter()
            .map(|t| (crate::json::texte(t, "Id"), crate::json::texte(t, "Titre")))
            .collect();
    }
    /// Lit le signal « analyse-pc » du bloc pc : `{ gpus: [noms], disques: [{ lettres, bus, media }] }`.
    pub fn lire_analyse_pc(&mut self, v: &Value) {
        self.cartes = crate::biblio::liste(v.get("gpus")).iter().map(crate::json::texte_de).collect();
        self.disques = crate::biblio::liste(v.get("disques"))
            .iter()
            .map(|d| {
                let l = crate::biblio::liste(d.get("lettres")).iter().map(crate::json::texte_de).collect();
                (l, crate::json::texte(d, "media"))
            })
            .collect();
    }
    /// Lit le signal « reglages-jeu » du bloc pc (`$script:TweakRows`) : les réglages conseillés
    /// (`Recommended -ne $false`) pas encore faits (`$script:LibTweaks`, bibliotheque.ps1:950).
    pub fn lire_reglages_jeu(&mut self, v: &Value) {
        self.reglages = crate::biblio::liste(v.get("lignes"))
            .iter()
            .filter(|t| t.get("recommande") != Some(&Value::Bool(false)) && t.get("fait") != Some(&Value::Bool(true)))
            .map(|t| (crate::json::texte(t, "id"), crate::json::texte(t, "titre")))
            .collect();
    }
}

/// Les cartes graphiques qui comptent (sans les écrans virtuels).
pub fn vraies_cartes(c: &Connu) -> Vec<String> {
    static VIRTUELLES: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta|Microsoft Basic").unwrap()
    });
    c.cartes.iter().filter(|n| !VIRTUELLES.is_match(n)).cloned().collect()
}

/// Le jeu passe avant les autres programmes (Test-GamePriority).
pub fn priorite_haute(sys: &dyn Systeme, g: &Jeu) -> bool {
    let n = noms_exe(g);
    let Some(p) = n.first() else { return false };
    sys.reg_nombre(Ruche::Machine, &format!("{IFEO}\\{p}\\PerfOptions"), "CpuPriorityClass") == Some(3)
}

/// La préférence de carte graphique d'un programme (Get-GpuPreference) : « GpuPreference=2; » = la puissante.
pub fn carte_puissante(sys: &dyn Systeme, exe: &str) -> bool {
    sys.reg_texte(Ruche::Utilisateur, DX, exe).map(|v| v.to_lowercase().contains("gpupreference=2")).unwrap_or(false)
}

/// Les points d'optimisation d'un jeu (Get-GameOptimizations). `fps` : mesure des FPS ; `mode` : mode jeu.
pub fn points(sys: &dyn Systeme, c: &Connu, g: &Jeu, fps: bool, mode: bool) -> Vec<Point> {
    let mut l = Vec::new();
    if !noms_exe(g).is_empty() {
        l.push(Point {
            id: "priority".into(),
            titre: "Priorité haute".into(),
            texte: "Le jeu passe avant les autres programmes (même Nevermind fermé).".into(),
            ok: priorite_haute(sys, g),
            interrupteur: true,
            ..Default::default()
        });
    }
    if vraies_cartes(c).len() >= 2 {
        l.push(Point {
            id: "gpu".into(),
            titre: "Carte graphique puissante".into(),
            texte: "Le jeu utilise la grosse carte, pas la puce intégrée.".into(),
            ok: carte_puissante(sys, g.premier_exe()),
            interrupteur: true,
            ..Default::default()
        });
    }
    let n = c.reglages.len();
    let texte = if n > 0 {
        let noms: Vec<&str> = c.reglages.iter().take(3).map(|x| x.1.as_str()).collect();
        format!("{n} à appliquer : {}{}.", noms.join(", "), if n > 3 { "..." } else { "" })
    } else {
        "Tous appliqués (valables pour tous tes jeux).".into()
    };
    l.push(Point {
        id: "tweaks".into(),
        titre: "Réglages Windows pour les jeux".into(),
        texte,
        ok: n == 0,
        correction: (n > 0).then(|| "tweaks".to_string()),
        ids: c.reglages.iter().map(|x| x.0.clone()).collect(),
        ..Default::default()
    });
    l.push(Point {
        id: "fps".into(),
        titre: "Mesure des FPS".into(),
        texte: "Tes FPS sont mesurés à chaque partie, avec un diagnostic si ça rame.".into(),
        ok: fps,
        interrupteur: true,
        ..Default::default()
    });
    l.push(Point {
        id: "mode".into(),
        titre: "Mode jeu (fermer des applis)".into(),
        texte: "OneDrive, Teams... fermés pendant la partie, relancés après.".into(),
        ok: mode,
        interrupteur: true,
        facultatif: true,
        ..Default::default()
    });
    let exe = g.premier_exe();
    if exe.chars().count() > 1 {
        let lettre = exe.chars().next().unwrap().to_uppercase().to_string();
        if let Some((_, t)) = c.disques.iter().find(|(ls, _)| ls.iter().any(|x| x.eq_ignore_ascii_case(&lettre))) {
            let hdd = t.eq_ignore_ascii_case("HDD");
            l.push(Point {
                id: "disk".into(),
                titre: if hdd {
                    format!("Installé sur un disque dur ({lettre}:)")
                } else {
                    format!("Installé sur un SSD ({lettre}:)")
                },
                texte: if hdd {
                    "Chargements plus longs et saccades possibles : déplace le jeu sur un SSD depuis son launcher."
                        .into()
                } else {
                    "Chargements rapides.".into()
                },
                ok: !hdd,
                ..Default::default()
            });
        }
    }
    l
}

/// Les points à faire (pas faits, corrigeables, pas facultatifs).
pub fn a_faire(l: &[Point]) -> Vec<&Point> {
    l.iter().filter(|p| !p.ok && (p.correction.is_some() || p.interrupteur) && !p.facultatif).collect()
}

/// Ce que change un profil de jeu (Get-ChangementsProfil) : « priority » ou « gpu » ; `on` = faux : effacé.
pub fn changements_profil(g: &Jeu, quoi: &str, on: bool) -> Vec<Value> {
    let mut v = Vec::new();
    if quoi == "priority" {
        for n in noms_exe(g) {
            let chemin = format!("HKLM:\\{IFEO}\\{n}\\PerfOptions");
            v.push(if on {
                json!({ "Type": "reg", "Path": chemin, "Name": "CpuPriorityClass", "Value": 3 })
            } else {
                json!({ "Type": "reg", "Path": chemin, "Name": "CpuPriorityClass", "Efface": true })
            });
        }
    } else {
        for e in &g.exes {
            let chemin = format!("HKCU:\\{DX}");
            v.push(if on {
                json!({ "Type": "reg", "Path": chemin, "Name": e, "Value": "GpuPreference=2;", "Kind": "String" })
            } else {
                json!({ "Type": "reg", "Path": chemin, "Name": e, "Efface": true })
            });
        }
    }
    v
}

/// Le nom d'un profil pour l'écran et l'historique.
pub fn libelle_profil(quoi: &str) -> &'static str {
    if quoi == "priority" { "priorité haute" } else { "carte graphique puissante" }
}
