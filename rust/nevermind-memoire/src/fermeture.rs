//! « Fermer » une appli (`Invoke-FermetureMemoire`, `Get-MessageFermeture`) : poliment, puis de force seulement si
//! elle ne répond plus ou s'est rangée près de l'horloge. Chaque geste revérifie sur la poignée gardée qu'il s'agit
//! bien du même programme, puis relit la règle « fermable » (la même que l'Aide).

use crate::constat::Cible;
use crate::pc::Pc;
use aide_regles::fermeture::{self, Contexte as ContexteRegle};
use nevermind_noyau::Permis;

/// Un processus pas fermé, et pourquoi
#[derive(Clone, Debug, PartialEq)]
pub struct Refuse {
    pub pid: i64,
    pub raison: String,
}

/// Ce qu'a donné un geste
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resultat {
    pub faits: Vec<Cible>,
    pub deja: Vec<Cible>,
    pub refuses: Vec<Refuse>,
}

/// Le geste sur chaque processus de l'appli. `mode` : « poli » (WM_CLOSE à ses fenêtres) ou « force ».
///
/// Lot J (suite, garde-fous) : avec le permis du verrou des gestes, ça compile :
/// ```no_run
/// fn fermer(
///     pc: &dyn nevermind_memoire::pc::Pc,
///     p: &nevermind_noyau::Permis,
///     regle: &nevermind_memoire::pc::ContexteRegle,
/// ) {
///     let _ = nevermind_memoire::fermeture::fermer(pc, p, &[], regle, "force");
/// }
/// ```
/// Sans lui, ça ne compile pas :
/// ```compile_fail
/// fn fermer(pc: &dyn nevermind_memoire::pc::Pc, regle: &nevermind_memoire::pc::ContexteRegle) {
///     let _ = nevermind_memoire::fermeture::fermer(pc, &[], regle, "force");
/// }
/// ```
pub fn fermer(pc: &dyn Pc, p: &Permis, cibles: &[Cible], ctx: &ContexteRegle, mode: &str) -> Resultat {
    let mut r = Resultat::default();
    for c in cibles {
        let v = pc.verifier(c.pid, c.debut, &c.chemin);
        if v == 0 {
            r.deja.push(c.clone());
            continue;
        }
        if v < 0 {
            r.refuses.push(Refuse {
                pid: c.pid,
                raison: "n'est plus le même programme (numéro repris, ou plus vérifiable)".into(),
            });
            continue;
        }
        if let Some(refus) = fermeture::refus(ctx, &pc.fiche(c.pid, &c.chemin), false) {
            r.refuses.push(Refuse { pid: c.pid, raison: refus.into() });
            continue;
        }
        let n = if mode == "force" {
            pc.forcer(p, c.pid, c.debut, &c.chemin)
        } else {
            pc.fermer_poliment(p, c.pid, c.debut, &c.chemin)
        };
        match n {
            -2 => r.deja.push(c.clone()),
            -1 => {
                r.refuses.push(Refuse { pid: c.pid, raison: "n'est plus le même programme (numéro repris)".into() })
            }
            -3 => r.refuses.push(Refuse { pid: c.pid, raison: "Windows refuse de le fermer".into() }),
            _ => r.faits.push(c.clone()),
        }
    }
    r
}

/// Ce que Nevermind dit quand l'appli n'est pas fermée au bout de 5 s ; `forcer` : « Forcer la fermeture » est proposé
pub fn message(nom: &str, etat: &str) -> (bool, String) {
    match etat {
        "bloquee" => (
            true,
            format!(
                "{nom} ne répond plus.\n\nForcer la fermeture ? Ce qui n'est pas enregistré dans {nom} sera perdu."
            ),
        ),
        "question" => (
            false,
            format!(
                "{nom} te pose une question : regarde sa fenêtre (par exemple pour enregistrer ton travail) et réponds-lui. Il se fermera ensuite."
            ),
        ),
        _ => (
            true,
            format!(
                "{nom} s'est rangé près de l'horloge au lieu de se fermer : il tourne encore en fond.\n\nFerme-le depuis son icône près de l'horloge, ou force la fermeture. Ce qui n'est pas enregistré dans {nom} sera perdu."
            ),
        ),
    }
}
