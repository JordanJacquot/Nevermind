// Cas d'essai du garde-fou G5 (lot J) : un faux bloc, jamais construit. Chaque ligne marquée « attendu » est une
// demande de Windows (ou une écriture du registre) atteinte depuis un tic par un chemin que le relevé d'avant le lot J
// ne suivait pas. Le relevé doit la voir.
pub mod tests;
mod lag;

impl Bloc for B {
    fn tic(&mut self, ctx: &C, effets: &mut E) {
        // 1. un appel sur une autre valeur (comme Ton PC : `at.session.repondre_ecran(`)
        self.session.repondre(ctx);
        // 2. une fonction d'un type (comme `Session::demarrer` du lag)
        let _ = Session::demarrer(ctx);
        // 3. du code livré dans un dossier `src/tests/` (comme nevermind-pc/src/tests/)
        tests::mesurer(ctx);
        // 4. un signal envoyé par le tic : son traitement doit être suivi
        effets.ajouter(Effet::Signal { nom: "partie-commencee".into(), donnees: json!({}) });
    }

    fn signal(&mut self, ctx: &C, nom: &str, _donnees: &V, _effets: &mut E) {
        match nom {
            "partie-commencee" => {
                let _ = ctx.aide("reg.ecrire", &json!({}), 1); // attendu
            }
            // (un signal envoyé seulement par un clic : son traitement peut demander, il n'est pas suivi)
            "demarrage-desactiver" => {
                let _ = ctx.aide("reg.effacer", &json!({}), 1);
            }
            _ => {}
        }
    }
}
