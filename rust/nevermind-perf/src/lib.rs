//! Nevermind en Rust : le bloc « perf ».
//!
//! Ce qu'il fait (portage de ecran-perf.ps1, des mesures en direct d'onglets.ps1, de jeu.ps1 et de diagnostic-fps.ps1) :
//! - les pages Performances, Optimisation gaming, Démarrage et Nettoyage (`pages`, les mêmes que Get-PagesPerf) ;
//! - les mesures en direct, légères (`mesures`) : rien d'inutile quand l'écran n'est pas regardé ni pendant une partie ;
//! - la partie en cours : détection du jeu, mode jeu (applis fermées puis relancées), mesure des FPS par l'Aide, le
//!   compteur par-dessus le jeu, le raccourci Ctrl+Maj+F, le bilan et le diagnostic de la partie ;
//! - l'alerte de température de la carte graphique.
//!
//! Signaux envoyés : « partie-commencee », « partie-finie », « mesures », « fps-partie-enregistree »,
//! « notification », « ajouter-jeu », « declarations », « ouvrir-fiche », « diagnostic-fps-pret ».
//! Signaux attendus : « jeux-liste », « reglages-jeu », « demarrage », « nettoyage », « connexion », « analyse-pc »,
//! « memoire-constat », « diagnostic-fps », « mesure-lag », « streameur » (bloc stream : la carte du mode streameur,
//! montrée dans Performances). Le contenu de chacun est décrit dans DIVERGENCES.md.

pub mod diag;
pub mod fichier;
pub mod fps;
pub mod jeu;
pub mod mesures;
pub mod pages;
pub mod systeme;
pub mod texte;
#[cfg(windows)]
pub mod windows;

use jeu::{AlerteChaleur, EntreeIndex, Fermee};
use mesures::{Besoin, Mesures};
use nevermind_noyau::aide::Mesure;
use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use pages::{ch, liste, nb, vrai};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use systeme::{Compteur, EtatCompteur, Programme, Systeme};

/// Les jeux dont l'anti-triche refuse une mesure démarrée après eux, connus d'avance ($FpsVeilleConnus)
const VEILLE_CONNUS: [&str; 1] = ["bf6.exe"];
/// La page des gestes automatiques du mode jeu, dans « Attends la fin de « Début de partie » (Mode jeu) »
const PAGE: &str = "Mode jeu";
/// Couleurs du chiffre du compteur ($Colors : ok, warn, bad)
const VERT: u32 = 0x22D37A;
const ORANGE: u32 = 0xF5A524;
const ROUGE: u32 = 0xF04438;
/// Sans lecture réussie des FPS depuis tant de secondes, la mesure passe « à reprendre » (lot G : avant, 20 lectures
/// ratées l'arrêtaient pour toute la partie, et les refus d'une Aide en pause les faisaient en 5 s)
const SANS_LECTURE_MAX: f64 = 10.0;

/// La partie en cours ($script:GameSession)
#[derive(Debug, Clone)]
struct Session {
    jeu: String,
    pid: u32,
    proc_nom: String,
    debut: f64,
    fermees: Vec<Fermee>,
}

/// La mesure des FPS d'une partie ($script:FpsTarget)
struct Cible {
    pid: u32,
    nom: String,
    cle: String,
    debut: f64,
    debut_s: String,
    images: fps::Images,
    courbe: Vec<f64>,
    ticks: u64,
    exclusif: bool,
    averti: bool,
    releves: diag::Releves,
    refus: bool,
    sans_image: u32,
    distant: bool,
    /// La dernière lecture réussie (`fps.lire`) : le temps sans réponse se compte depuis elle (lot G)
    derniere_lecture: f64,
    /// L'Aide n'a rien rendu depuis [`SANS_LECTURE_MAX`] : la mesure est redemandée (`fps.demarrer`, même jeu) dès
    /// qu'elle répond de nouveau ; le compteur dit « en attente » pendant ce temps (lot G, ajustement D)
    a_reprendre: bool,
    prochaine_reprise: f64,
    prochaine_lecture: f64,
    prochain_tick: f64,
    compteur_vu: bool,
    compteur_ticks: u64,
}

/// Une partie finie dont le bilan se calcule dans un autre fil
struct BilanEnCours {
    nom: String,
    cle: String,
    debut_s: String,
    exclusif: bool,
    courbe: Vec<f64>,
    refus: bool,
    erreur: String,
    releves: diag::Releves,
    fil: std::thread::JoinHandle<fps::Bilan>,
}

#[derive(Default)]
pub struct BlocPerf {
    sys: Option<Box<dyn Systeme>>,
    /// `$Live.Fast` : un test des composants est en cours (signal « mesures-rapides » du bloc pc)
    pub rapide: bool,
    compteur: Option<Box<dyn Compteur>>,
    t0: Option<Instant>,
    /// Horloge des tests (secondes) ; `None` : la vraie
    pub horloge: Option<f64>,
    demarre: bool,
    mesures: Mesures,
    chaleur: AlerteChaleur,
    proch_mesure: f64,
    proch_temperature: f64,
    proch_detection: f64,
    jeux: Option<Vec<Value>>,
    index: HashMap<String, EntreeIndex>,
    session: Option<Session>,
    cible: Option<Cible>,
    /// La mesure des FPS de la partie attend l'Aide, réveillée dans un fil à part (lot B)
    fps_a_reprendre: bool,
    bilans: Vec<BilanEnCours>,
    veille_demande: Option<f64>,
    veille_etat: String,
    /// `$script:FluxVeille` : la trace réseau a été mise en veille (un jeu guetté est installé)
    flux_veille: bool,
    /// `$script:LagSession` : une mesure du lag tourne (signal « mesure-lag » du bloc trafic, test rapide compris)
    lag_en_cours: bool,
    ext: Map<String, Value>,
    /// Les profils lus (priorité et carte puissante de chaque jeu), gardés jusqu'au prochain changement
    profils: Option<Vec<Value>>,
    profils_lus: bool,
    procs: Option<(f64, Vec<String>)>,
    dernier: Option<(f64, Value)>,
    raccourci_actif: Option<bool>,
}

impl BlocPerf {
    /// Un bloc sur un faux PC (tests)
    pub fn avec(sys: Box<dyn Systeme>, compteur: Box<dyn Compteur>) -> Self {
        Self { sys: Some(sys), compteur: Some(compteur), horloge: Some(0.0), ..Default::default() }
    }

    fn maintenant(&mut self) -> f64 {
        if let Some(h) = self.horloge {
            return h;
        }
        self.t0.get_or_insert_with(Instant::now).elapsed().as_secs_f64()
    }

    fn sys(&mut self) -> &mut dyn Systeme {
        if self.sys.is_none() {
            #[cfg(windows)]
            {
                self.sys = Some(Box::new(windows::WindowsReel::default()));
            }
            #[cfg(not(windows))]
            {
                self.sys = Some(Box::new(systeme::FauxSysteme::default()));
            }
        }
        self.sys.as_deref_mut().unwrap_or_else(|| unreachable!())
    }

    fn compteur(&mut self) -> &mut dyn Compteur {
        if self.compteur.is_none() {
            #[cfg(windows)]
            {
                self.compteur = Some(Box::new(windows::compteur::CompteurWindows::default()));
            }
            #[cfg(not(windows))]
            {
                self.compteur = Some(Box::new(systeme::FauxCompteur::default()));
            }
        }
        self.compteur.as_deref_mut().unwrap_or_else(|| unreachable!())
    }

    fn lire_mesures(&mut self, b: Besoin) {
        self.sys();
        if let Some(s) = self.sys.as_deref_mut() {
            self.mesures.lire(s, b);
        }
    }

    /// Le nombre de lectures de Windows faites depuis le début
    pub fn lectures(&self) -> u64 {
        self.sys.as_ref().map(|s| s.lectures()).unwrap_or(0)
    }

    /// Les dernières mesures en direct
    pub fn direct(&self) -> &mesures::Direct {
        &self.mesures.direct
    }

