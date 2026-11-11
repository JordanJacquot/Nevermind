//! Garde-fou du lot A (second audit, ajustement D) : un seul geste qui change le PC à la fois. Ce test relève, dans les
//! sources de tout le workspace, chaque endroit qui change le PC (écritures du registre, demandes à l'Aide qui
//! modifient, services, DNS, pare-feu, powercfg, écrans, souris, nettoyage, Defender, tâches planifiées...) ou qui
//! écrit sauvegarde.json et l'état d'origine des services et de l'alimentation, et vérifie que la fonction qui le fait
//! passe par le verrou des gestes : elle le prend (`prendre_geste`, `geste_prioritaire`, `attendre_geste`,
//! `lancer_geste`) ou vérifie que son fil le tient (`exiger_geste`, qui refuse sinon).
//!
//! Lot J (audit du 10/11, M5) : un relais vers l'Aide est vu quel que soit le nom de sa variable ; `exiger_geste()` ne
//! protège que si le refus arrête la fonction (`?`, un retour, ou l'écriture dans le bloc de `is_ok()`) et vient avant
//! l'écriture ; un appel hors d'une fonction est relevé (jamais protégé) ; `fichier_sauvegarde(` est relevé dans une
//! fonction qui écrit. Le passage obligé par le compilateur (le `Permis` du noyau, lot J) vient en plus : ce relevé
//! garde ce qu'il ne couvre pas encore (les écritures propres à chaque bloc, par exemple).
//!
//! Ne sont pas relevés : l'Aide elle-même et ses règles (elles font ce que Nevermind demande ; le verrou est côté
//! Nevermind), le pont PowerShell, les tests et les exemples. Toute autre exception est écrite ci-dessous, avec sa
//! raison, et le test vérifie qu'elle sert encore.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Les crates qui ne sont pas relevées, et pourquoi
const CRATES_HORS_RELEVE: &[(&str, &str)] = &[
    ("nevermind-aide", "l'Aide fait ce que Nevermind lui demande : le verrou est pris par Nevermind avant de demander"),
    ("aide-regles", "les règles de l'Aide : elles vérifient une demande, elles ne changent rien"),
    ("nevermind-pont", "la DLL de la version PowerShell : aucun geste de Nevermind en Rust"),
    ("nevermind-relais", "le relais de lancement : il ne change aucun réglage"),
];

/// Ce qui prouve qu'une fonction passe par le verrou
const MARQUES: &[&str] = &["prendre_geste(", "geste_prioritaire(", "attendre_geste(", "exiger_geste(", "lancer_geste("];

/// Les actions de l'Aide qui changent le PC (la même liste que le noyau, vérifiée par `la_liste_est_celle_du_noyau`)
const ACTIONS: &[&str] = nevermind_noyau::aide::ACTIONS_QUI_CHANGENT;

/// Ce qui change le PC dans le code (hors texte et commentaires)
const APPELS: &[&str] = &[
    // le registre
    ".set_u32(",
    ".set_u64(",
    ".set_string(",
    ".set_expand_string(",
    ".set_multi_string(",
    ".set_bytes(",
    ".remove_value(",
    ".remove_tree(",
    "ecrire_utilisateur(",
    "effacer_utilisateur(",
    "reg_utilisateur_effacer_cle(",
    "reg_utilisateur_ecrire(",
    "reg_utilisateur_effacer(",
    "origine_utilisateur(",
    "origine_utilisateur_oublier(",
    "origines_utilisateur_effacer(",
    "origines_machine_effacer(",
    "ecrire_registre(",
    "effacer_registre(",
    "changer_registre(",
    "vider_registre(",
    "noter_origine(",
    "set_reg(",
    "clear_reg(",
    "ecrire_reg(",
    "effacer_reg(",
    // l'alimentation, les écrans, la souris, les commandes
    ".powercfg(",
    "pc_args(",
    "windows::powercfg(",
    "curseur_ecrire(",
    "ecran_frequence(",
    "souris_relire(",
    "windows::souris(",
    "ChangeDisplaySettingsExW(",
    "SPI_SETMOUSE",
    "pc.commande(",
    // le nettoyage, Defender, les tâches planifiées, le profil du réseau
    "nettoyer_dossier(",
    "nettoyer_chemins(",
    "supprimer_menaces(",
    "desactiver_tache(",
    "passer_public(",
    // sauvegarde.json et l'état d'origine des services et de l'alimentation
    "sauvegarde_ecrire(",
    "sauvegarde_effacer(",
    "sauvegarde_fusionner(",
    "enregistrer_sauvegarde",
    "ecrire_sauvegarde_fusion(",
    "effacer_sauvegarde(",
    "ecrire_reglage(\"SvcOriginal\"",
    "ecrire_reglage(\"PcfgOriginal\"",
];

/// Lot J : les relais vers l'Aide (l'action n'est pas écrite : celui qui relaie doit tenir le verrou ou le vérifier),
/// quel que soit le nom de la variable qui porte l'action. `.demander(` : seulement sur l'Aide (`aide.demander(`).
const RELAIS_AIDE: &[&str] =
    &[".aide(", "::aide(", ".aide_sans_demande(", ".aide_de_fond(", ".aide_texte(", ".aide_lecture(", ".demander("];

