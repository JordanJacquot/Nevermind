//! Demander à l'Aide ce que Nevermind ne peut pas faire sans les droits administrateur (modules\aide-client.ps1).
//!
//! L'Aide (NevermindAide.exe, rust\nevermind-aide) est lancée par la tâche planifiée « \Mindset\Nevermind (aide) »,
//! que Mindset crée quand il installe Nevermind : pas de demande de Windows. Elle ne fait que ce que sa liste stricte
//! permet. Nevermind ne s'en sert que si c'est bien elle (tâche SYSTEM pour cet utilisateur : aide_regles::tache) ;
//! sinon, ou si elle ne répond pas, une action demandée par l'utilisateur passe par la demande normale de Windows (il
//! clique « Oui »). Nevermind ne fait JAMAIS lui-même les actions de l'Aide.
//!
//! - [`Contexte::aide`] / [`Contexte::aide_sans_demande`] : `Invoke-Aide` (et `-SansDemande`) ;
//! - [`Contexte::aide_fond`] : `Start-AideFond` (réveil au lancement, seulement par sa tâche) ;
//! - [`Contexte::aide_lecture`] : `Get-AideLecture` (lectures réservées aux administrateurs) ;
//! - [`Contexte::mesures_aide`] : `Test-MesuresAide` (mesures en continu par l'Aide) ;
//! - [`Contexte::aide_texte`] : `AideClient.Texte` (une réponse texte, depuis n'importe quel fil) ;
//! - [`variables_dangereuses`] : `Get-VariablesDangereuses` ;
//! - [`etat_tache`] / [`lire_tache`] : `Get-AideEtatTache` / `Read-AideTache`.
//!
//! Mode test (`Contexte::essai`) : jamais de tâche ni de demande de Windows (`$AideSansTache`, `$AideSansDemande`),
//! et le canal est celui de la copie d'essai, que la vraie Aide n'écoute jamais. Une fausse Aide peut être donnée au
//! contexte ([`Contexte::fausse_aide`]) ou lancée sur ce canal ([`crate::fausse_aide`]).
//!
//! Pas porté ici : la voie « sur place » de la désinstallation (Invoke-AideAction dans le même processus, en
//! administrateur), qui appartient à la désinstallation.

use crate::Contexte;
use crate::json_ps;
use serde_json::{Map, Value, json};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// CREATE_NO_WINDOW : un programme lancé par Nevermind n'ouvre jamais de fenêtre
pub const SANS_FENETRE: u32 = 0x0800_0000;

/// La tâche de l'Aide
pub const TACHE: &str = "\\Mindset\\Nevermind (aide)";
/// Demande de Windows refusée : pas de nouvelle demande avant 10 minutes
pub const DELAI_REFUS: Duration = Duration::from_secs(600);
/// Attente de l'Aide lancée par sa tâche
pub const ATTENTE_TACHE: Duration = Duration::from_secs(20);
/// Attente de l'Aide lancée par la demande de Windows (une fois « Oui » cliqué)
pub const ATTENTE_WINDOWS: Duration = Duration::from_secs(30);
/// Une demande de fond ([`Contexte::aide_de_fond`]) n'a pas eu de réponse : pas de nouvel essai avant ce délai (sauf
/// si le réveil de l'Aide réussit avant)
pub const PAUSE_FOND: Duration = Duration::from_secs(60);
/// Ce que rend une demande de fond pendant la pause qui suit un échec
pub const ERREUR_PAUSE: &str = "l'Aide ne répondait pas, nouvel essai dans un moment";

/// Les actions de l'Aide qui changent le PC : seulement sous le verrou d'un geste (lot A du second audit). Le point
/// de restauration (`restauration.creer`) n'y est pas : il ne change ni la sauvegarde ni un réglage, et « Appliquer »
/// rend le verrou pendant qu'il se crée (ajustement B).
pub const ACTIONS_QUI_CHANGENT: &[&str] = &[
    "reg.ecrire",
    "reg.effacer",
    "service.regler",
    "dns.regler",
    "carte.relancer",
    "pare-feu.bloquer",
    "pare-feu.debloquer",
    "pare-feu.debloquer-tout",
    "pare-feu.activer",
    "origines.oublier",
    "antivirus.retirer-exclusions",
    "wifi.manuel",
    "smb1.couper",
    "nettoyage.lancer",
    "processus.fermer",
];

/// Cette action de l'Aide change-t-elle le PC ?
pub fn change_le_pc(action: &str) -> bool {
    ACTIONS_QUI_CHANGENT.contains(&action)
}

/// Une fausse Aide donnée au contexte (tests des blocs) : l'action et ses paramètres, la réponse (`r`) ou l'erreur.
pub type FausseAide = Arc<dyn Fn(&str, &Value) -> Result<Value, String> + Send + Sync>;

