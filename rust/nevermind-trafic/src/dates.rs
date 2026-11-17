//! Les dates du bloc trafic, sans bibliothèque de dates : chrono n'est plus entretenue (RUSTSEC-2026-0543,
//! 17/11/2026). Les calculs de calendrier sont faits ici ; l'heure locale (heure d'été comprise) vient de Windows.

/// Une date en heure locale, avec l'écart à l'heure universelle de ce moment-là.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub annee: i32,
    pub mois: u32,
    pub jour: u32,
    pub heure: u32,
    pub minute: u32,
    pub seconde: u32,
    /// Les centaines de nanosecondes (les 7 chiffres du format « o » de .NET)
    pub fraction: u32,
    /// L'écart avec l'heure universelle, en minutes (+60 en hiver en France, +120 en été)
    pub decalage: i32,
}

/// Le nombre de jours depuis le 01/01/1970 (calendrier grégorien, années négatives comprises).
pub fn jours_depuis_civil(annee: i32, mois: u32, jour: u32) -> i64 {
    let a = i64::from(annee) - i64::from(mois <= 2);
    let ere = a.div_euclid(400);
    let ae = a - ere * 400;
    let m = i64::from(mois);
    let jour_annee = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(jour) - 1;
    let jour_ere = ae * 365 + ae / 4 - ae / 100 + jour_annee;
    ere * 146_097 + jour_ere - 719_468
}

/// L'inverse de `jours_depuis_civil` : (année, mois, jour).
pub fn civil_depuis_jours(jours: i64) -> (i32, u32, u32) {
    let z = jours + 719_468;
    let ere = z.div_euclid(146_097);
    let je = z - ere * 146_097;
    let ae = (je - je / 1460 + je / 36_524 - je / 146_096) / 365;
    let ja = je - (365 * ae + ae / 4 - ae / 100);
    let mp = (5 * ja + 2) / 153;
    let jour = (ja - (153 * mp + 2) / 5 + 1) as u32;
    let mois = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((ae + ere * 400 + i64::from(mois <= 2)) as i32, mois, jour)
}

fn jours_du_mois(annee: i32, mois: u32) -> u32 {
    match mois {
        2 if (annee % 4 == 0 && annee % 100 != 0) || annee % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

impl Date {
    fn depuis_secondes_locales(s: i64, fraction: u32, decalage: i32) -> Date {
        let (annee, mois, jour) = civil_depuis_jours(s.div_euclid(86_400));
        let r = s.rem_euclid(86_400) as u32;
        Date { annee, mois, jour, heure: r / 3600, minute: r % 3600 / 60, seconde: r % 60, fraction, decalage }
    }

    /// Les secondes depuis le 01/01/1970 à minuit, en heure locale.
    fn secondes_locales(&self) -> i64 {
        jours_depuis_civil(self.annee, self.mois, self.jour) * 86_400
            + i64::from(self.heure * 3600 + self.minute * 60 + self.seconde)
    }

    /// Le jour (nombre de jours depuis le 01/01/1970), pour compter des écarts en jours.
    pub fn jours(&self) -> i64 {
        jours_depuis_civil(self.annee, self.mois, self.jour)
    }

    /// Pour trier dans l'ordre du temps (en centaines de nanosecondes, heure universelle).
    pub fn cle(&self) -> i64 {
        (self.secondes_locales() - i64::from(self.decalage) * 60) * 10_000_000 + i64::from(self.fraction)
    }

    /// « 06/11 »
    pub fn jour_mois(&self) -> String {
        format!("{:02}/{:02}", self.jour, self.mois)
    }

    /// « 21:03 »
    pub fn heure_minute(&self) -> String {
        format!("{:02}:{:02}", self.heure, self.minute)
    }

    /// « 06/11/2026 »
    pub fn jour_mois_annee(&self) -> String {
        format!("{:02}/{:02}/{:04}", self.jour, self.mois, self.annee)
    }

    /// « 06/11/2026 à 21:03 »
    pub fn complete(&self) -> String {
        format!("{} à {}", self.jour_mois_annee(), self.heure_minute())
    }

    /// « 2026-11-06 »
    pub fn iso_jour(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.annee, self.mois, self.jour)
    }

    /// « 20261106-210312 »
    pub fn compacte(&self) -> String {
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            self.annee, self.mois, self.jour, self.heure, self.minute, self.seconde
        )
    }

    /// Le format « o » de .NET : « 2026-11-06T21:03:12.1234567+01:00 »
    pub fn format_o(&self) -> String {
        let d = self.decalage.unsigned_abs();
        format!(
            "{}T{:02}:{:02}:{:02}.{:07}{}{:02}:{:02}",
            self.iso_jour(),
            self.heure,
            self.minute,
            self.seconde,
            self.fraction,
            if self.decalage < 0 { '-' } else { '+' },
            d / 60,
            d % 60
        )
    }
}

