//! Les appels de Windows des mesures continues (étape 1b) : PresentMon (sa signature, son lancement, sa sortie), la
//! trace réseau ETW (StartTrace, OpenTrace, ProcessTrace : portés de la classe `NetFlow` de natif.cs), les compteurs
//! TCP (GetExtendedTcpTable, Get/SetPerTcpConnectionEStats : classe `TrafficMon`).
//!
//! Le piège connu : les dates des événements de la trace. Le lecteur en temps réel les reçoit en heure système
//! (FILETIME), converties par Windows tant que `PROCESS_TRACE_MODE_RAW_TIMESTAMP` n'est pas demandé (il ne l'est pas),
//! et l'heure de comparaison est `filetime()`, la même horloge. Jamais QueryPerformanceCounter (bug du 15/10).

use super::outils::texte;
use crate::mesures::fps::Decoupeur;
use crate::systeme::{EvenementReseau, Lance, LigneTcp, RappelPm, RappelTrace, RefusTrace, SortiePm, Trace};
use std::io::Read;
use std::net::IpAddr;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::process::CommandExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, FILETIME, GetLastError, HWND};
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetExtendedTcpTable, GetPerTcp6ConnectionEStats, GetPerTcpConnectionEStats, GetUnicastIpAddressTable,
    MIB_TCP_STATE, MIB_TCP6ROW, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_LH, MIB_TCPROW_LH_0, MIB_TCPROW_OWNER_PID,
    MIB_UNICASTIPADDRESS_TABLE, SetPerTcp6ConnectionEStats, SetPerTcpConnectionEStats, TCP_ESTATS_DATA_ROD_v0,
    TCP_TABLE_OWNER_PID_ALL, TcpConnectionEstatsData,
};
use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC, IN6_ADDR, IN6_ADDR_0};
use windows::Win32::Security::Cryptography::{
    CERT_NAME_STR_REVERSE_FLAG, CERT_STRING_TYPE, CERT_X500_NAME_STR, CertNameToStrW, PKCS_7_ASN_ENCODING,
    X509_ASN_ENCODING,
};
use windows::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO, WTD_CHOICE_FILE,
    WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvCertFromChain,
    WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
};
use windows::Win32::System::Diagnostics::Etw::{
    CONTROLTRACE_HANDLE, CloseTrace, ControlTraceW, EVENT_RECORD, EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_LOGFILEW,
    EVENT_TRACE_LOGFILEW_0, EVENT_TRACE_LOGFILEW_1, EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE, EnableTraceEx2,
    OpenTraceW, PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_REAL_TIME, PROCESSTRACE_HANDLE, ProcessTrace,
    StartTraceW, WNODE_FLAG_TRACED_GUID,
};
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows::core::{GUID, HSTRING, PCWSTR, PWSTR};

// ---------------------------------------------------------------------------------------------------------------
// PresentMon
// ---------------------------------------------------------------------------------------------------------------

/// L'état d'une signature, avec les noms de PowerShell (`Get-AuthenticodeSignature`, champ Status), que le message
/// de refus montre
fn etat_signature(code: i32) -> &'static str {
    match code as u32 {
        0 => "Valid",
        0x8009_0008 => "Incompatible",               // NTE_BAD_ALGID
        0x800B_0100 => "NotSigned",                  // TRUST_E_NOSIGNATURE
        0x8009_6010 | 0x8009_200D => "HashMismatch", // TRUST_E_BAD_DIGEST, CRYPT_E_BAD_MSG
        0x800B_0001 => "NotSupportedFileFormat",     // TRUST_E_PROVIDER_UNKNOWN
        0x800B_0111 => "NotTrusted",                 // TRUST_E_EXPLICIT_DISTRUST
        _ => "UnknownError",
    }
}

