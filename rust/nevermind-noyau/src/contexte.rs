//! Le contexte donné à chaque bloc : les dossiers, le journal, les réglages (parametres.json), les fichiers de
//! données sûrs, le registre, la sauvegarde et l'historique partagés, l'Aide (modules\donnees.ps1, aide-client.ps1).
//!
//! Les signatures d'origine (`nouveau`, `journal`, `reglage`, `ecrire_reglage`, `aide`) ne changent pas ; tout le
//! reste est ajouté. Le contexte peut servir depuis plusieurs fils (tout ce qui change est derrière un verrou), et
//! aucun verrou n'est gardé pendant une écriture au journal (qui peut elle-même noter `LogError` dans les réglages).

use crate::fichiers::Fichiers;
use crate::moment::{self, HeureLocale, Horloge};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

pub(crate) fn verrou<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// La clé (HKCU) où Nevermind garde lui-même une copie des valeurs d'origine des réglages de l'utilisateur
pub const ORIGINES_UTILISATEUR: &str = "Software\\Mindset\\Nevermind\\Origines";
/// Les réglages de Nevermind qui gardent l'état d'origine du PC (services, alimentation) : écrits seulement par un
/// geste qui tient le verrou (lot A)
pub const REGLAGES_D_ORIGINE: &[&str] = &["SvcOriginal", "PcfgOriginal"];

/// La clé (HKLM) où l'Aide garde les valeurs d'origine des réglages du PC
pub const ORIGINES_MACHINE: &str = "SOFTWARE\\Mindset\\Nevermind\\Aide\\Origines";

pub(crate) struct Interne {
    pub fichiers: Fichiers,
    pub horloge: Horloge,
    pub log_error: Option<String>,
    pub racine: Option<PathBuf>,
    pub version: String,
    /// Mode test : le faux registre de l'utilisateur (« chemin|nom » : valeur, type), jamais le vrai
    pub faux_registre: HashMap<String, (Value, String)>,
    /// Mode test : les valeurs d'origine « gardées dans le registre »
    pub origines_factices: Vec<Value>,
    /// Les dernières erreurs (`$global:Error`), pour « Signaler un problème »
    pub erreurs: Vec<String>,
}

pub struct Contexte {
    /// Le dossier de l'appli (fichiers livrés : ui, fichiers de données).
    pub appli: PathBuf,
    /// Le dossier des données (`$DataDir`).
    pub donnees: PathBuf,
    /// Nevermind est géré par Mindset (`$Managed`).
    pub geree: bool,
    /// Mode test (`$env:NEVERMIND_TEST`) : rien ne touche le PC.
    pub essai: bool,
    reglages: Mutex<Option<Map<String, Value>>>,
    pub(crate) interne: Mutex<Interne>,
    pub(crate) aide: crate::aide::EtatAide,
    pub(crate) sauvegarde: Mutex<Option<crate::sauvegarde::Sauvegarde>>,
    pub(crate) historique: Mutex<Option<Vec<Value>>>,
    pub(crate) ignores: Mutex<Option<Vec<String>>>,
    pub(crate) assistance: Mutex<crate::assistance::EtatAssistance>,
    pub(crate) maj: Mutex<crate::mises_a_jour::EtatMaj>,
    /// Le verrou « un geste qui change le PC à la fois », avec le journal de chaque geste (lot A, G-A et M-C)
    pub(crate) gestes: std::sync::Arc<crate::gestes::Verrou>,
    /// Un seul écrivain de sauvegarde.json à la fois (lot A, étape 2)
    pub(crate) ecrivain_sauvegarde: Mutex<()>,
    /// Lui-même, une fois partagé ([`Contexte::partage`]) : ce que reçoit un travail de fond
    soi: std::sync::OnceLock<std::sync::Weak<Contexte>>,
    /// Un travail de fond a rendu quelque chose (à relever sur le fil des blocs)
    a_relever: std::sync::atomic::AtomicBool,
}

