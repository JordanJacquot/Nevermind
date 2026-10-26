//! Les demandes que l'Aide accepte (`Invoke-AideAction ... -Verifier`, aide-actions.ps1) : chaque demande est
//! vérifiée seule, et ce qui n'est pas prévu ici est refusé. Ce qui affaiblirait la sécurité du PC n'est jamais
//! accepté, même pour « revenir en arrière » : rallumer le Bureau à distance, couper le contrôle des comptes ou le
//! pare-feu, ajouter une exclusion à l'antivirus, mettre un DNS inconnu, bloquer Internet à l'antivirus ou à Windows
//! Update.
//!
//! [`verifier`] contrôle une demande sans rien faire : elle rend la demande typée, prête à être faite par l'exe, ou le
//! message d'erreur (montré tel quel par Nevermind). Un vrai refus commence par « Demande refusée », « L'Aide garde ce
//! réglage » ou « L'Aide ne connaît pas » ; les autres messages sont des erreurs (carte réseau disparue...).

use crate::demarrage::{self, LecturesDemarrage};
use crate::dossiers::{self, Dossiers};
use crate::nettoyage::{self, Categorie};
use crate::registre::{self, Valeur};
use crate::{chemin, exes, ps, texte};
use serde_json::Value;
use std::collections::HashSet;
use std::net::IpAddr;

/// Version du protocole de l'Aide (`$AideVersion` : 4, l'Aide en SYSTEM ; 5, demandes resserrées, audit du 25/10)
pub const VERSION: i32 = 5;

/// Une carte réseau : son numéro et son identifiant (GUID en minuscules, sans accolades)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Carte {
    pub index: i32,
    pub guid: String,
}

/// Ce que Windows déclare des programmes (`Get-AideContexte`) : les antivirus inscrits au Centre de sécurité, et les
/// services (leur programme, leur processus)
#[derive(Clone, Debug, Default)]
pub struct ContexteProgrammes {
    pub antivirus: Vec<String>,
    pub services_exe: Vec<String>,
    pub services_pid: HashSet<i64>,
}

/// Ce que les règles lisent sur le PC (seulement des lectures)
pub trait Pc: LecturesDemarrage {
    /// Les dossiers de Windows
    fn dossiers(&self) -> &Dossiers;
    /// Le fichier existe (un fichier, pas un dossier)
    fn fichier_existe(&self, chemin: &str) -> bool;
    /// Le processus tourne
    fn processus_existe(&self, pid: i64) -> bool;
    /// La carte réseau de ce numéro (`Get-NetAdapter -IncludeHidden`), `None` si elle n'existe pas
    fn carte(&self, index: i32) -> Option<Carte>;
    /// Les adresses du réseau local qu'un DNS peut avoir sur cette carte : sa passerelle (la box) et les serveurs DNS
    /// donnés par la box (`Get-AideDnsLocaux`)
    fn dns_locaux(&self, carte: &Carte) -> Vec<String>;
    /// Les serveurs DNS d'origine que l'Aide a retenus pour cette carte, `None` si aucun
    fn dns_origine(&self, guid: &str) -> Option<String>;
    /// Les antivirus et les services (une erreur si Windows ne répond pas)
    fn contexte(&self) -> Result<ContexteProgrammes, String>;
    /// Un processus de ce programme est-il protégé par Windows (moteur d'antivirus) ? (niveau illisible : oui)
    fn processus_protege(&self, chemin: &str) -> bool;
}

/// Type de démarrage d'un service
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Demarrage {
    Desactive,
    Manuel,
    Automatique,
}

