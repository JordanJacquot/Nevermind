//! Le seul module qui parle à Windows (et le seul où le code « unsafe » est permis). Il remplit le trait `Systeme`
//! (et `Hote`) avec les vrais appels ; les règles et les demandes n'en savent rien.
//!
//! Rien n'est lu chez l'utilisateur : ni variables d'environnement (les dossiers viennent de Windows), ni profil, ni
//! modules. Les programmes lancés (netsh, dism) reçoivent un environnement neuf, fait ici.

#![allow(unsafe_code)]

mod canal;
mod com;
mod lectures;
mod mesures;
mod outils;
mod pare_feu;
mod processus;
mod reseau;
mod services;
mod taches;
mod temps;

use crate::systeme::{
    Entree, ErreurCanal, ErreurEffacement, Hote, Identite, Lance, LigneTcp, ProcessusVu, RappelPm, RappelTrace,
    RefusTrace, Systeme, TacheVue, Trace, ValeurLue,
};
use aide_regles::carte::FicheCarte;
use aide_regles::demandes::{Carte, ContexteProgrammes, Demarrage, Pc, ip_normale};
use aide_regles::demarrage::LecturesDemarrage;
use aide_regles::dossiers::Dossiers;
use aide_regles::fermeture::Fiche;
use aide_regles::registre::Ruche;
use aide_regles::texte;
use outils::{MemoireLocale, texte as texte_windows};
use serde_json::Value;
use std::os::windows::fs::MetadataExt;
use std::os::windows::process::CommandExt;
use std::sync::OnceLock;
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT, SetNamedSecurityInfoW,
};
use windows::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::LibraryLoader::{
    GetModuleFileNameW, LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories,
};
use windows::Win32::System::SystemInformation::{GetSystemDirectoryW, GetSystemWindowsDirectoryW};
use windows::Win32::UI::Shell::{
    FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows::core::{BOOL, GUID, HSTRING};
use windows_registry::{Key, LOCAL_MACHINE, Type, USERS};

/// Un dossier connu de Windows (Program Files...), lu par Windows et jamais dans les variables d'environnement
fn dossier_connu(id: &GUID) -> String {
    // SAFETY : texte rendu par Windows, libéré avec CoTaskMemFree
    unsafe {
        match SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) {
            Ok(p) => {
                let s = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
                s
            }
            Err(_) => String::new(),
        }
    }
}

/// Le PC, tel que Windows le décrit
pub struct Windows {
    dossiers: Dossiers,
    system32: String,
    racine: String,
    /// Le dossier aide-temp, une fois préparé : dossier temporaire des programmes lancés par l'Aide (dism)
    temp: OnceLock<String>,
}

/// Prépare le processus avant toute chose : les DLL ne sont cherchées que dans System32, le dossier courant devient
/// System32 (comme le lanceur C#), et la sécurité de COM est posée. Rend ce qui doit vivre jusqu'à la fin.
pub fn preparer_processus() -> impl Drop {
    // SAFETY : réglages du processus, avant tout autre appel
    unsafe {
        let _ = SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32);
    }
    let mut b = vec![0u16; 1024];
    // SAFETY : tampon et taille cohérents
    let n = unsafe { GetSystemDirectoryW(Some(&mut b)) } as usize;
    let system32 = String::from_utf16_lossy(&b[..n.min(b.len())]);
    let _ = std::env::set_current_dir(&system32);
    environnement_sur(&system32);
    com::securite_du_processus()
}

