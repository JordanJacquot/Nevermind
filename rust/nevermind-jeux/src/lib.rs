//! Nevermind en Rust : le bloc « jeux ». La bibliothèque de tous les jeux installés (bibliotheque.ps1, jeu.ps1,
//! optimisations.ps1), la page Jeux et la fiche d'un jeu (Get-PagesJeux, Register-ActionsJeux, Invoke-ActionJeux
//! d'ecran-jeux.ps1), l'optimisation d'un jeu et l'Organizer Dofus (organizer.ps1).
//!
//! Les pages sont les mêmes, au champ près, que celles de la version PowerShell. Les onglets FPS et Lag en ligne de la
//! page Jeux relisent les fichiers des autres blocs (fps.json, lag.json, historique.json), comme la version PowerShell.

pub mod biblio;
pub mod jaquettes;
pub mod json;
pub mod lancement;
pub mod optim;
pub mod organizer;
mod pages;
mod panneau_fps;
pub mod parties;
pub mod steam;
pub mod systeme;
pub mod texte;
#[cfg(windows)]
mod windows;

use biblio::Jeu;
use jaquettes::Jaquettes;
use lancement::{Desinstallation, Lancement};
use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use optim::Connu;
use parties::JournalParties;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use systeme::Systeme;
use texte::Moment;

/// Une question posée à l'utilisateur (comme Confirm-Action), en attente de sa réponse.
#[derive(Debug, Clone)]
enum Question {
    Desinstaller(Jeu),
    Optimiser(Jeu),
    SupprimerRestes(Vec<String>),
}

/// Une désinstallation suivie (Watch-Uninstall) : toutes les 10 s pendant 15 min, le jeu a-t-il disparu ?
#[derive(Debug, Clone)]
struct Surveillance {
    nom: String,
    exe: String,
    jusqua: Instant,
    prochaine: Instant,
}

type Resultat<T> = Arc<Mutex<Option<T>>>;

pub struct BlocJeux {
    sys: Arc<dyn Systeme>,
    /// La liste des jeux ($script:Games) ; `None` : pas encore cherchée.
    jeux: Option<Vec<Jeu>>,
    /// Les restes de jeux Steam désinstallés ($script:Leftovers) et les bibliothèques à l'analyse incomplète.
    restes: Vec<steam::Reste>,
    incomplets: Vec<String>,
    index: HashMap<String, biblio::Indexe>,
    recherche_en_cours: Option<Resultat<biblio::Decouverte>>,
    filtre: String,
    recherche: String,
    choisi: Option<Jeu>,
    montres: Vec<Jeu>,
    jeu_module: Option<Jeu>,
    optims: Vec<optim::Point>,
    jaquettes: Jaquettes,
    connu: Connu,
    /// Le jeu en cours (signal « partie-commencee » du bloc qui guette les jeux) ($script:GameSession.Game).
    en_jeu: Option<String>,
    /// Tailles et ancienneté des restes ($script:LeftoverSizes, $script:LeftoverRecents).
    tailles_restes: HashMap<String, (f64, bool)>,
    mesure_restes: Option<Resultat<Vec<(String, f64, bool)>>>,
    suppression: Option<Resultat<Vec<steam::Suppression>>>,
    restes_ouverts: bool,
    /// La partie mesurée dont le détail est ouvert, et les diagnostics reçus du bloc perf (par Id).
    partie_ouverte: Option<String>,
    diagnostics: HashMap<String, Value>,
    question: Option<(String, Question)>,
    numero_question: u64,
    surveillances: Vec<Surveillance>,
    lancement_jeu: Option<(String, Instant)>,
    dernier_jeu: Option<(Instant, Option<DernierJeu>)>,
    applis: Option<Vec<(String, String)>>,
    /// L'heure des pages (les tests la fixent).
    pub heure: Option<Moment>,
}

/// Le dernier jeu joué (Get-LastPlayedGame).
#[derive(Debug, Clone)]
pub struct DernierJeu {
    pub jeu: Jeu,
    pub derniere: Option<parties::PartieFps>,
}

impl Default for BlocJeux {
    fn default() -> Self {
        Self::avec(Arc::new(systeme::SystemeWindows))
    }
}

fn texte_valeur(v: &Value) -> String {
    json::texte_de(v)
}

impl BlocJeux {
    /// Le bloc avec un Windows donné (les tests donnent un faux Windows).
    pub fn avec(sys: Arc<dyn Systeme>) -> Self {
        BlocJeux {
            sys,
            jeux: None,
            restes: Vec::new(),
            incomplets: Vec::new(),
            index: HashMap::new(),
            recherche_en_cours: None,
            filtre: String::new(),
            recherche: String::new(),
            choisi: None,
            montres: Vec::new(),
            jeu_module: None,
            optims: Vec::new(),
            jaquettes: Jaquettes::default(),
            connu: Connu::default(),
            en_jeu: None,
            tailles_restes: HashMap::new(),
            mesure_restes: None,
            suppression: None,
            restes_ouverts: false,
            partie_ouverte: None,
            diagnostics: HashMap::new(),
            question: None,
            numero_question: 0,
            surveillances: Vec::new(),
            lancement_jeu: None,
            dernier_jeu: None,
            applis: None,
            heure: None,
        }
    }

    fn maintenant(&self) -> Moment {
        self.heure.unwrap_or_else(|| self.sys.maintenant())
    }

    /// Les jeux trouvés (pour les tests et les autres parties de l'appli).
    pub fn jeux(&self) -> Option<&[Jeu]> {
        self.jeux.as_deref()
    }

    /// Choisit le jeu de la fiche par son nom (comme un clic sur sa jaquette ; pour les tests).
    pub fn choisir(&mut self, nom: &str) {
        self.choisi = self.bibliotheque().into_iter().find(|g| g.nom == nom);
    }

    /// Le texte de la barre des restes pour des restes donnés (nom, dossier, taille en octets, récent) : pour les tests.
    pub fn essai_barre_restes(&mut self, restes: &[(String, String, f64, bool)], incomplet: bool) -> Option<String> {
        self.restes =
            restes.iter().map(|r| steam::Reste { nom: r.0.clone(), dossier: r.1.clone(), incomplet: false }).collect();
        self.tailles_restes = restes.iter().map(|r| (r.1.clone(), (r.2, r.3))).collect();
        self.incomplets = if incomplet { vec!["X".into()] } else { Vec::new() };
        self.barre_restes()
    }

