//! Le dossier des données de Nevermind, et ce qui en part à la désinstallation.
//!
//! - [`dossier_donnees`] : `Resolve-DataDir` de Nevermind.ps1 lancé avec `-Uninstall` (et `DossierDonnees` du lanceur
//!   C#) : la copie gérée par Mindset a ses données dans `<X>:\Mindset\Donnees\Nevermind` ou
//!   `%LOCALAPPDATA%\Mindset\Nevermind`, les autres dans `%LOCALAPPDATA%\OptiGame`. Rien n'est déplacé pendant une
//!   désinstallation (l'ancien dossier est gardé s'il est le seul à exister).
//! - [`retirer_donnees`] : `Remove-DonneesDesinstallation` (systeme.ps1). Gardées : seules partent la sauvegarde des
//!   réglages d'origine (sauvegarde.json, sa copie, les sauvegardes abîmées mises de côté), les demandes (*.demande) et
//!   l'historique (sauf si des réglages n'ont pas pu être remis comme avant) ; le dossier part s'il est vide.
//!   Supprimées : tout le dossier.

use serde_json::Value;
use std::path::{Path, PathBuf};
use suite_commun::chemins::{egal, feuille, joindre, parent};

/// `Get-FullDir` : chemin complet sans « \ » final, vide s'il n'est pas valable (pas un chemin complet)
fn dossier_complet(p: &str) -> String {
    let p = p.trim();
    // (`[IO.Path]::IsPathRooted` : « \... », « /... » ou « X:... »)
    let o = p.as_bytes();
    let enracine = o.first().is_some_and(|c| matches!(c, b'\\' | b'/')) || (o.len() >= 2 && o[1] == b':');
    if !enracine {
        return String::new();
    }
    suite_commun::chemins::chemin_complet(p).map(|c| c.trim_end_matches('\\').to_string()).unwrap_or_default()
}

/// La copie est-elle gérée par Mindset (`$Managed`) ? Le dossier noté pour Nevermind dans suite.json (fichier
/// modifiable par n'importe quel programme : il ne sert qu'à comparer) est celui de cette copie.
pub fn geree(suite: Option<&Value>, racine: &Path) -> bool {
    let Some(d) = suite.and_then(|s| s.get("applis")).and_then(|a| a.get("nevermind")).and_then(Value::as_str) else {
        return false;
    };
    let d = dossier_complet(d);
    !d.is_empty() && egal(&d, &dossier_complet(&racine.to_string_lossy()))
}

/// Cette copie est-elle là où Mindset installe Nevermind, `<X>:\Mindset\Programmes\Nevermind` ? Alors elle est gérée,
/// même si suite.json n'est pas trouvé dans le `%LOCALAPPDATA%` que Windows donne (DIVERGENCES.md, point 14) : ses
/// données sont `<X>:\Mindset\Donnees\Nevermind`, un chemin tiré du dossier de ce programme seulement.
pub fn installee_par_mindset(racine: &Path) -> bool {
    let r = dossier_complet(&racine.to_string_lossy());
    let p = parent(&r);
    let m = parent(p);
    !m.is_empty() && egal(feuille(&r), "Nevermind") && egal(feuille(p), "Programmes") && egal(feuille(m), "Mindset")
}

/// `Get-ManagedDataDir` : `<X>:\Mindset\Programmes\Nevermind` donne `<X>:\Mindset\Donnees\Nevermind`, sinon
/// `%LOCALAPPDATA%\Mindset\Nevermind`
pub fn dossier_donnees_gere(racine: &Path, localappdata: &str) -> PathBuf {
    let r = racine.to_string_lossy();
    let p = parent(&r);
    let m = parent(p);
    if !m.is_empty() && egal(feuille(p), "Programmes") && egal(feuille(m), "Mindset") {
        return PathBuf::from(joindre(m, "Donnees\\Nevermind"));
    }
    PathBuf::from(joindre(localappdata, "Mindset\\Nevermind"))
}