/// Lancée par la demande de Windows, l'Aide reçoit les variables d'environnement de l'utilisateur, qu'un programme
/// sans droits peut changer (HKCU\Environment). L'Aide ne les lit jamais, mais les programmes qu'elle lance en
/// hériteraient : les dossiers de Windows sont remis tels que Windows les donne, comme le faisait le lanceur C#
/// (outils\lanceur.cs, EnvironnementAide). TEMP et TMP sont remis plus tard, sur aide-temp (preparer_aide_temp).
fn environnement_sur(system32: &str) {
    let mut b = vec![0u16; 1024];
    // SAFETY : tampon et taille cohérents
    let n = unsafe { GetSystemWindowsDirectoryW(Some(&mut b)) } as usize;
    let windows = String::from_utf16_lossy(&b[..n.min(b.len())]).trim_end_matches('\\').to_string();
    let programmes = dossier_connu(&FOLDERID_ProgramFiles);
    let x86 = dossier_connu(&FOLDERID_ProgramFilesX86);
    let communes = dossier_connu(&FOLDERID_ProgramData);
    if windows.is_empty() || programmes.is_empty() || communes.is_empty() {
        return;
    }
    let x86 = if x86.is_empty() { programmes.clone() } else { x86 };
    let modules = format!("{system32}\\WindowsPowerShell\\v1.0\\Modules");
    for (nom, valeur) in [
        ("windir", &windows),
        ("SystemRoot", &windows),
        ("ProgramFiles", &programmes),
        ("ProgramW6432", &programmes),
        ("ProgramFiles(x86)", &x86),
        ("ProgramData", &communes),
        ("ALLUSERSPROFILE", &communes),
        ("PSModulePath", &modules),
    ] {
        // SAFETY : au tout début de main, avant le moindre autre fil
        unsafe { std::env::set_var(nom, valeur) };
    }
}

impl Windows {
    pub fn new() -> Windows {
        let mut b = vec![0u16; 1024];
        // SAFETY : tampons et tailles cohérents
        let (windows, system32, exe) = unsafe {
            let n = GetSystemWindowsDirectoryW(Some(&mut b)) as usize;
            let w = String::from_utf16_lossy(&b[..n.min(b.len())]);
            let n = GetSystemDirectoryW(Some(&mut b)) as usize;
            let s = String::from_utf16_lossy(&b[..n.min(b.len())]);
            let mut e = vec![0u16; 32768];
            let n = GetModuleFileNameW(None, &mut e) as usize;
            (w, s, String::from_utf16_lossy(&e[..n.min(e.len())]))
        };
        let racine = match exe.rfind('\\') {
            Some(i) => exe[..i].to_string(),
            None => exe,
        };
        Windows {
            dossiers: Dossiers {
                windows: windows.trim_end_matches('\\').to_string(),
                programmes: dossier_connu(&FOLDERID_ProgramFiles),
                programmes_x86: dossier_connu(&FOLDERID_ProgramFilesX86),
                donnees_communes: dossier_connu(&FOLDERID_ProgramData),
            },
            system32: system32.trim_end_matches('\\').to_string(),
            racine,
            temp: OnceLock::new(),
        }
    }

    fn ruche(r: Ruche) -> &'static Key {
        match r {
            Ruche::Machine => LOCAL_MACHINE,
            Ruche::Utilisateurs => USERS,
        }
    }

    /// Lance un programme de System32, sans fenêtre, avec un environnement neuf (rien de l'utilisateur), et attend
    /// qu'il finisse (son résultat est ignoré, comme dans la version PowerShell)
    fn lancer(&self, exe: &str, arguments: &str) {
        let mut c = std::process::Command::new(format!("{}\\{exe}", self.system32));
        c.raw_arg(arguments)
            .env_clear()
            .envs(self.environnement())
            .current_dir(&self.system32)
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let _ = c.status();
    }

    /// L'environnement des programmes lancés par l'Aide : neuf, fait ici (rien de l'utilisateur)
    fn environnement(&self) -> Vec<(String, String)> {
        let w = &self.dossiers.windows;
        let mut e = vec![
            ("SystemRoot".to_string(), w.clone()),
            ("windir".to_string(), w.clone()),
            ("SystemDrive".to_string(), w.get(..2).unwrap_or("C:").to_string()),
            ("ComSpec".to_string(), format!("{}\\cmd.exe", self.system32)),
            ("PATH".to_string(), format!("{0};{w};{0}\\Wbem", self.system32)),
            ("ProgramData".to_string(), self.dossiers.donnees_communes.clone()),
            ("ProgramFiles".to_string(), self.dossiers.programmes.clone()),
        ];
        if let Some(t) = self.temp.get() {
            e.push(("TEMP".to_string(), t.clone()));
            e.push(("TMP".to_string(), t.clone()));
        }
        e
    }
}

