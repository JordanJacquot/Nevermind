//! M8 (audit du 08/11) : chaque adresse qu'un bloc demande d'ouvrir passe le filtre de l'hôte (`adresse_permise`).
//! « Gérer les partages » (fsmgmt.msc) et « Planificateur de tâches » (taskschd.msc) étaient donnés par leur seul
//! nom : l'hôte les refusait, et le clic ne faisait rien. La liste écrite à la main de hote_tests.rs ne les avait pas
//! vus : ici, les sources de toutes les crates sont lues, et chaque adresse écrite en clair est passée au filtre.
//!
//! Ce qui est relevé (en clair dans les sources, hors des tests) :
//! - ce qui suit `OuvrirAdresse(`, `Ouvrir(`, `.ouvre(`, `.ouvrir(` et `action = Some(` (les boutons des fiches) ;
//! - toute adresse web, des Paramètres ou de la Sécurité de Windows, écrite n'importe où ;
//! - tout programme « .msc » ou « .cpl » : il doit passer par `programme_windows(` (son chemin complet) ;
//! - « run:programme » (lancé par ShellExecute, `Pc::lancer`) : un programme de System32 qui existe.
//!   « tab:n » ouvre un écran de Nevermind : rien à vérifier.

use nevermind_appli::hote::adresse_permise;
use std::path::{Path, PathBuf};

fn racine() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

/// (fichier, texte) de toutes les sources des crates de Nevermind (`src` seulement), sans les tests de l'hôte
fn sources() -> Vec<(String, String)> {
    fn lire(d: &Path, l: &mut Vec<(String, String)>) {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                lire(&p, l);
            } else if p.extension().is_some_and(|x| x == "rs") {
                l.push((p.display().to_string(), std::fs::read_to_string(&p).unwrap()));
            }
        }
    }
    let mut l = Vec::new();
    for e in std::fs::read_dir(racine()).unwrap().flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if n.starts_with("nevermind-") && e.path().join("src").is_dir() {
            lire(&e.path().join("src"), &mut l);
        }
    }
    l.retain(|(f, _)| !f.ends_with("hote_tests.rs"));
    l
}

/// Le texte d'un littéral qui commence juste après son guillemet ouvrant (`None` si ce n'est pas un littéral simple)
fn litteral(t: &str) -> Option<String> {
    let mut r = String::new();
    let mut c = t.chars();
    while let Some(x) = c.next() {
        match x {
            '"' => return Some(r),
            '\\' => match c.next()? {
                'n' => r.push('\n'),
                '\\' => r.push('\\'),
                '"' => r.push('"'),
                y => {
                    r.push('\\');
                    r.push(y);
                }
            },
            x => r.push(x),
        }
    }
    None
}

/// Les littéraux qui suivent `motif` (espaces permis entre les deux), avec leur ligne
fn apres(t: &str, motif: &str) -> Vec<(usize, String)> {
    let mut r = Vec::new();
    let mut debut = 0;
    while let Some(i) = t[debut..].find(motif) {
        let j = debut + i + motif.len();
        debut = j;
        let reste = t[j..].trim_start();
        let reste = reste.strip_prefix('&').unwrap_or(reste);
        if let Some(s) = reste.strip_prefix('"')
            && let Some(l) = litteral(s)
        {
            r.push((t[..j].lines().count(), l));
        }
    }
    r
}

/// Tous les littéraux simples d'un texte, avec leur ligne et ce qui les précède sur la ligne
fn tous(t: &str) -> Vec<(usize, String, String)> {
    let mut r = Vec::new();
    for (n, ligne) in t.lines().enumerate() {
        let code = ligne.trim_start();
        if code.starts_with("//") {
            continue;
        }
        let mut i = 0;
        while let Some(k) = ligne[i..].find('"') {
            let a = i + k;
            // (un littéral brut r"..." : lu tel quel jusqu'au guillemet suivant)
            let brut = ligne[..a].ends_with('r');
            let s = &ligne[a + 1..];
            let (texte, longueur) = if brut {
                match s.find('"') {
                    Some(f) => (s[..f].to_string(), f),
                    None => break,
                }
            } else {
                match litteral(s) {
                    Some(l) => {
                        // (la longueur dans la source : en sautant les échappements)
                        let mut f = 0;
                        let b = s.as_bytes();
                        while f < b.len() && b[f] != b'"' {
                            f += if b[f] == b'\\' { 2 } else { 1 };
                        }
                        (l, f)
                    }
                    None => break,
                }
            };
            r.push((n + 1, texte, ligne[..a].to_string()));
            i = a + 1 + longueur + 1;
            if i >= ligne.len() {
                break;
            }
        }
    }
    r
}

/// Une adresse de Windows ou du web, quelle que soit la façon dont elle est ouverte
fn est_adresse(s: &str) -> bool {
    let b = s.to_ascii_lowercase();
    (b.starts_with("https://")
        || b.starts_with("http://")
        || b.starts_with("ms-settings:")
        || b.starts_with("windowsdefender:"))
        && !b.contains(' ')
}

