//! Le canal nommé de l'Aide (`AideTuyau`, NevermindAide.ps1) : la première connexion est créée avec
//! FILE_FLAG_FIRST_PIPE_INSTANCE (Windows refuse si un autre programme a déjà un canal de ce nom) ; les suivantes
//! rejoignent ce canal-là, avec ses droits, et leur propriétaire est vérifié. Réseau refusé
//! (PIPE_REJECT_REMOTE_CLIENTS), 32 connexions au plus, 64 Ko dans chaque sens.

use super::outils::{MemoireLocale, Poignee, message};
use super::processus::sid_texte;
use crate::systeme::{Connexion, Ecoute, ErreurCanal};
use std::sync::mpsc;
use std::time::Duration;
use windows::Win32::Foundation::{
    DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, HANDLE,
    WAIT_OBJECT_0, WIN32_ERROR,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1, SE_KERNEL_OBJECT,
};
use windows::Win32::Security::{OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FlushFileBuffers, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::Win32::System::Threading::{CreateEventW, GetCurrentProcess, ResetEvent, WaitForSingleObject};
use windows::core::HSTRING;

const TAILLE: u32 = 65536;

/// Une opération en cours sur le canal : sa structure OVERLAPPED reste à la même place tant que Windows s'en sert
struct Operation {
    ov: Box<OVERLAPPED>,
    evenement: Poignee,
}

// SAFETY : la structure OVERLAPPED et l'événement ne servent qu'au fil qui tient la connexion
unsafe impl Send for Operation {}

impl Operation {
    fn nouvelle() -> Result<Self, String> {
        // SAFETY : événement sans nom, refermé par Poignee
        let e = unsafe { CreateEventW(None, true, false, None) }.map_err(|e| message(&e))?;
        let mut ov = Box::<OVERLAPPED>::default();
        ov.hEvent = e;
        Ok(Operation { ov, evenement: Poignee(e) })
    }

    fn preparer(&mut self) {
        let e = self.ov.hEvent;
        *self.ov = OVERLAPPED::default();
        self.ov.hEvent = e;
        // SAFETY : notre événement
        unsafe {
            let _ = ResetEvent(self.evenement.0);
        }
    }

    /// Attend la fin de l'opération au plus `ms` : `Some(octets)` si elle est finie
    fn attendre(&mut self, h: HANDLE, ms: u32) -> Result<Option<u32>, WIN32_ERROR> {
        // SAFETY : l'opération a été lancée sur `h` avec cette structure, qui n'a pas bougé
        unsafe {
            if WaitForSingleObject(self.evenement.0, ms) != WAIT_OBJECT_0 {
                return Ok(None);
            }
            let mut n = 0u32;
            match GetOverlappedResult(h, &*self.ov, &mut n, false) {
                Ok(()) => Ok(Some(n)),
                Err(e) => Err(WIN32_ERROR::from_error(&e).unwrap_or_default()),
            }
        }
    }

    /// Abandonne l'opération et attend que Windows ait fini de s'en servir
    fn annuler(&mut self, h: HANDLE) {
        // SAFETY : même poignée et même structure que l'opération lancée
        unsafe {
            let _ = CancelIoEx(h, Some(&*self.ov));
            let mut n = 0u32;
            let _ = GetOverlappedResult(h, &*self.ov, &mut n, true);
        }
    }
}

/// Un exemplaire du canal, en attente d'un client
pub struct EcouteWindows {
    tuyau: Option<Poignee>,
    op: Operation,
    en_attente: bool,
    connecte: bool,
}

/// Crée un exemplaire du canal
pub fn creer(nom: &str, sddl: &str, premier: bool, proprietaire: Option<&str>) -> Result<EcouteWindows, ErreurCanal> {
    let indisponible = |m: String| ErreurCanal::Indisponible(m);
    let mut sd = PSECURITY_DESCRIPTOR::default();
    // SAFETY : descripteur créé par Windows à partir du texte SDDL, libéré par MemoireLocale
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(&HSTRING::from(sddl), SDDL_REVISION_1, &mut sd, None)
            .map_err(|e| indisponible(message(&e)))?;
    }
    let _sd = MemoireLocale(sd.0);
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: false.into(),
    };
    let mut mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
    if premier {
        mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    // SAFETY : nom et attributs valables pendant l'appel
    let h = unsafe {
        CreateNamedPipeW(
            &HSTRING::from(format!("\\\\.\\pipe\\{nom}")),
            mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            32,
            TAILLE,
            TAILLE,
            0,
            Some(&sa),
        )
    };
    if h.is_invalid() {
        return Err(indisponible(message(&windows::core::Error::from_thread())));
    }
    let tuyau = Poignee(h);
    // Avec les droits administrateur, chaque exemplaire suivant est vérifié : propriétaire le système (ou les
    // administrateurs), sinon le canal a été repris par un autre programme entre-temps
    if let Some(attendu) = proprietaire {
        let mut o = PSID::default();
        let mut psd = PSECURITY_DESCRIPTOR::default();
        // SAFETY : lecture du propriétaire ; le descripteur rendu est libéré par MemoireLocale
        let lu = unsafe {
            let r = GetSecurityInfo(
                tuyau.0,
                SE_KERNEL_OBJECT,
                OWNER_SECURITY_INFORMATION,
                Some(&mut o),
                None,
                None,
                None,
                Some(&mut psd),
            );
            let _m = MemoireLocale(psd.0);
            if r.is_ok() { sid_texte(o) } else { None }
        };
        let lu = lu.unwrap_or_default();
        if lu != attendu {
            return Err(ErreurCanal::Repris(format!("propriétaire {lu}")));
        }
    }
    let op = Operation::nouvelle().map_err(indisponible)?;
    Ok(EcouteWindows { tuyau: Some(tuyau), op, en_attente: false, connecte: false })
}

