//! Les optimisations gaming (optimisations.ps1) : les réglages de Windows (`$Tweaks`), leur vérification, ce qu'ils
//! changent (déclaré à l'avance) et le plan d'alimentation ; le type de PC (portable, fixe, onduleur) ; les programmes
//! au démarrage.

use crate::modele::{Changements, Declaration};
use crate::pc::{Pc, ValeurReg};
use regex::Regex;
use std::sync::LazyLock;

pub const HAUTES_PERF: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
pub const NORMAL: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
pub const CURSEUR_PERF: &str = "ded574b5-45a0-4f42-8737-46345c09c238";
pub const OPTIMALES: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";
pub const SOURIS: &str = "HKCU:\\Control Panel\\Mouse";
pub const ACCES: [&str; 3] = [
    "HKCU:\\Control Panel\\Accessibility\\StickyKeys",
    "HKCU:\\Control Panel\\Accessibility\\Keyboard Response",
    "HKCU:\\Control Panel\\Accessibility\\ToggleKeys",
];
pub const DX: &str = "HKCU:\\Software\\Microsoft\\DirectX\\UserGpuPreferences";
pub const DO: &str =
    "Registry::HKEY_USERS\\S-1-5-20\\Software\\Microsoft\\Windows\\CurrentVersion\\DeliveryOptimization\\Settings";
pub const APPROUVE: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved";
pub const IFEO: &str = "HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options";

pub static GUID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").unwrap());

/// Un réglage gaming (`$Tweaks`).
#[derive(Debug, Clone)]
pub struct Reglage {
    pub id: &'static str,
    pub quoi: &'static str,
    pub impact: &'static str,
    pub titre: &'static str,
    pub description: &'static str,
    pub ok: &'static str,
    pub ko: &'static str,
    pub redemarrage: bool,
    pub build_min: i64,
    /// `Recommended = $false` : optionnel (ne compte pas dans le score)
    pub conseille: bool,
}

