//! La fenêtre « principale » de Nevermind, créée dans le code (règle 7 de docs\regles-tauri.md, dépôt Mindset). Une
//! seule, jamais recréée.
//!
//! - cachée et sans prendre la main au départ : Mindset l'accroche, puis la montre (contrat-appli, partie 7) ;
//! - titre exact « Nevermind » (Mindset la retrouve à sa classe « Tauri Window » et à ce titre) ;
//! - dossier WebView2 `<données de la suite>\WebView2-Tauri\Nevermind` s'il s'écrit, sinon
//!   `<données de Nevermind>\ecran-webview`, et options fixes du navigateur : tout par suite-commun ;
//! - la taille de la fenêtre de Nevermind PowerShell (interface.xaml:3 : 1120 x 740, au moins 920 x 600) ;
//! - fond sombre dès l'allumage (vue.ps1:76 : pas d'éclair blanc) ;
//! - outils de développement, zoom, saisie automatique et téléchargements coupés ; navigation fermée aux pages de
//!   l'appli (vue.ps1:124), aucune nouvelle fenêtre (vue.ps1:125).

use std::path::Path;
use std::sync::Arc;

use suite_commun::webview::{self, DossierEcran};
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::window::Color;
use tauri::{Manager, Runtime, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Le nom (label) de la fenêtre : celui de capabilities\principale.json
pub const LABEL: &str = "principale";
/// La taille de la fenêtre (interface.xaml:3), en pixels logiques
pub const TAILLE: (f64, f64) = (1120.0, 740.0);
/// La taille minimale hors de Mindset (interface.xaml:3) ; 0 une fois accrochée
pub const TAILLE_MIN: (f64, f64) = (920.0, 600.0);
/// L'adresse de la page de l'appli (rangée dans l'exe, sous Windows)
pub const ADRESSE_PAGE: &str = "http://tauri.localhost/index.html";

/// Seules les pages de l'appli s'affichent : `http://tauri.localhost/...`, sans nom, mot de passe ni port.
pub fn adresse_permise(adresse: &Url) -> bool {
    adresse.scheme() == "http"
        && adresse.host_str() == Some("tauri.localhost")
        && adresse.port().is_none()
        && adresse.username().is_empty()
        && adresse.password().is_none()
}

/// Le journal de l'appli, vu de la fenêtre
pub type Journal = Arc<dyn Fn(&str) + Send + Sync>;

fn court(adresse: &Url) -> String {
    adresse.as_str().chars().take(120).collect()
}

/// Crée la fenêtre « principale », cachée. `donnees_suite` : le dossier des données de la suite ; `donnees_appli` :
/// celui de Nevermind. Rend aussi le dossier WebView2 choisi, pour le journal.
pub fn creer_principale<R: Runtime, M: Manager<R>>(
    app: &M,
    donnees_suite: &Path,
    donnees_appli: &Path,
    journal: Journal,
) -> tauri::Result<(WebviewWindow<R>, DossierEcran)> {
    let choix = webview::choisir_dossier(donnees_suite, crate::NOM_APPLI, donnees_appli);
    let pour_navigation = Arc::clone(&journal);
    // (le label écrit en clair, égal à LABEL : verifier-tauri.ps1 le lit ici pour le relier à sa capability)
    let fenetre = WebviewWindowBuilder::new(app, "principale", WebviewUrl::App("index.html".into()))
        .title(crate::NOM_APPLI)
        .inner_size(TAILLE.0, TAILLE.1)
        .min_inner_size(TAILLE_MIN.0, TAILLE_MIN.1)
        .center()
        .visible(false)
        .focused(false)
        .background_color(Color(6, 5, 15, 255))
        .devtools(false)
        .zoom_hotkeys_enabled(false)
        .general_autofill_enabled(false)
        .data_directory(choix.dossier.clone())
        .additional_browser_args(webview::OPTIONS_NAVIGATEUR)
        .on_navigation(move |a| {
            let permise = adresse_permise(a);
            if !permise {
                pour_navigation(&format!("Écran : navigation refusée ({})", court(a)));
            }
            permise
        })
        .on_page_load(move |_, p| {
            if p.event() == PageLoadEvent::Finished {
                journal(&format!("Écran : page chargée {}", court(p.url())));
            }
        })
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .build()?;
    Ok((fenetre, choix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seules_les_pages_de_l_appli_sont_permises() {
        let a = |s: &str| s.parse::<Url>().unwrap();
        for s in [ADRESSE_PAGE, "http://tauri.localhost/", "http://tauri.localhost/three/three.module.js"] {
            assert!(adresse_permise(&a(s)), "{s}");
        }
        for s in [
            "https://tauri.localhost/",
            "http://tauri.localhost:8080/",
            "http://pack.localhost/fond.png",
            "http://fichier.localhost/x.png",
            "http://tauri.localhost.exemple.com/",
            "https://github.com/JordanJacquot/Nevermind",
            "http://moi:secret@tauri.localhost/",
            "file:///C:/Windows/win.ini",
            "about:blank",
            "javascript:alert(1)",
        ] {
            assert!(!adresse_permise(&a(s)), "{s}");
        }
    }
}
