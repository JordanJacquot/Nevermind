//! Le panneau de détail du bloc, comme l'écran le reçoit (`pages.panneau`) : la fiche d'un appareil
//! (Show-DeviceDetail, Update-DevPing de reseau.ps1) et l'audit (Invoke-NetAudit, Show-NetAuditResult).
//!
//! En PowerShell, ces panneaux sont dessinés en WPF puis traduits pour l'écran élément par élément
//! (Add-BlocsWpf d'ecran-panneau.ps1). Ici, ils sont écrits directement avec le résultat de cette traduction : les
//! titres de section gardent leur « ●  », une ligne nom/valeur devient une « ligne », un encadré de verdict une ligne
//! colorée, la jauge un chiffre, la courbe une courbe (voir DIVERGENCES.md : ces panneaux ne sont pas jugés).

use crate::appareil::Appareil;
use crate::audit::{CONSEILS, Point, libelle, rubrique};
use crate::texte::{egal, format_n};
use serde_json::{Value, json};

/// Les ports vérifiés sur un appareil et ce qu'ils veulent dire (`$PortInfo`) : nom, niveau, explication, adresse
pub const PORTS: &[(i64, &str, &str, &str, &str)] = &[
    (
        21,
        "Transfert de fichiers (FTP)",
        "warn",
        "Les fichiers et les mots de passe passent en clair sur le réseau.",
        "",
    ),
    (
        22,
        "Accès à distance sécurisé (SSH)",
        "info",
        "Permet de prendre la main sur l'appareil à distance, de façon chiffrée.",
        "",
    ),
    (
        23,
        "Accès à distance non protégé (Telnet)",
        "bad",
        "Accès à distance sans aucun chiffrement : à désactiver dans les réglages de l'appareil.",
        "",
    ),
    (25, "Envoi de mails (SMTP)", "info", "Serveur d'envoi de mails.", ""),
    (53, "Serveur DNS", "info", "Traduit les noms de sites en adresses. Normal pour une box ou un routeur.", ""),
    (80, "Page web", "info", "Page de réglages accessible depuis un navigateur.", "http://{0}"),
    (110, "Réception de mails (POP3)", "info", "Serveur de mails.", ""),
    (135, "Services Windows", "info", "Communication interne de Windows. Normal sur un PC Windows.", ""),
    (
        139,
        "Partage de fichiers (ancien)",
        "warn",
        "Ancienne version du partage de fichiers Windows. Normal sur un PC, mais à éviter ailleurs.",
        "",
    ),
    (143, "Réception de mails (IMAP)", "info", "Serveur de mails.", ""),
    (443, "Page web sécurisée", "info", "Page de réglages chiffrée, accessible depuis un navigateur.", "https://{0}"),
    (
        445,
        "Partage de fichiers Windows",
        "info",
        "Dossiers ou imprimantes partagés. Vérifie que tu partages seulement ce que tu veux.",
        "",
    ),
    (515, "Impression", "info", "Service d'impression réseau.", ""),
    (548, "Partage de fichiers Apple", "info", "Partage de fichiers d'un Mac ou d'un NAS.", ""),
    (554, "Flux vidéo", "info", "Souvent une caméra de surveillance ou un décodeur TV.", ""),
    (631, "Impression", "info", "Service d'impression réseau.", ""),
    (1883, "Objets connectés (MQTT)", "info", "Messagerie utilisée par la domotique.", ""),
    (
        3389,
        "Bureau à distance Windows",
        "warn",
        "Permet de prendre le contrôle du PC à distance. Désactive-le si tu ne t'en sers pas.",
        "",
    ),
    (5000, "Interface web (NAS, AirPlay)", "info", "Page de réglages ou service de diffusion.", "http://{0}:5000"),
    (5001, "Interface web sécurisée (NAS)", "info", "Page de réglages chiffrée.", "https://{0}:5001"),
    (5357, "Découverte réseau Windows", "info", "Permet aux autres appareils de voir ce PC sur le réseau.", ""),
    (5900, "Contrôle à distance (VNC)", "warn", "Prise de contrôle de l'écran à distance, souvent mal protégée.", ""),
    (7000, "AirPlay", "info", "Diffusion depuis un iPhone, un iPad ou un Mac.", ""),
    (8008, "Google Cast", "info", "Diffusion depuis un téléphone (Chromecast).", ""),
    (8009, "Google Cast", "info", "Diffusion depuis un téléphone (Chromecast).", ""),
    (8080, "Page web (autre port)", "info", "Page de réglages accessible depuis un navigateur.", "http://{0}:8080"),
    (8123, "Home Assistant", "info", "Interface de domotique.", "http://{0}:8123"),
    (8443, "Page web sécurisée (autre port)", "info", "Page de réglages chiffrée.", "https://{0}:8443"),
    (9100, "Imprimante réseau", "info", "Impression directe sur l'imprimante.", ""),
    (9295, "PlayStation Remote Play", "info", "Jouer à distance sur la console.", ""),
    (32400, "Serveur Plex", "info", "Serveur de films et séries.", "http://{0}:32400/web"),
    (
        62078,
        "Synchronisation iPhone ou iPad",
        "info",
        "Synchronisation avec un ordinateur. Normal sur un appareil Apple.",
        "",
    ),
];

