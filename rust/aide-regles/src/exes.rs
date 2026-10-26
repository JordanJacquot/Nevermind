//! Les listes de programmes et de services que l'Aide ne touche jamais, ou seulement en partie
//! (`$AideExesWindows`, `$AideExesProteges`, `$AideServicesMemoire`, `$AideServicesProteges`, aide-actions.ps1).

use crate::texte;

/// Programmes de Windows et de l'antivirus : jamais fermés, jamais bloqués, jamais de priorité changée
const EXES_WINDOWS: [&str; 44] = [
    "csrss",
    "lsass",
    "lsaiso",
    "smss",
    "wininit",
    "winlogon",
    "services",
    "svchost",
    "System",
    "Registry",
    "Idle",
    "MsMpEng",
    "NisSrv",
    "MpCmdRun",
    "MpDefenderCoreService",
    "MpSigStub",
    "MRT",
    "smartscreen",
    "MsSense",
    "SenseCncProxy",
    "SenseIR",
    "SenseNdr",
    "SenseTVM",
    "SecurityHealthService",
    "SecurityHealthSystray",
    "SecurityHealthHost",
    "SgrmBroker",
    "dwm",
    "explorer",
    "fontdrvhost",
    "sihost",
    "ctfmon",
    "conhost",
    "audiodg",
    "spoolsv",
    "taskhostw",
    "RuntimeBroker",
    "TrustedInstaller",
    "TiWorker",
    "wuauclt",
    "MoUsoCoreWorker",
    "ApplicationFrameHost",
    // (fin de la liste de Windows ; les deux suivants ne sont que dans la liste « protégés »)
    "Nevermind",
    "Mindset",
];
const NB_WINDOWS: usize = 42;

/// Le nom est-il dans la liste (avec ou sans « .exe », sans tenir compte des majuscules) ?
fn dans(liste: &[&str], nom: &str) -> bool {
    // (« .exe » final : quatre lettres ASCII, on peut couper le texte à cet endroit)
    let sans_exe = if texte::finit(nom, ".exe") { &nom[..nom.len() - 4] } else { nom };
    liste.iter().any(|x| texte::egal(nom, x) || texte::egal(sans_exe, x))
}

/// Un programme de Windows ou de l'antivirus, par son nom (`$AideExesWindows`)
pub fn windows(nom: &str) -> bool {
    dans(&EXES_WINDOWS[..NB_WINDOWS], nom)
}

/// Pareil, plus Nevermind et Mindset (`$AideExesProteges`, pour les demandes par nom)
pub fn protege(nom: &str) -> bool {
    dans(&EXES_WINDOWS, nom)
}

/// Les services que la page RAM passe en « manuel » (mises à jour, logiciels du fabricant), et remet comme avant.
/// Jamais un service de Windows.
const SERVICES_MEMOIRE: [&str; 22] = [
    "edgeupdate",
    "edgeupdatem",
    "gupdate",
    "gupdatem",
    "AdobeARMservice",
    "AdobeUpdateService",
    "MozillaMaintenance",
    "HPAppHelperCap",
    "HPDiagsCap",
    "HPSysInfoCap",
    "HpTouchpointAnalyticsService",
    "SupportAssistAgent",
    "DDVDataCollector",
    "DDVCollectorSvcApi",
    "DDVRulesProcessor",
    "ASUSSystemAnalysis",
    "ASUSSystemDiagnosis",
    "ASUSLinkNear",
    "ASUSLinkRemote",
    "ESRV_SVC_QUEENCREEK",
    "SystemUsageReportSvc_QUEENCREEK",
    "NvTelemetryContainer",
];

/// Un service de la page RAM (la liste, plus GoogleUpdater(Internal)?Service suivi de 0 à 20 chiffres ou points)
pub fn service_memoire(nom: &str) -> bool {
    if SERVICES_MEMOIRE.iter().any(|s| texte::egal(nom, s)) {
        return true;
    }
    for debut in ["GoogleUpdaterService", "GoogleUpdaterInternalService"] {
        if let Some(r) = texte::reste_apres(nom, debut)
            && r.chars().count() <= 20
            && r.chars().all(|c| c.is_ascii_digit() || c == '.')
        {
            return true;
        }
    }
    false
}

/// Les services indispensables à Windows ou à l'antivirus : jamais bloqués au pare-feu
const SERVICES_PROTEGES: [&str; 22] = [
    "WinDefend",
    "WdNisSvc",
    "WdFilter",
    "Sense",
    "SecurityHealthService",
    "wscsvc",
    "mpssvc",
    "BFE",
    "wuauserv",
    "UsoSvc",
    "WaaSMedicSvc",
    "BITS",
    "DoSvc",
    "CryptSvc",
    "TrustedInstaller",
    "Dnscache",
    "Dhcp",
    "NlaSvc",
    "nsi",
    "W32Time",
    "EventLog",
    "RpcSs",
];

/// Un service indispensable à Windows ou à l'antivirus
pub fn service_protege(nom: &str) -> bool {
    SERVICES_PROTEGES.iter().any(|s| texte::egal(nom, s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listes() {
        assert!(windows("lsass") && windows("LSASS.EXE") && windows("svchost") && !windows("Nevermind"));
        assert!(protege("Nevermind") && protege("mindset.exe") && protege("MsMpEng.exe"));
        assert!(!windows("chrome") && !windows("lsass.exe.exe") && !windows("xlsass"));
        assert!(service_memoire("GoogleUpdaterService140.0.1.0") && service_memoire("GoogleUpdaterInternalService"));
        assert!(!service_memoire("GoogleUpdaterService1; calc") && !service_memoire("edgeupdate2"));
        assert!(!service_memoire(""));
        assert!(service_protege("wuauserv") && !service_protege("DiagTrack"));
    }
}
