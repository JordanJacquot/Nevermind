//! Le nettoyage (systeme.ps1) : les catégories (`$CleanTargets`), ce que fait le nettoyage en un clic
//! (`Get-ChangementsNettoyage`), le nettoyage lui-même avec son journal (`Invoke-CleanTargets`), et le parcours des
//! dossiers de l'utilisateur (`$CleanWalk`, `$CleanScript`, `$CleanListScript`). Les dossiers de Windows sont vidés par
//! l'Aide, avec sa propre liste (jamais un chemin envoyé par Nevermind).

use crate::modele::Declaration;
use crate::net::{self, taille};
use crate::pc::{Pc, ResultatNettoyage, parametres};
use serde_json::{Value, json};
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Une catégorie du nettoyage
#[derive(Debug, Clone, PartialEq)]
pub struct Cible {
    pub id: &'static str,
    pub titre: &'static str,
    pub chemins: Vec<String>,
    pub exclus: Vec<String>,
    /// Cochée au départ (`Default -ne $false`)
    pub defaut: bool,
    pub avertir: bool,
    pub optionnelle: bool,
    /// Vidée par l'Aide
    pub aide: bool,
    pub desc: &'static str,
}

/// `$CleanTargets`
pub fn cibles(pc: &dyn Pc) -> Vec<Cible> {
    let temp = pc.dossier_temp();
    let win = pc.dossier_windows();
    let aide = |c| aide_regles::nettoyage::dossiers(c, &win);
    use aide_regles::nettoyage::Categorie as C;
    vec![
        Cible {
            id: "temp",
            titre: "Fichiers temporaires",
            chemins: vec![temp.clone()],
            exclus: vec!["claude".into()],
            defaut: true,
            avertir: false,
            optionnelle: false,
            aide: false,
            desc: "Restes laissés par tes programmes et les installations. Ce qui a moins de 24 h ou sert encore est gardé.",
        },
        Cible {
            id: "claude",
            titre: "Fichiers de travail de Claude",
            chemins: vec![format!("{temp}\\claude")],
            exclus: vec![],
            defaut: false,
            avertir: true,
            optionnelle: true,
            aide: false,
            desc: "Captures, scripts et téléchargements des discussions Claude Code. Tes discussions ne sont pas effacées, mais une discussion en cours peut encore en avoir besoin : garde-les si tu travailles avec Claude.",
        },
        Cible {
            id: "wu",
            titre: "Mises à jour Windows déjà installées",
            chemins: aide(C::Wu),
            exclus: vec![],
            defaut: true,
            avertir: false,
            optionnelle: false,
            aide: true,
            desc: "Fichiers téléchargés par Windows Update, inutiles une fois les mises à jour installées.",
        },
        Cible {
            id: "do",
            titre: "Partage des mises à jour",
            chemins: aide(C::Do),
            exclus: vec![],
            defaut: true,
            avertir: false,
            optionnelle: false,
            aide: true,
            desc: "Morceaux de mises à jour que Windows garde pour les partager avec d'autres PC. Windows les retélécharge au besoin.",
        },
        Cible {
            id: "dump",
            titre: "Rapports d'écran bleu",
            chemins: aide(C::Dump),
            exclus: vec![],
            defaut: true,
            avertir: false,
            optionnelle: false,
            aide: true,
            desc: "Créés lors d'un écran bleu. Utiles seulement pour comprendre un écran bleu récent.",
        },
    ]
}

/// `Get-CleanAllTargets` : les catégories à garder (Claude) ne partent jamais d'office
pub fn cibles_en_un_clic(pc: &dyn Pc) -> Vec<Cible> {
    cibles(pc).into_iter().filter(|c| c.defaut).collect()
}

/// `Get-ChangementsNettoyage`
pub fn changements(pc: &dyn Pc) -> Vec<Declaration> {
    let mut l: Vec<Declaration> = cibles_en_un_clic(pc)
        .iter()
        .map(|t| {
            Declaration::texte(&format!(
                "Supprime « {} » : {}",
                t.titre,
                t.chemins.iter().filter(|p| !p.is_empty()).cloned().collect::<Vec<_>>().join(", ")
            ))
        })
        .collect();
    l.push(Declaration::texte("Les fichiers exacts sont choisis pendant le nettoyage (ceux de moins de 24 h ou encore utilisés sont gardés) : leur liste est notée dans le journal du nettoyage. Ils ne peuvent pas être récupérés."));
    l
}

/// Le total d'un nettoyage (`Invoke-CleanTargets`)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Total {
    pub libere: f64,
    pub supprimes: i64,
    pub laisses: i64,
    pub refuses: i64,
    pub fichier: String,
}

