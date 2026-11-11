//! La table des signaux entre les blocs et l'hôte (`Effet::Signal`) : chaque signal envoyé a un récepteur qui
//! l'écoute sous le même nom, ou une raison écrite ici. Les sources de toutes les crates sont lues : un nouveau signal
//! envoyé sans être ajouté à cette table fait échouer le test.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// (signal, qui l'envoie, qui l'écoute). « hôte » : nevermind-appli (hote.rs, hote_communs.rs).
const TABLE: &[(&str, &[&str], &[&str])] = &[
    // Les signaux communs (signaux-communs-nevermind-rust.md), exécutés par l'hôte
    ("question", &["pc", "memoire", "protection", "jeux", "reseau", "trafic", "divers"], &["hôte"]),
    ("occupe", &["pc", "memoire", "protection", "reseau", "jeux"], &["hôte"]),
    ("resultat", &["jeux", "memoire"], &["hôte"]),
    ("historique", &["memoire", "protection", "reseau", "trafic"], &["hôte"]),
    ("sauvegarde-changee", &["pc", "memoire", "reseau"], &["hôte", "pc", "protection"]),
    ("sauvegarde-appliquer", &["trafic"], &["hôte"]),
    ("notification", &["perf", "trafic"], &["hôte"]),
    ("bulle", &["reseau"], &["hôte"]),
    ("veille-jeu-ajouter", &["trafic"], &["hôte"]),
    ("mesure-lag", &["trafic"], &["perf"]),
    ("partie-commencee", &["perf"], &["hôte", "jeux", "memoire", "reseau", "trafic", "stream"]),
    ("partie-finie", &["perf"], &["hôte", "jeux", "memoire", "reseau", "trafic", "stream"]),
    ("theme-choisi", &["divers"], &["hôte"]),
    // (la carte de Nevermind dans l'accueil de Mindset : etat.json, écrit par l'hôte)
    ("etat-mindset", &["pc", "protection"], &["hôte"]),
    // Entre les blocs
    ("mesures", &["perf"], &["pc"]),
    ("mesures-rapides", &["pc"], &["perf"]),
    ("analyse-pc", &["pc"], &["perf", "jeux"]),
    ("reglages-jeu", &["pc"], &["perf", "jeux", "divers"]),
    ("demarrage", &["pc"], &["perf"]),
    ("demarrage-programmes", &["pc"], &["memoire"]),
    ("nettoyage", &["pc"], &["perf"]),
    ("sauvegarde-nombre", &["pc"], &["protection"]),
    ("debit-mesure", &["pc"], &["reseau", "stream"]),
    ("test-debit", &["reseau", "stream"], &["pc"]),
    // (le mode streameur : sa carte dans la page Performances)
    ("streameur", &["stream"], &["perf"]),
    ("connexion", &["reseau"], &["perf"]),
    ("memoire-constat", &["memoire"], &["perf"]),
    ("jeux-liste", &["jeux"], &["perf", "divers"]),
    ("jeux-bibliotheque", &["jeux"], &["memoire"]),
    ("jeux-index", &["jeux"], &["trafic"]),
    ("declarations", &["perf", "jeux"], &["pc"]),
    ("appliquer-reglages-windows", &["jeux"], &["pc"]),
    ("diagnostic-fps", &["jeux"], &["perf"]),
    ("diagnostic-fps-pret", &["perf"], &["jeux"]),
    ("fps-partie-enregistree", &["perf"], &["jeux"]),
    ("ajouter-jeu", &["perf"], &["jeux"]),
    ("ouvrir-fiche", &["perf", "jeux"], &["pc"]),
    ("profils-changes", &["jeux"], &["perf"]),
    ("registre-ecrire", &["protection"], &["pc"]),
    ("demarrage-desactiver", &["protection"], &["pc"]),
    ("historique-annuler", &["protection"], &["pc"]),
    ("analyse-demandee", &["trafic"], &["protection"]),
    ("analyse-finie", &["protection"], &["trafic"]),
    ("jeux-restes-ouvrir", &["divers"], &["jeux"]),
    ("trafic-microsoft", &["divers"], &["trafic"]),
    // Envoyés par l'hôte
    ("etat-hote", &["hôte"], &["divers"]),
];

/// Les signaux envoyés que personne n'écoute, avec leur raison.
const SANS_RECEPTEUR: &[(&str, &str)] = &[
    (
        "reglage-change",
        "le bloc perf relit FpsMeasure, FpsOverlay, FpsOverlayStyle, FpsOverlayCorner et GameMode à chaque passage",
    ),
    ("organizer-ouvrir", "la fenêtre de l'Organizer (autre chantier, nevermind-jeux/src/organizer.rs)"),
    ("organizer", "la fenêtre de l'Organizer (autre chantier)"),
    ("visite", "l'action « visite » est prise par l'assistance du noyau avant le bloc divers : jamais envoyé"),
    (
        "reseau-carte",
        "la carte du réseau (constellation WPF) n'existe plus dans l'écran : la recherche mène à la page Réseau",
    ),
    ("ecran", "l'hôte dit l'écran montré ; les blocs le reçoivent aussi dans pages()"),
    ("mindset", "l'hôte dit l'accrochage dans Mindset ; aucun bloc n'en a besoin aujourd'hui"),
];

