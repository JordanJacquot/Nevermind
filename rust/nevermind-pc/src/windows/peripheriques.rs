//! La lecture des périphériques Logitech (casque, souris, clavier) : leur batterie et leur état de charge, par des
//! QUESTIONS HID seulement (jamais un réglage, jamais les notifications), avec notre identifiant de logiciel (0x0C ;
//! G Hub prend 0x0D et peut rester ouvert). Délais courts : deux à trois secondes au plus par appareil. Appelée dans
//! un travail de fond, jamais sur le fil des blocs.
//!
//! - Souris et claviers (HID++ 2.0, collection 0xFF00 de 20 octets) : la fonction 0x1004 (UNIFIED_BATTERY) et 0x0005
//!   (nom et sorte). Derrière un récepteur : les cases 1 à 6 ; branché directement : 0xFF seulement.
//! - Casques PRO X 2 (protocole « Centurion », collection 0xFFA0 de 64 octets) : le pont du récepteur, puis
//!   BATTERY_SOC (0x0104) du casque.
//!
//! Le décodage des réponses est dans `crate::peripheriques` (fonctions pures, testées).

use crate::peripheriques::{self as p, Charge, Peripherique, Reponse, Sorte};
use std::time::{Duration, Instant};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};
use windows::Win32::Devices::HumanInterfaceDevice::{
    HIDD_ATTRIBUTES, HIDP_CAPS, HidD_FlushQueue, HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid,
    HidD_GetPreparsedData, HidD_GetProductString, HidP_GetCaps, PHIDP_PREPARSED_DATA,
};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    ReadFile, WriteFile,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResultEx, OVERLAPPED};
use windows::Win32::System::Threading::{CreateEventW, ResetEvent};
use windows::core::PCWSTR;

const LOGITECH: &str = "Logitech";

/// Une collection HID d'un appareil Logitech
struct Hid {
    chemin: String,
    pid: u16,
    page: u16,
    usage: u16,
    entree: u16,
    sortie: u16,
    nom: String,
}

/// Une poignée HID ; `acces` 0 : seulement pour lire ses attributs (les claviers et souris de Windows le permettent)
fn ouvrir(chemin: &str, acces: u32, flags: FILE_FLAGS_AND_ATTRIBUTES) -> Option<HANDLE> {
    let w: Vec<u16> = chemin.encode_utf16().chain([0]).collect();
    // SAFETY : chemin terminé par zéro, vivant pendant l'appel ; la poignée rendue est fermée par l'appelant
    unsafe {
        CreateFileW(PCWSTR(w.as_ptr()), acces, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, flags, None)
            .ok()
    }
}

/// Les collections HID présentes de Logitech (VID 046D)
fn lister() -> Vec<Hid> {
    let mut v = Vec::new();
    // SAFETY : la liste SetupDi est détruite à la fin ; le détail est lu dans un tampon aligné de la taille demandée,
    // le chemin y est terminé par zéro ; chaque poignée ouverte est fermée
    unsafe {
        let guid = HidD_GetHidGuid();
        let Ok(set) = SetupDiGetClassDevsW(Some(&guid), PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE)
        else {
            return v;
        };
        let mut i = 0;
        loop {
            let mut d =
                SP_DEVICE_INTERFACE_DATA { cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32, ..Default::default() };
            if SetupDiEnumDeviceInterfaces(set, None, &guid, i, &mut d).is_err() {
                break;
            }
            i += 1;
            let mut taille = 0u32;
            let _ = SetupDiGetDeviceInterfaceDetailW(set, &d, None, 0, Some(&mut taille), None);
            if taille == 0 {
                continue;
            }
            let mut buf = vec![0u64; (taille as usize).div_ceil(8) + 1];
            let det = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*det).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            if SetupDiGetDeviceInterfaceDetailW(set, &d, Some(det), taille, None, None).is_err() {
                continue;
            }
            let ptr = std::ptr::addr_of!((*det).DevicePath) as *const u16;
            let max = (taille as usize).saturating_sub(4) / 2;
            let mut n = 0;
            while n < max && *ptr.add(n) != 0 {
                n += 1;
            }
            let chemin = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, n));
            if !chemin.to_lowercase().contains("vid_046d") {
                continue;
            }
            let Some(h) = ouvrir(&chemin, 0, FILE_FLAGS_AND_ATTRIBUTES(0)) else { continue };
            let mut a = HIDD_ATTRIBUTES { Size: size_of::<HIDD_ATTRIBUTES>() as u32, ..Default::default() };
            let _ = HidD_GetAttributes(h, &mut a);
            let mut pp = PHIDP_PREPARSED_DATA::default();
            let mut caps = HIDP_CAPS::default();
            if HidD_GetPreparsedData(h, &mut pp) {
                let _ = HidP_GetCaps(pp, &mut caps);
                let _ = HidD_FreePreparsedData(pp);
            }
            let mut nom = [0u16; 128];
            let _ = HidD_GetProductString(h, nom.as_mut_ptr() as _, 256);
            let fin = nom.iter().position(|&c| c == 0).unwrap_or(0);
            let _ = CloseHandle(h);
            v.push(Hid {
                chemin,
                pid: a.ProductID,
                page: caps.UsagePage,
                usage: caps.Usage,
                entree: caps.InputReportByteLength,
                sortie: caps.OutputReportByteLength,
                nom: String::from_utf16_lossy(&nom[..fin]).trim().to_string(),
            });
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    v
}

