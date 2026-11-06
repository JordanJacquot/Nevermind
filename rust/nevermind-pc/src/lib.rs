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

use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use pc::Pc;
use serde_json::{Map, Value};
use session::Session;
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

#[derive(Default)]
pub struct BlocPc {
    session: Session,
    kit: pages::Kit,
    /// Les mesures en direct (`$Live`), reçues par le signal « mesures »
    live: Value,
    /// La lecture lente du PC, dans un fil à part
    lecture: Option<std::thread::JoinHandle<lecture::Lu>>,
    lu: bool,
    charge: bool,
    /// D'où la fiche a été ouverte (`$script:Kit.FicheDe`)
    fiche_de: Option<String>,
    fin_ecran: Option<Instant>,
    /// Un faux PC (tests) : rien ne touche le vrai PC
    essai: Option<Box<dyn Pc + Send>>,
    tests: tests_composants::Tests,
    /// Les cases des pages du bloc perf (réglages, nettoyage) et la liste de démarrage
    cases: diffusion::Cases,
    /// L'écran montré (le dernier demandé par l'hôte)
    ecran: String,
}

impl BlocPc {
    /// Le bloc sur un faux PC (tests), avec ce qu'il aurait lu
    pub fn sur(pc: Box<dyn Pc + Send>, lu: lecture::Lu) -> Self {
        let mut b = BlocPc { essai: Some(pc), lu: true, charge: true, ..Default::default() };
        b.prendre(lu);
        b
    }

    /// La session (tests)
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Le faux PC (tests)
    pub fn faux_pc(&mut self) -> Option<&mut (dyn Pc + Send + 'static)> {
        self.essai.as_deref_mut()
    }

    fn prendre(&mut self, lu: lecture::Lu) {
        self.session.entrees = lu.entrees;
        self.session.demarrage = lu.demarrage;
        self.tests.donnees = self.session.entrees.data.clone();
    }

