//! L'Organizer côté Windows (organizer.ps1 et les classes WinFocus, OGNative.AddHotKey et MouseHook de natif.cs) :
//! le fil à part qui fait tourner la veille, les fenêtres du jeu (comme Get-Process -Name 'Dofus*' et leur fenêtre
//! principale), la fenêtre devant, la mise au premier plan (rien n'est envoyé au jeu : c'est un clic dans la barre des
//! tâches), les raccourcis pris à Windows seulement quand le jeu est devant, l'écoute des boutons de la souris sur son
//! propre fil, la barre flottante et la vue.

#[path = "organizer_barre.rs"]
mod barre;
#[path = "organizer_dessin.rs"]
pub mod dessin;
#[path = "organizer_logos.rs"]
pub mod logos;
#[path = "organizer_vue.rs"]
mod vue;

use crate::organizer::fil::{Commande, Params, Partage, verrou};
use crate::organizer::veille::{Bureau, Pastille, Veille};
use crate::organizer::{Config, Fenetre, Perso};
use dessin::Couleur;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, HOT_KEY_MODIFIERS, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows,
    GW_OWNER, GetForegroundWindow, GetMessageW, GetWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, HHOOK, HWND_MESSAGE, IsIconic, IsWindow, IsWindowVisible, KillTimer, MSG, MSLLHOOKSTRUCT,
    PM_NOREMOVE, PeekMessageW, PostMessageW, PostThreadMessageW, RegisterClassExW, SW_RESTORE, SetForegroundWindow,
    SetTimer, SetWindowsHookExW, ShowWindow, TranslateMessage, UnhookWindowsHookEx, WH_MOUSE_LL, WINDOW_EX_STYLE,
    WM_APP, WM_HOTKEY, WM_QUIT, WM_TIMER, WNDCLASSEXW, WS_OVERLAPPED,
};
use windows::core::{BOOL, PCWSTR, w};

/// Le message d'un bouton de souris pris (MouseHook.Message) : wParam = identifiant du raccourci.
pub const WM_SOURIS: u32 = 0x804E;
/// Le réveil du fil : une commande attend dans la file.
const WM_REVEIL: u32 = WM_APP + 1;
/// La minuterie de la veille (250 ms, comme $script:OrgTimer).
const MINUTERIE: usize = 1;
/// La minuterie qui relâche la touche d'une touche simulée.
const RELACHE: usize = 2;
/// La minuterie qui regarde, vue ouverte, si un logo de classe est arrivé.
const LOGOS: usize = 3;
/// Combien de temps la touche simulée reste appuyée : comme un appui au clavier (un jeu qui regarde le clavier une
/// fois par image la voit).
const DUREE_APPUI_MS: u32 = 60;

// ------------------------------------------------------------------ les touches simulées

/// Les touches à appuyer puis relâcher (modificateurs d'abord) : codes de touche Windows, dans l'ordre d'appui.
pub fn touches_a_appuyer(mods: u32, vk: u32) -> Vec<u16> {
    let mut l = Vec::new();
    for (bit, code) in [(2, 0x11u16), (1, 0x12), (4, 0x10), (8, 0x5B)] {
        if mods & bit != 0 {
            l.push(code);
        }
    }
    l.push(vk as u16);
    l
}

/// Appuie (ou relâche) ces touches dans la fenêtre devant, comme le clavier : avec le code de la touche physique
/// (celui de la disposition du clavier de cette fenêtre), pour les jeux qui le lisent.
fn clavier(touches: &[u16], relacher: bool) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC_EX, MapVirtualKeyExW, SendInput, VIRTUAL_KEY,
    };
    unsafe {
        let fil = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let disposition = GetKeyboardLayout(fil);
        let mut l: Vec<INPUT> = Vec::new();
        let ordre: Vec<u16> = if relacher { touches.iter().rev().copied().collect() } else { touches.to_vec() };
        for vk in ordre {
            let scan = MapVirtualKeyExW(u32::from(vk), MAPVK_VK_TO_VSC_EX, Some(disposition));
            let mut f = KEYBD_EVENT_FLAGS(0);
            if scan & 0xFF00 == 0xE000 {
                f |= KEYEVENTF_EXTENDEDKEY;
            }
            if relacher {
                f |= KEYEVENTF_KEYUP;
            }
            l.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        wScan: (scan & 0xFF) as u16,
                        dwFlags: f,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        SendInput(&l, size_of::<INPUT>() as i32) as usize == l.len()
    }
}

