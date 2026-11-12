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
    audits: AtomicUsize,
    caches_vides: AtomicUsize,
    /// Le DNS fixé à la main avant Nevermind (NameServer), et le Bureau à distance activé (M9)
    dns_avant: String,
    rdp: bool,
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
        self.audits.fetch_add(1, Ordering::SeqCst);
        assert!(!ips.iter().any(|i| i == "192.168.1.42"), "ce PC ne doit pas être audité");
        Ok(
            json!({ "Wifi": { "Ssid": "Essai", "Auth": "Ouvrir", "Cipher": "" }, "Profiles": [], "Ssdp": [], "Ports": ["192.168.1.50|23"],
            "Dns": { "Servers": ["192.168.1.1"], "Redirect": [], "NxHijack": "" }, "Pc": { "FirewallOff": ["Public"], "Category": "Private", "Rdp": self.rdp } }),
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
        self.dns_avant.clone()
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
    fn passer_public(&self, _: &nevermind_noyau::Permis, _: i64) -> Result<(), String> {
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

/// Audit G3 : changer le DNS ou redémarrer la carte passe par l'Aide (jusqu'à 60 s ; SMBv1 jusqu'à 15 min, par le
/// même chemin) : dans un travail de fond. Le clic rend la main tout de suite, le texte d'attente est montré aussitôt,
/// un deuxième clic ne refait rien, et la fin donne les mêmes messages qu'avant. Fausse Aide : rien sur le vrai PC.
#[test]
fn les_gestes_par_l_aide_ne_figent_pas() {
    use std::time::Instant;
    let s = Arc::new(FausseSonde::default());
    *s.bps.lock().unwrap() = 1e8;
    let ctx = contexte("fond").partage();
    let (feu, attente) = std::sync::mpsc::channel::<()>();
    let attente = Mutex::new(Some(attente));
    let demandes = Arc::new(Mutex::new(Vec::<String>::new()));
    let d = demandes.clone();
    ctx.fausse_aide(Arc::new(move |a: &str, _p: &Value| {
        d.lock().unwrap().push(a.to_string());
        if let Some(r) = attente.lock().unwrap().take() {
            let _ = r.recv_timeout(Duration::from_secs(60));
        }
        Ok(json!("ok"))
    }));
    let mut b = BlocReseau::avec_sonde(s.clone());
    let _ = page(&mut b, &ctx, "connexion");
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    // DNS : la main rendue tout de suite, pendant que l'Aide attend
    let mut e = Effets::default();
    let debut = Instant::now();
    assert!(b.action(&ctx, "dns", &json!("1"), &mut e));
    assert!(
        debut.elapsed() < Duration::from_secs(2),
        "le DNS a pris {:?} : le fil des blocs était figé",
        debut.elapsed()
    );
    let fin = Instant::now() + Duration::from_secs(10);
    while demandes.lock().unwrap().is_empty() {
        assert!(Instant::now() < fin, "l'Aide n'est pas appelée");
        std::thread::sleep(Duration::from_millis(10));
    }
    // Un deuxième clic pendant ce temps : rien ne repart, et c'est dit en clair (lot B ; avant, seulement le journal)
    let mut e = Effets::default();
    assert!(b.action(&ctx, "dns", &json!("1"), &mut e));
    assert_eq!(e.0, [Effet::Annonce("Attends la fin de « Changer le DNS » (Réseau)".into())]);
    let p = page(&mut b, &ctx, "connexion");
    assert!(p.to_string().contains("Redémarrer la carte réseau"), "la page ne répond pas");
    feu.send(()).unwrap();
    let mut tout = Vec::new();
    while !tout.iter().any(|x| matches!(x, Effet::Annonce(t) if t == "DNS modifié.")) {
        assert!(Instant::now() < fin, "le DNS ne finit pas : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(tout.iter().any(|x| matches!(x, Effet::Signal { nom, .. } if nom == "historique")));
    assert_eq!(*demandes.lock().unwrap(), ["dns.regler"], "le DNS est reparti");
    // (l'attente du DNS montrée, puis retirée à la fin ; lot B)
    let pastilles: Vec<Value> = tout
        .iter()
        .filter_map(|x| match x {
            Effet::Signal { nom, donnees } if nom == "occupe" => Some(donnees["texte"].clone()),
            _ => None,
        })
        .collect();
    assert_eq!(pastilles, [json!("Changement du DNS..."), json!("")]);
    // La carte : « Redémarrage de la carte réseau... » tout de suite, puis « elle se reconnecte »
    let mut e = Effets::default();
    b.action(&ctx, "carte-relancer", &Value::Null, &mut e);
    let mut e = Effets::default();
    assert!(b.action(&ctx, "reseau-question", &json!({ "id": "carte", "valeur": "oui" }), &mut e));
    // (lot I : la pastille porte sa clé)
    let occupe = |t: &str| Effet::Signal { nom: "occupe".into(), donnees: json!({ "texte": t, "travail": "carte" }) };
    assert!(e.0.contains(&occupe("Redémarrage de la carte réseau...")), "{:?}", e.0);
    let mut tout = Vec::new();
    while !tout.contains(&occupe("Carte réseau redémarrée, elle se reconnecte...")) {
        assert!(Instant::now() < fin + Duration::from_secs(10), "la carte ne finit pas : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = std::fs::remove_dir_all(&ctx.donnees);
}

/// M9 (audit du 08/11) : « Tout restaurer » et la désinstallation ne remettaient ni le DNS ni le Bureau à distance,
/// parce que rien n'était noté dans la sauvegarde. Maintenant la valeur d'avant est notée dans sauvegarde.json AVANT
/// que l'Aide ne change quoi que ce soit (Set-Dns, Set-Reg). Fausse Aide : rien sur le vrai PC.
#[test]
fn le_dns_et_le_bureau_a_distance_sont_notes_avant_d_etre_changes() {
    use std::time::Instant;
    let s = Arc::new(FausseSonde { dns_avant: "9.9.9.9,149.112.112.112".into(), rdp: true, ..Default::default() });
    *s.bps.lock().unwrap() = 1e8;
    let ctx = contexte("sauvegarde-dns").partage();
    let fichier = ctx.donnees.join("sauvegarde.json");
    // Ce que contient sauvegarde.json au moment où l'Aide est appelée
    let vus = Arc::new(Mutex::new(Vec::<(String, Value)>::new()));
    let v = vus.clone();
    let f = fichier.clone();
    ctx.fausse_aide(Arc::new(move |a: &str, _p: &Value| {
        let t = std::fs::read_to_string(&f).unwrap_or_default();
        let j = serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).unwrap_or(Value::Null);
        v.lock().unwrap().push((a.to_string(), j));
        Ok(json!("ok"))
    }));
    let mut b = BlocReseau::avec_sonde(s.clone());
    let _ = page(&mut b, &ctx, "connexion");
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    let fin = Instant::now() + Duration::from_secs(10);
    let mut e = Effets::default();
    assert!(b.action(&ctx, "dns", &json!("1"), &mut e));
    let mut tout = e.0;
    while !tout.iter().any(|x| matches!(x, Effet::Annonce(t) if t == "DNS modifié.")) {
        assert!(Instant::now() < fin, "le DNS ne finit pas : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    {
        let v = vus.lock().unwrap();
        assert_eq!(v[0].0, "dns.regler");
        assert_eq!(
            v[0].1["Dns"]["{ESSAI}"], "9.9.9.9,149.112.112.112",
            "le DNS d'avant n'est pas noté avant : {:?}",
            v[0].1
        );
    }
    assert_eq!(ctx.sauvegarde().dns.get("{ESSAI}"), Some(&json!("9.9.9.9,149.112.112.112")));
    // Un deuxième changement ne remplace pas le DNS d'avant Nevermind
    let mut e = Effets::default();
    b.action(&ctx, "dns", &json!("2"), &mut e);
    let mut tout = e.0;
    while vus.lock().unwrap().len() < 2 {
        assert!(Instant::now() < fin + Duration::from_secs(10), "le deuxième DNS ne part pas : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(ctx.sauvegarde().dns.get("{ESSAI}"), Some(&json!("9.9.9.9,149.112.112.112")));

    // Le Bureau à distance, depuis l'audit : « Désactiver », la question, puis l'Aide
    let mut e = Effets::default();
    for _ in 0..100 {
        b.travaux_finis(&ctx, &mut e);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(b.action(&ctx, "clic:BtnNetAudit", &Value::Null, &mut e));
    attendre(&mut b, &ctx, &mut e, |b, c| {
        b.pages(c, "panneau").get("panneau").is_some_and(|p| p["module"]["titre"] == "Points à corriger")
    });
    let pan = page(&mut b, &ctx, "panneau");
    let blocs = pan["blocs"].as_array().unwrap();
    let i = blocs.iter().position(|x| x["texte"] == "Bureau à distance activé").expect("le point Bureau à distance");
    let bouton = blocs[i..].iter().find(|x| x["texte"] == "Désactiver").cloned().expect("le bouton Désactiver");
    let mut e = Effets::default();
    assert!(b.action(&ctx, bouton["id"].as_str().unwrap(), &Value::Null, &mut e));
    let q =
        e.0.iter()
            .find_map(|x| match x {
                Effet::Signal { nom, donnees } if nom == "question" => Some(donnees.clone()),
                _ => None,
            })
            .expect("la question");
    assert!(q["texte"].as_str().unwrap().starts_with("Désactiver le Bureau à distance ?"), "{q}");
    let mut e = Effets::default();
    assert!(b.action(&ctx, "reseau-question", &json!({ "id": q["id"], "valeur": "oui" }), &mut e));
    let mut tout = e.0;
    while !tout.iter().any(|x| matches!(x, Effet::Signal { nom, .. } if nom == "historique")) {
        assert!(Instant::now() < fin + Duration::from_secs(20), "le Bureau à distance ne finit pas : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    let cle = r"HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server|fDenyTSConnections";
    let v = vus.lock().unwrap();
    let (a, sv) = v.iter().find(|(a, _)| a == "reg.ecrire").expect("l'Aide écrit le registre");
    assert_eq!(a, "reg.ecrire");
    assert!(sv["Registry"].get(cle).is_some(), "le Bureau à distance n'est pas noté avant : {sv}");
    assert!(ctx.sauvegarde().registre.contains_key(cle), "pas dans la sauvegarde");
    let _ = std::fs::remove_dir_all(&ctx.donnees);
}

/// Petit de l'audit du 08/11 : un changement définitif de l'audit (pare-feu réactivé...) arrivait vide dans
/// l'historique (« (définitif) » sans rien devant) : il était écrit sous `Detail`, alors que l'historique, « Tout
/// restaurer » et la désinstallation lisent `Quoi` (Add-ChangementDefinitif : Type, Quoi, Retour).
#[test]
fn un_changement_definitif_de_l_audit_a_son_texte_dans_l_historique() {
    use std::time::Instant;
    let s = Arc::new(FausseSonde::default());
    *s.bps.lock().unwrap() = 1e8;
    let ctx = contexte("definitif").partage();
    ctx.fausse_aide(Arc::new(|_: &str, _: &Value| Ok(json!("ok"))));
    let mut b = BlocReseau::avec_sonde(s.clone());
    let _ = page(&mut b, &ctx, "reseau");
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnNetAudit", &Value::Null, &mut e));
    attendre(&mut b, &ctx, &mut e, |b, c| {
        b.pages(c, "panneau").get("panneau").is_some_and(|p| p["module"]["titre"] == "Points à corriger")
    });
    let pan = page(&mut b, &ctx, "panneau");
    let bouton =
        pan["blocs"].as_array().unwrap().iter().find(|x| x["texte"] == "Réactiver le pare-feu").cloned().unwrap();
    let mut e = Effets::default();
    b.action(&ctx, bouton["id"].as_str().unwrap(), &Value::Null, &mut e);
    let q =
        e.0.iter()
            .find_map(|x| match x {
                Effet::Signal { nom, donnees } if nom == "question" => Some(donnees.clone()),
                _ => None,
            })
            .unwrap();
    let mut e = Effets::default();
    b.action(&ctx, "reseau-question", &json!({ "id": q["id"], "valeur": "oui" }), &mut e);
    let mut tout = e.0;
    let fin = Instant::now() + Duration::from_secs(10);
    let h = loop {
        if let Some(h) = tout.iter().find_map(|x| match x {
            Effet::Signal { nom, donnees } if nom == "historique" => Some(donnees.clone()),
            _ => None,
        }) {
            break h;
        }
        assert!(Instant::now() < fin, "pas d'historique : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(h["log"][0]["Type"], "definitif");
    assert_eq!(h["log"][0]["Quoi"], "Pare-feu réactivé sur tous les réseaux", "{h}");
    assert!(h["log"][0]["Retour"].as_str().is_some_and(|r| r.starts_with("Sécurité Windows")), "{h}");
    let _ = std::fs::remove_dir_all(&ctx.donnees);
}

/// Petit de l'audit du 08/11 : une page d'erreur (403, portail de Wi-Fi public) donnait un fabricants.txt vide, qui
/// n'était plus jamais retéléchargé. Seule une vraie liste est gardée, et un fichier vide est retéléchargé.
#[test]
fn seule_une_vraie_liste_de_fabricants_est_gardee() {
    use nevermind_reseau::appareil::{fabricants_complets, fabricants_depuis_csv, lire_fabricants};
    assert_eq!(fabricants_depuis_csv(""), None);
    assert_eq!(fabricants_depuis_csv("<html><body>403 Forbidden</body></html>"), None);
    assert_eq!(fabricants_depuis_csv("Registry,Assignment,Organization Name\nMA-L,F0B429,\"Apple, Inc.\"\n"), None);
    let mut csv = String::from("Registry,Assignment,Organization Name,Organization Address\n");
    for i in 0..1200 {
        csv.push_str(&format!("MA-L,{i:06X},\"Fabricant {i}, Inc.\",Adresse\n"));
    }
    let t = fabricants_depuis_csv(&csv).expect("une vraie liste");
    assert!(t.starts_with('\u{feff}'));
    assert_eq!(lire_fabricants(&t).get("00002A").map(String::as_str), Some("Fabricant 42, Inc."));
    assert_eq!(lire_fabricants(&t).len(), 1200);
    assert!(fabricants_complets(&t));
    assert!(!fabricants_complets("\u{feff}"), "le fichier vide d'avant est retéléchargé");
}

/// Lot A du second audit (G-A, M-C) : un seul geste qui change le PC à la fois, toutes pages confondues. Pendant
/// qu'un geste d'une autre page tient le verrou (« Appliquer » de Ton PC), « Changer le DNS » et « Couper le Bureau à
/// distance » sont REFUSÉS avec « Attends la fin de « Appliquer » (Ton PC) » ; rien n'est noté ni demandé à l'Aide, et
/// rien ne part tout seul quand le verrou est rendu. Ensuite, un nouveau clic passe. Fausse Aide : rien sur le vrai PC.
#[test]
fn un_clic_pendant_le_geste_d_une_autre_page_est_refuse() {
    use std::time::Instant;
    let s = Arc::new(FausseSonde { dns_avant: "9.9.9.9".into(), rdp: true, ..Default::default() });
    *s.bps.lock().unwrap() = 1e8;
    let ctx = contexte("refus").partage();
    let demandes = Arc::new(Mutex::new(Vec::<String>::new()));
    let dm = demandes.clone();
    ctx.fausse_aide(Arc::new(move |a: &str, _p: &Value| {
        dm.lock().unwrap().push(a.to_string());
        Ok(json!("ok"))
    }));
    let mut b = BlocReseau::avec_sonde(s.clone());
    let _ = page(&mut b, &ctx, "connexion");
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    let refus = Effet::Annonce("Attends la fin de « Appliquer » (Ton PC)".into());

    // Le DNS pendant « Appliquer »
    let appliquer = ctx.prendre_geste("Appliquer", "Ton PC").unwrap();
    let mut e = Effets::default();
    assert!(b.action(&ctx, "dns", &json!("1"), &mut e));
    assert!(e.0.contains(&refus), "le clic n'est pas refusé avec le bon texte : {:?}", e.0);
    assert!(ctx.sauvegarde().dns.is_empty(), "le DNS d'origine a été noté alors que rien ne change");

    // Le Bureau à distance, depuis l'audit, pendant « Appliquer »
    drop(appliquer);
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnNetAudit", &Value::Null, &mut e));
    attendre(&mut b, &ctx, &mut e, |b, c| {
        b.pages(c, "panneau").get("panneau").is_some_and(|p| p["module"]["titre"] == "Points à corriger")
    });
    let pan = page(&mut b, &ctx, "panneau");
    let blocs = pan["blocs"].as_array().unwrap();
    let i = blocs.iter().position(|x| x["texte"] == "Bureau à distance activé").expect("le point Bureau à distance");
    let bouton = blocs[i..].iter().find(|x| x["texte"] == "Désactiver").cloned().expect("le bouton Désactiver");
    let mut e = Effets::default();
    assert!(b.action(&ctx, bouton["id"].as_str().unwrap(), &Value::Null, &mut e));
    let q =
        e.0.iter()
            .find_map(|x| match x {
                Effet::Signal { nom, donnees } if nom == "question" => Some(donnees.clone()),
                _ => None,
            })
            .expect("la question");
    let appliquer = ctx.prendre_geste("Appliquer", "Ton PC").unwrap();
    let mut e = Effets::default();
    assert!(b.action(&ctx, "reseau-question", &json!({ "id": q["id"], "valeur": "oui" }), &mut e));
    assert!(e.0.contains(&refus), "le Bureau à distance n'est pas refusé avec le bon texte : {:?}", e.0);

    // Le verrou rendu : rien ne part tout seul
    drop(appliquer);
    let fin = Instant::now() + Duration::from_millis(600);
    while Instant::now() < fin {
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        b.tic(&ctx, false, &mut e);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(demandes.lock().unwrap().is_empty(), "un clic refusé est parti plus tard : {:?}", demandes.lock().unwrap());
    let cle = r"HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server|fDenyTSConnections";
    assert!(!ctx.sauvegarde().registre.contains_key(cle), "le Bureau à distance a été noté");

    // Un nouveau clic passe
    let mut e = Effets::default();
    assert!(b.action(&ctx, "dns", &json!("1"), &mut e));
    assert!(!e.0.contains(&refus));
    let fin = Instant::now() + Duration::from_secs(10);
    while !demandes.lock().unwrap().iter().any(|a| a == "dns.regler") {
        assert!(Instant::now() < fin, "le nouveau DNS ne part pas");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = std::fs::remove_dir_all(&ctx.donnees);
}

/// Lot B du second audit : la fin d'un geste de l'audit (SMBv1 peut durer 15 min) ne ramène plus l'utilisateur sur le
/// panneau de l'audit quand il est parti ailleurs, et ne relance pas l'audit (balayage des appareils) : il est refait
/// quand l'utilisateur revient sur son panneau, jamais pendant une partie. Fausse Aide : rien sur le vrai PC.
#[test]
fn la_fin_d_un_geste_ne_force_plus_l_ecran_de_l_audit() {
    use std::time::Instant;
    let s = Arc::new(FausseSonde::default());
    *s.bps.lock().unwrap() = 1e8;
    let ctx = contexte("audit-ailleurs").partage();
    let (feu, attente) = std::sync::mpsc::channel::<()>();
    let attente = Mutex::new(Some(attente));
    ctx.fausse_aide(Arc::new(move |_: &str, _: &Value| {
        if let Some(r) = attente.lock().unwrap().take() {
            let _ = r.recv_timeout(Duration::from_secs(60));
        }
        Ok(json!("ok"))
    }));
    let mut b = BlocReseau::avec_sonde(s.clone());
    let _ = page(&mut b, &ctx, "reseau");
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnNetAudit", &Value::Null, &mut e));
    attendre(&mut b, &ctx, &mut e, |b, c| {
        b.pages(c, "panneau").get("panneau").is_some_and(|p| p["module"]["titre"] == "Points à corriger")
    });
    assert_eq!(s.audits.load(Ordering::SeqCst), 1);
    let pan = page(&mut b, &ctx, "panneau");
    let bouton =
        pan["blocs"].as_array().unwrap().iter().find(|x| x["texte"] == "Réactiver le pare-feu").cloned().unwrap();
    let mut e = Effets::default();
    b.action(&ctx, bouton["id"].as_str().unwrap(), &Value::Null, &mut e);
    let q =
        e.0.iter()
            .find_map(|x| match x {
                Effet::Signal { nom, donnees } if nom == "question" => Some(donnees.clone()),
                _ => None,
            })
            .unwrap();
    let mut e = Effets::default();
    b.action(&ctx, "reseau-question", &json!({ "id": q["id"], "valeur": "oui" }), &mut e);
    // L'utilisateur part sur une autre page pendant le geste, puis le geste finit
    let _ = page(&mut b, &ctx, "jeux");
    feu.send(()).unwrap();
    let mut tout = e.0;
    let fin = Instant::now() + Duration::from_secs(10);
    while !tout.iter().any(|x| matches!(x, Effet::Signal { nom, .. } if nom == "historique")) {
        assert!(Instant::now() < fin, "le geste ne finit pas : {tout:?}");
        let mut e = Effets::default();
        b.tic(&ctx, false, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    for _ in 0..10 {
        let mut e = Effets::default();
        b.tic(&ctx, false, &mut e);
        tout.extend(e.0);
    }
    assert!(!tout.contains(&Effet::OuvrirEcran("panneau".into())), "l'écran est forcé sur l'audit : {tout:?}");
    assert_eq!(s.audits.load(Ordering::SeqCst), 1, "l'audit est relancé alors que l'utilisateur est ailleurs");
    // Une partie commence ; il revient sur le panneau : toujours rien pendant la partie
    let mut e = Effets::default();
    b.signal(&ctx, "partie-commencee", &json!({}), &mut e);
    let _ = page(&mut b, &ctx, "panneau");
    let mut e = Effets::default();
    b.tic(&ctx, false, &mut e);
    assert_eq!(s.audits.load(Ordering::SeqCst), 1, "l'audit est relancé pendant une partie");
    // La partie finie, sur le panneau : l'audit est refait
    let mut e = Effets::default();
    b.signal(&ctx, "partie-finie", &json!({}), &mut e);
    let _ = page(&mut b, &ctx, "panneau");
    let mut e = Effets::default();
    attendre(&mut b, &ctx, &mut e, |_, _| s.audits.load(Ordering::SeqCst) == 2);
    let _ = std::fs::remove_dir_all(&ctx.donnees);
}

/// Lot I, point 6 : « Audit » cliqué sans appareils connus lance d'abord un scan (une minute, plus le téléchargement
/// des fabricants la première fois), puis l'audit. Avant, la fin du scan ouvrait le panneau de l'audit où que soit
/// l'utilisateur. Maintenant, s'il est parti ailleurs (ou si une partie a commencé), l'écran ne saute pas : l'audit
/// est à refaire, et un nouveau clic sur « Audit » le lance.
#[test]
fn l_audit_apres_un_scan_ne_force_pas_l_ecran() {
    let s = Arc::new(FausseSonde::default());
    *s.bps.lock().unwrap() = 1e8;
    let ctx = contexte("audit-apres-scan");
    let mut b = BlocReseau::avec_sonde(s.clone());
    let _ = page(&mut b, &ctx, "reseau");
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnNetAudit", &Value::Null, &mut e));
    assert!(!e.0.contains(&Effet::OuvrirEcran("panneau".into())));
    // L'utilisateur part sur une autre page pendant le scan
    let fini = |b: &mut BlocReseau, c: &Contexte| {
        b.pages(c, "jeux").get("reseau").is_some_and(|p| p["actions"][0]["texte"] == "Scanner de nouveau")
    };
    let mut tout = Vec::new();
    for _ in 0..250 {
        let mut e = Effets::default();
        b.tic(&ctx, false, &mut e);
        tout.extend(e.0);
        if fini(&mut b, &ctx) {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(fini(&mut b, &ctx), "le scan ne finit pas");
    for _ in 0..10 {
        let mut e = Effets::default();
        b.tic(&ctx, false, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!tout.contains(&Effet::OuvrirEcran("panneau".into())), "l'écran est forcé sur l'audit : {tout:?}");
    assert_eq!(s.audits.load(Ordering::SeqCst), 0, "l'audit est lancé alors que l'utilisateur est ailleurs");
    // De retour sur la page Réseau : « Audit » le lance
    let _ = page(&mut b, &ctx, "reseau");
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnNetAudit", &Value::Null, &mut e));
    assert!(e.0.contains(&Effet::OuvrirEcran("panneau".into())));
    let mut e = Effets::default();
    attendre(&mut b, &ctx, &mut e, |_, _| s.audits.load(Ordering::SeqCst) == 1);
    let _ = std::fs::remove_dir_all(&ctx.donnees);
}

/// Lot I, point 6 : la pastille du test de connexion était envoyée SANS clé ; sa fin (texte vide sans clé) effaçait
/// toutes les pastilles du bloc, même celle de SMBv1 qui tournait encore. Maintenant chaque pastille porte sa clé.
#[test]
fn les_pastilles_du_reseau_portent_leur_cle() {
    let s = Arc::new(FausseSonde::default());
    *s.bps.lock().unwrap() = 1e8;
    let ctx = contexte("pastilles-cle");
    let mut b = BlocReseau::avec_sonde(s.clone());
    let _ = page(&mut b, &ctx, "connexion");
    let mut tout = Vec::new();
    let mut e = Effets::default();
    assert!(b.action(&ctx, "clic:BtnPing", &Value::Null, &mut e));
    tout.extend(e.0);
    for _ in 0..400 {
        let mut e = Effets::default();
        b.tic(&ctx, false, &mut e);
        tout.extend(e.0);
        if page(&mut b, &ctx, "connexion")["module"]["titre"] == "Ping 4 ms" {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let p: Vec<Value> = tout
        .iter()
        .filter_map(|x| match x {
            Effet::Signal { nom, donnees } if nom == "occupe" => Some(donnees.clone()),
            _ => None,
        })
        .collect();
    assert!(p.iter().any(|d| d["texte"] == "Test de la connexion..."), "{tout:?}");
    assert_eq!(p.last().map(|d| d["texte"].clone()), Some(json!("")), "l'attente reste à l'écran : {p:?}");
    assert!(
        p.iter().all(|d| d["travail"].as_str().is_some_and(|t| !t.is_empty())),
        "une pastille sans clé efface celles des autres gestes : {p:?}"
    );
    let _ = std::fs::remove_dir_all(&ctx.donnees);
}