/// Une collection ouverte pour poser des questions (lectures et écritures avec délai)
struct Canal {
    h: HANDLE,
    ev: HANDLE,
    sortie: usize,
    entree: usize,
}

impl Canal {
    fn nouveau(d: &Hid) -> Option<Self> {
        let h = ouvrir(&d.chemin, (GENERIC_READ | GENERIC_WRITE).0, FILE_FLAG_OVERLAPPED)?;
        // SAFETY : un événement à remise manuelle, fermé dans `drop` (avec la poignée, même en cas d'échec ici)
        let ev = match unsafe { CreateEventW(None, true, false, PCWSTR::null()) } {
            Ok(e) => e,
            Err(_) => {
                // SAFETY : poignée ouverte ci-dessus
                let _ = unsafe { CloseHandle(h) };
                return None;
            }
        };
        // SAFETY : poignée valide ; vide ce qui attendait d'être lu (des réponses à d'autres questions)
        let _ = unsafe { HidD_FlushQueue(h) };
        Some(Canal { h, ev, sortie: d.sortie as usize, entree: d.entree as usize })
    }

    /// Attendre la fin d'une opération ; sinon l'annuler (et attendre l'annulation : le tampon doit rester vivant)
    ///
    /// SAFETY (appelant) : `o` est l'opération en cours sur `self.h`, son tampon vit jusqu'au retour
    unsafe fn finir(&self, o: &OVERLAPPED, ms: u32) -> Option<u32> {
        let mut f = 0u32;
        // SAFETY : voir plus haut
        unsafe {
            if GetOverlappedResultEx(self.h, o, &mut f, ms, false).is_ok() {
                return Some(f);
            }
            let _ = CancelIoEx(self.h, Some(o));
            let _ = GetOverlappedResultEx(self.h, o, &mut f, 1000, false);
        }
        None
    }

    fn ecrire(&self, trame: &[u8]) -> bool {
        let mut b = vec![0u8; self.sortie];
        let n = trame.len().min(b.len());
        b[..n].copy_from_slice(&trame[..n]);
        let mut o = OVERLAPPED { hEvent: self.ev, ..Default::default() };
        // SAFETY : tampon et OVERLAPPED vivants jusqu'à la fin (ou l'annulation) de l'écriture, attendue ici
        unsafe {
            let _ = ResetEvent(self.ev);
            let _ = WriteFile(self.h, Some(&b), None, Some(&mut o));
            self.finir(&o, 1000).is_some()
        }
    }

    fn lire(&self, ms: u32) -> Option<Vec<u8>> {
        let mut b = vec![0u8; self.entree];
        let mut o = OVERLAPPED { hEvent: self.ev, ..Default::default() };
        // SAFETY : idem, pour la lecture
        let f = unsafe {
            let _ = ResetEvent(self.ev);
            let _ = ReadFile(self.h, Some(&mut b), None, Some(&mut o));
            self.finir(&o, ms)
        }?;
        b.truncate(f as usize);
        Some(b)
    }

    /// Poser une question et attendre la réponse que `juger` reconnaît (une seconde au plus, et pas après `fin`)
    fn demander(&self, q: &[u8], fin: Instant, juger: impl Fn(&[u8]) -> Reponse<'_>) -> Option<Vec<u8>> {
        if Instant::now() >= fin || !self.ecrire(q) {
            return None;
        }
        let limite = fin.min(Instant::now() + Duration::from_millis(1000));
        loop {
            let reste = limite.saturating_duration_since(Instant::now()).as_millis() as u32;
            if reste == 0 {
                return None;
            }
            let r = self.lire(reste)?;
            match juger(&r) {
                Reponse::Autre => continue,
                Reponse::Refus => return None,
                Reponse::Donnees(d) => return Some(d.to_vec()),
            }
        }
    }
}

impl Drop for Canal {
    fn drop(&mut self) {
        // SAFETY : poignées ouvertes par `nouveau`, fermées une fois
        unsafe {
            let _ = CloseHandle(self.ev);
            let _ = CloseHandle(self.h);
        }
    }
}

/// Ce qu'un appareil HID++ a répondu
struct Hidpp {
    batterie: Option<(u8, Charge)>,
    sorte: Option<Sorte>,
    nom: Option<String>,
}

