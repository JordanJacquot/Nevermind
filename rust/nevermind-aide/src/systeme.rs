//! Tout ce que l'Aide demande à Windows, derrière un trait : l'exe livré n'a qu'une seule façon de faire, les vrais
//! appels de Windows (module `windows`) ; seuls les tests de `cargo test` le remplacent par un faux PC.

use aide_regles::demandes::{Carte, Demarrage, Pc};
use aide_regles::fermeture::Fiche;
use aide_regles::registre::Ruche;
use serde_json::Value;
use std::net::IpAddr;
use std::sync::Arc;

/// Une valeur lue dans le registre
#[derive(Clone, Debug, PartialEq)]
pub enum ValeurLue {
    Texte(String),
    TexteEtendu(String),
    Binaire(Vec<u8>),
    DWord(u32),
    QWord(u64),
    Multi(Vec<String>),
    /// Un autre type (son numéro), et ses octets
    Autre(u32, Vec<u8>),
}

/// Qui fait tourner l'Aide
#[derive(Clone, Debug)]
pub struct Identite {
    /// SID du compte (S-1-5-18 en SYSTEM)
    pub compte: String,
    /// Avec les droits administrateur
    pub admin: bool,
    /// Numéro du processus
    pub pid: u32,
}

/// Un fichier ou un dossier trouvé en parcourant un dossier
#[derive(Clone, Debug)]
pub struct Entree {
    pub chemin: String,
    pub dossier: bool,
    /// Lien (jonction, lien symbolique) : jamais suivi
    pub lien: bool,
    pub lecture_seule: bool,
    pub taille: u64,
    /// Dernière écriture, en ticks UTC de .NET
    pub ecrit: i64,
}

/// Pourquoi un fichier n'a pas été effacé
#[derive(Clone, Debug)]
pub enum ErreurEffacement {
    /// Accès refusé
    Refuse,
    /// Fichier utilisé par un programme (ou autre erreur d'entrée-sortie)
    Utilise,
}

/// Un processus en cours
#[derive(Clone, Debug)]
pub struct ProcessusVu {
    pub pid: i64,
    /// Nom du programme (« chrome.exe »)
    pub nom: String,
}

/// Une tâche planifiée
#[derive(Clone, Debug)]
pub struct TacheVue {
    pub nom: String,
    /// Son dossier, « \ » final compris (« \Mindset\ », « \ »)
    pub dossier: String,
    pub desactivee: bool,
    pub compte: String,
    pub groupe: String,
    /// Programme et arguments de chaque action (vides pour une action qui ne lance pas de programme)
    pub actions: Vec<(String, String)>,
}

/// Ce que dit PresentMon, ligne par ligne (comme `OutputDataReceived` et `ErrorDataReceived` de .NET)
#[derive(Clone, Debug, PartialEq)]
pub enum SortiePm {
    /// Une ligne de sa sortie normale (les colonnes, puis une ligne par image)
    Ligne(String),
    /// Une ligne de sa sortie d'erreur
    Erreur(String),
    /// PresentMon s'est arrêté
    Fin,
}

/// Où arrive ce que dit PresentMon (appelé depuis n'importe quel fil)
pub type RappelPm = Arc<dyn Fn(SortiePm) + Send + Sync>;

/// Un PresentMon lancé
pub trait Lance: Send {
    /// L'arrête (s'il tourne encore)
    fn arreter(&mut self);
}

/// Un événement de la trace réseau de Windows (fournisseur « Kernel-Network »)
pub struct EvenementReseau<'a> {
    /// Son numéro (10 : TCP envoyé, 11 : TCP reçu, 42 : UDP envoyé... ; voir mesures::flux)
    pub id: u16,
    /// Sa date, en heure système (FILETIME, 100 ns depuis 1601) : Windows la convertit pour le lecteur en temps réel.
    /// JAMAIS un compteur QPC (bug du 15/10 : des âges de -13 milliards de ms).
    pub date: i64,
    /// Ses données (`None` : aucune)
    pub donnees: Option<&'a [u8]>,
}

/// Où arrivent les événements de la trace (depuis le fil de la trace)
pub type RappelTrace = Arc<dyn Fn(&EvenementReseau) + Send + Sync>;

/// Pourquoi Windows refuse de démarrer la trace : l'appel qui a échoué et son code d'erreur
#[derive(Clone, Debug, PartialEq)]
pub enum RefusTrace {
    /// StartTrace (après un second essai si une session restée ouverte a été fermée)
    Session(u32),
    /// EnableTraceEx2
    Fournisseur(u32),
    /// OpenTrace
    Lecture(u32),
}

/// Une trace réseau en cours
pub trait Trace: Send {
    /// Elle reçoit encore les événements
    fn tourne(&self) -> bool;
    /// Ferme la session de Windows et attend la fin de la lecture (2 s au plus)
    fn arreter(&mut self);
}

