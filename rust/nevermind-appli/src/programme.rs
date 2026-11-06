//! Le programme Mindset.exe de Nevermind : tout est branché ici.
//!
//! 1. L'identité `JordanJacquot.Nevermind`, les arguments (`-Demarrage`), le démarrage (vie\demarrage.rs : copie
//!    bloquée, copie gérée lancée à la main, une seule copie, dossier des données).
//! 2. Le contexte des blocs (version, dossier de la copie), le thème (le réglage, ou celui de la suite pour une copie
//!    gérée), l'hôte des blocs et son état de départ.
//! 3. Tauri : la page et ses trois commandes, les protocoles `pack` (les images du thème) et `fichier` (les images
//!    déclarées par les blocs), la fenêtre « principale » créée une seule fois, cachée et sans prendre la main.
//! 4. Deux fils :
//!    - le fil de la fenêtre (celui de Tauri) : la vie de l'appli, pas de 100 ms (veille de la page toutes les
//!      400 ms, demandes de Mindset toutes les 600 ms, thème de la suite toutes les 2,5 s) ; il ne fait jamais
//!      attendre la fenêtre ;
//!    - le fil des blocs : l'hôte et tous les blocs (pages, gestes, effets, et la minuterie de l'écran toutes les
//!      200 ms avec les tics des blocs). Un bloc lent (l'arrêt de la mesure des FPS peut prendre 8 s) ne fige
//!      jamais la fenêtre. Les deux fils se parlent par des files (ordres vers les blocs, demandes vers la fenêtre).
//! 5. La fin : les blocs fermés (sur leur fil, attendu au plus 10 s), la relance si le thème a changé, puis
//!    `std::process::exit` (aucun fil ne garde le processus en vie sans fenêtre).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::LocalKey;
use std::time::{Duration, Instant};

use nevermind_noyau::Contexte;
use serde_json::Value;
use tauri::{App, AppHandle, Manager, Window, WindowEvent};

use crate::commandes::Page;
use crate::fenetre::{LABEL, TAILLE_MIN, creer_principale};
use crate::hote::{Demande, Ecran, Hote};
use crate::pack::{self, Images};
use crate::pont::MessagePage;
use crate::vie::demarrage::{self, AFFICHER_PAGE, Depart, Environnement, Noms};
use crate::vie::surveillance::{Evenement, Fin, Reglages, Vie};
use crate::vie::{Fenetre, Systeme, Taille};
use crate::windows::fenetre::FenetreTauri;
use crate::windows::systeme::SystemeWindows;
use crate::windows::{compte, identite, registre, reseau, services, webview2};
use crate::{VERSION, theme};

/// Le pas de la vie de l'appli (fil de la fenêtre).
const PAS: Duration = Duration::from_millis(100);
/// La minuterie de l'écran (`Step-MinuterieKit`, accueil-console.ps1:479), sur le fil des blocs.
const PAS_ECRAN: Duration = Duration::from_millis(200);
/// Le signal de Mindset : levé, il relit `afficher-page.txt` (`Request-MenuMindset`).
const SIGNAL_MINDSET: &str = r"Local\Mindset-Afficher";
/// La fermeture des blocs est attendue au plus tant (la mesure des FPS peut prendre 8 s à s'arrêter).
const ATTENTE_FERMETURE: Duration = Duration::from_secs(10);

/// Ce que le fil de la fenêtre demande au fil des blocs.
enum Ordre {
    Message(MessagePage),
    Regardee(bool),
    Accrocher(bool),
    /// Le clic sur une bulle de Mindset (`notif:<id>`)
    Bulle(String),
    /// Le thème de la suite à enregistrer (`Set-Setting 'Theme'`) avant la relance
    Theme(String),
    Journal(String),
    /// Fermer les blocs, puis répondre
    Fermer(Sender<()>),
}

struct Etat {
    vie: Vie,
    systeme: SystemeWindows,
    fenetre: Option<FenetreTauri>,
    debut: Instant,
    fin: Option<Fin>,
    /// L'hôte, jusqu'à ce que son fil démarre (avec la fenêtre)
    hote: Option<Hote>,
    demandes: Option<Receiver<Vec<Demande>>>,
    app: Option<AppHandle>,
    programme: Option<PathBuf>,
    dossier_donnees: PathBuf,
}

thread_local! {
    static ETAT: RefCell<Option<Etat>> = const { RefCell::new(None) };
}

