//! Le journal Système (wevtapi, comme Get-WinEvent) : les écrans bleus, arrêts brutaux et erreurs matérielles des
//! 30 derniers jours, et le dernier test mémoire de Windows.

use serde_json::{Value, json};
use windows::Win32::System::EventLog::{
    EVT_HANDLE, EvtClose, EvtNext, EvtQuery, EvtQueryChannelPath, EvtQueryReverseDirection, EvtRender,
    EvtRenderEventXml,
};
use windows::core::HSTRING;

struct Poignee(EVT_HANDLE);
impl Drop for Poignee {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY : poignée ouverte par wevtapi, fermée une fois
            let _ = unsafe { EvtClose(self.0) };
        }
    }
}

fn requete(xpath: &str) -> Option<Poignee> {
    // SAFETY : textes valides le temps de l'appel
    unsafe {
        EvtQuery(
            None,
            &HSTRING::from("System"),
            &HSTRING::from(xpath),
            EvtQueryChannelPath.0 | EvtQueryReverseDirection.0,
        )
    }
    .ok()
    .map(Poignee)
}

fn suivants(q: &Poignee, max: usize) -> Vec<Poignee> {
    let mut l = Vec::new();
    loop {
        let mut lot = [0isize; 64];
        let mut n = 0u32;
        // SAFETY : tableau de poignées de la taille annoncée
        let ok = unsafe { EvtNext(q.0, &mut lot, 0, 0, &mut n) }.is_ok();
        if !ok || n == 0 {
            break;
        }
        for h in lot.iter().take(n as usize) {
            l.push(Poignee(EVT_HANDLE(*h)));
        }
        if l.len() >= max {
            break;
        }
    }
    l
}

fn depuis_30_jours(fournisseur: &str, filtre: &str) -> String {
    format!("*[System[Provider[@Name='{fournisseur}']{filtre} and TimeCreated[timediff(@SystemTime) <= 2592000000]]]")
}

/// `$r.Bsod`, `$r.Crash`, `$r.WheaErr`, `$r.WheaWarn`
pub fn comptes_30_jours() -> Vec<(&'static str, usize)> {
    let c = |x: String| requete(&x).map(|q| suivants(&q, 100_000).len()).unwrap_or(0);
    vec![
        ("Bsod", c(depuis_30_jours("Microsoft-Windows-WER-SystemErrorReporting", " and EventID=1001"))),
        ("Crash", c(depuis_30_jours("Microsoft-Windows-Kernel-Power", " and EventID=41"))),
        ("WheaErr", c(depuis_30_jours("Microsoft-Windows-WHEA-Logger", " and (Level=1 or Level=2)"))),
        ("WheaWarn", c(depuis_30_jours("Microsoft-Windows-WHEA-Logger", " and Level=3"))),
    ]
}

fn xml(e: &Poignee) -> String {
    let mut utilise = 0u32;
    let mut n = 0u32;
    // SAFETY : premier appel pour la taille, puis tampon de la taille demandée
    unsafe {
        let _ = EvtRender(None, e.0, EvtRenderEventXml.0, 0, None, &mut utilise, &mut n);
        let mut t = vec![0u16; (utilise as usize).div_ceil(2) + 1];
        if EvtRender(
            None,
            e.0,
            EvtRenderEventXml.0,
            (t.len() * 2) as u32,
            Some(t.as_mut_ptr().cast()),
            &mut utilise,
            &mut n,
        )
        .is_err()
        {
            return String::new();
        }
        let fin = t.iter().position(|c| *c == 0).unwrap_or(t.len());
        String::from_utf16_lossy(&t[..fin])
    }
}

/// `$r.MemDiag` : { Id, TimeCreated } du dernier résultat du test mémoire de Windows
pub fn dernier_test_memoire() -> Value {
    let Some(q) = requete("*[System[Provider[@Name='Microsoft-Windows-MemoryDiagnostics-Results']]]") else {
        return Value::Null;
    };
    let Some(e) = suivants(&q, 1).into_iter().next() else { return Value::Null };
    let x = xml(&e);
    let entre = |a: &str, b: &str| x.split_once(a).and_then(|(_, r)| r.split_once(b)).map(|(v, _)| v.to_string());
    let id = entre("<EventID", "</EventID>")
        .and_then(|v| v.rsplit('>').next().map(String::from))
        .and_then(|v| v.trim().parse::<i64>().ok());
    let quand = entre("SystemTime='", "'")
        .or_else(|| entre("SystemTime=\"", "\""))
        .map(|t| t.chars().take(19).collect::<String>());
    match id {
        Some(i) => json!({ "Id": i, "TimeCreated": quand }),
        None => Value::Null,
    }
}
