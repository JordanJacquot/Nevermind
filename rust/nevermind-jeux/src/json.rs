//! Les fichiers de données sûrs, comme donnees.ps1 (Read-JsonSur, Save-TexteSur) : écrits d'abord à côté (.tmp), puis
//! l'ancien devient la copie de secours (.bak) ; un fichier abîmé est mis de côté (.abime-<date>) et sa copie reprise.

use nevermind_noyau::Contexte;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

/// `$script:JsonBloques` : les fichiers qui n'ont pas pu être lus à leur dernière lecture (pris par un autre
/// programme) ; ils ne sont pas réécrits, ils gardent peut-être plus que ce que Nevermind a en mémoire
static BLOQUES: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

fn bloques() -> std::sync::MutexGuard<'static, HashSet<PathBuf>> {
    BLOQUES.lock().unwrap_or_else(|e| e.into_inner())
}

/// Le contenu du fichier (ou de sa copie de secours), `None` s'il n'y en a pas (Read-JsonSur).
/// `occupe` : le fichier existe mais n'a pas pu être lu (pris par un autre programme) : il ne faut pas le réécrire,
/// et [`ecrire_json_sur`] le refuse jusqu'à la prochaine lecture réussie (Save-JsonSur, donnees.ps1:57 ; avant le
/// 08/11, jeux.json et organizer.json étaient réécrits quand même, avec seulement ce qui venait d'être ajouté).
pub fn lire_json_sur(ctx: Option<&Contexte>, chemin: &Path) -> (Option<Value>, bool) {
    let (v, occupe) = lire_json_sur_une_fois(ctx, chemin);
    let mut b = bloques();
    if occupe && v.is_none() {
        b.insert(chemin.to_path_buf());
    } else {
        b.remove(chemin);
    }
    (v, occupe)
}

fn lire_json_sur_une_fois(ctx: Option<&Contexte>, chemin: &Path) -> (Option<Value>, bool) {
    let mut occupe = false;
    let bak = avec_suffixe(chemin, ".bak");
    for p in [chemin.to_path_buf(), bak] {
        if !p.is_file() {
            continue;
        }
        let mut texte = None;
        let mut err = String::new();
        for _ in 0..3 {
            match std::fs::read(&p) {
                Ok(o) => {
                    texte = Some(String::from_utf8_lossy(&o).trim_start_matches('\u{feff}').to_string());
                    break;
                }
                Err(e) => {
                    err = e.to_string();
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        let Some(texte) = texte else {
            journal(ctx, &format!("Fichier illisible pour l'instant ({}) : {err}", p.display()));
            occupe = true;
            continue;
        };
        let res = if texte.trim().is_empty() {
            Err("fichier vide".to_string())
        } else {
            serde_json::from_str::<Value>(&texte).map_err(|e| e.to_string())
        };
        match res {
            Ok(v) => {
                if p != chemin {
                    journal(ctx, &format!("{} repris de sa copie de secours", nom(chemin)));
                }
                return (Some(v), false);
            }
            Err(e) => {
                let dest = avec_suffixe(&p, &format!(".abime-{}", horodatage_fichier()));
                journal(ctx, &format!("Fichier abîmé ({} : {e}), mis de côté : {}", p.display(), nom(&dest)));
                if let Err(e) = std::fs::rename(&p, &dest) {
                    journal(ctx, &format!("Fichier abîmé pas mis de côté ({}) : {e}", p.display()));
                }
            }
        }
    }
    (None, occupe)
}

/// Écrit un fichier sans jamais le laisser à moitié écrit (Save-TexteSur) : .tmp, puis remplacement avec copie .bak.
pub fn ecrire_texte_sur(chemin: &Path, texte: &str) -> std::io::Result<()> {
    if let Some(d) = chemin.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = avec_suffixe(chemin, ".tmp");
    std::fs::write(&tmp, texte.as_bytes())?;
    if !chemin.exists() {
        return std::fs::rename(&tmp, chemin);
    }
    let bak = avec_suffixe(chemin, ".bak");
    std::fs::copy(chemin, &bak)?;
    std::fs::rename(&tmp, chemin).or_else(|_| {
        std::fs::copy(&tmp, chemin)?;
        let _ = std::fs::remove_file(&tmp);
        Ok(())
    })
}

/// Écrit un JSON (compact, comme `-Compress`, ou lisible).
pub fn ecrire_json_sur(chemin: &Path, v: &Value, compact: bool) -> std::io::Result<()> {
    if bloques().contains(chemin) {
        return Err(std::io::Error::other(format!("{} n'a pas pu être lu : il n'est pas réécrit", nom(chemin))));
    }
    let t = if compact { serde_json::to_string(v) } else { serde_json::to_string_pretty(v) }.unwrap_or_default();
    ecrire_texte_sur(chemin, &t)
}

pub fn avec_suffixe(p: &Path, suffixe: &str) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(suffixe);
    PathBuf::from(s)
}

fn nom(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

fn journal(ctx: Option<&Contexte>, m: &str) {
    if let Some(c) = ctx {
        c.journal(m);
    }
}

fn horodatage_fichier() -> String {
    // « yyyyMMdd-HHmmss » (heure du journal du noyau)
    let h = nevermind_noyau::contexte::horodatage();
    h.replace(['-', ':'], "").replace('T', "-")
}

/// Lecture d'un champ texte comme `[string]$x.Champ` : absent ou nul donne « », un nombre donne son texte.
pub fn texte(v: &Value, champ: &str) -> String {
    texte_de(v.get(champ).unwrap_or(&Value::Null))
}

pub fn texte_de(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// `[double]$x.Champ` (absent : 0).
pub fn nombre(v: &Value, champ: &str) -> f64 {
    nombre_de(v.get(champ).unwrap_or(&Value::Null))
}

pub fn nombre_de(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().replace(',', ".").parse().unwrap_or(0.0),
        Value::Bool(b) => f64::from(u8::from(*b)),
        _ => 0.0,
    }
}

/// `[bool]$x` de PowerShell.
pub fn vrai(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => match a.len() {
            0 => false,
            1 => vrai(&a[0]),
            _ => true,
        },
        Value::Object(_) => true,
    }
}
