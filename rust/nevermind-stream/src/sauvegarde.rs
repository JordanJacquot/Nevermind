//! La copie des anciens réglages, faite avant tout changement, et « Remettre comme avant » qui rend EXACTEMENT les
//! anciens fichiers (les mêmes octets ; un fichier qui n'existait pas est retiré).
//!
//! Une copie par logiciel, dans le dossier des données de Nevermind (`streameur\<obs|streamlabs>\`) : `copie.json`
//! (la liste des fichiers, s'ils existaient, la date) et un fichier `n.avant` par fichier gardé. La copie garde l'état
//! d'AVANT le premier changement de Nevermind : un second « Optimiser » ne la remplace pas (il ajoute seulement les
//! fichiers que le premier n'avait pas touchés, tels qu'ils sont encore).

use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Écrit un fichier d'un coup : d'abord à côté (`.nevermind-tmp`), puis il remplace l'ancien (un arrêt brutal ne
/// laisse jamais un fichier à moitié écrit).
pub fn ecrire_sur(chemin: &Path, octets: &[u8]) -> std::io::Result<()> {
    let mut tmp = chemin.as_os_str().to_owned();
    tmp.push(".nevermind-tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, octets)?;
    if let Err(e) = std::fs::rename(&tmp, chemin) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// La copie d'un logiciel
#[derive(Debug, Clone)]
pub struct Copie {
    pub dossier: PathBuf,
}

impl Copie {
    pub fn nouvelle(dossier: PathBuf) -> Copie {
        Copie { dossier }
    }

    fn fiche(&self) -> PathBuf {
        self.dossier.join("copie.json")
    }

    fn lire_fiche(&self) -> Option<Value> {
        let o = std::fs::read(self.fiche()).ok()?;
        serde_json::from_slice::<Value>(&o).ok().filter(|v| v["fichiers"].is_array())
    }

    /// Une copie existe (« Remettre comme avant » est possible)
    pub fn existe(&self) -> bool {
        self.lire_fiche().is_some()
    }

    /// La date de la copie (« 08/11/2026 à 23:10 »)
    pub fn date(&self) -> Option<String> {
        self.lire_fiche().and_then(|f| f["date"].as_str().map(str::to_string))
    }

    /// Les fichiers gardés (chemins)
    pub fn fichiers(&self) -> Vec<PathBuf> {
        self.lire_fiche()
            .and_then(|f| f["fichiers"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|x| x["chemin"].as_str().map(PathBuf::from))
            .collect()
    }

    /// Garde ces fichiers tels qu'ils sont (ceux qui ne sont pas déjà dans la copie). Erreur : rien ne doit être
    /// changé (la copie n'a pas pu être faite).
    pub fn garder(&self, fichiers: &[PathBuf], date: &str, profil: &str) -> Result<(), String> {
        std::fs::create_dir_all(&self.dossier).map_err(|e| format!("dossier de la copie : {e}"))?;
        let mut fiche = self.lire_fiche().unwrap_or_else(|| json!({ "date": date, "profil": profil, "fichiers": [] }));
        let mut liste = fiche["fichiers"].as_array().cloned().unwrap_or_default();
        let deja: Vec<String> = liste.iter().filter_map(|x| x["chemin"].as_str().map(str::to_lowercase)).collect();
        for f in fichiers {
            let c = f.to_string_lossy().to_string();
            if deja.contains(&c.to_lowercase()) {
                continue;
            }
            let n = liste.len() + 1;
            let nom = format!("{n}.avant");
            let entree = match std::fs::read(f) {
                Ok(o) => {
                    ecrire_sur(&self.dossier.join(&nom), &o).map_err(|e| format!("copie de {c} : {e}"))?;
                    // (relue : la copie doit être exacte avant qu'on touche à l'original)
                    if std::fs::read(self.dossier.join(&nom)).ok().as_deref() != Some(&o[..]) {
                        return Err(format!("copie de {c} : relue différente"));
                    }
                    json!({ "chemin": c, "existait": true, "copie": nom })
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({ "chemin": c, "existait": false }),
                Err(e) => return Err(format!("lecture de {c} : {e}")),
            };
            liste.push(entree);
        }
        fiche["fichiers"] = Value::Array(liste);
        let t = serde_json::to_vec_pretty(&fiche).map_err(|e| e.to_string())?;
        ecrire_sur(&self.fiche(), &t).map_err(|e| format!("fiche de la copie : {e}"))
    }

    /// Rend les anciens fichiers, puis efface la copie. Rend le nombre de fichiers remis.
    pub fn remettre(&self) -> Result<usize, String> {
        let fiche = self.lire_fiche().ok_or("aucune copie des anciens réglages")?;
        let liste = fiche["fichiers"].as_array().cloned().unwrap_or_default();
        let mut erreurs = Vec::new();
        let mut n = 0;
        for x in &liste {
            let Some(c) = x["chemin"].as_str() else { continue };
            let chemin = Path::new(c);
            if x["existait"].as_bool() == Some(true) {
                let nom = x["copie"].as_str().unwrap_or("");
                match std::fs::read(self.dossier.join(nom)) {
                    Ok(o) => match ecrire_sur(chemin, &o) {
                        Ok(()) if std::fs::read(chemin).ok().as_deref() == Some(&o[..]) => n += 1,
                        Ok(()) => erreurs.push(format!("{c} : relu différent")),
                        Err(e) => erreurs.push(format!("{c} : {e}")),
                    },
                    Err(e) => erreurs.push(format!("copie de {c} : {e}")),
                }
            } else {
                match std::fs::remove_file(chemin) {
                    Ok(()) => n += 1,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => n += 1,
                    Err(e) => erreurs.push(format!("{c} : {e}")),
                }
            }
        }
        if !erreurs.is_empty() {
            // (la copie est gardée : on peut réessayer)
            return Err(erreurs.join(" ; "));
        }
        let _ = std::fs::remove_dir_all(&self.dossier);
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nevermind-stream-copie-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn rend_les_octets_d_origine() {
        let d = dossier("octets");
        let a = d.join("basic.ini");
        let b = d.join("streamEncoder.json");
        let origine: Vec<u8> = b"\xEF\xBB\xBF[Video]\nBaseCX=1920\n\xC3\xA9t\xC3\xA9\r\n".to_vec();
        std::fs::write(&a, &origine).unwrap();
        let c = Copie::nouvelle(d.join("copie"));
        assert!(!c.existe());
        c.garder(&[a.clone(), b.clone()], "08/11/2026 à 23:00", "Sans titre").unwrap();
        assert!(c.existe());
        std::fs::write(&a, b"autre chose").unwrap();
        std::fs::write(&b, b"{}").unwrap();
        // un second « Optimiser » ne remplace pas la copie d'avant
        c.garder(&[a.clone(), b.clone()], "09/11/2026 à 10:00", "Sans titre").unwrap();
        assert_eq!(c.date().as_deref(), Some("08/11/2026 à 23:00"));
        assert_eq!(c.remettre().unwrap(), 2);
        assert_eq!(std::fs::read(&a).unwrap(), origine);
        assert!(!b.exists(), "un fichier qui n'existait pas est retiré");
        assert!(!c.existe());
        assert!(c.remettre().is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_fichier_ajoute_plus_tard_est_garde_tel_qu_il_etait() {
        let d = dossier("ajout");
        let a = d.join("basic.ini");
        let b = d.join("streamEncoder.json");
        std::fs::write(&a, b"A").unwrap();
        std::fs::write(&b, b"{\"x\":1}").unwrap();
        let c = Copie::nouvelle(d.join("copie"));
        c.garder(std::slice::from_ref(&a), "d", "p").unwrap();
        std::fs::write(&a, b"A2").unwrap();
        c.garder(&[a.clone(), b.clone()], "d2", "p").unwrap();
        std::fs::write(&b, b"{\"x\":2}").unwrap();
        assert_eq!(c.fichiers().len(), 2);
        c.remettre().unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), b"A");
        assert_eq!(std::fs::read(&b).unwrap(), b"{\"x\":1}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ecriture_d_un_coup() {
        let d = dossier("ecrire");
        let f = d.join("x.ini");
        ecrire_sur(&f, b"1").unwrap();
        ecrire_sur(&f, b"22").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"22");
        assert!(!d.join("x.ini.nevermind-tmp").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
