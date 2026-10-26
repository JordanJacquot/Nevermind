//! L'Aide elle-même (NevermindAide.ps1) : pour qui elle travaille, son canal, ses connexions, et quand elle s'arrête.
//!
//! Elle écoute sur un canal nommé réservé à l'utilisateur (refusé depuis le réseau), une demande par connexion : une
//! ligne JSON `{ "a": action, "p": paramètres }`, une ligne JSON en réponse `{ "ok": true, "r": ... }` ou
//! `{ "ok": false, "e": message }`. Avec les droits administrateur, elle ne répond qu'au programme qui fait tourner
//! Nevermind (Mindset.exe de ce même dossier, reconnu par son chemin complet) sous le compte servi. Elle s'arrête
//! toute seule quand Nevermind n'est plus ouvert.

use crate::actions::{self, EtatAide};
use crate::systeme::{Connexion, Ecoute, ErreurCanal, Hote, Systeme};
use aide_regles::{canal, chemin, client, demandes, sid};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Taille maximale d'une demande, et temps maximal pour la recevoir
const TAILLE_MAX: usize = 65536;
const DELAI_DEMANDE: Duration = Duration::from_secs(5);
/// Lignes gardées dans le journal
const LIGNES_MAX: usize = 200;

/// Journal gardé en mémoire (200 dernières lignes) : Nevermind le demande (« aide.journal ») et l'écrit dans le sien.
/// L'Aide n'écrit aucun fichier : en administrateur, écrire dans un dossier modifiable sans droits permettrait à un
/// programme d'y glisser un lien et de lui faire écrire ailleurs.
#[derive(Clone, Default)]
pub struct Journal(Arc<Mutex<VecDeque<String>>>);

impl Journal {
    pub fn ecrire(&self, s: &dyn Systeme, message: &str) {
        let mut l = self.0.lock().unwrap_or_else(|e| e.into_inner());
        l.push_back(format!("{} {message}", s.heure_locale()));
        while l.len() > LIGNES_MAX {
            l.pop_front();
        }
    }

    pub fn lignes(&self) -> Vec<String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
    }
}

/// Les droits du canal (SDDL), posés un par un : l'utilisateur servi lit et écrit (lui seul, aucun autre compte), le
/// réseau est refusé, et seule l'Aide crée les connexions : le système en SYSTEM, sinon les administrateurs (demande
/// de Windows). Propriétaire : le système ou les administrateurs, que Nevermind vérifie ; un programme sans droits ne
/// peut pas le donner à un canal. Sans droits administrateur (essais seulement), l'utilisateur peut aussi créer les
/// connexions (l'Aide ne peut alors rien faire de plus que lui).
pub fn droits_canal(sid_servi: &str, admin: bool, en_systeme: bool) -> String {
    // (mêmes masques que PipeSecurity de .NET : FullControl 0x1f019f ; ReadWrite 0x2019b plus Synchronize 0x100000 ;
    // CreateNewInstance 0x4)
    let createur = if en_systeme { "SY" } else { "BA" };
    let utilisateur = if admin { "0x12019b" } else { "0x12019f" };
    let proprietaire = if admin { format!("O:{createur}") } else { String::new() };
    format!("{proprietaire}D:P(D;;0x1f019f;;;NU)(A;;{utilisateur};;;{sid_servi})(A;;0x1f019f;;;{createur})")
}

