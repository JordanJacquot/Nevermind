//! Essai de la vraie barre flottante, de la vue et des raccourcis de l'Organizer, sur un BUREAU WINDOWS À PART
//! seulement (outils\essai-organizer.ps1 le lance par rust\nevermind-appli\outils\sur-bureau.ps1). Il refuse de
//! tourner sur le bureau affiché.
//!
//! Les fenêtres « du jeu » sont les siennes : des copies de lui-même (`organizer_essai fenetre <titre>`), une fenêtre
//! chacune, aux titres semblables à ceux du jeu. Jamais le vrai jeu, jamais les fenêtres de Jordan. Les raccourcis pris
//! sont impossibles à taper (Ctrl+Alt+Maj+Win avec F13 à F24) ; l'écoute globale de la souris n'est jamais posée.
//! Les appuis de raccourci sont des messages WM_HOTKEY postés au fil de l'Organizer.
//!
//!   organizer_essai <dossier de sortie>
#![windows_subsystem = "windows"]
#![allow(unsafe_code)]

#[cfg(windows)]
fn main() {
    essai::main();
}

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
mod essai {
    use nevermind_jeux::organizer::fil::{self, Commande, Params, essai as e};
    use nevermind_jeux::organizer::veille::Bureau;
    use std::fmt::Write as _;
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::{FILETIME, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::StationsAndDesktops::{
        DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS, GetThreadDesktop, GetUserObjectInformationW, OpenInputDesktop,
        UOI_NAME,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentThreadId, GetProcessTimes};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        HOT_KEY_MODIFIERS, RegisterHotKey, UnregisterHotKey, VK_F20, VK_F22,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW, MSG,
        PostMessageW, RegisterClassExW, WINDOW_EX_STYLE, WM_HOTKEY, WM_KEYDOWN, WM_XBUTTONDOWN, WNDCLASSEXW,
        WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };
    use windows::core::{HSTRING, PCWSTR, w};

    fn nom_bureau(h: HANDLE) -> String {
        let mut b = [0u16; 256];
        let mut n = 0u32;
        unsafe {
            let _ = GetUserObjectInformationW(h, UOI_NAME, Some(b.as_mut_ptr() as *mut _), 512, Some(&mut n));
        }
        let k = b.iter().position(|&c| c == 0).unwrap_or(0);
        String::from_utf16_lossy(&b[..k])
    }

