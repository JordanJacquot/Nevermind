//! Garde-fou de l'audit G5 : `aide(` est la seule voie qui peut proposer la demande de Windows. Elle est réservée aux
//! gestes que l'utilisateur vient de demander d'un clic (changer un réglage du PC, bloquer un programme, nettoyer...).
//! Les lectures, les mesures et les gestes de fond (minuterie, lancement d'un jeu, fin d'une lecture) passent par
//! `aide_sans_demande`, `aide_de_fond` ou `aide_lecture` : sinon, si la tâche de l'Aide manque, une demande de Windows
//! s'ouvrirait en pleine partie.
//!
//! Ce test relève dans les sources du workspace :
//! - chaque appel qui peut proposer la demande de Windows : `.aide(`, `Contexte::aide(` écrit autrement (`::aide(`),
//!   et les relais (`aide.demander(` de la page RAM) ; il refuse toute action qui n'est pas dans la liste des gestes
//!   sur clic ci-dessous. Ajouter une action à cette liste, c'est affirmer qu'elle ne part JAMAIS d'une minuterie ;
//! - (lot B du second audit) tout ce que le `tic` d'un bloc peut atteindre (les fonctions de son paquet qu'il appelle,
//!   de proche en proche) : là, aucun de ces appels n'est permis, même avec un nom de la liste, ni aucune écriture du
//!   registre (`changer_registre(`, `ecrire_registre(`, `effacer_registre(`, `vider_registre(` : un réglage du PC
//!   passe par `reg.ecrire` ou `reg.effacer` avec la demande de Windows).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Les actions de l'Aide permises avec `aide(` : chacune ne part que d'un clic (ou du « Oui » qui le suit)
const GESTES_SUR_CLIC: &[&str] = &[
    "reg.ecrire",                   // sauvegarde, corrections, réseau (bureau à distance) : boutons
    "reg.effacer",                  // retour en arrière : bouton
    "service.regler",               // couper un service, revenir en arrière : boutons
    "restauration.creer",           // point de restauration : bouton, ou avant « Tout corriger »
    "pare-feu.debloquer",           // revenir en arrière : bouton
    "pare-feu.debloquer-tout",      // « Tout restaurer » : bouton
    "pare-feu.bloquer",             // trafic, « Bloquer » puis « Oui »
    "pare-feu.activer",             // protection et réseau, « Réactiver le pare-feu »
    "origines.oublier",             // « Tout restaurer » : bouton
    "nettoyage.lister",             // « Analyser » du nettoyage : bouton
    "nettoyage.lancer",             // « Nettoyer » : bouton
    "antivirus.retirer-exclusions", // protection, « Retirer » puis « Oui »
    "wifi.manuel",                  // réseau, geste de l'audit : bouton
    "smb1.couper",                  // réseau, geste de l'audit : bouton
    "dns.regler",                   // réseau et retour en arrière : boutons
    "carte.relancer",               // réseau, « Relancer la carte » : bouton
    "processus.fermer",             // jeux, « Fermer X » du diagnostic d'une partie, puis « Oui »
];

/// Les appels qui peuvent proposer la demande de Windows : l'action est la première chaîne écrite dans l'appel
const APPELS: &[&str] = &[".aide(", "::aide(", "aide.demander("];

/// Les écritures du registre : un réglage du PC passe par l'Aide avec la demande de Windows (jamais depuis un tic)
const REGISTRE: &[&str] = &["changer_registre(", "ecrire_registre(", "effacer_registre(", "vider_registre("];

/// Les fonctions qui transmettent une action reçue (pas de nom écrit) : celui qui les appelle est relevé à sa place
const RELAIS: &[&str] = &[
    "nevermind-memoire/src/pc.rs", // AideDuContexte::demander (service.regler, sur clic)
    "nevermind-pc/src/systeme.rs", // Pc::aide de PcWindows (les appels pc.aide( sont relevés eux aussi)
    "nevermind-pc/src/pc.rs",      // Pc::aide_lecture par défaut (désinstalleur et faux PC ; PcWindows a la sienne)
    "nevermind-desinstalleur/src/os/mod.rs", // Pc::aide du désinstalleur (en administrateur, jamais d'Aide à réveiller)
];