/// Les signaux écoutés que personne n'envoie, avec leur raison.
const SANS_EMETTEUR: &[(&str, &str)] = &[
    ("protection-relire", "gardé pour un bloc qui voudrait faire relire la protection (Update-SecurityTab)"),
    ("jeux-rechercher", "gardé pour relancer la recherche des jeux depuis un autre bloc"),
];

fn racine() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn sources(dossier: &Path, l: &mut Vec<String>) {
    for e in std::fs::read_dir(dossier).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            sources(&p, l);
        } else if p.extension().is_some_and(|x| x == "rs") {
            l.push(std::fs::read_to_string(&p).unwrap());
        }
    }
}

/// Le texte de toutes les sources d'une crate (`src` seulement : pas les tests), sans les espaces.
fn texte_crate(nom: &str) -> String {
    let d = if nom == "hôte" { "nevermind-appli".to_string() } else { format!("nevermind-{nom}") };
    let mut l = Vec::new();
    sources(&racine().join(d).join("src"), &mut l);
    // (sans les tests de l'hôte, qui envoient des signaux factices)
    l.retain(|t| !t.starts_with("//! Les tests de l'hôte") && !t.contains("struct Factice"));
    l.join("\n").chars().filter(|c| !c.is_whitespace()).collect()
}

/// Les noms qui suivent un motif (`nom:"`, `sig("`...).
fn apres(t: &str, motif: &str) -> Vec<String> {
    let mut r = Vec::new();
    let mut reste = t;
    while let Some(i) = reste.find(motif) {
        reste = &reste[i + motif.len()..];
        let n: String = reste.chars().take_while(|c| c.is_ascii_lowercase() || *c == '-' || *c == ':').collect();
        if !n.is_empty() && reste[n.len()..].starts_with('"') {
            r.push(n);
        }
    }
    r
}

/// Les signaux envoyés par une crate (les motifs d'écriture utilisés dans les blocs).
fn envoyes(nom: &str) -> BTreeSet<String> {
    let t = texte_crate(nom);
    let mut s = BTreeSet::new();
    for m in ["Signal{nom:\"", "sig(\"", "signal(\"", "signal(effets,\""] {
        s.extend(apres(&t, m));
    }
    // (le bloc trafic nomme ses signaux par des constantes : `nom: signaux::X.into()`)
    let mut reste = t.as_str();
    while let Some(i) = reste.find("Signal{nom:signaux::") {
        reste = &reste[i + "Signal{nom:signaux::".len()..];
        let k: String = reste.chars().take_while(|c| c.is_ascii_uppercase() || *c == '_').collect();
        let def = format!("pubconst{k}:&str=\"");
        if let Some(v) = apres(&t, &def).into_iter().next() {
            s.insert(v);
        }
    }
    // (l'hôte : le signal « etat-hote » donné à tous les blocs)
    if nom == "hôte" {
        s.extend(apres(&t, "self.signaler(None,\""));
    }
    s
}

#[test]
fn chaque_signal_envoye_a_son_recepteur() {
    let crates = ["pc", "perf", "memoire", "protection", "jeux", "reseau", "trafic", "divers", "stream", "hôte"];
    let mut erreurs = Vec::new();
    // 1. Tout ce qui est envoyé est dans la table (ou sans récepteur, avec sa raison), et par les bons blocs
    for c in crates {
        for n in envoyes(c) {
            let dans_table = TABLE.iter().find(|(s, _, _)| *s == n);
            match dans_table {
                Some((_, de, _)) if !de.contains(&c) => {
                    erreurs.push(format!("« {n} » envoyé par {c}, pas noté dans la table"))
                }
                Some(_) => {}
                None if SANS_RECEPTEUR.iter().any(|(s, _)| *s == n) => {}
                None => erreurs.push(format!("« {n} » envoyé par {c} : ni dans la table, ni justifié")),
            }
        }
    }
    // 2. Chaque ligne de la table : l'émetteur l'envoie vraiment, le récepteur l'écoute sous ce nom
    for (n, de, vers) in TABLE {
        for c in *de {
            if !envoyes(c).contains(*n) {
                erreurs.push(format!("« {n} » : {c} ne l'envoie pas"));
            }
        }
        for c in *vers {
            let t = texte_crate(c);
            let ecoute = t.contains(&format!("\"{n}\"=>"))
                || t.contains(&format!("\"{n}\"|"))
                || t.contains(&format!("|\"{n}\""))
                || t.contains(&format!("=\"{n}\";"));
            if !ecoute {
                erreurs.push(format!("« {n} » : {c} ne l'écoute pas"));
            }
        }
    }
    // 3. Les signaux écoutés sans émetteur sont vraiment écoutés
    for (n, _) in SANS_EMETTEUR {
        assert!(
            crates.iter().all(|c| !envoyes(c).contains(*n)),
            "« {n} » a maintenant un émetteur : à mettre dans la table"
        );
    }
    assert!(erreurs.is_empty(), "{}", erreurs.join("\n"));
}
