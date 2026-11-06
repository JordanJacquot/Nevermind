//! Les pages de l'écran (kit) : l'onglet « Lag en ligne » (ecran-jeux.ps1, à placer par le bloc Jeux dans
//! Perf. In Game), la page « trafic » (ecran-jeux.ps1), et le panneau de détail d'une mesure du lag, d'un programme
//! du trafic et de « Ce que Windows envoie à Microsoft » (Show-LagSession, Show-TrafficApp, Show-WindowsPrivacy,
//! tels que ecran-panneau.ps1 les traduit pour l'écran).

use crate::format::{duree_jeu, entier, ms, nombre_n, octets, ps_nombre};
use crate::lag::diag::{connexion_seule, etat_etape, lieu_serveur, ligne_mesure};
use crate::microsoft;
use crate::serveurs::Annuaire;
use crate::trafic::alertes::{Alerte, libelle_signature};
use crate::trafic::etat::{App, Releve, nom_port, port_suspect};
use crate::trafic::types::{self, Sorte};
use crate::valeur::{champ, liste, nombre, num, texte, vrai};
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use serde_json::{Value, json};

/// Ce que ConvertTo-Json écrit pour `$(if ($faux) { ... })` dans une table : `{}` (le kit le prend comme une
/// valeur absente). Gardé tel quel pour que les pages soient les mêmes au champ près.
pub fn vide() -> Value {
    json!({})
}

/// `$TonsStatut`
pub fn ton_statut(n: &str) -> Value {
    match n {
        "ok" => json!("ok"),
        "warn" => json!("attention"),
        "bad" => json!("urgent"),
        "info" => json!(""),
        _ => Value::Null,
    }
}

/// `[datetime]$x` : la date d'une mesure en heure locale
pub fn date_locale(s: &str) -> Option<DateTime<Local>> {
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(d.with_timezone(&Local));
    }
    for f in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(s, f) {
            return Local.from_local_datetime(&n).earliest();
        }
    }
    None
}

/// Les mesures du lag, de la plus récente à la plus ancienne
pub fn mesures_recentes(sessions: &[Value], max: usize) -> Vec<&Value> {
    let mut l: Vec<(&Value, i64)> = sessions
        .iter()
        .map(|s| (s, date_locale(&texte(champ(s, "Date"))).map(|d| d.timestamp_micros()).unwrap_or(i64::MIN)))
        .collect();
    l.sort_by_key(|x| std::cmp::Reverse(x.1));
    l.into_iter().take(max).map(|x| x.0).collect()
}

fn etiquette_niveau(n: &str) -> &'static str {
    match n {
        "ok" => "Stable",
        "warn" => "À voir",
        "bad" => "Problème",
        _ => "",
    }
}

/// L'onglet « Lag en ligne » de Perf. In Game : { module, blocs } (`$modLag`, `$blocsLag`)
pub fn onglet_lag(sessions: &[Value], mesure_active: bool, en_cours: bool) -> Value {
    let lags = mesures_recentes(sessions, 15);
    let mut blocs = vec![
        json!({ "type": "reglages", "rangs": [{ "texte": "Mesurer ma connexion quand je joue", "note": "Pour tes jeux en ligne : Nevermind mesure ta box, Internet et le serveur du jeu pendant la partie, puis t'explique d'où vient le lag.", "bascule": { "id": "lag-mesure", "oui": mesure_active } }] }),
    ];
    if !en_cours {
        blocs.push(json!({ "type": "bouton", "id": "lag-test", "texte": "Tester ma connexion maintenant (30 s)" }));
    }
    if lags.is_empty() {
        blocs.push(json!({ "type": "texte", "texte": "Aucune mesure pour l'instant. Joue une partie en ligne (au moins une minute) ou lance le test rapide : le résultat apparaîtra ici." }));
    } else {
        blocs.push(json!({ "type": "titre", "texte": "Dernières mesures", "droite": "Clique pour le détail" }));
    }
    for s in &lags {
        let d = date_locale(&texte(champ(s, "Date")));
        let niv = texte(champ(champ(s, "Diag"), "Level"));
        blocs.push(json!({
            "type": "ligne", "id": format!("lag:{}", texte(champ(s, "Id"))),
            "quand": [d.map(|d| d.format("%d/%m").to_string()).unwrap_or_default(), d.map(|d| d.format("%H:%M").to_string()).unwrap_or_default()],
            "texte": format!("{} : {}", texte(champ(s, "Game")), texte(champ(champ(s, "Diag"), "Title"))),
            "note": format!("{}. {}", duree_jeu(num(champ(s, "Seconds"))), ligne_mesure(s)),
            "etiquette": etiquette_niveau(&niv), "ton": ton_statut(&niv),
        }));
    }
    let module = if let Some(s0) = lags.first() {
        let d0 = date_locale(&texte(champ(s0, "Date")));
        let niv = texte(champ(champ(s0, "Diag"), "Level"));
        let ligne = ligne_mesure(s0);
        let ton = ton_statut(&niv);
        json!({
            "icone": "internet",
            "couleur": match niv.as_str() { "ok" => "vert", "warn" => "orange", "bad" => "rouge", _ => "T" },
            "titre": texte(champ(champ(s0, "Diag"), "Title")),
            "texte": format!("Dernière mesure : {}, le {} à {}.{}", texte(champ(s0, "Game")), d0.map(|d| d.format("%d/%m").to_string()).unwrap_or_default(), d0.map(|d| d.format("%H:%M").to_string()).unwrap_or_default(), if ligne.is_empty() { String::new() } else { format!(" {ligne}.") }),
            "boutons": [{ "id": format!("lag:{}", texte(champ(s0, "Id"))), "texte": "Voir le détail" }],
            "resume": [
                { "nom": "État", "valeur": match niv.as_str() { "ok" => "Stable", "warn" => "À voir", "bad" => "Problème", _ => "-" }, "ton": if vrai(&ton) { ton } else { json!("") } },
                { "nom": "Durée", "valeur": duree_jeu(num(champ(s0, "Seconds"))) },
            ],
        })
    } else {
        json!({ "icone": "internet", "couleur": "doux", "titre": "Aucune mesure", "texte": "Joue une partie en ligne (au moins une minute) ou lance le test rapide : Nevermind te dira d'où vient le lag." })
    };
    json!({ "module": module, "blocs": blocs })
}