    /// Un réglage écrit, et les autres blocs prévenus (le bloc perf relit la mesure et le compteur).
    fn ecrire(&self, ctx: &Contexte, nom: &str, v: Value, effets: &mut Effets) {
        if let Err(e) = ctx.ecrire_reglage(nom, v.clone(), false) {
            ctx.journal(&e);
        }
        effets.ajouter(Effet::Signal { nom: "reglage-change".into(), donnees: json!({ "nom": nom, "valeur": v }) });
        effets.ajouter(Effet::Rafraichir);
    }

    /// Le détail d'une partie (Show-FpsSession) : le panneau, et son diagnostic demandé au bloc perf.
    fn ouvrir_partie(&mut self, id: &str, effets: &mut Effets) {
        self.partie_ouverte = Some(id.to_string());
        self.restes_ouverts = false;
        if !self.diagnostics.contains_key(id) {
            effets.ajouter(Effet::Signal { nom: "diagnostic-fps".into(), donnees: json!({ "Id": id }) });
        }
        effets.ajouter(Effet::OuvrirEcran("panneau".into()));
    }

    /// « Mes FPS ne sont pas normaux » (Invoke-FpsHelp) : le détail de la dernière partie, ou comment en mesurer une.
    fn aide_fps(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let mut l = parties::parties_fps(Some(ctx), &ctx.donnees);
        texte::trier_ps(&mut l, |a, c| c.moment.cmp(&a.moment));
        if let Some(s) = l.first()
            && s.brut.get("Diag").map(json::vrai).unwrap_or(false)
        {
            let id = s.id.clone();
            self.ouvrir_partie(&id, effets);
            return;
        }
        if !Self::mesure_fps(ctx) {
            self.ecrire(ctx, "FpsMeasure", Value::Bool(true), effets);
        }
        effets.ajouter(Effet::Signal {
            nom: "resultat".into(),
            donnees: json!({ "titre": "Trouvons d'où viennent tes problèmes de FPS", "lignes": [
                "1.  La mesure des FPS est activée.",
                "2.  Lance ton jeu et joue au moins 5 minutes, de préférence là où ça rame.",
                "3.  Si Nevermind ne reconnaît pas le jeu, appuie sur Ctrl + Maj + F en jeu (ou ajoute-le dans Profils par jeu).",
                "4.  Quitte le jeu : Nevermind t'explique d'où vient le problème et ce qu'il peut régler pour toi."
            ], "note": "Nevermind regarde qui freine (carte graphique ou processeur), la température, la mémoire, le disque et les programmes en arrière-plan." }),
        });
    }

    /// Le temps de jeu, l'Organizer, les jeux ajoutés : relus du disque.
    fn journal(&self, ctx: &Contexte) -> JournalParties {
        JournalParties::lire(Some(ctx), &ctx.donnees)
    }

    /// La bibliothèque (Get-LibraryGames).
    pub fn bibliotheque(&self) -> Vec<Jeu> {
        self.jeux.as_deref().map(biblio::bibliotheque).unwrap_or_default()
    }

    /// Cherche les jeux tout de suite (Update-GameCache, sans fil séparé : pour les tests et l'actualisation).
    pub fn rechercher_maintenant(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let d = biblio::decouvrir(self.sys.as_ref());
        self.recevoir_jeux(ctx, d, effets);
    }

    /// Lance la recherche des jeux dans un fil séparé (comme Invoke-Async).
    fn lancer_recherche(&mut self) {
        if self.recherche_en_cours.is_some() {
            return;
        }
        let r: Resultat<biblio::Decouverte> = Arc::new(Mutex::new(None));
        let r2 = r.clone();
        let sys = self.sys.clone();
        std::thread::spawn(move || {
            let d = biblio::decouvrir(sys.as_ref());
            *r2.lock().unwrap_or_else(|e| e.into_inner()) = Some(d);
        });
        self.recherche_en_cours = Some(r);
    }

    fn recevoir_jeux(&mut self, ctx: &Contexte, d: biblio::Decouverte, effets: &mut Effets) {
        self.restes = d.restes.iter().filter(|r| !r.incomplet).cloned().collect();
        self.incomplets = d.restes.iter().filter(|r| r.incomplet).map(|r| r.dossier.clone()).collect();
        if !self.incomplets.is_empty() {
            ctx.journal(&format!(
                "Bibliothèque: analyse des restes de jeux incomplète, une fiche de Steam n'a pas pu être lue ({})",
                self.incomplets.join(", ")
            ));
        }
        let ajoutes = biblio::jeux_ajoutes(ctx.reglage("CustomGames").as_ref());
        let tous = biblio::fusionner(d.jeux, ajoutes);
        self.index = biblio::index(&tous);
        self.jeux = Some(tous);
        self.jaquettes.oublier();
        self.applis = None;
        self.dernier_jeu = None;
        self.tailles_restes.retain(|k, _| self.restes.iter().any(|r| &r.dossier == k));
        // Les autres blocs (mode jeu, FPS, profils) apprennent la liste
        effets.ajouter(Effet::Signal { nom: "jeux-liste".into(), donnees: self.jeux_pour_signal() });
        // (signaux communs : le bloc mémoire attend les programmes des jeux de la bibliothèque)
        let exes: Vec<String> = self.bibliotheque().iter().flat_map(|g| g.exes.clone()).collect();
        effets.ajouter(Effet::Signal { nom: "jeux-bibliotheque".into(), donnees: json!({ "exes": exes }) });
        // (le bloc trafic attend les programmes de tous les jeux connus, ajoutés à la main compris)
        let connus: Vec<String> = self.jeux.iter().flatten().flat_map(|g| g.exes.clone()).collect();
        effets.ajouter(Effet::Signal { nom: "jeux-index".into(), donnees: json!({ "Exes": connus }) });
        effets.ajouter(Effet::Rafraichir);
    }

    fn jeux_pour_signal(&self) -> Value {
        let l: Vec<Value> = self
            .jeux
            .iter()
            .flatten()
            .map(|g| {
                json!({ "nom": g.nom, "exes": g.exes, "source": g.source, "dossier": g.dossier, "appid": g.appid,
                    "custom": g.ajoute })
            })
            .collect();
        json!({ "jeux": l })
    }

    fn applis(&mut self) -> Vec<(String, String)> {
        if self.applis.is_none() {
            self.applis = Some(self.sys.applis_demarrer());
        }
        self.applis.clone().unwrap_or_default()
    }

    fn comment_lancer(&mut self, g: &Jeu) -> Lancement {
        if g.lancement.to_lowercase().starts_with("xbox:") {
            let a = self.applis();
            lancement::lancement(g, &|| a.clone())
        } else {
            lancement::lancement(g, &Vec::new)
        }
    }

