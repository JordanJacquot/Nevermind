//! Le réseau avec la bibliothèque standard : balayage par ping, ports TCP, SSDP et fiches UPnP, NetBIOS, titres des
//! pages web (http seulement), l'audit (netsh pour le Wi-Fi, registre pour ce PC), la liste des fabricants.

use super::lectures;
use crate::sonde::Avancement;
use crate::upnp::permise;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SANS_FENETRE: u32 = 0x0800_0000;

/// Fait `f` sur chaque élément, `n` fils à la fois
fn en_parallele<T: Send + 'static>(
    elements: Vec<String>,
    n: usize,
    f: impl Fn(&str) -> Option<T> + Send + Sync + 'static,
) -> Vec<(usize, T)> {
    let f = Arc::new(f);
    let elements = Arc::new(elements);
    let suivant = Arc::new(AtomicUsize::new(0));
    let sortie = Arc::new(Mutex::new(Vec::new()));
    let fils: Vec<_> = (0..n.max(1))
        .map(|_| {
            let (f, e, s, o) = (f.clone(), elements.clone(), suivant.clone(), sortie.clone());
            std::thread::spawn(move || {
                loop {
                    let i = s.fetch_add(1, Ordering::Relaxed);
                    let Some(x) = e.get(i) else { break };
                    if let Some(r) = f(x) {
                        o.lock().unwrap_or_else(|p| p.into_inner()).push((i, r));
                    }
                }
            })
        })
        .collect();
    for h in fils {
        let _ = h.join();
    }
    let mut r = std::mem::take(&mut *sortie.lock().unwrap_or_else(|p| p.into_inner()));
    r.sort_by_key(|(i, _)| *i);
    r
}

/// Le ping de toutes les adresses (PingSweep) : « ip|ms|ttl »
pub fn balayage(ips: &[String], delai: u32, av: &Avancement) -> Vec<String> {
    let total = ips.len().max(1);
    let fait = Arc::new(AtomicUsize::new(0));
    let trouves = Arc::new(AtomicUsize::new(0));
    let (f2, t2) = (fait.clone(), trouves.clone());
    let progression = Arc::new(Mutex::new(0.0f64));
    let p2 = progression.clone();
    let r = en_parallele(ips.to_vec(), 64, move |ip| {
        let r = lectures::ping(ip, delai);
        let d = f2.fetch_add(1, Ordering::Relaxed) + 1;
        *p2.lock().unwrap_or_else(|p| p.into_inner()) = 70.0 * d as f64 / total as f64;
        r.map(|(ms, ttl)| {
            t2.fetch_add(1, Ordering::Relaxed);
            format!("{ip}|{ms}|{ttl}")
        })
    });
    av.trouves.store(trouves.load(Ordering::Relaxed) as i64, Ordering::Relaxed);
    av.mettre(*progression.lock().unwrap_or_else(|p| p.into_inner()));
    r.into_iter().map(|(_, l)| l).collect()
}

/// Les noms des adresses (ResolveNames) : « ip|nom », au plus `delai` ms en tout
pub fn noms(ips: &[String], delai: u64) -> Vec<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    for ip in ips {
        let (tx, ip) = (tx.clone(), ip.clone());
        std::thread::spawn(move || {
            let n = lectures::nom(&ip).unwrap_or_default();
            let _ = tx.send(format!("{ip}|{n}"));
        });
    }
    drop(tx);
    let fin = Instant::now() + Duration::from_millis(delai);
    let mut r = Vec::new();
    while let Some(reste) = fin.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(reste) {
            Ok(l) => r.push(l),
            Err(_) => break,
        }
    }
    r
}

/// La demande directe d'adresse physique aux adresses qui n'ont pas répondu (ArpSweepAsync) : « ip|mac »
pub fn arp_direct(ips: &[String], n: usize) -> Vec<String> {
    en_parallele(ips.to_vec(), n, |ip| lectures::arp(ip).map(|m| format!("{ip}|{m}")))
        .into_iter()
        .map(|(_, l)| l)
        .collect()
}