/// Les fonctions qu'un tic peut atteindre sans que ce soit une minuterie : la suite d'un clic, relevée au tic
/// (`action` : le dossier choisi dans la fenêtre de « Analyser un dossier », lot B, étape 1)
const SUITES_DE_CLIC: &[&str] = &["action"];

fn sources(d: &Path, r: &mut Vec<PathBuf>) {
    let Ok(l) = std::fs::read_dir(d) else { return };
    for e in l.flatten() {
        let p = e.path();
        let nom = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            // (les tests ont leurs fausses Aides ; target : ce qui est construit)
            if nom != "target" && nom != "tests" && !nom.starts_with('.') {
                sources(&p, r);
            }
        } else if nom.ends_with(".rs") {
            r.push(p);
        }
    }
}

/// Le texte sans les commentaires ni le contenu des chaînes (remplacés par des espaces : les positions restent) ;
/// les guillemets des chaînes sont gardés, pour lire le nom d'une action
fn code_seul(t: &str) -> String {
    let b: Vec<char> = t.chars().collect();
    let mut r: Vec<char> = Vec::with_capacity(b.len());
    let mut i = 0;
    let blanc = |c: char| if c == '\n' { '\n' } else { ' ' };
    while i < b.len() {
        let c = b[i];
        if c == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                r.push(' ');
                i += 1;
            }
        } else if c == '/' && b.get(i + 1) == Some(&'*') {
            while i < b.len() && !(b[i] == '*' && b.get(i + 1) == Some(&'/')) {
                r.push(blanc(b[i]));
                i += 1;
            }
            if i < b.len() {
                r.push(' ');
                r.push(' ');
                i += 2;
            }
        } else if c == 'r'
            && (b.get(i + 1) == Some(&'"') || b.get(i + 1) == Some(&'#'))
            && !mot(b.get(i.wrapping_sub(1)))
        {
            // (chaîne brute r"..." ou r#"..."#)
            let mut j = i + 1;
            let mut dieses = 0;
            while b.get(j) == Some(&'#') {
                dieses += 1;
                j += 1;
            }
            if b.get(j) != Some(&'"') {
                r.push(c);
                i += 1;
                continue;
            }
            r.extend(std::iter::repeat_n(' ', j - i));
            r.push('"');
            j += 1;
            while j < b.len() {
                if b[j] == '"' && (0..dieses).all(|k| b.get(j + 1 + k) == Some(&'#')) {
                    break;
                }
                r.push(blanc(b[j]));
                j += 1;
            }
            r.push('"');
            r.extend(std::iter::repeat_n(' ', dieses));
            i = j + 1 + dieses;
        } else if c == '"' {
            r.push('"');
            i += 1;
            while i < b.len() && b[i] != '"' {
                if b[i] == '\\' {
                    r.push(' ');
                    i += 1;
                }
                if i < b.len() {
                    r.push(blanc(b[i]));
                    i += 1;
                }
            }
            r.push('"');
            i += 1;
        } else if c == '\'' && b.get(i + 2) == Some(&'\'') {
            r.extend(['\'', ' ', '\'']);
            i += 3;
        } else if c == '\'' && b.get(i + 1) == Some(&'\\') {
            // ('\n', '\'' ...)
            let mut j = i + 2;
            while j < b.len() && b[j] != '\'' {
                j += 1;
            }
            r.extend(std::iter::repeat_n(' ', j + 1 - i));
            i = j + 1;
        } else {
            r.push(c);
            i += 1;
        }
    }
    // (un caractère remplacé garde la place de l'original en octets : les positions du code sont celles du texte)
    assert_eq!(r.len(), b.len());
    let mut s = String::with_capacity(t.len());
    for (x, o) in r.into_iter().zip(b) {
        s.push(x);
        if x != o {
            s.extend(std::iter::repeat_n(' ', o.len_utf8() - x.len_utf8()));
        }
    }
    s
}

