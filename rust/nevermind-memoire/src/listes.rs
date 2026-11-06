//! Les listes de memoire.ps1, recopiées telles quelles (mêmes motifs, mêmes noms, mêmes phrases).

use regex::Regex;
use std::sync::LazyLock;

/// Un motif comme `-match` de PowerShell (sans tenir compte des majuscules)
pub fn motif(m: &str) -> Regex {
    Regex::new(&format!("(?i){m}")).expect("motif de memoire.ps1")
}

/// `$EtiquettesMemoire`
pub fn etiquette(kind: &str) -> &'static str {
    match kind {
        "windows" => "Windows en a besoin",
        "inutile" => "Inutile au quotidien",
        "verifier" => "À vérifier",
        _ => "",
    }
}

/// Un service que Nevermind peut passer en « manuel »
pub struct ServiceInutile {
    pub motif: Regex,
    pub nom: &'static str,
}

/// `$ServicesInutilesMemoire`
pub static SERVICES_INUTILES: LazyLock<Vec<ServiceInutile>> = LazyLock::new(|| {
    [
        (r"^edgeupdatem?$", "Mise à jour de Microsoft Edge"),
        (r"^(gupdatem?|GoogleUpdater(Internal)?Service[\d.]{0,20})$", "Mise à jour de Google Chrome"),
        (r"^AdobeARMservice$", "Mise à jour d'Adobe Acrobat"),
        (r"^AdobeUpdateService$", "Mise à jour des logiciels Adobe"),
        (r"^MozillaMaintenance$", "Mise à jour de Firefox"),
        (
            r"^(HPAppHelperCap|HPDiagsCap|HPSysInfoCap|HpTouchpointAnalyticsService)$",
            "Logiciel HP (aide et statistiques)",
        ),
        (
            r"^(SupportAssistAgent|DDVDataCollector|DDVCollectorSvcApi|DDVRulesProcessor)$",
            "Logiciel Dell (SupportAssist et statistiques)",
        ),
        (
            r"^(ASUSSystemAnalysis|ASUSSystemDiagnosis|ASUSLinkNear|ASUSLinkRemote)$",
            "Logiciel ASUS (diagnostic, lien avec le téléphone)",
        ),
        (r"^(ESRV_SVC_QUEENCREEK|SystemUsageReportSvc_QUEENCREEK)$", "Mesure d'énergie Intel (statistiques)"),
        (r"^NvTelemetryContainer$", "Statistiques NVIDIA"),
    ]
    .into_iter()
    .map(|(m, nom)| ServiceInutile { motif: motif(m), nom })
    .collect()
});

/// `$ExesWindowsMemoire`
pub static EXES_WINDOWS: LazyLock<Regex> = LazyLock::new(|| {
    motif(
        r"^(system|secure system|registry|smss|csrss|wininit|winlogon|services|lsass|lsaiso|lsm|svchost|dwm|explorer|fontdrvhost|sihost|ctfmon|conhost|audiodg|spoolsv|taskhostw|runtimebroker|dllhost|wmiprvse|wmiapsrv|searchhost|searchapp|searchindexer|searchprotocolhost|searchfilterhost|startmenuexperiencehost|shellexperiencehost|shellhost|textinputhost|msmpeng|nissrv|mpdefendercoreservice|securityhealthservice|securityhealthsystray|smartscreen|sgrmbroker|wudfhost|dashost|unsecapp|applicationframehost|lockapp|backgroundtaskhost|aggregatorhost|mousocoreworker|tiworker|trustedinstaller|wlanext|msdtc|systemsettings|useroobebroker|comppkgsrv|sppsvc|ngciso|igfxem|igfxcuiservice|igfxext|nvdisplay\.container|nvcontainer|atiesrxx|atieclxx|amdfendrsr|rtkauduservice64|intelaudioservice)$",
    )
});

/// `$ProduitsWindowsMemoire`
pub static PRODUITS_WINDOWS: LazyLock<Regex> =
    LazyLock::new(|| motif(r"Windows. Operating System|Syst.me d.{1,6}exploitation|^Microsoft. Windows"));

/// `$NomsWindowsMemoire`
pub fn nom_windows(bas: &str) -> Option<&'static str> {
    Some(match bas {
        "msmpeng" | "mpdefendercoreservice" | "nissrv" => "Antivirus de Windows (Microsoft Defender)",
        "dwm" => "Affichage de Windows (fenêtres et effets)",
        "explorer" => "Explorateur de Windows (Bureau et barre des tâches)",
        "system" => "Windows lui-même (noyau)",
        "registry" => "Registre de Windows",
        "csrss" => "Windows lui-même (sessions)",
        "lsass" => "Sécurité de Windows (connexion)",
        "searchhost" | "searchindexer" | "searchprotocolhost" | "searchfilterhost" => "Recherche de Windows",
        "startmenuexperiencehost" | "shellexperiencehost" => "Menu Démarrer",
        "textinputhost" | "ctfmon" => "Clavier et saisie de Windows",
        "audiodg" => "Son de Windows",
        "wmiprvse" => "Informations du PC (WMI)",
        "runtimebroker" => "Autorisations des applis",
        "powershell" => "Windows PowerShell",
        "nvcontainer" | "nvdisplay.container" => "Pilote NVIDIA",
        _ => return None,
    })
}

