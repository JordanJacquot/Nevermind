//! La mesure des FPS d'une partie (la classe FrameMon de natif.cs) : les lignes de PresentMon, lues à l'Aide
//! (« fps.lire »), donnent la durée de chaque image ; le compteur en direct, la courbe, et le bilan de la partie
//! (moyenne, 1 % bas, 0,1 % bas, paliers des menus mis à part, saccades, qui freine). Puis l'enregistrement dans
//! fps.json (Save-FpsSession) et la bulle de fin de partie (Complete-FpsTarget).
//!
//! Même calcul que le C#, dans le même ordre (les sommes sont faites sur les images triées, comme lui), pour que les
//! chiffres soient les mêmes au centième près.

use crate::texte::{duree_jeu, n0, ps_round};
use serde_json::{Value, json};
use std::collections::VecDeque;

/// Au-delà, une image gardée sur deux est retirée (très longues parties : la mémoire reste bornée)
pub const MAX_IMAGES: usize = 2_000_000;

/// Paliers : moments bloqués sur une valeur ronde (menus, chargements, cinématiques)
const CAPS: [f64; 20] = [
    24.0, 25.0, 30.0, 40.0, 45.0, 48.0, 50.0, 60.0, 72.0, 75.0, 90.0, 100.0, 120.0, 144.0, 165.0, 170.0, 175.0, 180.0,
    200.0, 240.0,
];
const FENETRE_PALIER: usize = 30;
const PALIER_MIN_SEC: f64 = 2.0;

/// Un petit tirage au hasard (xorshift), pour l'image gardée dans chaque groupe des très longues parties
#[derive(Debug)]
struct Hasard(u64);
impl Hasard {
    fn suivant(&mut self, n: usize) -> usize {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        if n == 0 { 0 } else { (x % n as u64) as usize }
    }
}

/// Les images d'une partie (FrameMon)
#[derive(Debug)]
pub struct Images {
    tout: Vec<f64>,
    dernieres1: VecDeque<f64>,
    dernieres10: VecDeque<f64>,
    somme1: f64,
    somme10: f64,
    somme_ech: f64,
    nb_ech: u64,
    pub max_images: usize,
    pas: usize,
    saut: usize,
    choix: usize,
    hasard: Hasard,
    total_ms: f64,
    total_n: u64,
    i_ft: Option<usize>,
    i_mode: Option<usize>,
    i_cpu: Option<usize>,
    i_gpu: Option<usize>,
    busy_ft: f64,
    cpu_busy: f64,
    gpu_busy: f64,
    busy_n: u64,
    /// Mode d'affichage vu par PresentMon (« Hardware: Legacy Flip » : vrai plein écran)
    pub dernier_mode: String,
    pub derniere_erreur: String,
    /// Le jeu n'est pas au premier plan : ses images ne comptent pas
    pub en_pause: bool,
}

impl Default for Images {
    fn default() -> Self {
        let graine = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1)
            | 1;
        Self {
            tout: Vec::new(),
            dernieres1: VecDeque::new(),
            dernieres10: VecDeque::new(),
            somme1: 0.0,
            somme10: 0.0,
            somme_ech: 0.0,
            nb_ech: 0,
            max_images: MAX_IMAGES,
            pas: 1,
            saut: 0,
            choix: 0,
            hasard: Hasard(graine),
            total_ms: 0.0,
            total_n: 0,
            i_ft: None,
            i_mode: None,
            i_cpu: None,
            i_gpu: None,
            busy_ft: 0.0,
            cpu_busy: 0.0,
            gpu_busy: 0.0,
            busy_n: 0,
            dernier_mode: String::new(),
            derniere_erreur: String::new(),
            en_pause: false,
        }
    }
}

/// `double.TryParse(..., NumberStyles.Float, InvariantCulture)`
fn lire_nombre(t: &str) -> Option<f64> {
    let t = t.trim();
    if t.is_empty() {
        return None;
    }
    let v: f64 = t.parse().ok()?;
    v.is_finite().then_some(v)
}

