//! Les pages de l'écran (kit) : la page Réseau (`Get-PagesReseau` d'ecran-reseau.ps1) et la page Connexion (la
//! partie « Connexion » de `Get-PagesProtection` d'ecran-protection.ps1), avec le test « Tester ma connexion »
//! (Measure-Latency, Invoke-NetTest d'onglets.ps1).

use crate::appareil::Appareil;
use crate::audit::entier_ps;
use crate::net::{Connexion, bridage, format_vitesse, texte_bridage};
use crate::scan::{FILTRES, passe_filtre};
use crate::texte::{arrondi_dec, arrondi_pair, chaine, egal, format_n0, nombre};
use serde_json::{Map, Value, json};

/// L'icône et la famille (couleur) d'un appareil dans le système solaire (`Get-SorteReseau`)
pub fn sorte_reseau(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "Ce PC" | "Ordinateur" => ("pc", "P"),
        "Box Internet" => ("box", "P"),
        "Caméra" => ("camera", "orange"),
        "Imprimante" => ("imprimante", "P"),
        "Routeur ou répéteur Wi-Fi" => ("repeteur", "vert"),
        "Appareil Apple" | "Téléphone ou tablette" | "Téléphone probable" => ("tel", "S"),
        "TV ou multimédia" => ("tv", "T"),
        "Enceinte ou audio" => ("enceinte", "T"),
        "Objet connecté" => ("ampoule", "vert"),
        "Console de jeu" => ("console", "T"),
        "Box ou décodeur TV" => ("box", "T"),
        _ => ("inconnu", "doux"),
    }
}

/// Ce que la page Réseau montre (l'état du bloc)
pub struct EtatReseau<'a> {
    pub liste: &'a [Appareil],
    pub scan: bool,
    pub scan_texte: &'a str,
    pub scan_trouves: i64,
    pub progression: f64,
    pub filtre: &'a str,
    /// audit.json (Score, Date, Bad, Warn)
    pub audit: Option<&'a Value>,
    /// L'audit complet est en mémoire (« Voir le rapport »)
    pub audit_complet: bool,
    pub surveillance: bool,
    /// Le résumé et le conseil de la liste (`NetDevSummary`, `NetDevHint`)
    pub resume: &'a str,
    pub conseil: &'a str,
}

/// Le texte et le bouton de la carte de l'audit
pub fn textes_audit(audit: Option<&Value>) -> (String, String, bool) {
    match crate::audit::carte(audit) {
        Some((t, b)) => (t, b, true),
        None => (
            "Vérifie la sécurité de ton Wi-Fi, de ta box, de tes appareils et de ce PC. Tu obtiens une note, un rapport clair et des corrections en un clic.".into(),
            "Lancer l'audit".into(),
            false,
        ),
    }
}

fn ms_texte(ms: i64) -> String {
    if ms < 1 { "< 1 ms".into() } else { format!("{ms} ms") }
}

