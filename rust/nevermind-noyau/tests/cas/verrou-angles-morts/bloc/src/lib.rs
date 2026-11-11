// Cas d'essai du garde-fou du verrou des gestes (lot J) : un faux bloc, jamais construit. Chaque ligne marquée
// « attendu » change le PC sans tenir le verrou, et le relevé d'avant le lot J la laissait passer. Les lignes marquées
// « protégé » tiennent bien le verrou : le relevé ne doit pas les refuser.

// 1. un relais vers l'Aide dont la variable ne s'appelle pas `action`
pub fn relayer(ctx: &C, quoi: &str) -> R {
    ctx.aide(quoi, &json!({}), 1) // attendu
}

// 2. une fonction qui vérifie le verrou mais continue après un refus
pub fn continuer_apres_refus(ctx: &C) {
    let _ = ctx.exiger_geste();
    let _ = ctx.changer_registre("HKCU:\\Essai", "X", &json!(1), "DWord"); // attendu
}

// 3. une écriture hors d'une fonction (une valeur calculée une fois, au premier usage)
static AU_PREMIER_USAGE: LazyLock<()> = LazyLock::new(|| {
    let _ = contexte().ecrire_registre("HKCU:\\Essai", "Y", &json!(1), "DWord"); // attendu
});

// 4. sauvegarde.json écrit par son chemin, sans l'écrivain unique
pub fn ecraser_la_sauvegarde(ctx: &C, texte: &str) {
    let _ = std::fs::write(ctx.fichier_sauvegarde(), texte); // attendu
}

// Ce qui tient bien le verrou, ou ne change rien
pub fn relayer_sous_verrou(ctx: &C, quoi: &str) -> R {
    let _p = ctx.exiger_geste()?;
    ctx.aide(quoi, &json!({}), 1) // protégé
}

pub fn refuser_puis_rendre(ctx: &C) {
    if ctx.exiger_geste().is_err() {
        return;
    }
    let _ = ctx.changer_registre("HKCU:\\Essai", "Z", &json!(1), "DWord"); // protégé
}

pub fn ecrire_si_permis(ctx: &C) {
    if ctx.exiger_geste().is_ok() {
        let _ = ctx.changer_registre("HKCU:\\Essai", "W", &json!(1), "DWord"); // protégé
    }
}

pub fn lire_la_sauvegarde(ctx: &C) -> String {
    std::fs::read_to_string(ctx.fichier_sauvegarde()).unwrap_or_default()
}

pub fn lire_par_l_aide(ctx: &C) -> R {
    ctx.aide_sans_demande("lecture.pc", &json!({}), 1)
}