/// Une appli connue (`$AppsConnuesMemoire`)
pub struct AppConnue {
    pub motif: Regex,
    pub nom: &'static str,
    pub kind: &'static str,
    pub note: &'static str,
    pub origine: bool,
    pub jamais: bool,
    pub ensemble: bool,
}

const NAVIGATEUR: &str = "Ton navigateur. Le fermer libère cette mémoire.";

/// `$AppsConnuesMemoire`, dans le même ordre (le premier motif qui va l'emporte)
pub static APPS_CONNUES: LazyLock<Vec<AppConnue>> = LazyLock::new(|| {
    let a = |m: &str, nom: &'static str, kind: &'static str, note: &'static str| AppConnue {
        motif: motif(m),
        nom,
        kind,
        note,
        origine: false,
        jamais: false,
        ensemble: false,
    };
    vec![
        a(r"^msedge$", "Microsoft Edge", "verifier", NAVIGATEUR),
        AppConnue {
            jamais: true,
            ..a(
                r"^msedgewebview2$",
                "Affichage des applis (WebView2)",
                "verifier",
                "Sert à afficher l'intérieur de certaines applis, dont Nevermind et Mindset. Part quand ces applis sont fermées.",
            )
        },
        a(r"^chrome$", "Google Chrome", "verifier", NAVIGATEUR),
        a(r"^firefox$", "Firefox", "verifier", NAVIGATEUR),
        a(r"^(opera|opera_gx)$", "Opera", "verifier", NAVIGATEUR),
        a(r"^brave$", "Brave", "verifier", NAVIGATEUR),
        AppConnue {
            ensemble: true,
            ..a(
                r"^(mindset|nevermind|nevermindaide)$",
                "Mindset et ses applis",
                "verifier",
                "Les applis de la suite Mindset, dont Nevermind ouvert en ce moment.",
            )
        },
        a(
            r"^microsoftedgeupdate$",
            "Mise à jour de Microsoft Edge",
            "inutile",
            "Cherche les mises à jour de Edge. Pas besoin qu'il tourne tout le temps.",
        ),
        a(
            r"^(googleupdate|googlecrashhandler(64)?)$",
            "Mise à jour de Google Chrome",
            "inutile",
            "Cherche les mises à jour de Chrome. Pas besoin qu'il tourne tout le temps.",
        ),
        a(
            r"^(armsvc|adobearm|adobeupdateservice|acrotray)$",
            "Mise à jour d'Adobe",
            "inutile",
            "Cherche les mises à jour d'Adobe. Pas besoin qu'il tourne tout le temps.",
        ),
        AppConnue {
            origine: true,
            ..a(
                r"^(lightingservice|ledkeeper\d*|auracreatorservice|aurawallpaperservice|signalrgb|openrgb|mysticlight.*|rzchromastreamserver|rzchromasdkservice|chromasdk.*)$",
                "",
                "inutile",
                "Gère les lumières (RGB) du PC. Les lumières gardent souvent leur dernière couleur sans lui.",
            )
        },
        a(
            r"^(hpcommrecovery|hpsysinfo|hpdiags.*|hptouchpoint.*|hpsupportassistant|hpanalyticscomp|supportassist.*|dellsupportassist.*|asussystemanalysis|asussystemdiagnosis|asuslink.*|myasus|acerquickaccess|carecenter.*)$",
            "",
            "inutile",
            "Logiciel du fabricant du PC (aide, statistiques). Pas besoin qu'il tourne tout le temps.",
        ),
        AppConnue {
            origine: true,
            ..a(
                r"^(icue|corsair.*|razer.*|rzsynapse.*|lghub.*|steelseries.*|armourycrate.*|armourysocketserver|nzxt.*|msi\.centralserver|msi_center.*|dragoncenter.*)$",
                "",
                "verifier",
                "Règle tes lumières, ta souris, ton casque ou tes ventilateurs.",
            )
        },
        AppConnue {
            origine: true,
            ..a(
                r"^(steam|steamwebhelper|gameoverlayui(64)?|eaanticheat\.gameservice|easteamlauncher|ealocalhostsvc|epicgameslauncher|epicwebhelper|eadesktop|eabackgroundservice|ubisoftconnect|upc|battle\.net|riotclientservices|goggalaxy|galaxyclient)$",
                "",
                "verifier",
                "Lanceur de jeux.",
            )
        },
        AppConnue {
            origine: true,
            ..a(r"^(discord|ms-teams|teams|spotify|whatsapp|skype|zoom)$", "", "verifier", "Messagerie ou musique.")
        },
        AppConnue { origine: true, ..a(r"^onedrive$", "OneDrive", "verifier", "Synchronise tes fichiers.") },
        a(
            r"^(phoneexperiencehost|crossdeviceresume)$",
            "Lien avec le téléphone",
            "verifier",
            "Relie ton téléphone au PC. Si tu ne t'en sers pas, il peut être coupé dans les paramètres de Windows.",
        ),
        a(
            r"^(widgets|widgetservice)$",
            "Widgets de Windows",
            "verifier",
            "Le panneau météo et actualités. Il se coupe dans les paramètres de la barre des tâches.",
        ),
        a(
            r"^officeclicktorun$",
            "Microsoft Office (mises à jour)",
            "verifier",
            "Met à jour Office. Utile si tu te sers de Word ou d'Excel.",
        ),
        AppConnue {
            jamais: true,
            ..a(
                r"^(avastsvc|avastui|avgsvc|avgui|mcshield|mfemms|mcapexe|modulecoreservice|nortonsecurity|ns|nsbu|bdagent|bdservicehost|vsserv|avp|avpui|ekrn|egui|mbamservice|mbamtray|mbam)$",
                "",
                "verifier",
                "Antivirus en plus de celui de Windows. Un seul suffit : garde celui que tu as choisi.",
            )
        },
        a(
            r"^(vmmem|vmmemwsl|vmwp)$",
            "Machine virtuelle",
            "verifier",
            "Une machine virtuelle ou Linux (WSL) est ouverte. La fermer libère cette mémoire.",
        ),
    ]
});