/// Une demande vérifiée, prête à être faite
#[derive(Clone, Debug)]
pub enum Demande {
    Bonjour,
    RegEcrire {
        cle: String,
        nom: String,
        valeur: Valeur,
    },
    RegEffacer {
        cle: String,
        nom: String,
    },
    ServiceRegler {
        nom: String,
        demarrage: Demarrage,
        lancer: bool,
        arreter: bool,
    },
    DnsRegler {
        carte: Carte,
        serveurs: Vec<String>,
        origine_connue: bool,
    },
    PareFeuBloquer {
        nom: String,
        programme: String,
        services: Vec<String>,
    },
    /// `suite` : ce qui suit « Nevermind : bloque » (les deux noms, ancien et nouveau, sont retirés)
    PareFeuDebloquer {
        suite: String,
    },
    OriginesOublier,
    PareFeuDebloquerTout,
    PareFeuActiver,
    RestaurationCreer {
        description: String,
    },
    AntivirusRetirerExclusions {
        elements: Vec<String>,
    },
    LecturePc,
    LectureSecurite,
    LectureTaches,
    LectureChemins {
        pids: Vec<i64>,
    },
    WifiManuel {
        profils: Vec<String>,
    },
    Smb1Couper {
        client: bool,
    },
    NettoyageLister {
        categorie: Categorie,
        top: usize,
    },
    NettoyageLancer {
        categorie: Categorie,
    },
    /// Par nom (programme gourmand pendant une partie) ou par chemin (programme bloqué dans Trafic)
    ProcessusFermer {
        nom: String,
        chemin: Option<String>,
        contexte: ContexteProgrammes,
    },
    FpsDemarrer {
        pid: i64,
    },
    FpsVeiller {
        noms: Vec<String>,
    },
    FpsLire,
    FpsArreter,
    TraficTcp,
    FluxDemarrer,
    FluxVeiller,
    FluxLire,
    FluxArreter,
}

/// Les DNS connus que Nevermind propose
const DNS_CHOIX: [[&str; 2]; 3] = [["1.1.1.1", "1.0.0.1"], ["8.8.8.8", "8.8.4.4"], ["9.9.9.9", "149.112.112.112"]];

/// Une adresse IP lue comme Windows l'écrit (sans « %zone »), `None` si ce n'en est pas une, ou 0.0.0.0 et ::
/// (`ConvertTo-AideIp`)
pub fn ip_normale(t: &str) -> Option<String> {
    let t = texte::sans_espaces(t);
    let sans_zone = if t.contains(':') { t.split('%').next().unwrap_or(t) } else { t };
    let ip: IpAddr = sans_zone.parse().ok()?;
    let s = ip.to_string();
    if s == "0.0.0.0" || s == "::" { None } else { Some(s) }
}

/// Une adresse du réseau local (10/8, 172.16/12, 192.168/16, fe80::/10, fc00::/7 ; `Test-AideAdresseLocale`)
pub fn adresse_locale(t: &str) -> bool {
    let sans_zone = if t.contains(':') { t.split('%').next().unwrap_or(t) } else { t };
    match sans_zone.parse::<IpAddr>() {
        Ok(IpAddr::V4(a)) => {
            let o = a.octets();
            o[0] == 10 || (o[0] == 172 && (16..=31).contains(&o[1])) || (o[0] == 192 && o[1] == 168)
        }
        Ok(IpAddr::V6(a)) => {
            let o = a.octets();
            (o[0] == 0xfe && (o[1] & 0xc0) == 0x80) || (o[0] & 0xfe) == 0xfc
        }
        Err(_) => false,
    }
}

