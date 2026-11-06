//! La question de la désinstallation (`Show-ChoixDesinstallation`, systeme.ps1:609-684) et le message de fin
//! (`Show-MessageFinDesinstallation`), avec les fenêtres de Windows : les mêmes textes, la même case (décochée au
//! départ), la même note sous la case qui change quand on la coche, les mêmes boutons (« Désinstaller », « Annuler »,
//! Annuler choisi par défaut : Entrée par erreur = Annuler). La version PowerShell dessinait sa fenêtre en WPF ; ici
//! c'est la fenêtre de question de Windows (TaskDialog) : voir DIVERGENCES.md.

use crate::choix::Choix;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    TASKDIALOG_BUTTON, TASKDIALOG_NOTIFICATIONS, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TASKDIALOGCONFIG_1,
    TD_INFORMATION_ICON, TD_WARNING_ICON, TDE_FOOTER, TDF_ALLOW_DIALOG_CANCELLATION, TDIE_ICON_FOOTER,
    TDM_SET_ELEMENT_TEXT, TDM_UPDATE_ICON, TDN_VERIFICATION_CLICKED, TaskDialogIndirect,
};
use windows::Win32::UI::WindowsAndMessaging::{
    IDOK, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_OKCANCEL, MB_SETFOREGROUND, MESSAGEBOX_STYLE, MessageBoxW,
    SendMessageW,
};
use windows::core::{BOOL, HRESULT, HSTRING, PCWSTR};

const TITRE: &str = "Désinstaller Nevermind";
const CASE: &str = "Supprimer aussi mes données (préférences, jeux, mesures, journal)";
const NOTE_GARDER: &str =
    "Tes données sont gardées : si tu réinstalles Nevermind, tu retrouveras tes préférences, tes jeux et tes mesures.";
const NOTE_SUPPRIMER: &str = "Attention : tes données seront effacées pour de bon.";
const ID_OUI: i32 = 100;
const ID_NON: i32 = 101;

/// Les deux notes, gardées vivantes pendant la fenêtre (la fenêtre lit leur texte quand la case change)
struct Notes {
    garder: HSTRING,
    supprimer: HSTRING,
}

/// La case change : la note dessous aussi (texte et icône)
unsafe extern "system" fn rappel(
    hwnd: HWND,
    msg: TASKDIALOG_NOTIFICATIONS,
    wparam: WPARAM,
    _lparam: LPARAM,
    donnees: isize,
) -> HRESULT {
    if msg == TDN_VERIFICATION_CLICKED && donnees != 0 {
        // SAFETY : `donnees` est l'adresse des notes, vivantes pendant tout TaskDialogIndirect
        let notes = unsafe { &*(donnees as *const Notes) };
        let cochee = wparam.0 != 0;
        let (texte, icone) =
            if cochee { (&notes.supprimer, TD_WARNING_ICON) } else { (&notes.garder, TD_INFORMATION_ICON) };
        // SAFETY : messages de la fenêtre de question, textes vivants
        unsafe {
            SendMessageW(
                hwnd,
                TDM_SET_ELEMENT_TEXT.0 as u32,
                Some(WPARAM(TDE_FOOTER.0 as usize)),
                Some(LPARAM(texte.as_ptr() as isize)),
            );
            SendMessageW(
                hwnd,
                TDM_UPDATE_ICON.0 as u32,
                Some(WPARAM(TDIE_ICON_FOOTER.0 as usize)),
                Some(LPARAM(icone.0 as isize)),
            );
        }
    }
    HRESULT(0)
}

/// La question : « garder », « supprimer », ou `None` si l'utilisateur annule (ou ferme la fenêtre)
pub fn question(texte: &str) -> Option<Choix> {
    let notes = Notes { garder: HSTRING::from(NOTE_GARDER), supprimer: HSTRING::from(NOTE_SUPPRIMER) };
    let titre = HSTRING::from(TITRE);
    let principal = HSTRING::from("Désinstaller Nevermind ?");
    let contenu = HSTRING::from(texte);
    let case = HSTRING::from(CASE);
    let oui = HSTRING::from("Désinstaller");
    let non = HSTRING::from("Annuler");
    let boutons = [
        TASKDIALOG_BUTTON { nButtonID: ID_OUI, pszButtonText: PCWSTR(oui.as_ptr()) },
        TASKDIALOG_BUTTON { nButtonID: ID_NON, pszButtonText: PCWSTR(non.as_ptr()) },
    ];
    // SAFETY : le module de ce programme (son icône, ressource numéro 1)
    let instance = unsafe { GetModuleHandleW(None) }.unwrap_or_default();
    let config = TASKDIALOGCONFIG {
        cbSize: size_of::<TASKDIALOGCONFIG>() as u32,
        hInstance: instance.into(),
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION,
        pszWindowTitle: PCWSTR(titre.as_ptr()),
        // (MAKEINTRESOURCE(1) : l'icône de ce programme)
        Anonymous1: TASKDIALOGCONFIG_0 { pszMainIcon: PCWSTR(std::ptr::without_provenance(1)) },
        pszMainInstruction: PCWSTR(principal.as_ptr()),
        pszContent: PCWSTR(contenu.as_ptr()),
        cButtons: boutons.len() as u32,
        pButtons: boutons.as_ptr(),
        nDefaultButton: ID_NON,
        pszVerificationText: PCWSTR(case.as_ptr()),
        Anonymous2: TASKDIALOGCONFIG_1 { pszFooterIcon: TD_INFORMATION_ICON },
        pszFooter: PCWSTR(notes.garder.as_ptr()),
        pfCallback: Some(rappel),
        lpCallbackData: &notes as *const Notes as isize,
        ..Default::default()
    };
    let mut bouton = 0i32;
    let mut cochee = BOOL(0);
    // SAFETY : configuration remplie, textes, boutons et notes vivants pendant tout l'appel
    let r = unsafe { TaskDialogIndirect(&config, Some(&mut bouton), None, Some(&mut cochee)) };
    if r.is_err() {
        // (sans les contrôles modernes de Windows : une question plus simple, les données sont alors gardées)
        let q = format!("{texte}\n\n{NOTE_GARDER}");
        return (boite(&q, TITRE, MB_OKCANCEL | MB_ICONWARNING) == IDOK.0).then_some(Choix::Garder);
    }
    match bouton {
        ID_OUI if cochee.as_bool() => Some(Choix::Supprimer),
        ID_OUI => Some(Choix::Garder),
        _ => None,
    }
}

/// Une boîte de message de Windows : le bouton choisi
fn boite(texte: &str, titre: &str, style: MESSAGEBOX_STYLE) -> i32 {
    // SAFETY : textes valables pendant l'appel, aucune fenêtre parente
    unsafe { MessageBoxW(None, &HSTRING::from(texte), &HSTRING::from(titre), style | MB_SETFOREGROUND).0 }
}

/// Le message de fin (titre « Nevermind », comme la version PowerShell)
pub fn message_fin(texte: &str) {
    boite(texte, "Nevermind", MB_OK | MB_ICONINFORMATION);
}

/// Un avertissement (titre « Nevermind ») : droits refusés, variable dangereuse
pub fn avertissement(texte: &str) {
    boite(texte, "Nevermind", MB_OK | MB_ICONWARNING);
}
