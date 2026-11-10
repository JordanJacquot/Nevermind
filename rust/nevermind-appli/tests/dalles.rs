//! Les dalles d'outils (page Performances, Protection : kit.js `case 'outils'`, `.outil` de kit.css), second audit du
//! 09/11. L'allure validée par Jordan (modèle « D » : résumé court, barre fine en bas, état en bas à droite) ne change
//! pas au repos ; deux choses en plus :
//! - au clavier ou à la manette (focus visible), la note entière s'affiche dans une bulle sous la dalle : avant, elle
//!   n'était que l'infobulle, donc seulement à la souris (« Ping et stabilité » sans dire ce qu'est le ping) ;
//! - le jugement dit aussi en mots (`jugement`, ajouté après l'état : « 1,5 Go à nettoyer », « 2 points à voir »),
//!   pas seulement par la couleur.

use std::path::Path;

fn lire(nom: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui").join(nom);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Le rendu d'une dalle dans kit.js (`case 'outils'`, jusqu'au cas suivant)
fn rendu_des_dalles() -> String {
    let js = lire("kit.js");
    let debut = js.find("      case 'outils':").expect("kit.js : le rendu des dalles a disparu");
    let fin = js[debut..].find("      case 'raccourcis':").expect("kit.js : la fin du rendu des dalles");
    js[debut..debut + fin].to_string()
}

/// Une règle de kit.css (le texte entre `{` et `}` du sélecteur donné tel quel), sans espaces autour
fn regle(css: &str, selecteur: &str) -> String {
    let debut = css.find(&format!("{selecteur} {{")).unwrap_or_else(|| panic!("kit.css : « {selecteur} » a disparu"));
    let reste = &css[debut + selecteur.len() + 2..];
    reste[..reste.find('}').unwrap()].trim().to_string()
}

#[test]
fn la_note_d_une_dalle_se_voit_au_clavier() {
    let r = rendu_des_dalles();
    // (la note dans une bulle de la dalle active, cachée aux lecteurs d'écran : l'infobulle la leur dit déjà)
    assert!(r.contains(r#"<span class="bulle-o" aria-hidden="true">${esc(x.note)}</span>"#), "{r}");
    assert!(r.contains(r#"title="${esc(x.note)}""#), "l'infobulle de la souris a disparu");
    let css = lire("kit.css");
    // Cachée au repos et au survol (l'allure validée ne change pas), montrée au focus visible seulement
    assert!(regle(&css, ".outil .bulle-o").starts_with("display: none;"), "{}", regle(&css, ".outil .bulle-o"));
    assert!(regle(&css, ".outil .bulle-o").contains("position: absolute;"), "la bulle déplacerait les dalles");
    assert_eq!(regle(&css, ".outil.f:focus-visible .bulle-o"), "display: block;");
    assert!(!css.contains(".outil.f:hover .bulle-o") && !css.contains(".outil:hover .bulle-o"));
}

#[test]
fn le_jugement_d_une_dalle_se_dit_en_mots() {
    let r = rendu_des_dalles();
    assert!(r.contains("const dit = x.etiquette && x.jugement ? `${x.etiquette} ${x.jugement}` : x.etiquette;"), "{r}");
    assert!(r.contains("${esc(dit)}</span>"), "l'état écrit n'est plus celui qui porte le jugement : {r}");
}