/// La page Réseau (`Get-PagesReseau`)
pub fn page_reseau(e: &EtatReseau) -> Map<String, Value> {
    let liste = e.liste;
    let a_voir: Vec<&Appareil> = liste.iter().filter(|d| d.new || d.camera).collect();
    let boxe = liste.iter().find(|d| d.gateway);
    let (texte_audit, bouton_audit, sauve) = textes_audit(e.audit);
    let voir_rapport = sauve && e.audit_complet;
    let mut filtre = if e.filtre.is_empty() { "all".to_string() } else { e.filtre.to_string() };
    if !FILTRES.iter().any(|(id, _)| egal(id, &filtre)) {
        filtre = "all".into();
    }
    let id_filtre = FILTRES.iter().find(|(id, _)| egal(id, &filtre)).map(|(id, _)| *id).unwrap_or("all");
    let montres: Vec<&Appareil> = liste.iter().filter(|d| passe_filtre(id_filtre, d)).collect();
    let planetes: Vec<Value> = liste
        .iter()
        .filter(|d| !d.gateway)
        .map(|d| {
            let s = sorte_reseau(&d.kind);
            json!({ "nom": d.title, "icone": s.0, "couleur": s.1, "ping": d.ms.map(|m| m as f64).unwrap_or(50.0),
                "attention": d.camera, "nouveau": d.new })
        })
        .collect();
    // (« ip|mac » : une table sans majuscules, la dernière place l'emporte)
    let mut index: Vec<(String, usize)> = Vec::new();
    for (k, d) in liste.iter().enumerate() {
        let cle = format!("{}|{}", d.ip, d.mac).to_lowercase();
        match index.iter_mut().find(|(c, _)| *c == cle) {
            Some(x) => x.1 = k,
            None => index.push((cle, k)),
        }
    }
    let place =
        |d: &Appareil| index.iter().find(|(c, _)| *c == format!("{}|{}", d.ip, d.mac).to_lowercase()).map(|x| x.1);
    let ligne = |d: &Appareil| {
        let s = sorte_reseau(&d.kind);
        let mut badges = Vec::new();
        if d.soi {
            badges.push("Ce PC");
        }
        if d.gateway {
            badges.push("Ta box");
        }
        if d.new {
            badges.push("Nouveau");
        }
        if d.camera {
            badges.push("Caméra ?");
        }
        if d.hidden {
            badges.push("Discret");
        }
        let attention = d.new || d.camera;
        json!({ "type": "ligne", "id": format!("net-app:{}", place(d).map(|k| k.to_string()).unwrap_or_default()),
            "icone": s.0, "couleur": if attention { "orange" } else { s.1 }, "texte": d.title,
            "note": format!("{}{}", d.kind, if d.ip.is_empty() { String::new() } else { format!(" · {}", d.ip) }),
            "etiquette": badges.join(" · "),
            "ton": if attention { "attention" } else if d.soi || d.gateway { "ok" } else { "" },
            "valeur": d.ms.map(ms_texte) })
    };
    let n = liste.len();
    let module = if n > 0 {
        let ok = n - a_voir.len();
        let nb_nouveaux = liste.iter().filter(|d| d.new).count();
        let mut boutons = vec![json!({ "id": "clic:BtnNetAudit", "texte": bouton_audit })];
        if voir_rapport {
            boutons.push(
                json!({ "id": "clic:BtnNetAuditView", "texte": "Voir le rapport de l'audit", "style": "second" }),
            );
        }
        json!({
            "anneau": { "valeur": ok, "sur": n, "petit": format!("/{n}"), "ton": if a_voir.is_empty() { Value::Null } else { json!("attention") } },
            "titre": if a_voir.is_empty() { "Tout est en ordre".to_string() } else { format!("{} appareil{} à vérifier", a_voir.len(), if a_voir.len() > 1 { "s" } else { "" }) },
            "texte": format!("{ok} appareil{} sur {n} {} normalement.", if ok > 1 { "s" } else { "" }, if ok > 1 { "sont connus et répondent" } else { "est connu et répond" }),
            "boutons": boutons,
            "resume": [
                { "nom": "Box", "valeur": boxe.map(|b| b.title.clone()).unwrap_or_else(|| "?".into()) },
                { "nom": "Nouveaux", "valeur": nb_nouveaux.to_string(), "ton": if nb_nouveaux > 0 { "attention" } else { "ok" } }
            ]
        })
    } else {
        json!({ "icone": "radar", "couleur": "S", "titre": if e.scan { "Scan en cours" } else { "Pas encore scanné" },
            "texte": "Découvre tous les appareils connectés à ton réseau : téléphones, consoles, TV, box, objets connectés...",
            "boutons": if e.scan { json!([]) } else { json!([{ "id": "clic:BtnNetScan", "texte": "Scanner le réseau" }]) } })
    };
    let mut blocs = Vec::new();
    if e.scan {
        let k = e.scan_trouves;
        let s = if k > 1 { "s" } else { "" };
        let barre = arrondi_pair(e.progression);
        blocs.push(json!({ "type": "ligne", "icone": "radar", "couleur": "S",
            "texte": if e.scan_texte.is_empty() { "Scan du réseau..." } else { e.scan_texte },
            "note": if k != 0 { format!("{k} appareil{s} trouvé{s} pour l'instant") } else { "Ça prend une à deux minutes.".into() },
            "barre": barre, "valeur": format!("{barre} %") }));
    }
    if n > 0 {
        blocs.push(json!({ "type": "reseau", "centre": boxe.map(|b| b.title.clone()).unwrap_or_else(|| "Ta box".into()),
            "appareils": planetes, "legende": "Plus un appareil tourne près de la box, plus il répond vite. Orange : à vérifier." }));
    }
    let (etiquette, ton) = match e.audit {
        Some(a) => {
            let sc = entier_ps(a.get("Score"));
            let premier = match a.get("Score") {
                Some(Value::Array(v)) => chaine(v.first()),
                x => chaine(x),
            };
            (
                json!(format!("{premier} sur 100")),
                json!(if sc >= 85 {
                    "ok"
                } else if sc >= 60 {
                    "attention"
                } else {
                    "urgent"
                }),
            )
        }
        None => (Value::Null, Value::Null),
    };
    blocs.push(json!({ "type": "ligne", "icone": "bouclier", "couleur": "T", "texte": "Audit de sécurité du réseau", "note": texte_audit,
        "etiquette": etiquette, "ton": ton, "bouton": { "id": "clic:BtnNetAudit", "texte": bouton_audit, "style": "second" } }));
    if !a_voir.is_empty() {
        blocs.push(json!({ "type": "titre", "texte": "À vérifier" }));
    }
    for d in &a_voir {
        blocs.push(ligne(d));
    }
    if n > 0 {
        blocs.push(json!({ "type": "titre", "texte": "Appareils", "droite": e.resume }));
        let choix: Vec<Value> = FILTRES
            .iter()
            .filter_map(|(id, lbl)| {
                let k = liste.iter().filter(|d| passe_filtre(id, d)).count();
                (*id == "all" || k > 0).then(|| json!({ "id": id, "texte": format!("{lbl} ({k})") }))
            })
            .collect();
        if choix.len() > 1 {
            blocs.push(json!({ "type": "choix", "id": "net-filtre", "valeur": filtre, "options": choix }));
        }
        for d in &montres {
            blocs.push(ligne(d));
        }
    }
    blocs.push(json!({ "type": "reglages", "rangs": [{ "texte": "Me prévenir quand un nouvel appareil se connecte",
        "note": "Vérifié toutes les 10 minutes, tant que Nevermind est ouvert.", "bascule": { "id": "coche:ChkNetWatch", "oui": e.surveillance } }] }));
    if n > 0 && !e.conseil.is_empty() {
        blocs.push(json!({ "type": "note", "texte": e.conseil }));
    }
    let actions = if n > 0 {
        json!([{ "id": "clic:BtnNetScan", "texte": if e.scan { "Scan en cours..." } else { "Scanner de nouveau" }, "icone": "actu" }])
    } else {
        json!([])
    };
    let mut m = Map::new();
    m.insert(
        "reseau".into(),
        json!({ "titre": "Réseau", "sous": "Tous les appareils connectés chez toi : téléphones, consoles, TV, box, objets connectés.",
            "icone": "radar", "couleur": "S", "actions": actions, "module": module, "blocs": blocs }),
    );
    m
}

