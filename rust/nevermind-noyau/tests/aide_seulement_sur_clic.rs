//! Garde-fou de l'audit G5 : `aide(` est la seule voie qui peut proposer la demande de Windows. Elle est réservée aux
//! gestes que l'utilisateur vient de demander d'un clic (changer un réglage du PC, bloquer un programme, nettoyer...).
//! Les lectures, les mesures et les gestes de fond (minuterie, lancement d'un jeu, fin d'une lecture) passent par
//! `aide_sans_demande` ou `aide_lecture` : sinon, si la tâche de l'Aide manque, une demande de Windows s'ouvrirait en
//! pleine partie.
//!
//! Ce test relève chaque appel `.aide(` des sources du workspace et refuse toute action qui n'est pas dans la liste des
//! gestes sur clic ci-dessous. Ajouter une action à cette liste, c'est affirmer qu'elle ne part JAMAIS d'une minuterie.

use std::path::{Path, PathBuf};

/// Les actions de l'Aide permises avec `aide(` : chacune ne part que d'un clic (ou du « Oui » qui le suit)
const GESTES_SUR_CLIC: &[&str] = &[
    "reg.ecrire",                   // sauvegarde, corrections, réseau (bureau à distance) : boutons
    "reg.effacer",                  // retour en arrière : bouton
    "service.regler",               // couper un service, revenir en arrière : boutons
    "restauration.creer",           // point de restauration : bouton, ou avant « Tout corriger »
    "pare-feu.debloquer",           // revenir en arrière : bouton
    "pare-feu.debloquer-tout",      // « Tout restaurer » : bouton
    "pare-feu.bloquer",             // trafic, « Bloquer » puis « Oui »
    "pare-feu.activer",             // protection et réseau, « Réactiver le pare-feu »
    "origines.oublier",             // « Tout restaurer » : bouton
    "nettoyage.lister",             // « Analyser » du nettoyage : bouton
    "nettoyage.lancer",             // « Nettoyer » : bouton
    "antivirus.retirer-exclusions", // protection, « Retirer » puis « Oui »
    "wifi.manuel",                  // réseau, geste de l'audit : bouton
    "smb1.couper",                  // réseau, geste de l'audit : bouton
    "dns.regler",                   // réseau et retour en arrière : boutons
    "carte.relancer",               // réseau, « Relancer la carte » : bouton
];

/// Les fonctions qui transmettent une action reçue (pas de nom écrit) : celui qui les appelle est relevé à sa place
const RELAIS: &[&str] = &[
    "nevermind-memoire/src/pc.rs", // AideDuContexte::demander (service.regler, sur clic)
    "nevermind-pc/src/systeme.rs", // Pc::aide de PcWindows (les appels pc.aide( sont relevés eux aussi)
    "nevermind-pc/src/pc.rs",      // Pc::aide_lecture par défaut (désinstalleur et faux PC ; PcWindows a la sienne)
];

fn sources(d: &Path, r: &mut Vec<PathBuf>) {
    let Ok(l) = std::fs::read_dir(d) else { return };
    for e in l.flatten() {
        let p = e.path();
        let nom = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            // (les tests ont leurs fausses Aides ; target : ce qui est construit)
            if nom != "target" && nom != "tests" && !nom.starts_with('.') {
                sources(&p, r);
            }
        } else if nom.ends_with(".rs") {
            r.push(p);
        }
    }
}

/// Chaque appel `.aide(` : (fichier, ligne, action écrite ou `None`)
fn appels(racine: &Path) -> Vec<(String, usize, Option<String>)> {
    let mut fichiers = Vec::new();
    sources(racine, &mut fichiers);
    let mut r = Vec::new();
    for f in fichiers {
        let t = std::fs::read_to_string(&f).unwrap_or_default();
        let rel = f.strip_prefix(racine).unwrap_or(&f).to_string_lossy().replace('\\', "/");
        let mut depuis = 0;
        while let Some(i) = t[depuis..].find(".aide(") {
            let debut = depuis + i;
            depuis = debut + ".aide(".len();
            let ligne = t[..debut].matches('\n').count() + 1;
            let suite = t[depuis..].trim_start();
            let action = suite.strip_prefix('"').and_then(|s| s.find('"').map(|n| s[..n].to_string()));
            r.push((rel.clone(), ligne, action));
        }
    }
    r
}

#[test]
fn aide_avec_demande_seulement_pour_un_clic() {
    let racine = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    let l = appels(&racine);
    assert!(l.len() >= 20, "les appels de l'Aide n'ont pas été trouvés ({} seulement)", l.len());
    let mut refus = Vec::new();
    for (f, n, a) in &l {
        match a {
            Some(a) if GESTES_SUR_CLIC.contains(&a.as_str()) => {}
            Some(a) => {
                refus.push(format!("{f}:{n} : « {a} » n'est pas un geste sur clic (aide_sans_demande ou aide_lecture)"))
            }
            None if RELAIS.contains(&f.as_str()) => {}
            None => refus.push(format!("{f}:{n} : action non écrite, à vérifier puis à noter dans RELAIS")),
        }
    }
    assert!(
        refus.is_empty(),
        "appels de l'Aide qui peuvent proposer la demande de Windows hors d'un clic :\n{}",
        refus.join("\n")
    );
}

/// Le relevé lui-même : il voit un appel écrit sur plusieurs lignes et une lecture interdite
#[test]
fn le_releve_voit_les_lectures() {
    let d = nevermind_noyau::dossier_essai("releve-aide");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("bloc/src")).unwrap();
    std::fs::write(
        d.join("bloc/src/lib.rs"),
        "fn tic(ctx: &C) { let _ = ctx.aide(\n    \"lecture.pc\", &json!({}), 10); let _ = ctx.aide_sans_demande(\"trafic.tcp\", &v, 1); }",
    )
    .unwrap();
    let l = appels(&d);
    assert_eq!(l, vec![("bloc/src/lib.rs".to_string(), 1, Some("lecture.pc".to_string()))]);
    let _ = std::fs::remove_dir_all(&d);
}
