//! Les règles de la page Sécurité, sans Windows : les points à vérifier (`Get-SecurityChecks`), la note sur 100
//! (`Get-ProtectionScore`), l'état résumé (`Get-EtatSecuriteKit`), les menaces récentes (`Get-ThreatHistory`), la note
//! des analyses (`Update-SecurityTab`), la lecture de l'Aide (`Add-AideLectureSecurite`), le tri des tâches planifiées
//! (`$SecDataWork`), les menaces d'une analyse (`Get-ScanThreats`) et le message de « Désactiver ces tâches ».

use crate::etat::{Approuve, Demarrage, Detection, Environnement, EtatS, Menace, Tache};
use crate::ps::{self, Date};
use serde_json::Value;
use std::collections::HashSet;

/// Ce que fait un bouton d'un point à vérifier
#[derive(Debug, Clone, PartialEq)]
pub enum Sorte {
    SupprimerMenaces,
    /// `Open-Url` (une page de Sécurité Windows, des paramètres)
    Ouvrir(String),
    MettreAJourBase,
    RetirerExclusions(Vec<String>),
    ReactiverPareFeu,
    ReactiverUac,
    /// Le fichier hosts dans le Bloc-notes
    OuvrirFichier(String),
    Analyser {
        chemins: Vec<String>,
        titre: String,
    },
    /// L'Explorateur, l'élément sélectionné
    OuvrirEmplacement(String),
    DesactiverDemarrage(Vec<Demarrage>),
    DesactiverTaches(Vec<Tache>),
    Planificateur,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Geste {
    pub texte: String,
    pub confirmer: Option<String>,
    pub sans_rafraichir: bool,
    pub sorte: Sorte,
}

fn geste(texte: &str, sorte: Sorte) -> Geste {
    Geste { texte: texte.into(), confirmer: None, sans_rafraichir: false, sorte }
}
fn geste_confirme(texte: &str, confirmer: &str, sorte: Sorte) -> Geste {
    Geste { texte: texte.into(), confirmer: Some(confirmer.into()), sans_rafraichir: false, sorte }
}

/// Un point à vérifier
#[derive(Debug, Clone, PartialEq)]
pub struct Constat {
    /// "bad" ou "warn"
    pub statut: &'static str,
    pub titre: String,
    pub detail: String,
    /// `Items` (absent : `None`)
    pub elements: Option<Vec<String>>,
    /// `Trust` : ce que « C'est moi » approuve
    pub confiance: Vec<Approuve>,
    pub actions: Vec<Geste>,
}

impl Constat {
    fn nouveau(statut: &'static str, titre: String, detail: &str, actions: Vec<Geste>) -> Constat {
        Constat { statut, titre, detail: detail.into(), elements: None, confiance: Vec::new(), actions }
    }
    pub fn grave(&self) -> bool {
        self.statut == "bad"
    }
    /// `if ($c.Items)`
    pub fn a_elements(&self) -> bool {
        match &self.elements {
            None => false,
            Some(l) => match l.len() {
                0 => false,
                1 => !l[0].is_empty(),
                _ => true,
            },
        }
    }
}

fn s(n: usize) -> &'static str {
    if n > 1 { "s" } else { "" }
}

/// Les clés approuvées (table de PowerShell : sans tenir compte des majuscules)
pub struct Approuves(HashSet<String>);

impl Approuves {
    pub fn de(liste: &[Approuve]) -> Approuves {
        Approuves(liste.iter().map(|a| a.cle.to_lowercase()).collect())
    }
    pub fn contient(&self, cle: &str) -> bool {
        self.0.contains(&cle.to_lowercase())
    }
}

/// Les lignes du fichier hosts qui redirigent quelque chose (ni commentaire, ni localhost)
pub fn lignes_hosts(lignes: &[String]) -> Vec<String> {
    lignes
        .iter()
        .map(|l| trim_ps(l).to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !hote_local(l) && !boucle_localhost(l))
        .collect()
}

/// `String.Trim()` de .NET (espaces Unicode)
fn trim_ps(t: &str) -> &str {
    t.trim_matches(|c: char| c.is_whitespace())
}

/// `-match '\s(localhost|localhost\.localdomain|broadcasthost)$'`
fn hote_local(l: &str) -> bool {
    let b = l.to_lowercase();
    ["localhost", "localhost.localdomain", "broadcasthost"]
        .iter()
        .any(|n| b.strip_suffix(n).and_then(|d| d.chars().last()).is_some_and(char::is_whitespace))
}

/// `-match '^(127\.0\.0\.1|::1)\s+localhost'`
fn boucle_localhost(l: &str) -> bool {
    let b = l.to_lowercase();
    ["127.0.0.1", "::1"].iter().any(|d| {
        b.strip_prefix(d).is_some_and(|r| {
            let t = r.trim_start_matches(char::is_whitespace);
            t.len() < r.len() && t.starts_with("localhost")
        })
    })
}