    /// Le bureau de ce fil, et celui affiché à l'écran.
    fn bureaux() -> (String, String) {
        unsafe {
            let moi = GetThreadDesktop(GetCurrentThreadId()).map(|d| nom_bureau(HANDLE(d.0))).unwrap_or_default();
            let ecran = OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS)
                .map(|d| nom_bureau(HANDLE(d.0)))
                .unwrap_or_default();
            (moi, ecran)
        }
    }

    #[link(name = "imm32")]
    unsafe extern "system" {
        fn ImmDisableIME(fil: u32) -> i32;
    }

    unsafe extern "system" fn proc_fenetre(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(h, m, wp, lp) }
    }

    /// Une fenêtre de ce programme, le temps que Windows y pose ses fenêtres de saisie.
    fn premiere_fenetre() {
        use windows::Win32::UI::WindowsAndMessaging::{PM_REMOVE, PeekMessageW, TranslateMessage};
        unsafe {
            let inst = GetModuleHandleW(PCWSTR::null()).unwrap();
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(proc_fenetre),
                hInstance: inst.into(),
                lpszClassName: w!("OrganizerEssaiPremiere"),
                ..Default::default()
            };
            RegisterClassExW(&wc);
            let _ = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("OrganizerEssaiPremiere"),
                w!("Essai"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                0,
                0,
                200,
                100,
                None,
                None,
                Some(inst.into()),
                None,
            );
            let fin = Instant::now() + Duration::from_millis(800);
            while Instant::now() < fin {
                let mut m = MSG::default();
                while PeekMessageW(&mut m, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&m);
                    DispatchMessageW(&m);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    /// Une fenêtre « du jeu » d'essai, avec ce titre, jusqu'à ce que l'essai arrête ce programme.
    fn fenetre(titre: &str) {
        unsafe {
            // (sans les fenêtres de saisie de Windows : sur un bureau neuf, elles passeraient pour la fenêtre principale)
            ImmDisableIME(u32::MAX);
            let inst = GetModuleHandleW(PCWSTR::null()).unwrap();
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(proc_fenetre),
                hInstance: inst.into(),
                lpszClassName: w!("OrganizerEssaiFenetre"),
                ..Default::default()
            };
            RegisterClassExW(&wc);
            let _ = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("OrganizerEssaiFenetre"),
                &HSTRING::from(titre),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                640,
                400,
                None,
                None,
                Some(inst.into()),
                None,
            );
            let mut m = MSG::default();
            while GetMessageW(&mut m, None, 0, 0).0 > 0 {
                DispatchMessageW(&m);
            }
        }
    }

    /// Diagnostic : les fenêtres de premier niveau de ces programmes (visible, propriétaire, classe, titre).
    fn lister(pids: &[u32]) -> String {
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GW_OWNER, GetClassNameW, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        };
        struct R<'a> {
            pids: &'a [u32],
            l: Vec<String>,
        }
        unsafe extern "system" fn f(h: HWND, lp: LPARAM) -> windows::core::BOOL {
            let r = unsafe { &mut *(lp.0 as *mut R) };
            let mut p = 0u32;
            unsafe { GetWindowThreadProcessId(h, Some(&mut p)) };
            if r.pids.contains(&p) {
                let mut t = [0u16; 200];
                let n = unsafe { GetWindowTextW(h, &mut t) };
                let mut c = [0u16; 200];
                let k = unsafe { GetClassNameW(h, &mut c) };
                let o = unsafe { GetWindow(h, GW_OWNER) }.map(|o| o.0 as isize).unwrap_or(0);
                r.l.push(format!(
                    "  pid {p} hwnd {} visible {} propriétaire {o} classe {} titre {:?}",
                    h.0 as isize,
                    unsafe { IsWindowVisible(h) }.as_bool(),
                    String::from_utf16_lossy(&c[..k.max(0) as usize]),
                    String::from_utf16_lossy(&t[..n.max(0) as usize])
                ));
            }
            windows::core::BOOL(1)
        }
        let mut r = R { pids, l: Vec::new() };
        unsafe {
            let _ = EnumWindows(Some(f), LPARAM(&mut r as *mut _ as isize));
        }
        r.l.join(
            "
",
        )
    }

    fn temps_processeur() -> f64 {
        let (mut a, mut b, mut k, mut u) =
            (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        unsafe {
            let _ = GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut k, &mut u);
        }
        let v = |f: FILETIME| ((u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)) as f64 / 1e7;
        v(k) + v(u)
    }

    /// Le processeur pris par ce programme pendant `s` secondes, en % d'un cœur.
    fn mesurer(s: u64) -> f64 {
        let t0 = temps_processeur();
        let i = Instant::now();
        std::thread::sleep(Duration::from_secs(s));
        (temps_processeur() - t0) / i.elapsed().as_secs_f64() * 100.0
    }

    pub fn main() {
        let a: Vec<String> = std::env::args().collect();
        if a.get(1).map(String::as_str) == Some("fenetre") {
            fenetre(a.get(2).map(String::as_str).unwrap_or(""));
            return;
        }
        let sortie = PathBuf::from(a.get(1).cloned().unwrap_or_else(|| ".".into()));
        let _ = std::fs::create_dir_all(&sortie);
        {
            // (une erreur du programme est écrite dans panique.txt : sans console, elle ne se verrait pas)
            let f = sortie.join("panique.txt");
            std::panic::set_hook(Box::new(move |i| {
                let _ = std::fs::OpenOptions::new().create(true).append(true).open(&f).and_then(|mut o| {
                    use std::io::Write;
                    writeln!(
                        o,
                        "{i}
{}",
                        std::backtrace::Backtrace::force_capture()
                    )
                });
            }));
        }
        let mut r = String::new();
        let (moi, ecran) = bureaux();
        let _ = writeln!(r, "bureau de l'essai : {moi:?}, bureau à l'écran : {ecran:?}");
        // Sécurité : jamais sur le bureau affiché (ni sur celui par défaut)
        if moi.is_empty() || moi.eq_ignore_ascii_case("Default") || moi.eq_ignore_ascii_case(&ecran) {
            let _ = writeln!(r, "REFUS : ce n'est pas un bureau à part, rien n'est lancé.");
            let _ = std::fs::write(sortie.join("resultats.txt"), r);
            std::process::exit(2);
        }
        let dossier = sortie.join("donnees");
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(&dossier).unwrap();
        let m = "Ctrl+Alt+Maj+Win+";
        let config = format!(
            r#"{{"On":true,"Bar":true,"BarAlways":false,"Order":["Alice"],"Hotkeys":{{"Next":"{m}F23","Prev":"{m}F24","P1":"{m}F13","P2":"{m}F14","P3":"{m}F20","P4":"{m}Souris4","P5":"","P6":"","P7":"","P8":""}}}}"#
        );
        std::fs::write(dossier.join("organizer.json"), config).unwrap();
        // Un autre programme qui a déjà pris Ctrl+Alt+Maj+Win+F20 (ce fil) : l'Organizer doit le dire
        let pris =
            unsafe { RegisterHotKey(None, 1, HOT_KEY_MODIFIERS(1 | 2 | 4 | 8 | 0x4000), u32::from(VK_F20.0)) }.is_ok();
        let _ = writeln!(r, "touche prise d'avance par l'essai : {pris}");
        // Les fenêtres « du jeu » : des copies de ce programme
        // Le premier programme d'un bureau neuf reçoit les fenêtres de l'indicateur de saisie de Windows : ce sera
        // celui-ci (une fenêtre à lui d'abord), pas une fenêtre « du jeu »
        premiere_fenetre();
        // Les fenêtres « du jeu » : une copie de ce programme sous un autre nom (lui seul est cherché)
        let exe = sortie.join("organizer_essai_fenetre.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let mut enfants = Vec::new();
        for t in ["Bob - Iop - 3.0.12 - Release", "Alice - Cra - 3.1.4 - Release", "Dofus"] {
            if let Ok(c) = std::process::Command::new(&exe).args(["fenetre", t]).creation_flags(0x0800_0000).spawn() {
                enfants.push(c);
            }
        }
        for c in &enfants {
            let _ = writeln!(r, "enfant pid {}", c.id());
        }
        let _ = writeln!(
            r,
            "fenêtres de premier niveau des enfants :
{}",
            lister(&enfants.iter().map(|c| c.id()).collect::<Vec<_>>())
        );
        std::thread::sleep(Duration::from_millis(1500));
        let devant = Arc::new(AtomicIsize::new(0));
        let programme = exe.file_stem().unwrap().to_string_lossy().to_string();
        let (partage, fil) = e::lancer(Params {
            donnees: dossier.clone(),
            theme: String::new(),
            vue: true,
            programme,
            essai: true,
            devant_essai: Some(devant.clone()),
        });
        std::thread::sleep(Duration::from_millis(2500));
        let fen = e::geste(|x| {
            let f = x.bureau.fenetres();
            f.iter().map(|w| format!("{}|{}|{}|{}", w.hwnd, w.pid, w.nom, w.titre)).collect::<Vec<_>>().join("\n")
        });
        let _ = writeln!(r, "fenêtres vues :\n{fen}");
        let tous: Vec<u32> = enfants.iter().map(|c| c.id()).collect();
        let _ = writeln!(r, "fenêtres de premier niveau des enfants (après) :\n{}", lister(&tous));
        let h = |nom: &str| -> isize {
            fen.lines()
                .find(|l| l.split('|').nth(2) == Some(nom))
                .and_then(|l| l.split('|').next()?.parse().ok())
                .unwrap_or(0)
        };
        let (bob, alice) = (h("Bob"), h("Alice"));
        let etat = || {
            e::geste(|x| {
                format!(
                    "touches prises {}, barre visible {}, refusées {:?}, ordre {:?}",
                    x.veille.touches_prises,
                    x.veille.barre_visible(),
                    x.veille.refusees,
                    x.veille.config.ordre
                )
            })
        };
        let _ = writeln!(r, "avant (rien devant) : {}", etat());
        let _ = std::fs::create_dir_all(sortie.join("0"));
        let _ = writeln!(r, "captures 0 : {:?}", e::captures(&sortie.join("0")));
        // Bob devant : raccourcis pris, barre montrée
        devant.store(bob, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(800));
        let _ = writeln!(r, "Bob devant : {}", etat());
        let _ = std::fs::create_dir_all(sortie.join("1"));
        let _ = writeln!(r, "captures 1 : {:?}", e::captures(&sortie.join("1")));
        // Perso suivant (Next = 7100) : Alice ; P2 (7112) : Bob ; P1 (7111) : Alice ; Prev (7101) : Bob
        for (id, quoi) in [(7100, "suivant"), (7112, "P2"), (7111, "P1"), (7101, "précédent"), (7113, "P3 (refusée)")]
        {
            let _ = e::poster(WM_HOTKEY, id);
            std::thread::sleep(Duration::from_millis(400));
            let d = devant.load(Ordering::Relaxed);
            let n = if d == alice {
                "Alice"
            } else if d == bob {
                "Bob"
            } else {
                "?"
            };
            let vraie = unsafe { GetForegroundWindow() }.0 as isize;
            let _ = writeln!(r, "raccourci {quoi} ({id}) : devant {n} (vraie fenêtre devant de ce bureau : {vraie})");
        }
        let _ = std::fs::create_dir_all(sortie.join("2"));
        let _ = writeln!(r, "captures 2 : {:?}", e::captures(&sortie.join("2")));
        // (rien devant pendant le choix : la touche choisie n'est jamais prise à Windows)
        devant.store(0, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(500));
        // Choisir une touche dans la vue : clic (geste), puis une vraie touche envoyée à la fenêtre de la vue
        let vue = e::geste(|x| {
            x.veille.capture = Some("P5".into());
            let nevermind_jeux::organizer::fil::essai::Etat { veille, bureau, .. } = x;
            veille.montrer_vue(bureau, true);
            bureau.vue_hwnd().to_string()
        });
        let _ = std::fs::create_dir_all(sortie.join("3"));
        let _ = writeln!(r, "captures 3 (choix d'une touche) : {:?}", e::captures(&sortie.join("3")));
        if let Ok(hv) = vue.parse::<isize>() {
            unsafe {
                let _ = PostMessageW(Some(HWND(hv as *mut _)), WM_KEYDOWN, WPARAM(VK_F22.0 as usize), LPARAM(0));
            }
        }
        std::thread::sleep(Duration::from_millis(400));
        let _ = writeln!(r, "touche choisie pour P5 : {}", e::geste(|x| x.veille.config.touche("P5")));
        // (remise à « aucune » tout de suite : aucune touche simple ne doit être prise, même sur ce bureau)
        let _ = e::geste(|x| {
            x.veille.capture = Some("P5".into());
            let nevermind_jeux::organizer::fil::essai::Etat { veille, bureau, .. } = x;
            veille.choisir_touche(bureau, "");
            String::new()
        });
        let _ = writeln!(r, "P5 remise à : {:?}", e::geste(|x| x.veille.config.touche("P5")));
        // Une touche simulée : le bouton latéral avant (choisi comme dans la vue), puis la touche A choisie au clavier
        // dans la vue (un bouton de souris pendant ce choix ne compte pas). Dans un essai, rien n'est envoyé pour de
        // vrai : la touche est seulement notée.
        let _ = e::geste(|x| {
            x.veille.capture = Some("S1".into());
            let nevermind_jeux::organizer::fil::essai::Etat { veille, bureau, .. } = x;
            veille.choisir_touche(bureau, "Souris5");
            veille.capture = Some("E1".into());
            veille.montrer_vue(bureau, true);
            String::new()
        });
        if let Ok(hv) = vue.parse::<isize>() {
            unsafe {
                let _ = PostMessageW(Some(HWND(hv as *mut _)), WM_XBUTTONDOWN, WPARAM(2 << 16), LPARAM(0));
            }
            std::thread::sleep(Duration::from_millis(300));
            let _ = writeln!(
                r,
                "touche simulée, après un bouton de souris pendant le choix : {:?} (en choix : {})",
                e::geste(|x| x.veille.config.envoi("S1")),
                e::geste(|x| format!("{:?}", x.veille.capture))
            );
            unsafe {
                let _ = PostMessageW(Some(HWND(hv as *mut _)), WM_KEYDOWN, WPARAM(0x41), LPARAM(0));
            }
        }
        std::thread::sleep(Duration::from_millis(400));
        let _ = writeln!(
            r,
            "touche simulée choisie : bouton {:?}, touche {:?}",
            e::geste(|x| x.veille.config.touche("S1")),
            e::geste(|x| x.veille.config.envoi("S1"))
        );
        let _ = std::fs::create_dir_all(sortie.join("4"));
        let _ = writeln!(r, "captures 4 (touche simulée réglée) : {:?}", e::captures(&sortie.join("4")));
        devant.store(bob, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(800));
        let _ = e::poster(e::WM_SOURIS, 7131);
        std::thread::sleep(Duration::from_millis(400));
        let d = devant.load(Ordering::Relaxed);
        let _ = writeln!(
            r,
            "bouton de la touche simulée appuyé, Bob devant : envoyées {:?}, toujours Bob devant : {}",
            e::envois(),
            d == bob
        );
        devant.store(0, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(600));
        let _ = e::poster(e::WM_SOURIS, 7131);
        std::thread::sleep(Duration::from_millis(300));
        let _ = writeln!(r, "même bouton, jeu plus devant : envoyées {:?}", e::envois());
        let _ = e::geste(|x| {
            let nevermind_jeux::organizer::fil::essai::Etat { veille, bureau, .. } = x;
            veille.capture = Some("S1".into());
            veille.choisir_touche(bureau, "");
            veille.capture = Some("E1".into());
            veille.choisir_touche(bureau, "");
            String::new()
        });
        // Rien devant : raccourcis rendus, barre cachée
        devant.store(0, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(600));
        let _ = writeln!(r, "rien devant : {}", etat());
        // Le processeur : barre montrée (animation), puis cachée, puis Organizer coupé (le fil s'arrête)
        devant.store(bob, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(500));
        let _ = writeln!(r, "processeur, barre montrée et animée : {:.2} % d'un cœur", mesurer(10));
        let c = e::geste(|x| {
            let d = x.bureau.barre_cout_images(200);
            format!(
                "{:.3} ms par image, soit {:.2} % d'un cœur à 20 images par seconde",
                d.as_secs_f64() * 1000.0 / 200.0,
                d.as_secs_f64() / 200.0 * 20.0 * 100.0
            )
        });
        let _ = writeln!(r, "animation de la barre : {c}");
        let c = e::geste(|x| {
            let t = Instant::now();
            for _ in 0..200 {
                let _ = x.bureau.fenetres();
            }
            format!("{:.3} ms par recherche des fenêtres du jeu", t.elapsed().as_secs_f64() * 1000.0 / 200.0)
        });
        let _ = writeln!(r, "veille : {c}");
        devant.store(0, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(500));
        let _ = writeln!(r, "processeur, veille seule (barre cachée) : {:.2} % d'un cœur", mesurer(10));
        e::commande(&partage, Commande::Cacher);
        std::thread::sleep(Duration::from_millis(800));
        let _ = writeln!(r, "Organizer coupé et vue fermée : fil fini {}", e::fini(&partage));
        let _ = writeln!(r, "processeur, Organizer coupé : {:.2} % d'un cœur", mesurer(5));
        let _ = writeln!(
            r,
            "organizer.json : {}",
            std::fs::read_to_string(dossier.join("organizer.json")).unwrap_or_default()
        );
        let (journal, statuts) = e::sorties(&partage);
        let _ = writeln!(r, "journal :\n  {}", journal.join("\n  "));
        let _ = writeln!(r, "bulles : {statuts:?}");
        // (le fil fini : attendu au plus 5 s)
        e::commande(&partage, Commande::Quitter);
        let fin = Instant::now() + Duration::from_secs(5);
        while !fil.is_finished() && Instant::now() < fin {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = writeln!(r, "fil arrêté : {}", fil.is_finished());
        unsafe {
            let _ = UnregisterHotKey(None, 1);
        }
        for mut c in enfants {
            let _ = c.kill();
            let _ = c.wait();
        }
        let _ = fil::actif();
        let _ = std::fs::write(sortie.join("resultats.txt"), r);
    }
}
