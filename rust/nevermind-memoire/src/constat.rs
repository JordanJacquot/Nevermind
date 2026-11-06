//! Le constat (`Get-ConstatMemoire`) : calcul seul, sur les données lues ou injectées. Mêmes regroupements, mêmes
//! étiquettes, mêmes alertes et mêmes corrections que memoire.ps1, et la même règle « fermable » (aide-regles).

use crate::donnees::{Donnees, Etiquette, Jeux, Pilote, Processus, Service};
use crate::listes::{self, APPS_CONNUES, DOSSIERS_JEUX, EXES_WINDOWS, LANCEURS, PRODUITS_WINDOWS, SERVICES_INUTILES};
use crate::texte::{self, Date, arrondi_pair, contient, taille};
use aide_regles::fermeture::{self, Contexte as ContexteRegle, Fiche, Parente, Reference};
use std::collections::{HashMap, HashSet};

const MO: f64 = 1024.0 * 1024.0;
const GO: f64 = MO * 1024.0;

/// Un processus qu'une appli fermable fermera : `@{ Pid; Debut; Chemin; Octets }`
#[derive(Clone, Debug, PartialEq)]
pub struct Cible {
    pub pid: i64,
    pub debut: i64,
    pub chemin: String,
    pub octets: f64,
}

/// Une ligne de la page : une appli (tous ses processus additionnés)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Appli {
    pub cle: String,
    pub nom: String,
    pub kind: String,
    pub note: String,
    pub etiquette: String,
    pub origine: bool,
    pub jamais: bool,
    pub au_demarrage: bool,
    pub par_windows: bool,
    pub octets: f64,
    pub nb: usize,
    pub exes: Vec<String>,
    pub services: Vec<String>,
    pub corrections: Vec<String>,
    pub cibles: Vec<Cible>,
    pub refus: Vec<String>,
    pub jeu: bool,
    pub lanceur: bool,
    pub fermable: bool,
}

/// Une correction sûre : un service passé en « manuel »
#[derive(Clone, Debug, PartialEq)]
pub struct Correction {
    pub id: String,
    pub nom: String,
    /// Le service (`Changes = @(@{ Type = 'svc'; Name; Demarrage = 'Manual' })`)
    pub service: String,
    pub demarrage: String,
}

/// Une alerte (pilotes ou matériel)
#[derive(Clone, Debug, PartialEq)]
pub struct Alerte {
    pub id: String,
    pub ton: String,
    pub titre: String,
    pub texte: String,
    pub quoi: String,
}

/// Les seuils de la mémoire des pilotes
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seuils {
    pub fort: f64,
    pub surveiller: f64,
}

/// Le constat complet
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Constat {
    pub pc: String,
    pub date: Date,
    pub installee: f64,
    pub installee_lue: bool,
    pub visible: f64,
    pub reservee: f64,
    pub utilisee: f64,
    pub disponible: f64,
    pub cache: f64,
    pub modifiee: f64,
    pub pool_np: f64,
    pub pool_p: f64,
    pub compressee: f64,
    pub seuils: Option<Seuils>,
    pub pourcent: i64,
    pub applis: Vec<Appli>,
    pub total_applis: f64,
    pub reste: f64,
    pub alertes: Vec<Alerte>,
    pub corrections: Vec<Correction>,
    pub etiquettes: Vec<Etiquette>,
}

impl Constat {
    pub fn seuils(&self) -> Seuils {
        self.seuils.unwrap_or_else(|| seuils_pilotes(self.installee))
    }
}

/// `Get-SeuilsPilotes` : alerte franche au-delà de 5 % de la mémoire (entre 1 et 2 Go), « à surveiller » un peu en
/// dessous (au plus 0,8 Go)
pub fn seuils_pilotes(memoire: f64) -> Seuils {
    let fort = (2.0 * GO).min((1.0 * GO).max(0.05 * memoire));
    Seuils { fort, surveiller: (0.8 * GO).min(0.75 * fort) }
}

/// `-replace '(?i)\.exe$', ''`
pub fn sans_exe(nom: &str) -> &str {
    if nom.len() >= 4 && nom.is_char_boundary(nom.len() - 4) && nom[nom.len() - 4..].eq_ignore_ascii_case(".exe") {
        &nom[..nom.len() - 4]
    } else {
        nom
    }
}

