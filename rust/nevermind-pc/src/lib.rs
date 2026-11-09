//! Nevermind en Rust : le bloc « pc » (Ton PC, Tableau de bord, la fiche d'un point, Tests des composants), et ce qui
//! règle Windows pour le jeu (optimisations, programmes au démarrage, nettoyage, sauvegarde et « Tout restaurer »).
//!
//! Portage de ecran-pc.ps1, analyse.ps1, tableau-de-bord.ps1, optimisations.ps1, systeme.ps1 (nettoyage, restauration),
//! tests.ps1 (la page) et des onglets Démarrage, Nettoyage et Sauvegarde de onglets.ps1. La version PowerShell est la
//! référence : les juges (tests/juges, outils\exporter-juges-pc.ps1) vérifient que les pages et la suite exacte des
//! gestes sur le PC sont les mêmes.
//!
//! Léger : rien ne tourne en continu. Les lectures lentes (WMI, disques) sont faites une fois, dans un fil à part, à la
//! première ouverture ; ensuite seulement quand on relance l'analyse ou après une correction.

pub mod analyse;
pub mod corrections;
pub mod diffusion;
pub mod fiche;
pub mod lecture;
pub mod modele;
pub mod net;
pub mod nettoyage;
pub mod pages;
pub mod pc;
pub mod reglages;
pub mod score;
pub mod session;
pub mod systeme;
pub mod temps;
pub mod tests;
pub mod tests_composants;
#[cfg(windows)]
mod windows;

use nevermind_noyau::travaux::{Rendu, Travaux};
use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use pc::Pc;
use serde_json::{Map, Value, json};
use session::Session;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Les pages d'avant de Nevermind vers celles du kit (`$KitPagesIndex`, accueil-console.ps1)
fn page_kit(index: i64) -> Option<&'static str> {
    Some(match index {
        0 => "tableau",
        1 => "optimisation",
        2 => "demarrage",
        3 => "connexion",
        4 => "nettoyage",
        5 => "tests",
        6 => "securite",
        7 => "sauvegarde",
        _ => return None,
    })
}

/// Un faux PC (tests), partagé avec un travail de fond
type FauxPc = Arc<Mutex<Box<dyn Pc + Send>>>;

/// Ce qu'un travail de fond du bloc emporte et rend (audit G3) : une copie de la session et des cases (la page
/// montre l'ancienne pendant ce temps), et ce que le PC a demandé de montrer à la fin du geste
struct Atelier {
    session: Session,
    cases: diffusion::Cases,
    annonces: Vec<Effet>,
    /// Le PC a été relu (« Analyser ») : les données des tests des composants suivent
    relu: bool,
}

/// Ce qui suit la fin d'un travail de fond
enum Suite {
    /// Un geste : la fiche (ouverte avant le geste ou non), l'écran, les autres blocs à jour
    Geste { avant: bool },
    /// La fin de la lecture lente (complétée par l'Aide, puis l'analyse) : la diffusion est due
    Lecture,
}

/// Un geste : fait sur place (avec ce que le PC a demandé de montrer), ou parti en fond (sa fin fait la suite)
enum Fait {
    Sur(Vec<Effet>),
    Fond,
}

/// La clé du travail de fond du bloc : un seul geste sur la session à la fois
const TRAVAIL: &str = "pc";

/// La page, dans « Attends la fin de « Appliquer » (Ton PC) »
const PAGE: &str = "Ton PC";

/// Le nom d'un geste de la page qui ne prend pas le verrou des gestes, dans « Attends la fin de ... »
fn nom_sans_verrou(id: &str) -> &str {
    match id {
        "analyser" | "clic:BtnCleanScan" => "Analyser",
        "fiche:close" | "retour" => "Fermer la fiche",
        "fiche:ignore" => "Ignorer",
        autre => autre,
    }
}

/// Le texte d'attente d'un geste long qui n'en montre pas d'autre (lot B : « Analyser » du Nettoyage passe par l'Aide,
/// jusqu'à 2 min par catégorie)
fn attente_du_geste(id: &str) -> Option<&'static str> {
    match id {
        "clic:BtnCleanScan" => Some("Recherche des fichiers à nettoyer..."),
        _ => None,
    }
}

/// Ce qu'un geste de la page fait du verrou commun des gestes (lot A du second audit)
enum Verrou {
    /// Rien ne change sur le PC (analyse, lecture des dossiers, ignorer un point)
    Aucun,
    /// Un clic qui change le PC : le verrou est pris, ou le clic est refusé
    Clic(String),
    /// Un geste automatique qui note dans la sauvegarde (un profil de jeu) : il attend son tour (nom, page)
    Tour(String, String),
    /// La réponse à « Tes écrans s'affichent bien ? » : jamais refusée, l'écran doit revenir
    Priorite(String),
}

#[derive(Default)]
pub struct BlocPc {
    session: Session,
    kit: pages::Kit,
    /// Les mesures en direct (`$Live`), reçues par le signal « mesures »
    live: Value,
    /// La lecture lente du PC, dans un fil à part
    lecture: Option<std::thread::JoinHandle<lecture::Lu>>,
    lu: bool,
    /// La lecture lente vient de finir (dans un tic, ou dans `pages` quand l'écran est regardé) : la diffusion aux
    /// autres blocs est due
    a_diffuser: bool,
    charge: bool,
    /// D'où la fiche a été ouverte (`$script:Kit.FicheDe`)
    fiche_de: Option<String>,
    fin_ecran: Option<Instant>,
    /// Un faux PC (tests) : rien ne touche le vrai PC
    essai: Option<FauxPc>,
    tests: tests_composants::Tests,
    /// Les cases des pages du bloc perf (réglages, nettoyage) et la liste de démarrage
    cases: diffusion::Cases,
    /// L'écran montré (le dernier demandé par l'hôte)
    ecran: String,
    /// Les gestes longs sur la session (point de restauration, corrections, nettoyage, analyse) : dans un fil à
    /// part (audit G3), un à la fois
    travaux: Travaux<Atelier>,
    /// Le nom du geste long en cours (`None` : la fin de la lecture), pour dire en clair pourquoi un clic est refusé
    travail_nom: Option<String>,
    suite: Option<Suite>,
    /// Les demandes des autres blocs arrivées pendant un geste long : faites après, dans l'ordre
    signaux_en_attente: Vec<(String, Value)>,
    /// sauvegarde.json a changé pendant un geste long : relue après
    sv_a_relire: bool,
    /// Les effets demandés hors d'un appel qui en reçoit (rendus au prochain appel)
    en_attente: Vec<Effet>,
    /// Les programmes de Windows lancés (mdsched, perfmon) : la question de Windows attend l'utilisateur, dans un
    /// fil à part ; la fin dit un échec (M8)
    lancements: Travaux<Result<(), String>>,
}