/// `Get-SecurityChecks` : les points à vérifier, dans l'ordre de la version PowerShell
pub fn constats(e: &EtatS, env: &Environnement, approuves: &[Approuve], hosts: &str) -> Vec<Constat> {
    let ok = Approuves::de(approuves);
    let mut l = Vec::new();

    // Antivirus
    match &e.mp {
        Some(mp) if mp.normal() => {
            if !mp.temps_reel {
                l.push(Constat::nouveau(
                    "bad",
                    "Protection en temps réel désactivée".into(),
                    "Les virus ne sont plus bloqués au moment où ils arrivent. Réactive la protection.",
                    vec![geste("Ouvrir la protection", Sorte::Ouvrir("windowsdefender://threatsettings".into()))],
                ));
            }
            if mp.age_base > 3 {
                l.push(Constat::nouveau(
                    "warn",
                    format!("Base de virus vieille de {} jours", mp.age_base),
                    "Les nouveaux virus ne sont pas encore connus de ton antivirus. Mets la base à jour.",
                    vec![geste("Mettre à jour", Sorte::MettreAJourBase)],
                ));
            }
        }
        _ if e.autres_av.is_empty() => l.push(Constat::nouveau(
            "bad",
            "Aucun antivirus actif détecté".into(),
            "Ton PC n'est pas protégé. Active Microsoft Defender dans Sécurité Windows.",
            vec![geste("Ouvrir Sécurité Windows", Sorte::Ouvrir("windowsdefender://threat".into()))],
        )),
        _ => {}
    }

    // Exclusions de l'antivirus
    let excl: Vec<String> = e.exclusions.iter().filter(|x| !ok.contient(&format!("exclusion|{x}"))).cloned().collect();
    if !excl.is_empty() {
        let n = excl.len();
        let confiance: Vec<Approuve> = excl
            .iter()
            .map(|x| Approuve::nouveau("Exclusion de l'antivirus", format!("exclusion|{x}"), x.clone()))
            .collect();
        let mut c = Constat::nouveau(
            "warn",
            format!("{n} élément{} exclu{} de l'antivirus", s(n), s(n)),
            "Ces dossiers ou programmes ne sont jamais analysés. Si tu ne les as pas ajoutés toi-même (ou un jeu que tu connais), c'est suspect : les virus s'ajoutent souvent eux-mêmes en exclusion.",
            vec![geste_confirme(
                "Retirer les exclusions",
                "Retirer ces exclusions de Microsoft Defender ? Ces éléments seront de nouveau analysés. Ce changement protège ton PC : « Tout restaurer » ne l'annulera pas (la façon de revenir en arrière est notée dans l'historique).",
                Sorte::RetirerExclusions(excl.clone()),
            )],
        );
        c.elements = Some(confiance.iter().map(|t| t.texte.clone()).collect());
        c.confiance = confiance;
        l.push(c);
    }
    let nb = e.exclusions_nombre;
    if excl.is_empty() && nb > 0 && !ok.contient(&format!("exclusions|{nb}")) {
        let n = nb as usize;
        let mut c = Constat::nouveau(
            "warn",
            format!("{nb} élément{} exclu{} de l'antivirus", s(n), s(n)),
            "Ces dossiers ou programmes ne sont jamais analysés. Si tu ne les as pas ajoutés toi-même (ou un jeu que tu connais), c'est suspect : les virus s'ajoutent souvent eux-mêmes en exclusion. Regarde-les dans Sécurité Windows, Protection contre les virus et menaces, « Gérer les paramètres », Exclusions.",
            vec![Geste {
                texte: "Voir les exclusions".into(),
                confirmer: None,
                sans_rafraichir: true,
                sorte: Sorte::Ouvrir("windowsdefender://threatsettings".into()),
            }],
        );
        c.confiance = vec![Approuve::nouveau(
            "Exclusions de l'antivirus",
            format!("exclusions|{nb}"),
            format!("{nb} exclusion{} de l'antivirus", s(n)),
        )];
        l.push(c);
    }

    // Pare-feu et contrôle des comptes
    if !e.pare_feu() {
        l.push(Constat::nouveau(
            "bad",
            "Pare-feu désactivé".into(),
            &format!(
                "Le pare-feu de Windows est coupé ({}) : ton PC est exposé sur le réseau.",
                e.pare_feu_coupe.join(", ")
            ),
            vec![geste_confirme(
                "Réactiver le pare-feu",
                "Réactiver le pare-feu de Windows sur tous les réseaux ? Ce changement protège ton PC : « Tout restaurer » ne l'annulera pas (la façon de revenir en arrière est notée dans l'historique).",
                Sorte::ReactiverPareFeu,
            )],
        ));
    }
    if !e.uac {
        l.push(Constat::nouveau(
            "bad",
            "Contrôle des comptes désactivé".into(),
            "Les programmes peuvent modifier Windows sans te demander la permission. Il faut le réactiver (redémarrage nécessaire).",
            vec![geste_confirme(
                "Réactiver",
                "Réactiver le contrôle des comptes ? Un redémarrage sera nécessaire.",
                Sorte::ReactiverUac,
            )],
        ));
    }

    // Fichier hosts
    let lignes: Vec<String> = env
        .hosts
        .as_deref()
        .map(lignes_hosts)
        .unwrap_or_default()
        .into_iter()
        .filter(|x| !ok.contient(&format!("hosts|{x}")))
        .collect();
    if !lignes.is_empty() {
        let n = lignes.len();
        let confiance: Vec<Approuve> = lignes
            .iter()
            .map(|x| Approuve::nouveau("Ligne du fichier hosts", format!("hosts|{x}"), x.clone()))
            .collect();
        let mut c = Constat::nouveau(
            "warn",
            format!("Fichier hosts modifié ({n} ligne{})", s(n)),
            "Ce fichier peut rediriger des sites vers d'autres adresses. Des logiciels l'utilisent pour bloquer la pub, mais un virus peut s'en servir pour t'envoyer vers de faux sites.",
            vec![geste("Ouvrir le fichier", Sorte::OuvrirFichier(hosts.into()))],
        );
        c.elements = Some(confiance.iter().map(|t| t.texte.clone()).collect());
        c.confiance = confiance;
        l.push(c);
    }

    // Proxy
    let actif = match &env.proxy_actif {
        Value::Number(n) => n.as_f64() == Some(1.0),
        Value::String(t) => t == "1",
        Value::Bool(b) => *b,
        _ => false,
    };
    let pac = ps::texte(&env.proxy_pac);
    if actif || !pac.is_empty() {
        let quoi = if pac.is_empty() { ps::texte(&env.proxy_serveur) } else { format!("script {pac}") };
        if !ok.contient(&format!("proxy|{quoi}")) {
            let mut c = Constat::nouveau(
                "warn",
                "Un proxy détourne ta navigation".into(),
                &format!(
                    "Tout ton trafic web passe par : {quoi}. Si tu ne l'as pas configuré toi-même (VPN, travail), c'est suspect."
                ),
                vec![geste("Paramètres du proxy", Sorte::Ouvrir("ms-settings:network-proxy".into()))],
            );
            c.confiance = vec![Approuve::nouveau("Proxy", format!("proxy|{quoi}"), quoi.clone())];
            l.push(c);
        }
    }

    // Fichiers piégés (double extension, scripts) dans les dossiers de l'utilisateur
    let sans = |liste: &[String]| -> Vec<String> {
        liste.iter().filter(|x| !ok.contient(&format!("fichier|{x}"))).cloned().collect()
    };
    let fichiers = |liste: &[String], sorte: &str| -> Vec<Approuve> {
        liste.iter().map(|x| Approuve::nouveau(sorte, format!("fichier|{x}"), x.clone())).collect()
    };
    let analyser = |liste: &[String], titre: &str| -> Vec<Geste> {
        vec![
            geste("Analyser avec Defender", Sorte::Analyser { chemins: liste.to_vec(), titre: titre.into() }),
            geste("Ouvrir l'emplacement", Sorte::OuvrirEmplacement(liste[0].clone())),
        ]
    };
    let doubles = sans(&e.doubles);
    let scripts = sans(&e.scripts);
    if !doubles.is_empty() {
        let n = doubles.len();
        let mut c = Constat::nouveau(
            "bad",
            format!("{n} fichier{} déguisé{}", s(n), s(n)),
            "Un nom comme « facture.pdf.exe » fait croire à un document, mais c'est un programme. C'est la ruse la plus courante des virus : ne l'ouvre surtout pas.",
            analyser(&doubles, "Analyse des fichiers déguisés"),
        );
        c.confiance = fichiers(&doubles, "Fichier");
        c.elements = Some(doubles);
        l.push(c);
    }
    if !scripts.is_empty() {
        let n = scripts.len();
        let mut c = Constat::nouveau(
            "warn",
            format!("{n} script{} dans tes téléchargements", s(n)),
            "Ces types de fichiers (.vbs, .js, .hta, .scr...) servent rarement à autre chose qu'à installer des virus quand ils viennent d'Internet. Si tu ne sais pas d'où ils viennent, supprime-les.",
            analyser(&scripts, "Analyse des scripts téléchargés"),
        );
        c.confiance = fichiers(&scripts, "Fichier");
        c.elements = Some(scripts);
        l.push(c);
    }

    // Programmes non signés cachés dans des dossiers à risque
    let caches = sans(&e.caches);
    if !caches.is_empty() {
        let n = caches.len();
        let mut c = Constat::nouveau(
            "warn",
            format!("{n} programme{} non signé{} dans des dossiers à risque", s(n), s(n)),
            "Ces programmes sont posés directement dans des dossiers où les virus aiment se cacher, et aucun éditeur ne les a signés. Ce n'est pas forcément grave (vieux installeurs...), mais ça vaut une analyse.",
            analyser(&caches, "Analyse des programmes cachés"),
        );
        c.confiance = fichiers(&caches, "Programme");
        c.elements = Some(caches);
        l.push(c);
    }

    // Programmes au démarrage placés dans des dossiers à risque ou non signés
    let suspects: Vec<String> = e.demarrage_suspect.iter().map(|x| x.to_lowercase()).collect();
    let dem: Vec<Demarrage> = env
        .demarrage
        .iter()
        .filter(|d| {
            d.actif
                && suspects.contains(&d.exe.to_lowercase())
                && !ok.contient(&format!("demarrage|{}|{}", d.nom, d.exe))
        })
        .cloned()
        .collect();
    if !dem.is_empty() {
        let n = dem.len();
        let confiance: Vec<Approuve> = dem
            .iter()
            .map(|d| {
                Approuve::nouveau(
                    "Programme au démarrage",
                    format!("demarrage|{}|{}", d.nom, d.exe),
                    format!("{}  :  {}", d.nom, d.exe),
                )
            })
            .collect();
        let mut c = Constat::nouveau(
            "warn",
            format!("{n} programme{} au démarrage à vérifier", s(n)),
            "Ils se lancent avec Windows et ne sont pas signés par un éditeur, ou sont rangés dans un dossier inhabituel. Si tu ne les reconnais pas, analyse-les et désactive-les.",
            vec![
                geste(
                    "Analyser avec Defender",
                    Sorte::Analyser {
                        chemins: dem.iter().map(|d| d.exe.clone()).collect(),
                        titre: "Analyse des programmes au démarrage".into(),
                    },
                ),
                geste_confirme(
                    "Désactiver au démarrage",
                    "Désactiver ces programmes au démarrage ? Ils restent installés.",
                    Sorte::DesactiverDemarrage(dem.clone()),
                ),
            ],
        );
        c.elements = Some(confiance.iter().map(|t| t.texte.clone()).collect());
        c.confiance = confiance;
        l.push(c);
    }

    // Tâches planifiées suspectes
    let taches: Vec<Tache> =
        e.taches.iter().filter(|t| !ok.contient(&format!("tache|{}{}|{}", t.chemin, t.nom, t.cmd))).cloned().collect();
    if !taches.is_empty() {
        let n = taches.len();
        let confiance: Vec<Approuve> = taches
            .iter()
            .map(|t| {
                Approuve::nouveau(
                    "Tâche planifiée",
                    format!("tache|{}{}|{}", t.chemin, t.nom, t.cmd),
                    format!("{}  :  {}", t.nom, t.cmd),
                )
            })
            .collect();
        let grave = taches.iter().any(|t| t.grave);
        let mut c = Constat::nouveau(
            if grave { "bad" } else { "warn" },
            format!("{n} tâche{} planifiée{} suspecte{}", s(n), s(n), s(n)),
            "Ces tâches lancent en cachette des commandes qui téléchargent ou exécutent du code, ou des programmes rangés dans des dossiers temporaires. C'est une technique classique des virus pour revenir après chaque redémarrage.",
            vec![
                geste_confirme(
                    "Désactiver ces tâches",
                    "Désactiver ces tâches planifiées ? Tu pourras les réactiver dans le Planificateur de tâches.",
                    Sorte::DesactiverTaches(taches.clone()),
                ),
                geste("Planificateur de tâches", Sorte::Planificateur),
            ],
        );
        c.elements = Some(confiance.iter().map(|t| t.texte.clone()).collect());
        c.confiance = confiance;
        l.push(c);
    }
    l
}