/// La file des ordres vers le fil des blocs (la page y dépose ses messages depuis n'importe quel fil).
static ORDRES: OnceLock<Sender<Ordre>> = OnceLock::new();

fn ordonner(o: Ordre) {
    if let Some(t) = ORDRES.get() {
        let _ = t.send(o);
    }
}

/// La page de Tauri, vue de l'hôte.
struct EcranPage(AppHandle);

impl Ecran for EcranPage {
    fn etat(&mut self, etat: Value) {
        self.0.state::<Page>().changer(etat);
    }
    fn ordre(&mut self, ordre: Value) {
        self.0.state::<Page>().ordre(ordre);
    }
}

/// Le dossier du programme (celui de Mindset.exe).
fn dossier_programme() -> Option<String> {
    std::env::current_exe().ok()?.parent()?.to_str().map(str::to_string)
}

#[cfg(not(feature = "essai"))]
fn environnement() -> (Environnement, Noms) {
    let v = |n: &str| std::env::var(n).ok().filter(|x| !x.is_empty());
    let local = v("LOCALAPPDATA").unwrap_or_default();
    let env = Environnement {
        racine: dossier_programme(),
        fichier_suite: PathBuf::from(&local).join(r"Mindset\suite.json"),
        localappdata: local,
        program_files: v("ProgramFiles"),
        program_w6432: v("ProgramW6432"),
        version: VERSION.into(),
        donnees_essai: None,
        disques: ('C'..='Z').collect(),
    };
    (env, Noms::nevermind())
}

/// Construction d'essai : tout vient du dossier d'essai, rien des vraies données ; sans lui, rien ne se fait.
#[cfg(feature = "essai")]
fn environnement() -> (Environnement, Noms) {
    let Ok(e) = crate::essai::lire() else { std::process::exit(4) };
    let env = Environnement {
        racine: dossier_programme(),
        fichier_suite: e.local.join(r"Mindset\suite.json"),
        localappdata: e.local.to_string_lossy().into_owned(),
        program_files: None,
        program_w6432: None,
        version: VERSION.into(),
        donnees_essai: Some(e.donnees.clone()),
        disques: Vec::new(),
    };
    (env, e.noms())
}

/// `Write-LaunchNote` (Nevermind.ps1:77-79) : une ligne au journal avant que le contexte existe.
fn note_lancement(dossier: &Path, texte: &str) {
    use std::io::Write;
    let _ = std::fs::create_dir_all(dossier);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dossier.join("journal.txt")) {
        let _ = write!(f, "{} {texte} (processus {})\r\n", nevermind_noyau::contexte::horodatage(), std::process::id());
    }
}

/// Une ligne au journal, depuis le fil de la fenêtre ou un protocole (sans le contexte, qui vit avec les blocs).
fn journal_dans(dossier: &Path, texte: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dossier.join("journal.txt")) {
        let _ = write!(f, "{} {texte}\r\n", nevermind_noyau::contexte::horodatage());
    }
}

/// Le dossier des données de la suite, où va `WebView2-Tauri\Nevermind` : pour une copie gérée, celui qui contient le
/// dossier de Nevermind ; sinon son propre dossier.
fn donnees_suite(gere: bool, donnees: &Path) -> PathBuf {
    match donnees.parent() {
        Some(p) if gere => p.to_path_buf(),
        _ => donnees.to_path_buf(),
    }
}

