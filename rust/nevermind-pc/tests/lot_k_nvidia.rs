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

/// Relecture du 14/11 : Volta (TITAN V, Quadro GV100) et les MX Pascal ou Maxwell (MX 1xx à 3xx) restent sur la branche
/// 580, comme Pascal ; les cartes Turing et après les plus récentes, portables compris, sont bien suivies.
#[test]
fn volta_et_mx_anciennes_hors_liste_cartes_recentes_dedans() {
    use nevermind_pc::analyse::nvidia_suivie;
    for n in [
        "NVIDIA GeForce RTX 5090",
        "NVIDIA GeForce RTX 5060 Laptop GPU",
        "NVIDIA RTX PRO 6000 Blackwell Workstation Edition",
        "NVIDIA GeForce GTX 1630",
        "NVIDIA GeForce MX550",
        "NVIDIA GeForce MX570",
        "NVIDIA T500",
        "NVIDIA T400 4GB",
    ] {
        assert!(nvidia_suivie(n), "{n}");
    }
    for n in [
        "NVIDIA TITAN V",
        "NVIDIA Quadro GV100",
        "NVIDIA GeForce MX350",
        "NVIDIA GeForce MX250",
        "NVIDIA GeForce MX130",
        "NVIDIA GeForce GTX 1050 Ti",
        "NVIDIA Tesla T4",
        "Intel(R) Arc(TM) A770 Graphics",
    ] {
        assert!(!nvidia_suivie(n), "{n}");
    }
}