/// Essai : les touches qui auraient été envoyées (rien n'est envoyé pour de vrai dans un essai).
static ESSAI_ENVOIS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Essai : les touches simulées demandées depuis le début (« mods:vk »).
pub fn essai_envois() -> Vec<String> {
    verrou(&ESSAI_ENVOIS).clone()
}

// ------------------------------------------------------------------ le thème

/// Les couleurs de la DA (cyan, violet, magenta du thème) et ses coins (carrés pour le thème Terminal).
#[derive(Debug, Clone)]
pub struct Theme {
    pub p: Couleur,
    pub s: Couleur,
    pub t: Couleur,
    pub carre: bool,
    pub police: &'static str,
}

/// Le thème choisi (réglage « Theme », anciens noms compris), comme themes.ps1.
pub fn theme(nom: &str) -> Theme {
    let n = match nom {
        "retro" => "arcade",
        "dresseur" => "rubis",
        x => x,
    };
    let (p, s, t) = match n {
        "crepuscule" => ("#FF8A3D", "#FF3D7F", "#FFC83D"),
        "terminal" => ("#33FF77", "#B6FF3B", "#00E0A0"),
        "arcade" => ("#FBD000", "#E52521", "#1E88E5"),
        "rubis" => ("#FF3B3B", "#FFDE00", "#3B6FFF"),
        _ => ("#00E5FF", "#B04BFF", "#FF2EB5"),
    };
    let terminal = n == "terminal";
    Theme {
        p: Couleur::hex(p),
        s: Couleur::hex(s),
        t: Couleur::hex(t),
        carre: terminal,
        police: if terminal { "Cascadia Code, Consolas" } else { "Segoe UI Variable Display, Segoe UI" },
    }
}

impl Theme {
    /// Le bord « plasma » (New-OrgPlasmaBrush) : cyan, violet, magenta, cyan.
    pub fn plasma(&self, angle: f32, opacite: f32) -> dessin::Peinture {
        dessin::Peinture::Plasma { couleurs: [self.p, self.s, self.t, self.p], angle, opacite }
    }
    /// Get-OrgBarRadius : en pilule, sauf thème Terminal (coins carrés).
    pub fn rayon(&self, rond: f32, carre: f32) -> f32 {
        if self.carre { carre } else { rond }
    }
}

// ------------------------------------------------------------------ les fenêtres du jeu

fn hwnd(h: isize) -> HWND {
    HWND(h as *mut _)
}

/// Le programme d'une fenêtre ([WinFocus]::Pid).
pub fn pid_de(h: isize) -> u32 {
    if h == 0 {
        return 0;
    }
    let mut p = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd(h), Some(&mut p)) };
    p
}

fn titre_de(h: isize) -> String {
    unsafe {
        let n = GetWindowTextLengthW(hwnd(h));
        if n <= 0 {
            return String::new();
        }
        let mut b = vec![0u16; n as usize + 1];
        let k = GetWindowTextW(hwnd(h), &mut b);
        String::from_utf16_lossy(&b[..k.max(0) as usize])
    }
}

/// La fenêtre principale de chaque programme, comme Process.MainWindowHandle de .NET : la première fenêtre visible
/// sans propriétaire, dans l'ordre de Windows. Les fenêtres du fil de l'Organizer (barre, vue) ne comptent pas.
fn fenetres_principales(fil: u32) -> Vec<(isize, u32)> {
    struct Recolte {
        l: Vec<(isize, u32)>,
        fil: u32,
    }
    unsafe extern "system" fn chaque(h: HWND, lp: LPARAM) -> BOOL {
        let r = unsafe { &mut *(lp.0 as *mut Recolte) };
        unsafe {
            if !IsWindowVisible(h).as_bool() || GetWindow(h, GW_OWNER).is_ok_and(|o| !o.is_invalid()) {
                return BOOL(1);
            }
            let mut p = 0u32;
            let t = GetWindowThreadProcessId(h, Some(&mut p));
            if t == r.fil || p == 0 || r.l.iter().any(|x| x.1 == p) {
                return BOOL(1);
            }
            r.l.push((h.0 as isize, p));
        }
        BOOL(1)
    }
    let mut r = Recolte { l: Vec::new(), fil };
    unsafe {
        let _ = EnumWindows(Some(chaque), LPARAM(&mut r as *mut _ as isize));
    }
    r.l
}