/// La signature Authenticode d'un fichier : son état (noms de PowerShell) et le nom complet de celui qui a signé
/// (« CN=Intel Corporation, O=Intel Corporation, ... », comme `SignerCertificate.Subject` de .NET)
fn signature(chemin: &str) -> (&'static str, String) {
    let nom = HSTRING::from(chemin);
    let mut fichier = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(nom.as_ptr()),
        ..Default::default()
    };
    let mut donnees = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut fichier },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY : structures locales, vivantes pendant les appels ; l'état gardé par Windows est rendu à la fin
    // (WTD_STATEACTION_CLOSE) ; le certificat est lu dans les données de Windows avant ce rendu
    unsafe {
        let code = WinVerifyTrust(HWND::default(), &mut action, (&raw mut donnees).cast());
        let mut sujet = String::new();
        let prov = WTHelperProvDataFromStateData(donnees.hWVTStateData);
        if !prov.is_null() {
            let signataire = WTHelperGetProvSignerFromChain(prov, 0, false, 0);
            if !signataire.is_null() {
                let cert = WTHelperGetProvCertFromChain(signataire, 0);
                if !cert.is_null() && !(*cert).pCert.is_null() {
                    let info = (*(*cert).pCert).pCertInfo;
                    if !info.is_null() {
                        let sorte = CERT_STRING_TYPE(CERT_X500_NAME_STR.0 | CERT_NAME_STR_REVERSE_FLAG);
                        let enc = X509_ASN_ENCODING | PKCS_7_ASN_ENCODING;
                        let n = CertNameToStrW(enc, &(*info).Subject, sorte, None);
                        let mut b = vec![0u16; n as usize + 1];
                        CertNameToStrW(enc, &(*info).Subject, sorte, Some(&mut b));
                        sujet = texte(&b);
                    }
                }
            }
        }
        donnees.dwStateAction = WTD_STATEACTION_CLOSE;
        let _ = WinVerifyTrust(HWND::default(), &mut action, (&raw mut donnees).cast());
        (etat_signature(code), sujet)
    }
}

/// Le nom complet du signataire contient-il « CN=Intel Corporation » suivi d'une virgule ou de la fin
/// (`-match 'CN=Intel Corporation(,|$)'`, sans tenir compte des majuscules, comme PowerShell) ?
fn signe_par_intel(sujet: &str) -> bool {
    let s = aide_regles::texte::minuscules(sujet);
    let motif = "cn=intel corporation";
    s.match_indices(motif).any(|(i, _)| matches!(s[i + motif.len()..].chars().next(), None | Some(',')))
}

/// PresentMon du dossier de Nevermind (`Get-AidePresentMon`) : seulement s'il est bien signé par Intel
pub fn presentmon(racine: &str) -> Result<String, String> {
    let pm = format!("{racine}\\fichiers\\outils-tiers\\PresentMon.exe");
    if std::fs::metadata(&pm).is_err() {
        return Err("PresentMon est absent du dossier de Nevermind.".into());
    }
    // (ouvert en lecture seule pendant le contrôle : personne ne peut le remplacer à ce moment-là)
    let _ouvert = std::fs::OpenOptions::new().read(true).share_mode(1).open(&pm); // FILE_SHARE_READ
    let (etat, sujet) = signature(&pm);
    if etat != "Valid" || !signe_par_intel(&sujet) {
        return Err(format!("PresentMon n'est pas signé par Intel ({etat}) : l'Aide ne le lance pas."));
    }
    Ok(pm)
}

/// Un environnement neuf pour PresentMon (rien de l'utilisateur), comme pour les autres programmes lancés par l'Aide
fn commande(exe: &str, arguments: &str, env: &[(String, String)]) -> Command {
    let mut c = Command::new(exe);
    c.raw_arg(arguments).env_clear().creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    for (k, v) in env {
        c.env(k, v);
    }
    c
}

/// Lit un flot jusqu'à sa fin, ligne par ligne (comme .NET : voir Decoupeur)
fn lignes(mut flot: impl Read, rappel: impl Fn(String)) {
    let mut tampon = [0u8; 8192];
    let mut d = Decoupeur::default();
    loop {
        match flot.read(&mut tampon) {
            Ok(0) | Err(_) => break,
            Ok(n) => d.ajouter(&tampon[..n], &rappel),
        }
    }
    d.finir(&rappel);
}

/// PresentMon lancé
struct PresentMonLance(Arc<Mutex<Child>>);