/// Un programme qu'on peut fermer ou bloquer (`Get-AideProgramme`) : un .exe qui existe, hors de Windows (sauf
/// `windows_permis`) et de l'antivirus. Rend son chemin complet.
pub fn programme(chemin_demande: &str, windows_permis: bool, pc: &dyn Pc) -> Result<String, String> {
    let c = chemin_demande;
    if c.is_empty()
        || !chemin::enracine(c)
        || !texte::finit(c, ".exe")
        || c.chars().any(|x| (x as u32) < 0x20 || "*?\"<>|".contains(x))
    {
        return Err(format!("Demande refusée : programme non valable ({c})."));
    }
    let plein = match chemin::complet(c) {
        Some(p) if texte::egal(&p, c) => p,
        _ => return Err(format!("Demande refusée : chemin non valable ({c}).")),
    };
    if !pc.fichier_existe(&plein) {
        return Err(format!("Ce programme n'existe plus : {plein}"));
    }
    let nom = texte::nom_fichier(&plein).to_string();
    if exes::protege(&nom) {
        return Err(format!("Demande refusée : {nom} fait partie de Windows ou de l'antivirus."));
    }
    let d = pc.dossiers();
    let defender = [
        format!("{}\\Windows Defender\\", d.programmes),
        format!("{}\\Windows Defender\\", d.programmes_x86),
        format!("{}\\Microsoft\\Windows Defender\\", d.donnees_communes),
        format!("{}\\Windows Defender Advanced Threat Protection\\", d.programmes),
        format!("{}\\Microsoft\\Windows Defender Advanced Threat Protection\\", d.donnees_communes),
    ];
    if defender.iter().any(|x| texte::commence(&plein, x)) {
        return Err(format!("Demande refusée : {nom} fait partie de l'antivirus."));
    }
    if !windows_permis && d.dans_windows(&plein) {
        return Err(format!("Demande refusée : {nom} fait partie de Windows."));
    }
    Ok(plein)
}

/// Pourquoi ce programme ne peut pas être fermé ni bloqué (`Get-AideRefusProgramme`), `None` s'il peut l'être.
/// `service_permis` : svchost.exe pour un service précis (la règle du pare-feu ne vise que ce service).
pub fn refus_programme(
    ctx: &ContexteProgrammes,
    chemin: &str,
    service_permis: bool,
    pc: &dyn Pc,
) -> Option<&'static str> {
    if dossiers::est_antivirus(&ctx.antivirus, chemin, &pc.dossiers().windows) {
        return Some("est un programme de ton antivirus");
    }
    if service_permis {
        return None;
    }
    if ctx.services_exe.iter().any(|s| texte::egal(s, chemin)) {
        return Some("fait tourner un service (un programme qui tourne en fond pour Windows ou un logiciel)");
    }
    if pc.processus_protege(chemin) {
        return Some("est protégé par Windows (antivirus)");
    }
    None
}

/// Un nom de règle du pare-feu créée par Nevermind : ce qui suit « Nevermind : bloque » (ou l'ancien
/// « OptiGame : bloque »)
fn suite_regle(nom: &str) -> Option<&str> {
    texte::reste_apres(nom, "Nevermind : bloque ").or_else(|| texte::reste_apres(nom, "OptiGame : bloque "))
}

/// Un nom de programme pour `processus.fermer` (`^[\w .()+\-]{1,100}$`)
fn nom_programme_valable(n: &str) -> bool {
    (1..=100).contains(&n.chars().count()) && n.chars().all(|c| texte::lettre_de_mot(c) || " .()+-".contains(c))
}

/// Un nom de programme guetté par `fps.veiller` (`^[a-z0-9][a-z0-9 ._()-]{0,60}\.exe$`, déjà en minuscules)
fn nom_jeu_valable(n: &str) -> bool {
    let Some(debut) = n.strip_suffix(".exe") else {
        return false;
    };
    let mut c = debut.chars();
    let premier_ok = c.next().is_some_and(|x| x.is_ascii_lowercase() || x.is_ascii_digit());
    premier_ok
        && debut.chars().count() <= 61
        && c.all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || " ._()-".contains(x))
}

