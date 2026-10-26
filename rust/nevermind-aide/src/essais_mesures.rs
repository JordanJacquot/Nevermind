//! Rejeu des enregistrements des mesures continues (cas\mesures\*.json, `cargo test` seulement) : les mêmes fichiers
//! que outils\tester-mesures.ps1 passe à la version PowerShell. Chaque demande passe par le canal (serveur::repondre),
//! sur le faux PC : PresentMon, la trace réseau et les tables TCP de Windows sont remplacés par l'enregistrement.
//! Les deux versions doivent donner exactement les réponses attendues.

use crate::actions::EtatAide;
use crate::essais::{ELODIE, FauxSysteme, etat_aide, p};
use crate::mesures::fps::Decoupeur;
use crate::serveur::{self, Journal};
use crate::systeme::{EvenementReseau, LigneTcp, RefusTrace, SortiePm};
use serde_json::{Value, json};
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// Un fichier du corpus cas\mesures
fn corpus(nom: &str, minimum: usize) -> Vec<Value> {
    let dossier =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("cas").join("mesures");
    let texte = std::fs::read_to_string(dossier.join(nom)).unwrap();
    let f: Value = serde_json::from_str(texte.trim_start_matches('\u{feff}')).unwrap();
    assert_eq!(f["format"], 1, "{nom} : format");
    let cas = f["cas"].as_array().unwrap().clone();
    // (un fichier vidé ou coupé ne passe pas)
    assert!(cas.len() >= minimum, "{nom} : {} cas, au moins {minimum} attendus", cas.len());
    cas
}

/// Une demande, comme sur le canal ; la réponse en JSON
fn demander(s: &FauxSysteme, etat: &EtatAide, action: &str, p: &Value) -> Value {
    let ligne = json!({ "a": action, "p": p }).to_string();
    let (r, _) = serveur::repondre(&ligne, s, 1, etat, &Journal::default());
    serde_json::from_str(&r).unwrap()
}

/// Compare les réponses obtenues à celles attendues ; rend les différences
fn comparer(nom: &str, cas: &Value, reponses: &[Value], autres: &[(&str, Value)]) -> Vec<String> {
    let mut e = Vec::new();
    let attendues = cas["attendu"]["reponses"].as_array().unwrap();
    if attendues.len() != reponses.len() {
        e.push(format!("{nom} « {} » : {} réponses, {} attendues", cas["nom"], reponses.len(), attendues.len()));
    }
    for (k, (a, o)) in attendues.iter().zip(reponses).enumerate() {
        if a != o {
            e.push(format!("{nom} « {} », réponse {} :\n  attendu : {a}\n  obtenu  : {o}", cas["nom"], k + 1));
        }
    }
    for (n, o) in autres {
        if &cas["attendu"][n] != o {
            e.push(format!("{nom} « {} », {n} :\n  attendu : {}\n  obtenu  : {o}", cas["nom"], cas["attendu"][n]));
        }
    }
    e
}

#[test]
fn corpus_presentmon() {
    let mut echecs = Vec::new();
    for c in corpus("presentmon.json", 9) {
        let mut s = FauxSysteme::new();
        for (pid, nom) in c["entree"]["jeux"].as_object().unwrap() {
            s.processus.push(p(pid.parse().unwrap(), &format!("C:\\Jeux\\{}", nom.as_str().unwrap()), ELODIE));
        }
        let etat = etat_aide();
        let mut reponses = Vec::new();
        // (les sorties du PresentMon en cours, coupées en lignes comme le fait l'exe)
        let (mut sortie, mut erreurs) = (Decoupeur::default(), Decoupeur::default());
        let mut lances = 0;
        for e in c["entree"]["etapes"].as_array().unwrap() {
            if let Some(r) = e.get("presentmon_refus") {
                s.mesures.lock().unwrap().presentmon_refus = r.as_str().map(String::from);
                continue;
            }
            if let Some(a) = e["demande"].as_str() {
                reponses.push(demander(&s, &etat, a, &e["p"]));
                let n = s.mesures.lock().unwrap().lancements.iter().filter(|x| !x.contains("terminate")).count();
                if n != lances {
                    lances = n;
                    (sortie, erreurs) = (Decoupeur::default(), Decoupeur::default());
                }
                continue;
            }
            let rappel = s.mesures.lock().unwrap().rappel_pm.clone().expect("une sortie sans PresentMon lancé");
            if let Some(t) = e["sortie"].as_str() {
                sortie.ajouter(t.as_bytes(), |l| rappel(SortiePm::Ligne(l)));
            } else if let Some(t) = e["erreurs"].as_str() {
                erreurs.ajouter(t.as_bytes(), |l| rappel(SortiePm::Erreur(l)));
            } else {
                sortie.finir(|l| rappel(SortiePm::Ligne(l)));
                erreurs.finir(|l| rappel(SortiePm::Erreur(l)));
                rappel(SortiePm::Fin);
            }
        }
        let lancements = json!(s.mesures.lock().unwrap().lancements.clone());
        echecs.extend(comparer("presentmon.json", &c, &reponses, &[("lancements", lancements)]));
    }
    assert!(echecs.is_empty(), "\n{}", echecs.join("\n"));
}