/// `Invoke-CleanOne`
fn nettoyer_une(pc: &mut dyn Pc, t: &Cible) -> ResultatNettoyage {
    if let Err(e) = pc.exiger_geste() {
        return ResultatNettoyage { refuses: 1, journal: vec![format!("[REFUSÉ]   {e}")], ..Default::default() };
    }
    if !t.aide {
        return pc.nettoyer_dossier(t.id, &t.chemins, &t.exclus);
    }
    match pc.aide("nettoyage.lancer", parametres(&[("Id", json!(t.id))]), 900000) {
        Ok(r) => ResultatNettoyage {
            libere: net::champ_nombre(&r, "Freed"),
            supprimes: net::champ_entier(&r, "Deleted"),
            laisses: net::champ_entier(&r, "Skipped"),
            refuses: net::champ_entier(&r, "Errors"),
            journal: net::champ_liste(&r, "Log").iter().map(net::texte).collect(),
        },
        Err(e) => ResultatNettoyage {
            libere: 0.0,
            supprimes: 0,
            laisses: 0,
            refuses: 1,
            journal: vec![format!("[REFUSÉ]   {e}")],
        },
    }
}

/// `Invoke-CleanTargets` : nettoie les catégories, écrit le journal (dossier « nettoyage », 10 derniers gardés) et note
/// le dernier nettoyage (`LastClean`).
pub fn nettoyer(pc: &mut dyn Pc, cibles: &[Cible]) -> Total {
    let maintenant = pc.maintenant();
    let (a, mo, j, h, mi, s) = maintenant.parties();
    let mut lignes = vec![
        format!("Nettoyage Nevermind du {j:02}/{mo:02}/{a:04} à {h:02}:{mi:02}:{s:02}"),
        "SUPPRIMÉ : effacé.  LAISSÉ : utilisé par un programme, il sera effacé une prochaine fois.  REFUSÉ : Windows n'autorise pas à l'effacer.".to_string(),
    ];
    let mut tot = Total::default();
    for t in cibles {
        pc.statut(&format!("Nettoyage: {}...", t.titre));
        let r = nettoyer_une(pc, t);
        lignes.push(String::new());
        lignes.push(format!("== {} ({}) ==", t.titre, t.chemins.join(", ")));
        lignes.extend(r.journal.iter().cloned());
        if r.journal.is_empty() {
            lignes.push("(dossier déjà vide)".into());
        }
        tot.libere += r.libere;
        tot.supprimes += r.supprimes;
        tot.laisses += r.laisses;
        tot.refuses += r.refuses;
    }
    lignes.push(String::new());
    lignes.push(format!(
        "Total : {} libérés, {} fichier(s) supprimé(s), {} laissé(s), {} refusé(s).",
        taille(tot.libere),
        tot.supprimes,
        tot.laisses,
        tot.refuses
    ));
    let nom = format!("nettoyage-{a:04}-{mo:02}-{j:02}_{h:02}-{mi:02}-{s:02}.log");
    let fichier = pc.journal_nettoyage(&nom, &lignes);
    let _ = pc.ecrire_reglage(
        "LastClean",
        json!({ "Date": format!("{}.0000000", maintenant.iso()), "Freed": tot.libere, "Deleted": tot.supprimes, "Skipped": tot.laisses, "Errors": tot.refuses, "File": fichier }),
        false,
    );
    pc.journal(&format!(
        "Nettoyage: {} libérés, {} supprimés, {} laissés, {} refusés",
        taille(tot.libere),
        tot.supprimes,
        tot.laisses,
        tot.refuses
    ));
    tot.fichier = fichier;
    tot
}

// ---------------------------------------------------------------------------
// Le parcours des dossiers de l'utilisateur (sans suivre les liens : on ne sort jamais du dossier prévu)
// ---------------------------------------------------------------------------
fn est_lien(m: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        m.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        m.file_type().is_symlink()
    }
}

/// `Get-CleanFiles` : les fichiers de plus de 24 h (et les dossiers parcourus)
fn fichiers(
    racine: &Path,
    sauter: &[String],
    avant: SystemTime,
    dossiers: &mut Vec<String>,
    sortie: &mut Vec<(String, u64, SystemTime)>,
) {
    let Ok(lecture) = std::fs::read_dir(racine) else { return };
    for e in lecture.flatten() {
        let Ok(m) = std::fs::symlink_metadata(e.path()) else { continue };
        let chemin = e.path().to_string_lossy().to_string();
        if m.is_dir() || (est_lien(&m) && e.path().is_dir()) {
            if est_lien(&m) {
                continue;
            }
            if sauter.iter().any(|s| s.eq_ignore_ascii_case(&chemin)) {
                continue;
            }
            dossiers.push(chemin);
            fichiers(&e.path(), sauter, avant, dossiers, sortie);
        } else if let Ok(t) = m.modified()
            && t < avant
        {
            sortie.push((chemin, m.len(), t));
        }
    }
}

fn sauter(chemins: &[String], exclus: &[String]) -> Vec<String> {
    let mut s = Vec::new();
    for p in chemins {
        for x in exclus.iter().filter(|x| !x.is_empty()) {
            s.push(format!("{}\\{}", p.trim_end_matches('\\'), x));
        }
    }
    s
}