/// En test seulement : la tâche de l'Aide simulée (`schtasks /Run`), pour essayer le vrai réveil de [`Contexte`] sans
/// jamais toucher aux tâches du PC. Rend vrai si la tâche « a démarré ».
pub type FausseTache = Arc<dyn Fn() -> bool + Send + Sync>;

/// Ce qu'il faut pour joindre l'Aide de cette copie
#[derive(Clone, Debug)]
pub struct Config {
    /// NevermindAide.exe de cette copie (seul programme écouté)
    pub exe: String,
    /// Le compte de l'utilisateur
    pub sid: String,
    /// Le nom du canal (« Nevermind-Aide-<sid>-<suffixe> »)
    pub canal: String,
    /// schtasks.exe de Windows
    pub schtasks: String,
    /// Nevermind tourne en administrateur
    pub admin: bool,
}

#[derive(Default)]
struct Etat {
    sans_tache: bool,
    sans_demande: bool,
    refus: Option<Instant>,
    voie: Option<String>,
    mesures_echec: Option<Instant>,
    /// La dernière demande de fond restée sans réponse ([`PAUSE_FOND`])
    fond_echec: Option<Instant>,
}

/// L'état de l'Aide pour ce contexte
#[derive(Default)]
pub struct EtatAide {
    config: OnceLock<Config>,
    etat: Mutex<Etat>,
    reveil: AtomicBool,
    /// Un réveil a été demandé pendant qu'un autre, qui va peut-être échouer, était en cours : il est refait
    reveil_redemande: AtomicBool,
    pub(crate) fausse: Mutex<Option<FausseAide>>,
    /// En test : les actions demandées par `aide` (celles qui peuvent proposer la demande de Windows)
    pub(crate) avec_demande: Mutex<Vec<String>>,
    pub(crate) exe_attendu: Mutex<Option<String>>,
    pub(crate) attentes: Mutex<Option<(Duration, Duration)>>,
    /// En test seulement : la tâche de l'Aide simulée ([`Contexte::tache_aide_essai`])
    pub(crate) tache_essai: Mutex<Option<FausseTache>>,
    /// En test seulement : Nevermind tourne-t-il en administrateur ? ([`Contexte::regler_admin_essai`])
    pub(crate) admin_essai: Mutex<Option<bool>>,
    /// En test seulement : la pause après une demande restée sans réponse ([`Contexte::regler_pause_essai`])
    pub(crate) pause_essai: Mutex<Option<Duration>>,
}

fn verrou<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Les variables que .NET lit au démarrage d'un programme (aide_regles::variables), présentes dans l'environnement de
/// Nevermind ou du compte (HKCU\Environment), triées sans doublon (`Get-VariablesDangereuses`).
pub fn variables_dangereuses() -> Vec<String> {
    let mut noms: Vec<String> = std::env::vars_os().map(|(k, _)| k.to_string_lossy().into_owned()).collect();
    noms.extend(crate::registre::noms_valeurs("HKCU:\\Environment"));
    variables_dangereuses_de(&noms)
}

/// Le tri de `Get-VariablesDangereuses` sur une liste de noms (sans tenir compte des majuscules, sans doublon)
pub fn variables_dangereuses_de(noms: &[String]) -> Vec<String> {
    let mut d: Vec<String> = noms.iter().filter(|n| aide_regles::variables::dangereuse(n)).cloned().collect();
    d.sort_by_key(|a| a.to_lowercase());
    // (comme Sort-Object -Unique : de deux noms égaux sans les majuscules, le dernier reste)
    let mut r: Vec<String> = Vec::new();
    for x in d {
        match r.last_mut() {
            Some(l) if l.eq_ignore_ascii_case(&x) => *l = x,
            _ => r.push(x),
        }
    }
    r
}

/// `Get-AideEtatTache` : « systeme » (la bonne), « autre », ou « » (pas de tâche)
pub fn etat_tache(xml: &str, exe: &str, sid: &str) -> &'static str {
    aide_regles::tache::etat(xml, exe, sid, &crate::windows::traduire_compte)
}

/// `Read-AideTache` : le XML de la tâche (`schtasks /Query /TN <tâche> /XML`), vide si elle n'existe pas
pub fn lire_tache(tache: &str, schtasks: &str) -> String {
    let Ok(s) = std::process::Command::new(schtasks)
        .args(["/Query", "/TN", tache, "/XML"])
        .creation_flags(SANS_FENETRE)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
    else {
        return String::new();
    };
    if !s.status.success() || s.stdout.is_empty() {
        return String::new();
    }
    let t = crate::windows::texte_oem(&s.stdout);
    crate::journal::lignes(&t).join("\n")
}

