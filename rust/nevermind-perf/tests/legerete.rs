//! La mesure de légèreté sur le vrai PC, en lecture seule (rien n'est écrit, aucune fenêtre, aucun programme lancé) :
//! une minute écran regardé, une minute écran caché, sans jeu. À lancer à la main :
//!   cargo test --release --offline -p nevermind-perf --test legerete -- --ignored --nocapture

#[cfg(windows)]
#[test]
#[ignore]
fn legerete_sur_le_vrai_pc() {
    use nevermind_noyau::{Bloc, Contexte, Effets, dossier_essai};
    use nevermind_perf::BlocPerf;
    use nevermind_perf::windows::temps_processeur_soi;
    use std::time::{Duration, Instant};

    let d = dossier_essai("perf-legerete");
    let ctx = Contexte::nouveau(d.join("appli"), d.join("donnees"), false, true);
    let mut b = BlocPerf::default();
    let secondes: u64 = std::env::var("LEGERETE_SECONDES").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
    for cache in [false, true] {
        let l0 = b.lectures();
        let c0 = temps_processeur_soi();
        let t0 = Instant::now();
        let mut e = Effets::default();
        while t0.elapsed() < Duration::from_secs(secondes) {
            let attente = b.tic(&ctx, cache, &mut e).unwrap_or(Duration::from_secs(1));
            e.0.clear();
            std::thread::sleep(attente.min(Duration::from_secs(secondes).saturating_sub(t0.elapsed())));
        }
        let cpu_ms = (temps_processeur_soi() - c0) as f64 / 10_000.0;
        let duree = t0.elapsed().as_secs_f64();
        println!(
            "LEGERETE ecran {} : {:.1} s, processeur {:.1} ms ({:.3} % d'un cœur), {} lectures de Windows ({:.0} par minute) ; mesures : {:?}",
            if cache { "caché" } else { "regardé" },
            duree,
            cpu_ms,
            100.0 * cpu_ms / 1000.0 / duree,
            b.lectures() - l0,
            (b.lectures() - l0) as f64 * 60.0 / duree,
            b.direct()
        );
    }
    let _ = std::fs::remove_dir_all(&d);
}