/// Les points de la page (Get-PagesProtection) : les menaces actives d'abord, puis les graves, puis les autres
pub fn points_page(e: &EtatS, constats: &[Constat]) -> Vec<Constat> {
    let mut l = Vec::new();
    let actives = e.actives();
    if !actives.is_empty() {
        let n = actives.len();
        let mut c = Constat::nouveau(
            "bad",
            format!("{n} menace{} toujours active{}", s(n), s(n)),
            "Microsoft Defender a trouvé des virus qui ne sont pas encore supprimés.",
            vec![geste_confirme(
                "Supprimer les menaces",
                "Supprimer toutes les menaces actives trouvées par Microsoft Defender ?",
                Sorte::SupprimerMenaces,
            )],
        );
        c.elements = Some(actives.iter().map(|m| m.nom.clone()).collect());
        l.push(c);
    }
    // (Sort-Object de Windows PowerShell ne garde pas l'ordre des égaux : son tri est refait à l'identique)
    let mut tri: Vec<&Constat> = constats.iter().collect();
    tri_dotnet(&mut tri, &|c: &&Constat| if c.grave() { 0 } else { 1 });
    l.extend(tri.into_iter().cloned());
    l
}

/// Le tri rapide de .NET Framework (ArraySortHelper.DepthLimitedQuickSort), celui de Sort-Object dans Windows
/// PowerShell 5.1 : mêmes échanges, donc le même ordre pour les éléments égaux
pub fn tri_dotnet<T>(l: &mut [T], cle: &dyn Fn(&T) -> i64) {
    if l.len() > 1 {
        let n = l.len() as isize - 1;
        tri_rapide(l, cle, 0, n, 32);
    }
}

