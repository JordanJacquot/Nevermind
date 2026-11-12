//! « Désinstaller Nevermind.exe » (programme cargo NevermindDesinstaller, renommé dans le zip) : remet Windows comme
//! avant Nevermind, garde ou supprime ses données, puis retire Nevermind de son dossier. Rend 0.
//!
//! - sans option : la question des données est posée (garder, coché pour supprimer, ou annuler) ;
//! - `-GarderDonnees` ou `-SupprimerDonnees` (le désinstalleur de Mindset les passe) : pas de question ; les deux à la
//!   fois : la question est posée ;
//! - lancé sans les droits administrateur, il se relance lui-même avec (une seule demande de Windows), avec la même
//!   option, et attend la fin pour rendre le même code (Nevermind.ps1:330-350).
//!
//! Lot C du second audit (M-A, M1) : le compte peut remplacer son dossier des données, son bureau ou son menu Démarrer
//! par une jonction à tout moment. Donc :
//! - la partie sans droits fait ce qui touche ces dossiers et n'a besoin d'aucun droit : avant la demande de Windows,
//!   le journal mis au propre (raccourci s'il dépasse 1 Mo) ; après, les raccourcis qui lancent cette copie ;
//! - la partie administrateur (relancée par la demande, ou lancée directement en administrateur par le désinstalleur de
//!   Mindset) ne lit ni n'écrit RIEN par un simple chemin dans ces dossiers : son contexte est « sans disque » ; elle lit
//!   la sauvegarde, les réglages et l'historique elle-même, dans le dossier ouvert et vérifié, chaque fichier ouvert par
//!   rapport à lui sans suivre de lien (`os::lire_donnees`) ; le journal reste en mémoire et n'est ajouté qu'à la fin,
//!   de la même façon (`os::ecrire_donnees`) ; jamais `raccourcir`, jamais une sauvegarde abîmée renommée.
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
    use nevermind_desinstalleur::choix::{Choix, choix_impose};
    use nevermind_desinstalleur::os::{self, PcDesinstallation, SystemeWindows, fenetre};
    use nevermind_desinstalleur::sur_place::AideSurPlace;
    use nevermind_desinstalleur::{donnees, travail};
    use nevermind_noyau::Contexte;
    use nevermind_pc::corrections::Sauvegarde;
    use nevermind_pc::systeme::PcWindows;
    use std::path::PathBuf;

    /// Où sont cette copie et ses données (le même calcul sans droits et en administrateur)
    struct Lieux {
        racine: PathBuf,
        geree: bool,
        donnees: PathBuf,
        suite: Option<PathBuf>,
    }

    fn lieux() -> Option<Lieux> {
        let racine = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from))?;
        let local = os::localappdata();
        let suite = std::fs::read(PathBuf::from(&local).join("Mindset").join("suite.json")).ok().and_then(|o| {
            let t = String::from_utf8_lossy(&o).trim_start_matches('\u{feff}').to_string();
            serde_json::from_str::<serde_json::Value>(&t).ok()
        });
        // (suite.json de l'utilisateur, ou le dossier où Mindset installe Nevermind : DIVERGENCES.md, point 14)
        let geree = donnees::geree(suite.as_ref(), &racine) || donnees::installee_par_mindset(&racine);
        let donnees = donnees::dossier_donnees(geree, &racine, &local);
        // (piloté par Mindset : les conseils de fin sont écrits dans le dossier des données de la suite)
        let suite = donnees::dossier_suite(geree, &racine, &local);
        Some(Lieux { racine, geree, donnees, suite })
    }

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
            return sans_droits(impose);
        }
        en_administrateur(impose)
    }

    /// La partie sans droits : ce qui touche les dossiers du compte, fait par le compte lui-même, autour de la demande
    /// de Windows
    fn sans_droits(impose: Option<Choix>) -> i32 {
        let ou = lieux();
        // Avant la demande : le dossier des données et son journal mis au propre ici (créé, raccourci s'il dépasse
        // 1 Mo, comme au démarrage de Nevermind) ; en administrateur, ce n'est jamais fait (lot C, M-A)
        if let Some(l) = &ou {
            let _ = Contexte::nouveau(l.racine.clone(), l.donnees.clone(), l.geree, false);
        }
        match os::relancer_en_administrateur(impose.map(|c| c.option()).unwrap_or("")) {
            Ok(code) => {
                // Après, si la copie est bien désinstallée (ses programmes sont partis) : les raccourcis du bureau et du
                // menu Démarrer qui la lancent, effacés par le compte (lot C, M1)
                if let Some(l) = &ou
                    && !l.racine.join("Mindset.exe").exists()
                    && !l.racine.join("Nevermind.exe").exists()
                {
                    os::retirer_raccourcis_du_compte(&l.racine);
                }
                code as i32
            }
            Err(_) => {
                fenetre::avertissement(
                    "La désinstallation de Nevermind a besoin des droits administrateur pour remettre les réglages de Windows comme avant.\n\nRelance-la et clique sur « Oui » quand Windows le demande.",
                );
                1
            }
        }
    }

    /// La partie administrateur : les réglages de Windows, l'Aide, les tâches, et les données sans jamais suivre de lien
    fn en_administrateur(impose: Option<Choix>) -> i32 {
        // Les DLL seulement de System32, le dossier courant System32, les dossiers de Windows remis tels que Windows les
        // donne, la sécurité de COM (le même code que l'Aide)
        let _processus = nevermind_desinstalleur::preparer_processus();
        let Some(l) = lieux() else {
            fenetre::avertissement("Le dossier de Nevermind est introuvable : rien n'a été changé.");
            return 1;
        };
        // (lot C, M-A : rien n'est lu ni écrit par chemin dans le dossier des données ; il n'est ni créé ni raccourci.
        //  `Import-Backup` (sauvegarde.json, sa copie de secours, les valeurs d'origine gardées dans le registre), les
        //  réglages (états d'origine des services et de l'alimentation) et l'historique : lus sans suivre de lien)
        let (ctx, reglages_lus) = os::contexte_administrateur(&l.racine, &l.donnees, l.geree);
        let s = ctx.sauvegarde();
        let probleme = s.probleme.clone();
        let sv = Sauvegarde::depuis(&s.en_json());

        let aide = AideSurPlace::nouvelle();
        let mut pc = PcDesinstallation { pc: PcWindows::nouveau(&ctx), aide: &aide, ctx: &ctx };
        let mut sys = SystemeWindows {
            ctx: &ctx,
            aide: &aide,
            racine: l.racine.clone(),
            muet: impose.is_some(),
            reglages_relus: None,
        };
        let lieux =
            travail::Lieux { racine: l.racine, donnees: l.donnees.clone(), suite: l.suite, pid: std::process::id() };
        // (« Tout restaurer » du bloc pc ne change le PC que sous le verrou des gestes, lot A : le désinstalleur, seul
        //  dans son processus, le prend pour toute la désinstallation)
        // (lot J, suite : et le permis de ce verrou, que demandent les tâches planifiées, la clé de l'Aide et la
        //  fermeture des programmes ; sur ce fil, il ne peut pas manquer une fois le geste pris)
        let Ok(_geste) = ctx.prendre_geste("Désinstaller", "Désinstallation") else {
            fenetre::avertissement("Nevermind n'a pas pu commencer la désinstallation : rien n'a été changé.");
            return 1;
        };
        let Ok(permis) = ctx.exiger_geste() else {
            fenetre::avertissement("Nevermind n'a pas pu commencer la désinstallation : rien n'a été changé.");
            return 1;
        };
        // (la sauvegarde, les réglages et l'historique lus ici servent à la question ; ils sont relus après la fermeture
        //  de l'Aide et de Nevermind, juste avant la remise en état : lot H, M2)
        let _ = travail::desinstaller(&mut pc, &mut sys, &permis, sv, probleme.as_deref(), &lieux, impose);
        // Le journal et les états d'origine changés, à la fin, sans suivre de lien (si les données sont gardées) ; la
        // référence : les réglages relus juste avant la remise en état
        let reference = sys.reglages_relus.take().unwrap_or(reglages_lus);
        os::ecrire_donnees(&l.donnees, &ctx, reference.as_ref());
        0
    }
}