/// La taille d'un fichier dans le journal du nettoyage
fn taille_journal(b: f64) -> String {
    const KO: f64 = 1024.0;
    if b >= KO * KO * KO {
        format!("{} Go", net::n_fr(b / (KO * KO * KO), 1))
    } else if b >= KO * KO {
        format!("{} Mo", net::n_fr(b / (KO * KO), 1))
    } else {
        format!("{} Ko", net::n_fr((b / KO).ceil().max(1.0), 0))
    }
}

/// `$CleanScript` : le nettoyage d'une catégorie de l'utilisateur, fichier par fichier
pub fn nettoyer_chemins(chemins: &[String], exclus: &[String], maintenant: SystemTime) -> ResultatNettoyage {
    let avant = maintenant.checked_sub(Duration::from_secs(86400)).unwrap_or(maintenant);
    let saut = sauter(chemins, exclus);
    let mut r = ResultatNettoyage::default();
    for p in chemins {
        let racine = Path::new(p);
        if !racine.exists() {
            continue;
        }
        let mut dossiers = Vec::new();
        let mut liste = Vec::new();
        fichiers(racine, &saut, avant, &mut dossiers, &mut liste);
        for (f, len, _) in liste {
            let l = len as f64;
            if let Ok(m) = std::fs::metadata(&f)
                && m.permissions().readonly()
            {
                let mut pr = m.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                pr.set_readonly(false);
                let _ = std::fs::set_permissions(&f, pr);
            }
            match std::fs::remove_file(&f) {
                Ok(()) => {
                    r.libere += l;
                    r.supprimes += 1;
                    r.journal.push(format!("[SUPPRIMÉ] {f} - {}", taille_journal(l)));
                }
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    r.refuses += 1;
                    r.journal.push(format!("[REFUSÉ]   {f} - {} - Accès refusé", taille_journal(l)));
                }
                Err(_) => {
                    r.laisses += 1;
                    r.journal
                        .push(format!("[LAISSÉ]   {f} - {} - Fichier utilisé par un programme", taille_journal(l)));
                }
            }
        }
        // Dossiers restés vides : du plus profond au moins profond (le dossier de départ reste)
        dossiers.sort_by_key(|d| std::cmp::Reverse(d.len()));
        for d in dossiers {
            if std::fs::read_dir(&d).map(|mut x| x.next().is_none()).unwrap_or(false) {
                let _ = std::fs::remove_dir(&d);
            }
        }
    }
    r
}

/// `$CleanListScript` : taille, nombre et les plus gros fichiers d'une catégorie (« Voir les fichiers »)
pub fn lister_chemins(chemins: &[String], exclus: &[String], maintenant: SystemTime, top: usize) -> Value {
    let avant = maintenant.checked_sub(Duration::from_secs(86400)).unwrap_or(maintenant);
    let saut = sauter(chemins, exclus);
    let mut tous = Vec::new();
    for p in chemins {
        if Path::new(p).exists() {
            let mut d = Vec::new();
            fichiers(Path::new(p), &saut, avant, &mut d, &mut tous);
        }
    }
    let somme: f64 = tous.iter().map(|x| x.1 as f64).sum();
    let n = tous.len();
    tous.sort_by_key(|x| std::cmp::Reverse(x.1));
    let plus_gros: Vec<Value> = tous
        .iter()
        .take(top)
        .map(|(f, l, t)| {
            json!([f, *l as f64, t.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)])
        })
        .collect();
    json!({ "Size": somme, "Count": n, "Top": plus_gros })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nettoyage_d_un_dossier_d_essai() {
        let d = nevermind_noyau::dossier_essai("pc-nettoyage");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sous\\vide")).unwrap();
        std::fs::create_dir_all(d.join("claude")).unwrap();
        std::fs::write(d.join("a.tmp"), vec![0u8; 3000]).unwrap();
        std::fs::write(d.join("sous\\b.tmp"), vec![0u8; 10]).unwrap();
        std::fs::write(d.join("claude\\garde.txt"), b"x").unwrap();
        let racine = d.to_string_lossy().to_string();
        // (« maintenant » dans deux jours : les fichiers ont plus de 24 h)
        let plus_tard = SystemTime::now() + Duration::from_secs(2 * 86400);
        let l = lister_chemins(std::slice::from_ref(&racine), &["claude".into()], plus_tard, 10);
        assert_eq!(l["Count"], json!(2));
        let r = nettoyer_chemins(std::slice::from_ref(&racine), &["claude".into()], plus_tard);
        assert_eq!(r.supprimes, 2);
        assert!(r.journal.iter().any(|x| x.starts_with("[SUPPRIMÉ] ") && x.ends_with(" - 3 Ko")));
        assert!(d.join("claude\\garde.txt").exists(), "le dossier de Claude est gardé");
        assert!(!d.join("sous").exists(), "les dossiers vides partent");
        assert!(d.exists(), "le dossier de départ reste");
        // (maintenant : rien n'a plus de 24 h)
        std::fs::write(d.join("neuf.tmp"), b"x").unwrap();
        assert_eq!(nettoyer_chemins(&[racine], &[], SystemTime::now()).supprimes, 0);
        let _ = std::fs::remove_dir_all(&d);
    }
}
