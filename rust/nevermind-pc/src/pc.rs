//! Tout ce que le bloc lit ou fait sur le PC passe par [`Pc`] : le registre, l'Aide, powercfg, les écrans, la souris,
//! les réglages de Nevermind (parametres.json), la sauvegarde, l'historique, les messages et les questions.
//!
//! Le vrai PC : `windows::PcWindows` (lectures sans droits en natif, écritures du registre de l'utilisateur comme
//! donnees.ps1, tout le reste par l'Aide). Les tests : un faux PC qui NOTE chaque geste au lieu de le faire, avec les
//! mêmes noms que le faux PC de outils\exporter-juges-pc.ps1, pour comparer la suite exacte des gestes.

use serde_json::{Map, Value, json};

/// Une valeur du registre (`GetValue` de .NET : DWORD signé, QWORD, texte, liste de textes, octets).
#[derive(Debug, Clone, PartialEq)]
pub enum ValeurReg {
    DWord(i64),
    QWord(i64),
    Texte(String),
    TexteEtendu(String),
    Multi(Vec<String>),
    Octets(Vec<u8>),
}

impl ValeurReg {
    /// Le type comme le donne .NET (`GetValueKind`) : DWord, QWord, String, ExpandString, MultiString, Binary.
    pub fn sorte(&self) -> &'static str {
        match self {
            ValeurReg::DWord(_) => "DWord",
            ValeurReg::QWord(_) => "QWord",
            ValeurReg::Texte(_) => "String",
            ValeurReg::TexteEtendu(_) => "ExpandString",
            ValeurReg::Multi(_) => "MultiString",
            ValeurReg::Octets(_) => "Binary",
        }
    }

    /// La valeur en JSON, comme la version PowerShell l'écrit (les octets : une liste de nombres).
    pub fn json(&self) -> Value {
        match self {
            ValeurReg::DWord(n) | ValeurReg::QWord(n) => json!(n),
            ValeurReg::Texte(s) | ValeurReg::TexteEtendu(s) => json!(s),
            ValeurReg::Multi(l) => json!(l),
            ValeurReg::Octets(o) => json!(o.iter().map(|b| i64::from(*b)).collect::<Vec<_>>()),
        }
    }

    /// Une valeur écrite d'après son type (« DWord », « String », « Binary »...), comme `SetValue` de .NET.
    pub fn depuis(valeur: &Value, sorte: &str) -> ValeurReg {
        let n = crate::net::nombre_de(valeur);
        match sorte {
            "DWord" => ValeurReg::DWord(crate::net::entier(n)),
            "QWord" => ValeurReg::QWord(crate::net::entier(n)),
            "Binary" => ValeurReg::Octets(
                crate::net::liste(valeur).iter().map(|v| crate::net::entier(crate::net::nombre_de(v)) as u8).collect(),
            ),
            "MultiString" => ValeurReg::Multi(crate::net::liste(valeur).iter().map(crate::net::texte).collect()),
            "ExpandString" => ValeurReg::TexteEtendu(crate::net::texte(valeur)),
            _ => ValeurReg::Texte(crate::net::texte(valeur)),
        }
    }

    /// `[string]$v` (une liste : ses éléments séparés par une espace).
    pub fn texte(&self) -> String {
        match self {
            ValeurReg::DWord(n) | ValeurReg::QWord(n) => n.to_string(),
            ValeurReg::Texte(s) | ValeurReg::TexteEtendu(s) => s.clone(),
            ValeurReg::Multi(l) => l.join(" "),
            ValeurReg::Octets(o) => o.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(" "),
        }
    }

    /// `[int]$v` (`None` si ce n'est pas un nombre : une liste, un texte illisible).
    pub fn entier(&self) -> Option<i64> {
        match self {
            ValeurReg::DWord(n) | ValeurReg::QWord(n) => Some(*n),
            ValeurReg::Texte(s) | ValeurReg::TexteEtendu(s) => {
                let t = s.trim();
                if t.is_empty() { Some(0) } else { t.parse::<f64>().ok().map(crate::net::entier) }
            }
            _ => None,
        }
    }

    /// Une liste (MultiString, Binary) : `-is [array]`
    pub fn est_liste(&self) -> bool {
        matches!(self, ValeurReg::Multi(_) | ValeurReg::Octets(_))
    }
}

/// Le résultat du nettoyage d'une catégorie (`Invoke-CleanOne`) : { Freed, Deleted, Skipped, Errors, Log }.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResultatNettoyage {
    pub libere: f64,
    pub supprimes: i64,
    pub laisses: i64,
    pub refuses: i64,
    pub journal: Vec<String>,
}

