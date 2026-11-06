//! L'identité de l'appli pour la barre des tâches : `JordanJacquot.Nevermind` (Nevermind.ps1:429-430), la même que le
//! raccourci que Mindset écrit.

use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use windows::core::HSTRING;

/// L'identité de Nevermind (`$AppUserModelId`).
pub const ID_APPLI: &str = "JordanJacquot.Nevermind";

/// Pose l'identité du processus (avant toute fenêtre).
pub fn poser() -> windows::core::Result<()> {
    let id = HSTRING::from(ID_APPLI);
    // SAFETY : texte valide pendant l'appel ; seulement un réglage de ce processus
    unsafe { SetCurrentProcessExplicitAppUserModelID(&id) }
}
