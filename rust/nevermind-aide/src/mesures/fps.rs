//! La mesure des FPS pour Nevermind (classe `AideFps` de NevermindAide.ps1) : PresentMon demande les droits
//! administrateur, il tourne donc ici, et ses lignes sont gardées jusqu'à ce que Nevermind les lise (« fps.lire »),
//! 20 000 au plus. « #ERR » : une erreur, « #FIN » : arrêté.
//!
//! Veille (« fps.veiller ») : certains anti-triche (BF6) refusent toute nouvelle mesure une fois le jeu lancé, mais
//! laissent tourner celle qui existait avant. PresentMon est alors lancé avant le jeu, sur le nom de son programme, et
//! la mesure du jeu (« fps.demarrer ») s'y rattache au lieu de repartir de zéro.
//!
//! Même comportement que la version PowerShell, réponse pour réponse : Nevermind (FrameMon.StartRemote, natif.cs) lit
//! ces lignes toutes les 250 ms et les compte comme si PresentMon tournait chez lui.

use crate::systeme::{Lance, SortiePm, Systeme};
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

/// Lignes gardées au plus, et rendues au plus par lecture
const LIGNES_MAX: usize = 20000;
const LIGNES_PAR_LECTURE: usize = 5000;
/// Ce que PresentMon reçoit après la cible : sa sortie en texte, sans affichage, sous le nom de session de Nevermind
const OPTIONS: &str = "--output_stdout --no_console_stats --stop_existing_session --session_name Nevermind";

#[derive(Default)]
struct Etat {
    /// Le PresentMon en cours (son numéro de lancement, et de quoi l'arrêter) ; un PresentMon arrêté peut encore
    /// parler après le lancement du suivant : on n'écoute que celui en cours
    en_cours: Option<(u64, Box<dyn Lance>)>,
    lancements: u64,
    /// Le PresentMon du dernier lancement (pour fermer sa session à l'arrêt)
    exe: String,
    lignes: VecDeque<String>,
    fin: bool,
    /// Première ligne de PresentMon (noms des colonnes), redonnée au rattachement
    entete: Option<String>,
    /// Programmes guettés (minuscules, triés), vide hors veille
    veille: Vec<String>,
    rattachee: bool,
    donner_entete: bool,
}

impl Etat {
    /// Veille en cours, pas encore prise par un jeu
    fn en_veille(&self) -> bool {
        !self.veille.is_empty() && !self.rattachee && self.en_cours.is_some() && !self.fin
    }
}

/// La mesure des FPS de l'Aide (une seule à la fois)
#[derive(Clone, Default)]
pub struct Fps(Arc<Mutex<Etat>>);

/// Tri de .NET `StringComparer.OrdinalIgnoreCase` (noms déjà vérifiés : lettres minuscules, chiffres, « ._()- »)
fn ordre(a: &str, b: &str) -> Ordering {
    a.bytes().map(|c| c.to_ascii_uppercase()).cmp(b.bytes().map(|c| c.to_ascii_uppercase()))
}