/// Une demande d'écriture du registre faite par Nevermind lui-même (réglages de l'utilisateur, HKCU).
pub trait Pc {
    // ---- Le verrou des gestes (lot A du second audit : un seul geste qui change le PC à la fois)
    /// Ce geste tient-il le verrou sur ce fil ? Sinon rien n'est changé (le vrai PC : `Contexte::exiger_geste` ; les
    /// faux PC des tests : toujours oui).
    fn exiger_geste(&self) -> Result<(), String> {
        Ok(())
    }
    /// Ajustement B : le verrou rendu le temps du point de restauration (rien n'est noté pendant ce temps)
    fn suspendre_geste(&mut self) -> Option<u64> {
        None
    }
    /// Le verrou repris après le point de restauration, à son tour
    fn reprendre_geste(&mut self, _id: u64) -> Result<(), String> {
        Ok(())
    }

    // ---- Le registre
    /// L'état d'une valeur (`Get-RegState`) : `Ok(None)` si elle n'existe pas, `Err` si la clé est illisible.
    fn reg_lire(&self, chemin: &str, nom: &str) -> Result<Option<ValeurReg>, String>;
    /// Les noms des valeurs d'une clé (programmes au démarrage), `None` si la clé n'existe pas.
    fn reg_noms(&self, _chemin: &str) -> Option<Vec<String>> {
        None
    }
    /// Écrire une valeur du registre de l'utilisateur (HKCU) : Nevermind le fait lui-même (donnees.ps1).
    fn reg_utilisateur_ecrire(&mut self, chemin: &str, nom: &str, valeur: &ValeurReg) -> Result<(), String>;
    fn reg_utilisateur_effacer(&mut self, chemin: &str, nom: &str) -> Result<(), String>;
    /// La copie de la valeur d'origine dans le registre de l'utilisateur (`Software\Mindset\Nevermind\Origines`), si elle
    /// n'y est pas déjà.
    fn origine_utilisateur(&mut self, cle: &str, json: &str);
    fn origine_utilisateur_oublier(&mut self, cle: &str);
    fn origines_utilisateur_effacer(&mut self);
    /// (seulement si l'Aide n'a pas pu oublier les siennes : la clé de la machine, qui demande les droits)
    fn origines_machine_effacer(&mut self) -> Result<(), String>;

    // ---- L'Aide (tout ce qui demande les droits administrateur)
    /// Un geste demandé d'un clic : peut proposer la demande de Windows si l'Aide ne répond pas.
    fn aide(&mut self, action: &str, parametres: Value, delai_ms: u64) -> Result<Value, String>;
    /// `Get-AideLecture` : une lecture réservée aux administrateurs (fin d'une lecture, pas un clic), jamais de
    /// demande de Windows. Par défaut `aide` : seulement pour un Pc dont l'Aide ne demande jamais rien (le
    /// désinstalleur, déjà administrateur ; les faux PC des tests). Celui de Nevermind passe par `ctx.aide_lecture`.
    fn aide_lecture(&mut self, action: &str, parametres: Value) -> Option<Value> {
        // (une lecture ; une action qui change le PC passerait par le verrou)
        if nevermind_noyau::aide::change_le_pc(action) {
            self.exiger_geste().ok()?;
        }
        self.aide(action, parametres, 60000).ok()
    }

    // ---- La lecture lente du PC
    /// Relire tout le PC (« Analyser » : `$AnalysisDataWork`), dans le travail de fond. Par défaut `None` : ce PC ne
    /// se relit pas (les faux PC des juges : l'analyse est refaite sur ce qui a été lu au début).
    fn relire(&mut self) -> Option<crate::lecture::Lu> {
        None
    }
    /// La réponse de NVIDIA au « dernier pilote Game Ready » (le texte JSON de gfwsl.geforce.com), dans le travail de
    /// fond ; `None` : pas de réponse (pas d'Internet, mode test, faux PC des juges).
    fn reponse_pilote_nvidia(&mut self) -> Option<String> {
        None
    }

    // ---- Alimentation, écrans, souris, programmes
    /// powercfg avec ses arguments : (code de sortie, texte écrit).
    fn powercfg(&mut self, args: &[String]) -> (i32, String);
    /// Le curseur du mode d'alimentation de Windows (`None` : pas de curseur sur ce PC).
    fn curseur(&self) -> Option<String>;
    fn curseur_ecrire(&mut self, guid: &str) -> i32;
    /// Les écrans : « appareil|nom|largeur|hauteur|fréquence|fréquence max|principal ».
    fn ecrans(&self) -> Vec<String>;
    fn ecran_frequence(&mut self, appareil: &str, hz: i64) -> i32;
    fn souris_relire(&mut self, seuil1: i64, seuil2: i64, vitesse: i64);
    /// Un service : (démarrage « Automatic »/« Manual »/« Disabled », il tourne), `None` s'il n'existe pas.
    fn service(&self, nom: &str) -> Option<(String, bool)>;
    /// Lancer un programme de Windows (`Start-Process` : ShellExecute, qui pose la question de Windows s'il demande
    /// les droits, comme mdsched.exe). Attend la réponse à cette question : hors du fil des blocs. `Err` : pourquoi il
    /// ne s'est pas ouvert (M8, audit du 08/11 : l'erreur était jetée, et le bouton ne faisait rien).
    fn lancer(&mut self, args: &[String]) -> Result<(), String>;
    /// Ouvrir une adresse (`Open-Url`).
    fn ouvrir(&mut self, adresse: &str);
    /// Une commande (`shutdown.exe /r /fw /t 10`) : son code de sortie.
    fn commande(&mut self, exe: &str, args: &[String]) -> i32;
    /// Les règles du pare-feu posées par Nevermind (« Nevermind : bloque ... », « OptiGame : bloque ... »).
    fn regles_pare_feu(&self) -> usize {
        0
    }
    /// Le numéro actuel d'une carte réseau d'après son identifiant (il change après un redémarrage).
    fn index_carte(&self, _guid: &str) -> Option<i64> {
        None
    }
    /// Le dossier des fichiers temporaires de l'utilisateur (`$env:TEMP`) et celui de Windows.
    fn dossier_temp(&self) -> String;
    fn dossier_windows(&self) -> String;
    /// Le fichier d'hibernation existe (`Test-HibernateOn`).
    fn hiberfil(&self) -> bool;
    /// Nettoyer une catégorie de l'utilisateur (fichiers temporaires), faite par Nevermind lui-même.
    fn nettoyer_dossier(&mut self, id: &str, chemins: &[String], exclus: &[String]) -> ResultatNettoyage;