/// La clé du lancement d'un programme de Windows : un à la fois (un double clic ne pose pas deux questions)
const LANCEMENT: &str = "lancement";

impl BlocPc {
    /// Le bloc sur un faux PC (tests), avec ce qu'il aurait lu
    pub fn sur(pc: Box<dyn Pc + Send>, lu: lecture::Lu) -> Self {
        let mut b = BlocPc { essai: Some(Arc::new(Mutex::new(pc))), lu: true, charge: true, ..Default::default() };
        b.prendre(lu);
        b
    }

    /// La session (tests)
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Le faux PC (tests)
    pub fn faux_pc(&mut self) -> Option<&mut (dyn Pc + Send + 'static)> {
        let m = Arc::get_mut(self.essai.as_mut()?)?;
        Some(m.get_mut().unwrap_or_else(|e| e.into_inner()).as_mut())
    }

    /// Ce que dit un clic refusé parce qu'un geste de la page tourne encore (lot B)
    fn refus_occupe(&self) -> String {
        match &self.travail_nom {
            Some(n) => nevermind_noyau::gestes::texte_refus(n, PAGE),
            None => "Un instant : Nevermind finit de lire ton PC. Réessaie dans quelques secondes.".into(),
        }
    }

    /// Un geste long est en cours (tests)
    pub fn travail_en_cours(&self) -> bool {
        self.travaux.occupe()
    }

    fn prendre(&mut self, lu: lecture::Lu) {
        self.session.entrees = lu.entrees;
        self.session.demarrage = lu.demarrage;
        self.tests.donnees = self.session.entrees.data.clone();
    }

    /// Le PC sur lequel agir : le faux (tests) ou le vrai
    fn avec_pc<R>(&mut self, ctx: &Contexte, f: impl FnOnce(&mut Self, &mut dyn Pc) -> R) -> (R, Vec<Effet>) {
        if let Some(p) = self.essai.clone() {
            let mut g = p.lock().unwrap_or_else(|e| e.into_inner());
            let r = f(self, g.as_mut());
            return (r, Vec::new());
        }
        let mut p = systeme::PcWindows::nouveau(ctx);
        let r = f(self, &mut p);
        (r, p.annonces())
    }

    /// Un programme de Windows (« Tester la mémoire », « Test complet Windows », l'historique de fiabilité) : lancé
    /// par ShellExecute dans un travail de fond, puisque la question de Windows attend l'utilisateur ; un échec est
    /// dit (M8, audit du 08/11 : mdsched demande les droits, ne se lançait pas, et rien n'était dit).
    fn lancer_programme(&mut self, ctx: &Contexte, args: Vec<String>, effets: &mut Effets) {
        if self.lancements.en_cours(LANCEMENT) {
            effets.ajouter(Effet::Annonce("Un programme attend encore ta réponse à la demande de Windows.".into()));
            return;
        }
        let essai = self.essai.clone();
        self.lancements.lancer(ctx, LANCEMENT, move |fond| {
            let r = match essai {
                Some(p) => p.lock().unwrap_or_else(|e| e.into_inner()).lancer(&args),
                None => systeme::PcWindows::nouveau(fond.ctx).lancer(&args),
            };
            r.map_err(|e| session::message_lancement(&args, &e))
        });
        self.relever_lancements(effets);
    }

    /// La fin des lancements (sur le fil des blocs) : l'échec en bulle
    fn relever_lancements(&mut self, effets: &mut Effets) {
        for r in self.lancements.relever() {
            match r {
                Rendu::Effet(e) => effets.ajouter(e),
                Rendu::Fini { resultat: Ok(Ok(())), .. } => {}
                Rendu::Fini { resultat: Ok(Err(e)), .. } => effets.ajouter(Effet::Annonce(e)),
                Rendu::Fini { resultat: Err(e), .. } => effets.ajouter(Effet::Annonce(session::message_erreur(&e))),
            }
        }
    }