pub const REGLAGES: [Reglage; 9] = [
    Reglage {
        id: "power",
        quoi: "Mettre Windows en mode performances : plan « Performances élevées » sur un PC fixe, mode « Meilleures performances » sur un portable.",
        impact: "Important",
        titre: "Plan d'alimentation",
        description: "Le processeur reste à pleine vitesse au lieu de ralentir pour économiser l'énergie, ce qui réduit les micro-saccades. Sur un portable, la batterie se vide plus vite: le réglage s'applique surtout quand il est branché.",
        ok: "Mode performances actif: le processeur tourne à pleine vitesse.",
        ko: "Le plan actuel laisse le processeur ralentir pour économiser l'énergie.",
        redemarrage: false,
        build_min: 0,
        conseille: true,
    },
    Reglage {
        id: "dvr",
        quoi: "Couper l'option « Enregistrer ce qui s'est passé » de la Xbox Game Bar.",
        impact: "Important",
        titre: "Enregistrement en arrière-plan (Xbox Game Bar)",
        description: "Coupe l'enregistrement permanent des 30 dernières secondes de jeu, qui coûte des FPS. Les captures manuelles (Win + Alt + R), ShadowPlay et Radeon ReLive continuent de fonctionner.",
        ok: "Désactivé: la Game Bar ne filme pas tes parties en continu.",
        ko: "La Game Bar filme tes parties en continu, ce qui coûte des FPS.",
        redemarrage: false,
        build_min: 0,
        conseille: true,
    },
    Reglage {
        id: "gamemode",
        quoi: "Activer le mode jeu de Windows.",
        impact: "Moyen",
        titre: "Mode jeu Windows",
        description: "Windows donne la priorité au jeu lancé et évite d'installer des mises à jour ou d'afficher des notifications pendant que tu joues.",
        ok: "Activé.",
        ko: "Désactivé: Windows peut lancer des tâches de fond pendant tes parties.",
        redemarrage: false,
        build_min: 0,
        conseille: true,
    },
    Reglage {
        id: "hags",
        quoi: "Activer la planification GPU à accélération matérielle (prise en compte au prochain redémarrage).",
        impact: "Moyen",
        titre: "Planification GPU à accélération matérielle",
        description: "La carte graphique gère elle-même sa mémoire: un peu moins de latence, et c'est obligatoire pour la génération d'images DLSS 3. Nécessite une carte récente (NVIDIA GTX 1000 ou plus, AMD RX 5000 ou plus).",
        ok: "Activée.",
        ko: "Désactivée (ou non prise en charge par ta carte graphique).",
        redemarrage: true,
        build_min: 0,
        conseille: true,
    },
    Reglage {
        id: "windowed",
        quoi: "Activer « Optimisations pour les jeux en mode fenêtré » dans les paramètres graphiques de Windows.",
        impact: "Moyen",
        titre: "Optimisations pour les jeux en mode fenêtré",
        description: "Réduit la latence des jeux DirectX 10 et 11 lancés en fenêtré ou en plein écran sans bordure, et permet d'utiliser Auto HDR et le taux de rafraîchissement variable.",
        ok: "Activées.",
        ko: "Désactivées: plus de latence en fenêtré et en plein écran sans bordure.",
        redemarrage: false,
        build_min: 22000,
        conseille: true,
    },
    Reglage {
        id: "mouse",
        quoi: "Décocher « Améliorer la précision du pointeur » (effet immédiat, ta sensibilité en jeu ne change pas).",
        impact: "Moyen",
        titre: "Accélération de la souris",
        description: "Désactive « Améliorer la précision du pointeur ». Un même mouvement de la main donne toujours le même déplacement à l'écran: indispensable pour bien viser dans les FPS.",
        ok: "Désactivée: ta visée est constante.",
        ko: "Activée: le curseur dépend de la vitesse de ton geste, mauvais pour viser.",
        redemarrage: false,
        build_min: 0,
        conseille: true,
    },
    Reglage {
        id: "hotkeys",
        quoi: "Désactiver les raccourcis clavier des touches rémanentes, filtres et bascules. Ces fonctions restent disponibles dans les paramètres d'accessibilité.",
        impact: "Léger",
        titre: "Raccourcis d'accessibilité",
        description: "Empêche la fenêtre des touches rémanentes de s'ouvrir quand tu appuies 5 fois sur Maj en pleine partie (idem pour les touches filtres et bascules).",
        ok: "Désactivés: plus de fenêtre surprise en pleine partie.",
        ko: "Appuyer 5 fois sur Maj ouvre une fenêtre qui te sort du jeu.",
        redemarrage: false,
        build_min: 0,
        conseille: true,
    },
    Reglage {
        id: "delivery",
        quoi: "Désactiver le partage des mises à jour Windows avec d'autres PC.",
        impact: "Léger",
        titre: "Partage des mises à jour sur Internet",
        description: "Empêche Windows d'envoyer ses mises à jour à d'autres PC via ta connexion, pour garder ta bande passante montante pour tes parties.",
        ok: "Pas de partage sur Internet.",
        ko: "Windows envoie des mises à jour à d'autres PC via ta connexion.",
        redemarrage: false,
        build_min: 0,
        conseille: true,
    },
    Reglage {
        id: "transparency",
        quoi: "Désactiver les effets de transparence de Windows.",
        impact: "Léger",
        titre: "Effets de transparence",
        description: "Petit gain sur les PC modestes seulement. C'est surtout esthétique: garde-les si ton PC est puissant.",
        ok: "Désactivés.",
        ko: "Activés (impact très léger, surtout utile de les couper sur un PC modeste).",
        redemarrage: false,
        build_min: 0,
        conseille: false,
    },
];

pub fn reglage(id: &str) -> Option<&'static Reglage> {
    REGLAGES.iter().find(|r| r.id == id)
}

/// `Get-AvailableTweaks`
pub fn disponibles(build: i64) -> Vec<&'static Reglage> {
    REGLAGES.iter().filter(|r| r.build_min == 0 || build >= r.build_min).collect()
}

/// `Get-TweakWeight`
pub fn poids(r: &Reglage) -> i64 {
    if !r.conseille {
        return 0;
    }
    match r.impact {
        "Important" => 3,
        "Moyen" => 2,
        _ => 1,
    }
}

/// La valeur d'un réglage du registre (`Get-RegValue` : rien si absente ou illisible)
pub fn valeur(pc: &dyn Pc, chemin: &str, nom: &str) -> Option<ValeurReg> {
    pc.reg_lire(chemin, nom).ok().flatten()
}