/// Le nom de chaque programme (sans « .exe »), comme Process.ProcessName.
fn noms_programmes() -> HashMap<u32, String> {
    let mut m = HashMap::new();
    unsafe {
        let Ok(s) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return m };
        let mut e = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(s, &mut e).is_ok();
        while ok {
            let n = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            let nom = String::from_utf16_lossy(&e.szExeFile[..n]);
            let nom = match nom.len().checked_sub(4) {
                Some(i) if nom.is_char_boundary(i) && nom[i..].eq_ignore_ascii_case(".exe") => nom[..i].to_string(),
                _ => nom,
            };
            m.insert(e.th32ProcessID, nom);
            ok = Process32NextW(s, &mut e).is_ok();
        }
        let _ = CloseHandle(s);
    }
    m
}

/// Met une fenêtre au premier plan ([WinFocus]::Focus) : comme la barre des tâches, rien n'est envoyé au jeu.
pub fn afficher(h: isize) -> bool {
    unsafe {
        if h == 0 || !IsWindow(Some(hwnd(h))).as_bool() {
            return false;
        }
        if IsIconic(hwnd(h)).as_bool() {
            let _ = ShowWindow(hwnd(h), SW_RESTORE);
        }
        if SetForegroundWindow(hwnd(h)).as_bool() {
            return true;
        }
        // (Windows refuse parfois : on s'accroche un instant au fil de la fenêtre active, comme la barre des tâches)
        let fg = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let moi = GetCurrentThreadId();
        let att = fg != 0 && fg != moi && AttachThreadInput(moi, fg, true).as_bool();
        let _ = BringWindowToTop(hwnd(h));
        let ok = SetForegroundWindow(hwnd(h)).as_bool();
        if att {
            let _ = AttachThreadInput(moi, fg, false);
        }
        ok
    }
}

// ------------------------------------------------------------------ l'écoute des boutons de la souris (MouseHook)

static SOURIS_LIENS: Mutex<Vec<(u32, i32)>> = Mutex::new(Vec::new());
static SOURIS_CIBLE: AtomicIsize = AtomicIsize::new(0);
static SOURIS_CROCHET: AtomicIsize = AtomicIsize::new(0);
static SOURIS_AVALE: AtomicU32 = AtomicU32::new(0);
static SOURIS_FIL: AtomicU32 = AtomicU32::new(0);

/// Ce que fait l'écoute d'un message de souris : avaler le message (le jeu ne le reçoit pas), et le raccourci à
/// signaler. `bouton` : 3 molette, 4 et 5 latéraux (0 : un autre message) ; `avale` : le bouton dont on avale le
/// relâchement. Renvoie (avaler, raccourci, nouvel « avale »).
pub fn decision_souris(
    message: u32,
    x_bouton: u32,
    mods: u32,
    liens: &[(u32, i32)],
    avale: u32,
) -> (bool, Option<i32>, u32) {
    let appui = message == 0x0207 || message == 0x020B;
    let relache = message == 0x0208 || message == 0x020C;
    let bouton = match message {
        0x0207 | 0x0208 => 3,
        0x020B | 0x020C => {
            if x_bouton == 1 {
                4
            } else {
                5
            }
        }
        _ => 0,
    };
    if bouton == 0 {
        return (false, None, avale);
    }
    if relache && avale == bouton {
        return (true, None, 0);
    }
    if appui && let Some((_, id)) = liens.iter().find(|(k, _)| *k == (mods << 8) | bouton) {
        return (true, Some(*id), bouton);
    }
    (false, None, avale)
}

fn tenue(vk: i32) -> bool {
    (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
}

unsafe extern "system" fn crochet_souris(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let m = wp.0 as u32;
        if (0x0207..=0x020C).contains(&m) {
            let info = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
            let xb = (info.mouseData >> 16) & 0xFFFF;
            let mods = u32::from(tenue(0x12))
                | (u32::from(tenue(0x11)) << 1)
                | (u32::from(tenue(0x10)) << 2)
                | (u32::from(tenue(0x5B) || tenue(0x5C)) << 3);
            let liens = verrou(&SOURIS_LIENS).clone();
            let (avaler, id, nouvel) = decision_souris(m, xb, mods, &liens, SOURIS_AVALE.load(Ordering::Relaxed));
            SOURIS_AVALE.store(nouvel, Ordering::Relaxed);
            if let Some(id) = id {
                let c = SOURIS_CIBLE.load(Ordering::Relaxed);
                unsafe {
                    let _ = PostMessageW(Some(hwnd(c)), WM_SOURIS, WPARAM(id as usize), LPARAM(0));
                }
            }
            if avaler {
                return LRESULT(1);
            }
        }
    }
    let h = SOURIS_CROCHET.load(Ordering::Relaxed);
    unsafe { CallNextHookEx(Some(HHOOK(h as *mut _)), code, wp, lp) }
}

