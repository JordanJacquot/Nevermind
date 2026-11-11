//! Nevermind en Rust : le bloc « stream », le mode streameur. Il règle le LOGICIEL de stream (OBS Studio, Streamlabs
//! Desktop), jamais le PC : la qualité (taille de l'image, images par seconde), le débit, l'encodeur (la carte
//! graphique), d'après la vitesse d'envoi de la connexion, la carte graphique, l'écran et la plateforme.
//!
//! - [`calcul`] : le choix de la qualité (fonctions pures) ;
//! - [`ini`], [`reglages`], [`service`] : les fichiers d'OBS et de Streamlabs, lus et changés sans rien perdre ;
//! - [`sauvegarde`] : la copie des anciens réglages (seulement ce que Nevermind change) et « Remettre comme avant » ;
//! - [`systeme`] (et `reel`, `windows`) : ce qui est lu du PC (logiciels installés, processus, cartes, écran) ;
//! - [`pages`] : la page « Mode streameur » et sa carte dans Performances.
//!
//! Page : `streameur` (dans Performances). Actions : `stream:optimiser:<obs|streamlabs>`,
//! `stream:remettre:<...>`, `stream:plateforme:<...>` (valeur : twitch, youtube, kick, facebook, autre),
//! `stream:mesurer`.
//! Signaux envoyés : `streameur` {disponible, regle} (au bloc perf, pour sa carte), `test-debit` {} (au bloc pc : le
//! test « Connexion Internet »). Signal attendu : `debit-mesure` {R:[ping, téléchargement, envoi...]} (bloc pc).
//!
//! Règles : Nevermind ne ferme jamais le logiciel (s'il est ouvert : « Ferme OBS puis reclique sur Optimiser ») ; la
//! clé de stream n'est jamais lue dans une variable, ni écrite au journal, ni montrée ; les anciennes valeurs
//! sont notées avant tout changement ; un OBS portable est trouvé quand il est ouvert.

pub mod calcul;
pub mod ini;
pub mod pages;
pub mod reel;
pub mod reglages;
pub mod sauvegarde;
pub mod service;
pub mod systeme;
#[cfg(windows)]
mod windows;

use calcul::{Carte, Entree, Logiciel, Plan, Plateforme, calculer};
use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use pages::{Changement, Vue, VueLogiciel};
use reglages::{Etat, Probleme};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use systeme::{Installation, Systeme};

/// Le fichier de Nevermind pour le mode streameur (dans son dossier de données)
pub const FICHIER: &str = "streameur.json";
/// Le dossier des copies des anciens réglages (une par logiciel)
pub const COPIES: &str = "streameur";
/// Une mesure demandée sans réponse est oubliée après ce délai
const MESURE_MAX: Duration = Duration::from_secs(150);
/// La page ouverte : le logiciel ouvert ou fermé, et ses réglages, sont relus à ce rythme
const SUIVI: Duration = Duration::from_secs(3);

/// Ce que Nevermind garde (streameur.json)
#[derive(Debug, Clone, Default)]
struct Memoire {
    envoi: Option<(u32, String)>,
    plateformes: HashMap<Logiciel, Plateforme>,
    changements: HashMap<Logiciel, Changement>,
    /// Les dossiers des OBS que rien n'a installés (dézippés, portables), vus ouverts une fois
    obs_vus: Vec<PathBuf>,
    /// Le dossier du dernier OBS vu ouvert (installé ou portable) : celui qui est réglé quand il y en a plusieurs
    /// et qu'aucun n'est ouvert (lot K)
    obs_dernier: Option<PathBuf>,
}

