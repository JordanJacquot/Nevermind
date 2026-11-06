//! Le seul module qui parle à Windows (et le seul où le code « unsafe » est permis) : les lectures de la page RAM
//! (lecture seule, sans droits administrateur) et les deux gestes de « Fermer » sur des processus déjà gardés.
//!
//! Plus léger que la version PowerShell : la liste des processus et leur mémoire propre viennent d'un seul appel
//! (NtQuerySystemInformation) au lieu de deux requêtes WMI ; les services d'un seul EnumServicesStatusEx ; les
//! compteurs de la mémoire de PDH ; chaque processus n'est ouvert qu'une fois pour sa fiche.

#![allow(unsafe_code)]

mod fenetres;
mod lectures;
mod processus;

use crate::constat::Cible;
use crate::donnees::{Donnees, Pilote};
use crate::pc::{Lecture, Pc};
use aide_regles::dossiers::Dossiers;
use aide_regles::fermeture::{Contexte as ContexteRegle, Fiche, Reference};
use std::collections::HashMap;
use std::path::PathBuf;

/// Le vrai PC
pub struct PcWindows;

/// Rend toutes les poignées gardées (`[NmFermeture]::Relacher()`)
pub fn relacher() {
    processus::relacher();
}

/// Les dossiers de Windows (donnés par Windows, jamais par les variables d'environnement)
pub fn dossiers() -> Dossiers {
    use ::windows::Win32::UI::Shell::{
        FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86, FOLDERID_Windows,
    };
    Dossiers {
        windows: lectures::dossier_connu(&FOLDERID_Windows).unwrap_or_default(),
        programmes: lectures::dossier_connu(&FOLDERID_ProgramFiles).unwrap_or_default(),
        programmes_x86: lectures::dossier_connu(&FOLDERID_ProgramFilesX86).unwrap_or_default(),
        donnees_communes: lectures::dossier_connu(&FOLDERID_ProgramData).unwrap_or_default(),
    }
}

/// La lecture complète (`Get-DonneesMemoire`)
pub fn lire(demarrage: &[String], connus: &HashMap<String, Vec<Pilote>>) -> Lecture {
    let mut journal = Vec::new();
    let dos = dossiers();
    let mut d = Donnees {
        pc: lectures::nom_du_pc(),
        date: Some(lectures::maintenant()),
        windir: dos.windows.clone(),
        programmes: dos.programmes.clone(),
        demarrage: demarrage.iter().filter(|x| !x.is_empty()).cloned().collect(),
        ..Default::default()
    };
    match lectures::memoire() {
        Ok(m) => {
            d.visible = m.visible;
            d.disponible = m.disponible;
            d.installee = m.installee;
        }
        Err(e) => journal.push(format!("Mémoire : système illisible ({e})")),
    }
    match lectures::compteurs() {
        Ok(c) => {
            d.disponible = c.disponible;
            d.cache = c.cache;
            d.modifiee = c.modifiee;
            d.pool_np = c.pool_np;
            d.pool_p = c.pool_p;
        }
        Err(e) => journal.push(format!("Mémoire : compteurs illisibles ({e})")),
    }
    // Pour le bouton « Fermer » : Nevermind lui-même, les antivirus, et les poignées gardées jusqu'à la prochaine lecture
    processus::relacher();
    let moi = processus::reference(std::process::id());
    d.antivirus = lectures::antivirus(&dos);
    match processus::liste(&moi) {
        Ok(l) => d.processus = l,
        Err(e) => journal.push(format!("Mémoire : liste des programmes illisible ({e})")),
    }
    d.moi =
        Some(crate::donnees::Moi { id: moi.id, session: moi.session, compte: moi.compte.clone(), eleve: moi.eleve });
    match lectures::services() {
        Ok(s) => d.services = s,
        Err(e) => journal.push(format!("Mémoire : services illisibles ({e})")),
    }
    // Pilotes : seulement si leur mémoire est élevée, les plus grosses étiquettes (et leur pilote, s'il est déjà connu)
    let seuils = crate::constat::seuils_pilotes(if d.installee != 0.0 { d.installee } else { d.visible });
    if d.pool_np >= seuils.surveiller {
        match lectures::etiquettes_pool(3) {
            Ok(et) => {
                d.etiquettes = et
                    .into_iter()
                    .filter(|(_, o)| *o >= 50.0 * 1024.0 * 1024.0)
                    .map(|(tag, octets)| crate::donnees::Etiquette {
                        pilotes: connus.get(&tag).cloned().unwrap_or_default(),
                        tag,
                        octets,
                    })
                    .collect()
            }
            Err(e) => journal.push(format!("Mémoire : étiquettes des pilotes illisibles ({e})")),
        }
    }
    Lecture { donnees: Some(d), journal, erreur: None }
}

