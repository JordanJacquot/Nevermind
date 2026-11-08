//! Nevermind en Rust : l'appli. Elle remplace l'hôte PowerShell et WPF (Nevermind.ps1, vue.ps1, accueil-console.ps1,
//! navigation.ps1, ecran-panneau.ps1, themes.ps1, et ce qui sert à la fenêtre dans evenements.ps1, interface.ps1 et
//! natif.cs) : une fenêtre Tauri qui montre le kit d'écrans de la suite (ui\), assemble les pages de tous les blocs,
//! leur passe les gestes, exécute leurs effets, et vit avec Mindset comme le demande son contrat.
//!
//! - [`blocs`] : la liste des blocs, dans l'ordre d'accueil-console.ps1 (le point d'entrée des blocs) ;
//! - [`hote`] : l'écran assemblé, les gestes, le panneau de détail, les tics des blocs (sans Tauri : testé à part) ;
//! - [`vie`] : le démarrage et les surveillances (instance unique, données, Mindset, thème de la suite, veille) ;
//! - [`theme`], [`pack`] : le thème, les images d'un pack, les protocoles `pack` et `fichier` ;
//! - [`commandes`], [`pont`] : les trois commandes de la page et la lecture de ses messages ;
//! - [`fenetre`] : la fenêtre « principale » ;
//! - `programme` : le programme Mindset.exe tout branché ; `windows` : le seul module où « unsafe » est permis ;
//! - `essai` et `png` : la construction d'essai seulement (option cargo `essai`, jamais livrée).

use nevermind_noyau::Bloc;

pub mod commandes;
#[cfg(feature = "essai")]
pub mod essai;
pub mod fenetre;
pub mod hote;
pub mod pack;
#[cfg(any(feature = "essai", test))]
pub mod png;
pub mod pont;
#[cfg(windows)]
pub mod programme;
pub mod theme;
pub mod vie;
#[cfg(windows)]
pub mod windows;

pub use commandes::Page;

/// Le nom de l'appli : titre de sa fenêtre (Mindset la cherche à ce titre), nom de son dossier WebView2.
pub const NOM_APPLI: &str = "Nevermind";
/// La version de l'appli (celle de son Cargo.toml : `$AppVersion` de Nevermind.ps1, la même que version.json).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Tous les blocs de Nevermind, dans l'ordre où accueil-console.ps1 appelle Get-PagesX et Invoke-ActionX (Pc, Perf,
/// Protection, Panneau, Jeux, Reseau, Divers). Cette liste reste le point d'entrée des blocs.
pub fn blocs() -> Vec<Box<dyn Bloc>> {
    vec![
        Box::new(nevermind_pc::BlocPc::default()),
        Box::new(nevermind_perf::BlocPerf::default()),
        Box::new(nevermind_memoire::BlocMemoire::default()),
        Box::new(nevermind_protection::BlocProtection::default()),
        Box::new(nevermind_jeux::BlocJeux::default()),
        Box::new(nevermind_reseau::BlocReseau::default()),
        Box::new(nevermind_trafic::BlocTrafic::default()),
        // (le mode streameur, dans Performances : ajouté le 08/11/2026, sans équivalent PowerShell)
        Box::new(nevermind_stream::BlocStream::default()),
        Box::new(nevermind_divers::BlocDivers::default()),
    ]
}

/// Ajoute à Tauri la page partagée et les commandes de l'appli (les seules que la page peut appeler, chacune avec sa
/// permission dans capabilities\principale.json).
pub fn preparer<R: tauri::Runtime>(constructeur: tauri::Builder<R>, page: Page) -> tauri::Builder<R> {
    constructeur.manage(page).invoke_handler(tauri::generate_handler![
        commandes::suivre_page,
        commandes::noter_visibilite,
        commandes::action
    ])
}

/// La configuration de Tauri lue à la construction (tauri.conf.json, capabilities\, les pages de ui\ rangées dans
/// l'exe, avec leurs empreintes ajoutées à la CSP).
pub fn contexte<R: tauri::Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}
