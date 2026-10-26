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