    fn reglage_vrai(ctx: &Contexte, nom: &str, defaut: bool) -> bool {
        jeu::reglage_vrai(ctx.reglage(nom).as_ref(), defaut)
    }
    /// Test-FpsMeasure (les versions 1.0.14 et 1.0.15 n'avaient que le réglage de l'overlay)
    fn mesure_fps(ctx: &Contexte) -> bool {
        let defaut = Self::reglage_vrai(ctx, "FpsOverlay", false);
        Self::reglage_vrai(ctx, "FpsMeasure", defaut)
    }

    fn notifier(effets: &mut Effets, titre: &str, texte: &str, action: Value) {
        effets.ajouter(Effet::Signal {
            nom: "notification".into(),
            donnees: json!({ "titre": titre, "texte": texte, "action": action }),
        });
        effets.ajouter(Effet::Annonce(format!("{titre} : {texte}")));
    }

    fn fichier_mode_jeu(ctx: &Contexte) -> std::path::PathBuf {
        ctx.donnees.join("mode-jeu.json")
    }

    // -----------------------------------------------------------------------
    // Programmes ouverts
    // -----------------------------------------------------------------------
    fn programmes(&mut self) -> Vec<Programme> {
        let l = self.sys().programmes();
        let t = self.maintenant();
        self.procs = Some((t, l.iter().map(|p| p.nom.clone()).collect()));
        l
    }

    /// Les noms des programmes ouverts, gardés 20 s (Get-AppsModeJeuKit)
    fn noms_programmes(&mut self) -> Vec<String> {
        let t = self.maintenant();
        if let Some((a, l)) = &self.procs
            && t - a < 20.0
        {
            return l.clone();
        }
        self.programmes();
        self.procs.as_ref().map(|p| p.1.clone()).unwrap_or_default()
    }

    // -----------------------------------------------------------------------
    // Mode jeu : début et fin de partie
    // -----------------------------------------------------------------------
    fn reprise(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let f = Self::fichier_mode_jeu(ctx);
        let Some(j) = fichier::lire(ctx, &f) else { return };
        let fermees = jeu::reprise(&j);
        if fermees.is_empty() {
            fichier::effacer(ctx, &f);
            return;
        }
        let pid = nb(j.get("Pid")) as u32;
        let proc_nom = ch(j.get("Proc"));
        let vivant = pid != 0 && self.sys().vivant(pid);
        let progs = if vivant { self.programmes() } else { Vec::new() };
        let meme = progs.iter().any(|p| p.pid == pid && !proc_nom.is_empty() && texte::egal(&p.nom, &proc_nom));
        let noms: Vec<&str> = fermees.iter().map(|x| x.nom.as_str()).collect();
        if vivant && meme {
            ctx.journal(&format!(
                "Mode jeu: {} toujours en cours au lancement de Nevermind, applis fermées: {}",
                ch(j.get("Game")),
                noms.join(", ")
            ));
            let t = self.maintenant();
            self.session = Some(Session { jeu: ch(j.get("Game")), pid, proc_nom, debut: t, fermees });
            return;
        }
        let geste = ctx.geste_prioritaire("Fin de partie", PAGE);
        let rates = self.relancer(&fermees);
        fichier::effacer(ctx, &f);
        drop(geste);
        ctx.journal(&format!(
            "Mode jeu: applis de la partie de {} relancées au lancement de Nevermind{}",
            ch(j.get("Game")),
            if rates.is_empty() { String::new() } else { format!(", à relancer à la main: {}", rates.join(", ")) }
        ));
        let (t, x) = jeu::bulle_fin(&rates);
        Self::notifier(effets, &t, &x, Value::Null);
    }

    fn relancer(&mut self, fermees: &[Fermee]) -> Vec<String> {
        let ouverts: Vec<String> = self.programmes().into_iter().map(|p| p.nom).collect();
        let sys = self.sys();
        jeu::relancer(fermees, |n| ouverts.iter().any(|o| texte::egal(o, n)), |c, a| sys.lancer(c, a))
    }

    fn debut_partie(&mut self, ctx: &Contexte, jeu_nom: &str, pid: u32, proc_nom: &str, effets: &mut Effets) {
        let mode_on = Self::reglage_vrai(ctx, "GameMode", false);
        let sel = if mode_on { jeu::selection(ctx.reglage("GameModeApps").as_ref()) } else { Vec::new() };
        let mut ouverts: Vec<(String, String)> = Vec::new();
        if !sel.is_empty() {
            let progs = self.programmes();
            for p in progs {
                if jeu::APPS_MODE_JEU.iter().any(|a| sel.iter().any(|s| texte::egal(s, a.id)) && a.correspond(&p.nom)) {
                    let c = self.sys().chemin(p.pid);
                    ouverts.push((p.nom, c));
                }
            }
        }
        // (la règle commune de fermeture, celle de la page RAM : l'Aide l'applique à chaque programme de ce nom)
        // (automatique : passe en priorité, jamais refusé ni retardé par un geste de l'utilisateur ; ajustement A du lot A)
        let geste = ctx.geste_prioritaire("Début de partie", PAGE);
        let fermees = jeu::fermer_pour_partie(mode_on, &sel, &ouverts, |a, procs| {
            let mut noms: Vec<&str> = Vec::new();
            for p in procs {
                if !noms.iter().any(|n| texte::egal(n, &p.0)) {
                    noms.push(&p.0);
                }
            }
            let mut n = 0;
            for nom in noms {
                // (jamais de demande de Windows : la partie commence, personne n'a cliqué ; audit G5)
                match ctx.aide_de_fond("processus.fermer", &json!({ "Nom": nom }), 15000) {
                    Ok(v) => n += nb(Some(&v)) as usize,
                    Err(e) => ctx.journal(&format!("Mode jeu : {} pas fermé ({e})", a.nom)),
                }
            }
            n
        });
        drop(geste);
        let t = self.maintenant();
        let debut_iso = nevermind_noyau::contexte::horodatage();
        let f = Self::fichier_mode_jeu(ctx);
        match jeu::fichier_partie(jeu_nom, pid, proc_nom, &debut_iso, &fermees) {
            Some(v) => {
                if let Err(e) = fichier::ecrire(&f, &v, false) {
                    ctx.journal(&format!("Mode jeu : liste des applis fermées non enregistrée ({e})"));
                }
            }
            None => fichier::effacer(ctx, &f),
        }
        let noms: Vec<&str> = fermees.iter().map(|x| x.nom.as_str()).collect();
        ctx.journal(&format!("Mode jeu: {jeu_nom} lancé, applis fermées: {}", noms.join(", ")));
        effets.ajouter(Effet::Signal {
            nom: "partie-commencee".into(),
            donnees: json!({ "jeu": jeu_nom, "exe": proc_nom, "pid": pid }),
        });
        self.session =
            Some(Session { jeu: jeu_nom.into(), pid, proc_nom: proc_nom.into(), debut: t, fermees: fermees.clone() });
        if Self::mesure_fps(ctx) && self.cible.is_none() {
            self.demarrer_cible(ctx, pid, jeu_nom, proc_nom, effets);
        }
        if let Some((t, x)) = jeu::bulle_debut(jeu_nom, &fermees) {
            Self::notifier(effets, &t, &x, Value::Null);
        }
        effets.ajouter(Effet::Rafraichir);
    }