/// Démarre l'écoute sur son propre fil (Windows attend ce fil à chaque mouvement de la souris, dans tout le PC : il ne
/// fait rien d'autre).
fn souris_demarrer(cible: isize) -> bool {
    SOURIS_CIBLE.store(cible, Ordering::Relaxed);
    if SOURIS_CROCHET.load(Ordering::Relaxed) != 0 {
        return true;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let r = std::thread::Builder::new().name("Nevermind : boutons de la souris".into()).spawn(move || unsafe {
        let mut m = MSG::default();
        let _ = PeekMessageW(&mut m, None, 0, 0, PM_NOREMOVE);
        SOURIS_FIL.store(GetCurrentThreadId(), Ordering::Relaxed);
        let module = GetModuleHandleW(PCWSTR::null()).ok();
        let h = SetWindowsHookExW(WH_MOUSE_LL, Some(crochet_souris), module.map(Into::into), 0);
        let h = h.map(|h| h.0 as isize).unwrap_or(0);
        SOURIS_CROCHET.store(h, Ordering::Relaxed);
        let _ = tx.send(h);
        if h == 0 {
            return;
        }
        while GetMessageW(&mut m, None, 0, 0).0 > 0 {}
        let _ = UnhookWindowsHookEx(HHOOK(h as *mut _));
    });
    if r.is_err() {
        return false;
    }
    let h = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap_or(0);
    if let (0, Ok(f)) = (h, r) {
        let _ = f.join();
        return false;
    }
    h != 0
}

fn souris_arreter() {
    let f = SOURIS_FIL.swap(0, Ordering::Relaxed);
    if f != 0 {
        unsafe {
            let _ = PostThreadMessageW(f, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
    SOURIS_CROCHET.store(0, Ordering::Relaxed);
    SOURIS_AVALE.store(0, Ordering::Relaxed);
}

// ------------------------------------------------------------------ le vrai Windows pour la veille

/// Windows pour la veille : fenêtres du jeu, raccourcis, souris, barre, vue.
pub struct BureauWin {
    partage: Arc<Partage>,
    donnees: PathBuf,
    programme: String,
    essai: bool,
    devant_essai: Option<Arc<std::sync::atomic::AtomicIsize>>,
    fil: u32,
    pub messages: isize,
    pub barre: barre::Barre,
    pub vue: Option<vue::FenetreVue>,
    /// La vue est à refaire (avec cette liste) après l'appel en cours de la veille.
    vue_a_refaire: Option<Vec<Perso>>,
    noms: HashMap<(u32, isize), Option<String>>,
    /// La touche simulée encore appuyée (relâchée par la minuterie RELACHE).
    appuyees: Vec<u16>,
}

impl BureauWin {
    /// Relâche la touche simulée encore appuyée.
    fn relacher(&mut self) {
        unsafe {
            let _ = KillTimer(Some(hwnd(self.messages)), RELACHE);
        }
        let l = std::mem::take(&mut self.appuyees);
        if !l.is_empty() && !self.essai {
            clavier(&l, true);
        }
    }

    /// Essai : la fenêtre de la vue (0 si fermée) et celle de la barre.
    pub fn vue_hwnd(&self) -> isize {
        self.vue.as_ref().map(|v| v.hwnd()).unwrap_or(0)
    }
    pub fn barre_hwnd(&self) -> isize {
        self.barre.hwnd()
    }
    pub fn barre_cout_images(&mut self, n: u32) -> std::time::Duration {
        self.barre.cout_images(n)
    }
    /// Une bulle d'information pour l'écran de Nevermind (Set-Status).
    pub fn partage_statut(&self, m: &str) {
        self.partage.statut(m);
    }
}

impl Bureau for BureauWin {
    fn fenetres(&mut self) -> Vec<Fenetre> {
        let principales = fenetres_principales(self.fil);
        // (le nom du programme n'est cherché que pour une fenêtre pas encore vue : une seule liste des programmes)
        self.noms.retain(|k, _| principales.iter().any(|(h, p)| (*p, *h) == *k));
        if principales.iter().any(|(h, p)| !self.noms.contains_key(&(*p, *h))) {
            let noms = noms_programmes();
            let pre = self.programme.to_lowercase();
            for (h, p) in &principales {
                self.noms
                    .entry((*p, *h))
                    .or_insert_with(|| noms.get(p).map(|n| n.to_lowercase()).filter(|n| n.starts_with(&pre)));
            }
        }
        let mut l: Vec<(String, u32, isize)> = principales
            .iter()
            .filter_map(|(h, p)| self.noms.get(&(*p, *h)).cloned().flatten().map(|n| (n, *p, *h)))
            .collect();
        // (Get-Process range par nom, puis par numéro)
        l.sort();
        l.into_iter().map(|(_, p, h)| Fenetre::depuis_titre(&titre_de(h), p, h)).collect()
    }

    fn devant(&mut self) -> isize {
        if let Some(d) = &self.devant_essai {
            return d.load(Ordering::Relaxed);
        }
        unsafe { GetForegroundWindow() }.0 as isize
    }

    fn pid(&mut self, h: isize) -> u32 {
        pid_de(h)
    }

    fn afficher(&mut self, h: isize) -> bool {
        let ok = afficher(h);
        // (essai : la fenêtre « devant » simulée suit la mise au premier plan demandée)
        if let Some(d) = &self.devant_essai
            && unsafe { IsWindow(Some(hwnd(h))) }.as_bool()
        {
            d.store(h, Ordering::Relaxed);
            return true;
        }
        ok
    }

    fn prendre_touche(&mut self, id: i32, mods: u32, vk: u32) -> bool {
        // (MOD_NOREPEAT : une touche tenue ne répète pas)
        unsafe { RegisterHotKey(Some(hwnd(self.messages)), id, HOT_KEY_MODIFIERS(mods | 0x4000), vk) }.is_ok()
    }

    fn rendre_touche(&mut self, id: i32) {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd(self.messages)), id);
        }
    }

    fn souris_vider(&mut self) {
        verrou(&SOURIS_LIENS).clear();
    }

    fn souris_lier(&mut self, mods: u32, bouton: u32, id: i32) {
        let mut l = verrou(&SOURIS_LIENS);
        let k = (mods << 8) | bouton;
        l.retain(|x| x.0 != k);
        l.push((k, id));
    }

    fn souris_demarrer(&mut self) -> bool {
        // (essai : l'écoute globale de la souris n'est jamais posée)
        self.essai || souris_demarrer(self.messages)
    }

    fn souris_arreter(&mut self) {
        if !self.essai {
            souris_arreter();
        }
    }

    fn envoyer_touche(&mut self, mods: u32, vk: u32) -> bool {
        // (la précédente encore appuyée : relâchée d'abord)
        self.relacher();
        let l = touches_a_appuyer(mods, vk);
        if self.essai {
            verrou(&ESSAI_ENVOIS).push(format!("{mods}:{vk}"));
        } else if !clavier(&l, false) {
            clavier(&l, true);
            return false;
        }
        self.appuyees = l;
        unsafe { SetTimer(Some(hwnd(self.messages)), RELACHE, DUREE_APPUI_MS, None) };
        true
    }

    fn survol_barre(&mut self) -> bool {
        self.barre.survolee()
    }

    fn barre_dessiner(&mut self, pastilles: &[Pastille]) {
        self.barre.dessiner(pastilles);
    }

    fn barre_montrer(&mut self) {
        self.barre.montrer();
    }

    fn barre_cacher(&mut self) {
        self.barre.cacher();
    }

    fn barre_devant(&mut self) {
        self.barre.devant();
    }

    fn vue_redessiner(&mut self, liste: &[Perso]) {
        self.vue_a_refaire = Some(liste.to_vec());
    }

    fn enregistrer(&mut self, c: &Config) {
        if let Err(e) = crate::json::ecrire_json_sur(&self.donnees.join("organizer.json"), &c.vers_json(), false) {
            self.partage.journal(&format!("Organizer : enregistrement impossible : {e}"));
        }
    }

    fn journal(&mut self, m: &str) {
        self.partage.journal(m);
    }

    fn statut(&mut self, m: &str) {
        self.partage.statut(m);
    }
}

// ------------------------------------------------------------------ le fil

pub struct Etat {
    pub veille: Veille,
    pub bureau: BureauWin,
    minuterie: bool,
}

thread_local! {
    static ETAT: RefCell<Option<Etat>> = const { RefCell::new(None) };
}

/// Fait quelque chose avec l'état du fil (`None` si l'état est déjà pris : un message envoyé par Windows pendant un
/// appel de la veille, il est laissé à Windows).
pub fn avec<R>(f: impl FnOnce(&mut Etat) -> R) -> Option<R> {
    ETAT.with(|e| {
        let mut g = e.try_borrow_mut().ok()?;
        let etat = g.as_mut()?;
        // (une erreur ici ne doit jamais fermer Nevermind : on est souvent dans un appel de Windows, qui ne la
        // laisserait pas remonter ; elle est notée au journal et le geste est abandonné)
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let r = f(etat);
            etat.apres();
            r
        }));
        match r {
            Ok(r) => Some(r),
            Err(p) => {
                let m = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned());
                etat.bureau.partage.journal(&format!("Organizer : erreur ({})", m.unwrap_or_default()));
                None
            }
        }
    })
}

