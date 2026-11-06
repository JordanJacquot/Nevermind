//! Les fichiers de données du bloc (appareils.json, audit.json, fabricants.txt), lus et écrits comme
//! Read-JsonSur et Save-JsonSur de donnees.ps1 : écrits à côté (.tmp) puis remplacés d'un coup, l'ancien gardé en
//! copie de secours (.bak) ; un fichier abîmé mis de côté (.abime-<date>) et sa copie de secours reprise.
//! (À remplacer par la version du noyau quand elle existera.)

use serde_json::Value;
use std::path::{Path, PathBuf};

fn avec(p: &Path, suffixe: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffixe);
    PathBuf::from(s)
}

/// Le contenu du fichier (ou de sa copie de secours), `None` s'il n'y en a pas
pub fn lire_json(chemin: &Path, horodatage: &str, journal: &dyn Fn(&str)) -> Option<Value> {
    for p in [chemin.to_path_buf(), avec(chemin, ".bak")] {
        if !p.exists() {
            continue;
        }
        let Ok(t) = std::fs::read_to_string(&p) else {
            journal(&format!("Fichier illisible pour l'instant ({})", p.display()));
            continue;
        };
        let t = t.trim_start_matches('\u{feff}');
        match serde_json::from_str::<Value>(t) {
            Ok(v) if !t.trim().is_empty() => {
                if p != chemin {
                    journal(&format!("{} repris de sa copie de secours", nom(chemin)));
                }
                return Some(v);
            }
            r => {
                let dest = avec(&p, &format!(".abime-{horodatage}"));
                let cause = match r {
                    Err(e) => e.to_string(),
                    Ok(_) => "fichier vide".into(),
                };
                journal(&format!("Fichier abîmé ({} : {cause}), mis de côté : {}", p.display(), nom(&dest)));
                let _ = std::fs::rename(&p, &dest);
            }
        }
    }
    None
}

fn nom(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Écrit le texte sans jamais laisser un fichier à moitié écrit (Save-TexteSur)
pub fn ecrire_texte(chemin: &Path, texte: &str) -> std::io::Result<()> {
    if let Some(d) = chemin.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = avec(chemin, ".tmp");
    std::fs::write(&tmp, texte)?;
    if !chemin.exists() {
        return std::fs::rename(&tmp, chemin);
    }
    let bak = avec(chemin, ".bak");
    std::fs::copy(chemin, &bak)?;
    std::fs::rename(&tmp, chemin)
}

pub fn ecrire_json(chemin: &Path, v: &Value) -> std::io::Result<()> {
    ecrire_texte(chemin, &serde_json::to_string_pretty(v).unwrap_or_default())
}
