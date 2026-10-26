//! Points de restauration : un par 24 h au plus (`Test-AideRestaurationRecente`, aide-actions.ps1). De nombreux
//! points chasseraient les anciens, ceux qui servent.

/// Une heure en « ticks » de .NET (100 ns depuis le 1er janvier de l'an 1) par heure et par minute
pub const TICKS_HEURE: i64 = 36_000_000_000;
pub const TICKS_MINUTE: i64 = 600_000_000;

/// Un point a-t-il été créé il y a moins de 24 h ? `derniere` et `maintenant` : en ticks UTC (0 : aucun point noté).
/// Un point noté plus de 5 minutes dans le futur (horloge changée) ne compte pas.
pub fn recente(derniere: i64, maintenant: i64) -> bool {
    if derniere <= 0 {
        return false;
    }
    let ecart = maintenant - derniere;
    ecart < 24 * TICKS_HEURE && ecart > -5 * TICKS_MINUTE
}