    /// Un geste long sur la session (audit G3) : fait dans un travail de fond, sur une copie de la session et des
    /// cases, que la fin remet en place (`relever_travaux`). `direct` : le texte d'attente du PC est montré tout de
    /// suite. `false` si un autre geste long est en cours (rien n'est lancé : un double clic ne refait rien).
    ///
    /// Lot A du second audit : un geste qui change le PC prend le verrou commun des gestes au moment du clic
    /// (`verrou`) ; un autre geste le tient : le clic est refusé, « Attends la fin de ... » (rendu en `Err`), rien ne
    /// part plus tard. Un profil de jeu (automatique) attend son tour dans le travail ; la réponse à « Tes écrans
    /// s'affichent bien ? » passe en priorité.
    fn en_fond(
        &mut self,
        ctx: &Contexte,
        quoi: &str,
        suite: Suite,
        direct: bool,
        verrou: Verrou,
        f: impl FnOnce(&mut Atelier, &mut dyn Pc) + Send + 'static,
    ) -> Result<bool, String> {
        if self.travaux.occupe() {
            ctx.journal(&format!("Ton PC : {quoi} pas lancé, un geste est encore en cours"));
            return Ok(false);
        }
        let nom = match (&suite, &verrou) {
            (Suite::Lecture, _) => None,
            (_, Verrou::Clic(n) | Verrou::Priorite(n) | Verrou::Tour(n, _)) => Some(n.clone()),
            (_, Verrou::Aucun) => Some(nom_sans_verrou(quoi).to_string()),
        };
        let attente = attente_du_geste(quoi);
        let (geste, tour) = match verrou {
            Verrou::Aucun => (None, None),
            Verrou::Clic(nom) => (Some(ctx.prendre_geste(&nom, PAGE)?), None),
            Verrou::Priorite(nom) => (Some(ctx.geste_prioritaire(&nom, PAGE)), None),
            Verrou::Tour(nom, page) => (None, Some((nom, page))),
        };
        let mut at =
            Atelier { session: self.session.clone(), cases: self.cases.clone(), annonces: Vec::new(), relu: false };
        let essai = self.essai.clone();
        self.travail_nom = nom;
        self.suite = Some(suite);
        Ok(self.travaux.lancer(ctx, TRAVAIL, move |fond| {
            // (l'attente d'un geste qui n'en montre pas d'autre, retirée à la fin du travail ; lot B)
            if let Some(t) = attente {
                fond.effet(nevermind_noyau::signaux::occupe(t));
            }
            // (le geste emporté par le travail : il rend le verrou à la fin, même après une panique)
            if let Some(g) = &geste {
                g.ici();
            }
            // (un geste automatique attend son tour, sans figer le fil des blocs)
            let _tour = match tour {
                None => None,
                Some((nom, page)) => {
                    let attente = fond.ctx.geste_en_cours();
                    if let Some((n, p)) = &attente {
                        fond.effet(nevermind_noyau::signaux::occupe(&format!(
                            "« {nom} » se fera juste après « {n} » ({p})..."
                        )));
                    }
                    let r = fond.ctx.attendre_geste(&nom, &page, nevermind_noyau::gestes::ATTENTE_REPRISE);
                    if attente.is_some() {
                        fond.effet(nevermind_noyau::signaux::occupe(""));
                    }
                    match r {
                        Ok(g) => Some(g),
                        Err(e) => {
                            at.annonces.push(Effet::Annonce(format!("« {nom} » n'a pas été fait. {e}.")));
                            return at;
                        }
                    }
                }
            };
            match essai {
                Some(p) => {
                    let mut g = p.lock().unwrap_or_else(|e| e.into_inner());
                    f(&mut at, g.as_mut());
                }
                None => {
                    let envoi = |e: Effet| fond.effet(e);
                    let mut p = systeme::PcWindows::nouveau(fond.ctx);
                    if direct {
                        p = p.en_direct(&envoi);
                    }
                    f(&mut at, &mut p);
                    at.annonces.extend(p.annonces());
                }
            }
            drop(geste);
            at
        }))
    }

    /// Ce que le travail de fond a rendu (sur le fil des blocs) : le texte d'attente, puis à la fin la session mise
    /// à jour, la fiche, l'écran et les autres blocs ; ensuite les demandes des autres blocs qui ont attendu.
    fn relever_travaux(&mut self, ctx: &Contexte, effets: &mut Effets) {
        for r in self.travaux.relever() {
            let resultat = match r {
                Rendu::Effet(e) => {
                    effets.ajouter(e);
                    continue;
                }
                Rendu::Fini { resultat, .. } => resultat,
            };
            let suite = self.suite.take();
            match resultat {
                Ok(at) => {
                    self.session = at.session;
                    self.cases = at.cases;
                    if at.relu {
                        self.tests.donnees = self.session.entrees.data.clone();
                    }
                    match suite {
                        Some(Suite::Geste { avant }) => {
                            self.apres(avant, at.annonces, effets);
                            self.diffuser(ctx, effets);
                        }
                        // (comme avant : ce que dit le PC à la fin de la lecture n'est pas montré)
                        Some(Suite::Lecture) | None => {
                            self.lu = true;
                            self.a_diffuser = true;
                        }
                    }
                }
                Err(e) => {
                    if matches!(suite, Some(Suite::Lecture)) {
                        self.lu = true;
                    }
                    effets.ajouter(Effet::Signal { nom: "occupe".into(), donnees: json!({ "texte": "" }) });
                    effets.ajouter(Effet::Annonce(session::message_erreur(&e)));
                    // (lot A : ce que le geste avait déjà noté avant l'erreur est dans sauvegarde.json ; la copie
                    //  d'avant le geste n'est pas gardée, la sauvegarde est relue et les autres blocs le savent)
                    if matches!(suite, Some(Suite::Geste { .. })) {
                        self.session.sv = systeme::lire_sauvegarde(ctx);
                        effets.ajouter(Effet::Signal { nom: "sauvegarde-changee".into(), donnees: json!({}) });
                    }
                }
            }
            if std::mem::take(&mut self.sv_a_relire) && self.session.attente.is_none() {
                self.session.sv = systeme::lire_sauvegarde(ctx);
            }
            // (les demandes des autres blocs arrivées pendant le geste, dans l'ordre, tant qu'aucune ne repart en fond)
            while !self.travaux.occupe() && !self.signaux_en_attente.is_empty() {
                let (nom, donnees) = self.signaux_en_attente.remove(0);
                self.signal_pc(ctx, &nom, &donnees, effets);
            }
        }
    }

    /// La diffusion due après la lecture lente (une fois)
    fn diffuser_si_du(&mut self, ctx: &Contexte, effets: &mut Effets) {
        if self.lu && std::mem::take(&mut self.a_diffuser) {
            effets.ajouter(Effet::Rafraichir);
            self.diffuser(ctx, effets);
        }
    }

    fn charger(&mut self, ctx: &Contexte) {
        if self.charge {
            return;
        }
        self.charge = true;
        self.tests.troisdmark = !ctx.essai && tests::troisdmark();
        self.cases.restauration = true;
        self.session.sv = systeme::lire_sauvegarde(ctx);
        self.session.ignores = systeme::lire_ignores(ctx);
    }

    /// La lecture lente : lancée une fois ; l'analyse est faite dès qu'elle est finie
    fn suivre_lecture(&mut self, ctx: &Contexte) {
        if self.lu {
            if self.session.analyse.is_none() && self.essai.is_some() {
                self.avec_pc(ctx, |s, pc| s.session.analyser(pc));
            }
            return;
        }
        // (la fin de la lecture est déjà en cours dans un travail de fond, ou un geste long attend : plus tard)
        if self.travaux.occupe() {
            return;
        }
        match &self.lecture {
            None => {
                // (mode test sans faux PC, la construction d'essai : le vrai PC est LU, comme la version PowerShell en
                //  test, pour que Ton PC montre son analyse ; rien n'y est écrit : PcWindows refuse toute écriture en
                //  test)
                let _ = ctx;
                self.lecture = Some(std::thread::spawn(lecture::lire));
            }
            Some(h) if h.is_finished() => {
                if let Some(h) = self.lecture.take()
                    && let Ok(lu) = h.join()
                {
                    self.prendre(lu);
                }
                // (l'Aide peut mettre jusqu'à 20 s à se réveiller pour sa lecture : dans un travail de fond, audit G3 ;
                //  la diffusion suit sa fin)
                let _ = self.en_fond(ctx, "fin de la lecture", Suite::Lecture, false, Verrou::Aucun, |at, pc| {
                    systeme::completer_par_aide(pc, &mut at.session.entrees);
                    systeme::completer_pilote_nvidia(pc, &mut at.session.entrees);
                    at.session.analyser(pc);
                });
                let mut e = Effets::default();
                self.relever_travaux(ctx, &mut e);
                self.en_attente.extend(e.0);
            }
            Some(_) => {}
        }
    }