impl Lance for PresentMonLance {
    fn arreter(&mut self) {
        let mut c = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(c.try_wait(), Ok(None)) {
            let _ = c.kill();
        }
    }
}

/// Lance PresentMon ; ses lignes arrivent dans `rappel`, puis la fin (une fois ses deux sorties fermées et lui arrêté)
pub fn lancer_presentmon(
    exe: &str,
    arguments: &str,
    env: &[(String, String)],
    rappel: RappelPm,
) -> Result<Box<dyn Lance>, String> {
    let mut c = commande(exe, arguments, env);
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut enfant = c.spawn().map_err(|e| e.to_string())?;
    let sortie = enfant.stdout.take();
    let erreurs = enfant.stderr.take();
    let enfant = Arc::new(Mutex::new(enfant));
    let r2 = Arc::clone(&rappel);
    let fil_erreurs = std::thread::spawn(move || {
        if let Some(e) = erreurs {
            lignes(e, |l| r2(SortiePm::Erreur(l)));
        }
    });
    let e2 = Arc::clone(&enfant);
    std::thread::spawn(move || {
        if let Some(s) = sortie {
            lignes(s, |l| rappel(SortiePm::Ligne(l)));
        }
        let _ = fil_erreurs.join();
        // (attend sa fin sans garder le verrou : l'arrêt doit pouvoir le prendre)
        loop {
            let fini = !matches!(e2.lock().unwrap_or_else(|e| e.into_inner()).try_wait(), Ok(None));
            if fini {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        rappel(SortiePm::Fin);
    });
    Ok(Box::new(PresentMonLance(enfant)))
}

/// Ferme la session de mesure « Nevermind » que PresentMon arrêté de force laisse ouverte (3 s au plus)
pub fn fermer_session_presentmon(exe: &str, env: &[(String, String)]) {
    if exe.is_empty() {
        return;
    }
    let mut c = commande(exe, "--terminate_existing_session --session_name Nevermind", env);
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let Ok(mut e) = c.spawn() else { return };
    let limite = Instant::now() + Duration::from_secs(3);
    while Instant::now() < limite && matches!(e.try_wait(), Ok(None)) {
        std::thread::sleep(Duration::from_millis(50));
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Trace réseau (ETW, fournisseur Microsoft-Windows-Kernel-Network)
// ---------------------------------------------------------------------------------------------------------------

const SESSION: &str = "Nevermind-Reseau";
/// Nom d'avant la 1.3.7 : une session restée ouverte sous ce nom après une mise à jour est fermée au démarrage
const ANCIENNE_SESSION: &str = "OptiGame-Reseau";
const FOURNISSEUR: GUID = GUID::from_u128(0x7DD42A49_5329_4832_8DFD_43D979153A88);
/// Taille des réglages d'une session : la structure, puis la place du nom
const TAILLE_PROPRIETES: usize = 120 + 1024;

/// Les réglages d'une session de trace (comme `NewProps` de natif.cs), dans un tampon aligné
fn proprietes() -> Vec<u64> {
    const _: () = assert!(size_of::<EVENT_TRACE_PROPERTIES>() == 120);
    let mut b = vec![0u64; TAILLE_PROPRIETES / 8];
    // SAFETY : le tampon est aligné et assez grand pour la structure
    let p = unsafe { &mut *(b.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>()) };
    p.Wnode.BufferSize = TAILLE_PROPRIETES as u32;
    p.Wnode.ClientContext = 1; // horloge QPC pour la session ; le lecteur reçoit quand même l'heure système
    p.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
    p.BufferSize = 64; // Ko
    p.LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
    p.FlushTimer = 1; // s
    p.LoggerNameOffset = 120;
    b
}

/// Arrête la session de ce nom (erreurs ignorées)
fn arreter_session(nom: &str) {
    let mut p = proprietes();
    // SAFETY : réglages dans un tampon aligné de la bonne taille
    unsafe {
        let _ = ControlTraceW(
            CONTROLTRACE_HANDLE::default(),
            &HSTRING::from(nom),
            p.as_mut_ptr().cast(),
            EVENT_TRACE_CONTROL_STOP,
        );
    }
}

/// Reçoit chaque événement de la trace (fil de ProcessTrace) et le passe au rappel rangé dans le contexte
unsafe extern "system" fn recevoir(ev: *mut EVENT_RECORD) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY : événement donné par Windows pendant l'appel ; le contexte est le rappel de la trace, qui vit
        // jusqu'à la fin de la lecture (TraceWindows::arreter)
        unsafe {
            let e = &*ev;
            if e.UserContext.is_null() {
                return;
            }
            let rappel = &*(e.UserContext as *const RappelTrace);
            let donnees = (!e.UserData.is_null())
                .then(|| std::slice::from_raw_parts(e.UserData as *const u8, usize::from(e.UserDataLength)));
            rappel(&EvenementReseau { id: e.EventHeader.EventDescriptor.Id, date: e.EventHeader.TimeStamp, donnees });
        }
    }));
}

