//! Les juges exportés de la version PowerShell (outils\exporter-juges-reseau.ps1) : chaque cas est rejoué en Rust et
//! doit donner exactement la même chose.

use nevermind_reseau::appareil::{Appareil, Fabricants, InterfaceLocale, Locaux, vendeur};
use nevermind_reseau::audit;
use nevermind_reseau::net::{Connexion, alerte_carte, bridage, capacite, format_vitesse, texte_bridage};
use nevermind_reseau::pages::{self, EtatReseau, ResultatPing};
use nevermind_reseau::scan::{self, Autour, BrutApprofondi, BrutScan, Connus};
use nevermind_reseau::texte::{chaine, liste, reel, vrai};
use nevermind_reseau::upnp;
use serde_json::Value;
use std::path::PathBuf;

fn juge(nom: &str) -> Vec<Value> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("juges").join(format!("{nom}.json"));
    let t = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    match serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).expect("juge lisible") {
        Value::Array(a) => a,
        x => vec![x],
    }
}

fn fabricants() -> Fabricants {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/juges/fabricants.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(p).expect("fabricants")).expect("json");
    v.as_object().expect("objet").iter().map(|(k, v)| (k.to_uppercase(), chaine(Some(v)))).collect()
}

/// Ce que reçoit le kit : « {} » de PowerShell 5.1 remis à null (comme Send-OrdreVue de vue.ps1)
fn kit(v: &Value) -> Value {
    match v {
        Value::Object(o) if o.is_empty() => Value::Null,
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), kit(x))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(kit).collect()),
        x => x.clone(),
    }
}

/// Égalité de deux JSON : les nombres par leur valeur, les objets sans tenir compte de l'ordre des clés. Rend le
/// chemin de la première différence.
fn diff(a: &Value, b: &Value, chemin: &str) -> Option<String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0));
            ((x - y).abs() > 1e-9).then(|| format!("{chemin} : {x} au lieu de {y}"))
        }
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                let (u, v) = (x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null));
                if x.contains_key(k) != y.contains_key(k) {
                    return Some(format!("{chemin}.{k} : présent d'un seul côté ({u} / {v})"));
                }
                if let Some(d) = diff(u, v, &format!("{chemin}.{k}")) {
                    return Some(d);
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!(
                    "{chemin} : {} éléments au lieu de {}\n  rust: {a}\n  ps:   {b}",
                    x.len(),
                    y.len()
                ));
            }
            x.iter().zip(y).enumerate().find_map(|(i, (u, v))| diff(u, v, &format!("{chemin}[{i}]")))
        }
        _ => (a != b).then(|| format!("{chemin} : {a} au lieu de {b}")),
    }
}

fn verifier(nom: &str, echecs: &mut Vec<String>, i: usize, rust: &Value, ps: &Value) {
    if let Some(d) = diff(rust, ps, "") {
        echecs.push(format!("{nom} cas {i} : {d}"));
    }
}

fn fin(nom: &str, n: usize, echecs: Vec<String>) {
    if !echecs.is_empty() {
        for e in echecs.iter().take(8) {
            eprintln!("{e}");
        }
        panic!("{nom} : {} cas sur {n} différents", echecs.len());
    }
    eprintln!("{nom} : {n} cas, tous identiques");
}

#[test]
fn vendeurs() {
    let fab = fabricants();
    let cas = juge("vendeur");
    for c in &cas {
        let sans = vrai(c.get("sansFabricants"));
        let v = vendeur(&chaine(c.get("mac")), if sans { None } else { Some(&fab) });
        assert_eq!(v, chaine(c.get("vendeur")), "mac {}", chaine(c.get("mac")));
    }
}

#[test]
fn sous_reseaux() {
    for c in juge("sous-reseau") {
        let ips = scan::sous_reseau(&chaine(c.get("ip")), reel(c.get("prefixe")) as i64);
        assert_eq!(ips, liste(c.get("ips")), "{} /{}", chaine(c.get("ip")), chaine(c.get("prefixe")));
    }
}