    /// `Invoke-FindingAction` : un onglet (« tab:4 »), un programme (« run:... ») ou une adresse
    fn aller(&mut self, ctx: &Contexte, cible: &str, effets: &mut Effets) {
        if let Some(n) = cible.strip_prefix("tab:") {
            if let Some(p) = n.trim().parse::<i64>().ok().and_then(page_kit) {
                effets.ajouter(Effet::OuvrirEcran(p.into()));
            }
            return;
        }
        if let Some(r) = cible.strip_prefix("run:") {
            let args: Vec<String> = match r.split_once(' ') {
                Some((a, b)) => vec![a.into(), b.into()],
                None => vec![r.into()],
            };
            self.lancer_programme(ctx, args, effets);
            return;
        }
        effets.ajouter(Effet::OuvrirAdresse(cible.into()));
    }

    /// Après un geste : la fiche montrée ou fermée (`Update-FicheKit`), les messages, l'écran à jour
    fn apres(&mut self, avant: bool, annonces: Vec<Effet>, effets: &mut Effets) {
        let ecran = self.ecran.clone();
        for a in annonces {
            effets.ajouter(a);
        }
        // (une question est montrée par l'hôte, signal « question » : la fiche n'en garde rien)
        if self.session.fiche.as_ref().is_some_and(|f| f.mode == "question") && self.essai.is_none() {
            self.session.fiche = None;
        }
        let ouverte = self.session.fiche.is_some();
        if ouverte && !avant {
            self.fiche_de = Some(if ecran.is_empty() { "accueil".into() } else { ecran });
            effets.ajouter(Effet::OuvrirEcran("fiche".into()));
        } else if !ouverte && avant {
            let de = self.fiche_de.take().filter(|d| d != "fiche").unwrap_or_else(|| "accueil".into());
            effets.ajouter(Effet::OuvrirEcran(de));
        }
        if self.session.fiche.as_ref().is_some_and(|f| f.mode == "display") {
            if self.fin_ecran.is_none() {
                self.fin_ecran = Some(Instant::now() + Duration::from_secs(15));
            }
        } else {
            self.fin_ecran = None;
        }
        effets.ajouter(Effet::Rafraichir);
    }
}

impl BlocPc {
    /// Les données des pages des autres blocs (perf, mémoire), relues après une analyse ou un changement
    fn diffuser(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let Some(a) = self.session.analyse.clone() else { return };
        let build = a.build;
        let ((plan_ok, nombre), _) = self.avec_pc(ctx, |s, pc| {
            let cree = s.session.sv.plan_cree.clone();
            diffusion::lire_reglages(pc, build, cree.as_deref(), &mut s.cases);
            diffusion::lire_demarrage(pc, &s.session.demarrage, &mut s.cases);
            s.tests.ecrans = pc.ecrans();
            (reglages::test_plan(pc, cree.as_deref()), corrections::compte_total(&s.session.sv, pc))
        });
        self.tests.portable = a.portable;
        let sig = |n: &str, d: Value| Effet::Signal { nom: n.into(), donnees: d };
        effets.ajouter(sig("analyse-pc", diffusion::signal_analyse(&a, &self.session.entrees.data, plan_ok)));
        effets.ajouter(sig("reglages-jeu", diffusion::signal_reglages(&self.cases)));
        effets.ajouter(sig("demarrage", diffusion::signal_demarrage(&self.cases)));
        effets.ajouter(sig("demarrage-programmes", diffusion::signal_programmes(&self.cases)));
        if self.cases.nettoyage_lu {
            effets.ajouter(sig("nettoyage", diffusion::signal_nettoyage(&self.cases)));
        }
        // (Get-BackupCount : tout ce que « Tout restaurer » remettrait, origines gardées par l'Aide comprises, pour la
        //  page Sauvegarde du bloc protection ; le score et les points à régler vont à l'accueil par « _accueil »)
        effets.ajouter(sig("sauvegarde-nombre", json!({ "n": nombre })));
        // (la carte de Nevermind dans l'accueil de Mindset : `Write-EtatMindset`, écrite par l'hôte dans etat.json)
        effets.ajouter(sig("etat-mindset", json!({ "pc": score::etat_mindset(&a) })));
    }

