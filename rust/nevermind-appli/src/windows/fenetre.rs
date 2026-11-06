//! La fenêtre principale vue par Windows : accrochée dans Mindset ou non (`EmbedInfo.IsEmbedded`), visible, devant
//! (`VueNatif.EstDevant`), la propriété `Mindset.BarrePropre` (`EmbedInfo.BarrePropre`), la taille minimale gardée
//! quand Mindset change la taille (comme Mindcare en Rust). Et [`FenetreTauri`], le trait `Fenetre` de la vie de
//! l'appli rempli avec la vraie fenêtre.

use tauri::{LogicalSize, WebviewWindow};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GWL_STYLE, GetAncestor, GetForegroundWindow, GetParent, GetWindowLongPtrW, GetWindowThreadProcessId,
    IsIconic, IsWindow, IsWindowVisible, MINMAXINFO, RemovePropW, SW_HIDE, SW_MINIMIZE, SW_RESTORE, SW_SHOWNA,
    SWP_NOSIZE, SendMessageW, SetPropW, ShowWindow, WINDOWPOS, WM_GETMINMAXINFO, WM_WINDOWPOSCHANGING, WS_CHILD,
    WS_MINIMIZE,
};
use windows::core::w;

use super::hwnd;
use crate::vie::{Fenetre, Niveau, Taille};

/// Le numéro de la sous-classe de [`garder_taille_minimale`] (« NMMI »).
const SOUS_CLASSE_TAILLE: usize = 0x4E4D_4D49;
/// Au plus tant de niveaux de parents (garde contre une boucle).
const NIVEAUX_MAX: usize = 64;

/// Garde la taille minimale quand un AUTRE programme change la taille (`SetWindowPos`) : tao répond lui-même à
/// `WM_WINDOWPOSCHANGING` sans `DefWindowProc`, qui seul borne la taille (vu pour Mindcare en Rust). Rien une fois
/// accrochée (`WS_CHILD`). À appeler une fois, sur le fil de la fenêtre.
pub fn garder_taille_minimale(h: usize) -> bool {
    // SAFETY : une fenêtre de ce programme, sur ce fil ; la procédure ne garde aucune donnée
    existe(h) && unsafe { SetWindowSubclass(hwnd(h), Some(taille_minimale), SOUS_CLASSE_TAILLE, 0) }.as_bool()
}

unsafe extern "system" fn taille_minimale(
    h: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _sous_classe: usize,
    _donnees: usize,
) -> LRESULT {
    if message == WM_WINDOWPOSCHANGING && lparam.0 != 0 {
        // SAFETY : pour WM_WINDOWPOSCHANGING, lparam est un WINDOWPOS modifiable pendant l'appel
        let (pos, style) = unsafe { (&mut *(lparam.0 as *mut WINDOWPOS), GetWindowLongPtrW(h, GWL_STYLE) as u32) };
        if !pos.flags.contains(SWP_NOSIZE) && style & (WS_CHILD.0 | WS_MINIMIZE.0) == 0 {
            let mut mm = MINMAXINFO::default();
            // SAFETY : la fenêtre de ce fil (appel direct) ; mm vit pendant l'appel
            unsafe { SendMessageW(h, WM_GETMINMAXINFO, None, Some(LPARAM(&mut mm as *mut MINMAXINFO as isize))) };
            pos.cx = pos.cx.max(mm.ptMinTrackSize.x);
            pos.cy = pos.cy.max(mm.ptMinTrackSize.y);
        }
    }
    // SAFETY : le message tel que reçu, au suivant de la chaîne
    unsafe { DefSubclassProc(h, message, wparam, lparam) }
}

/// La fenêtre existe encore (`IsWindow`).
pub fn existe(h: usize) -> bool {
    // SAFETY : IsWindow accepte n'importe quelle valeur
    h != 0 && unsafe { IsWindow(Some(hwnd(h))) }.as_bool()
}

fn parent(h: usize) -> usize {
    // SAFETY : lecture seule ; sans parent, une erreur lue comme 0
    unsafe { GetParent(hwnd(h)) }.map(|p| p.0 as usize).unwrap_or(0)
}

/// Accrochée dans Mindset : `WS_CHILD` ou un parent (natif.cs:2362-2367).
pub fn est_accrochee(h: usize) -> bool {
    if !existe(h) {
        return false;
    }
    // SAFETY : lecture seule du style d'une fenêtre qui existe
    let style = unsafe { GetWindowLongPtrW(hwnd(h), GWL_STYLE) };
    (style as u32) & WS_CHILD.0 != 0 || parent(h) != 0
}

/// Visible (`IsWindowVisible` : elle et tous ses parents).
pub fn est_visible(h: usize) -> bool {
    // SAFETY : lecture seule
    existe(h) && unsafe { IsWindowVisible(hwnd(h)) }.as_bool()
}