/// Lot J : le chemin de sauvegarde.json (relevé dans une fonction qui écrit un fichier : sauvegarde.json passe par
/// l'écrivain unique, sous le verrou)
const CHEMIN_SAUVEGARDE: &str = "fichier_sauvegarde(";

/// Ce qui écrit, efface ou lance (pour un programme écrit dans un texte, ou le chemin de sauvegarde.json)
const ECRITURES: &[&str] = &[
    "Command::new",
    "fs::write",
    "fs::rename",
    "remove_file",
    "ecrire_sur(",
    "ecrire_texte_sur(",
    "fs::copy",
    "File::create",
    "OpenOptions",
];

/// Lot J : le nom donné à ce qui est relevé hors d'une fonction
const HORS_FONCTION: &str = "(hors d'une fonction)";

/// Dans un texte du code : un programme qui change le PC
const DANS_TEXTE: &[&str] = &["powercfg.exe", "schtasks", "MpCmdRun", "Set-NetConnectionProfile", "sauvegarde.json"];

/// Une commande qui ne fait que lire (powercfg /q, /list, /getactivescheme, /batteryreport, schtasks /Query)
const LECTURES: &[&str] = &["\"/q\"", "\"/list\"", "\"/getactivescheme\"", "\"/batteryreport\"", "\"/Query\""];

/// Les fonctions relevées qui n'ont pas à passer par le verrou : (fichier, fonction, raison)
const EXCEPTIONS: &[(&str, &str, &str)] = &[
    (
        "nevermind-appli/src/essai.rs",
        "aide_lente",
        "la fausse Aide de la construction d'essai : elle ne fait rien sur le PC",
    ),
    (
        "nevermind-desinstalleur/src/os/mod.rs",
        "schtasks",
        "la désinstallation retire la tâche de l'Aide : Nevermind est fermé, et le désinstalleur tient le verrou pendant tout son travail (main.rs)",
    ),
    (
        "nevermind-desinstalleur/src/os/mod.rs",
        "effacer_cle_aide",
        "la désinstallation efface la clé de l'Aide : Nevermind est fermé, et le désinstalleur tient le verrou pendant tout son travail (main.rs)",
    ),
    // (lot J : les relais vers l'Aide dont l'action n'est pas écrite, et qui n'ont pas à tenir le verrou)
    (
        "nevermind-noyau/src/aide.rs",
        "arreter_mesure",
        "l'arrêt d'une mesure (fps.arreter, flux.arreter : `Mesure::arret`), qui ne change rien ; et aide_texte refuse d'elle-même une action qui change le PC sans le verrou",
    ),
    (
        "nevermind-noyau/src/aide.rs",
        "aide_lecture",
        "un point d'entrée du noyau : il mène à `invoquer`, qui refuse une action qui change le PC sans le verrou (exiger_geste)",
    ),
    (
        "nevermind-pc/src/systeme.rs",
        "aide_lecture",
        "la lecture de Ton PC passe à ctx.aide_lecture, qui mène à `invoquer` : une action qui change le PC y est refusée sans le verrou",
    ),
    (
        "nevermind-noyau/src/sauvegarde.rs",
        "importer_sauvegarde_de",
        "la relecture de sauvegarde.json met de côté un fichier abîmé (renommé, jamais écrit) ; sa copie de secours, et les valeurs d'origine gardées dans le registre, sont reprises",
    ),
    // (lot J, étape 3b : les voies sans permis du trait Pc ne servent plus qu'aux lectures ; un changement y est refusé,
    //  il passe par la méthode qui demande le permis, aide_qui_change ou powercfg_changer)
    (
        "nevermind-pc/src/systeme.rs",
        "aide",
        "Pc::aide de Nevermind : ctx.aide refuse une action qui change le PC (elle passe par aide_qui_change, avec le permis)",
    ),
    (
        "nevermind-pc/src/systeme.rs",
        "powercfg",
        "Pc::powercfg de Nevermind : seulement /getactivescheme, /list et /q ; un changement y est refusé (powercfg_changer, avec le permis)",
    ),
    (
        "nevermind-desinstalleur/src/os/mod.rs",
        "aide",
        "Pc::aide de la désinstallation : une action qui change le PC y est refusée (aide_qui_change, avec le permis)",
    ),
    (
        "nevermind-desinstalleur/src/os/mod.rs",
        "powercfg_system32",
        "la commande elle-même : appelée seulement par powercfg (lectures, un changement y est refusé) et powercfg_changer (avec le permis)",
    ),
];

/// Les primitives (le vrai appel à Windows) : elles ne vérifient pas le verrou elles-mêmes, mais leur nom est un motif
/// relevé ci-dessus, donc chaque fonction qui les appelle doit passer par le verrou : (fichier, fonction)
const PRIMITIVES: &[(&str, &str)] = &[
    ("nevermind-noyau/src/registre.rs", "ecrire_utilisateur"),
    ("nevermind-noyau/src/registre.rs", "effacer_utilisateur"),
    ("nevermind-pc/src/windows/mod.rs", "reg_utilisateur_ecrire"),
    ("nevermind-pc/src/windows/mod.rs", "reg_utilisateur_effacer"),
    ("nevermind-pc/src/windows/mod.rs", "reg_utilisateur_effacer_cle"),
    ("nevermind-pc/src/windows/mod.rs", "ecran_frequence"),
    ("nevermind-pc/src/windows/mod.rs", "souris"),
    ("nevermind-reseau/src/windows/mod.rs", "passer_public"),
    ("nevermind-reseau/src/windows/reseau.rs", "passer_public"),
];

