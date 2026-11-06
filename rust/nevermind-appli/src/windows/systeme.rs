//! Le trait `Systeme` de la vie de l'appli rempli avec Windows : les verrous ([`super::synchro`]), le dossier de
//! Mindset dans HKLM ([`super::registre`]), le lancement d'un programme, l'Explorateur.
//!
//! Construction d'essai (option cargo `essai`) : HKLM n'est jamais lu (Mindset est « introuvable »), rien n'est
//! ouvert par l'Explorateur, aucun signal n'est levé, et seul ce programme-ci peut être lancé (une relance). Une
//! copie d'essai ne peut donc jamais ouvrir le vrai Mindset du PC.

use std::io;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use windows::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};

use super::synchro::{self, Verrou};
use crate::vie::{PriseVerrou, Systeme};

/// CREATE_NO_WINDOW : aucun programme lancé n'ouvre de console.
const SANS_CONSOLE: u32 = 0x0800_0000;

#[derive(Debug, Default)]
pub struct SystemeWindows {
    verrous: Vec<Verrou>,
}

impl SystemeWindows {
    pub fn nouveau() -> SystemeWindows {
        SystemeWindows::default()
    }
}

/// Construction d'essai : `exe` est-il ce programme-ci ?
#[cfg(feature = "essai")]
fn est_ce_programme(exe: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).ok();
    let moi = std::env::current_exe().ok().and_then(|e| canon(&e));
    moi.is_some() && canon(exe) == moi
}

impl Systeme for SystemeWindows {
    fn verrou_existe(&mut self, nom: &str) -> bool {
        synchro::existe(nom)
    }

    fn prendre_verrou(&mut self, nom: &str, attente: Duration) -> PriseVerrou {
        let (r, v) = synchro::prendre(nom, attente);
        if let Some(v) = v {
            self.verrous.push(v);
        }
        r
    }

    fn liberer_verrous(&mut self) {
        self.verrous.clear();
    }

    #[cfg(not(feature = "essai"))]
    fn emplacement_mindset(&mut self) -> Option<String> {
        super::registre::lire_hklm(super::registre::CLE_MINDSET, super::registre::VALEUR_EMPLACEMENT)
    }

    #[cfg(feature = "essai")]
    fn emplacement_mindset(&mut self) -> Option<String> {
        None
    }

    fn lancer(&mut self, exe: &Path, arguments: &[&str], dossier: Option<&Path>) -> io::Result<()> {
        #[cfg(feature = "essai")]
        if !est_ce_programme(exe) {
            return Err(io::Error::other("construction d'essai : seul ce programme peut être relancé"));
        }
        let mut c = Command::new(exe);
        c.args(arguments).creation_flags(SANS_CONSOLE);
        if let Some(d) = dossier {
            c.current_dir(d);
        }
        c.spawn().map(|_| ())
    }

    #[cfg(not(feature = "essai"))]
    fn ouvrir_par_explorateur(&mut self, cible: &str) -> io::Result<()> {
        // (assistance.ps1:222-225, Nevermind.ps1:68 : l'Explorateur ouvre le programme ou l'adresse, sans les droits
        // de ce processus)
        let windir = std::env::var("windir").unwrap_or_else(|_| r"C:\Windows".into());
        Command::new(Path::new(&windir).join("explorer.exe"))
            .arg(cible)
            .creation_flags(SANS_CONSOLE)
            .spawn()
            .map(|_| ())
    }

    #[cfg(feature = "essai")]
    fn ouvrir_par_explorateur(&mut self, _cible: &str) -> io::Result<()> {
        Err(io::Error::other("construction d'essai : rien n'est ouvert"))
    }

    fn permettre_premier_plan(&mut self) {
        // SAFETY : un simple réglage, sans mémoire
        unsafe {
            let _ = AllowSetForegroundWindow(ASFW_ANY);
        }
    }

    #[cfg(not(feature = "essai"))]
    fn lever_signal(&mut self, nom: &str) {
        let _ = synchro::lever(nom);
    }

    #[cfg(feature = "essai")]
    fn lever_signal(&mut self, _nom: &str) {}

    fn dormir(&mut self, duree: Duration) {
        std::thread::sleep(duree);
    }

    #[cfg(not(feature = "essai"))]
    fn demander_mindset(&mut self, mindset_installe: bool) -> bool {
        let texte = "Nevermind s'utilise maintenant avec Mindset\n\nInstalle Mindset, le portail de la suite : il reprend \
                     cette copie avec tes réglages et tes données, rien n'est perdu.";
        let bouton =
            if mindset_installe { "Ouvrir Mindset maintenant ?" } else { "Télécharger Mindset maintenant ?" };
        super::message::demander(&format!("{texte}\n\n{bouton}"), crate::NOM_APPLI)
    }

    /// Construction d'essai : aucune boîte (le script d'essai le voit au code de sortie 3).
    #[cfg(feature = "essai")]
    fn demander_mindset(&mut self, _mindset_installe: bool) -> bool {
        std::process::exit(3)
    }
}
