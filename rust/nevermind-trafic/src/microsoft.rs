//! Ce que Windows envoie à Microsoft (microsoft.ps1) : les identifiants du PC, les réglages qui envoient des
//! données en plus du minimum (« Couper », annulable depuis la page Sauvegarde), et les envois vus en direct par la
//! surveillance du Trafic. Le contenu est chiffré : ce qui est envoyé vient de la documentation de Microsoft.

use crate::serveurs::Annuaire;
use crate::trafic::etat::{Dest, Releve};
use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;

pub const DIAG_KEY: &str = r"HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection";
pub const DIAG_POLICY_KEY: &str = r"HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection";
pub const CDM_KEY: &str = r"HKCU:\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager";
pub const CDM_VALUES: [&str; 7] = [
    "SubscribedContent-338388Enabled",
    "SubscribedContent-338389Enabled",
    "SubscribedContent-353694Enabled",
    "SubscribedContent-353696Enabled",
    "SilentInstalledAppsEnabled",
    "SystemPaneSuggestionsEnabled",
    "SoftLandingEnabled",
];

/// Serveurs de Microsoft
pub static MS_DOMAINES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)microsoft|windows\.com|windowsupdate|live\.com|live\.net|msn\.com|bing\.(com|net)|office|skype|xbox|azure|msedge|msftconnecttest|onedrive|sharepoint|1drv|microsoftonline|msauth|trafficmanager\.net|msecnd").expect("motif")
});
static MS_PROPRIO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)microsoft|msft").expect("motif"));

/// À quel service de Windows sert un serveur de Microsoft (`$MsEndpoints`)
static SERVICES: LazyLock<Vec<(Regex, &'static str, &'static str)>> = LazyLock::new(|| {
    [
        (r"v10\.events\.data|v20\.events\.data|self\.events\.data|mobile\.events\.data|browser\.events\.data|events\.data\.microsoft|umwatson|watson\.", "Télémétrie de Windows", "identifiant de l'appareil, modèle et matériel, version de Windows, plantages"),
        (r"settings-win\.data|settings\.data\.microsoft", "Réglages de la télémétrie", "Windows demande à Microsoft quoi collecter (identifiant de l'appareil, version)"),
        (r"arc\.msn\.com|iris\.microsoft|ris\.api|g\.msn\.com|api\.msn\.com|assets\.msn|ntp\.msn|www\.msn\.com|img-s-msn|msn\.com", "Pubs et suggestions de Windows", "langue, région, identifiant publicitaire s'il est activé, pour choisir les pubs et les suggestions"),
        (r"login\.live|login\.microsoftonline|account\.live|account\.microsoft|msauth|msa\.", "Compte Microsoft", "ton identifiant de compte et un jeton de connexion chiffré"),
        (r"wns\.windows|notify\.windows|client\.wns", "Notifications", "un identifiant de l'appareil pour recevoir les notifications des applis"),
        (r"activity\.windows\.com", "Historique d'activité", "les applis et fichiers que tu ouvres, pour les retrouver sur tes autres appareils"),
        (r"windowsupdate|delivery\.mp\.microsoft|update\.microsoft|dl\.delivery|tlu\.dl|download\.windowsupdate", "Windows Update", "version de Windows et liste des mises à jour installées, pour savoir quoi télécharger"),
        (r"licensing\.mp|displaycatalog|storeedgefd|storecatalogrevocation|store\.microsoft", "Microsoft Store et licences", "identifiant de compte et de l'appareil, pour vérifier tes achats"),
        (r"onedrive|skyapi|storage\.live|1drv|sharepoint", "OneDrive", "tes fichiers synchronisés et ton identifiant de compte"),
        (r"bing\.(com|net)", "Recherche Bing", "ce que tu tapes dans la recherche Windows, pour proposer des résultats web"),
        (r"xboxlive|xbox\.com", "Xbox", "ton compte Xbox, tes succès et ta présence en ligne"),
        (r"smartscreen|wd\.microsoft|wdcp|defender", "Protection (SmartScreen, Defender)", "l'empreinte des fichiers et l'adresse des sites vérifiés, pour bloquer les virus"),
        (r"edge\.microsoft|msedge", "Microsoft Edge", "synchronisation et services du navigateur"),
        (r"msftconnecttest", "Test de connexion", "rien de personnel : Windows vérifie juste qu'Internet marche"),
        (r"office|officeapps|outlook", "Office et Outlook", "tes documents ou tes mails si tu les utilises, et ton identifiant de compte"),
    ]
    .into_iter()
    .map(|(rx, l, t)| (Regex::new(&format!("(?i){rx}")).expect("motif"), l, t))
    .collect()
});

