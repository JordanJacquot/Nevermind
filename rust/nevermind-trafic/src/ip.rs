//! Les adresses : réseau local ou Internet (`Test-PrivateIp`), et leur forme hexadécimale pour l'annuaire
//! (`ConvertTo-IpHex`, avec la lecture des adresses de .NET : « 10 » vaut 0.0.0.10).

use regex::Regex;
use std::sync::LazyLock;

static PRIVEE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(10\.|127\.|192\.168\.|169\.254\.|172\.(1[6-9]|2\d|3[01])\.|0\.|2(2[4-9]|[34]\d|5[0-5])\.|::1$|fe80:|f[cd][0-9a-f]{2}:|ff)")
        .expect("motif")
});

/// Adresse qui ne mène pas à Internet : réseau local, et annonces au réseau (224 à 255 : groupes et
/// 255.255.255.255, que FC 27 envoie sur le port 9999 et qui était pris pour le serveur du jeu)
pub fn privee(ip: &str) -> bool {
    PRIVEE.is_match(ip) || ip == "::"
}

/// Une partie d'une adresse IPv4 à la façon de inet_addr (décimal, 0x hexadécimal, 0 octal)
fn partie_v4(p: &str) -> Option<u64> {
    if p.is_empty() {
        return None;
    }
    let (base, chiffres) = if let Some(h) = p.strip_prefix("0x").or_else(|| p.strip_prefix("0X")) {
        (16, h)
    } else if p.len() > 1 && p.starts_with('0') {
        (8, &p[1..])
    } else {
        (10, p)
    };
    if chiffres.is_empty() {
        return if base == 16 { None } else { Some(0) };
    }
    u64::from_str_radix(chiffres, base).ok().filter(|&v| v <= u32::MAX as u64)
}

/// Les octets d'une adresse IPv4 (1 à 4 parties, la dernière remplit le reste), comme `IPAddress.Parse`
fn octets_v4(ip: &str) -> Option<[u8; 4]> {
    let parties: Vec<&str> = ip.split('.').collect();
    if parties.is_empty() || parties.len() > 4 {
        return None;
    }
    let mut v = Vec::new();
    for p in &parties {
        v.push(partie_v4(p)?);
    }
    let n = v.len();
    for x in &v[..n - 1] {
        if *x > 255 {
            return None;
        }
    }
    let reste = 4 - (n - 1);
    let dernier = v[n - 1];
    if reste < 4 && dernier >= 1u64 << (8 * reste) {
        return None;
    }
    let mut o = [0u8; 4];
    for (i, x) in v[..n - 1].iter().enumerate() {
        o[i] = *x as u8;
    }
    for k in 0..reste {
        o[3 - k] = ((dernier >> (8 * k)) & 0xFF) as u8;
    }
    Some(o)
}

/// Les octets d'une adresse (IPv4 ou IPv6), `None` si illisible
pub fn octets(ip: &str) -> Option<Vec<u8>> {
    let ip = ip.trim();
    if ip.is_empty() {
        return None;
    }
    if ip.contains(':') {
        let sans_zone = ip.split('%').next().unwrap_or(ip);
        let sans = sans_zone.trim_start_matches('[').trim_end_matches(']');
        return sans.parse::<std::net::Ipv6Addr>().ok().map(|a| a.octets().to_vec());
    }
    if !ip.chars().all(|c| c.is_ascii_hexdigit() || c == '.' || c == 'x' || c == 'X') {
        return None;
    }
    octets_v4(ip).map(|o| o.to_vec())
}

/// `ConvertTo-IpHex` : « 0a000001 », ou vide si l'adresse est illisible
pub fn hex(ip: &str) -> String {
    octets(ip).map(|o| o.iter().map(|b| format!("{b:02x}")).collect()).unwrap_or_default()
}

#[cfg(test)]
mod essais {
    use super::*;

    #[test]
    fn adresses() {
        assert_eq!(hex("10"), "0000000a");
        assert_eq!(hex("1.2.3.4.5"), "");
        assert_eq!(hex("256.1.1.1"), "");
        assert_eq!(hex("::1"), "00000000000000000000000000000001");
        assert!(privee("255.255.255.255"));
        assert!(!privee("223.5.5.5"));
    }
}