impl Contexte {
    /// Le contexte de ces dossiers. Le dossier des données est créé, et son journal raccourci s'il dépasse 1 Mo
    /// (comme au chargement de donnees.ps1).
    pub fn nouveau(appli: PathBuf, donnees: PathBuf, geree: bool, essai: bool) -> Self {
        let _ = std::fs::create_dir_all(&donnees);
        crate::journal::raccourcir(&donnees);
        let horloge = moment::horloge_du_pc();
        Self {
            appli,
            donnees,
            geree,
            essai,
            reglages: Mutex::new(None),
            interne: Mutex::new(Interne {
                fichiers: Fichiers::avec_horloge(horloge.clone()),
                horloge,
                log_error: None,
                racine: None,
                version: env!("CARGO_PKG_VERSION").to_string(),
                faux_registre: HashMap::new(),
                origines_factices: Vec::new(),
                erreurs: Vec::new(),
            }),
            aide: Default::default(),
            sauvegarde: Mutex::new(None),
            historique: Mutex::new(None),
            ignores: Mutex::new(None),
            assistance: Mutex::new(Default::default()),
            maj: Mutex::new(Default::default()),
            gestes: Default::default(),
            ecrivain_sauvegarde: Mutex::new(()),
            soi: Default::default(),
            a_relever: Default::default(),
        }
    }

    /// Le contexte partagé entre le fil des blocs et les travaux de fond (`crate::travaux`) : l'hôte le crée ainsi.
    /// Un contexte qui n'est pas partagé (les tests d'un bloc seul) fait ses travaux de fond sur place.
    pub fn partage(self) -> std::sync::Arc<Contexte> {
        let a = std::sync::Arc::new(self);
        let _ = a.soi.set(std::sync::Arc::downgrade(&a));
        a
    }

    /// Ce contexte partagé, s'il l'est
    pub fn partage_actuel(&self) -> Option<std::sync::Arc<Contexte>> {
        self.soi.get().and_then(std::sync::Weak::upgrade)
    }

    /// Un travail de fond a rendu quelque chose : l'hôte appellera `Bloc::travaux_finis`
    pub fn travail_a_relever(&self) {
        self.a_relever.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// L'hôte : un travail a-t-il rendu quelque chose depuis la dernière fois ? (remis à faux)
    pub fn travaux_a_relever(&self) -> bool {
        self.a_relever.swap(false, std::sync::atomic::Ordering::SeqCst)
    }

    // -----------------------------------------------------------------------------------------------------------
    // L'heure, la version, la racine
    // -----------------------------------------------------------------------------------------------------------

    /// L'heure locale (celle du PC, ou l'horloge figée des tests)
    pub fn maintenant(&self) -> HeureLocale {
        let h = verrou(&self.interne).horloge.clone();
        h()
    }

    /// Tests : une horloge figée (journal, fichiers mis de côté, historique)
    pub fn figer_horloge(&self, h: HeureLocale) {
        let mut i = verrou(&self.interne);
        i.horloge = moment::horloge_figee(h);
        i.fichiers = Fichiers::avec_horloge(i.horloge.clone());
    }

    /// La version de Nevermind (`$AppVersion`), donnée par l'hôte
    pub fn version(&self) -> String {
        verrou(&self.interne).version.clone()
    }

    /// L'hôte donne la version de Nevermind (`$AppVersion`)
    pub fn definir_version(&self, v: &str) {
        verrou(&self.interne).version = v.to_string();
    }

    /// Le dossier de cette copie de Nevermind (`Get-AppRoot`) : celui de NevermindAide.exe, du canal de l'Aide et
    /// des mises à jour. Par défaut, comme `Get-AppRoot` : le parent du dossier de l'appli s'il s'appelle
    /// « fichiers », sinon le dossier de l'appli lui-même.
    pub fn racine(&self) -> PathBuf {
        if let Some(r) = verrou(&self.interne).racine.clone() {
            return r;
        }
        let fichiers = self.appli.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("fichiers"));
        match self.appli.parent() {
            Some(p) if fichiers => p.to_path_buf(),
            _ => self.appli.clone(),
        }
    }

