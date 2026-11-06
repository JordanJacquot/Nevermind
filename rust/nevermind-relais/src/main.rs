//! Nevermind.exe : un petit relais (portage de D:\Projets\Nevermind\outils\relais.cs). Le vrai programme de Nevermind
//! s'appelle Mindset.exe (nevermind-appli), pour que le Gestionnaire des tâches range toute la suite sur une seule
//! ligne « Mindset ». Nevermind.exe reste pour tout ce qui l'appelle encore (anciennes versions de Mindset,
//! raccourcis, apps.json ; contrat-appli, partie 4) : il lance Mindset.exe de son dossier avec les mêmes arguments,
//! puis se ferme aussitôt. Si Mindset.exe manque : un message en français, rien d'autre.

// Pas de console : le relais n'a aucune fenêtre à lui (seulement le message s'il y a un problème).
#![windows_subsystem = "windows"]

mod ligne;
mod message;

use std::os::windows::process::CommandExt;
use std::process::Command;

fn main() {
    let dossier = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf()));
    let Some(dossier) = dossier else {
        message::montrer(ligne::MESSAGE_ABSENT, false);
        std::process::exit(1);
    };
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let r = ligne::relayer(&dossier, &arguments, |exe, ligne, travail| {
        // fenêtre voulue : Mindset.exe est le programme à fenêtre de Nevermind, que l'utilisateur (ou Mindset) ouvre.
        // CREATE_NO_WINDOW (règle de la suite) : sans effet pour un programme graphique, aucune console possible.
        let mut c = Command::new(exe);
        c.current_dir(travail).creation_flags(0x0800_0000);
        if !ligne.is_empty() {
            c.raw_arg(ligne);
        }
        // (le relais n'attend pas : il se ferme aussitôt, Mindset.exe continue seul)
        c.spawn().map(|_| ())
    });
    if let Err(p) = r {
        message::montrer(&p.texte(), matches!(p, ligne::Probleme::Echec(_)));
        std::process::exit(1);
    }
}
