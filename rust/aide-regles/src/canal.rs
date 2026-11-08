//! Nom du canal et du verrou de l'Aide : propres au compte servi et au dossier de cette copie de Nevermind
//! (`Get-AideSuffixe`, aide-actions.ps1 ; `$NomTuyau` et le verrou, NevermindAide.ps1 ; `AideClient.Nom()`, natif.cs).
//! Une copie de test ne parle jamais à l'Aide de la copie installée, et chacune a la sienne.

use crate::{chemin, sha256, texte};

/// Le suffixe : les 6 premiers octets (12 caractères hexadécimaux) du SHA-256 du dossier complet, sans « \ » final,
/// en minuscules comme .NET. `None` si le dossier n'est pas un chemin « X:\... ».
pub fn suffixe(dossier: &str) -> Option<String> {
    let plein = chemin::complet(dossier)?;
    let d = texte::minuscules(plein.trim_end_matches('\\'));
    let e = sha256::empreinte(d.as_bytes());
    Some(e[..6].iter().map(|o| format!("{o:02x}")).collect())
}

/// Le nom du canal : `Nevermind-Aide-<sid>-<suffixe>`
pub fn nom_canal(sid: &str, suffixe: &str) -> String {
    format!("Nevermind-Aide-{sid}-{suffixe}")
}

/// Le nom du verrou (une seule Aide à la fois) : `Local\Nevermind-Aide-<sid>-<suffixe>`
pub fn nom_verrou(sid: &str, suffixe: &str) -> String {
    format!("Local\\Nevermind-Aide-{sid}-{suffixe}")
}

/// Les noms communs des sessions de mesure, d'avant l'audit du 08/11 (toutes les Aides avaient les mêmes) : ceux d'une
/// Aide qui n'a pas encore nommé les siennes
pub const SESSION_FPS_COMMUNE: &str = "Nevermind";
pub const SESSION_RESEAU_COMMUNE: &str = "Nevermind-Reseau";

/// Le nom de la session de mesure des FPS (PresentMon) de cette Aide : `Nevermind-<sid>-<suffixe>`. Propre au compte
/// servi et à la copie, comme le canal (audit du 08/11) : deux Aides (deux comptes ouverts en même temps, une copie
/// d'essai à côté de la copie installée) ne se coupent plus la mesure l'une de l'autre.
pub fn nom_session_fps(sid: &str, suffixe: &str) -> String {
    format!("Nevermind-{sid}-{suffixe}")
}

/// Le nom de la session de la trace réseau (ETW) de cette Aide : `Nevermind-Reseau-<sid>-<suffixe>`
pub fn nom_session_reseau(sid: &str, suffixe: &str) -> String {
    format!("Nevermind-Reseau-{sid}-{suffixe}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_propres_a_chaque_aide() {
        let (a, b) = ("S-1-5-21-1-2-3-1001", "S-1-5-21-1-2-3-1002");
        let s = suffixe("C:\\Mindset\\Programmes\\Nevermind").unwrap();
        let t = suffixe("D:\\Essai\\Nevermind").unwrap();
        assert_eq!(nom_session_fps(a, &s), format!("Nevermind-{a}-{s}"));
        assert_eq!(nom_session_reseau(a, &s), format!("Nevermind-Reseau-{a}-{s}"));
        // Deux comptes, ou deux copies : quatre noms différents, jamais les noms communs
        let noms = [nom_session_fps(a, &s), nom_session_fps(b, &s), nom_session_fps(a, &t), nom_session_reseau(a, &s)];
        for (i, x) in noms.iter().enumerate() {
            assert!(noms.iter().skip(i + 1).all(|y| y != x), "{noms:?}");
            assert!(x != SESSION_FPS_COMMUNE && x != SESSION_RESEAU_COMMUNE);
            // (un nom sans espace ni guillemet : il passe tel quel dans la ligne de commande de PresentMon, et il
            // tient dans la place réservée au nom d'une session ETW, 512 caractères)
            assert!(x.len() < 100 && x.bytes().all(|o| o.is_ascii_alphanumeric() || o == b'-'), "{x}");
        }
    }
}