// -----------------------------------------------------------------------------------------------------------------
// Le relevé
// -----------------------------------------------------------------------------------------------------------------

/// Un fichier lu : le code sans les commentaires (`code`), et le même sans les textes ni les caractères (`squelette`),
/// aux mêmes positions
struct Source {
    code: Vec<u8>,
    squelette: Vec<u8>,
}

fn ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn lire_source(t: &str) -> Source {
    let b = t.as_bytes();
    let mut code = b.to_vec();
    let mut sq = b.to_vec();
    let blanc = |v: &mut Vec<u8>, i: usize| {
        if v[i] != b'\n' {
            v[i] = b' ';
        }
    };
    let mut i = 0;
    while i < b.len() {
        // commentaires
        if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                blanc(&mut code, i);
                blanc(&mut sq, i);
                i += 1;
            }
            continue;
        }
        if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            let mut prof = 0;
            while i < b.len() {
                if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                    prof += 1;
                    for k in i..i + 2 {
                        blanc(&mut code, k);
                        blanc(&mut sq, k);
                    }
                    i += 2;
                    continue;
                }
                if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                    prof -= 1;
                    for k in i..i + 2 {
                        blanc(&mut code, k);
                        blanc(&mut sq, k);
                    }
                    i += 2;
                    if prof == 0 {
                        break;
                    }
                    continue;
                }
                blanc(&mut code, i);
                blanc(&mut sq, i);
                i += 1;
            }
            continue;
        }
        // textes bruts (r"...", r#"..."#, br"...")
        let avant_ident = i > 0 && ident(b[i - 1]);
        if !avant_ident && (b[i] == b'r' || (b[i] == b'b' && b.get(i + 1) == Some(&b'r'))) {
            let mut j = if b[i] == b'b' { i + 2 } else { i + 1 };
            let mut dieses = 0;
            while b.get(j) == Some(&b'#') {
                dieses += 1;
                j += 1;
            }
            if b.get(j) == Some(&b'"') {
                let fin: Vec<u8> = std::iter::once(b'"').chain(std::iter::repeat_n(b'#', dieses)).collect();
                let mut k = j + 1;
                while k < b.len() && !b[k..].starts_with(&fin) {
                    blanc(&mut sq, k);
                    k += 1;
                }
                i = k + fin.len();
                continue;
            }
        }
        // textes
        if b[i] == b'"' {
            let mut k = i + 1;
            while k < b.len() && b[k] != b'"' {
                if b[k] == b'\\' {
                    blanc(&mut sq, k);
                    k += 1;
                }
                if k < b.len() {
                    blanc(&mut sq, k);
                }
                k += 1;
            }
            i = k + 1;
            continue;
        }
        // caractères ('a', '\n', '{', 'é') ; pas les durées de vie ('a)
        if b[i] == b'\'' {
            let k = i + 1;
            if b.get(k) == Some(&b'\\') {
                let mut e = k + 1;
                while e < b.len() && b[e] != b'\'' {
                    e += 1;
                }
                for x in k..e {
                    blanc(&mut sq, x);
                }
                i = e + 1;
                continue;
            }
            if let Some(c) = t.get(k..).and_then(|s| s.chars().next()) {
                let l = c.len_utf8();
                if b.get(k + l) == Some(&b'\'') {
                    for x in k..k + l {
                        blanc(&mut sq, x);
                    }
                    i = k + l + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    Source { code, squelette: sq }
}

/// Une fonction : son nom et les positions de son corps
struct Fonction {
    nom: String,
    debut: usize,
    fin: usize,
}

fn fonctions(sq: &[u8]) -> Vec<Fonction> {
    let mut r = Vec::new();
    let mut i = 0;
    while i + 3 < sq.len() {
        let mot =
            sq[i] == b'f' && sq[i + 1] == b'n' && sq[i + 2].is_ascii_whitespace() && (i == 0 || !ident(sq[i - 1]));
        if !mot {
            i += 1;
            continue;
        }
        let mut j = i + 3;
        while j < sq.len() && sq[j].is_ascii_whitespace() {
            j += 1;
        }
        let n0 = j;
        while j < sq.len() && ident(sq[j]) {
            j += 1;
        }
        let nom = String::from_utf8_lossy(&sq[n0..j]).into_owned();
        // le corps : la première accolade au niveau zéro des parenthèses, ou « ; » (sans corps)
        let mut paren = 0i32;
        let mut corps = None;
        while j < sq.len() {
            match sq[j] {
                b'(' | b'[' => paren += 1,
                b')' | b']' => paren -= 1,
                b'{' if paren == 0 => {
                    corps = Some(j);
                    break;
                }
                b';' if paren == 0 => break,
                _ => {}
            }
            j += 1;
        }
        if let Some(d) = corps {
            let mut prof = 0i32;
            let mut k = d;
            while k < sq.len() {
                match sq[k] {
                    b'{' => prof += 1,
                    b'}' => {
                        prof -= 1;
                        if prof == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            r.push(Fonction { nom, debut: d, fin: k });
        }
        i += 3;
    }
    r
}

/// Les modules de test d'un fichier (`#[cfg(test)] mod x { ... }`) : leurs positions, et les fichiers de test
/// déclarés à part (`#[cfg(test)] #[path = "x.rs"] mod tests;`)
fn modules_de_test(code: &[u8], sq: &[u8], dossier: &Path) -> (Vec<(usize, usize)>, Vec<PathBuf>) {
    let texte = String::from_utf8_lossy(code);
    let mut zones = Vec::new();
    let mut fichiers = Vec::new();
    let mut depuis = 0;
    while let Some(p) = texte[depuis..].find("#[cfg(test)]") {
        let a = depuis + p;
        depuis = a + 12;
        let suite = &texte[depuis..];
        let chemin =
            suite.trim_start().strip_prefix("#[path = \"").and_then(|s| s.find('"').map(|n| s[..n].to_string()));
        let Some(m) = suite.find("mod ") else { continue };
        let apres = &suite[m + 4..];
        let nom: String = apres.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let reste = apres[nom.len()..].trim_start();
        if reste.starts_with(';') {
            fichiers.push(match chemin {
                Some(c) => dossier.join(c),
                None => dossier.join(format!("{nom}.rs")),
            });
            continue;
        }
        if reste.starts_with('{') {
            let d = depuis + m + 4 + nom.len() + (apres[nom.len()..].len() - reste.len());
            let mut prof = 0i32;
            let mut k = d;
            while k < sq.len() {
                match sq[k] {
                    b'{' => prof += 1,
                    b'}' => {
                        prof -= 1;
                        if prof == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            zones.push((a, k));
        }
    }
    (zones, fichiers)
}

fn sources(d: &Path, r: &mut Vec<PathBuf>, racine: bool) {
    let Ok(l) = std::fs::read_dir(d) else { return };
    for e in l.flatten() {
        let p = e.path();
        let nom = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            // (à la racine d'une crate : ses tests, exemples et ce qui est construit)
            let hors = nom == "target"
                || nom.starts_with('.')
                || (racine && matches!(nom.as_str(), "tests" | "examples" | "benches"));
            if !hors {
                sources(&p, r, false);
            }
        } else if nom.ends_with(".rs") {
            r.push(p);
        }
    }
}

/// Un endroit qui change le PC : (fichier, ligne, fonction, ce qui a été vu)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Endroit {
    fichier: String,
    ligne: usize,
    fonction: String,
    quoi: String,
    protege: bool,
}

/// Les arguments d'un appel commençant à `i` (jusqu'à la parenthèse fermante), dans le code
fn arguments(code: &[u8], sq: &[u8], i: usize) -> String {
    let Some(o) = sq[i..].iter().position(|c| *c == b'(').map(|o| o + i) else { return String::new() };
    let mut prof = 0i32;
    let mut k = o;
    while k < sq.len() {
        match sq[k] {
            b'(' => prof += 1,
            b')' => {
                prof -= 1;
                if prof == 0 {
                    break;
                }
            }
            _ => {}
        }
        k += 1;
    }
    String::from_utf8_lossy(&code[o..k.min(code.len())]).into_owned()
}

/// Le relevé d'un dossier de crates (le workspace, ou un faux workspace pour la contre-épreuve)
pub fn relever(racine: &Path) -> Vec<Endroit> {
    let mut r = Vec::new();
    let Ok(crates) = std::fs::read_dir(racine) else { return r };
    let mut liste: Vec<PathBuf> = crates.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    liste.sort();
    for c in liste {
        let nom = c.file_name().unwrap().to_string_lossy().into_owned();
        if CRATES_HORS_RELEVE.iter().any(|(n, _)| *n == nom) || nom == "target" || nom.starts_with('.') {
            continue;
        }
        let mut fichiers = Vec::new();
        sources(&c, &mut fichiers, true);
        // (les fichiers de test déclarés à part, `#[cfg(test)] #[path] mod`)
        let mut de_test: BTreeSet<PathBuf> = BTreeSet::new();
        for f in &fichiers {
            let t = std::fs::read_to_string(f).unwrap_or_default();
            let s = lire_source(&t);
            let (_, autres) = modules_de_test(&s.code, &s.squelette, f.parent().unwrap());
            de_test.extend(autres);
        }
        for f in fichiers {
            if de_test.contains(&f) {
                continue;
            }
            let t = std::fs::read_to_string(&f).unwrap_or_default();
            let s = lire_source(&t);
            let (zones, _) = modules_de_test(&s.code, &s.squelette, f.parent().unwrap());
            let fonctions = fonctions(&s.squelette);
            let rel = f.strip_prefix(racine).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            let code = &s.code;
            let sq = &s.squelette;
            // (ce qui a été vu, et s'il compte aussi hors d'une fonction : un appel, pas une liste de noms)
            let mut vus: Vec<(usize, String, bool)> = Vec::new();
            // Les appels (dans le code, hors texte) : pas leur définition (« fn nom(»)
            for a in APPELS {
                let motif = a.as_bytes();
                let mut d = 0;
                while let Some(p) = find(&code[d..], motif) {
                    let i = d + p;
                    d = i + 1;
                    // (le premier caractère doit être du code, et le motif ne doit pas être la fin d'un autre nom)
                    if sq[i] != code[i] || (motif[0] != b'.' && i > 0 && ident(code[i - 1])) {
                        continue;
                    }
                    let avant = String::from_utf8_lossy(&code[i.saturating_sub(4)..i]).into_owned();
                    if avant.ends_with("fn ") {
                        continue;
                    }
                    if matches!(*a, ".powercfg(" | "pc_args(" | "windows::powercfg(" | "pc.commande(") {
                        let args = arguments(code, sq, i);
                        if LECTURES.iter().any(|l| args.contains(l)) {
                            continue;
                        }
                    }
                    vus.push((i, a.trim_matches(['.', '(']).to_string(), a.ends_with('(')));
                }
            }
            // Lot J : les relais vers l'Aide, quel que soit le nom de la variable qui porte l'action (une action écrite
            // est relevée plus bas si elle change le PC)
            for a in RELAIS_AIDE {
                let motif = a.as_bytes();
                let mut d = 0;
                while let Some(p) = find(&code[d..], motif) {
                    let i = d + p;
                    d = i + 1;
                    if sq[i] != code[i] {
                        continue;
                    }
                    // (`.demander(` : seulement sur l'Aide, `aide.demander(`)
                    if *a == ".demander(" && !mot_avant(code, i).eq_ignore_ascii_case("aide") {
                        continue;
                    }
                    let args = arguments_coupes(code, sq, i);
                    let premier = args.first().map(|x| x.trim().trim_start_matches('&').trim().to_string());
                    let action = match (premier, args.get(1)) {
                        (Some(c), Some(x)) if *a == ".demander(" && c.ends_with("ctx") => Some(x.trim().to_string()),
                        (p, _) => p,
                    };
                    if action.as_deref().is_none_or(|x| x.starts_with('"') || x.is_empty()) {
                        continue;
                    }
                    vus.push((i, format!("relais vers l'Aide ({})", a.trim_matches(['.', ':', '('])), true));
                }
            }
            // Les actions de l'Aide qui changent le PC, écrites telles quelles
            for a in ACTIONS {
                let motif = format!("\"{a}\"");
                let mut d = 0;
                while let Some(p) = find(&code[d..], motif.as_bytes()) {
                    let i = d + p;
                    d = i + 1;
                    if sq[i + 1] == code[i + 1] {
                        continue;
                    }
                    vus.push((i, format!("Aide {a}"), false));
                }
            }
            // Les programmes qui changent le PC, dans un texte
            for a in DANS_TEXTE {
                let mut d = 0;
                while let Some(p) = find(&code[d..], a.as_bytes()) {
                    let i = d + p;
                    d = i + 1;
                    if sq[i] == code[i] {
                        continue;
                    }
                    vus.push((i, (*a).to_string(), false));
                }
            }
            // Lot J : sauvegarde.json pris par son chemin (`fichier_sauvegarde(`), pas par l'écrivain unique
            let motif = CHEMIN_SAUVEGARDE.as_bytes();
            let mut d = 0;
            while let Some(p) = find(&code[d..], motif) {
                let i = d + p;
                d = i + 1;
                let avant = String::from_utf8_lossy(&code[i.saturating_sub(4)..i]).into_owned();
                if sq[i] != code[i] || (i > 0 && ident(code[i - 1])) || avant.ends_with("fn ") {
                    continue;
                }
                vus.push((i, CHEMIN_SAUVEGARDE.trim_end_matches('(').to_string(), false));
            }
            for (i, quoi, appel) in vus {
                if zones.iter().any(|(a, b)| i >= *a && i <= *b) {
                    continue;
                }
                let ligne = code[..i].iter().filter(|c| **c == b'\n').count() + 1;
                // (la plus petite fonction qui le contient ; hors d'une fonction, lot J : un appel compte, une valeur
                //  calculée au premier usage par exemple, jamais protégé ; un nom dans une liste ne compte pas)
                let Some(fct) = fonctions.iter().filter(|x| i > x.debut && i < x.fin).min_by_key(|x| x.fin - x.debut)
                else {
                    if appel {
                        r.push(Endroit {
                            fichier: rel.clone(),
                            ligne,
                            fonction: HORS_FONCTION.into(),
                            quoi,
                            protege: false,
                        });
                    }
                    continue;
                };
                // (pour un programme dans un texte ou le chemin de sauvegarde.json : seulement s'il est lancé ou écrit,
                //  sauf une lecture)
                let corps = String::from_utf8_lossy(&code[fct.debut..fct.fin]).into_owned();
                if DANS_TEXTE.contains(&quoi.as_str()) || quoi == CHEMIN_SAUVEGARDE.trim_end_matches('(') {
                    let ecrit = ECRITURES.iter().any(|w| corps.contains(w));
                    if !ecrit || LECTURES.iter().any(|l| corps.contains(l)) {
                        continue;
                    }
                }
                let protege = protege_avant(&corps, i - fct.debut) || demande_le_permis(code, fct);
                r.push(Endroit { fichier: rel.clone(), ligne, fonction: fct.nom.clone(), quoi, protege });
            }
        }
    }
    r.sort();
    r.dedup();
    r
}

/// Les arguments d'un appel commençant à `i`, coupés aux virgules du premier niveau (le texte du code)
fn arguments_coupes(code: &[u8], sq: &[u8], i: usize) -> Vec<String> {
    let Some(o) = sq[i..].iter().position(|c| *c == b'(').map(|o| o + i) else { return Vec::new() };
    let mut r = Vec::new();
    let mut prof = 0i32;
    let mut debut = o + 1;
    let mut k = o;
    while k < sq.len() {
        match sq[k] {
            b'(' | b'[' | b'{' => prof += 1,
            b')' | b']' | b'}' => {
                prof -= 1;
                if prof == 0 {
                    break;
                }
            }
            b',' if prof == 1 => {
                r.push(String::from_utf8_lossy(&code[debut..k]).into_owned());
                debut = k + 1;
            }
            _ => {}
        }
        k += 1;
    }
    let dernier = String::from_utf8_lossy(&code[debut..k.min(code.len())]).into_owned();
    if !dernier.trim().is_empty() {
        r.push(dernier);
    }
    r
}

/// Le mot juste avant le point qui est à `i` (`aide` dans `self.aide.demander(`)
fn mot_avant(code: &[u8], i: usize) -> String {
    let mut k = i;
    while k > 0 && ident(code[k - 1]) {
        k -= 1;
    }
    String::from_utf8_lossy(&code[k..i]).into_owned()
}

/// La fin du bloc entre accolades qui commence à `debut`
fn fin_bloc(t: &str, debut: usize) -> usize {
    let mut prof = 0i32;
    for (k, c) in t[debut..].char_indices() {
        match c {
            '{' => prof += 1,
            '}' => {
                prof -= 1;
                if prof == 0 {
                    return debut + k;
                }
            }
            _ => {}
        }
    }
    t.len()
}

/// La fonction passe-t-elle par le verrou avant `pos` (position dans son corps) ? Elle le prend (`prendre_geste(`,
/// `geste_prioritaire(`, `attendre_geste(`, `lancer_geste(`), ou elle le vérifie ET s'arrête sur un refus (lot J) :
/// `exiger_geste()?`, `exiger_geste().ok()?`, `exiger_geste().map_err(..)?`, un refus suivi d'un bloc qui rend
/// (`if ... exiger_geste().is_err() { return }`, `if let Err(_) = ...exiger_geste() { return }`), ou l'endroit dans le
/// bloc d'un `if ... exiger_geste().is_ok() { }`. Un `exiger_geste()` dont le refus est oublié ne compte pas.
fn protege_avant(corps: &str, pos: usize) -> bool {
    if MARQUES.iter().filter(|m| **m != "exiger_geste(").any(|m| corps.contains(m)) {
        return true;
    }
    // (lot J : `let permis = ctx.exiger_geste();` puis l'endroit dans `if let Ok(p) = &permis { }`)
    let mut d = 0;
    while let Some(q) = corps[d..].find("if let Ok(") {
        let i = d + q;
        d = i + 1;
        let Some(o) = corps[i..].find('{').map(|o| i + o) else { break };
        let sujet = corps[i..o].rsplit('=').next().unwrap_or("").trim().trim_start_matches('&').trim();
        let pris = !sujet.is_empty()
            && sujet.chars().all(|c| c.is_alphanumeric() || c == '_')
            && corps[..i].rfind(&format!("let {sujet} = ")).is_some_and(|l| {
                corps[l..].split(';').next().is_some_and(|x| x.trim_end().ends_with("exiger_geste()"))
            });
        if pris && pos > o && pos < fin_bloc(corps, o) {
            return true;
        }
    }
    let motif = "exiger_geste()";
    let mut d = 0;
    while let Some(p) = corps[d..].find(motif) {
        let i = d + p;
        d = i + 1;
        if i > pos {
            break;
        }
        let j = i + motif.len();
        let apres = corps[j..].trim_start();
        // (lot J : `exiger_geste().and_then(|p| ...)` : la suite n'a lieu qu'avec le permis)
        if apres.starts_with('?') || apres.starts_with(".ok()?") || apres.starts_with(".and_then(") {
            return true;
        }
        if let Some(m) = apres.strip_prefix(".map_err(") {
            let fin = fin_parentheses(m);
            if m[fin..].trim_start().starts_with('?') {
                return true;
            }
        }
        // (le bloc qui suit, dans la même instruction)
        let Some(o) = corps[j..].find(['{', ';']).map(|o| j + o) else { continue };
        if corps.as_bytes()[o] == b';' {
            continue;
        }
        let entre = &corps[j..o];
        let fin = fin_bloc(corps, o);
        // (le début de l'instruction : `if let Ok(p) = ctx.exiger_geste() { ... }`)
        let debut = corps[..i].rfind([';', '{', '}']).map(|x| x + 1).unwrap_or(0);
        let si_permis = corps[debut..i].contains("let Ok(");
        if entre.trim_start().starts_with(".is_ok()") || (si_permis && entre.trim().is_empty()) {
            if pos > o && pos < fin {
                return true;
            }
            continue;
        }
        if corps[o..fin].contains("return") {
            return true;
        }
    }
    false
}

/// Lot J : la fonction demande le permis du verrou en paramètre (`p: &Permis`) : le compilateur refuse de l'appeler
/// sans lui, et on ne l'obtient que du verrou
fn demande_le_permis(code: &[u8], fct: &Fonction) -> bool {
    let avant = String::from_utf8_lossy(&code[..fct.debut]).into_owned();
    let Some(f) = avant.rfind(&format!("fn {}", fct.nom)) else { return false };
    avant[f..].split([',', '(', ')']).any(|x| {
        x.split_once(':').is_some_and(|(_, t)| {
            let t = t.trim();
            t == "&Permis" || t == "&crate::Permis" || t == "&nevermind_noyau::Permis"
        })
    })
}

/// La parenthèse fermante d'un texte qui commence juste après une parenthèse ouvrante (la position qui la suit)
fn fin_parentheses(t: &str) -> usize {
    let mut prof = 1i32;
    for (k, c) in t.char_indices() {
        match c {
            '(' => prof += 1,
            ')' => {
                prof -= 1;
                if prof == 0 {
                    return k + 1;
                }
            }
            _ => {}
        }
    }
    t.len()
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    h.windows(n.len()).position(|w| w == n)
}

fn exception(f: &str, fct: &str) -> bool {
    EXCEPTIONS.iter().any(|(x, y, _)| *x == f && (*y == fct || *y == "*"))
        || PRIMITIVES.iter().any(|(x, y)| *x == f && *y == fct)
}

fn racine() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

#[test]
fn chaque_geste_qui_change_le_pc_passe_par_le_verrou() {
    let l = relever(&racine());
    assert!(l.len() >= 60, "les endroits qui changent le PC n'ont pas été trouvés ({} seulement)", l.len());
    let refus: Vec<String> = l
        .iter()
        .filter(|e| !e.protege && !exception(&e.fichier, &e.fonction))
        .map(|e| format!("{}:{} ({}) : {} sans le verrou des gestes", e.fichier, e.ligne, e.fonction, e.quoi))
        .collect();
    assert!(
        refus.is_empty(),
        "{} endroit(s) changent le PC sans passer par le verrou :\n{}",
        refus.len(),
        refus.join("\n")
    );
    // Chaque exception sert encore (sinon elle cacherait un oubli plus tard)
    for (f, fct, _) in EXCEPTIONS {
        assert!(
            l.iter().any(|e| e.fichier == *f && (e.fonction == *fct || *fct == "*") && !e.protege),
            "exception inutile : {f} ({fct})"
        );
    }
    // Chaque primitive est relevée là où elle est appelée (son nom est un motif), et elle est bien relevée elle-même
    for (f, fct) in PRIMITIVES {
        assert!(
            APPELS.iter().any(|a| *a == format!("{fct}(") || *a == format!("windows::{fct}(")),
            "primitive {f} ({fct}) : son nom doit être un motif relevé, pour que ses appels passent par le verrou"
        );
        assert!(l.iter().any(|e| e.fichier == *f && e.fonction == *fct), "primitive inutile : {f} ({fct})");
    }
}

/// La liste des actions de l'Aide qui changent le PC est complète : chaque action de la liste stricte de l'Aide est
/// soit une lecture, soit dans la liste du noyau (sauf le point de restauration, ajustement B)
#[test]
fn la_liste_est_celle_du_noyau() {
    let t = std::fs::read_to_string(racine().join("aide-regles/src/demandes.rs")).unwrap();
    let mut actions: BTreeSet<String> = BTreeSet::new();
    for motif in ["a(\"", "(\""] {
        let mut d = 0;
        while let Some(p) = t[d..].find(motif) {
            let i = d + p + motif.len();
            d = i;
            let fin = t[i..].find('"').unwrap_or(0);
            let a = &t[i..i + fin];
            let nom_valable = a.split_once('.').is_some_and(|(x, y)| {
                x.chars().next().is_some_and(|c| c.is_ascii_lowercase()) && !y.is_empty() && !y.contains('.')
            });
            if nom_valable && a.chars().all(|c| c.is_ascii_lowercase() || c == '.' || c == '-') {
                actions.insert(a.to_string());
            }
        }
    }
    let lectures = [
        "aide.bonjour",
        "aide.journal",
        "lecture.pc",
        "lecture.securite",
        "lecture.taches",
        "lecture.chemins",
        "fps.lire",
        "fps.arreter",
        "fps.demarrer",
        "fps.veiller",
        "trafic.tcp",
        "flux.demarrer",
        "flux.veiller",
        "flux.lire",
        "flux.arreter",
        "nettoyage.lister",
        "restauration.creer",
    ];
    for a in &actions {
        assert!(
            ACTIONS.contains(&a.as_str()) || lectures.contains(&a.as_str()),
            "action de l'Aide « {a} » : à classer (elle change le PC ? alors dans ACTIONS_QUI_CHANGENT)"
        );
    }
    assert!(actions.len() >= 25, "{actions:?}");
}

/// Contre-épreuve : le relevé voit un faux appel non protégé (registre, Aide, sauvegarde), et le voit protégé
/// quand la fonction prend le verrou
#[test]
fn le_releve_voit_un_geste_sans_verrou() {
    let d = nevermind_noyau::dossier_essai("releve-gestes");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("bloc/src")).unwrap();
    std::fs::create_dir_all(d.join("bloc/tests")).unwrap();
    std::fs::write(
        d.join("bloc/src/lib.rs"),
        "// ctx.aide(\"reg.ecrire\") dans un commentaire : pas relevé\n\
         fn clic(ctx: &C) {\n    let _ = ctx.aide(\n        \"dns.regler\", &json!({}), 10);\n}\n\
         fn protege(ctx: &C) {\n    let g = ctx.prendre_geste(\"x\", \"y\");\n    lancer(move || ctx.changer_registre(\"HKCU:\\\\x\", \"n\", &v, \"DWord\"));\n}\n\
         fn texte() -> &'static str {\n    \"ctx.changer_registre( dans un texte\"\n}\n\
         fn sauver(p: &mut P) {\n    p.sauvegarde_ecrire(&j);\n}\n\
         fn lire(p: &P) {\n    p.powercfg(&[\"/list\".into()]);\n}\n\
         #[cfg(test)]\nmod tests {\n    fn t(c: &C) { c.ecrire_registre(\"a\", \"b\", &v, \"DWord\"); }\n}\n",
    )
    .unwrap();
    std::fs::write(d.join("bloc/tests/t.rs"), "fn t(c: &C) { c.ecrire_registre(\"a\", \"b\", &v, \"DWord\"); }")
        .unwrap();
    let l = relever(&d);
    let vus: Vec<(usize, &str, &str, bool)> =
        l.iter().map(|e| (e.ligne, e.fonction.as_str(), e.quoi.as_str(), e.protege)).collect();
    assert_eq!(
        vus,
        vec![
            (4, "clic", "Aide dns.regler", false),
            (8, "protege", "changer_registre", true),
            (14, "sauver", "sauvegarde_ecrire", false)
        ],
        "{l:#?}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

// -----------------------------------------------------------------------------------------------------------------
// Lot J : les angles morts du relevé (audit du 10/11, M5)
// -----------------------------------------------------------------------------------------------------------------

/// Un dossier de cas d'essai (`tests/cas/<nom>` : de faux blocs, jamais construits)
fn cas(nom: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("cas").join(nom)
}

/// Lot J, étape 1 : quatre écritures sans le verrou que le relevé laissait passer (`tests/cas/verrou-angles-morts`).
/// Contre-épreuve : avant le lot J, ce test échouait sur les quatre (sortie gardée dans le rapport du lot J) ;
/// maintenant le relevé les refuse, et voit protégées les lignes marquées « protégé » (le verrou vérifié et le refus
/// suivi d'un retour, ou l'écriture dans le bloc d'un `is_ok()`), sans relever les lectures.
#[test]
fn angles_morts_du_releve_des_gestes() {
    let d = cas("verrou-angles-morts");
    let l = relever(&d);
    let refuses: Vec<String> = l.iter().filter(|e| !e.protege).map(|e| format!("{}:{}", e.fichier, e.ligne)).collect();
    let mut manques = Vec::new();
    for (quoi, ou) in [
        ("1. relais vers l'Aide dont la variable ne s'appelle pas `action`", "bloc/src/lib.rs:7"),
        ("2. fonction qui continue après un refus de exiger_geste", "bloc/src/lib.rs:13"),
        ("3. écriture hors d'une fonction", "bloc/src/lib.rs:18"),
        ("4. sauvegarde.json écrit par fichier_sauvegarde(", "bloc/src/lib.rs:23"),
    ] {
        if !refuses.iter().any(|r| r == ou) {
            manques.push(format!("{quoi} ({ou}) non refusé"));
        }
    }
    assert!(manques.is_empty(), "angles morts du garde-fou du verrou :\n{}\n(relevé : {l:#?})", manques.join("\n"));
    // (rien d'autre : les refus sont les lignes « attendu », les lignes « protégé » sont vues protégées, et les
    //  lectures ne sont pas relevées)
    let marquees = |marque: &str| -> Vec<String> {
        let t = std::fs::read_to_string(d.join("bloc/src/lib.rs")).unwrap();
        t.lines()
            .enumerate()
            .filter(|(_, x)| x.contains(marque))
            .map(|(n, _)| format!("bloc/src/lib.rs:{}", n + 1))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    };
    let mut refuses = refuses;
    refuses.sort();
    refuses.dedup();
    assert_eq!(refuses, marquees("// attendu"), "{l:#?}");
    let mut proteges: Vec<String> =
        l.iter().filter(|e| e.protege).map(|e| format!("{}:{}", e.fichier, e.ligne)).collect();
    proteges.sort();
    proteges.dedup();
    assert_eq!(proteges, marquees("// protégé"), "{l:#?}");
}