/// Le texte du bouton de la surveillance (`$ui.BtnTraffic.Content`)
pub fn bouton_surveillance(demarree: bool, active: bool) -> &'static str {
    if active {
        "Arrêter la surveillance"
    } else if demarree {
        "Reprendre la surveillance"
    } else {
        "Démarrer la surveillance"
    }
}

/// Les programmes montrés (au plus 40, du plus gros échange au plus petit)
pub fn apps_montrees(r: &Releve) -> Vec<&App> {
    let mut l: Vec<&App> = r.apps.values().filter(|a| a.nb_destinations() > 0 || a.out > 0.0).collect();
    l.sort_by(|a, b| (b.out + b.entrant).total_cmp(&(a.out + a.entrant)));
    l.truncate(40);
    l
}

/// Les actions d'une alerte (`Get-TrafficAlertActions`)
pub fn actions_alerte(a: &App) -> Vec<&'static str> {
    let mut l = vec!["C'est normal, je lui fais confiance"];
    if !a.chemin.is_empty() {
        l.extend(["Analyser avec l'antivirus", "Bloquer l'accès à Internet", "Ouvrir l'emplacement"]);
    }
    l
}

/// « depuis 3 min »
fn depuis(mins: i64) -> String {
    if mins < 1 { "moins d'une minute".into() } else { format!("{mins} min") }
}

