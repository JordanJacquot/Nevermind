//! Les mesures des tests des composants (natif.cs) : le calcul du processeur (le même que .NET, au bit près), la
//! mémoire (petits morceaux), le débit et le lag contre un faux réseau (jamais Internet), l'arrêt en cours de route,
//! et deux courts essais réels : un disque (un fichier d'essai dans %TEMP%, toujours effacé) et le processeur.

use nevermind_pc::tests::mesures::{self, Compteur, Moteur, Reseau};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Les résultats de `CpuWork` (natif.cs), refaits dans PowerShell avec les mêmes opérations (entiers de 64 bits
/// qui débordent, doubles de .NET), pour comparer au bit près
const CPU_DOTNET: [(i64, i64, i64); 3] =
    [(12345, 0, 0), (12345, 1000, 3554153303644800288), (-7, 2000, -591003077293101239)];

#[test]
fn calcul_du_processeur_comme_dotnet() {
    for (graine, tours, attendu) in CPU_DOTNET {
        assert_eq!(mesures::calcul(graine, tours), attendu, "CpuWork({graine}, {tours})");
    }
    // (toujours le même résultat : c'est ce qui permet de voir un processeur instable)
    assert_eq!(mesures::calcul(12345, 2_000_000), mesures::calcul(12345, 2_000_000));
}

#[test]
fn memoire_sans_erreur_et_arret() {
    let m = Moteur::default();
    // 16 morceaux de 1024 nombres (128 Ko)
    let r = mesures::memoire_par(&m, 16 * 1024 * 8, 1024).expect("pas arrêté");
    assert_eq!(r.len(), 5);
    assert_eq!(r[3], 0.0, "erreurs");
    assert!((r[4] - 16.0 * 8192.0 / 1_073_741_824.0).abs() < 1e-15, "Go vérifiés : {}", r[4]);
    assert!(r[0] > 0.0 && r[1] > 0.0 && r[2] > 0.0);
    assert_eq!(m.lire().1, 100.0);
    assert_eq!(m.lire().0, "copy");
    // (au moins 2 morceaux, même pour très peu d'octets)
    let r = mesures::memoire_par(&m, 10, 1024).unwrap();
    assert!((r[4] - 2.0 * 8192.0 / 1_073_741_824.0).abs() < 1e-15);
    let m = Moteur::default();
    m.annuler.store(true, Ordering::Relaxed);
    assert!(mesures::memoire_par(&m, 16 * 1024 * 8, 1024).is_none());
}

/// Un faux réseau : des pings donnés à l'avance, et des serveurs qui répondent (ou refusent)
struct FauxReseau {
    pings: Mutex<Vec<Option<f64>>>,
    refuses: Vec<&'static str>,
    demandes: Mutex<Vec<String>>,
    flux: AtomicUsize,
}

impl FauxReseau {
    fn nouveau(pings: Vec<Option<f64>>, refuses: Vec<&'static str>) -> Self {
        FauxReseau { pings: Mutex::new(pings), refuses, demandes: Mutex::new(Vec::new()), flux: AtomicUsize::new(0) }
    }
}

impl Reseau for FauxReseau {
    fn ping(&self, _hote: &str, _delai: u32) -> Option<f64> {
        let mut p = self.pings.lock().unwrap();
        if p.is_empty() { Some(20.0) } else { p.remove(0) }
    }
    fn flux(&self, url: &str, _envoi: bool, c: &Compteur) -> Result<(), String> {
        self.demandes.lock().unwrap().push(url.into());
        if self.refuses.contains(&url) {
            return Err("refusé".into());
        }
        self.flux.fetch_add(1, Ordering::Relaxed);
        // (125 000 octets toutes les 10 ms : 100 Mb/s par flux)
        while c.encore() {
            c.ajouter(125_000);
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
}

#[test]
fn debit_contre_un_faux_reseau() {
    let m = Moteur::default();
    let r = FauxReseau::nouveau(vec![], vec!["https://a"]);
    let t = Instant::now();
    let d = mesures::debit(&m, &r, &["https://a", "https://b"], false, 1.0, 2, 10.0, 55.0);
    assert!(t.elapsed() < Duration::from_secs(3));
    // (deux flux à environ 100 Mb/s chacun)
    assert!(d > 100.0 && d < 260.0, "débit {d}");
    assert_eq!(m.lire().1, 55.0);
    let dem = r.demandes.lock().unwrap().clone();
    assert!(dem.contains(&"https://a".to_string()) && dem.contains(&"https://b".to_string()), "{dem:?}");
    // Aucun serveur ne répond : -1
    let r = FauxReseau::nouveau(vec![], vec!["https://a", "https://b"]);
    assert_eq!(mesures::debit(&m, &r, &["https://a", "https://b"], true, 1.0, 3, 0.0, 0.0), -1.0);
}

#[test]
fn lag_en_charge_contre_un_faux_reseau() {
    let m = Moteur::default();
    // Au repos : 30, 10, 20 (médiane 20, gigue des valeurs triées 10), puis des pings perdus pendant la charge
    let mut p = vec![Some(30.0), Some(10.0), Some(20.0)];
    p.extend(std::iter::repeat_n(None, 2));
    p.extend(std::iter::repeat_n(Some(80.0), 40));
    let r = FauxReseau::nouveau(p, vec![]);
    let x = mesures::lag(&m, &r, &["https://d"], &["https://u"], "1.1.1.1", 0.4, 2.6).expect("pas arrêté");
    assert_eq!(x.len(), 9);
    assert_eq!(x[0], 20.0, "médiane au repos");
    assert_eq!(x[8], 10.0, "gigue au repos");
    assert_eq!(x[3], 0.0, "pertes au repos");
    assert!(x[4] > 0.0, "pertes pendant le téléchargement : {}", x[4]);
    assert_eq!(x[2], 80.0);
    assert!(x[6] > 0.0 && x[7] > 0.0, "débits {} {}", x[6], x[7]);
    // (le débit mesuré pendant le lag ne touche pas la progression : elle reste dans l'étape de l'envoi, 62 à 100 ;
    //  comme LoadedLatency, elle ne passe pas à 100 à la fin, le panneau le fait)
    let p = m.lire().1;
    assert!((62.0..=100.0).contains(&p), "progression {p}");
    let m = Moteur::default();
    m.annuler.store(true, Ordering::Relaxed);
    assert!(mesures::lag(&m, &r, &["https://d"], &["https://u"], "1.1.1.1", 0.4, 2.6).is_none());
}

#[test]
fn recherche_d_erreurs() {
    let m = Moteur::default();
    let l = mesures::erreurs(&m, &["C".into(), "D".into()], &|x| {
        if x == "C" { Ok("NoErrorsFound".into()) } else { Err("Accès refusé.".into()) }
    });
    assert_eq!(l, vec!["C|NoErrorsFound".to_string(), "D|ERR Accès refusé.".to_string()]);
    assert_eq!(m.lire(), ("scan".to_string(), 100.0, 0.0));
}

fn fichier_essai(nom: &str) -> String {
    std::env::temp_dir()
        .join(format!("nevermind-essai-{nom}-{}.tmp", std::process::id()))
        .to_string_lossy()
        .into_owned()
}

/// Un court essai réel : un fichier d'essai de 16 Mo dans %TEMP% (écrit, relu, puis effacé)
#[cfg(windows)]
#[test]
fn disque_essai_reel_court() {
    let f = fichier_essai("disque");
    let m = Moteur::default();
    let r = mesures::disque(&m, &f, 16 * 1024 * 1024).expect("le test du disque a échoué").expect("pas arrêté");
    assert!(!std::path::Path::new(&f).exists(), "le fichier d'essai doit être effacé");
    assert_eq!(r.len(), 4);
    assert!(r.iter().all(|x| *x > 0.0), "{r:?}");
    assert_eq!(m.lire().1, 100.0);
    eprintln!(
        "Disque (16 Mo dans %TEMP%) : écriture {:.0} Mo/s, lecture {:.0} Mo/s, {:.0} lectures 4K/s",
        r[0], r[1], r[3]
    );
    // Arrêté avant le début : rien, et toujours pas de fichier
    let m = Moteur::default();
    m.annuler.store(true, Ordering::Relaxed);
    assert_eq!(mesures::disque(&m, &f, 16 * 1024 * 1024), Ok(None));
    assert!(!std::path::Path::new(&f).exists());
    // Un dossier qui n'existe pas : l'erreur de Windows, en texte
    let e = mesures::disque(&Moteur::default(), r"Z:\nevermind\pas-la\essai.tmp", 8 * 1024 * 1024);
    assert!(matches!(e, Err(ref x) if !x.is_empty()), "{e:?}");
}

/// Un court essai réel du processeur (une demi-seconde sur un cœur, une demi-seconde sur tous)
#[test]
fn processeur_essai_reel_court() {
    let m = Moteur::default();
    let t = Instant::now();
    let r = mesures::cpu(&m, 0.5, 0.5, || {}).expect("pas arrêté");
    assert!(t.elapsed() < Duration::from_secs(5));
    assert_eq!(r[2], 0.0, "erreurs de calcul");
    assert_eq!(r[3], std::thread::available_parallelism().unwrap().get() as f64);
    assert!(r[0] > 0.0 && r[1] >= r[0] * 0.5, "{r:?}");
    eprintln!("Processeur : {:.0} points sur un cœur, {:.0} sur {} threads", r[0], r[1], r[3]);
    let m = Moteur::default();
    m.annuler.store(true, Ordering::Relaxed);
    assert!(mesures::cpu(&m, 0.5, 0.5, || {}).is_none());
}