/// Les questions HID++ à l'appareil `dev` : `None` s'il ne répond pas (absent, éteint)
fn questions_hidpp(c: &Canal, dev: u8, fin: Instant) -> Option<Hidpp> {
    let appel = |index: u8, numero: u8, params: &[u8]| {
        c.demander(&p::hidpp_appel(dev, index, numero, params), fin, |r| p::hidpp_reponse(r, dev, index, numero))
    };
    let index_de = |f: u16| {
        c.demander(&p::hidpp_question_fonction(dev, f), fin, |r| p::hidpp_reponse(r, dev, 0, 0))
            .and_then(|d| d.first().copied())
    };
    let ib = index_de(0x1004)?;
    let batterie = if ib != 0 { appel(ib, 1, &[]).as_deref().and_then(p::hidpp_batterie) } else { None };
    let (mut sorte, mut nom) = (None, None);
    if let Some(i) = index_de(0x0005).filter(|i| *i != 0) {
        sorte = appel(i, 2, &[]).as_deref().and_then(p::hidpp_sorte);
        if let Some(n) = appel(i, 0, &[]).and_then(|d| d.first().copied()).map(usize::from) {
            let mut o: Vec<u8> = Vec::new();
            while o.len() < n.min(64) {
                let Some(d) = appel(i, 1, &[o.len() as u8]) else { break };
                let avant = o.len();
                o.extend(d.iter().take(n - o.len()).copied().take_while(|b| *b != 0));
                if o.len() == avant {
                    break;
                }
            }
            let t = String::from_utf8_lossy(&o).trim().to_string();
            if !t.is_empty() {
                nom = Some(t);
            }
        }
    }
    Some(Hidpp { batterie, sorte, nom })
}

fn appareil(sorte: Sorte, nom: String, sans_fil: bool, batterie: Option<(u8, Charge)>) -> Peripherique {
    Peripherique {
        sorte,
        nom: p::nom_connu(sorte, nom),
        marque: LOGITECH.into(),
        sans_fil,
        batterie: batterie.map(|b| b.0),
        charge: batterie.map(|b| b.1).unwrap_or(Charge::Batterie),
        logiciel: None,
    }
}

/// La batterie d'un casque « Centurion » (PRO X 2)
fn casque(c: &Canal, fin: Instant) -> Option<(u8, Charge)> {
    let pont = *c.demander(&p::centurion_question_pont(), fin, p::centurion_pont)?.first()?;
    let d = c.demander(&p::centurion_question_batterie(pont), fin, |r| p::centurion_reponse(r, pont, 0))?;
    let i = p::centurion_index_batterie(&d)?;
    let d = c.demander(&p::centurion_question_etat(pont, i), fin, |r| p::centurion_reponse(r, pont, i))?;
    p::centurion_batterie(&d)
}

/// Tous les périphériques Logitech trouvés (sans ordre ; `ranger` les met dans l'ordre de la page)
pub fn lire() -> Vec<Peripherique> {
    let tous = lister();
    let mut l = Vec::new();
    let mut vus: Vec<u16> = Vec::new();
    // Souris et claviers : la collection HID++ longue (20 octets)
    for d in tous.iter().filter(|d| d.page == 0xFF00 && d.sortie == 20) {
        if vus.contains(&d.pid) {
            continue;
        }
        vus.push(d.pid);
        let Some(c) = Canal::nouveau(d) else { continue };
        if p::est_recepteur(d.pid, &d.nom) {
            let fin = Instant::now() + Duration::from_secs(3);
            for dev in 1..=6u8 {
                // (derrière un récepteur, un appareil sans batterie lue n'est pas montré : case vide ou inconnue)
                if let Some(r) = questions_hidpp(&c, dev, fin)
                    && r.batterie.is_some()
                {
                    let sorte = r.sorte.unwrap_or(Sorte::Souris);
                    let nom = r.nom.unwrap_or_else(|| format!("{} {LOGITECH}", sorte.genre()));
                    l.push(appareil(sorte, nom, true, r.batterie));
                }
            }
        } else {
            let fin = Instant::now() + Duration::from_secs(2);
            let Some(r) = questions_hidpp(&c, 0xFF, fin) else { continue };
            // (la sorte : celle que dit l'appareil, sinon ses collections de clavier ou de souris)
            let meme =
                |page: u16, usage: u16| tous.iter().any(|x| x.pid == d.pid && x.page == page && x.usage == usage);
            let sorte = r.sorte.or_else(|| {
                if meme(1, 6) {
                    Some(Sorte::Clavier)
                } else if meme(1, 2) {
                    Some(Sorte::Souris)
                } else {
                    None
                }
            });
            let Some(sorte) = sorte else { continue };
            let nom = if d.nom.is_empty() {
                r.nom.unwrap_or_else(|| format!("{} {LOGITECH}", sorte.genre()))
            } else {
                d.nom.clone()
            };
            l.push(appareil(sorte, nom, false, r.batterie));
        }
    }
    // Casques : la collection « Centurion » (64 octets)
    let mut vus: Vec<u16> = Vec::new();
    for d in tous.iter().filter(|d| d.page == 0xFFA0 && d.sortie >= 64) {
        if vus.contains(&d.pid) {
            continue;
        }
        vus.push(d.pid);
        let Some(c) = Canal::nouveau(d) else { continue };
        if let Some(b) = casque(&c, Instant::now() + Duration::from_secs(2)) {
            let nom = if d.nom.is_empty() { "Casque Logitech".to_string() } else { d.nom.clone() };
            l.push(appareil(Sorte::Casque, nom, true, Some(b)));
        }
    }
    l
}