impl Ecoute for EcouteWindows {
    type C = ConnexionWindows;

    fn attendre(&mut self, ms: u32) -> Result<bool, String> {
        let Some(t) = &self.tuyau else { return Err("canal fermé".into()) };
        let h = t.0;
        if !self.en_attente && !self.connecte {
            self.op.preparer();
            // SAFETY : la structure OVERLAPPED reste en place (Box) jusqu'à la fin de l'attente
            match unsafe { ConnectNamedPipe(h, Some(&mut *self.op.ov)) } {
                Ok(()) => self.connecte = true,
                Err(e) => match WIN32_ERROR::from_error(&e) {
                    Some(ERROR_IO_PENDING) => self.en_attente = true,
                    Some(ERROR_PIPE_CONNECTED) => self.connecte = true,
                    _ => return Err(message(&e)),
                },
            }
        }
        if self.en_attente {
            match self.op.attendre(h, ms) {
                Ok(Some(_)) => {
                    self.en_attente = false;
                    self.connecte = true;
                }
                Ok(None) => {}
                Err(_) => {
                    // (un client venu et reparti aussitôt : on attend le suivant)
                    self.en_attente = false;
                }
            }
        }
        Ok(self.connecte)
    }

    fn connexion(mut self) -> ConnexionWindows {
        let tuyau = self.tuyau.take();
        let mut client = 0u32;
        if let Some(t) = &tuyau {
            // SAFETY : canal connecté
            unsafe {
                let _ = GetNamedPipeClientProcessId(t.0, &mut client);
            }
        }
        ConnexionWindows { tuyau, client, op: Operation::nouvelle().ok(), limite_vidage: LIMITE_VIDAGE }
    }
}

/// Le temps laissé au client pour lire la réponse, une fois écrite (Nevermind la lit tout de suite)
const LIMITE_VIDAGE: Duration = Duration::from_secs(10);

impl Drop for EcouteWindows {
    fn drop(&mut self) {
        if self.en_attente
            && let Some(t) = &self.tuyau
        {
            self.op.annuler(t.0);
        }
    }
}

/// Une connexion d'un client
pub struct ConnexionWindows {
    tuyau: Option<Poignee>,
    client: u32,
    op: Option<Operation>,
    /// Le temps laissé au client pour lire la réponse (`LIMITE_VIDAGE`, plus court dans les essais)
    limite_vidage: Duration,
}