// ---------------------------------------------------------------------------
// Connexion
// ---------------------------------------------------------------------------

/// Une cible du test « Tester ma connexion » : son nom, son adresse, la box ou Internet
pub fn cibles(n: &Connexion) -> Vec<(String, String, bool)> {
    let mut t = Vec::new();
    if !n.gateway.is_empty() && n.gateway != "0.0.0.0" {
        t.push(("Ta box (réseau local)".to_string(), n.gateway.clone(), true));
    }
    t.push(("Internet : Cloudflare".into(), "1.1.1.1".into(), false));
    t.push(("Internet : Google".into(), "8.8.8.8".into(), false));
    t
}

/// Le résultat d'une cible (`$script:PingResults`)
#[derive(Clone, Debug, PartialEq)]
pub struct ResultatPing {
    pub label: String,
    pub moyenne: f64,
    pub gigue: f64,
    pub pertes: f64,
    pub statut: &'static str,
    pub verdict: &'static str,
}

impl ResultatPing {
    pub fn json(&self) -> Value {
        json!({ "Label": self.label, "Avg": self.moyenne, "Jitter": self.gigue, "Loss": self.pertes, "Status": self.statut, "Verdict": self.verdict })
    }
}

/// Moyenne, gigue et pertes de 20 pings (`Measure-Latency`) ; `None` = pas de réponse
pub fn mesure(temps: &[Option<f64>]) -> (f64, f64, f64) {
    let ok: Vec<f64> = temps.iter().flatten().copied().collect();
    let perdus = temps.len() - ok.len();
    let (mut moy, mut gig) = (0.0, 0.0);
    if !ok.is_empty() {
        moy = ok.iter().sum::<f64>() / ok.len() as f64;
        if ok.len() > 1 {
            let d: Vec<f64> = ok.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
            gig = d.iter().sum::<f64>() / d.len() as f64;
        }
    }
    let n = temps.len().max(1) as f64;
    (arrondi_dec(moy, 1), arrondi_dec(gig, 1), arrondi_dec(100.0 * perdus as f64 / n, 0))
}

