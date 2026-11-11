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
//!
//! Ce que suit le relevé (lot F : d'un fichier à l'autre du même paquet, jamais dans un autre paquet) : `self.nom(` et
//! `Self::nom(` (dans le fichier, sinon dans le paquet), `module::nom(` (le fichier de ce module), `nom(` (dans le
//! fichier). Lot J : aussi un appel sur une autre valeur (`x.nom(` : toutes les fonctions de ce nom du paquet, faute de
//! connaître le type), une fonction d'un type (`Type::nom(` : celles d'un `impl Type` du paquet), le code livré dans
//! un dossier `src/tests/`, et le traitement des signaux qu'un tic envoie (`signal` de chaque bloc, seulement les
//! branches de ces signaux).

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
/// (lot J : `aide_qui_change(` est `aide(` avec le permis du verrou des gestes ; l'action est la première chaîne)
const APPELS: &[&str] = &[".aide(", "::aide(", "aide.demander(", ".aide_qui_change("];

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

/// Lot J : les signaux déjà reçus puis rejoués plus tard par un tic (fichier, fonction qui rejoue, traitement) : le
/// traitement est suivi comme celui d'un signal (seulement les branches d'un signal envoyé par un tic), pas en entier
const REJEUX_DE_SIGNAUX: &[(&str, &str, &str, &str)] = &[(
    "nevermind-pc/src/lib.rs",
    "relever_travaux",
    "signal_pc",
    "les demandes des autres pages reçues pendant un geste de Ton PC, faites à sa fin (des clics, ou des signaux déjà suivis par le relevé)",
)];

/// Lot J : les fonctions qu'un appel sur une autre valeur (`pc.aide_lecture(`) atteint par son nom, mais jamais depuis
/// un tic : (fichier, fonction, raison). Le test vérifie que chacune existe encore et demande bien quelque chose.
const JAMAIS_DEPUIS_UN_TIC: &[(&str, &str, &str)] = &[(
    "nevermind-pc/src/pc.rs",
    "aide_lecture",
    "la lecture par défaut du trait Pc : seulement le désinstalleur (déjà administrateur, aucune demande de Windows) et les faux PC des tests ; celui de Nevermind (PcWindows, systeme.rs) a la sienne, par ctx.aide_lecture",
)];