    /// Les gestes des pages du bloc perf dont la logique est ici (onglets Optimisation, Démarrage, Nettoyage)
    fn action_perf(
        &mut self,
        ctx: &Contexte,
        id: &str,
        valeur: &Value,
        avant: bool,
        effets: &mut Effets,
    ) -> Option<Fait> {
        let k = net::champ_entier(valeur, "valeur") as usize;
        let oui = net::champ_vrai(valeur, "oui");
        let rien = || Some(Fait::Sur(Vec::new()));
        match id {
            "tweak" => {
                if let Some(r) = self.cases.reglages.get_mut(k).filter(|r| !r.1) {
                    r.2 = oui;
                }
                rien()
            }
            "opti:recommande" => {
                for r in self.cases.reglages.iter_mut().filter(|r| !r.1) {
                    r.2 = reglages::reglage(&r.0).is_some_and(|t| t.conseille);
                }
                rien()
            }
            "coche:ChkRestore" => {
                self.cases.restauration = net::vrai(valeur);
                rien()
            }
            // (le point de restauration, jusqu'à 10 min, puis chaque réglage par l'Aide : dans un travail de fond)
            "clic:BtnApply" => {
                let ids: Vec<String> =
                    self.cases.reglages.iter().filter(|r| !r.1 && r.2).map(|r| r.0.clone()).collect();
                let rest = self.cases.restauration;
                Some(self.geste_fond(ctx, id, avant, move |at, pc| at.session.appliquer_reglages(pc, &ids, rest)))
            }
            "nett" => {
                if let Some(r) = self.cases.nettoyage.get_mut(k).filter(|r| r.2 > 0) {
                    r.3 = oui;
                }
                rien()
            }
            // (la taille des dossiers à nettoyer : un parcours des disques)
            "clic:BtnCleanScan" => {
                Some(self.geste_fond(ctx, id, avant, |at, pc| diffusion::lire_nettoyage(pc, &mut at.cases)))
            }
            // (le nettoyage de Windows par l'Aide, jusqu'à 15 min)
            "clic:BtnClean" => {
                let ids: Vec<String> =
                    self.cases.nettoyage.iter().filter(|r| r.3).map(|r| r.0.id.to_string()).collect();
                Some(self.geste_fond(ctx, id, avant, move |at, pc| {
                    at.session.nettoyer(pc, &ids);
                    diffusion::lire_nettoyage(pc, &mut at.cases);
                }))
            }
            "nett-journal" => {
                let (f, annonces) = self.avec_pc(ctx, |_, pc| {
                    pc.reglage("LastClean").map(|l| net::champ_texte(&l, "File")).filter(|f| !f.is_empty())
                });
                if let Some(f) = f {
                    // (le chemin entre guillemets : il peut avoir des espaces)
                    self.lancer_programme(ctx, vec!["notepad.exe".into(), format!("\"{f}\"")], effets);
                }
                Some(Fait::Sur(annonces))
            }
            "clic:BtnRefreshStartup" => rien(),
            // (un programme de tout le PC passe par l'Aide)
            "clic:BtnDisableStartup" => {
                let a_couper: Vec<(reglages::Demarrage, String)> = self
                    .cases
                    .demarrage
                    .iter()
                    .filter(|x| x.2 == "safe" && x.0.actif)
                    .map(|x| (x.0.clone(), x.1.clone()))
                    .collect();
                Some(self.geste_fond(ctx, id, avant, move |at, pc| {
                    for (d, nom) in &a_couper {
                        if let Err(e) = at.session.demarrage(pc, d, nom, false) {
                            pc.message(&session::message_erreur(&e));
                        }
                    }
                }))
            }
            _ if id.starts_with("nett-voir:") => {
                effets.ajouter(Effet::Annonce(
                    "La liste des fichiers arrive bientôt dans cette version de Nevermind.".into(),
                ));
                rien()
            }
            _ if id.starts_with("dem:") => {
                let n: usize = id[4..].parse().ok()?;
                let (d, nom) = self.cases.demarrage.get(n).map(|x| (x.0.clone(), x.1.clone()))?;
                let on = net::vrai(valeur);
                Some(self.geste_fond(ctx, id, avant, move |at, pc| {
                    if let Err(e) = at.session.demarrage(pc, &d, &nom, on) {
                        pc.message(&session::message_erreur(&e));
                    }
                }))
            }
            _ => None,
        }
    }

    /// Un geste long (audit G3) : lancé en fond, avec le texte d'attente montré tout de suite ; sa fin fait la
    /// suite (`apres`, `diffuser`). Sans contexte partagé (tests d'un bloc seul), il est déjà fini ici.
    fn geste_fond(
        &mut self,
        ctx: &Contexte,
        id: &str,
        avant: bool,
        f: impl FnOnce(&mut Atelier, &mut dyn Pc) + Send + 'static,
    ) -> Fait {
        let verrou = self.verrou_du_geste(id);
        match self.en_fond(ctx, id, Suite::Geste { avant }, true, verrou, f) {
            Ok(true) => Fait::Fond,
            // (un geste de la page tourne encore : dit en clair, rien ne part ; lot B)
            Ok(false) => Fait::Sur(vec![Effet::Annonce(self.refus_occupe())]),
            // (un autre geste tient le verrou : le clic est refusé, et rien ne part plus tard)
            Err(refus) => Fait::Sur(vec![Effet::Annonce(refus)]),
        }
    }

    /// Ce que le geste `id` (ou la demande d'un autre bloc) fait du verrou des gestes, et son nom dans « Attends la fin
    /// de ... »
    fn verrou_du_geste(&self, id: &str) -> Verrou {
        let clic = |n: &str| Verrou::Clic(n.to_string());
        let ecran = Verrou::Priorite("Fréquence de l'écran".into());
        let s = &self.session;
        let attend_l_ecran = matches!(s.attente, Some(session::Attente::Ecran(_)));
        let en_attente = s.geste_en_attente().unwrap_or("Tout corriger");
        let fiche = s.fiche.as_ref();
        let mode = fiche.map(|f| f.mode.as_str()).unwrap_or("");
        match id {
            "analyser" | "clic:BtnCleanScan" | "fiche:close" | "retour" => Verrou::Aucun,
            "clic:BtnApply" => clic("Appliquer"),
            "clic:BtnClean" => clic("Nettoyer"),
            "clic:BtnDisableStartup" | "demarrage-desactiver" => clic("Désactiver au démarrage"),
            "clic:BtnUndoAll" | "pc:tout-restaurer" => clic("Tout restaurer"),
            "fiche:annuler" => clic("Revenir en arrière"),
            "historique-annuler" => clic("Annuler"),
            "registre-ecrire" => clic("Réactiver"),
            "appliquer-reglages-windows" => clic("Régler Windows"),
            "declarations" => Verrou::Tour("Profil de jeu".into(), "Jeux".into()),
            "pc:reponse" if attend_l_ecran => ecran,
            "pc:reponse" => clic(en_attente),
            "fiche:run" => match mode {
                "display" => ecran,
                "question" => clic(en_attente),
                _ if fiche.is_some_and(|f| f.items.len() > 1) => clic("Tout corriger"),
                _ => clic("Corriger"),
            },
            "fiche:ignore" => match mode {
                "display" => ecran,
                "result" => clic("Revenir en arrière"),
                _ => Verrou::Aucun,
            },
            _ if id.starts_with("dem:") => clic("Démarrage"),
            _ => clic(id),
        }
    }