impl Default for Windows {
    fn default() -> Self {
        Windows::new()
    }
}

/// Un texte UTF-16 du registre (sans les zéros de fin)
fn texte_reg(o: &[u8]) -> String {
    let l: Vec<u16> = o.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    texte_windows(&l)
}

impl LecturesDemarrage for Windows {
    fn commande_run(&self, run32: bool, nom: &str) -> Option<String> {
        let cle = if run32 {
            "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run"
        } else {
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run"
        };
        match self.reg_lire(Ruche::Machine, cle, nom)? {
            ValeurLue::Texte(t) | ValeurLue::TexteEtendu(t) => Some(t),
            _ => None,
        }
    }

    fn antivirus(&self) -> Vec<String> {
        let _com = com::Com::pret();
        lectures::antivirus(&self.dossiers)
    }
}

impl Pc for Windows {
    fn dossiers(&self) -> &Dossiers {
        &self.dossiers
    }

    fn fichier_existe(&self, chemin: &str) -> bool {
        std::fs::metadata(chemin).is_ok_and(|m| m.is_file())
    }

    fn processus_existe(&self, pid: i64) -> bool {
        processus::liste().iter().any(|p| p.0 == pid)
    }

    fn carte(&self, index: i32) -> Option<Carte> {
        reseau::carte(index)
    }

    fn fiche_carte(&self, index: i32) -> Option<FicheCarte> {
        reseau::fiche(index)
    }

    fn dns_locaux(&self, carte: &Carte) -> Vec<String> {
        let mut r = reseau::passerelles(carte.index);
        for base in ["Tcpip", "Tcpip6"] {
            let cle =
                format!("SYSTEM\\CurrentControlSet\\Services\\{base}\\Parameters\\Interfaces\\{{{}}}", carte.guid);
            for nom in ["DhcpNameServer", "DhcpDefaultGateway"] {
                let textes = match self.reg_lire(Ruche::Machine, &cle, nom) {
                    Some(ValeurLue::Texte(t)) | Some(ValeurLue::TexteEtendu(t)) => vec![t],
                    Some(ValeurLue::Multi(l)) => l,
                    _ => Vec::new(),
                };
                for t in textes {
                    r.extend(t.split([',', ' ', '\t', '\r', '\n']).filter_map(ip_normale));
                }
            }
            if let Some(ValeurLue::Binaire(b)) = self.reg_lire(Ruche::Machine, &cle, "Dhcpv6DNSServers") {
                for c in b.as_chunks::<16>().0 {
                    if let Some(ip) = ip_normale(&std::net::Ipv6Addr::from(*c).to_string()) {
                        r.push(ip);
                    }
                }
            }
        }
        let mut uniques = Vec::new();
        for x in r {
            if !uniques.contains(&x) {
                uniques.push(x);
            }
        }
        uniques
    }

    fn dns_origine(&self, guid: &str) -> Option<String> {
        match self.reg_lire(Ruche::Machine, "SOFTWARE\\Mindset\\Nevermind\\Aide\\Dns", guid)? {
            ValeurLue::Texte(t) | ValeurLue::TexteEtendu(t) => Some(t),
            ValeurLue::Multi(l) => Some(l.join(" ")),
            ValeurLue::DWord(n) => Some(n.to_string()),
            _ => Some(String::new()),
        }
    }

    fn contexte(&self) -> Result<ContexteProgrammes, String> {
        let _com = com::Com::pret();
        lectures::contexte(&self.dossiers)
    }

    fn processus_protege(&self, chemin: &str) -> bool {
        processus::liste()
            .iter()
            .any(|p| texte::egal(&processus::chemin(p.0), chemin) && processus::protection(p.0) != 0)
    }
}

impl Systeme for Windows {
    fn identite(&self) -> Identite {
        let (compte, admin) = processus::moi();
        Identite { compte, admin, pid: std::process::id() }
    }