impl Connexion for ConnexionWindows {
    fn client(&self) -> u32 {
        self.client
    }

    fn lire(&mut self, tampon: &mut [u8], ms: u32) -> Result<Option<usize>, String> {
        let (Some(t), Some(op)) = (&self.tuyau, self.op.as_mut()) else { return Err("canal fermé".into()) };
        let h = t.0;
        op.preparer();
        // SAFETY : le tampon et la structure OVERLAPPED restent en place jusqu'à la fin de la lecture (attendue ou
        // annulée avant de rendre la main)
        let r = unsafe { ReadFile(h, Some(tampon), None, Some(&mut *op.ov)) };
        if let Err(e) = r {
            match WIN32_ERROR::from_error(&e) {
                Some(ERROR_IO_PENDING) => {}
                Some(ERROR_BROKEN_PIPE) => return Ok(Some(0)),
                _ => return Err(message(&e)),
            }
        }
        match op.attendre(h, ms) {
            Ok(Some(n)) => Ok(Some(n as usize)),
            Ok(None) => {
                op.annuler(h);
                Ok(None)
            }
            Err(ERROR_BROKEN_PIPE) => Ok(Some(0)),
            Err(e) => Err(format!("erreur {} en lisant la demande", e.0)),
        }
    }

    fn ecrire(&mut self, octets: &[u8]) -> Result<(), String> {
        let (Some(t), Some(op)) = (&self.tuyau, self.op.as_mut()) else { return Err("canal fermé".into()) };
        let h = t.0;
        let mut fait = 0;
        while fait < octets.len() {
            op.preparer();
            // SAFETY : comme pour la lecture
            let r = unsafe { WriteFile(h, Some(&octets[fait..]), None, Some(&mut *op.ov)) };
            if let Err(e) = r
                && WIN32_ERROR::from_error(&e) != Some(ERROR_IO_PENDING)
            {
                return Err(message(&e));
            }
            match op.attendre(h, 60_000) {
                Ok(Some(n)) if n > 0 => fait += n as usize,
                Ok(_) => {
                    op.annuler(h);
                    return Err("le client ne lit pas la réponse".into());
                }
                Err(e) => return Err(format!("erreur {} en écrivant la réponse", e.0)),
            }
        }
        vider(h, self.limite_vidage)
    }
}

/// Attend que le client ait tout lu (FlushFileBuffers, comme WaitForPipeDrain), mais pas plus de `limite` (second audit
/// du 09/11, P3) : FlushFileBuffers n'a pas de délai, et un client qui envoie une demande puis ne lit jamais la réponse
/// bloquait ce fil pour toujours (l'Aide en SYSTEM ne s'arrêtait plus, et 31 clients ainsi prenaient toutes les
/// connexions). L'attente se fait dans un fil à part, sur une copie de la poignée ; passé le délai, le canal est
/// déconnecté (la réponse non lue est perdue), ce qui finit aussi l'attente de ce fil.
fn vider(h: HANDLE, limite: Duration) -> Result<(), String> {
    let mut copie = HANDLE::default();
    // SAFETY : copie de notre poignée dans notre propre processus, refermée par le fil qui attend (Poignee)
    unsafe {
        DuplicateHandle(GetCurrentProcess(), h, GetCurrentProcess(), &mut copie, 0, false, DUPLICATE_SAME_ACCESS)
            .map_err(|e| message(&e))?;
    }
    // (une poignée n'est qu'un numéro : il passe au fil sous cette forme, et redevient la Poignee qui le referme)
    let brut = copie.0 as usize;
    let (fini, attente) = mpsc::channel();
    let fil = std::thread::spawn(move || {
        let p = Poignee(HANDLE(brut as *mut core::ffi::c_void));
        // SAFETY : la copie appartient à ce fil
        let r = unsafe { FlushFileBuffers(p.0) }.map_err(|e| message(&e));
        let _ = fini.send(r);
    });
    match attente.recv_timeout(limite) {
        Ok(r) => {
            let _ = fil.join();
            r
        }
        Err(_) => {
            // SAFETY : notre canal ; déconnecté, il rend aussi la main à FlushFileBuffers
            unsafe {
                let _ = DisconnectNamedPipe(h);
            }
            // (le fil finit tout de suite ; s'il ne le faisait pas, il est laissé : cette connexion, elle, se termine)
            if attente.recv_timeout(Duration::from_secs(5)).is_ok() {
                let _ = fil.join();
            }
            Err("le client ne lit pas la réponse".into())
        }
    }
}