/// `$DossiersJeuxMemoire`
pub static DOSSIERS_JEUX: LazyLock<Regex> = LazyLock::new(|| {
    motif(
        r"\\(steamapps\\common|Epic Games|EA Games|Origin Games|Ubisoft Game Launcher\\games|GOG Games|GOG Galaxy\\Games|Riot Games|Battle\.net|XboxGames|Rockstar Games|Electronic Arts)\\",
    )
});

/// `$LanceursMemoire`
pub static LANCEURS: LazyLock<Regex> = LazyLock::new(|| {
    motif(
        r"^(steam|steamwebhelper|gameoverlayui(64)?|easteamlauncher|ealocalhostsvc|ealauncher|epicgameslauncher|epicwebhelper|eadesktop|eabackgroundservice|origin|ubisoftconnect|upc|uplaywebcore|battle\.net|battle\.net launcher|riotclientservices|riotclientux|riotclientuxrender|goggalaxy|galaxyclient|galaxyclient helper|xboxpcapp)$",
    )
});

/// Les jeux que la mesure des FPS connaît d'office (`$FpsVeilleConnus`, aide-client.ps1)
pub const FPS_VEILLE_CONNUS: &[&str] = &["bf6.exe"];

/// `Get-GenrePilote` : le genre de matériel d'un pilote, d'après son nom, sa description ou son fabricant
pub fn genre_pilote(t: &str) -> &'static str {
    static GENRES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
        [
            ("Killer|Rivet", "carte réseau Killer"),
            (r"Realtek.*(Ethernet|PCIe|GbE|LAN|Wireless|WLAN|Wi-?Fi)|rt\d+x\d+|rtwlan|rtcx", "carte réseau Realtek"),
            ("Ethernet|Wireless|Wi-?Fi|WLAN|Network|NDIS|LAN ", "carte réseau"),
            ("Avast|AVG|McAfee|Norton|Symantec|Bitdefender|Kaspersky|ESET|Malwarebytes|Sophos|Trend", "antivirus"),
            (
                "RGB|Aura|Lighting|iCUE|Corsair|Razer|Mystic|SignalRGB|OpenRGB|ASUS|MSI|Gigabyte|ASRock|NZXT",
                "logiciel RGB ou de carte mère",
            ),
            ("NVIDIA|Radeon|AMD|Graphics|Display", "carte graphique"),
            ("Storage|RST|NVMe|SATA|AHCI|RAID", "disque"),
            ("Audio|Sound|Nahimic|Sonic", "son"),
        ]
        .into_iter()
        .map(|(m, g)| (motif(m), g))
        .collect()
    });
    GENRES.iter().find(|(m, _)| m.is_match(t)).map_or("", |(_, g)| g)
}