/// La page Trafic (`$pages.trafic`). `releve` : `None` tant que la surveillance n'a jamais été démarrée.
pub fn page_trafic(
    releve: Option<&Releve>,
    active: bool,
    alertes: &[Alerte],
    annuaire_permis: bool,
    maintenant: f64,
) -> Value {
    let apps: Vec<&App> = releve.map(apps_montrees).unwrap_or_default();
    let alertes: Vec<&Alerte> = alertes.iter().filter(|a| a.niveau != "info").collect();
    let (mut out, mut inn) = (0.0, 0.0);
    for x in &apps {
        out += x.out;
        inn += x.entrant;
    }
    let max_o = apps.iter().map(|a| a.out).fold(1.0f64, f64::max).max(1.0);
    let mins = releve.map(|r| entier((maintenant - r.debut) / 60.0)).unwrap_or(0);
    let mut blocs: Vec<Value> = Vec::new();
    if active || !apps.is_empty() {
        blocs.push(json!({ "type": "chiffres", "elements": [
            { "titre": "Envoyé", "valeur": octets(out) }, { "titre": "Reçu", "valeur": octets(inn) },
            { "titre": "Programmes", "valeur": apps.len().to_string() },
            { "titre": "À vérifier", "valeur": alertes.len().to_string(), "ton": if alertes.is_empty() { "ok" } else { "attention" } },
        ] }));
    }
    if !alertes.is_empty() {
        blocs.push(json!({ "type": "titre", "texte": "À vérifier" }));
    }
    for (k, al) in alertes.iter().enumerate() {
        let Some(a) = releve.and_then(|r| r.apps.get(&al.cle)) else { continue };
        let bad = al.niveau == "bad";
        blocs.push(json!({ "type": "ligne", "icone": "alerte", "couleur": if bad { "rouge" } else { "orange" },
            "texte": format!("{} : {}", a.titre, if bad { "comportement suspect" } else { "à vérifier" }),
            "note": format!("Pourquoi : {}.", al.pourquoi.join(" ; ")), "etiquette": if bad { "Important" } else { "À voir" }, "ton": ton_statut(al.niveau) }));
        for (j, l) in actions_alerte(a).iter().enumerate() {
            blocs.push(json!({ "type": "bouton", "id": format!("trafic-al:{k}:{j}"), "texte": l, "style": if j > 0 { json!("second") } else { vide() } }));
        }
    }
    if active && alertes.is_empty() {
        blocs.push(json!({ "type": "pastilles", "elements": [{ "texte": "Rien d'anormal pour l'instant" }] }));
    }
    blocs.push(json!({ "type": "reglages", "rangs": [{ "texte": "Identifier les serveurs sans nom", "note": "Page Trafic et Lag en ligne : cherche à qui appartient chaque adresse inconnue (Valve, Riot, Amazon...) dans l'annuaire public rdap.org. Seule l'adresse du serveur est envoyée.", "bascule": { "id": "trafic-annuaire", "oui": annuaire_permis } }] }));
    blocs.push(json!({ "type": "ligne", "icone": "windows", "couleur": "doux", "texte": "Ce que Windows envoie à Microsoft", "note": "Les identifiants de ton PC, les réglages qui en envoient plus que le minimum, et les envois vus en direct.", "bouton": { "id": "trafic-microsoft", "texte": "Voir", "style": "second" } }));
    if !apps.is_empty() {
        blocs.push(json!({ "type": "titre", "texte": "Programmes qui communiquent avec Internet", "droite": format!("depuis {}", depuis(mins)) }));
        for (k, x) in apps.iter().enumerate() {
            let dests = x.nb_destinations();
            let sig = libelle_signature(x);
            blocs.push(json!({ "type": "ligne", "id": format!("trafic-app:{k}"), "image": if x.chemin.is_empty() { vide() } else { Value::Null }, "icone": "app", "couleur": "doux", "texte": x.titre,
                "note": format!("{} · {dests} destination{}", sig.0, if dests > 1 { "s" } else { "" }),
                "barre": entier(100.0 * x.out / max_o), "valeur": format!("↑ {}", octets(x.out)), "ton": if x.debit > 50.0 * 1024.0 { "attention" } else { "" } }));
        }
    } else if active {
        blocs.push(json!({ "type": "texte", "texte": "Aucun programme ne communique avec Internet pour le moment." }));
    }
    blocs.push(json!({ "type": "note", "texte": "Le contenu des échanges est chiffré (HTTPS) : Nevermind voit quel programme parle à qui et combien il envoie, jamais ce qu'il y a dedans." }));
    let n = apps.len();
    json!({ "titre": "Ce qui sort de ton PC", "sous": "Quels programmes communiquent avec Internet, avec qui, combien ils envoient, et ce qui est anormal.", "icone": "courbe", "couleur": "T",
        "module": {
            "icone": "courbe", "couleur": if !alertes.is_empty() { "orange" } else if active { "vert" } else { "T" },
            "titre": if !active { "Surveillance arrêtée".to_string() } else if !alertes.is_empty() { format!("{} à vérifier", alertes.len()) } else { "Rien d'anormal".into() },
            "texte": if active { format!("Surveillance en cours depuis {}. {n} programme{} parlé avec Internet.", depuis(mins), if n > 1 { "s ont" } else { " a" }) } else { "Démarre la surveillance pour voir ce que tes programmes envoient sur Internet.".into() },
            "boutons": [{ "id": "clic:BtnTraffic", "texte": bouton_surveillance(releve.is_some(), active), "style": if active { json!("second") } else { vide() } }],
        },
        "blocs": blocs })
}

// ---------------------------------------------------------------------------------------------------------------
// Le panneau (Get-PanneauKit)
// ---------------------------------------------------------------------------------------------------------------

/// Ce qu'un bouton du panneau fait (`panneau:b:N`)
#[derive(Clone, Debug, PartialEq)]
pub enum BoutonPanneau {
    /// Un constat du lag : « Limiter Windows Update » (titre du constat)
    LimiterWindowsUpdate(String),
    /// Une action d'un programme du trafic (clé, numéro de l'action de `actions_alerte`)
    ActionApp(String, usize),
    /// « Ne plus lui faire confiance »
    PlusConfiance(String),
    /// « Tout couper » (numéros des réglages)
    ToutCouper(Vec<usize>),
    /// « Voir le détail réel (outil de Microsoft) »
    OutilMicrosoft,
    /// « Couper » un réglage
    Couper(usize),
}