/// `$v -ne n` en PowerShell (rien : vrai ; un texte : comparé comme texte)
fn different(v: &Option<ValeurReg>, n: i64) -> bool {
    match v {
        None => true,
        Some(ValeurReg::DWord(x)) | Some(ValeurReg::QWord(x)) => *x != n,
        Some(ValeurReg::Texte(s)) | Some(ValeurReg::TexteEtendu(s)) => !s.eq_ignore_ascii_case(&n.to_string()),
        Some(ValeurReg::Multi(l)) => l.iter().any(|s| s != &n.to_string()),
        Some(ValeurReg::Octets(o)) => o.iter().any(|b| i64::from(*b) != n),
    }
}
fn egal(v: &Option<ValeurReg>, n: i64) -> bool {
    match v {
        None => false,
        Some(ValeurReg::DWord(x)) | Some(ValeurReg::QWord(x)) => *x == n,
        Some(ValeurReg::Texte(s)) | Some(ValeurReg::TexteEtendu(s)) => s.eq_ignore_ascii_case(&n.to_string()),
        Some(ValeurReg::Multi(l)) => l.iter().any(|s| s == &n.to_string()),
        Some(ValeurReg::Octets(o)) => o.iter().any(|b| i64::from(*b) == n),
    }
}
fn texte_de(v: &Option<ValeurReg>) -> String {
    v.as_ref().map(|x| x.texte()).unwrap_or_default()
}

/// Le plan d'alimentation actif (`Get-ActiveScheme`) : (identifiant en minuscules, nom)
pub fn plan_actif(pc: &mut dyn Pc) -> Option<(String, String)> {
    static R: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\s*\((.+)\)").unwrap()
    });
    let (_, o) = pc.powercfg(&["/getactivescheme".into()]);
    R.captures(&o).map(|c| (c[1].to_lowercase(), c[2].trim().to_string()))
}

/// `powercfg /list | Out-String`
pub fn liste_plans(pc: &mut dyn Pc) -> String {
    pc.powercfg(&["/list".into()]).1
}

/// `Test-PowerPlan`
pub fn test_plan(pc: &mut dyn Pc, plan_cree: Option<&str>) -> bool {
    let Some((g, nom)) = plan_actif(pc) else { return false };
    let mut bons = vec![HAUTES_PERF.to_string(), OPTIMALES.to_string()];
    if let Some(c) = plan_cree {
        bons.push(c.to_string());
    }
    static NOM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)perf|ultim|gaming").unwrap());
    if bons.iter().any(|b| b.eq_ignore_ascii_case(&g)) || NOM.is_match(&nom) {
        return true;
    }
    pc.curseur().is_some_and(|c| c.eq_ignore_ascii_case(CURSEUR_PERF))
}

