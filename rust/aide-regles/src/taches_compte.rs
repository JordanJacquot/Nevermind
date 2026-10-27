//! Tâches planifiées montrées par l'Aide (`Test-AideTacheDuCompte`, aide-actions.ps1) : seulement celles qui tournent
//! pour le compte qui demande (ou pour un groupe dont il fait partie d'office), et celles des comptes du système.
//! Jamais celles des autres comptes du PC.

use crate::texte;

/// SYSTEM, service local, service réseau
pub const COMPTES_SYSTEME: [&str; 3] = ["S-1-5-18", "S-1-5-19", "S-1-5-20"];
/// Tout le monde, utilisateurs interactifs, utilisateurs authentifiés, Utilisateurs
pub const GROUPES_DE_TOUS: [&str; 4] = ["S-1-1-0", "S-1-5-4", "S-1-5-11", "S-1-5-32-545"];

/// Un compte en SID (`ConvertTo-AideSid`) : tel quel s'il en est un, sinon traduit par Windows (`traduire`, comme
/// `NTAccount.Translate`) ; vide si inconnu.
pub fn vers_sid(compte: &str, traduire: &dyn Fn(&str) -> Option<String>) -> String {
    let c = texte::sans_espaces(compte);
    if c.is_empty() {
        return String::new();
    }
    if let Some(r) = texte::reste_apres(c, "S-1-")
        && !r.is_empty()
        && r.chars().all(|x| x.is_ascii_digit() || x == '-')
    {
        return c.to_string();
    }
    traduire(c).unwrap_or_default()
}

/// La tâche (compte `user_id`, groupe `group_id`) est-elle montrée au compte `sid` ?
pub fn montree(user_id: &str, group_id: &str, sid: &str, traduire: &dyn Fn(&str) -> Option<String>) -> bool {
    if sid.is_empty() {
        return false;
    }
    let u = vers_sid(user_id, traduire);
    if !u.is_empty() {
        return texte::egal(&u, sid) || COMPTES_SYSTEME.iter().any(|s| texte::egal(s, &u));
    }
    let g = vers_sid(group_id, traduire);
    !g.is_empty() && GROUPES_DE_TOUS.iter().any(|s| texte::egal(s, &g))
}