fn mot(c: Option<&char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || *c == '_')
}

/// L'action d'un appel : la première chaîne écrite entre la parenthèse ouvrante et la fin de l'appel
fn action_ecrite(brut: &str, code: &str, apres: usize) -> Option<String> {
    let mut profondeur = 1;
    let cb: Vec<(usize, char)> = code[apres..].char_indices().collect();
    for (k, c) in cb {
        match c {
            '(' | '[' | '{' => profondeur += 1,
            ')' | ']' | '}' => {
                profondeur -= 1;
                if profondeur == 0 {
                    return None;
                }
            }
            '"' => {
                let debut = apres + k + 1;
                let fin = brut[debut..].find('"')? + debut;
                return Some(brut[debut..fin].to_string());
            }
            _ => {}
        }
    }
    None
}

/// Chaque appel qui peut proposer la demande de Windows : (fichier, ligne, action écrite ou `None`)
fn appels(racine: &Path) -> Vec<(String, usize, Option<String>)> {
    let mut r = Vec::new();
    for f in lus(racine) {
        for m in APPELS {
            for i in positions(&f.code, m) {
                let ligne = f.code[..i].matches('\n').count() + 1;
                r.push((f.rel.clone(), ligne, action_ecrite(&f.brut, &f.code, i + m.len())));
            }
        }
    }
    r.sort();
    r
}

fn positions(code: &str, motif: &str) -> Vec<usize> {
    let mut r = Vec::new();
    let mut depuis = 0;
    while let Some(i) = code[depuis..].find(motif) {
        let p = depuis + i;
        depuis = p + motif.len();
        // (« .aide( » n'est pas « ma_fonction_aide( » ; « ::aide( » pas « ::aide_texte( » : le motif finit par « ( »)
        let avant = code[..p].chars().last();
        if motif.starts_with(|c: char| c.is_alphabetic()) && mot(avant.as_ref()) && avant != Some('.') {
            continue;
        }
        r.push(p);
    }
    r
}

/// Un fichier source lu : son chemin relatif, son texte, et son code sans commentaires ni chaînes
struct Lu {
    rel: String,
    brut: String,
    code: String,
}

fn lus(racine: &Path) -> Vec<Lu> {
    let mut fichiers = Vec::new();
    sources(racine, &mut fichiers);
    fichiers.sort();
    fichiers
        .into_iter()
        .map(|f| {
            let brut = std::fs::read_to_string(&f).unwrap_or_default();
            let rel = f.strip_prefix(racine).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            Lu { code: code_seul(&brut), rel, brut }
        })
        .collect()
}

/// Une fonction : son nom, son fichier, et son corps (positions dans le code)
struct Fonction {
    nom: String,
    /// (le fichier où elle est écrite, gardé pour la lecture)
    #[allow(dead_code)]
    fichier: usize,
    corps: (usize, usize),
}

/// Les fonctions d'un fichier (le corps : de l'accolade ouvrante à la fermante)
fn fonctions(code: &str, fichier: usize) -> Vec<Fonction> {
    let mut r = Vec::new();
    for p in positions(code, "fn ") {
        if mot(code[..p].chars().last().as_ref()) {
            continue;
        }
        let reste = &code[p + 3..];
        let nom: String = reste.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if nom.is_empty() {
            continue;
        }
        // (le corps commence à la première accolade ; un « ; » avant : une déclaration sans corps)
        let Some(o) = reste.find(['{', ';']) else { continue };
        if reste.as_bytes()[o] == b';' {
            continue;
        }
        let debut = p + 3 + o;
        let mut profondeur = 0;
        let mut fin = debut;
        for (k, c) in code[debut..].char_indices() {
            match c {
                '{' => profondeur += 1,
                '}' => {
                    profondeur -= 1;
                    if profondeur == 0 {
                        fin = debut + k;
                        break;
                    }
                }
                _ => {}
            }
        }
        r.push(Fonction { nom, fichier, corps: (debut, fin) });
    }
    r
}

