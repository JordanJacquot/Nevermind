//! L'heure locale du PC, comme `Get-Date`, et ses formes écrites par Nevermind (journal, fichiers mis de côté,
//! historique, nom du fichier « Signaler un problème »).

use std::sync::Arc;
pub use suite_commun::json_sur::HeureLocale;

/// Donne l'heure locale (celle du PC ; une horloge figée dans les tests).
pub type Horloge = Arc<dyn Fn() -> HeureLocale + Send + Sync>;

/// L'heure locale du PC (Windows : GetLocalTime, par la bibliothèque commune).
pub fn maintenant() -> HeureLocale {
    suite_commun::json_sur::heure_du_pc()
}

/// L'horloge du PC.
pub fn horloge_du_pc() -> Horloge {
    Arc::new(maintenant)
}

/// Une horloge arrêtée sur ce moment (les tests).
pub fn horloge_figee(h: HeureLocale) -> Horloge {
    Arc::new(move || h)
}

/// `Get-Date -Format s` : « 2026-11-06T21:05:09 ».
pub fn iso(h: &HeureLocale) -> String {
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", h.annee, h.mois, h.jour, h.heure, h.minute, h.seconde)
}

/// `Get-Date -Format 'yyyyMMdd-HHmmss'` : « 20261106-210509 ».
pub fn compacte(h: &HeureLocale) -> String {
    h.compacte()
}

/// `(Get-Date).ToString('dd/MM/yyyy à HH:mm')` : la date d'une ligne de l'historique.
pub fn date_historique(h: &HeureLocale) -> String {
    format!("{:02}/{:02}/{:04} à {:02}:{:02}", h.jour, h.mois, h.annee, h.heure, h.minute)
}

/// `Get-Date -Format 'dd/MM/yyyy HH:mm'`.
pub fn date_courte(h: &HeureLocale) -> String {
    format!("{:02}/{:02}/{:04} {:02}:{:02}", h.jour, h.mois, h.annee, h.heure, h.minute)
}

/// `Get-Date -Format 'yyyy-MM-dd HH.mm'` (le nom du fichier « Signaler un problème »).
pub fn date_fichier(h: &HeureLocale) -> String {
    format!("{:04}-{:02}-{:02} {:02}.{:02}", h.annee, h.mois, h.jour, h.heure, h.minute)
}

/// Lit « 2026-11-06T21:05:09 » (la forme `iso`).
pub fn lire_iso(t: &str) -> Option<HeureLocale> {
    let b = t.as_bytes();
    if b.len() != 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let n = |a: usize, z: usize| t.get(a..z)?.parse::<u32>().ok();
    Some(HeureLocale {
        annee: n(0, 4)? as i32,
        mois: n(5, 7)?,
        jour: n(8, 10)?,
        heure: n(11, 13)?,
        minute: n(14, 16)?,
        seconde: n(17, 19)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formes() {
        let h = HeureLocale { annee: 2026, mois: 1, jour: 2, heure: 3, minute: 4, seconde: 5 };
        assert_eq!(iso(&h), "2026-01-02T03:04:05");
        assert_eq!(compacte(&h), "20260102-030405");
        assert_eq!(date_historique(&h), "02/01/2026 à 03:04");
        assert_eq!(date_fichier(&h), "2026-01-02 03.04");
        assert_eq!(lire_iso("2026-01-02T03:04:05"), Some(h));
    }

    #[test]
    fn heure_locale_pas_utc() {
        // (même lecture que Get-Date : l'heure du PC, à la seconde près)
        let h = maintenant();
        assert!(h.annee >= 2026 && (1..=12).contains(&h.mois));
    }
}