/// Le panneau en construction : ses blocs et ses boutons
#[derive(Default)]
pub struct Panneau {
    pub blocs: Vec<Value>,
    pub boutons: Vec<BoutonPanneau>,
}

impl Panneau {
    fn titre_section(&mut self, t: &str) {
        self.blocs.push(json!({ "type": "titre", "texte": format!("●  {}", t.to_uppercase()) }));
    }
    fn texte(&mut self, t: &str) {
        self.blocs.push(json!({ "type": "texte", "texte": t }));
    }
    fn note(&mut self, t: &str) {
        self.blocs.push(json!({ "type": "note", "texte": t }));
    }
    fn titre(&mut self, t: &str) {
        self.blocs.push(json!({ "type": "titre", "texte": t }));
    }
    fn bouton(&mut self, t: &str, principal: bool, b: BoutonPanneau) {
        self.blocs.push(json!({ "type": "bouton", "id": format!("panneau:b:{}", self.boutons.len()), "texte": t, "style": if principal { Value::Null } else { json!("second") } }));
        self.boutons.push(b);
    }
    /// Une ligne de tableau : nom, détail, valeur (`New-InfoRows`, grilles de deux colonnes)
    fn ligne(&mut self, textes: &[String]) {
        let t: Vec<&String> = textes
            .iter()
            .filter(|x| !x.is_empty() && !(x.chars().count() <= 2 && matches!(x.as_str(), "✓" | "!" | "i" | "×" | "›")))
            .collect();
        if t.len() >= 2 && !t.iter().any(|x| x.chars().any(|c| c.is_lowercase())) {
            self.blocs.push(json!({ "type": "titre", "texte": t[0], "droite": t[t.len() - 1] }));
        } else if t.len() >= 2 {
            let mut l = json!({ "type": "ligne", "texte": t[0], "valeur": t[t.len() - 1] });
            if t.len() > 2 {
                l["note"] = json!(t[1..t.len() - 1].iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" · "));
            }
            self.blocs.push(l);
        } else if t.len() == 1 {
            self.texte(t[0]);
        }
    }
    /// Un constat (`New-SecurityCard`) : titre, détail, éléments (un à trois : seul le premier, comme un encadré), boutons
    fn constat(&mut self, titre: &str, detail: &str, elements: &[String], boutons: Vec<(String, BoutonPanneau)>) {
        self.titre(titre);
        self.texte(detail);
        if !elements.is_empty() {
            if elements.len() <= 3 {
                match elements.iter().find(|e| e.chars().count() > 3) {
                    Some(p) => self.note(p),
                    None => elements.iter().for_each(|e| self.texte(e)),
                }
            } else {
                for e in elements {
                    self.texte(e);
                }
            }
        }
        for (i, (t, b)) in boutons.into_iter().enumerate() {
            self.bouton(&t, i == 0, b);
        }
    }

    /// La page du panneau (`Get-PanneauKit`, une fois le panneau rempli et le test fini)
    pub fn page(
        &self,
        titre: &str,
        sous: &str,
        icone: &str,
        etat: &str,
        texte_etat: &str,
        parent: Option<&str>,
    ) -> Value {
        let ton = match etat {
            "ok" => "ok",
            "warn" => "attention",
            "bad" => "urgent",
            _ => "",
        };
        json!({
            "titre": titre, "sous": sous, "icone": icone, "couleur": "P", "retour": "appli",
            "parent": parent.filter(|p| !["accueil", "panneau", "fiche"].contains(p)),
            "module": {
                "icone": match ton { "ok" => "ok", "attention" | "urgent" => "alerte", _ => icone },
                "couleur": match ton { "ok" => "vert", "attention" => "orange", "urgent" => "rouge", _ => "P" },
                "titre": if texte_etat.is_empty() { titre } else { texte_etat }, "texte": sous,
                "boutons": [{ "id": "panneau:close", "texte": "Fermer", "style": "second" }],
            },
            "blocs": self.blocs,
        })
    }
}