    /// Les gestes de la page que ce bloc prend (pour ne pas en faire un deuxième pendant un geste long)
    fn est_a_moi(&self, id: &str) -> bool {
        let num = |p: &str| id.strip_prefix(p).is_some_and(|n| n.parse::<usize>().is_ok());
        matches!(
            id,
            "tweak"
                | "opti:recommande"
                | "coche:ChkRestore"
                | "clic:BtnApply"
                | "nett"
                | "clic:BtnCleanScan"
                | "clic:BtnClean"
                | "nett-journal"
                | "clic:BtnRefreshStartup"
                | "clic:BtnDisableStartup"
                | "analyser"
                | "toutcorriger"
                | "pc:reponse"
                | "fiche:annuler"
                | "fiche:run"
                | "fiche:ignore"
                | "fiche:close"
                | "fiche:open"
                | "clic:BtnUndoAll"
                | "pc:tout-restaurer"
        ) || (id == "retour" && self.session.fiche.is_some())
            || id.starts_with("nett-voir:")
            || id.starts_with("dem:")
            || num("fiche:")
            || num("constat:")
            || num("composant:")
    }
}

impl Bloc for BlocPc {
    fn nom(&self) -> &'static str {
        "pc"
    }

    fn pages(&mut self, ctx: &Contexte, ecran: &str) -> Map<String, Value> {
        self.charger(ctx);
        self.suivre_lecture(ctx);
        self.ecran = ecran.to_string();
        let mut p = pages::pages_pc(self.session.analyse.as_ref(), &self.live, &mut self.kit);
        p.insert("tests".into(), self.tests.page());
        if let Some(pa) = self.tests.panneau() {
            p.insert("panneau".into(), pa);
        }
        if let Some(f) = &self.session.fiche {
            p.insert("fiche".into(), fiche::page(f, self.fiche_de.as_deref()));
        }
        // La tuile « Ton PC » de l'accueil (Get-EcranNevermind : le score, et les points de Get-HubFindings)
        let r = session::resume(self.session.analyse.as_ref());
        p.insert("_accueil".into(), json!({ "pc": { "score": r["score"], "a_regler": r.get("aRegler") } }));
        p
    }

    fn action(&mut self, ctx: &Contexte, id: &str, valeur: &Value, effets: &mut Effets) -> bool {
        self.charger(ctx);
        // (les tests des composants et leur panneau : tests_composants.rs ; ici seulement ce qui passe par le PC)
        if id.starts_with("test-b:") || id.starts_with("panneau:") {
            let Some(gestes) = self.tests.action(ctx, id, effets) else { return false };
            let mut batterie = false;
            for g in gestes {
                match g {
                    // (mdsched : par ShellExecute, dans un travail de fond, M8)
                    tests_composants::Geste::Lancer(a) => self.lancer_programme(ctx, a, effets),
                    tests_composants::Geste::Batterie => batterie = true,
                    _ => {}
                }
            }
            let (_, annonces) = self.avec_pc(ctx, |_, pc| {
                if batterie {
                    let f = format!("{}\\rapport-batterie.html", pc.dossier_temp());
                    if pc.commande("powercfg.exe", &["/batteryreport".into(), "/output".into(), f.clone()]) == 0 {
                        pc.ouvrir(&f);
                    }
                }
            });
            for a in annonces {
                effets.ajouter(a);
            }
            return true;
        }
        self.rendre_en_attente(effets);
        // (un geste long sur la session est en cours, audit G3 : pas un deuxième geste par-dessus, un double clic ne
        //  refait rien ; la page montre l'attente)
        if self.travaux.occupe() && self.est_a_moi(id) {
            ctx.journal(&format!("Ton PC : geste {id} pas fait, un geste est encore en cours"));
            // (dit en clair, et la page redessinée : une bascule revient à sa place avec une explication ; lot B)
            effets.ajouter(Effet::Annonce(self.refus_occupe()));
            effets.ajouter(Effet::Rafraichir);
            return true;
        }
        let avant = self.session.fiche.is_some();
        let num = |p: &str| id.strip_prefix(p).and_then(|n| n.parse::<usize>().ok());
        let question =
            self.session.attente.is_some() && self.session.fiche.as_ref().is_some_and(|f| f.mode == "question");
        let fait = match self.action_perf(ctx, id, valeur, avant, effets) {
            Some(f) => f,
            None => match id {
                "analyser" => {
                    effets.ajouter(Effet::Annonce("Analyse de ton PC lancée".into()));
                    // (relire le PC : les données lentes aussi, comme $AnalysisDataWork ; WMI, dans le travail de fond)
                    // (pas en construction d'essai sur le vrai PC ; un faux PC décide lui-même, `Pc::relire`)
                    let relire = self.lu && (self.essai.is_some() || !ctx.essai);
                    self.geste_fond(ctx, id, avant, move |at, pc| {
                        if relire && let Some(lu) = pc.relire() {
                            at.session.entrees = lu.entrees;
                            at.session.demarrage = lu.demarrage;
                            // (et ce que seule l'Aide lit, à chaque analyse comme `Invoke-Analysis` :
                            //  `Add-AideLecturePc`, analyse.ps1:195 ; audit M10)
                            systeme::completer_par_aide(pc, &mut at.session.entrees);
                            // (et le dernier pilote NVIDIA, demandé à chaque lecture comme `$AnalysisDataWork`)
                            systeme::completer_pilote_nvidia(pc, &mut at.session.entrees);
                            at.relu = true;
                        }
                        at.session.analyser(pc);
                    })
                }
                "toutcorriger" => Fait::Sur(self.avec_pc(ctx, |s, pc| s.session.ouvrir_tout(pc)).1),
                "pc:reponse" => {
                    let oui = net::champ_texte(valeur, "valeur") == "oui" || valeur.as_str() == Some("oui");
                    self.geste_fond(ctx, id, avant, move |at, pc| at.session.repondre(pc, oui))
                }
                // (« Revenir en arrière » de la fiche de résultat d'un autre bloc, montrée par l'hôte : `Invoke-UndoLastRun`)
                "fiche:annuler" => {
                    let log = net::champ_liste(valeur, "log");
                    let h = Some(net::champ_texte(valeur, "historique")).filter(|h| !h.is_empty());
                    self.geste_fond(ctx, id, avant, move |at, pc| at.session.revenir_journal(pc, Some(log), h))
                }
                "fiche:run" => self.geste_fond(ctx, id, avant, |at, pc| at.session.executer(pc)),
                "fiche:ignore" => self.geste_fond(ctx, id, avant, |at, pc| at.session.bouton_ignorer(pc)),
                // (« Non » à une question : la suite de « Tout corriger » peut continuer, en fond)
                "fiche:close" if question => self.geste_fond(ctx, id, avant, |at, pc| at.session.repondre(pc, false)),
                "fiche:close" => {
                    if self.session.fiche.as_ref().is_some_and(|f| f.mode != "display") {
                        self.session.fiche = None;
                    }
                    Fait::Sur(Vec::new())
                }
                "fiche:open" => {
                    let cible = self.session.fiche.as_ref().and_then(|f| f.ouvrir.clone());
                    if let Some(c) = cible {
                        self.session.fiche = None;
                        self.aller(ctx, &c, effets);
                    }
                    Fait::Sur(Vec::new())
                }
                "retour" if self.session.fiche.is_some() => {
                    match self.session.fiche.as_ref().map(|f| f.mode.clone()).as_deref() {
                        Some("display") => Fait::Sur(Vec::new()),
                        Some("question") => self.geste_fond(ctx, id, avant, |at, pc| at.session.repondre(pc, false)),
                        _ => {
                            self.session.fiche = None;
                            Fait::Sur(Vec::new())
                        }
                    }
                }
                "clic:BtnUndoAll" | "pc:tout-restaurer" => self.geste_fond(ctx, id, avant, |at, pc| {
                    let d = systeme::changements_definitifs(pc);
                    at.session.restaurer_tout(pc, &d);
                }),
                _ if num("fiche:").is_some() || num("constat:").is_some() => {
                    let f = if let Some(k) = num("fiche:") {
                        self.kit.fiches.get(k).cloned()
                    } else {
                        num("constat:").and_then(|k| self.kit.constats.get(k).cloned())
                    };
                    match f {
                        Some(f) => Fait::Sur(self.avec_pc(ctx, |s, pc| s.session.ouvrir(pc, &f)).1),
                        None => Fait::Sur(Vec::new()),
                    }
                }
                _ if num("composant:").is_some() => {
                    if let Some(x) = num("composant:").and_then(|k| self.kit.actions_composants.get(k).cloned()) {
                        self.aller(ctx, &x, effets);
                    }
                    Fait::Sur(Vec::new())
                }
                _ => return false,
            },
        };
        match fait {
            Fait::Sur(annonces) => {
                self.apres(avant, annonces, effets);
                self.diffuser(ctx, effets);
            }
            // (la suite à la fin du travail ; déjà là sans contexte partagé)
            Fait::Fond => self.relever_travaux(ctx, effets),
        }
        true
    }

    fn tic(&mut self, ctx: &Contexte, cache: bool, effets: &mut Effets) -> Option<Duration> {
        // (les tests des composants : toutes les 100 ms pendant un test ; sinon un tour par demi-seconde, sans rien
        //  lire, pour qu'un test lancé par un bouton soit suivi : l'hôte ne rappelle pas un bloc qui a dit « plus besoin »)
        self.rendre_en_attente(effets);
        self.relever_travaux(ctx, effets);
        let t = self.tests.tic_bloc(ctx, effets);
        let r = self.tic_pc(ctx, cache, effets);
        Some(r.map_or(t, |r| r.min(t)))
    }

    fn travaux_finis(&mut self, ctx: &Contexte, effets: &mut Effets) {
        self.relever_lancements(effets);
        self.relever_travaux(ctx, effets);
        self.diffuser_si_du(ctx, effets);
    }

    fn signal(&mut self, ctx: &Contexte, nom: &str, donnees: &Value, effets: &mut Effets) {
        match nom {
            "mesures" => self.tests.mesures(donnees),
            "test-debit" => return self.tests.test_debit(ctx, effets),
            _ => {}
        }
        self.signal_pc(ctx, nom, donnees, effets);
    }

    fn fermer(&mut self, _ctx: &Contexte) {
        self.tests.arreter_tout();
    }
}