/// Une trace en cours
struct TraceWindows {
    lecture: PROCESSTRACE_HANDLE,
    fil: Option<std::thread::JoinHandle<()>>,
    tourne: Arc<AtomicBool>,
    /// Le rappel (contexte des événements), le nom et la description de la lecture : gardés jusqu'à la fin
    contexte: *mut RappelTrace,
    _nom: Vec<u16>,
    _lecture: Box<EVENT_TRACE_LOGFILEW>,
}

// SAFETY : les pointeurs gardés ne sont touchés que par arreter (une fois), après la fin de la lecture
unsafe impl Send for TraceWindows {}

impl Trace for TraceWindows {
    fn tourne(&self) -> bool {
        self.tourne.load(Ordering::SeqCst)
    }

    fn arreter(&mut self) {
        arreter_session(SESSION);
        if self.lecture.Value != 0 && self.lecture.Value != u64::MAX {
            // SAFETY : poignée de lecture ouverte par OpenTraceW, fermée une seule fois
            unsafe {
                let _ = CloseTrace(self.lecture);
            }
            self.lecture.Value = 0;
        }
        let mut lu = true;
        if let Some(f) = self.fil.take() {
            let limite = Instant::now() + Duration::from_secs(2);
            while !f.is_finished() && Instant::now() < limite {
                std::thread::sleep(Duration::from_millis(20));
            }
            lu = f.is_finished();
        }
        self.tourne.store(false, Ordering::SeqCst);
        // (lecture finie : le rappel peut être rendu ; sinon il reste, plutôt qu'un événement sur une mémoire rendue)
        if lu && !self.contexte.is_null() {
            // SAFETY : créé par Box::into_raw dans demarrer_trace, plus utilisé par personne
            drop(unsafe { Box::from_raw(self.contexte) });
            self.contexte = std::ptr::null_mut();
        }
    }
}

impl Drop for TraceWindows {
    fn drop(&mut self) {
        if self.fil.is_some() {
            self.arreter();
        }
    }
}

