//! Le diagnostic d'une mesure du lag (`Get-SegStatus`, `Get-LagDiagnosis`, `Set-LagDiag`) et la ligne de
//! résumé d'une mesure (`Get-LagSessionLine`). La mesure est lue comme elle est gardée dans lag.json.

use crate::format::{ms, octets, pays, ps_nombre};
use crate::valeur::{champ, liste, nombre, num, texte, vrai};
use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;

/// L'état d'une étape du chemin : « bad », « warn », « ok » ou « none » (pas mesurée, ou sans réponse)
pub fn etat_etape(s: &Value, sorte: &str) -> &'static str {
    if s.is_null() || vrai(champ(s, "Dead")) {
        return "none";
    }
    let (perte, jit, p95, med) =
        (num(champ(s, "Loss")), num(champ(s, "Jit")), num(champ(s, "P95")), num(champ(s, "Med")));
    if sorte == "gw" {
        if perte >= 2.0 || jit >= 10.0 || p95 >= 50.0 {
            return "bad";
        }
        if perte >= 0.5 || jit >= 4.0 || p95 >= 20.0 {
            return "warn";
        }
        return "ok";
    }
    if perte >= 3.0 || jit >= 20.0 || p95 >= med + 80.0 {
        return "bad";
    }
    if perte >= 1.0 || jit >= 8.0 || p95 >= med + 35.0 {
        return "warn";
    }
    "ok"
}

fn mauvais(e: &str) -> bool {
    e == "bad" || e == "warn"
}

/// Seul le serveur de connexion du jeu (TCP) a été vu : Windows a refusé la trace réseau
pub fn connexion_seule(r: &Value) -> bool {
    let s = champ(r, "Server");
    vrai(s) && texte(champ(s, "Proto")) == "tcp" && !vrai(champ(r, "Etw"))
}

/// Où se trouve un serveur : « Frankfurt am Main, Allemagne », « Allemagne » ou ''
pub fn lieu_serveur(s: &Value) -> String {
    if !vrai(s) || !vrai(champ(s, "Pays")) {
        return String::new();
    }
    let p = pays(&texte(champ(s, "Pays")));
    if vrai(champ(s, "Ville")) { format!("{}, {p}", texte(champ(s, "Ville"))) } else { p }
}

/// Un constat
#[derive(Clone, Debug)]
pub struct Constat {
    pub statut: &'static str,
    pub titre: String,
    pub detail: String,
    /// `None` : pas de conseils (gardé `[null]` dans lag.json, comme `@($_.Tips)`)
    pub conseils: Option<Vec<String>>,
    pub actions: Vec<String>,
}

pub struct Diagnostic {
    pub niveau: &'static str,
    pub titre: String,
    pub constats: Vec<Constat>,
}