/// `Resolve-DataDir` pendant la désinstallation (Nevermind.ps1:134-154, `$Uninstall` : rien n'est déplacé)
pub fn dossier_donnees(geree: bool, racine: &Path, localappdata: &str) -> PathBuf {
    let ancien = PathBuf::from(joindre(localappdata, "OptiGame"));
    if !geree {
        return ancien;
    }
    let nouveau = dossier_donnees_gere(racine, localappdata);
    if nouveau.is_dir() || !ancien.is_dir() { nouveau } else { ancien }
}

/// Le dossier des données de la suite : le parent du dossier des données d'une copie gérée (`<X>:\Mindset\Donnees`,
/// ou `%LOCALAPPDATA%\Mindset`), même si ses données sont encore dans l'ancien dossier ; `None` pour une copie non gérée
pub fn dossier_suite(geree: bool, racine: &Path, localappdata: &str) -> Option<PathBuf> {
    if !geree {
        return None;
    }
    let d = dossier_donnees_gere(racine, localappdata);
    let p = parent(&d.to_string_lossy()).to_string();
    (!p.is_empty()).then(|| PathBuf::from(p))
}

/// Le message d'une erreur de Windows, sans « (os error N) » à la fin
pub fn message_erreur(e: &std::io::Error) -> String {
    let t = e.to_string();
    match t.rfind(" (os error ") {
        Some(i) if t.ends_with(')') => t[..i].trim_end().to_string(),
        _ => t,
    }
}

/// Un nom correspond-il au filtre (`Get-ChildItem -Filter` : « * » pour n'importe quoi, « ? » pour un caractère),
/// sans tenir compte des majuscules ?
pub fn correspond(nom: &str, filtre: &str) -> bool {
    fn ok(n: &[char], f: &[char]) -> bool {
        match f.split_first() {
            None => n.is_empty(),
            Some(('*', reste)) => (0..=n.len()).any(|i| ok(&n[i..], reste)),
            Some(('?', reste)) => !n.is_empty() && ok(&n[1..], reste),
            Some((c, reste)) => n.first().is_some_and(|x| x == c) && ok(&n[1..], reste),
        }
    }
    let n: Vec<char> = nom.to_lowercase().chars().collect();
    let f: Vec<char> = filtre.to_lowercase().chars().collect();
    ok(&n, &f)
}

/// Supprime le dossier et tout ce qu'il contient (`Remove-Item -Recurse -Force`), sans jamais suivre un lien : un
/// lien est retiré seul, ce qu'il vise n'est pas touché (le désinstalleur tourne en administrateur : un lien posé dans
/// les données, avant ou pendant le travail, ne doit pas lui faire effacer autre chose ; audit du 08/11, M4). Sous
/// Windows, tout est ouvert sans suivre les liens, vérifié, puis effacé par sa poignée (`os::effacement`). Continue
/// malgré une erreur ; rend la première.
pub fn supprimer_dossier(d: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        crate::os::effacement::supprimer_dossier(d).map_err(|e| message_erreur(&e))
    }
    #[cfg(not(windows))]
    {
        par_chemin::supprimer_dossier(d)
    }
}

/// Ce qui part quand les données sont gardées (dans cet ordre), sans l'historique
pub const PARTENT: [&str; 5] =
    ["sauvegarde.json", "sauvegarde.json.bak", "sauvegarde.json.tmp", "sauvegarde-abimee-*.json", "*.demande"];
/// L'historique (et sa copie de secours, qui serait reprise), quand tout a été remis comme avant
pub const HISTORIQUE: [&str; 3] = ["historique.json", "historique.json.bak", "historique.json.tmp"];

fn filtres(garder_historique: bool) -> Vec<&'static str> {
    let mut filtres: Vec<&str> = PARTENT.to_vec();
    if !garder_historique {
        filtres.extend(HISTORIQUE);
    }
    filtres
}

