//! Les tests des composants (tests.ps1) : le déroulé et le panneau (`banc`, `kit`), les résultats (`rendus`), les
//! mesures (`mesures`, dans un fil à part) et ce qui parle à Windows (`windows`).
//!
//! Le bloc ne fait jamais le travail lui-même : un test (disque, processeur, mémoire, connexion, lag, erreurs) tourne
//! dans un fil à part ; la santé d'un disque et nvidia-smi sont lus dans un fil à part aussi. À chaque tour (100 ms
//! pendant un test), le bloc lit seulement la progression partagée (`mesures::Moteur`).

pub mod banc;
pub mod kit;
pub mod mesures;
pub mod rendus;
#[cfg(windows)]
pub mod windows;

use banc::{Lieu, Resultat, Travail};
use mesures::Moteur;
use serde_json::Value;
use std::sync::Arc;
use std::thread::JoinHandle;

/// Le travail en cours dans un fil à part
#[derive(Default)]
pub struct Atelier {
    pub mesure: Option<(Arc<Moteur>, JoinHandle<Resultat>)>,
    pub sante: Option<(usize, JoinHandle<Option<Value>>)>,
    pub gpu: Option<(usize, JoinHandle<Option<String>>)>,
    /// La dernière fois que l'écran a été redessiné pendant un test
    pub redessine: Option<std::time::Instant>,
}

impl std::fmt::Debug for Atelier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Atelier").field("mesure", &self.mesure.is_some()).finish()
    }
}

fn nombre(v: &Value, k: &str) -> f64 {
    crate::net::champ_nombre(v, k)
}

/// Lancer le travail d'un test dans un fil à part (`Invoke-Async $Def.Work $Def.Arg`)
pub fn lancer(travail: Travail, arg: &Value) -> (Arc<Moteur>, JoinHandle<Resultat>) {
    let m = Arc::new(Moteur::default());
    let m2 = m.clone();
    let arg = arg.clone();
    let h = std::thread::spawn(move || travailler(&m2, travail, &arg));
    (m, h)
}

fn travailler(m: &Moteur, travail: Travail, arg: &Value) -> Resultat {
    let fini = |r: Option<Vec<f64>>| r.map_or(Resultat::Arrete, Resultat::R);
    #[cfg(windows)]
    {
        let reseau = windows::ReseauWindows;
        match travail {
            Travail::Disque => {
                match mesures::disque(m, &crate::net::champ_texte(arg, "File"), nombre(arg, "Size") as i64) {
                    Ok(r) => fini(r),
                    Err(e) => Resultat::Erreur(e),
                }
            }
            Travail::Cpu => fini(mesures::cpu(m, nombre(arg, "Single"), nombre(arg, "Multi"), windows::abaisser)),
            Travail::Memoire => fini(mesures::memoire(m, nombre(arg, "Bytes") as i64)),
            Travail::Reseau => fini(mesures::reseau(m, &reseau)),
            Travail::Lag => {
                let l = |k: &str| crate::net::champ_liste(arg, k).iter().map(crate::net::texte).collect::<Vec<_>>();
                let (d, u) = (l("Down"), l("Up"));
                let d: Vec<&str> = d.iter().map(String::as_str).collect();
                let u: Vec<&str> = u.iter().map(String::as_str).collect();
                fini(mesures::lag(m, &reseau, &d, &u, "1.1.1.1", 6.0, 11.0))
            }
            Travail::Erreurs => {
                let lettres: Vec<String> =
                    crate::net::champ_liste(arg, "Letters").iter().map(crate::net::texte).collect();
                Resultat::Lignes(mesures::erreurs(m, &lettres, &windows::verifier_volume))
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (m, travail, arg, fini);
        Resultat::Erreur("Ce test ne marche que sous Windows.".into())
    }
}

/// Ce que le banc lit du vrai PC au moment d'un clic
pub struct LieuWindows;

impl Lieu for LieuWindows {
    fn volume(&self, lettre: &str) -> Option<(f64, f64)> {
        #[cfg(windows)]
        {
            let v = crate::windows::volume(lettre);
            (!v.is_null()).then(|| (nombre(&v, "Size"), nombre(&v, "SizeRemaining")))
        }
        #[cfg(not(windows))]
        {
            let _ = lettre;
            None
        }
    }
    fn lecteur_systeme(&self) -> String {
        std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into())
    }
    fn temp(&self) -> String {
        std::env::var("TEMP").unwrap_or_default()
    }
    fn smi(&self) -> bool {
        #[cfg(windows)]
        {
            windows::chemin_smi().is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
}

/// Le PC du mode test : rien n'est lu (aucun lecteur prêt, pas de nvidia-smi)
pub struct LieuEssai;

impl Lieu for LieuEssai {
    fn volume(&self, _lettre: &str) -> Option<(f64, f64)> {
        None
    }
    fn lecteur_systeme(&self) -> String {
        "C:".into()
    }
    fn temp(&self) -> String {
        String::new()
    }
    fn smi(&self) -> bool {
        false
    }
}

/// `Test-3DMark` : 3DMark est installé dans une bibliothèque Steam (lecture du registre et des dossiers)
pub fn troisdmark() -> bool {
    #[cfg(windows)]
    {
        let Ok(Some(v)) = crate::windows::reg_lire(r"HKCU:\Software\Valve\Steam", "SteamPath") else { return false };
        let steam = v.texte().replace('/', "\\");
        if steam.is_empty() {
            return false;
        }
        let mut libs = vec![steam.clone()];
        let vdf = std::path::Path::new(&steam).join(r"steamapps\libraryfolders.vdf");
        if let Ok(t) = std::fs::read_to_string(&vdf)
            && let Ok(re) = regex::Regex::new(r#""path"\s+"([^"]+)""#)
        {
            for c in re.captures_iter(&t) {
                libs.push(c[1].replace(r"\\", r"\"));
            }
        }
        libs.iter().any(|l| std::path::Path::new(l).join(r"steamapps\common\3DMark").exists())
    }
    #[cfg(not(windows))]
    {
        false
    }
}
