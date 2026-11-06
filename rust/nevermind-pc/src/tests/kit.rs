//! Le panneau d'un test tel que l'écran le reçoit. En PowerShell, le panneau est dessiné en WPF (visuels.ps1 :
//! jauges, tuiles chiffrées, verdicts, détails repliables, barres, courbe, étapes) puis traduit pour l'écran élément
//! par élément (`Add-BlocsWpf`, ecran-panneau.ps1). Ici chaque élément garde ce qui compte pour cette traduction (ses
//! textes, leur taille, leur couleur, s'ils sont en gras, si l'encadré a un fond), et la même traduction en est faite,
//! avec ses règles exactes (un chiffre en gros devient un chiffre, un texte gras en capitales un titre...).

use crate::net::n_fr;
use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;

/// Les couleurs de Nevermind (`$Colors`, interface.ps1)
pub const OK: &str = "#22D37A";
pub const WARN: &str = "#F5A524";
pub const BAD: &str = "#F04438";
pub const INFO: &str = "#4EA8FF";
pub const VIOLET: &str = "#B18CFF";
pub const BLANC: &str = "#FFFFFF";
pub const DOUX: &str = "#A6A1BC";

/// `$Colors[$Status]`
pub fn couleur_statut(statut: &str) -> &'static str {
    match statut {
        "ok" => OK,
        "warn" => WARN,
        "bad" => BAD,
        _ => INFO,
    }
}

/// `Get-TonKit` (ecran-pc.ps1) : vert « ok », orange « attention », rouge « urgent », sinon rien
pub fn ton(couleur: &str) -> &'static str {
    let h = couleur.trim_start_matches('#');
    let h = if h.len() == 8 { &h[2..] } else { h };
    if h.len() != 6 {
        return "";
    }
    let c = |i: usize| i64::from_str_radix(&h[i..i + 2], 16).unwrap_or(0);
    let (r, g, b) = (c(0), c(2), c(4));
    if g > 150 && r < 120 && b < 170 {
        return "ok";
    }
    if r > 190 && g < 110 {
        return "urgent";
    }
    if r > 190 && (110..210).contains(&g) && b < 120 {
        return "attention";
    }
    ""
}

fn couleur_ton(t: &str) -> Value {
    match t {
        "ok" => json!("vert"),
        "attention" => json!("orange"),
        "urgent" => json!("rouge"),
        _ => Value::Null,
    }
}

/// La longueur d'un texte comme .NET la compte (en unités UTF-16)
fn longueur(t: &str) -> usize {
    t.encode_utf16().count()
}

/// Le format d'un nombre (`'{0:N0}' -f`, `'{0:N1}' -f`)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fmt {
    N0,
    N1,
    /// Un texte fixe (le « ✓ » ou le « ! » des tuiles de la recherche d'erreurs)
    Fixe(&'static str),
}

impl Fmt {
    pub fn f(self, x: f64) -> String {
        match self {
            Fmt::N0 => n_fr(x, 0),
            Fmt::N1 => n_fr(x, 1),
            Fmt::Fixe(s) => s.into(),
        }
    }
    /// Les décimales gardées par la courbe de l'écran (`'N(\d)'`)
    pub fn decimales(self) -> i32 {
        match self {
            Fmt::N0 => 0,
            Fmt::N1 => 1,
            Fmt::Fixe(_) => 1,
        }
    }
}

/// Une jauge (`New-Gauge`) : l'écran en montre la valeur finale
#[derive(Debug, Clone)]
pub struct Jauge {
    pub titre: String,
    pub valeur: f64,
    pub fmt: Fmt,
    pub unite: String,
    pub couleur: String,
}

pub fn jauge(titre: &str, valeur: f64, fmt: Fmt, unite: &str, couleur: &str) -> Jauge {
    Jauge { titre: titre.into(), valeur, fmt, unite: unite.into(), couleur: couleur.into() }
}

/// Un élément du panneau
#[derive(Debug, Clone)]
pub enum El {
    /// Un texte (`New-Text` et les TextBlock du panneau) : texte, taille, couleur, gras
    Texte(String, f64, String, bool),
    /// Un titre de section (`New-SectionTitle`)
    Section(String),
    /// Un encadré à fond de couleur et ses textes (verdict, étape, note du lag) : (texte, couleur)
    Encadre(Vec<(String, String)>),
    /// Une rangée de jauges (`New-GaugeRow`)
    Jauges(Vec<Jauge>),
    /// Des tuiles chiffrées (`New-StatRow`) : (valeur, format, couleur, libellé)
    Tuiles(Vec<(f64, Fmt, String, String)>),
    /// Les détails repliables (`New-Details`) : (nom, valeur), et s'ils sont dépliés
    Details(Vec<(String, String)>, bool),
    /// Les barres de comparaison (`New-CompareBars`) : (libellé, valeur), unité
    Barres(Vec<(String, f64)>, String),
    /// Une courbe (`New-LiveChart`) : points, format, unité, titre (la ligne de référence)
    Courbe(Vec<f64>, Fmt, String, String),
    /// Une rangée (une grille à deux colonnes qui contient des dessins : la note du lag et ses jauges)
    Rangee(Vec<El>),
}