/// Le bilan d'une partie : { FPS moyen, 1 % bas, 0,1 % bas, images, secondes }, { secondes en palier, FPS du palier },
/// { part du processeur, part de la carte graphique (-1 si inconnues), saccades }
#[derive(Clone, Debug, PartialEq)]
pub struct Bilan {
    pub resume: [f64; 5],
    pub palier: [f64; 2],
    pub occupe: [f64; 3],
}

impl Images {
    /// Le nombre d'images gardées
    pub fn nombre(&self) -> usize {
        self.tout.len()
    }

    /// Une ligne de PresentMon (Feed). La première donne le nom des colonnes.
    pub fn ligne(&mut self, ligne: &str) {
        let cols: Vec<&str> = ligne.split(',').collect();
        if self.i_ft.is_none() {
            for (i, c) in cols.iter().enumerate() {
                let c = c.trim();
                if self.i_ft.is_none()
                    && matches!(c, "FrameTime" | "MsBetweenAppStart" | "msBetweenPresents" | "MsBetweenPresents")
                {
                    self.i_ft = Some(i);
                }
                if c == "PresentMode" {
                    self.i_mode = Some(i);
                }
                if c == "CPUBusy" || c == "MsCPUBusy" {
                    self.i_cpu = Some(i);
                }
                if c == "GPUBusy" || c == "MsGPUBusy" {
                    self.i_gpu = Some(i);
                }
            }
            return;
        }
        if let Some(m) = self.i_mode
            && m < cols.len()
        {
            self.dernier_mode = cols[m].to_string();
        }
        let i_ft = self.i_ft.unwrap_or(usize::MAX);
        if i_ft >= cols.len() || self.en_pause {
            return;
        }
        let Some(ft) = lire_nombre(cols[i_ft]) else { return };
        if ft <= 0.0 || ft > 5000.0 {
            return;
        }
        self.total_ms += ft;
        self.total_n += 1;
        let garder = self.saut == self.choix;
        self.saut += 1;
        if self.saut >= self.pas {
            self.saut = 0;
            self.choix = if self.pas > 1 { self.hasard.suivant(self.pas) } else { 0 };
        }
        if garder {
            self.tout.push(ft);
            if self.tout.len() >= self.max_images.max(1000) {
                self.eclaircir();
            }
        }
        self.dernieres1.push_back(ft);
        self.somme1 += ft;
        while self.somme1 > 1000.0 && self.dernieres1.len() > 1 {
            self.somme1 -= self.dernieres1.pop_front().unwrap_or(0.0);
        }
        self.dernieres10.push_back(ft);
        self.somme10 += ft;
        while self.somme10 > 10000.0 && self.dernieres10.len() > 1 {
            self.somme10 -= self.dernieres10.pop_front().unwrap_or(0.0);
        }
        self.somme_ech += ft;
        self.nb_ech += 1;
        if let (Some(c), Some(g)) = (self.i_cpu, self.i_gpu)
            && c < cols.len()
            && g < cols.len()
            && let (Some(cb), Some(gb)) = (lire_nombre(cols[c]), lire_nombre(cols[g]))
        {
            self.busy_ft += ft;
            self.cpu_busy += cb.min(ft);
            self.gpu_busy += gb.min(ft);
            self.busy_n += 1;
        }
    }

    fn eclaircir(&mut self) {
        let n = self.tout.len() / 2;
        for i in 0..n {
            let k = 2 * i + self.hasard.suivant(2);
            self.tout[i] = self.tout[k];
        }
        self.tout.truncate(n);
        self.pas *= 2;
        self.saut = 0;
        self.choix = self.hasard.suivant(self.pas);
    }

    /// FPS moyen depuis le dernier appel (un point de la courbe), -1 si aucune image entre temps
    pub fn echantillon(&mut self) -> f64 {
        let r = if self.somme_ech > 0.0 { self.nb_ech as f64 * 1000.0 / self.somme_ech } else { -1.0 };
        self.somme_ech = 0.0;
        self.nb_ech = 0;
        r
    }

