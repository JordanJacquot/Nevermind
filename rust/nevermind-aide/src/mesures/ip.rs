//! Les adresses IP écrites comme .NET les écrit (`IPAddress.ToString`, qui passe par Windows pour l'IPv6 :
//! `WSAAddressToString`). Nevermind compare ces textes entre eux (serveur du jeu, adresses déjà vues) : ils doivent
//! être identiques à ceux de la version PowerShell, y compris pour les formes rares (IPv4 dans de l'IPv6, ISATAP).

/// Une adresse IPv4 (4 octets, dans l'ordre du réseau) : « 1.2.3.4 »
pub fn ipv4(o: &[u8]) -> String {
    format!("{}.{}.{}.{}", o[0], o[1], o[2], o[3])
}

/// Les huit mots de 16 bits d'une adresse IPv6
fn mots(o: &[u8; 16]) -> [u16; 8] {
    let mut m = [0u16; 8];
    for (i, x) in m.iter_mut().enumerate() {
        *x = u16::from_be_bytes([o[2 * i], o[2 * i + 1]]);
    }
    m
}

/// Des mots en hexadécimal séparés par « : », la plus longue suite d'au moins deux zéros (la première à égalité)
/// remplacée par « :: »
fn hexa(m: &[u16]) -> String {
    let (mut debut, mut long) = (usize::MAX, 0);
    let mut i = 0;
    while i < m.len() {
        if m[i] == 0 {
            let j = (i..m.len()).find(|&k| m[k] != 0).unwrap_or(m.len());
            if j - i > long {
                debut = i;
                long = j - i;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    if long < 2 {
        debut = usize::MAX;
    }
    let mut s = String::new();
    let mut i = 0;
    while i < m.len() {
        if i == debut {
            s.push_str("::");
            i += long;
            continue;
        }
        if !s.is_empty() && !s.ends_with(':') {
            s.push(':');
        }
        s.push_str(&format!("{:x}", m[i]));
        i += 1;
    }
    s
}

/// Une adresse IPv6 (16 octets, dans l'ordre du réseau), comme Windows l'écrit (`RtlIpv6AddressToString`)
pub fn ipv6(o: &[u8; 16]) -> String {
    let m = mots(o);
    let v4 = ipv4(&o[12..16]);
    // IPv4 dans de l'IPv6 (« ::ffff:1.2.3.4 », « ::ffff:0:1.2.3.4 », « ::1.2.3.4 ») : seulement si le premier octet de
    // l'IPv4 n'est pas nul (« ::1 » reste « ::1 »)
    if m[..4].iter().all(|&x| x == 0) && m[6] != 0 {
        match (m[4], m[5]) {
            (0, 0xffff) => return format!("::ffff:{v4}"),
            (0xffff, 0) => return format!("::ffff:0:{v4}"),
            (0, 0) => return format!("::{v4}"),
            _ => {}
        }
    }
    // ISATAP (« fe80::5efe:1.2.3.4 », « fe80::200:5efe:1.2.3.4 »)
    if m[4] & 0xfffd == 0 && m[5] == 0x5efe {
        let debut = hexa(&m[..6]);
        let sep = if debut.ends_with(':') { "" } else { ":" };
        return format!("{debut}{sep}{v4}");
    }
    hexa(&m)
}

/// Une adresse IPv6 est-elle une IPv4 « déguisée » (::ffff:a.b.c.d, `IsIPv4MappedToIPv6`) ?
pub fn ipv4_dans_ipv6(o: &[u8; 16]) -> bool {
    o[..10].iter().all(|&x| x == 0) && o[10] == 0xff && o[11] == 0xff
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    fn v6(t: &str) -> String {
        ipv6(&t.parse::<Ipv6Addr>().unwrap().octets())
    }

    #[test]
    fn comme_windows() {
        assert_eq!(v6("::"), "::");
        assert_eq!(v6("::1"), "::1");
        assert_eq!(v6("2a01:e0a:0:0:1:0:0:5"), "2a01:e0a::1:0:0:5");
        assert_eq!(v6("2001:0:9d38:6abd:0:0:0:1"), "2001:0:9d38:6abd::1");
        assert_eq!(v6("fe80::1c2b:3a4d:5e6f:7a8b"), "fe80::1c2b:3a4d:5e6f:7a8b");
        assert_eq!(v6("::ffff:52.1.2.3"), "::ffff:52.1.2.3");
        assert_eq!(v6("::ffff:0:52.1.2.3"), "::ffff:0:52.1.2.3");
        assert_eq!(v6("::52.1.2.3"), "::52.1.2.3");
        assert_eq!(v6("::0.0.1.2"), "::102");
        assert_eq!(v6("fe80::5efe:10.0.0.1"), "fe80::5efe:10.0.0.1");
        assert!(ipv4_dans_ipv6(&"::ffff:1.2.3.4".parse::<Ipv6Addr>().unwrap().octets()));
        assert!(!ipv4_dans_ipv6(&"::1.2.3.4".parse::<Ipv6Addr>().unwrap().octets()));
    }
}
