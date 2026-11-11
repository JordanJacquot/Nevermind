// Cas d'essai du garde-fou G5 (lot J) : voir ../lib.rs
pub struct Session;

impl Session {
    pub fn repondre(&mut self, ctx: &C) {
        let _ = ctx.ecrire_registre("HKLM", "X", &json!(1), "DWord"); // attendu
    }

    pub fn demarrer(ctx: &C) -> Session {
        let _ = ctx.aide("reg.effacer", &json!({}), 1); // attendu
        Session
    }
}