/// L'étiquette du panneau d'un appareil (`$DevTags`) : seule « NET » a son icône (courbe), les autres « test »
fn icone_appareil(kind: &str) -> &'static str {
    let tag = match kind {
        "Ce PC" | "Ordinateur" => "PC",
        "Box Internet" | "Box ou décodeur TV" => "BOX",
        "Routeur ou répéteur Wi-Fi" => "WIFI",
        "Téléphone ou tablette" | "Téléphone probable" => "TEL",
        "Console de jeu" => "JEU",
        "TV ou multimédia" => "TV",
        "Imprimante" => "IMP",
        "Objet connecté" => "IOT",
        "Appareil Apple" => "APP",
        "Caméra" => "CAM",
        "Enceinte ou audio" => "AUD",
        _ => "NET",
    };
    if tag == "NET" { "courbe" } else { "test" }
}

fn titre_section(t: &str) -> Value {
    json!({ "type": "titre", "texte": format!("●  {}", t.to_uppercase()) })
}

/// Une ligne de tableau (nom, valeur) traduite : une valeur vide ne laisse que le nom
fn rang(nom: &str, valeur: &str) -> Value {
    let v = valeur.trim();
    if v.is_empty() {
        json!({ "type": "texte", "texte": nom })
    } else {
        json!({ "type": "ligne", "texte": nom, "valeur": v })
    }
}

/// Un texte en gras de couleur (verdict) : ligne verte ou orange ou rouge, sinon un titre (court) ou un texte
fn gras(t: &str, ton: &str) -> Value {
    match ton {
        "ok" => json!({ "type": "ligne", "icone": "ok", "couleur": "vert", "texte": t }),
        "attention" => json!({ "type": "ligne", "icone": "alerte", "couleur": "orange", "texte": t }),
        "urgent" => json!({ "type": "ligne", "icone": "alerte", "couleur": "rouge", "texte": t }),
        _ if t.chars().count() <= 80 => json!({ "type": "titre", "texte": t }),
        _ => json!({ "type": "texte", "texte": t }),
    }
}

fn ton_niveau(n: &str) -> &'static str {
    match n {
        "ok" => "ok",
        "warn" => "attention",
        "bad" => "urgent",
        _ => "",
    }
}

/// Le ping en direct d'un appareil (`$script:DevPing`)
#[derive(Clone, Debug, Default)]
pub struct PingDirect {
    pub temps: Vec<f64>,
    pub courbe: Vec<f64>,
    pub envoyes: u32,
    pub perdus: u32,
    pub dernier: f64,
}