/// Une connexion TCP de la table de Windows (`MIB_TCPROW_OWNER_PID`, `MIB_TCP6ROW_OWNER_PID`), telle quelle
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LigneTcp {
    pub v6: bool,
    /// État (2 : en écoute, 5 : établie)
    pub etat: u32,
    /// Adresses (les 4 premiers octets en IPv4), dans l'ordre du réseau
    pub adresse_locale: [u8; 16],
    pub adresse_distante: [u8; 16],
    /// Ports tels que Windows les range (octets du réseau dans un entier : voir mesures::tcp::port)
    pub port_local: u32,
    pub port_distant: u32,
    /// Zones IPv6 (0 en IPv4)
    pub zone_locale: u32,
    pub zone_distante: u32,
    pub pid: u32,
}

/// Les appels à Windows dont l'Aide se sert pour faire les demandes (après leur vérification)
pub trait Systeme: Pc + Send + Sync + 'static {
    // --- Qui, où, quand
    fn identite(&self) -> Identite;
    /// Le dossier de NevermindAide.exe (celui de cette copie de Nevermind)
    fn racine(&self) -> String;
    /// L'heure locale, « 2026-10-26T12:00:00 » (`Get-Date -Format s`)
    fn heure_locale(&self) -> String;
    /// Maintenant, en ticks UTC de .NET
    fn ticks_utc(&self) -> i64;
    /// Une heure UTC en heure locale (ticks de .NET), selon les règles d'heure d'été de cette date
    fn ticks_local(&self, utc: i64) -> i64;

    // --- Registre
    fn reg_lire(&self, ruche: Ruche, cle: &str, nom: &str) -> Option<ValeurLue>;
    fn reg_ecrire(&self, ruche: Ruche, cle: &str, nom: &str, valeur: &ValeurLue) -> Result<(), String>;
    /// Efface la valeur (rien à faire si la clé ou la valeur n'existent pas)
    fn reg_effacer(&self, ruche: Ruche, cle: &str, nom: &str) -> Result<(), String>;
    /// Efface la clé et tout ce qu'elle contient
    fn reg_effacer_arbre(&self, ruche: Ruche, cle: &str) -> Result<(), String>;

    // --- Réglages
    fn service_regler(&self, nom: &str, demarrage: Demarrage, lancer: bool, arreter: bool) -> Result<(), String>;
    /// Met ces serveurs DNS sur la carte (aucun : ceux de la box, par DHCP)
    fn dns_appliquer(&self, carte: &Carte, serveurs: &[String]) -> Result<(), String>;
    fn dns_vider_cache(&self);
    /// Relance la carte réseau de ce numéro (éteinte puis rallumée, comme Restart-NetAdapter), et attend que Windows
    /// l'ait fait
    fn carte_relancer(&self, index: i32) -> Result<(), String>;
    /// Les noms de toutes les règles du pare-feu
    fn pare_feu_regles(&self) -> Result<Vec<String>, String>;
    fn pare_feu_ajouter(&self, nom: &str, programme: &str, service: Option<&str>, entrant: bool) -> Result<(), String>;
    /// Retire toutes les règles de ce nom exact
    fn pare_feu_retirer(&self, nom: &str) -> Result<(), String>;
    fn pare_feu_renommer(&self, ancien: &str, nouveau: &str) -> Result<(), String>;
    /// Rallume le pare-feu pour les trois profils
    fn pare_feu_activer(&self) -> Result<(), String>;
    fn point_restauration(&self, description: &str) -> Result<(), String>;
    /// Retire cet élément des exclusions de l'antivirus de Windows (chemin, programme ou extension ; erreurs ignorées)
    fn retirer_exclusion(&self, element: &str);
    /// Passe ce profil Wi-Fi en connexion manuelle (erreurs ignorées)
    fn wifi_manuel(&self, profil: &str);
    /// Coupe SMB1 sur le serveur, et sur le client si demandé (erreurs ignorées)
    fn smb1_couper(&self, client: bool);

    // --- Lectures réservées aux administrateurs
    /// Secure Boot, puce TPM, santé des disques
    fn lecture_pc(&self) -> Value;
    /// Antivirus de Windows : nombre d'exclusions, menaces, détections
    fn lecture_securite(&self) -> Value;
    /// Toutes les tâches planifiées (cachées comprises)
    fn taches(&self) -> Vec<TacheVue>;
    /// Un nom de compte en SID, `None` si inconnu
    fn traduire_compte(&self, nom: &str) -> Option<String>;
    /// L'emplacement des programmes en cours (numéro, chemin)
    fn chemins_processus(&self) -> Vec<(i64, String)>;

    // --- Fichiers (nettoyage)
    /// Ce que contient le dossier (sans suivre les liens)
    fn entrees(&self, dossier: &str) -> Vec<Entree>;
    fn dossier_existe(&self, dossier: &str) -> bool;
    fn effacer_fichier(&self, chemin: &str, lecture_seule: bool) -> Result<(), ErreurEffacement>;
    /// Efface le dossier s'il est vide
    fn effacer_dossier_vide(&self, dossier: &str);

    // --- Processus
    fn processus(&self) -> Vec<ProcessusVu>;
    /// Le chemin complet du programme du processus (vide si illisible)
    fn chemin_processus(&self, pid: i64) -> String;
    /// Ce que Windows dit d'un processus (`Read-AideFiche` ; `chemin` : à défaut de le lire)
    fn fiche(&self, pid: i64, chemin: &str) -> Fiche;
    /// Ferme le processus ; vrai si Windows l'a fait
    fn fermer_processus(&self, pid: i64) -> bool;

    // --- Mesures continues (étape 1b)
    /// PresentMon du dossier de Nevermind, seulement s'il est bien signé par Intel (`Get-AidePresentMon`) : son
    /// chemin, ou le message d'erreur
    fn presentmon(&self) -> Result<String, String>;
    /// Lance PresentMon avec ces arguments, sans fenêtre ; ses lignes arrivent dans `rappel`, puis `SortiePm::Fin`
    fn lancer_presentmon(&self, chemin: &str, arguments: &str, rappel: RappelPm) -> Result<Box<dyn Lance>, String>;
    /// Ferme la session de mesure que PresentMon arrêté de force laisse ouverte dans Windows (attend 3 s au plus)
    fn fermer_session_presentmon(&self, chemin: &str);
    /// Démarre la trace réseau (session « Nevermind-Reseau ») : la trace, ou la raison du refus de Windows
    fn demarrer_trace(&self, rappel: RappelTrace) -> Result<Box<dyn Trace>, RefusTrace>;
    /// Les adresses de ce PC (toutes les cartes, bouclage compris)
    fn adresses_locales(&self) -> Vec<IpAddr>;
    /// Maintenant, en heure système (FILETIME, 100 ns depuis 1601) : l'horloge des événements de la trace
    fn filetime(&self) -> i64;
    /// La table des connexions TCP de Windows (`None` si Windows ne la donne pas)
    fn table_tcp(&self, v6: bool) -> Option<Vec<LigneTcp>>;
    /// Demande à Windows de compter les octets de cette connexion : vrai s'il accepte (administrateur seulement)
    fn tcp_compter(&self, l: &LigneTcp) -> bool;
    /// Octets envoyés et reçus de cette connexion depuis que Windows les compte (`None` si illisibles)
    fn tcp_octets(&self, l: &LigneTcp) -> Option<(u64, u64)>;
}