    /// Le dernier jeu joué, gardé 20 s (Get-DernierJeuKit, Get-LastPlayedGame).
    fn dernier_jeu(&mut self, ctx: &Contexte, log: &JournalParties, fps: &[parties::PartieFps]) -> Option<DernierJeu> {
        if let Some((t, j)) = &self.dernier_jeu
            && t.elapsed() < Duration::from_secs(20)
        {
            return j.clone();
        }
        let _ = ctx;
        let j = self.calculer_dernier_jeu(log, fps);
        self.dernier_jeu = Some((Instant::now(), j.clone()));
        j
    }

    fn calculer_dernier_jeu(&self, log: &JournalParties, fps: &[parties::PartieFps]) -> Option<DernierJeu> {
        self.jeux.as_ref()?;
        let jeux = self.bibliotheque();
        let mut meilleur: Option<(&Jeu, Moment)> = None;
        for g in &jeux {
            let Some(e) = log.get(&g.nom) else { continue };
            if e.last.is_empty() {
                continue;
            }
            let Some(d) = texte::lire_date(&e.last) else { continue };
            if meilleur.map(|(_, m)| d > m).unwrap_or(true) {
                meilleur = Some((g, d));
            }
        }
        let (g, _) = meilleur?;
        let mut ss: Vec<&parties::PartieFps> =
            fps.iter().filter(|s| self.nom_partie(s).to_lowercase() == g.nom.to_lowercase()).collect();
        crate::texte::trier_ps(&mut ss, |a, c| a.moment.cmp(&c.moment));
        Some(DernierJeu { jeu: g.clone(), derniere: ss.last().map(|s| (*s).clone()) })
    }

    /// Le nom affiché d'une partie (Get-SessionName) : le vrai nom du jeu s'il est connu.
    fn nom_partie(&self, s: &parties::PartieFps) -> String {
        if !s.cle.is_empty()
            && let Some(x) = self.index.get(&s.cle.to_lowercase())
        {
            return x.jeu.clone();
        }
        s.jeu.clone()
    }

    fn adresse_jaquette(&mut self, ctx: &Contexte, g: &Jeu, tous: &[Jeu]) -> Value {
        let sys = self.sys.clone();
        match self.jaquettes.jaquette(sys.as_ref(), &ctx.donnees, g, tous, false) {
            Some(f) => Value::String(self.jaquettes.adresse(&f)),
            None => Value::Null,
        }
    }

    fn reglage_bool(ctx: &Contexte, nom: &str, defaut: bool) -> bool {
        match ctx.reglage(nom) {
            Some(v) => json::vrai(&v),
            None => defaut,
        }
    }

    /// « Mesurer mes FPS » (Test-FpsMeasure) : FpsMeasure, sinon l'ancien réglage de l'overlay.
    fn mesure_fps(ctx: &Contexte) -> bool {
        match ctx.reglage("FpsMeasure") {
            Some(v) => json::vrai(&v),
            None => Self::reglage_bool(ctx, "FpsOverlay", false),
        }
    }

    fn organizer_visible(&self, ctx: &Contexte) -> bool {
        let cfg = organizer::Config::lire(Some(ctx), &ctx.donnees);
        let noms: Vec<&str> = self.jeux.iter().flatten().map(|g| g.nom.as_str()).collect();
        organizer::visible(ctx.reglage("OrganizerAffiche").as_ref(), cfg.on, &noms)
    }

    fn statut(&self, effets: &mut Effets, texte: &str) {
        effets.ajouter(Effet::Annonce(texte.to_string()));
    }

    /// Show-Message : dans l'écran, une bulle (le noyau n'a pas encore de message à bouton OK).
    fn message(&self, effets: &mut Effets, texte: &str) {
        effets.ajouter(Effet::Annonce(texte.to_string()));
    }

    fn poser_question(&mut self, effets: &mut Effets, q: Question, texte: &str, details: Vec<Value>) {
        self.numero_question += 1;
        let id = format!("jeux-{}", self.numero_question);
        effets.ajouter(Effet::Signal {
            nom: "question".into(),
            // (signaux communs : l'hôte renvoie l'action « jeux-reponse » avec {id, valeur: bouton choisi})
            donnees: json!({ "id": id, "titre": "", "texte": texte, "details": details, "action": "jeux-reponse",
                "boutons": [{ "id": "oui", "texte": "Oui" }, { "id": "non", "texte": "Non", "style": "second" }] }),
        });
        self.question = Some((id, q));
    }

    // ------------------------------------------------------------------ gestes

    /// Lancer un jeu (Start-LibraryGame).
    fn lancer_jeu(&mut self, ctx: &Contexte, g: &Jeu, effets: &mut Effets) {
        let comment = self.comment_lancer(g);
        let (chemin, par_url) = match &comment {
            Lancement::Exe { chemin, .. } => (chemin.clone(), false),
            Lancement::Adresse(a) => (a.clone(), true),
        };
        if !par_url && !std::path::Path::new(&chemin).exists() {
            self.message(
                effets,
                &format!(
                    "Le jeu « {} » est introuvable :\n{chemin}\n\nIl a peut-être été désinstallé ou déplacé. Clique sur « Actualiser ».",
                    g.nom
                ),
            );
            return;
        }
        ctx.journal(&format!("Bibliothèque: lancement de {} ({chemin})", g.nom));
        match comment {
            Lancement::Exe { chemin, arguments } => {
                if ctx.essai {
                    ctx.journal("Bibliothèque: copie de test, le jeu n'est pas lancé");
                } else if let Err(e) = self.sys.lancer(&chemin, &arguments, &texte::dossier_parent(&chemin)) {
                    ctx.journal(&format!("Bibliothèque: lancement impossible ({e})"));
                }
            }
            Lancement::Adresse(a) => effets.ajouter(Effet::OuvrirAdresse(a)),
        }
        self.lancement_jeu = Some((g.nom.clone(), Instant::now()));
        let par = if !g.source.is_empty() && par_url { format!(" par {}", g.source) } else { String::new() };
        self.statut(effets, &format!("Lancement de {}{par}...", g.nom));
    }

    /// Désinstaller un jeu (Uninstall-LibraryGame) : la question d'abord, puis `desinstaller_confirme`.
    fn desinstaller(&mut self, ctx: &Contexte, g: &Jeu, effets: &mut Effets) {
        let src = g.source_affichee().to_string();
        match lancement::desinstallation(g) {
            Desinstallation::Aucune => {
                self.message(effets, &format!("Nevermind ne connaît pas le désinstalleur de « {} ».\n\nLa liste des applications de Windows va s'ouvrir : cherche le jeu et clique sur « Désinstaller ».", g.nom));
                effets.ajouter(Effet::OuvrirAdresse("ms-settings:appsfeatures".into()));
            }
            Desinstallation::Launcher(a) => {
                self.message(effets, &format!("« {} » se désinstalle depuis {src} : Nevermind l'ouvre pour toi.\n\nDans {src}, fais un clic droit sur le jeu (ou ouvre ses options), puis « Désinstaller ».", g.nom));
                effets.ajouter(Effet::OuvrirAdresse(a));
                self.apres_desinstallation(ctx, g, effets);
            }
            _ => {
                let t = format!(
                    "Désinstaller « {} » ?\n\nLe désinstalleur de {src} va s'ouvrir et te demander de confirmer. Tes sauvegardes dans le cloud ne sont pas touchées.",
                    g.nom
                );
                self.poser_question(effets, Question::Desinstaller(g.clone()), &t, Vec::new());
            }
        }
    }