/// Démarre la trace réseau ; sinon, l'appel de Windows qui a échoué et son code
pub fn demarrer_trace(rappel: RappelTrace) -> Result<Box<dyn Trace>, RefusTrace> {
    arreter_session(ANCIENNE_SESSION);
    let mut session = CONTROLTRACE_HANDLE::default();
    let nom = HSTRING::from(SESSION);
    let mut p = proprietes();
    // SAFETY : réglages dans un tampon aligné de la bonne taille, nom terminé par un zéro
    let mut r = unsafe { StartTraceW(&mut session, &nom, p.as_mut_ptr().cast()) };
    if r == ERROR_ALREADY_EXISTS {
        // (une session restée ouverte, Aide arrêtée brutalement : on la remplace)
        arreter_session(SESSION);
        p = proprietes();
        // SAFETY : comme plus haut
        r = unsafe { StartTraceW(&mut session, &nom, p.as_mut_ptr().cast()) };
    }
    if r.0 != 0 {
        return Err(RefusTrace::Session(r.0));
    }
    // (niveau 5, mots-clés 0x30 : envoi et réception)
    // SAFETY : session ouverte juste avant, GUID local
    let r = unsafe { EnableTraceEx2(session, &FOURNISSEUR, 1, 5, 0x30, 0, 0, None) };
    if r.0 != 0 {
        arreter_session(SESSION);
        return Err(RefusTrace::Fournisseur(r.0));
    }

    let contexte = Box::into_raw(Box::new(rappel));
    let mut nom_lecture: Vec<u16> = SESSION.encode_utf16().chain([0]).collect();
    let mut lecture = Box::new(EVENT_TRACE_LOGFILEW {
        LoggerName: PWSTR(nom_lecture.as_mut_ptr()),
        Anonymous1: EVENT_TRACE_LOGFILEW_0 {
            ProcessTraceMode: PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD,
        },
        Anonymous2: EVENT_TRACE_LOGFILEW_1 { EventRecordCallback: Some(recevoir) },
        Context: contexte.cast(),
        ..Default::default()
    });
    // SAFETY : description de la lecture gardée vivante (Box) avec son nom, jusqu'à la fin de la trace
    let h = unsafe { OpenTraceW(&mut *lecture) };
    if h.Value == u64::MAX {
        // SAFETY : simple lecture de l'erreur de Windows
        let e = unsafe { GetLastError() };
        arreter_session(SESSION);
        // SAFETY : créé juste avant, jamais donné à Windows (OpenTrace a échoué)
        drop(unsafe { Box::from_raw(contexte) });
        return Err(RefusTrace::Lecture(e.0));
    }
    let tourne = Arc::new(AtomicBool::new(true));
    let t2 = Arc::clone(&tourne);
    let hv = h.Value;
    let fil = std::thread::spawn(move || {
        // SAFETY : poignée de lecture ouverte, fermée par arreter (ProcessTrace rend alors la main)
        unsafe {
            let _ = ProcessTrace(&[PROCESSTRACE_HANDLE { Value: hv }], None, None);
        }
        t2.store(false, Ordering::SeqCst);
    });
    Ok(Box::new(TraceWindows { lecture: h, fil: Some(fil), tourne, contexte, _nom: nom_lecture, _lecture: lecture }))
}

/// Les adresses de ce PC (toutes les cartes, bouclage compris)
pub fn adresses_locales() -> Vec<IpAddr> {
    let mut r = Vec::new();
    let mut t: *mut MIB_UNICASTIPADDRESS_TABLE = std::ptr::null_mut();
    // SAFETY : table donnée par Windows, lue dans ses limites puis rendue
    unsafe {
        if GetUnicastIpAddressTable(AF_UNSPEC, &mut t).is_err() || t.is_null() {
            return r;
        }
        let n = (*t).NumEntries as usize;
        for l in std::slice::from_raw_parts((*t).Table.as_ptr(), n) {
            if let Some(a) = super::reseau::adresse(&l.Address) {
                r.push(a);
            }
        }
        FreeMibTable(t.cast());
    }
    r
}

/// Maintenant, en heure système (FILETIME)
pub fn filetime() -> i64 {
    // SAFETY : simple lecture de l'horloge
    let f: FILETIME = unsafe { GetSystemTimePreciseAsFileTime() };
    ((u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)) as i64
}

// ---------------------------------------------------------------------------------------------------------------
// Compteurs TCP
// ---------------------------------------------------------------------------------------------------------------