/// Ce que l'hôte fait d'une cible ouverte par un bloc : `Err` si le clic ne ferait rien
fn verifier(cible: &str) -> Result<(), String> {
    if cible.starts_with("tab:") {
        return Ok(());
    }
    if let Some(r) = cible.strip_prefix("run:") {
        let exe = r.split(' ').next().unwrap_or_default();
        if exe.contains(['\\', '/']) {
            return Err(format!("« {cible} » : un programme de Windows se donne par son nom (pris dans System32)"));
        }
        let complet = nevermind_noyau::programme_windows(exe);
        return if Path::new(&complet).is_file() {
            Ok(())
        } else {
            Err(format!("« {cible} » : {complet} n'existe pas"))
        };
    }
    // (une adresse faite avec format! : la partie fixe suffit au filtre)
    let fixe = cible.split('{').next().unwrap_or_default();
    if adresse_permise(fixe) { Ok(()) } else { Err(format!("« {cible} » est refusée par l'hôte")) }
}

#[test]
fn chaque_adresse_ouverte_par_un_bloc_passe_le_filtre_de_l_hote() {
    let mut vues = 0;
    let mut erreurs = Vec::new();
    for (f, t) in sources() {
        let court = f.rsplit_once("rust").map_or(f.as_str(), |x| x.1).to_string();
        // Ce qui est passé à une ouverture
        for motif in ["OuvrirAdresse(", "Ouvrir(", ".ouvre(", ".ouvrir(", "action = Some("] {
            for (n, s) in apres(&t, motif) {
                // (les textes des boutons : « action = Some("Mettre à jour") » est le libellé d'une fiche)
                if motif == "action = Some(" && !s.contains(':') {
                    continue;
                }
                vues += 1;
                if let Err(e) = verifier(&s) {
                    erreurs.push(format!("{court}:{n} : {e}"));
                }
            }
        }
        for (n, s, avant) in tous(&t) {
            let b = s.to_ascii_lowercase();
            // Un programme .msc ou .cpl : toujours par son chemin complet
            if (b.ends_with(".msc") || b.ends_with(".cpl")) && !b.contains(['\\', '/', ' ']) {
                vues += 1;
                if !avant.trim_end().ends_with("programme_windows(") {
                    erreurs.push(format!("{court}:{n} : « {s} » donné par son seul nom, sans programme_windows("));
                    continue;
                }
                let complet = nevermind_noyau::programme_windows(&s);
                if !adresse_permise(&complet) {
                    erreurs.push(format!("{court}:{n} : {complet} refusée par l'hôte"));
                }
                if !Path::new(&complet).is_file() {
                    erreurs.push(format!("{court}:{n} : {complet} n'existe pas"));
                }
            }
            // Une adresse web ou de Windows écrite n'importe où
            if est_adresse(&s) {
                vues += 1;
                if let Err(e) = verifier(&s) {
                    erreurs.push(format!("{court}:{n} : {e}"));
                }
            }
            if let Some(r) = s.strip_prefix("run:").filter(|r| !r.is_empty()) {
                vues += 1;
                if let Err(e) = verifier(&format!("run:{r}")) {
                    erreurs.push(format!("{court}:{n} : {e}"));
                }
            }
        }
    }
    assert!(vues > 40, "trop peu d'adresses relevées ({vues}) : la lecture des sources ne marche plus");
    assert!(erreurs.is_empty(), "{} adresse(s) qui ne s'ouvriraient pas :\n{}", erreurs.len(), erreurs.join("\n"));
}

/// Le relevé voit bien ce qu'il cherche (sinon le test du dessus passerait sans rien regarder)
#[test]
fn le_releve_voit_les_ouvertures() {
    let t = "effets.ajouter(Effet::OuvrirAdresse(\"fsmgmt.msc\".into()));\n    c.action = Some(\"run:mdsched.exe\".into());\n";
    assert_eq!(apres(t, "OuvrirAdresse("), vec![(1, "fsmgmt.msc".to_string())]);
    assert_eq!(apres(t, "action = Some("), vec![(2, "run:mdsched.exe".to_string())]);
    assert!(verifier("fsmgmt.msc").is_err(), "un nom seul est refusé par l'hôte");
    assert!(verifier(&nevermind_noyau::programme_windows("fsmgmt.msc")).is_ok());
    assert!(verifier("run:mdsched.exe").is_ok());
    assert!(verifier("run:programme-qui-n-existe-pas.exe").is_err());
    let l = tous("let x = r\"C:\\a\"; let y = \"taskschd.msc\";");
    assert_eq!(l.iter().map(|x| x.1.as_str()).collect::<Vec<_>>(), vec!["C:\\a", "taskschd.msc"]);
}