/// `Get-MsService` : (nom du service, ce qu'il envoie)
pub fn service(nom: &str) -> Option<(&'static str, &'static str)> {
    if nom.is_empty() {
        return None;
    }
    SERVICES.iter().find(|(rx, _, _)| rx.is_match(nom)).map(|(_, l, t)| (*l, *t))
}

/// Ce que Nevermind lit de Windows pour cette page (registre et services, en lecture seulement)
pub trait LecturesWindows {
    /// `Get-RegNum` : un nombre du registre (`None` s'il manque ou s'il n'est pas un nombre)
    fn reg_nombre(&self, chemin: &str, nom: &str) -> Option<i64>;
    /// Un texte du registre (vide s'il manque)
    fn reg_texte(&self, chemin: &str, nom: &str) -> String;
    /// Un service : (en marche, type de démarrage « Automatic », « Manual », « Disabled »...) ; `None` s'il n'existe pas
    fn service(&self, nom: &str) -> Option<(bool, String)>;
    /// Le compte de la session est un compte Microsoft (`PrincipalSource` vaut « MicrosoftAccount »)
    fn compte_microsoft(&self) -> bool;
}

/// Une écriture demandée par « Couper » (passée à la sauvegarde : notée pour « Revenir en arrière »)
#[derive(Clone, Debug, PartialEq)]
pub enum Ecriture {
    /// `Set-Reg` (DWord)
    Registre { chemin: String, nom: String, valeur: i64 },
    /// `Set-ServiceOff` (arrêté et désactivé)
    ServiceCoupe { nom: String },
}

impl Ecriture {
    pub fn json(&self) -> Value {
        match self {
            Ecriture::Registre { chemin, nom, valeur } => {
                json!({ "Type": "reg", "Path": chemin, "Name": nom, "Value": valeur, "Kind": "DWord" })
            }
            Ecriture::ServiceCoupe { nom } => json!({ "Type": "svc", "Name": nom, "Demarrage": "Disabled" }),
        }
    }
}

fn reg(chemin: &str, nom: &str, valeur: i64) -> Ecriture {
    Ecriture::Registre { chemin: chemin.into(), nom: nom.into(), valeur }
}

/// Un réglage qui envoie des données à Microsoft en plus du minimum
#[derive(Clone, Debug)]
pub struct Reglage {
    pub id: &'static str,
    pub titre: &'static str,
    pub actif: bool,
    pub admin: bool,
    pub texte: &'static str,
    pub couper: Vec<Ecriture>,
}

/// Niveau des diagnostics : (niveau, fixé par une stratégie, par défaut)
pub fn niveau_diag(w: &dyn LecturesWindows) -> (i64, bool, bool) {
    if let Some(p) = w.reg_nombre(DIAG_POLICY_KEY, "AllowTelemetry") {
        return (p, true, false);
    }
    let v = w.reg_nombre(DIAG_KEY, "AllowTelemetry");
    (v.unwrap_or(1), false, v.is_none())
}