    fn fin_partie(&mut self, ctx: &Contexte, effets: &mut Effets) {
        self.fps_a_reprendre = false;
        let Some(s) = self.session.take() else { return };
        if self.cible.as_ref().is_some_and(|c| c.pid == s.pid) {
            self.arreter_cible(ctx, false);
        }
        // (veille redemandée dès le prochain passage : un jeu relancé aussitôt la trouve prête)
        self.veille_demande = None;
        // (la remise en état : en priorité, jamais refusée ni retardée, même à la fermeture ; ajustement A du lot A)
        let geste = ctx.geste_prioritaire("Fin de partie", PAGE);
        let rates = self.relancer(&s.fermees);
        drop(geste);
        fichier::effacer(ctx, &Self::fichier_mode_jeu(ctx));
        let t = self.maintenant();
        let secondes = (t - s.debut).max(0.0);
        effets.ajouter(Effet::Signal {
            nom: "partie-finie".into(),
            donnees: json!({ "jeu": s.jeu, "exe": s.proc_nom, "pid": s.pid, "duree_s": secondes }),
        });
        ctx.journal(&format!(
            "Mode jeu: fin de {} après {} min{}",
            s.jeu,
            texte::ps_int(secondes / 60.0),
            if rates.is_empty() { String::new() } else { format!(", à relancer à la main: {}", rates.join(", ")) }
        ));
        if !s.fermees.is_empty() {
            let (t, x) = jeu::bulle_fin(&rates);
            Self::notifier(effets, &t, &x, Value::Null);
        }
        effets.ajouter(Effet::Rafraichir);
    }

    /// Test-GameRunning (toutes les 5 s)
    fn detection(&mut self, ctx: &Contexte, effets: &mut Effets) {
        if let Some(s) = &self.session {
            let pid = s.pid;
            if !self.sys().vivant(pid) {
                self.fin_partie(ctx, effets);
            }
            return;
        }
        let progs = self.programmes();
        self.veille(ctx, &progs);
        if self.index.is_empty() {
            return;
        }
        let mut ouverts = Vec::new();
        for p in progs.iter().filter(|p| self.index.contains_key(&texte::minuscules(&p.nom))) {
            ouverts.push(jeu::Ouvert { pid: p.pid, nom: p.nom.clone(), chemin: String::new() });
        }
        for o in &mut ouverts {
            o.chemin = self.sys().chemin(o.pid);
        }
        if let Some((nom, pid)) = jeu::detecter(&self.index, &ouverts) {
            let proc_nom = ouverts.iter().find(|o| o.pid == pid).map(|o| o.nom.clone()).unwrap_or_default();
            self.debut_partie(ctx, &nom, pid, &proc_nom, effets);
        }
    }

    // -----------------------------------------------------------------------
    // Veille de l'Aide avant les jeux à anti-triche (Update-FpsVeille)
    // -----------------------------------------------------------------------
    fn veille_jeux(ctx: &Contexte) -> Vec<String> {
        let mut l: Vec<String> = VEILLE_CONNUS.iter().map(|s| s.to_string()).collect();
        for x in liste(ctx.reglage("FpsVeille").as_ref()) {
            let n = ch(Some(&x)).to_lowercase();
            if !n.is_empty() && !l.contains(&n) {
                l.push(n);
            }
        }
        l
    }

    /// Update-FpsVeille (hors partie, toutes les 5 s) : la veille de la mesure des FPS (`fps.veiller`) ET de la trace
    /// réseau de « Lag en ligne » (`flux.veiller`), demandée à l'Aide au plus toutes les 30 s, seulement si elle écoute
    /// déjà (`[AideClient]::Joignable(1)`, puis `[AideClient]::Texte` : jamais de réveil ni de demande de Windows).
    /// Pas pour un jeu guetté déjà ouvert : la veille serait relancée sans lui.
    fn veille(&mut self, ctx: &Contexte, progs: &[Programme]) {
        let fps = Self::mesure_fps(ctx) && self.cible.is_none();
        let flux_voulue = Self::reglage_vrai(ctx, "LagMeasure", true) && !self.lag_en_cours;
        let flux = self.flux_veille_utile(ctx, flux_voulue);
        if !(fps || flux) {
            return;
        }
        let t = self.maintenant();
        if self.veille_demande.is_some_and(|d| t - d < 30.0) {
            return;
        }
        let noms = Self::veille_jeux(ctx);
        // (un jeu guetté est ouvert : la veille reste telle quelle, l'Aide la relancerait sans lui)
        if noms.iter().any(|n| progs.iter().any(|p| texte::egal(&format!("{}.exe", p.nom), n))) {
            return;
        }
        if noms.is_empty() {
            self.veille_demande = Some(t);
            return;
        }
        // (lot G : l'Aide pas prête, en pause après une absence de réponse, ne compte pas pour les 30 s : la veille
        //  est redemandée au passage suivant, 5 s plus tard, et repart dès que l'Aide répond de nouveau)
        if !ctx.aide_prete() {
            return;
        }
        self.veille_demande = Some(t);
        let mut erreurs: Vec<String> = Vec::new();
        if fps && let Err(e) = ctx.aide_texte("fps.veiller", &json!({ "Noms": noms }).to_string(), 15000) {
            erreurs.push(format!("FPS : {e}"));
        }
        if flux && let Err(e) = ctx.aide_texte("flux.veiller", "{}", 15000) {
            erreurs.push(format!("trace réseau : {e}"));
        }
        let quoi: Vec<&str> = [(fps, "FPS"), (flux, "trace réseau")].iter().filter(|x| x.0).map(|x| x.1).collect();
        let etat = if erreurs.is_empty() {
            format!("en veille avant {} ({})", noms.join(", "), quoi.join(" et "))
        } else {
            format!("veille impossible ({})", erreurs.join(" ; "))
        };
        if etat != self.veille_etat {
            ctx.journal(&format!("Mesure des FPS : {etat}"));
            self.veille_etat = etat;
        }
    }

    /// Test-FluxVeilleUtile (lag.ps1) : la trace réseau en veille tourne en administrateur et coûte du processeur
    /// pendant les téléchargements ; seulement si un des jeux guettés est installé. Sinon, celle déjà lancée (jeu
    /// désinstallé, mesure du lag coupée) est arrêtée (Stop-FluxVeille).
    fn flux_veille_utile(&mut self, ctx: &Contexte, voulue: bool) -> bool {
        let utile = voulue
            && Self::veille_jeux(ctx).iter().any(|n| {
                let base = n.strip_suffix(".exe").unwrap_or(n);
                self.index.contains_key(&texte::minuscules(base))
            });
        if utile {
            self.flux_veille = true;
        } else if self.flux_veille && !self.lag_en_cours {
            self.flux_veille = false;
            ctx.journal("Lag : trace réseau en veille arrêtée (aucun jeu guetté installé, ou mesure du lag coupée)");
            // (jamais attendu sur le fil des blocs ; gardé si l'Aide ne répond pas, lot G)
            ctx.arreter_mesure(Mesure::Trace);
        }
        utile
    }

    /// Update-FpsVeilleAntiTriche : les jeux à anti-triche EA repérés à l'installation, guettés dès leur première partie
    fn veille_anti_triche(&mut self, ctx: &Contexte) {
        let Some(jeux) = self.jeux.clone() else { return };
        let deja = Self::veille_jeux(ctx);
        let place = 10usize.saturating_sub(deja.len());
        let sys = self.sys();
        let trouves = jeu::jeux_anti_triche(&jeux, |exe| {
            let Some(d) = std::path::Path::new(exe).parent() else { return false };
            let d = d.to_string_lossy();
            sys.fichier_existe(&format!(r"{d}\EAAntiCheat.GameServiceLauncher.exe"))
                || sys.fichier_existe(&format!(r"{d}\EAAntiCheat.Installer.exe"))
                || sys.dossier_existe(&format!(r"{d}\EAAntiCheat"))
        });
        let nouveaux: Vec<String> = trouves.into_iter().filter(|n| !deja.contains(n)).take(place).collect();
        if nouveaux.is_empty() {
            return;
        }
        let mut l = liste(ctx.reglage("FpsVeille").as_ref());
        l.extend(nouveaux.iter().map(|n| json!(n)));
        let _ = ctx.ecrire_reglage("FpsVeille", Value::Array(l), false);
        ctx.journal(&format!(
            "Mesure des FPS : anti-triche EA repéré ({}), la mesure sera lancée avant le jeu",
            nouveaux.join(", ")
        ));
    }