    fn racine(&self) -> String {
        self.racine.clone()
    }

    fn heure_locale(&self) -> String {
        temps::heure_locale()
    }

    fn ticks_utc(&self) -> i64 {
        temps::ticks_utc()
    }

    fn ticks_local(&self, utc: i64) -> i64 {
        temps::ticks_local(utc)
    }

    fn reg_lire(&self, ruche: Ruche, cle: &str, nom: &str) -> Option<ValeurLue> {
        let k = Self::ruche(ruche).open(cle).ok()?;
        let v = k.get_value(nom).ok()?;
        let o: &[u8] = &v;
        Some(match v.ty() {
            Type::U32 => ValeurLue::DWord(u32::from_le_bytes(o.get(..4)?.try_into().ok()?)),
            Type::U64 => ValeurLue::QWord(u64::from_le_bytes(o.get(..8)?.try_into().ok()?)),
            Type::String => ValeurLue::Texte(texte_reg(o)),
            Type::ExpandString => ValeurLue::TexteEtendu(texte_reg(o)),
            Type::MultiString => {
                let l: Vec<u16> = o.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
                ValeurLue::Multi(l.split(|&c| c == 0).filter(|x| !x.is_empty()).map(String::from_utf16_lossy).collect())
            }
            Type::Bytes => ValeurLue::Binaire(o.to_vec()),
            Type::Other(n) => ValeurLue::Autre(n, o.to_vec()),
        })
    }

    fn reg_ecrire(&self, ruche: Ruche, cle: &str, nom: &str, valeur: &ValeurLue) -> Result<(), String> {
        let k = Self::ruche(ruche).create(cle).map_err(|e| e.message())?;
        match valeur {
            ValeurLue::DWord(n) => k.set_u32(nom, *n),
            ValeurLue::QWord(n) => k.set_u64(nom, *n),
            ValeurLue::Texte(t) => k.set_string(nom, t),
            ValeurLue::TexteEtendu(t) => k.set_expand_string(nom, t),
            ValeurLue::Binaire(o) => k.set_bytes(nom, Type::Bytes, o),
            ValeurLue::Multi(l) => k.set_multi_string(nom, &l.iter().map(String::as_str).collect::<Vec<_>>()),
            ValeurLue::Autre(t, o) => k.set_bytes(nom, Type::Other(*t), o),
        }
        .map_err(|e| e.message())
    }

    fn reg_effacer(&self, ruche: Ruche, cle: &str, nom: &str) -> Result<(), String> {
        let Ok(k) = Self::ruche(ruche).options().read().write().open(cle) else { return Ok(()) };
        match k.remove_value(nom) {
            Ok(()) => Ok(()),
            Err(e) if e.code().0 as u32 == 0x8007_0002 => Ok(()), // (valeur absente)
            Err(e) => Err(e.message()),
        }
    }

    fn reg_effacer_arbre(&self, ruche: Ruche, cle: &str) -> Result<(), String> {
        Self::ruche(ruche).remove_tree(cle).map_err(|e| e.message())
    }

    fn service_regler(&self, nom: &str, demarrage: Demarrage, lancer: bool, arreter: bool) -> Result<(), String> {
        services::regler(nom, demarrage, lancer, arreter)
    }

    fn dns_appliquer(&self, carte: &Carte, serveurs: &[String]) -> Result<(), String> {
        reseau::appliquer(carte, serveurs)
    }

    fn dns_vider_cache(&self) {
        reseau::vider_cache();
    }

    fn carte_relancer(&self, index: i32) -> Result<(), String> {
        let _com = com::Com::pret();
        reseau::relancer(index)
    }

    fn pare_feu_regles(&self) -> Result<Vec<String>, String> {
        let _com = com::Com::pret();
        pare_feu::noms()
    }

    fn pare_feu_ajouter(&self, nom: &str, programme: &str, service: Option<&str>, entrant: bool) -> Result<(), String> {
        let _com = com::Com::pret();
        pare_feu::ajouter(nom, programme, service, entrant)
    }

