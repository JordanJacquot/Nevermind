//! La vie de Nevermind avec Mindset, sans Tauri : le portage de Nevermind.ps1 (démarrage, instance unique, dossier
//! des données) et de evenements.ps1 (accrochage dans Mindset, demandes d'affichage et de fermeture, thème de la
//! suite), pour ce qui touche au contrat entre Mindset et une appli (D:\Projets\Mindset\docs\contrat-appli.md).
//!
//! Tout ce qui parle à Windows est derrière des traits ([`Systeme`], [`Fenetre`]) : le programme les remplit avec
//! Windows et Tauri, les tests avec des faux. Les fichiers (suite.json, afficher.demande, quitter.demande...) sont lus
//! et écrits pour de vrai, aux chemins donnés par l'appelant : les tests prennent un dossier d'essai.
//!
//! - [`demarrage`] : Nevermind.ps1, des lignes 28 à 380 ;
//! - [`surveillance`] : evenements.ps1 (`Update-EmbedState`, `Watch-Requests`, `Watch-SuiteTheme`) et la veille de
//!   la page (vue.ps1, `Update-VeilleVue`).

pub mod demarrage;
pub mod surveillance;

use std::io;
use std::path::Path;
use std::time::Duration;

/// Comment s'est passée la prise d'un verrou (`Enter-Instance`, Nevermind.ps1:99-113).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriseVerrou {
    /// Créé, ou libéré par l'autre copie pendant l'attente (abandonné compris) : il est à nous
    Pris,
    /// Windows refuse de l'ouvrir : il appartient à une copie lancée en administrateur (« refuse »)
    Refuse,
    /// Toujours tenu après l'attente (« occupe »)
    Occupe,
}

/// Windows, sauf la fenêtre et les fichiers.
pub trait Systeme {
    /// `Test-InstanceRunning` pour un nom : le verrou existe (accès refusé compris).
    fn verrou_existe(&mut self, nom: &str) -> bool;
    /// Crée et prend le verrou (`New-Object Mutex($true, nom, [ref]$nouveau)`), en attendant au plus `attente` s'il
    /// est déjà tenu. Pris, il est gardé jusqu'à [`Systeme::liberer_verrous`].
    fn prendre_verrou(&mut self, nom: &str, attente: Duration) -> PriseVerrou;
    /// `Exit-Instance` : rend tous les verrous pris.
    fn liberer_verrous(&mut self);
    /// `InstallLocation` de `HKLM\...\Uninstall\Mindset`, s'il se lit (lecture seule).
    fn emplacement_mindset(&mut self) -> Option<String>;
    /// Lance un programme sans l'attendre (`Start-Process`).
    fn lancer(&mut self, exe: &Path, arguments: &[&str], dossier: Option<&Path>) -> io::Result<()>;
    /// `Start-Process explorer.exe "<cible>"` : un programme ou une adresse ouverts par l'Explorateur.
    fn ouvrir_par_explorateur(&mut self, cible: &str) -> io::Result<()>;
    /// `AllowSetForegroundWindow(-1)` (Send-ShowRequest) : la copie ouverte peut passer devant.
    fn permettre_premier_plan(&mut self);
    /// Lève un signal nommé d'un autre programme (celui de Mindset : `Local\Mindset-Afficher`).
    fn lever_signal(&mut self, nom: &str);
    /// Attend (les boucles d'attente du démarrage).
    fn dormir(&mut self, duree: Duration);
    /// Le message « Nevermind s'utilise maintenant avec Mindset » (`Show-MindsetRequired`) : `true` si l'utilisateur
    /// demande d'ouvrir (ou de télécharger) Mindset.
    fn demander_mindset(&mut self, mindset_installe: bool) -> bool;
}

/// Une taille de fenêtre, en pixels logiques.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Taille {
    pub largeur: f64,
    pub hauteur: f64,
}

/// La fenêtre ou l'un de ses parents, pour la veille de la page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Niveau {
    /// `IsWindowVisible`
    pub visible: bool,
    /// `IsIconic`
    pub reduite: bool,
}

/// La fenêtre principale (une seule, créée au démarrage, jamais recréée : contrat-appli partie 7).
pub trait Fenetre {
    /// La fenêtre a son handle.
    fn existe(&self) -> bool;
    /// Accrochée dans Mindset : `WS_CHILD` ou un parent (`EmbedInfo.IsEmbedded`, natif.cs:2362-2367).
    fn est_accrochee(&self) -> bool;
    /// `IsWindowVisible` (`EmbedInfo.IsShown`).
    fn est_visible(&self) -> bool;
    /// Se montre sans prendre la main (evenements.ps1:72-80 ; `SW_SHOWNA`), rendue si elle était réduite.
    fn montrer_sans_activer(&mut self);
    /// Se montre au premier plan (`Show-MainWindow`, assistance.ps1:239-251).
    fn montrer_et_activer(&mut self);
    /// Se cache (`Hide-ToTray` d'une copie gérée : `$Window.Hide()`).
    fn cacher(&mut self);
    /// Se réduit dans la barre des tâches (copie de travail, qui n'a pas d'icône près de l'horloge).
    fn reduire(&mut self);
    /// La taille minimale ; `None` : aucune (accrochée, evenements.ps1:70-71).
    fn poser_taille_minimale(&mut self, taille: Option<Taille>);
    /// La propriété `Mindset.BarrePropre` (natif.cs:2354-2359) : Mindset cache alors sa barre.
    fn poser_barre_propre(&mut self, oui: bool);
    /// La fenêtre, puis chacun de ses parents jusqu'au premier niveau (`GetParent`).
    fn niveaux(&self) -> Vec<Niveau>;
    /// Sa fenêtre principale (la sienne, ou Mindset) est au premier plan (`VueNatif.EstDevant`, natif.cs:2462-2468).
    fn est_devant(&self) -> bool;
    /// Le processus au premier plan est celui-ci (`Test-ThemeSansGener` : `$fg -eq $PID`).
    fn premier_plan_est_a_moi(&self) -> bool;
    /// La page est-elle visible pour WebView2 (`SetIsVisible`) : cachée, elle se met en pause.
    fn page_visible(&mut self, visible: bool);
    /// La page en veille profonde (`TrySuspend`) ou réveillée (`Resume`), une minute après avoir été cachée.
    fn page_en_veille(&mut self, veille: bool);
}