/// Les méthodes appelées sur le bloc lui-même dans un corps (`self.nom(`, `Self::nom(`, même coupés par un retour à la
/// ligne) : le chemin d'un tic vers ses aides
fn appeles(corps: &str) -> HashSet<String> {
    let mut r = HashSet::new();
    let c: Vec<char> = corps.chars().collect();
    let mut i = 0;
    while i < c.len() {
        if c[i].is_alphabetic() || c[i] == '_' {
            let debut = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            if c.get(i) != Some(&'(') {
                continue;
            }
            // (ce qui précède, sans les espaces : « self. » ou « Self:: »)
            let mut k = debut;
            while k > 0 && c[k - 1].is_whitespace() {
                k -= 1;
            }
            let avant: String = c[..k].iter().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect();
            let devant_self = |s: &str, sep: &str| {
                avant.strip_suffix(sep).is_some_and(|a| {
                    a.trim_end().ends_with(s)
                        && !mot(a.trim_end().strip_suffix(s).and_then(|x| x.chars().last()).as_ref())
                })
            };
            if devant_self("self", ".") || devant_self("Self", "::") {
                r.insert(c[debut..i].iter().collect());
            }
        } else {
            i += 1;
        }
    }
    r
}

/// Ce que le tic d'un bloc peut atteindre (lui-même et les méthodes du bloc qu'il appelle, de proche en proche, dans
/// son fichier) et qui peut proposer la demande de Windows ou écrire le registre
fn depuis_les_tics(racine: &Path) -> Vec<String> {
    atteint_depuis_les_tics(racine).0
}

/// Le relevé, et les fonctions atteintes (« fichier::nom »)
fn atteint_depuis_les_tics(racine: &Path) -> (Vec<String>, Vec<String>) {
    let l = lus(racine);
    let mut refus = Vec::new();
    let mut atteintes = Vec::new();
    for (k, lu) in l.iter().enumerate() {
        let du_fichier = fonctions(&lu.code, k);
        let mut par_nom: HashMap<&str, Vec<&Fonction>> = HashMap::new();
        for f in &du_fichier {
            par_nom.entry(f.nom.as_str()).or_default().push(f);
        }
        let mut a_voir: Vec<(&Fonction, String)> =
            du_fichier.iter().filter(|f| f.nom == "tic").map(|f| (f, "tic".to_string())).collect();
        let mut vus: HashSet<usize> = HashSet::new();
        while let Some((f, chemin)) = a_voir.pop() {
            if !vus.insert(f.corps.0) {
                continue;
            }
            atteintes.push(format!("{}::{}", lu.rel, f.nom));
            let corps = &lu.code[f.corps.0..f.corps.1];
            for m in APPELS.iter().chain(REGISTRE) {
                for i in positions(corps, m) {
                    let ligne = lu.code[..f.corps.0 + i].matches('\n').count() + 1;
                    refus.push(format!(
                        "{}:{ligne} : « {} » atteint depuis un tic ({chemin})",
                        lu.rel,
                        m.trim_end_matches('('),
                    ));
                }
            }
            for n in appeles(corps) {
                if SUITES_DE_CLIC.contains(&n.as_str()) {
                    continue;
                }
                if let Some(v) = par_nom.get(n.as_str()) {
                    a_voir.extend(v.iter().map(|g| (*g, format!("{chemin} > {}", g.nom))));
                }
            }
        }
    }
    refus.sort();
    (refus, atteintes)
}

fn racine() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

/// Les appels qui peuvent proposer la demande de Windows ne portent que des gestes sur clic
fn refus_des_appels(racine: &Path) -> Vec<String> {
    let mut refus = Vec::new();
    for (f, n, a) in appels(racine) {
        match a {
            Some(a) if GESTES_SUR_CLIC.contains(&a.as_str()) => {}
            Some(a) => refus.push(format!(
                "{f}:{n} : « {a} » n'est pas un geste sur clic (aide_sans_demande, aide_de_fond ou aide_lecture)"
            )),
            None if RELAIS.contains(&f.as_str()) => {}
            None => refus.push(format!("{f}:{n} : action non écrite, à vérifier puis à noter dans RELAIS")),
        }
    }
    refus
}

