//! Lot K (petits restants de l'audit du 10/11) : les cartes NVIDIA encore prises en charge par le dernier pilote
//! « Game Ready » (rien n'est lu sur le PC : seulement des noms de cartes).

/// NVIDIA (audit du 10/11, lot E) : la Quadro T2000 (Turing, portables), encore prise en charge par le dernier pilote,
/// manquait dans la liste ; ses variantes « Max-Q » aussi. Les autres Turing pro (T400 à T1200, Quadro RTX) y étaient.
#[test]
fn la_quadro_t2000_est_prise_en_charge() {
    use nevermind_pc::analyse::nvidia_suivie;
    for n in [
        "NVIDIA Quadro T2000",
        "NVIDIA Quadro T2000 with Max-Q Design",
        "NVIDIA Quadro T1000",
        "NVIDIA T550 Laptop GPU",
        "NVIDIA T600 Laptop GPU",
        "NVIDIA T1200 Laptop GPU",
        "NVIDIA Quadro RTX 3000",
        "NVIDIA RTX 2000 Ada Generation",
    ] {
        assert!(nvidia_suivie(n), "{n}");
    }
    // Pascal et avant (branche 580) : toujours dehors, même avec un nombre proche
    for n in ["NVIDIA Quadro P2000", "NVIDIA Quadro M2000", "NVIDIA Quadro K2000", "NVIDIA Quadro P1000"] {
        assert!(!nvidia_suivie(n), "{n}");
    }
}
