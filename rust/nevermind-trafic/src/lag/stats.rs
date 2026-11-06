//! Les statistiques d'une série de pings (`Get-PingStats`), la série réduite du graphique (`Compress-PingSeries`)
//! et les pics qui tombent en même temps qu'un pic de la box ou qu'un téléchargement (`Get-LagSpikeLinks`).

use crate::format::{arrondi, entier};
use crate::valeur::{champ, n, num, texte};
use serde_json::{Map, Value, json};

/// Les chiffres d'une étape du chemin
#[derive(Clone, Debug, PartialEq)]
pub struct Stats {
    pub n: i64,
    pub perte: f64,
    pub mort: bool,
    pub med: f64,
    pub p95: f64,
    pub jit: f64,
    pub max: f64,
    pub pics: Vec<f64>,
}

impl Stats {
    /// Comme `Get-PingStats` le rend (Spikes : la liste des moments)
    pub fn json(&self) -> Value {
        json!({ "N": self.n, "Loss": n(self.perte), "Dead": self.mort, "Med": n(self.med), "P95": n(self.p95), "Jit": n(self.jit), "Max": n(self.max), "Spikes": self.pics.iter().map(|x| n(*x)).collect::<Vec<_>>() })
    }
    /// Comme gardé dans la mesure (`Stats` de lag.json : Spikes est le nombre de pics)
    pub fn json_garde(&self) -> Value {
        json!({ "Med": n(self.med), "P95": n(self.p95), "Jit": n(self.jit), "Loss": n(self.perte), "Max": n(self.max), "Dead": self.mort, "N": self.n, "Spikes": self.pics.len() })
    }
}

/// Statistiques d'une série de pings [t, ms, t, ms...] (ms -1 = perdu) ; `None` sous 8 pings
pub fn ping(a: &[f64]) -> Option<Stats> {
    let n = entier(a.len() as f64 / 2.0) as usize;
    if n < 8 {
        return None;
    }
    let mut ok: Vec<f64> = Vec::new();
    let mut perdus = 0usize;
    for i in 0..n {
        let r = a[2 * i + 1];
        if r < 0.0 {
            perdus += 1;
        } else {
            ok.push(r);
        }
    }
    let perte = arrondi(100.0 * perdus as f64 / n as f64, 1);
    if (ok.len() as f64) < f64::max(4.0, n as f64 * 0.2) {
        return Some(Stats {
            n: n as i64,
            perte,
            mort: true,
            med: -1.0,
            p95: -1.0,
            jit: 0.0,
            max: -1.0,
            pics: Vec::new(),
        });
    }
    let mut jit = 0.0;
    for i in 1..ok.len() {
        jit += (ok[i] - ok[i - 1]).abs();
    }
    jit /= f64::max(1.0, ok.len() as f64 - 1.0);
    let mut tri = ok.clone();
    tri.sort_by(f64::total_cmp);
    let med = tri[entier(tri.len() as f64 / 2.0) as usize];
    let p95 = tri[(0.95 * (tri.len() as f64 - 1.0)).floor() as usize];
    let lim = med + f64::max(30.0, 4.0 * jit);
    let mut pics = Vec::new();
    for i in 0..n {
        let r = a[2 * i + 1];
        if r < 0.0 || r > lim {
            pics.push(a[2 * i]);
        }
    }
    Some(Stats {
        n: n as i64,
        perte,
        mort: false,
        med: arrondi(med, 1),
        p95: arrondi(p95, 1),
        jit: arrondi(jit, 1),
        max: arrondi(*tri.last().unwrap_or(&0.0), 1),
        pics,
    })
}

