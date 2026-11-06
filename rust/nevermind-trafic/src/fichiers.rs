//! Les fichiers de données du bloc (lag.json, serveurs.json), lus et écrits comme `Read-JsonSur` et
//! `Save-JsonSur` (donnees.ps1) : copie de secours « .bak », fichier abîmé mis de côté, jamais une liste vide qui
//! écraserait un fichier illisible.

use serde_json::Value;
use std::path::Path;

/// Le contenu (ou celui de sa copie de secours) ; `Err` si le fichier existe mais est pris par un autre programme
#[allow(clippy::result_unit_err)]
pub fn lire(chemin: &Path, journal: &dyn Fn(&str)) -> Result<Option<Value>, ()> {
    let mut occupe = false;
    let bak = chemin.with_file_name(format!("{}.bak", chemin.file_name().and_then(|n| n.to_str()).unwrap_or("")));
    for p in [chemin.to_path_buf(), bak] {
        if !p.exists() {
            continue;
        }
        let mut texte = None;
        for _ in 0..3 {
            match std::fs::read(&p) {
                Ok(o) => {
                    texte = Some(String::from_utf8_lossy(&o).into_owned());
                    break;
                }
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
            }
        }
        let Some(t) = texte else {
            journal(&format!("Fichier illisible pour l'instant ({})", p.display()));
            occupe = true;
            continue;
        };
        let t = t.trim_start_matches('\u{feff}');
        match serde_json::from_str::<Value>(t) {
            Ok(v) if !t.trim().is_empty() => {
                if p != chemin {
                    journal(&format!(
                        "{} repris de sa copie de secours",
                        chemin.file_name().and_then(|n| n.to_str()).unwrap_or("")
                    ));
                }
                return Ok(Some(v));
            }
            r => {
                let pourquoi = match r {
                    Err(e) => e.to_string(),
                    _ => "fichier vide".into(),
                };
                let dest = format!("{}.abime-{}", p.display(), chrono::Local::now().format("%Y%m%d-%H%M%S"));
                journal(&format!(
                    "Fichier abîmé ({} : {pourquoi}), mis de côté : {}",
                    p.display(),
                    Path::new(&dest).file_name().and_then(|n| n.to_str()).unwrap_or("")
                ));
                let _ = std::fs::rename(&p, &dest);
            }
        }
    }
    if occupe { Err(()) } else { Ok(None) }
}

/// Écrit le fichier sans risque : d'abord à côté, puis à sa place (l'ancien devient la copie de secours)
pub fn ecrire(chemin: &Path, v: &Value) -> Result<(), String> {
    if let Some(d) = chemin.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let nom = chemin.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    let tmp = chemin.with_file_name(format!("{nom}.tmp"));
    let bak = chemin.with_file_name(format!("{nom}.bak"));
    std::fs::write(&tmp, serde_json::to_string(v).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if !chemin.exists() {
        return std::fs::rename(&tmp, chemin).map_err(|e| e.to_string());
    }
    let _ = std::fs::copy(chemin, &bak);
    std::fs::rename(&tmp, chemin).map_err(|e| e.to_string())
}
