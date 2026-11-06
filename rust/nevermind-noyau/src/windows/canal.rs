//! Le canal nommé de l'Aide, côté Nevermind (`AideClient`, natif.cs) : savoir si elle écoute (`Joignable`), lui
//! envoyer une demande et lire sa réponse (`Appeler`), en vérifiant que c'est bien elle (`ServeurAccepte` :
//! aide_regles::serveur).
//!
//! L'échange : une ligne JSON envoyée (UTF-8 sans BOM, finie par « \r\n » comme `StreamWriter.WriteLine`), une ligne
//! JSON reçue. Connexion anonyme (l'Aide ne peut pas prendre l'identité de Nevermind).
//!
//! [`FauxServeur`] : une fausse Aide pour les tests (un canal créé par le test lui-même, à un nom que la vraie Aide
//! n'écoute jamais). Jamais utilisée par l'appli.

use super::{MemoireLocale, Poignee, message, sid_texte};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE,
};
use windows::Win32::Security::Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT};
use windows::Win32::Security::{OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE, FlushFileBuffers, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    ReadFile, SECURITY_ANONYMOUS, SECURITY_SQOS_PRESENT, WriteFile,
};
use windows::Win32::System::IO::CancelIoEx;
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::core::{HSTRING, PWSTR};

fn chemin_canal(nom: &str) -> HSTRING {
    HSTRING::from(format!("\\\\.\\pipe\\{nom}"))
}

/// L'Aide écoute-t-elle ? (sans s'y connecter ; `WaitNamedPipe`, au moins 1 ms)
pub fn joignable(nom: &str, ms: u32) -> bool {
    // SAFETY : un nom terminé par un zéro, vivant pendant l'appel
    unsafe { WaitNamedPipeW(&chemin_canal(nom), ms.max(1)).as_bool() }
}

/// Le chemin complet du programme d'un processus (vide si illisible)
fn chemin_processus(pid: u32) -> String {
    // SAFETY : ouverture en lecture limitée, refermée par Poignee ; tampon et taille cohérents
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let h = Poignee(h);
        let mut b = vec![0u16; 1024];
        let mut n = b.len() as u32;
        match QueryFullProcessImageNameW(h.0, PROCESS_NAME_WIN32, PWSTR(b.as_mut_ptr()), &mut n) {
            Ok(()) => String::from_utf16_lossy(&b[..n as usize]),
            Err(_) => String::new(),
        }
    }
}

/// Le propriétaire (SID) d'un canal ouvert (vide si illisible)
fn proprietaire(h: HANDLE) -> String {
    let mut o = PSID::default();
    let mut psd = PSECURITY_DESCRIPTOR::default();
    // SAFETY : lecture du propriétaire ; le descripteur rendu est libéré par MemoireLocale
    unsafe {
        let r = GetSecurityInfo(
            h,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            Some(&mut o),
            None,
            None,
            None,
            Some(&mut psd),
        );
        let _m = MemoireLocale(psd.0);
        if r.is_err() {
            return String::new();
        }
        sid_texte(o).unwrap_or_default()
    }
}

/// Une poignée gardée comme un nombre, pour le fil qui lit la réponse
#[derive(Clone, Copy)]
struct Brute(usize);

