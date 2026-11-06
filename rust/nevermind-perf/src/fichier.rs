//! Les fichiers de données jamais perdus (Save-JsonSur / Read-JsonSur de donnees.ps1) : écrits d'abord à côté (.tmp),
//! puis l'ancien devient la copie de secours (.bak) ; à la lecture, un fichier abîmé est mis de côté (.abime-<date>) et
//! sa copie de secours reprise, jamais une liste vide qui écraserait tout.

use nevermind_noyau::Contexte;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn avec(p: &Path, suffixe: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffixe);
    PathBuf::from(s)
}

/// Le contenu du fichier (ou de sa copie de secours), `None` s'il n'y en a pas
pub fn lire(ctx: &Contexte, p: &Path) -> Option<Value> {
    for (k, f) in [p.to_path_buf(), avec(p, ".bak")].iter().enumerate() {
        if !f.exists() {
            continue;
        }
        let Ok(octets) = std::fs::read(f) else {
            ctx.journal(&format!("Fichier illisible pour l'instant ({})", f.display()));
            continue;
        };
        let texte = String::from_utf8_lossy(&octets);
        let texte = texte.trim_start_matches('\u{feff}');
        match serde_json::from_str::<Value>(texte) {
            Ok(v) if !texte.trim().is_empty() => {
                if k == 1 {
                    ctx.journal(&format!(
                        "{} repris de sa copie de secours",
                        p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                    ));
                }
                return Some(v);
            }
            _ => {
                let dest = avec(
                    f,
                    &format!(".abime-{}", nevermind_noyau::contexte::horodatage().replace([':', '-', 'T'], "")),
                );
                ctx.journal(&format!("Fichier abîmé ({}), mis de côté : {}", f.display(), dest.display()));
                let _ = std::fs::rename(f, &dest);
            }
        }
    }
    None
}

/// Écrit le fichier sans jamais le laisser à moitié écrit
pub fn ecrire(p: &Path, v: &Value, compact: bool) -> std::io::Result<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let texte = if compact { serde_json::to_string(v) } else { serde_json::to_string_pretty(v) }
        .map_err(std::io::Error::other)?;
    let tmp = avec(p, ".tmp");
    std::fs::write(&tmp, texte)?;
    if p.exists() {
        let _ = std::fs::rename(p, avec(p, ".bak"));
    }
    std::fs::rename(&tmp, p)
}

/// Efface le fichier et ses copies (la copie de secours serait reprise sinon)
pub fn effacer(ctx: &Contexte, p: &Path) {
    for f in [p.to_path_buf(), avec(p, ".bak"), avec(p, ".tmp")] {
        if f.exists()
            && let Err(e) = std::fs::remove_file(&f)
        {
            ctx.journal(&format!("Mode jeu : {} non effacé ({e})", f.display()));
        }
    }
}