/// `Get-PrivacyItems`
pub fn reglages(w: &dyn LecturesWindows) -> Vec<Reglage> {
    let (diag, _, _) = niveau_diag(w);
    let adid = w.reg_nombre(r"HKCU:\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo", "Enabled");
    let tail = w.reg_nombre(
        r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Privacy",
        "TailoredExperiencesWithDiagnosticDataEnabled",
    );
    let tipc = w.reg_nombre(r"HKCU:\Software\Microsoft\Input\TIPC", "Enabled");
    let cdm = CDM_VALUES.iter().filter(|n| w.reg_nombre(CDM_KEY, n) == Some(1)).count();
    let iris =
        w.reg_nombre(r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced", "Start_IrisRecommendations");
    let bing = w.reg_nombre(r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Search", "BingSearchEnabled");
    let bing_pol = w.reg_nombre(r"HKCU:\Software\Policies\Microsoft\Windows\Explorer", "DisableSearchBoxSuggestions");
    let speech = w.reg_nombre(r"HKCU:\Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy", "HasAccepted");
    let dt = w.service("DiagTrack");
    let mut couper_diag = vec![reg(DIAG_KEY, "AllowTelemetry", 1)];
    if w.reg_nombre(DIAG_POLICY_KEY, "AllowTelemetry").is_some() {
        couper_diag.push(reg(DIAG_POLICY_KEY, "AllowTelemetry", 1));
    }
    let mut couper_suggest: Vec<Ecriture> = CDM_VALUES.iter().map(|n| reg(CDM_KEY, n, 0)).collect();
    couper_suggest.push(reg(
        r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "Start_IrisRecommendations",
        0,
    ));
    vec![
        Reglage {
            id: "diag",
            titre: "Données de diagnostic facultatives",
            actif: diag >= 2,
            admin: true,
            texte: "En plus du minimum : les sites visités dans Edge, les applis que tu utilises et combien de temps, et le détail du matériel, avec l'identifiant de l'appareil.",
            couper: couper_diag,
        },
        Reglage {
            id: "adid",
            titre: "Identifiant publicitaire",
            actif: adid != Some(0),
            admin: false,
            texte: "Un numéro unique donné aux applis et à leurs régies pub pour te reconnaître d'une appli à l'autre et cibler les pubs.",
            couper: vec![reg(r"HKCU:\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo", "Enabled", 0)],
        },
        Reglage {
            id: "tailored",
            titre: "Expériences personnalisées",
            actif: tail != Some(0),
            admin: false,
            texte: "Microsoft se sert de tes données de diagnostic pour choisir les conseils, pubs et recommandations qu'il te montre.",
            couper: vec![reg(
                r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Privacy",
                "TailoredExperiencesWithDiagnosticDataEnabled",
                0,
            )],
        },
        Reglage {
            id: "suggest",
            titre: "Pubs et suggestions de Windows",
            actif: cdm > 0 || iris == Some(1),
            admin: false,
            texte: "Pubs du menu Démarrer et de l'écran de verrouillage, applis installées toutes seules, conseils. Windows envoie ta langue, ta région et ton identifiant publicitaire pour les choisir.",
            couper: couper_suggest,
        },
        Reglage {
            id: "bing",
            titre: "Recherche Bing dans le menu Démarrer",
            actif: bing != Some(0) && bing_pol != Some(1),
            admin: false,
            texte: "Ce que tu tapes dans la recherche de Windows part chez Bing pour proposer des résultats web. S'applique après redémarrage.",
            couper: vec![
                reg(r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Search", "BingSearchEnabled", 0),
                reg(r"HKCU:\Software\Policies\Microsoft\Windows\Explorer", "DisableSearchBoxSuggestions", 1),
            ],
        },
        Reglage {
            id: "typing",
            titre: "Envoi de ce que tu tapes et écris",
            actif: tipc != Some(0),
            admin: false,
            texte: "Des extraits de ce que tu tapes au clavier et écris au stylet, pour améliorer la saisie de Microsoft.",
            couper: vec![
                reg(r"HKCU:\Software\Microsoft\Input\TIPC", "Enabled", 0),
                reg(r"HKCU:\Software\Microsoft\InputPersonalization", "RestrictImplicitTextCollection", 1),
                reg(r"HKCU:\Software\Microsoft\InputPersonalization", "RestrictImplicitInkCollection", 1),
            ],
        },
        Reglage {
            id: "speech",
            titre: "Reconnaissance vocale en ligne",
            actif: speech == Some(1),
            admin: false,
            texte: "Ta voix est envoyée à Microsoft quand tu dictes du texte ou parles à une appli.",
            couper: vec![reg(
                r"HKCU:\Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy",
                "HasAccepted",
                0,
            )],
        },
        Reglage {
            id: "diagtrack",
            titre: "Service de télémétrie (DiagTrack)",
            actif: dt.as_ref().is_some_and(|(marche, demarrage)| *marche || demarrage != "Disabled"),
            admin: true,
            texte: "C'est lui qui envoie les données de diagnostic avec l'identifiant de l'appareil. L'arrêter ne gêne ni Windows Update, ni les jeux, ni le Store.",
            couper: vec![Ecriture::ServiceCoupe { nom: "DiagTrack".into() }],
        },
    ]
}

pub const ORANGE: &str = "#F5A524";
pub const VERT: &str = "#22D37A";
pub const BLANC: &str = "#FFFFFF";

/// `Get-PcIdentifiers` : (nom, valeur, couleur)
pub fn identifiants(w: &dyn LecturesWindows) -> Vec<(String, String, &'static str)> {
    let (diag, _, defaut) = niveau_diag(w);
    let dt_on = w.service("DiagTrack").is_some_and(|(marche, _)| marche);
    let adid = w.reg_nombre(r"HKCU:\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo", "Enabled");
    let mid = w
        .reg_texte(r"HKLM:\SOFTWARE\Microsoft\SQMClient", "MachineId")
        .trim_matches(|c| c == '{' || c == '}')
        .to_string();
    let msa = w.compte_microsoft();
    vec![
        (
            "Identifiant de l'appareil".into(),
            if dt_on {
                "Envoyé avec chaque rapport de diagnostic".into()
            } else {
                "Pas envoyé : service de télémétrie arrêté".into()
            },
            if dt_on { ORANGE } else { VERT },
        ),
        (
            "Niveau des diagnostics".into(),
            if diag >= 2 {
                "Facultatives (le maximum)".into()
            } else {
                format!("Obligatoires (le minimum){}", if defaut { ", par défaut" } else { "" })
            },
            if diag >= 2 { ORANGE } else { VERT },
        ),
        (
            "Identifiant publicitaire".into(),
            if adid != Some(0) { "Activé : donné aux applis pour les pubs".into() } else { "Désactivé".into() },
            if adid != Some(0) { ORANGE } else { VERT },
        ),
        (
            "Identifiant de l'installation".into(),
            if mid.is_empty() {
                "Absent".into()
            } else {
                format!("Présent ({}...), utilisé par les rapports d'erreurs", mid.chars().take(5).collect::<String>())
            },
            BLANC,
        ),
        (
            "Compte Microsoft".into(),
            if msa {
                "Lié à ce PC : ton identifiant de compte part pour la synchro, le Store, OneDrive, Xbox".into()
            } else {
                "Compte local : pas d'identifiant de compte envoyé".into()
            },
            BLANC,
        ),
    ]
}

/// Un programme qui parle à Microsoft en ce moment
#[allow(clippy::type_complexity)]
pub struct EnvoiMs<'a> {
    pub app: &'a crate::trafic::etat::App,
    pub coups: Vec<(&'a Dest, String, Option<(&'static str, &'static str)>)>,
    pub out: f64,
    pub entrant: f64,
}

/// `Get-MsTraffic`
pub fn envois<'a>(r: &'a Releve, annuaire: &mut Annuaire) -> Vec<EnvoiMs<'a>> {
    let mut rows = Vec::new();
    for a in r.apps.values() {
        let mut coups = Vec::new();
        for d in a.dest.values() {
            if d.privee {
                continue;
            }
            let nm = r.dns.get(&d.distante).cloned().unwrap_or_default();
            let ow = annuaire.proprietaire(&d.distante).map(|e| format!("{} {}", e.o, e.n));
            if MS_DOMAINES.is_match(&nm) || ow.is_some_and(|o| MS_PROPRIO.is_match(&o)) {
                let s = service(&nm);
                coups.push((d, nm, s));
            }
        }
        if coups.is_empty() {
            continue;
        }
        let (mut o, mut i) = (0.0, 0.0);
        for (d, _, _) in &coups {
            o += d.out;
            i += d.entrant;
        }
        rows.push(EnvoiMs { app: a, coups, out: o, entrant: i });
    }
    rows.sort_by(|x, y| (y.out + y.entrant).total_cmp(&(x.out + x.entrant)).then(y.coups.len().cmp(&x.coups.len())));
    rows
}