/// Les données d'un événement de la trace, dans l'ordre de Windows : pid, taille, adresses de destination et source,
/// ports de destination et source (octets du réseau), puis la suite (non lue)
fn donnees(v: &Value) -> Option<Vec<u8>> {
    if v["vide"] == true {
        return None;
    }
    if let Some(h) = v["octets"].as_str() {
        return Some((0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap()).collect());
    }
    let v6 = [26, 27, 58, 59].contains(&v["id"].as_i64().unwrap());
    let mut b = Vec::new();
    b.extend((v["pid"].as_i64().unwrap() as i32).to_le_bytes());
    b.extend((v["taille"].as_i64().unwrap() as i32).to_le_bytes());
    for a in [&v["dst"], &v["src"]] {
        match a.as_str().unwrap().parse::<IpAddr>().unwrap() {
            IpAddr::V4(x) if v6 => b.extend(x.to_ipv6_mapped().octets()),
            IpAddr::V4(x) => b.extend(x.octets()),
            IpAddr::V6(x) => b.extend(x.octets()),
        }
    }
    for pt in [&v["dport"], &v["sport"]] {
        b.extend((pt.as_u64().unwrap() as u16).to_be_bytes());
    }
    b.extend([0u8; 8]);
    Some(b)
}

#[test]
fn corpus_trace_reseau() {
    let mut echecs = Vec::new();
    for c in corpus("trace-reseau.json", 4) {
        let s = FauxSysteme::new();
        let base = c["entree"]["base"].as_i64().unwrap();
        s.mesures.lock().unwrap().locales =
            c["entree"]["locales"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().parse().unwrap()).collect();
        let etat = etat_aide();
        let mut reponses = Vec::new();
        for e in c["entree"]["etapes"].as_array().unwrap() {
            if let Some(a) = e["demande"].as_str() {
                {
                    let mut m = s.mesures.lock().unwrap();
                    m.filetime = base + 10 * e["t"].as_i64().unwrap();
                    // Ce que Windows répond (codes de StartTrace dans l'ordre : 183, une session restée ouverte, est
                    // fermée par l'exe et la session redemandée ; puis EnableTraceEx2)
                    let w = &e["windows"];
                    let codes: Vec<u32> = w["demarrage"]
                        .as_array()
                        .map(|l| l.iter().map(|x| x.as_u64().unwrap() as u32).collect())
                        .unwrap_or_default();
                    let code = match codes.as_slice() {
                        [183, suite, ..] => *suite,
                        [c, ..] => *c,
                        [] => 0,
                    };
                    let activation = w["activation"].as_u64().unwrap_or(0) as u32;
                    m.trace_refus = if code != 0 {
                        Some(RefusTrace::Session(code))
                    } else if activation != 0 {
                        Some(RefusTrace::Fournisseur(activation))
                    } else {
                        None
                    };
                }
                reponses.push(demander(&s, &etat, a, &e["p"]));
            } else if e["fin"] == true {
                s.mesures.lock().unwrap().trace_tourne.store(false, Ordering::SeqCst);
            } else {
                for v in e["evenements"].as_array().unwrap() {
                    let d = donnees(v);
                    etat.mesures.flux.evenement(&EvenementReseau {
                        id: v["id"].as_u64().unwrap() as u16,
                        date: base + 10 * v["t"].as_i64().unwrap(),
                        donnees: d.as_deref(),
                    });
                }
            }
        }
        echecs.extend(comparer("trace-reseau.json", &c, &reponses, &[]));
    }
    assert!(echecs.is_empty(), "\n{}", echecs.join("\n"));
}

/// Un port en nombre, rangé comme Windows le range (octets du réseau dans un entier)
fn port_brut(p: &Value) -> u32 {
    let p = p.as_u64().unwrap() as u32;
    ((p & 0xFF) << 8) | ((p >> 8) & 0xFF)
}

fn adresse(t: &Value) -> [u8; 16] {
    let mut o = [0u8; 16];
    match t.as_str().unwrap().parse::<IpAddr>().unwrap() {
        IpAddr::V4(x) => o[..4].copy_from_slice(&x.octets()),
        IpAddr::V6(x) => o = x.octets(),
    }
    o
}