/// Le détail d'une mesure du lag (`Show-LagSession`) : (panneau, titre, sous-titre, état, texte de l'état)
pub fn panneau_lag(s: &Value) -> (Panneau, String, String, String, String) {
    let mut p = Panneau::default();
    let d = date_locale(&texte(champ(s, "Date")));
    let titre = format!("Connexion : {}", texte(champ(s, "Game")));
    let sous = format!(
        "{}, {} de mesure",
        d.map(|d| d.format("%d/%m/%Y à %H:%M").to_string()).unwrap_or_default(),
        duree_jeu(num(champ(s, "Seconds")))
    );
    let niv = texte(champ(champ(s, "Diag"), "Level"));
    let etat_texte = match niv.as_str() {
        "ok" => "Stable",
        "warn" => "À améliorer",
        _ => "Lag trouvé",
    };
    p.titre_section("Le chemin de tes paquets");
    let srv = champ(s, "Server");
    let conn = connexion_seule(s);
    let lieu = lieu_serveur(srv);
    let gw_sous = if vrai(champ(s, "Wifi")) {
        let mut t = "Wi-Fi".to_string();
        if vrai(champ(s, "Band")) {
            t.push_str(&format!(" {}", texte(champ(s, "Band"))));
        }
        if !champ(s, "Signal").is_null() {
            t.push_str(&format!(
                ", signal {} % (au plus bas {} %)",
                texte(champ(s, "SignalAvg")),
                texte(champ(s, "Signal"))
            ));
        }
        t
    } else {
        "Câble Ethernet".into()
    };
    let srv_sous = if vrai(srv) {
        let qui = if vrai(champ(srv, "Owner")) { texte(champ(srv, "Owner")) } else { texte(champ(srv, "Ip")) };
        format!(
            "{qui}{}, {} port {}{}",
            if lieu.is_empty() { String::new() } else { format!(", situé à {lieu}") },
            texte(champ(srv, "Proto")).to_uppercase(),
            texte(champ(srv, "Port")),
            if vrai(champ(srv, "ViaProxy")) {
                ", mesuré au dernier routeur (le serveur ne répond pas au ping)"
            } else {
                ""
            }
        )
    } else {
        "Non repéré".into()
    };
    let segs = [
        ("gw", "Ton PC vers ta box".to_string(), gw_sous),
        (
            "isp",
            "Ta box vers ton fournisseur".into(),
            if vrai(champ(s, "Isp")) { texte(champ(s, "Isp")) } else { "Premier routeur de ton fournisseur".into() },
        ),
        ("ref", "Internet".into(), "Serveur de référence (Cloudflare, 1.1.1.1)".into()),
        ("srv", if conn { "Serveur de connexion du jeu".into() } else { "Serveur du jeu".into() }, srv_sous),
    ];
    let st = champ(s, "Stats");
    for (k, nom, sous_seg) in segs {
        let x = champ(st, k);
        if k == "srv" && !vrai(srv) && vrai(champ(s, "Quick")) {
            continue;
        }
        let _ = etat_etape(x, k);
        if vrai(x) && !vrai(champ(x, "Dead")) {
            p.ligne(&[
                nom,
                sous_seg,
                ms(nombre(champ(x, "Med"))),
                format!(
                    "variation {} ms, pics {}, perdus {} %",
                    pn(champ(x, "Jit")),
                    ms(nombre(champ(x, "P95"))),
                    pn(champ(x, "Loss"))
                ),
            ]);
        } else {
            p.note(&nom);
        }
    }
    p.titre_section("Le ping pendant la mesure");
    // (la courbe de la version PowerShell est un dessin, envoyé en image ; ici une courbe de l'écran par étape :
    // DIVERGENCES.md)
    let series = champ(s, "Series");
    for (k, nom) in [("gw", "Ton PC vers ta box"), ("ref", "Internet (référence)"), ("srv", "Serveur du jeu")] {
        let v: Vec<f64> = liste(champ(series, k)).iter().map(num).collect();
        if v.len() >= 2 {
            p.blocs.push(json!({ "type": "courbe", "points": v.iter().map(|x| crate::valeur::n(x.max(0.0))).collect::<Vec<_>>(), "unite": "ms", "titre": nom }));
        }
    }
    for (k, nom) in [("gw", "Ton PC vers ta box"), ("ref", "Internet (référence)"), ("srv", "Serveur du jeu")] {
        if vrai(champ(series, k)) {
            p.texte(&format!("●  {nom}"));
        }
    }
    p.texte("▬  paquets perdus");
    p.titre_section("Ce que Nevermind a trouvé");
    for fd in liste(champ(champ(s, "Diag"), "Findings")) {
        let tips: Vec<String> = liste(champ(&fd, "Tips")).iter().filter(|t| vrai(t)).map(texte).collect();
        let titre_c = texte(champ(&fd, "Title"));
        let mut boutons = Vec::new();
        for a in liste(champ(&fd, "Actions")) {
            let l = texte(champ(&a, "Label"));
            if l == crate::lag::diag::ACTION_WINDOWS_UPDATE {
                // (recréée depuis le diagnostic : seulement si le constat la propose encore)
                let vivant = crate::lag::diag::diagnostic(s);
                if vivant.constats.iter().any(|c| c.titre == titre_c && c.actions.contains(&l)) {
                    boutons.push((l.clone(), BoutonPanneau::LimiterWindowsUpdate(titre_c.clone())));
                }
            }
        }
        p.constat(&titre_c, &texte(champ(&fd, "Detail")), &tips, boutons);
    }
    p.texte(&format!(
        "Pings envoyés deux fois par seconde vers chaque étape. « Variation » : de combien le ping change d'un envoi à l'autre (au-dessus de 10 ms, ça se sent en jeu). « Pics » : 95 % des pings étaient en dessous. {} Si l'option « Pays du serveur dans Lag en ligne » est activée (Paramètres, Réseau et vie privée), le pays du serveur vient de l'annuaire public ipinfo.io (seule l'adresse du serveur est envoyée).",
        if vrai(champ(s, "Etw")) { "" } else { "Windows a refusé la trace réseau pendant cette mesure : seul le serveur de connexion du jeu (TCP) était visible." }
    ));
    (p, titre, sous, niv, etat_texte.into())
}