/// Une connexion au canal de l'Aide
pub trait Connexion: Send {
    /// Le processus qui s'est connecté (0 si inconnu)
    fn client(&self) -> u32;
    /// Lit ce qui arrive dans `tampon`, en attendant au plus `ms` millisecondes : `Ok(None)` si rien n'est arrivé à
    /// temps, `Ok(Some(0))` si le client a coupé
    fn lire(&mut self, tampon: &mut [u8], ms: u32) -> Result<Option<usize>, String>;
    /// Écrit tout, et attend que le client l'ait lu
    fn ecrire(&mut self, octets: &[u8]) -> Result<(), String>;
}

/// Le canal en attente d'une connexion
pub trait Ecoute: Send {
    type C: Connexion + 'static;
    /// Attend une connexion au plus `ms` millisecondes : `Ok(true)` si un client s'est connecté
    fn attendre(&mut self, ms: u32) -> Result<bool, String>;
    /// Le canal connecté, qui devient la connexion de ce client
    fn connexion(self) -> Self::C;
}

/// Pourquoi un exemplaire du canal n'a pas été créé
#[derive(Debug)]
pub enum ErreurCanal {
    /// Le canal a été pris par un autre programme (propriétaire inattendu) : l'Aide s'arrête
    Repris(String),
    /// Toutes les connexions sont prises, ou autre erreur passagère
    Indisponible(String),
}

/// Ce dont le démarrage et l'écoute de l'Aide ont besoin, en plus des demandes
pub trait Hote: Systeme {
    type E: Ecoute;
    /// Prend le verrou de ce nom (une seule Aide à la fois) : `None` s'il est déjà pris. Gardé tant que la valeur vit.
    fn verrou(&self, nom: &str) -> Option<Box<dyn Send>>;
    /// Prépare le dossier aide-temp (réservé au système et aux administrateurs quand l'Aide en a les droits)
    fn preparer_aide_temp(&self, dossier: &str, admin: bool) -> Result<(), String>;
    /// Un exemplaire du canal `nom`, avec les droits `sddl`. `premier` : le tout premier (refusé par Windows si un autre
    /// programme a déjà un canal de ce nom). `proprietaire` : le propriétaire attendu des exemplaires suivants.
    fn canal(&self, nom: &str, sddl: &str, premier: bool, proprietaire: Option<&str>) -> Result<Self::E, ErreurCanal>;
}