    // -----------------------------------------------------------------------
    // Mesure des FPS d'une partie
    // -----------------------------------------------------------------------
    fn demarrer_cible(&mut self, ctx: &Contexte, pid: u32, nom: &str, exe: &str, effets: &mut Effets) {
        let pm = ctx.appli.join("outils-tiers").join("PresentMon.exe");
        if !self.sys().fichier_existe(&pm.to_string_lossy()) {
            effets.ajouter(Effet::Annonce(
                "Mesure des FPS indisponible : PresentMon est absent du dossier de l'app.".into(),
            ));
            return;
        }
        if self.cible.is_some() {
            self.arreter_cible(ctx, false);
        }
        // Start-FpsMesure : Test-MesuresAide (l'Aide réveillée au besoin, sans demande de Windows), puis
        // [FrameMon]::StartRemote (`fps.demarrer` par [AideClient]::Texte). Le repli sur place n'est pas porté.
        if !ctx.mesures_aide() {
            ctx.journal("Mesure des FPS: l'Aide ne répond pas.");
            // (pendant une partie : reprise quand l'Aide, réveillée en fond, écoute de nouveau)
            self.fps_a_reprendre = self.session.as_ref().is_some_and(|s| s.pid == pid);
            return;
        }
        self.fps_a_reprendre = false;
        let attache = match ctx.aide_texte("fps.demarrer", &format!("{{\"Pid\":{pid}}}"), 15000) {
            Ok(r) => r == "rattachee",
            Err(e) => {
                ctx.journal(&format!("Mesure des FPS par l'Aide impossible : {e}"));
                ctx.journal(&format!("Mesure des FPS: {e}"));
                return;
            }
        };
        ctx.journal(&format!(
            "Mesure des FPS : par l'Aide{}",
            if attache { ", reprise de la veille lancée avant le jeu" } else { "" }
        ));
        let chemin = self.sys().chemin(pid);
        let t = self.maintenant();
        let mut releves = diag::Releves::default();
        releves.chemin = chemin;
        self.cible = Some(Cible {
            pid,
            nom: nom.into(),
            cle: exe.to_lowercase(),
            debut: t,
            debut_s: nevermind_noyau::contexte::horodatage(),
            images: fps::Images::default(),
            courbe: Vec::new(),
            ticks: 0,
            exclusif: false,
            averti: false,
            releves,
            refus: false,
            sans_image: 0,
            distant: true,
            derniere_lecture: t,
            a_reprendre: false,
            prochaine_reprise: t,
            prochaine_lecture: t + 0.25,
            prochain_tick: t + 0.5,
            compteur_vu: false,
            compteur_ticks: 0,
        });
        ctx.journal(&format!("Mesure des FPS: {nom}"));
    }

    /// Les lignes de PresentMon gardées par l'Aide (toutes les 250 ms)
    fn lire_fps(&mut self, ctx: &Contexte) {
        let maintenant = self.maintenant();
        let Some(c) = self.cible.as_mut() else { return };
        if c.a_reprendre {
            Self::reprendre_fps(ctx, c, maintenant);
            return;
        }
        if !c.distant {
            return;
        }
        match ctx.aide_texte("fps.lire", "{}", 5000) {
            Ok(t) => {
                c.derniere_lecture = maintenant;
                for l in t.split('\n') {
                    let l = l.trim_end_matches('\r');
                    if l.is_empty() {
                        continue;
                    }
                    if l == "#FIN" {
                        c.distant = false;
                    } else if let Some(e) = l.strip_prefix("#ERR ") {
                        c.images.derniere_erreur = e.to_string();
                    } else {
                        c.images.ligne(l);
                    }
                }
            }
            Err(e) => {
                // (un refus « en pause » n'est pas une lecture ratée : l'Aide n'a pas été jointe ; seul compte le temps
                //  depuis la dernière lecture réussie. Lot G)
                if e != nevermind_noyau::aide::ERREUR_PAUSE {
                    c.images.derniere_erreur = e;
                }
                if maintenant - c.derniere_lecture >= SANS_LECTURE_MAX {
                    c.a_reprendre = true;
                    c.prochaine_reprise = maintenant;
                    ctx.journal(&format!(
                        "Mesure des FPS : l'Aide ne répond plus depuis {} s, la mesure reprendra quand elle répondra",
                        texte::ps_int(maintenant - c.derniere_lecture)
                    ));
                }
            }
        }
    }

    /// La mesure « à reprendre » (lot G, ajustement D) : dès que l'Aide répond de nouveau (`mesures_aide` : réveillée
    /// au besoin dans un fil à part, sans demande de Windows), la mesure est redemandée pour le même jeu (l'Aide a pu
    /// redémarrer et la perdre). Au plus une fois par seconde.
    fn reprendre_fps(ctx: &Contexte, c: &mut Cible, maintenant: f64) {
        if maintenant < c.prochaine_reprise {
            return;
        }
        c.prochaine_reprise = maintenant + 1.0;
        if !ctx.mesures_aide() {
            return;
        }
        match ctx.aide_texte("fps.demarrer", &format!("{{\"Pid\":{}}}", c.pid), 15000) {
            Ok(r) => {
                c.a_reprendre = false;
                c.distant = true;
                c.derniere_lecture = maintenant;
                ctx.journal(&format!(
                    "Mesure des FPS : reprise par l'Aide{}",
                    if r == "rattachee" { ", sur la veille lancée avant le jeu" } else { "" }
                ));
            }
            Err(e) => {
                if e != nevermind_noyau::aide::ERREUR_PAUSE {
                    c.images.derniere_erreur = e;
                    // (l'Aide répond mais refuse : pas une demande par seconde)
                    c.prochaine_reprise = maintenant + 5.0;
                }
            }
        }
    }

