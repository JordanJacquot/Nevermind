//! Ce que le bloc lit dans Windows et les rares gestes qu'il y fait, derrière un trait : le module `windows` donne le
//! vrai, les tests un faux (rien ne touche le PC pendant un test). Toutes les lectures sont sans droits et sans lancer
//! de programme ; chacune est comptée (`lectures`), pour mesurer la légèreté.

/// Un programme ouvert : numéro, nom (sans « .exe », comme `ProcessName`), temps de processeur (s), mémoire (octets)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Programme {
    pub pid: u32,
    pub nom: String,
    pub cpu: f64,
    pub memoire: f64,
}

/// Ce que dit la carte NVIDIA (nvml.dll), comme les colonnes de nvidia-smi
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Nvidia {
    pub utilisation: Option<f64>,
    pub temperature: Option<f64>,
    pub vram_utilisee: Option<f64>,
    pub vram_totale: Option<f64>,
    pub puissance: Option<f64>,
    pub puissance_max: Option<f64>,
}

/// Ce que le compteur par-dessus le jeu doit montrer
#[derive(Clone, Debug, PartialEq)]
pub struct EtatCompteur {
    /// « complet » ou « discret »
    pub style: String,
    /// hg, hd, bg, bd
    pub coin: String,
    pub chiffre: String,
    pub sous: String,
    /// La couleur du chiffre (vert fluide, orange, rouge), en RGB
    pub couleur: u32,
}

/// Le compteur de FPS par-dessus le jeu (une petite fenêtre native transparente, que la souris traverse) et le
/// raccourci Ctrl+Maj+F
pub trait Compteur: Send {
    /// Montrer (ou mettre à jour) le compteur sur l'écran de la fenêtre au premier plan
    fn montrer(&mut self, e: &EtatCompteur);
    fn cacher(&mut self);
    /// Le remettre devant les autres fenêtres (toutes les 3 s)
    fn devant(&mut self);
    /// Prendre (ou rendre) le raccourci Ctrl+Maj+F à Windows ; faux s'il est déjà pris par un autre programme
    fn raccourci(&mut self, actif: bool) -> bool;
    /// Le raccourci a été pressé depuis le dernier appel
    fn raccourci_presse(&mut self) -> bool;
}

pub trait Systeme: Send {
    /// GetSystemTimes : (temps d'inactivité, temps total) de tous les cœurs, en 100 ns
    fn temps_processeur(&mut self) -> Option<(u64, u64)>;
    /// GlobalMemoryStatusEx : (mémoire totale, mémoire utilisée), en octets
    fn memoire(&mut self) -> Option<(f64, f64)>;
    /// D3DKMTQueryStatistics : le temps de travail de chaque moteur de chaque carte graphique (100 ns), et l'instant de
    /// la lecture (100 ns)
    fn moteurs_gpu(&mut self) -> Option<(Vec<i64>, i64)>;
    /// La carte NVIDIA, si nvml.dll est là
    fn nvidia(&mut self) -> Option<Nvidia>;
    /// PDH : (vitesse du processeur en % de la normale, disque occupé en %)
    fn compteurs(&mut self) -> (Option<f64>, Option<f64>);
    /// Les programmes ouverts (une seule lecture de Windows pour tous)
    fn programmes(&mut self) -> Vec<Programme>;
    /// Le chemin d'un programme (droit de lecture minimal : marche aussi avec les anti-triche), vide sinon
    fn chemin(&mut self, pid: u32) -> String;
    /// Le programme tourne encore
    fn vivant(&mut self, pid: u32) -> bool;
    /// Le programme de la fenêtre au premier plan, et le titre de cette fenêtre
    fn devant(&mut self) -> (u32, String);
    /// Relancer une appli fermée par le mode jeu (sans droits, sans console) ; faux si elle n'a pas pu l'être
    fn lancer(&mut self, chemin: &str, args: Option<&str>) -> bool;
    /// Le PC tourne sur batterie (ou onduleur)
    fn sur_batterie(&mut self) -> bool;
    /// Le PC a une batterie
    fn batterie(&mut self) -> bool;
    /// Les écrans : « nom|carte|largeur|hauteur|Hz|Hz max|principal » (GetDisplays de natif.cs)
    fn ecrans(&mut self) -> Vec<String>;
    /// Les noms des cartes graphiques
    fn cartes(&mut self) -> Vec<String>;
    fn coeurs(&self) -> usize;
    /// Le disque du système : (taille, place libre, lettre « C: »)
    fn disque_systeme(&mut self) -> Option<(f64, f64, String)>;
    /// Priorité haute d'un jeu (Image File Execution Options\<exe>\PerfOptions, CpuPriorityClass = 3)
    fn priorite(&mut self, exe: &str) -> bool;
    /// La carte graphique choisie pour un programme (HKCU\Software\Microsoft\DirectX\UserGpuPreferences)
    fn preference_gpu(&mut self, exe: &str) -> String;
    fn fichier_existe(&mut self, chemin: &str) -> bool;
    fn dossier_existe(&mut self, chemin: &str) -> bool;
    /// Le nombre de lectures de Windows faites depuis le début
    fn lectures(&self) -> u64;
}