impl Etat {
    /// Après chaque geste : la vue refaite si la veille l'a demandé, la minuterie allumée seulement si la veille tourne.
    fn apres(&mut self) {
        // (un logo de classe vient d'arriver : la vue et la barre sont redessinées avec lui)
        if logos::arrives() {
            let Etat { veille, bureau, .. } = self;
            if veille.vue {
                veille.montrer_vue(bureau, true);
            }
            veille.dessiner_barre(bureau);
        }
        if let Some(l) = self.bureau.vue_a_refaire.take() {
            let devant = self.bureau.devant();
            if let Some(v) = self.bureau.vue.as_mut() {
                v.modele(crate::organizer::vue::modele(&self.veille, &l, devant));
            }
        }
        let m = self.bureau.messages;
        if self.veille.active && !self.minuterie {
            unsafe { SetTimer(Some(hwnd(m)), MINUTERIE, 250, None) };
            self.minuterie = true;
        } else if !self.veille.active && self.minuterie {
            unsafe {
                let _ = KillTimer(Some(hwnd(m)), MINUTERIE);
            }
            self.minuterie = false;
        }
    }

    /// La vue : ouverte (Show-OrgView $true) ou fermée.
    pub fn ouvrir_vue(&mut self) {
        if let Some(v) = &self.bureau.vue {
            v.devant();
            return;
        }
        let t = self.bureau.barre.theme.clone();
        self.bureau.vue = vue::FenetreVue::creer(t);
        // (les logos qui arrivent pendant que la vue est ouverte : regardés chaque seconde)
        unsafe { SetTimer(Some(hwnd(self.bureau.messages)), LOGOS, 1000, None) };
        let Etat { veille, bureau, .. } = self;
        veille.montrer_vue(bureau, true);
    }

