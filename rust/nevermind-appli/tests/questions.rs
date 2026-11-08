//! Audit M10 (point 5) : deux questions à l'écran. `kit.question` remplaçait la boîte de la première question mais
//! laissait son écouteur clavier (posé sur `window`, en capture). Les deux écouteurs restaient actifs : un Entrée
//! répondait à l'ancienne question, puis au premier bouton de la nouvelle, quel que soit le bouton choisi (par exemple
//! « Forcer la fermeture » de la page RAM, posée par le tic pendant qu'une autre question est ouverte). La version
//! PowerShell (fichiers\ui\kit.js) avait le même code.
//!
//! Désormais une seule question est à l'écran à la fois : la suivante attend son tour, et l'écouteur de la question
//! est retiré quand elle se ferme, avant que la suivante ne s'affiche.

use std::path::Path;

fn lire(nom: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui").join(nom);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Le corps de `kit.question(q)` (jusqu'à la méthode suivante du kit, `pause()`)
fn corps_question(js: &str) -> &str {
    let debut = js.find("    question(q) {").expect("kit.js : kit.question(q) a disparu");
    let fin = js[debut..].find("\n    pause()").expect("kit.js : la fin de kit.question");
    &js[debut..debut + fin]
}

#[test]
fn une_seule_question_a_la_fois_et_son_ecouteur_part_avec_elle() {
    let js = lire("kit.js");
    let q = corps_question(&js);

    // (un seul écouteur clavier de question dans tout le kit, posé dans kit.question)
    assert_eq!(
        js.matches("addEventListener('keydown', clavier, true)").count(),
        1,
        "kit.js : l'écouteur clavier de la question est posé à plusieurs endroits"
    );
    let pose = q.find("addEventListener('keydown', clavier, true)").expect("kit.question ne pose plus son écouteur");

    // Une question déjà à l'écran : la nouvelle attend, sans poser d'écouteur ni toucher à la boîte
    let garde = q
        .find("if (questionVue) { questionsEnAttente.push(q); return; }")
        .expect("kit.question : une deuxième question remplace la première (son écouteur reste actif)");
    assert!(garde < pose, "kit.question : l'écouteur est posé avant de regarder si une question est déjà à l'écran");
    let boite = q.find("fond.innerHTML = `").expect("kit.question : la boîte de la question");
    assert!(garde < boite, "kit.question : la boîte de la première question est remplacée avant d'attendre son tour");
    assert!(q[garde..pose].contains("questionVue = true;"), "kit.question : la question montrée n'est pas notée");

    // La réponse : l'écouteur est retiré, puis la question suivante (s'il y en a une) prend la place
    let rep = q.find("const repondre = (v) => {").expect("kit.question : repondre a disparu");
    let rep = &q[rep..rep + q[rep..].find("};").expect("fin de repondre")];
    let retire =
        rep.find("removeEventListener('keydown', clavier, true);").expect("repondre ne retire plus l'écouteur");
    let libre = rep.find("questionVue = false;").expect("repondre ne libère plus la place");
    let suivante = rep
        .find("const s = questionsEnAttente.shift(); if (s) kit.question(s);")
        .expect("repondre : la question suivante n'est plus montrée après la réponse");
    assert!(
        retire < suivante && libre < suivante,
        "repondre : la question suivante s'affiche avant que l'écouteur parte"
    );
    let envoi =
        rep.find("envoyer({ type: 'reponse', id: q.id, valeur: v });").expect("repondre n'envoie plus la réponse");
    assert!(envoi < suivante, "repondre : la réponse part après la question suivante");

    // Les questions en attente sont gardées au niveau du kit (pas recréées à chaque question)
    assert_eq!(js.matches("const questionsEnAttente = [];").count(), 1, "kit.js : la file des questions");
    assert_eq!(js.matches("let questionVue = false;").count(), 1, "kit.js : la question montrée");
    assert!(!q.contains("const questionsEnAttente"), "kit.js : la file des questions est recréée à chaque question");
}