/// L'écart avec l'heure universelle du PC à ce moment (secondes depuis 1970, heure universelle), en minutes.
pub fn decalage_local(utc: i64) -> i32 {
    let u = Date::depuis_secondes_locales(utc, 0, 0);
    if !(1601..=30_827).contains(&u.annee) {
        return 0;
    }
    let champs = [u.annee as u32, u.mois, u.jour, u.heure, u.minute, u.seconde].map(|x| x as u16);
    let Some(l) = crate::windows::heure_locale(champs) else {
        return 0;
    };
    let local = Date {
        annee: i32::from(l[0]),
        mois: u32::from(l[1]),
        jour: u32::from(l[2]),
        heure: u32::from(l[3]),
        minute: u32::from(l[4]),
        seconde: u32::from(l[5]),
        fraction: 0,
        decalage: 0,
    };
    ((local.secondes_locales() - utc) / 60) as i32
}

/// Ce moment (secondes depuis 1970 en heure universelle) en heure locale du PC.
pub fn a_l_heure_locale(utc: i64, fraction: u32) -> Date {
    let d = decalage_local(utc);
    Date::depuis_secondes_locales(utc + i64::from(d) * 60, fraction, d)
}

/// Maintenant, en heure locale du PC.
pub fn maintenant() -> Date {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    a_l_heure_locale(t.as_secs() as i64, t.subsec_nanos() / 100)
}

/// Un nombre d'au moins 1 chiffre, au plus `max` chiffres, au début de `t` : (valeur, reste).
fn nombre(t: &str, max: usize) -> Option<(u32, &str)> {
    let n = t.bytes().take(max).take_while(u8::is_ascii_digit).count();
    if n == 0 {
        return None;
    }
    Some((t[..n].parse().ok()?, &t[n..]))
}

fn attendre<'a>(t: &'a str, c: &[u8]) -> Option<&'a str> {
    t.as_bytes().first().filter(|b| c.contains(b)).map(|_| &t[1..])
}

/// Lit « 2026-11-06 » (le jour seulement).
pub fn lire_jour(t: &str) -> Option<i64> {
    let (a, t) = nombre(t, 4)?;
    let (m, t) = nombre(attendre(t, b"-")?, 2)?;
    let (j, t) = nombre(attendre(t, b"-")?, 2)?;
    let a = a as i32;
    (t.is_empty() && (1..=12).contains(&m) && (1..=jours_du_mois(a, m)).contains(&j))
        .then(|| jours_depuis_civil(a, m, j))
}