/// Les ports TCP ouverts de plusieurs appareils (ScanHosts) : « ip|port »
pub fn ports_hotes(ips: &[String], ports: &[i64], delai: u32, av: &Avancement, de: f64, a: f64) -> Vec<String> {
    let cles: Vec<String> = ips.iter().flat_map(|ip| ports.iter().map(move |p| format!("{ip}|{p}"))).collect();
    let total = cles.len().max(1);
    let fait = Arc::new(AtomicUsize::new(0));
    let f2 = fait.clone();
    let r = en_parallele(cles, 128, move |c| {
        let (ip, p) = c.split_once('|')?;
        let adr: SocketAddr = format!("{ip}:{p}").parse().ok()?;
        let ok = TcpStream::connect_timeout(&adr, Duration::from_millis(u64::from(delai))).is_ok();
        f2.fetch_add(1, Ordering::Relaxed);
        ok.then(|| c.to_string())
    });
    if a > de {
        av.mettre(de + (a - de) * fait.load(Ordering::Relaxed) as f64 / total as f64);
    }
    r.into_iter().map(|(_, l)| l).collect()
}

/// Une demande SSDP (M-SEARCH) : « ip|st|adresse » des réponses
pub fn ssdp(ip_locale: &str, st: &str, duree: Duration) -> Vec<String> {
    let mut r = Vec::new();
    let Ok(s) = UdpSocket::bind(format!("{ip_locale}:0")) else { return r };
    let _ = s.set_read_timeout(Some(Duration::from_millis(300)));
    let m = format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: {st}\r\n\r\n"
    );
    if s.send_to(m.as_bytes(), "239.255.255.250:1900").is_err() {
        return r;
    }
    let fin = Instant::now() + duree;
    let mut b = [0u8; 4096];
    while Instant::now() < fin {
        let Ok((n, de)) = s.recv_from(&mut b) else { continue };
        let t = String::from_utf8_lossy(&b[..n]);
        let (mut st, mut loc) = (String::new(), String::new());
        for l in t.split("\r\n") {
            let bas = l.to_lowercase();
            if bas.starts_with("st:") {
                st = l[3..].trim().to_string();
            } else if bas.starts_with("location:") {
                loc = l[9..].trim().to_string();
            }
        }
        let x = format!("{}|{st}|{loc}", de.ip());
        if !r.contains(&x) {
            r.push(x);
        }
    }
    r
}