    pub fn fermer_vue(&mut self) {
        let Etat { veille, bureau, .. } = self;
        // (une création de bouton pas finie est oubliée)
        if veille.capture.is_some() || veille.creation.is_some() {
            veille.annuler_choix(bureau);
        }
        veille.montrer_vue(bureau, false);
        veille.capture = None;
        veille.glisse = false;
        unsafe {
            let _ = KillTimer(Some(hwnd(bureau.messages)), LOGOS);
        }
        if let Some(v) = bureau.vue.take() {
            v.detruire();
        }
    }

    fn commande(&mut self, c: Commande) -> bool {
        match c {
            Commande::OuvrirVue => self.ouvrir_vue(),
            Commande::Cacher => {
                if self.bureau.vue.is_some() {
                    self.fermer_vue();
                }
                if self.veille.config.on {
                    let Etat { veille, bureau, .. } = self;
                    veille.allumer(bureau, false);
                }
            }
            Commande::Quitter => return false,
        }
        true
    }
}

unsafe extern "system" fn proc_messages(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match m {
        WM_TIMER if wp.0 == MINUTERIE => {
            avec(|e| {
                if e.veille.active {
                    let Etat { veille, bureau, .. } = e;
                    veille.passe(bureau, false);
                }
            });
            LRESULT(0)
        }
        WM_TIMER if wp.0 == LOGOS => {
            // (rien à faire ici : `apres` regarde si un logo est arrivé)
            avec(|_| ());
            LRESULT(0)
        }
        WM_TIMER if wp.0 == RELACHE => {
            avec(|e| e.bureau.relacher());
            LRESULT(0)
        }
        WM_ESSAI => {
            essai_traiter();
            LRESULT(0)
        }
        WM_HOTKEY | WM_SOURIS => {
            let id = wp.0 as i32;
            avec(|e| {
                let Etat { veille, bureau, .. } = e;
                veille.touche(bureau, id);
            });
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(h, m, wp, lp) },
    }
}