/// Le programme : ne revient jamais (`std::process::exit`).
pub fn lancer() -> ! {
    let _ = identite::poser();
    let arguments = demarrage::lire_arguments(std::env::args().skip(1));
    let (env, noms) = environnement();
    let mut systeme = SystemeWindows::nouveau();
    let l = match demarrage::demarrer(arguments, &env, &noms, &mut systeme, &mut note_lancement) {
        Depart::Lancer(l) => *l,
        _ => std::process::exit(0),
    };
    let essai = cfg!(feature = "essai");
    let racine = env.racine.as_deref().map(PathBuf::from).unwrap_or_default();
    let ctx = Contexte::nouveau(racine.clone(), l.dossier_donnees.clone(), l.gere, essai);
    // (le noyau les attend avant la première demande à l'Aide)
    ctx.definir_version(VERSION);
    ctx.definir_racine(racine);
    if let Some(n) = &l.note_donnees {
        ctx.journal(n);
    }
    // Le thème (themes.ps1:30-176)
    let packs = l.dossier_donnees.join("packs");
    let reglage = match ctx.reglage("Theme") {
        Some(Value::String(t)) => t,
        _ => "neon".to_string(),
    };
    let choix = theme::choisir(&reglage, l.gere, &l.suite, &packs);
    if let Some(t) = &choix.a_ecrire {
        let _ = ctx.ecrire_reglage("Theme", Value::String(t.clone()), false);
    }
    for j in &choix.journal {
        ctx.journal(j);
    }
    let build =
        registre::lire_hklm(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuildNumber").unwrap_or_default();
    ctx.journal(&format!(
        "Démarrage Nevermind {VERSION} (Rust, Windows build {build}){}",
        if arguments.demarrage { ", lancé avec Windows" } else { "" }
    ));
    let utilisateur = std::env::var("USERNAME").unwrap_or_default();
    let prenom = theme::prenom(compte::nom_complet(&utilisateur).as_deref(), &utilisateur);
    let phrase = theme::phrase(&choix.theme, &prenom);
    let images = Images::default();
    let journal_page = {
        let d = l.dossier_donnees.clone();
        move |t: &str| journal_dans(&d, t)
    };
    let mut hote = Hote::nouveau(
        crate::blocs(),
        ctx,
        theme::Habillage::depuis(&choix.theme),
        phrase,
        VERSION,
        images.clone(),
        Box::new(reseau::connexion),
    );
    hote.etat_service = Box::new(services::etat);
    if essai {
        hote.dossier_etat = Some(l.dossier_donnees.clone());
    }
    let page = Page::new(hote.etat_initial(Duration::ZERO), journal_page.clone());
    page.recevoir(|m| ordonner(Ordre::Message(m)));
    let date_suite = std::fs::metadata(&l.fichier_suite).ok().and_then(|m| m.modified().ok());
    let reglages = Reglages {
        gere: l.gere,
        fichier_suite: l.fichier_suite.clone(),
        dossier_mindset: l.dossier_mindset.clone(),
        dossier_donnees: l.dossier_donnees.clone(),
        packs,
        taille_min: Taille { largeur: TAILLE_MIN.0, hauteur: TAILLE_MIN.1 },
        theme: choix.cle.clone(),
        date_suite,
    };
    ETAT.with(|e| {
        *e.borrow_mut() = Some(Etat {
            vie: Vie::nouvelle(reglages),
            systeme,
            fenetre: None,
            debut: Instant::now(),
            fin: None,
            hote: Some(hote),
            demandes: None,
            app: None,
            programme: l.programme.clone(),
            dossier_donnees: l.dossier_donnees.clone(),
        })
    });

    let dossier_pack = choix.theme.pack.as_ref().map(|p| p.dossier.clone());
    let pour_pack = journal_page.clone();
    let pour_fenetre = journal_page.clone();
    let montree = !arguments.demarrage;
    let suite = donnees_suite(l.gere, &l.dossier_donnees);
    let donnees_appli = l.dossier_donnees.clone();
    let constructeur = crate::preparer(tauri::Builder::default(), page)
        .register_uri_scheme_protocol("pack", move |_, demande| {
            let chemin = demande.uri().path();
            let (reponse, refus) = pack::repondre_pack(dossier_pack.as_deref(), chemin);
            if let Some(r) = refus {
                pour_pack(&format!(
                    "Thème : fichier refusé ({}) : {}",
                    r.raison(),
                    chemin.chars().take(120).collect::<String>()
                ));
            }
            reponse
        })
        .register_uri_scheme_protocol("fichier", move |_, demande| images.repondre(demande.uri().path()))
        .setup(move |app| {
            ouvrir_fenetre(app, &suite, &donnees_appli, montree, &pour_fenetre);
            Ok(())
        })
        .on_window_event(evenement_fenetre);
    match constructeur.build(crate::contexte()) {
        Ok(app) => {
            let _ = app.run_return(|_, _| {});
        }
        Err(e) => {
            journal_page(&format!("Écran impossible : Tauri n'a pas pu démarrer ({e})"));
            ecran_impossible();
        }
    }
    finir()
}

/// `Show-EcranImpossible` (accueil-console.ps1:399-422) : un message simple, puis la fermeture.
fn ecran_impossible() {
    #[cfg(not(feature = "essai"))]
    crate::windows::message::informer(
        "Nevermind n'a pas pu afficher son écran.\n\nRedémarre le PC. Si ça continue, réinstalle Nevermind depuis Mindset.",
        crate::NOM_APPLI,
    );
}

/// Crée la fenêtre principale (une seule fois), coupe les réglages de WebView2, la montre si elle n'est pas lancée
/// cachée, puis démarre le fil des blocs et les surveillances.
fn ouvrir_fenetre(
    app: &App,
    suite: &Path,
    donnees: &Path,
    montree: bool,
    journal: &(impl Fn(&str) + Clone + Send + Sync + 'static),
) {
    if app.get_webview_window(LABEL).is_some() {
        return;
    }
    let pour_page = journal.clone();
    let (f, choix) = match creer_principale(app, suite, donnees, std::sync::Arc::new(move |t: &str| pour_page(t))) {
        Ok(x) => x,
        Err(e) => {
            journal(&format!("Écran impossible : la fenêtre n'a pas pu être créée ({e})"));
            ecran_impossible();
            app.handle().exit(1);
            return;
        }
    };
    journal(&format!("Écran : dossier WebView2 {}", choix.dossier.display()));
    if let Some(r) = &choix.raison {
        journal(&format!("Écran : dossier WebView2 de la suite non pris, repli ({r})"));
    }
    let h = f.hwnd().map(|h| h.0 as usize).unwrap_or(0);
    if !crate::windows::fenetre::garder_taille_minimale(h) {
        journal("Écran : taille minimale non gardée (sous-classe refusée)");
    }
    let pour_reglages = journal.clone();
    if let Err(e) = webview2::couper_reglages(&f, move |t| pour_reglages(t)) {
        journal(&format!("Écran : réglages de WebView2 non posés ({e})"));
    }
    if montree {
        let _ = f.show();
        let _ = f.set_focus();
    }
    let pour_fenetre = journal.clone();
    let fenetre = FenetreTauri::nouvelle(f, h, move |t| pour_fenetre(t));
    ETAT.with(|e| {
        let mut e = e.borrow_mut();
        let Some(etat) = e.as_mut() else { return };
        etat.fenetre = Some(fenetre);
        etat.app = Some(app.handle().clone());
        etat.debut = Instant::now();
        if let Some(hote) = etat.hote.take() {
            let (to, ro) = mpsc::channel();
            let (td, rd) = mpsc::channel();
            let _ = ORDRES.set(to);
            etat.demandes = Some(rd);
            let a = app.handle().clone();
            let r = std::thread::Builder::new().name("blocs".into()).spawn(move || fil_des_blocs(hote, a, ro, td));
            if let Err(x) = r {
                journal(&format!("Écran impossible : le fil des blocs n'a pas pu démarrer ({x})"));
            }
        }
    });
    minuterie(app.handle().clone());
}

/// Le fil des blocs : l'hôte, ses ordres, sa minuterie de 200 ms. Il finit quand la file des ordres se ferme, ou
/// sur `Ordre::Fermer`.
fn fil_des_blocs(mut hote: Hote, app: AppHandle, ordres: Receiver<Ordre>, demandes: Sender<Vec<Demande>>) {
    let mut ecran = EcranPage(app);
    let debut = Instant::now();
    let mut prochain = PAS_ECRAN;
    loop {
        let t = debut.elapsed();
        let d = match ordres.recv_timeout(prochain.saturating_sub(t)) {
            Ok(Ordre::Message(m)) => hote.recevoir(m, t, &mut ecran),
            Ok(Ordre::Regardee(v)) => hote.regardee(v, t, &mut ecran),
            Ok(Ordre::Accrocher(v)) => hote.accrocher(v, t, &mut ecran),
            Ok(Ordre::Bulle(id)) => {
                hote.bulle_cliquee(&id, t, &mut ecran);
                Vec::new()
            }
            Ok(Ordre::Theme(th)) => {
                let _ = hote.ctx.ecrire_reglage("Theme", Value::String(th), false);
                Vec::new()
            }
            Ok(Ordre::Journal(j)) => {
                hote.ctx.journal(&j);
                Vec::new()
            }
            Ok(Ordre::Fermer(fini)) => {
                hote.fermer();
                let _ = fini.send(());
                return;
            }
            Err(RecvTimeoutError::Timeout) => {
                while prochain <= t {
                    prochain += PAS_ECRAN;
                }
                hote.minuterie(t, &mut ecran)
            }
            Err(RecvTimeoutError::Disconnected) => {
                hote.fermer();
                return;
            }
        };
        if !d.is_empty() {
            let _ = demandes.send(d);
        }
    }
}

/// La minuterie : toutes les 100 ms, un tour demandé au fil de la fenêtre.
fn minuterie(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(PAS);
            let a = app.clone();
            if app.run_on_main_thread(move || tour(&a)).is_err() {
                break;
            }
        }
    });
}

