//! Les conseils passés à Mindset quand il pilote la désinstallation (`-GarderDonnees` ou `-SupprimerDonnees` :
//! DIVERGENCES.md, point 15). La fenêtre de fin n'est pas montrée ; ce qu'elle aurait dit d'important, en plus de
//! « Nevermind est désinstallé. » et de la phrase sur les données (Mindset les dit lui-même), est écrit dans
//! `<dossier des données de la suite>\Nevermind.conseils-desinstallation.txt`, que Mindset affiche dans son bilan.
//!
//! Le contrat (docs\contrat-appli.md de Mindset) : UTF-8 sans BOM, une ligne par conseil, au plus 10 lignes de
//! 300 caractères ; rien à dire : pas de fichier (un ancien est effacé) ; copie non gérée : rien n'est écrit. Le
//! fichier est écrit à côté puis renommé, sans jamais suivre un lien (le désinstalleur tourne en administrateur).

use std::io::Write;
use std::path::Path;

/// Le nom du fichier, dans le dossier des données de la suite
pub const NOM: &str = "Nevermind.conseils-desinstallation.txt";
/// Au plus tant de lignes
pub const MAX_LIGNES: usize = 10;
/// Au plus tant de caractères par ligne
pub const MAX_CARACTERES: usize = 300;

/// « Redémarre le PC » : des réglages ont été remis comme avant
pub const REDEMARRER: &str = "Redémarre le PC pour que tous les réglages d'origine soient pris en compte.";

/// Les lignes telles qu'elles seront écrites : chacune sur une seule ligne (espaces et retours à la ligne réduits à un
/// espace), sans ligne vide, coupée à [`MAX_CARACTERES`] caractères, et au plus [`MAX_LIGNES`] lignes
pub fn preparer<I, S>(lignes: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    lignes
        .into_iter()
        .map(|l| l.as_ref().split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .map(|l| l.chars().take(MAX_CARACTERES).collect::<String>().trim_end().to_string())
        .take(MAX_LIGNES)
        .collect()
}

/// Un lien (point de jonction, lien symbolique) : jamais suivi
fn est_lien(m: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        m.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
    }
    #[cfg(not(windows))]
    {
        m.file_type().is_symlink()
    }
}

/// Écrit les conseils (déjà [`preparer`]) dans `dossier_suite`, ou efface un ancien fichier s'il n'y a rien à dire.
/// `None` (copie non gérée par Mindset) : rien n'est fait. Le dossier de la suite doit exister et ne pas être un lien
/// (il n'est jamais créé ici). Le fichier est écrit à côté (`.tmp`, créé neuf), puis renommé à sa place : un lien
/// posé à la place du fichier est remplacé, ce qu'il vise n'est pas touché.
pub fn ecrire(dossier_suite: Option<&Path>, lignes: &[String]) -> Result<(), String> {
    let Some(dossier) = dossier_suite else { return Ok(()) };
    match std::fs::symlink_metadata(dossier) {
        Ok(m) if m.is_dir() && !est_lien(&m) => {}
        Ok(_) => return Err(format!("{} n'est pas un dossier", dossier.display())),
        // (dossier absent : il n'y a rien à effacer, et rien n'est créé)
        Err(_) => return Ok(()),
    }
    let fichier = dossier.join(NOM);
    let tmp = dossier.join(format!("{NOM}.tmp"));
    let erreur = |e: std::io::Error| crate::donnees::message_erreur(&e);
    // (remove_file retire un lien lui-même, jamais ce qu'il vise)
    let effacer = |p: &Path| match std::fs::remove_file(p) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(erreur(e)),
        _ => Ok(()),
    };
    effacer(&tmp)?;
    if lignes.is_empty() {
        return effacer(&fichier);
    }
    let mut texte = lignes.join("\r\n");
    texte.push_str("\r\n");
    let ecrit = (|| -> std::io::Result<()> {
        // (create_new : jamais un fichier ou un lien déjà là)
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(texte.as_bytes())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, &fichier)
    })();
    if let Err(e) = ecrit {
        let _ = std::fs::remove_file(&tmp);
        return Err(erreur(e));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lignes_coupees() {
        let longue = "é".repeat(400);
        let l = preparer(["  a\r\n  b  ", "", "   ", &longue]);
        assert_eq!(l[0], "a b");
        assert_eq!(l.len(), 2);
        assert_eq!(l[1].chars().count(), MAX_CARACTERES);
        let n: Vec<String> = (0..25).map(|i| format!("ligne {i}")).collect();
        let l = preparer(&n);
        assert_eq!(l.len(), MAX_LIGNES);
        assert_eq!(l[9], "ligne 9");
    }
}