    /// Le PC sur lequel agir : le faux (tests) ou le vrai
    fn avec_pc<R>(&mut self, ctx: &Contexte, f: impl FnOnce(&mut Self, &mut dyn Pc) -> R) -> (R, Vec<Effet>) {
        if let Some(mut p) = self.essai.take() {
            let r = f(self, p.as_mut());
            self.essai = Some(p);
            return (r, Vec::new());
        }
        let mut p = systeme::PcWindows::nouveau(ctx);
        let r = f(self, &mut p);
        (r, p.annonces())
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
        match &self.lecture {
            None => {
                if ctx.essai && self.essai.is_none() {
                    // (mode test sans faux PC : rien n'est lu sur le vrai PC)
                    self.lu = true;
                    return;
                }
                self.lecture = Some(std::thread::spawn(lecture::lire));
            }
            Some(h) if h.is_finished() => {
                if let Some(h) = self.lecture.take()
                    && let Ok(lu) = h.join()
                {
                    self.prendre(lu);
                }
                self.lu = true;
                self.avec_pc(ctx, |s, pc| {
                    systeme::completer_par_aide(pc, &mut s.session.entrees);
                    s.session.analyser(pc)
                });
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
            self.avec_pc(ctx, |_, pc| pc.lancer(&args));
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
        let (plan_ok, _) = self.avec_pc(ctx, |s, pc| {
            let cree = s.session.sv.plan_cree.clone();
            diffusion::lire_reglages(pc, build, cree.as_deref(), &mut s.cases);
            diffusion::lire_demarrage(pc, &s.session.demarrage, &mut s.cases);
            s.tests.ecrans = pc.ecrans();
            reglages::test_plan(pc, cree.as_deref())
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
        effets.ajouter(sig("pc:analyse", session::resume(Some(&a))));
    }

    /// Les gestes des pages du bloc perf dont la logique est ici (onglets Optimisation, Démarrage, Nettoyage)
    fn action_perf(&mut self, ctx: &Contexte, id: &str, valeur: &Value, effets: &mut Effets) -> Option<Vec<Effet>> {
        let k = net::champ_entier(valeur, "valeur") as usize;
        let oui = net::champ_vrai(valeur, "oui");
        match id {
            "tweak" => {
                if let Some(r) = self.cases.reglages.get_mut(k).filter(|r| !r.1) {
                    r.2 = oui;
                }
                Some(Vec::new())
            }
            "opti:recommande" => {
                for r in self.cases.reglages.iter_mut().filter(|r| !r.1) {
                    r.2 = reglages::reglage(&r.0).is_some_and(|t| t.conseille);
                }
                Some(Vec::new())
            }
            "coche:ChkRestore" => {
                self.cases.restauration = net::vrai(valeur);
                Some(Vec::new())
            }
            "clic:BtnApply" => {
                let ids: Vec<String> =
                    self.cases.reglages.iter().filter(|r| !r.1 && r.2).map(|r| r.0.clone()).collect();
                let rest = self.cases.restauration;
                Some(self.avec_pc(ctx, |s, pc| s.session.appliquer_reglages(pc, &ids, rest)).1)
            }
            "nett" => {
                if let Some(r) = self.cases.nettoyage.get_mut(k).filter(|r| r.2 > 0) {
                    r.3 = oui;
                }
                Some(Vec::new())
            }
            "clic:BtnCleanScan" => Some(self.avec_pc(ctx, |s, pc| diffusion::lire_nettoyage(pc, &mut s.cases)).1),
            "clic:BtnClean" => {
                let ids: Vec<String> =
                    self.cases.nettoyage.iter().filter(|r| r.3).map(|r| r.0.id.to_string()).collect();
                Some(
                    self.avec_pc(ctx, |s, pc| {
                        s.session.nettoyer(pc, &ids);
                        diffusion::lire_nettoyage(pc, &mut s.cases);
                    })
                    .1,
                )
            }
            "nett-journal" => Some(
                self.avec_pc(ctx, |_, pc| {
                    if let Some(f) =
                        pc.reglage("LastClean").map(|l| net::champ_texte(&l, "File")).filter(|f| !f.is_empty())
                    {
                        pc.lancer(&["notepad.exe".into(), f]);
                    }
                })
                .1,
            ),
            "clic:BtnRefreshStartup" => Some(Vec::new()),
            "clic:BtnDisableStartup" => {
                let a_couper: Vec<(reglages::Demarrage, String)> = self
                    .cases
                    .demarrage
                    .iter()
                    .filter(|x| x.2 == "safe" && x.0.actif)
                    .map(|x| (x.0.clone(), x.1.clone()))
                    .collect();
                Some(
                    self.avec_pc(ctx, |s, pc| {
                        for (d, nom) in &a_couper {
                            if let Err(e) = s.session.demarrage(pc, d, nom, false) {
                                pc.message(&session::message_erreur(&e));
                            }
                        }
                    })
                    .1,
                )
            }
            _ if id.starts_with("nett-voir:") => {
                effets.ajouter(Effet::Annonce(
                    "La liste des fichiers arrive bientôt dans cette version de Nevermind.".into(),
                ));
                Some(Vec::new())
            }
            _ if id.starts_with("dem:") => {
                let n: usize = id[4..].parse().ok()?;
                let (d, nom) = self.cases.demarrage.get(n).map(|x| (x.0.clone(), x.1.clone()))?;
                let on = net::vrai(valeur);
                Some(
                    self.avec_pc(ctx, |s, pc| {
                        if let Err(e) = s.session.demarrage(pc, &d, &nom, on) {
                            pc.message(&session::message_erreur(&e));
                        }
                    })
                    .1,
                )
            }
            _ => None,
        }
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
        p
    }

    fn action(&mut self, ctx: &Contexte, id: &str, valeur: &Value, effets: &mut Effets) -> bool {
        self.charger(ctx);
        // (les tests des composants et leur panneau : tests_composants.rs ; ici seulement ce qui passe par le PC)
        if id.starts_with("test-b:") || id.starts_with("panneau:") {
            let Some(gestes) = self.tests.action(ctx, id, effets) else { return false };
            let (_, annonces) = self.avec_pc(ctx, |_, pc| {
                for g in gestes {
                    match g {
                        tests_composants::Geste::Lancer(a) => pc.lancer(&a),
                        tests_composants::Geste::Batterie => {
                            let f = format!("{}\\rapport-batterie.html", pc.dossier_temp());
                            if pc.commande("powercfg.exe", &["/batteryreport".into(), "/output".into(), f.clone()]) == 0
                            {
                                pc.ouvrir(&f);
                            }
                        }
                        _ => {}
                    }
                }
            });
            for a in annonces {
                effets.ajouter(a);
            }
            return true;
        }
        let avant = self.session.fiche.is_some();
        if let Some(a) = self.action_perf(ctx, id, valeur, effets) {
            self.apres(avant, a, effets);
            self.diffuser(ctx, effets);
            return true;
        }
        let num = |p: &str| id.strip_prefix(p).and_then(|n| n.parse::<usize>().ok());
        let (pris, annonces) = match id {
            "analyser" => {
                effets.ajouter(Effet::Annonce("Analyse de ton PC lancée".into()));
                if self.lu && self.essai.is_none() && !ctx.essai {
                    // (relire le PC : les données lentes aussi, comme $AnalysisDataWork)
                    let lu = lecture::lire();
                    self.prendre(lu);
                }
                self.avec_pc(ctx, |s, pc| {
                    s.session.analyser(pc);
                    true
                })
            }
            "toutcorriger" => self.avec_pc(ctx, |s, pc| {
                s.session.ouvrir_tout(pc);
                true
            }),
            "pc:reponse" => {
                let oui = net::champ_texte(valeur, "valeur") == "oui" || valeur.as_str() == Some("oui");
                self.avec_pc(ctx, |s, pc| {
                    s.session.repondre(pc, oui);
                    true
                })
            }
            "fiche:run" => self.avec_pc(ctx, |s, pc| {
                s.session.executer(pc);
                true
            }),
            "fiche:ignore" => self.avec_pc(ctx, |s, pc| {
                s.session.bouton_ignorer(pc);
                true
            }),
            "fiche:close" => self.avec_pc(ctx, |s, pc| {
                if s.session.attente.is_some() && s.session.fiche.as_ref().is_some_and(|f| f.mode == "question") {
                    s.session.repondre(pc, false);
                } else if s.session.fiche.as_ref().is_some_and(|f| f.mode != "display") {
                    s.session.fiche = None;
                }
                true
            }),
            "fiche:open" => {
                let cible = self.session.fiche.as_ref().and_then(|f| f.ouvrir.clone());
                if let Some(c) = cible {
                    self.session.fiche = None;
                    self.aller(ctx, &c, effets);
                }
                (true, Vec::new())
            }
            "retour" if self.session.fiche.is_some() => self.avec_pc(ctx, |s, pc| {
                match s.session.fiche.as_ref().map(|f| f.mode.clone()).as_deref() {
                    Some("display") => {}
                    Some("question") => s.session.repondre(pc, false),
                    _ => s.session.fiche = None,
                }
                true
            }),
            "clic:BtnUndoAll" | "pc:tout-restaurer" => self.avec_pc(ctx, |s, pc| {
                let d = systeme::changements_definitifs(pc);
                s.session.restaurer_tout(pc, &d);
                true
            }),
            _ if num("fiche:").is_some() || num("constat:").is_some() => {
                let f = if let Some(k) = num("fiche:") {
                    self.kit.fiches.get(k).cloned()
                } else {
                    num("constat:").and_then(|k| self.kit.constats.get(k).cloned())
                };
                match f {
                    Some(f) => self.avec_pc(ctx, |s, pc| {
                        s.session.ouvrir(pc, &f);
                        true
                    }),
                    None => (true, Vec::new()),
                }
            }
            _ if num("composant:").is_some() => {
                if let Some(x) = num("composant:").and_then(|k| self.kit.actions_composants.get(k).cloned()) {
                    self.aller(ctx, &x, effets);
                }
                (true, Vec::new())
            }
            _ => (false, Vec::new()),
        };
        if pris {
            self.apres(avant, annonces, effets);
            self.diffuser(ctx, effets);
        }
        pris
    }

    fn tic(&mut self, ctx: &Contexte, cache: bool, effets: &mut Effets) -> Option<Duration> {
        // (les tests des composants : toutes les 100 ms pendant un test ; sinon un tour par demi-seconde, sans rien
        //  lire, pour qu'un test lancé par un bouton soit suivi : l'hôte ne rappelle pas un bloc qui a dit « plus besoin »)
        let t = self.tests.tic_bloc(ctx, effets);
        let r = self.tic_pc(ctx, cache, effets);
        Some(r.map_or(t, |r| r.min(t)))
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
            if self.lu {
                effets.ajouter(Effet::Rafraichir);
                self.diffuser(ctx, effets);
                return None;
            }
            return Some(Duration::from_millis(250));
        }
        // « Tes écrans s'affichent bien ? » : le compte à rebours, puis l'ancien réglage revient tout seul
        if let Some(fin) = self.fin_ecran {
            let reste = fin.saturating_duration_since(Instant::now()).as_secs_f64().ceil() as i64;
            if reste <= 0 {
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
        let (fait, a) = match nom {
            "mesures" => {
                self.live = donnees.clone();
                (false, Vec::new())
            }
            "pc:analyser" => self.avec_pc(ctx, |s, pc| {
                s.session.analyser(pc);
                true
            }),
            "pc:appliquer-reglages" => self.avec_pc(ctx, |s, pc| {
                s.session.appliquer_reglages(pc, &liste("ids"), net::champ_vrai(donnees, "restauration"));
                true
            }),
            "pc:demarrage" => self.avec_pc(ctx, |s, pc| {
                let d = reglages::Demarrage {
                    nom: texte("nom"),
                    source: String::new(),
                    commande: String::new(),
                    approuve: texte("approuve"),
                    nom_valeur: texte("nomValeur"),
                    actif: true,
                    exe: String::new(),
                };
                if let Err(e) = s.session.demarrage(pc, &d, &texte("nom"), net::champ_vrai(donnees, "activer")) {
                    pc.message(&session::message_erreur(&e));
                }
                true
            }),
            "pc:nettoyer" => self.avec_pc(ctx, |s, pc| {
                s.session.nettoyer(pc, &liste("ids"));
                true
            }),
            "pc:tout-restaurer" => self.avec_pc(ctx, |s, pc| {
                let d = systeme::changements_definitifs(pc);
                s.session.restaurer_tout(pc, &d);
                true
            }),
            // (le profil d'un jeu, bloc perf : `Set-GameProfile`, les changements déclarés faits ici, notés dans l'historique)
            "declarations" => self.avec_pc(ctx, |s, pc| {
                let l: Vec<modele::Declaration> =
                    net::champ_liste(donnees, "changements").iter().map(modele::Declaration::depuis_json).collect();
                if let Err(e) = s.session.declarations(pc, &l, &texte("historique")) {
                    pc.message(&session::message_erreur(&e));
                }
                true
            }),
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
}