/// La fenêtre des messages du fil (raccourcis, minuterie, réveil), jamais affichée.
fn fenetre_messages() -> Option<isize> {
    unsafe {
        let inst = GetModuleHandleW(PCWSTR::null()).ok()?;
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc_messages),
            hInstance: inst.into(),
            lpszClassName: w!("NevermindOrganizerMessages"),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let h = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("NevermindOrganizerMessages"),
            w!("Nevermind Organizer"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(inst.into()),
            None,
        )
        .ok()?;
        Some(h.0 as isize)
    }
}

/// Le fil de l'Organizer : il tourne tant que l'Organizer est allumé ou que sa vue est ouverte.
pub fn tourner(p: Params, partage: Arc<Partage>) {
    unsafe {
        let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let Some(messages) = fenetre_messages() else {
        partage.journal("Organizer : fenêtre des messages impossible.");
        verrou(&partage.file).fini = true;
        return;
    };
    {
        let mut f = verrou(&partage.file);
        let m = messages;
        f.reveil = Some(Box::new(move || unsafe {
            let _ = PostMessageW(Some(hwnd(m)), WM_REVEIL, WPARAM(0), LPARAM(0));
        }));
    }
    let config = Config::lire(None, &p.donnees);
    // (les logos des classes : le dossier de la version PowerShell ; jamais téléchargés pendant un essai)
    logos::regler(p.donnees.join("organizer"), !p.essai);
    let t = theme(&p.theme);
    let bureau = BureauWin {
        partage: partage.clone(),
        donnees: p.donnees.clone(),
        programme: p.programme.clone(),
        essai: p.essai,
        devant_essai: p.devant_essai.clone(),
        fil: unsafe { GetCurrentThreadId() },
        messages,
        barre: barre::Barre::nouvelle(t, config.barre_x, config.barre_y),
        vue: None,
        vue_a_refaire: None,
        noms: HashMap::new(),
        appuyees: Vec::new(),
    };
    ESSAI_MESSAGES.store(messages, Ordering::Relaxed);
    let on = config.on;
    ETAT.with(|e| *e.borrow_mut() = Some(Etat { veille: Veille::nouvelle(config), bureau, minuterie: false }));
    // (au démarrage : l'organizer reprend s'il était actif ; la vue si elle est demandée)
    avec(|e| {
        if on {
            let Etat { veille, bureau, .. } = e;
            veille.demarrer(bureau);
        }
        if p.vue {
            e.ouvrir_vue();
        }
    });
    let mut quitter = false;
    loop {
        // Les commandes en attente ; plus rien à faire (éteint, vue fermée) : le fil s'arrête
        loop {
            let c = {
                let mut f = verrou(&partage.file);
                match f.commandes.pop_front() {
                    Some(c) => Some(c),
                    None => {
                        let fini = quitter || avec(|e| !e.veille.active && e.bureau.vue.is_none()).unwrap_or(true);
                        if fini {
                            f.fini = true;
                            f.reveil = None;
                        }
                        if fini {
                            quitter = true;
                        }
                        None
                    }
                }
            };
            match c {
                Some(c) => {
                    if avec(|e| e.commande(c)) == Some(false) {
                        quitter = true;
                    }
                }
                None => break,
            }
        }
        if quitter {
            break;
        }
        let mut m = MSG::default();
        if unsafe { GetMessageW(&mut m, None, 0, 0) }.0 <= 0 {
            break;
        }
        if m.message == WM_REVEIL {
            continue;
        }
        unsafe {
            let _ = TranslateMessage(&m);
            DispatchMessageW(&m);
        }
    }
    // La fin : raccourcis rendus, barre cachée, fenêtres détruites
    avec(|e| {
        e.bureau.relacher();
        let Etat { veille, bureau, .. } = e;
        veille.rendre_touches(bureau);
        veille.cacher_barre(bureau);
        veille.active = false;
    });
    {
        let mut f = verrou(&partage.file);
        f.fini = true;
        f.reveil = None;
    }
    ETAT.with(|e| {
        if let Some(mut e) = e.borrow_mut().take() {
            if let Some(v) = e.bureau.vue.take() {
                v.detruire();
            }
            e.bureau.barre.detruire();
        }
    });
    souris_arreter();
    ESSAI_MESSAGES.store(0, Ordering::Relaxed);
    unsafe {
        let _ = DestroyWindow(hwnd(messages));
    }
    dessin::liberer();
}

// ------------------------------------------------------------------ pour l'essai sur un bureau à part

static ESSAI_MESSAGES: AtomicIsize = AtomicIsize::new(0);

/// Essai : la fenêtre des messages du fil (pour y poster un faux appui de raccourci).
pub fn essai_messages() -> isize {
    ESSAI_MESSAGES.load(Ordering::Relaxed)
}

/// Essai : poste un message à la fenêtre des messages du fil (WM_HOTKEY d'un raccourci, par exemple).
pub fn essai_poster(message: u32, wp: usize) -> bool {
    let h = essai_messages();
    h != 0 && unsafe { PostMessageW(Some(hwnd(h)), message, WPARAM(wp), LPARAM(0)) }.is_ok()
}

/// Essai : demande au fil d'écrire les images de la barre et de la vue (fichiers BMP) dans ce dossier.
pub fn essai_captures(dossier: &std::path::Path) -> (bool, bool) {
    let d = dossier.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    *verrou(&ESSAI_DEMANDE) = Some(Box::new(move || {
        let r = avec(|e| {
            let b = e.bureau.barre.capturer(&d.join("barre.bmp"));
            let v = e.bureau.vue.as_mut().is_some_and(|v| v.capturer(&d.join("vue.bmp")));
            (b, v)
        })
        .unwrap_or((false, false));
        let _ = tx.send(r);
    }));
    if !essai_poster(WM_ESSAI, 0) {
        return (false, false);
    }
    rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap_or((false, false))
}

/// Essai : un geste dans le fil (cliquer une pastille, ouvrir la vue...), le résultat en texte.
pub fn essai_geste(f: impl FnOnce(&mut Etat) -> String + Send + 'static) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    *verrou(&ESSAI_DEMANDE) = Some(Box::new(move || {
        let r = avec(f).unwrap_or_else(|| "occupé".into());
        let _ = tx.send(r);
    }));
    if !essai_poster(WM_ESSAI, 0) {
        return "pas de fil".into();
    }
    rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap_or_else(|_| "délai".into())
}