fn tri_rapide<T>(l: &mut [T], cle: &dyn Fn(&T) -> i64, mut gauche: isize, mut droite: isize, mut limite: i32) {
    let cmp = |l: &[T], a: isize, b: isize| cle(&l[a as usize]).cmp(&cle(&l[b as usize]));
    let echanger_si_plus_grand = |l: &mut [T], a: isize, b: isize| {
        if a != b && cmp(l, a, b) == std::cmp::Ordering::Greater {
            l.swap(a as usize, b as usize);
        }
    };
    loop {
        if limite == 0 {
            // (jamais atteint avec si peu d'éléments ; un tri sûr à la place du tas de .NET)
            l[gauche as usize..=droite as usize].sort_by_key(|x| cle(x));
            return;
        }
        let (mut i, mut j) = (gauche, droite);
        let milieu = i + ((j - i) >> 1);
        echanger_si_plus_grand(l, i, milieu);
        echanger_si_plus_grand(l, i, j);
        echanger_si_plus_grand(l, milieu, j);
        let x = cle(&l[milieu as usize]);
        loop {
            while cle(&l[i as usize]) < x {
                i += 1;
            }
            while x < cle(&l[j as usize]) {
                j -= 1;
            }
            if i > j {
                break;
            }
            if i < j {
                l.swap(i as usize, j as usize);
            }
            i += 1;
            j -= 1;
            if i > j {
                break;
            }
        }
        limite -= 1;
        if j - gauche <= droite - i {
            if gauche < j {
                tri_rapide(l, cle, gauche, j, limite);
            }
            gauche = i;
        } else {
            if i < droite {
                tri_rapide(l, cle, i, droite, limite);
            }
            droite = j;
        }
        if gauche >= droite {
            break;
        }
    }
}