#[test]
fn upnp_adresses() {
    for c in juge("upnp") {
        let (a, ip) = (chaine(c.get("adresse")), chaine(c.get("ip")));
        assert_eq!(upnp::permise(&a, &ip), vrai(c.get("permise")), "{a} pour {ip}");
    }
}

fn locaux(v: Option<&Value>) -> Locaux {
    v.and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .map(|(ip, x)| {
                    (
                        ip.clone(),
                        InterfaceLocale {
                            mac: chaine(x.get("Mac")),
                            nom: chaine(x.get("Name")),
                            desc: chaine(x.get("Desc")),
                            wifi: vrai(x.get("Wifi")),
                            active: vrai(x.get("Up")),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn brut_approfondi(v: &Value) -> BrutApprofondi {
    BrutApprofondi {
        arp: liste(v.get("Arp")),
        mdns: liste(v.get("Mdns")),
        wsd: liste(v.get("Wsd")),
        upnp: liste(v.get("Upnp")),
        v6: liste(v.get("V6")),
        netbios: liste(v.get("NetBios")),
        ports: liste(v.get("Ports")),
        titres: liste(v.get("Titles")),
    }
}

/// Rejoue Invoke-NetworkScan puis Invoke-NetDeepScan
fn rejouer_scan(e: &Value, fab: &Fabricants) -> Value {
    let net = Connexion::depuis_json(&e["net"]);
    let loc = locaux(e.get("locaux"));
    let f = vrai(e.get("fabricants")).then_some(fab);
    let a = Autour { poste: "PC-ESSAI", aujourdhui: "2026-11-06", fab: f, locaux: &loc };
    let mut messages: Vec<String> = Vec::new();
    let mut statut = String::new();
    let mut sortie = serde_json::json!({ "erreur": "", "appareils": [], "connus": null, "resume": "",
        "conseil": "Clique sur « Scanner le réseau » pour voir les appareils connectés.",
        "premier": false, "statut": "", "messages": [], "ipsScan": 0, "manquants": 0, "trouves": [] });
    let fini = |mut s: Value, messages: Vec<String>, statut: String| {
        s["messages"] = serde_json::json!(messages);
        s["statut"] = serde_json::json!(statut);
        s
    };
    let Some(net) = net else {
        messages.push("Aucune connexion réseau détectée.".into());
        return fini(sortie, messages, statut);
    };
    let Some(ip) = net.ip.clone().filter(|x| !x.is_empty()) else {
        messages.push("Impossible de lire l'adresse de ce PC.".into());
        return fini(sortie, messages, statut);
    };
    let ips = scan::sous_reseau(&ip, net.prefix);
    statut = "Scan du réseau...".into();
    sortie["ipsScan"] = serde_json::json!(ips.len());
    let s = &e["scan"];
    if s.get("Error").is_some() {
        messages.push(format!("Le scan n'a pas pu se faire : {}", chaine(s.get("Error"))));
        return fini(sortie, messages, statut);
    }
    let brut = BrutScan { vivants: liste(s.get("Alive")), arp: liste(s.get("Arp")), noms: liste(s.get("Names")) };
    let mut connus = match e.get("connus") {
        Some(Value::String(t)) => Connus::depuis(serde_json::from_str::<Value>(t).ok().as_ref()),
        _ => Connus::default(),
    };
    let (mut liste_app, premier) = scan::assembler(&net, &ips, &brut, &mut connus, &a);
    sortie["premier"] = serde_json::json!(premier);
    sortie["connus"] = connus.json();
    let (r, c) = scan::textes_liste(&liste_app, premier, "");
    statut = format!("Scan terminé : {} appareils trouvés.", liste_app.len());
    let d = &e["deep"];
    let man = scan::manquants(&ips, &liste_app);
    sortie["manquants"] = serde_json::json!(man.len());
    sortie["trouves"] = serde_json::json!(liste_app.iter().map(|x| x.ip.clone()).collect::<Vec<_>>());
    if d.get("Error").is_none() {
        let ajoutes = scan::fusionner(&mut liste_app, &brut_approfondi(d), Some(&net), &ips, &a);
        statut = scan::apres_approfondi(&mut liste_app, &ajoutes, &mut connus, "2026-11-06");
        sortie["connus"] = connus.json();
        let (r, c) = scan::textes_liste(&liste_app, premier, "");
        sortie["resume"] = serde_json::json!(r);
        sortie["conseil"] = serde_json::json!(c);
    } else {
        sortie["resume"] = serde_json::json!(r);
        sortie["conseil"] = serde_json::json!(c);
    }
    sortie["appareils"] = serde_json::to_value(&liste_app).expect("appareils");
    fini(sortie, messages, statut)
}

#[test]
fn scans() {
    let fab = fabricants();
    let cas = juge("scan");
    let mut echecs = Vec::new();
    // (les appareils vus seulement en IPv6 n'ont pas d'adresse IPv4 : PowerShell les trie dans un ordre qui n'est pas
    // fixé (Sort-Object, à égalité) ; ils sont rangés par adresse des deux côtés, voir DIVERGENCES.md)
    let ranger = |v: &Value| {
        let mut v = v.clone();
        if let Some(a) = v.get_mut("appareils").and_then(Value::as_array_mut) {
            a.sort_by_key(|d| {
                let ip = chaine(d.get("Ip"));
                let groupe = if vrai(d.get("Self")) {
                    0
                } else if vrai(d.get("Gateway")) {
                    1
                } else {
                    2
                };
                (groupe, nevermind_reseau::texte::cle_version(&ip), ip)
            });
        }
        v
    };
    for (i, c) in cas.iter().enumerate() {
        let r = rejouer_scan(&c["entree"], &fab);
        verifier("scan", &mut echecs, i, &ranger(&r), &ranger(&c["sortie"]));
    }
    fin("scan", cas.len(), echecs);
}

fn appareils(v: Option<&Value>) -> Vec<Appareil> {
    serde_json::from_value(v.cloned().unwrap_or(Value::Array(vec![]))).expect("appareils lisibles")
}

fn audit_sauve(v: Option<&Value>) -> Option<Value> {
    match v {
        Some(Value::String(t)) => serde_json::from_str::<Value>(t).ok(),
        _ => None,
    }
}

#[test]
fn pages_reseau() {
    let cas = juge("pages-reseau");
    let mut echecs = Vec::new();
    for (i, c) in cas.iter().enumerate() {
        let e = &c["entree"];
        let l = appareils(e.get("appareils"));
        let (resume, conseil) = if l.is_empty() {
            (String::new(), "Clique sur « Scanner le réseau » pour voir les appareils connectés.".to_string())
        } else {
            scan::textes_liste(&l, vrai(e.get("premier")), &chaine(e.get("filtreListe")))
        };
        let a = audit_sauve(e.get("audit"));
        let etat = EtatReseau {
            liste: &l,
            scan: vrai(e.get("scan")),
            scan_texte: &chaine(e.get("scanTexte")),
            scan_trouves: reel(e.get("scanTrouves")) as i64,
            progression: reel(e.get("progression")),
            filtre: &chaine(e.get("filtre")),
            audit: a.as_ref(),
            audit_complet: vrai(e.get("auditComplet")),
            surveillance: vrai(e.get("surveillance")),
            resume: &resume,
            conseil: &conseil,
        };
        let p = Value::Object(pages::page_reseau(&etat));
        let r = serde_json::json!({ "pages": p, "resume": resume, "conseil": conseil });
        verifier("pages-reseau", &mut echecs, i, &kit(&r), &kit(&c["sortie"]));
    }
    fin("pages-reseau", cas.len(), echecs);
}

/// Les éléments d'un point dont l'ordre vient d'une table PowerShell (voir DIVERGENCES.md) : triés des deux côtés
fn trier_elements(v: &mut Value) {
    if let Some(p) = v.get_mut("points").and_then(Value::as_array_mut) {
        for c in p {
            if c["Cat"] == "devices"
                && let Some(t) = c.get("Title").and_then(Value::as_str)
                && (t.starts_with("Telnet")
                    || t.starts_with("Transfert")
                    || t.starts_with("Prise")
                    || t.starts_with("Flux"))
                && let Some(el) = c.get_mut("Items").and_then(Value::as_array_mut)
            {
                el.sort_by_key(|x| x.to_string());
            }
        }
    }
}

#[test]
fn audits() {
    let cas = juge("audit");
    let mut echecs = Vec::new();
    for (i, c) in cas.iter().enumerate() {
        let e = &c["entree"];
        let l = appareils(e.get("appareils"));
        let net = &e["net"];
        let mut s = serde_json::json!({ "erreur": "", "messages": [], "statut": "", "ips": [], "ports": [], "points": [],
            "note": null, "nombre": null, "enregistre": null,
            "carte": { "texte": "", "bouton": "Lancer l'audit", "voir": false } });
        if vrai(e.get("scanEnCours")) {
            s["messages"] = serde_json::json!(["Attends la fin du scan du réseau, puis relance l'audit."]);
        } else if !vrai(net.get("Ip")) {
            s["messages"] = serde_json::json!(["Aucune connexion réseau détectée."]);
        } else {
            let ips: Vec<String> = l.iter().filter(|d| !d.soi).map(|d| d.ip.clone()).collect();
            s["ips"] = serde_json::json!(ips);
            s["ports"] = serde_json::json!(audit::PORTS);
            let r = &e["resultat"];
            if r.get("Error").is_none() {
                let pts = audit::points(r, &l, &chaine(net.get("Gateway")), net.get("IfIndex").unwrap_or(&Value::Null));
                let (score, bad, warn) = audit::note(&pts);
                s["points"] = Value::Array(pts.iter().map(audit::Point::json).collect());
                s["note"] = serde_json::json!(score);
                s["nombre"] = serde_json::json!(ips.len());
                let sauve = audit::sauvegarde(score, "06/11/2026 à 21:30", bad, warn);
                let (t, b, _) = pages::textes_audit(Some(&sauve));
                s["carte"] = serde_json::json!({ "texte": t, "bouton": b, "voir": true });
                s["enregistre"] = sauve;
                s["statut"] = serde_json::json!(format!("Audit du réseau : {score} sur 100."));
            }
        }
        let mut attendu = c["sortie"].clone();
        trier_elements(&mut s);
        trier_elements(&mut attendu);
        verifier("audit", &mut echecs, i, &s, &attendu);
    }
    fin("audit", cas.len(), echecs);
}

#[test]
fn cartes_bridees() {
    let cas = juge("carte-bridee");
    let mut echecs = Vec::new();
    for (i, c) in cas.iter().enumerate() {
        let r = if let Some(v) = c.get("format") {
            serde_json::json!({ "format": v, "vitesse": format_vitesse(reel(Some(v))) })
        } else {
            let n = Connexion::depuis_json(&c["net"]);
            let b = bridage(n.as_ref());
            serde_json::json!({ "net": c["net"], "capacite": n.as_ref().map(capacite),
                "bridage": b.as_ref().map(|b| serde_json::json!({ "Bps": b.bps, "Capacite": b.capacite, "Fois": b.fois })),
                "texte": b.as_ref().map(texte_bridage), "vitesse": n.as_ref().map(|n| format_vitesse(n.bps)) })
        };
        verifier("carte-bridee", &mut echecs, i, &r, c);
    }
    fin("carte-bridee", cas.len(), echecs);
}

#[test]
fn bulles_carte() {
    let cas = juge("carte-bulle");
    let mut echecs = Vec::new();
    for (i, c) in cas.iter().enumerate() {
        let mut alertee = None;
        let mut journal = Vec::new();
        let etapes: Vec<Value> = c["etapes"]
            .as_array()
            .expect("étapes")
            .iter()
            .map(|e| {
                let n = Connexion::depuis_json(&e["net"]);
                let r = alerte_carte(n.as_ref(), &mut alertee, vrai(e.get("partie")));
                if let Some((l, _)) = &r {
                    journal.push(l.clone());
                }
                serde_json::json!({ "net": e["net"], "partie": e["partie"], "resultat": r.is_some(),
                    "bulle": r.map(|(_, b)| serde_json::json!({ "Titre": b.titre, "Texte": b.texte })), "alertee": alertee })
            })
            .collect();
        verifier("carte-bulle", &mut echecs, i, &serde_json::json!({ "etapes": etapes, "journal": journal }), c);
    }
    fin("carte-bulle", cas.len(), echecs);
}

#[test]
fn connexions() {
    let cas = juge("connexion");
    let mut echecs = Vec::new();
    for (i, c) in cas.iter().enumerate() {
        let e = &c["entree"];
        let net = Connexion::depuis_json(&e["net"]);
        let mut res: Vec<ResultatPing> = Vec::new();
        if vrai(e.get("teste"))
            && let Some(n) = &net
        {
            for (lbl, hote, local) in pages::cibles(n) {
                let t: Vec<Option<f64>> = (0..20)
                    .map(|k| e["pings"].get(&hote).and_then(|a| a.get(k)).map(|x| reel(Some(x))).filter(|x| *x >= 0.0))
                    .collect();
                res.push(pages::verdict(&lbl, &hote, local, pages::mesure(&t)));
            }
        }
        let debit: Option<Vec<f64>> =
            e.get("debit").and_then(Value::as_array).map(|a| a.iter().map(|x| reel(Some(x))).collect());
        let p = pages::page_connexion(net.as_ref(), &res, debit.as_deref());
        let r =
            serde_json::json!({ "connexion": p, "resultats": res.iter().map(ResultatPing::json).collect::<Vec<_>>() });
        verifier("connexion", &mut echecs, i, &kit(&r), &kit(&c["sortie"]));
    }
    fin("connexion", cas.len(), echecs);
}

#[test]
fn surveillances() {
    let fab = fabricants();
    let cas = juge("surveillance");
    let mut echecs = Vec::new();
    for (i, c) in cas.iter().enumerate() {
        let net = Connexion::depuis_json(&c["net"]).expect("net");
        let ips = scan::sous_reseau(net.ip.as_deref().unwrap_or(""), net.prefix);
        let connus = match c.get("connus") {
            Some(Value::String(t)) => Connus::depuis(serde_json::from_str::<Value>(t).ok().as_ref()),
            _ => Connus::default(),
        };
        let mut vus = Connus::default();
        let etapes: Vec<Value> = c["etapes"]
            .as_array()
            .expect("étapes")
            .iter()
            .map(|e| {
                let mut bulles = Vec::new();
                let mut journal = Vec::new();
                if !vrai(e.get("erreur"))
                    && let Some((l, b)) =
                        scan::surveiller(&net, &ips, &liste(e.get("arp")), &connus, &mut vus, "2026-11-06", Some(&fab))
                {
                    journal.push(l);
                    bulles.push(serde_json::json!({ "Titre": b.titre, "Texte": b.texte }));
                }
                serde_json::json!({ "arp": e["arp"], "erreur": e["erreur"], "bulles": bulles, "journal": journal })
            })
            .collect();
        verifier(
            "surveillance",
            &mut echecs,
            i,
            &serde_json::json!({ "net": c["net"], "connus": c["connus"], "etapes": etapes }),
            c,
        );
    }
    fin("surveillance", cas.len(), echecs);
}
