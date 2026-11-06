//! Les dates de l'analyse (pilote, BIOS, test mémoire, allumage du PC) : un moment à la seconde, heure locale,
//! comme les `[datetime]` de PowerShell (sans fuseau).

/// Un moment, en secondes depuis le 1er janvier 1970 (heure locale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Moment(pub i64);

impl Moment {
    /// « 2026-11-06T12:00:00 » (les secondes et ce qui suit sont facultatifs), `None` si illisible.
    pub fn lire(s: &str) -> Option<Moment> {
        let s = s.trim();
        let (d, h) = match s.split_once(['T', ' ']) {
            Some((d, h)) => (d, h),
            None => (s, "00:00:00"),
        };
        let mut p = d.split('-');
        let a: i64 = p.next()?.parse().ok()?;
        let m: i64 = p.next()?.parse().ok()?;
        let j: i64 = p.next()?.parse().ok()?;
        let h = h.trim_end_matches('Z');
        let h = h.split(['+', '.']).next().unwrap_or("");
        let mut q = h.split(':');
        let hh: i64 = q.next().unwrap_or("0").parse().ok()?;
        let mm: i64 = q.next().unwrap_or("0").parse().ok()?;
        let ss: i64 = q.next().unwrap_or("0").parse().ok()?;
        if !(1..=12).contains(&m) || !(1..=31).contains(&j) {
            return None;
        }
        Some(Moment(jours_depuis_civil(a, m, j) * 86400 + hh * 3600 + mm * 60 + ss))
    }

    pub fn jours(self, n: i64) -> Moment {
        Moment(self.0 + n * 86400)
    }

    /// (année, mois, jour, heure, minute, seconde)
    pub fn parties(self) -> (i64, i64, i64, i64, i64, i64) {
        let j = self.0.div_euclid(86400);
        let r = self.0.rem_euclid(86400);
        let (a, m, d) = civil_depuis_jours(j);
        (a, m, d, r / 3600, (r % 3600) / 60, r % 60)
    }

    /// « 06/11/2026 »
    pub fn jjmmaaaa(self) -> String {
        let (a, m, d, ..) = self.parties();
        format!("{d:02}/{m:02}/{a:04}")
    }

    /// « 2026-11-06T12:00:00 »
    pub fn iso(self) -> String {
        let (a, m, d, h, mi, s) = self.parties();
        format!("{a:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}")
    }

    /// Durée jusqu'à `plus_tard`, en secondes.
    pub fn jusqua(self, plus_tard: Moment) -> i64 {
        plus_tard.0 - self.0
    }
}

/// `(Get-Date) - $d` en jours entiers (`.Days` d'un TimeSpan : tronqué vers zéro).
pub fn jours_entre(avant: Moment, apres: Moment) -> i64 {
    (apres.0 - avant.0) / 86400
}

fn jours_depuis_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_depuis_jours(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        let m = Moment::lire("2026-11-06T12:00:00").unwrap();
        assert_eq!(m.iso(), "2026-11-06T12:00:00");
        assert_eq!(m.jjmmaaaa(), "06/11/2026");
        assert_eq!(m.jours(-365).jjmmaaaa(), "06/11/2025");
        assert_eq!(Moment::lire("2024-02-29").unwrap().jjmmaaaa(), "29/02/2024");
    }
}
