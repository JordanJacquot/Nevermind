//! L'heure, en ticks de .NET (100 ns depuis le 1er janvier de l'an 1), comme les dates que Nevermind attend.

use super::outils::DEBUT_FILETIME;
use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTimeAsFileTime};
use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime};

fn vers_filetime(ticks: i64) -> FILETIME {
    let f = (ticks - DEBUT_FILETIME).max(0) as u64;
    FILETIME { dwLowDateTime: f as u32, dwHighDateTime: (f >> 32) as u32 }
}

fn depuis_filetime(f: FILETIME) -> i64 {
    ((u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)) as i64 + DEBUT_FILETIME
}

/// Maintenant, en ticks UTC
pub fn ticks_utc() -> i64 {
    // SAFETY : simple lecture de l'horloge
    depuis_filetime(unsafe { GetSystemTimeAsFileTime() })
}

/// Une heure UTC en heure locale, selon les règles d'heure d'été de cette date-là
pub fn ticks_local(utc: i64) -> i64 {
    let f = vers_filetime(utc);
    let mut st = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    let mut fl = FILETIME::default();
    // SAFETY : conversions de dates par Windows, structures locales
    unsafe {
        if FileTimeToSystemTime(&f, &mut st).is_err()
            || SystemTimeToTzSpecificLocalTime(None, &st, &mut local).is_err()
            || SystemTimeToFileTime(&local, &mut fl).is_err()
        {
            return utc;
        }
    }
    // (SYSTEMTIME s'arrête à la milliseconde : seul le décalage est pris, appliqué à l'heure exacte)
    let mut ft = FILETIME::default();
    // SAFETY : comme plus haut
    if unsafe { SystemTimeToFileTime(&st, &mut ft) }.is_err() {
        return utc;
    }
    utc + (depuis_filetime(fl) - depuis_filetime(ft))
}

/// L'heure locale, « 2026-10-26T12:00:00 »
pub fn heure_locale() -> String {
    // SAFETY : simple lecture de l'horloge
    let t = unsafe { GetLocalTime() };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

/// Une date de fichier (FILETIME en nombre) en ticks UTC
pub fn ticks_fichier(f: u64) -> i64 {
    f as i64 + DEBUT_FILETIME
}
