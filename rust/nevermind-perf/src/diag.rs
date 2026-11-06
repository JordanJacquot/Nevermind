//! Le diagnostic des FPS d'une partie (diagnostic-fps.ps1) : les relevés pris pendant la partie (Add-FpsSysSample,
//! Merge-FpsProcSample), leur résumé enregistré avec la partie (Get-FpsDiagData), puis ce qu'il en dit (Get-FpsDiagnosis) :
//! qui freine, et ce qui se corrige, en commençant toujours par ce qui coûte le moins en qualité.

use crate::jeu::gpus_reelles;
use crate::pages::{ch, liste, nb, vrai};
use crate::texte::{egal, ps_int, ps_round};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// Un relevé du système (toutes les 5 s, jeu au premier plan) ; `None` : pas mesuré
#[derive(Clone, Copy, Debug, Default)]
pub struct Releve {
    pub cpu: Option<f64>,
    pub perf: Option<f64>,
    pub ram: Option<f64>,
    pub gpu: Option<f64>,
    pub temp: Option<f64>,
    pub vram: Option<f64>,
    pub power: Option<f64>,
    pub disk: Option<f64>,
}

/// Les relevés d'une partie
#[derive(Debug, Default)]
pub struct Releves {
    pub sys: Vec<Releve>,
    /// Temps de processeur par programme (nom, secondes), noms sans tenir compte des majuscules (une table de
    /// PowerShell)
    pub proc_cpu: Vec<(String, f64)>,
    pub proc_mem: Vec<(String, f64)>,
    pub proc_secondes: f64,
    prec: Option<HashMap<i64, (String, f64, f64)>>,
    prec_t: f64,
    pub sur_batterie: bool,
    pub chemin: String,
}

fn ajouter(l: &mut Vec<(String, f64)>, nom: &str, v: f64, garder_max: bool) {
    match l.iter_mut().find(|(n, _)| egal(n, nom)) {
        Some(x) => {
            if garder_max {
                if v > x.1 {
                    x.1 = v;
                }
            } else {
                x.1 += v;
            }
        }
        None => l.push((nom.to_string(), v)),
    }
}

impl Releves {
    /// Merge-FpsProcSample : un relevé des programmes (« pid|nom|secondes de processeur|mémoire ») pris à `t` secondes
    pub fn programmes(&mut self, lignes: &[String], t: f64) {
        let mut cur: HashMap<i64, (String, f64, f64)> = HashMap::new();
        let mut ordre: Vec<i64> = Vec::new();
        for l in lignes {
            let x: Vec<&str> = l.split('|').collect();
            if x.len() < 4 {
                continue;
            }
            let (Ok(id), Ok(c), Ok(m)) =
                (x[0].trim().parse::<i64>(), x[2].trim().parse::<f64>(), x[3].trim().parse::<f64>())
            else {
                continue;
            };
            if !cur.contains_key(&id) {
                ordre.push(id);
            }
            cur.insert(id, (x[1].to_string(), c, m));
        }
        if let Some(prec) = &self.prec {
            for id in &ordre {
                let Some(a) = prec.get(id) else { continue };
                let c = &cur[id];
                let d = c.1 - a.1;
                if d <= 0.0 {
                    continue;
                }
                ajouter(&mut self.proc_cpu, &c.0, d, false);
            }
            self.proc_secondes += t - self.prec_t;
        }
        let mut mem: Vec<(String, f64)> = Vec::new();
        for id in &ordre {
            let c = &cur[id];
            ajouter(&mut mem, &c.0, c.2, false);
        }
        for (n, v) in mem {
            ajouter(&mut self.proc_mem, &n, v, true);
        }
        self.prec = Some(cur);
        self.prec_t = t;
    }
}

/// Un disque du PC : ses lettres, et son type (« SSD », « HDD »...)
pub struct Disque {
    pub lettres: Vec<String>,
    pub bus: String,
    pub media: String,
}

/// Ce que Get-FpsDiagData lit en plus des relevés
pub struct ContexteDiag<'a> {
    pub coeurs: usize,
    pub disques: &'a [Disque],
    /// GetDisplays : « nom|carte|largeur|hauteur|Hz|Hz max|principal »
    pub ecrans: &'a [String],
    pub gpus: &'a [Value],
    pub portable: bool,
    pub onduleur: bool,
    pub mode_jeu: bool,
}