    /// L'hôte donne le dossier de cette copie (à faire avant la première demande à l'Aide)
    pub fn definir_racine(&self, p: PathBuf) {
        verrou(&self.interne).racine = Some(p);
    }

    // -----------------------------------------------------------------------------------------------------------
    // Le journal
    // -----------------------------------------------------------------------------------------------------------

    /// Le fichier du journal
    pub fn fichier_journal(&self) -> PathBuf {
        self.donnees.join("journal.txt")
    }

    /// Une ligne au journal (journal.txt, comme `Write-Log`). Journal inaccessible : la cause est gardée (une seule
    /// fois) dans les préférences (`LogError`), qui elles s'écrivent.
    pub fn journal(&self, message: &str) {
        let h = moment::iso(&self.maintenant());
        if let Err(e) = crate::journal::ajouter(&self.fichier_journal(), &h, message) {
            let texte = {
                let mut i = verrou(&self.interne);
                if i.log_error.is_some() {
                    return;
                }
                let t = format!("{h} {}: {e}", crate::journal::nom_erreur(&e));
                i.log_error = Some(t.clone());
                t
            };
            let _ = self.ecrire_reglage("LogError", Value::String(texte), false);
        }
    }

    /// Une erreur gardée pour « Signaler un problème » (`$global:Error`, les 80 dernières), et notée au journal
    pub fn noter_erreur(&self, message: &str) {
        {
            let mut i = verrou(&self.interne);
            i.erreurs.insert(0, message.to_string());
            i.erreurs.truncate(80);
        }
        self.journal(message);
    }

    /// Les erreurs gardées (la plus récente d'abord)
    pub fn erreurs(&self) -> Vec<String> {
        verrou(&self.interne).erreurs.clone()
    }

    // -----------------------------------------------------------------------------------------------------------
    // Les fichiers de données sûrs
    // -----------------------------------------------------------------------------------------------------------

    /// `Read-JsonSur` : le contenu d'un fichier de données (ou de sa copie de secours), `None` s'il n'y en a pas.
    /// Abîmé : mis de côté et noté au journal. Pris par un autre programme : il ne sera pas réécrit avant d'être relu.
    pub fn lire_json(&self, chemin: &Path) -> Option<Value> {
        let notes = std::cell::RefCell::new(Vec::new());
        let r = verrou(&self.interne).fichiers.lire_json(chemin, &|m: &str| notes.borrow_mut().push(m.to_string()));
        for n in notes.into_inner() {
            self.journal(&n);
        }
        r
    }

    /// `Save-JsonSur` (forme lisible ; `compact` pour `-Compress`, `bom` pour `-Bom`)
    pub fn ecrire_json(&self, chemin: &Path, valeur: &Value, compact: bool, bom: bool) -> Result<(), String> {
        verrou(&self.interne).fichiers.ecrire_json(chemin, valeur, !compact, bom)
    }

    // -----------------------------------------------------------------------------------------------------------
    // Les réglages (parametres.json)
    // -----------------------------------------------------------------------------------------------------------

    /// Le fichier des préférences
    pub fn fichier_reglages(&self) -> PathBuf {
        self.donnees.join("parametres.json")
    }

    fn charger_reglages(&self) {
        let mut r = verrou(&self.reglages);
        if r.is_some() {
            return;
        }
        let notes = std::cell::RefCell::new(Vec::new());
        let lu = verrou(&self.interne)
            .fichiers
            .lire_json(&self.fichier_reglages(), &|m: &str| notes.borrow_mut().push(m.to_string()));
        // (abîmé : sa copie de secours est reprise ; elle garde aussi SvcOriginal et PcfgOriginal, de quoi remettre
        // les services et l'alimentation comme avant)
        *r = Some(match lu {
            Some(Value::Object(m)) => m,
            _ => Map::new(),
        });
        drop(r);
        for n in notes.into_inner() {
            self.journal(&n);
        }
    }