#[cfg(test)]
mod essais {
    use super::*;
    use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
    use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE, OPEN_EXISTING};

    /// Un canal de l'essai (nom à lui, pour le seul compte qui le lance) et son client connecté
    fn canal_et_client(nom: &str) -> (ConnexionWindows, Poignee) {
        let sddl = format!("D:P(A;;GA;;;{})", super::super::processus::moi().0);
        let mut e = creer(nom, &sddl, true, None).map_err(|e| format!("{e:?}")).unwrap();
        // SAFETY : ouverture du canal de l'essai, refermée par Poignee
        let h = unsafe {
            CreateFileW(
                &HSTRING::from(format!(r"\\.\pipe\{nom}")),
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .unwrap();
        assert!(e.attendre(2000).unwrap(), "le client de l'essai ne s'est pas connecté");
        (e.connexion(), Poignee(h))
    }

    /// Écrit la réponse dans un fil à part : ce qu'`ecrire` rend, s'il rend dans les `ms` millisecondes
    fn ecrire_au_plus(mut c: ConnexionWindows, ms: u64) -> Option<Result<(), String>> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(c.ecrire(b"{\"ok\":true}\n"));
        });
        rx.recv_timeout(Duration::from_millis(ms)).ok()
    }

    /// Second audit du 09/11 (P3), sur un vrai canal de l'essai (rien de l'Aide, aucun droit) : un client qui envoie
    /// une demande et ne lit jamais la réponse ne bloque plus le fil de sa connexion (avant : FlushFileBuffers attendait
    /// pour toujours). Passé le délai, l'écriture rend une erreur et le canal est coupé.
    #[test]
    fn un_client_qui_ne_lit_pas_ne_bloque_plus_la_connexion() {
        let nom = format!("Nevermind-Essai-Vidage-{}-{}", std::process::id(), line!());
        let (mut c, client) = canal_et_client(&nom);
        c.limite_vidage = Duration::from_millis(300);
        let r = ecrire_au_plus(c, 5000).expect("l'écriture de la réponse ne rend pas la main");
        assert_eq!(r, Err("le client ne lit pas la réponse".into()));
        // (le canal est coupé : le client ne lit plus rien)
        let mut b = [0u8; 64];
        let mut n = 0u32;
        // SAFETY : lecture sur la poignée du client de l'essai
        assert!(unsafe { ReadFile(client.0, Some(&mut b), Some(&mut n), None) }.is_err());
    }

    /// Contre-épreuve : un client qui lit reçoit toute la réponse, et l'écriture rend « ok »
    #[test]
    fn un_client_qui_lit_recoit_la_reponse() {
        let nom = format!("Nevermind-Essai-Vidage-{}-{}", std::process::id(), line!());
        let (c, client) = canal_et_client(&nom);
        let brut = client.0.0 as usize;
        let lecteur = std::thread::spawn(move || {
            let mut b = [0u8; 64];
            let mut n = 0u32;
            // SAFETY : lecture sur la poignée du client de l'essai, gardée ouverte jusqu'à la fin du test
            let _ = unsafe { ReadFile(HANDLE(brut as *mut core::ffi::c_void), Some(&mut b), Some(&mut n), None) };
            b[..n as usize].to_vec()
        });
        assert_eq!(ecrire_au_plus(c, 5000), Some(Ok(())));
        assert_eq!(lecteur.join().unwrap(), b"{\"ok\":true}\n");
        drop(client);
    }
}