/// Le verdict d'une cible (`Invoke-NetTest`)
pub fn verdict(label: &str, hote: &str, local: bool, m: (f64, f64, f64)) -> ResultatPing {
    let (moy, gig, pertes) = m;
    let (mut st, mut v) = if local {
        if pertes > 0.0 || moy > 5.0 || gig > 3.0 {
            (
                "warn",
                "Réseau local instable. C'est typique du Wi-Fi (murs, distance, voisins) : rapproche-toi de la box ou passe en câble.",
            )
        } else {
            ("ok", "Liaison avec ta box rapide et stable.")
        }
    } else if pertes >= 5.0 {
        ("bad", "Pertes de paquets : tu risques de la téléportation et des tirs qui ne comptent pas.")
    } else if pertes > 0.0 {
        ("warn", "Quelques pertes de paquets.")
    } else if gig > 15.0 {
        ("warn", "Ping instable (gigue élevée) : sensations de lag irrégulières.")
    } else if moy < 30.0 {
        ("ok", "Excellent pour jouer en ligne.")
    } else if moy < 60.0 {
        ("ok", "Bon pour jouer en ligne.")
    } else if moy < 100.0 {
        ("warn", "Moyen : jouable, mais tu seras désavantagé dans les jeux compétitifs.")
    } else {
        ("bad", "Ping élevé.")
    };
    if pertes == 100.0 {
        st = "bad";
        v = "Aucune réponse (ce serveur bloque peut-être le ping).";
    }
    ResultatPing { label: format!("{label} ({hote})"), moyenne: moy, gigue: gig, pertes, statut: st, verdict: v }
}

fn couleur_statut(s: &str) -> &'static str {
    match s {
        "ok" => "vert",
        "warn" => "orange",
        "bad" => "rouge",
        _ => "P",
    }
}
fn ton_statut(s: &str) -> &'static str {
    match s {
        "ok" => "ok",
        "warn" => "attention",
        "bad" => "urgent",
        _ => "",
    }
}

/// Les choix de DNS (`$DnsChoices`) : automatique, Cloudflare, Google, Quad9
pub const DNS_CHOIX: [&[&str]; 4] =
    [&[], &["1.1.1.1", "1.0.0.1"], &["8.8.8.8", "8.8.4.4"], &["9.9.9.9", "149.112.112.112"]];

/// Le dernier débit mesuré (`Get-DernierDebit`) : la liste `R` du test « Connexion Internet »
pub fn dernier_debit(r: Option<&[f64]>) -> Option<(String, String)> {
    let r = r?;
    if r.len() < 3 || r[1] < 0.0 || r[2] < 0.0 {
        return None;
    }
    Some((format_n0(r[1]), format_n0(r[2])))
}