/// Le texte de la réponse à une ligne de demande (`None` : ligne vide, pas de réponse), et le résumé pour le journal
pub fn repondre(
    ligne: &str,
    s: &dyn Systeme,
    client: u32,
    etat: &EtatAide,
    journal: &Journal,
) -> (String, Option<String>) {
    let mut action = String::new();
    let resultat: Result<Value, String> = (|| {
        let d: Value = serde_json::from_str(ligne).map_err(|_| "Demande illisible (JSON non valable).".to_string())?;
        action = aide_regles::ps::chaine(d.get("a"));
        let p = d.get("p").cloned().unwrap_or(Value::Null);
        // Le journal de l'Aide (« Signaler un problème » de Nevermind), sans compter cette demande-ci
        if aide_regles::texte::egal(&action, "aide.journal") {
            return Ok(json!(journal.lignes()));
        }
        let demande = demandes::verifier(&action, &p, s)?;
        match catch_unwind(AssertUnwindSafe(|| actions::faire(demande, s, client, etat))) {
            Ok(r) => r,
            Err(_) => Err("Erreur interne de l'Aide.".into()),
        }
    })();
    match resultat {
        Ok(r) => {
            let resume = (!aide_regles::texte::egal(&action, "aide.journal")).then(|| format!("{action} : fait"));
            (json!({ "ok": true, "r": r }).to_string(), resume)
        }
        Err(e) => {
            let resume = format!("{action} : refusé ou échoué ({e})");
            (json!({ "ok": false, "e": e }).to_string(), Some(resume))
        }
    }
}

/// Une connexion : la demande lue (64 Ko au plus, en 5 s au plus, sinon la connexion est coupée), faite, la réponse
/// écrite. Un client muet ou bavard ne bloque pas l'Aide et ne lui fait pas garder des mégaoctets en mémoire.
pub fn traiter<C: Connexion>(mut c: C, s: &dyn Systeme, etat: &EtatAide, journal: &Journal) {
    let client = c.client();
    let resume = (|| -> Result<Option<String>, String> {
        let mut tampon = vec![0u8; TAILLE_MAX];
        let mut lu = 0;
        let mut fin = None;
        let limite = Instant::now() + DELAI_DEMANDE;
        while fin.is_none() {
            if lu >= tampon.len() {
                return Err("demande trop longue (plus de 64 Ko), connexion coupée".into());
            }
            let reste = limite.saturating_duration_since(Instant::now());
            if reste.is_zero() {
                return Err("demande trop lente (plus de 5 s), connexion coupée".into());
            }
            let ms = u32::try_from(reste.as_millis().max(1)).unwrap_or(u32::MAX);
            let n = match c.lire(&mut tampon[lu..], ms)? {
                None => return Err("demande trop lente (plus de 5 s), connexion coupée".into()),
                Some(0) => break,
                Some(n) => n,
            };
            if let Some(i) = tampon[lu..lu + n].iter().position(|&b| b == b'\n') {
                fin = Some(lu + i);
            }
            lu += n;
        }
        let ligne = String::from_utf8_lossy(&tampon[..fin.unwrap_or(lu)]).trim_end_matches('\r').to_string();
        if ligne.is_empty() {
            return Ok(None);
        }
        let (reponse, resume) = repondre(&ligne, s, client, etat, journal);
        c.ecrire(format!("{reponse}\r\n").as_bytes())?;
        Ok(resume)
    })();
    let resume = match resume {
        Ok(r) => r,
        Err(e) => Some(format!("Échange interrompu : {e}")),
    };
    drop(c);
    if let Some(r) = resume {
        journal.ecrire(s, &r);
    }
}

/// Le programme qui parle à l'Aide est-il Nevermind, de l'utilisateur servi ? Mindset.exe de ce dossier (son chemin
/// complet) et, avec les droits administrateur, sous le compte servi : en SYSTEM, l'Aide voit aussi les programmes des
/// autres comptes ouverts sur le PC, qui peuvent utiliser la même copie de Nevermind.
fn client_servi(s: &dyn Systeme, pid: i64, chemin_client: &str, racine: &str, admin: bool, sid_servi: &str) -> bool {
    client::accepte(chemin_client, racine)
        && (!admin || aide_regles::texte::egal(&s.fiche(pid, chemin_client).compte, sid_servi))
}

/// Nevermind (la fenêtre) est-il ouvert pour l'utilisateur servi ? Mindset.exe de ce dossier, reconnu par son chemin
/// complet : les autres applis de la suite et Mindset lui-même tournent aussi dans un « Mindset.exe ».
fn nevermind_ouvert(s: &dyn Systeme, racine: &str, admin: bool, sid_servi: &str) -> bool {
    s.processus().iter().any(|p| {
        let c = s.chemin_processus(p.pid);
        !c.is_empty() && client_servi(s, p.pid, &c, racine, admin, sid_servi)
    })
}