impl Pc for PcWindows {
    fn lire(&self, demarrage: &[String], connus: &HashMap<String, Vec<Pilote>>) -> Lecture {
        lire(demarrage, connus)
    }
    fn chercher_pilotes(&self, tags: &[String]) -> HashMap<String, Vec<Pilote>> {
        lectures::pilotes_etiquettes(&dossiers().windows, tags)
    }
    fn contexte_fermeture(&self) -> Result<ContexteRegle, String> {
        let dos = dossiers();
        let services_pid = lectures::services()?.into_iter().filter(|s| s.pid > 0).map(|s| s.pid).collect();
        let moi = processus::reference(std::process::id());
        Ok(ContexteRegle {
            windows: dos.windows.clone(),
            programmes: dos.programmes.clone(),
            antivirus: lectures::antivirus(&dos),
            services_pid: Some(services_pid),
            reference: Some(Reference { id: moi.id, session: moi.session, compte: moi.compte, eleve: moi.eleve }),
            processus: HashMap::new(),
        })
    }
    fn fiche(&self, pid: i64, chemin: &str) -> Fiche {
        processus::fiche(pid, chemin)
    }
    fn verifier(&self, pid: i64, debut: i64, chemin: &str) -> i32 {
        processus::verifier(pid, debut, chemin)
    }
    fn fermer_poliment(&self, pid: i64, debut: i64, chemin: &str) -> i32 {
        let v = processus::verifier(pid, debut, chemin);
        if v == 0 {
            return -2;
        }
        if v < 0 {
            return -1;
        }
        fenetres::fermer_poliment(pid)
    }
    fn forcer(&self, pid: i64, debut: i64, chemin: &str) -> i32 {
        let v = processus::verifier(pid, debut, chemin);
        if v == 0 {
            return -2;
        }
        if v < 0 {
            return -1;
        }
        processus::terminer(pid)
    }
    fn vivant(&self, pid: i64) -> i32 {
        processus::vivant(pid)
    }
    fn etat_fermeture(&self, restants: &[Cible]) -> &'static str {
        fenetres::etat(restants)
    }
    fn service(&self, nom: &str) -> Option<(String, bool)> {
        lectures::service(nom)
    }
    fn bureau(&self) -> Option<PathBuf> {
        lectures::dossier_connu(&::windows::Win32::UI::Shell::FOLDERID_Desktop).map(PathBuf::from)
    }
}

/// Essais : garde la poignée d'un processus (comme la lecture) et rend son heure de démarrage, 0 si impossible
#[doc(hidden)]
pub fn garder(pid: u32) -> i64 {
    processus::garder(pid)
}

/// Le temps processeur pris par ce programme depuis son démarrage, en secondes (pour les mesures de légèreté)
pub fn temps_processeur() -> f64 {
    use ::windows::Win32::Foundation::FILETIME;
    use ::windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let (mut c, mut e, mut k, mut u) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY : lecture des temps de ce processus
    let _ = unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) };
    let v = |f: FILETIME| ((u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)) as f64 / 1e7;
    v(k) + v(u)
}