/// `Get-ProtectionScore`
pub fn note(e: &EtatS, constats: &[Constat]) -> i64 {
    let mp = e.mp.as_ref();
    let normal = mp.is_some_and(|m| m.normal());
    let autres = !e.autres_av.is_empty();
    let mut n = 0i64;
    if mp.is_some_and(|m| m.temps_reel && m.normal()) || autres {
        n += 30;
    }
    if mp.is_some_and(|m| m.age_base <= 3) || (autres && !normal) {
        n += 15;
    }
    if e.pare_feu() {
        n += 15;
    }
    if e.uac {
        n += 10;
    }
    if e.exclusions.is_empty() && e.exclusions_nombre == 0 {
        n += 10;
    }
    if mp.is_some_and(|m| m.age_analyse <= 14) || autres {
        n += 10;
    }
    if e.actives().is_empty() {
        n += 10;
    }
    for c in constats {
        n -= if c.grave() { 10 } else { 3 };
    }
    if !e.actives().is_empty() {
        n = n.min(30);
    }
    n.clamp(0, 100)
}

/// Une ligne de l'état résumé (`Get-EtatSecuriteKit`)
#[derive(Debug, Clone, PartialEq)]
pub struct LigneEtat {
    pub nom: &'static str,
    pub valeur: String,
    pub ton: &'static str,
}

pub fn etat_resume(e: &EtatS) -> Vec<LigneEtat> {
    let mut l = Vec::new();
    let ligne = |nom, valeur: String, ton| LigneEtat { nom, valeur, ton };
    match &e.mp {
        Some(mp) if mp.normal() => {
            l.push(ligne("Antivirus", "Microsoft Defender".into(), "ok"));
            l.push(ligne(
                "Protection en direct",
                if mp.temps_reel { "Activée" } else { "Désactivée" }.into(),
                if mp.temps_reel { "ok" } else { "urgent" },
            ));
            let age = mp.age_base;
            l.push(ligne(
                "Base de virus",
                if age <= 0 {
                    "À jour".into()
                } else {
                    format!("Il y a {age} jour{}", if age > 1 { "s" } else { "" })
                },
                if age <= 3 { "ok" } else { "attention" },
            ));
            l.push(ligne(
                "Dernière analyse",
                mp.fin_analyse.map(|d| d.jour_heure()).unwrap_or_else(|| "Jamais".into()),
                if mp.age_analyse <= 14 { "ok" } else { "attention" },
            ));
        }
        _ if !e.autres_av.is_empty() => l.push(ligne("Antivirus", e.autres_av.join(", "), "ok")),
        _ => l.push(ligne("Antivirus", "Aucun".into(), "urgent")),
    }
    l.push(ligne(
        "Pare-feu",
        if e.pare_feu() { "Activé" } else { "Désactivé" }.into(),
        if e.pare_feu() { "ok" } else { "urgent" },
    ));
    l.push(ligne(
        "Contrôle des comptes",
        if e.uac { "Activé" } else { "Désactivé" }.into(),
        if e.uac { "ok" } else { "urgent" },
    ));
    l
}

/// Une menace récente (`Get-ThreatHistory`)
#[derive(Debug, Clone, PartialEq)]
pub struct MenaceRecente {
    pub nom: String,
    pub quand: Option<Date>,
    pub fichier: String,
    pub etat: String,
    pub active: bool,
}

/// `-replace '^file:_', ''` (sans tenir compte des majuscules)
pub fn sans_file(t: &str) -> String {
    if t.len() >= 6 && t.is_char_boundary(6) && t[..6].eq_ignore_ascii_case("file:_") {
        t[6..].to_string()
    } else {
        t.to_string()
    }
}

pub fn menaces_recentes(e: &EtatS) -> Vec<MenaceRecente> {
    let trouver = |id: &str| -> Option<&Menace> { e.menaces.iter().rev().find(|m| ps::egal(&m.id, id)) };
    e.detections
        .iter()
        .map(|d: &Detection| {
            let m = trouver(&d.id);
            let etat = match ps::entier(&d.statut).unwrap_or(0) {
                1 => "Détectée",
                2 => "Nettoyée",
                3 => "En quarantaine",
                4 => "Supprimée",
                5 => "Autorisée",
                6 => "Bloquée",
                _ => "Traitée",
            };
            let active = m.is_some_and(|m| m.active);
            MenaceRecente {
                nom: m.map(|m| m.nom.clone()).unwrap_or_else(|| format!("Menace {}", d.id)),
                quand: d.quand,
                fichier: d.ressources.first().map(|r| sans_file(&ps::texte(r))).unwrap_or_default(),
                etat: if active { "Toujours active".into() } else { etat.into() },
                active,
            }
        })
        .collect()
}

/// La note sous les analyses (`$ui.SecScanNote.Text`)
pub fn note_analyses(e: &EtatS) -> String {
    if e.defender_ok() {
        "Les analyses utilisent Microsoft Defender, l'antivirus intégré à Windows.".into()
    } else if !e.autres_av.is_empty() {
        format!(
            "Ton antivirus ({}) remplace Microsoft Defender : lance les analyses depuis ton antivirus.",
            e.autres_av.join(", ")
        )
    } else {
        "Active Microsoft Defender pour pouvoir lancer une analyse.".into()
    }
}

// ---------------------------------------------------------------------------
// Tâches planifiées : ce qui les rend suspectes ($SecDataWork et Add-AideLectureSecurite)
// ---------------------------------------------------------------------------

/// `$exe -match '(powershell|pwsh|cmd|wscript|cscript|mshta)(\.exe)?$'`
fn interprete(exe: &str) -> bool {
    let b = exe.to_lowercase();
    let b = b.strip_suffix(".exe").map(str::to_string).into_iter().chain([b.clone()]).collect::<Vec<_>>();
    b.iter().any(|x| ["powershell", "pwsh", "cmd", "wscript", "cscript", "mshta"].iter().any(|n| x.ends_with(n)))
}