    /// Pour le compteur : { FPS de la dernière seconde, 1 % bas des 10 dernières secondes, FPS moyen de la partie }
    pub fn direct(&self) -> [f64; 3] {
        let mut r = [0.0; 3];
        if !self.dernieres1.is_empty() && self.somme1 > 0.0 {
            r[0] = self.dernieres1.len() as f64 * 1000.0 / self.somme1;
        }
        let mut d: Vec<f64> = self.dernieres10.iter().copied().collect();
        r[1] = bas(&mut d, 0.99);
        if self.total_ms > 0.0 {
            r[2] = self.total_n as f64 * 1000.0 / self.total_ms;
        }
        r
    }

    /// Le bilan de la partie, sur une copie des images (Bilan / BilanAsync)
    pub fn copie(&self) -> CopiePartie {
        CopiePartie {
            images: self.tout.clone(),
            poids: self.pas,
            busy_ft: self.busy_ft,
            cpu_busy: self.cpu_busy,
            gpu_busy: self.gpu_busy,
            busy_n: self.busy_n,
        }
    }
    pub fn bilan(&self) -> Bilan {
        self.copie().calculer()
    }
}

/// Les images copiées pour le bilan (calculé dans un autre fil sur une longue partie)
#[derive(Clone, Debug)]
pub struct CopiePartie {
    images: Vec<f64>,
    poids: usize,
    busy_ft: f64,
    cpu_busy: f64,
    gpu_busy: f64,
    busy_n: u64,
}

fn cap_de(fps: f64) -> f64 {
    for c in CAPS {
        if (fps - c).abs() <= 1.5f64.max(c * 0.03) {
            return c;
        }
    }
    0.0
}

/// `Array.Sort` des doubles (sans NaN : les durées sont filtrées)
fn trier(v: &mut [f64]) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
}

/// « 1 % bas » : FPS moyen des 1 % d'images les plus lentes
fn bas(t: &mut [f64], pct: f64) -> f64 {
    if t.is_empty() {
        return 0.0;
    }
    trier(t);
    let count = 1usize.max((t.len() as f64 * (1.0 - pct)).floor() as usize);
    let mut somme = 0.0;
    for x in &t[t.len() - count..] {
        somme += x;
    }
    1000.0 / (somme / count as f64)
}

/// Images de jeu (paliers retirés) ; (images, secondes en palier, FPS du palier principal)
fn images_de_jeu(f: &[f64], poids: usize) -> (Vec<f64>, f64, f64) {
    let n = f.len();
    if n < FENETRE_PALIER * 4 {
        return (f.to_vec(), 0.0, 0.0);
    }
    let mut tri = f.to_vec();
    trier(&mut tri);
    let med_fps = 1000.0 / tri[n / 2];
    let blocs = n / FENETRE_PALIER;
    let mut cap = vec![0.0; blocs];
    for (b, c) in cap.iter_mut().enumerate() {
        let (mut somme, mut sq) = (0.0, 0.0);
        for x in &f[b * FENETRE_PALIER..(b + 1) * FENETRE_PALIER] {
            somme += x;
            sq += x * x;
        }
        let moy = somme / FENETRE_PALIER as f64;
        let sd = (sq / FENETRE_PALIER as f64 - moy * moy).max(0.0).sqrt();
        let fps = 1000.0 / moy;
        *c = if sd / moy < 0.05 && fps < 0.85 * med_fps { cap_de(fps) } else { 0.0 };
    }
    let mut retire = vec![false; blocs];
    let mut par_cap: Vec<(f64, f64)> = Vec::new();
    let mut s = 0;
    while s < blocs {
        if cap[s] == 0.0 {
            s += 1;
            continue;
        }
        let mut e = s;
        let mut duree = 0.0;
        while e < blocs && cap[e] == cap[s] {
            for x in &f[e * FENETRE_PALIER..(e + 1) * FENETRE_PALIER] {
                duree += x;
            }
            e += 1;
        }
        duree *= poids as f64;
        if duree >= PALIER_MIN_SEC * 1000.0 {
            for r in retire.iter_mut().take(e).skip(s) {
                *r = true;
            }
            match par_cap.iter_mut().find(|(c, _)| *c == cap[s]) {
                Some(x) => x.1 += duree,
                None => par_cap.push((cap[s], duree)),
            }
        }
        s = e;
    }
    let mut garde = Vec::with_capacity(n);
    let (mut garde_ms, mut total_ms) = (0.0, 0.0);
    for (i, x) in f.iter().enumerate() {
        total_ms += x;
        let b = i / FENETRE_PALIER;
        if b < blocs && retire[b] {
            continue;
        }
        garde.push(*x);
        garde_ms += x;
    }
    if garde.len() < 300 || garde_ms * (poids as f64) < 20000.0 || garde_ms < 0.2 * total_ms {
        return (f.to_vec(), 0.0, 0.0);
    }
    let (mut pl_sec, mut pl_fps) = (0.0, 0.0);
    for (c, d) in &par_cap {
        pl_sec += d / 1000.0;
        let actuel = par_cap.iter().find(|(x, _)| *x == pl_fps).map(|x| x.1);
        if pl_fps == 0.0 || actuel.is_some_and(|a| *d > a) {
            pl_fps = *c;
        }
    }
    (garde, pl_sec, pl_fps)
}