fn pn(x: &Value) -> String {
    match nombre(x) {
        Some(f) => ps_nombre(f),
        None => texte(x),
    }
}

/// Les échanges groupés par type pour un programme (`Get-AppDestGroups`) : le réseau local en dernier, sinon le
/// plus gros volume d'abord
pub struct Groupe<'a> {
    pub sorte: Sorte,
    pub out: f64,
    pub entrant: f64,
    pub dests: Vec<&'a crate::trafic::etat::Dest>,
}

pub fn groupes<'a>(r: &'a Releve, a: &'a App, annuaire: &mut Annuaire) -> Vec<Groupe<'a>> {
    let mut g: Vec<Groupe> = Vec::new();
    let outil = crate::trafic::alertes::OUTILS_DISTANCE.is_match(&a.nom);
    for d in a.dest.values() {
        let t = if d.privee {
            types::LOCAL
        } else {
            let p = annuaire.proprietaire(&d.distante).map(|e| (e.o.clone(), e.n.clone()));
            types::sorte(
                &r.nom_dest(d),
                d.port,
                d.out,
                d.entrant,
                outil,
                p.as_ref().map(|(o, n)| (o.as_str(), n.as_str())),
            )
        };
        match g.iter_mut().find(|x| x.sorte.id == t.id) {
            Some(x) => {
                x.out += d.out;
                x.entrant += d.entrant;
                x.dests.push(d);
            }
            None => g.push(Groupe { sorte: t, out: d.out, entrant: d.entrant, dests: vec![d] }),
        }
    }
    g.sort_by(|x, y| {
        (y.sorte.id != "local")
            .cmp(&(x.sorte.id != "local"))
            .then((y.out + y.entrant).total_cmp(&(x.out + x.entrant)))
            .then(y.dests.len().cmp(&x.dests.len()))
    });
    g
}

