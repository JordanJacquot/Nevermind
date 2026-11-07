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

/// Les bulles des anneaux (page Ton PC : processeur, mémoire vive, carte graphique, température) ne doivent pas
/// s'emballer quand la valeur change (vu par Jordan le 07/11/2026, Nevermind ouvert depuis 14 h) : leur position
/// n'est plus « secondes depuis l'ouverture × vitesse », mais un courant avancé image par image, gardé entre 0 et 2π.
#[test]
fn les_bulles_des_anneaux_ne_s_emballent_pas_quand_la_valeur_change() {
    let js = lire("plasma.js");
    assert!(
        !js.contains("t * (6.0 + v * 5.0)"),
        "plasma.js : la position des bulles multiplie de nouveau le temps par la vitesse"
    );
    assert!(
        js.contains("sin(u * 70.0 - ph)"),
        "plasma.js : le shader de l'anneau ne lit plus le courant de bulles (ph)"
    );
    assert!(js.contains("% TOUR"), "plasma.js : le courant de bulles n'est plus gardé entre 0 et 2π");
    assert_eq!(js.matches("ph: { value: 0 }").count(), 2, "plasma.js : un anneau n'a plus son courant de bulles");
    assert_eq!(js.matches("vitesseBulles(").count(), 2, "plasma.js : un anneau n'avance plus son courant de bulles");
}