impl Etat {
    fn journal(&self, t: &str) {
        journal_dans(&self.dossier_donnees, t);
    }

    /// Ce que l'hôte demande au programme.
    fn demandes(&mut self, d: Vec<Demande>) {
        for x in d {
            match x {
                Demande::Quitter => {
                    if let Some(f) = self.vie.quitter() {
                        self.journal("Fermeture");
                        self.fin = Some(f);
                    }
                }
                Demande::Reduire => {
                    if let Some(f) = self.fenetre.as_mut() {
                        // (géré par Mindset : rangé par Mindset ; sinon dans la barre des tâches)
                        if self.vie.reglages().gere { f.cacher() } else { f.reduire() }
                    }
                }
                Demande::MenuMindset => {
                    let d = self.vie.reglages().dossier_mindset.clone();
                    let ligne = match std::fs::create_dir_all(&d)
                        .and_then(|()| std::fs::write(d.join(AFFICHER_PAGE), "menu"))
                    {
                        Ok(()) => {
                            self.systeme.lever_signal(SIGNAL_MINDSET);
                            "Retour au menu de Mindset demandé".to_string()
                        }
                        Err(e) => format!("Retour au menu de Mindset impossible ({e})"),
                    };
                    self.journal(&ligne);
                }
                Demande::OuvrirAdresse(a) => {
                    let permise = a.starts_with("https://")
                        || a.starts_with("http://")
                        || a.starts_with("ms-settings:")
                        || Path::new(&a).is_absolute();
                    if !permise || self.systeme.ouvrir_par_explorateur(&a).is_err() {
                        self.journal(&format!("Ouverture impossible : {}", a.chars().take(120).collect::<String>()));
                    }
                }
                Demande::Partie(oui) => self.vie.partie = oui,
            }
        }
    }