/// Un verdict (`New-Verdict`) : l'icône, puis le texte, de la couleur du statut
pub fn verdict(statut: &str, texte: &str) -> El {
    let c = couleur_statut(statut).to_string();
    let icone = match statut {
        "ok" => "✓",
        "bad" | "warn" => "!",
        _ => "i",
    };
    El::Encadre(vec![(icone.into(), c.clone()), (texte.into(), c)])
}

pub fn section(t: &str) -> El {
    El::Section(t.into())
}

pub fn texte_doux(t: &str) -> El {
    El::Texte(t.into(), 13.0, DOUX.into(), false)
}

static TITRE_CAPITALES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-ZÀ-Ý0-9 '/()-]{4,}\n?$").unwrap());
static ICONE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[✓!i×›]\n?$").unwrap());
static FIN_PONCTUATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[.:]\n?$").unwrap());

/// Un TextBlock traduit (`Add-BlocsWpf`, cas du TextBlock)
fn bloc_texte(t: &str, taille: f64, couleur: &str, gras: bool) -> Option<Value> {
    let t = t.trim();
    if t.is_empty() {
        return None;
    }
    let n = longueur(t);
    let to = ton(couleur);
    if n <= 3 && ICONE.is_match(t) {
        return None;
    }
    if taille >= 30.0 && n <= 12 {
        return Some(json!({ "type": "chiffres", "elements": [{ "titre": "", "valeur": t, "ton": to }] }));
    }
    if gras && n <= 70 && !FIN_PONCTUATION.is_match(t) && TITRE_CAPITALES.is_match(t) {
        return Some(json!({ "type": "titre", "texte": t }));
    }
    Some(if gras && !to.is_empty() {
        json!({ "type": "ligne", "icone": if to == "ok" { "ok" } else { "alerte" }, "couleur": couleur_ton(to), "texte": t })
    } else if gras && n <= 80 {
        json!({ "type": "titre", "texte": t })
    } else {
        json!({ "type": "texte", "texte": t })
    })
}

fn chiffre_jauge(j: &Jauge) -> Value {
    json!({ "titre": j.titre, "valeur": j.fmt.f(j.valeur), "unite": j.unite, "ton": ton(&j.couleur) })
}

/// `[math]::Round(x, d)` (au plus proche, à égalité vers le pair)
pub fn arrondi_dec(x: f64, d: i32) -> f64 {
    let p = 10f64.powi(d);
    (x * p).round_ties_even() / p
}

/// Ce qui est traduit, et les boutons trouvés (leur numéro donne `panneau:b:<n>`)
#[derive(Debug, Default)]
pub struct Traduction {
    pub blocs: Vec<Value>,
    /// Pour chaque bouton trouvé : l'élément (son rang dans le corps) qui le porte
    pub boutons: Vec<usize>,
}

impl Traduction {
    fn plein(&self) -> bool {
        self.blocs.len() > 160
    }