/// La page Connexion
pub fn page_connexion(net: Option<&Connexion>, pings: &[ResultatPing], debit: Option<&[f64]>) -> Value {
    let inter = pings.iter().find(|p| p.label.starts_with("Internet"));
    let mut choix_dns = 0;
    if let Some(n) = net
        && !n.dns.is_empty()
    {
        for (k, c) in DNS_CHOIX.iter().enumerate().skip(1) {
            if n.dns.to_lowercase().contains(&c[0].to_lowercase()) {
                choix_dns = k;
            }
        }
    }
    let br = bridage(net);
    let mut infos = Vec::new();
    if let Some(n) = net {
        infos.push(json!({ "texte": "Type", "valeur": if n.wifi { "Wi-Fi" } else { "Câble (Ethernet)" } }));
        infos.push(json!({ "texte": "Carte", "valeur": n.desc }));
        if br.is_none() {
            infos.push(json!({ "texte": "Vitesse", "valeur": n.speed }));
        }
        infos.push(json!({ "texte": "Box", "valeur": n.gateway }));
        if !n.dns.is_empty() {
            infos.push(json!({ "texte": "DNS actuel", "valeur": n.dns }));
        }
    }
    let module = if let Some(p) = inter {
        json!({ "icone": "ondes", "couleur": couleur_statut(p.statut), "titre": format!("Ping {} ms", nombre(p.moyenne)), "texte": p.verdict,
            "boutons": [{ "id": "clic:BtnPing", "texte": "Tester ma connexion" }],
            "resume": [{ "nom": "Gigue", "valeur": format!("{} ms", nombre(p.gigue)) },
                       { "nom": "Pertes", "valeur": format!("{} %", nombre(p.pertes)), "ton": if p.pertes > 0.0 { "attention" } else { "ok" } }] })
    } else if net.is_none() {
        json!({ "icone": "ondes", "couleur": "rouge", "titre": "Pas de connexion", "texte": "Aucune connexion Internet détectée.",
            "boutons": [{ "id": "clic:BtnPing", "texte": "Réessayer" }] })
    } else {
        json!({ "icone": "ondes", "couleur": "P", "titre": "Pas encore testée", "texte": "Le test envoie quelques petits messages à ta box et à Internet pendant une dizaine de secondes.",
            "boutons": [{ "id": "clic:BtnPing", "texte": "Tester ma connexion" }] })
    };
    let mut blocs = Vec::new();
    if !pings.is_empty() {
        blocs.push(json!({ "type": "titre", "texte": "Dernier test" }));
    }
    for p in pings {
        blocs.push(json!({ "type": "ligne", "icone": if p.label.starts_with("Ta box") { "box" } else { "internet" }, "couleur": couleur_statut(p.statut),
            "texte": p.label, "note": p.verdict, "valeur": format!("{} ms", nombre(p.moyenne)),
            "etiquette": format!("gigue {} ms · pertes {} %", nombre(p.gigue), nombre(p.pertes)), "ton": ton_statut(p.statut) }));
    }
    blocs.push(json!({ "type": "titre", "texte": "Débit" }));
    let d = dernier_debit(debit);
    match &d {
        Some((down, up)) => blocs.push(json!({ "type": "chiffres", "elements": [
            { "titre": "Téléchargement", "valeur": down, "unite": "Mb/s", "ton": "ok" },
            { "titre": "Envoi", "valeur": up, "unite": "Mb/s", "ton": "ok" }] })),
        None => blocs.push(json!({ "type": "texte", "texte": "La vitesse de ta connexion en téléchargement et en envoi, mesurée en une vingtaine de secondes." })),
    }
    blocs.push(json!({ "type": "bouton", "id": "debit", "texte": if d.is_some() { "Mesurer à nouveau" } else { "Mesurer mon débit" }, "style": "second" }));
    if !infos.is_empty() {
        blocs.push(json!({ "type": "titre", "texte": "Ta connexion" }));
    }
    if let Some(b) = &br {
        blocs.push(json!({ "type": "ligne", "icone": "alerte", "couleur": "orange", "texte": "Vitesse",
            "note": format!("{} Redémarrer la carte la remet en général à pleine vitesse (Internet coupé 5 à 10 secondes).", texte_bridage(b)),
            "valeur": format_vitesse(b.bps), "etiquette": "Trop lente", "ton": "attention" }));
        blocs.push(json!({ "type": "bouton", "id": "carte-relancer", "texte": "Redémarrer la carte réseau" }));
    }
    if !infos.is_empty() {
        blocs.push(json!({ "type": "reglages", "rangs": infos }));
    }
    blocs.push(json!({ "type": "titre", "texte": "Serveur DNS" }));
    blocs.push(json!({ "type": "choix", "id": "dns", "valeur": choix_dns.to_string(), "options": [
        { "id": "0", "texte": "Automatique (celui de ta box)" }, { "id": "1", "texte": "Cloudflare" }, { "id": "2", "texte": "Google" }, { "id": "3", "texte": "Quad9" }] }));
    blocs.push(json!({ "type": "bouton", "id": "clic:BtnDnsFlush", "texte": "Vider le cache DNS", "style": "second" }));
    blocs.push(json!({ "type": "note", "texte": "Bon à savoir : le DNS ne change pas ton ping en jeu, il sert seulement à trouver l'adresse des serveurs. Un DNS rapide accélère un peu la navigation et la connexion des lanceurs de jeux (Steam, Epic...)." }));
    json!({ "titre": "Connexion", "parent": "perf", "sous": "Mesure ton temps de réponse (ping), sa stabilité (gigue), les messages perdus en route et ton débit.",
        "icone": "ondes", "couleur": "P", "module": module, "blocs": blocs })
}
