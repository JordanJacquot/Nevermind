//! Nevermind en Rust : l'appli. Base posée par l'intégration : la liste des blocs, dans l'ordre où accueil-console.ps1
//! appelle Get-PagesX et Invoke-ActionX (Pc, Perf, Protection, Panneau, Jeux, Reseau, Divers). L'agent « hôte » en
//! fait la fenêtre Tauri ; cette liste reste le point d'entrée des blocs.

use nevermind_noyau::Bloc;

/// Tous les blocs de Nevermind, dans l'ordre des pages et des actions.
pub fn blocs() -> Vec<Box<dyn Bloc>> {
    vec![
        Box::new(nevermind_pc::BlocPc::default()),
        Box::new(nevermind_perf::BlocPerf::default()),
        Box::new(nevermind_memoire::BlocMemoire::default()),
        Box::new(nevermind_protection::BlocProtection::default()),
        Box::new(nevermind_jeux::BlocJeux::default()),
        Box::new(nevermind_reseau::BlocReseau::default()),
        Box::new(nevermind_trafic::BlocTrafic::default()),
        Box::new(nevermind_divers::BlocDivers::default()),
    ]
}
