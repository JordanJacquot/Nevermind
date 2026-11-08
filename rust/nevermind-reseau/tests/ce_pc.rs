//! Audit M10 (point 1) : l'audit du réseau disait toujours « Aucun dossier partagé », et jamais « Wi-Fi ouvert réglé en
//! réseau privé » : les partages et la catégorie du réseau étaient vides en dur. La version PowerShell les lit
//! (audit-reseau.ps1:132-141) : `Get-NetConnectionProfile` (NetworkCategory : Public, Private, DomainAuthenticated) et
//! `Get-SmbShare` (sans les partages spéciaux ni ceux qui finissent par « $ »), avec `Get-SmbShareAccess` : les droits
//! « Allow » de « Tout le monde », séparés par des virgules (« Nom|Chemin|Droits »).
//!
//! En Rust, la sonde lit les mêmes choses sans droits : le registre des partages (LanmanServer\Shares et leur
//! descripteur de sécurité) et le gestionnaire des réseaux de Windows. Ici, des lectures inventées : rien du vrai PC.

use nevermind_reseau::audit;
use nevermind_reseau::ce_pc::{LecturePc, PartageLu, bloc_pc};
use serde_json::{Value, json};

const TOUT_LE_MONDE: [u8; 12] = [1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0];
const ADMINISTRATEURS: [u8; 16] = [1, 2, 0, 0, 0, 0, 0, 5, 32, 0, 0, 0, 0x20, 2, 0, 0];
const UTILISATEURS: [u8; 16] = [1, 2, 0, 0, 0, 0, 0, 5, 32, 0, 0, 0, 0x21, 2, 0, 0];
const LECTURE: u32 = 0x0012_00A9;
const MODIFICATION: u32 = 0x0013_01BF;
const CONTROLE_TOTAL: u32 = 0x001F_01FF;

/// Un descripteur de sécurité « auto-relatif » (comme dans le registre) : ses entrées (0 autoriser, 1 refuser)
fn descripteur(entrees: &[(u8, u32, &[u8])]) -> Vec<u8> {
    let mut acl = Vec::new();
    for (sorte, masque, sid) in entrees {
        let taille = (8 + sid.len()) as u16;
        acl.extend([*sorte, 0]);
        acl.extend(taille.to_le_bytes());
        acl.extend(masque.to_le_bytes());
        acl.extend(*sid);
    }
    let mut d = vec![1, 0];
    d.extend(0x8004u16.to_le_bytes()); // (auto-relatif, liste d'accès présente)
    d.extend([0u8; 12]); // (ni propriétaire, ni groupe, ni audit)
    d.extend(20u32.to_le_bytes());
    d.extend([2, 0]);
    d.extend(((8 + acl.len()) as u16).to_le_bytes());
    d.extend((entrees.len() as u16).to_le_bytes());
    d.extend([0, 0]);
    d.extend(acl);
    d
}

fn hexa(t: &str) -> Vec<u8> {
    (0..t.len()).step_by(2).map(|i| u8::from_str_radix(&t[i..i + 2], 16).unwrap()).collect()
}

fn partage(nom: &str, chemin: &str, securite: Option<Vec<u8>>) -> PartageLu {
    PartageLu {
        nom: nom.into(),
        lignes: vec!["CSCFlags=0".into(), "MaxUses=4294967295".into(), format!("Path={chemin}"), "Type=0".into()],
        securite,
    }
}