/// `Test-JeuMemoire` : un programme est-il un jeu ? (un lanceur n'en est jamais un)
pub fn est_jeu(p: &Processus, image: &str, jeux: Option<&Jeux>) -> bool {
    let exe = sans_exe(&p.nom).to_lowercase();
    if LANCEURS.is_match(&exe) {
        return false;
    }
    if let Some(j) = jeux {
        if j.en_cours != 0 && p.pid == j.en_cours {
            return true;
        }
        if !image.is_empty() && contient(&j.exes, &image.to_lowercase()) {
            return true;
        }
        if contient(&j.noms, &exe) {
            return true;
        }
    }
    !image.is_empty() && DOSSIERS_JEUX.is_match(image)
}

/// L'appli d'un processus (`Get-AppliMemoire`)
struct AppliDe {
    cle: String,
    nom: String,
    kind: &'static str,
    note: String,
    origine: bool,
    jamais: bool,
    ensemble: bool,
}

fn appli_de(p: &Processus, services: &[&Service], windir: &str) -> AppliDe {
    let exe = sans_exe(&p.nom);
    let bas = exe.to_lowercase();
    let simple = |cle: String, nom: String, kind: &'static str, note: &str| AppliDe {
        cle,
        nom,
        kind,
        note: note.into(),
        origine: false,
        jamais: false,
        ensemble: false,
    };
    // Un service de la liste « inutile » dans ce processus : c'est lui qui compte
    for s in services {
        for i in SERVICES_INUTILES.iter() {
            if i.motif.is_match(&s.nom) {
                return simple(
                    format!("inutile:{}", i.nom),
                    i.nom.into(),
                    "inutile",
                    "Service qui ne sert pas au quotidien. Nevermind peut le passer en « manuel » : il se lancera seulement quand son logiciel en a besoin.",
                );
            }
        }
    }
    // Les « Hôte de service » : rattachés à leurs services
    if bas == "svchost" {
        if !services.is_empty() {
            let mut tri: Vec<&Service> = services.to_vec();
            tri.sort_by(|a, b| texte::comparer_culture(&a.nom, &b.nom));
            let titre = if tri[0].titre.is_empty() { tri[0].nom.clone() } else { tri[0].titre.clone() };
            let n = tri.len();
            let nom = if n > 1 {
                let s = if n > 2 { "s" } else { "" };
                format!("{titre} et {} autre{s} service{s}", n - 1)
            } else {
                titre
            };
            let noms: Vec<&str> = tri.iter().map(|s| s.nom.as_str()).collect();
            let quatre = noms.iter().take(4).copied().collect::<Vec<_>>().join(", ");
            return simple(
                format!("svc:{}", noms.join(",")),
                nom,
                "windows",
                &format!("Service de Windows (Hôte de service : {quatre}{}).", if n > 4 { "..." } else { "" }),
            );
        }
        return simple("svc:".into(), "Services de Windows".into(), "windows", "Hôte de service de Windows.");
    }
    let nom_produit = || {
        if !p.produit.is_empty() && !PRODUITS_WINDOWS.is_match(&p.produit) {
            p.produit.clone()
        } else if !p.description.is_empty() {
            p.description.clone()
        } else {
            exe.to_string()
        }
    };
    for a in APPS_CONNUES.iter() {
        if a.motif.is_match(&bas) {
            let nom = if a.nom.is_empty() { nom_produit() } else { a.nom.to_string() };
            return AppliDe {
                cle: format!("app:{nom}"),
                nom,
                kind: a.kind,
                note: a.note.into(),
                origine: a.origine,
                jamais: a.jamais,
                ensemble: a.ensemble,
            };
        }
    }
    let windows = EXES_WINDOWS.is_match(&bas)
        || (!p.chemin.is_empty()
            && !windir.is_empty()
            && texte::commence(&p.chemin, &format!("{}\\", windir.trim_end_matches('\\'))));
    let nom = match listes::nom_windows(&bas) {
        Some(n) => n.to_string(),
        None => nom_produit(),
    };
    if windows {
        return simple(format!("app:{nom}"), nom, "windows", "Fait partie de Windows ou d'un pilote de ton matériel.");
    }
    AppliDe { origine: true, ..simple(format!("app:{nom}"), nom, "verifier", "Un logiciel installé sur ce PC.") }
}

