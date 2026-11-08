//! Nevermind en Rust : le bloc « jeux ». La bibliothèque de tous les jeux installés (bibliotheque.ps1, jeu.ps1,
//! optimisations.ps1), la page Jeux et la fiche d'un jeu (Get-PagesJeux, Register-ActionsJeux, Invoke-ActionJeux
//! d'ecran-jeux.ps1), l'optimisation d'un jeu et l'Organizer Dofus (organizer.ps1).
//!
//! Les pages sont les mêmes, au champ près, que celles de la version PowerShell. Les onglets FPS et Lag en ligne de la
//! page Jeux relisent les fichiers des autres blocs (fps.json, lag.json, historique.json), comme la version PowerShell.

pub mod biblio;
pub mod jaquettes;
pub mod jaquettes_web;
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
use nevermind_noyau::travaux::{Rendu, Travaux};
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
    /// « Fermer X » du diagnostic d'une partie : le nom du programme
    FermerProgramme(String),
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

/// Ce que rend un travail de fond du bloc (audit G3)
enum FinJeux {
    /// La recherche des jeux (registre et disques)
    Recherche(Box<biblio::Decouverte>),
    /// La fenêtre « Choisis le programme du jeu (.exe) » : le programme choisi, ou rien
    Choisi(Option<String>),
    /// « Fermer X » du diagnostic d'une partie, par l'Aide : le nom du programme et la réponse
    Ferme(String, Result<Value, String>),
    /// Les jaquettes cherchées sur Internet ; `true` : lancé par un geste (les messages sont montrés)
    Jaquettes(Vec<jaquettes_web::Trouvee>, bool),
    /// Un jeu ou son désinstalleur lancé (après la demande de Windows s'il demande les droits)
    Lance { jeu: Box<Jeu>, desinstalleur: bool, r: Result<(), String> },
}

/// Ce qui est dit quand la recherche des jeux est finie
enum ApresRecherche {
    Texte(String),
    /// « N jeux trouvés. »
    Compte,
}

const RECHERCHE: &str = "recherche";
const FERMER: &str = "fermer-programme";
const JAQUETTES: &str = "jaquettes";
const CHOIX: &str = "choix";
const LANCEMENT: &str = "lancement";