/// Les sources d'un dossier de crates. `profondeur` : 0 pour le dossier des crates, 1 pour une crate.
fn sources(d: &Path, r: &mut Vec<PathBuf>, profondeur: usize) {
    let Ok(l) = std::fs::read_dir(d) else { return };
    for e in l.flatten() {
        let p = e.path();
        let nom = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            // (les tests d'une crate ont leurs fausses Aides ; target : ce qui est construit. Lot J : seulement le
            //  dossier `tests` à la racine d'une crate, plus un dossier `src/tests/` de code livré, comme Ton PC)
            let hors = nom == "target"
                || nom.starts_with('.')
                || (profondeur == 1 && matches!(nom.as_str(), "tests" | "examples" | "benches"));
            if !hors {
                sources(&p, r, profondeur + 1);
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
    sources(racine, &mut fichiers, 0);
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

/// Une fonction : son nom, son corps (positions dans le code), ses paramètres (sans `self`) et le type de son `impl`
struct Fonction {
    nom: String,
    corps: (usize, usize),
    /// (nom, type) de chaque paramètre, sans `self`
    params: Vec<(String, String)>,
    /// Le type de l'`impl` qui la contient (`impl Session` : « Session » ; `impl Bloc for B` : « B »)
    type_impl: Option<String>,
}

/// L'accolade fermante du bloc qui commence à `debut` (fin du code si elle manque)
fn fin_du_bloc(code: &str, debut: usize) -> usize {
    let mut profondeur = 0;
    for (k, c) in code[debut..].char_indices() {
        match c {
            '{' => profondeur += 1,
            '}' => {
                profondeur -= 1;
                if profondeur == 0 {
                    return debut + k;
                }
            }
            _ => {}
        }
    }
    code.len()
}

/// La parenthèse (ou le chevron) fermante de celle qui est à `debut`
fn fermante(t: &str, debut: usize, ouvre: char, ferme: char) -> usize {
    let mut profondeur = 0;
    let mut prec = ' ';
    for (k, c) in t[debut..].char_indices() {
        if c == ouvre {
            profondeur += 1;
        } else if c == ferme && !(ferme == '>' && prec == '-') {
            profondeur -= 1;
            if profondeur == 0 {
                return debut + k;
            }
        }
        prec = c;
    }
    t.len()
}

/// Les morceaux de `t` séparés par les virgules du premier niveau (hors parenthèses, crochets, accolades ; et hors
/// chevrons pour des types)
fn au_premier_niveau(t: &str, chevrons: bool) -> Vec<String> {
    let mut r = Vec::new();
    let mut profondeur = 0i32;
    let mut cour = String::new();
    let mut prec = ' ';
    for c in t.chars() {
        match c {
            '(' | '[' | '{' => profondeur += 1,
            ')' | ']' | '}' => profondeur -= 1,
            '<' if chevrons => profondeur += 1,
            '>' if chevrons && prec != '-' => profondeur -= 1,
            ',' if profondeur == 0 => {
                r.push(std::mem::take(&mut cour));
                prec = c;
                continue;
            }
            _ => {}
        }
        cour.push(c);
        prec = c;
    }
    if !cour.trim().is_empty() {
        r.push(cour);
    }
    r
}

/// Les blocs `impl` d'un fichier : (accolade ouvrante, accolade fermante, type)
fn blocs_impl(code: &str) -> Vec<(usize, usize, String)> {
    let mut r = Vec::new();
    for p in positions(code, "impl") {
        // (le mot `impl` au début d'un élément : pas `-> impl Fn`, ni un nom qui le contient)
        let avant = code[..p].trim_end();
        if mot(code[..p].chars().last().as_ref())
            || !code[p + 4..].starts_with([' ', '<', '\n', '\r'])
            || !(avant.is_empty() || avant.ends_with(['}', ';', '{', ']']) || avant.ends_with("unsafe"))
        {
            continue;
        }
        let Some(o) = code[p..].find(['{', ';']).map(|o| p + o) else { continue };
        if code.as_bytes()[o] == b';' {
            continue;
        }
        let mut entete = code[p + 4..o].trim_start();
        if entete.starts_with('<') {
            entete = &entete[(fermante(entete, 0, '<', '>') + 1).min(entete.len())..];
        }
        if let Some(i) = entete.find(" for ") {
            entete = &entete[i + 5..];
        }
        let chemin: String =
            entete.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':').collect();
        let ty = chemin.rsplit("::").next().unwrap_or("").to_string();
        if !ty.is_empty() {
            r.push((o, fin_du_bloc(code, o), ty));
        }
    }
    r
}

/// Les fonctions d'un fichier (le corps : de l'accolade ouvrante à la fermante)
fn fonctions(code: &str) -> Vec<Fonction> {
    let impls = blocs_impl(code);
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
        let fin = fin_du_bloc(code, debut);
        // (les paramètres : après le nom et ses génériques, sans `self`)
        let entete = &reste[..o];
        let apres_nom = entete.find(nom.as_str()).map(|i| i + nom.len()).unwrap_or(0);
        let mut a = apres_nom + entete[apres_nom..].len() - entete[apres_nom..].trim_start().len();
        if entete[a..].starts_with('<') {
            a = (fermante(entete, a, '<', '>') + 1).min(entete.len());
        }
        let params = match entete[a..].find('(').map(|x| a + x) {
            Some(ouvre) => {
                let ferme = fermante(entete, ouvre, '(', ')').max(ouvre + 1);
                au_premier_niveau(&entete[ouvre + 1..ferme], true)
                    .into_iter()
                    .filter_map(|x| {
                        let (n, t) = x.split_once(':')?;
                        let n = n.trim().trim_start_matches("mut ").trim().to_string();
                        (n != "self").then(|| (n, t.trim().to_string()))
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        let type_impl =
            impls.iter().filter(|(a, b, _)| p > *a && p < *b).min_by_key(|(a, b, _)| b - a).map(|x| x.2.clone());
        r.push(Fonction { nom, corps: (debut, fin), params, type_impl });
    }
    r
}

/// Un appel relevé dans un corps, que le relevé sait suivre : sur le bloc lui-même (`self.nom(`, `Self::nom(`), par
/// un module du même paquet (`module::nom(`, `crate::module::nom(`), par un type (`Session::demarrer(`, lot J), par
/// son nom, ou sur une autre valeur (`s.session.repondre_ecran(`, lot J)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Appel {
    Soi(String),
    Module(String, String),
    Type(String, String),
    Nom(String),
    Autre(String),
}

/// Les appels d'un corps que le relevé peut suivre (même coupés par un retour à la ligne), avec leurs arguments : le
/// chemin d'un tic vers ses aides
fn appeles(corps: &str) -> Vec<(Appel, Vec<String>)> {
    let mut r = Vec::new();
    let c: Vec<char> = corps.chars().collect();
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    // (le mot qui finit juste avant `k`, espaces sautés : son début et lui)
    let mot_avant = |mut k: usize| -> (usize, String) {
        while k > 0 && c[k - 1].is_whitespace() {
            k -= 1;
        }
        let fin = k;
        while k > 0 && ident(c[k - 1]) {
            k -= 1;
        }
        (k, c[k..fin].iter().collect())
    };
    let mut i = 0;
    while i < c.len() {
        if !(c[i].is_alphabetic() || c[i] == '_') {
            i += 1;
            continue;
        }
        let debut = i;
        while i < c.len() && ident(c[i]) {
            i += 1;
        }
        if c.get(i) != Some(&'(') {
            continue;
        }
        let nom: String = c[debut..i].iter().collect();
        // (les arguments, jusqu'à la parenthèse fermante)
        let mut profondeur = 0;
        let mut k = i;
        while k < c.len() {
            match c[k] {
                '(' => profondeur += 1,
                ')' => {
                    profondeur -= 1;
                    if profondeur == 0 {
                        break;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        let texte: String = c[i + 1..k.min(c.len()).max(i + 1)].iter().collect();
        let args = au_premier_niveau(&texte, false);
        // (ce qui précède, sans les espaces : « self. », « x. », « Self:: », « Type:: » ou « module:: »)
        let mut k = debut;
        while k > 0 && c[k - 1].is_whitespace() {
            k -= 1;
        }
        let appel = if k > 0 && c[k - 1] == '.' {
            let (d, m) = mot_avant(k - 1);
            if m == "self" && !(d > 0 && (ident(c[d - 1]) || c[d - 1] == '.')) {
                Some(Appel::Soi(nom))
            } else {
                Some(Appel::Autre(nom))
            }
        } else if k > 1 && c[k - 1] == ':' && c[k - 2] == ':' {
            let (_, m) = mot_avant(k - 2);
            if m == "Self" {
                Some(Appel::Soi(nom))
            } else if m.starts_with(|x: char| x.is_uppercase()) {
                Some(Appel::Type(m, nom))
            } else if !m.is_empty() {
                Some(Appel::Module(m, nom))
            } else {
                None
            }
        } else if mot_avant(k).1 != "fn" {
            Some(Appel::Nom(nom))
        } else {
            None
        };
        if let Some(a) = appel {
            r.push((a, args));
        }
    }
    r
}

/// Ce que le tic d'un bloc peut atteindre (lui-même et les fonctions de son paquet qu'il appelle, de proche en
/// proche, d'un fichier à l'autre) et qui peut proposer la demande de Windows ou écrire le registre
fn depuis_les_tics(racine: &Path) -> Vec<String> {
    atteint_depuis_les_tics(racine).0
}

/// Le paquet d'un fichier (son premier dossier) et son module (le nom du fichier, ou de son dossier pour `mod.rs`)
fn paquet_et_module(rel: &str) -> (&str, &str) {
    let paquet = rel.split('/').next().unwrap_or("");
    let mut parties = rel.trim_end_matches(".rs").rsplit('/');
    let dernier = parties.next().unwrap_or("");
    let module = if dernier == "mod" { parties.next().unwrap_or("") } else { dernier };
    (paquet, module)
}

/// Comment une fonction est atteinte (lot J) : depuis un tic (tout son corps compte), ou par le traitement d'un
/// signal envoyé par un tic. `Signal(k)` : son k-ième paramètre (sans `self`) est le nom du signal ; dans un
/// `match` sur ce paramètre, seules comptent les branches d'un signal envoyé par un tic, et les branches « autre »
/// (`_`, un nom lié).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Par {
    Tic,
    Signal(usize),
}

/// Les noms de signaux connus par leur constante (`PARTIE_COMMENCEE` : « partie-commencee ») ; et les fonctions qui
/// fabriquent un signal : celles du noyau, par le nom qu'elles envoient (`signaux::partie_commencee(` :
/// « partie-commencee »), et les enveloppes qui envoient le nom reçu (`signal(effets, "x", ...)`, `sig("x", ...)`),
/// par un nom vide
type Noms = HashMap<String, String>;

fn noms_de_signaux(l: &[Lu], toutes: &[Vec<Fonction>]) -> (Noms, Noms) {
    let mut constantes = HashMap::new();
    // (les constantes du noyau en dernier : un même nom ailleurs ne les cache pas)
    let mut ordre: Vec<&Lu> = l.iter().collect();
    ordre.sort_by_key(|lu| lu.rel.ends_with("nevermind-noyau/src/signaux.rs"));
    for lu in ordre {
        for p in positions(&lu.code, "const ") {
            let reste = &lu.code[p + 6..];
            let nom: String = reste.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            let Some(fin) = reste.find(';') else { continue };
            let decl = &reste[..fin];
            if !(decl.contains("&str") || decl.contains("&'static str")) {
                continue;
            }
            let Some(g) = decl.find('"') else { continue };
            let debut = p + 6 + g + 1;
            let Some(n) = lu.brut[debut..].find('"') else { continue };
            if !nom.is_empty() {
                constantes.insert(nom, lu.brut[debut..debut + n].to_string());
            }
        }
    }
    let mut fabriques = HashMap::new();
    for (lu, fs) in l.iter().zip(toutes) {
        // (les enveloppes : `Signal { nom: n.into()` avec un nom reçu, dans une fonction ou une fermeture nommée)
        for p in positions(&lu.code, "Signal") {
            let Some((_, valeur)) = champ_nom(lu, &lu.code[p..], p) else { continue };
            if valeur.is_some() {
                continue;
            }
            let avant = &lu.code[..p];
            let fermeture = avant.rfind("let ").filter(|&d| p - d < 120 && avant[d..].contains('|'));
            let nom: String = match fermeture {
                Some(d) => avant[d + 4..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect(),
                None => fs
                    .iter()
                    .filter(|f| p > f.corps.0 && p < f.corps.1)
                    .min_by_key(|f| f.corps.1 - f.corps.0)
                    .map(|f| f.nom.clone())
                    .unwrap_or_default(),
            };
            if !nom.is_empty() {
                fabriques.insert(nom, String::new());
            }
        }
        if !lu.rel.ends_with("nevermind-noyau/src/signaux.rs") {
            continue;
        }
        for f in fs {
            let corps = &lu.code[f.corps.0..f.corps.1];
            for (a, args) in appeles(corps) {
                if a == Appel::Nom("signal".into())
                    && let Some(v) = args.first().and_then(|x| constantes.get(x.trim()))
                {
                    fabriques.insert(f.nom.clone(), v.clone());
                }
            }
        }
    }
    (constantes, fabriques)
}

/// Le champ `nom` d'un `Signal { nom: ..., }` qui commence `texte` (à `decale` dans le fichier) : sa fin, et sa valeur
/// quand elle est écrite (une chaîne ou une constante connue) ; `Some(None)` : un nom reçu d'ailleurs
fn champ_nom(lu: &Lu, texte: &str, decale: usize) -> Option<(usize, Option<String>)> {
    let apres = texte.strip_prefix("Signal")?;
    let accolade = apres.len() - apres.trim_start().len();
    if !apres.trim_start().starts_with('{') {
        return None;
    }
    let ouvre = 6 + accolade;
    let ferme = fin_du_bloc(texte, ouvre);
    let dedans = &texte[ouvre + 1..ferme];
    let n = dedans.find("nom:")?;
    let valeur = dedans[n + 4..].trim_start();
    let v = ouvre + 1 + n + 4 + (dedans[n + 4..].len() - valeur.len());
    if let Some(chaine) = valeur.strip_prefix('"') {
        let fin = chaine.find('"')? + 1;
        return Some((ferme, Some(lu.brut[decale + v + 1..decale + v + fin].to_string())));
    }
    let chemin: String = valeur.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':').collect();
    let dernier = chemin.rsplit("::").next().unwrap_or("");
    let connu = dernier.starts_with(|c: char| c.is_uppercase());
    Some((ferme, connu.then(|| dernier.to_string())))
}

/// Les valeurs des chaînes écrites dans `code` (un morceau qui commence à `decale` dans le fichier) ; leur texte vient
/// du brut
fn chaines(lu: &Lu, code: &str, decale: usize) -> Vec<String> {
    let mut r = Vec::new();
    let mut k = 0;
    while let Some(o) = code[k..].find('"').map(|o| k + o) {
        let Some(f) = code[o + 1..].find('"').map(|f| o + 1 + f) else { break };
        r.push(lu.brut[decale + o + 1..decale + f].to_string());
        k = f + 1;
    }
    r
}

/// Les noms des signaux qu'un corps (qui commence à `decale` dans le fichier) envoie : `Signal { nom: "x" }` (ou une
/// constante), un appel à une fonction du noyau qui fabrique un signal, et les chaînes ou constantes données à une
/// enveloppe (`signal(effets, "x", ...)`)
fn signaux_ecrits(lu: &Lu, corps: &str, decale: usize, constantes: &Noms, fabriques: &Noms) -> Vec<String> {
    let mut r = Vec::new();
    let constante = |x: &str| constantes.get(x).cloned();
    for p in positions(corps, "Signal") {
        if let Some((_, Some(v))) = champ_nom(lu, &corps[p..], decale + p) {
            r.extend(if v.starts_with(|c: char| c.is_uppercase()) { constante(&v) } else { Some(v) });
        }
    }
    for (nom, valeur) in fabriques {
        for p in positions(corps, &format!("{nom}(")) {
            if mot(corps[..p].chars().last().as_ref()) {
                continue;
            }
            if !valeur.is_empty() {
                r.push(valeur.clone());
                continue;
            }
            let ouvre = p + nom.len();
            let ferme = fermante(corps, ouvre, '(', ')');
            let args = &corps[ouvre..ferme];
            // (le premier nom écrit : une chaîne, ou une constante écrite avant elle)
            let guillemet = args.find('"').unwrap_or(args.len());
            let premier = args[..guillemet]
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .find_map(constante)
                .or_else(|| chaines(lu, args, decale + ouvre).into_iter().next());
            r.extend(premier);
        }
    }
    r
}

/// Dans le corps d'un traitement de signal (qui commence à `decale` dans le fichier) : les branches d'un `match` sur
/// `parametre` qui ne traitent aucun signal envoyé par un tic (positions dans `corps`, à écarter)
fn branches_ecartees(
    lu: &Lu,
    corps: &str,
    decale: usize,
    parametre: &str,
    emis: &HashSet<String>,
    constantes: &Noms,
) -> Vec<(usize, usize)> {
    let mut r = Vec::new();
    for p in positions(corps, "match ") {
        if mot(corps[..p].chars().last().as_ref()) {
            continue;
        }
        let expr = corps[p + 6..].trim_start();
        let e = corps.len() - expr.len();
        let sujet: String = expr.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if sujet != parametre || !corps[e + sujet.len()..].trim_start().starts_with('{') {
            continue;
        }
        let ouvre = e + sujet.len() + corps[e + sujet.len()..].find('{').unwrap_or(0);
        let ferme = fin_du_bloc(corps, ouvre);
        let mut k = ouvre + 1;
        loop {
            while k < ferme && (corps.as_bytes()[k].is_ascii_whitespace() || corps.as_bytes()[k] == b',') {
                k += 1;
            }
            if k >= ferme {
                break;
            }
            // (le motif, jusqu'à « => » au premier niveau)
            let mut profondeur = 0;
            let mut fleche = None;
            for (j, ch) in corps[k..ferme].char_indices() {
                match ch {
                    '(' | '[' | '{' => profondeur += 1,
                    ')' | ']' | '}' => profondeur -= 1,
                    '=' if profondeur == 0 && corps[k + j..].starts_with("=>") => {
                        fleche = Some(k + j);
                        break;
                    }
                    _ => {}
                }
            }
            let Some(fleche) = fleche else { break };
            // (le corps de la branche : un bloc, ou une expression jusqu'à la virgule)
            let mut b = fleche + 2;
            while b < ferme && corps.as_bytes()[b].is_ascii_whitespace() {
                b += 1;
            }
            let fin = if corps[b..].starts_with('{') {
                fin_du_bloc(corps, b) + 1
            } else {
                let mut profondeur = 0;
                let mut f = ferme;
                for (j, ch) in corps[b..ferme].char_indices() {
                    match ch {
                        '(' | '[' | '{' => profondeur += 1,
                        ')' | ']' | '}' => profondeur -= 1,
                        ',' if profondeur == 0 => {
                            f = b + j;
                            break;
                        }
                        _ => {}
                    }
                }
                f
            };
            // (le motif sans sa garde : ses chaînes et ses constantes ; rien de tel : `_` ou un nom lié, gardé)
            let motif = &corps[k..fleche];
            let sans_garde = motif.find(" if ").map(|g| &motif[..g]).unwrap_or(motif);
            let mut noms = chaines(lu, sans_garde, decale + k);
            for x in sans_garde.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
                if let Some(v) = constantes.get(x) {
                    noms.push(v.clone());
                }
            }
            if !noms.is_empty() && !noms.iter().any(|n| emis.contains(n)) {
                r.push((b, fin));
            }
            k = fin;
        }
    }
    r
}

/// Le corps d'une fonction tel qu'il compte pour le relevé : tout, ou sans les branches écartées d'un traitement de
/// signal (remplacées par des espaces : les positions restent)
fn corps_compte(lu: &Lu, f: &Fonction, par: Par, emis: &HashSet<String>, constantes: &Noms) -> String {
    let corps = &lu.code[f.corps.0..f.corps.1];
    let Par::Signal(k) = par else { return corps.to_string() };
    let Some((p, _)) = f.params.get(k) else { return corps.to_string() };
    let ecartees = branches_ecartees(lu, corps, f.corps.0, p, emis, constantes);
    corps
        .char_indices()
        .map(|(i, c)| {
            if c != '\n' && ecartees.iter().any(|(a, b)| i >= *a && i < *b) {
                " ".repeat(c.len_utf8())
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// Le relevé, et les fonctions atteintes (« fichier::nom »). Un appel est suivi dans le même paquet seulement (lot F :
/// avant, dans le même fichier seulement) : `self.nom(` d'abord dans le fichier, sinon dans les autres fichiers du
/// paquet ; `module::nom(` dans le fichier de ce module ; `nom(` dans le fichier. Lot J : aussi `x.nom(` (toutes
/// les fonctions de ce nom du paquet), `Type::nom(` (les fonctions de ce nom dans un `impl Type` du paquet), et le
/// traitement (`signal`) des signaux qu'un tic envoie, dans tous les paquets.
fn atteint_depuis_les_tics(racine: &Path) -> (Vec<String>, Vec<String>) {
    let l = lus(racine);
    let toutes: Vec<Vec<Fonction>> = l.iter().map(|lu| fonctions(&lu.code)).collect();
    let (constantes, fabriques) = noms_de_signaux(&l, &toutes);
    // (les signaux envoyés par ce qu'un tic atteint, jusqu'à ce qu'il n'en vienne plus de nouveaux)
    let mut emis: HashSet<String> = HashSet::new();
    loop {
        let (refus, atteintes, vus) = parcourir(&l, &toutes, &emis, &constantes, &fabriques);
        if vus.is_subset(&emis) {
            return (refus, atteintes);
        }
        emis.extend(vus);
    }
}

/// Un parcours depuis les tics (et les traitements des signaux `emis`) : le relevé, les fonctions atteintes et les
/// signaux qu'elles envoient
fn parcourir(
    l: &[Lu],
    toutes: &[Vec<Fonction>],
    emis: &HashSet<String>,
    constantes: &Noms,
    fabriques: &Noms,
) -> (Vec<String>, Vec<String>, HashSet<String>) {
    let pm: Vec<(&str, &str)> = l.iter().map(|lu| paquet_et_module(&lu.rel)).collect();
    // (les fonctions d'un fichier, par nom)
    let nommees = |k: usize, n: &str| -> Vec<(usize, usize)> {
        toutes[k].iter().enumerate().filter(|(_, f)| f.nom == n).map(|(j, _)| (k, j)).collect()
    };
    let du_paquet = |k: usize| -> Vec<usize> { (0..l.len()).filter(|&x| pm[x].0 == pm[k].0).collect() };
    let mut refus = Vec::new();
    let mut atteintes = Vec::new();
    let mut envoyes: HashSet<String> = HashSet::new();
    let mut vus: HashSet<(usize, usize, Par)> = HashSet::new();
    let mut a_voir: Vec<((usize, usize), Par, String)> = Vec::new();
    for k in 0..l.len() {
        a_voir.extend(nommees(k, "tic").into_iter().map(|x| (x, Par::Tic, "tic".to_string())));
        // (lot J : le traitement des signaux envoyés par un tic, dans chaque bloc)
        if !emis.is_empty() {
            for (k, j) in nommees(k, "signal") {
                if let Some(i) = toutes[k][j].params.iter().position(|(_, t)| t.replace(' ', "") == "&str") {
                    a_voir.push(((k, j), Par::Signal(i), "signal envoyé par un tic".to_string()));
                }
            }
        }
    }
    while let Some(((kf, j), par, chemin)) = a_voir.pop() {
        if !vus.insert((kf, j, par)) {
            continue;
        }
        let (lu, f) = (&l[kf], &toutes[kf][j]);
        atteintes.push(format!("{}::{}", lu.rel, f.nom));
        let corps = corps_compte(lu, f, par, emis, constantes);
        envoyes.extend(signaux_ecrits(lu, &corps, f.corps.0, constantes, fabriques));
        for m in APPELS.iter().chain(REGISTRE) {
            for i in positions(&corps, m) {
                let ligne = lu.code[..f.corps.0 + i].matches('\n').count() + 1;
                refus.push(format!(
                    "{}:{ligne} : « {} » atteint depuis un tic ({chemin})",
                    lu.rel,
                    m.trim_end_matches('('),
                ));
            }
        }
        // (le nom du signal passé à une autre fonction : elle le traite à son tour, branche par branche)
        let parametre = match par {
            Par::Signal(k) => f.params.get(k).map(|p| p.0.clone()),
            Par::Tic => None,
        };
        for (a, args) in appeles(&corps) {
            let nom = match &a {
                Appel::Soi(n) | Appel::Module(_, n) | Appel::Type(_, n) | Appel::Nom(n) | Appel::Autre(n) => n,
            };
            if SUITES_DE_CLIC.contains(&nom.as_str()) {
                continue;
            }
            let cibles: Vec<(usize, usize)> = match &a {
                // (une demande à l'Aide : déjà relevée là où elle est écrite)
                Appel::Soi(n) | Appel::Autre(n)
                    if APPELS.iter().any(|m| m.trim_start_matches('.') == format!("{n}(")) =>
                {
                    Vec::new()
                }
                Appel::Soi(n) => {
                    let ici = nommees(kf, n);
                    if ici.is_empty() { du_paquet(kf).into_iter().flat_map(|x| nommees(x, n)).collect() } else { ici }
                }
                Appel::Module(m, n) => {
                    du_paquet(kf).into_iter().filter(|&x| pm[x].1 == m).flat_map(|x| nommees(x, n)).collect()
                }
                Appel::Type(t, n) => du_paquet(kf)
                    .into_iter()
                    .flat_map(|x| nommees(x, n))
                    .filter(|&(x, g)| toutes[x][g].type_impl.as_deref() == Some(t.as_str()))
                    .collect(),
                Appel::Nom(n) => nommees(kf, n),
                Appel::Autre(n) => du_paquet(kf)
                    .into_iter()
                    .flat_map(|x| nommees(x, n))
                    .filter(|&(x, _)| !JAMAIS_DEPUIS_UN_TIC.iter().any(|(fi, fo, _)| *fi == l[x].rel && fo == n))
                    .collect(),
            };
            let suite = match &parametre {
                Some(p) => args.iter().position(|x| x.trim() == p).map(Par::Signal).unwrap_or(Par::Tic),
                None => Par::Tic,
            };
            for (x, g) in cibles {
                let ou = if x == kf { String::new() } else { format!(" ({})", l[x].rel) };
                let cible = &toutes[x][g];
                // (un signal déjà reçu, rejoué : suivi comme un traitement de signal)
                let rejeu = REJEUX_DE_SIGNAUX
                    .iter()
                    .any(|(fi, de, vers, _)| *fi == lu.rel && *de == f.nom && *vers == cible.nom);
                let suite = match cible.params.iter().position(|(_, t)| t.replace(' ', "") == "&str") {
                    Some(i) if rejeu => Par::Signal(i),
                    _ => suite,
                };
                a_voir.push(((x, g), suite, format!("{chemin} > {}{ou}", cible.nom)));
            }
        }
    }
    refus.sort();
    refus.dedup();
    atteintes.sort();
    atteintes.dedup();
    (refus, atteintes, envoyes)
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
        // (d'un fichier à l'autre du même paquet, lot F : la fin de la lecture de Ton PC, le début d'une partie, la
        //  fin d'une analyse)
        "nevermind-pc/src/systeme.rs::completer_par_aide",
        "nevermind-perf/src/jeu.rs::fermer_pour_partie",
        "nevermind-protection/src/regles.rs::menaces_analyse",
        // (lot J : un appel sur une autre valeur, une fonction d'un type, le traitement d'un signal envoyé par un tic,
        //  et le code livré sous src/tests/)
        "nevermind-pc/src/session.rs::repondre_ecran",
        "nevermind-trafic/src/lag/session.rs::demarrer",
        "nevermind-trafic/src/lib.rs::signal",
        "nevermind-pc/src/lib.rs::signal_pc",
        "nevermind-pc/src/tests/banc.rs::lancer",
    ] {
        assert!(atteintes.iter().any(|a| a == f), "le relevé n'atteint pas {f} depuis un tic");
    }
    assert!(atteintes.len() >= 1000, "seulement {} fonctions atteintes depuis les tics", atteintes.len());
    // (lot J : chaque exception sert encore)
    let l = lus(&r);
    for (fichier, fonction, _) in JAMAIS_DEPUIS_UN_TIC {
        let lu = l
            .iter()
            .find(|x| x.rel == *fichier)
            .unwrap_or_else(|| panic!("exception inutile : {fichier} n'existe plus"));
        let demande = fonctions(&lu.code).iter().any(|f| {
            f.nom == *fonction && APPELS.iter().any(|m| !positions(&lu.code[f.corps.0..f.corps.1], m).is_empty())
        });
        assert!(demande, "exception inutile : {fichier} ({fonction}) n'existe plus ou ne demande plus rien");
    }
    for (fichier, de, vers, _) in REJEUX_DE_SIGNAUX {
        assert!(
            atteintes.iter().any(|a| *a == format!("{fichier}::{de}"))
                && atteintes.iter().any(|a| *a == format!("{fichier}::{vers}")),
            "exception inutile : {fichier} ({de} > {vers})"
        );
    }
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

/// Lot F : le relevé suit un tic d'un fichier à l'autre du MÊME paquet (une méthode du bloc écrite dans un autre
/// fichier, `module::nom(`), et une fonction appelée par son nom dans le même fichier ; jamais dans un autre paquet
/// (un nom pareil ailleurs n'est pas suivi)
#[test]
fn le_releve_suit_les_tics_dans_le_paquet_seulement() {
    let d = nevermind_noyau::dossier_essai("releve-aide-paquet");
    let _ = std::fs::remove_dir_all(&d);
    for x in ["bloc/src/outils", "autre/src/outils"] {
        std::fs::create_dir_all(d.join(x)).unwrap();
    }
    let ecrire = |f: &str, t: &str| std::fs::write(d.join(f), t).unwrap();
    ecrire(
        "bloc/src/lib.rs",
        "mod outils;\nmod suite;\nimpl Bloc for B {\n    fn tic(&mut self, ctx: &C) {\n        self.passer(ctx);\n        let _ = outils::lire(ctx);\n        let _ = autre::outils::ailleurs(ctx);\n        let _ = crate::outils::mesurer(ctx);\n        sonder(ctx);\n    }\n}\nfn sonder(ctx: &C) {\n    ctx.ecrire_registre(\"HKLM\", \"Y\", &json!(1), \"DWord\");\n}\n",
    );
    ecrire(
        "bloc/src/suite.rs",
        "impl B {\n    fn passer(&mut self, ctx: &C) {\n        ctx.changer_registre(\"HKLM\", \"X\", &json!(1), \"DWord\");\n    }\n}\n",
    );
    ecrire(
        "bloc/src/outils/mod.rs",
        "pub fn lire(ctx: &C) -> V {\n    ctx.aide(\"lecture.pc\", &json!({}), 1)\n}\npub fn mesurer(ctx: &C) -> V {\n    ctx.aide_de_fond(\"flux.lire\", &json!({}), 1)\n}\n",
    );
    ecrire(
        "autre/src/outils.rs",
        "pub fn ailleurs(ctx: &C) {\n    ctx.aide(\"processus.fermer\", &json!({}), 1);\n}\npub fn passer(ctx: &C) {\n    ctx.aide(\"reg.ecrire\", &json!({}), 1);\n}\npub fn sonder(ctx: &C) {\n    ctx.aide(\"reg.effacer\", &json!({}), 1);\n}\n",
    );
    let (refus, atteintes) = atteint_depuis_les_tics(&d);
    assert_eq!(
        refus,
        vec![
            "bloc/src/lib.rs:13 : « ecrire_registre » atteint depuis un tic (tic > sonder)".to_string(),
            "bloc/src/outils/mod.rs:2 : « .aide » atteint depuis un tic (tic > lire (bloc/src/outils/mod.rs))"
                .to_string(),
            "bloc/src/suite.rs:3 : « changer_registre » atteint depuis un tic (tic > passer (bloc/src/suite.rs))"
                .to_string(),
        ]
    );
    assert!(atteintes.iter().any(|a| a == "bloc/src/outils/mod.rs::mesurer"), "{atteintes:?}");
    assert!(!atteintes.iter().any(|a| a.starts_with("autre/")), "un autre paquet a été suivi : {atteintes:?}");
    let _ = std::fs::remove_dir_all(&d);
}

// -----------------------------------------------------------------------------------------------------------------
// Lot J : les angles morts du relevé (audit du 10/11, M5)
// -----------------------------------------------------------------------------------------------------------------

/// Un dossier de cas d'essai (`tests/cas/<nom>` : de faux blocs, jamais construits)
fn cas(nom: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("cas").join(nom)
}

/// Les lignes d'un dossier de cas marquées « // attendu » (« fichier:ligne »)
fn attendus(racine: &Path) -> Vec<String> {
    fn tous(d: &Path, r: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                tous(&p, r);
            } else if p.extension().is_some_and(|x| x == "rs") {
                r.push(p);
            }
        }
    }
    let mut fichiers = Vec::new();
    tous(racine, &mut fichiers);
    let mut r = Vec::new();
    for f in fichiers {
        let rel = f.strip_prefix(racine).unwrap().to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
        for (n, l) in std::fs::read_to_string(&f).unwrap().lines().enumerate() {
            if l.contains("// attendu") {
                r.push(format!("{rel}:{}", n + 1));
            }
        }
    }
    r.sort();
    r
}

/// Lot J, étape 1 : les quatre chemins vers une demande de Windows hors d'un clic que le relevé ne voyait pas
/// (`tests/cas/g5-angles-morts`). Contre-épreuve : avant le lot J, ce test échouait sur les quatre (sortie gardée dans le
/// rapport du lot J) ; maintenant le relevé les voit, et ne voit rien d'autre (pas le traitement d'un signal envoyé
/// seulement par un clic).
#[test]
fn angles_morts_du_releve_g5() {
    let d = cas("g5-angles-morts");
    let (refus, _) = atteint_depuis_les_tics(&d);
    let mauvais = refus_des_appels(&d);
    let vu = |ou: &str| refus.iter().any(|r| r.starts_with(&format!("{ou} ")));
    let mut manques = Vec::new();
    for (quoi, ou) in [
        ("1. appel sur une autre valeur (self.session.repondre) non suivi", "bloc/src/lag/session.rs:6"),
        ("2. fonction d'un type (Session::demarrer) non suivie", "bloc/src/lag/session.rs:10"),
        ("3. src/tests/ de code livré jamais lu depuis un tic", "bloc/src/tests/mod.rs:4"),
        ("4. signal envoyé par un tic : son traitement (signal) non suivi", "bloc/src/lib.rs:22"),
    ] {
        if !vu(ou) {
            manques.push(format!("{quoi} ({ou})"));
        }
    }
    if !mauvais.iter().any(|r| r.starts_with("bloc/src/tests/mod.rs:4 ")) {
        manques.push(
            "3. src/tests/ de code livré jamais lu par le relevé des appels (lecture.pc avec la demande de Windows)"
                .into(),
        );
    }
    assert_eq!(
        attendus(&d),
        vec![
            "bloc/src/lag/session.rs:10",
            "bloc/src/lag/session.rs:6",
            "bloc/src/lib.rs:22",
            "bloc/src/tests/mod.rs:4"
        ],
        "le cas d'essai a changé : mettre la liste à jour"
    );
    assert!(manques.is_empty(), "angles morts du garde-fou G5 :\n{}\n(refus vus : {refus:#?})", manques.join("\n"));
    // (rien d'autre : chaque refus est une ligne marquée « attendu », pas le traitement de « demarrage-desactiver »)
    let mut lignes: Vec<String> = refus.iter().map(|r| r.split(" : ").next().unwrap_or("").to_string()).collect();
    lignes.sort();
    lignes.dedup();
    assert_eq!(lignes, attendus(&d), "{refus:#?}");
}