    /// Update-FpsTarget (toutes les 500 ms)
    fn tick_cible(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let Some(pid) = self.cible.as_ref().map(|c| c.pid) else { return };
        if !self.sys().vivant(pid) {
            self.arreter_cible(ctx, false);
            return;
        }
        let (fg, _) = self.sys().devant();
        let devant = fg == pid;
        let veille = Self::veille_jeux(ctx);
        let t = self.maintenant();
        let mut lire_sys = false;
        let lire_procs;
        {
            let Some(c) = self.cible.as_mut() else { return };
            c.images.en_pause = !devant;
            c.ticks += 1;
            let sans_image = c.images.nombre() == 0;
            // (l'Aide qui ne répond pas n'est pas l'anti-triche qui bloque : ce temps-là ne compte pas ; lot G)
            if devant && sans_image && !c.a_reprendre {
                c.sans_image += 1;
            }
            if !c.refus && sans_image && !c.a_reprendre {
                let refus = jeu::refus_mesure(&c.images.derniere_erreur);
                if refus || (c.sans_image >= 60 && veille.contains(&format!("{}.exe", c.cle))) {
                    c.refus = true;
                    ctx.journal(&format!(
                        "Mesure des FPS : bloquée par l'anti-triche de {}{}",
                        c.nom,
                        if refus { "" } else { " (aucune image après 30 s)" }
                    ));
                    effets.ajouter(Effet::Annonce(format!(
                        "{} : l'anti-triche de ce jeu bloque la mesure des FPS. À la prochaine partie, ouvre Nevermind avant de lancer le jeu : la mesure démarrera avant lui.",
                        c.nom
                    )));
                    if refus {
                        let e = format!("{}.exe", c.cle);
                        if !veille.contains(&e) {
                            let mut l = liste(ctx.reglage("FpsVeille").as_ref());
                            l.push(json!(e));
                            let _ = ctx.ecrire_reglage("FpsVeille", Value::Array(l), false);
                            ctx.journal(&format!("Mesure des FPS : {e} refuse une mesure démarrée après lui, elle sera lancée avant lui la prochaine fois"));
                        }
                    }
                }
            }
            if c.ticks % 10 == 0 {
                let v = c.images.echantillon();
                if v > 0.0 {
                    c.courbe.push(texte::ps_round(v, 1));
                }
                lire_sys = devant;
            }
            lire_procs = c.ticks % 20 == 0;
        }
        if lire_sys {
            // (un relevé frais, au lieu de la dernière mesure en direct, peut-être vieille de 10 s)
            self.lire_mesures(Besoin::TOUT);
            let r = self.mesures.direct.releve();
            let batterie = self.sys().sur_batterie();
            if let Some(c) = self.cible.as_mut() {
                c.releves.sys.push(r);
                if batterie {
                    c.releves.sur_batterie = true;
                }
            }
            if let Some(m) = self.mesures.direct.gpu_temp
                && let Some(txt) = self.chaleur.tester(Some(m), t)
            {
                Self::alerte_chaleur(ctx, effets, &txt, m);
            }
        }
        if lire_procs {
            let soi = std::process::id();
            let progs: Vec<String> = self
                .sys()
                .programmes()
                .into_iter()
                .filter(|p| p.pid > 4 && p.pid != pid && p.pid != soi)
                .map(|p| format!("{}|{}|{}|{}", p.pid, p.nom, p.cpu, p.memoire))
                .collect();
            if let Some(c) = self.cible.as_mut() {
                c.releves.programmes(&progs, t);
            }
        }
        // Le compteur par-dessus le jeu
        let afficher = Self::reglage_vrai(ctx, "FpsOverlay", false);
        let style = if ch(ctx.reglage("FpsOverlayStyle").as_ref()) == "discret" { "discret" } else { "complet" };
        let coin = {
            let c = ch(ctx.reglage("FpsOverlayCorner").as_ref());
            if ["hg", "hd", "bg", "bd"].contains(&c.as_str()) { c } else { "hg".into() }
        };
        let mut ordre: Option<Option<EtatCompteur>> = None;
        let mut devant_ordre = false;
        if let Some(c) = self.cible.as_mut() {
            let exclusif = c.images.dernier_mode.to_lowercase().contains("legacy");
            if exclusif {
                c.exclusif = true;
            }
            if afficher {
                if exclusif && !c.averti {
                    c.averti = true;
                    ctx.journal(&format!(
                        "Mesure des FPS: {} est en plein écran exclusif, le compteur est caché.",
                        c.nom
                    ));
                    effets.ajouter(Effet::Annonce(format!("{} est en plein écran : le compteur ne peut pas s'afficher par-dessus, il est caché. La mesure continue.", c.nom)));
                }
                if !devant || exclusif {
                    if c.compteur_vu {
                        c.compteur_vu = false;
                        ordre = Some(None);
                    }
                } else if c.a_reprendre {
                    // (l'Aide ne répond plus : le compteur attend, la mesure reprendra ; lot G)
                    c.compteur_vu = true;
                    ordre = Some(Some(EtatCompteur {
                        style: style.into(),
                        coin,
                        chiffre: "...".into(),
                        sous: if style == "discret" { String::new() } else { "Mesure en attente".into() },
                        couleur: ORANGE,
                    }));
                } else if let Some((chiffre, sous, fps_v)) = fps::texte_compteur(&c.images, c.refus, t - c.debut) {
                    let couleur = if c.images.nombre() == 0 || fps_v >= 60.0 {
                        VERT
                    } else if fps_v >= 30.0 {
                        ORANGE
                    } else {
                        ROUGE
                    };
                    c.compteur_vu = true;
                    ordre = Some(Some(EtatCompteur {
                        style: style.into(),
                        coin,
                        chiffre,
                        sous: if style == "discret" { String::new() } else { sous },
                        couleur,
                    }));
                    c.compteur_ticks += 1;
                    devant_ordre = c.compteur_ticks.is_multiple_of(6);
                } else if !c.compteur_vu {
                    c.compteur_vu = true;
                    ordre = Some(Some(EtatCompteur {
                        style: style.into(),
                        coin,
                        chiffre: "...".into(),
                        sous: if style == "discret" { String::new() } else { "Mesure en cours...".into() },
                        couleur: VERT,
                    }));
                }
            } else if c.compteur_vu {
                c.compteur_vu = false;
                ordre = Some(None);
            }
        }
        match ordre {
            Some(Some(e)) => self.compteur().montrer(&e),
            Some(None) => self.compteur().cacher(),
            None => {}
        }
        if devant_ordre {
            self.compteur().devant();
        }
    }

    /// Stop-FpsTarget : le bilan est calculé dans un autre fil (millions d'images sur une longue partie)
    fn arreter_cible(&mut self, ctx: &Contexte, attendre: bool) {
        self.arreter_cible_en(ctx, if attendre { Some(Duration::from_secs(10)) } else { None });
    }

    /// `arreter_cible` avec un temps compté : `bilan` (le bilan attendu au plus tant)
    fn arreter_cible_en(&mut self, ctx: &Contexte, bilan: Option<Duration>) {
        let Some(c) = self.cible.take() else { return };
        if c.compteur_vu {
            self.compteur().cacher();
        }
        let copie = c.images.copie();
        // ([FrameMon]::Stop : toujours, même après #FIN ou une Aide muette, comme `remoteOn` en PowerShell. Lot G :
        //  jamais attendu sur le fil des blocs, et gardé si l'Aide ne répond pas, envoyé dès qu'elle répond ; jeté si
        //  une nouvelle mesure démarre avant)
        ctx.arreter_mesure(Mesure::Fps);
        let fil = std::thread::spawn(move || copie.calculer());
        let b = BilanEnCours {
            nom: c.nom,
            cle: c.cle,
            debut_s: c.debut_s,
            exclusif: c.exclusif,
            courbe: c.courbe,
            refus: c.refus,
            erreur: c.images.derniere_erreur,
            releves: c.releves,
            fil,
        };
        self.bilans.push(b);
        if let Some(d) = bilan {
            let mut e = Effets::default();
            let fin = Instant::now() + d;
            while self.bilans.iter().any(|b| !b.fil.is_finished()) && Instant::now() < fin {
                std::thread::sleep(Duration::from_millis(20));
            }
            self.bilans_finis(ctx, &mut e);
        }
    }