/// Vérifie la demande `action` (paramètres `p`) sans rien faire
pub fn verifier(action: &str, p: &Value, pc: &dyn Pc) -> Result<Demande, String> {
    let a = |n: &str| texte::egal(action, n);
    let sans_parametre = [
        ("aide.bonjour", Demande::Bonjour),
        ("origines.oublier", Demande::OriginesOublier),
        ("pare-feu.debloquer-tout", Demande::PareFeuDebloquerTout),
        ("pare-feu.activer", Demande::PareFeuActiver),
        ("lecture.pc", Demande::LecturePc),
        ("lecture.securite", Demande::LectureSecurite),
        ("lecture.taches", Demande::LectureTaches),
        ("fps.lire", Demande::FpsLire),
        ("fps.arreter", Demande::FpsArreter),
        ("trafic.tcp", Demande::TraficTcp),
        ("flux.demarrer", Demande::FluxDemarrer),
        ("flux.veiller", Demande::FluxVeiller),
        ("flux.lire", Demande::FluxLire),
        ("flux.arreter", Demande::FluxArreter),
    ];
    if let Some((_, d)) = sans_parametre.into_iter().find(|(n, _)| a(n)) {
        return Ok(d);
    }

    if a("reg.ecrire") {
        let cle = registre::cle_normale(&ps::chaine(ps::param(p, "Cle")))?;
        let nom = ps::chaine(ps::param(p, "Nom"));
        let r = registre::regle(&cle, &nom)?;
        let valeur = registre::valeur(r, ps::param(p, "Valeur"), &ps::chaine(ps::param(p, "Type")))?;
        // Programmes lancés avec Windows : jamais couper Sécurité Windows ni un antivirus ; les remettre actifs reste
        // permis (StartupApproved : premier octet impair = coupé, 2 ou 6 = actif)
        let coupe = match &valeur {
            Valeur::Binaire(o) => o.first().is_none_or(|x| x & 1 == 1),
            Valeur::DWord(v) => v & 1 == 1,
        };
        if r.demarrage()
            && texte::contient(&cle, "\\StartupApproved\\")
            && coupe
            && demarrage::securite(&cle, &nom, None, None, pc.dossiers(), pc)
        {
            return Err(format!(
                "Demande refusée : {nom} protège ton PC (Sécurité Windows ou antivirus), l'Aide ne le coupe jamais au démarrage."
            ));
        }
        return Ok(Demande::RegEcrire { cle, nom, valeur });
    }

    if a("reg.effacer") {
        let cle = registre::cle_normale(&ps::chaine(ps::param(p, "Cle")))?;
        let nom = ps::chaine(ps::param(p, "Nom"));
        let r = registre::regle(&cle, &nom)?;
        if !r.effacer {
            return Err(format!("L'Aide garde ce réglage : {}.", r.garde.unwrap_or_default()));
        }
        return Ok(Demande::RegEffacer { cle, nom });
    }

    if a("service.regler") {
        let nom = ps::chaine(ps::param(p, "Nom"));
        let diagtrack = texte::egal(&nom, "DiagTrack");
        if !diagtrack && !exes::service_memoire(&nom) {
            return Err(format!("Demande refusée : le service {nom} n'est pas dans la liste de l'Aide."));
        }
        let dem = ps::chaine(ps::param(p, "Demarrage"));
        let demarrage = if texte::egal(&dem, "Disabled") {
            Demarrage::Desactive
        } else if texte::egal(&dem, "Manual") {
            Demarrage::Manuel
        } else if texte::egal(&dem, "Automatic") {
            Demarrage::Automatique
        } else {
            return Err(format!("Demande refusée : type de démarrage {dem}."));
        };
        // « Désactivé » : seulement la télémétrie. Les services de la page RAM passent en « Manuel », jamais plus : un
        // service de mise à jour désactivé couperait pour de bon les mises à jour du navigateur.
        if demarrage == Demarrage::Desactive && !diagtrack {
            return Err(format!(
                "Demande refusée : l'Aide ne désactive pas {nom} (seulement « Manuel »), ses mises à jour doivent continuer."
            ));
        }
        let lancer = ps::vrai(ps::param(p, "Lancer"));
        let arreter = ps::vrai(ps::param(p, "Arreter"));
        return Ok(Demande::ServiceRegler { nom, demarrage, lancer, arreter });
    }

    if a("dns.regler") {
        let index = ps::entier(ps::param(p, "Index"))?;
        let serveurs = ps::liste(p, "Serveurs");
        if serveurs.len() > 4 {
            return Err("Demande refusée : trop de serveurs DNS.".into());
        }
        if index <= 0 {
            return Err("Demande refusée : carte réseau non valable.".into());
        }
        let carte = pc.carte(index).ok_or("La carte réseau de ce réglage n'existe plus sur ce PC.")?;
        let texte_serveurs = serveurs.join(",");
        let mut connu = serveurs.is_empty() || DNS_CHOIX.iter().any(|c| texte::egal(&c.join(","), &texte_serveurs));
        if !connu {
            // Ceux de la box : des adresses privées, et seulement la passerelle de cette carte ou les DNS que la box donne
            let locaux = pc.dns_locaux(&carte);
            connu = serveurs.iter().all(|s| {
                adresse_locale(s) && ip_normale(s).is_some_and(|ip| locaux.iter().any(|l| texte::egal(l, &ip)))
            });
        }
        let origine = pc.dns_origine(&carte.guid);
        if !connu && let Some(o) = &origine {
            let o: Vec<&str> = o.split([',', ' ', '\t', '\r', '\n']).filter(|x| !x.is_empty()).collect();
            connu = texte::egal(&o.join(","), &texte_serveurs);
        }
        if !connu {
            return Err("L'Aide garde ce réglage : elle ne met que des DNS connus (Cloudflare, Google, Quad9), ceux de ta box, ou ceux que tu avais avant Nevermind.".into());
        }
        return Ok(Demande::DnsRegler { carte, serveurs, origine_connue: origine.is_some() });
    }

    if a("pare-feu.bloquer") {
        let nom = ps::texte_requis(p, "Nom", 200)?;
        if !suite_regle(&nom).is_some_and(|s| !s.is_empty() && !s.contains(['*', '?', '[', ']'])) {
            return Err("Demande refusée : nom de règle non valable.".into());
        }
        let services = ps::liste(p, "Services");
        for s in &services {
            let forme = (1..=80).contains(&s.chars().count())
                && s.chars().all(|c| texte::lettre_de_mot(c) || c == '.' || c == '-');
            if !forme {
                return Err(format!("Demande refusée : service {s} non valable."));
            }
            if exes::service_protege(s) {
                return Err(format!("Demande refusée : le service {s} est indispensable à Windows ou à l'antivirus."));
            }
        }
        let prog = programme(&ps::chaine(ps::param(p, "Programme")), true, pc)?;
        let court = texte::nom_fichier(&prog).to_string();
        let pour_service = texte::egal(&court, "svchost.exe") && !services.is_empty();
        if exes::protege(&court) && !pour_service {
            return Err(format!("Demande refusée : {court} est indispensable à Windows ou à l'antivirus."));
        }
        let ctx = pc.contexte()?;
        if let Some(r) = refus_programme(&ctx, &prog, pour_service, pc) {
            return Err(format!("Demande refusée : {court} {r}."));
        }
        return Ok(Demande::PareFeuBloquer { nom, programme: prog, services });
    }

    if a("pare-feu.debloquer") {
        let nom = ps::texte_requis(p, "Nom", 200)?;
        let Some(suite) = suite_regle(&nom) else {
            return Err("Demande refusée : seules les règles de Nevermind peuvent être retirées.".into());
        };
        return Ok(Demande::PareFeuDebloquer { suite: suite.to_string() });
    }

    if a("restauration.creer") {
        let description = ps::chaine(ps::param(p, "Description"));
        if !["Nevermind", "Nevermind (manuel)"].iter().any(|d| texte::egal(d, &description)) {
            return Err("Demande refusée : description non valable.".into());
        }
        return Ok(Demande::RestaurationCreer { description });
    }

    if a("antivirus.retirer-exclusions") {
        let elements = ps::liste(p, "Elements");
        if elements.is_empty()
            || elements.len() > 200
            || elements.iter().any(|x| x.is_empty() || texte::longueur(x) > 1000 || texte::a_controle(x))
        {
            return Err("Demande refusée : liste d'exclusions non valable.".into());
        }
        return Ok(Demande::AntivirusRetirerExclusions { elements });
    }

    if a("lecture.chemins") {
        let ids = ps::liste(p, "Pids");
        let forme = |x: &str| (1..=9).contains(&x.len()) && x.bytes().all(|b| b.is_ascii_digit());
        if ids.is_empty() || ids.len() > 500 || !ids.iter().all(|x| forme(x)) {
            return Err("Demande refusée : liste de processus non valable.".into());
        }
        let pids = ids.iter().filter_map(|x| x.parse().ok()).collect();
        return Ok(Demande::LectureChemins { pids });
    }

    if a("wifi.manuel") {
        let profils = ps::liste(p, "Profils");
        if profils.is_empty()
            || profils.len() > 50
            || profils.iter().any(|x| {
                x.is_empty() || texte::longueur(x) > 64 || x.contains('"') || texte::a_controle(x) || x.ends_with('\\')
            })
        {
            return Err("Demande refusée : noms de Wi-Fi non valables.".into());
        }
        return Ok(Demande::WifiManuel { profils });
    }

    if a("smb1.couper") {
        return Ok(Demande::Smb1Couper { client: ps::vrai(ps::param(p, "Client")) });
    }

    if a("nettoyage.lister") || a("nettoyage.lancer") {
        let id = ps::chaine(ps::param(p, "Id"));
        let Some(categorie) = nettoyage::categorie(&id) else {
            return Err(format!("Demande refusée : catégorie de nettoyage {id} inconnue."));
        };
        if a("nettoyage.lancer") {
            return Ok(Demande::NettoyageLancer { categorie });
        }
        let top = ps::entier(ps::param(p, "Top"))?;
        let top = if (0..=1000).contains(&top) { top as usize } else { 300 };
        return Ok(Demande::NettoyageLister { categorie, top });
    }

    if a("processus.fermer") {
        let mut nom = texte::sans_espaces(&ps::chaine(ps::param(p, "Nom"))).to_string();
        let chemin_demande = ps::chaine(ps::param(p, "Chemin"));
        let mut chemin = None;
        if !chemin_demande.is_empty() {
            let c = programme(&chemin_demande, false, pc)?;
            nom = texte::nom_sans_extension(&c).to_string();
            chemin = Some(c);
        } else if !nom_programme_valable(&nom) {
            return Err("Demande refusée : nom de programme non valable.".into());
        }
        if exes::protege(&nom) {
            return Err(format!("Demande refusée : {nom} fait partie de Windows ou de l'antivirus."));
        }
        let contexte = pc.contexte()?;
        if let Some(c) = &chemin
            && let Some(r) = refus_programme(&contexte, c, false, pc)
        {
            return Err(format!("Demande refusée : {} {r}.", texte::nom_fichier(c)));
        }
        return Ok(Demande::ProcessusFermer { nom, chemin, contexte });
    }

    if a("fps.demarrer") {
        let pid = ps::lire_entier(&ps::chaine(ps::param(p, "Pid"))).filter(|n| i32::try_from(*n).is_ok());
        let Some(pid) = pid.filter(|&n| n > 4) else {
            return Err("Demande refusée : processus non valable.".into());
        };
        if !pc.processus_existe(pid) {
            return Err("Le jeu n'est plus lancé.".into());
        }
        return Ok(Demande::FpsDemarrer { pid });
    }

    if a("fps.veiller") {
        let mut noms: Vec<String> = Vec::new();
        for n in ps::liste(p, "Noms") {
            let n = texte::minuscules(&n);
            if !noms.contains(&n) {
                noms.push(n);
            }
        }
        if noms.is_empty() || noms.len() > 10 || !noms.iter().all(|n| nom_jeu_valable(n)) {
            return Err("Demande refusée : noms de programmes non valables.".into());
        }
        return Ok(Demande::FpsVeiller { noms });
    }

    Err(format!("L'Aide ne connaît pas la demande « {action} »."))
}

/// Un message d'erreur est-il un vrai refus de l'Aide (et non une erreur quelconque) ?
pub fn est_un_refus(message: &str) -> bool {
    ["Demande refusée", "L'Aide garde ce réglage", "L'Aide ne connaît pas"].iter().any(|d| message.contains(d))
}