impl PingDirect {
    /// Une réponse (ou son absence) : Update-DevPing
    pub fn ajouter(&mut self, rtt: Option<f64>) {
        self.envoyes += 1;
        match rtt {
            Some(r) => {
                self.temps.push(r);
                if self.temps.len() > 60 {
                    self.temps.remove(0);
                }
                self.dernier = r;
                self.courbe.push(r);
            }
            None => {
                self.perdus += 1;
                self.courbe.push(0.0);
            }
        }
        if self.courbe.len() > 160 {
            self.courbe.remove(0);
        }
    }
}

/// Les services ouverts : en cours de recherche, ou trouvés
#[derive(Clone, Debug, PartialEq)]
pub enum Services {
    Recherche,
    Trouves(Vec<i64>),
}

/// Ce que le panneau garde du dernier état de la fiche : les boutons, dans l'ordre (`panneau:b:N`)
#[derive(Clone, Debug, PartialEq)]
pub enum BoutonPanneau {
    Ouvrir(String),
    ActionAudit(usize, usize),
    Exporter,
}

/// La fiche d'un appareil
pub fn fiche_appareil(
    d: &Appareil,
    premiere: Option<&str>,
    ping: &PingDirect,
    svc: &Services,
    boutons: &mut Vec<BoutonPanneau>,
) -> Value {
    let mut b = vec![titre_section("Identité")];
    let vu = if d.soi {
        "Ce PC".to_string()
    } else if let Some(p) = premiere.filter(|p| !p.is_empty()) {
        let x: Vec<&str> = p.split('-').collect();
        if x.len() == 3 { format!("{}/{}/{}", x[2], x[1], x[0]) } else { p.to_string() }
    } else if !d.mac.is_empty() {
        "Avant la mise à jour 1.0.8".into()
    } else {
        "Inconnue".into()
    };
    let mut rangs: Vec<(String, String)> = vec![
        ("Adresse sur le réseau".into(), d.ip.clone()),
        ("Adresse physique (MAC)".into(), if d.mac.is_empty() { "Inconnue".into() } else { d.mac.clone() }),
        ("Fabricant".into(), if d.vendor.is_empty() { "Inconnu".into() } else { d.vendor.clone() }),
        ("Type".into(), d.kind.clone()),
    ];
    let mut r = |n: &str, v: String| rangs.push((n.into(), v));
    if !d.host.is_empty() {
        r("Nom sur le réseau", d.host.clone());
    }
    if !d.adapter.is_empty() {
        r("Carte réseau", d.adapter.clone());
    }
    if !d.announced.is_empty() && !egal(&d.announced, &d.host) {
        r("Nom annoncé par l'appareil", d.announced.clone());
    }
    if !d.model.is_empty() {
        r("Modèle", format!("{}{}", if d.maker.is_empty() { String::new() } else { format!("{} ", d.maker) }, d.model));
    }
    if !d.nb_name.is_empty() {
        r(
            "Nom Windows",
            format!(
                "{}{}",
                d.nb_name,
                if d.nb_group.is_empty() { String::new() } else { format!(" (groupe {})", d.nb_group) }
            ),
        );
    }
    if !d.os_guess.is_empty() {
        r("Système probable", d.os_guess.clone());
    }
    if !d.service_labels.is_empty() {
        r("Ce qu'il propose", d.service_labels.join(", "));
    }
    if !d.web_title.is_empty() {
        r("Sa page de réglages", d.web_title.clone());
    }
    if !d.ipv6.is_empty() {
        r("Adresse IPv6", d.ipv6.iter().take(2).cloned().collect::<Vec<_>>().join(", "));
    }
    if !d.found_by.is_empty() {
        r("Trouvé grâce à", d.found_by.join(", "));
    }
    if d.hidden {
        r(
            "Appareil discret",
            "Il ne répond pas au ping. C'est normal pour beaucoup de téléphones et de PC protégés par un pare-feu."
                .into(),
        );
    }
    if d.camera {
        r(
            "Caméra possible",
            format!("Indices : {}. Vérifie que tu sais à qui elle est et où elle filme.", d.camera_why.join(", ")),
        );
    }
    r("Vu pour la première fois", vu);
    if d.new {
        r("Statut", "Nouvel appareil depuis le dernier scan".into());
    }
    if egal(&d.vendor, "Adresse privée") {
        r(
            "Bon à savoir",
            "Les téléphones récents cachent leur vraie adresse physique : le fabricant ne peut pas être connu.".into(),
        );
    }
    b.extend(rangs.iter().map(|(n, v)| rang(n, v)));

    b.push(titre_section("Temps de réponse en direct"));
    b.push(json!({ "type": "chiffres", "elements": [{ "titre": "Temps de réponse", "valeur": format_n(ping.dernier, 0), "unite": "ms", "ton": "" }] }));
    let t = &ping.temps;
    let perte = 100.0 * f64::from(ping.perdus) / f64::from(ping.envoyes.max(1));
    let (mut moy, mut min, mut max, mut gig) =
        ("...".to_string(), "...".to_string(), "...".to_string(), "...".to_string());
    if !t.is_empty() {
        let a = t.iter().sum::<f64>() / t.len() as f64;
        let mut j = 0.0;
        for i in 1..t.len() {
            j += (t[i] - t[i - 1]).abs();
        }
        if t.len() > 1 {
            j /= (t.len() - 1) as f64;
        }
        moy = if a < 1.0 { "< 1 ms".into() } else { format!("{} ms", format_n(a, 0)) };
        min = format!("{} ms", format_n(t.iter().copied().fold(f64::MAX, f64::min), 0));
        max = format!("{} ms", format_n(t.iter().copied().fold(f64::MIN, f64::max), 0));
        gig = format!("{} ms", format_n(j, 1));
    }
    let pertes = if ping.envoyes > 0 { format!("{} %", format_n(perte, 0)) } else { "...".into() };
    for (n, v) in [
        ("Moyenne", moy),
        ("Plus rapide", min),
        ("Plus lent", max),
        ("Variation (gigue)", gig),
        ("Réponses perdues", pertes),
    ] {
        b.push(rang(n, &v));
    }
    let gigue =
        if t.len() > 1 { t.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (t.len() - 1) as f64 } else { 0.0 };
    let verdict = if ping.envoyes < 3 {
        gras("Mesure en cours...", "")
    } else if t.is_empty() {
        gras(
            "Cet appareil ne répond pas au ping. C'est normal pour certains téléphones, consoles ou PC qui le bloquent.",
            "",
        )
    } else if perte >= 5.0 || gigue > 15.0 {
        gras(
            "Connexion instable : des réponses se perdent ou arrivent en retard. Souvent le signe d'un Wi-Fi faible.",
            "attention",
        )
    } else {
        gras("Connexion stable.", "ok")
    };
    b.push(verdict);
    if ping.courbe.len() >= 2 {
        b.push(json!({ "type": "courbe", "points": ping.courbe.iter().map(|x| x.round()).collect::<Vec<_>>(), "unite": "ms", "titre": "" }));
    }

    b.push(titre_section("Services ouverts"));
    match svc {
        Services::Recherche => {
            b.push(json!({ "type": "texte", "texte": "Recherche des services proposés par cet appareil..." }))
        }
        Services::Trouves(ouverts) => {
            if ouverts.is_empty() {
                b.push(json!({ "type": "texte", "texte": "Aucun service ouvert parmi les plus courants. C'est normal pour un téléphone, une console ou une TV : ils n'acceptent pas de connexions." }));
            }
            let mut pire = "ok";
            let mut notes = Vec::new();
            for port in ouverts {
                let Some((_, nom, niveau, expl, url)) = PORTS.iter().find(|p| p.0 == *port) else { continue };
                if *niveau == "bad" {
                    pire = "bad";
                    notes.push(*nom);
                } else if *niveau == "warn" && pire != "bad" {
                    pire = "warn";
                    notes.push(*nom);
                }
                let ton = ton_niveau(niveau);
                let num = port.to_string();
                if url.is_empty() {
                    // (encadré de texte seul : le premier texte de plus de 3 lettres, en note)
                    if num.chars().count() > 3 {
                        b.push(match ton {
                            "" => json!({ "type": "note", "texte": num }),
                            t => gras(&num, t),
                        });
                    } else {
                        b.push(json!({ "type": "note", "texte": nom }));
                    }
                } else {
                    b.push(gras(&num, ton));
                    b.push(json!({ "type": "titre", "texte": nom }));
                    b.push(json!({ "type": "texte", "texte": expl }));
                    boutons.push(BoutonPanneau::Ouvrir(url.replace("{0}", &d.ip)));
                    b.push(json!({ "type": "bouton", "id": format!("panneau:b:{}", boutons.len() - 1), "texte": "Ouvrir", "style": "second" }));
                }
            }
            let texte = match pire {
                "bad" => format!(
                    "À corriger : {}. Désactive ce service dans les réglages de l'appareil, ou demande à la personne qui l'a installé.",
                    notes.join(", ")
                ),
                "warn" => format!(
                    "À surveiller : {}. Si tu ne sais pas pourquoi c'est ouvert, désactive-le dans les réglages de l'appareil.",
                    notes.join(", ")
                ),
                _ => {
                    if ouverts.is_empty() {
                        "Rien à signaler.".into()
                    } else {
                        "Rien d'inhabituel pour ce type d'appareil.".into()
                    }
                }
            };
            b.push(gras(&texte, ton_niveau(pire)));
            b.push(json!({ "type": "texte", "texte": format!("Seuls les {} services les plus courants sont vérifiés.", PORTS.len()) }));
        }
    }
    json!({ "titre": d.title, "sous": d.kind, "icone": icone_appareil(&d.kind), "couleur": "P", "retour": "appli", "parent": "reseau",
        "module": { "icone": "ok", "couleur": "vert", "titre": "Ping en direct", "texte": d.kind,
            "boutons": [{ "id": "panneau:close", "texte": "Fermer", "style": "second" }] },
        "blocs": b })
}