type Demande = Box<dyn FnOnce() + Send>;
static ESSAI_DEMANDE: Mutex<Option<Demande>> = Mutex::new(None);
const WM_ESSAI: u32 = WM_APP + 2;

/// (appelé par la boucle des messages : voir `proc_messages`)
pub fn essai_traiter() {
    let d = verrou(&ESSAI_DEMANDE).take();
    if let Some(d) = d {
        d();
    }
}

/// Le point de la souris à l'écran.
pub fn souris_ecran() -> POINT {
    let mut p = POINT::default();
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut p);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn souris() {
        let liens = vec![((2 << 8) | 4, 7100), (5, 7111)];
        // (Ctrl + latéral arrière : avalé et signalé ; son relâchement avalé aussi)
        assert_eq!(decision_souris(0x020B, 1, 2, &liens, 0), (true, Some(7100), 4));
        assert_eq!(decision_souris(0x020C, 1, 2, &liens, 4), (true, None, 0));
        // (sans Ctrl : laissé au jeu)
        assert_eq!(decision_souris(0x020B, 1, 0, &liens, 0), (false, None, 0));
        assert_eq!(decision_souris(0x020B, 2, 0, &liens, 0), (true, Some(7111), 5));
        assert_eq!(decision_souris(0x0207, 0, 0, &liens, 0), (false, None, 0));
        assert_eq!(decision_souris(0x0200, 0, 0, &liens, 3), (false, None, 3));
    }

    #[test]
    fn touches_simulees() {
        // (Ctrl + Maj + A : Ctrl, Maj, puis A ; A seul : A)
        assert_eq!(touches_a_appuyer(6, 0x41), vec![0x11, 0x10, 0x41]);
        assert_eq!(touches_a_appuyer(0, 0x41), vec![0x41]);
    }

    #[test]
    fn themes() {
        assert!(theme("terminal").carre);
        assert_eq!(theme("retro").p, Couleur::hex("#FBD000"));
        assert_eq!(theme("").p, Couleur::hex("#00E5FF"));
    }
}
