//! La page montrée ne doit pas se redessiner au rythme de l'écran (mesuré le 06/11/2026 sur un écran à 360 Hz :
//! la 3D et les bords de plasma redessinés 360 fois par seconde, environ 150 % d'un cœur page Performances).
//! - la boucle de plasma.js dessine au plus 60 images par seconde ;
//! - aucune animation CSS sans fin n'avance en continu (`linear infinite`) : elle avance par pas (`steps(n)`), au plus
//!   60 pas par seconde, ce qui donne le même mouvement à l'oeil.

use std::path::Path;

fn lire(nom: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui").join(nom);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// La durée d'une animation (`7s`, `2.2s`, `300ms`) en secondes.
fn secondes(t: &str) -> Option<f64> {
    if let Some(ms) = t.strip_suffix("ms") {
        return ms.parse::<f64>().ok().map(|v| v / 1000.0);
    }
    t.strip_suffix('s')?.parse().ok()
}

#[test]
fn la_boucle_3d_est_limitee_a_60_images_par_seconde() {
    let js = lire("plasma.js");
    assert!(js.contains("const PAS_IMAGE = 1000 / 60;"), "plasma.js : la limite de 60 images par seconde a disparu");
    let debut = js.find("function image(maintenant)").expect("plasma.js : la boucle image(maintenant) a disparu");
    let fin = js[debut..].find("\n  }").expect("plasma.js : fin de la boucle image");
    let boucle = &js[debut..debut + fin];
    let saut = boucle
        .find("if (maintenant - derniere < PAS_IMAGE - 1) return;")
        .expect("plasma.js : l'image en trop n'est plus sautée");
    // (le saut doit venir avant tout dessin de la 3D)
    for dessin in ["acc.image", "sol.image", "anneaux("] {
        if let Some(i) = boucle.find(dessin) {
            assert!(saut < i, "plasma.js : {dessin} est appelé avant le saut des images en trop");
        }
    }
    assert_eq!(js.matches("requestAnimationFrame(").count(), 2, "plasma.js : une autre boucle d'animation est apparue");
}

#[test]
fn les_animations_sans_fin_avancent_par_pas() {
    let css = lire("kit.css");
    let mut vues = 0;
    for (i, _) in css.match_indices("animation:") {
        let regle: String = css[i + "animation:".len()..].chars().take_while(|c| *c != ';' && *c != '}').collect();
        let mots: Vec<&str> = regle.split_whitespace().collect();
        if !mots.contains(&"infinite") {
            continue;
        }
        assert!(!mots.contains(&"linear"), "kit.css : animation sans fin en continu ({})", regle.trim());
        if let Some(pas) = mots.iter().find(|m| m.starts_with("steps(")) {
            vues += 1;
            let n: f64 = pas.trim_start_matches("steps(").trim_end_matches(')').parse().expect("steps(n)");
            let duree = mots.iter().find_map(|m| secondes(m)).expect("durée de l'animation");
            assert!(n / duree <= 60.5, "kit.css : plus de 60 pas par seconde ({})", regle.trim());
        }
    }
    assert!(vues >= 4, "kit.css : les animations en pas ont disparu ({vues})");
}