/// L'audit pendant qu'il tourne
pub fn audit_en_cours(nombre: usize, progression: f64) -> Value {
    let p = progression.clamp(0.0, 100.0);
    json!({ "titre": "Audit de sécurité du réseau", "sous": "Wi-Fi, box, DNS, appareils et ce PC", "icone": "bouclier", "couleur": "P", "retour": "appli", "parent": "reseau",
        "module": { "icone": "bouclier", "couleur": "P", "titre": "Audit en cours", "texte": "Wi-Fi, box, DNS, appareils et ce PC", "boutons": [] },
        "blocs": [
            { "type": "ligne", "icone": "radar", "couleur": "S", "texte": "Audit en cours", "barre": crate::texte::arrondi_pair(p), "valeur": format!("{} %", format_n(p, 0)) },
            { "type": "texte", "texte": format!("Vérification de ton Wi-Fi, de ta box et de {nombre} appareils. Ça prend une vingtaine de secondes, rien n'est modifié pendant l'audit.") }
        ] })
}

/// L'audit qui n'a pas pu aller au bout
pub fn audit_echec(erreur: &str) -> Value {
    json!({ "titre": "Audit de sécurité du réseau", "sous": "Wi-Fi, box, DNS, appareils et ce PC", "icone": "bouclier", "couleur": "P", "retour": "appli", "parent": "reseau",
        "module": { "icone": "alerte", "couleur": "rouge", "titre": "Échec", "texte": "Wi-Fi, box, DNS, appareils et ce PC",
            "boutons": [{ "id": "panneau:again", "texte": "Refaire le test" }, { "id": "panneau:close", "texte": "Fermer", "style": "second" }] },
        "blocs": [gras(&format!("L'audit n'a pas pu aller au bout : {}", if erreur.is_empty() { "erreur inconnue" } else { erreur }), "urgent")] })
}