    fn pare_feu_retirer(&self, nom: &str) -> Result<(), String> {
        let _com = com::Com::pret();
        pare_feu::retirer(nom)
    }

    fn pare_feu_renommer(&self, ancien: &str, nouveau: &str) -> Result<(), String> {
        let _com = com::Com::pret();
        pare_feu::renommer(ancien, nouveau)
    }

    fn pare_feu_activer(&self) -> Result<(), String> {
        let _com = com::Com::pret();
        pare_feu::activer()
    }

    fn point_restauration(&self, description: &str) -> Result<(), String> {
        let _com = com::Com::pret();
        lectures::point_restauration(description)
    }

    fn retirer_exclusion(&self, element: &str) {
        let _com = com::Com::pret();
        lectures::retirer_exclusion(element);
    }

    fn wifi_manuel(&self, profil: &str) {
        // (le nom n'a ni guillemet, ni caractère de contrôle, ni « \ » final : vérifié par la demande)
        self.lancer("netsh.exe", &format!("wlan set profileparameter \"name={profil}\" connectionmode=manual"));
    }

    fn smb1_couper(&self, client: bool) {
        {
            let _com = com::Com::pret();
            lectures::smb1_serveur();
        }
        if client {
            let scratch = match self.temp.get() {
                Some(t) => format!(" /ScratchDir:\"{}\"", t.trim_end_matches('\\')),
                None => String::new(),
            };
            self.lancer(
                "dism.exe",
                &format!("/Online /Disable-Feature /FeatureName:SMB1Protocol /NoRestart /Quiet{scratch}"),
            );
        }
    }

    fn lecture_pc(&self) -> Value {
        let _com = com::Com::pret();
        lectures::pc()
    }

    fn lecture_securite(&self) -> Value {
        let _com = com::Com::pret();
        lectures::securite()
    }

    fn taches(&self) -> Vec<TacheVue> {
        let _com = com::Com::pret();
        taches::toutes().unwrap_or_default()
    }

    fn traduire_compte(&self, nom: &str) -> Option<String> {
        processus::traduire(nom)
    }

    fn chemins_processus(&self) -> Vec<(i64, String)> {
        let _com = com::Com::pret();
        lectures::chemins_processus()
    }

    fn entrees(&self, dossier: &str) -> Vec<Entree> {
        let Ok(l) = std::fs::read_dir(dossier) else { return Vec::new() };
        l.flatten()
            .filter_map(|e| {
                // (sur Windows, les informations d'une entrée sont celles du lien lui-même, jamais de sa cible)
                let m = e.metadata().ok()?;
                let a = m.file_attributes();
                Some(Entree {
                    chemin: format!("{}\\{}", dossier.trim_end_matches('\\'), e.file_name().to_string_lossy()),
                    dossier: a & 0x10 != 0,
                    lien: a & 0x400 != 0,
                    lecture_seule: a & 0x1 != 0,
                    taille: m.file_size(),
                    ecrit: temps::ticks_fichier(m.last_write_time()),
                })
            })
            .collect()
    }

    fn dossier_existe(&self, dossier: &str) -> bool {
        std::path::Path::new(dossier).is_dir()
    }

    fn effacer_fichier(&self, chemin: &str, lecture_seule: bool) -> Result<(), ErreurEffacement> {
        if lecture_seule && let Ok(m) = std::fs::symlink_metadata(chemin) {
            let mut p = m.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            p.set_readonly(false);
            let _ = std::fs::set_permissions(chemin, p);
        }
        std::fs::remove_file(chemin).map_err(|e| match e.raw_os_error() {
            Some(5) => ErreurEffacement::Refuse,
            _ => ErreurEffacement::Utilise,
        })
    }

    fn effacer_dossier_vide(&self, dossier: &str) {
        if std::fs::read_dir(dossier).is_ok_and(|mut l| l.next().is_none()) {
            let _ = std::fs::remove_dir(dossier);
        }
    }

