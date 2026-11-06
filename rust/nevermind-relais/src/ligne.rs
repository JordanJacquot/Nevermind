//! La ligne de commande passée à Mindset.exe, et le relais lui-même sans Windows (le lancement est donné par
//! l'appelant : le programme lance vraiment, les tests regardent seulement ce qui serait lancé).

use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

/// Le message quand Mindset.exe manque (le même que relais.cs)
pub const MESSAGE_ABSENT: &str =
    "Le programme « Mindset.exe » est introuvable à côté de celui-ci.\n\nRéinstalle Nevermind depuis Mindset.";

/// Un argument tel que le programme lancé le relira (règles de Windows : guillemets, et « \ » juste avant un
/// guillemet), exactement comme `Citer` de relais.cs : un argument non vide sans espace, tabulation ni guillemet
/// passe tel quel ; sinon il est mis entre guillemets, chaque guillemet devient `\"` avec ses `\` d'avant doublés,
/// et les `\` de la fin sont doublés (pour ne pas cacher le guillemet de fin).
pub fn citer(a: &OsStr) -> OsString {
    let a: Vec<u16> = a.encode_wide().collect();
    let special = |c: u16| c == u16::from(b' ') || c == u16::from(b'\t') || c == u16::from(b'"');
    if !a.is_empty() && !a.iter().any(|&c| special(c)) {
        return OsString::from_wide(&a);
    }
    let barre = u16::from(b'\\');
    let guillemet = u16::from(b'"');
    let mut s = vec![guillemet];
    let mut barres = 0usize;
    for &c in &a {
        if c == barre {
            barres += 1;
            continue;
        }
        if c == guillemet {
            s.extend(std::iter::repeat_n(barre, barres * 2 + 1));
            s.push(guillemet);
            barres = 0;
            continue;
        }
        s.extend(std::iter::repeat_n(barre, barres));
        barres = 0;
        s.push(c);
    }
    s.extend(std::iter::repeat_n(barre, barres * 2));
    s.push(guillemet);
    OsString::from_wide(&s)
}

/// Les arguments cités, séparés par une espace (comme relais.cs)
pub fn ligne_de_commande(arguments: &[OsString]) -> OsString {
    let mut ligne = OsString::new();
    for (i, a) in arguments.iter().enumerate() {
        if i > 0 {
            ligne.push(" ");
        }
        ligne.push(citer(a));
    }
    ligne
}

/// Pourquoi Nevermind ne s'ouvre pas (le message de relais.cs)
#[derive(Debug)]
pub enum Probleme {
    /// Mindset.exe n'est pas à côté du relais (un avertissement)
    Absent,
    /// Windows n'a pas pu lancer Mindset.exe (une erreur, avec la raison de Windows)
    Echec(std::io::Error),
}

impl Probleme {
    /// Le texte à montrer
    pub fn texte(&self) -> String {
        match self {
            Probleme::Absent => MESSAGE_ABSENT.to_string(),
            Probleme::Echec(e) => format!("Nevermind n'a pas pu s'ouvrir :\n\n{e}"),
        }
    }
}

/// Ce que le relais fait, sans Windows : Mindset.exe de `dossier` (le dossier du relais) est lancé par `lancer`
/// (programme, ligne de commande, dossier de travail), avec les mêmes arguments.
pub fn relayer(
    dossier: &Path,
    arguments: &[OsString],
    lancer: impl FnOnce(&Path, &OsStr, &Path) -> std::io::Result<()>,
) -> Result<(), Probleme> {
    let exe: PathBuf = dossier.join("Mindset.exe");
    if !exe.is_file() {
        return Err(Probleme::Absent);
    }
    lancer(&exe, &ligne_de_commande(arguments), dossier).map_err(Probleme::Echec)
}

#[cfg(test)]
mod tests;