    /// Un pas de la vie de l'appli.
    fn tour(&mut self) {
        let t = self.debut.elapsed();
        if let Some(r) = &self.demandes {
            let recues: Vec<Vec<Demande>> = r.try_iter().collect();
            for d in recues {
                self.demandes(d);
            }
        }
        if self.fin.is_some() {
            return;
        }
        let Etat { vie, systeme, fenetre, dossier_donnees, .. } = self;
        let Some(f) = fenetre.as_mut() else { return };
        let d = dossier_donnees.clone();
        let evenements = vie.avancer(t, f, systeme, &mut |x| journal_dans(&d, x));
        for ev in evenements {
            match ev {
                Evenement::Accrochee(on) => ordonner(Ordre::Accrocher(on)),
                Evenement::Regardee(on) => ordonner(Ordre::Regardee(on)),
                Evenement::Bulle(id) => ordonner(Ordre::Bulle(id)),
                Evenement::Theme(th) => ordonner(Ordre::Theme(th)),
                Evenement::Fin(fin) => {
                    if self.fin.is_none() {
                        ordonner(Ordre::Journal("Fermeture".into()));
                        self.fin = Some(fin);
                    }
                }
            }
        }
    }
}

fn tour(app: &AppHandle) {
    let fin = ETAT.with(|e| {
        let mut e = e.try_borrow_mut().ok()?;
        let etat = e.as_mut()?;
        if etat.fin.is_none() {
            etat.tour();
            #[cfg(feature = "essai")]
            essai_demandes(etat);
        }
        etat.fin
    });
    if fin.is_some() {
        app.exit(0);
    }
}

/// La croix, Alt + F4, ou `WM_CLOSE` venu de Mindset (son bouton « Fermer ») : Nevermind quitte (contrat-appli :
/// « Fermer » de Mindset doit faire quitter l'appli). La fenêtre n'est jamais détruite par Tauri.
fn evenement_fenetre(w: &Window, ev: &WindowEvent) {
    match ev {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            croix(w.app_handle());
        }
        WindowEvent::Destroyed => w.app_handle().exit(0),
        _ => {}
    }
}

