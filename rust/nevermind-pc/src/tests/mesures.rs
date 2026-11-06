//! Les mesures des tests (natif.cs `DiskTest`, `CpuTest`, `MemTest`, `NetSpeed`, `LoadedLatency`, et `$NetWork`,
//! `$RepairWork` de tests.ps1), faites dans un fil à part. La progression, l'étape et la valeur en direct sont
//! écrites dans un [`Moteur`] partagé, lu par le bloc à chaque tour (comme `[OGNative]::Progress`, `Phase`,
//! `LiveValue`), et `annuler` les arrête (`[OGNative]::Cancel`).
//!
//! Le réseau (ping, téléchargement, envoi) passe par le trait [`Reseau`] : la vraie version (Windows) est dans
//! `tests::windows`, les tests en donnent une fausse.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// L'état d'un test en cours, partagé entre le fil du test et le bloc
#[derive(Debug, Default)]
pub struct Moteur {
    pub annuler: AtomicBool,
    progres: AtomicU64,
    direct: AtomicU64,
    phase: Mutex<String>,
    /// `QuietSpeed` : le débit mesuré pendant le lag en charge ne touche ni la progression ni la valeur en direct
    discret: AtomicBool,
}

impl Moteur {
    pub fn arrete(&self) -> bool {
        self.annuler.load(Ordering::Relaxed)
    }
    pub fn progres(&self, v: f64) {
        self.progres.store(v.to_bits(), Ordering::Relaxed);
    }
    pub fn direct(&self, v: f64) {
        self.direct.store(v.to_bits(), Ordering::Relaxed);
    }
    pub fn phase(&self, p: &str) {
        if let Ok(mut x) = self.phase.lock() {
            *x = p.into();
        }
    }
    /// (étape, progression, valeur en direct)
    pub fn lire(&self) -> (String, f64, f64) {
        (
            self.phase.lock().map(|x| x.clone()).unwrap_or_default(),
            f64::from_bits(self.progres.load(Ordering::Relaxed)),
            f64::from_bits(self.direct.load(Ordering::Relaxed)),
        )
    }
}

/// Une erreur du travail (son message, montré tel quel dans le panneau)
pub type Echec = String;

/// Un tampon aligné sur 4096 octets (les lectures et écritures sans le cache de Windows l'exigent)
struct Tampon {
    v: Vec<u8>,
    debut: usize,
    taille: usize,
}

impl Tampon {
    fn nouveau(taille: usize) -> Tampon {
        let v = vec![0u8; taille + 4096];
        let debut = v.as_ptr().align_offset(4096);
        Tampon { v, debut, taille }
    }
    fn tranche(&mut self, n: usize) -> &mut [u8] {
        let n = n.min(self.taille);
        &mut self.v[self.debut..self.debut + n]
    }
}