/// La table des connexions TCP (avec le processus de chacune) ; trois essais si elle grandit entre-temps
pub fn table_tcp(v6: bool) -> Option<Vec<LigneTcp>> {
    let af = if v6 { AF_INET6.0 } else { AF_INET.0 } as u32;
    let mut taille = 0u32;
    // SAFETY : premier appel sans tampon, pour la taille
    unsafe {
        GetExtendedTcpTable(None, &mut taille, false, af, TCP_TABLE_OWNER_PID_ALL, 0);
    }
    for _ in 0..3 {
        taille += 4096;
        let mut b = vec![0u64; (taille as usize).div_ceil(8)];
        // SAFETY : tampon aligné d'au moins `taille` octets
        let r = unsafe {
            GetExtendedTcpTable(Some(b.as_mut_ptr().cast()), &mut taille, false, af, TCP_TABLE_OWNER_PID_ALL, 0)
        };
        if r != 0 {
            continue;
        }
        let n = b[0] as u32 as usize; // (dwNumEntries, puis les lignes à partir du 4e octet)
        let mut lignes = Vec::with_capacity(n);
        // SAFETY : Windows a écrit n lignes après le nombre ; lecture sans alignement
        unsafe {
            let debut = b.as_ptr().cast::<u8>().add(4);
            for i in 0..n {
                if v6 {
                    let l = std::ptr::read_unaligned(debut.cast::<MIB_TCP6ROW_OWNER_PID>().add(i));
                    lignes.push(LigneTcp {
                        v6: true,
                        etat: l.dwState,
                        adresse_locale: l.ucLocalAddr,
                        adresse_distante: l.ucRemoteAddr,
                        port_local: l.dwLocalPort,
                        port_distant: l.dwRemotePort,
                        zone_locale: l.dwLocalScopeId,
                        zone_distante: l.dwRemoteScopeId,
                        pid: l.dwOwningPid,
                    });
                } else {
                    let l = std::ptr::read_unaligned(debut.cast::<MIB_TCPROW_OWNER_PID>().add(i));
                    let mut a = [0u8; 16];
                    let mut d = [0u8; 16];
                    a[..4].copy_from_slice(&l.dwLocalAddr.to_ne_bytes());
                    d[..4].copy_from_slice(&l.dwRemoteAddr.to_ne_bytes());
                    lignes.push(LigneTcp {
                        v6: false,
                        etat: l.dwState,
                        adresse_locale: a,
                        adresse_distante: d,
                        port_local: l.dwLocalPort,
                        port_distant: l.dwRemotePort,
                        pid: l.dwOwningPid,
                        ..Default::default()
                    });
                }
            }
        }
        return Some(lignes);
    }
    None
}

fn ligne_v4(l: &LigneTcp) -> MIB_TCPROW_LH {
    let a = |o: &[u8; 16]| u32::from_ne_bytes([o[0], o[1], o[2], o[3]]);
    MIB_TCPROW_LH {
        Anonymous: MIB_TCPROW_LH_0 { dwState: l.etat },
        dwLocalAddr: a(&l.adresse_locale),
        dwLocalPort: l.port_local,
        dwRemoteAddr: a(&l.adresse_distante),
        dwRemotePort: l.port_distant,
    }
}

fn ligne_v6(l: &LigneTcp) -> MIB_TCP6ROW {
    MIB_TCP6ROW {
        State: MIB_TCP_STATE(l.etat as i32),
        LocalAddr: IN6_ADDR { u: IN6_ADDR_0 { Byte: l.adresse_locale } },
        dwLocalScopeId: l.zone_locale,
        dwLocalPort: l.port_local,
        RemoteAddr: IN6_ADDR { u: IN6_ADDR_0 { Byte: l.adresse_distante } },
        dwRemoteScopeId: l.zone_distante,
        dwRemotePort: l.port_distant,
    }
}

/// Demande à Windows de compter les octets de cette connexion (refusé sans droits administrateur)
pub fn tcp_compter(l: &LigneTcp) -> bool {
    let rw = [1u8]; // TCP_ESTATS_DATA_RW_v0 : EnableCollection
    // SAFETY : ligne locale, tampon d'un octet
    unsafe {
        if l.v6 {
            SetPerTcp6ConnectionEStats(&ligne_v6(l), TcpConnectionEstatsData, &rw, 0, 0) == 0
        } else {
            SetPerTcpConnectionEStats(&ligne_v4(l), TcpConnectionEstatsData, &rw, 0, 0) == 0
        }
    }
}

/// Octets envoyés et reçus de cette connexion
pub fn tcp_octets(l: &LigneTcp) -> Option<(u64, u64)> {
    let mut rod = TCP_ESTATS_DATA_ROD_v0::default();
    // SAFETY : la structure est vue comme un tampon de sa taille exacte
    let o = unsafe { std::slice::from_raw_parts_mut((&raw mut rod).cast::<u8>(), size_of::<TCP_ESTATS_DATA_ROD_v0>()) };
    // SAFETY : ligne locale, tampon de la bonne taille
    let r = unsafe {
        if l.v6 {
            GetPerTcp6ConnectionEStats(&ligne_v6(l), TcpConnectionEstatsData, None, 0, None, 0, Some(o), 0)
        } else {
            GetPerTcpConnectionEStats(&ligne_v4(l), TcpConnectionEstatsData, None, 0, None, 0, Some(o), 0)
        }
    };
    (r == 0).then_some((rod.DataBytesOut, rod.DataBytesIn))
}