impl Fps {
    fn etat(&self) -> MutexGuard<'_, Etat> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Mesure du jeu `pid` (PresentMon s'arrête avec lui)
    pub fn demarrer(&self, s: &dyn Systeme, exe: &str, pid: i64) -> Result<(), String> {
        self.etat().rattachee = true;
        self.lancer(s, exe, &format!("--process_id {pid} --terminate_on_proc_exit"))
    }

    /// Guette ces programmes (« bf6.exe ») avant leur lancement. Déjà en veille sur les mêmes : rien à faire.
    pub fn veiller(&self, s: &dyn Systeme, exe: &str, noms: &[String]) -> Result<(), String> {
        let mut tri = noms.to_vec();
        tri.sort_by(|a, b| ordre(a, b));
        {
            let mut e = self.etat();
            if e.en_veille() && tri.join("|") == e.veille.join("|") {
                return Ok(());
            }
            e.rattachee = false;
        }
        let cible: Vec<String> = tri.iter().map(|n| format!("--process_name \"{n}\"")).collect();
        self.lancer(s, exe, &cible.join(" "))?;
        self.etat().veille = tri;
        Ok(())
    }

    /// Le jeu (son programme) est-il guetté par la veille en cours ? Si oui, la mesure s'y rattache.
    pub fn rattacher(&self, nom: &str) -> bool {
        let mut e = self.etat();
        let nom = aide_regles::texte::minuscules(nom);
        if !e.en_veille() || !e.veille.contains(&nom) {
            return false;
        }
        e.donner_entete = e.entete.is_some();
        e.rattachee = true;
        true
    }

    fn lancer(&self, s: &dyn Systeme, exe: &str, cible: &str) -> Result<(), String> {
        self.arreter(s);
        let numero = {
            let mut e = self.etat();
            e.lignes.clear();
            e.fin = false;
            e.exe = exe.to_string();
            e.entete = None;
            e.lancements += 1;
            e.lancements
        };
        let fps = self.clone();
        let rappel = Arc::new(move |sortie: SortiePm| fps.recevoir(numero, sortie));
        // (le verrou n'est pas gardé pendant le lancement : les lignes de ce PresentMon attendent qu'il soit noté)
        let mut e = self.etat();
        match s.lancer_presentmon(exe, &format!("{cible} {OPTIONS}"), rappel) {
            Ok(l) => {
                e.en_cours = Some((numero, l));
                Ok(())
            }
            Err(m) => Err(m),
        }
    }

    /// Ce que dit le PresentMon numéro `numero`
    fn recevoir(&self, numero: u64, sortie: SortiePm) {
        let mut e = self.etat();
        if e.en_cours.as_ref().map(|x| x.0) != Some(numero) {
            return;
        }
        match sortie {
            SortiePm::Ligne(l) => {
                if e.entete.is_none() {
                    e.entete = Some(l.clone());
                    if !e.rattachee {
                        return;
                    }
                }
                e.lignes.push_back(l);
                while e.lignes.len() > LIGNES_MAX {
                    e.lignes.pop_front();
                }
            }
            SortiePm::Erreur(l) => {
                if !l.is_empty() {
                    e.lignes.push_back(format!("#ERR {l}"));
                }
            }
            SortiePm::Fin => e.fin = true,
        }
    }

    /// Les lignes arrivées depuis la dernière lecture (5 000 au plus), « #FIN » une fois PresentMon arrêté et tout lu
    pub fn lire(&self) -> String {
        let mut e = self.etat();
        let mut t = String::new();
        // Première lecture après un rattachement : les noms des colonnes d'abord
        if e.donner_entete {
            e.donner_entete = false;
            if let Some(x) = &e.entete {
                t.push_str(x);
                t.push('\n');
            }
        }
        let mut n = 0;
        while n < LIGNES_PAR_LECTURE {
            let Some(l) = e.lignes.pop_front() else { break };
            t.push_str(&l);
            t.push('\n');
            n += 1;
        }
        if e.fin && e.lignes.is_empty() {
            t.push_str("#FIN\n");
        }
        t
    }

    /// Arrête PresentMon (et la veille), et ferme sa session dans Windows. Les lignes déjà reçues restent à lire.
    pub fn arreter(&self, s: &dyn Systeme) {
        let (lance, exe) = {
            let mut e = self.etat();
            e.veille.clear();
            (e.en_cours.take(), e.exe.clone())
        };
        let Some((_, mut l)) = lance else { return };
        l.arreter();
        drop(l);
        s.fermer_session_presentmon(&exe);
    }
}

/// Coupe la sortie de PresentMon en lignes comme .NET (`OutputDataReceived`) : « \r\n », « \r » ou « \n » finissent
/// une ligne (« \r\n » coupé entre deux lectures compris), une ligne vide compte, la dernière ligne sans fin aussi
#[derive(Default)]
pub struct Decoupeur {
    ligne: Vec<u8>,
    apres_cr: bool,
}

impl Decoupeur {
    /// Des octets lus : chaque ligne complète part dans `rappel`
    pub fn ajouter(&mut self, octets: &[u8], mut rappel: impl FnMut(String)) {
        for &b in octets {
            if self.apres_cr && b == b'\n' {
                self.apres_cr = false;
                continue;
            }
            self.apres_cr = b == b'\r';
            if b == b'\r' || b == b'\n' {
                rappel(String::from_utf8_lossy(&self.ligne).into_owned());
                self.ligne.clear();
            } else {
                self.ligne.push(b);
            }
        }
    }

    /// La fin de la sortie : la dernière ligne, si elle n'est pas vide
    pub fn finir(&mut self, mut rappel: impl FnMut(String)) {
        if !self.ligne.is_empty() {
            rappel(String::from_utf8_lossy(&self.ligne).into_owned());
            self.ligne.clear();
        }
    }
}
