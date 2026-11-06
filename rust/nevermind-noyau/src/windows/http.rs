//! Le réseau par WinHTTP, la bibliothèque de Windows (https compris, avec les réglages de proxy du PC) : une demande
//! GET, sa réponse entière. Comme `Invoke-RestMethod` / `Invoke-WebRequest` avec `User-Agent: Nevermind` et un délai.
//! Les redirections (GitHub vers ses serveurs de fichiers) sont suivies par WinHTTP. Une adresse 127.0.0.1 (les
//! tests, avec leur faux serveur) passe sans proxy.

use std::ffi::c_void;
use windows::Win32::Networking::WinHttp::{
    WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPEN_REQUEST_FLAGS,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen,
    WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts,
};
use windows::core::{HSTRING, PCWSTR};

/// Une poignée WinHTTP, fermée quand on la lâche
struct P(*mut c_void);

impl P {
    fn de(p: *mut c_void) -> Result<P, String> {
        if p.is_null() { Err(super::message(&windows::core::Error::from_thread())) } else { Ok(P(p)) }
    }
}

impl Drop for P {
    fn drop(&mut self) {
        // SAFETY : une poignée ouverte par WinHttpOpen, WinHttpConnect ou WinHttpOpenRequest, fermée une seule fois
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}

/// Une adresse découpée : https ?, l'hôte, le port, le chemin
pub fn decouper(url: &str) -> Result<(bool, String, u16, String), String> {
    let (securise, reste) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(format!("adresse inattendue : {url}"));
    };
    let (hote_port, chemin) = match reste.find('/') {
        Some(i) => (&reste[..i], &reste[i..]),
        None => (reste, "/"),
    };
    let (hote, port) = match hote_port.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().map_err(|_| format!("adresse inattendue : {url}"))?),
        None => (hote_port, if securise { 443 } else { 80 }),
    };
    if hote.is_empty() {
        return Err(format!("adresse inattendue : {url}"));
    }
    Ok((securise, hote.to_string(), port, chemin.to_string()))
}

/// GET : le code de réponse et le contenu. `delai_s` : comme `-TimeoutSec`.
pub fn obtenir(url: &str, delai_s: u32) -> Result<(u16, Vec<u8>), String> {
    let (securise, hote, port, chemin) = decouper(url)?;
    let local = hote == "127.0.0.1" || hote.eq_ignore_ascii_case("localhost");
    let acces = if local { WINHTTP_ACCESS_TYPE_NO_PROXY } else { WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY };
    let ms = (delai_s.max(1) * 1000) as i32;
    // SAFETY : des chaînes terminées par un zéro, vivantes pendant chaque appel ; les poignées sont refermées par P
    unsafe {
        let session = P::de(WinHttpOpen(&HSTRING::from("Nevermind"), acces, PCWSTR::null(), PCWSTR::null(), 0))?;
        WinHttpSetTimeouts(session.0, ms, ms, ms, ms).map_err(|e| super::message(&e))?;
        let connexion = P::de(WinHttpConnect(session.0, &HSTRING::from(hote), port, 0))?;
        let drapeaux = if securise { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
        let demande = P::de(WinHttpOpenRequest(
            connexion.0,
            &HSTRING::from("GET"),
            &HSTRING::from(chemin),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            drapeaux,
        ))?;
        WinHttpSendRequest(demande.0, None, None, 0, 0, 0).map_err(|e| super::message(&e))?;
        WinHttpReceiveResponse(demande.0, std::ptr::null_mut()).map_err(|e| super::message(&e))?;
        let mut statut = 0u32;
        let mut longueur = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            demande.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut statut as *mut u32 as *mut c_void),
            &mut longueur,
            std::ptr::null_mut(),
        )
        .map_err(|e| super::message(&e))?;
        let mut corps = Vec::new();
        let mut tampon = vec![0u8; 65536];
        loop {
            let mut lu = 0u32;
            WinHttpReadData(demande.0, tampon.as_mut_ptr() as *mut c_void, tampon.len() as u32, &mut lu)
                .map_err(|e| super::message(&e))?;
            if lu == 0 {
                break;
            }
            corps.extend_from_slice(&tampon[..lu as usize]);
        }
        drop(demande);
        drop(connexion);
        drop(session);
        Ok((statut as u16, corps))
    }
}