    fn bilans_finis(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let mut i = 0;
        while i < self.bilans.len() {
            if !self.bilans[i].fil.is_finished() {
                i += 1;
                continue;
            }
            let b = self.bilans.remove(i);
            let Ok(bilan) = b.fil.join() else {
                ctx.journal("Mesure des FPS (bilan): calcul impossible");
                continue;
            };
            self.partie_finie(
                ctx, b.nom, b.cle, b.debut_s, b.exclusif, b.courbe, b.refus, b.erreur, b.releves, &bilan, effets,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn partie_finie(
        &mut self,
        ctx: &Contexte,
        nom: String,
        cle: String,
        debut_s: String,
        exclusif: bool,
        courbe: Vec<f64>,
        refus: bool,
        erreur: String,
        releves: diag::Releves,
        bilan: &fps::Bilan,
        effets: &mut Effets,
    ) {
        if bilan.resume[3] == 0.0 {
            ctx.journal(&format!("Mesure des FPS: aucune image reçue pour {nom}. {erreur}"));
        }
        let gpus = self.cartes();
        let ecrans = self.sys().ecrans();
        let coeurs = self.sys().coeurs();
        let analyse = self.ext.get("analyse-pc").cloned().unwrap_or(Value::Null);
        let disques: Vec<diag::Disque> = liste(analyse.get("disques"))
            .iter()
            .map(|d| diag::Disque {
                lettres: liste(d.get("lettres")).iter().map(|l| ch(Some(l))).collect(),
                bus: ch(d.get("bus")),
                media: ch(d.get("media")),
            })
            .collect();
        let portable =
            if analyse.get("portable").is_some() { vrai(analyse.get("portable")) } else { self.sys().batterie() };
        let cd = diag::ContexteDiag {
            coeurs,
            disques: &disques,
            ecrans: &ecrans,
            gpus: &gpus,
            portable,
            onduleur: vrai(analyse.get("onduleur")),
            mode_jeu: Self::reglage_vrai(ctx, "GameMode", false),
        };
        let donnees = diag::donnees(&releves, &bilan.occupe, &cd);
        let id = identifiant();
        let t = fps::Partie { nom: &nom, cle: &cle, debut_s: &debut_s, exclusif, courbe: &courbe, refus };
        match fps::fin_partie(&t, bilan, Some(donnees), &id) {
            fps::FinPartie::Rien => {}
            fps::FinPartie::BulleSeule(titre, texte) => Self::notifier(effets, &titre, &texte, Value::Null),
            fps::FinPartie::Enregistrer { partie, bulle, .. } => {
                let f = ctx.donnees.join("fps.json");
                let mut l = liste(fichier::lire(ctx, &f).as_ref());
                l.push(partie.clone());
                let debut = l.len().saturating_sub(200);
                let l: Vec<Value> = l.split_off(debut);
                if let Err(e) = fichier::ecrire(&f, &Value::Array(l), true) {
                    ctx.journal(&format!("Écriture des FPS impossible: {e}"));
                }
                effets.ajouter(Effet::Signal { nom: "fps-partie-enregistree".into(), donnees: partie });
                Self::notifier(effets, &bulle.0, &bulle.1, json!({ "ecran": "fps", "partie": id }));
                effets.ajouter(Effet::Rafraichir);
            }
        }
    }

    /// Ctrl+Maj+F : lance ou arrête la mesure sur le jeu au premier plan (Switch-FpsManual)
    fn manuel(&mut self, ctx: &Contexte, effets: &mut Effets) {
        if !Self::mesure_fps(ctx) {
            Self::notifier(
                effets,
                "Mesure des FPS désactivée",
                "Active-la dans Nevermind : page Jeux, Perf. In Game, FPS.",
                Value::Null,
            );
            return;
        }
        if let Some(n) = self.cible.as_ref().map(|c| c.nom.clone()) {
            self.arreter_cible(ctx, false);
            effets.ajouter(Effet::Annonce(format!("Mesure des FPS arrêtée ({n}).")));
            return;
        }
        let (fg, titre) = self.sys().devant();
        if fg == 0 || fg == std::process::id() {
            return;
        }
        let Some(p) = self.programmes().into_iter().find(|p| p.pid == fg) else { return };
        let nom = match jeu::jeu_connu(&self.index, &p.nom) {
            Some(j) => j.to_string(),
            None if !titre.is_empty() => titre,
            None => p.nom.clone(),
        };
        self.demarrer_cible(ctx, fg, &nom, &p.nom, effets);
        Self::notifier(
            effets,
            "Mesure des FPS lancée",
            &format!("{nom} : appuie de nouveau sur Ctrl + Maj + F pour arrêter."),
            Value::Null,
        );
    }

    fn alerte_chaleur(ctx: &Contexte, effets: &mut Effets, texte: &str, t: f64) {
        ctx.journal(&format!("Alerte température: carte graphique à {} °C", texte::ps_int(t)));
        Self::notifier(effets, "Carte graphique très chaude", texte, Value::Null);
    }

    fn cartes(&mut self) -> Vec<Value> {
        if let Some(g) = self.ext.get("analyse-pc").and_then(|a| a.get("gpus")).filter(|g| g.is_array()) {
            return liste(Some(g));
        }
        self.sys().cartes().into_iter().map(Value::String).collect()
    }

    // -----------------------------------------------------------------------
    // Pages
    // -----------------------------------------------------------------------
    /// Le dernier jeu joué (Get-LastPlayedGame, gardé 20 s) : { jeu, moy }
    fn dernier_jeu(&mut self, ctx: &Contexte) -> Value {
        let t = self.maintenant();
        if let Some((a, v)) = &self.dernier
            && t - a < 20.0
        {
            return v.clone();
        }
        let mut r = Value::Null;
        if let Some(jeux) = &self.jeux {
            let log = fichier::lire(ctx, &ctx.donnees.join("jeux.json")).unwrap_or(Value::Null);
            let mut meilleur: Option<(String, jeu::Date)> = None;
            for g in jeux {
                let n = ch(g.get("nom"));
                let Some(e) = log.get(&n) else { continue };
                let Some(d) = jeu::date_iso(&ch(e.get("Last"))) else { continue };
                if meilleur.as_ref().is_none_or(|m| d > m.1) {
                    meilleur = Some((n, d));
                }
            }
            if let Some((n, _)) = meilleur {
                let parties = liste(fichier::lire(ctx, &ctx.donnees.join("fps.json")).as_ref());
                let mut miennes: Vec<&Value> = parties.iter().filter(|p| ch(p.get("Game")) == n).collect();
                miennes.sort_by_key(|p| jeu::date_iso(&ch(p.get("Date"))));
                let moy = miennes.last().map(|p| p.get("Avg").cloned().unwrap_or(Value::Null)).unwrap_or(Value::Null);
                r = json!({ "jeu": n, "moy": moy });
            }
        }
        self.dernier = Some((t, r.clone()));
        r
    }

    fn jeux_avec_profils(&mut self) -> Option<Vec<Value>> {
        let jeux = self.jeux.clone()?;
        if let Some(p) = &self.profils {
            return Some(p.clone());
        }
        let mut out = Vec::new();
        for g in jeux {
            let mut g = g.clone();
            let exes = liste(g.get("exes"));
            let noms = jeu::noms_exes_profil(&exes);
            let prio = noms.first().is_some_and(|n| self.sys().priorite(n));
            let gpu = exes.first().map(|e| self.sys().preference_gpu(&ch(Some(e)))).unwrap_or_default();
            if let Value::Object(m) = &mut g {
                m.insert("prio".into(), json!(prio));
                m.insert("gpu".into(), json!(gpu));
            }
            out.push(g);
        }
        Some(out)
    }

    /// L'entrée des pages (la même forme que les juges)
    pub fn entree(&mut self, ctx: &Contexte, ecran: &str) -> Value {
        let ext = |k: &str, c: &str| self.ext.get(k).and_then(|v| v.get(c)).cloned().unwrap_or(Value::Null);
        let reglages = ext("reglages-jeu", "lignes");
        let restauration = ext("reglages-jeu", "restauration");
        let internet = ext("connexion", "internet");
        let bridage = ext("connexion", "bridage");
        let demarrage = ext("demarrage", "entrees");
        let texte_dem = ext("demarrage", "texte");
        let nettoyage = ext("nettoyage", "lignes");
        let texte_net = ext("nettoyage", "texte");
        let bouton_net = ext("nettoyage", "bouton");
        let analyse = ext("analyse-pc", "analyse");
        let memoire = self.ext.get("memoire-constat").cloned().unwrap_or(Value::Null);
        let dernier = self.dernier_jeu(ctx);
        let disque = self.sys().disque_systeme();
        let d = &self.mesures.direct;
        let ram = if d.fait && d.ram_totale.is_some() {
            json!({ "utilisee": d.ram_utilisee, "totale": d.ram_totale })
        } else {
            Value::Null
        };
        let dn = ctx.reglage("LastClean").filter(|v| !v.is_null()).map(|v| {
            let f = ch(v.get("File"));
            json!({ "date": ch(v.get("Date")), "libere": v.get("Freed"), "supprimes": v.get("Deleted"), "laisses": v.get("Skipped"), "fichier": !f.is_empty() && std::path::Path::new(&f).exists() })
        });
        let processus = self.noms_programmes();
        let session = self
            .session
            .as_ref()
            .map(|s| json!({ "jeu": s.jeu, "fermees": s.fermees.iter().map(|f| f.nom.clone()).collect::<Vec<_>>() }));
        let jeux =
            if ecran == "optimisation" || self.profils_lus { self.jeux_avec_profils() } else { self.jeux.clone() };
        if ecran == "optimisation"
            && let Some(j) = &jeux
        {
            self.profils = Some(j.clone());
            self.profils_lus = true;
        }
        let n_lib = self.jeux.as_ref().map(|j| {
            let mut vus: Vec<String> = Vec::new();
            for g in j {
                let n = ch(g.get("nom"));
                if !n.is_empty() && !vus.contains(&n) {
                    vus.push(n);
                }
            }
            vus.len()
        });
        let gpus = self.cartes();
        json!({
            "ecran": ecran, "reglages": reglages, "restauration": restauration, "internet": internet, "bridage": bridage, "dernier": dernier,
            "demarrage": demarrage, "texteDemarrage": texte_dem, "nettoyage": nettoyage,
            "disque": disque.as_ref().map(|d| json!({ "total": d.0, "libre": d.1 })), "lecteur": disque.as_ref().map(|d| d.2.clone()).unwrap_or_else(|| "C:".into()),
            "texteNettoyage": texte_net, "boutonNettoyer": bouton_net, "dernierNettoyage": dn, "memoire": memoire, "ram": ram, "analyse": analyse,
            "modeJeu": Self::reglage_vrai(ctx, "GameMode", false), "appsModeJeu": ctx.reglage("GameModeApps"), "processus": processus, "session": session,
            "jeux": jeux, "gpus": gpus, "nLib": n_lib.unwrap_or(0), "profilsLus": self.profils_lus,
            "streameur": self.ext.get("streameur").and_then(|s| s.get("carte")).cloned().unwrap_or(Value::Null)
        })
    }

    fn mettre_raccourci(&mut self, ctx: &Contexte) {
        let voulu = Self::mesure_fps(ctx);
        if self.raccourci_actif == Some(voulu) || (self.raccourci_actif.is_none() && !voulu) {
            self.raccourci_actif = Some(voulu);
            return;
        }
        let ok = self.compteur().raccourci(voulu);
        if voulu && !ok {
            ctx.journal("Raccourci Ctrl+Maj+F déjà pris par un autre programme.");
        }
        self.raccourci_actif = Some(voulu);
    }
}

/// Un identifiant de partie (10 caractères hexadécimaux, comme [guid] coupé)
fn identifiant() -> String {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let x = (n as u64) ^ (u64::from(std::process::id()) << 40) ^ 0x9E37_79B9_7F4A_7C15;
    let x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    format!("{:010x}", x & 0xff_ffff_ffff)
}

impl Bloc for BlocPerf {
    fn nom(&self) -> &'static str {
        "perf"
    }

    fn pages(&mut self, ctx: &Contexte, ecran: &str) -> Map<String, Value> {
        let e = self.entree(ctx, ecran);
        let mut p = pages::pages(&e);
        // La tuile « Performances » de l'accueil (Get-EcranNevermind : le même compte que la page, Get-ReglagesKit)
        let lignes = liste(e.get("reglages"));
        let sous = if lignes.is_empty() {
            "Pour gagner des FPS".to_string()
        } else {
            let (_, _, reste, _) = pages::compter_reglages(&lignes);
            pages::titre_reglages(lignes.len(), reste)
        };
        p.insert("_accueil".into(), json!({ "perf": { "sous": sous } }));
        p
    }

    fn action(&mut self, ctx: &Contexte, id: &str, valeur: &Value, effets: &mut Effets) -> bool {
        match id {
            "modejeu" => {
                let on = vrai(Some(valeur));
                let _ = ctx.ecrire_reglage("GameMode", json!(on), false);
                effets.ajouter(Effet::Annonce(
                    if on { "Mode jeu automatique activé." } else { "Mode jeu automatique désactivé." }.into(),
                ));
                effets.ajouter(Effet::Rafraichir);
                true
            }
            "modejeu-app" => {
                let id = ch(valeur.get("valeur"));
                let mut cur: Vec<String> =
                    jeu::selection(ctx.reglage("GameModeApps").as_ref()).into_iter().filter(|x| x != &id).collect();
                if vrai(valeur.get("oui")) {
                    cur.push(id);
                }
                let _ = ctx.ecrire_reglage("GameModeApps", json!(cur), false);
                effets.ajouter(Effet::Rafraichir);
                true
            }
            "ajouterjeu" => {
                effets.ajouter(Effet::Signal { nom: "ajouter-jeu".into(), donnees: json!({}) });
                true
            }
            _ => {
                if let Some(k) = id.strip_prefix("conseil:").and_then(|k| k.parse::<usize>().ok()) {
                    let l: Vec<Value> = liste(self.ext.get("analyse-pc").and_then(|a| a.get("analyse")))
                        .into_iter()
                        .filter(|a| {
                            ch(a.get("id")).to_lowercase().starts_with("tweak:")
                                && texte::egal(&ch(a.get("statut")), "warn")
                        })
                        .collect();
                    if let Some(f) = l.get(k) {
                        effets.ajouter(Effet::Signal { nom: "ouvrir-fiche".into(), donnees: f.clone() });
                    }
                    return true;
                }
                if let Some(r) = id.strip_prefix("profil:") {
                    let mut p = r.split(':');
                    let (Some(k), Some(quoi)) = (p.next().and_then(|k| k.parse::<usize>().ok()), p.next()) else {
                        return false;
                    };
                    if quoi != "priority" && quoi != "gpu" {
                        return false;
                    }
                    let jeux = self.profils.clone().unwrap_or_default();
                    let gpus = self.cartes();
                    let (pr, _) = pages::profils(&jeux, &gpus);
                    if let Some(x) = pr.get(k) {
                        let on = vrai(Some(valeur));
                        let changements = changements_profil(&x.jeu, quoi, on);
                        let label = if quoi == "priority" { "priorité haute" } else { "carte graphique puissante" };
                        let etat = if on { "activée" } else { "désactivée" };
                        effets.ajouter(Effet::Signal {
                            nom: "declarations".into(),
                            donnees: json!({ "changements": changements, "historique": format!("Profil de {} : {label} {etat}", x.nom) }),
                        });
                        effets.ajouter(Effet::Annonce(format!("{} : {label} {etat}.", x.nom)));
                        self.profils = None;
                        effets.ajouter(Effet::Rafraichir);
                    }
                    return true;
                }
                false
            }
        }
    }

    fn tic(&mut self, ctx: &Contexte, cache: bool, effets: &mut Effets) -> Option<Duration> {
        let t = self.maintenant();
        if !self.demarre {
            self.demarre = true;
            self.reprise(ctx, effets);
        }
        self.mettre_raccourci(ctx);
        if self.raccourci_actif == Some(true) && self.compteur().raccourci_presse() {
            self.manuel(ctx, effets);
        }
        // La mesure des FPS de la partie n'a pas pu démarrer, l'Aide ne répondait pas (elle est réveillée dans un fil
        // à part, lot B) : reprise dès qu'elle écoute, pendant la même partie
        if self.fps_a_reprendre
            && self.cible.is_none()
            && let Some((pid, jeu, proc_nom)) =
                self.session.as_ref().map(|s| (s.pid, s.jeu.clone(), s.proc_nom.clone()))
            && ctx.aide_prete()
        {
            self.fps_a_reprendre = false;
            if Self::mesure_fps(ctx) {
                self.demarrer_cible(ctx, pid, &jeu, &proc_nom, effets);
            }
        }
        // La partie mesurée : lignes de PresentMon toutes les 250 ms, le reste toutes les 500 ms
        if let Some(c) = &self.cible {
            let (l, k) = (c.prochaine_lecture, c.prochain_tick);
            if t >= l {
                self.lire_fps(ctx);
                if let Some(c) = self.cible.as_mut() {
                    c.prochaine_lecture = t + 0.25;
                }
            }
            if t >= k {
                if let Some(c) = self.cible.as_mut() {
                    c.prochain_tick = t + 0.5;
                }
                self.tick_cible(ctx, effets);
            }
        }
        if !self.bilans.is_empty() {
            self.bilans_finis(ctx, effets);
        }
        // Le jeu (toutes les 5 s)
        if t >= self.proch_detection {
            self.proch_detection = t + 5.0;
            self.detection(ctx, effets);
        }
        // Les mesures en direct : seulement l'écran regardé (pendant une partie, Nevermind doit être devant)
        // (`$Live.Fast`, signal « mesures-rapides » du bloc pc : un test des composants veut ses mesures toutes les
        //  400 ms, même écran caché)
        let regarde =
            self.rapide || (!cache && (self.session.is_none() || self.sys().devant().0 == std::process::id()));
        if regarde {
            if t >= self.proch_mesure {
                self.proch_mesure = t + if self.rapide { 0.4 } else { 1.0 };
                self.lire_mesures(Besoin::TOUT);
                effets.ajouter(Effet::Signal { nom: "mesures".into(), donnees: self.mesures.direct.json() });
                let temp = self.mesures.direct.gpu_temp;
                if let Some(x) = self.chaleur.tester(temp, t) {
                    Self::alerte_chaleur(ctx, effets, &x, temp.unwrap_or(0.0));
                }
            }
        } else if t >= self.proch_temperature {
            // (écran caché : la température seule, pour l'alerte ; plus souvent quand la carte chauffe)
            self.lire_mesures(Besoin::TEMPERATURE);
            let temp = self.mesures.direct.gpu_temp;
            if let Some(x) = self.chaleur.tester(temp, t) {
                Self::alerte_chaleur(ctx, effets, &x, temp.unwrap_or(0.0));
            }
            self.proch_temperature = t + if self.chaleur.chauffe() { 5.0 } else { 10.0 };
        }
        // Le prochain passage
        let mut prochain = self.proch_detection;
        if regarde {
            prochain = prochain.min(self.proch_mesure);
        } else {
            prochain = prochain.min(self.proch_temperature);
        }
        if let Some(c) = &self.cible {
            prochain = prochain.min(c.prochaine_lecture).min(c.prochain_tick);
        }
        if !self.bilans.is_empty() {
            prochain = prochain.min(t + 0.2);
        }
        if self.raccourci_actif == Some(true) {
            prochain = prochain.min(t + 1.0);
        }
        Some(Duration::from_secs_f64((prochain - t).clamp(0.05, 10.0)))
    }

    fn signal(&mut self, ctx: &Contexte, nom: &str, donnees: &Value, effets: &mut Effets) {
        match nom {
            // (un profil changé depuis la fiche d'un jeu, bloc jeux : les profils sont relus dans le registre)
            "profils-changes" => self.profils = None,
            // (bloc trafic : une mesure du lag commence ou finit, `$script:LagSession` de Update-FpsVeille)
            "mesure-lag" => self.lag_en_cours = vrai(donnees.get("en_cours")),
            "mesures-rapides" => {
                self.rapide = vrai(donnees.get("on"));
                if self.rapide {
                    self.proch_mesure = 0.0;
                }
            }
            "jeux-liste" => {
                let jeux = if donnees.is_array() { liste(Some(donnees)) } else { liste(donnees.get("jeux")) };
                self.index = jeu::index_jeux(&jeux);
                self.jeux = Some(jeux);
                self.profils = None;
                self.dernier = None;
                self.veille_anti_triche(ctx);
            }
            "reglages-jeu" | "demarrage" | "nettoyage" | "connexion" | "analyse-pc" | "memoire-constat"
            | "streameur" => {
                self.ext.insert(nom.into(), donnees.clone());
            }
            "diagnostic-fps" => {
                let id = ch(donnees.get("Id"));
                let parties = liste(fichier::lire(ctx, &ctx.donnees.join("fps.json")).as_ref());
                let Some(s) = parties
                    .iter()
                    .find(|p| ch(p.get("Id")) == id || (ch(p.get("Id")).is_empty() && ch(p.get("Date")) == id))
                else {
                    return;
                };
                let cle = ch(s.get("Key"));
                let connu = self.index.get(&cle);
                let nom_jeu = connu.map(|e| e.jeu.clone()).unwrap_or_else(|| ch(s.get("Game")));
                let jeu_connu = connu.is_some() || s.get("Diag").is_some_and(|d| vrai(d.get("Path")));
                let exes: Vec<Value> = match connu {
                    Some(e) => e.exes.iter().map(|x| json!(x)).collect(),
                    None => {
                        s.get("Diag").map(|d| vec![d.get("Path").cloned().unwrap_or(Value::Null)]).unwrap_or_default()
                    }
                };
                let prio = jeu::noms_exes_profil(&exes).first().is_some_and(|n| self.sys().priorite(n));
                let analyse = self.ext.get("analyse-pc").cloned().unwrap_or(Value::Null);
                let ecran = s.get("Diag").map(|d| ch(d.get("Display"))).unwrap_or_default();
                let ecran_a_regler = !ecran.is_empty()
                    && liste(analyse.get("analyse")).iter().any(|a| {
                        ch(a.get("id")) == format!("display:{ecran}") && !texte::egal(&ch(a.get("statut")), "ok")
                    });
                let nv = analyse.get("nvidia").filter(|n| !n.is_null()).map(|n| {
                    (
                        n.get("installe").filter(|x| !x.is_null()).map(|x| ch(Some(x))),
                        ch(n.get("dernier")),
                        ch(n.get("url")),
                    )
                });
                let c = diag::ContexteDiagnostic {
                    plan_ok: analyse.get("planOk").map(|p| vrai(Some(p))).unwrap_or(true),
                    jeu_connu,
                    prio,
                    nom_jeu,
                    autres_parties: parties.iter().filter(|p| ch(p.get("Key")) == cle && ch(p.get("Id")) != id).count(),
                    ecran_a_regler,
                    nvidia: nv,
                };
                let d = diag::diagnostic(s, &c);
                effets.ajouter(Effet::Signal {
                    nom: "diagnostic-fps-pret".into(),
                    donnees: json!({ "Id": id, "diagnostic": d.json() }),
                });
            }
            _ => {}
        }
    }

    fn fermer(&mut self, ctx: &Contexte) {
        // (Mindset arrête l'appli 6 s après lui avoir demandé de quitter, contrat partie 5 ; le PowerShell laissait
        //  3,5 s à l'action en cours. D'abord ce qui compte pour l'utilisateur : les applis fermées pour la partie
        //  relancées ; puis l'arrêt de la mesure des FPS en temps compté, le bilan gardé s'il est prêt à temps. Petit
        //  de l'audit du 08/11 : jusqu'à 8 s pour l'arrêt, puis 10 s pour le bilan, avant la relance.)
        let mut e = Effets::default();
        let cible = self.cible.take();
        if self.session.is_some() {
            self.fin_partie(ctx, &mut e);
        }
        if cible.is_some() {
            self.cible = cible;
            self.arreter_cible_en(ctx, Some(Duration::from_secs(1)));
        }
        // (lot G : un arrêt encore gardé, celui-ci ou un d'avant que l'Aide n'a pas reçu, est tenté une dernière fois,
        //  même pendant la pause, en 1,5 s au plus)
        ctx.envoyer_arrets_gardes(Duration::from_millis(1500));
        if self.raccourci_actif == Some(true) {
            self.compteur().raccourci(false);
        }
        if let Some(c) = self.compteur.as_deref_mut() {
            c.cacher();
        }
    }
}

/// Get-ChangementsProfil : ce que change un profil de jeu (« priority » : priorité haute de chaque programme du jeu ;
/// « gpu » : carte graphique puissante) ; éteint, les valeurs sont effacées
pub fn changements_profil(g: &Value, quoi: &str, on: bool) -> Vec<Value> {
    const IFEO: &str = r"HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options";
    const DX: &str = r"HKCU:\Software\Microsoft\DirectX\UserGpuPreferences";
    let exes = liste(g.get("exes"));
    if quoi == "priority" {
        jeu::noms_exes_profil(&exes)
            .into_iter()
            .map(|n| {
                if on {
                    json!({ "Type": "reg", "Path": format!(r"{IFEO}\{n}\PerfOptions"), "Name": "CpuPriorityClass", "Value": 3 })
                } else {
                    json!({ "Type": "reg", "Path": format!(r"{IFEO}\{n}\PerfOptions"), "Name": "CpuPriorityClass", "Efface": true })
                }
            })
            .collect()
    } else {
        exes.iter()
            .map(|e| {
                let e = ch(Some(e));
                if on {
                    json!({ "Type": "reg", "Path": DX, "Name": e, "Value": "GpuPreference=2;", "Kind": "String" })
                } else {
                    json!({ "Type": "reg", "Path": DX, "Name": e, "Efface": true })
                }
            })
            .collect()
    }
}
