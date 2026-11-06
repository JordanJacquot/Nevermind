//! Le seul module qui parle à Windows (et le seul où le code « unsafe » est permis) : la lecture de la protection
//! (`$SecDataWork`, sans droits administrateur), les analyses de Defender et les gestes des points à vérifier.
//! Tout programme lancé l'est sans fenêtre de console (CREATE_NO_WINDOW).

#![allow(unsafe_code)]

mod acl;
mod com;
mod lecture;
mod signature;
mod taches;

use crate::ps::Date;
use crate::regles::Detections;
use crate::systeme::{Lecture, Systeme, TypeAnalyse};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use windows::Win32::System::SystemInformation::GetLocalTime;

/// CREATE_NO_WINDOW : aucun programme lancé n'ouvre de fenêtre de console
const SANS_FENETRE: u32 = 0x0800_0000;
pub(crate) const DEFENDER: &str = "root\\Microsoft\\Windows\\Defender";

/// Le message d'une erreur de Windows, en clair
pub(crate) fn message(e: &windows::core::Error) -> String {
    let m = e.message();
    let m = m.trim();
    if m.is_empty() { format!("erreur 0x{:08x}", e.code().0) } else { m.to_string() }
}

/// Le vrai système
pub struct Vrai;

fn variable(nom: &str) -> Option<String> {
    std::env::var(nom).ok().filter(|v| !v.is_empty())
}

/// MpCmdRun.exe de Microsoft Defender
fn mpcmdrun() -> String {
    format!(
        "{}\\Windows Defender\\MpCmdRun.exe",
        variable("ProgramFiles").unwrap_or_else(|| "C:\\Program Files".into())
    )
}

/// Lance MpCmdRun (sans fenêtre) et attend sa fin ; `arret` levé : il est arrêté (notre propre processus seulement)
fn lancer_mpcmdrun(args: &[&str], arret: Option<&AtomicBool>) -> Result<(), String> {
    let mut c = Command::new(mpcmdrun())
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(SANS_FENETRE)
        .spawn()
        .map_err(|e| format!("Microsoft Defender ne répond pas ({e})"))?;
    let sortie = c.stdout.take();
    let lu = std::thread::spawn(move || {
        let mut t = String::new();
        if let Some(mut s) = sortie {
            use std::io::Read;
            let mut b = Vec::new();
            let _ = s.read_to_end(&mut b);
            t = String::from_utf8_lossy(&b).into_owned();
        }
        t
    });
    let code = loop {
        if let Some(s) = c.try_wait().map_err(|e| e.to_string())? {
            break s.code().unwrap_or(-1);
        }
        if arret.is_some_and(|a| a.load(Ordering::SeqCst)) {
            let _ = c.kill();
            let _ = c.wait();
            // (comme le bouton Arrêter de la version PowerShell : MpCmdRun -Cancel)
            let _ = Command::new(mpcmdrun())
                .arg("-Cancel")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(SANS_FENETRE)
                .status();
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(300));
    };
    let texte = lu.join().unwrap_or_default();
    // (0 : rien trouvé ; 2 : des menaces trouvées, l'analyse a bien eu lieu)
    if code == 0 || code == 2 {
        return Ok(());
    }
    let derniere = texte.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("").to_string();
    Err(if derniere.is_empty() { format!("Microsoft Defender a répondu {code}") } else { derniere })
}

impl Systeme for Vrai {
    fn lire(&self) -> Lecture {
        lecture::lire()
    }

    fn est_admin(&self) -> bool {
        // SAFETY : simple question à Windows
        unsafe { windows::Win32::UI::Shell::IsUserAnAdmin().as_bool() }
    }

    fn variable(&self, nom: &str) -> Option<String> {
        variable(nom)
    }

    fn existe(&self, chemin: &str) -> bool {
        std::path::Path::new(chemin).exists()
    }

    fn script_protege(&self, chemin: &str) -> bool {
        std::path::Path::new(chemin).exists() && acl::protege(chemin)
    }

    fn temp(&self) -> String {
        variable("TEMP").unwrap_or_default()
    }

    fn public(&self) -> String {
        format!("{}\\Users\\Public", variable("SystemDrive").unwrap_or_default())
    }

    fn hosts(&self) -> String {
        format!("{}\\System32\\drivers\\etc\\hosts", variable("windir").unwrap_or_else(|| "C:\\WINDOWS".into()))
    }

    fn maintenant(&self) -> Date {
        // SAFETY : simple lecture de l'horloge
        let t = unsafe { GetLocalTime() };
        Date {
            annee: i64::from(t.wYear),
            mois: u32::from(t.wMonth),
            jour: u32::from(t.wDay),
            heure: u32::from(t.wHour),
            minute: u32::from(t.wMinute),
            seconde: u32::from(t.wSecond),
        }
    }

    fn analyser(&self, sorte: TypeAnalyse, chemins: &[String], arret: &AtomicBool) -> Result<(), String> {
        match sorte {
            TypeAnalyse::Rapide => lancer_mpcmdrun(&["-Scan", "-ScanType", "1"], Some(arret)),
            TypeAnalyse::Complete => lancer_mpcmdrun(&["-Scan", "-ScanType", "2"], Some(arret)),
            TypeAnalyse::Personnalisee => {
                for p in chemins {
                    if arret.load(Ordering::SeqCst) {
                        break;
                    }
                    if std::path::Path::new(p).exists() {
                        lancer_mpcmdrun(&["-Scan", "-ScanType", "3", "-File", p], Some(arret))?;
                    }
                }
                Ok(())
            }
        }
    }

    fn mettre_a_jour_base(&self) -> Result<(), String> {
        lancer_mpcmdrun(&["-SignatureUpdate"], None)
    }

    fn supprimer_menaces(&self) -> Result<(), String> {
        let _c = com::Com::pret();
        com::methode(DEFENDER, "MSFT_MpThreat", "Remove")
    }

    fn desactiver_tache(&self, nom: &str, chemin: &str) -> Result<bool, String> {
        let _c = com::Com::pret();
        taches::desactiver(nom, chemin)
    }

    fn detections(&self) -> Detections {
        let _c = com::Com::pret();
        lecture::detections_locales()
    }

    fn bloc_notes(&self, chemin: &str) {
        let _ = Command::new("notepad.exe").arg(chemin).creation_flags(SANS_FENETRE).spawn();
    }

    fn montrer(&self, chemin: &str) {
        let dossier = variable("windir").unwrap_or_else(|| "C:\\WINDOWS".into());
        let _ = Command::new(format!("{dossier}\\explorer.exe"))
            .raw_arg(format!("/select,\"{chemin}\""))
            .creation_flags(SANS_FENETRE)
            .spawn();
    }
}
