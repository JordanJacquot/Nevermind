//! Les réglages de WebView2 que Tauri ne transmet pas, et la veille de la page.
//!
//! - Réglages (règle 5 de docs\regles-tauri.md, dépôt Mindset ; console.ps1:102-103 de Mindset, vue.ps1:115-116 de
//!   Nevermind) : menu du clic droit, raccourcis du navigateur (F5, Ctrl+F, Ctrl+P...), saisie automatique et
//!   mots de passe gardés, coupés ; outils de développement et zoom (déjà coupés par le constructeur de la fenêtre)
//!   recoupés ici pour en être sûr. Chaque réglage est relu après coup et noté au journal. La page de l'appli est
//!   ensuite ouverte de nouveau (`Navigate`, pas `Reload` : posé pendant la première navigation de wry, un
//!   rechargement l'annulait et rechargeait `about:blank`, refusé par la navigation fermée ; vu à l'essai du
//!   02/11/2026) : un réglage qui ne vaudrait qu'à la navigation suivante vaut alors pour elle.
//! - Veille : WebView2 ne sait pas que Mindset a caché ou réduit la fenêtre qui le contient ; la vie de l'appli
//!   le regarde (la fenêtre et tous ses parents visibles et non réduits) et le lui dit (`SetIsVisible`). Cachée, la
//!   page passe à `document.visibilityState = "hidden"` et se met en pause.

use tauri::WebviewWindow;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2_3, ICoreWebView2Controller, ICoreWebView2Settings3, ICoreWebView2Settings4,
};
use webview2_com::TrySuspendCompletedHandler;
use windows::core::{BOOL, HSTRING, Interface};

/// Ce que WebView2 dit de ses réglages, relus après les avoir coupés.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reglages {
    pub menu_clic_droit: bool,
    pub raccourcis_navigateur: bool,
    pub saisie_automatique: bool,
    pub mots_de_passe: bool,
    pub outils_developpement: bool,
    pub zoom: bool,
}

impl Reglages {
    /// La ligne du journal : tout coupé, ou ce qui reste allumé.
    pub fn texte(&self) -> String {
        let allumes: Vec<&str> = [
            (self.menu_clic_droit, "menu du clic droit"),
            (self.raccourcis_navigateur, "raccourcis du navigateur"),
            (self.saisie_automatique, "saisie automatique"),
            (self.mots_de_passe, "mots de passe gardés"),
            (self.outils_developpement, "outils de développement"),
            (self.zoom, "zoom"),
        ]
        .into_iter()
        .filter_map(|(allume, nom)| allume.then_some(nom))
        .collect();
        if allumes.is_empty() {
            "Écran : menu du clic droit, raccourcis du navigateur, saisie automatique, mots de passe, outils de \
             développement et zoom coupés"
                .to_string()
        } else {
            format!("Écran : encore allumés malgré les réglages : {}", allumes.join(", "))
        }
    }
}

/// Coupe les réglages, les relit, puis ouvre de nouveau la page de l'appli.
fn couper(c: &ICoreWebView2Controller) -> windows::core::Result<Reglages> {
    // SAFETY : appels COM ordinaires sur le contrôleur de WebView2 que Tauri nous prête, sur le fil de la fenêtre
    // (with_webview) ; les BOOL relus sont des variables locales passées par pointeur valide pendant chaque appel.
    unsafe {
        let w = c.CoreWebView2()?;
        let s = w.Settings()?;
        s.SetAreDefaultContextMenusEnabled(false)?;
        s.SetAreDevToolsEnabled(false)?;
        s.SetIsZoomControlEnabled(false)?;
        s.SetIsStatusBarEnabled(false)?;
        let s3: ICoreWebView2Settings3 = s.cast()?;
        s3.SetAreBrowserAcceleratorKeysEnabled(false)?;
        let s4: ICoreWebView2Settings4 = s.cast()?;
        s4.SetIsGeneralAutofillEnabled(false)?;
        s4.SetIsPasswordAutosaveEnabled(false)?;
        let lire = |f: &dyn Fn(*mut BOOL) -> windows::core::Result<()>| -> windows::core::Result<bool> {
            let mut v = BOOL::default();
            f(&mut v)?;
            Ok(v.as_bool())
        };
        let r = Reglages {
            menu_clic_droit: lire(&|p| s.AreDefaultContextMenusEnabled(p))?,
            raccourcis_navigateur: lire(&|p| s3.AreBrowserAcceleratorKeysEnabled(p))?,
            saisie_automatique: lire(&|p| s4.IsGeneralAutofillEnabled(p))?,
            mots_de_passe: lire(&|p| s4.IsPasswordAutosaveEnabled(p))?,
            outils_developpement: lire(&|p| s.AreDevToolsEnabled(p))?,
            zoom: lire(&|p| s.IsZoomControlEnabled(p))?,
        };
        w.Navigate(&HSTRING::from(crate::fenetre::ADRESSE_PAGE))?;
        Ok(r)
    }
}

/// Coupe les réglages de la page de cette fenêtre (sur le fil de la fenêtre, un peu plus tard) ; le résultat va au
/// journal.
pub fn couper_reglages(f: &WebviewWindow, journal: impl Fn(&str) + Send + 'static) -> tauri::Result<()> {
    f.with_webview(move |pw| match couper(&pw.controller()) {
        Ok(r) => journal(&r.texte()),
        Err(e) => journal(&format!("Écran : réglages de WebView2 non posés ({e})")),
    })
}

/// La page est-elle visible (`SetIsVisible`) : cachée, WebView2 la met en pause.
pub fn page_visible(f: &WebviewWindow, visible: bool) -> tauri::Result<()> {
    f.with_webview(move |pw| {
        // SAFETY : appel COM ordinaire sur le contrôleur de WebView2 que Tauri nous prête, sur le fil de la fenêtre
        let _ = unsafe { pw.controller().SetIsVisible(visible) };
    })
}

/// La veille profonde de la page (vue.ps1:261-262, `TrySuspendAsync`), une minute après qu'elle a été cachée ; ou son
/// réveil (vue.ps1:245, `Resume`). WebView2 n'accepte la veille que pour une page cachée (`SetIsVisible(false)`).
pub fn page_en_veille(f: &WebviewWindow, veille: bool) -> tauri::Result<()> {
    f.with_webview(move |pw| {
        // SAFETY : appels COM ordinaires sur la WebView2 que Tauri nous prête, sur le fil de la fenêtre ; le
        // gestionnaire de fin ne garde rien
        let _ = unsafe {
            pw.controller().CoreWebView2().and_then(|w| w.cast::<ICoreWebView2_3>()).and_then(|w3| {
                if veille {
                    let h = TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(())));
                    w3.TrySuspend(&h)
                } else {
                    w3.Resume()
                }
            })
        };
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texte_du_journal() {
        let tout_coupe = Reglages {
            menu_clic_droit: false,
            raccourcis_navigateur: false,
            saisie_automatique: false,
            mots_de_passe: false,
            outils_developpement: false,
            zoom: false,
        };
        assert!(tout_coupe.texte().ends_with("coupés"));
        let r = Reglages { menu_clic_droit: true, mots_de_passe: true, ..tout_coupe };
        assert_eq!(r.texte(), "Écran : encore allumés malgré les réglages : menu du clic droit, mots de passe gardés");
    }
}