/// `[datetime]$x` : une date écrite par les mesures, en heure locale du PC. Avec un écart (« Z », « +01:00 ») elle
/// est ramenée à l'heure du PC ; sans écart, elle y est déjà. « 2026-11-06T21:03:12.1234567+01:00 »,
/// « 2026-11-06T21:03:12 », « 2026-11-06 21:03:12 ».
pub fn lire(t: &str) -> Option<Date> {
    let (a, t) = nombre(t, 4)?;
    let (mo, t) = nombre(attendre(t, b"-")?, 2)?;
    let (j, t) = nombre(attendre(t, b"-")?, 2)?;
    let (h, t) = nombre(attendre(t, b"Tt ")?, 2)?;
    let (mi, t) = nombre(attendre(t, b":")?, 2)?;
    let (s, mut t) = nombre(attendre(t, b":")?, 2)?;
    let a = a as i32;
    if !(1..=12).contains(&mo) || !(1..=jours_du_mois(a, mo)).contains(&j) || h > 23 || mi > 59 || s > 59 {
        return None;
    }
    let mut fraction = 0;
    if let Some(r) = attendre(t, b".") {
        let n = r.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 {
            return None;
        }
        let chiffres: String = r[..n].chars().chain(std::iter::repeat('0')).take(7).collect();
        fraction = chiffres.parse().ok()?;
        t = &r[n..];
    }
    let base = Date { annee: a, mois: mo, jour: j, heure: h, minute: mi, seconde: s, fraction, decalage: 0 };
    let ecrite = base.secondes_locales();
    let ecart = if t.is_empty() {
        None
    } else if t == "Z" || t == "z" {
        Some(0)
    } else {
        let signe = if t.starts_with('-') { -1 } else { 1 };
        let (eh, r) = nombre(attendre(t, b"+-")?, 2)?;
        let (em, r) = nombre(attendre(r, b":")?, 2)?;
        if !r.is_empty() || eh > 23 || em > 59 {
            return None;
        }
        Some(signe * (eh * 60 + em) as i32)
    };
    Some(match ecart {
        Some(e) => a_l_heure_locale(ecrite - i64::from(e) * 60, fraction),
        None => {
            let d = decalage_local(ecrite - i64::from(decalage_local(ecrite)) * 60);
            Date { decalage: d, ..base }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendrier() {
        assert_eq!(jours_depuis_civil(1970, 1, 1), 0);
        assert_eq!(jours_depuis_civil(2000, 3, 1), 11_017);
        assert_eq!(jours_depuis_civil(2026, 11, 17), 20_774);
        for j in [-800_000, -1, 0, 59, 11_016, 20_774, 2_000_000] {
            let (a, m, d) = civil_depuis_jours(j);
            assert_eq!(jours_depuis_civil(a, m, d), j);
        }
        assert_eq!(civil_depuis_jours(11_016), (2000, 2, 29));
    }

    #[test]
    fn formats() {
        let d =
            Date { annee: 2026, mois: 1, jour: 2, heure: 3, minute: 4, seconde: 5, fraction: 1_234_567, decalage: 60 };
        assert_eq!(d.format_o(), "2026-01-02T03:04:05.1234567+01:00");
        assert_eq!(Date { decalage: -330, fraction: 0, ..d }.format_o(), "2026-01-02T03:04:05.0000000-05:30");
        assert_eq!(d.compacte(), "20260102-030405");
        assert_eq!(d.iso_jour(), "2026-01-02");
        assert_eq!(d.complete(), "02/01/2026 à 03:04");
        assert_eq!((d.jour_mois(), d.heure_minute()), ("02/01".to_string(), "03:04".to_string()));
    }

    #[test]
    fn lectures() {
        // Sans écart : déjà à l'heure du PC
        let d = lire("2026-11-06T21:03:12").unwrap();
        assert_eq!((d.annee, d.mois, d.jour, d.heure, d.minute, d.seconde, d.fraction), (2026, 11, 6, 21, 3, 12, 0));
        assert_eq!(lire("2026-11-06 21:03:12").unwrap().heure_minute(), "21:03");
        assert_eq!(lire("2026-11-06T21:03:12.5").unwrap().fraction, 5_000_000);
        assert_eq!(lire("2026-11-06T21:03:12.123456789").unwrap().fraction, 1_234_567);
        // Avec un écart : ramenée à l'heure du PC, le même instant
        let z = lire("2026-11-06T20:03:12Z").unwrap();
        let p = lire("2026-11-06T21:03:12.0000000+01:00").unwrap();
        let m = lire("2026-11-06T15:03:12-05:00").unwrap();
        assert_eq!((z.cle(), p.cle()), (m.cle(), m.cle()));
        assert_eq!(z.decalage, decalage_local(z.cle() / 10_000_000));
        assert_eq!(z, a_l_heure_locale(jours_depuis_civil(2026, 11, 6) * 86_400 + 20 * 3600 + 3 * 60 + 12, 0));
        // Refusées
        for x in [
            "",
            "2026-11-06",
            "2026-13-06T21:03:12",
            "2026-02-30T01:00:00",
            "2026-11-06T24:00:00",
            "2026-11-06T21:03:12+",
            "2026-11-06T21:03:12.",
            "2026-11-06T21:03:12 x",
            "hier",
        ] {
            assert_eq!(lire(x), None, "{x}");
        }
        // L'ordre du temps
        assert!(lire("2026-11-06T21:03:12").unwrap().cle() < lire("2026-11-06T21:03:13").unwrap().cle());
        assert!(lire("2025-12-31T23:59:59").unwrap().cle() < lire("2026-01-01 00:00:00").unwrap().cle());
    }

    #[test]
    fn jours() {
        assert_eq!(lire_jour("2026-11-17"), Some(20_774));
        assert_eq!(lire_jour("2026-1-5"), Some(jours_depuis_civil(2026, 1, 5)));
        for x in ["", "2026-11", "2026-11-31", "2026-11-17x", "x"] {
            assert_eq!(lire_jour(x), None, "{x}");
        }
        let m = maintenant();
        assert!(m.annee >= 2026 && (-12 * 60..=14 * 60).contains(&m.decalage));
        // La même heure que celle de Windows (GetLocalTime), à quelques secondes près
        let w = nevermind_noyau::moment::maintenant();
        let w = Date {
            annee: w.annee,
            mois: w.mois,
            jour: w.jour,
            heure: w.heure,
            minute: w.minute,
            seconde: w.seconde,
            fraction: 0,
            decalage: 0,
        };
        assert!((w.secondes_locales() - m.secondes_locales()).abs() <= 5, "{w:?} {m:?}");
        assert_eq!(lire(&m.format_o()).unwrap().cle(), m.cle());
    }
}