/// `$args -match '(-enc|-encodedcommand|frombase64|downloadstring|downloadfile|invoke-expression|\biex\b|-w(indowstyle)?\s+h(idden)?|http)'`
fn arguments_caches(a: &str) -> bool {
    let b = a.to_lowercase();
    if ["-enc", "frombase64", "downloadstring", "downloadfile", "invoke-expression", "http"]
        .iter()
        .any(|m| b.contains(m))
    {
        return true;
    }
    let c: Vec<char> = b.chars().collect();
    // \biex\b
    for i in 0..c.len() {
        if c[i..].starts_with(&['i', 'e', 'x'])
            && (i == 0 || !ps::mot(c[i - 1]))
            && c.get(i + 3).is_none_or(|x| !ps::mot(*x))
        {
            return true;
        }
    }
    // -w(indowstyle)?\s+h
    let long: Vec<char> = "indowstyle".chars().collect();
    for i in 0..c.len() {
        if !c[i..].starts_with(&['-', 'w']) {
            continue;
        }
        let apres = i + 2;
        let mut departs = vec![apres];
        if c[apres..].starts_with(&long) {
            departs.insert(0, apres + long.len());
        }
        for d in departs {
            let mut k = d;
            while k < c.len() && c[k].is_whitespace() {
                k += 1;
            }
            if k > d && c.get(k) == Some(&'h') {
                return true;
            }
        }
    }
    false
}

/// `[Environment]::ExpandEnvironmentVariables` : `%NOM%` remplacé par sa valeur (nom sans majuscules), inconnu laissé
pub fn developper(t: &str, valeur: &dyn Fn(&str) -> Option<String>) -> String {
    let mut r = String::new();
    let mut reste = t;
    while let Some(i) = reste.find('%') {
        r.push_str(&reste[..i]);
        let apres = &reste[i + 1..];
        match apres.find('%') {
            Some(j) if j > 0 => match valeur(&apres[..j]) {
                Some(v) => {
                    r.push_str(&v);
                    reste = &apres[j + 1..];
                }
                None => {
                    r.push('%');
                    reste = apres;
                }
            },
            _ => {
                r.push('%');
                reste = apres;
            }
        }
    }
    r.push_str(reste);
    r
}

/// Ce qu'il faut pour trier les tâches : les dossiers à risque et deux contrôles de fichiers
pub struct ContexteTaches<'a> {
    pub temp: &'a str,
    pub public: &'a str,
    /// La variable d'environnement (pour %SystemRoot%...)
    pub variable: &'a dyn Fn(&str) -> Option<String>,
    /// Le script existe et seul un administrateur peut le modifier (Test-Path et `$protege`)
    pub script_protege: &'a dyn Fn(&str) -> bool,
    /// Le fichier existe (Test-Path)
    pub existe: &'a dyn Fn(&str) -> bool,
    /// Tâche de démarrage de Nevermind et son script ($AutoStartTask, Get-AppScript) : seulement pour la lecture sur place
    pub propre_tache: Option<(&'a str, &'a str)>,
}

/// Le script de l'installateur de Mindset dans les arguments de sa tâche, s'ils ont exactement sa forme
fn script_installateur(args: &str, aide: bool) -> Option<String> {
    // (minuscules ASCII seulement : les positions restent celles du texte d'origine)
    let b = args.to_ascii_lowercase();
    let mut r = b.as_str();
    let pris = |r: &mut &str, m: &str| -> bool {
        if let Some(x) = r.strip_prefix(m) {
            *r = x;
            true
        } else {
            false
        }
    };
    if !pris(&mut r, "-noprofile ") {
        return None;
    }
    if aide {
        pris(&mut r, "-noninteractive ");
    }
    if !pris(&mut r, "-executionpolicy bypass -windowstyle hidden -file \"") {
        return None;
    }
    let fin = r.find('"')?;
    let script_bas = &r[..fin];
    let suffixe = "\\fichiers\\installer-admin.ps1";
    if script_bas.len() <= suffixe.len() || !script_bas.ends_with(suffixe) {
        return None;
    }
    let debut = args.len() - r.len();
    let script = args[debut..debut + fin].to_string();
    r = &r[fin + 1..];
    if !pris(&mut r, " -action demande ") {
        return None;
    }
    if aide && let Some(x) = r.strip_prefix("-sid s-1-5-21-") {
        let n = x.find(|c: char| !(c.is_ascii_digit() || c == '-')).unwrap_or(x.len());
        if n > 0 && x[n..].starts_with(' ') {
            r = &x[n + 1..];
        }
    }
    if !pris(&mut r, "-demande \"") {
        return None;
    }
    let f = r.find('"')?;
    let dem = &r[..f];
    if dem.len() <= "\\installation.demande".len() || !dem.ends_with("\\installation.demande") || &r[f..] != "\"" {
        return None;
    }
    Some(script)
}