    fn desinstaller_confirme(&mut self, ctx: &Contexte, g: &Jeu, effets: &mut Effets) {
        match lancement::desinstallation(g) {
            Desinstallation::Adresse(a) => effets.ajouter(Effet::OuvrirAdresse(a)),
            Desinstallation::Exe { chemin, arguments } => {
                if !std::path::Path::new(&chemin).exists() {
                    self.message(
                        effets,
                        &format!(
                            "Le désinstalleur est introuvable :\n{chemin}\n\nDésinstalle le jeu depuis {}.",
                            g.source_affichee()
                        ),
                    );
                    return;
                }
                if ctx.essai {
                    ctx.journal("Bibliothèque: copie de test, le désinstalleur n'est pas lancé");
                } else if let Err(e) = self.sys.lancer(&chemin, &arguments, &texte::dossier_parent(&chemin)) {
                    ctx.journal(&format!("Bibliothèque: désinstalleur impossible à lancer ({e})"));
                }
            }
            _ => return,
        }
        self.apres_desinstallation(ctx, g, effets);
    }

    fn apres_desinstallation(&mut self, ctx: &Contexte, g: &Jeu, effets: &mut Effets) {
        let src = g.source_affichee().to_string();
        ctx.journal(&format!("Bibliothèque: désinstallation de {} demandée ({src})", g.nom));
        self.statut(
            effets,
            &format!(
                "Désinstallation de {} : suis les instructions de {src}. La liste se mettra à jour toute seule.",
                g.nom
            ),
        );
        let maintenant = Instant::now();
        self.surveillances.push(Surveillance {
            nom: g.nom.clone(),
            exe: g.premier_exe().to_string(),
            jusqua: maintenant + Duration::from_secs(15 * 60),
            prochaine: maintenant + Duration::from_secs(10),
        });
    }

    /// Un point d'optimisation changé (Set-GameOptimization).
    fn changer_optimisation(&mut self, ctx: &Contexte, nom: &str, id: &str, on: bool, effets: &mut Effets) {
        let Some(g) = self.bibliotheque().into_iter().find(|g| g.nom == nom) else { return };
        match id {
            "priority" | "gpu" => self.changer_profil(&g, id, on, effets),
            "fps" => {
                if let Err(e) = ctx.ecrire_reglage("FpsMeasure", Value::Bool(on), false) {
                    ctx.journal(&e);
                }
                effets.ajouter(Effet::Signal {
                    nom: "reglage-change".into(),
                    donnees: json!({ "nom": "FpsMeasure", "valeur": on }),
                });
            }
            "mode" => {
                if let Err(e) = ctx.ecrire_reglage("GameMode", Value::Bool(on), false) {
                    ctx.journal(&e);
                }
                effets.ajouter(Effet::Signal {
                    nom: "reglage-change".into(),
                    donnees: json!({ "nom": "GameMode", "valeur": on }),
                });
            }
            _ => {}
        }
    }

    /// Un profil de jeu (Set-GameProfile) : les changements partent à la sauvegarde (avec l'historique), qui les fait.
    fn changer_profil(&mut self, g: &Jeu, quoi: &str, on: bool, effets: &mut Effets) {
        let label = optim::libelle_profil(quoi);
        let etat = if on { "activée" } else { "désactivée" };
        effets.ajouter(Effet::Signal {
            nom: "declarations".into(),
            donnees: json!({ "titre": format!("Profil de {} : {label} {etat}", g.nom),
                "changements": optim::changements_profil(g, quoi, on) }),
        });
        self.statut(effets, &format!("{} : {label} {etat}.", g.nom));
        effets.ajouter(Effet::Signal { nom: "profils-changes".into(), donnees: json!({ "jeu": g.nom }) });
    }

    /// « Tout optimiser » (Invoke-GameOptimize) : la question d'abord.
    fn optimiser(&mut self, ctx: &Contexte, g: &Jeu, effets: &mut Effets) {
        let points = optim::points(
            self.sys.as_ref(),
            &self.connu,
            g,
            Self::mesure_fps(ctx),
            Self::reglage_bool(ctx, "GameMode", false),
        );
        let items: Vec<optim::Point> = optim::a_faire(&points).into_iter().cloned().collect();
        if items.is_empty() {
            self.statut(effets, &format!("{} est déjà optimisé.", g.nom));
            return;
        }
        let mut details = Vec::new();
        for it in &items {
            match it.id.as_str() {
                "priority" | "gpu" => details.extend(optim::changements_profil(g, &it.id, true)),
                "fps" => details.push(json!({ "Type": "texte", "Texte": "Mesure des FPS de Nevermind : maintenant coupée, après activée (réglage de Nevermind, rien ne change dans Windows)." })),
                "tweaks" => details.push(json!({ "Type": "reglages-windows", "Ids": it.ids })),
                _ => {}
            }
        }
        let liste: Vec<String> = items.iter().map(|i| format!("•  {}", i.titre)).collect();
        let t = format!(
            "Optimiser {} ?\n\n{}\n\n« Détails techniques » montre chaque réglage, sa valeur maintenant et après. Chacun de ces réglages s'annule depuis la page Sauvegarde.",
            g.nom,
            liste.join("\n")
        );
        self.poser_question(effets, Question::Optimiser(g.clone()), &t, details);
    }

