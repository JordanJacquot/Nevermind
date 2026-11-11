//! Lot K (audit3 du lot A, défaut 6) : un `sauvegarde.json.<pid>-<n>.tmp` laissé par un arrêt brutal de Nevermind
//! restait dans le dossier des données, même après une désinstallation qui garde les données (seul
//! `sauvegarde.json.tmp` partait). Rien n'est effacé ici : seulement la liste de ce qui part.

use nevermind_desinstalleur::donnees::{PARTENT, correspond};

#[test]
fn les_fichiers_temporaires_de_la_sauvegarde_partent_avec_elle() {
    let part = |n: &str| PARTENT.iter().any(|f| correspond(n, f));
    for n in ["sauvegarde.json.4242-0.tmp", "sauvegarde.json.17-12.tmp", "sauvegarde.json.tmp"] {
        assert!(part(n), "{n} reste après la désinstallation");
    }
    for n in ["historique.json.4242-0.tmp", "autre.json.1-1.tmp", "sauvegarde.json.4242-0.tmp.txt"] {
        assert!(!part(n), "{n} ne doit pas partir avec la sauvegarde");
    }
}