#[test]
fn aide_avec_demande_seulement_pour_un_clic() {
    let r = racine();
    let l = appels(&r);
    assert!(l.len() >= 20, "les appels de l'Aide n'ont pas été trouvés ({} seulement)", l.len());
    let refus = refus_des_appels(&r);
    assert!(
        refus.is_empty(),
        "appels de l'Aide qui peuvent proposer la demande de Windows hors d'un clic :\n{}",
        refus.join("\n")
    );
}

/// Lot B : rien de ce qu'un tic peut atteindre ne peut proposer la demande de Windows ni écrire le registre
#[test]
fn rien_depuis_un_tic() {
    let r = racine();
    let (refus, atteintes) = atteint_depuis_les_tics(&r);
    // (le relevé suit bien les tics jusqu'à leurs aides : sinon il ne prouverait rien)
    for f in [
        "nevermind-perf/src/lib.rs::debut_partie",
        "nevermind-trafic/src/lib.rs::lignes_tcp",
        "nevermind-protection/src/lib.rs::finir_lecture",
        "nevermind-reseau/src/bloc.rs::fin_geste_audit",
    ] {
        assert!(atteintes.iter().any(|a| a == f), "le relevé n'atteint pas {f} depuis un tic");
    }
    assert!(atteintes.len() >= 50, "seulement {} fonctions atteintes depuis les tics", atteintes.len());
    assert!(
        refus.is_empty(),
        "depuis un tic, des appels qui peuvent proposer la demande de Windows :\n{}",
        refus.join("\n")
    );
}

/// Le relevé lui-même : il voit un appel écrit sur plusieurs lignes, `Contexte::aide(` écrit autrement, un relais, et
/// ce qu'un tic atteint (même avec un nom de la liste, ou une écriture du registre de la machine) ; pas les lectures,
/// ni ce qui est dans un commentaire ou une chaîne, ni une écriture faite sur clic
#[test]
fn le_releve_voit_les_lectures_les_relais_et_les_tics() {
    let d = nevermind_noyau::dossier_essai("releve-aide");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("bloc/src")).unwrap();
    std::fs::write(
        d.join("bloc/src/lib.rs"),
        r#"fn tic(ctx: &C) { let _ = ctx.aide(
    "lecture.pc", &json!({}), 10); let _ = ctx.aide_sans_demande("trafic.tcp", &v, 1); self.passer(ctx); }
fn passer(&mut self, ctx: &C) {
    // ctx.aide("dans.un.commentaire", &v, 1)
    let _ = format!("ctx.aide(\"dans.une.chaine\")");
    nevermind_noyau::Contexte::aide(ctx, "processus.fermer", &json!({}), 1);
    ctx.changer_registre(r"HKLM:\SOFTWARE\Essai", "X", &json!(1), "DWord");
}
fn action(&mut self, ctx: &C) { ctx.ecrire_registre(r"HKLM:\SOFTWARE\Essai", "Y", &json!(1), "DWord"); aide.demander(ctx, "service.regler", &v); }
"#,
    )
    .unwrap();
    assert_eq!(
        appels(&d),
        vec![
            ("bloc/src/lib.rs".to_string(), 1, Some("lecture.pc".to_string())),
            ("bloc/src/lib.rs".to_string(), 6, Some("processus.fermer".to_string())),
            ("bloc/src/lib.rs".to_string(), 9, Some("service.regler".to_string())),
        ]
    );
    assert_eq!(
        depuis_les_tics(&d),
        vec![
            "bloc/src/lib.rs:1 : « .aide » atteint depuis un tic (tic)".to_string(),
            "bloc/src/lib.rs:6 : « ::aide » atteint depuis un tic (tic > passer)".to_string(),
            "bloc/src/lib.rs:7 : « changer_registre » atteint depuis un tic (tic > passer)".to_string(),
        ]
    );
    let _ = std::fs::remove_dir_all(&d);
}