    /// Un réglage (`Get-Setting`), `None` s'il n'existe pas.
    pub fn reglage(&self, nom: &str) -> Option<Value> {
        self.charger_reglages();
        verrou(&self.reglages).as_ref().and_then(|m| m.get(nom).cloned())
    }

    /// `Get-Setting <nom> <défaut>`
    pub fn reglage_ou(&self, nom: &str, defaut: Value) -> Value {
        self.reglage(nom).unwrap_or(defaut)
    }

    /// `[bool](Get-Setting <nom> <défaut>)` (vrai au sens de PowerShell)
    pub fn reglage_bool(&self, nom: &str, defaut: bool) -> bool {
        crate::aide::vrai(&self.reglage_ou(nom, Value::Bool(defaut)))
    }

    /// Tous les réglages (une copie)
    pub fn reglages(&self) -> Map<String, Value> {
        self.charger_reglages();
        verrou(&self.reglages).clone().unwrap_or_default()
    }

    /// Écrire un réglage (`Set-Setting`). `exiger` : erreur si le fichier n'a pas pu être écrit (rien n'est changé).
    pub fn ecrire_reglage(&self, nom: &str, valeur: Value, exiger: bool) -> Result<(), String> {
        // (l'état d'origine des services et de l'alimentation : noté seulement par un geste qui tient le verrou)
        if REGLAGES_D_ORIGINE.contains(&nom) {
            self.exiger_geste()?;
        }
        self.charger_reglages();
        let erreur = {
            let mut r = verrou(&self.reglages);
            let m = r.get_or_insert_with(Map::new);
            let avant = m.insert(nom.to_string(), valeur);
            let copie = Value::Object(m.clone());
            let ecrit = verrou(&self.interne).fichiers.ecrire_json(&self.fichier_reglages(), &copie, true, false);
            match ecrit {
                Ok(()) => None,
                Err(e) => {
                    if exiger {
                        match avant {
                            Some(v) => {
                                m.insert(nom.to_string(), v);
                            }
                            None => {
                                m.remove(nom);
                            }
                        }
                    }
                    Some(e)
                }
            }
        };
        if let Some(e) = erreur {
            self.journal(&format!("Écriture des préférences impossible: {e}"));
            if exiger {
                return Err("Nevermind n'a pas pu noter l'état d'origine (son fichier de préférences est pris par un autre programme) : rien n'a été changé. Réessaie dans un instant.".into());
            }
        }
        Ok(())
    }

    /// Les réglages sont relus du disque au prochain appel (après un changement fait par un autre programme)
    pub fn oublier_reglages(&self) {
        *verrou(&self.reglages) = None;
    }

    // -----------------------------------------------------------------------------------------------------------
    // L'Aide
    // -----------------------------------------------------------------------------------------------------------

    /// Une demande à l'Aide (`Invoke-Aide`) : sa réponse (`r`), ou l'erreur en français. `delai_ms` : l'attente de
    /// la réponse (120000 par défaut en PowerShell). Peut proposer la demande de Windows ; voir aussi
    /// `aide_sans_demande`.
    pub fn aide(&self, action: &str, parametres: &Value, delai_ms: u64) -> Result<Value, String> {
        self.invoquer_aide(action, parametres, Duration::from_millis(delai_ms), false)
    }

    // -----------------------------------------------------------------------------------------------------------
    // Le registre
    // -----------------------------------------------------------------------------------------------------------

    /// La clé où Nevermind garde les valeurs d'origine des réglages de l'utilisateur (une clé à part en test)
    pub fn cle_origines_utilisateur(&self) -> String {
        // (en test, toujours la même clé à part : aucune variable d'environnement n'est lue, l'exe livré n'en connaît
        //  aucune, outils\verifier-sans-essai.ps1 ; le faux registre du contexte sert de toute façon en test)
        if self.essai { "Software\\MindsetTest\\Origines".into() } else { ORIGINES_UTILISATEUR.into() }
    }

    fn cle_faux(chemin: &str, nom: &str) -> String {
        format!("{}|{nom}", crate::registre::cle_normale(chemin))
    }