    fn processus(&self) -> Vec<ProcessusVu> {
        processus::liste().into_iter().map(|(pid, _parent, nom)| ProcessusVu { pid, nom }).collect()
    }

    fn chemin_processus(&self, pid: i64) -> String {
        processus::chemin(pid)
    }

    fn fiche(&self, pid: i64, chemin: &str) -> Fiche {
        processus::fiche(pid, chemin)
    }

    fn fermer_processus(&self, pid: i64) -> bool {
        processus::fermer(pid)
    }

    fn presentmon(&self) -> Result<String, String> {
        mesures::presentmon(&self.racine)
    }

    fn lancer_presentmon(&self, chemin: &str, arguments: &str, rappel: RappelPm) -> Result<Box<dyn Lance>, String> {
        mesures::lancer_presentmon(chemin, arguments, &self.environnement(), rappel)
    }

    fn fermer_session_presentmon(&self, chemin: &str) {
        mesures::fermer_session_presentmon(chemin, &self.environnement());
    }

    fn demarrer_trace(&self, rappel: RappelTrace) -> Result<Box<dyn Trace>, RefusTrace> {
        mesures::demarrer_trace(rappel)
    }

    fn adresses_locales(&self) -> Vec<std::net::IpAddr> {
        mesures::adresses_locales()
    }

    fn filetime(&self) -> i64 {
        mesures::filetime()
    }

    fn table_tcp(&self, v6: bool) -> Option<Vec<LigneTcp>> {
        mesures::table_tcp(v6)
    }

    fn tcp_compter(&self, l: &LigneTcp) -> bool {
        mesures::tcp_compter(l)
    }

    fn tcp_octets(&self, l: &LigneTcp) -> Option<(u64, u64)> {
        mesures::tcp_octets(l)
    }
}

impl Hote for Windows {
    type E = canal::EcouteWindows;

    fn verrou(&self, nom: &str) -> Option<Box<dyn Send>> {
        processus::verrou(nom).map(|v| Box::new(v) as Box<dyn Send>)
    }

    fn preparer_aide_temp(&self, dossier: &str, admin: bool) -> Result<(), String> {
        match std::fs::create_dir(dossier) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
        let m = std::fs::symlink_metadata(dossier).map_err(|e| e.to_string())?;
        if m.file_attributes() & 0x400 != 0 || !m.is_dir() {
            return Err("lien".into());
        }
        if admin {
            // Réservé au système et aux administrateurs (droits hérités par ce qu'il contient)
            let mut sd = PSECURITY_DESCRIPTOR::default();
            // SAFETY : descripteur créé par Windows et libéré par MemoireLocale ; la liste des droits est lue dedans
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    &HSTRING::from("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"),
                    SDDL_REVISION_1,
                    &mut sd,
                    None,
                )
                .map_err(|e| e.message())?;
                let _m = MemoireLocale(sd.0);
                let mut present = BOOL::default();
                let mut defaut = BOOL::default();
                let mut dacl: *mut ACL = std::ptr::null_mut();
                GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaut).map_err(|e| e.message())?;
                let r = SetNamedSecurityInfoW(
                    &HSTRING::from(dossier),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    None,
                    None,
                    Some(dacl),
                    None,
                );
                if r.0 != 0 {
                    return Err(format!("droits d'aide-temp non posés (erreur {})", r.0));
                }
            }
        }
        let t = dossier.trim_end_matches('\\').to_string();
        // (TEMP et TMP de l'utilisateur, piégeables, remplacés comme dans NevermindAide.ps1 ; au démarrage, avant que
        // l'Aide ne serve la moindre demande)
        // SAFETY : appelé une fois au démarrage (serveur::lancer), avant les fils des demandes
        unsafe {
            std::env::set_var("TEMP", &t);
            std::env::set_var("TMP", &t);
        }
        let _ = self.temp.set(t);
        Ok(())
    }

    fn canal(&self, nom: &str, sddl: &str, premier: bool, proprietaire: Option<&str>) -> Result<Self::E, ErreurCanal> {
        canal::creer(nom, sddl, premier, proprietaire)
    }
}
