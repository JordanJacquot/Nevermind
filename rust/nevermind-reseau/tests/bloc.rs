//! Le bloc entier avec une fausse sonde : aucun ping, aucun scan, rien sur le vrai PC. Les données sont dans un
//! dossier d'essai. L'Aide n'est pas branchée dans ce noyau : ce qui écrit doit échouer proprement (message), jamais
//! toucher le PC.

use nevermind_noyau::{Bloc, Contexte, Effet, Effets, dossier_essai};
use nevermind_reseau::BlocReseau;
use nevermind_reseau::appareil::{Fabricants, InterfaceLocale, Locaux};
use nevermind_reseau::net::Connexion;
use nevermind_reseau::scan::{BrutApprofondi, BrutScan};
use nevermind_reseau::sonde::{Avancement, Sonde};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct FausseSonde {
    bps: Mutex<f64>,
    pings: AtomicUsize,
    scans: AtomicUsize,
    caches_vides: AtomicUsize,
}

fn net(bps: f64) -> Connexion {
    Connexion {
        if_index: 7,
        gateway: "192.168.1.1".into(),
        nom: "Ethernet".into(),
        desc: "Carte d'essai".into(),
        speed: "100 Mbps".into(),
        wifi: false,
        guid: "{ESSAI}".into(),
        mac: "04-D9-F5-00-00-01".into(),
        ip: Some("192.168.1.42".into()),
        prefix: 24,
        dns: "192.168.1.1".into(),
        bps,
        vitesses: vec!["1,0 Gbit/s Duplex intégral".into()],
        vitesses_reg: vec![],
    }
}