/// Un hasard simple (le contenu écrit et les positions lues n'ont pas besoin d'être ceux de .NET)
struct Hasard(u64);
impl Hasard {
    fn suivant(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn reel(&mut self) -> f64 {
        (self.suivant() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Vitesse d'un disque, sans passer par le cache de Windows (`DiskTest`) :
/// { écriture Mo/s, lecture Mo/s, lecture 4K Mo/s, opérations 4K par seconde }, rien si arrêté. Le fichier d'essai
/// est toujours effacé à la fin.
pub fn disque(m: &Moteur, fichier: &str, taille: i64) -> Result<Option<Vec<f64>>, Echec> {
    let r = disque_sans_effacer(m, fichier, taille);
    let _ = std::fs::remove_file(fichier);
    r
}

#[cfg(windows)]
fn disque_sans_effacer(m: &Moteur, fichier: &str, taille: i64) -> Result<Option<Vec<f64>>, Echec> {
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::windows::fs::OpenOptionsExt;
    const BLOC: usize = 8 * 1024 * 1024;
    const SANS_CACHE: u32 = 0x2000_0000;
    const ECRITURE_DIRECTE: u32 = 0x8000_0000;
    const SEQUENTIEL: u32 = 0x0800_0000;
    const AU_HASARD: u32 = 0x1000_0000;
    const DUREE: f64 = 6.0;
    let err = |e: std::io::Error| e.to_string();
    let taille = (BLOC as i64).max(taille / BLOC as i64 * BLOC as i64);
    let blocs = taille / BLOC as i64;
    let mut tampon = Tampon::nouveau(BLOC);
    let mut h = Hasard(42);
    for x in tampon.tranche(BLOC).chunks_mut(8) {
        let v = h.suivant().to_le_bytes();
        x.copy_from_slice(&v[..x.len()]);
    }
    let mut r = vec![0.0; 4];
    let lisse = |m: &Moteur, direct: &mut f64, inst: f64| {
        *direct = if *direct == 0.0 { inst } else { *direct * 0.85 + inst * 0.15 };
        m.direct(*direct);
    };

    m.phase("write");
    let mut ecrits = 0i64;
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .share_mode(0)
            .custom_flags(SANS_CACHE | ECRITURE_DIRECTE)
            .open(fichier)
            .map_err(err)?;
        let t = Instant::now();
        let (mut dernier, mut direct) = (0.0, 0.0);
        m.direct(0.0);
        for i in 0..blocs {
            if m.arrete() {
                return Ok(None);
            }
            if t.elapsed().as_secs_f64() > DUREE {
                break;
            }
            f.write_all(tampon.tranche(BLOC)).map_err(err)?;
            ecrits = i + 1;
            let s = t.elapsed().as_secs_f64();
            m.progres(40.0 * ((i + 1) as f64 / blocs as f64).max(s / DUREE));
            lisse(m, &mut direct, BLOC as f64 / 1_048_576.0 / (s - dernier).max(1e-6));
            dernier = s;
        }
        r[0] = ecrits as f64 * BLOC as f64 / 1_048_576.0 / t.elapsed().as_secs_f64();
    }

    m.phase("read");
    {
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(SANS_CACHE | SEQUENTIEL)
            .open(fichier)
            .map_err(err)?;
        let t = Instant::now();
        let (mut dernier, mut direct) = (0.0, 0.0);
        m.direct(0.0);
        let mut lus = 0i64;
        for i in 0..ecrits {
            if m.arrete() {
                return Ok(None);
            }
            if t.elapsed().as_secs_f64() > DUREE {
                break;
            }
            f.read_exact(tampon.tranche(BLOC)).map_err(err)?;
            lus = i + 1;
            let s = t.elapsed().as_secs_f64();
            m.progres(40.0 + 30.0 * ((i + 1) as f64 / ecrits as f64).max(s / DUREE));
            lisse(m, &mut direct, BLOC as f64 / 1_048_576.0 / (s - dernier).max(1e-6));
            dernier = s;
        }
        r[1] = lus as f64 * BLOC as f64 / 1_048_576.0 / t.elapsed().as_secs_f64();
    }

    m.phase("random");
    {
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(SANS_CACHE | AU_HASARD)
            .open(fichier)
            .map_err(err)?;
        let mut h = Hasard(7);
        let pages = ecrits * BLOC as i64 / 4096;
        let mut n = 0u64;
        let t = Instant::now();
        while t.elapsed().as_secs_f64() < 5.0 {
            if m.arrete() {
                return Ok(None);
            }
            let pos = (h.reel() * pages as f64) as u64 * 4096;
            f.seek(SeekFrom::Start(pos)).map_err(err)?;
            f.read_exact(tampon.tranche(4096)).map_err(err)?;
            n += 1;
            if n & 127 == 0 {
                let s = t.elapsed().as_secs_f64();
                m.progres(70.0 + 30.0 * (s / 5.0).min(1.0));
                m.direct(n as f64 / s.max(1e-6));
            }
        }
        let s = t.elapsed().as_secs_f64();
        r[3] = n as f64 / s;
        r[2] = n as f64 * 4096.0 / 1_048_576.0 / s;
    }
    m.progres(100.0);
    Ok(Some(r))
}

#[cfg(not(windows))]
fn disque_sans_effacer(_m: &Moteur, _fichier: &str, _taille: i64) -> Result<Option<Vec<f64>>, Echec> {
    Err("Ce test ne marche que sous Windows.".into())
}

/// Le calcul du processeur (`CpuWork`) : toujours le même résultat, sinon le processeur est instable
pub fn calcul(graine: i64, tours: i64) -> i64 {
    let mut x = graine;
    let mut d = graine as f64;
    for _ in 0..tours {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        d = d * 1.0000001 + (x & 1023) as f64 * 0.5;
        if d > 1e12 {
            d /= 3.0;
        }
    }
    x ^ (d as i64)
}

/// Processeur (`CpuTest`) : { score 1 cœur, score tous les cœurs, erreurs de calcul, nombre de threads }, rien si arrêté
pub fn cpu(m: &Moteur, un: f64, tous: f64, abaisser: fn()) -> Option<Vec<f64>> {
    const TOURS: i64 = 2_000_000;
    let reference = calcul(12345, TOURS);
    let total = un + tous;
    let erreurs = Arc::new(AtomicI64::new(0));
    m.phase("single");
    let mut n = 0i64;
    let t = Instant::now();
    while t.elapsed().as_secs_f64() < un {
        if m.arrete() {
            return None;
        }
        if calcul(std::hint::black_box(12345), TOURS) != reference {
            erreurs.fetch_add(1, Ordering::Relaxed);
        }
        n += 1;
        m.progres(100.0 * t.elapsed().as_secs_f64() / total);
    }
    let seul = n as f64 / t.elapsed().as_secs_f64();

    m.phase("multi");
    let fils = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let compte = Arc::new(AtomicI64::new(0));
    let depart = Instant::now();
    let arret = Arc::new(AtomicBool::new(false));
    let mut h = Vec::new();
    for _ in 0..fils {
        let (compte, erreurs, arret) = (compte.clone(), erreurs.clone(), arret.clone());
        h.push(std::thread::spawn(move || {
            abaisser();
            let (mut local, mut mauvais) = (0, 0);
            while depart.elapsed().as_secs_f64() < tous && !arret.load(Ordering::Relaxed) {
                if calcul(std::hint::black_box(12345), TOURS) != reference {
                    mauvais += 1;
                }
                local += 1;
            }
            compte.fetch_add(local, Ordering::Relaxed);
            erreurs.fetch_add(mauvais, Ordering::Relaxed);
        }));
    }
    while h.iter().any(|x| !x.is_finished()) {
        if m.arrete() {
            arret.store(true, Ordering::Relaxed);
        }
        std::thread::sleep(Duration::from_millis(100));
        m.progres(100.0 * (un + tous.min(depart.elapsed().as_secs_f64())) / total);
    }
    for x in h {
        let _ = x.join();
    }
    if m.arrete() {
        return None;
    }
    let tous_s = compte.load(Ordering::Relaxed) as f64 / depart.elapsed().as_secs_f64();
    m.progres(100.0);
    Some(vec![seul * 10.0, tous_s * 10.0, erreurs.load(Ordering::Relaxed) as f64, fils as f64])
}

const MULT: i64 = -7046029254386353131;

fn cle(c: usize) -> i64 {
    ((c as i64) << 32) ^ 0x5555555555555555
}

/// Chaque morceau traité par plusieurs fils (`ParallelChunks`)
fn en_parallele(m: &Moteur, mem: &mut [Vec<i64>], travail: &(dyn Fn(&mut [i64], usize) + Sync), p0: f64, p1: f64) {
    let total = mem.len();
    let suivant = AtomicUsize::new(0);
    let finis = AtomicUsize::new(0);
    let t = Instant::now();
    m.direct(0.0);
    let longueur = mem.first().map_or(0, |x| x.len());
    let fils = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(total).max(1);
    let morceaux: Vec<Mutex<&mut Vec<i64>>> = mem.iter_mut().map(Mutex::new).collect();
    std::thread::scope(|s| {
        for _ in 0..fils {
            s.spawn(|| {
                while !m.arrete() {
                    let i = suivant.fetch_add(1, Ordering::Relaxed);
                    if i >= total {
                        break;
                    }
                    if let Ok(mut a) = morceaux[i].lock() {
                        travail(a.as_mut_slice(), i);
                    }
                    let f = finis.fetch_add(1, Ordering::Relaxed) + 1;
                    m.progres(p0 + (p1 - p0) * f as f64 / total as f64);
                    m.direct(
                        f as f64 * (longueur as f64 * 8.0) / 1_073_741_824.0 / t.elapsed().as_secs_f64().max(1e-6),
                    );
                }
            });
        }
    });
}

/// Mémoire vive (`MemTest`) : écrit des motifs, les relit et compte les erreurs.
/// { écriture Go/s, lecture Go/s, copie Go/s, erreurs, Go testés }, rien si arrêté
pub fn memoire(m: &Moteur, octets: i64) -> Option<Vec<f64>> {
    memoire_par(m, octets, 8 * 1024 * 1024)
}

/// (la même, avec des morceaux de `longs` nombres : 8 Mi en vrai, moins pour les essais)
pub fn memoire_par(m: &Moteur, octets: i64, longs: usize) -> Option<Vec<f64>> {
    let morceaux = 2.max(octets / (longs as i64 * 8)) as usize;
    let mut mem: Vec<Vec<i64>> = Vec::new();
    m.phase("alloc");
    for i in 0..morceaux {
        if m.arrete() {
            return None;
        }
        mem.push(vec![0i64; longs]);
        m.progres(5.0 * (i + 1) as f64 / morceaux as f64);
    }
    let go = morceaux as f64 * (longs as f64 * 8.0) / 1_073_741_824.0;
    let erreurs = AtomicI64::new(0);

    m.phase("write");
    let t = Instant::now();
    en_parallele(
        m,
        &mut mem,
        &|a, c| {
            let k = cle(c);
            for (j, x) in a.iter_mut().enumerate() {
                *x = k ^ (j as i64).wrapping_mul(MULT);
            }
        },
        5.0,
        30.0,
    );
    if m.arrete() {
        return None;
    }
    let ecriture = go / t.elapsed().as_secs_f64();

    m.phase("read");
    let t = Instant::now();
    en_parallele(
        m,
        &mut mem,
        &|a, c| {
            let k = cle(c);
            let e = a.iter().enumerate().filter(|(j, x)| **x != k ^ (*j as i64).wrapping_mul(MULT)).count() as i64;
            if e > 0 {
                erreurs.fetch_add(e, Ordering::Relaxed);
            }
        },
        30.0,
        55.0,
    );
    if m.arrete() {
        return None;
    }
    let lecture = go / t.elapsed().as_secs_f64();

    m.phase("pattern");
    en_parallele(
        m,
        &mut mem,
        &|a, c| {
            let k = !cle(c);
            for (j, x) in a.iter_mut().enumerate() {
                *x = !(k ^ (j as i64).wrapping_mul(MULT));
            }
        },
        55.0,
        70.0,
    );
    en_parallele(
        m,
        &mut mem,
        &|a, c| {
            let k = !cle(c);
            let e = a.iter().enumerate().filter(|(j, x)| **x != !(k ^ (*j as i64).wrapping_mul(MULT))).count() as i64;
            if e > 0 {
                erreurs.fetch_add(e, Ordering::Relaxed);
            }
        },
        70.0,
        85.0,
    );
    if m.arrete() {
        return None;
    }

    m.phase("copy");
    let t = Instant::now();
    let mut paires = 0;
    let mut i = 0;
    while i + 1 < morceaux {
        if m.arrete() {
            return None;
        }
        let (a, b) = mem.split_at_mut(i + 1);
        b[0].copy_from_slice(&a[i]);
        paires += 1;
        m.progres(85.0 + 15.0 * (i + 2) as f64 / morceaux as f64);
        i += 2;
    }
    let copie = paires as f64 * (longs as f64 * 8.0) / 1_073_741_824.0 / t.elapsed().as_secs_f64();
    m.progres(100.0);
    Some(vec![ecriture, lecture, copie, erreurs.load(Ordering::Relaxed) as f64, go])
}

// ---------------------------------------------------------------------------
// Réseau
// ---------------------------------------------------------------------------

/// Ce qu'un flux de débit sait : continuer ou pas, et compter les octets passés
pub struct Compteur<'a> {
    total: &'a AtomicU64,
    depart: Instant,
    secondes: f64,
    moteur: &'a Moteur,
}

impl Compteur<'_> {
    /// Le temps n'est pas fini et le test n'est pas arrêté
    pub fn encore(&self) -> bool {
        self.depart.elapsed().as_secs_f64() < self.secondes && !self.moteur.arrete()
    }
    pub fn ajouter(&self, n: u64) {
        self.total.fetch_add(n, Ordering::Relaxed);
    }
}

/// Le réseau des tests : un ping, et un flux de téléchargement ou d'envoi
pub trait Reseau: Sync {
    /// Le temps de réponse en ms, rien si pas de réponse
    fn ping(&self, hote: &str, delai_ms: u32) -> Option<f64>;
    /// Une demande à un serveur : télécharge (ou envoie) en comptant les octets tant que `c.encore()`.
    /// Une erreur (serveur qui refuse, coupure) : le suivant est essayé.
    fn flux(&self, url: &str, envoi: bool, c: &Compteur) -> Result<(), String>;
}

/// Débit Internet en Mb/s (`NetSpeed`) : plusieurs connexions en parallèle ; si un serveur refuse, le suivant ;
/// -1 si aucun n'a répondu
#[allow(clippy::too_many_arguments)]
pub fn debit(
    m: &Moteur,
    r: &dyn Reseau,
    urls: &[&str],
    envoi: bool,
    secondes: f64,
    flux: usize,
    p0: f64,
    p1: f64,
) -> f64 {
    let index = AtomicUsize::new(0);
    let total = AtomicU64::new(0);
    let depart = Instant::now();
    let discret = m.discret.load(Ordering::Relaxed);
    if !discret {
        m.direct(0.0);
    }
    std::thread::scope(|s| {
        let mut h = Vec::new();
        for _ in 0..flux {
            h.push(s.spawn(|| {
                let c = Compteur { total: &total, depart, secondes, moteur: m };
                while c.encore() {
                    let ui = index.load(Ordering::Relaxed);
                    if ui >= urls.len() {
                        break;
                    }
                    if r.flux(urls[ui], envoi, &c).is_err() {
                        if !c.encore() {
                            break;
                        }
                        let _ = index.compare_exchange(ui, ui + 1, Ordering::Relaxed, Ordering::Relaxed);
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }));
        }
        let (mut avant, mut t_avant) = (0u64, 0.0);
        while h.iter().any(|x| !x.is_finished()) {
            std::thread::sleep(Duration::from_millis(100));
            if discret {
                continue;
            }
            let now = depart.elapsed().as_secs_f64();
            m.progres(p0 + (p1 - p0) * (now / secondes).min(1.0));
            if now - t_avant >= 0.3 {
                let b = total.load(Ordering::Relaxed);
                m.direct((b - avant) as f64 * 8.0 / 1e6 / (now - t_avant));
                avant = b;
                t_avant = now;
            }
        }
    });
    let ecoule = depart.elapsed().as_secs_f64().min(secondes + 0.5);
    if !discret {
        m.progres(p1);
    }
    let t = total.load(Ordering::Relaxed);
    if t == 0 { -1.0 } else { t as f64 * 8.0 / 1e6 / ecoule }
}

/// La connexion Internet (`$NetWork`) : { ping moyen, téléchargement Mb/s, envoi Mb/s, pings perdus }, rien si arrêté
pub fn reseau(m: &Moteur, r: &dyn Reseau) -> Option<Vec<f64>> {
    m.phase("ping");
    let mut temps = Vec::new();
    for i in 0..10 {
        if m.arrete() {
            return None;
        }
        if let Some(t) = r.ping("1.1.1.1", 1000) {
            temps.push(t);
            m.direct(t);
        }
        m.progres((i + 1) as f64);
        std::thread::sleep(Duration::from_millis(100));
    }
    m.phase("down");
    let d = debit(m, r, &super::banc::SERVEURS_DESCENTE, false, 8.0, 4, 10.0, 55.0);
    if m.arrete() {
        return None;
    }
    m.phase("up");
    let u = debit(m, r, &[super::banc::SERVEUR_MONTEE], true, 8.0, 4, 55.0, 100.0);
    if m.arrete() {
        return None;
    }
    let moy = if temps.is_empty() { -1.0 } else { temps.iter().sum::<f64>() / temps.len() as f64 };
    Some(vec![moy, d, u, (10 - temps.len()) as f64])
}

/// Des pings pendant `secondes` : { médiane, pertes %, gigue }
fn pings(m: &Moteur, r: &dyn Reseau, hote: &str, secondes: f64, p0: f64, p1: f64) -> [f64; 3] {
    let mut temps = Vec::new();
    let (mut envoyes, mut perdus) = (0, 0);
    let t = Instant::now();
    while t.elapsed().as_secs_f64() < secondes && !m.arrete() {
        let t0 = t.elapsed().as_millis() as i64;
        envoyes += 1;
        match r.ping(hote, 1500) {
            Some(x) => {
                temps.push(x);
                m.direct(x);
            }
            None => perdus += 1,
        }
        m.progres(p0 + (p1 - p0) * (t.elapsed().as_secs_f64() / secondes).min(1.0));
        let attente = 150 - (t.elapsed().as_millis() as i64 - t0);
        if attente > 0 {
            std::thread::sleep(Duration::from_millis(attente as u64));
        }
    }
    let (mut moy, mut gigue) = (-1.0, 0.0);
    if !temps.is_empty() {
        temps.sort_by(f64::total_cmp);
        // (médiane : un ping isolé très lent ne fausse pas la note)
        moy = temps[temps.len() / 2];
        for i in 1..temps.len() {
            gigue += (temps[i] - temps[i - 1]).abs();
        }
        if temps.len() > 1 {
            gigue /= (temps.len() - 1) as f64;
        }
    }
    [moy, if envoyes > 0 { 100.0 * perdus as f64 / envoyes as f64 } else { 0.0 }, gigue]
}

/// Latence en charge (`LoadedLatency`) : { ping repos, ping téléchargement, ping envoi, pertes repos %, pertes
/// téléch. %, pertes envoi %, débit téléch. Mb/s, débit envoi Mb/s, gigue repos }, rien si arrêté
pub fn lag(
    m: &Moteur,
    r: &dyn Reseau,
    descente: &[&str],
    montee: &[&str],
    hote: &str,
    repos: f64,
    charge: f64,
) -> Option<Vec<f64>> {
    let charge_de = |urls: &[&str], envoi: bool, p0: f64, p1: f64| -> [f64; 3] {
        m.discret.store(true, Ordering::Relaxed);
        let mbps = std::thread::scope(|s| {
            let job = s.spawn(|| debit(m, r, urls, envoi, charge, 8, 0.0, 0.0));
            // (le temps que la connexion se remplisse)
            std::thread::sleep(Duration::from_millis(1500));
            let p = pings(m, r, hote, charge - 2.0, p0, p1);
            (p, job.join().unwrap_or(-1.0))
        });
        m.discret.store(false, Ordering::Relaxed);
        [mbps.0[0], mbps.0[1], mbps.1]
    };
    m.phase("idle");
    let i = pings(m, r, hote, repos, 0.0, 25.0);
    if m.arrete() {
        return None;
    }
    m.phase("down");
    let d = charge_de(descente, false, 25.0, 62.0);
    if m.arrete() {
        return None;
    }
    m.phase("up");
    let u = charge_de(montee, true, 62.0, 100.0);
    if m.arrete() {
        return None;
    }
    Some(vec![i[0], d[0], u[0], i[1], d[1], u[1], d[2], u[2], i[2]])
}

/// La recherche d'erreurs (`$RepairWork`) : « lettre|résultat » (ou « lettre|ERR message ») pour chaque lecteur
pub fn erreurs(m: &Moteur, lettres: &[String], verifier: &dyn Fn(&str) -> Result<String, String>) -> Vec<String> {
    m.phase("scan");
    let mut l = Vec::new();
    for (i, x) in lettres.iter().enumerate() {
        l.push(match verifier(x) {
            Ok(r) => format!("{x}|{r}"),
            Err(e) => format!("{x}|ERR {e}"),
        });
        m.progres(100.0 * (i + 1) as f64 / lettres.len() as f64);
    }
    l
}