/// La fenêtre puis chacun de ses parents, visibles et réduits ou non.
pub fn niveaux(h: usize) -> Vec<Niveau> {
    let mut l = Vec::new();
    let mut x = h;
    while existe(x) && l.len() < NIVEAUX_MAX {
        // SAFETY : lectures seules sur une fenêtre qui existe
        let (visible, reduite) = unsafe { (IsWindowVisible(hwnd(x)).as_bool(), IsIconic(hwnd(x)).as_bool()) };
        l.push(Niveau { visible, reduite });
        x = parent(x);
    }
    l
}

/// La fenêtre de premier niveau qui contient `h` (`GetAncestor(GA_ROOT)`), ou `h`.
fn racine(h: usize) -> usize {
    // SAFETY : lecture seule
    let r = unsafe { GetAncestor(hwnd(h), GA_ROOT) }.0 as usize;
    if r != 0 { r } else { h }
}

/// `VueNatif.EstDevant` (natif.cs:2462-2468) : sa fenêtre principale (la sienne, ou Mindset) est au premier plan.
pub fn est_devant(h: usize) -> bool {
    // SAFETY : lecture seule
    let devant = unsafe { GetForegroundWindow() }.0 as usize;
    h != 0 && devant != 0 && racine(devant) == racine(h)
}

/// Le processus de la fenêtre au premier plan est celui-ci (`Test-ThemeSansGener`).
pub fn premier_plan_a_moi() -> bool {
    let mut pid = 0u32;
    // SAFETY : lectures seules ; pid vit pendant l'appel
    unsafe {
        let f = GetForegroundWindow();
        if f.0.is_null() {
            return false;
        }
        GetWindowThreadProcessId(f, Some(&mut pid));
    }
    pid == std::process::id()
}

/// `Mindset.BarrePropre` sur la fenêtre (natif.cs:2354-2359).
pub fn barre_propre(h: usize, oui: bool) {
    if !existe(h) {
        return;
    }
    // SAFETY : une fenêtre de ce programme ; une propriété à valeur entière (aucune mémoire derrière)
    unsafe {
        if oui {
            let _ = SetPropW(
                hwnd(h),
                w!("Mindset.BarrePropre"),
                Some(HANDLE(std::ptr::dangling_mut::<core::ffi::c_void>())),
            );
        } else {
            let _ = RemovePropW(hwnd(h), w!("Mindset.BarrePropre"));
        }
    }
}

fn montrer(h: usize, cmd: windows::Win32::UI::WindowsAndMessaging::SHOW_WINDOW_CMD) {
    if existe(h) {
        // SAFETY : la fenêtre de ce programme, qui existe
        unsafe {
            let _ = ShowWindow(hwnd(h), cmd);
        }
    }
}

/// La vraie fenêtre principale.
pub struct FenetreTauri {
    fenetre: WebviewWindow,
    h: usize,
    journal: Box<dyn Fn(&str) + Send + Sync>,
}

impl FenetreTauri {
    pub fn nouvelle(fenetre: WebviewWindow, h: usize, journal: impl Fn(&str) + Send + Sync + 'static) -> FenetreTauri {
        FenetreTauri { fenetre, h, journal: Box::new(journal) }
    }
    pub fn hwnd(&self) -> usize {
        self.h
    }
}

