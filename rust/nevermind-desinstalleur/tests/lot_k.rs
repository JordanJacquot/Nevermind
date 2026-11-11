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

/// Lot J (le défaut que le lot K a corrigé dans Ton PC, resté dans le désinstalleur) : pendant « Tout restaurer » de
/// la désinstallation, la lecture des règles du pare-feu qui échoue comptait 0 règle. Le déblocage n'était pas tenté
/// et la ligne « Internet bloqué pour X » était marquée annulée alors que la règle était peut-être toujours là. Le Pc
/// de la désinstallation dit maintenant qu'il n'a pas pu lire (`None`), comme celui de Nevermind. Le vrai pare-feu
/// n'est pas lu : sa lecture échoue par l'essai.
#[cfg(windows)]
#[test]
fn pare_feu_illisible_pendant_la_desinstallation_n_est_pas_zero_regle() {
    use nevermind_desinstalleur::os::PcDesinstallation;
    use nevermind_desinstalleur::sur_place::AideSurPlace;
    use nevermind_pc::pc::Pc;
    let d = nevermind_noyau::dossier_essai("desinstalleur-lot-j-pare-feu");
    let _ = std::fs::remove_dir_all(&d);
    let ctx = nevermind_noyau::Contexte::nouveau(d.join("a"), d.join("d"), false, true);
    let aide = AideSurPlace::nouvelle().avec_pare_feu_illisible();
    let pc = PcDesinstallation { pc: nevermind_pc::systeme::PcWindows::nouveau(&ctx), aide: &aide, ctx: &ctx };
    assert_eq!(pc.regles_pare_feu_lues(), None, "une lecture ratée est prise pour « aucune règle »");
    assert_eq!(pc.regles_pare_feu(), 0);
    let _ = std::fs::remove_dir_all(&d);
}