/// Une tâche suspecte, ou `None`. `aide` : la liste vient de l'Aide (Add-AideLectureSecurite), sinon la lecture sur place.
pub fn tache_suspecte(nom: &str, chemin: &str, exe: &str, args: &str, c: &ContexteTaches, aide: bool) -> Option<Tache> {
    let exe = developper(exe, c.variable);
    let exe = exe.trim_matches('"').to_string();
    if !aide
        && let Some((propre, script)) = c.propre_tache
        && ps::egal(nom, propre)
        && chemin == "\\"
    {
        {
            let b = exe.to_lowercase();
            if b.ends_with("\\nevermind.exe") && ps::egal(args, "-Demarrage") {
                let dossier = exe.rsplit_once('\\').map(|x| x.0).unwrap_or("");
                if ["Nevermind.ps1", "OptiGame.ps1"].iter().any(|f| (c.existe)(&format!("{dossier}\\fichiers\\{f}"))) {
                    return None;
                }
            }
            if b.ends_with("\\windowspowershell\\v1.0\\powershell.exe")
                && let Some(s) = script_propre(args)
                && (ps::egal(&s, script) || (c.existe)(&s))
            {
                return None;
            }
        }
    }
    if ps::egal(chemin, "\\Mindset\\")
        && ps::egal(nom, "Mindset (installation)")
        && exe.to_lowercase().ends_with("\\system32\\windowspowershell\\v1.0\\powershell.exe")
        && let Some(s) = script_installateur(args, aide)
        && (c.script_protege)(&s)
    {
        return None;
    }
    let cachee = interprete(&exe) && arguments_caches(args);
    let risque = ps::commence(&exe, c.temp) || ps::commence(&exe, c.public);
    (cachee || risque).then(|| Tache {
        nom: nom.into(),
        chemin: chemin.into(),
        cmd: format!("{exe} {args}").trim_matches(|x: char| x.is_whitespace()).to_string(),
        grave: cachee,
    })
}

/// `-match '^-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "([^"]+\\fichiers\\(?:Nevermind|OptiGame)\.ps1)" -Demarrage$'`
fn script_propre(args: &str) -> Option<String> {
    let debut = "-noprofile -executionpolicy bypass -windowstyle hidden -file \"";
    let b = args.to_ascii_lowercase();
    let r = b.strip_prefix(debut)?;
    let fin = r.find('"')?;
    let s = &r[..fin];
    if &r[fin..] != "\" -demarrage" {
        return None;
    }
    let ok =
        ["\\fichiers\\nevermind.ps1", "\\fichiers\\optigame.ps1"].iter().any(|f| s.len() > f.len() && s.ends_with(f));
    ok.then(|| args[debut.len()..debut.len() + fin].to_string())
}

// ---------------------------------------------------------------------------
// La lecture de l'Aide (lecture.securite, lecture.taches), quand Nevermind n'a pas les droits
// ---------------------------------------------------------------------------

/// `Add-AideLectureSecurite` (sans droits administrateur) : `lecture` = réponse de lecture.securite (`None` : pas
/// d'Aide), `taches` = réponse de lecture.taches (`Err` : pas lue, la liste sur place reste)
pub fn completer_par_aide(e: &mut EtatS, lecture: Option<&Value>, taches: Result<&Value, String>, c: &ContexteTaches) {
    let Some(l) = lecture.filter(|l| ps::vrai(l)) else { return };
    let nb = match l.get("ExclusionsNombre") {
        None | Some(Value::Null) => -1,
        Some(v) => ps::entier(v).unwrap_or(-1),
    };
    if nb > e.exclusions.len() as i64 {
        e.exclusions_nombre = nb;
    }
    let vrais =
        |k: &str| -> Vec<Value> { l.get(k).map(ps::liste).unwrap_or_default().into_iter().filter(ps::vrai).collect() };
    e.menaces = vrais("Menaces")
        .iter()
        .map(|m| Menace {
            id: m.get("ThreatID").map(ps::texte).unwrap_or_default(),
            nom: m.get("ThreatName").map(ps::texte).unwrap_or_default(),
            gravite: m.get("SeverityID").cloned().unwrap_or(Value::Null),
            active: m.get("IsActive").is_some_and(ps::vrai),
        })
        .collect();
    e.detections = vrais("Detections")
        .iter()
        .map(|d| Detection {
            id: d.get("ThreatID").map(ps::texte).unwrap_or_default(),
            statut: d.get("ThreatStatusID").cloned().unwrap_or(Value::Null),
            quand: Some(Date::depuis_ticks(d.get("Quand").map(|q| ps::reel(q) as i64).unwrap_or(0))),
            ressources: d.get("Resources").map(ps::liste).unwrap_or_default(),
        })
        .collect();
    match taches {
        Err(_) => {}
        Ok(t) => {
            e.taches = ps::liste(t)
                .iter()
                .filter(|x| ps::vrai(x))
                .filter_map(|x| {
                    let g = |k: &str| x.get(k).map(ps::texte).unwrap_or_default();
                    tache_suspecte(&g("Nom"), &g("Chemin"), &g("Exe"), &g("Args"), c, true)
                })
                .collect();
        }
    }
}

// ---------------------------------------------------------------------------
// Après une analyse : les menaces trouvées (Get-ScanThreats)
// ---------------------------------------------------------------------------