/// `Remove-DonneesDesinstallation` : les données après la restauration. `garder_historique` : des réglages n'ont pas
/// pu être remis comme avant (ou la sauvegarde avait un problème). Rend la liste des erreurs (vide si tout va bien).
/// Un dossier des données qui est un lien, ou qui passe par un lien, n'est jamais touché (audit du 08/11, M4).
#[cfg(windows)]
pub fn retirer_donnees(dossier: &Path, garder: bool, garder_historique: bool) -> Vec<String> {
    use crate::os::effacement::{self, EtatDonnees};
    let mut erreurs = Vec::new();
    let etat = match effacement::ouvrir_donnees(dossier) {
        Ok(e) => e,
        Err(e) => {
            erreurs.push(format!("Données: {}", message_erreur(&e)));
            return erreurs;
        }
    };
    if !garder {
        if !matches!(etat, EtatDonnees::Absent) {
            drop(etat);
            if let Err(e) = supprimer_dossier(dossier) {
                erreurs.push(format!("Données: {e}"));
            }
        }
        return erreurs;
    }
    let d = match etat {
        EtatDonnees::Absent => return erreurs,
        EtatDonnees::Lien => {
            erreurs.push(format!("Données: {}", effacement::PAR_UN_LIEN));
            return erreurs;
        }
        EtatDonnees::Dossier(d) => d,
    };
    // (les fichiers du dossier seulement, cachés compris, dans l'ordre où Windows les donne)
    let fichiers = d.fichiers().unwrap_or_default();
    for f in filtres(garder_historique) {
        for nom in &fichiers {
            if correspond(nom, f)
                && let Err(e) = d.effacer_fichier(nom)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                erreurs.push(format!("Données ({nom}): {}", message_erreur(&e)));
            }
        }
    }
    // Plus rien à garder : le dossier part aussi
    d.effacer_si_vide();
    erreurs
}

#[cfg(not(windows))]
pub fn retirer_donnees(dossier: &Path, garder: bool, garder_historique: bool) -> Vec<String> {
    par_chemin::retirer_donnees(dossier, garder, garder_historique)
}

/// Hors Windows (le désinstalleur n'y tourne pas) : le même travail par les chemins
#[cfg(not(windows))]
mod par_chemin {
    use super::*;

    fn est_lien(m: &std::fs::Metadata) -> bool {
        m.file_type().is_symlink()
    }

    fn effacer_fichier(p: &Path) -> std::io::Result<()> {
        std::fs::remove_file(p)
    }