    // ---- Les fichiers de Nevermind
    fn reglage(&self, nom: &str) -> Option<Value>;
    fn ecrire_reglage(&mut self, nom: &str, valeur: Value, exiger: bool) -> Result<(), String>;
    /// sauvegarde.json (`Save-Backup`).
    fn sauvegarde_ecrire(&mut self, sauvegarde: &Value) -> Result<(), String>;
    /// sauvegarde.json par l'écrivain unique (lot A, étape 2) : seulement ce que le geste a changé depuis sa copie
    /// (`base` devenue `voulue`) est fusionné dans le fichier relu. Rend la copie à jour (`None` : ce PC écrit sa copie
    /// telle quelle, les faux PC des tests).
    fn sauvegarde_fusionner(&mut self, _base: &Value, voulue: &Value) -> Result<Option<Value>, String> {
        self.exiger_geste()?;
        self.sauvegarde_ecrire(voulue).map(|_| None)
    }
    /// Effacer sauvegarde.json et ses copies (fin de « Tout restaurer »).
    fn sauvegarde_effacer(&mut self);
    /// ignores.json
    fn ignores_ecrire(&mut self, ignores: &[String]);
    /// Une entrée de l'historique (`Add-History`) : son identifiant, `None` si rien n'a changé.
    fn historique_ajouter(&mut self, titre: &str, details: &[String], journal: &[Value]) -> Option<String>;
    fn historique_annule(&mut self, id: &str);
    /// Le journal de nettoyage (`nettoyage\nettoyage-<date>.log`) : son chemin, vide s'il n'a pas pu être écrit.
    fn journal_nettoyage(&mut self, nom: &str, lignes: &[String]) -> String;
    /// Une ligne au journal de Nevermind.
    fn journal(&mut self, _ligne: &str) {}

    // ---- L'écran
    fn message(&mut self, texte: &str);
    /// Une question Oui / Non : `Some(réponse)` si elle est connue tout de suite (tests), `None` si elle est posée
    /// à l'écran (la suite reprend avec la réponse).
    fn question(&mut self, texte: &str) -> Option<bool>;
    fn statut(&mut self, _texte: &str) {}
    /// La réponse à « Tes écrans s'affichent bien ? » (keep, revert, timeout) si elle est connue tout de suite (tests),
    /// `None` si elle est posée à l'écran (15 secondes, puis retour à l'ancien réglage).
    fn choix_ecran(&mut self) -> Option<String> {
        None
    }
    /// « Tout restaurer » : les entrées de l'historique entièrement remises sont marquées annulées
    /// (`Set-HistoryAllUndone`). `pare_feu_echoue` : le déblocage des règles du pare-feu a échoué, les
    /// lignes « pare-feu » restent à annuler (lot A).
    fn historique_tout_annule(&mut self, _sauvegarde: &crate::corrections::Sauvegarde, _pare_feu_echoue: bool) {}
    /// Les changements définitifs notés dans l'historique (« Tout restaurer » ne les annule pas)
    fn definitifs(&mut self) -> Vec<String> {
        Vec::new()
    }

    /// L'heure de maintenant (« 2026-11-06T12:00:00 »), le jour pour les âges.
    fn maintenant(&self) -> crate::temps::Moment;
}

/// Les paramètres d'une demande à l'Aide, dans l'ordre des clés (comme le faux PC des juges les note).
pub fn parametres(p: &[(&str, Value)]) -> Value {
    let mut m = Map::new();
    let mut v: Vec<&(&str, Value)> = p.iter().collect();
    v.sort_by(|a, b| a.0.cmp(b.0));
    for (k, x) in v {
        m.insert((*k).to_string(), x.clone());
    }
    Value::Object(m)
}
