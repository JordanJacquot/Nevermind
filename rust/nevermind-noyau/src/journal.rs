//! Le journal de Nevermind (journal.txt, dans le dossier de données) : une ligne datée par événement (`Write-Log`,
//! modules\donnees.ps1), et sa mise au propre au démarrage.

use std::io::Write;
use std::path::Path;

/// Au-delà de cette taille, le journal est raccourci au démarrage.
pub const TAILLE_MAX: u64 = 1024 * 1024;
/// Les lignes gardées quand il est raccourci (les plus anciennes passent dans journal-ancien.txt).
pub const LIGNES_GARDEES: usize = 3000;

/// Les lignes d'un texte comme `[IO.File]::ReadAllLines` : coupées à « \r\n », « \r » ou « \n », sans ligne vide
/// après un dernier retour à la ligne.
pub fn lignes(texte: &str) -> Vec<&str> {
    let mut l = Vec::new();
    let mut debut = 0;
    let o = texte.as_bytes();
    let mut i = 0;
    while i < o.len() {
        match o[i] {
            b'\r' => {
                l.push(&texte[debut..i]);
                i += if o.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                debut = i;
            }
            b'\n' => {
                l.push(&texte[debut..i]);
                i += 1;
                debut = i;
            }
            _ => i += 1,
        }
    }
    if debut < o.len() {
        l.push(&texte[debut..]);
    }
    l
}

/// `[IO.File]::WriteAllLines` (UTF-8 sans BOM) : chaque ligne suivie de « \r\n ».
fn ecrire_lignes(f: &Path, l: &[&str]) -> std::io::Result<()> {
    let mut t = String::new();
    for x in l {
        t.push_str(x);
        t.push_str("\r\n");
    }
    std::fs::write(f, t.as_bytes())
}

/// Au chargement (donnees.ps1) : un journal de plus de 1 Mo et de plus de 3000 lignes garde ses 3000 dernières ; les
/// plus anciennes vont dans journal-ancien.txt (qui est remplacé). Une erreur n'empêche rien.
pub fn raccourcir(dossier: &Path) {
    let f = dossier.join("journal.txt");
    let Ok(m) = std::fs::metadata(&f) else { return };
    if !m.is_file() || m.len() <= TAILLE_MAX {
        return;
    }
    let Ok(o) = std::fs::read(&f) else { return };
    let texte = suite_commun::json_sur::texte_du_fichier(&o);
    let toutes = lignes(&texte);
    if toutes.len() <= LIGNES_GARDEES {
        return;
    }
    let coupe = toutes.len() - LIGNES_GARDEES;
    if ecrire_lignes(&dossier.join("journal-ancien.txt"), &toutes[..coupe]).is_ok() {
        let _ = ecrire_lignes(&f, &toutes[coupe..]);
    }
}

/// Ajoute une ligne (« 2026-11-06T21:05:09 message ») à la fin du journal, UTF-8 sans BOM.
pub fn ajouter(fichier: &Path, horodatage: &str, message: &str) -> std::io::Result<()> {
    let ligne = format!("{horodatage} {message}\r\n");
    std::fs::OpenOptions::new().create(true).append(true).open(fichier)?.write_all(ligne.as_bytes())
}

/// Le nom de l'erreur, comme `$_.Exception.GetType().Name` (pour `LogError`) : en PowerShell, l'appel de
/// `[IO.File]::AppendAllText` qui échoue donne toujours une `MethodInvocationException` (l'erreur de .NET est dedans).
pub fn nom_erreur(_e: &std::io::Error) -> &'static str {
    "MethodInvocationException"
}

/// Les `n` dernières lignes du journal (`Get-Content -Tail n`).
pub fn dernieres_lignes(fichier: &Path, n: usize) -> Vec<String> {
    let Ok(o) = std::fs::read(fichier) else { return Vec::new() };
    let t = suite_commun::json_sur::texte_du_fichier(&o);
    let l = lignes(&t);
    l[l.len().saturating_sub(n)..].iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lignes_comme_readalllines() {
        assert_eq!(lignes("a\r\nb\nc\rd"), vec!["a", "b", "c", "d"]);
        assert_eq!(lignes("a\r\n"), vec!["a"]);
        assert_eq!(lignes("a\r\n\r\n"), vec!["a", ""]);
        assert!(lignes("").is_empty());
    }
}