/// `Get-OrigineMemoire` : d'où vient une appli, et donc comment s'en passer
pub fn origine(au_demarrage: bool, par_windows: bool) -> &'static str {
    if au_demarrage {
        return "Il se lance avec le PC : si tu ne t'en sers pas, coupe-le dans la page Démarrage.";
    }
    if par_windows {
        return "Il n'est pas dans la page Démarrage : c'est son logiciel qui le fait lancer par Windows. Si tu ne t'en sers pas, désinstaller ce logiciel libère cette mémoire.";
    }
    "Il ne se lance pas avec le PC : il est ouvert en ce moment. Le fermer libère cette mémoire."
}

/// Le contexte de la règle « fermable » d'après ce qui a été lu (services, antivirus, Nevermind lui-même)
fn contexte_regle(d: &Donnees) -> ContexteRegle {
    let services_pid = if d.services.is_empty() {
        None
    } else {
        Some(d.services.iter().filter(|s| s.pid != 0).map(|s| s.pid).collect::<HashSet<i64>>())
    };
    let mut processus = HashMap::new();
    for p in &d.processus {
        processus.insert(p.pid, Parente { parent: p.parent, chemin: p.image_ou_chemin().to_string() });
    }
    ContexteRegle {
        windows: d.windir.clone(),
        programmes: if d.programmes.is_empty() { r"C:\Program Files".into() } else { d.programmes.clone() },
        antivirus: d.antivirus.iter().filter(|x| !x.is_empty()).cloned().collect(),
        services_pid,
        reference: d.moi.as_ref().map(|m| Reference {
            id: m.id,
            session: m.session,
            compte: m.compte.clone(),
            eleve: m.eleve,
        }),
        processus,
    }
}

/// La fiche d'un processus pour la règle
pub fn fiche(p: &Processus) -> Fiche {
    Fiche {
        id: p.pid,
        chemin: p.image_ou_chemin().to_string(),
        session: p.session,
        compte: p.compte.clone(),
        protection: p.protection,
        eleve: p.eleve,
    }
}