/// `Test-AccessHotkeys` : `Err` si une valeur n'est pas un nombre (Test-Tweak donne alors « pas fait »)
fn test_raccourcis(pc: &dyn Pc) -> Result<bool, ()> {
    for p in ACCES {
        if let Some(f) = valeur(pc, p, "Flags") {
            let n = f.entier().ok_or(())?;
            if n & 4 != 0 {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// `Test-Tweak` : le réglage est fait (une erreur de lecture : pas fait)
pub fn test(pc: &mut dyn Pc, id: &str, plan_cree: Option<&str>) -> bool {
    match id {
        "power" => test_plan(pc, plan_cree),
        "dvr" => different(
            &valeur(pc, "HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\GameDVR", "HistoricalCaptureEnabled"),
            1,
        ),
        "gamemode" => different(&valeur(pc, "HKCU:\\Software\\Microsoft\\GameBar", "AutoGameModeEnabled"), 0),
        "hags" => egal(&valeur(pc, "HKLM:\\SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers", "HwSchMode"), 2),
        "windowed" => {
            texte_de(&valeur(pc, DX, "DirectXUserGlobalSettings")).to_lowercase().contains("swapeffectupgradeenable=1")
        }
        "mouse" => texte_de(&valeur(pc, SOURIS, "MouseSpeed")) == "0",
        "hotkeys" => test_raccourcis(pc).unwrap_or(false),
        "delivery" => different(&valeur(pc, DO, "DownloadMode"), 3),
        "transparency" => egal(
            &valeur(
                pc,
                "HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
                "EnableTransparency",
            ),
            0,
        ),
        _ => false,
    }
}

/// Ce que change un réglage (`$Tweak.Changes`)
pub fn changements(id: &str) -> Changements {
    let dw = |c: &str, n: &str, v: i64| Declaration::reg(c, n, ValeurReg::DWord(v), "");
    let s = |c: &str, n: &str, v: &str| Declaration::reg(c, n, ValeurReg::Texte(v.into()), "String");
    match id {
        "power" => Changements::Alimentation,
        "dvr" => Changements::Liste(vec![dw(
            "HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\GameDVR",
            "HistoricalCaptureEnabled",
            0,
        )]),
        "gamemode" => Changements::Liste(vec![
            dw("HKCU:\\Software\\Microsoft\\GameBar", "AutoGameModeEnabled", 1),
            dw("HKCU:\\Software\\Microsoft\\GameBar", "AllowAutoGameMode", 1),
        ]),
        "hags" => {
            Changements::Liste(vec![dw("HKLM:\\SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers", "HwSchMode", 2)])
        }
        "windowed" => Changements::Fenetre,
        "mouse" => Changements::Liste(vec![
            s(SOURIS, "MouseSpeed", "0"),
            s(SOURIS, "MouseThreshold1", "0"),
            s(SOURIS, "MouseThreshold2", "0"),
        ]),
        "hotkeys" => Changements::Raccourcis,
        "delivery" => Changements::Liste(vec![dw(DO, "DownloadMode", 0)]),
        "transparency" => Changements::Liste(vec![dw(
            "HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
            "EnableTransparency",
            0,
        )]),
        _ => Changements::Aucun,
    }
}

/// `Get-ChangementAlimentation` (lu sans rien changer, même ordre de choix que `Set-PowerPlan`)
pub fn changement_alimentation(pc: &mut dyn Pc, portable: bool, plan_cree: Option<&str>) -> Vec<Declaration> {
    let p = |cible: &str, guid: &str| Declaration {
        sorte: "power".into(),
        cible: cible.into(),
        guid: guid.into(),
        ..Default::default()
    };
    let s = plan_actif(pc);
    if portable && s.as_ref().is_some_and(|(g, _)| g == NORMAL) {
        return vec![p(
            "Utilisation normale, curseur sur « Meilleures performances » (si Windows le refuse : Performances élevées)",
            "",
        )];
    }
    let liste = liste_plans(pc);
    for g in [Some(HAUTES_PERF), plan_cree].into_iter().flatten() {
        if g.is_empty() {
            continue;
        }
        if let Ok(r) = Regex::new(&format!(r"(?i){}\s*\((.+?)\)", regex::escape(g)))
            && let Some(c) = r.captures(&liste)
        {
            return vec![p(c[1].trim(), g)];
        }
    }
    vec![
        p("Performances élevées", ""),
        Declaration::texte(
            "Ce plan n'est pas proposé sur ce PC : Nevermind en crée d'abord une copie (powercfg -duplicatescheme), ou passe le curseur de Windows sur « Meilleures performances » si ce n'est pas possible.",
        ),
    ]
}

/// `Get-ChangementsAccessHotkeys` : le bit 4 de « Flags » retiré, le reste gardé
pub fn changements_raccourcis(pc: &dyn Pc) -> Result<Vec<Declaration>, String> {
    let mut l = Vec::new();
    for p in ACCES {
        if let Some(f) = valeur(pc, p, "Flags") {
            let n = f.entier().ok_or_else(|| {
                format!("Impossible de convertir la valeur « {} » en type « System.Int32 ».", f.texte())
            })?;
            l.push(Declaration::reg(p, "Flags", ValeurReg::Texte((n & !4).to_string()), "String"));
        }
    }
    Ok(l)
}

/// `Get-ChangementWindowedOptim`
pub fn changement_fenetre(pc: &dyn Pc) -> Vec<Declaration> {
    let cur = texte_de(&valeur(pc, DX, "DirectXUserGlobalSettings"));
    let mut parts: Vec<String> = cur
        .split(';')
        .filter(|p| !p.is_empty() && !p.to_lowercase().starts_with("swapeffectupgradeenable="))
        .map(String::from)
        .collect();
    parts.push("SwapEffectUpgradeEnable=1".into());
    vec![Declaration::reg(DX, "DirectXUserGlobalSettings", ValeurReg::Texte(format!("{};", parts.join(";"))), "String")]
}

// ---------------------------------------------------------------------------
// Type de PC
// ---------------------------------------------------------------------------
static ONDULEUR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bups\b|onduleur|back-?ups|smart-?ups|\bapc\b|eaton|cyberpower|powerwalker|bluewalker|salicru|riello|infosec|tripp.?lite|vertiv|liebert|ablerex|powercom|mustek|legrand|socomec|\bcp\d{3,4}|\bbr\d{3,4}|\bbx\d{3,4}|\bbe\d{3,4}|\bvi ?\d{3,4}").unwrap()
});

/// `Test-IsUps` : une batterie au plomb, ou un nom d'onduleur
pub fn est_onduleur(b: &serde_json::Value) -> bool {
    use crate::net::{champ_entier, champ_texte};
    if b.is_null() {
        return false;
    }
    if champ_entier(b, "Chemistry") == 3 {
        return true;
    }
    ONDULEUR.is_match(&format!(
        "{} {} {}",
        champ_texte(b, "Name"),
        champ_texte(b, "DeviceID"),
        champ_texte(b, "Description")
    ))
}

const MOBILES: [i64; 11] = [8, 9, 10, 11, 12, 14, 18, 21, 30, 31, 32];
const BUREAUX: [i64; 13] = [3, 4, 5, 6, 7, 13, 15, 16, 17, 23, 24, 35, 36];

fn boitiers(data: &serde_json::Value) -> Vec<i64> {
    crate::net::champ_liste(data, "Chassis").iter().map(|v| crate::net::entier(crate::net::nombre_de(v))).collect()
}

/// `Test-IsDesktop`
pub fn est_fixe(data: &serde_json::Value) -> bool {
    let c = boitiers(data);
    if crate::net::champ_entier(data, "PCType") == 2 {
        return false;
    }
    if c.iter().any(|x| MOBILES.contains(x)) {
        return false;
    }
    c.iter().any(|x| BUREAUX.contains(x))
}

/// `Test-IsLaptop` (avec les données de l'analyse)
pub fn est_portable(batteries: &[serde_json::Value], data: &serde_json::Value) -> bool {
    let b: Vec<&serde_json::Value> = batteries.iter().filter(|x| !est_onduleur(x)).collect();
    let c = boitiers(data);
    if crate::net::champ_entier(data, "PCType") == 2 || c.iter().any(|x| MOBILES.contains(x)) {
        return true;
    }
    if c.iter().any(|x| BUREAUX.contains(x)) {
        return false;
    }
    !b.is_empty()
}

/// Les marques d'onduleurs d'après l'identifiant USB (`$UpsVendors`)
pub fn marque_usb(vid: &str) -> Option<&'static str> {
    let v = vid.to_uppercase();
    Some(match v.as_str() {
        "051D" => "APC",
        "0463" => "Eaton",
        "0764" => "CyberPower",
        "09AE" => "Tripp Lite",
        "10AF" => "Liebert (Vertiv)",
        "06DA" => "PowerWalker, Salicru ou Riello",
        "0D9F" => "Powercom",
        "0665" | "0925" => "onduleur générique",
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Programmes au démarrage
// ---------------------------------------------------------------------------
/// Une entrée de démarrage (`Read-StartupItems`)
#[derive(Debug, Clone, PartialEq)]
pub struct Demarrage {
    pub nom: String,
    pub source: String,
    pub commande: String,
    pub approuve: String,
    pub nom_valeur: String,
    pub actif: bool,
    pub exe: String,
}

/// `Test-StartupEnabled` : le premier octet de StartupApproved, bit 1 = désactivé
pub fn demarrage_actif(pc: &dyn Pc, approuve: &str, nom: &str) -> bool {
    match valeur(pc, approuve, nom) {
        Some(ValeurReg::Octets(o)) if !o.is_empty() => o[0] & 1 == 0,
        _ => true,
    }
}

pub static SANS_RISQUE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(Blitz|Discord|Steam|Epic ?Games|EpicGamesLauncher|Spotify|OneDrive|Teams|Skype|EADesktop|EA app|Origin|Battle\.net|Ubisoft|Uplay|GOG Galaxy|GalaxyClient|Riot ?Client|Overwolf|Medal|Zoom|WhatsApp|Telegram|Messenger|CCleaner|MicrosoftEdgeAutoLaunch|Opera|Brave|Adobe Creative Cloud|CCXProcess|AdobeGCInvoker)\b|MicrosoftEdgeAutoLaunch|\bEA\b|EALauncher").unwrap()
});

/// `Get-ChangementDemarrage` : sa valeur dans StartupApproved (2 : activé, 3 : désactivé), comme le Gestionnaire des tâches
pub fn changement_demarrage(d: &Demarrage, activer: bool) -> Declaration {
    let mut o = vec![0u8; 12];
    o[0] = if activer { 2 } else { 3 };
    Declaration::reg(&d.approuve, &d.nom_valeur, ValeurReg::Octets(o), "Binary")
}

/// `Get-ChangementsProfil $jeu 'gpu' $true` : la carte graphique puissante pour chaque exécutable
pub fn changements_profil_gpu(exes: &[String]) -> Vec<Declaration> {
    exes.iter().map(|e| Declaration::reg(DX, e, ValeurReg::Texte("GpuPreference=2;".into()), "String")).collect()
}

/// `Get-GpuPreference`
pub fn preference_gpu(pc: &dyn Pc, exe: &str) -> String {
    texte_de(&valeur(pc, DX, exe))
}