    fn optimiser_confirme(&mut self, ctx: &Contexte, g: &Jeu, effets: &mut Effets) {
        let points = optim::points(
            self.sys.as_ref(),
            &self.connu,
            g,
            Self::mesure_fps(ctx),
            Self::reglage_bool(ctx, "GameMode", false),
        );
        let items: Vec<optim::Point> = optim::a_faire(&points).into_iter().cloned().collect();
        let mut faits = Vec::new();
        for it in &items {
            match it.id.as_str() {
                "priority" => {
                    self.changer_profil(g, "priority", true, effets);
                    faits.push("Priorité haute");
                }
                "gpu" => {
                    self.changer_profil(g, "gpu", true, effets);
                    faits.push("Carte graphique puissante");
                }
                "fps" => {
                    if let Err(e) = ctx.ecrire_reglage("FpsMeasure", Value::Bool(true), false) {
                        ctx.journal(&e);
                    }
                    effets.ajouter(Effet::Signal {
                        nom: "reglage-change".into(),
                        donnees: json!({ "nom": "FpsMeasure", "valeur": true }),
                    });
                    faits.push("Mesure des FPS");
                }
                _ => {}
            }
        }
        match items.iter().find(|i| i.id == "tweaks") {
            Some(tw) => effets
                .ajouter(Effet::Signal { nom: "appliquer-reglages-windows".into(), donnees: json!({ "ids": tw.ids }) }),
            None => {
                let mut lignes = vec!["Fait :".to_string()];
                lignes.extend(faits.iter().map(|f| format!("•  {f}")));
                lignes.push("Joue une partie : Nevermind mesurera tes FPS pour vérifier.".into());
                effets.ajouter(Effet::Signal {
                    nom: "resultat".into(),
                    donnees: json!({ "titre": format!("{} est optimisé", g.nom), "lignes": lignes }),
                });
            }
        }
    }

    /// Ajouter un jeu à la main (Add-CustomGame).
    pub fn ajouter_jeu(&mut self, ctx: &Contexte, exe: Option<String>, nom: Option<String>, effets: &mut Effets) {
        let exe = match exe {
            Some(e) => e,
            None => match self.sys.choisir_exe() {
                Some(e) => e,
                None => return,
            },
        };
        let nom = match nom {
            Some(n) => n,
            None => {
                let (produit, desc) = self.sys.infos_programme(&exe);
                [produit, desc, texte::nom_sans_extension(&exe)]
                    .into_iter()
                    .find(|x| !x.trim().is_empty() && x.chars().count() < 60)
                    .unwrap_or_default()
            }
        };
        let base = texte::nom_sans_extension(&exe).to_lowercase();
        if base.chars().count() < 3 || biblio::EXE_GENERIQUE.is_match(&base) {
            self.message(effets, &format!("Le nom « {} » est trop courant pour reconnaître le jeu à coup sûr.\n\nChoisis plutôt le programme qui porte le nom du jeu (souvent dans un sous-dossier comme Binaries\\Win64).", texte::nom_fichier(&exe)));
            return;
        }
        let mut l: Vec<Value> = biblio::liste(ctx.reglage("CustomGames").as_ref())
            .into_iter()
            .filter(|x| json::texte(x, "Exe") != exe)
            .collect();
        l.push(json!({ "Name": nom, "Exe": exe }));
        if let Err(e) = ctx.ecrire_reglage("CustomGames", Value::Array(l), false) {
            ctx.journal(&e);
        }
        self.rechercher_maintenant(ctx, effets);
        self.statut(effets, &format!("{nom} ajouté à tes jeux : Nevermind le reconnaîtra à son lancement."));
    }

    /// Retirer un jeu ajouté à la main (Remove-CustomGame).
    pub fn retirer_jeu(&mut self, ctx: &Contexte, exe: &str, effets: &mut Effets) {
        let l: Vec<Value> = biblio::liste(ctx.reglage("CustomGames").as_ref())
            .into_iter()
            .filter(|x| json::vrai(x) && json::texte(x, "Exe") != exe)
            .collect();
        if let Err(e) = ctx.ecrire_reglage("CustomGames", Value::Array(l), false) {
            ctx.journal(&e);
        }
        self.rechercher_maintenant(ctx, effets);
        self.statut(effets, "Jeu retiré de la liste.");
    }

    /// Montrer ou cacher l'Organizer (Set-OrgVisible). Caché : il est aussi coupé.
    fn organizer_affiche(&mut self, ctx: &Contexte, oui: bool, effets: &mut Effets) {
        if let Err(e) = ctx.ecrire_reglage("OrganizerAffiche", Value::Bool(oui), false) {
            ctx.journal(&e);
        }
        if !oui {
            let mut c = organizer::Config::lire(Some(ctx), &ctx.donnees);
            if c.on {
                c.on = false;
                c.ecrire(Some(ctx), &ctx.donnees);
                effets.ajouter(Effet::Signal { nom: "organizer".into(), donnees: json!({ "on": false }) });
                self.statut(effets, "Organizer Dofus coupé.");
            }
        }
    }

    // ------------------------------------------------------------------ restes de jeux désinstallés