/// Le constat complet (`Get-ConstatMemoire`)
pub fn constat(d: &Donnees) -> Constat {
    let visible = d.visible;
    let mut inst = d.installee;
    let lisible = inst >= visible && inst > 0.0;
    if !lisible {
        inst = visible; // (mémoire installée illisible : pas de « réservé » inventé)
    }
    let reservee = (inst - visible).max(0.0);
    let utilisee = (visible - d.disponible).max(0.0);

    let mut svc_par_pid: HashMap<i64, Vec<&Service>> = HashMap::new();
    for s in &d.services {
        if s.pid != 0 {
            svc_par_pid.entry(s.pid).or_default().push(s);
        }
    }
    let mut nom_par_pid: HashMap<i64, String> = HashMap::new();
    let mut parent_par_pid: HashMap<i64, i64> = HashMap::new();
    for p in &d.processus {
        nom_par_pid.insert(p.pid, p.nom.to_lowercase());
        parent_par_pid.insert(p.pid, p.parent);
    }
    let demarrage: Vec<String> = d.demarrage.iter().filter(|x| !x.is_empty()).map(|x| x.to_lowercase()).collect();
    // La règle de fermeture sur ce qui a été lu (pour tous les processus dont la poignée est gardée)
    let ctx = contexte_regle(d);
    let mut refus_par: HashMap<i64, Option<&'static str>> = HashMap::new();
    for p in d.processus.iter().filter(|p| p.debut != 0) {
        refus_par.insert(p.pid, fermeture::refus(&ctx, &fiche(p), false));
    }
    let vide: Vec<&Service> = Vec::new();
    let mut un_jeu = false;
    let mut ordre: Vec<String> = Vec::new();
    let mut groupes: HashMap<String, Appli> = HashMap::new();
    let mut compressee = 0.0;
    let (mut edge_avance, mut chrome_avance) = (false, false);
    for p in &d.processus {
        let exe = sans_exe(&p.nom);
        if texte::egal(exe, "Memory Compression") {
            compressee += p.octets;
            continue;
        }
        if texte::egal(exe, "Idle") || p.pid == 0 {
            continue;
        }
        if texte::egal(exe, "msedge") && listes::motif("--no-startup-window").is_match(&p.ligne) {
            edge_avance = true;
        }
        if texte::egal(exe, "chrome") && listes::motif("--no-startup-window").is_match(&p.ligne) {
            chrome_avance = true;
        }
        let services = svc_par_pid.get(&p.pid).unwrap_or(&vide);
        let a = appli_de(p, services, &d.windir);
        // (une appli = un dossier de programme : Chrome et Chrome Canary, même nom, sont deux lignes)
        let image = p.image_ou_chemin();
        let mut cle = a.cle.clone();
        if a.kind != "windows" && cle.starts_with("app:") && !a.ensemble && !image.is_empty() {
            cle = format!("{cle}|{}", texte::dossier(image).to_lowercase());
        }
        // (les clés d'une table de PowerShell ne tiennent pas compte des majuscules : « Steam » et « steam » vont ensemble)
        let g = groupes.entry(cle.to_lowercase()).or_insert_with(|| {
            ordre.push(cle.to_lowercase());
            Appli {
                cle: cle.clone(),
                nom: a.nom.clone(),
                kind: a.kind.into(),
                note: a.note.clone(),
                origine: a.origine,
                jamais: a.jamais,
                ..Default::default()
            }
        });
        g.octets += p.octets;
        g.nb += 1;
        // Fermable ? La règle de l'Aide sur ce processus ; sans poignée gardée (Debut), pas vérifiable : non
        let refus: Option<String> = if p.debut == 0 {
            Some("pas pu être vérifié".into())
        } else {
            refus_par.get(&p.pid).copied().flatten().map(str::to_string)
        };
        if est_jeu(p, image, d.jeux.as_ref()) {
            g.jeu = true;
            un_jeu = true;
        } else if LANCEURS.is_match(exe) {
            g.lanceur = true;
        }
        match refus {
            Some(r) => g.refus.push(format!("{exe} : {r}")),
            None => g.cibles.push(Cible { pid: p.pid, debut: p.debut, chemin: image.to_string(), octets: p.octets }),
        }
        // (lui ou un de ses parents, jusqu'à 4 crans : Steam et son « WebHelper »)
        let mut x = p.pid;
        let mut nom_x = Some(format!("{exe}.exe").to_lowercase());
        let mut i = 0;
        while i < 5 && nom_x.as_deref().is_some_and(|n| !n.is_empty()) {
            if demarrage.iter().any(|m| Some(m.as_str()) == nom_x.as_deref()) {
                g.au_demarrage = true;
                break;
            }
            x = parent_par_pid.get(&x).copied().unwrap_or(0);
            nom_x = if x != 0 { nom_par_pid.get(&x).cloned() } else { None };
            i += 1;
        }
        if matches!(nom_par_pid.get(&p.parent).map(String::as_str), Some("services.exe" | "svchost.exe")) {
            g.par_windows = true; // (lancé par un service ou une tâche planifiée)
        }
        let e = format!("{exe}.exe");
        if !contient(&g.exes, &e) {
            g.exes.push(e);
        }
        for s in services {
            if !contient(&g.services, &s.nom) {
                g.services.push(s.nom.clone());
            }
        }
    }

    // Les corrections sûres
    let mut corrections = Vec::new();
    for s in &d.services {
        if !(texte::egal(&s.demarrage, "Auto") || texte::egal(&s.demarrage, "Automatic")) {
            continue;
        }
        let Some(i) = SERVICES_INUTILES.iter().find(|i| i.motif.is_match(&s.nom)) else { continue };
        corrections.push(Correction {
            id: format!("svc:{}", s.nom),
            nom: format!("{} ({}) : passe en « manuel »", i.nom, s.nom),
            service: s.nom.clone(),
            demarrage: "Manual".into(),
        });
    }

    // Les étiquettes des applis selon ce qui a été trouvé
    let mut applis: Vec<Appli> = Vec::new();
    for cle in &ordre {
        let Some(mut g) = groupes.remove(cle) else { continue };
        let mut ids: Vec<String> = Vec::new();
        for c in &corrections {
            if contient(&g.services, &c.service) && !ids.contains(&c.id) {
                ids.push(c.id.clone());
            }
        }
        g.corrections = ids;
        if contient(&g.exes, "msedge.exe") && edge_avance {
            g.kind = "inutile".into();
            g.note = "Edge se lance en avance avec Windows pour s'ouvrir plus vite. Pour le couper : dans Edge, Paramètres, puis Système et performances, et désactive « Boost de démarrage ».".into();
        }
        if contient(&g.exes, "chrome.exe") && chrome_avance {
            g.kind = "inutile".into();
            g.note = "Chrome reste ouvert en arrière-plan, même fenêtres fermées. Pour le couper : dans Chrome, Paramètres, puis Système, et désactive « Poursuivre l'exécution des applications en arrière-plan ».".into();
        }
        if g.origine {
            g.note = format!("{} {}", g.note, origine(g.au_demarrage, g.par_windows));
        }
        g.etiquette = listes::etiquette(&g.kind).into();
        // Le bouton « Fermer » : toute l'appli, et seulement si chacun de ses processus peut l'être
        if g.kind == "windows" {
            g.refus.push("fait partie de Windows (étiquette)".into());
        }
        if g.jamais {
            g.refus.push("jamais fermée par Nevermind".into());
        }
        if g.jeu {
            g.refus.push("est un jeu".into());
            g.note = "Ton jeu : Nevermind ne le ferme pas.".into();
        } else if g.lanceur && un_jeu {
            g.refus.push("sert au jeu en cours".into());
            g.note = format!("{} Sert à ton jeu en cours : Nevermind ne le ferme pas pendant la partie.", g.note);
        }
        if !g.services.is_empty() {
            g.refus.push("contient un service".into());
        }
        g.fermable = g.refus.is_empty() && !g.cibles.is_empty();
        applis.push(g);
    }
    // Du plus gros au plus petit (à égalité : l'ordre de la lecture)
    applis.sort_by(|a, b| b.octets.partial_cmp(&a.octets).unwrap_or(std::cmp::Ordering::Equal));
    let total_applis: f64 = applis.iter().map(|g| g.octets).sum();

    // Les alertes
    let mut alertes = Vec::new();
    let seuils = seuils_pilotes(inst);
    let np = d.pool_np;
    if np >= seuils.surveiller {
        let fort = np >= seuils.fort;
        // Le pilote suspect : d'abord le pilote d'un fabricant (ceux de Windows fuient rarement), dans la plus grosse
        // étiquette qui en a un ; sinon un pilote de Windows
        let microsoft = listes::motif("^Microsoft");
        let mut choix: Option<(&Pilote, f64)> = None;
        'tour: for tiers_seuls in [true, false] {
            for e in &d.etiquettes {
                if let Some(p) = e.pilotes.iter().find(|p| !tiers_seuls || !microsoft.is_match(&p.societe)) {
                    choix = Some((p, e.octets));
                    break 'tour;
                }
            }
        }
        let (mut suspect, mut nom_pilote) = (String::new(), String::new());
        if let Some((x, octets)) = choix {
            let genre = listes::genre_pilote(&format!("{} {} {}", x.fichier, x.description, x.societe));
            let qui =
                if x.description.is_empty() { x.fichier.clone() } else { format!("{} ({})", x.description, x.fichier) };
            suspect = format!(
                "C'est probablement le pilote {qui}{}{}, qui en garde {}.",
                if genre.is_empty() { String::new() } else { format!(", {genre}") },
                if x.societe.is_empty() { String::new() } else { format!(", de {}", x.societe) },
                taille(octets)
            );
            nom_pilote = if x.description.is_empty() { x.fichier.clone() } else { x.description.clone() };
        }
        alertes.push(Alerte {
            id: "pilotes".into(),
            ton: if fort { "urgent" } else { "attention" }.into(),
            titre: if fort {
                format!("Un pilote garde trop de mémoire ({})", taille(np))
            } else {
                format!("Mémoire des pilotes un peu élevée ({}), à surveiller", taille(np))
            },
            texte: format!(
                "{}Souvent un pilote qui garde de la mémoire sans la rendre : carte réseau (Killer, Realtek), antivirus en plus de celui de Windows, logiciel RGB ou de carte mère.",
                if suspect.is_empty() { String::new() } else { format!("{suspect} ") }
            ),
            quoi: if fort {
                format!(
                    "Installe la dernière version {} depuis le site du fabricant du PC ou de la carte mère, puis redémarre le PC.",
                    if nom_pilote.is_empty() {
                        "du pilote en cause (souvent celui de la carte réseau)".to_string()
                    } else {
                        format!("du pilote « {nom_pilote} »")
                    }
                )
            } else {
                format!(
                    "Redémarre le PC et regarde de nouveau dans quelques jours : si ça monte encore, mets à jour {}.",
                    if nom_pilote.is_empty() {
                        "le pilote de ta carte réseau".to_string()
                    } else {
                        format!("le pilote « {nom_pilote} »")
                    }
                )
            },
        });
    }
    if lisible && reservee >= GO {
        alertes.push(Alerte {
            id: "materiel".into(),
            ton: "attention".into(),
            titre: format!("{} réservés au matériel", taille(reservee)),
            texte: "Cette mémoire est installée mais Windows ne peut pas s'en servir : souvent la puce graphique intégrée au processeur qui se la réserve, réglée dans le BIOS.".into(),
            quoi: "Dans le BIOS, baisse la mémoire de la puce graphique (« UMA Frame Buffer » ou « iGPU Memory »), ou demande à quelqu'un qui s'y connaît.".into(),
        });
    }

    Constat {
        pc: d.pc.clone(),
        date: d.date.unwrap_or_default(),
        installee: inst,
        installee_lue: lisible,
        visible,
        reservee,
        utilisee,
        disponible: d.disponible,
        cache: d.cache,
        modifiee: d.modifiee,
        pool_np: np,
        pool_p: d.pool_p,
        compressee,
        seuils: Some(seuils),
        pourcent: if visible != 0.0 { arrondi_pair(100.0 * utilisee / visible) } else { 0 },
        applis,
        total_applis,
        reste: (utilisee - total_applis - np - compressee).max(0.0),
        alertes,
        corrections,
        etiquettes: d.etiquettes.clone(),
    }
}