pub struct BlocJeux {
    sys: Arc<dyn Systeme>,
    /// La liste des jeux ($script:Games) ; `None` : pas encore cherchée.
    jeux: Option<Vec<Jeu>>,
    /// Les restes de jeux Steam désinstallés ($script:Leftovers) et les bibliothèques à l'analyse incomplète.
    restes: Vec<steam::Reste>,
    incomplets: Vec<String>,
    index: HashMap<String, biblio::Indexe>,
    recherche_en_cours: Option<Resultat<biblio::Decouverte>>,
    /// La recherche des jeux demandée par un geste, et la fenêtre de choix du programme : dans un fil à part
    travaux: Travaux<FinJeux>,
    apres_recherche: Vec<ApresRecherche>,
    /// La liste a encore changé pendant une recherche : une autre, ensuite
    relancer_recherche: bool,
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
    /// Le réseau des jaquettes depuis Internet ; `internet_faux` : celui d'un test (le seul permis en mode test)
    internet: Arc<dyn jaquettes_web::Internet>,
    internet_faux: bool,
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
            travaux: Travaux::default(),
            apres_recherche: Vec::new(),
            relancer_recherche: false,
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
            internet: Arc::new(jaquettes_web::InternetWindows),
            internet_faux: false,
        }
    }

    /// Le bloc avec un faux Windows et un faux Internet (tests) : rien ne part sur le vrai réseau.
    pub fn avec_internet(sys: Arc<dyn Systeme>, internet: Arc<dyn jaquettes_web::Internet>) -> Self {
        BlocJeux { internet, internet_faux: true, ..Self::avec(sys) }
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

    /// Le jeu d'une partie (Get-SessionGame, diagnostic-fps.ps1:118-127) : le jeu connu, sinon le programme mesuré
    fn jeu_de_partie(&self, s: &parties::PartieFps) -> Option<Jeu> {
        if let Some(x) = self.index.get(&s.cle.to_lowercase())
            && let Some(g) = self.jeux.iter().flatten().find(|g| g.nom == x.jeu)
        {
            return Some(g.clone());
        }
        let chemin = s.brut.get("Diag").map(|d| json::texte(d, "Path")).unwrap_or_default();
        (!chemin.is_empty()).then(|| Jeu { nom: self.nom_partie(s), exes: vec![chemin], ..Default::default() })
    }

    /// Un bouton du diagnostic de la partie ouverte (« fps-diag:<cause>:<bouton> ») : les Actions de `Add-DiagItem`
    /// (diagnostic-fps.ps1:184-279), cliquées dans `New-SecurityCard` (la question `Confirm` d'abord). Audit M10.
    fn geste_diagnostic(&mut self, ctx: &Contexte, place: &str, effets: &mut Effets) {
        let mut p = place.split(':').map(|n| n.parse::<usize>().ok());
        let (Some(Some(k)), Some(Some(j)), None) = (p.next(), p.next(), p.next()) else { return };
        let Some(pid) = self.partie_ouverte.clone() else { return };
        let Some(a) = self
            .diagnostics
            .get(&pid)
            .and_then(|d| biblio::liste(d.get("elements")).get(k).cloned())
            .and_then(|e| biblio::liste(e.get("actions")).get(j).cloned())
        else {
            return;
        };
        let Some(s) = parties::parties_fps(Some(ctx), &ctx.donnees).into_iter().find(|p| p.id == pid) else { return };
        let arg = json::texte(&a, "arg");
        let quoi = json::texte(&a, "quoi");
        ctx.journal(&format!("Diagnostic d'une partie : {}", json::texte(&a, "texte")));
        match quoi.as_str() {
            // (Set-GameProfile, puis le message ; Hide-TestPanel : le panneau se ferme)
            "gpu" | "priorite" => {
                let Some(g) = self.jeu_de_partie(&s) else { return };
                self.partie_ouverte = None;
                effets.ajouter(Effet::OuvrirEcran("jeux".into()));
                self.changer_profil(&g, if quoi == "gpu" { "gpu" } else { "priority" }, true, effets);
                let m = if quoi == "gpu" {
                    format!("C'est réglé. Relance {} pour que ce soit pris en compte.", g.nom)
                } else {
                    format!("C'est réglé : {} passera avant les autres programmes à son prochain lancement.", g.nom)
                };
                self.message(effets, &m);
            }
            // (Invoke-TweakFix @('power') : le bloc pc règle Windows et montre le résultat)
            "alimentation" => {
                self.partie_ouverte = None;
                effets.ajouter(Effet::Signal {
                    nom: "appliquer-reglages-windows".into(),
                    donnees: json!({ "ids": ["power"] }),
                });
            }
            // (Open-Sheet du point de l'écran où tournait le jeu : le bloc pc ouvre sa fiche)
            "ecran" => {
                let ecran = s.brut.get("Diag").map(|d| json::texte(d, "Display")).unwrap_or_default();
                if ecran.is_empty() {
                    return;
                }
                self.partie_ouverte = None;
                effets.ajouter(Effet::Signal {
                    nom: "ouvrir-fiche".into(),
                    donnees: json!({ "id": format!("display:{ecran}") }),
                });
            }
            // (Open-Url : seulement une page web)
            "pilote" => {
                if arg.starts_with("https://") {
                    effets.ajouter(Effet::OuvrirAdresse(arg));
                }
            }
            // (Set-Setting 'GameMode' $true ; le bloc perf relit le réglage)
            "modejeu" => {
                if let Err(e) = ctx.ecrire_reglage("GameMode", Value::Bool(true), false) {
                    ctx.journal(&e);
                }
                effets.ajouter(Effet::Signal {
                    nom: "reglage-change".into(),
                    donnees: json!({ "nom": "GameMode", "valeur": true }),
                });
                self.statut(
                    effets,
                    "Mode jeu automatique activé : les applis cochées seront fermées pendant tes parties.",
                );
                effets.ajouter(Effet::Rafraichir);
            }
            "fermer" if !arg.is_empty() => {
                let t = json::texte(&a, "confirmer");
                let t = if t.is_empty() { format!("Fermer tous les {arg} ?") } else { t };
                self.poser_question(effets, Question::FermerProgramme(arg), &t, Vec::new());
            }
            _ => {}
        }
    }

    /// « Fermer X » confirmé (Stop-ProgrammeNom) : jamais la suite Mindset ; l'Aide applique la règle commune de
    /// fermeture à chaque programme de ce nom, dans un travail de fond (elle peut mettre du temps à répondre).
    fn fermer_programme(&mut self, ctx: &Contexte, nom: String, effets: &mut Effets) {
        let nom = nom.trim().to_string();
        if nevermind_noyau::pont::nom_de_la_suite(&nom) {
            self.message(effets, &format!("{nom} fait partie de la suite Mindset : Nevermind ne le ferme pas."));
            return;
        }
        self.statut(effets, &format!("Fermeture de {nom}..."));
        self.travaux.lancer(ctx, FERMER, move |f| {
            let r = f.ctx.aide("processus.fermer", &json!({ "Nom": nom }), 15000);
            FinJeux::Ferme(nom, r)
        });
        self.relever_travaux(ctx, effets);
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

    /// Cherche les jeux après un geste (Actualiser, un jeu ajouté ou retiré, une désinstallation) : le registre et
    /// les disques sont parcourus dans un travail de fond (audit G3) ; `apres` est dit à la fin. Une recherche déjà
    /// en cours : une autre la suit (la liste a changé entre-temps).
    fn rechercher(&mut self, ctx: &Contexte, effets: &mut Effets, apres: Option<ApresRecherche>) {
        self.apres_recherche.extend(apres);
        if self.travaux.en_cours(RECHERCHE) {
            self.relancer_recherche = true;
            return;
        }
        let sys = self.sys.clone();
        self.travaux.lancer(ctx, RECHERCHE, move |_| FinJeux::Recherche(Box::new(biblio::decouvrir(sys.as_ref()))));
        self.relever_travaux(ctx, effets);
    }

    /// Ce que les travaux de fond ont rendu (sur le fil des blocs)
    fn relever_travaux(&mut self, ctx: &Contexte, effets: &mut Effets) {
        for r in self.travaux.relever() {
            match r {
                Rendu::Effet(e) => effets.ajouter(e),
                Rendu::Fini { resultat: Ok(FinJeux::Recherche(d)), .. } => {
                    if std::mem::take(&mut self.relancer_recherche) {
                        self.rechercher(ctx, effets, None);
                        continue;
                    }
                    self.recevoir_jeux(ctx, *d, effets);
                    for a in std::mem::take(&mut self.apres_recherche) {
                        match a {
                            ApresRecherche::Texte(t) => self.statut(effets, &t),
                            ApresRecherche::Compte => {
                                let n = self.bibliotheque().len();
                                self.statut(effets, &format!("{n} jeux trouvés."));
                            }
                        }
                    }
                }
                Rendu::Fini { resultat: Ok(FinJeux::Choisi(exe)), .. } => {
                    if let Some(e) = exe {
                        self.ajouter_jeu(ctx, Some(e), None, effets);
                    }
                }
                Rendu::Fini { resultat: Ok(FinJeux::Jaquettes(l, par_geste)), .. } => {
                    self.recevoir_jaquettes(ctx, &l, par_geste, effets)
                }
                Rendu::Fini { resultat: Ok(FinJeux::Ferme(nom, r)), .. } => match r {
                    Ok(_) => self.statut(effets, &format!("{nom} fermé.")),
                    Err(e) => {
                        ctx.journal(&format!("Diagnostic d'une partie : {nom} pas fermé ({e})"));
                        self.message(effets, &format!("{nom} n'a pas pu être fermé ({e})."));
                    }
                },
                Rendu::Fini { resultat: Ok(FinJeux::Lance { jeu, desinstalleur, r }), .. } => {
                    self.fin_lancement(ctx, &jeu, desinstalleur, r, effets);
                }
                Rendu::Fini { cle, resultat: Err(e) } => {
                    ctx.journal(&format!("Bibliothèque : {cle} impossible ({e})"));
                    if cle == RECHERCHE {
                        self.apres_recherche.clear();
                    }
                }
            }
        }
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
        // (la bibliothèque est à jour : les jaquettes qui manquent, comme Update-Library puis Start-CoverDownload)
        self.lancer_jaquettes(ctx, false, effets);
    }

    /// `Start-CoverDownload` : les jeux sans image, cherchés sur Internet dans un travail de fond (seulement si le
    /// réglage est allumé ; jamais le vrai réseau en mode test). Audit M10.
    fn lancer_jaquettes(&mut self, ctx: &Contexte, par_geste: bool, effets: &mut Effets) {
        if self.travaux.en_cours(JAQUETTES)
            || !Self::reglage_bool(ctx, "LibCoversOnline", false)
            || (ctx.essai && !self.internet_faux)
        {
            return;
        }
        let a_faire = self.jaquettes_a_chercher(ctx);
        if a_faire.is_empty() {
            return;
        }
        if par_geste {
            let n = a_faire.len();
            self.statut(effets, &format!("Recherche des jaquettes de {n} jeu{}...", if n > 1 { "x" } else { "" }));
        }
        let net = self.internet.clone();
        let dir = jaquettes_web::dossier(&ctx.donnees);
        self.travaux.lancer(ctx, JAQUETTES, move |_| {
            FinJeux::Jaquettes(jaquettes_web::chercher(net.as_ref(), &dir, &a_faire), par_geste)
        });
        self.relever_travaux(ctx, effets);
    }

    /// Les jeux à chercher (le filtre de `Start-CoverDownload`) : aucune image de Steam ni du launcher, et pas encore
    /// cherché, ou cherché sans succès il y a plus de 7 jours, ou son image a disparu, ou ses images officielles
    /// sont connues
    fn jaquettes_a_chercher(&mut self, ctx: &Contexte) -> Vec<jaquettes_web::ATrouver> {
        let maintenant = self.maintenant();
        let sys = self.sys.clone();
        let index = self.jaquettes.index(&ctx.donnees).clone();
        let mut l = Vec::new();
        for g in self.bibliotheque() {
            if self.jaquettes.art_steam(sys.as_ref(), &g, "library_600x900.jpg").is_some()
                || self.jaquettes.art_steam(sys.as_ref(), &g, "library_capsule.jpg").is_some()
                || g.art.as_ref().is_some_and(|a| !a.cover.is_empty())
            {
                continue;
            }
            let chercher = match index.get(&g.nom.to_lowercase()) {
                None => true,
                Some(c) => {
                    let vieux = texte::lire_date(&c.date).is_some_and(|d| (maintenant.0 - d.0) as f64 / 86400.0 > 7.0);
                    (c.cover.is_empty() && vieux)
                        || (!c.cover.is_empty() && !std::path::Path::new(&c.cover).exists())
                        || (c.cover.is_empty() && jaquettes_web::officielle(&g.nom).is_some())
                }
            };
            if chercher {
                l.push(jaquettes_web::ATrouver { nom: g.nom.clone(), appid: g.appid.clone() });
            }
        }
        l
    }

    /// `Receive-Covers` : index.json mis à jour, puis la bibliothèque redessinée si une image a été trouvée
    fn recevoir_jaquettes(
        &mut self,
        ctx: &Contexte,
        l: &[jaquettes_web::Trouvee],
        par_geste: bool,
        effets: &mut Effets,
    ) {
        let n = match jaquettes_web::ranger(ctx, &ctx.donnees, l, &self.maintenant().iso()) {
            Ok(n) => n,
            Err(e) => {
                ctx.journal(&format!("Jaquettes: {e}"));
                return;
            }
        };
        ctx.journal(&format!("Jaquettes: {n} trouvée(s) sur {}", l.len()));
        if par_geste || n > 0 {
            let s = if n > 1 { "s" } else { "" };
            self.statut(effets, &format!("{n} jaquette{s} ajoutée{s} à ta bibliothèque."));
        }
        if n > 0 {
            self.jaquettes.oublier();
            effets.ajouter(Effet::Rafraichir);
        }
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

    /// Lancer un jeu (Start-LibraryGame). Le même jeu n'est pas relancé pendant 20 s ($script:LibLaunching : le
    /// bouton disait « Lancement... ») : un double clic ne le lance pas deux fois.
    fn lancer_jeu(&mut self, ctx: &Contexte, g: &Jeu, effets: &mut Effets) {
        if let Some((n, t)) = &self.lancement_jeu
            && *n == g.nom
            && t.elapsed() < Duration::from_secs(20)
        {
            ctx.journal(&format!("Bibliothèque: {} est déjà en train de se lancer, pas relancé", g.nom));
            return;
        }
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
                if ctx.essai && !self.sys.faux() {
                    ctx.journal("Bibliothèque: copie de test, le jeu n'est pas lancé");
                } else if !self.lancer_programme(ctx, g, false, chemin, arguments, effets) {
                    return;
                }
            }
            Lancement::Adresse(a) => effets.ajouter(Effet::OuvrirAdresse(a)),
        }
        self.lancement_jeu = Some((g.nom.clone(), Instant::now()));
        let par = if !g.source.is_empty() && par_url { format!(" par {}", g.source) } else { String::new() };
        self.statut(effets, &format!("Lancement de {}{par}...", g.nom));
        self.relever_travaux(ctx, effets);
    }

    /// Un jeu ou son désinstalleur lancé dans un travail de fond (M8, audit du 08/11) : s'il demande les droits,
    /// Windows pose sa question et le lancement attend la réponse. Un échec est dit (`fin_lancement`), au lieu d'un
    /// faux « Lancement de... » ou d'une surveillance de la désinstallation pour rien. `false` : un autre lancement
    /// attend encore la réponse de l'utilisateur, rien n'est lancé.
    fn lancer_programme(
        &mut self,
        ctx: &Contexte,
        g: &Jeu,
        desinstalleur: bool,
        chemin: String,
        arguments: String,
        effets: &mut Effets,
    ) -> bool {
        if self.travaux.en_cours(LANCEMENT) {
            self.message(effets, "Un autre programme attend encore ta réponse à la demande de Windows.");
            return false;
        }
        let sys = self.sys.clone();
        let jeu = Box::new(g.clone());
        self.travaux.lancer(ctx, LANCEMENT, move |_| {
            let r = sys.lancer(&chemin, &arguments, &texte::dossier_parent(&chemin));
            FinJeux::Lance { jeu, desinstalleur, r }
        })
    }

    /// La fin d'un lancement (sur le fil des blocs) : l'échec dit à l'utilisateur ; un désinstalleur ouvert est suivi
    fn fin_lancement(
        &mut self,
        ctx: &Contexte,
        g: &Jeu,
        desinstalleur: bool,
        r: Result<(), String>,
        effets: &mut Effets,
    ) {
        match r {
            Ok(()) if desinstalleur => self.apres_desinstallation(ctx, g, effets),
            Ok(()) => {}
            Err(e) => {
                let quoi = if desinstalleur { "Désinstalleur" } else { "Lancement" };
                ctx.journal(&format!("Bibliothèque: {quoi} de {} impossible ({e})", g.nom));
                if !desinstalleur {
                    self.lancement_jeu = None;
                }
                let e = e.trim_end_matches('.');
                self.message(
                    effets,
                    &if desinstalleur {
                        format!(
                            "Le désinstalleur de « {} » ne s'est pas ouvert : {e}.\n\nRien n'a été désinstallé.",
                            g.nom
                        )
                    } else {
                        format!("« {} » ne s'est pas lancé : {e}.", g.nom)
                    },
                );
            }
        }
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
                if ctx.essai && !self.sys.faux() {
                    ctx.journal("Bibliothèque: copie de test, le désinstalleur n'est pas lancé");
                } else {
                    // (la suite, avec la surveillance de la désinstallation, seulement s'il s'est ouvert : fin_lancement)
                    if self.lancer_programme(ctx, g, true, chemin, arguments, effets) {
                        self.relever_travaux(ctx, effets);
                    }
                    return;
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
            // (la fenêtre de choix attend l'utilisateur sans limite : dans un fil à part, audit G3 ; la suite vient
            //  avec le programme choisi. Déjà ouverte : pas une deuxième.)
            None => {
                if !self.travaux.en_cours(CHOIX) {
                    let sys = self.sys.clone();
                    self.travaux.lancer(ctx, CHOIX, move |_| FinJeux::Choisi(sys.choisir_exe()));
                    self.relever_travaux(ctx, effets);
                }
                return;
            }
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
        self.rechercher(
            ctx,
            effets,
            Some(ApresRecherche::Texte(format!("{nom} ajouté à tes jeux : Nevermind le reconnaîtra à son lancement."))),
        );
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
        self.rechercher(ctx, effets, Some(ApresRecherche::Texte("Jeu retiré de la liste.".into())));
    }

    /// Montrer ou cacher l'Organizer (Set-OrgVisible). Caché : il est aussi coupé.
    fn organizer_affiche(&mut self, ctx: &Contexte, oui: bool, effets: &mut Effets) {
        if let Err(e) = ctx.ecrire_reglage("OrganizerAffiche", Value::Bool(oui), false) {
            ctx.journal(&e);
        }
        // (le fil de l'Organizer, s'il tourne, ferme la vue et coupe l'Organizer lui-même)
        if !oui && !organizer::fil::cacher() {
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
                organizer::fil::ouvrir_vue(ctx);
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
            // (« panneau:close » : le bouton de retour en haut du panneau et Échap, envoyés par l'hôte au bloc qui a
            //  ouvert le panneau ; avant le 08/11/2026, le bloc jeux ne les prenait pas et rien ne se passait)
            "panneau-fps-fermer" => {
                self.partie_ouverte = None;
                effets.ajouter(Effet::OuvrirEcran("jeux".into()));
            }
            "panneau:close" if self.partie_ouverte.is_some() => {
                self.partie_ouverte = None;
                effets.ajouter(Effet::OuvrirEcran("jeux".into()));
            }
            "panneau:close" if self.restes_ouverts => {
                self.restes_ouverts = false;
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
                self.rechercher(ctx, effets, Some(ApresRecherche::Compte));
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
                if on {
                    self.lancer_jaquettes(ctx, true, effets);
                } else {
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
                                Question::FermerProgramme(n) => self.fermer_programme(ctx, n, effets),
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
                } else if let Some(x) = id.strip_prefix("fps-diag:") {
                    self.geste_diagnostic(ctx, x, effets);
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
        organizer::fil::tic(ctx, effets);
        self.relever_travaux(ctx, effets);
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
            self.rechercher(ctx, effets, None);
        }
        let occupe = self.recherche_en_cours.is_some()
            || self.travaux.occupe()
            || self.mesure_restes.is_some()
            || self.suppression.is_some()
            || !self.surveillances.is_empty();
        Some(organizer::fil::delai(if occupe { Duration::from_millis(500) } else { Duration::from_secs(30) }))
    }

    fn travaux_finis(&mut self, ctx: &Contexte, effets: &mut Effets) {
        self.relever_travaux(ctx, effets);
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
            "organizer-ouvrir" => organizer::fil::ouvrir_vue(ctx),
            "jeux-rechercher" => self.rechercher(ctx, effets, None),
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

    fn fermer(&mut self, ctx: &Contexte) {
        organizer::fil::fermer(ctx);
    }
}