fn croix(app: &AppHandle) {
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        let r = ETAT.with(|e| {
            let Ok(mut e) = e.try_borrow_mut() else { return Err(()) };
            let Some(etat) = e.as_mut() else { return Ok(false) };
            etat.demandes(vec![Demande::Quitter]);
            Ok(etat.fin.is_some())
        });
        match r {
            Ok(true) => a.exit(0),
            Ok(false) => {}
            Err(()) => croix(&a),
        }
    });
}

/// Construction d'essai : les messages demandés (`actions.demande`, un JSON par ligne, comme ceux du kit ; ou un
/// ordre `{ "kit": ... }` envoyé tel quel), et la capture demandée (`capture.demande`).
#[cfg(feature = "essai")]
fn essai_demandes(etat: &mut Etat) {
    use crate::essai::{ACTIONS_DEMANDE, CAPTURE, CAPTURE_DEMANDE};
    let demande = etat.dossier_donnees.join(ACTIONS_DEMANDE);
    if let Ok(texte) = std::fs::read_to_string(&demande) {
        let _ = std::fs::remove_file(&demande);
        for ligne in texte.trim_start_matches('\u{feff}').lines().map(str::trim).filter(|l| !l.is_empty()) {
            let Ok(v) = serde_json::from_str::<Value>(ligne) else { continue };
            if v.get("kit").is_some() {
                if let Some(app) = etat.app.clone() {
                    app.state::<Page>().ordre(v);
                }
                continue;
            }
            match crate::pont::lire_message(&v) {
                Ok(m) => ordonner(Ordre::Message(m)),
                Err(r) => etat.journal(&format!("Essai : demande refusée ({})", r.raison())),
            }
        }
    }
    let demande = etat.dossier_donnees.join(CAPTURE_DEMANDE);
    if demande.is_file() {
        let _ = std::fs::remove_file(&demande);
        let Some(f) = etat.fenetre.as_ref() else { return };
        let png = crate::windows::capture::capturer(f.hwnd())
            .and_then(|(l, h, px)| crate::png::encoder(l, h, &px).map(|p| (l, h, p)));
        match png {
            Some((l, h, p)) => match std::fs::write(etat.dossier_donnees.join(CAPTURE), p) {
                Ok(()) => etat.journal(&format!("Essai : capture de {l} x {h}")),
                Err(e) => etat.journal(&format!("Essai : capture non écrite ({e})")),
            },
            None => etat.journal("Essai : capture impossible"),
        }
    }
}

/// La fin (Nevermind.ps1:481-496) : les blocs fermés sur leur fil, la relance si le thème a changé (verrous rendus
/// d'abord), puis `std::process::exit`.
fn finir() -> ! {
    let (t, r) = mpsc::channel();
    ordonner(Ordre::Fermer(t));
    let fermes = r.recv_timeout(ATTENTE_FERMETURE).is_ok();
    ETAT.with(|e| {
        if let Ok(mut e) = e.try_borrow_mut()
            && let Some(etat) = e.as_mut()
        {
            if etat.fin.is_none() {
                etat.journal("Fermeture : la fenêtre a disparu");
            }
            if !fermes && ORDRES.get().is_some() {
                etat.journal("Fermeture : les blocs n'ont pas fini en 10 s");
            }
            if let Some(Fin::Relancer { cachee }) = etat.fin {
                let d = etat.dossier_donnees.clone();
                let Etat { vie, systeme, programme, .. } = etat;
                vie.relancer(cachee, programme.as_deref(), systeme, &mut |x| journal_dans(&d, x));
            }
            etat.systeme.liberer_verrous();
        }
    });
    quitter_sans_rien_lacher(&ETAT, 0)
}

/// Quitte le processus sans lâcher l'état du fil qui quitte (voir Mindcare en Rust : lâcher là des fils déjà
/// arrêtés par Windows pouvait bloquer la fin pour toujours).
pub fn quitter_sans_rien_lacher<T: 'static>(etat: &'static LocalKey<RefCell<Option<T>>>, code: i32) -> ! {
    let reste = etat.with(|e| e.try_borrow_mut().ok().and_then(|mut e| e.take()));
    std::mem::forget(reste);
    std::process::exit(code)
}