impl CopiePartie {
    pub fn calculer(&self) -> Bilan {
        let mut resume = [0.0; 5];
        let mut palier = [0.0; 2];
        let mut occupe = [-1.0, -1.0, 0.0];
        if self.busy_n > 100 && self.busy_ft > 0.0 {
            occupe[0] = self.cpu_busy / self.busy_ft;
            occupe[1] = self.gpu_busy / self.busy_ft;
        }
        if !self.images.is_empty() {
            let (jeu, pl_sec, pl_fps) = images_de_jeu(&self.images, self.poids);
            palier = [pl_sec, pl_fps];
            let mut arr = jeu.clone();
            trier(&mut arr);
            let n = arr.len();
            let mut total = 0.0;
            for x in &arr {
                total += x;
            }
            resume[0] = n as f64 * 1000.0 / total;
            resume[1] = bas(&mut arr.clone(), 0.99);
            resume[2] = bas(&mut arr.clone(), 0.999);
            resume[3] = n as f64 * self.poids as f64;
            resume[4] = total * self.poids as f64 / 1000.0;
            let mediane = arr[n / 2];
            let limite = 25f64.max(mediane * 2.5);
            let k = jeu.iter().filter(|x| **x > limite).count();
            occupe[2] = k as f64 * self.poids as f64;
        }
        Bilan { resume, palier, occupe }
    }
}

// ---------------------------------------------------------------------------
// Enregistrement d'une partie (fps.json) et bulle de fin
// ---------------------------------------------------------------------------

/// Compress-Series : la courbe ramenée à 160 points au plus
pub fn compresser(v: &[f64]) -> Vec<f64> {
    if v.len() <= 160 {
        return v.to_vec();
    }
    let n = v.len();
    (0..160)
        .map(|i| {
            let a = i * n / 160;
            let b = (i + 1) * n / 160 - 1;
            let part = &v[a..=b];
            let mut somme = 0.0;
            for x in part {
                somme += x;
            }
            ps_round(somme / part.len() as f64, 1)
        })
        .collect()
}

/// Test-FpsProblem : moins de 60 en moyenne, ou des chutes fortes et fréquentes
pub fn probleme(avg: f64, low1: f64, secondes: f64, saccades: Option<f64>) -> bool {
    let par_min = saccades.map(|s| s / (secondes / 60.0).max(1.0)).unwrap_or(0.0);
    avg < 60.0 || low1 < 0.5 * avg || par_min > 6.0
}

/// Get-FpsVerdict
pub fn verdict(avg: f64, low1: f64) -> (&'static str, &'static str) {
    let ratio = if avg > 0.0 { low1 / avg } else { 0.0 };
    if avg < 30.0 {
        return ("bad", "Moins de 30 FPS en moyenne : le jeu saccade. Baisse la qualité graphique ou la résolution.");
    }
    if ratio < 0.5 {
        return (
            "warn",
            "Moyenne correcte, mais des chutes fréquentes : ce sont elles que tu ressens comme des saccades. Souvent des programmes en arrière-plan, un disque plein ou une surchauffe.",
        );
    }
    if avg < 60.0 {
        return ("warn", "Jouable, mais en dessous de 60 FPS la fluidité se ressent dans les jeux rapides.");
    }
    if ratio < 0.7 {
        return ("ok", "Bonne moyenne, avec quelques petites chutes par moments.");
    }
    ("ok", "Fluide et régulier : très bonne partie.")
}