/// Le détail d'un programme du trafic (`Show-TrafficApp`)
pub fn panneau_app(
    r: &Releve,
    a: &App,
    alerte: Option<&Alerte>,
    confiance: Option<&str>,
    annuaire: &mut Annuaire,
) -> (Panneau, String, String, String, String) {
    let mut p = Panneau::default();
    let (etat, etat_texte) = match alerte.map(|x| x.niveau) {
        Some("bad") => ("bad", "Suspect"),
        Some("warn") => ("warn", "À vérifier"),
        Some(n) => (n, "Normal"),
        None => ("ok", "Normal"),
    };
    let sig = libelle_signature(a);
    p.titre_section("Le programme");
    let mut lignes = vec![
        ("Nom".to_string(), format!("{} ({})", a.titre, a.nom)),
        ("Éditeur".into(), sig.0.clone()),
        ("Emplacement".into(), if a.chemin.is_empty() { "Programme de Windows".into() } else { a.chemin.clone() }),
        ("Envoyé".into(), octets(a.out)),
        ("Reçu".into(), octets(a.entrant)),
        (
            "Utilise aussi l'UDP".into(),
            if a.udp { "Oui (jeux, appels, vidéo : destinations non visibles)".into() } else { "Non".into() },
        ),
    ];
    if !a.services.is_empty() {
        lignes.push((
            "Services Windows".into(),
            a.services.iter().map(|s| format!("{} ({})", s.titre, s.nom)).collect::<Vec<_>>().join(", "),
        ));
    }
    for (k, v) in lignes {
        p.ligne(&[k, v]);
    }
    if let Some(d) = confiance {
        p.texte(&format!(
            "Tu as approuvé ce programme{} : il n'est plus signalé.",
            if d.is_empty() { String::new() } else { format!(" le {d}") }
        ));
        p.bouton("Ne plus lui faire confiance", false, BoutonPanneau::PlusConfiance(a.cle.clone()));
    }
    let actions = actions_alerte(a);
    if let Some(al) = alerte {
        p.titre_section("Pourquoi il est signalé");
        p.constat(
            if al.niveau == "bad" { "Comportement suspect" } else { "À vérifier" },
            &format!("{}.", al.pourquoi.join(" ; ")),
            &[],
            actions
                .iter()
                .enumerate()
                .map(|(i, l)| (l.to_string(), BoutonPanneau::ActionApp(a.cle.clone(), i)))
                .collect(),
        );
    } else if !a.chemin.is_empty() {
        for (i, l) in actions.iter().enumerate().skip(1) {
            p.bouton(l, false, BoutonPanneau::ActionApp(a.cle.clone(), i));
        }
    }
    let gs = groupes(r, a, annuaire);
    let ids: Vec<&str> = gs.iter().filter(|g| g.sorte.id != "local").map(|g| g.sorte.id).collect();
    if !ids.is_empty() {
        let mo = 1024.0 * 1024.0;
        let mut l: Vec<String> = Vec::new();
        if a.out < 5.0 * mo {
            l.push(format!(
                "Aucun gros envoi : {} en tout, pas de fichiers ni de vidéo envoyés en masse",
                octets(a.out)
            ));
        } else if a.out > 3.0 * a.entrant && a.out > 50.0 * mo {
            l.push(format!(
                "Gros envoi : {} envoyés contre {} reçus (fichiers, vidéo ou sauvegarde)",
                octets(a.out),
                octets(a.entrant)
            ));
        } else if a.out > 3.0 * a.entrant {
            l.push(format!("Envoie bien plus qu'il ne reçoit : {} contre {}", octets(a.out), octets(a.entrant)));
        } else {
            l.push(format!("Envoi moyen ({}) : normal pour discuter, jouer ou naviguer", octets(a.out)));
        }
        if !ids.iter().any(|i| !["unknown", "other", "upload", "download", "owned"].contains(i)) {
            l.push(
                "Les serveurs ne disent pas à quoi ils servent : impossible de deviner quel type de données part"
                    .into(),
            );
        } else {
            l.push(
                if ids.contains(&"ads") {
                    "Publicité et suivi : ce que tu fais est mesuré"
                } else {
                    "Aucune publicité ni pistage repéré"
                }
                .into(),
            );
            l.push(
                if ids.contains(&"telemetry") {
                    "Envoie des statistiques d'utilisation (pas tes fichiers)"
                } else {
                    "Aucune statistique d'utilisation repérée"
                }
                .into(),
            );
            l.push(
                if ids.contains(&"remote") {
                    "Peut transmettre l'image de ton écran (prise en main à distance)"
                } else {
                    "Aucune prise en main à distance"
                }
                .into(),
            );
        }
        p.titre_section("En résumé");
        for x in l {
            p.texte(&x);
        }
    }
    p.titre_section("Ce qu'il envoie (probablement)");
    p.texte(&format!(
        "Deviné d'après le nom des serveurs et les volumes : le contenu, chiffré, n'est jamais lu.{}",
        if a.udp { " Les échanges UDP (jeu, voix) ne sont pas comptés." } else { "" }
    ));
    if gs.is_empty() {
        p.texte("Aucune connexion vue pour l'instant.");
    }
    for g in &gs {
        p.ligne(&[
            g.sorte.libelle.to_string(),
            g.sorte.texte.to_string(),
            format!("↑ {}", octets(g.out)),
            format!("↓ {}", octets(g.entrant)),
        ]);
        let mut ds = g.dests.clone();
        ds.sort_by(|x, y| (y.out + y.entrant).total_cmp(&(x.out + x.entrant)));
        for d in ds.iter().take(6) {
            let nm = r.dns.get(&d.distante).cloned().unwrap_or_default();
            let mut t = vec!["●".to_string(), if nm.is_empty() { d.distante.clone() } else { nm.clone() }];
            if !d.privee
                && let Some(ow) = annuaire.proprietaire(&d.distante)
            {
                t.push(format!("Appartient à {}", ow.libelle()));
            }
            if let Some((l, x)) = microsoft::service(&nm) {
                t.push(format!("Microsoft, {l} : {x}"));
            }
            let pn = nom_port(d.port);
            let sus = port_suspect(d.port);
            t.push(format!(
                "{}port {}{}",
                if nm.is_empty() { String::new() } else { format!("{}, ", d.distante) },
                d.port,
                match (pn, sus) {
                    (Some(n), _) => format!(" ({n})"),
                    (None, Some(s)) => format!(" ({s})"),
                    _ => String::new(),
                }
            ));
            t.push(format!("↑ {}    ↓ {}", octets(d.out), octets(d.entrant)));
            p.ligne(&t);
        }
        if ds.len() > 6 {
            let k = ds.len() - 6;
            p.texte(&format!(
                "+ {k} autre{} serveur{} du même type",
                if ds.len() > 7 { "s" } else { "" },
                if ds.len() > 7 { "s" } else { "" }
            ));
        }
    }
    let sous = if a.chemin.is_empty() { a.nom.clone() } else { a.chemin.clone() };
    (p, a.titre.clone(), sous, etat.into(), etat_texte.into())
}

