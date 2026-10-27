//! La fiche de version de NevermindPont.dll (Propriétés > Détails dans Windows) : produit « Nevermind », version celle
//! de Nevermind.ps1 ($AppVersion, sur quatre nombres), comme NevermindAide.exe (même build.rs). Le job
//! « Construire » de release.yml la vérifie.
//!
//! Sans aucune dépendance : la ressource de version est écrite ici en binaire (fichier .res), puis donnée telle quelle
//! à l'éditeur de liens de Microsoft (link.exe accepte les .res).

use std::path::PathBuf;

/// Un texte en UTF-16, terminé par un zéro
fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

fn aligner(b: &mut Vec<u8>) {
    while !b.len().is_multiple_of(4) {
        b.push(0);
    }
}

/// Un bloc de VS_VERSIONINFO : longueur, longueur de la valeur, sorte (1 : texte), clé, valeur, enfants
fn bloc(cle: &str, valeur: &[u8], longueur_valeur: u16, texte: bool, enfants: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![0, 0];
    b.extend(longueur_valeur.to_le_bytes());
    b.extend(u16::from(texte).to_le_bytes());
    b.extend(utf16(cle));
    aligner(&mut b);
    b.extend(valeur);
    for e in enfants {
        aligner(&mut b);
        b.extend(e);
    }
    let n = b.len() as u16;
    b[..2].copy_from_slice(&n.to_le_bytes());
    b
}

/// Une valeur texte de la fiche (« ProductName » = « Nevermind »)
fn texte(cle: &str, valeur: &str) -> Vec<u8> {
    let v = utf16(valeur);
    bloc(cle, &v, (v.len() / 2) as u16, true, &[])
}

/// Une ressource dans un fichier .res : en-tête (taille, sorte, numéro, langue), puis ses données
fn ressource(sorte: u16, numero: u16, langue: u16, donnees: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend((donnees.len() as u32).to_le_bytes());
    b.extend(32u32.to_le_bytes());
    b.extend([0xFF, 0xFF]);
    b.extend(sorte.to_le_bytes());
    b.extend([0xFF, 0xFF]);
    b.extend(numero.to_le_bytes());
    b.extend(0u32.to_le_bytes()); // DataVersion
    b.extend(if sorte == 0 { 0u16 } else { 0x0030u16 }.to_le_bytes()); // MOVEABLE | PURE
    b.extend(langue.to_le_bytes());
    b.extend(0u32.to_le_bytes()); // Version
    b.extend(0u32.to_le_bytes()); // Characteristics
    b.extend(donnees);
    aligner(&mut b);
    b
}

fn main() {
    let manifeste = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let ps1 = manifeste.join("..").join("..").join("Nevermind").join("fichiers").join("Nevermind.ps1");
    println!("cargo:rerun-if-changed={}", ps1.display());
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    // $AppVersion = '1.14.3' (première ligne qui commence ainsi), sur quatre nombres : 1.14.3.0
    let contenu = std::fs::read_to_string(&ps1).expect("Nevermind.ps1 introuvable");
    let version = contenu
        .lines()
        .find_map(|l| l.strip_prefix("$AppVersion = '").and_then(|r| r.split('\'').next()))
        .expect("$AppVersion introuvable dans Nevermind.ps1");
    let mut n: Vec<u16> = version.split('.').map(|x| x.parse().expect("version non valable")).collect();
    assert!((2..=4).contains(&n.len()), "version non valable : {version}");
    n.resize(4, 0);
    let v4 = n.iter().map(u16::to_string).collect::<Vec<_>>().join(".");
    let ms = (u32::from(n[0]) << 16) | u32::from(n[1]);
    let ls = (u32::from(n[2]) << 16) | u32::from(n[3]);

    // VS_FIXEDFILEINFO (le 2 : une DLL)
    let mut fixe = Vec::new();
    for x in [0xFEEF_04BDu32, 0x0001_0000, ms, ls, ms, ls, 0x3F, 0, 0x0004_0004, 2, 0, 0, 0] {
        fixe.extend(x.to_le_bytes());
    }
    // Français (France), Unicode
    let (langue, page) = (0x040Cu16, 1200u16);
    let table = bloc(
        &format!("{langue:04X}{page:04X}"),
        &[],
        0,
        true,
        &[
            texte("CompanyName", "Nevermind"),
            texte("FileDescription", "Nevermind"),
            texte("FileVersion", &v4),
            texte("InternalName", "NevermindPont"),
            texte("OriginalFilename", "NevermindPont.dll"),
            texte("ProductName", "Nevermind"),
            texte("ProductVersion", &v4),
        ],
    );
    let chaines = bloc("StringFileInfo", &[], 0, true, &[table]);
    let mut traduction = Vec::new();
    traduction.extend(langue.to_le_bytes());
    traduction.extend(page.to_le_bytes());
    let variables = bloc("VarFileInfo", &[], 0, true, &[bloc("Translation", &traduction, 4, false, &[])]);
    let fiche = bloc("VS_VERSION_INFO", &fixe, fixe.len() as u16, false, &[chaines, variables]);

    let mut res = ressource(0, 0, 0, &[]);
    res.extend(ressource(16, 1, langue, &fiche)); // RT_VERSION, numéro 1
    let sortie = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("version.res");
    std::fs::write(&sortie, res).unwrap();
    println!("cargo:rustc-link-arg-cdylib={}", sortie.display());
}
