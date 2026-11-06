//! Le panneau d'une analyse de Microsoft Defender (`Invoke-DefenderScan`, securite.ps1:494) tel que l'écran le reçoit :
//! `Show-TestPanel` puis la traduction de `Get-PanneauKit` (ecran-panneau.ps1), comme le panneau d'un test du bloc pc.
//! Le radar animé n'est pas photographié (« SansImageKit ») : l'état, le temps et les verdicts suffisent.

use serde_json::{Value, json};
use std::time::Instant;

/// Le panneau ouvert : ce que l'analyse en a dit (`etat`, `titre`, `sous`, `texte` ou `resultat`, `temps`, `lignes`,
/// `boutons`), son début (le temps en cours), et son numéro (`$script:Kit.PanneauNumero`).
#[derive(Debug, Clone)]
pub struct PanneauAnalyse {
    pub donnees: Value,
    pub debut: Instant,
    pub numero: u64,
}

fn texte(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

/// `New-Verdict` traduit (un encadré coloré d'un seul texte) : vert « ok », orange « warn », rouge « bad », une note
/// sinon.
fn verdict(statut: &str, t: &str) -> Value {
    match statut {
        "ok" => json!({ "type": "ligne", "icone": "ok", "couleur": "vert", "texte": t }),
        "warn" => json!({ "type": "ligne", "icone": "alerte", "couleur": "orange", "texte": t }),
        "bad" => json!({ "type": "ligne", "icone": "alerte", "couleur": "rouge", "texte": t }),
        _ => json!({ "type": "note", "texte": t }),
    }
}

impl PanneauAnalyse {
    pub fn en_cours(&self) -> bool {
        texte(&self.donnees, "etat") == "en-cours"
    }

    /// La page du panneau (titre, sous, icone, module, blocs, numero).
    pub fn page(&self) -> Value {
        let d = &self.donnees;
        let etat = texte(d, "etat");
        let en_cours = etat == "en-cours";
        let temps = if en_cours {
            let s = self.debut.elapsed().as_secs();
            format!("{:02}:{:02}", s / 60, s % 60)
        } else {
            texte(d, "temps")
        };
        // Set-TestState : run (Analyse en cours), info (Arrêtée), bad (Échec, menaces), warn (inconnu), ok (aucune)
        let (statut, etat_texte) = match etat.as_str() {
            "en-cours" => ("run", "Analyse en cours".to_string()),
            "arretee" => ("info", texte(d, "resultat")),
            "echec" | "menaces" => ("bad", texte(d, "resultat")),
            "inconnu" => ("warn", texte(d, "resultat")),
            _ => ("ok", texte(d, "resultat")),
        };
        let mut blocs = vec![json!({ "type": "chiffres", "elements": [{ "titre": "", "valeur": temps, "ton": "" }] })];
        if en_cours {
            let t = texte(d, "texte");
            if !t.is_empty() {
                blocs.push(json!({ "type": "texte", "texte": t }));
            }
        } else {
            let v = if etat == "menaces" { "bad" } else { statut };
            for l in d.get("lignes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                blocs.push(verdict(v, l));
            }
            for (n, b) in d.get("boutons").and_then(Value::as_array).into_iter().flatten().enumerate() {
                blocs.push(json!({ "type": "bouton", "id": format!("panneau:b:{n}"), "texte": texte(b, "texte") }));
            }
        }
        let mut boutons = Vec::new();
        if en_cours {
            boutons.push(json!({ "id": "panneau:stop", "texte": "Arrêter le test", "style": "danger" }));
        } else {
            boutons.push(json!({ "id": "panneau:close", "texte": "Fermer", "style": "second" }));
        }
        let (icone, couleur) = match statut {
            "ok" => ("ok", "vert"),
            "warn" => ("alerte", "orange"),
            "bad" => ("alerte", "rouge"),
            _ => ("bouclier", "P"),
        };
        json!({
            "titre": texte(d, "titre"), "sous": texte(d, "sous"), "icone": "bouclier", "couleur": "P",
            "module": { "icone": icone, "couleur": couleur, "titre": etat_texte, "texte": texte(d, "sous"), "boutons": boutons },
            "blocs": blocs, "numero": self.numero,
        })
    }

    /// L'action d'un bouton du panneau (`panneau:b:<n>`) : celle que l'analyse a donnée.
    pub fn bouton(&self, n: usize) -> Option<String> {
        let b = self.donnees.get("boutons")?.as_array()?.get(n)?;
        Some(texte(b, "id")).filter(|i| !i.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panneau_en_cours_puis_menaces() {
        let p = PanneauAnalyse {
            donnees: json!({ "etat": "en-cours", "titre": "Analyse rapide", "sous": "Microsoft Defender", "texte": "Analyse des endroits." }),
            debut: Instant::now(),
            numero: 3,
        };
        let v = p.page();
        assert_eq!(v["module"]["titre"], "Analyse en cours");
        assert_eq!(v["module"]["boutons"][0]["id"], "panneau:stop");
        assert_eq!(v["blocs"][0]["elements"][0]["valeur"], "00:00");
        assert_eq!(v["blocs"][1]["texte"], "Analyse des endroits.");
        assert_eq!(v["numero"], 3);
        let p = PanneauAnalyse {
            donnees: json!({ "etat": "menaces", "titre": "Analyse rapide", "sous": "Microsoft Defender", "resultat": "1 menace trouvée",
                "temps": "02:05", "lignes": ["Trojan   C:\\x.exe"], "boutons": [{ "id": "sec-supprimer-trouvees", "texte": "Supprimer les menaces" }] }),
            debut: Instant::now(),
            numero: 3,
        };
        let v = p.page();
        assert_eq!(v["module"]["couleur"], "rouge");
        assert_eq!(v["module"]["boutons"][0]["id"], "panneau:close");
        assert_eq!(v["blocs"][0]["elements"][0]["valeur"], "02:05");
        assert_eq!(v["blocs"][1]["couleur"], "rouge");
        assert_eq!(v["blocs"][2]["id"], "panneau:b:0");
        assert_eq!(p.bouton(0).as_deref(), Some("sec-supprimer-trouvees"));
    }
}