/// Série réduite pour le graphique : le pire ping de chaque tranche (-1 si tout est perdu)
pub fn reduire(a: &[f64], pas: f64) -> Vec<f64> {
    let mut sortie: Vec<f64> = Vec::new();
    let mut cur = -2.0;
    let mut b: i64 = 0;
    let mut i = 0;
    while i + 1 < a.len() {
        let k = (a[i] / pas).floor() as i64;
        while b < k {
            sortie.push(cur);
            cur = -2.0;
            b += 1;
        }
        let r = a[i + 1];
        cur = if r < 0.0 { if cur == -2.0 { -1.0 } else { cur } } else { f64::max(cur, r) };
        i += 2;
    }
    if cur != -2.0 {
        sortie.push(cur);
    }
    sortie.into_iter().map(|x| if x == -2.0 { -1.0 } else { arrondi(x, 0) }).collect()
}

/// Pics du ping principal en même temps qu'un pic de la box (à 1 s près) ou qu'un téléchargement d'un autre
/// programme (plus de 500 Ko/s, à 2,5 s près). `fond` : les relevés { T, Rate, Name, NameRate }.
pub struct Liens {
    pub local: i64,
    pub fond: i64,
    /// Les programmes vus, avec leur plus gros débit (dans l'ordre où ils sont vus)
    pub noms: Vec<(String, f64)>,
}

impl Liens {
    pub fn json(&self) -> Value {
        let mut m = Map::new();
        for (k, v) in &self.noms {
            m.insert(k.clone(), crate::valeur::n(*v));
        }
        json!({ "Local": self.local, "BgHit": self.fond, "BgNames": m })
    }
}

pub fn liens(pics: &[f64], pics_box: &[f64], fond: &[Value]) -> Liens {
    liens_ordre(pics, pics_box, fond, true)
}

/// Comme la version PowerShell, défaut compris : `[Array]::Sort($tf, $fond)` trie les dates mais PAS les relevés
/// (PowerShell passe une copie de `$fond`), si bien que le programme nommé est celui du relevé au même rang dans
/// l'ordre d'origine. Gardé pour la preuve de DIVERGENCES.md (les juges l'utilisent), jamais à l'écran.
pub fn liens_comme_powershell(pics: &[f64], pics_box: &[f64], fond: &[Value]) -> Liens {
    liens_ordre(pics, pics_box, fond, false)
}

fn liens_ordre(pics: &[f64], pics_box: &[f64], fond: &[Value], trier_releves: bool) -> Liens {
    let mut r = Liens { local: 0, fond: 0, noms: Vec::new() };
    let mut sp = pics.to_vec();
    if sp.is_empty() {
        return r;
    }
    sp.sort_by(f64::total_cmp);
    let mut gw = pics_box.to_vec();
    gw.sort_by(f64::total_cmp);
    let mut f: Vec<&Value> = fond.iter().filter(|b| b.is_object() && num(champ(b, "Rate")) > 500.0 * 1024.0).collect();
    let mut tf: Vec<f64> = f.iter().map(|b| num(champ(b, "T"))).collect();
    tf.sort_by(f64::total_cmp);
    if trier_releves {
        f.sort_by(|a, b| num(champ(a, "T")).total_cmp(&num(champ(b, "T"))));
    }
    let (mut j, mut k) = (0usize, 0usize);
    for &t in &sp {
        while j < gw.len() && (t - gw[j]) > 1.0 {
            j += 1;
        }
        if j < gw.len() && (gw[j] - t).abs() <= 1.0 {
            r.local += 1;
        }
        while k < tf.len() && (t - tf[k]) > 2.5 {
            k += 1;
        }
        let mut vu = false;
        let mut m = k;
        while m < tf.len() && (tf[m] - t).abs() <= 2.5 {
            vu = true;
            let b = f[m];
            let nom = texte(champ(b, "Name"));
            if !nom.is_empty() {
                let debit = num(champ(b, "NameRate"));
                match r.noms.iter_mut().find(|(n, _)| *n == nom) {
                    Some(x) => x.1 = f64::max(x.1, debit),
                    None => r.noms.push((nom, f64::max(0.0, debit))),
                }
            }
            m += 1;
        }
        if vu {
            r.fond += 1;
        }
    }
    r
}