impl BlocPc {
    fn tic_pc(&mut self, ctx: &Contexte, _cache: bool, effets: &mut Effets) -> Option<Duration> {
        self.charger(ctx);
        if !self.lu {
            self.suivre_lecture(ctx);
            if !self.lu {
                return Some(Duration::from_millis(250));
            }
        }
        // (la lecture finie ici, ou dans `pages` quand l'écran est regardé : les autres blocs et la carte de Mindset
        //  la reçoivent une fois)
        if std::mem::take(&mut self.a_diffuser) {
            effets.ajouter(Effet::Rafraichir);
            self.diffuser(ctx, effets);
            return None;
        }
        // « Tes écrans s'affichent bien ? » : le compte à rebours, puis l'ancien réglage revient tout seul (pas pendant
        //  un geste long : sa fin remet la session en place)
        if let Some(fin) = self.fin_ecran.filter(|_| !self.travaux.occupe()) {
            let reste = fin.saturating_duration_since(Instant::now()).as_secs_f64().ceil() as i64;
            if reste <= 0 {
                // (automatique : jamais refusé, l'écran doit revenir à l'ancienne fréquence ; ajustement A du lot A)
                let _priorite = ctx.geste_prioritaire("Fréquence de l'écran", PAGE);
                let (_, a) = self.avec_pc(ctx, |s, pc| s.session.repondre_ecran(pc, "timeout"));
                self.apres(true, a, effets);
                return None;
            }
            if let Some(f) = self.session.fiche.as_mut() {
                f.boutons.ignore = Some(format!("Revenir en arrière ({reste})"));
            }
            effets.ajouter(Effet::Rafraichir);
            return Some(Duration::from_millis(250));
        }
        None
    }