fn stat(sys: &[Releve], f: impl Fn(&Releve) -> Option<f64>, comment: &str) -> Value {
    let v: Vec<f64> = sys.iter().filter_map(&f).collect();
    if v.is_empty() {
        return Value::Null;
    }
    let mut somme = 0.0;
    for x in &v {
        somme += x;
    }
    let r = match comment {
        "max" => v.iter().copied().fold(f64::MIN, f64::max),
        "min" => v.iter().copied().fold(f64::MAX, f64::min),
        _ => somme / v.len() as f64,
    };
    json!(ps_round(r, 1))
}

/// Get-FpsDiagData : le résumé des relevés, enregistré avec la partie (champ « Diag » de fps.json)
pub fn donnees(t: &Releves, occupe: &[f64; 3], c: &ContexteDiag) -> Value {
    let coeurs = c.coeurs.max(1) as f64;
    let secs = t.proc_secondes.max(1.0);
    let mut top: Vec<(String, f64)> = t
        .proc_cpu
        .iter()
        .map(|(n, v)| (n.clone(), ps_round(100.0 * v / secs / coeurs, 1)))
        .filter(|x| x.1 >= 1.0)
        .collect();
    top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    top.truncate(6);
    let mut top_mem = t.proc_mem.clone();
    top_mem.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    top_mem.truncate(6);
    let chemin = &t.chemin;
    let lecteur = if chemin.chars().count() > 1 {
        chemin.chars().next().map(|x| x.to_uppercase().to_string()).unwrap_or_default()
    } else {
        String::new()
    };
    let mut media = String::new();
    for d in c.disques {
        if !lecteur.is_empty() && d.lettres.iter().any(|l| egal(l, &lecteur)) {
            media = if egal(&d.bus, "NVMe") || egal(&d.media, "SSD") {
                "SSD".into()
            } else if egal(&d.media, "HDD") {
                "HDD".into()
            } else {
                d.media.clone()
            };
        }
    }
    let (mut hz, mut hz_max, mut ecran) = (0i64, 0i64, String::new());
    if let Some(p) = c.ecrans.iter().find(|e| e.split('|').nth(6) == Some("1")) {
        let x: Vec<&str> = p.split('|').collect();
        hz = x.get(4).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        hz_max = x.get(5).and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        ecran = x[0].to_string();
    }
    let noms = gpus_reelles(c.gpus);
    let a = |m: &[&str]| {
        noms.iter().any(|n| {
            let l = n.to_lowercase();
            m.iter().any(|x| l.contains(x))
        })
    };
    let mut o = Map::new();
    o.insert("CpuRatio".into(), json!(ps_round(occupe[0], 3)));
    o.insert("GpuRatio".into(), json!(ps_round(occupe[1], 3)));
    o.insert("Stutters".into(), json!(ps_int(occupe[2])));
    let s = &t.sys;
    o.insert("Cpu".into(), stat(s, |r| r.cpu, "avg"));
    o.insert("CpuMax".into(), stat(s, |r| r.cpu, "max"));
    o.insert("Perf".into(), stat(s, |r| r.perf, "avg"));
    o.insert("Ram".into(), stat(s, |r| r.ram, "max"));
    o.insert("Gpu".into(), stat(s, |r| r.gpu, "avg"));
    o.insert("Temp".into(), stat(s, |r| r.temp, "max"));
    o.insert("Vram".into(), stat(s, |r| r.vram, "max"));
    o.insert("Power".into(), stat(s, |r| r.power, "avg"));
    o.insert("Disk".into(), stat(s, |r| r.disk, "max"));
    o.insert("DiskAvg".into(), stat(s, |r| r.disk, "avg"));
    o.insert("OnBattery".into(), json!(t.sur_batterie));
    o.insert("Top".into(), Value::Array(top.iter().map(|(n, p)| json!({ "Name": n, "Pct": p })).collect()));
    o.insert(
        "TopMem".into(),
        Value::Array(top_mem.iter().map(|(n, v)| json!({ "Name": n, "MB": ps_int(v / 1048576.0) })).collect()),
    );
    o.insert("Path".into(), json!(chemin));
    o.insert("Drive".into(), json!(lecteur));
    o.insert("Media".into(), json!(media));
    o.insert("Hz".into(), json!(hz));
    o.insert("HzMax".into(), json!(hz_max));
    o.insert("Display".into(), json!(ecran));
    o.insert("Laptop".into(), json!(c.portable));
    o.insert("Dual".into(), json!(noms.len() >= 2));
    o.insert("Nvidia".into(), json!(a(&["nvidia", "geforce"])));
    o.insert("Rtx".into(), json!(a(&["rtx"])));
    o.insert("Amd".into(), json!(a(&["radeon", "amd"])));
    o.insert("GameMode".into(), json!(c.mode_jeu));
    o.insert("Ups".into(), json!(c.onduleur));
    Value::Object(o)
}

