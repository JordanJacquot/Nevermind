//! La porte officielle de Streamlabs Desktop : son API sur le tube nommé `\\.\pipe\slobs` (JSON-RPC 2.0, un message
//! par ligne), celle dont se servent les applis compagnons (Stream Deck...). Elle ne répond que quand Streamlabs est
//! ouvert, sous le compte de l'utilisateur, sans droits administrateur.
//!
//! Une demande attend sa réponse au plus [`DELAI`] : un Streamlabs qui ne répond pas ne bloque jamais Nevermind (la
//! lecture se fait sur un fil à part, abandonné après le délai). Les messages qui ne sont pas la réponse (les
//! événements de Streamlabs) sont ignorés.

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Le tube de Streamlabs Desktop
pub const TUBE: &str = r"\\.\pipe\slobs";
/// Le temps laissé à Streamlabs pour répondre à une demande
pub const DELAI: Duration = Duration::from_secs(8);
/// Au plus ce nombre d'octets lus pour une réponse (le formulaire Sortie fait quelques dizaines de ko)
const TAILLE_MAX: usize = 8 * 1024 * 1024;

static NUMERO: AtomicU64 = AtomicU64::new(1);

/// La demande JSON-RPC (une ligne)
pub fn demande(id: u64, ressource: &str, methode: &str, args: &[Value]) -> Vec<u8> {
    let mut params = json!({ "resource": ressource });
    if !args.is_empty() {
        params["args"] = Value::Array(args.to_vec());
    }
    let mut o = serde_json::to_vec(&json!({ "jsonrpc": "2.0", "id": id, "method": methode, "params": params }))
        .unwrap_or_default();
    o.push(b'\n');
    o
}

/// La réponse à la demande `id` dans ce qui a été reçu : `None` tant qu'elle n'est pas arrivée en entier
pub fn reponse(recu: &[u8], id: u64) -> Option<Result<Value, String>> {
    for ligne in recu.split(|b| *b == b'\n') {
        let Ok(v) = serde_json::from_slice::<Value>(ligne) else { continue };
        if v["id"].as_u64() != Some(id) {
            continue;
        }
        if let Some(e) = v.get("error").filter(|e| !e.is_null()) {
            let m = e["message"].as_str().map(str::to_string).unwrap_or_else(|| e.to_string());
            return Some(Err(format!("Streamlabs refuse : {m}")));
        }
        return Some(Ok(v.get("result").cloned().unwrap_or(Value::Null)));
    }
    None
}

/// Une demande à l'API de Streamlabs par ce tube, et sa réponse
pub fn appeler(tube: &str, ressource: &str, methode: &str, args: &[Value]) -> Result<Value, String> {
    let id = NUMERO.fetch_add(1, Ordering::Relaxed);
    let message = demande(id, ressource, methode, args);
    let tube = tube.to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("streamlabs-api".into())
        .spawn(move || {
            let _ = tx.send(echanger(&tube, &message, id));
        })
        .map_err(|e| format!("fil de l'API : {e}"))?;
    rx.recv_timeout(DELAI).unwrap_or_else(|_| Err("Streamlabs ne répond pas".into()))
}

fn echanger(tube: &str, message: &[u8], id: u64) -> Result<Value, String> {
    // (toutes les instances du tube occupées, ERROR_PIPE_BUSY 231 : réessayé un instant)
    let mut f = None;
    for _ in 0..20 {
        match std::fs::OpenOptions::new().read(true).write(true).open(tube) {
            Ok(x) => {
                f = Some(x);
                break;
            }
            Err(e) if e.raw_os_error() == Some(231) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err("Streamlabs n'est pas ouvert".into()),
            Err(e) => return Err(format!("tube de Streamlabs : {e}")),
        }
    }
    let mut f = f.ok_or("tube de Streamlabs occupé")?;
    f.write_all(message).map_err(|e| format!("tube de Streamlabs : {e}"))?;
    let mut recu = Vec::new();
    let mut tampon = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut tampon).map_err(|e| format!("tube de Streamlabs : {e}"))?;
        if n == 0 {
            return Err("Streamlabs a fermé le tube sans répondre".into());
        }
        recu.extend_from_slice(&tampon[..n]);
        if let Some(r) = reponse(&recu, id) {
            return r;
        }
        if recu.len() > TAILLE_MAX {
            return Err("réponse de Streamlabs trop longue".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demande_et_reponse() {
        let d = demande(7, "SettingsService", "getSettingsFormData", &[json!("Output")]);
        assert_eq!(
            String::from_utf8(d).unwrap(),
            "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"getSettingsFormData\",\"params\":{\"resource\":\"SettingsService\",\"args\":[\"Output\"]}}\n"
        );
        // un événement avant la réponse, puis la réponse coupée en deux
        let mut recu =
            b"{\"jsonrpc\":\"2.0\",\"id\":null,\"result\":{\"_type\":\"EVENT\"}}\n{\"jsonrpc\":\"2.0\",\"id\":7,\"res"
                .to_vec();
        assert!(reponse(&recu, 7).is_none());
        recu.extend_from_slice(b"ult\":[1,2]}\n");
        assert_eq!(reponse(&recu, 7), Some(Ok(json!([1, 2]))));
        // une réponse sans résultat (une méthode qui ne rend rien)
        assert_eq!(reponse(b"{\"jsonrpc\":\"2.0\",\"id\":3}\n", 3), Some(Ok(Value::Null)));
        let e = reponse(
            b"{\"jsonrpc\":\"2.0\",\"id\":4,\"error\":{\"code\":-32600,\"message\":\"resource not found\"}}",
            4,
        );
        assert_eq!(e, Some(Err("Streamlabs refuse : resource not found".into())));
    }

    #[test]
    fn tube_absent() {
        let r = appeler(r"\\.\pipe\nevermind-essai-tube-absent", "StreamingService", "getModel", &[]);
        assert!(r.is_err());
    }
}