    pub fn supprimer_dossier(d: &Path) -> Result<(), String> {
        let mut premiere: Option<String> = None;
        fn parcourir(d: &Path, premiere: &mut Option<String>) {
            let note = |e: std::io::Error, premiere: &mut Option<String>| {
                if premiere.is_none() {
                    *premiere = Some(message_erreur(&e));
                }
            };
            match std::fs::read_dir(d) {
                Ok(entrees) => {
                    for e in entrees.flatten() {
                        let p = e.path();
                        let Ok(m) = std::fs::symlink_metadata(&p) else { continue };
                        let r = if est_lien(&m) {
                            effacer_fichier(&p)
                        } else if m.is_dir() {
                            parcourir(&p, premiere);
                            continue;
                        } else {
                            effacer_fichier(&p)
                        };
                        if let Err(e) = r {
                            note(e, premiere);
                        }
                    }
                }
                Err(e) => note(e, premiere),
            }
            if let Err(e) = std::fs::remove_dir(d) {
                note(e, premiere);
            }
        }
        match std::fs::symlink_metadata(d) {
            Ok(m) if est_lien(&m) => effacer_fichier(d).map_err(|e| message_erreur(&e))?,
            Ok(_) => parcourir(d, &mut premiere),
            Err(e) => return Err(message_erreur(&e)),
        }
        match premiere {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    pub fn retirer_donnees(dossier: &Path, garder: bool, garder_historique: bool) -> Vec<String> {
        let mut erreurs = Vec::new();
        match std::fs::symlink_metadata(dossier) {
            Ok(m) if m.is_dir() => {}
            Ok(m) if est_lien(&m) && !garder => {
                let _ = effacer_fichier(dossier);
                return erreurs;
            }
            _ => return erreurs,
        }
        if !garder {
            if let Err(e) = supprimer_dossier(dossier) {
                erreurs.push(format!("Données: {e}"));
            }
            return erreurs;
        }
        let fichiers: Vec<(String, PathBuf)> = std::fs::read_dir(dossier)
            .map(|r| {
                r.flatten()
                    .filter(|e| std::fs::symlink_metadata(e.path()).is_ok_and(|m| !m.is_dir()))
                    .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                    .collect()
            })
            .unwrap_or_default();
        for f in filtres(garder_historique) {
            for (nom, p) in &fichiers {
                if correspond(nom, f)
                    && let Err(e) = effacer_fichier(p)
                    && e.kind() != std::io::ErrorKind::NotFound
                {
                    erreurs.push(format!("Données ({nom}): {}", message_erreur(&e)));
                }
            }
        }
        if std::fs::read_dir(dossier).is_ok_and(|mut r| r.next().is_none()) {
            let _ = std::fs::remove_dir(dossier);
        }
        erreurs
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn filtres() {
        assert!(correspond("sauvegarde-abimee-20260101-101010.json", "sauvegarde-abimee-*.json"));
        assert!(correspond("AFFICHER.DEMANDE", "*.demande"));
        assert!(correspond(".demande", "*.demande"));
        assert!(!correspond("afficher.demande.txt", "*.demande"));
        assert!(!correspond("sauvegarde.json.old", "sauvegarde.json"));
        assert!(correspond("Sauvegarde.JSON", "sauvegarde.json"));
    }

    #[test]
    fn copie_geree() {
        let s = json!({ "applis": { "nevermind": "D:\\Mindset\\Programmes\\Nevermind\\" } });
        assert!(geree(Some(&s), Path::new("d:\\mindset\\programmes\\nevermind")));
        assert!(!geree(Some(&s), Path::new("D:\\Ailleurs\\Nevermind")));
        assert!(!geree(
            Some(&json!({ "applis": { "nevermind": "Programmes\\Nevermind" } })),
            Path::new("Programmes\\Nevermind")
        ));
        assert!(!geree(Some(&json!({ "applis": { "nevermind": 3 } })), Path::new("D:\\x")));
        assert!(!geree(None, Path::new("D:\\x")));
        assert_eq!(
            dossier_donnees_gere(Path::new("E:\\Mindset\\Programmes\\Nevermind"), "C:\\U\\AppData\\Local"),
            PathBuf::from("E:\\Mindset\\Donnees\\Nevermind")
        );
        assert_eq!(
            dossier_donnees_gere(Path::new("C:\\Program Files\\Mindset\\Nevermind"), "C:\\U\\AppData\\Local"),
            PathBuf::from("C:\\U\\AppData\\Local\\Mindset\\Nevermind")
        );
        assert_eq!(dossier_donnees(false, Path::new("C:\\x"), "C:\\L"), PathBuf::from("C:\\L\\OptiGame"));
        // Le dossier des données de la suite : le parent de celui d'une copie gérée, rien pour une autre
        assert_eq!(
            dossier_suite(true, Path::new("E:\\Mindset\\Programmes\\Nevermind"), "C:\\L"),
            Some(PathBuf::from("E:\\Mindset\\Donnees"))
        );
        assert_eq!(
            dossier_suite(true, Path::new("C:\\Program Files\\Mindset\\Nevermind"), "C:\\L"),
            Some(PathBuf::from("C:\\L\\Mindset"))
        );
        assert_eq!(dossier_suite(false, Path::new("E:\\Mindset\\Programmes\\Nevermind"), "C:\\L"), None);
    }

    #[test]
    fn installee_la_ou_mindset_l_installe() {
        assert!(installee_par_mindset(Path::new("E:\\Mindset\\Programmes\\Nevermind")));
        assert!(installee_par_mindset(Path::new("e:\\mindset\\programmes\\nevermind\\")));
        assert!(installee_par_mindset(Path::new("C:\\Users\\a\\Temp\\x\\Mindset\\Programmes\\Nevermind")));
        assert!(!installee_par_mindset(Path::new("E:\\Mindset\\Programmes\\Evermind")));
        assert!(!installee_par_mindset(Path::new("E:\\Autre\\Programmes\\Nevermind")));
        assert!(!installee_par_mindset(Path::new("C:\\Program Files\\Mindset\\Nevermind")));
        assert!(!installee_par_mindset(Path::new("Mindset\\Programmes\\Nevermind")));
        assert!(!installee_par_mindset(Path::new("D:\\Nevermind")));
    }

    #[test]
    fn un_lien_dans_les_donnees_n_est_jamais_suivi() {
        let base = std::env::temp_dir().join(format!("nevermind-desinst-lien-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let cible = base.join("cible");
        let donnees = base.join("donnees");
        std::fs::create_dir_all(&cible).unwrap();
        std::fs::create_dir_all(donnees.join("sous")).unwrap();
        std::fs::write(cible.join("precieux.txt"), "ne pas toucher").unwrap();
        std::fs::write(donnees.join("sous").join("a.txt"), "a").unwrap();
        // Un point de jonction vers « cible » (sans droits : mklink /J, sans fenêtre)
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("cmd.exe")
                .raw_arg(format!("/c mklink /J \"{}\" \"{}\"", donnees.join("lien").display(), cible.display()))
                .creation_flags(0x0800_0000)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        assert!(retirer_donnees(&donnees, false, false).is_empty());
        assert!(!donnees.exists());
        assert!(cible.join("precieux.txt").exists(), "le lien a été suivi");
        let _ = std::fs::remove_dir_all(&base);
    }

    // -----------------------------------------------------------------------------------------------------------------
    // M4 (audit du 08/11) : en administrateur, une jonction posée à la place d'un dossier (avant ou PENDANT le travail)
    // ne fait jamais effacer autre chose
    // -----------------------------------------------------------------------------------------------------------------

    type Rappel = Box<dyn FnMut(&Path)>;
    thread_local! {
        static AVANT_LECTURE: std::cell::RefCell<Option<Rappel>> = const { std::cell::RefCell::new(None) };
    }

    /// Appelé quand un sous-dossier vient d'être jugé « vrai dossier », juste avant qu'il soit lu : l'essai y gagne la
    /// course (il remplace le dossier par une jonction à ce moment-là)
    pub(crate) fn avant_lecture(p: &Path) {
        AVANT_LECTURE.with(|r| {
            if let Some(f) = r.borrow_mut().as_mut() {
                f(p);
            }
        });
    }

    /// Un point de jonction `lien` vers `cible` (sans droits : mklink /J, sans fenêtre)
    #[cfg(windows)]
    fn jonction(lien: &Path, cible: &Path) {
        use std::os::windows::process::CommandExt;
        let st = std::process::Command::new(format!("{}\\cmd.exe", nevermind_noyau::windows::dossier_systeme()))
            .raw_arg(format!("/d /c mklink /J \"{}\" \"{}\"", lien.display(), cible.display()))
            .creation_flags(0x0800_0000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(st.success(), "mklink /J {} a échoué", lien.display());
        let m = std::fs::symlink_metadata(lien).unwrap();
        use std::os::windows::fs::MetadataExt;
        assert!(m.file_attributes() & 0x400 != 0, "pas une jonction : {}", lien.display());
    }

    fn dossier_essai(nom: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("nevermind-desinst-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    /// Un précieux hors des données, que rien ne doit toucher
    fn precieux(dossier: &Path) -> PathBuf {
        std::fs::create_dir_all(dossier).unwrap();
        for n in ["precieux.txt", "sauvegarde.json", "historique.json", "x.demande"] {
            std::fs::write(dossier.join(n), "ne pas toucher").unwrap();
        }
        dossier.join("precieux.txt")
    }

    fn intact(dossier: &Path) -> bool {
        ["precieux.txt", "sauvegarde.json", "historique.json", "x.demande"].iter().all(|n| dossier.join(n).exists())
    }

    #[cfg(windows)]
    #[test]
    fn une_jonction_posee_pendant_la_suppression_n_est_jamais_suivie() {
        let base = dossier_essai("course");
        let cible = base.join("cible");
        precieux(&cible);
        let donnees = base.join("donnees");
        std::fs::create_dir_all(donnees.join("sous")).unwrap();
        std::fs::write(donnees.join("sous").join("a.txt"), "a").unwrap();
        std::fs::write(donnees.join("jeux.json"), "{}").unwrap();
        // La course gagnée : « sous » vient d'être jugé vrai dossier ; il est mis de côté et remplacé par une jonction
        let (sous, parti, c) = (donnees.join("sous"), base.join("sous-parti"), cible.clone());
        AVANT_LECTURE.with(|r| {
            *r.borrow_mut() = Some(Box::new(move |p: &Path| {
                if p.file_name() == sous.file_name() && !parti.exists() {
                    std::fs::rename(&sous, &parti).unwrap();
                    jonction(&sous, &c);
                }
            }))
        });
        let _ = retirer_donnees(&donnees, false, false);
        AVANT_LECTURE.with(|r| *r.borrow_mut() = None);
        assert!(intact(&cible), "la jonction posée pendant la suppression a été suivie");
        assert!(!donnees.join("jeux.json").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(windows)]
    #[test]
    fn un_dossier_parent_en_jonction_n_est_jamais_traverse() {
        let base = dossier_essai("parent");
        let ailleurs = base.join("ailleurs");
        precieux(&ailleurs.join("Nevermind"));
        // « Mindset » (le parent des données, dans le compte) remplacé par une jonction vers un autre dossier
        jonction(&base.join("Mindset"), &ailleurs);
        let donnees = base.join("Mindset").join("Nevermind");
        let e = retirer_donnees(&donnees, false, false);
        assert!(intact(&ailleurs.join("Nevermind")), "la jonction du dossier parent a été traversée");
        assert_eq!(e.len(), 1, "{e:?}");
        // Données gardées : pareil, rien n'est effacé à travers la jonction
        let e = retirer_donnees(&donnees, true, false);
        assert!(intact(&ailleurs.join("Nevermind")), "la jonction du dossier parent a été traversée (données gardées)");
        assert_eq!(e.len(), 1, "{e:?}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(windows)]
    #[test]
    fn des_donnees_gardees_en_jonction_ne_sont_jamais_suivies() {
        let base = dossier_essai("garder");
        let cible = base.join("cible");
        precieux(&cible);
        let donnees = base.join("donnees");
        jonction(&donnees, &cible);
        let e = retirer_donnees(&donnees, true, false);
        assert!(intact(&cible), "le dossier des données en jonction a été suivi");
        assert_eq!(e.len(), 1, "{e:?}");
        // Supprimées : seule la jonction part
        let e = retirer_donnees(&donnees, false, false);
        assert!(e.is_empty(), "{e:?}");
        assert!(std::fs::symlink_metadata(&donnees).is_err());
        assert!(intact(&cible));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(windows)]
    #[test]
    fn donnees_gardees_et_supprimees_pour_de_vrai() {
        let base = dossier_essai("vrai");
        let donnees = base.join("donnees");
        std::fs::create_dir_all(donnees.join("sous").join("plus")).unwrap();
        for n in ["sauvegarde.json", "Sauvegarde-Abimee-1.json", "historique.json", "a.demande", "jeux.json"] {
            std::fs::write(donnees.join(n), "x").unwrap();
        }
        std::fs::write(donnees.join("sous").join("plus").join("b.txt"), "b").unwrap();
        // (un fichier en lecture seule part aussi, comme avec Remove-Item -Force)
        let ro = donnees.join("sous").join("lecture-seule.txt");
        std::fs::write(&ro, "r").unwrap();
        let mut perm = std::fs::metadata(&ro).unwrap().permissions();
        perm.set_readonly(true);
        std::fs::set_permissions(&ro, perm).unwrap();
        // Gardées, historique gardé : seules la sauvegarde et les demandes partent
        assert!(retirer_donnees(&donnees, true, true).is_empty());
        let mut reste: Vec<String> =
            std::fs::read_dir(&donnees).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into()).collect();
        reste.sort();
        assert_eq!(reste, vec!["historique.json", "jeux.json", "sous"]);
        // Gardées : l'historique part aussi
        assert!(retirer_donnees(&donnees, true, false).is_empty());
        assert!(!donnees.join("historique.json").exists());
        // Supprimées : tout part
        assert!(retirer_donnees(&donnees, false, false).is_empty());
        assert!(!donnees.exists());
        // Absent : rien à faire, pas d'erreur
        assert!(retirer_donnees(&donnees, false, false).is_empty());
        assert!(retirer_donnees(&donnees, true, false).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }
}
