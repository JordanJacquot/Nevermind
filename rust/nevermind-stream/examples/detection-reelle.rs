//! Lecture réelle du PC, en lecture seule : les logiciels de stream trouvés (registre et fichiers présents), les
//! processus qui comptent, les cartes graphiques et l'écran. Aucun réglage n'est lu ni changé.
//!
//!   cargo run --offline -p nevermind-stream --example detection-reelle

use nevermind_stream::reel::SystemeReel;
use nevermind_stream::systeme::Systeme;

fn main() {
    let s = SystemeReel;
    let t = std::time::Instant::now();
    let i = s.installations();
    let ms_installations = t.elapsed().as_secs_f64() * 1000.0;
    let t = std::time::Instant::now();
    let p = s.processus();
    let ms_processus = t.elapsed().as_secs_f64() * 1000.0;
    let t = std::time::Instant::now();
    let c = s.cartes();
    let ms_cartes = t.elapsed().as_secs_f64() * 1000.0;
    println!("Logiciels ({ms_installations:.1} ms) : {i:#?}");
    println!("Processus ({ms_processus:.1} ms) : {p:#?}");
    println!("Cartes ({ms_cartes:.1} ms) : {c:#?}");
    println!("Écran : {:?}", s.ecran());
}