/// Un faux PC pour les tests : rien n'est lu ni fait, tout est écrit dans ses champs
#[derive(Default)]
pub struct FauxSysteme {
    pub cpu: Option<(u64, u64)>,
    pub ram: Option<(f64, f64)>,
    pub gpu: Option<(Vec<i64>, i64)>,
    pub nv: Option<Nvidia>,
    pub perf: Option<f64>,
    pub disque: Option<f64>,
    pub progs: Vec<Programme>,
    pub chemins: std::collections::HashMap<u32, String>,
    pub premier_plan: (u32, String),
    pub lances: Vec<String>,
    pub lancements_rates: Vec<String>,
    pub batterie_seule: bool,
    /// Les fichiers présents sur le faux PC (PresentMon de l'appli...)
    pub fichiers: Vec<String>,
    /// Programmes ouverts et fenêtre au premier plan que le test change pendant que le bloc tourne (un jeu lancé
    /// puis quitté) ; à la place de `progs` et `premier_plan` quand il est donné
    pub partage: Option<std::sync::Arc<std::sync::Mutex<Partage>>>,
    pub n: u64,
}

/// Ce que le test change sur le faux PC pendant que le bloc tourne
#[derive(Default)]
pub struct Partage {
    pub progs: Vec<Programme>,
    pub premier_plan: u32,
}

impl FauxSysteme {
    fn progs_vus(&self) -> Vec<Programme> {
        match &self.partage {
            Some(p) => p.lock().unwrap_or_else(|e| e.into_inner()).progs.clone(),
            None => self.progs.clone(),
        }
    }
}

impl Systeme for FauxSysteme {
    fn temps_processeur(&mut self) -> Option<(u64, u64)> {
        self.n += 1;
        self.cpu
    }
    fn memoire(&mut self) -> Option<(f64, f64)> {
        self.n += 1;
        self.ram
    }
    fn moteurs_gpu(&mut self) -> Option<(Vec<i64>, i64)> {
        self.n += 1;
        self.gpu.clone()
    }
    fn nvidia(&mut self) -> Option<Nvidia> {
        self.n += 1;
        self.nv
    }
    fn compteurs(&mut self) -> (Option<f64>, Option<f64>) {
        self.n += 1;
        (self.perf, self.disque)
    }
    fn programmes(&mut self) -> Vec<Programme> {
        self.n += 1;
        self.progs_vus()
    }
    fn chemin(&mut self, pid: u32) -> String {
        self.n += 1;
        self.chemins.get(&pid).cloned().unwrap_or_default()
    }
    fn vivant(&mut self, pid: u32) -> bool {
        self.n += 1;
        self.progs_vus().iter().any(|p| p.pid == pid)
    }
    fn devant(&mut self) -> (u32, String) {
        self.n += 1;
        match &self.partage {
            Some(p) => (p.lock().unwrap_or_else(|e| e.into_inner()).premier_plan, String::new()),
            None => self.premier_plan.clone(),
        }
    }
    fn lancer(&mut self, chemin: &str, args: Option<&str>) -> bool {
        self.lances.push(format!("{chemin}|{}", args.unwrap_or("")));
        !self.lancements_rates.iter().any(|r| r == chemin)
    }
    fn sur_batterie(&mut self) -> bool {
        self.batterie_seule
    }
    fn batterie(&mut self) -> bool {
        false
    }
    fn ecrans(&mut self) -> Vec<String> {
        Vec::new()
    }
    fn cartes(&mut self) -> Vec<String> {
        Vec::new()
    }
    fn coeurs(&self) -> usize {
        8
    }
    fn disque_systeme(&mut self) -> Option<(f64, f64, String)> {
        None
    }
    fn priorite(&mut self, _exe: &str) -> bool {
        false
    }
    fn preference_gpu(&mut self, _exe: &str) -> String {
        String::new()
    }
    fn fichier_existe(&mut self, chemin: &str) -> bool {
        self.fichiers.iter().any(|f| f.eq_ignore_ascii_case(chemin))
    }
    fn dossier_existe(&mut self, _chemin: &str) -> bool {
        false
    }
    fn lectures(&self) -> u64 {
        self.n
    }
}

/// Un faux compteur pour les tests
#[derive(Default)]
pub struct FauxCompteur {
    pub montre: Option<EtatCompteur>,
    pub fois_devant: u32,
    pub raccourci_pris: bool,
    pub presse: bool,
}

impl Compteur for FauxCompteur {
    fn montrer(&mut self, e: &EtatCompteur) {
        self.montre = Some(e.clone());
    }
    fn cacher(&mut self) {
        self.montre = None;
    }
    fn devant(&mut self) {
        self.fois_devant += 1;
    }
    fn raccourci(&mut self, actif: bool) -> bool {
        self.raccourci_pris = actif;
        true
    }
    fn raccourci_presse(&mut self) -> bool {
        std::mem::take(&mut self.presse)
    }
}
