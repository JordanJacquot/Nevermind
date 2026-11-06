//! `(Get-AuthenticodeSignature $f).Status -eq 'Valid'` : la signature du fichier, ou celle de son catalogue de Windows
//! (les programmes de Windows sont signés par catalogue, sans signature dans le fichier).

use std::mem::size_of;
use std::os::windows::io::AsRawHandle;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Security::Cryptography::Catalog::{
    CATALOG_INFO, CryptCATAdminAcquireContext2, CryptCATAdminCalcHashFromFileHandle2, CryptCATAdminEnumCatalogFromHash,
    CryptCATAdminReleaseCatalogContext, CryptCATAdminReleaseContext, CryptCATCatalogInfoFromContext,
};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_CATALOG_INFO, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CHOICE_CATALOG, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    WinVerifyTrust,
};
use windows::core::{HSTRING, PCWSTR, w};

/// WinVerifyTrust sur des données prêtes ; 0 : signature valable
///
/// # Safety
/// `donnees` doit pointer vers des structures vivantes pendant l'appel.
unsafe fn verifier(donnees: &mut WINTRUST_DATA) -> i32 {
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY : structures de l'appelant ; l'état gardé par Windows est rendu (WTD_STATEACTION_CLOSE)
    unsafe {
        let code = WinVerifyTrust(HWND::default(), &mut action, (donnees as *mut WINTRUST_DATA).cast());
        donnees.dwStateAction = WTD_STATEACTION_CLOSE;
        let _ = WinVerifyTrust(HWND::default(), &mut action, (donnees as *mut WINTRUST_DATA).cast());
        code
    }
}

/// Signature dans le fichier
fn integree(chemin: &str) -> bool {
    let nom = HSTRING::from(chemin);
    let mut fichier = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(nom.as_ptr()),
        ..Default::default()
    };
    let mut d = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut fichier },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    // SAFETY : structures locales vivantes pendant l'appel
    unsafe { verifier(&mut d) == 0 }
}

/// Signature par un catalogue de Windows (empreinte SHA256, puis SHA1 pour les anciens catalogues)
fn par_catalogue(chemin: &str) -> bool {
    let Ok(f) = std::fs::File::open(chemin) else { return false };
    let h = HANDLE(f.as_raw_handle());
    let nom = HSTRING::from(chemin);
    for algo in [w!("SHA256"), PCWSTR::null()] {
        let mut admin = 0isize;
        // SAFETY : contexte de catalogue rendu à la fin ; tampons locaux
        unsafe {
            if CryptCATAdminAcquireContext2(&mut admin, None, algo, None, None).is_err() {
                continue;
            }
            let mut n = 0u32;
            let _ = CryptCATAdminCalcHashFromFileHandle2(admin, h, &mut n, None, None);
            let mut empreinte = vec![0u8; n as usize];
            let ok = n > 0
                && CryptCATAdminCalcHashFromFileHandle2(admin, h, &mut n, Some(empreinte.as_mut_ptr()), None).is_ok();
            let mut valable = false;
            if ok {
                empreinte.truncate(n as usize);
                let cat = CryptCATAdminEnumCatalogFromHash(admin, &empreinte, None, None);
                if cat != 0 {
                    let mut info = CATALOG_INFO { cbStruct: size_of::<CATALOG_INFO>() as u32, ..Default::default() };
                    if CryptCATCatalogInfoFromContext(cat, &mut info, 0).is_ok() {
                        let etiquette: String = empreinte.iter().map(|b| format!("{b:02X}")).collect();
                        let etiquette = HSTRING::from(etiquette);
                        let mut ci = WINTRUST_CATALOG_INFO {
                            cbStruct: size_of::<WINTRUST_CATALOG_INFO>() as u32,
                            pcwszCatalogFilePath: PCWSTR(info.wszCatalogFile.as_ptr()),
                            pcwszMemberTag: PCWSTR(etiquette.as_ptr()),
                            pcwszMemberFilePath: PCWSTR(nom.as_ptr()),
                            hMemberFile: h,
                            pbCalculatedFileHash: empreinte.as_mut_ptr(),
                            cbCalculatedFileHash: empreinte.len() as u32,
                            hCatAdmin: admin,
                            ..Default::default()
                        };
                        let mut d = WINTRUST_DATA {
                            cbStruct: size_of::<WINTRUST_DATA>() as u32,
                            dwUIChoice: WTD_UI_NONE,
                            fdwRevocationChecks: WTD_REVOKE_NONE,
                            dwUnionChoice: WTD_CHOICE_CATALOG,
                            Anonymous: WINTRUST_DATA_0 { pCatalog: &mut ci },
                            dwStateAction: WTD_STATEACTION_VERIFY,
                            ..Default::default()
                        };
                        valable = verifier(&mut d) == 0;
                    }
                    let _ = CryptCATAdminReleaseCatalogContext(admin, cat, 0);
                }
            }
            let _ = CryptCATAdminReleaseContext(admin, 0);
            if valable {
                return true;
            }
        }
    }
    false
}

/// Le fichier porte une signature valable (dans le fichier ou par catalogue)
pub fn valide(chemin: &str) -> bool {
    integree(chemin) || par_catalogue(chemin)
}