    /// `Get-RegValue` : la valeur (variables « %...% » remplacées), `None` si elle n'existe pas. Mode test : le faux
    /// registre de l'utilisateur d'abord.
    pub fn registre_valeur(&self, chemin: &str, nom: &str) -> Option<Value> {
        if self.essai
            && crate::registre::utilisateur(chemin)
            && let Some((v, _)) = verrou(&self.interne).faux_registre.get(&Self::cle_faux(chemin, nom))
        {
            return if v.is_null() { None } else { Some(v.clone()) };
        }
        crate::registre::valeur(chemin, nom)
    }

    /// `Get-RegState` : `{ Existed, Value, Kind }`. Mode test : le faux registre de l'utilisateur d'abord.
    pub fn registre_etat(&self, chemin: &str, nom: &str) -> Value {
        if self.essai
            && crate::registre::utilisateur(chemin)
            && let Some((v, k)) = verrou(&self.interne).faux_registre.get(&Self::cle_faux(chemin, nom))
        {
            return if v.is_null() {
                json!({ "Existed": false, "Value": null, "Kind": null })
            } else {
                json!({ "Existed": true, "Value": v, "Kind": k })
            };
        }
        crate::registre::etat(chemin, nom)
    }

    /// `Write-RegValue` : un réglage de l'utilisateur (HKCU) est écrit par Nevermind ; un réglage du PC passe par
    /// l'Aide (`reg.ecrire`). Mode test : le faux registre, jamais le vrai. Seulement sous le verrou d'un geste.
    pub fn ecrire_registre(&self, chemin: &str, nom: &str, valeur: &Value, kind: &str) -> Result<(), String> {
        self.exiger_geste()?;
        if !crate::registre::utilisateur(chemin) {
            self.aide("reg.ecrire", &json!({ "Cle": chemin, "Nom": nom, "Valeur": valeur, "Type": kind }), 120000)?;
            return Ok(());
        }
        if self.essai {
            verrou(&self.interne).faux_registre.insert(Self::cle_faux(chemin, nom), (valeur.clone(), kind.to_string()));
            return Ok(());
        }
        crate::registre::ecrire_utilisateur(chemin, nom, valeur, kind)
    }

    /// `Remove-RegValue` : comme `ecrire_registre` (l'Aide pour le PC : `reg.effacer`)
    pub fn effacer_registre(&self, chemin: &str, nom: &str) -> Result<(), String> {
        self.exiger_geste()?;
        if !crate::registre::utilisateur(chemin) {
            self.aide("reg.effacer", &json!({ "Cle": chemin, "Nom": nom }), 120000)?;
            return Ok(());
        }
        if self.essai {
            verrou(&self.interne).faux_registre.insert(Self::cle_faux(chemin, nom), (Value::Null, String::new()));
            return Ok(());
        }
        crate::registre::effacer_utilisateur(chemin, nom)
    }

    /// Tests : le faux registre de l'utilisateur (« HKCU:\...|nom » : valeur, type ; une valeur nulle = effacée)
    pub fn faux_registre(&self) -> HashMap<String, (Value, String)> {
        verrou(&self.interne).faux_registre.clone()
    }

    /// Tests : les valeurs d'origine « gardées dans le registre » (`Read-OriginesRegistre`, jamais les vraies)
    pub fn origines_factices(&self, l: Vec<Value>) {
        verrou(&self.interne).origines_factices = l;
    }

    /// `Read-OriginesRegistre` (mode test : les valeurs factices)
    pub fn origines_registre(&self) -> Vec<Value> {
        if self.essai {
            return verrou(&self.interne).origines_factices.clone();
        }
        crate::registre::origines(ORIGINES_MACHINE, &self.cle_origines_utilisateur())
    }

    // (les changements de l'action en cours, `$script:RunLog`, sont dans le journal de chaque geste : gestes.rs)
}

/// L'heure locale au format de `Get-Date -Format s` (« 2026-11-06T21:05:09 »).
pub fn horodatage() -> String {
    moment::iso(&moment::maintenant())
}
