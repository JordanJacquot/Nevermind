//! Une adresse annoncée par un appareil (UPnP) n'est ouverte que si c'est http:// vers cet appareil lui-même
//! (`UpnpAdresse.Permise` de natif.cs, audit du 25/10, constat 16) : jamais file://, un partage réseau, https, des
//! identifiants dans l'adresse, un nom au lieu d'une adresse, ni une autre machine.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Une adresse IPv4 écrite comme Windows l'accepte (`inet_aton` : 1 à 4 parties, en décimal, 0x hexadécimal ou
/// 0 octal ; la dernière partie remplit les octets qui restent)
fn ipv4_souple(s: &str) -> Option<Ipv4Addr> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let mut n: Vec<u64> = Vec::new();
    for p in &parts {
        if p.is_empty() {
            return None;
        }
        let v = if let Some(h) = p.strip_prefix("0x").or_else(|| p.strip_prefix("0X")) {
            if h.is_empty() { 0 } else { u64::from_str_radix(h, 16).ok()? }
        } else if p.len() > 1 && p.starts_with('0') {
            u64::from_str_radix(&p[1..], 8).ok()?
        } else {
            p.parse::<u64>().ok()?
        };
        n.push(v);
    }
    let dernier = *n.last()?;
    let avant = &n[..n.len() - 1];
    if avant.iter().any(|v| *v > 255) {
        return None;
    }
    let reste = 4 - avant.len();
    if dernier >= 1u64 << (8 * reste as u32) {
        return None;
    }
    let mut x: u64 = 0;
    for (i, v) in avant.iter().enumerate() {
        x |= v << (8 * (3 - i));
    }
    x |= dernier;
    Some(Ipv4Addr::from(x as u32))
}

/// `IPAddress.TryParse`
fn ip(s: &str) -> Option<IpAddr> {
    let s = s.trim();
    if s.contains(':') {
        return s.trim_matches(|c| c == '[' || c == ']').parse::<Ipv6Addr>().ok().map(IpAddr::V6);
    }
    ipv4_souple(s).map(IpAddr::V4)
}

fn normale(a: IpAddr) -> IpAddr {
    match a {
        IpAddr::V6(v) => v.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(v)),
        x => x,
    }
}

/// `UpnpAdresse.Permise(adresse, ip)`
pub fn permise(adresse: &str, ip_appareil: &str) -> bool {
    if adresse.is_empty() || ip_appareil.is_empty() {
        return false;
    }
    let a = adresse.trim();
    let Some(reste) = a.get(..7).filter(|d| d.eq_ignore_ascii_case("http://")).map(|_| &a[7..]) else {
        return false;
    };
    let fin = reste.find(['/', '?', '#']).unwrap_or(reste.len());
    let autorite = &reste[..fin];
    if autorite.contains('\\') {
        return false;
    }
    let hote_port = match autorite.rsplit_once('@') {
        Some((u, h)) => {
            if !u.is_empty() {
                return false;
            }
            h
        }
        None => autorite,
    };
    let (hote, port) = if let Some(r) = hote_port.strip_prefix('[') {
        let Some((h, apres)) = r.split_once(']') else { return false };
        (format!("[{h}]"), apres.strip_prefix(':').unwrap_or(if apres.is_empty() { "" } else { "x" }).to_string())
    } else {
        match hote_port.split_once(':') {
            Some((h, p)) => (h.to_string(), p.to_string()),
            None => (hote_port.to_string(), String::new()),
        }
    };
    if !port.is_empty()
        && (!port.bytes().all(|c| c.is_ascii_digit()) || port.parse::<u32>().map_or(true, |p| p > 65535))
    {
        return false;
    }
    let annonce = if hote.starts_with('[') {
        hote.trim_matches(|c| c == '[' || c == ']').parse::<Ipv6Addr>().ok().map(IpAddr::V6)
    } else {
        ipv4_souple(&hote).map(IpAddr::V4)
    };
    let (Some(x), Some(y)) = (annonce, ip(ip_appareil)) else { return false };
    normale(x) == normale(y)
}