impl Fenetre for FenetreTauri {
    fn existe(&self) -> bool {
        existe(self.h)
    }
    fn est_accrochee(&self) -> bool {
        est_accrochee(self.h)
    }
    fn est_visible(&self) -> bool {
        est_visible(self.h)
    }
    fn montrer_sans_activer(&mut self) {
        // (evenements.ps1:74 : réduite, elle est d'abord rendue)
        // SAFETY : lecture seule
        if existe(self.h) && unsafe { IsIconic(hwnd(self.h)) }.as_bool() {
            montrer(self.h, SW_RESTORE);
        }
        montrer(self.h, SW_SHOWNA);
    }
    fn montrer_et_activer(&mut self) {
        let _ = self.fenetre.show();
        if self.fenetre.is_minimized().unwrap_or(false) {
            let _ = self.fenetre.unminimize();
        }
        let _ = self.fenetre.set_always_on_top(true);
        let _ = self.fenetre.set_always_on_top(false);
        let _ = self.fenetre.set_focus();
    }
    fn cacher(&mut self) {
        montrer(self.h, SW_HIDE);
    }
    fn reduire(&mut self) {
        montrer(self.h, SW_MINIMIZE);
    }
    fn poser_taille_minimale(&mut self, taille: Option<Taille>) {
        let t = taille.map(|t| LogicalSize::new(t.largeur, t.hauteur));
        let _ = self.fenetre.set_min_size(t);
    }
    fn poser_barre_propre(&mut self, oui: bool) {
        barre_propre(self.h, oui);
    }
    fn niveaux(&self) -> Vec<Niveau> {
        niveaux(self.h)
    }
    fn est_devant(&self) -> bool {
        est_devant(self.h)
    }
    fn premier_plan_est_a_moi(&self) -> bool {
        premier_plan_a_moi()
    }
    fn page_visible(&mut self, visible: bool) {
        if let Err(e) = super::webview2::page_visible(&self.fenetre, visible) {
            (self.journal)(&format!("Veille de l'écran : {e}"));
        }
    }
    fn page_en_veille(&mut self, veille: bool) {
        if let Err(e) = super::webview2::page_en_veille(&self.fenetre, veille) {
            (self.journal)(&format!("Veille de l'écran : {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    //! Sur de vraies fenêtres Windows (la classe « STATIC »), créées sur un BUREAU WINDOWS À PART (CreateDesktop,
    //! propre à ce fil) : même montrées, elles n'apparaissent jamais à l'écran.

    use super::*;
    use windows::Win32::Foundation::GENERIC_ALL;
    use windows::Win32::System::StationsAndDesktops::{
        CloseDesktop, CreateDesktopW, DESKTOP_CONTROL_FLAGS, GetThreadDesktop, SetThreadDesktop,
    };
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetPropW, SetParent, SetWindowLongPtrW, WINDOW_EX_STYLE, WINDOW_STYLE,
        WS_OVERLAPPEDWINDOW,
    };
    use windows::core::{HSTRING, PCWSTR};

    fn creer(style: WINDOW_STYLE, parent: Option<HWND>) -> usize {
        // SAFETY : classe système, textes constants ; la fenêtre est détruite à la fin
        let h = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("NevermindEssai"),
                style,
                0,
                0,
                200,
                150,
                parent,
                None,
                None,
                None,
            )
        }
        .expect("fenêtre d'essai");
        h.0 as usize
    }

    fn sur_un_bureau_a_part(essai: impl FnOnce() + Send + 'static) {
        std::thread::spawn(move || {
            static NUMERO: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = NUMERO.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let nom = HSTRING::from(format!("NevermindEssai-{}-fenetre-{n}", std::process::id()));
            // SAFETY : nom valide ; bureau neuf, jamais affiché
            let bureau =
                unsafe { CreateDesktopW(&nom, PCWSTR::null(), None, DESKTOP_CONTROL_FLAGS(0), GENERIC_ALL.0, None) }
                    .expect("bureau d'essai");
            // SAFETY : ce fil n'a encore aucune fenêtre ; il revient à son bureau avant de fermer celui d'essai
            unsafe {
                let avant = GetThreadDesktop(GetCurrentThreadId()).expect("bureau du fil");
                SetThreadDesktop(bureau).expect("passer sur le bureau d'essai");
                essai();
                SetThreadDesktop(avant).expect("revenir au bureau d'avant");
                let _ = CloseDesktop(bureau);
            }
        })
        .join()
        .expect("essai sur le bureau à part");
    }

    fn detruire(h: usize) {
        // SAFETY : une fenêtre d'essai de ce fil
        unsafe {
            let _ = DestroyWindow(hwnd(h));
        }
    }

    #[test]
    fn accrochee_comme_par_mindset_et_barre_propre() {
        sur_un_bureau_a_part(|| {
            let panneau = creer(WS_OVERLAPPEDWINDOW, None);
            let appli = creer(WS_OVERLAPPEDWINDOW, None);
            assert!(!est_accrochee(appli));
            // SAFETY : fenêtres d'essai de ce fil
            unsafe {
                let style = GetWindowLongPtrW(hwnd(appli), GWL_STYLE);
                SetWindowLongPtrW(hwnd(appli), GWL_STYLE, style | WS_CHILD.0 as isize);
                SetParent(hwnd(appli), Some(hwnd(panneau))).expect("SetParent");
            }
            assert!(est_accrochee(appli));
            assert_eq!(niveaux(appli).len(), 2);
            barre_propre(appli, true);
            // SAFETY : lecture d'une propriété d'une fenêtre d'essai
            assert_eq!(unsafe { GetPropW(hwnd(appli), w!("Mindset.BarrePropre")) }.0 as usize, 1);
            barre_propre(appli, false);
            // SAFETY : idem
            assert!(unsafe { GetPropW(hwnd(appli), w!("Mindset.BarrePropre")) }.0.is_null());
            assert!(!est_devant(appli), "le bureau à part n'a rien au premier plan");
            for h in [appli, panneau] {
                detruire(h);
            }
            assert!(!existe(appli) && niveaux(appli).is_empty());
        });
    }

    #[test]
    fn visible_seulement_si_tous_les_niveaux_le_sont() {
        sur_un_bureau_a_part(|| {
            let panneau = creer(WS_OVERLAPPEDWINDOW, None);
            let appli = creer(WS_CHILD, Some(hwnd(panneau)));
            montrer(appli, SW_SHOWNA);
            assert!(!est_visible(appli), "le panneau est encore caché");
            montrer(panneau, SW_SHOWNA);
            assert!(est_visible(appli));
            detruire(panneau);
            assert!(!existe(appli));
        });
    }
}