/// Une ligne de la part cachée : `@{ Nom; Octets; Note }`
pub struct LigneCachee {
    pub nom: &'static str,
    pub octets: f64,
    pub note: String,
}

/// `Get-PartCacheeMemoire`
pub fn part_cachee(c: &Constat) -> Vec<LigneCachee> {
    let mut l = Vec::new();
    if c.installee_lue {
        l.push(LigneCachee {
            nom: "Réservé au matériel",
            octets: c.reservee,
            note: "Installée mais pas utilisable par Windows (souvent la puce graphique intégrée).".into(),
        });
    }
    l.push(LigneCachee {
        nom: "Pilotes (mémoire fixe)",
        octets: c.pool_np,
        note: format!(
            "La mémoire que les pilotes gardent toujours en mémoire vive. Normal jusqu'à {} environ sur ce PC.",
            taille(c.seuils().surveiller)
        ),
    });
    l.push(LigneCachee {
        nom: "Pilotes et Windows (mémoire qui peut aller sur le disque)",
        octets: c.pool_p,
        note: "Windows peut la mettre de côté sur le disque si la mémoire manque.".into(),
    });
    l.push(LigneCachee {
        nom: "Mémoire compressée",
        octets: c.compressee,
        note: "Windows serre ce qui sert peu pour gagner de la place : c'est plutôt bon signe.".into(),
    });
    l.push(LigneCachee {
        nom: "Autre (Windows lui-même, mémoire partagée)",
        octets: c.reste,
        note: "Ce qui n'appartient à aucune appli en particulier.".into(),
    });
    l.push(LigneCachee {
        nom: "En cache (ne compte pas)",
        octets: c.cache,
        note: "Des fichiers gardés en mémoire pour aller plus vite. Windows la rend tout de suite dès qu'une appli en a besoin : ce n'est pas de la mémoire prise.".into(),
    });
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seuils() {
        let s = seuils_pilotes(16.0 * GO);
        assert!((s.fort - GO).abs() < 1.0);
        assert!((s.surveiller - 0.75 * GO).abs() < 1.0);
    }
}