// ---------------------------------------------------------------------------
// Get-FpsDiagnosis
// ---------------------------------------------------------------------------

/// Programmes du système qu'on ne propose jamais de fermer ($SystemProcs)
const SYSTEME: [&str; 47] = [
    "System",
    "Idle",
    "Registry",
    "Memory Compression",
    "smss",
    "csrss",
    "wininit",
    "winlogon",
    "services",
    "lsass",
    "svchost",
    "dwm",
    "explorer",
    "fontdrvhost",
    "sihost",
    "ctfmon",
    "conhost",
    "audiodg",
    "spoolsv",
    "SearchHost",
    "StartMenuExperienceHost",
    "ShellExperienceHost",
    "RuntimeBroker",
    "TextInputHost",
    "dllhost",
    "WmiPrvSE",
    "MsMpEng",
    "NisSrv",
    "SecurityHealthService",
    "SgrmBroker",
    "powershell",
    "Nevermind",
    "NevermindAide",
    "Mindset",
    "PresentMon",
    "nvcontainer",
    "NVDisplay.Container",
    "amdfendrsr",
    "atiesrxx",
    "atieclxx",
    "steam",
    "steamwebhelper",
    "EpicGamesLauncher",
    "BEService",
    "vgc",
    "vgk",
    "msedgewebview2",
];

pub fn programme_systeme(nom: &str) -> bool {
    let l = nom.to_lowercase();
    SYSTEME.iter().any(|s| egal(s, nom)) || l.starts_with("easyanticheat") || l.starts_with("eaanticheat")
}
fn navigateur(nom: &str) -> bool {
    ["chrome", "msedge", "firefox", "opera", "brave", "vivaldi"].iter().any(|b| egal(b, nom))
}

/// Le paramètre d'une correction : un jeu, un texte (nom de programme, adresse), un écran de l'analyse, ou rien
#[derive(Clone, Debug, PartialEq)]
pub enum ArgDiag {
    Rien,
    Texte(String),
    Jeu,
    Ecran,
}

/// Une correction proposée (un bouton)
#[derive(Clone, Debug, PartialEq)]
pub struct ActionDiag {
    /// Ce que fait le bouton : gpu, alimentation, fermer, modejeu, ecran, priorite, pilote
    pub quoi: &'static str,
    pub texte: String,
    pub arg: ArgDiag,
    pub confirmer: Option<String>,
}

/// Une cause trouvée
#[derive(Clone, Debug, PartialEq)]
pub struct ElementDiag {
    pub statut: &'static str,
    pub titre: String,
    pub detail: String,
    pub lignes: Vec<String>,
    pub actions: Vec<ActionDiag>,
}

/// Le diagnostic d'une partie
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub limite: &'static str,
    pub probleme: bool,
    pub statut: &'static str,
    pub titre: String,
    pub texte: String,
    pub elements: Vec<ElementDiag>,
    pub palier: Option<i64>,
}