/// Ce que devient une partie finie (Complete-FpsTarget) : rien (trop courte), une bulle seule (aucune image et
/// l'anti-triche l'a bloquée), ou l'enregistrement et sa bulle.
#[derive(Debug, PartialEq)]
pub enum FinPartie {
    Rien,
    BulleSeule(String, String),
    Enregistrer { partie: Value, bulle: (String, String), probleme: bool },
}

/// `t` : nom du jeu, clé (nom du programme en minuscules), début (format « s »), plein écran exclusif, courbe,
/// anti-triche refusé ; `diag` : le diagnostic (Get-FpsDiagData), dont Stutters
pub struct Partie<'a> {
    pub nom: &'a str,
    pub cle: &'a str,
    pub debut_s: &'a str,
    pub exclusif: bool,
    pub courbe: &'a [f64],
    pub refus: bool,
}

pub fn fin_partie(t: &Partie, b: &Bilan, diag: Option<Value>, id: &str) -> FinPartie {
    let s = &b.resume;
    let titre = format!("Partie terminée : {}", t.nom);
    if s[3] == 0.0 {
        if t.refus {
            return FinPartie::BulleSeule(titre, "L'anti-triche de ce jeu bloque la mesure des FPS. À la prochaine partie, ouvre Nevermind avant de lancer le jeu : la mesure démarrera avant lui.".into());
        }
        return FinPartie::Rien;
    }
    if s[4] < 30.0 || s[3] < 300.0 {
        return FinPartie::Rien;
    }
    let avg = ps_round(s[0], 1);
    let low1 = ps_round(s[1], 1);
    let secondes = crate::texte::ps_int(s[4]);
    let saccades = diag.as_ref().and_then(|d| d.get("Stutters")).and_then(Value::as_f64);
    let pb = probleme(avg, low1, secondes as f64, if diag.is_some() { Some(saccades.unwrap_or(0.0)) } else { None });
    let partie = json!({
        "Id": id, "Date": t.debut_s, "Game": t.nom, "Key": t.cle,
        "Avg": avg, "Low1": low1, "Low01": ps_round(s[2], 1), "Seconds": secondes, "Frames": crate::texte::ps_int(s[3]),
        "Exclusive": t.exclusif, "Series": compresser(t.courbe), "Diag": diag.unwrap_or(Value::Null),
        "MenuSec": crate::texte::ps_int(b.palier[0]), "MenuFps": crate::texte::ps_int(b.palier[1])
    });
    let bulle = if pb {
        (
            titre,
            format!(
                "{} FPS en moyenne, avec des chutes. Nevermind a regardé d'où ça vient : clique ici pour voir et corriger.",
                n0(s[0])
            ),
        )
    } else {
        (
            titre,
            format!(
                "{} de jeu, {} FPS en moyenne (1 % bas {}). Tout était fluide.",
                duree_jeu(s[4]),
                n0(s[0]),
                n0(s[1])
            ),
        )
    };
    FinPartie::Enregistrer { partie, bulle, probleme: pb }
}

/// Le texte du compteur : (chiffre, sous-titre) ; `None` : rien à changer
pub fn texte_compteur(i: &Images, refus: bool, secondes_depuis_debut: f64) -> Option<(String, String, f64)> {
    if i.nombre() == 0 {
        if refus {
            return Some(("?".into(), "L'anti-triche de ce jeu bloque la mesure des FPS".into(), 0.0));
        }
        if secondes_depuis_debut > 8.0 {
            return Some(("?".into(), "Aucune image reçue pour le moment.".into(), 0.0));
        }
        return None;
    }
    let l = i.direct();
    Some((n0(l[0]), format!("1 % bas {}    moyenne {}", n0(l[1]), n0(l[2])), l[0]))
}
