//! Les tests du relais, sans rien lancer : la citation des arguments (cas tirés des règles de `Citer` de relais.cs,
//! puis relus par Windows lui-même avec CommandLineToArgvW, sans lancer de programme) et le chemin de Mindset.exe
//! (dossier d'essai sous %TEMP%, avec un faux Mindset.exe vide jamais lancé).

// (seulement ici : la relecture de la ligne par Windows)
#![allow(unsafe_code)]

use super::*;

fn os(s: &str) -> OsString {
    OsString::from(s)
}

/// La ligne relue par Windows comme le ferait Mindset.exe (CommandLineToArgvW), sans le nom du programme
fn relire(ligne: &OsStr) -> Vec<OsString> {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::UI::Shell::CommandLineToArgvW;
    use windows::core::PCWSTR;
    let mut complete: Vec<u16> = OsStr::new("Mindset.exe ").encode_wide().collect();
    complete.extend(ligne.encode_wide());
    complete.push(0);
    let mut n = 0i32;
    // SAFETY : texte terminé par un zéro, vivant pendant l'appel ; le tableau rendu est lu dans ses limites (n) puis
    // rendu à Windows
    unsafe {
        let tableau = CommandLineToArgvW(PCWSTR(complete.as_ptr()), &mut n);
        assert!(!tableau.is_null(), "CommandLineToArgvW a échoué");
        let lus = (1..n as usize).map(|i| OsString::from_wide((*tableau.add(i)).as_wide())).collect();
        let _ = LocalFree(Some(HLOCAL(tableau.cast())));
        lus
    }
}

/// Les cas fixes : l'argument et ce que relais.cs en fait
const CAS: &[(&str, &str)] = &[
    // tels quels : non vides, sans espace, tabulation ni guillemet (même avec des « \ »)
    ("-Demarrage", "-Demarrage"),
    ("/Demarrage", "/Demarrage"),
    (r"C:\Mindset\Programmes\Nevermind", r"C:\Mindset\Programmes\Nevermind"),
    (r"\\serveur\partage\", r"\\serveur\partage\"),
    ("éàü", "éàü"),
    // vide : deux guillemets
    ("", r#""""#),
    // espace, tabulation : entre guillemets
    ("a b", r#""a b""#),
    ("a\tb", "\"a\tb\""),
    (" ", r#"" ""#),
    // les « \ » qui ne sont pas avant un guillemet restent tels quels
    (r"C:\Program Files\Mindset", r#""C:\Program Files\Mindset""#),
    (r"a\\b c", r#""a\\b c""#),
    // les « \ » de la fin sont doublés (ils sont avant le guillemet de fin)
    (r"C:\Program Files\", r#""C:\Program Files\\""#),
    (r"a b\\", r#""a b\\\\""#),
    // un guillemet devient \" ; les « \ » juste avant lui sont doublés, plus un
    (r#"a"b"#, r#""a\"b""#),
    (r#"a\"b"#, r#""a\\\"b""#),
    (r#"a\\"b"#, r#""a\\\\\"b""#),
    (r#"""#, r#""\"""#),
    (r#"dit "bonjour" à tous"#, r#""dit \"bonjour\" à tous""#),
];

#[test]
fn chaque_argument_est_cite_comme_relais_cs() {
    for (argument, attendu) in CAS {
        assert_eq!(citer(OsStr::new(argument)), os(attendu), "argument {argument:?}");
    }
}

#[test]
fn les_arguments_sont_separes_par_une_espace() {
    assert_eq!(ligne_de_commande(&[]), os(""));
    assert_eq!(ligne_de_commande(&[os("-Demarrage")]), os("-Demarrage"));
    assert_eq!(ligne_de_commande(&[os("-Demarrage"), os("a b"), os(""), os(r"c\")]), os(r#"-Demarrage "a b" "" c\"#));
}

#[test]
fn windows_relit_exactement_les_memes_arguments() {
    // Chaque cas fixe seul, puis tous ensemble
    let tous: Vec<OsString> = CAS.iter().map(|(a, _)| os(a)).collect();
    for a in &tous {
        assert_eq!(relire(&ligne_de_commande(std::slice::from_ref(a))), vec![a.clone()], "argument {a:?}");
    }
    assert_eq!(relire(&ligne_de_commande(&tous)), tous);
    // Des arguments tirés au hasard (toujours les mêmes) dans les caractères qui comptent
    let lettres: Vec<u16> = "a \t\"\\é".encode_utf16().collect();
    let mut graine: u64 = 0x5EED_1234_ABCD_0001;
    for _ in 0..3000 {
        let mut arguments = Vec::new();
        graine = graine.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        for _ in 0..1 + (graine >> 60) as usize % 4 {
            let mut a = Vec::new();
            graine = graine.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            for _ in 0..(graine >> 58) as usize % 12 {
                graine = graine.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                a.push(lettres[(graine >> 33) as usize % lettres.len()]);
            }
            arguments.push(OsString::from_wide(&a));
        }
        assert_eq!(relire(&ligne_de_commande(&arguments)), arguments);
    }
}

/// Un dossier d'essai neuf sous le dossier temporaire, effacé à la fin
struct Essai(PathBuf);
impl Essai {
    fn new(nom: &str) -> Essai {
        let d = std::env::temp_dir().join(format!("nevermind-relais-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dossier d'essai");
        Essai(d)
    }
}
impl Drop for Essai {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn sans_mindset_exe_le_message_de_relais_cs() {
    let e = Essai::new("absent");
    // (un dossier du même nom ne compte pas)
    std::fs::create_dir(e.0.join("Mindset.exe")).unwrap();
    let mut appele = false;
    let r = relayer(&e.0, &[os("-Demarrage")], |_, _, _| {
        appele = true;
        Ok(())
    });
    assert!(!appele, "rien ne doit être lancé");
    let p = r.expect_err("Mindset.exe manque");
    assert!(matches!(p, Probleme::Absent));
    assert_eq!(
        p.texte(),
        "Le programme « Mindset.exe » est introuvable à côté de celui-ci.\n\nRéinstalle Nevermind depuis Mindset."
    );
}

#[test]
fn mindset_exe_de_son_dossier_avec_les_memes_arguments() {
    let e = Essai::new("present");
    // un faux Mindset.exe vide : il n'est jamais lancé, seulement regardé
    std::fs::write(e.0.join("Mindset.exe"), b"").unwrap();
    let mut vu = None;
    let r = relayer(&e.0, &[os("-Demarrage"), os("a b")], |exe, ligne, dossier| {
        vu = Some((exe.to_path_buf(), ligne.to_os_string(), dossier.to_path_buf()));
        Ok(())
    });
    assert!(r.is_ok());
    let (exe, ligne, dossier) = vu.expect("lancement demandé");
    assert_eq!(exe, e.0.join("Mindset.exe"));
    assert_eq!(ligne, os(r#"-Demarrage "a b""#));
    assert_eq!(dossier, e.0);
    // Un échec de Windows : le message de relais.cs, avec la raison
    let r = relayer(&e.0, &[], |_, _, _| Err(std::io::Error::other("accès refusé")));
    assert_eq!(r.expect_err("échec").texte(), "Nevermind n'a pas pu s'ouvrir :\n\naccès refusé");
}