/// `schtasks /Run /TN <tâche>` : vrai si Windows l'a lancée
fn lancer_tache(tache: &str, schtasks: &str) -> bool {
    std::process::Command::new(schtasks)
        .args(["/Run", "/TN", tache])
        .creation_flags(SANS_FENETRE)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// La demande envoyée sur le canal : `{"a":"<action>","p":<paramètres>}` (comme `ConvertTo-Json -Compress`)
pub fn demande(action: &str, parametres: &Value) -> String {
    let mut m = Map::new();
    m.insert("a".into(), json!(action));
    m.insert("p".into(), parametres.clone());
    json_ps::compact(&Value::Object(m))
}

/// Ce qu'a donné le travail (`$AideTravail`)
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Travail {
    pub reponse: Option<String>,
    pub voie: String,
    pub erreur: String,
    /// L'état de la tâche, s'il a été lu
    pub etat: Option<String>,
    pub refus: bool,
}

/// La ligne du journal quand la voie change (`$script:AideVoie`)
pub fn ligne_voie(voie: &str, erreur: &str) -> String {
    match voie {
        "aide" => format!(
            "Aide : les actions administrateur passent par l'Aide{}",
            if erreur.is_empty() { String::new() } else { format!(" ({erreur}, demande de Windows acceptée)") }
        ),
        "local" => format!("Aide : Nevermind fait lui-même les actions administrateur ({erreur})"),
        _ => format!("Aide : indisponible ({erreur})"),
    }
}

impl Contexte {
    /// La configuration de l'Aide de cette copie (lue une fois)
    pub fn aide_config(&self) -> &Config {
        self.aide.config.get_or_init(|| {
            let racine = self.racine();
            let exe = verrou(&self.aide.exe_attendu)
                .clone()
                .unwrap_or_else(|| racine.join("NevermindAide.exe").to_string_lossy().into_owned());
            let sid = crate::windows::sid_utilisateur();
            let suffixe = match aide_regles::canal::suffixe(&racine.to_string_lossy()) {
                Some(s) => s,
                None => {
                    self.journal(
                        "Aide : nom du canal inconnu (dossier de l'appli non valable), l'Aide ne sera pas jointe",
                    );
                    "aucune".to_string()
                }
            };
            let schtasks = PathBuf::from(crate::windows::dossier_systeme()).join("schtasks.exe");
            Config {
                exe,
                canal: aide_regles::canal::nom_canal(&sid, &suffixe),
                sid,
                schtasks: schtasks.to_string_lossy().into_owned(),
                admin: match *verrou(&self.aide.admin_essai) {
                    Some(a) if self.essai => a,
                    _ => crate::windows::est_admin(),
                },
            }
        })
    }

    /// Une fausse Aide est-elle donnée (tests, construction d'essai) ?
    pub fn a_fausse_aide(&self) -> bool {
        verrou(&self.aide.fausse).is_some()
    }

    /// Une fausse Aide pour les tests : toutes les demandes lui vont (jamais au canal), rien n'est lancé.
    pub fn fausse_aide(&self, f: FausseAide) {
        *verrou(&self.aide.fausse) = Some(f);
    }

    /// Tests : les actions demandées par [`Contexte::aide`], la seule voie qui peut proposer la demande de Windows
    /// (relevées en mode test ou avec une fausse Aide). Une lecture, une mesure ou un geste de fond (minuterie,
    /// lancement d'un jeu, fin d'une lecture) ne doit jamais y être : seulement la réponse à un clic.
    pub fn aide_demandes_relevees(&self) -> Vec<String> {
        verrou(&self.aide.avec_demande).clone()
    }

    /// Tests : le programme attendu au bout du canal (le test lui-même, pour une fausse Aide sur le vrai canal), et
    /// des attentes plus courtes (tâche, demande de Windows). À appeler avant la première demande.
    pub fn regler_aide_essai(&self, exe_attendu: Option<String>, attentes: Option<(Duration, Duration)>) {
        *verrou(&self.aide.exe_attendu) = exe_attendu;
        *verrou(&self.aide.attentes) = attentes;
    }

    /// Tests : Nevermind tourne-t-il en administrateur ? Sans ce réglage, c'est le vrai compte du test : en
    /// administrateur sur les machines de GitHub, sans droits chez Jordan ; le réveil des mesures n'est pas le même
    /// (`mesures_aide`). À appeler avant la première demande. Refusé hors du mode test.
    pub fn regler_admin_essai(&self, admin: bool) -> Result<(), String> {
        if !self.essai {
            return Err("le compte d'essai seulement en mode test".into());
        }
        *verrou(&self.aide.admin_essai) = Some(admin);
        Ok(())
    }

    /// Tests : une pause plus courte que [`PAUSE_FOND`] après une demande restée sans réponse. Refusé hors du mode
    /// test.
    pub fn regler_pause_essai(&self, pause: Duration) -> Result<(), String> {
        if !self.essai {
            return Err("une autre pause seulement en mode test".into());
        }
        *verrou(&self.aide.pause_essai) = Some(pause);
        Ok(())
    }

    fn pause_fond(&self) -> Duration {
        if self.essai { verrou(&self.aide.pause_essai).unwrap_or(PAUSE_FOND) } else { PAUSE_FOND }
    }

    /// La dernière demande à l'Aide est restée sans réponse : les demandes de fond et les mesures renoncent tout de
    /// suite pendant la pause. La pause finie, avec le contexte partagé, l'Aide est d'abord SONDÉE dans un fil à part
    /// (le réveil, dont la réussite lève la pause) et la demande renonce encore : la pause repart (lot F ; avant, la
    /// première demande après la pause était faite sur le fil appelant, et une Aide qui accepte la connexion sans
    /// jamais répondre coûtait son attente entière, 15 s, sur le fil des blocs à chaque minute). Sans contexte partagé
    /// (tests d'un bloc seul, juges), la demande part de nouveau, comme avant.
    fn en_pause_fond(&self, partage: Option<&Arc<Contexte>>) -> bool {
        let mut e = verrou(&self.aide.etat);
        let Some(t) = e.fond_echec else { return false };
        if t.elapsed() < self.pause_fond() {
            return true;
        }
        let Some(p) = partage else { return false };
        e.fond_echec = Some(Instant::now());
        drop(e);
        p.reveiller_en_fond();
        true
    }

    /// Une demande à l'Aide est restée sans réponse : la pause, et le réveil (la sonde) dans un fil à part
    fn noter_sans_reponse(&self, partage: Option<&Arc<Contexte>>) {
        verrou(&self.aide.etat).fond_echec = Some(Instant::now());
        if let Some(p) = partage {
            p.reveiller_en_fond();
        }
    }

    /// L'Aide écoute et a répondu à la dernière demande (ou la sonde l'a fait) : une mesure peut lui être demandée
    /// sans risquer une longue attente sur le fil des blocs (lot F : une Aide qui accepte la connexion sans répondre
    /// est « joignable », mais pas prête). Pendant la pause, la sonde part comme pour [`Contexte::aide_de_fond`].
    pub fn aide_prete(&self) -> bool {
        if !self.aide_joignable(1) {
            return false;
        }
        match self.partage_actuel() {
            Some(p) => !self.en_pause_fond(Some(&p)),
            None => true,
        }
    }

    /// Tests : la tâche de l'Aide simulée. Le réveil de [`Contexte`] (`travail`) la « lance » au lieu de `schtasks`,
    /// puis attend l'Aide comme en vrai. Refusée hors du mode test : jamais d'effet sur la vraie tâche.
    pub fn tache_aide_essai(&self, f: FausseTache) -> Result<(), String> {
        if !self.essai {
            return Err("une fausse tâche de l'Aide seulement en mode test".into());
        }
        *verrou(&self.aide.tache_essai) = Some(f);
        Ok(())
    }

    /// `[AideClient]::Joignable(ms)` : l'Aide écoute-t-elle ?
    pub fn aide_joignable(&self, ms: u32) -> bool {
        if verrou(&self.aide.fausse).is_some() {
            return true;
        }
        crate::windows::canal::joignable(&self.aide_config().canal, ms)
    }

    fn sans_tache(&self) -> bool {
        self.essai || verrou(&self.aide.etat).sans_tache
    }

    fn sans_demande_globale(&self) -> bool {
        self.essai || verrou(&self.aide.etat).sans_demande
    }

    /// `$AideTravail` (hors désinstallation) : réveille l'Aide si besoin (sa tâche SYSTEM, sinon la demande de
    /// Windows si elle est permise), envoie la demande, rapporte la réponse. `reveiller` faux : jamais de réveil ici
    /// (ni tâche ni attente : une demande de fond, lot B), seulement l'Aide qui écoute déjà.
    fn travail(&self, texte_demande: &str, delai: Duration, demander: bool, reveiller: bool) -> Travail {
        let c = self.aide_config().clone();
        let (attente_tache, attente_windows) = verrou(&self.aide.attentes).unwrap_or((ATTENTE_TACHE, ATTENTE_WINDOWS));
        let joignable = |ms: u32| crate::windows::canal::joignable(&c.canal, ms);
        let attendre = |d: Duration| {
            let fin = Instant::now() + d;
            while Instant::now() < fin && !joignable(200) {
                std::thread::sleep(Duration::from_millis(150));
            }
        };
        // (en test seulement : la tâche simulée, jamais schtasks)
        let tache_essai = if self.essai { verrou(&self.aide.tache_essai).clone() } else { None };
        let mut t = Travail::default();
        if reveiller && !joignable(200) && (tache_essai.is_some() || !self.sans_tache()) {
            let etat = match &tache_essai {
                Some(_) => "systeme",
                None => etat_tache(&lire_tache(TACHE, &c.schtasks), &c.exe, &c.sid),
            };
            t.etat = Some(etat.to_string());
            match etat {
                "systeme" => {
                    let lancee = match &tache_essai {
                        Some(f) => f(),
                        None => lancer_tache(TACHE, &c.schtasks),
                    };
                    if lancee {
                        attendre(attente_tache);
                        if !joignable(1) {
                            t.erreur = "la tâche de l'Aide ne répond pas".into();
                        }
                    } else {
                        t.erreur = "la tâche de l'Aide n'a pas démarré".into();
                    }
                }
                "autre" => t.erreur = "ancienne tâche de l'Aide (Mindset la remplacera)".into(),
                _ => t.erreur = "pas de tâche pour l'Aide".into(),
            }
        }
        // Repli : la demande normale de Windows (l'utilisateur clique « Oui »), jamais une autre tâche
        if reveiller && !joignable(1) && demander {
            match crate::windows::demande_windows(
                &c.exe,
                &format!("-Sid {}", c.sid),
                &crate::windows::dossier_systeme(),
            ) {
                Ok(()) => {
                    attendre(attente_windows);
                    if !joignable(1) {
                        t.erreur = "l'Aide lancée par la demande de Windows ne répond pas".into();
                    }
                }
                Err(_) => {
                    t.refus = true;
                    t.erreur = "demande de Windows refusée".into();
                }
            }
        }
        if joignable(200) {
            let ms = delai.as_millis().min(u128::from(u32::MAX)) as u32;
            match crate::windows::canal::appeler(&c.canal, &c.exe, texte_demande, 3000, ms) {
                Ok(r) => {
                    t.reponse = Some(r);
                    t.voie = "aide".into();
                }
                Err(e) => t.erreur = e,
            }
        } else if t.erreur.is_empty() {
            t.erreur = "l'Aide ne répond pas".into();
        }
        t
    }

    /// `Invoke-Aide` : fait faire une action de la liste de l'Aide et rend son résultat (`r`). Refusée ou ratée :
    /// l'erreur en français (le message de l'Aide, ou pourquoi elle n'a pas pu être jointe).
    pub(crate) fn invoquer_aide(
        &self,
        action: &str,
        parametres: &Value,
        delai: Duration,
        sans_demande: bool,
    ) -> Result<Value, String> {
        self.invoquer(action, parametres, delai, sans_demande, false)
    }

    /// `fond` : une demande de fond (lot B, [`Contexte::aide_de_fond`]) : jamais de réveil sur ce fil, une pause
    /// après un échec, le réveil fait dans un fil à part.
    fn invoquer(
        &self,
        action: &str,
        parametres: &Value,
        delai: Duration,
        sans_demande: bool,
        fond: bool,
    ) -> Result<Value, String> {
        // (une action qui change le PC seulement sous le verrou d'un geste, lot A ; le point de restauration n'en a
        //  pas besoin, ajustement B)
        if change_le_pc(action) {
            self.exiger_geste()?;
        }
        let fausse = verrou(&self.aide.fausse).clone();
        if !sans_demande && (self.essai || fausse.is_some()) {
            verrou(&self.aide.avec_demande).push(action.to_string());
        }
        if let Some(f) = fausse {
            return f(action, parametres);
        }
        // (une demande de fond restée sans réponse il y a peu : pas de nouvel essai, rien n'attend ; lot B)
        let partage = if fond { self.partage_actuel() } else { None };
        if fond && self.en_pause_fond(partage.as_ref()) {
            return Err(ERREUR_PAUSE.into());
        }
        let mut demander = !sans_demande && !self.sans_demande_globale();
        if demander
            && let Some(r) = verrou(&self.aide.etat).refus
            && r.elapsed() < DELAI_REFUS
        {
            demander = false;
        }
        // (une variable lue par .NET au démarrage : l'Aide lancée par la demande de Windows la recevrait avec les
        // droits administrateur. Seule sa tâche SYSTEM, qui ne reçoit rien du compte, reste possible.)
        let danger = if demander { variables_dangereuses() } else { Vec::new() };
        if !danger.is_empty() {
            demander = false;
            self.journal(&format!(
                "Aide : pas de demande de Windows, variables dangereuses dans l'environnement du compte ({})",
                danger.join(", ")
            ));
        }
        // (une demande de fond ne réveille jamais l'Aide sur ce fil : le réveil part dans un fil à part ; sans contexte
        //  partagé, seulement les tests d'un bloc seul et les juges, il est fait sur place comme avant)
        let o = self.travail(&demande(action, parametres), delai, demander, partage.is_none());
        if o.reponse.is_some() {
            // (l'Aide a répondu, à n'importe quelle demande : plus de pause ; lot F)
            verrou(&self.aide.etat).fond_echec = None;
        } else if fond {
            self.noter_sans_reponse(partage.as_ref());
        }
        let changement = {
            let mut e = verrou(&self.aide.etat);
            // (tâche absente ou pas la bonne : plus de schtasks jusqu'au prochain lancement ; la demande de Windows reste)
            if matches!(o.etat.as_deref(), Some("") | Some("autre")) {
                e.sans_tache = true;
            }
            if o.refus {
                e.refus = Some(Instant::now());
            }
            if e.voie.as_deref() != Some(o.voie.as_str()) {
                e.voie = Some(o.voie.clone());
                true
            } else {
                false
            }
        };
        if changement {
            self.journal(&ligne_voie(&o.voie, &o.erreur));
        }
        let Some(rep) = o.reponse else {
            self.journal(&format!("Aide : {action} impossible ({})", o.erreur));
            if o.refus {
                return Err("Nevermind a besoin de ton accord pour ça : réessaie, et clique « Oui » dans la demande de Windows.".into());
            }
            if let Some(d) = danger.first() {
                return Err(format!(
                    "Par prudence, Nevermind ne demande pas les droits administrateur : un réglage caché de ton compte Windows ({d}) pourrait faire lancer un autre programme avec ces droits. Lance une analyse antivirus, puis ouvre Mindset pour réparer Nevermind."
                ));
            }
            return Err(format!(
                "Nevermind a besoin de son Aide pour ça, mais elle ne répond pas ({}). Ouvre Mindset et mets Nevermind à jour.",
                o.erreur
            ));
        };
        let j: Value = serde_json::from_str(&rep).map_err(|e| format!("Réponse de l'Aide illisible ({e})"))?;
        if !vrai(&j["ok"]) {
            let e = texte_ps(&j["e"]);
            self.journal(&format!("Aide : {action} refusé ou raté : {e}"));
            return Err(e);
        }
        Ok(j.get("r").cloned().unwrap_or(Value::Null))
    }

    /// `Invoke-Aide -SansDemande` : jamais de demande de Windows (lectures et mesures en fond : seulement l'Aide
    /// déjà lancée ou sa tâche).
    pub fn aide_sans_demande(&self, action: &str, parametres: &Value, delai_ms: u64) -> Result<Value, String> {
        self.invoquer_aide(action, parametres, Duration::from_millis(delai_ms), true)
    }

    /// Une demande faite depuis un tic ou un geste automatique (lectures du trafic, mesure du lag, mode jeu au début
    /// d'une partie) : comme [`Contexte::aide_sans_demande`] (jamais de demande de Windows), mais elle n'attend
    /// JAMAIS le réveil de l'Aide sur le fil qui l'appelle (lot B du second audit : `schtasks /Run` puis jusqu'à 20 s
    /// d'attente figeaient le fil des blocs). L'Aide qui n'écoute pas est réveillée dans un fil à part ; après une
    /// demande restée sans réponse, les suivantes renoncent tout de suite pendant [`PAUSE_FOND`] (ou jusqu'au réveil
    /// réussi). Une Aide qui répond mais refuse ne compte pas comme un échec.
    pub fn aide_de_fond(&self, action: &str, parametres: &Value, delai_ms: u64) -> Result<Value, String> {
        self.invoquer(action, parametres, Duration::from_millis(delai_ms), true, true)
    }

    /// Le réveil de l'Aide dans un fil à part (sa tâche SYSTEM seulement, jamais de demande de Windows), un seul à la
    /// fois ; réussi, il lève la pause des demandes de fond et des mesures.
    fn reveiller_en_fond(self: &Arc<Self>) {
        // (un réveil déjà en cours : la demande est notée, pas perdue ; il sera refait s'il échoue)
        if self.aide.reveil.swap(true, Ordering::SeqCst) {
            self.aide.reveil_redemande.store(true, Ordering::SeqCst);
            return;
        }
        let ctx = self.clone();
        let lance = std::thread::Builder::new().name("nevermind-reveil-aide".into()).spawn(move || {
            loop {
                let ok = ctx.aide_sans_demande("aide.bonjour", &json!({}), 10000).is_ok();
                if ok {
                    ctx.aide.reveil_redemande.store(false, Ordering::SeqCst);
                    let mut e = verrou(&ctx.aide.etat);
                    e.fond_echec = None;
                    e.mesures_echec = None;
                }
                ctx.aide.reveil.store(false, Ordering::SeqCst);
                // (raté, et redemandé pendant ce temps : encore une fois, sauf si un autre réveil vient de partir)
                if ok
                    || !ctx.aide.reveil_redemande.swap(false, Ordering::SeqCst)
                    || ctx.aide.reveil.swap(true, Ordering::SeqCst)
                {
                    break;
                }
            }
        });
        if lance.is_err() {
            self.aide.reveil.store(false, Ordering::SeqCst);
        }
    }

    /// `Start-AideFond` : réveille l'Aide au lancement, en arrière-plan, seulement par sa tâche SYSTEM si c'est bien
    /// elle (jamais de demande de Windows au lancement). Rien en mode test.
    pub fn aide_fond(&self) {
        if self.sans_tache() || verrou(&self.aide.fausse).is_some() {
            return;
        }
        let c = self.aide_config().clone();
        std::thread::spawn(move || {
            if crate::windows::canal::joignable(&c.canal, 50) {
                return;
            }
            if etat_tache(&lire_tache(TACHE, &c.schtasks), &c.exe, &c.sid) != "systeme" {
                return;
            }
            let _ = lancer_tache(TACHE, &c.schtasks);
        });
    }

    /// `Get-AideLecture` : une lecture réservée aux administrateurs, par l'Aide (jamais de demande de Windows).
    /// En administrateur, seulement si l'Aide tourne déjà. `None` si elle n'a rien donné.
    pub fn aide_lecture(&self, action: &str, parametres: &Value) -> Option<Value> {
        if self.aide_config().admin && !self.aide_joignable(1) {
            return None;
        }
        self.aide_sans_demande(action, parametres, 60000).ok()
    }

    /// `Test-MesuresAide` : les mesures en continu (FPS, trafic, trace réseau) passent-elles par l'Aide ? Sans droits,
    /// l'Aide est réveillée au besoin (au plus une fois par minute après un échec ; dans un fil à part avec le contexte
    /// partagé : rend faux sans attendre) ; en administrateur, Nevermind ne l'attend pas.
    pub fn mesures_aide(&self) -> bool {
        if self.aide_joignable(1) {
            // (lot F : joignable mais sans réponse à la dernière demande, elle n'est pas prête : la sonde part en fond)
            return self.aide_prete();
        }
        if self.aide_config().admin {
            return false;
        }
        if verrou(&self.aide.etat).mesures_echec.is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
            return false;
        }
        // (lot B : le réveil n'est jamais attendu ici, sur le fil des blocs au début d'une partie ; il part dans un fil
        //  à part et la mesure reprend quand l'Aide écoute. Sans contexte partagé, seulement les tests d'un bloc seul
        //  et les juges : sur place, comme avant.)
        if let Some(p) = self.partage_actuel() {
            verrou(&self.aide.etat).mesures_echec = Some(Instant::now());
            p.reveiller_en_fond();
            return false;
        }
        // (un réveil déjà en cours : pas un deuxième schtasks ni une deuxième attente de 20 s)
        if self.aide.reveil.swap(true, Ordering::SeqCst) {
            return false;
        }
        let r = self.aide_sans_demande("aide.bonjour", &json!({}), 10000);
        self.aide.reveil.store(false, Ordering::SeqCst);
        let ok = r.is_ok();
        verrou(&self.aide.etat).mesures_echec = if ok { None } else { Some(Instant::now()) };
        ok
    }

    /// `[AideClient]::Texte` : une demande dont la réponse est un texte (les lignes de PresentMon relayées par
    /// l'Aide), depuis n'importe quel fil, sans réveil. `parametres_json` : les paramètres déjà en JSON.
    pub fn aide_texte(&self, action: &str, parametres_json: &str, delai_ms: u64) -> Result<String, String> {
        if change_le_pc(action) {
            self.exiger_geste()?;
        }
        let fausse = verrou(&self.aide.fausse).clone();
        if let Some(f) = fausse {
            let p: Value = serde_json::from_str(parametres_json).unwrap_or(Value::Null);
            return f(action, &p).map(|v| match v {
                Value::String(s) => s,
                Value::Null => String::new(),
                autre => autre.to_string(),
            });
        }
        let c = self.aide_config();
        // (lot B : une Aide arrêtée n'a plus de canal ; sans ce coup d'œil, la connexion l'attendait 2 s à chaque
        //  demande, et `fps.lire` en fait une toutes les 250 ms sur le fil des blocs)
        if !crate::windows::canal::joignable(&c.canal, 200) {
            return Err("l'Aide ne répond pas".into());
        }
        // (lot F : une Aide qui écoute mais n'a pas répondu à la dernière demande n'est pas attendue de nouveau tant
        //  que la sonde, dans un fil à part, n'a pas eu de réponse ; avec le contexte partagé seulement)
        let partage = self.partage_actuel();
        if partage.is_some() && self.en_pause_fond(partage.as_ref()) {
            return Err(ERREUR_PAUSE.into());
        }
        let rep = match crate::windows::canal::appeler(
            &c.canal,
            &c.exe,
            &format!("{{\"a\":\"{action}\",\"p\":{parametres_json}}}"),
            2000,
            delai_ms.min(u64::from(u32::MAX)) as u32,
        ) {
            Ok(r) => r,
            Err(e) => {
                if partage.is_some() {
                    self.noter_sans_reponse(partage.as_ref());
                }
                return Err(e);
            }
        };
        verrou(&self.aide.etat).fond_echec = None;
        reponse_texte(&rep)
    }
}