/// « Ce que Windows envoie à Microsoft » (`Show-WindowsPrivacy`)
pub fn panneau_microsoft(
    w: &dyn microsoft::LecturesWindows,
    r: Option<&Releve>,
    annuaire: &mut Annuaire,
) -> (Panneau, String, String, String, String, Vec<microsoft::Reglage>) {
    let mut p = Panneau::default();
    let items = microsoft::reglages(w);
    let on: Vec<usize> = items.iter().enumerate().filter(|(_, i)| i.actif).map(|(k, _)| k).collect();
    let n = on.len();
    if n > 0 {
        p.blocs.push(json!({ "type": "ligne", "icone": "alerte", "couleur": "orange", "texte": format!("{n} réglage{} envoie{} des données à Microsoft en plus du minimum. Tu peux tout couper, c'est annulable.", if n > 1 { "s" } else { "" }, if n > 1 { "nt" } else { "" }) }));
        p.bouton("Tout couper", true, BoutonPanneau::ToutCouper(on.clone()));
    } else {
        p.blocs.push(json!({ "type": "ligne", "icone": "ok", "couleur": "vert", "texte": "Windows n'envoie que le minimum à Microsoft : tout ce qui peut être coupé l'est." }));
    }
    p.bouton("Voir le détail réel (outil de Microsoft)", false, BoutonPanneau::OutilMicrosoft);
    p.titre_section("Les identifiants de ton PC");
    for (k, v, _) in microsoft::identifiants(w) {
        p.ligne(&[k, v]);
    }
    p.titre_section("Ce qui envoie des données");
    let mut ordre: Vec<usize> = (0..items.len()).collect();
    ordre.sort_by_key(|&k| !items[k].actif);
    for k in ordre {
        let it = &items[k];
        if it.actif {
            p.titre(it.titre);
            p.blocs.push(json!({ "type": "ligne", "icone": "alerte", "couleur": "orange", "texte": "Envoie" }));
            p.texte(it.texte);
            p.bouton("Couper", false, BoutonPanneau::Couper(k));
        } else {
            p.note(it.titre);
        }
    }
    p.titre_section("En ce moment");
    match r {
        None => p.texte("Lance la surveillance de la page Trafic pour voir quels programmes parlent à Microsoft."),
        Some(r) => {
            let envois = microsoft::envois(r, annuaire);
            if envois.is_empty() {
                p.texte("Aucun échange avec Microsoft vu depuis le début de la surveillance.");
            }
            for e in envois.iter().take(12) {
                let mut svcs: Vec<(String, Option<&'static str>)> = Vec::new();
                for (_, _, s) in &e.coups {
                    let k = s.map(|x| x.0.to_string()).unwrap_or_default();
                    if !svcs.iter().any(|(x, _)| *x == k) {
                        svcs.push((k, s.map(|x| x.1)));
                    }
                }
                svcs.sort_by_key(|(k, _)| k.is_empty());
                let lignes: Vec<String> = svcs
                    .iter()
                    .map(|(k, t)| {
                        if k.is_empty() {
                            "Autre service Microsoft : le nom du serveur ne dit pas lequel".into()
                        } else {
                            format!("{k} : {}", t.unwrap_or(""))
                        }
                    })
                    .collect();
                if lignes.len() == 1 {
                    p.note(&e.app.titre);
                } else {
                    p.ligne(&[e.app.titre.clone(), format!("↑ {}   ↓ {}", octets(e.out), octets(e.entrant))]);
                    for l in lignes {
                        p.texte(&l);
                    }
                }
            }
        }
    }
    p.texte("Le contenu est chiffré : ce qui est envoyé est décrit d'après la documentation de Microsoft, pas lu. Les données « obligatoires » (identifiant de l'appareil, version, plantages) ne peuvent pas être coupées sur Windows Famille ou Pro sans arrêter le service de télémétrie.");
    let (etat, etat_texte) = if n > 0 {
        ("warn".to_string(), format!("{n} à couper"))
    } else {
        ("ok".to_string(), "Le minimum".to_string())
    };
    (
        p,
        "Ce que Windows envoie à Microsoft".into(),
        "Identifiants, réglages et envois vus en direct".into(),
        etat,
        etat_texte,
        items,
    )
}

/// `'{0:N0}'` pour les tests
pub fn n0(x: f64) -> String {
    nombre_n(x, 0)
}