/// Une détection lue (Id, quand, ressources)
#[derive(Debug, Clone, PartialEq)]
pub struct DetectionLue {
    pub id: String,
    pub quand: Date,
    pub ressources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenaceTrouvee {
    pub nom: String,
    pub fichier: String,
}

/// Les détections lues sur place (`Err` : pas lues) et les noms des menaces
pub type Detections = (Result<Vec<DetectionLue>, String>, Vec<(String, String)>);

/// `Lu` faux : résultat inconnu (jamais « Aucune menace »)
#[derive(Debug, Clone, PartialEq)]
pub struct ResultatAnalyse {
    pub lu: bool,
    pub menaces: Vec<MenaceTrouvee>,
}

/// `Get-ScanThreats` : `aide` = lecture.securite de l'Aide (sans droits seulement), `locale` = les détections lues sur
/// place (`Err` : pas lues) et les noms des menaces
pub fn menaces_analyse(
    debut: Date,
    est_admin: bool,
    aide: Option<&Value>,
    locale: &dyn Fn() -> Detections,
) -> (ResultatAnalyse, Option<String>) {
    let depuis = Date::depuis_ticks(debut.ticks() - 5 * 10_000_000);
    let mut det: Vec<DetectionLue> = Vec::new();
    let mut noms: Vec<(String, String)> = Vec::new();
    let mut fiable = false;
    let mut journal = None;
    if !est_admin && let Some(l) = aide.filter(|l| ps::vrai(l)) {
        det = l
            .get("Detections")
            .map(ps::liste)
            .unwrap_or_default()
            .iter()
            .filter(|x| ps::vrai(x))
            .map(|d| DetectionLue {
                id: d.get("ThreatID").map(ps::texte).unwrap_or_default(),
                quand: Date::depuis_ticks(d.get("Quand").map(|q| ps::reel(q) as i64).unwrap_or(0)),
                ressources: d.get("Resources").map(ps::liste).unwrap_or_default().iter().map(ps::texte).collect(),
            })
            .collect();
        for m in l.get("Menaces").map(ps::liste).unwrap_or_default().iter().filter(|x| ps::vrai(x)) {
            noms.push((
                m.get("ThreatID").map(ps::texte).unwrap_or_default(),
                m.get("ThreatName").map(ps::texte).unwrap_or_default(),
            ));
        }
        fiable = l.get("DetectionsLues").is_some_and(ps::vrai) || !det.is_empty();
    }
    if !fiable {
        let (lu, n) = locale();
        match lu {
            Ok(d) => {
                det = d;
                fiable = est_admin;
            }
            Err(m) => {
                journal = Some(format!("Analyse Defender : détections non lues ({m})"));
                det = Vec::new();
            }
        }
        noms.extend(n);
    }
    let nom = |id: &str| noms.iter().rev().find(|(i, _)| ps::egal(i, id)).map(|x| x.1.clone()).unwrap_or_default();
    let menaces: Vec<MenaceTrouvee> = det
        .iter()
        .filter(|d| d.quand >= depuis)
        .map(|d| MenaceTrouvee {
            nom: nom(&d.id),
            fichier: d.ressources.first().map(|r| sans_file(r)).unwrap_or_default(),
        })
        .collect();
    (ResultatAnalyse { lu: fiable || !menaces.is_empty(), menaces }, journal)
}

/// « Désactiver ces tâches » (Disable-SuspectTasks) : `faites` et `ratees` sont les noms. `Ok(None)` : tout est fait ;
/// `Ok(Some(message))` : une partie seulement ; `Err(message)` : aucune n'a pu l'être (rien n'est noté comme fait).
pub fn message_taches(faites: &[String], ratees: &[String]) -> Result<Option<String>, String> {
    if ratees.is_empty() {
        return Ok(None);
    }
    let plus = ratees.len() > 1;
    let txt = format!(
        "Je n'ai pas pu désactiver {} : {}.\n\nWindows ne me laisse pas faire : {} par un administrateur. Lance une analyse rapide de Defender, et si tu ne {} reconnais pas, demande de l'aide à quelqu'un de confiance.",
        if plus { "ces tâches" } else { "cette tâche" },
        ratees.join(", "),
        if plus { "elles ont sans doute été créées" } else { "elle a sans doute été créée" },
        if plus { "les" } else { "la" }
    );
    if faites.is_empty() {
        return Err(txt);
    }
    let n = faites.len();
    Ok(Some(format!("{n} tâche{} désactivée{}.\n\n{txt}", s(n), s(n))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_comme_powershell() {
        let l: Vec<String> = [
            "127.0.0.1 localhost",
            "::1  localhost",
            "# x",
            "  ",
            "0.0.0.0 pub.fr",
            "1.2.3.4\tLOCALHOST",
            "x broadcasthost",
            "127.0.0.1localhost",
        ]
        .iter()
        .map(|x| x.to_string())
        .collect();
        assert_eq!(lignes_hosts(&l), vec!["0.0.0.0 pub.fr".to_string(), "127.0.0.1localhost".to_string()]);
    }

    #[test]
    fn commandes_cachees() {
        assert!(arguments_caches("-WindowStyle Hidden -File x"));
        assert!(arguments_caches("-w  h"));
        assert!(!arguments_caches("-win hidden"));
        assert!(arguments_caches("IEX(1)"));
        assert!(!arguments_caches("iexplore"));
        assert!(interprete("C:\\x\\notcmd.exe") && interprete("MSHTA") && !interprete("cmd.exe.bak"));
    }

    #[test]
    fn installateur_de_mindset() {
        let a = "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File \"C:\\M\\fichiers\\installer-admin.ps1\" -Action demande -Demande \"C:\\x\\installation.demande\"";
        assert_eq!(script_installateur(a, false).as_deref(), Some("C:\\M\\fichiers\\installer-admin.ps1"));
        let b = "-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -File \"C:\\M\\fichiers\\installer-admin.ps1\" -Action demande -Sid S-1-5-21-1-2-3 -Demande \"C:\\x\\installation.demande\"";
        assert!(script_installateur(b, true).is_some() && script_installateur(b, false).is_none());
        assert!(script_installateur(&format!("{a} x"), false).is_none());
    }

    #[test]
    fn variables() {
        let v = |n: &str| (n.eq_ignore_ascii_case("systemroot")).then(|| "C:\\WINDOWS".to_string());
        assert_eq!(developper("%SystemRoot%\\a %x% 100%", &v), "C:\\WINDOWS\\a %x% 100%");
    }
}