/// La réponse d'une demande texte (`AideClient.Texte`) : `r` si `ok` vaut vrai, sinon l'erreur de l'Aide (`e`), ou
/// « L'Aide a refusé. ».
pub fn reponse_texte(rep: &str) -> Result<String, String> {
    let j: Option<Value> = serde_json::from_str(rep).ok();
    let ok = j.as_ref().is_some_and(|j| j["ok"] == Value::Bool(true));
    if !ok {
        return Err(j.and_then(|j| j["e"].as_str().map(str::to_string)).unwrap_or_else(|| "L'Aide a refusé.".into()));
    }
    Ok(j.and_then(|j| j["r"].as_str().map(str::to_string)).unwrap_or_default())
}

/// Vrai au sens de PowerShell (`if ($x)`)
pub(crate) fn vrai(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(_) => true,
    }
}

/// `[string]$x` d'une valeur JSON
pub(crate) fn texte_ps(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Array(a) => a.iter().map(texte_ps).collect::<Vec<_>>().join(" "),
        autre => autre.to_string(),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// La mesure des FPS « en veille » (jeux dont l'anti-triche refuse une mesure démarrée après leur lancement)
// ---------------------------------------------------------------------------------------------------------------

/// Les jeux connus pour refuser une mesure démarrée après eux (`$FpsVeilleConnus`)
pub const FPS_VEILLE_CONNUS: [&str; 1] = ["bf6.exe"];

impl Contexte {
    /// `Get-FpsVeilleJeux` : les jeux guettés (connus, et ceux que Nevermind a vu refuser : réglage FpsVeille), en
    /// minuscules, sans doublon
    pub fn fps_veille_jeux(&self) -> Vec<String> {
        let mut l: Vec<String> = FPS_VEILLE_CONNUS.iter().map(|s| s.to_string()).collect();
        if let Some(Value::Array(a)) = self.reglage("FpsVeille") {
            l.extend(a.iter().map(texte_ps));
        } else if let Some(v) = self.reglage("FpsVeille") {
            l.push(texte_ps(&v));
        }
        let mut vu = Vec::new();
        for x in l.into_iter().filter(|x| !x.is_empty()).map(|x| x.to_lowercase()) {
            if !vu.contains(&x) {
                vu.push(x);
            }
        }
        vu
    }

    /// `Add-FpsVeilleJeu` : ce jeu refuse une mesure démarrée après lui ; elle sera lancée avant lui la prochaine fois
    pub fn ajouter_fps_veille(&self, exe: &str) {
        let mut e = exe.to_lowercase();
        if !e.ends_with(".exe") {
            e.push_str(".exe");
        }
        if self.fps_veille_jeux().contains(&e) {
            return;
        }
        let mut l: Vec<Value> = match self.reglage("FpsVeille") {
            Some(Value::Array(a)) => a,
            Some(Value::Null) | None => Vec::new(),
            Some(v) => vec![v],
        };
        l.push(json!(e));
        let _ = self.ecrire_reglage("FpsVeille", Value::Array(l), false);
        self.journal(&format!(
            "Mesure des FPS : {e} refuse une mesure démarrée après lui, elle sera lancée avant lui la prochaine fois"
        ));
    }
}

/// `Test-FpsRefus` : un refus de Windows (anti-triche), message de PresentMon ou de l'Aide
pub fn fps_refus(erreur: &str) -> bool {
    let e = erreur.to_lowercase();
    e.contains("access denied") || e.contains("accès refusé") || e.contains("performance log users")
}