impl Sonde for FausseSonde {
    fn connexion(&self) -> Option<Connexion> {
        Some(net(*self.bps.lock().unwrap()))
    }
    fn locaux(&self) -> Locaux {
        vec![(
            "192.168.1.42".into(),
            InterfaceLocale {
                mac: "04-D9-F5-00-00-01".into(),
                nom: "Ethernet".into(),
                desc: "x".into(),
                wifi: false,
                active: true,
            },
        )]
    }
    fn poste(&self) -> String {
        "PC-ESSAI".into()
    }
    fn scan(&self, ips: &[String], _: i64, av: &Avancement) -> Result<BrutScan, String> {
        self.scans.fetch_add(1, Ordering::Relaxed);
        assert_eq!(ips.len(), 254);
        av.mettre(100.0);
        Ok(BrutScan {
            vivants: vec!["192.168.1.1|2|64".into(), "192.168.1.42|0|128".into(), "192.168.1.20|15|64".into()],
            arp: vec![
                "192.168.1.1|D8-EB-46-00-00-01|Reachable".into(),
                "192.168.1.20|F0-B4-29-00-00-02|Reachable".into(),
                "192.168.1.30|02-11-22-33-44-55|Stale".into(),
            ],
            noms: vec!["192.168.1.20|iPhone-de-Lea.home".into()],
        })
    }
    fn approfondi(
        &self,
        _: &str,
        _: i64,
        manquants: &[String],
        _: &[String],
        _: &Avancement,
    ) -> Result<BrutApprofondi, String> {
        assert!(!manquants.iter().any(|m| m == "192.168.1.20"));
        Ok(BrutApprofondi {
            arp: vec!["192.168.1.50|BC-54-36-00-00-09".into()],
            ports: vec!["192.168.1.50|554".into(), "192.168.1.50|8000".into()],
            ..Default::default()
        })
    }
    fn audit(&self, _: &str, _: &str, _: i64, ips: &[String], _: &[i64], _: &Avancement) -> Result<Value, String> {
        assert!(!ips.iter().any(|i| i == "192.168.1.42"), "ce PC ne doit pas être audité");
        Ok(
            json!({ "Wifi": { "Ssid": "Essai", "Auth": "Ouvrir", "Cipher": "" }, "Profiles": [], "Ssdp": [], "Ports": ["192.168.1.50|23"],
            "Dns": { "Servers": ["192.168.1.1"], "Redirect": [], "NxHijack": "" }, "Pc": { "FirewallOff": ["Public"], "Category": "Private" } }),
        )
    }
    fn ports(&self, _: &str, _: &[i64], _: u32) -> Vec<i64> {
        vec![80, 23]
    }
    fn ping(&self, _: &str, _: u32) -> Option<f64> {
        self.pings.fetch_add(1, Ordering::Relaxed);
        Some(4.0)
    }
    fn dns_fixes(&self, _: &str) -> String {
        String::new()
    }
    fn vider_cache_dns(&self) -> Result<(), String> {
        self.caches_vides.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn attendre_carte(&self, _: i64) -> bool {
        true
    }
    fn fabricants(&self, _: &Path) -> Option<Fabricants> {
        Some(
            [
                ("D8EB46".to_string(), "Sagemcom Broadband SAS".to_string()),
                ("BC5436".to_string(), "Hangzhou Hikvision Digital Technology Co.,Ltd.".to_string()),
            ]
            .into(),
        )
    }
    fn passer_public(&self, _: i64) -> Result<(), String> {
        Err("essai".into())
    }
    fn maintenant(&self) -> [u32; 5] {
        [2026, 11, 6, 21, 30]
    }
}

fn contexte(nom: &str) -> Contexte {
    let d = dossier_essai(&format!("reseau-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    Contexte::nouveau(d.clone(), d, false, true)
}

/// Des passages de `tic` jusqu'à ce que `fini` soit vrai (5 s au plus)
fn attendre(b: &mut BlocReseau, ctx: &Contexte, e: &mut Effets, fini: impl Fn(&mut BlocReseau, &Contexte) -> bool) {
    for _ in 0..250 {
        b.tic(ctx, false, e);
        if fini(b, ctx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("délai dépassé");
}

fn page(b: &mut BlocReseau, ctx: &Contexte, nom: &str) -> Value {
    b.pages(ctx, nom).get(nom).cloned().unwrap_or(Value::Null)
}

#[test]
fn scan_audit_fiche() {
    let s = Arc::new(FausseSonde::default());
    *s.bps.lock().unwrap() = 1e9;
    let ctx = contexte("scan");
    let mut b = BlocReseau::avec_sonde(s.clone());
    let mut e = Effets::default();
    let p = page(&mut b, &ctx, "reseau");
    assert_eq!(p["module"]["titre"], "Pas encore scanné");
    assert!(b.action(&ctx, "clic:BtnNetScan", &Value::Null, &mut e));
    attendre(&mut b, &ctx, &mut e, |b, c| page(b, c, "reseau")["actions"][0]["texte"] == "Scanner de nouveau");
    let p = page(&mut b, &ctx, "reseau");
    let titres: Vec<String> = p["blocs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["type"] == "ligne")
        .map(|x| x["texte"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(titres.contains(&"iPhone-de-Lea".to_string()), "{titres:?}");
    assert!(titres.contains(&"PC-ESSAI (ce PC)".to_string()));
    // La caméra trouvée par la recherche approfondie (fabricant + flux vidéo)
    assert!(
        p["blocs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["texte"] == "Hikvision Digital" || x["etiquette"].as_str().unwrap_or("").contains("Caméra")),
        "{p}"
    );
    let connus: Value =
        serde_json::from_str(&std::fs::read_to_string(ctx.donnees.join("appareils.json")).unwrap()).unwrap();
    assert_eq!(connus["D8-EB-46-00-00-01"], "2026-11-06");
    assert_eq!(s.scans.load(Ordering::Relaxed), 1);

    // L'audit : panneau, note, audit.json
    assert!(b.action(&ctx, "clic:BtnNetAudit", &Value::Null, &mut e));
    assert!(e.0.contains(&Effet::OuvrirEcran("panneau".into())));
    attendre(&mut b, &ctx, &mut e, |b, c| {
        b.pages(c, "panneau").get("panneau").is_some_and(|p| p["module"]["titre"] == "Points à corriger")
    });
    let a: Value = serde_json::from_str(&std::fs::read_to_string(ctx.donnees.join("audit.json")).unwrap()).unwrap();
    assert_eq!(a["Date"], "06/11/2026 à 21:30");
    assert!(a["Score"].as_i64().unwrap() < 60);
    let r = page(&mut b, &ctx, "reseau");
    assert_eq!(r["module"]["boutons"][1]["id"], "clic:BtnNetAuditView");
    // Un geste de l'audit qui demande une confirmation : la question, puis l'Aide (pas branchée ici : un message)
    let pan = page(&mut b, &ctx, "panneau");
    let bouton =
        pan["blocs"].as_array().unwrap().iter().find(|x| x["texte"] == "Réactiver le pare-feu").cloned().unwrap();
    let mut e = Effets::default();
    assert!(b.action(&ctx, bouton["id"].as_str().unwrap(), &Value::Null, &mut e));
    let q =
        e.0.iter()
            .find_map(|x| match x {
                Effet::Signal { nom, donnees } if nom == "question" => Some(donnees.clone()),
                _ => None,
            })
            .unwrap();
    let mut e = Effets::default();
    assert!(b.action(&ctx, "reseau-question", &json!({ "id": q["id"], "valeur": "oui" }), &mut e));
    assert!(e.0.iter().any(|x| matches!(x, Effet::Annonce(t) if t.contains("Aide"))), "{:?}", e.0);
    assert!(!e.0.iter().any(|x| matches!(x, Effet::Signal { nom, .. } if nom == "historique")));

    // La fiche d'un appareil : ping en direct et services
    let mut e = Effets::default();
    assert!(b.action(&ctx, "panneau:close", &Value::Null, &mut e));
    assert!(b.action(&ctx, "net-app:2", &Value::Null, &mut e));
    attendre(&mut b, &ctx, &mut e, |b, c| {
        let p = page(b, c, "panneau");
        p["blocs"].as_array().is_some_and(|a| a.iter().any(|x| x["texte"] == "Connexion stable."))
            && p.to_string().contains("Telnet")
    });
    let n = s.pings.load(Ordering::Relaxed);
    // Écran caché : plus de ping
    for _ in 0..10 {
        b.tic(&ctx, true, &mut e);
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(s.pings.load(Ordering::Relaxed) <= n + 1);
    assert!(b.action(&ctx, "retour", &Value::Null, &mut e));
    assert!(b.pages(&ctx, "reseau").get("panneau").is_none());
    let _ = std::fs::remove_dir_all(&ctx.donnees);
}

#[test]
fn connexion_carte_dns() {
    let s = Arc::new(FausseSonde::default());
    *s.bps.lock().unwrap() = 1e8;
    let ctx = contexte("connexion");
    let mut b = BlocReseau::avec_sonde(s.clone());
    let p = page(&mut b, &ctx, "connexion");
    assert!(p.to_string().contains("Redémarrer la carte réseau"));
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    let c =
        e.0.iter()
            .find_map(|x| match x {
                Effet::Signal { nom, donnees } if nom == "connexion" => Some(donnees.clone()),
                _ => None,
            })
            .unwrap();
    assert_eq!(c["bridage"], true);
    // Redémarrer la carte : une question d'abord ; « non » ne fait rien
    let mut e = Effets::default();
    assert!(b.action(&ctx, "carte-relancer", &Value::Null, &mut e));
    assert!(e.0.iter().any(|x| matches!(x, Effet::Signal { nom, .. } if nom == "question")));
    let mut e = Effets::default();
    assert!(b.action(&ctx, "reseau-question", &json!({ "id": "carte", "valeur": "non" }), &mut e));
    assert!(e.0.is_empty());
    // DNS : l'Aide (pas branchée ici) refuse, rien n'est noté dans l'historique
    let mut e = Effets::default();
    assert!(b.action(&ctx, "dns", &json!("1"), &mut e));
    assert!(e.0.iter().any(|x| matches!(x, Effet::Annonce(_))));
    assert!(!e.0.iter().any(|x| matches!(x, Effet::Signal { nom, .. } if nom == "historique")));
    // Vider le cache DNS : par la sonde
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnDnsFlush", &Value::Null, &mut e));
    assert_eq!(s.caches_vides.load(Ordering::Relaxed), 1);
    // Le test du ping (faux ping : 4 ms partout)
    assert!(b.action(&ctx, "clic:BtnPing", &Value::Null, &mut e));
    let mut e = Effets::default();
    for _ in 0..400 {
        b.tic(&ctx, false, &mut e);
        if page(&mut b, &ctx, "connexion")["module"]["titre"] == "Ping 4 ms" {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(page(&mut b, &ctx, "connexion")["module"]["titre"], "Ping 4 ms");
    assert_eq!(s.pings.load(Ordering::Relaxed), 60);
    // Une action qui n'est pas au bloc
    assert!(!b.action(&ctx, "clic:BtnAutre", &Value::Null, &mut e));
    assert!(!b.action(&ctx, "panneau:close", &Value::Null, &mut e));
    let _ = std::fs::remove_dir_all(&ctx.donnees);
}

/// Lecture réelle (sans droits, rien n'est envoyé sur le réseau) : la connexion active, à comparer avec
/// `$ActiveNetWork` de la version PowerShell. Lancé à la main : cargo test -- --ignored lecture_reelle
#[cfg(windows)]
#[test]
#[ignore]
fn lecture_reelle() {
    use nevermind_reseau::windows::SondeWindows;
    let s = SondeWindows;
    println!("CONNEXION {}", s.connexion().map(|c| c.json()).unwrap_or(Value::Null));
    for (ip, l) in s.locaux() {
        println!("LOCAL {ip} {} {} wifi={} actif={}", l.mac, l.nom, l.wifi, l.active);
    }
}

/// Le temps d'une lecture de la connexion (Watch-CarteReseau, toutes les minutes). Lancé à la main.
#[cfg(windows)]
#[test]
#[ignore]
fn mesure_lecture() {
    use nevermind_reseau::windows::SondeWindows;
    let t: Vec<u128> = (0..5)
        .map(|_| {
            let d = std::time::Instant::now();
            let _ = SondeWindows.connexion();
            d.elapsed().as_micros()
        })
        .collect();
    println!("RUST µs {t:?}");
}
