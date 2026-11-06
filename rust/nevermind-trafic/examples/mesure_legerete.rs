//! Mesure de légèreté (lectures seulement, rien n'est changé) : le coût d'un passage de la surveillance du Trafic
//! (connexions TCP, processus, services) lu directement par Windows.

fn main() {
    use nevermind_trafic::windows;
    let mut c = windows::CompteursTcp::default();
    let n = 50;
    let t = std::time::Instant::now();
    let mut lignes = 0;
    for _ in 0..n {
        lignes = c.lignes().len();
        let _ = windows::processus();
    }
    let passage = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
    let t = std::time::Instant::now();
    for _ in 0..10 {
        let _ = windows::services();
    }
    let svc = t.elapsed().as_secs_f64() * 1000.0 / 10.0;
    println!("connexions : {lignes} ; passage (TCP + processus) : {passage:.2} ms ; services : {svc:.2} ms");
}