#[test]
fn partages_et_categorie_comme_la_version_powershell() {
    // (un vrai descripteur, celui de « print$ » d'un PC de la suite : Tout le monde en lecture, Administrateurs en
    //  contrôle total)
    let vrai = hexa(
        "0100048048000000580000000000000014000000020034000200000000001400A900120001010000000000010000000000001800FF011F00010200000000000520000000200200000102000000000005200000002002000001020000000000052000000020020000",
    );
    let lu = LecturePc {
        categorie: Some(1),
        partages: vec![
            partage("print$", r"C:\WINDOWS\system32\spool\drivers", Some(vrai.clone())),
            partage("Musique", r"D:\Musique", Some(vrai)),
            partage(
                "Photos",
                r"C:\Users\Léa\Photos",
                Some(descripteur(&[(0, CONTROLE_TOTAL, &ADMINISTRATEURS), (0, LECTURE, &UTILISATEURS)])),
            ),
            partage(
                "Jeux",
                r"E:\Jeux",
                Some(descripteur(&[
                    (1, MODIFICATION, &TOUT_LE_MONDE),
                    (0, LECTURE, &TOUT_LE_MONDE),
                    (0, MODIFICATION, &TOUT_LE_MONDE),
                ])),
            ),
            partage("Films", r"E:\Films", Some(descripteur(&[(0, CONTROLE_TOTAL, &TOUT_LE_MONDE)]))),
            partage("Bizarre", r"E:\Bizarre", Some(descripteur(&[(0, 0x0012_0089, &TOUT_LE_MONDE)]))),
            // (aucun descripteur écrit : Windows partage alors en lecture pour tout le monde)
            partage("Ancien", r"E:\Ancien", None),
            // (une liste d'accès vide « nulle » : tout le monde a tous les droits)
            partage("Ouvert", r"E:\Ouvert", Some(vec![1, 0, 0x04, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])),
            // (un descripteur abîmé : aucun droit lu, le partage reste montré)
            partage("Abime", r"E:\Abime", Some(vec![1, 0, 0x04, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 200, 0, 0, 0])),
            PartageLu { nom: "SansChemin".into(), lignes: vec!["Type=0".into()], securite: None },
        ],
        ..Default::default()
    };
    let pc = bloc_pc(&lu);
    assert_eq!(pc["Category"], json!("Private"));
    assert_eq!(
        pc["Shares"],
        json!([
            r"Musique|D:\Musique|Read",
            r"Photos|C:\Users\Léa\Photos|",
            r"Jeux|E:\Jeux|Read,Change",
            r"Films|E:\Films|Full",
            r"Bizarre|E:\Bizarre|Custom",
            r"Ancien|E:\Ancien|Read",
            r"Ouvert|E:\Ouvert|Full",
            r"Abime|E:\Abime|",
            "SansChemin||Read"
        ])
    );
    for (n, attendu) in [(Some(0), "Public"), (Some(1), "Private"), (Some(2), "DomainAuthenticated"), (None, ""), (Some(7), "")]
    {
        assert_eq!(bloc_pc(&LecturePc { categorie: n, ..Default::default() })["Category"], json!(attendu), "{n:?}");
    }
}

fn titres(r: &Value) -> Vec<String> {
    audit::points(r, &[], "192.168.1.1", &json!(7)).into_iter().map(|p| p.titre).collect()
}

/// Ce que l'audit en dit (Get-NetAuditChecks, déjà jugé contre la version PowerShell) : plus de fausse assurance
#[test]
fn l_audit_voit_les_partages_et_le_wifi_ouvert_en_prive() {
    let lu = LecturePc {
        categorie: Some(1),
        partages: vec![partage("Musique", r"D:\Musique", Some(descripteur(&[(0, LECTURE, &TOUT_LE_MONDE)])))],
        ..Default::default()
    };
    let r = json!({ "Wifi": { "Ssid": "Gare", "Auth": "Ouvrir", "Cipher": "Aucun" }, "Pc": bloc_pc(&lu) });
    let t = titres(&r);
    assert!(!t.iter().any(|x| x == "Aucun dossier partagé"), "fausse assurance : {t:?}");
    assert!(t.iter().any(|x| x == "1 dossier partagé avec tout le monde"), "{t:?}");
    assert!(t.iter().any(|x| x == "Wi-Fi ouvert réglé en réseau privé"), "{t:?}");
    // (et sans partage, réseau public : le point vert, le mode public)
    let r = json!({ "Pc": bloc_pc(&LecturePc { categorie: Some(0), ..Default::default() }) });
    let t = titres(&r);
    assert!(t.iter().any(|x| x == "Aucun dossier partagé") && t.iter().any(|x| x == "Réseau en mode public"), "{t:?}");
}