    /// Les restes à proposer (Get-BigLeftovers) : plus de 50 Mo, pas récents, les plus gros d'abord.
    fn gros_restes(&self) -> Vec<(steam::Reste, f64)> {
        let mut v: Vec<(steam::Reste, f64)> = self
            .restes
            .iter()
            .filter_map(|r| {
                let (t, recent) = *self.tailles_restes.get(&r.dossier)?;
                (t >= 50.0 * 1024.0 * 1024.0 && !recent && std::path::Path::new(&r.dossier).exists())
                    .then(|| (r.clone(), t))
            })
            .collect();
        crate::texte::trier_ps(&mut v, |a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    fn texte_incomplet(&self) -> &'static str {
        if self.incomplets.is_empty() {
            ""
        } else {
            " Analyse incomplète : une fiche de Steam n'a pas pu être lue, les dossiers de cette bibliothèque ne sont pas proposés."
        }
    }

    /// La barre des restes (Update-LeftoverBar) : son texte, ou `None` si elle est cachée. Lance la mesure des tailles.
    fn barre_restes(&mut self) -> Option<String> {
        let liste: Vec<steam::Reste> =
            self.restes.iter().filter(|r| std::path::Path::new(&r.dossier).exists()).cloned().collect();
        if liste.is_empty() {
            return None;
        }
        let manquants: Vec<String> =
            liste.iter().filter(|r| !self.tailles_restes.contains_key(&r.dossier)).map(|r| r.dossier.clone()).collect();
        if !manquants.is_empty() {
            if self.mesure_restes.is_none() {
                let r: Resultat<Vec<(String, f64, bool)>> = Arc::new(Mutex::new(None));
                let r2 = r.clone();
                std::thread::spawn(move || {
                    let v = manquants
                        .into_iter()
                        .map(|d| {
                            let (t, rec) = steam::mesurer_reste(&d, 7);
                            (d, t, rec)
                        })
                        .collect();
                    *r2.lock().unwrap_or_else(|e| e.into_inner()) = Some(v);
                });
                self.mesure_restes = Some(r);
            }
            return None;
        }
        let gros = self.gros_restes();
        if gros.is_empty() {
            return None;
        }
        let taille: f64 = gros.iter().map(|x| x.1).sum();
        let n = gros.len();
        let mut noms = gros.iter().take(3).map(|x| x.0.nom.clone()).collect::<Vec<_>>().join(", ");
        if n > 3 {
            noms.push_str("...");
        }
        Some(format!(
            "{n} jeu{} désinstallé{} {} laissé {} sur le disque ({noms}) : {} à récupérer.{}",
            if n > 1 { "x" } else { "" },
            if n > 1 { "s" } else { "" },
            if n > 1 { "ont" } else { "a" },
            if n > 1 { "leur dossier" } else { "son dossier" },
            taille_lisible(taille),
            self.texte_incomplet()
        ))
    }

    /// Supprimer des restes (Remove-Leftovers) : garde-fous, question, puis suppression dans un fil séparé.
    fn supprimer_restes(&mut self, dossiers: Vec<String>, force: bool, effets: &mut Effets) {
        if self.suppression.is_some() {
            return;
        }
        let installes: Vec<String> =
            self.jeux.iter().flatten().map(|g| g.dossier.to_lowercase()).filter(|d| !d.is_empty()).collect();
        let connus: Vec<String> = self.restes.iter().map(|r| r.dossier.to_lowercase()).collect();
        static FORME: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)\\steamapps\\common\\[^\\]+$").unwrap());
        let ok: Vec<String> = dossiers
            .into_iter()
            .filter(|d| {
                FORME.is_match(d)
                    && connus.contains(&d.to_lowercase())
                    && !installes.contains(&d.to_lowercase())
                    && std::path::Path::new(d).exists()
            })
            .collect();
        if ok.is_empty() {
            self.message(effets, "Aucun de ces dossiers ne peut être supprimé par Nevermind.");
            return;
        }
        if !force {
            let taille: f64 = ok.iter().map(|d| self.tailles_restes.get(d).map(|x| x.0).unwrap_or(0.0)).sum();
            let quoi = if ok.len() == 1 {
                let nom = self.restes.iter().find(|r| r.dossier == ok[0]).map(|r| r.nom.clone()).unwrap_or_default();
                format!("le dossier de « {nom} »")
            } else {
                format!("{} dossiers de jeux désinstallés", ok.len())
            };
            let t = format!(
                "Supprimer définitivement {quoi} ({}) ?\n\nIls ne passent pas par la corbeille : la place est libérée tout de suite, mais ils ne pourront pas être récupérés. Les sauvegardes ou mods qu'ils contiennent seront perdus.",
                taille_lisible(taille)
            );
            self.poser_question(effets, Question::SupprimerRestes(ok), &t, Vec::new());
            return;
        }
        let r: Resultat<Vec<steam::Suppression>> = Arc::new(Mutex::new(None));
        let r2 = r.clone();
        std::thread::spawn(move || {
            let v = steam::supprimer_restes(&ok);
            *r2.lock().unwrap_or_else(|e| e.into_inner()) = Some(v);
        });
        self.suppression = Some(r);
    }

    fn recevoir_suppression(&mut self, ctx: &Contexte, res: Vec<steam::Suppression>, effets: &mut Effets) {
        let mut libere = 0.0;
        let mut echecs = Vec::new();
        for s in res {
            let nom = self.restes.iter().find(|r| r.dossier == s.dossier).map(|r| r.nom.clone());
            if !s.raison.is_empty() {
                echecs.push(format!("{} : {}", nom.unwrap_or_else(|| s.dossier.clone()), s.raison));
                if s.garde {
                    self.restes.retain(|r| r.dossier != s.dossier);
                }
            } else {
                libere += self.tailles_restes.get(&s.dossier).map(|x| x.0).unwrap_or(0.0);
                self.restes.retain(|r| r.dossier != s.dossier);
            }
        }
        ctx.journal(&format!(
            "Bibliothèque: {} libérés{}",
            taille_lisible(libere),
            if echecs.is_empty() { String::new() } else { format!(", échecs : {}", echecs.join(" | ")) }
        ));
        self.statut(effets, &format!("{} libérés sur le disque.", taille_lisible(libere)));
        if self.restes_ouverts && self.gros_restes().is_empty() {
            self.restes_ouverts = false;
        }
        let mut lignes = vec![format!("{} libérés sur le disque.", taille_lisible(libere))];
        if !echecs.is_empty() {
            lignes.push("Pas supprimés :".into());
            lignes.extend(echecs.iter().map(|e| format!("•  {e}")));
        }
        let titre = if echecs.is_empty() { "Place libérée" } else { "Suppression terminée, avec des exceptions" };
        effets.ajouter(Effet::Signal { nom: "resultat".into(), donnees: json!({ "titre": titre, "lignes": lignes }) });
        effets.ajouter(Effet::Rafraichir);
    }

    /// Le panneau des restes (Show-Leftovers), en page « panneau » de l'écran.
    fn panneau_restes(&self) -> Value {
        let l = self.gros_restes();
        let n = l.len();
        let mut blocs = vec![
            json!({ "type": "texte", "texte": format!("Ces jeux ne sont plus installés (Steam ne les connaît plus), mais leur dossier est resté sur le disque. La suppression est définitive : ils ne passent pas par la corbeille, la place est libérée tout de suite et ils ne peuvent pas être récupérés. S'ils contiennent des sauvegardes ou des mods, ouvre-les pour vérifier avant.{}", self.texte_incomplet()) }),
        ];
        if n > 1 && self.suppression.is_none() {
            let total: f64 = l.iter().map(|x| x.1).sum();
            blocs.push(json!({ "type": "bouton", "id": "jeux-restes-tout", "texte": format!("Tout supprimer définitivement ({})", taille_lisible(total)), "style": null }));
        }
        for (k, (r, t)) in l.iter().enumerate() {
            blocs.push(json!({ "type": "titre", "texte": format!("{}  ·  {}", r.nom, taille_lisible(*t)) }));
            blocs.push(json!({ "type": "texte", "texte": r.dossier }));
            blocs.push(json!({ "type": "bouton", "id": format!("jeux-reste-ouvrir:{k}"), "texte": "Ouvrir", "style": "second" }));
            if self.suppression.is_none() {
                blocs.push(json!({ "type": "bouton", "id": format!("jeux-reste-supprimer:{k}"), "texte": "Supprimer", "style": "second" }));
            }
        }
        let etat = if self.suppression.is_some() {
            "Suppression en cours".to_string()
        } else {
            format!("{n} dossier{}", if n > 1 { "s" } else { "" })
        };
        let sous = "Dossiers que Steam a laissés après la désinstallation";
        json!({ "titre": "Restes de jeux désinstallés", "sous": sous, "icone": "poubelle", "couleur": "P", "retour": "appli",
            "parent": "jeux",
            "module": { "icone": "poubelle", "couleur": "P", "titre": etat, "texte": sous,
                "boutons": [{ "id": "jeux-restes-fermer", "texte": "Fermer", "style": "second" }] },
            "blocs": blocs })
    }
}

