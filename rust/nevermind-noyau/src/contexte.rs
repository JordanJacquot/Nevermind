//! Le contexte donné à chaque bloc : les dossiers, le journal, les réglages (parametres.json), l'Aide.
//! Première version simple, posée par l'intégration ; l'agent « noyau » la rend identique à donnees.ps1 et
//! aide-client.ps1 (fichiers sûrs, .bak, Exiger, Invoke-Aide) SANS changer les signatures publiques.

use serde_json::{Map, Value};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Contexte {
    /// Le dossier de l'appli (fichiers livrés : ui, fichiers de données).
    pub appli: PathBuf,
    /// Le dossier des données (`$DataDir`).
    pub donnees: PathBuf,
    /// Nevermind est géré par Mindset (`$Managed`).
    pub geree: bool,
    /// Mode test (`$env:NEVERMIND_TEST`) : rien ne touche le PC.
    pub essai: bool,
    reglages: Mutex<Option<Map<String, Value>>>,
}

impl Contexte {
    pub fn nouveau(appli: PathBuf, donnees: PathBuf, geree: bool, essai: bool) -> Self {
        Self { appli, donnees, geree, essai, reglages: Mutex::new(None) }
    }

    /// Une ligne au journal (journal.txt, comme `Write-Log`).
    pub fn journal(&self, message: &str) {
        use std::io::Write;
        let _ = std::fs::create_dir_all(&self.donnees);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.donnees.join("journal.txt"))
        {
            let _ = write!(f, "{} {}\r\n", horodatage(), message);
        }
    }

    /// Un réglage (`Get-Setting`), `None` s'il n'existe pas.
    pub fn reglage(&self, nom: &str) -> Option<Value> {
        let mut r = self.reglages.lock().unwrap_or_else(|e| e.into_inner());
        if r.is_none() {
            let lu = std::fs::read_to_string(self.donnees.join("parametres.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            *r = Some(lu);
        }
        r.as_ref().and_then(|m| m.get(nom).cloned())
    }

    /// Écrire un réglage (`Set-Setting`). `exiger` : erreur si le fichier n'a pas pu être écrit (rien n'est changé).
    pub fn ecrire_reglage(&self, nom: &str, valeur: Value, exiger: bool) -> Result<(), String> {
        let _ = self.reglage(nom);
        let mut r = self.reglages.lock().unwrap_or_else(|e| e.into_inner());
        let m = r.get_or_insert_with(Map::new);
        let avant = m.insert(nom.to_string(), valeur);
        let texte = serde_json::to_string_pretty(&Value::Object(m.clone())).unwrap_or_default();
        let _ = std::fs::create_dir_all(&self.donnees);
        if let Err(e) = std::fs::write(self.donnees.join("parametres.json"), texte) {
            self.journal(&format!("Écriture des préférences impossible: {e}"));
            if exiger {
                match avant {
                    Some(v) => {
                        m.insert(nom.to_string(), v);
                    }
                    None => {
                        m.remove(nom);
                    }
                }
                return Err("Nevermind n'a pas pu noter l'état d'origine (son fichier de préférences est pris par un autre programme) : rien n'a été changé. Réessaie dans un instant.".into());
            }
        }
        Ok(())
    }

    /// Une demande à l'Aide (`Invoke-Aide`) : réponse JSON, ou l'erreur en français. Première version : non branchée.
    pub fn aide(&self, action: &str, _parametres: &Value, _delai_ms: u64) -> Result<Value, String> {
        Err(format!("Aide pas encore branchée en Rust ({action})"))
    }
}

/// L'heure au format de `Get-Date -Format s` (première version : heure UTC ; l'agent noyau met l'heure locale).
pub fn horodatage() -> String {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (j, r) = (s / 86400, s % 86400);
    let (a, m, d) = civil(j as i64);
    format!("{a:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", r / 3600, (r % 3600) / 60, r % 60)
}

fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}