/// Le résultat de l'audit (`Show-NetAuditResult`)
pub fn audit_resultat(
    points: &[Point],
    score: i64,
    date: &str,
    nombre: usize,
    boutons: &mut Vec<BoutonPanneau>,
) -> Value {
    let bad = points.iter().filter(|c| c.statut == "bad").count();
    let warn = points.iter().filter(|c| c.statut == "warn").count();
    let ok = points.iter().filter(|c| c.statut == "ok").count();
    let ton_note = if score >= 85 {
        "ok"
    } else if score >= 60 {
        "attention"
    } else {
        "urgent"
    };
    let mut b = vec![
        titre_section("Résultat"),
        json!({ "type": "chiffres", "elements": [{ "titre": "Sécurité du réseau", "valeur": format_n(score as f64, 0), "unite": "sur 100", "ton": ton_note }] }),
        gras(libelle(score), ton_note),
        json!({ "type": "texte", "texte": format!("Audit du {date}, {nombre} appareils vérifiés.") }),
    ];
    for (n, t, st) in [(bad, "à corriger", "bad"), (warn, "à surveiller", "warn"), (ok, "OK", "ok")] {
        if n == 0 && st != "ok" {
            continue;
        }
        b.push(gras(&format!("{n} {t}"), ton_niveau(st)));
    }
    boutons.push(BoutonPanneau::Exporter);
    b.push(json!({ "type": "bouton", "id": format!("panneau:b:{}", boutons.len() - 1), "texte": "Exporter le rapport", "style": "second" }));
    for (st, titre) in [("bad", "À corriger"), ("warn", "À surveiller"), ("info", "Bon à savoir")] {
        let items: Vec<(usize, &Point)> = points.iter().enumerate().filter(|(_, c)| c.statut == st).collect();
        if items.is_empty() {
            continue;
        }
        b.push(titre_section(titre));
        for (k, c) in items {
            b.push(json!({ "type": "ligne", "icone": if st == "info" { "aide" } else { "alerte" },
                "couleur": match st { "bad" => "rouge", "warn" => "orange", _ => "P" }, "texte": c.titre, "note": c.detail }));
            if !c.elements.is_empty() {
                let mut el: Vec<String> = c.elements.iter().take(5).cloned().collect();
                if c.elements.len() > 5 {
                    el.push(format!("et {} autre(s)...", c.elements.len() - 5));
                }
                b.push(json!({ "type": "liste", "elements": el }));
            }
            for (j, a) in c.actions.iter().enumerate() {
                boutons.push(BoutonPanneau::ActionAudit(k, j));
                b.push(json!({ "type": "bouton", "id": format!("panneau:b:{}", boutons.len() - 1), "texte": a.label,
                    "style": if j == 0 { Value::Null } else { json!("second") } }));
            }
        }
    }
    let oks: Vec<&Point> = points.iter().filter(|c| c.statut == "ok").collect();
    if !oks.is_empty() {
        b.push(titre_section("Tout va bien"));
        for c in oks {
            let mut l = json!({ "type": "ligne", "texte": format!("{}   {}", rubrique(&c.cat), c.titre), "etiquette": "Bon", "ton": "ok" });
            let mut reste: Vec<String> = vec![c.detail.clone()];
            reste.extend(c.elements.iter().take(6).map(|i| format!("•  {i}")));
            let valeur = reste.pop().unwrap_or_default();
            l["valeur"] = json!(valeur);
            if !reste.is_empty() {
                l["note"] = json!(reste.join(" · "));
            }
            b.push(l);
        }
    }
    b.push(titre_section("Bons réflexes"));
    for t in CONSEILS {
        b.push(json!({ "type": "texte", "texte": format!("•  {t}") }));
    }
    let (icone, couleur, titre) = if bad > 0 {
        ("alerte", "rouge", "Points à corriger")
    } else if warn > 0 {
        ("alerte", "orange", "À surveiller")
    } else {
        ("ok", "vert", "Tout va bien")
    };
    json!({ "titre": "Audit de sécurité du réseau", "sous": "Wi-Fi, box, DNS, appareils et ce PC", "icone": "bouclier", "couleur": "P", "retour": "appli", "parent": "reseau",
        "module": { "icone": icone, "couleur": couleur, "titre": titre, "texte": "Wi-Fi, box, DNS, appareils et ce PC",
            "boutons": [{ "id": "panneau:again", "texte": "Refaire le test" }, { "id": "panneau:close", "texte": "Fermer", "style": "second" }] },
        "blocs": b })
}