impl Memoire {
    fn lire(v: &Value) -> Memoire {
        let mut m = Memoire::default();
        if let Some(l) = v["obs_vus"].as_array() {
            m.obs_vus = l.iter().filter_map(|x| x.as_str().map(PathBuf::from)).collect();
        }
        m.obs_dernier = v["obs_dernier"].as_str().filter(|s| !s.is_empty()).map(PathBuf::from);
        if let (Some(k), Some(d)) = (v["envoi"]["kbps"].as_u64(), v["envoi"]["date"].as_str()) {
            m.envoi = Some((k as u32, d.to_string()));
        }
        for l in [Logiciel::Obs, Logiciel::Streamlabs] {
            if let Some(p) = v["plateformes"][l.id()].as_str().and_then(Plateforme::depuis_id) {
                m.plateformes.insert(l, p);
            }
            let c = &v["changements"][l.id()];
            if let (Some(d), Some(p)) = (c["date"].as_str(), c["phrase"].as_str()) {
                let lignes = c["lignes"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                m.changements.insert(l, Changement { date: d.into(), phrase: p.into(), lignes });
            }
        }
        m
    }
    fn valeur(&self) -> Value {
        let mut plateformes = Map::new();
        let mut changements = Map::new();
        for l in [Logiciel::Obs, Logiciel::Streamlabs] {
            if let Some(p) = self.plateformes.get(&l) {
                plateformes.insert(l.id().into(), json!(p.id()));
            }
            if let Some(c) = self.changements.get(&l) {
                changements.insert(l.id().into(), json!({ "date": c.date, "phrase": c.phrase, "lignes": c.lignes }));
            }
        }
        json!({
            "envoi": self.envoi.as_ref().map(|(k, d)| json!({ "kbps": k, "date": d })),
            "plateformes": plateformes,
            "changements": changements,
            "obs_vus": self.obs_vus.iter().map(|d| d.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            "obs_dernier": self.obs_dernier.as_ref().map(|d| d.to_string_lossy().into_owned()),
        })
    }
}

/// Un logiciel trouvé, avec ce qui a été lu de ses réglages
#[derive(Debug, Clone)]
struct Trouve {
    installation: Installation,
    etat: Result<Etat, Probleme>,
    ouvert: bool,
}

/// Le bloc « stream »
#[derive(Default)]
pub struct BlocStream {
    sys: Option<Box<dyn Systeme>>,
    memoire: Option<Memoire>,
    trouves: Vec<Trouve>,
    detecte: bool,
    dernier_ecran: String,
    cartes: Option<Vec<Carte>>,
    ecran: Option<(u32, u32)>,
    mesure: Option<Instant>,
    mesure_auto: bool,
    alertes: HashMap<Logiciel, String>,
    en_attente: Vec<Effet>,
    carte_envoyee: Option<(bool, bool)>,
    suivi: Option<Instant>,
    en_partie: bool,
    /// Les OBS ouverts à la dernière détection (pour voir s'ouvrir un OBS portable depuis Performances)
    obs_ouverts: Vec<PathBuf>,
}

impl BlocStream {
    /// Avec un faux PC (les tests)
    pub fn avec_systeme(s: Box<dyn Systeme>) -> BlocStream {
        BlocStream { sys: Some(s), ..Default::default() }
    }

    fn sys(&mut self, ctx: &Contexte) -> &dyn Systeme {
        if self.sys.is_none() {
            // (construction d'essai : jamais le registre ni les vrais réglages)
            self.sys = Some(if ctx.essai {
                Box::new(systeme::SystemeEssai { fichier: ctx.donnees.join("streameur-essai.json") })
            } else {
                Box::new(reel::SystemeReel)
            });
        }
        self.sys.as_deref().unwrap_or_else(|| unreachable!())
    }

    fn memoire(&mut self, ctx: &Contexte) -> &mut Memoire {
        if self.memoire.is_none() {
            let v = ctx.lire_json(&ctx.donnees.join(FICHIER)).unwrap_or(Value::Null);
            self.memoire = Some(Memoire::lire(&v));
        }
        self.memoire.get_or_insert_with(Memoire::default)
    }

    fn garder(&mut self, ctx: &Contexte) {
        let v = self.memoire(ctx).valeur();
        if let Err(e) = ctx.ecrire_json(&ctx.donnees.join(FICHIER), &v, false, false) {
            ctx.journal(&format!("Mode streameur : {FICHIER} pas écrit ({e})"));
        }
    }

    fn copie(ctx: &Contexte, l: Logiciel) -> sauvegarde::Copie {
        sauvegarde::Copie::nouvelle(ctx.donnees.join(COPIES).join(l.id()))
    }

    /// Cherche les logiciels installés, relit leurs réglages et s'ils sont ouverts
    fn detecter(&mut self, ctx: &Contexte) {
        let _ = self.memoire(ctx);
        let lire_cartes = self.cartes.is_none();
        let sys = self.sys(ctx);
        let mut installations = sys.installations();
        let processus = sys.processus();
        let ecran = sys.ecran();
        let cartes = if lire_cartes { Some(sys.cartes()) } else { None };
        // Un OBS que rien n'a installé (dézippé, portable) : trouvé par le chemin de son obs64.exe quand il est ouvert,
        // puis gardé (streameur.json) tant que son dossier existe
        let mut vus = self.memoire.as_ref().map(|m| m.obs_vus.clone()).unwrap_or_default();
        let avant = vus.clone();
        for d in systeme::obs_ouverts_inconnus(&processus, &installations) {
            if !vus.iter().any(|x| systeme::meme(x, &d)) {
                vus.push(d);
            }
        }
        let sys = self.sys(ctx);
        let mut gardes = Vec::new();
        for d in vus {
            if let Some(i) = sys.obs_vu(&d) {
                systeme::ajouter(&mut installations, i);
                gardes.push(d);
            }
        }
        self.obs_ouverts = systeme::obs_ouverts_inconnus(&processus, &[]);
        if gardes != avant {
            for d in gardes.iter().filter(|d| !avant.contains(d)) {
                ctx.journal(&format!("Mode streameur : OBS trouvé ouvert dans {}", d.display()));
            }
            self.memoire(ctx).obs_vus = gardes;
            self.garder(ctx);
        }
        let obs = self.choisir_obs(ctx, &installations, &processus);
        // (OBS d'abord, puis Streamlabs : un onglet par logiciel)
        let mut trouves: Vec<Trouve> = Vec::new();
        for l in [Logiciel::Obs, Logiciel::Streamlabs] {
            if let Some(i) = match l {
                Logiciel::Obs => obs.as_ref(),
                Logiciel::Streamlabs => installations.iter().find(|i| i.logiciel == l),
            } {
                let etat = reglages::lire(l, &i.reglages);
                let ouvert = systeme::ouvert(l, &processus, &installations);
                trouves.push(Trouve { installation: i.clone(), etat, ouvert });
            }
        }
        if let Some(c) = cartes {
            self.cartes = Some(c);
        }
        self.ecran = ecran.or(self.ecran);
        if !self.detecte || trouves.len() != self.trouves.len() {
            let noms: Vec<String> = trouves
                .iter()
                .map(|t| format!("{} ({})", t.installation.logiciel.nom(), t.installation.dossier.display()))
                .collect();
            ctx.journal(&format!(
                "Mode streameur : {}",
                if noms.is_empty() { "aucun logiciel de stream".to_string() } else { noms.join(", ") }
            ));
        }
        self.trouves = trouves;
        self.detecte = true;
        self.signaler_carte();
    }

    /// L'OBS à régler quand le PC en a plusieurs (un installé et un portable, par exemple ; lot K) : celui qui est ouvert ;
    /// sinon le dernier vu ouvert (gardé dans streameur.json) ; sans souvenir (streameur.json d'avant), le dernier OBS
    /// portable vu ouvert, puisque l'utilisateur s'en est servi ; sinon le premier trouvé (l'installé), comme avant.
    fn choisir_obs(
        &mut self,
        ctx: &Contexte,
        installations: &[Installation],
        processus: &[systeme::Processus],
    ) -> Option<Installation> {
        let candidats: Vec<&Installation> = installations.iter().filter(|i| i.logiciel == Logiciel::Obs).collect();
        // (les OBS ouverts, sauf le moteur de Streamlabs : aucun OBS n'est « connu » dans cette liste)
        let streamlabs: Vec<Installation> =
            installations.iter().filter(|i| i.logiciel == Logiciel::Streamlabs).cloned().collect();
        let ouverts = systeme::obs_ouverts_inconnus(processus, &streamlabs);
        let ouvert = candidats.iter().find(|i| ouverts.iter().any(|d| systeme::meme(d, &i.dossier)));
        if let Some(i) = ouvert {
            let m = self.memoire(ctx);
            if candidats.len() > 1 && !m.obs_dernier.as_ref().is_some_and(|d| systeme::meme(d, &i.dossier)) {
                ctx.journal(&format!("Mode streameur : plusieurs OBS, celui qui est ouvert compte ({})", i.dossier.display()));
            }
            if !m.obs_dernier.as_ref().is_some_and(|d| systeme::meme(d, &i.dossier)) {
                m.obs_dernier = Some(i.dossier.clone());
                self.garder(ctx);
            }
            return Some((*i).clone());
        }
        let m = self.memoire(ctx);
        let souvenir = match &m.obs_dernier {
            Some(d) => candidats.iter().find(|i| systeme::meme(d, &i.dossier)),
            None => m.obs_vus.iter().rev().find_map(|d| candidats.iter().find(|i| systeme::meme(d, &i.dossier))),
        };
        souvenir.or(candidats.first()).map(|i| (*i).clone())
    }

    /// La carte de la page Performances (au bloc perf), si elle change
    fn signaler_carte(&mut self) {
        let dispo = !self.trouves.is_empty();
        let regle = self.memoire.as_ref().is_some_and(|m| !m.changements.is_empty());
        if self.carte_envoyee != Some((dispo, regle)) {
            self.carte_envoyee = Some((dispo, regle));
            self.en_attente.push(Effet::Signal {
                nom: "streameur".into(),
                donnees: json!({ "disponible": dispo, "regle": regle, "carte": pages::carte_perf(dispo, regle) }),
            });
            self.en_attente.push(Effet::Rafraichir);
        }
    }

    fn plateforme(&mut self, ctx: &Contexte, t: &Trouve) -> (Plateforme, Option<Plateforme>, bool) {
        let lue = t.etat.as_ref().ok().and_then(|e| e.service.as_ref()).and_then(|s| s.plateforme());
        let choisie = self.memoire(ctx).plateformes.get(&t.installation.logiciel).copied();
        match choisie {
            Some(p) => (p, lue, true),
            None => (lue.unwrap_or(Plateforme::Twitch), lue, false),
        }
    }

    fn plan(&mut self, ctx: &Contexte, t: &Trouve) -> Option<Plan> {
        let (plateforme, _, _) = self.plateforme(ctx, t);
        let envoi = self.memoire(ctx).envoi.as_ref()?.0;
        let etat = t.etat.as_ref().ok();
        Some(calculer(&Entree {
            envoi_kbps: envoi,
            cartes: self.cartes.clone().unwrap_or_default(),
            base: reglages::base(etat, self.ecran),
            plateforme,
            version: etat
                .and_then(|e| e.version)
                .or_else(|| t.installation.version.as_deref().and_then(calcul::Version::lire)),
        }))
    }

    fn vue(&mut self, ctx: &Contexte) -> Vue {
        let mut logiciels = Vec::new();
        for t in self.trouves.clone() {
            let l = t.installation.logiciel;
            let (plateforme, lue, choisie) = self.plateforme(ctx, &t);
            let plan = self.plan(ctx, &t);
            let copie = Self::copie(ctx, l);
            let copie_date = copie.existe().then(|| copie.date().unwrap_or_default());
            let changement = self.memoire(ctx).changements.get(&l).cloned();
            logiciels.push(VueLogiciel {
                logiciel: l,
                etat: t.etat,
                ouvert: t.ouvert,
                plateforme,
                plateforme_lue: lue,
                plateforme_choisie: choisie,
                plan,
                copie: copie_date,
                changement,
                alerte: self.alertes.get(&l).cloned(),
            });
        }
        let envoi = self.memoire(ctx).envoi.clone();
        Vue {
            logiciels,
            envoi,
            mesure_en_cours: self.mesure.is_some(),
            cartes: self.cartes.clone().unwrap_or_default(),
            ecran: self.ecran,
        }
    }

    fn demander_mesure(&mut self) -> Effet {
        self.mesure = Some(Instant::now());
        Effet::Signal { nom: "test-debit".into(), donnees: json!({}) }
    }

    /// Le logiciel est-il ouvert, maintenant (relu au moment du clic)
    fn ouvert_maintenant(&mut self, ctx: &Contexte, l: Logiciel) -> bool {
        let sys = self.sys(ctx);
        systeme::ouvert(l, &sys.processus(), &sys.installations())
    }

    fn optimiser(&mut self, ctx: &Contexte, l: Logiciel, effets: &mut Effets) {
        self.detecter(ctx);
        let Some(t) = self.trouves.iter().find(|t| t.installation.logiciel == l).cloned() else {
            effets.ajouter(Effet::Annonce(pages::SANS_LOGICIEL.into()));
            return;
        };
        if self.ouvert_maintenant(ctx, l) {
            let m = pages::ferme_puis(l, "Optimiser");
            ctx.journal(&format!("Mode streameur : {} est ouvert, rien n'est changé", l.nom()));
            self.alertes.insert(l, m.clone());
            effets.ajouter(Effet::Annonce(m));
            return;
        }
        let etat = match &t.etat {
            Ok(e) => e.clone(),
            Err(_) => {
                effets
                    .ajouter(Effet::Annonce(format!("Les réglages {} ne se lisent pas : rien n'est changé.", l.de())));
                return;
            }
        };
        let Some(plan) = self.plan(ctx, &t) else {
            if self.mesure.is_none() {
                let e = self.demander_mesure();
                effets.ajouter(e);
            }
            effets
                .ajouter(Effet::Annonce("Nevermind mesure d'abord ta connexion, puis reclique sur Optimiser.".into()));
            return;
        };
        let date = nevermind_noyau::moment::date_historique(&ctx.maintenant());
        match reglages::appliquer(&etat, &plan, &Self::copie(ctx, l), &date) {
            Ok(n) => {
                let lignes = reglages::changements(&etat, &plan, &n);
                ctx.journal(&format!(
                    "Mode streameur : {} réglé (profil « {} ») : {}x{} à {} i/s, {} kb/s, encodeur {}, {}",
                    l.nom(),
                    etat.profil,
                    plan.sortie.0,
                    plan.sortie.1,
                    plan.ips,
                    plan.video_kbps,
                    plan.encodeur.simple(),
                    plan.plateforme.nom()
                ));
                let phrase = match l {
                    Logiciel::Obs => plan.phrase(),
                    Logiciel::Streamlabs => format!("Encodé par {}, {}", plan.encodeur.par(), plan.debit()),
                };
                self.memoire(ctx).changements.insert(l, Changement { date, phrase, lignes });
                self.garder(ctx);
                self.alertes.remove(&l);
                effets.ajouter(Effet::Annonce(pages::annonce_reglee(l, &plan)));
            }
            Err(e) => {
                ctx.noter_erreur(&format!("Mode streameur : {} pas réglé : {e}", l.nom()));
                let m = format!("Nevermind n'a pas pu changer les réglages {} : rien n'a été changé.", l.de());
                self.alertes.insert(l, m.clone());
                effets.ajouter(Effet::Annonce(m));
            }
        }
        self.detecter(ctx);
        effets.ajouter(Effet::Rafraichir);
    }

    fn remettre(&mut self, ctx: &Contexte, l: Logiciel, effets: &mut Effets) {
        if self.ouvert_maintenant(ctx, l) {
            let m = pages::ferme_puis(l, "Remettre comme avant");
            self.alertes.insert(l, m.clone());
            effets.ajouter(Effet::Annonce(m));
            return;
        }
        match Self::copie(ctx, l).remettre() {
            Ok(b) => {
                ctx.journal(&format!(
                    "Mode streameur : réglages d'avant remis dans {} ({} fichier(s) remis, {} disparu(s), {} copie(s) abîmée(s), {} réglage(s) changé(s) depuis gardé(s))",
                    l.nom(),
                    b.remis,
                    b.disparus,
                    b.abimes,
                    b.gardes
                ));
                // (la copie est effacée dans tous ces cas : réessayer ne changerait rien)
                self.memoire(ctx).changements.remove(&l);
                self.garder(ctx);
                self.alertes.remove(&l);
                effets.ajouter(Effet::Annonce(pages::annonce_remise(l, &b)));
            }
            Err(e) => {
                ctx.noter_erreur(&format!("Mode streameur : réglages d'avant pas remis dans {} : {e}", l.nom()));
                let m = format!(
                    "Nevermind n'a pas pu remettre les réglages d'avant dans {}. Réessaie dans un instant.",
                    l.court()
                );
                self.alertes.insert(l, m.clone());
                effets.ajouter(Effet::Annonce(m));
            }
        }
        self.detecter(ctx);
        effets.ajouter(Effet::Rafraichir);
    }
}

impl Bloc for BlocStream {
    fn nom(&self) -> &'static str {
        "stream"
    }

    fn pages(&mut self, ctx: &Contexte, ecran: &str) -> Map<String, Value> {
        // (la détection est refaite à chaque ouverture de la page, et de Performances où est sa carte)
        if !self.detecte || (ecran != self.dernier_ecran && (ecran == "streameur" || ecran == "perf")) {
            self.detecter(ctx);
        }
        if ecran != self.dernier_ecran && ecran == "streameur" {
            self.alertes.clear();
        }
        self.dernier_ecran = ecran.to_string();
        // Jamais mesuré : la page lance la mesure toute seule (une fois)
        if ecran == "streameur"
            && !self.trouves.is_empty()
            && !self.mesure_auto
            && self.mesure.is_none()
            && self.memoire(ctx).envoi.is_none()
        {
            self.mesure_auto = true;
            let e = self.demander_mesure();
            self.en_attente.push(e);
        }
        let v = self.vue(ctx);
        let mut m = Map::new();
        m.insert("streameur".into(), pages::page(&v));
        m
    }

    fn action(&mut self, ctx: &Contexte, id: &str, valeur: &Value, effets: &mut Effets) -> bool {
        let Some(reste) = id.strip_prefix("stream:") else { return false };
        let (geste, logiciel) = reste.split_once(':').unwrap_or((reste, ""));
        let l = Logiciel::depuis_id(logiciel);
        match (geste, l) {
            ("optimiser", Some(l)) => self.optimiser(ctx, l, effets),
            ("remettre", Some(l)) => self.remettre(ctx, l, effets),
            ("plateforme", Some(l)) => {
                if let Some(p) = valeur.as_str().and_then(Plateforme::depuis_id) {
                    self.memoire(ctx).plateformes.insert(l, p);
                    self.garder(ctx);
                    effets.ajouter(Effet::Rafraichir);
                }
            }
            ("mesurer", _) => {
                if self.mesure.is_none() {
                    let e = self.demander_mesure();
                    effets.ajouter(e);
                }
                effets.ajouter(Effet::Rafraichir);
            }
            _ => return false,
        }
        self.signaler_carte();
        effets.0.append(&mut self.en_attente);
        true
    }

    fn tic(&mut self, ctx: &Contexte, cache: bool, effets: &mut Effets) -> Option<Duration> {
        if !self.detecte {
            self.detecter(ctx);
        }
        if self.mesure.is_some_and(|t| t.elapsed() > MESURE_MAX) {
            self.mesure = None;
            effets.ajouter(Effet::Rafraichir);
        }
        // La page regardée : le logiciel ouvert ou fermé, et ses réglages, sont suivis
        if !cache
            && !self.en_partie
            && self.dernier_ecran == "streameur"
            && self.suivi.is_none_or(|t| t.elapsed() >= SUIVI)
        {
            self.suivi = Some(Instant::now());
            let avant: Vec<(bool, bool)> = self.trouves.iter().map(|t| (t.ouvert, t.etat.is_ok())).collect();
            self.detecter(ctx);
            let apres: Vec<(bool, bool)> = self.trouves.iter().map(|t| (t.ouvert, t.etat.is_ok())).collect();
            if avant != apres {
                effets.ajouter(Effet::Rafraichir);
            }
        } else if !cache
            && !self.en_partie
            && self.dernier_ecran == "perf"
            && self.trouves.is_empty()
            && self.suivi.is_none_or(|t| t.elapsed() >= SUIVI)
        {
            // La carte grisée de Performances : un OBS portable qui s'ouvre la dégrise (seuls les processus sont
            // relus, la détection entière seulement s'il y a un nouvel OBS ouvert)
            self.suivi = Some(Instant::now());
            let ouverts = systeme::obs_ouverts_inconnus(&self.sys(ctx).processus(), &[]);
            if ouverts != self.obs_ouverts {
                self.detecter(ctx);
                effets.ajouter(Effet::Rafraichir);
            }
        }
        effets.0.append(&mut self.en_attente);
        Some(if self.dernier_ecran == "streameur" && !cache {
            Duration::from_millis(1000)
        } else {
            Duration::from_secs(5)
        })
    }

    fn signal(&mut self, ctx: &Contexte, nom: &str, donnees: &Value, effets: &mut Effets) {
        match nom {
            "debit-mesure" => self.debit_mesure(ctx, donnees, effets),
            // (pendant une partie, rien n'est relu)
            "partie-commencee" => self.en_partie = true,
            "partie-finie" => self.en_partie = false,
            _ => {}
        }
    }
}

impl BlocStream {
    /// Le test « Connexion Internet » est fini (bloc pc) : son envoi (R[2], en Mb/s) est gardé
    fn debit_mesure(&mut self, ctx: &Contexte, donnees: &Value, effets: &mut Effets) {
        self.mesure = None;
        let envoi = donnees["R"].as_array().and_then(|r| r.get(2)).and_then(Value::as_f64).filter(|u| *u > 0.0);
        if let Some(u) = envoi {
            let kbps = (u * 1000.0).round().min(f64::from(u32::MAX)) as u32;
            let date = nevermind_noyau::moment::date_historique(&ctx.maintenant());
            ctx.journal(&format!("Mode streameur : envoi mesuré {kbps} kb/s"));
            self.memoire(ctx).envoi = Some((kbps, date));
            self.garder(ctx);
        }
        effets.ajouter(Effet::Rafraichir);
    }
}