/// Ce que le diagnostic lit en dehors de la partie
pub struct ContexteDiagnostic {
    /// Test-PowerPlan : le mode d'alimentation est bon (vrai si inconnu)
    pub plan_ok: bool,
    /// Le jeu est connu de Nevermind (un profil peut lui être fait)
    pub jeu_connu: bool,
    /// Test-GamePriority du jeu
    pub prio: bool,
    /// Le nom du jeu affiché
    pub nom_jeu: String,
    /// D'autres parties du même jeu ont été mesurées
    pub autres_parties: usize,
    /// L'analyse a un réglage d'écran à faire pour l'écran de la partie (display:<écran>, pas « ok »)
    pub ecran_a_regler: bool,
    /// Pilote NVIDIA : (version installée, dernière version, adresse de la page)
    pub nvidia: Option<(Option<String>, String, String)>,
}

fn version(t: &str) -> Option<Vec<u64>> {
    let p: Option<Vec<u64>> = t.trim().split('.').map(|x| x.parse().ok()).collect();
    p.filter(|v| (2..=4).contains(&v.len()))
}

fn el(
    statut: &'static str,
    titre: impl Into<String>,
    detail: impl Into<String>,
    lignes: Vec<String>,
    actions: Vec<ActionDiag>,
) -> ElementDiag {
    ElementDiag {
        statut,
        titre: titre.into(),
        detail: detail.into(),
        lignes: lignes.into_iter().filter(|l| !l.is_empty()).collect(),
        actions,
    }
}

/// Un nombre dans un texte de PowerShell (`"$x"`, culture invariante)
fn invariant(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") }
}

fn opt(d: &Value, k: &str) -> Option<f64> {
    d.get(k).filter(|v| !v.is_null()).map(|v| nb(Some(v)))
}