// Essais sur le vrai Windows, sans droits et sans rien changer : signature de PresentMon (fichiers d'essai dans le
// dossier temporaire), lecture de la sortie d'un programme lancé, horloge, tables de connexions.
#[cfg(test)]
mod essais {
    use super::*;

    fn dossier_essai(nom: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("nevermind-aide-essai-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("fichiers").join("outils-tiers")).unwrap();
        d
    }

    #[test]
    fn presentmon_absent_mal_signe_ou_d_intel() {
        let d = dossier_essai("pm");
        let racine = d.to_string_lossy().into_owned();
        assert_eq!(presentmon(&racine).unwrap_err(), "PresentMon est absent du dossier de Nevermind.");
        // (un exe sans signature : celui de ces essais)
        let pm = d.join("fichiers").join("outils-tiers").join("PresentMon.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &pm).unwrap();
        assert_eq!(
            presentmon(&racine).unwrap_err(),
            "PresentMon n'est pas signé par Intel (NotSigned) : l'Aide ne le lance pas."
        );
        // Le vrai, celui du dépôt : signé par Intel
        let depot = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("Nevermind");
        let vrai = depot.join("fichiers").join("outils-tiers").join("PresentMon.exe");
        if vrai.exists() {
            std::fs::copy(&vrai, &pm).unwrap();
            assert_eq!(presentmon(&racine).unwrap(), pm.to_string_lossy());
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nom_du_signataire() {
        assert!(signe_par_intel("CN=Intel Corporation, O=Intel Corporation, L=Santa Clara, S=California, C=US"));
        assert!(signe_par_intel("C=US, CN=Intel Corporation"));
        assert!(!signe_par_intel("CN=Intel Corporation Fake, O=Pirate"));
        assert!(!signe_par_intel("CN=Microsoft Corporation, O=Intel Corporation"));
    }

    #[test]
    fn sortie_d_un_programme_lance() {
        let w = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let (envoi, reception) = std::sync::mpsc::channel();
        let rappel: RappelPm = Arc::new(move |s| {
            let _ = envoi.send(s);
        });
        let _l = lancer_presentmon(
            &format!("{w}\\System32\\cmd.exe"),
            "/d /c echo un& echo.& echo deux& echo erreur 1>&2",
            &[("SystemRoot".into(), w.clone())],
            rappel,
        )
        .unwrap();
        let mut sorties = Vec::new();
        while let Ok(s) = reception.recv_timeout(Duration::from_secs(10)) {
            let fin = s == SortiePm::Fin;
            sorties.push(s);
            if fin {
                break;
            }
        }
        let lignes: Vec<_> = sorties.iter().filter(|s| matches!(s, SortiePm::Ligne(_))).cloned().collect();
        assert_eq!(
            lignes,
            vec![SortiePm::Ligne("un".into()), SortiePm::Ligne("".into()), SortiePm::Ligne("deux".into())]
        );
        assert!(sorties.contains(&SortiePm::Erreur("erreur ".into())));
        assert_eq!(sorties.last(), Some(&SortiePm::Fin));
    }

    #[test]
    fn horloge_en_heure_systeme() {
        // FILETIME de maintenant, comparé à l'horloge de Rust (secondes depuis 1970 + 11 644 473 600 s)
        let rust = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let ft = filetime() / 10_000_000 - 11_644_473_600;
        assert!((ft - rust).abs() <= 2, "{ft} / {rust}");
    }

    #[test]
    fn tables_et_adresses_lues() {
        assert!(table_tcp(false).is_some());
        assert!(table_tcp(true).is_some());
        assert!(adresses_locales().contains(&IpAddr::from([127, 0, 0, 1])));
    }
}