    fn signal_pc(&mut self, ctx: &Contexte, nom: &str, donnees: &Value, effets: &mut Effets) {
        self.charger(ctx);
        let texte = |k: &str| net::champ_texte(donnees, k);
        let liste = |k: &str| net::champ_liste(donnees, k).iter().map(net::texte).collect::<Vec<_>>();
        let avant = self.session.fiche.is_some();
        // (un geste long est en cours, audit G3 : sa session sera remise en place à sa fin ; les demandes des autres
        //  blocs sont faites après, dans l'ordre, et sauvegarde.json relue)
        if self.travaux.occupe() {
            match nom {
                "sauvegarde-changee" => {
                    self.sv_a_relire = true;
                    return;
                }
                // (le clic d'une autre page qui change le PC, pendant un geste de Ton PC qui tient le verrou : refusé,
                //  « Attends la fin de ... », et rien ne part plus tard ; lot A du second audit)
                "appliquer-reglages-windows" | "registre-ecrire" | "demarrage-desactiver" | "historique-annuler"
                    if ctx.refus_geste().is_some() =>
                {
                    let refus = ctx.refus_geste().unwrap_or_default();
                    ctx.journal(&format!("Ton PC : {nom} refusé ({refus})"));
                    effets.ajouter(Effet::Annonce(refus));
                    return;
                }
                // (un profil de jeu, automatique, n'est jamais refusé : il attend son tour)
                "appliquer-reglages-windows"
                | "declarations"
                | "registre-ecrire"
                | "demarrage-desactiver"
                | "historique-annuler"
                | "ouvrir-fiche" => {
                    ctx.journal(&format!("Ton PC : {nom} attend la fin du geste en cours"));
                    self.signaux_en_attente.push((nom.to_string(), donnees.clone()));
                    return;
                }
                _ => {}
            }
        }
        // Les demandes qui passent par l'Aide (un réglage du PC) : dans un travail de fond
        let fond = match nom {
            // (la fiche d'un jeu ou d'une partie, bloc jeux : `Invoke-TweakFix`)
            "appliquer-reglages-windows" => {
                let ids = liste("ids");
                Some(self.geste_fond(ctx, nom, avant, move |at, pc| at.session.corriger_reglages(pc, &ids)))
            }
            // (le profil d'un jeu, blocs perf et jeux : `Set-GameProfile`, les changements déclarés faits ici, notés
            //  dans l'historique sous le titre donné : « historique » du bloc perf, « titre » du bloc jeux)
            "declarations" => {
                let l: Vec<modele::Declaration> =
                    net::champ_liste(donnees, "changements").iter().map(modele::Declaration::depuis_json).collect();
                let t = if texte("historique").is_empty() { texte("titre") } else { texte("historique") };
                Some(self.geste_fond(ctx, nom, avant, move |at, pc| {
                    if let Err(e) = at.session.declarations(pc, &l, &t) {
                        pc.message(&session::message_erreur(&e));
                    }
                }))
            }
            // (« Réactiver » le contrôle des comptes, bloc protection : `Set-Reg` avec sa sauvegarde, puis l'historique)
            "registre-ecrire" => {
                let mut d = donnees.clone();
                d["Type"] = json!("reg");
                let l = vec![modele::Declaration::depuis_json(&d)];
                let t = texte("titre");
                Some(self.geste_fond(ctx, nom, avant, move |at, pc| {
                    if let Err(e) = at.session.declarations(pc, &l, &t) {
                        pc.message(&session::message_erreur(&e));
                    }
                }))
            }
            // (« Désactiver au démarrage » des programmes suspects, bloc protection : `Set-StartupState $i $false`)
            "demarrage-desactiver" => {
                let a_couper: Vec<(reglages::Demarrage, String)> = net::champ_liste(donnees, "elements")
                    .iter()
                    .filter_map(|e| {
                        let (n, x) = (net::champ_texte(e, "Nom"), net::champ_texte(e, "Exe"));
                        self.cases
                            .demarrage
                            .iter()
                            .find(|d| {
                                d.0.nom.eq_ignore_ascii_case(&n) && (x.is_empty() || d.0.exe.eq_ignore_ascii_case(&x))
                            })
                            .or_else(|| self.cases.demarrage.iter().find(|d| d.0.nom.eq_ignore_ascii_case(&n)))
                            .map(|d| (d.0.clone(), d.1.clone()))
                    })
                    .collect();
                Some(self.geste_fond(ctx, nom, avant, move |at, pc| {
                    for (d, n) in &a_couper {
                        if let Err(e) = at.session.demarrage(pc, d, n, false) {
                            pc.message(&session::message_erreur(&e));
                        }
                    }
                }))
            }
            // (« Annuler » d'une ligne de l'historique, page Sauvegarde du bloc protection : `Undo-HistoryEntry`)
            "historique-annuler" => {
                let id = texte("id");
                let h = ctx.historique().into_iter().find(|h| h["Id"].as_str() == Some(id.as_str()));
                h.map(|h| self.geste_fond(ctx, nom, avant, move |at, pc| at.session.annuler_entree(pc, &h)))
            }
            _ => None,
        };
        if let Some(f) = fond {
            if let Fait::Sur(annonces) = f {
                for a in annonces {
                    effets.ajouter(a);
                }
            }
            self.relever_travaux(ctx, effets);
            return;
        }
        let (fait, a) = match nom {
            "mesures" => {
                self.live = donnees.clone();
                (false, Vec::new())
            }
            // (un autre bloc a écrit sauvegarde.json, ou l'historique a changé : la sauvegarde est relue, pour ne
            //  jamais écraser ce qu'il y a noté)
            "sauvegarde-changee" => {
                if self.session.attente.is_none() {
                    self.session.sv = systeme::lire_sauvegarde(ctx);
                }
                // (puis le compte de la page Sauvegarde, rediffusé)
                (self.session.analyse.is_some(), Vec::new())
            }
            "ouvrir-fiche" => {
                let id = texte("id");
                let f = self.session.analyse.as_ref().and_then(|a| a.actifs.iter().find(|f| f.id == id).cloned());
                match f {
                    Some(f) => self.avec_pc(ctx, |s, pc| {
                        s.session.ouvrir(pc, &f);
                        true
                    }),
                    None => (false, Vec::new()),
                }
            }
            _ => (false, Vec::new()),
        };
        if fait {
            self.apres(avant, a, effets);
            self.diffuser(ctx, effets);
        }
    }

    /// Les effets demandés hors d'un appel qui en reçoit (fin de la lecture relevée dans `pages`)
    fn rendre_en_attente(&mut self, effets: &mut Effets) {
        for e in self.en_attente.drain(..) {
            effets.ajouter(e);
        }
    }
}