impl Diagnostic {
    /// Comme `Set-LagDiag` le garde dans la mesure
    pub fn json(&self) -> Value {
        json!({
            "Level": self.niveau,
            "Title": self.titre,
            "Findings": self.constats.iter().map(|c| json!({
                "Status": c.statut, "Title": c.titre, "Detail": c.detail,
                "Tips": match &c.conseils { Some(t) => json!(t), None => json!([null]) },
                "Actions": c.actions.iter().map(|a| json!({ "Label": a })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }
}

static WINDOWS_UPDATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)Windows : .*(Optimisation de livraison|Optimisation de la distribution|Delivery Optimization|Windows Update|BITS|transfert intelligent)")
        .expect("motif")
});
static BANDE_2: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^2").expect("motif"));

fn c(statut: &'static str, titre: &str, detail: String, conseils: Option<Vec<String>>) -> Constat {
    Constat { statut, titre: titre.into(), detail, conseils, actions: Vec::new() }
}
fn v(l: &[&str]) -> Option<Vec<String>> {
    Some(l.iter().map(|s| s.to_string()).collect())
}

/// `"$($x.Champ)"` d'un nombre
fn pn(x: &Value) -> String {
    match nombre(x) {
        Some(f) => ps_nombre(f),
        None => texte(x),
    }
}

/// Le canal Wi-Fi en nombre (`[int]$R.Channel`)
fn canal(x: &Value) -> Option<i64> {
    nombre(x).map(crate::format::entier)
}

/// Les constats, du plus grave au moins grave
pub fn diagnostic(r: &Value) -> Diagnostic {
    let st = champ(r, "Stats");
    let (gw, rf, srv) = (champ(st, "gw"), champ(st, "ref"), champ(st, "srv"));
    let (g_s, r_s, s_s) = (etat_etape(gw, "gw"), etat_etape(rf, "ref"), etat_etape(srv, "srv"));
    let mut f: Vec<Constat> = Vec::new();
    let pct = |a: f64, b: f64| if b > 0.0 { a / b } else { 0.0 };
    // 1. Entre le PC et la box
    if mauvais(g_s) {
        if vrai(champ(r, "Wifi")) {
            let mut tips = vec!["Le plus efficace : brancher le PC à la box avec un câble Ethernet (ou un boîtier CPL si la box est loin).".to_string()];
            let bande = texte(champ(r, "Band"));
            if BANDE_2.is_match(&bande)
                || (vrai(champ(r, "Channel")) && canal(champ(r, "Channel")).is_some_and(|c| c <= 14))
            {
                tips.push("Ton PC est connecté en Wi-Fi 2,4 GHz : choisis le réseau 5 GHz de ta box, il est plus rapide et moins encombré.".into());
            }
            if let Some(sig) = nombre(champ(r, "Signal"))
                && sig < 60.0
            {
                tips.push(format!(
                    "Le signal est descendu à {} % : rapproche le PC de la box ou ajoute un répéteur.",
                    pn(champ(r, "Signal"))
                ));
            }
            tips.push("Évite les gros téléchargements sur les autres appareils pendant que tu joues.".into());
            f.push(c(
                g_s,
                "Le Wi-Fi fait laguer",
                format!(
                    "Entre ton PC et ta box : {} % de paquets perdus, un ping qui varie de {} ms en moyenne, avec des pics à {}. En jeu, ça donne des téléportations et des retours en arrière.",
                    pn(champ(gw, "Loss")),
                    pn(champ(gw, "Jit")),
                    ms(nombre(champ(gw, "P95")))
                ),
                Some(tips),
            ));
        } else {
            f.push(c(
                g_s,
                "Problème entre le PC et la box",
                format!(
                    "Même en câble, {} % des paquets se perdent entre ton PC et ta box, avec des pics à {}.",
                    pn(champ(gw, "Loss")),
                    ms(nombre(champ(gw, "P95")))
                ),
                v(&[
                    "Essaie un autre câble Ethernet et une autre prise de la box.",
                    "Si tu passes par des boîtiers CPL, branche le PC directement sur la box pour comparer.",
                    "Mets à jour le pilote de ta carte réseau (site du fabricant de ta carte mère).",
                ]),
            ));
        }
    }
    // 2. Entre la box et Internet
    if !mauvais(g_s) && mauvais(r_s) {
        let apps: Vec<Value> = liste(champ(r, "BgApps")).into_iter().filter(|a| vrai(champ(a, "Name"))).collect();
        if !apps.is_empty() && pct(num(champ(r, "BgSpikes")), num(champ(r, "Spikes"))) >= 0.3 {
            let noms = apps
                .iter()
                .map(|a| format!("{} ({}/s)", texte(champ(a, "Name")), octets(num(champ(a, "Rate")))))
                .collect::<Vec<_>>()
                .join(", ");
            let mut k = c(
                r_s,
                "Un téléchargement sature ta connexion",
                format!(
                    "Pendant les pics de ping, ton PC téléchargeait ou envoyait en même temps : {noms}. La connexion était pleine, les paquets du jeu attendaient leur tour."
                ),
                v(&[
                    "Dans Steam : Paramètres, Téléchargements, décoche « Autoriser les téléchargements pendant une partie ».",
                    "Active le Mode jeu de Nevermind : il ferme les launchers et la synchronisation pendant tes parties.",
                    "Lance tes mises à jour et téléchargements avant ou après tes parties.",
                ]),
            );
            if apps.iter().any(|a| WINDOWS_UPDATE.is_match(&texte(champ(a, "Name")))) {
                k.actions.push(ACTION_WINDOWS_UPDATE.into());
            }
            f.push(k);
        } else {
            let isp = if vrai(champ(r, "Isp")) { format!(" ({})", texte(champ(r, "Isp"))) } else { String::new() };
            let mut tips = vec![
                "Regarde si quelqu'un de la maison regarde une vidéo, télécharge ou sauvegarde en ligne pendant tes parties.".to_string(),
                "Redémarre ta box : débranche-la 30 secondes.".into(),
                "Fais le test « Latence en charge » de la page Tests : il montre si ta box gère mal les gros téléchargements.".into(),
                format!("Si ça dure plusieurs jours, contacte ton fournisseur{isp} avec ces chiffres."),
            ];
            if !vrai(champ(r, "Etw")) {
                tips.push("Windows a refusé la trace réseau pendant cette partie (souvent l'anti-triche du jeu) : Nevermind n'a pas pu voir si un programme de ce PC téléchargeait.".into());
            }
            f.push(c(
                r_s,
                "Ta connexion Internet sature ou décroche",
                format!(
                    "Ton PC et ta box communiquent bien, mais le ping vers Internet a des pics (jusqu'à {}, {} % perdus). Soit un autre appareil de la maison utilise beaucoup la connexion, soit ta box ou ton fournisseur a un souci.",
                    ms(nombre(champ(rf, "P95"))),
                    pn(champ(rf, "Loss"))
                ),
                Some(tips),
            ));
        }
    }
    // 3. Le serveur du jeu
    let serveur = champ(r, "Server");
    let owner = if vrai(serveur) && vrai(champ(serveur, "Owner")) {
        texte(champ(serveur, "Owner"))
    } else {
        "un hébergeur inconnu".into()
    };
    let lieu = lieu_serveur(serveur);
    let connexion = connexion_seule(r);
    let vivant = |x: &Value| vrai(x) && !vrai(champ(x, "Dead"));
    if connexion {
        f.push(c(
            "info",
            "Serveur de la partie non repéré",
            format!(
                "Windows a refusé la trace réseau pendant cette partie (l'anti-triche du jeu) : Nevermind n'a vu que le serveur de connexion du jeu ({owner}{}, {}). Il gère ton compte et les menus, pas forcément la partie : son éloignement ne veut pas dire que tes parties se jouent loin.",
                if lieu.is_empty() { String::new() } else { format!(", situé à {lieu}") },
                if vivant(srv) { format!("{} de ping", ms(nombre(champ(srv, "Med")))) } else { "sans réponse au ping".into() }
            ),
            v(&["Laisse Nevermind ouvert avant de lancer le jeu : la trace réseau démarre alors avant l'anti-triche et le vrai serveur de la partie est repéré."]),
        ));
    } else if vivant(srv) && vivant(rf) {
        let (sm, rm) = (num(champ(srv, "Med")), num(champ(rf, "Med")));
        if sm > 70.0 && sm - rm > 45.0 {
            f.push(c(
                "warn",
                "Le serveur du jeu est loin",
                format!(
                    "Le serveur appartient à {owner}{} : {} de ping, alors qu'Internet répond en {} depuis chez toi. Ce délai vient de la distance, pas de ta connexion.",
                    if lieu.is_empty() { String::new() } else { format!(" et se trouve à {lieu}") },
                    ms(Some(sm)),
                    ms(Some(rm))
                ),
                v(&[
                    "Dans les options du jeu, choisis une région de serveurs plus proche (Europe, France).",
                    "Si tu joues avec des amis loin de toi, le jeu peut choisir un serveur entre vous.",
                ]),
            ));
        }
        if mauvais(s_s) && r_s == "ok" && !mauvais(g_s) {
            f.push(c(
                "warn",
                "Le serveur du jeu avait des soucis",
                format!(
                    "Ta connexion était stable, mais le ping vers le serveur ({owner}{}) variait beaucoup : pics à {}, {} % perdus. Le problème est du côté du serveur ou de la route jusqu'à lui.",
                    if lieu.is_empty() { String::new() } else { format!(", {lieu}") },
                    ms(nombre(champ(srv, "P95"))),
                    pn(champ(srv, "Loss"))
                ),
                v(&[
                    "Change de serveur ou de région si le jeu le permet.",
                    "Regarde si d'autres joueurs signalent des soucis (réseaux sociaux du jeu).",
                ]),
            ));
        }
    }
    let mins = f64::max(1.0, num(champ(r, "Seconds")) / 60.0);
    let trous = num(champ(r, "Gaps"));
    if vrai(champ(r, "Gaps")) && trous / mins >= 2.0 && !mauvais(g_s) && !mauvais(r_s) {
        f.push(c(
            "warn",
            "Le serveur envoyait ses données par à-coups",
            format!(
                "{} fois, le serveur du jeu n'a rien envoyé pendant plus d'un quart de seconde (le plus long : {} ms), alors que ta connexion était stable. Ça vient du serveur ou de la route jusqu'à lui.",
                pn(champ(r, "Gaps")),
                pn(champ(r, "MaxGap"))
            ),
            v(&["Change de serveur ou de région si le jeu le permet."]),
        ));
    }
    // 4. VPN
    if vrai(champ(r, "Vpn")) {
        f.push(c(
            "warn",
            "Un VPN rallonge le chemin",
            "Ta connexion passe par un VPN : chaque paquet fait un détour par le serveur du VPN, ce qui ajoute du ping.".into(),
            v(&["Coupe le VPN pendant tes parties, ou choisis un serveur VPN proche de chez toi."]),
        ));
    }
    // 5. Serveur non repéré
    if !vrai(champ(r, "Quick")) && !vrai(serveur) {
        f.push(c(
            "info",
            "Serveur du jeu non repéré",
            if vrai(champ(r, "Etw")) {
                "Le jeu n'a pas échangé assez avec un serveur pour que Nevermind le reconnaisse (menu, partie hors ligne ?).".into()
            } else {
                "Windows a refusé la trace réseau pendant cette partie (souvent l'anti-triche du jeu) : Nevermind n'a pas pu repérer le serveur du jeu en UDP.".into()
            },
            None,
        ));
    }
    if !f.iter().any(|x| mauvais(x.statut)) {
        let mut parts = Vec::new();
        if vivant(gw) {
            parts.push(format!("box {}", ms(nombre(champ(gw, "Med")))));
        }
        if vivant(rf) {
            parts.push(format!("Internet {}", ms(nombre(champ(rf, "Med")))));
        }
        if vivant(srv) && !connexion {
            parts.push(format!(
                "serveur {}{}",
                ms(nombre(champ(srv, "Med"))),
                if lieu.is_empty() { String::new() } else { format!(" ({lieu})") }
            ));
        }
        f.insert(
            0,
            c(
                "ok",
                "Ta connexion était stable",
                format!(
                    "Ping : {}, sans pic ni perte notable. Si le jeu saccadait, ça vient plutôt de tes FPS (Perf. In Game, onglet FPS).",
                    parts.join(", ")
                ),
                None,
            ),
        );
    }
    let niveau = if f.iter().any(|x| x.statut == "bad") {
        "bad"
    } else if f.iter().any(|x| x.statut == "warn") {
        "warn"
    } else {
        "ok"
    };
    let titre = f.iter().find(|x| x.statut == niveau).map(|x| x.titre.clone()).unwrap_or_default();
    Diagnostic { niveau, titre, constats: f }
}

/// Le libellé de l'action « Limiter Windows Update » d'un constat
pub const ACTION_WINDOWS_UPDATE: &str = "Limiter Windows Update";

/// La ligne de résumé d'une mesure (`Get-LagSessionLine`)
pub fn ligne_mesure(s: &Value) -> String {
    let mut p = Vec::new();
    let conn = connexion_seule(s);
    let st = champ(s, "Stats");
    for k in ["gw", "ref", "srv"] {
        let x = champ(st, k);
        if vrai(x) && !vrai(champ(x, "Dead")) {
            let nom = match k {
                "gw" => "Box",
                "ref" => "Internet",
                _ => {
                    if conn {
                        "Serveur de connexion"
                    } else {
                        "Serveur"
                    }
                }
            };
            p.push(format!("{nom} {}", ms(nombre(champ(x, "Med")))));
        }
    }
    let lieu = lieu_serveur(champ(s, "Server"));
    if !lieu.is_empty() {
        p.push(format!("serveur {}situé à {lieu}", if conn { "de connexion " } else { "" }));
    }
    p.join(", ")
}
