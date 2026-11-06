//! Les fichiers de données de Nevermind (préférences, historique, mesures, jeux...) : jamais perdus
//! (`Save-TexteSur`, `Save-JsonSur`, `Read-JsonSur`, modules\donnees.ps1).
//!
//! - Écriture : d'abord à côté (`.tmp`), puis l'ancien est remplacé d'un coup et reste en copie de secours (`.bak`) :
//!   un arrêt brutal ne laisse jamais un fichier à moitié écrit.
//! - Lecture : un fichier abîmé est mis de côté (`.abime-<date>`), noté au journal, et sa copie de secours reprise ;
//!   jamais une liste vide qui écraserait tout à l'enregistrement suivant. Un fichier pris par un autre programme
//!   (antivirus...) n'est pas abîmé : on n'y touche pas, et il n'est plus réécrit tant qu'il n'a pas été relu.
//!
//! Contrairement à `json_sur` de la bibliothèque commune (qui n'accepte qu'un objet), un fichier de Nevermind peut
//! contenir une liste (historique.json, ignores.json) : la lecture rend toute valeur JSON, comme `ConvertFrom-Json`.
//! Le décodage du texte (marque d'encodage, UTF-8) et la reconnaissance d'un fichier pris viennent de la bibliothèque
//! commune.

use crate::json_ps;
use crate::moment::{self, Horloge};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `<fichier><fin>`
pub fn a_cote(fichier: &Path, fin: &str) -> PathBuf {
    let mut s = fichier.as_os_str().to_owned();
    s.push(fin);
    PathBuf::from(s)
}

/// Le nom d'un fichier (`[IO.Path]::GetFileName`)
pub fn nom(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `Save-TexteSur` : écrit le texte (UTF-8, avec ou sans BOM) à côté, puis remplace l'ancien d'un coup (copie de
/// secours `.bak`). Disque qui ne sait pas remplacer d'un coup : copie de secours, puis copie par-dessus.
pub fn ecrire_texte_sur(chemin: &Path, texte: &str, bom: bool) -> std::io::Result<()> {
    let tmp = a_cote(chemin, ".tmp");
    let mut octets = Vec::with_capacity(texte.len() + 3);
    if bom {
        octets.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    octets.extend_from_slice(texte.as_bytes());
    std::fs::write(&tmp, &octets)?;
    if !chemin.exists() {
        return std::fs::rename(&tmp, chemin);
    }
    let bak = a_cote(chemin, ".bak");
    if crate::windows::remplacer_fichier(&tmp, chemin, &bak).is_err() {
        std::fs::copy(chemin, &bak)?;
        std::fs::copy(&tmp, chemin)?;
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(())
}

/// Les fichiers de données et leur état « pris » (`$script:JsonBloques`).
pub struct Fichiers {
    bloques: HashSet<PathBuf>,
    horloge: Horloge,
}

impl Default for Fichiers {
    fn default() -> Self {
        Fichiers { bloques: HashSet::new(), horloge: moment::horloge_du_pc() }
    }
}

impl Fichiers {
    /// Avec cette horloge (les noms des fichiers mis de côté)
    pub fn avec_horloge(horloge: Horloge) -> Fichiers {
        Fichiers { bloques: HashSet::new(), horloge }
    }

    /// Ce fichier n'a pas pu être lu (pris par un autre programme) : il n'est pas réécrit
    pub fn est_bloque(&self, chemin: &Path) -> bool {
        self.bloques.contains(chemin)
    }

    /// `Save-JsonSur` : `lisible` faux pour `-Compress`, `bom` pour `-Bom`. Un fichier qui n'a pas pu être lu au
    /// démarrage n'est pas réécrit (il garde peut-être plus que ce qu'on a) : erreur.
    pub fn ecrire_json(&self, chemin: &Path, valeur: &Value, lisible: bool, bom: bool) -> Result<(), String> {
        if self.bloques.contains(chemin) {
            return Err(format!("{} n'a pas pu être lu : il n'est pas réécrit", nom(chemin)));
        }
        let texte = if lisible { json_ps::lisible(valeur) } else { json_ps::compact(valeur) };
        ecrire_texte_sur(chemin, &texte, bom).map_err(|e| e.to_string())
    }

    /// `Read-JsonSur` : le contenu du fichier (ou de sa copie de secours), `None` s'il n'y en a pas. Ce qui se passe
    /// est écrit au journal (`journal`).
    pub fn lire_json(&mut self, chemin: &Path, journal: &dyn Fn(&str)) -> Option<Value> {
        self.bloques.remove(chemin);
        let mut occupe = false;
        let bak = a_cote(chemin, ".bak");
        for p in [chemin, bak.as_path()] {
            if !p.is_file() {
                continue;
            }
            let mut texte = None;
            let mut erreur = String::new();
            for _ in 0..3 {
                match std::fs::read(p) {
                    Ok(o) => {
                        texte = Some(suite_commun::json_sur::texte_du_fichier(&o));
                        break;
                    }
                    Err(e) => {
                        erreur = e.to_string();
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            }
            let Some(texte) = texte else {
                journal(&format!("Fichier illisible pour l'instant ({}) : {erreur}", p.display()));
                occupe = true;
                continue;
            };
            let lu = if texte.trim().is_empty() {
                Err("fichier vide".to_string())
            } else {
                serde_json::from_str::<Value>(texte.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())
            };
            match lu {
                Ok(j) => {
                    if p != chemin {
                        journal(&format!("{} repris de sa copie de secours", nom(chemin)));
                    }
                    return if j.is_null() { None } else { Some(j) };
                }
                Err(m) => {
                    let dest = a_cote(p, &format!(".abime-{}", moment::compacte(&(self.horloge)())));
                    journal(&format!("Fichier abîmé ({} : {m}), mis de côté : {}", p.display(), nom(&dest)));
                    let deplace = if dest.exists() {
                        Err("Impossible de créer un fichier déjà existant.".to_string())
                    } else {
                        std::fs::rename(p, &dest).map_err(|e| e.to_string())
                    };
                    if let Err(e) = deplace {
                        journal(&format!("Fichier abîmé pas mis de côté ({}) : {e}", p.display()));
                    }
                }
            }
        }
        if occupe {
            self.bloques.insert(chemin.to_path_buf());
        }
        None
    }
}