/// Lance l'Aide : `sid_recu` est la valeur de `-Sid`. Rend quand l'Aide s'arrête.
pub fn lancer<H: Hote>(h: Arc<H>, sid_recu: Option<&str>) {
    let s: &H = &h;
    let moi = s.identite();
    let en_systeme = moi.compte == sid::SYSTEME;
    // Pour qui l'Aide travaille : en SYSTEM, seulement l'utilisateur inscrit dans la tâche ; lancée en secours, son
    // propre compte seulement. Sinon elle ne démarre pas.
    let Ok(sid_servi) = sid::compte_servi(&moi.compte, sid_recu) else {
        return;
    };
    let racine = s.racine();
    let Some(suffixe) = canal::suffixe(&racine) else {
        return;
    };

    // aide-temp (compatibilité : la version PowerShell y compilait son code C#, et les essais vérifient ses droits)
    let dossier_temp = chemin::joindre(&racine, "aide-temp");
    let temp = match s.preparer_aide_temp(&dossier_temp, moi.admin) {
        Ok(()) => format!("{dossier_temp}\\"),
        Err(_) if moi.admin => return,
        Err(_) => String::new(),
    };

    // Une seule Aide à la fois
    let Some(_verrou) = s.verrou(&canal::nom_verrou(&sid_servi, &suffixe)) else {
        return;
    };

    let journal = Journal::default();
    let nom = canal::nom_canal(&sid_servi, &suffixe);
    let sddl = droits_canal(&sid_servi, moi.admin, en_systeme);
    let proprietaire = moi.admin.then_some(if en_systeme { "S-1-5-18" } else { "S-1-5-32-544" });

    // La toute première connexion, refusée par Windows si un autre programme a déjà pris ce nom
    let premier = match s.canal(&nom, &sddl, true, None) {
        Ok(e) => e,
        Err(ErreurCanal::Repris(e)) | Err(ErreurCanal::Indisponible(e)) => {
            journal.ecrire(s, &format!("Canal déjà utilisé par un autre programme, l'Aide s'arrête ({e})"));
            return;
        }
    };
    let mode = if en_systeme {
        " en SYSTEM"
    } else if !moi.admin {
        " SANS droits administrateur"
    } else {
        ""
    };
    journal.ecrire(s, &format!("Aide {} démarrée{mode} pour {sid_servi} (processus {})", demandes::VERSION, moi.pid));
    if moi.admin
        && let Err(e) = actions::renommer_anciennes_regles(s)
    {
        journal.ecrire(s, &format!("Règles du pare-feu non renommées : {e}"));
    }

    let etat = Arc::new(EtatAide {
        admin: moi.admin,
        compte: moi.compte.clone(),
        pid: moi.pid,
        temp,
        mesures: Default::default(),
    });
    let en_cours = Arc::new(AtomicUsize::new(0));
    let mut ecoute = Some(premier);
    let mut pris = false;
    let mut seul = Instant::now(); // depuis quand Nevermind n'est plus ouvert
    let mut cherche = Instant::now(); // dernière recherche de Nevermind

    // Canal suivant, ou rien quand toutes les connexions sont prises (demandes lentes qui s'entassent) : l'Aide ne
    // s'arrête pas pour autant, elle réessaie juste après. `Err` : le canal a été repris par un autre programme.
    let suivant = |pris: &mut bool| -> Result<Option<H::E>, ()> {
        match s.canal(&nom, &sddl, false, proprietaire) {
            Ok(e) => {
                if *pris {
                    *pris = false;
                    journal.ecrire(s, "Connexions libérées, l'Aide écoute de nouveau");
                }
                Ok(Some(e))
            }
            Err(ErreurCanal::Repris(e)) => {
                journal.ecrire(s, &format!("Canal repris par un autre programme, l'Aide s'arrête ({e})"));
                Err(())
            }
            Err(ErreurCanal::Indisponible(e)) => {
                if !*pris {
                    *pris = true;
                    journal
                        .ecrire(s, &format!("Toutes les connexions sont prises, nouvel essai dans un instant ({e})"));
                }
                Ok(None)
            }
        }
    };

    loop {
        let pret = match ecoute.as_mut() {
            Some(e) => match e.attendre(1000) {
                Ok(p) => p,
                Err(err) => {
                    journal.ecrire(s, &format!("Erreur de l'Aide : {err}"));
                    break;
                }
            },
            None => {
                std::thread::sleep(Duration::from_millis(200));
                false
            }
        };
        if !pret {
            if ecoute.is_none() {
                match suivant(&mut pris) {
                    Ok(e) => ecoute = e,
                    Err(()) => break,
                }
            }
            if cherche.elapsed() < Duration::from_secs(5) {
                continue; // (Nevermind cherché toutes les 5 s)
            }
            cherche = Instant::now();
            if nevermind_ouvert(s, &racine, moi.admin, &sid_servi) {
                seul = Instant::now();
            } else if seul.elapsed() >= Duration::from_secs(20) && en_cours.load(Ordering::SeqCst) == 0 {
                break;
            }
            continue;
        }
        let Some(connectee) = ecoute.take() else { continue };
        let connexion = connectee.connexion();
        // Le canal suivant est prêt avant de traiter celui-ci : il n'y a jamais de moment sans Aide à l'écoute
        match suivant(&mut pris) {
            Ok(e) => ecoute = e,
            Err(()) => break,
        }
        let pid_client = connexion.client();
        let chemin_client = if pid_client != 0 { s.chemin_processus(i64::from(pid_client)) } else { String::new() };
        if moi.admin && !client_servi(s, i64::from(pid_client), &chemin_client, &racine, moi.admin, &sid_servi) {
            let qui = if chemin_client.is_empty() { format!("processus {pid_client}") } else { chemin_client.clone() };
            let compte = if pid_client != 0 {
                format!(" (compte {})", s.fiche(i64::from(pid_client), &chemin_client).compte)
            } else {
                String::new()
            };
            journal.ecrire(s, &format!("Refusé : demande venant de {qui}{compte}"));
            drop(connexion);
            continue;
        }
        seul = Instant::now();
        // Une demande : lue, vérifiée et faite dans un fil à part (une analyse antivirus ne bloque pas les autres)
        en_cours.fetch_add(1, Ordering::SeqCst);
        let (h2, etat2, journal2, en_cours2) =
            (Arc::clone(&h), Arc::clone(&etat), journal.clone(), Arc::clone(&en_cours));
        std::thread::spawn(move || {
            let _ = catch_unwind(AssertUnwindSafe(|| traiter(connexion, &*h2, &etat2, &journal2)));
            en_cours2.fetch_sub(1, Ordering::SeqCst);
        });
    }
    drop(ecoute);
    // Mesures en cours : PresentMon et la trace réseau s'arrêtent avec l'Aide
    etat.mesures.arreter_tout(s);
    journal.ecrire(s, "Aide arrêtée (Nevermind fermé)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn droits_du_canal() {
        let u = "S-1-5-21-1-2-3-1001";
        assert_eq!(
            droits_canal(u, true, true),
            format!("O:SYD:P(D;;0x1f019f;;;NU)(A;;0x12019b;;;{u})(A;;0x1f019f;;;SY)")
        );
        assert_eq!(
            droits_canal(u, true, false),
            format!("O:BAD:P(D;;0x1f019f;;;NU)(A;;0x12019b;;;{u})(A;;0x1f019f;;;BA)")
        );
        assert_eq!(
            droits_canal(u, false, false),
            format!("D:P(D;;0x1f019f;;;NU)(A;;0x12019f;;;{u})(A;;0x1f019f;;;BA)")
        );
    }
}
