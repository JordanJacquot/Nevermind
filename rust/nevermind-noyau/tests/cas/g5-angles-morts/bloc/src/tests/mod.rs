// Cas d'essai du garde-fou G5 (lot J) : du code livré dans un dossier `src/tests/` (`pub mod tests;`), comme
// nevermind-pc/src/tests/ (les tests de connexion de Ton PC). Voir ../lib.rs
pub fn mesurer(ctx: &C) {
    let _ = ctx.aide("lecture.pc", &json!({}), 1); // attendu
}