/// Une page en http simple (jamais https, ni redirection) : en-têtes et corps
fn http(
    url: &str,
    methode: &str,
    corps: &str,
    entetes: &[(&str, String)],
    delai: Duration,
) -> Option<(String, String)> {
    let reste = url.strip_prefix("http://")?;
    let (hote, chemin) = match reste.find('/') {
        Some(i) => (&reste[..i], &reste[i..]),
        None => (reste, "/"),
    };
    let adr = if hote.contains(':') { hote.to_string() } else { format!("{hote}:80") };
    let adr = adr.to_socket_addrs().ok()?.next()?;
    let mut s = TcpStream::connect_timeout(&adr, delai).ok()?;
    let _ = s.set_read_timeout(Some(delai));
    let mut req = format!(
        "{methode} {chemin} HTTP/1.0\r\nHost: {hote}\r\nUser-Agent: Mozilla/5.0 Nevermind\r\nConnection: close\r\n"
    );
    for (k, v) in entetes {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if !corps.is_empty() {
        req.push_str(&format!("Content-Length: {}\r\n", corps.len()));
    }
    req.push_str("\r\n");
    req.push_str(corps);
    s.write_all(req.as_bytes()).ok()?;
    let mut b = Vec::new();
    let _ = s.take(512 * 1024).read_to_end(&mut b);
    let t = String::from_utf8_lossy(&b).into_owned();
    let (e, c) = t.split_once("\r\n\r\n")?;
    if !e.starts_with("HTTP/") || !e.split_whitespace().nth(1).is_some_and(|c| c.starts_with('2')) {
        return None;
    }
    Some((e.to_string(), c.to_string()))
}

/// Le texte de la première balise `nom` (sans tenir compte d'un préfixe d'espace de noms)
fn balise(xml: &str, nom: &str) -> String {
    let re = crate::texte::re(&format!(r"<(?:\w+:)?{nom}(?:\s[^>]*)?>([\s\S]*?)</(?:\w+:)?{nom}>"));
    re.captures(xml).map(|c| c[1].trim().to_string()).unwrap_or_default()
}

/// Les fiches UPnP des appareils qui s'annoncent : « ip|nom|marque|modèle|numéro|type »
pub fn upnp(ip_locale: &str) -> Vec<String> {
    let mut lieux: Vec<(String, Vec<String>)> = Vec::new();
    for l in ssdp(ip_locale, "upnp:rootdevice", Duration::from_millis(2500)) {
        let p: Vec<&str> = l.splitn(3, '|').collect();
        let (ip, loc) = (p[0].to_string(), p.get(2).unwrap_or(&"").to_string());
        match lieux.iter_mut().find(|(i, _)| *i == ip) {
            Some(e) if !e.1.contains(&loc) => e.1.push(loc),
            Some(_) => {}
            None => lieux.push((ip, vec![loc])),
        }
    }
    let mut r = Vec::new();
    for (ip, locs) in lieux {
        for loc in locs.iter().take(3) {
            if !permise(loc, &ip) {
                continue;
            }
            let Some((_, x)) = http(loc, "GET", "", &[], Duration::from_secs(3)) else { continue };
            let dev = balise(&x, "device");
            if dev.is_empty() {
                continue;
            }
            let ty = balise(&dev, "deviceType");
            r.push(
                format!(
                    "{ip}|{}|{}|{}|{}|{ty}",
                    balise(&dev, "friendlyName"),
                    balise(&dev, "manufacturer"),
                    balise(&dev, "modelName"),
                    balise(&dev, "modelNumber")
                )
                .replace(['\r', '\n'], " "),
            );
            if !ty.contains("WFADevice") {
                break;
            }
        }
    }
    r
}

/// Le nom Windows de chaque adresse (NetBIOS, port 137) : « ip|nom|groupe »
pub fn netbios(ips: &[String], delai: u64) -> Vec<String> {
    en_parallele(ips.to_vec(), 32, move |ip| {
        let s = UdpSocket::bind("0.0.0.0:0").ok()?;
        let _ = s.set_read_timeout(Some(Duration::from_millis(delai)));
        let mut q = vec![0x4e, 0x42, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0x20];
        let mut nom = [0u8; 16];
        nom[0] = b'*';
        for c in nom {
            q.push(b'A' + (c >> 4));
            q.push(b'A' + (c & 0x0f));
        }
        q.extend_from_slice(&[0, 0, 0x21, 0, 1]);
        s.send_to(&q, format!("{ip}:137")).ok()?;
        let mut b = [0u8; 1024];
        let (n, _) = s.recv_from(&mut b).ok()?;
        let b = &b[..n];
        let mut i = 12;
        while i < b.len() && b[i] != 0 {
            i += b[i] as usize + 1;
        }
        i += 1 + 10;
        let nb = *b.get(i)? as usize;
        i += 1;
        let (mut machine, mut groupe) = (String::new(), String::new());
        for k in 0..nb {
            let e = b.get(i + k * 18..i + k * 18 + 18)?;
            let nm = String::from_utf8_lossy(&e[..15]).trim_end().to_string();
            let est_groupe = e[16] & 0x80 != 0;
            if e[15] == 0 {
                if est_groupe && groupe.is_empty() {
                    groupe = nm;
                } else if !est_groupe && machine.is_empty() {
                    machine = nm;
                }
            }
        }
        (!machine.is_empty()).then(|| format!("{ip}|{machine}|{groupe}"))
    })
    .into_iter()
    .map(|(_, l)| l)
    .collect()
}

/// Le titre et le serveur des pages web (http seulement) : « adresse|titre|serveur »
pub fn titres(urls: &[String], delai: u64) -> Vec<String> {
    en_parallele(urls.to_vec(), 16, move |u| {
        let (e, c) = http(u, "GET", "", &[], Duration::from_millis(delai))?;
        let titre = balise(&c, "title").replace(['\r', '\n', '|'], " ");
        let serveur = e
            .lines()
            .find(|l| l.to_lowercase().starts_with("server:"))
            .map(|l| l[7..].trim().to_string())
            .unwrap_or_default();
        Some(format!("{u}|{titre}|{serveur}"))
    })
    .into_iter()
    .map(|(_, l)| l)
    .collect()
}

/// Un programme de Windows, sans fenêtre : sa sortie
fn lancer(exe: &str, args: &[&str]) -> String {
    Command::new(exe)
        .args(args)
        .creation_flags(SANS_FENETRE)
        .output()
        .map(|o| lectures::texte_oem(&o.stdout))
        .unwrap_or_default()
}

fn champ(lignes: &[String], noms: &[&str]) -> String {
    for l in lignes {
        if let Some((k, v)) = l.split_once(':')
            && noms.iter().any(|n| k.trim().eq_ignore_ascii_case(n))
        {
            return v.trim().to_string();
        }
    }
    String::new()
}

/// L'audit (`$NetAuditWork`)
pub fn audit(ip: &str, passerelle: &str, if_index: i64, ips: &[String], ports: &[i64], av: &Avancement) -> Value {
    let ouvert = crate::texte::re("^(ouvrir|ouvert|open)");
    av.phase("wifi");
    av.mettre(2.0);
    let mut out = serde_json::Map::new();
    let li: Vec<String> = lancer("netsh", &["wlan", "show", "interfaces"]).lines().map(str::to_string).collect();
    let ssid = champ(&li, &["SSID"]);
    if !ssid.is_empty() {
        out.insert("Wifi".into(), json!({ "Ssid": ssid, "Auth": champ(&li, &["Authentication", "Authentification"]), "Cipher": champ(&li, &["Cipher", "Chiffrement"]) }));
    }
    let profils: Vec<String> = lancer("netsh", &["wlan", "show", "profiles"])
        .lines()
        .filter(|l| l.starts_with("  ") && !l.contains('<'))
        .filter_map(|l| l.split_once(": ").map(|(_, n)| n.trim().to_string()))
        .filter(|n| !n.is_empty())
        .take(40)
        .collect();
    let lignes: Vec<String> = profils
        .iter()
        .map(|n| {
            let pl: Vec<String> = lancer("netsh", &["wlan", "show", "profile", &format!("name={n}")])
                .lines()
                .map(str::to_string)
                .collect();
            let auth = champ(&pl, &["Authentication", "Authentification"]);
            let ciph = champ(&pl, &["Cipher", "Chiffrement"]);
            let mode = champ(&pl, &["Connection mode", "Mode de connexion"]);
            let est_ouvert = ouvert.is_match(&auth) || ciph.to_uppercase().contains("WEP");
            format!("{n}|{auth}|{ciph}|{}|{}", u8::from(mode.to_lowercase().contains("auto")), u8::from(est_ouvert))
        })
        .collect();
    out.insert("Profiles".into(), json!(lignes));
    av.phase("box");
    av.mettre(8.0);
    let ss = ssdp(ip, "ssdp:all", Duration::from_secs(3));
    av.mettre(18.0);
    let gw_loc = ss.iter().find_map(|l| {
        let p: Vec<&str> = l.splitn(3, '|').collect();
        let st = p.get(1).copied().unwrap_or("");
        let loc = p.get(2).copied().unwrap_or("");
        (p[0] == passerelle
            && crate::texte::re("InternetGatewayDevice|WAN(IP|PPP)Connection").is_match(st)
            && permise(loc, passerelle))
        .then(|| loc.to_string())
    });
    out.insert("Ssdp".into(), json!(ss));
    if let Some(loc) = gw_loc {
        out.insert("Igd".into(), igd(&loc, passerelle));
    }
    av.phase("dns");
    av.mettre(25.0);
    let serveurs =
        lectures::adaptateurs().into_iter().find(|a| i64::from(a.index) == if_index).map(|a| a.dns).unwrap_or_default();
    let mut detours = Vec::new();
    for (nom, a, b) in [("one.one.one.one", "1.1.1.1", "1.0.0.1"), ("dns.google", "8.8.8.8", "8.8.4.4")] {
        if let Ok(x) = format!("{nom}:0").to_socket_addrs() {
            let v4: Vec<String> = x.filter(|s| s.is_ipv4()).map(|s| s.ip().to_string()).collect();
            let faux: Vec<String> = v4.into_iter().filter(|i| i != a && i != b).collect();
            if !faux.is_empty() {
                detours.push(format!("{nom} répond {} au lieu de {a}", faux.join(", ")));
            }
        }
    }
    let n = Instant::now().elapsed().as_nanos() ^ u128::from(std::process::id());
    let faux = format!("nevermind-{:012x}.com:0", n & 0xffff_ffff_ffff);
    let nx = faux
        .to_socket_addrs()
        .map(|x| x.map(|s| s.ip().to_string()).collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    out.insert("Dns".into(), json!({ "Servers": serveurs, "Redirect": detours, "NxHijack": nx }));
    av.mettre(35.0);
    av.phase("devices");
    out.insert("Ports".into(), json!(ports_hotes(ips, ports, 800, av, 35.0, 85.0)));
    av.phase("pc");
    av.mettre(86.0);
    let dw = |cle: &str, nom: &str| lectures::registre_texte(cle, nom).and_then(|v| v.parse::<u32>().ok());
    let pare: Vec<&str> = [("DomainProfile", "Domain"), ("StandardProfile", "Private"), ("PublicProfile", "Public")]
        .iter()
        .filter(|(k, _)| {
            dw(
                &format!(r"SYSTEM\CurrentControlSet\Services\SharedAccess\Parameters\FirewallPolicy\{k}"),
                "EnableFirewall",
            ) == Some(0)
        })
        .map(|(_, n)| *n)
        .collect();
    let partages: Vec<String> = Vec::new();
    out.insert(
        "Pc".into(),
        json!({ "Smb1": dw(r"SYSTEM\CurrentControlSet\Services\LanmanServer\Parameters", "SMB1") == Some(1),
            "Smb1Client": lectures::registre_cle(r"SYSTEM\CurrentControlSet\Services\mrxsmb10"),
            "Rdp": dw(r"SYSTEM\CurrentControlSet\Control\Terminal Server", "fDenyTSConnections") == Some(0),
            "FirewallOff": pare, "Category": "", "Shares": partages }),
    );
    av.mettre(100.0);
    Value::Object(out)
}

/// La box (UPnP IGD) : son adresse Internet et ses règles d'ouverture de ports
fn igd(loc: &str, passerelle: &str) -> Value {
    let mut maps: Vec<String> = Vec::new();
    let mut externe = String::new();
    if let Some((_, x)) = http(loc, "GET", "", &[], Duration::from_secs(4)) {
        let re_svc = crate::texte::re(r"<(?:\w+:)?service>([\s\S]*?)</(?:\w+:)?service>");
        let svc = re_svc
            .captures_iter(&x)
            .map(|c| c[1].to_string())
            .find(|s| crate::texte::re("WAN(IP|PPP)Connection").is_match(&balise(s, "serviceType")));
        if let Some(s) = svc {
            let ctl = balise(&s, "controlURL");
            let ty = balise(&s, "serviceType");
            let autorite = loc.strip_prefix("http://").and_then(|r| r.split('/').next()).unwrap_or("");
            let url = if crate::texte::re("^[a-z][a-z0-9+.-]*:").is_match(&ctl)
                || ctl.starts_with("//")
                || ctl.starts_with("\\\\")
            {
                ctl
            } else {
                format!("http://{autorite}{}", if ctl.starts_with('/') { ctl } else { format!("/{ctl}") })
            };
            if permise(&url, passerelle) {
                let soap = |action: &str, inner: &str| {
                    let corps = format!(
                        "<?xml version=\"1.0\"?><s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} xmlns:u=\"{ty}\">{inner}</u:{action}></s:Body></s:Envelope>"
                    );
                    http(
                        &url,
                        "POST",
                        &corps,
                        &[
                            ("Content-Type", "text/xml; charset=\"utf-8\"".into()),
                            ("SOAPAction", format!("\"{ty}#{action}\"")),
                        ],
                        Duration::from_secs(3),
                    )
                    .map(|(_, c)| c)
                };
                if let Some(e) = soap("GetExternalIPAddress", "") {
                    externe = balise(&e, "NewExternalIPAddress");
                }
                for i in 0..64 {
                    let Some(r) =
                        soap("GetGenericPortMappingEntry", &format!("<NewPortMappingIndex>{i}</NewPortMappingIndex>"))
                    else {
                        break;
                    };
                    maps.push(format!(
                        "{}|{}|{}|{}|{}",
                        balise(&r, "NewExternalPort"),
                        balise(&r, "NewProtocol"),
                        balise(&r, "NewInternalPort"),
                        balise(&r, "NewInternalClient"),
                        balise(&r, "NewPortMappingDescription").replace('|', " ")
                    ));
                }
            }
        }
    }
    json!({ "Maps": maps, "External": externe })
}

/// fabricants.txt : la liste de l'IEEE, téléchargée la première fois (curl de Windows, sans fenêtre). Une réponse
/// d'erreur (403, 5xx, portail d'un Wi-Fi public) n'est jamais gardée (`--fail`, et la liste doit être complète) :
/// avant, elle donnait un fichier vide, et la liste n'était plus jamais téléchargée (petit de l'audit du 08/11).
pub fn telecharger_fabricants(chemin: &std::path::Path) {
    let tmp = chemin.with_extension("txt.csv");
    let ok = Command::new(nevermind_noyau::programme_windows("curl.exe"))
        .args(["-s", "--fail", "-L", "-m", "60", "-A", "Mozilla/5.0 Nevermind", "-o"])
        .arg(&tmp)
        .arg("https://standards-oui.ieee.org/oui/oui.csv")
        .creation_flags(SANS_FENETRE)
        .status()
        .is_ok_and(|s| s.success());
    let lu = if ok { std::fs::read_to_string(&tmp).ok() } else { None };
    let _ = std::fs::remove_file(&tmp);
    if let Some(sortie) = lu.as_deref().and_then(crate::appareil::fabricants_depuis_csv) {
        let _ = crate::fichiers::ecrire_texte(chemin, &sortie);
    }
}

/// Passer le réseau de cette carte en public (Set-NetConnectionProfile, comme la version PowerShell : sans droits
/// administrateur, Windows peut refuser)
pub fn passer_public(index: i64) -> Result<(), String> {
    let o = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("Set-NetConnectionProfile -InterfaceIndex {index} -NetworkCategory Public -ErrorAction Stop"),
        ])
        .creation_flags(SANS_FENETRE)
        .output()
        .map_err(|e| e.to_string())?;
    if o.status.success() { Ok(()) } else { Err(lectures::texte_oem(&o.stderr).trim().to_string()) }
}