/// Une taille de fichier (Format-Size, systeme.ps1).
pub fn taille_lisible(o: f64) -> String {
    const K: f64 = 1024.0;
    if o >= K * K * K * K {
        return format!("{} To", texte::n1(o / (K * K * K * K)));
    }
    if o >= K * K * K {
        return format!("{} Go", texte::n1(o / (K * K * K)));
    }
    if o >= K * K {
        return format!("{} Mo", texte::n0(o / (K * K)));
    }
    format!("{} Ko", texte::n0(o / K))
}

impl Bloc for BlocJeux {
    fn nom(&self) -> &'static str {
        "jeux"
    }

    fn pages(&mut self, ctx: &Contexte, ecran: &str) -> Map<String, Value> {
        pages::pages(self, ctx, ecran)
    }

    fn action(&mut self, ctx: &Contexte, id: &str, valeur: &Value, effets: &mut Effets) -> bool {
        let num = |prefixe: &str| -> Option<usize> { id.strip_prefix(prefixe).and_then(|x| x.parse().ok()) };
        match id {
            "jeux-cherche" => self.recherche = texte_valeur(valeur),
            "jeux-filtre" => self.filtre = texte_valeur(valeur),
            "jeux-organizer" => {
                effets.ajouter(Effet::OuvrirEcran("jeux".into()));
                effets.ajouter(Effet::Signal { nom: "organizer-ouvrir".into(), donnees: Value::Null });
            }
            "jeu-module-lancer" => {
                if let Some(g) = self.jeu_module.clone() {
                    self.lancer_jeu(ctx, &g, effets);
                }
            }
            "jeu-module-fiche" => {
                if let Some(g) = self.jeu_module.clone() {
                    self.choisi = Some(g);
                    effets.ajouter(Effet::OuvrirEcran("jeu".into()));
                }
            }
            "jeu-lancer" => {
                if let Some(g) = self.choisi.clone() {
                    self.lancer_jeu(ctx, &g, effets);
                }
            }
            "jeu-optimiser" => {
                if let Some(g) = self.choisi.clone() {
                    self.optimiser(ctx, &g, effets);
                }
            }
            "jeu-dossier" => {
                if let Some(g) = &self.choisi {
                    let d =
                        if g.dossier.is_empty() { texte::dossier_parent(g.premier_exe()) } else { g.dossier.clone() };
                    effets.ajouter(Effet::OuvrirAdresse(d));
                }
            }
            "jeu-retirer" => {
                if let Some(g) = self.choisi.clone() {
                    self.retirer_jeu(ctx, g.premier_exe(), effets);
                    self.choisi = None;
                    effets.ajouter(Effet::OuvrirEcran("jeux".into()));
                }
            }
            "jeu-desinstaller" => {
                if let Some(g) = self.choisi.clone() {
                    self.desinstaller(ctx, &g, effets);
                }
            }
            "org-affiche" => self.organizer_affiche(ctx, json::vrai(valeur), effets),
            // La mesure des FPS et le compteur (Set-FpsMeasure, Set-FpsOverlayOn, Set-FpsOverlayStyle,
            // Set-FpsOverlayCorner) : des réglages relus par le bloc perf, qui lance la mesure et le compteur.
            "fps-mesure" => {
                let on = if let Value::Bool(b) = valeur { *b } else { true };
                self.ecrire(ctx, "FpsMeasure", Value::Bool(on), effets);
                self.statut(
                    effets,
                    if on { "Mesure des FPS activée : lance un jeu." } else { "Mesure des FPS désactivée." },
                );
            }
            "overlay-on" => {
                let on = json::vrai(valeur);
                self.ecrire(ctx, "FpsOverlay", Value::Bool(on), effets);
                self.statut(
                    effets,
                    if on { "Compteur de FPS affiché pendant les parties." } else { "Compteur de FPS masqué." },
                );
            }
            "overlay-style" => {
                let st = texte_valeur(valeur);
                self.ecrire(ctx, "FpsOverlayStyle", Value::String(st.clone()), effets);
                self.statut(effets, &format!("Compteur de FPS : style {st}."));
            }
            "overlay-coin" => {
                let c = match texte_valeur(valeur).as_str() {
                    "haut-gauche" => "hg",
                    "haut-droite" => "hd",
                    "bas-gauche" => "bg",
                    "bas-droite" => "bd",
                    _ => return true,
                };
                self.ecrire(ctx, "FpsOverlayCorner", json!(c), effets);
                let nom = match c {
                    "hg" => "en haut à gauche",
                    "hd" => "en haut à droite",
                    "bg" => "en bas à gauche",
                    _ => "en bas à droite",
                };
                self.statut(effets, &format!("Compteur de FPS : {nom}."));
            }
            "fps-aide" => self.aide_fps(ctx, effets),
            "panneau-fps-fermer" => {
                self.partie_ouverte = None;
                effets.ajouter(Effet::OuvrirEcran("jeux".into()));
            }
            "fps-ajouter" => {
                let s = self
                    .partie_ouverte
                    .clone()
                    .and_then(|id| parties::parties_fps(Some(ctx), &ctx.donnees).into_iter().find(|p| p.id == id));
                if let Some(s) = s {
                    let chemin = s.brut.get("Diag").map(|d| json::texte(d, "Path")).unwrap_or_default();
                    let nom = self.nom_partie(&s);
                    self.ajouter_jeu(ctx, Some(chemin), Some(nom), effets);
                }
            }
            "clic:BtnLibRefresh" => {
                self.statut(effets, "Recherche de tes jeux...");
                self.rechercher_maintenant(ctx, effets);
                let n = self.bibliotheque().len();
                self.statut(effets, &format!("{n} jeux trouvés."));
            }
            "clic:BtnLibAdd" => self.ajouter_jeu(ctx, None, None, effets),
            "clic:BtnLibLeftovers" | "jeux-restes" => {
                self.restes_ouverts = true;
                effets.ajouter(Effet::OuvrirEcran("panneau".into()));
            }
            "jeux-restes-fermer" => {
                self.restes_ouverts = false;
                effets.ajouter(Effet::OuvrirEcran("jeux".into()));
            }
            "jeux-restes-tout" => {
                let l = self.gros_restes().into_iter().map(|x| x.0.dossier).collect();
                self.supprimer_restes(l, false, effets);
            }
            "coche:ChkLibCovers" => {
                let on = json::vrai(valeur);
                if let Err(e) = ctx.ecrire_reglage("LibCoversOnline", Value::Bool(on), false) {
                    ctx.journal(&e);
                }
                if !on {
                    self.statut(effets, "Jaquettes depuis Internet désactivées (celles déjà trouvées restent).");
                }
            }
            "jeux-reponse" => {
                let qid = json::texte(valeur, "id");
                let bouton = json::texte(valeur, "valeur");
                match self.question.take() {
                    Some((id_q, q)) if id_q == qid => {
                        if bouton == "oui" {
                            match q {
                                Question::Desinstaller(g) => self.desinstaller_confirme(ctx, &g, effets),
                                Question::Optimiser(g) => self.optimiser_confirme(ctx, &g, effets),
                                Question::SupprimerRestes(l) => self.supprimer_restes(l, true, effets),
                            }
                        }
                    }
                    autre => {
                        self.question = autre;
                        return false;
                    }
                }
            }
            _ => {
                if let Some(k) = num("jeu:") {
                    if let Some(g) = self.montres.get(k).cloned() {
                        self.choisi = Some(g);
                        effets.ajouter(Effet::Rafraichir);
                        effets.ajouter(Effet::OuvrirEcran("jeu".into()));
                    }
                } else if let Some(k) = num("jeu-opt:") {
                    let (it, g) = (self.optims.get(k).cloned(), self.choisi.clone());
                    if let (Some(it), Some(g)) = (it, g) {
                        if it.interrupteur {
                            self.changer_optimisation(ctx, &g.nom, &it.id, json::vrai(valeur), effets);
                        } else if it.correction.as_deref() == Some("tweaks") {
                            effets.ajouter(Effet::Signal {
                                nom: "appliquer-reglages-windows".into(),
                                donnees: json!({ "ids": it.ids }),
                            });
                        }
                    }
                } else if let Some(x) = id.strip_prefix("fps:") {
                    if !x.is_empty() && x.len() <= 40 {
                        self.ouvrir_partie(x, effets);
                    }
                } else if let Some(k) = num("jeux-reste-ouvrir:") {
                    if let Some((r, _)) = self.gros_restes().get(k) {
                        effets.ajouter(Effet::OuvrirAdresse(r.dossier.clone()));
                    }
                } else if let Some(k) = num("jeux-reste-supprimer:") {
                    if let Some((r, _)) = self.gros_restes().get(k).cloned() {
                        self.supprimer_restes(vec![r.dossier], false, effets);
                    }
                } else {
                    return false;
                }
            }
        }
        true
    }

    fn tic(&mut self, ctx: &Contexte, _cache: bool, effets: &mut Effets) -> Option<Duration> {
        if self.jeux.is_none() && self.recherche_en_cours.is_none() {
            self.lancer_recherche();
        }
        if let Some(r) = &self.recherche_en_cours {
            let fini = r.lock().unwrap_or_else(|e| e.into_inner()).take();
            if let Some(d) = fini {
                self.recherche_en_cours = None;
                self.recevoir_jeux(ctx, d, effets);
            }
        }
        if let Some(r) = &self.mesure_restes {
            let fini = r.lock().unwrap_or_else(|e| e.into_inner()).take();
            if let Some(v) = fini {
                self.mesure_restes = None;
                for (d, t, rec) in v {
                    self.tailles_restes.insert(d, (t, rec));
                }
                effets.ajouter(Effet::Rafraichir);
            }
        }
        if let Some(r) = &self.suppression {
            let fini = r.lock().unwrap_or_else(|e| e.into_inner()).take();
            if let Some(v) = fini {
                self.suppression = None;
                self.recevoir_suppression(ctx, v, effets);
            }
        }
        let maintenant = Instant::now();
        let mut refaire = false;
        let mut i = 0;
        while i < self.surveillances.len() {
            let s = self.surveillances[i].clone();
            if maintenant > s.jusqua {
                self.surveillances.remove(i);
                continue;
            }
            if maintenant >= s.prochaine {
                if !s.exe.is_empty() && std::path::Path::new(&s.exe).exists() {
                    self.surveillances[i].prochaine = maintenant + Duration::from_secs(10);
                } else {
                    self.surveillances.remove(i);
                    refaire = true;
                    self.statut(effets, &format!("{} est désinstallé.", s.nom));
                    ctx.journal(&format!("Bibliothèque: {} désinstallé", s.nom));
                    continue;
                }
            }
            i += 1;
        }
        if refaire {
            self.rechercher_maintenant(ctx, effets);
        }
        let occupe = self.recherche_en_cours.is_some()
            || self.mesure_restes.is_some()
            || self.suppression.is_some()
            || !self.surveillances.is_empty();
        Some(if occupe { Duration::from_millis(500) } else { Duration::from_secs(30) })
    }

    fn signal(&mut self, ctx: &Contexte, nom: &str, donnees: &Value, effets: &mut Effets) {
        match nom {
            "analyse-pc" => self.connu.lire_analyse_pc(donnees),
            "reglages-jeu" => self.connu.lire_reglages_jeu(donnees),
            "partie-commencee" => self.en_jeu = Some(json::texte(donnees, "jeu")).filter(|s| !s.is_empty()),
            "partie-finie" => {
                let jeu = json::texte(donnees, "jeu");
                // (signaux communs : {jeu, exe, pid, duree_s} ; le début de la partie = maintenant moins la durée)
                let maintenant = self.maintenant();
                let debut = donnees
                    .get("duree_s")
                    .map(|d| Moment(maintenant.0 - json::nombre_de(d).max(0.0).round() as i64))
                    .or_else(|| texte::lire_date(&json::texte(donnees, "debut")));
                if self.en_jeu.as_deref() == Some(jeu.as_str()) {
                    self.en_jeu = None;
                }
                if let Some(d) = debut {
                    let mut j = self.journal(ctx);
                    j.ajouter(Some(ctx), &ctx.donnees, &jeu, d, self.maintenant());
                    self.dernier_jeu = None;
                    effets.ajouter(Effet::Rafraichir);
                }
            }
            "jeux-rechercher" => self.rechercher_maintenant(ctx, effets),
            "ajouter-jeu" => self.ajouter_jeu(ctx, None, None, effets),
            "jeux-restes-ouvrir" => {
                if !self.gros_restes().is_empty() {
                    self.restes_ouverts = true;
                    effets.ajouter(Effet::OuvrirEcran("panneau".into()));
                }
            }
            "diagnostic-fps-pret" => {
                let id = json::texte(donnees, "Id");
                self.diagnostics.insert(id, donnees.get("diagnostic").cloned().unwrap_or(Value::Null));
                effets.ajouter(Effet::Rafraichir);
            }
            "fps-partie-enregistree" => {
                self.dernier_jeu = None;
                effets.ajouter(Effet::Rafraichir);
            }
            _ => {}
        }
    }
}