#[test]
fn corpus_tcp() {
    let mut echecs = Vec::new();
    for c in corpus("tcp.json", 3) {
        let s = FauxSysteme::new();
        let etat = etat_aide();
        let mut reponses = Vec::new();
        for passage in c["entree"]["passages"].as_array().unwrap() {
            {
                let mut m = s.mesures.lock().unwrap();
                for (i, cle) in ["v4", "v6"].into_iter().enumerate() {
                    m.tables[i] = passage[cle].as_array().map(|t| {
                        t.iter()
                            .map(|x| {
                                let l = LigneTcp {
                                    v6: i == 1,
                                    etat: x["etat"].as_u64().unwrap() as u32,
                                    adresse_locale: adresse(&x["local"]),
                                    adresse_distante: adresse(&x["distante"]),
                                    port_local: port_brut(&x["port_local"]),
                                    port_distant: port_brut(&x["port_distant"]),
                                    zone_locale: x["zone_locale"].as_u64().unwrap_or(0) as u32,
                                    zone_distante: x["zone_distante"].as_u64().unwrap_or(0) as u32,
                                    pid: x["pid"].as_u64().unwrap() as u32,
                                };
                                let octets =
                                    x["octets"].as_array().map(|o| (o[0].as_u64().unwrap(), o[1].as_u64().unwrap()));
                                (l, x["compter"] == true, octets)
                            })
                            .collect()
                    });
                }
            }
            reponses.push(demander(&s, &etat, "trafic.tcp", &Value::Null));
        }
        echecs.extend(comparer("tcp.json", &c, &reponses, &[]));
    }
    assert!(echecs.is_empty(), "\n{}", echecs.join("\n"));
}

/// Contre-épreuve : une réponse fausse glissée dans un cas fait échouer la comparaison
#[test]
fn contre_epreuve() {
    let mut c = corpus("tcp.json", 3).remove(0);
    let bonnes: Vec<Value> = c["attendu"]["reponses"].as_array().unwrap().clone();
    assert!(comparer("tcp.json", &c, &bonnes, &[]).is_empty());
    c["attendu"]["reponses"][0]["r"]["CompteursOk"] = json!(false);
    assert_eq!(comparer("tcp.json", &c, &bonnes, &[]).len(), 1);
}

/// Les âges se comptent dans l'horloge des événements (heure système) : un événement d'il y a 2,5 s a 2 500 ms, même
/// si l'heure système est très loin de zéro (avec un compteur QPC, l'âge vaudrait des milliards de ms : bug du 15/10)
#[test]
fn ages_en_heure_systeme() {
    let s = FauxSysteme::new();
    let etat = etat_aide();
    let maintenant = 134_064_000_000_000_000;
    s.mesures.lock().unwrap().filetime = maintenant;
    assert_eq!(demander(&s, &etat, "flux.demarrer", &Value::Null)["r"], "ok");
    let mut d = vec![0u8; 28];
    d[0] = 7;
    d[8..12].copy_from_slice(&[52, 1, 2, 3]);
    etat.mesures.flux.evenement(&EvenementReseau { id: 42, date: maintenant - 25_000_000, donnees: Some(&d) });
    let r = demander(&s, &etat, "flux.lire", &Value::Null);
    assert_eq!(r["r"][0], "7|udp|52.1.2.3|0|1|0|0|0|0|0|0|2500");
}

/// Au delà de 20 000 échanges, plus aucun n'est ajouté (20 001 gardés, comme NetFlow : « Count > 20000 »)
#[test]
fn echanges_bornes() {
    let s = FauxSysteme::new();
    let etat = etat_aide();
    assert_eq!(demander(&s, &etat, "flux.demarrer", &Value::Null)["r"], "ok");
    for i in 0..20_100u32 {
        let mut d = vec![0u8; 28];
        d[..4].copy_from_slice(&i.to_le_bytes());
        d[8..12].copy_from_slice(&[52, 1, 2, 3]);
        etat.mesures.flux.evenement(&EvenementReseau { id: 42, date: 1, donnees: Some(&d) });
    }
    assert_eq!(demander(&s, &etat, "flux.lire", &Value::Null)["r"].as_array().unwrap().len(), 20_001);
}

/// L'Aide qui s'arrête arrête PresentMon (et ferme sa session) et la trace
#[test]
fn arret_de_l_aide() {
    let s = FauxSysteme::new();
    let etat = etat_aide();
    assert_eq!(demander(&s, &etat, "fps.veiller", &json!({ "Noms": ["bf6.exe"] }))["r"], "ok");
    assert_eq!(demander(&s, &etat, "flux.demarrer", &Value::Null)["r"], "ok");
    let tourne = Arc::clone(&s.mesures.lock().unwrap().trace_tourne);
    etat.mesures.arreter_tout(&s);
    assert!(!tourne.load(Ordering::SeqCst));
    assert_eq!(
        s.mesures.lock().unwrap().lancements.last().unwrap(),
        "--terminate_existing_session --session_name Nevermind"
    );
}