/// Ouvre le canal (comme `NamedPipeClientStream.Connect(ms)` : réessayé tant qu'il est occupé ou absent)
fn connecter(nom: &str, connexion_ms: u32) -> Result<Poignee, String> {
    let chemin = chemin_canal(nom);
    let fin = Instant::now() + Duration::from_millis(u64::from(connexion_ms));
    loop {
        // SAFETY : un nom terminé par un zéro ; connexion anonyme (l'Aide ne prend pas l'identité de Nevermind)
        let r = unsafe {
            CreateFileW(
                &chemin,
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(SECURITY_SQOS_PRESENT.0 | SECURITY_ANONYMOUS.0),
                None,
            )
        };
        match r {
            Ok(h) => return Ok(Poignee(h)),
            Err(e) => {
                let code = (e.code().0 & 0xFFFF) as u32;
                if code != ERROR_PIPE_BUSY.0 && code != ERROR_FILE_NOT_FOUND.0 {
                    return Err(message(&e));
                }
            }
        }
        let reste = fin.saturating_duration_since(Instant::now());
        if reste.is_zero() {
            return Err("Le délai de connexion à l'Aide est dépassé.".into());
        }
        if !joignable(nom, reste.as_millis().min(u128::from(u32::MAX)) as u32) {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Lit une ligne (jusqu'à « \n », sans « \r »). `None` : l'autre a coupé sans rien envoyer.
fn lire_ligne(h: HANDLE) -> Option<String> {
    let mut tout = Vec::new();
    let mut b = [0u8; 4096];
    loop {
        let mut n = 0u32;
        // SAFETY : tampon à nous, de sa taille
        let ok = unsafe { ReadFile(h, Some(&mut b), Some(&mut n), None) }.is_ok();
        if !ok || n == 0 {
            break;
        }
        tout.extend_from_slice(&b[..n as usize]);
        if tout.contains(&b'\n') {
            break;
        }
    }
    if tout.is_empty() {
        return None;
    }
    let fin = tout.iter().position(|&o| o == b'\n').unwrap_or(tout.len());
    let mut l = &tout[..fin];
    if l.last() == Some(&b'\r') {
        l = &l[..l.len() - 1];
    }
    Some(String::from_utf8_lossy(l).into_owned())
}

fn ecrire_tout(h: HANDLE, o: &[u8]) -> Result<(), String> {
    let mut fait = 0;
    while fait < o.len() {
        let mut n = 0u32;
        // SAFETY : tampon à nous, de sa taille
        unsafe { WriteFile(h, Some(&o[fait..]), Some(&mut n), None) }.map_err(|e| message(&e))?;
        if n == 0 {
            return Err("canal fermé".into());
        }
        fait += n as usize;
    }
    // SAFETY : notre poignée
    let _ = unsafe { FlushFileBuffers(h) };
    Ok(())
}

/// Envoie une demande à l'Aide et rend sa réponse (une ligne JSON), comme `AideClient.Appeler`. Une erreur si l'Aide
/// ne répond pas, ou si celui qui répond n'est pas elle (`exe_attendu` : NevermindAide.exe de cette copie).
pub fn appeler(
    nom: &str,
    exe_attendu: &str,
    demande: &str,
    connexion_ms: u32,
    reponse_ms: u32,
) -> Result<String, String> {
    let c = connecter(nom, connexion_ms)?;
    let mut pid = 0u32;
    // SAFETY : notre poignée du canal, un nombre à nous
    let serveur = if unsafe { GetNamedPipeServerProcessId(c.0, &mut pid) }.is_ok() {
        chemin_processus(pid)
    } else {
        String::new()
    };
    let proprio = proprietaire(c.0);
    if !aide_regles::serveur::accepte(&serveur, &proprio, exe_attendu) {
        let qui = if serveur.is_empty() {
            format!("inconnu, canal de {}", if proprio.is_empty() { "?" } else { &proprio })
        } else {
            serveur
        };
        return Err(format!("Le programme qui répond à la place de l'Aide n'est pas Nevermind ({qui})."));
    }
    ecrire_tout(c.0, format!("{demande}\r\n").as_bytes())?;
    let brute = Brute(c.0.0 as usize);
    let (tx, rx) = mpsc::channel();
    let fil = std::thread::spawn(move || {
        let h = HANDLE(brute.0 as *mut _);
        let _ = tx.send(lire_ligne(h));
    });
    let r = match rx.recv_timeout(Duration::from_millis(u64::from(reponse_ms.max(1)))) {
        Ok(Some(l)) => Ok(l),
        Ok(None) => Err("L'Aide a coupé l'échange sans répondre.".to_string()),
        Err(_) => {
            // SAFETY : notre poignée ; la lecture en cours est abandonnée avant que la poignée soit refermée
            let _ = unsafe { CancelIoEx(c.0, None) };
            Err("L'Aide n'a pas répondu à temps.".to_string())
        }
    };
    let _ = fil.join();
    drop(c);
    r
}

/// Ce que la fausse Aide répond à une demande (la ligne reçue) : `Some(ligne)` ; `Some("")` pour couper l'échange
/// sans répondre ; `None` pour ne rien répondre (l'Aide qui ne répond pas à temps).
pub type Repondre = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Une fausse Aide pour les tests : un canal créé ici, au nom donné (celui d'une copie d'essai, que la vraie Aide
/// n'écoute jamais). Elle note chaque demande reçue. Arrêtée quand on la lâche.
pub struct FauxServeur {
    nom: String,
    arret: Arc<AtomicBool>,
    fil: Option<std::thread::JoinHandle<()>>,
    /// Les demandes reçues (lignes JSON, telles qu'envoyées par Nevermind)
    pub recues: Arc<Mutex<Vec<String>>>,
}

fn creer_instance(nom: &str) -> Result<Poignee, String> {
    // SAFETY : un nom terminé par un zéro ; droits par défaut (ceux du compte qui fait le test)
    let h = unsafe {
        CreateNamedPipeW(
            &chemin_canal(nom),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            65536,
            65536,
            0,
            None,
        )
    };
    if h.is_invalid() {
        return Err(message(&windows::core::Error::from_thread()));
    }
    Ok(Poignee(h))
}

impl FauxServeur {
    /// Lance la fausse Aide sur ce canal.
    pub fn lancer(nom: &str, repondre: Repondre) -> Result<FauxServeur, String> {
        let premiere = creer_instance(nom)?;
        let arret = Arc::new(AtomicBool::new(false));
        let recues = Arc::new(Mutex::new(Vec::new()));
        let (n, a, r) = (nom.to_string(), arret.clone(), recues.clone());
        let fil = std::thread::spawn(move || {
            let mut courante = premiere;
            loop {
                // SAFETY : notre exemplaire du canal ; attente bloquante d'un client
                let c = unsafe { ConnectNamedPipe(courante.0, None) };
                if let Err(e) = c
                    && (e.code().0 & 0xFFFF) as u32 != ERROR_PIPE_CONNECTED.0
                {
                    break;
                }
                if a.load(Ordering::SeqCst) {
                    break;
                }
                // (l'exemplaire suivant est prêt avant de répondre : la fausse Aide reste « joignable »)
                let Ok(suivante) = creer_instance(&n) else { break };
                let h = courante.0;
                if let Some(ligne) = lire_ligne(h) {
                    r.lock().unwrap_or_else(|e| e.into_inner()).push(ligne.clone());
                    match repondre(&ligne) {
                        Some(rep) if rep.is_empty() => {}
                        Some(rep) => {
                            let _ = ecrire_tout(h, format!("{rep}\r\n").as_bytes());
                        }
                        None => {
                            // (pas de réponse : on attend que Nevermind coupe)
                            while lire_ligne(h).is_some() {}
                        }
                    }
                }
                // SAFETY : notre exemplaire, déconnecté avant d'être refermé
                unsafe {
                    let _ = DisconnectNamedPipe(h);
                }
                courante = suivante;
            }
        });
        Ok(FauxServeur { nom: nom.to_string(), arret, fil: Some(fil), recues })
    }

    /// Les demandes reçues jusqu'ici
    pub fn demandes(&self) -> Vec<String> {
        self.recues.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Drop for FauxServeur {
    fn drop(&mut self) {
        self.arret.store(true, Ordering::SeqCst);
        // (une connexion pour réveiller le fil qui attend un client)
        let _ = connecter(&self.nom, 500);
        if let Some(f) = self.fil.take() {
            let _ = f.join();
        }
    }
}