    fn element(&mut self, e: &El, rang: usize) {
        if self.plein() {
            return;
        }
        match e {
            El::Texte(t, taille, c, gras) => {
                if let Some(b) = bloc_texte(t, *taille, c, *gras) {
                    self.blocs.push(b);
                }
            }
            // (police de 11,5 ; couleur de premier plan par défaut, noire, hors de l'arbre visuel)
            El::Section(t) => {
                if let Some(b) = bloc_texte(&format!("●  {}", t.to_uppercase()), 11.5, "#000000", true) {
                    self.blocs.push(b);
                }
            }
            El::Encadre(textes) => self.encadre(textes),
            El::Jauges(js) => {
                if !js.is_empty() {
                    self.blocs.push(
                        json!({ "type": "chiffres", "elements": js.iter().map(chiffre_jauge).collect::<Vec<_>>() }),
                    );
                }
            }
            El::Tuiles(ts) => {
                // (la tuile n'a pas de fond uni : ses deux textes sont traduits l'un après l'autre)
                for (v, fmt, c, libelle) in ts {
                    for b in [bloc_texte(&fmt.f(*v), 28.0, c, true), bloc_texte(libelle, 12.5, DOUX, false)]
                        .into_iter()
                        .flatten()
                    {
                        if self.plein() {
                            return;
                        }
                        self.blocs.push(b);
                    }
                }
            }
            El::Details(rangs, ouvert) => {
                self.boutons.push(rang);
                self.blocs.push(json!({ "type": "bouton", "id": format!("panneau:b:{}", self.boutons.len() - 1),
                    "texte": if *ouvert { "Masquer les infos  ▴" } else { "Voir toutes les infos  ▾" }, "style": "second" }));
                if *ouvert {
                    for (a, b) in rangs {
                        if self.plein() {
                            return;
                        }
                        self.rangee_tableau(&[a.as_str(), b.as_str()]);
                    }
                }
            }
            El::Barres(rangs, unite) => {
                for (l, v) in rangs {
                    if self.plein() {
                        return;
                    }
                    let val = format!("{} {unite}", n_fr(*v, 0));
                    self.rangee_tableau(&[l.as_str(), val.as_str()]);
                }
            }
            El::Courbe(points, fmt, unite, titre) => {
                if points.len() >= 2 {
                    let d = fmt.decimales();
                    let pts: Vec<f64> = points.iter().map(|p| arrondi_dec(*p, d)).collect();
                    self.blocs.push(json!({ "type": "courbe", "points": pts, "unite": unite, "titre": titre }));
                }
            }
            El::Rangee(enfants) => {
                for x in enfants {
                    self.element(x, rang);
                }
            }
        }
    }

    /// Un encadré coloré avec seulement du texte : un verdict, une étape, la note du lag
    fn encadre(&mut self, textes: &[(String, String)]) {
        if (1..=3).contains(&textes.len())
            && let Some((t, c)) = textes.iter().find(|(t, _)| longueur(t.trim()) > 3)
        {
            let to = ton(c);
            let t = t.trim();
            self.blocs.push(if to.is_empty() {
                json!({ "type": "note", "texte": t })
            } else {
                json!({ "type": "ligne", "icone": if to == "ok" { "ok" } else { "alerte" }, "couleur": couleur_ton(to), "texte": t })
            });
            return;
        }
        // (sinon son contenu : une grille à deux colonnes, ou un texte seul)
        if textes.len() >= 2 {
            let l: Vec<&str> = textes.iter().map(|(t, _)| t.as_str()).collect();
            self.rangee_tableau(&l);
        } else {
            for (t, c) in textes {
                if let Some(b) = bloc_texte(t, 13.0, c, false) {
                    self.blocs.push(b);
                }
            }
        }
    }

    /// Une ligne de tableau (nom, détail, valeur), comme une grille de plusieurs colonnes sans bouton
    fn rangee_tableau(&mut self, textes: &[&str]) {
        let tb: Vec<&str> = textes
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty() && !(longueur(t) <= 2 && ICONE.is_match(t)))
            .collect();
        let minuscule = |t: &&str| t.chars().any(|c| c.is_ascii_lowercase() || ('à'..='ÿ').contains(&c));
        if tb.len() >= 2 && !tb.iter().any(minuscule) {
            self.blocs.push(json!({ "type": "titre", "texte": tb[0], "droite": tb[tb.len() - 1] }));
        } else if tb.len() >= 2 {
            let mut l = json!({ "type": "ligne", "texte": tb[0], "valeur": tb[tb.len() - 1] });
            if tb.len() > 2 {
                l["note"] = json!(tb[1..tb.len() - 1].join(" · "));
            }
            self.blocs.push(l);
        } else if tb.len() == 1 {
            self.blocs.push(json!({ "type": "texte", "texte": tb[0] }));
        }
    }
}

/// Les blocs de l'écran d'un corps de panneau
pub fn traduire(corps: &[El]) -> Traduction {
    let mut t = Traduction::default();
    for (i, e) in corps.iter().enumerate() {
        t.element(e, i);
    }
    t
}

/// Les étapes d'un test (`New-Stepper`, `Update-Stepper`) : faites, en cours, à venir
pub fn etapes(libelles: &[&str], courante: Option<usize>, tout_fait: bool) -> Vec<El> {
    let idx = if tout_fait { libelles.len() } else { courante.unwrap_or(usize::MAX) };
    libelles
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let (t, c) = if courante.is_none() && !tout_fait {
                (format!("○  {l}"), "#655E7E")
            } else if i < idx {
                (format!("✓  {l}"), OK)
            } else if i == idx {
                (format!("●  {l}"), BLANC)
            } else {
                (format!("○  {l}"), "#655E7E")
            };
            El::Encadre(vec![(t, c.into())])
        })
        .collect()
}
