//! « Désinstaller Nevermind.exe » (programme cargo NevermindDesinstaller, renommé dans le zip) : remet Windows comme
//! avant Nevermind, garde ou supprime ses données, puis retire Nevermind de son dossier. Rend 0.
//!
//! - sans option : la question des données est posée (garder, coché pour supprimer, ou annuler) ;
//! - `-GarderDonnees` ou `-SupprimerDonnees` (le désinstalleur de Mindset les passe) : pas de question ; les deux à la
//!   fois : la question est posée ;
//! - lancé sans les droits administrateur, il se relance lui-même avec (une seule demande de Windows), avec la même
//!   option, et attend la fin pour rendre le même code (Nevermind.ps1:330-350).
//!
//! Il ne dépend pas de l'appli (il marche si Mindset.exe de Nevermind est abîmé ou absent : contrat-appli, partie 9).

// Pas de console : seulement la question et le message de fin.
#![windows_subsystem = "windows"]

#[cfg(windows)]
fn main() {
    std::process::exit(windows_main::lancer());
}

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
mod windows_main {
    use nevermind_desinstalleur::choix::choix_impose;
    use nevermind_desinstalleur::os::{self, PcDesinstallation, SystemeWindows, fenetre};
    use nevermind_desinstalleur::sur_place::AideSurPlace;
    use nevermind_desinstalleur::{donnees, travail};
    use nevermind_noyau::Contexte;
    use nevermind_pc::corrections::Sauvegarde;
    use nevermind_pc::systeme::PcWindows;
    use std::path::PathBuf;

    pub fn lancer() -> i32 {
        let arguments: Vec<String> = std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
        let impose = choix_impose(&arguments);

        // Seule la désinstallation demande les droits administrateur (une fois) : elle remet les réglages de Windows
        // comme avant (sauf les changements définitifs qui protègent le PC) et retire l'Aide.
        if !nevermind_noyau::windows::est_admin() {
            // (audit du 25/10, constat 15 : même règle et même liste que Get-VariablesDangereuses) Une variable qu'un
            // programme lancé avec les droits recevrait du compte, posée par un programme sans droits : pas de demande
            if let Some(d) = nevermind_noyau::aide::variables_dangereuses().first() {
                fenetre::avertissement(&format!(
                    "Par prudence, la désinstallation ne demande pas les droits administrateur : un réglage caché de ton compte Windows ({d}) pourrait faire lancer un autre programme avec ces droits.\n\nLance une analyse antivirus, puis désinstalle Nevermind depuis Mindset."
                ));
                return 1;
            }
            return match os::relancer_en_administrateur(impose.map(|c| c.option()).unwrap_or("")) {
                Ok(code) => code as i32,
                Err(_) => {
                    fenetre::avertissement(
                        "La désinstallation de Nevermind a besoin des droits administrateur pour remettre les réglages de Windows comme avant.\n\nRelance-la et clique sur « Oui » quand Windows le demande.",
                    );
                    1
                }
            };
        }

        // En administrateur : les DLL seulement de System32, le dossier courant System32, les dossiers de Windows remis
        // tels que Windows les donne, la sécurité de COM (le même code que l'Aide)
        let _processus = nevermind_desinstalleur::preparer_processus();
        let Some(racine) = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)) else {
            fenetre::avertissement("Le dossier de Nevermind est introuvable : rien n'a été changé.");
            return 1;
        };
        let local = os::localappdata();
        let suite = std::fs::read(PathBuf::from(&local).join("Mindset").join("suite.json")).ok().and_then(|o| {
            let t = String::from_utf8_lossy(&o).trim_start_matches('\u{feff}').to_string();
            serde_json::from_str::<serde_json::Value>(&t).ok()
        });
        let geree = donnees::geree(suite.as_ref(), &racine);
        let dossier = donnees::dossier_donnees(geree, &racine, &local);
        let ctx = Contexte::nouveau(racine.clone(), dossier.clone(), geree, false);
        ctx.definir_racine(racine.clone());
        // `Import-Backup` : sauvegarde.json, sa copie de secours, et les valeurs d'origine gardées dans le registre
        ctx.importer_sauvegarde();
        let s = ctx.sauvegarde();
        let probleme = s.probleme.clone();
        let sv = Sauvegarde::depuis(&s.en_json());

        let aide = AideSurPlace::nouvelle();
        let mut pc = PcDesinstallation { pc: PcWindows::nouveau(&ctx), aide: &aide };
        let mut sys = SystemeWindows { ctx: &ctx, aide: &aide, racine: racine.clone() };
        let lieux = travail::Lieux { racine, donnees: dossier, pid: std::process::id() };
        let _ = travail::desinstaller(&mut pc, &mut sys, sv, probleme.as_deref(), &lieux, impose);
        0
    }
}