/// Get-FpsDiagnosis. `s` : la partie (Avg, Low1, Seconds, Series, Diag, Key).
pub fn diagnostic(s: &Value, c: &ContexteDiagnostic) -> Diagnostic {
    let Some(d) = s.get("Diag").filter(|d| !d.is_null()) else {
        return Diagnostic {
            limite: "unknown",
            probleme: false,
            statut: "info",
            titre: "Pas de diagnostic pour cette partie".into(),
            texte: "Elle a été mesurée par une ancienne version de Nevermind. Rejoue une partie pour avoir le diagnostic complet.".into(),
            elements: Vec::new(),
            palier: None,
        };
    };
    let avg = nb(s.get("Avg"));
    let low = nb(s.get("Low1"));
    let secondes = nb(s.get("Seconds"));
    let per_min = nb(d.get("Stutters")) / (secondes / 60.0).max(1.0);
    let low_fps = avg < 60.0;
    let stutter = low < 0.5 * avg || per_min > 6.0;
    let problem = low_fps || stutter;
    let jeu = c.jeu_connu;
    let nom = &c.nom_jeu;
    let gpu = opt(d, "Gpu");
    let gpu_ratio = nb(d.get("GpuRatio"));
    let cpu_ratio = nb(d.get("CpuRatio"));
    let hz = nb(d.get("Hz"));
    let hz_max = nb(d.get("HzMax"));

    // Qui freine ? FPS bloqués : moyenne sur une valeur ronde (ou la fréquence de l'écran) et courbe presque plate
    let series: Vec<f64> = liste(s.get("Series")).iter().filter(|x| !x.is_null()).map(|x| nb(Some(x))).collect();
    let mut flat = low >= 0.75 * avg;
    if series.len() >= 10 {
        let mut somme = 0.0;
        for x in &series {
            somme += x;
        }
        let mean = somme / series.len() as f64;
        let mut var = 0.0;
        for x in &series {
            var += (x - mean) * (x - mean);
        }
        flat = flat && mean > 0.0 && (var / series.len() as f64).sqrt() / mean < 0.02;
    }
    let mut cap: Option<i64> = None;
    for cv in [30.0, 60.0, 75.0, 90.0, 100.0, 120.0, 144.0, 165.0, 170.0, 180.0, 200.0, 240.0, 280.0, 360.0, hz] {
        if flat && cv != 0.0 && (avg - cv).abs() <= 1.5f64.max(cv * 0.03) {
            cap = Some(ps_int(cv));
            break;
        }
    }
    let gpu_bound = gpu_ratio >= 0.85 || (gpu_ratio < 0.0 && gpu.is_some_and(|g| g >= 93.0));
    let igpu = vrai(d.get("Dual")) && vrai(d.get("Nvidia")) && gpu.is_some_and(|g| g < 10.0) && avg < 60.0;
    let cpu_bound = !gpu_bound
        && (cpu_ratio >= 0.85 || (0.0..0.7).contains(&gpu_ratio) || (gpu_ratio < 0.0 && gpu.is_some_and(|g| g < 75.0)));
    let limite = if igpu {
        "igpu"
    } else if cap.is_some() && !gpu_bound {
        "cap"
    } else if gpu_bound {
        "gpu"
    } else if cpu_bound {
        "cpu"
    } else {
        "mixed"
    };
    let gpu_label = gpu.map(|g| format!(" (utilisée à {} %)", ps_int(g))).unwrap_or_default();
    let (titre, mut texte): (String, String) = match limite {
        "igpu" => ("Le jeu tourne sur la petite puce graphique".into(), "Ton PC a deux cartes graphiques, et le jeu utilise la moins puissante (celle intégrée au processeur). La grosse carte est restée presque au repos.".into()),
        "cap" => {
            let cp = cap.unwrap_or(0);
            (
                format!("Tes FPS sont bloqués à {cp}"),
                if hz_max != 0.0 && cp as f64 >= hz_max {
                    format!("C'est la fréquence de ton écran ({} Hz) : afficher plus d'images ne servirait à rien. C'est idéal.", invariant(hz_max))
                } else {
                    "Une limite de FPS ou la synchronisation verticale (V-Sync) bloque les images. Ce n'est pas ton PC qui peine.".into()
                },
            )
        }
        "gpu" => (format!("C'est ta carte graphique qui décide des FPS{gpu_label}"), "Elle travaille à fond pour dessiner chaque image. Pour gagner des FPS, il faut lui demander un peu moins de travail dans les options graphiques du jeu.".into()),
        "cpu" => ("C'est ton processeur qui limite les FPS".into(), "La carte graphique attend que le processeur lui prépare les images. Des programmes en arrière-plan, un mode d'économie d'énergie ou certains réglages du jeu peuvent en être la cause.".into()),
        _ => ("Ni la carte graphique ni le processeur ne sont à fond".into(), "Les FPS sont limités par autre chose : le jeu lui-même, une limite de FPS, ou des petits blocages (disque, mémoire).".into()),
    };
    if !problem && limite != "cap" {
        texte = format!("Ta partie était fluide. {texte}");
    }

    let mut items: Vec<ElementDiag> = Vec::new();
    let batterie = vrai(d.get("OnBattery"));
    if batterie && vrai(d.get("Ups")) && !vrai(d.get("Laptop")) {
        items.push(el("bad", "Coupure de courant pendant la partie", "Ton PC tournait sur l'onduleur : Windows peut alors brider le PC, et l'onduleur ne tient que quelques minutes.", vec!["Quand le courant est coupé, enregistre et quitte ta partie.".into()], vec![]));
    } else if batterie {
        items.push(el("bad", "Le PC était sur batterie", "Sur batterie, Windows bride le processeur et la carte graphique pour tenir plus longtemps : les FPS chutent.", vec!["Branche le chargeur quand tu joues.".into()], vec![]));
    }
    if limite == "igpu" && jeu {
        items.push(el("bad", format!("Forcer la grosse carte graphique pour {nom}"), "Windows choisit parfois la puce intégrée pour économiser la batterie. Nevermind peut obliger le jeu à utiliser la carte puissante.", vec!["Relance le jeu après le réglage.".into()],
            vec![ActionDiag { quoi: "gpu", texte: "Utiliser la carte puissante".into(), arg: ArgDiag::Jeu, confirmer: None }]));
    }
    let perf = opt(d, "Perf");
    let cpu = opt(d, "Cpu");
    if !c.plan_ok && (limite == "cpu" || stutter || low_fps) {
        items.push(el(
            "warn",
            "Windows est en mode économie d'énergie",
            "Le processeur ralentit dès qu'il peut, ce qui donne des chutes de FPS et des micro-saccades.",
            vec![],
            vec![ActionDiag {
                quoi: "alimentation",
                texte: "Mettre en mode performances".into(),
                arg: ArgDiag::Rien,
                confirmer: None,
            }],
        ));
    } else if let Some(p) = perf
        && p < 85.0
        && cpu.is_some_and(|x| x >= 50.0)
    {
        items.push(el("warn", "Le processeur a ralenti pendant la partie", format!("Il tournait en moyenne à {} % de sa vitesse normale alors qu'il était très occupé : souvent une surchauffe.", ps_int(p)),
            vec!["Dépoussière les grilles et les ventilateurs du PC.".into(), "Vérifie que les ventilateurs du processeur tournent.".into(), "Sur un portable, pose-le sur une surface dure (pas sur un lit ou un coussin).".into()], vec![]));
    }
    if let Some(t) = opt(d, "Temp")
        && t >= 83.0
    {
        items.push(el(
            "warn",
            format!("Ta carte graphique a chauffé ({} °C)", ps_int(t)),
            "Au-delà de 83 °C environ, elle baisse sa vitesse pour se protéger, et les FPS avec.",
            vec![
                "Dépoussière la carte et les entrées d'air du boîtier.".into(),
                "Laisse de l'espace autour du PC pour qu'il respire.".into(),
                "Si le boîtier a peu de ventilateurs, en ajouter un aide beaucoup.".into(),
            ],
            vec![],
        ));
    }
    if let Some(v) = opt(d, "Vram")
        && v >= 95.0
    {
        items.push(el("warn", "La mémoire de ta carte graphique était pleine", format!("Elle était remplie à {} % : le jeu doit alors aller chercher ses textures plus loin, ce qui donne des saccades.", ps_int(v)),
            vec!["Dans le jeu, baisse la qualité des textures d'un cran (ex : Ultra vers Élevé). La différence se voit à peine.".into(), "Ferme les vidéos et les navigateurs pendant que tu joues : ils utilisent aussi cette mémoire.".into()], vec![]));
    }
    let cle = ch(s.get("Key"));
    if let Some(r) = opt(d, "Ram")
        && r >= 90.0
    {
        let lourds: Vec<Value> = liste(d.get("TopMem"))
            .into_iter()
            .filter(|x| {
                let n = ch(x.get("Name"));
                !programme_systeme(&n) && !egal(&n, &cle)
            })
            .take(4)
            .collect();
        let actions = lourds
            .iter()
            .take(2)
            .map(|x| {
                let n = ch(x.get("Name"));
                let onglets = if navigateur(&n) {
                    "\n\nTes onglets seront proposés à la réouverture du navigateur."
                } else {
                    ""
                };
                ActionDiag {
                    quoi: "fermer",
                    texte: format!("Fermer {n}"),
                    arg: ArgDiag::Texte(n.clone()),
                    confirmer: Some(format!(
                        "Fermer tous les {n} ?{onglets} Si tu as du travail pas enregistré dedans, ferme-le toi-même."
                    )),
                }
            })
            .collect();
        let lignes = lourds
            .iter()
            .map(|x| format!("{} : {} Go", ch(x.get("Name")), invariant(ps_round(nb(x.get("MB")) / 1024.0, 1))))
            .collect();
        items.push(el(
            "warn",
            format!("La mémoire vive était presque pleine ({} %)", ps_int(r)),
            "Quand elle est pleine, Windows déplace des données sur le disque et le jeu saccade.",
            lignes,
            actions,
        ));
    }
    let top = liste(d.get("Top"));
    let bg: Vec<&Value> = top
        .iter()
        .filter(|x| {
            let n = ch(x.get("Name"));
            !["System", "Idle", "dwm", "audiodg", "MsMpEng", "csrss"].iter().any(|s| egal(s, &n))
                && nb(x.get("Pct")) >= 4.0
        })
        .collect();
    if !bg.is_empty() && (limite == "cpu" || stutter) {
        let mut actions: Vec<ActionDiag> = bg
            .iter()
            .filter(|p| !programme_systeme(&ch(p.get("Name"))))
            .take(2)
            .map(|p| {
                let n = ch(p.get("Name"));
                ActionDiag {
                    quoi: "fermer",
                    texte: format!("Fermer {n}"),
                    arg: ArgDiag::Texte(n.clone()),
                    confirmer: Some(format!(
                        "Fermer tous les {n} ? Si tu as du travail pas enregistré dedans, ferme-le toi-même."
                    )),
                }
            })
            .collect();
        if !vrai(d.get("GameMode")) {
            actions.push(ActionDiag {
                quoi: "modejeu",
                texte: "Activer le mode jeu".into(),
                arg: ArgDiag::Rien,
                confirmer: None,
            });
        }
        let mut lignes: Vec<String> = bg
            .iter()
            .map(|x| format!("{} : {} % du processeur", ch(x.get("Name")), invariant(ps_round(nb(x.get("Pct")), 0))))
            .collect();
        if top.iter().any(|x| ch(x.get("Name")).eq_ignore_ascii_case("MsMpEng") && nb(x.get("Pct")) >= 4.0) {
            lignes.push("MsMpEng, c'est l'antivirus : une analyse tournait pendant la partie. Lance les analyses quand tu ne joues pas.".into());
        }
        items.push(el(
            "warn",
            "Des programmes en arrière-plan utilisaient ton processeur",
            "Pendant que tu jouais, ces programmes prenaient du temps de calcul au jeu.",
            lignes,
            actions,
        ));
    }
    let media = ch(d.get("Media"));
    if stutter && (egal(&media, "HDD") || opt(d, "Disk").is_some_and(|x| x >= 90.0)) {
        let lignes = if egal(&media, "HDD") {
            vec![
                format!("Le jeu est installé sur un disque dur classique ({}:), bien plus lent qu'un SSD.", ch(d.get("Drive"))),
                "Steam : clic droit sur le jeu > Propriétés > Fichiers installés > Déplacer le dossier d'installation, vers un SSD.".into(),
                "Epic : désinstalle puis réinstalle le jeu sur un SSD.".into(),
            ]
        } else {
            vec![
                "Le disque était occupé à 100 % par moments pendant la partie.".into(),
                "Évite les téléchargements et les mises à jour (Steam, Windows) pendant que tu joues.".into(),
            ]
        };
        items.push(el(
            "warn",
            "Le disque ralentit le jeu",
            "Les saccades arrivent quand le jeu doit charger des données et que le disque n'arrive pas à suivre.",
            lignes,
            vec![],
        ));
    }
    if limite == "cap"
        && let Some(cp) = cap
        && hz_max > cp as f64
    {
        let disp = vrai(d.get("Display")) && c.ecran_a_regler;
        if cp <= 60 && hz <= 60.0 && hz_max > 60.0 && disp {
            items.push(el("warn", format!("Ton écran est réglé en {} Hz alors qu'il peut faire {} Hz", invariant(hz), invariant(hz_max)), "L'écran n'affiche que 60 images par seconde : le jeu se cale dessus. En le passant à sa vraie fréquence, tu gagnes en fluidité sans rien perdre.", vec![],
                vec![ActionDiag { quoi: "ecran", texte: "Régler l'écran".into(), arg: ArgDiag::Ecran, confirmer: None }]));
        } else {
            items.push(el("info", format!("Une limite bloque le jeu à {cp} FPS"), format!("Ton écran peut afficher {} images par seconde.", invariant(hz_max)),
                vec!["Dans les options graphiques du jeu, désactive la synchronisation verticale (V-Sync), ou monte la limite de FPS.".into(), format!("Idéal : limite de FPS à {} (la fréquence de ton écran), avec G-Sync / FreeSync si ton écran le gère.", invariant(hz_max))], vec![]));
        }
    }
    let rtx = vrai(d.get("Rtx"));
    let amd = vrai(d.get("Amd"));
    let nvidia = vrai(d.get("Nvidia"));
    if limite == "gpu" && problem {
        let up = if rtx {
            "Active le DLSS en mode « Qualité » (ta carte NVIDIA RTX le gère) : souvent +30 à 50 % de FPS, image presque identique."
        } else if amd {
            "Active le FSR en mode « Qualité » : souvent +30 à 40 % de FPS, image presque identique."
        } else {
            "Active le DLSS, le FSR ou le XeSS en mode « Qualité » s'ils sont proposés : souvent +30 % de FPS, image presque identique."
        };
        items.push(el("info", "Les réglages du jeu qui font gagner le plus, pour le moins de perte", "À changer dans les options graphiques du jeu, dans cet ordre. Rejoue après chaque étape : arrête toi dès que c'est fluide.",
            vec![up.into(), "Ombres : de Ultra à Moyen (+10 à 20 %, se voit peu en jeu).".into(), "Effets volumétriques, nuages, brouillard : Bas (+10 %).".into(), "Réflexions et ray tracing : désactivés ou Bas (le ray tracing peut coûter 30 à 50 %).".into(),
                "Occlusion ambiante : Moyen.".into(), "Garde les textures au maximum si la mémoire de ta carte suffit : elles coûtent très peu de FPS.".into(), "En dernier recours seulement : résolution de rendu à 85 ou 90 %.".into()], vec![]));
    }
    if limite == "cpu" && problem {
        let mut actions = Vec::new();
        if jeu && !c.prio {
            actions.push(ActionDiag {
                quoi: "priorite",
                texte: "Donner la priorité au jeu".into(),
                arg: ArgDiag::Jeu,
                confirmer: None,
            });
        }
        let dernier = if nvidia {
            "Active NVIDIA Reflex s'il est proposé (mode « Activé »)."
        } else if amd {
            "Active AMD Anti-Lag dans le logiciel AMD."
        } else {
            ""
        };
        items.push(el("info", "Les réglages du jeu qui soulagent le processeur", "Ta carte graphique a de la marge : tu peux même monter les effets visuels sans perdre de FPS. Ce qui compte ici :",
            vec!["Distance d'affichage (ou de vue) : d'un cran plus bas.".into(), "Densité de foule, de végétation ou de personnages : Moyen.".into(), "Qualité de la physique ou des particules : Moyen.".into(), dernier.into()], actions));
    }
    if stutter && !items.iter().any(|i| i.statut == "bad" || i.statut == "warn") && c.autres_parties == 0 {
        items.push(el("info", "Première partie mesurée sur ce jeu", "Les premières minutes d'un jeu (et après une mise à jour du pilote graphique) saccadent souvent : le jeu prépare ses effets visuels. Rejoue une partie pour comparer.", vec![], vec![]));
    }
    if problem
        && nvidia
        && let Some((inst, dernier, url)) = &c.nvidia
        && let Some(i) = inst
        && let (Some(a), Some(b)) = (version(i), version(dernier))
        && a < b
    {
        items.push(el(
            "warn",
            format!("Pilote NVIDIA pas à jour ({i}, le dernier est le {dernier})"),
            "Les nouveaux pilotes corrigent souvent des chutes de FPS dans les jeux récents.",
            vec![],
            vec![ActionDiag {
                quoi: "pilote",
                texte: "Page du pilote".into(),
                arg: ArgDiag::Texte(url.clone()),
                confirmer: None,
            }],
        ));
    }
    if !problem {
        items.push(el(
            "ok",
            "Aucun problème de FPS sur cette partie",
            if limite == "gpu" {
                "Si tu veux encore plus de FPS un jour, commence par le DLSS ou le FSR en mode Qualité."
            } else {
                "Rien à régler."
            },
            vec![],
            vec![],
        ));
    }
    let statut = if items.iter().any(|i| i.statut == "bad") {
        "bad"
    } else if problem {
        "warn"
    } else {
        "ok"
    };
    Diagnostic { limite, probleme: problem, statut, titre, texte, elements: items, palier: cap }
}

impl Diagnostic {
    /// En JSON, pour l'écran (et les juges : le paramètre d'un jeu ou d'un écran s'écrit « objet »)
    pub fn json(&self) -> Value {
        json!({
            "limite": self.limite, "probleme": self.probleme, "statut": self.statut, "titre": self.titre, "texte": self.texte, "palier": self.palier,
            "elements": self.elements.iter().map(|e| json!({ "statut": e.statut, "titre": e.titre, "detail": e.detail, "lignes": e.lignes,
                "actions": e.actions.iter().map(|a| json!({ "quoi": a.quoi, "texte": a.texte, "arg": match &a.arg { ArgDiag::Rien => Value::Null, ArgDiag::Texte(t) => json!(t), ArgDiag::Jeu | ArgDiag::Ecran => json!("objet") }, "confirmer": a.confirmer })).collect::<Vec<_>>() })).collect::<Vec<_>>()
        })
    }
}
