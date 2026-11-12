//! La copie des anciens réglages, faite avant tout changement, et « Remettre comme avant ».
//!
//! Une copie par logiciel, dans le dossier des données de Nevermind (`streameur\<obs|streamlabs>\copie.json`). Elle ne
//! garde QUE ce que Nevermind a changé (audit du 09/11, M-B) :
//! - basic.ini : chaque clé changée, avec sa valeur d'avant (ou « absente ») et celle que Nevermind a écrite, et les
//!   sections que Nevermind a dû ajouter ;
//! - streamEncoder.json : le fichier d'avant (ses octets, ou « absent »), l'empreinte de celui que Nevermind a écrit, et
//!   chaque clé changée.
//!
//! « Remettre comme avant » ne remet que ces clés, et seulement celles qui ont encore la valeur écrite par Nevermind :
//! tout ce que l'utilisateur a changé dans OBS depuis (dossier d'enregistrement, son, compte connecté, ou une clé que
//! Nevermind avait changée et qu'il a retouchée) est gardé. streamEncoder.json revient octet pour octet s'il est encore
//! celui que Nevermind a écrit, sinon clé par clé.
//!
//! La copie garde l'état d'AVANT le premier changement de Nevermind : un second « Optimiser » garde les valeurs d'avant
//! déjà notées et n'ajoute que les nouvelles clés. Un contrôle (empreinte de la liste) dit si la copie est abîmée :
//! une copie abîmée n'est jamais remise. Un fichier dont le dossier n'existe plus (profil d'OBS renommé ou supprimé)
//! est compté à part, sans erreur.
//!
//! Les anciennes copies (avant le 10/11/2026 : `copie.json` sans « version », et le fichier entier dans `n.avant`) se
//! relisent sans rien casser : de l'ancien basic.ini, seules les clés que Nevermind sait changer sont reprises et
//! remises (le reste du fichier actuel est gardé) ; l'ancien streamEncoder.json est remis entier, comme avant. Une
//! ancienne copie vide ou illisible est refusée.

use serde_json::{Map, Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Le format de copie.json écrit maintenant
const VERSION: u64 = 2;
/// Le « chemin » des réglages changés par l'API de Streamlabs dans la copie (ce n'est pas un fichier)
pub const CHEMIN_API: &str = "streamlabs:api";

/// Les clés de basic.ini que Nevermind a pu changer avant le 10/11/2026 (pour relire une ancienne copie)
const CLES_ANCIENNES: [(&str, &str); 20] = [
    ("Output", "Mode"),
    ("Video", "OutputCX"),
    ("Video", "OutputCY"),
    ("Video", "FPSType"),
    ("Video", "FPSCommon"),
    ("AdvOut", "Encoder"),
    ("AdvOut", "Track1Bitrate"),
    ("AdvOut", "Track2Bitrate"),
    ("AdvOut", "Track3Bitrate"),
    ("AdvOut", "Track4Bitrate"),
    ("AdvOut", "Track5Bitrate"),
    ("AdvOut", "Track6Bitrate"),
    ("AdvOut", "RescaleFilter"),
    ("AdvOut", "Rescale"),
    ("SimpleOutput", "StreamEncoder"),
    ("SimpleOutput", "VBitrate"),
    ("SimpleOutput", "ABitrate"),
    ("SimpleOutput", "NVENCPreset2"),
    ("SimpleOutput", "AMDPreset"),
    ("SimpleOutput", "Preset"),
];

/// Écrit un fichier d'un coup : d'abord à côté (`.nevermind-tmp`), mis sur le disque, puis il remplace l'ancien (un
/// arrêt brutal ne laisse jamais un fichier à moitié écrit, ni rempli de zéros).
pub fn ecrire_sur(chemin: &Path, octets: &[u8]) -> std::io::Result<()> {
    let mut tmp = chemin.as_os_str().to_owned();
    tmp.push(".nevermind-tmp");
    let tmp = PathBuf::from(tmp);
    let ecrit = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(octets)?;
        // (NTFS ne journalise que les noms : sans cela, le remplacement peut arriver sur le disque avant le contenu)
        f.sync_all()
    })();
    if let Err(e) = ecrit.and_then(|()| std::fs::rename(&tmp, chemin)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// L'empreinte d'octets (FNV-1a sur 64 bits) : pour voir qu'un fichier ou la copie a changé, pas pour la sécurité
fn empreinte(o: &[u8]) -> u64 {
    o.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
}

fn hex(o: &[u8]) -> String {
    o.iter().map(|b| format!("{b:02x}")).collect()
}

fn depuis_hex(t: &str) -> Option<Vec<u8>> {
    if !t.len().is_multiple_of(2) || !t.is_ascii() {
        return None;
    }
    (0..t.len()).step_by(2).map(|i| u8::from_str_radix(&t[i..i + 2], 16).ok()).collect()
}

/// Une clé de basic.ini changée par Nevermind
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleIni {
    pub section: String,
    pub cle: String,
    /// La valeur d'avant (`None` : la clé n'existait pas)
    pub avant: Option<String>,
    /// La valeur écrite par Nevermind (`None` : retirée)
    pub apres: Option<String>,
}

/// Une clé de streamEncoder.json changée par Nevermind (`None` : absente)
#[derive(Debug, Clone, PartialEq)]
pub struct CleJson {
    pub cle: String,
    pub avant: Option<Value>,
    pub apres: Option<Value>,
}

/// Ce que Nevermind change dans un fichier (donné à [`Copie::garder`] avant de l'écrire)
#[derive(Debug, Clone)]
pub enum Modif {
    /// Un fichier .ini (basic.ini) : les clés changées, et les sections ajoutées
    Ini { chemin: PathBuf, cles: Vec<CleIni>, sections: Vec<String> },
    /// Un fichier JSON (streamEncoder.json) : ses octets d'avant (`None` : absent), ceux écrits, les clés changées
    Json { chemin: PathBuf, avant: Option<Vec<u8>>, apres: Vec<u8>, cles: Vec<CleJson> },
    /// Des réglages de Streamlabs changés par son API (streamlabs.rs), chacun avec sa valeur d'avant et celle écrite
    Api { cles: Vec<crate::streamlabs::Reglage> },
}

/// Une clé de .ini dans la copie : `apres` vaut `None` quand la valeur écrite n'est pas connue (ancienne copie) : elle
/// est alors toujours remise
#[derive(Debug, Clone)]
struct CleIniCopie {
    section: String,
    cle: String,
    avant: Option<String>,
    apres: Option<Option<String>>,
}

/// Un fichier dans la copie
#[derive(Debug, Clone)]
enum Entree {
    Ini {
        chemin: String,
        cles: Vec<CleIniCopie>,
        sections: Vec<String>,
    },
    /// `empreinte` : celle du fichier écrit par Nevermind (`None` : pas connue, ancienne copie : remis entier)
    Json {
        chemin: String,
        avant: Option<Vec<u8>>,
        empreinte: Option<u64>,
        cles: Vec<CleJson>,
    },
    /// Des réglages de Streamlabs changés par son API (`cle` : la cible, comme « Video/outputWidth »)
    Api {
        cles: Vec<CleJson>,
    },
    /// Une ancienne copie illisible pour ce fichier : jamais remise
    Abimee {
        chemin: String,
    },
}

impl Entree {
    fn chemin(&self) -> &str {
        match self {
            Entree::Ini { chemin, .. } | Entree::Json { chemin, .. } | Entree::Abimee { chemin } => chemin,
            Entree::Api { .. } => CHEMIN_API,
        }
    }
}

#[derive(Debug, Clone)]
struct Fiche {
    date: String,
    profil: String,
    fichiers: Vec<Entree>,
}

/// copie.json lu
enum Lecture {
    Aucune,
    /// Le fichier est là mais ne se lit pas (coupé, contrôle faux, format inconnu)
    Abimee,
    /// Windows ne laisse pas lire le fichier (pris par un autre programme, comme un antivirus qui l'analyse, ou accès
    /// refusé) : il n'est jamais pris pour abîmé, ni effacé, ni remplacé (lot K)
    Illisible(String),
    Bonne(Fiche),
}

/// Ce que « Remettre comme avant » a fait
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Bilan {
    /// Les fichiers remis (ou qui n'avaient plus rien à remettre)
    pub remis: usize,
    /// Les fichiers dont le dossier n'existe plus (profil d'OBS renommé ou supprimé)
    pub disparus: usize,
    /// Les fichiers dont la copie est abîmée (jamais remis), ou toute la copie
    pub abimes: usize,
    /// Les clés changées par l'utilisateur depuis, gardées telles qu'il les a mises
    pub gardes: usize,
    /// Les parties de la copie pas encore remises (Streamlabs ouvert pour ce que son API a changé, fermé pour ses
    /// fichiers) : gardées pour la fois suivante
    pub restants: usize,
}

/// De quoi défaire [`Copie::garder`] si le changement ne se fait pas
#[derive(Debug)]
pub struct Annulation {
    fiche: Option<Vec<u8>>,
}

/// Des clés JSON (ou de l'API) dans la copie : `avant`, `apres` absents quand la valeur était absente
fn cles_valeur(cles: &[CleJson]) -> Vec<Value> {
    cles.iter()
        .map(|c| {
            let mut o = json!({ "cle": c.cle });
            if let Some(v) = &c.avant {
                o["avant"] = v.clone();
            }
            if let Some(v) = &c.apres {
                o["apres"] = v.clone();
            }
            o
        })
        .collect()
}

fn cles_lues(f: &Value) -> Option<Vec<CleJson>> {
    let mut cles = Vec::new();
    for c in f["cles"].as_array()? {
        cles.push(CleJson {
            cle: c["cle"].as_str()?.to_string(),
            avant: c.get("avant").cloned(),
            apres: c.get("apres").cloned(),
        });
    }
    Some(cles)
}

fn valeur_texte(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

impl Fiche {
    fn valeur(&self) -> Value {
        let fichiers: Vec<Value> = self
            .fichiers
            .iter()
            .map(|e| match e {
                Entree::Ini { chemin, cles, sections } => {
                    let cles: Vec<Value> = cles
                        .iter()
                        .map(|c| {
                            let mut o = json!({ "section": c.section, "cle": c.cle, "avant": c.avant });
                            if let Some(a) = &c.apres {
                                o["apres"] = json!(a);
                            }
                            o
                        })
                        .collect();
                    json!({ "chemin": chemin, "sorte": "ini", "cles": cles, "sections_ajoutees": sections })
                }
                Entree::Json { chemin, avant, empreinte, cles } => {
                    json!({ "chemin": chemin, "sorte": "json", "avant": avant.as_deref().map(hex),
                        "empreinte": empreinte.map(|e| format!("{e:016x}")), "cles": cles_valeur(cles) })
                }
                Entree::Api { cles } => json!({ "chemin": CHEMIN_API, "sorte": "api", "cles": cles_valeur(cles) }),
                Entree::Abimee { chemin } => json!({ "chemin": chemin, "sorte": "abimee" }),
            })
            .collect();
        let fichiers = Value::Array(fichiers);
        let controle = format!("{:016x}", empreinte(&serde_json::to_vec(&fichiers).unwrap_or_default()));
        json!({ "version": VERSION, "date": self.date, "profil": self.profil, "controle": controle, "fichiers": fichiers })
    }

    /// Une copie au format d'aujourd'hui. `None` : abîmée.
    fn lire(v: &Value) -> Option<Fiche> {
        let fichiers = v.get("fichiers")?;
        let controle = format!("{:016x}", empreinte(&serde_json::to_vec(fichiers).ok()?));
        if v["controle"].as_str() != Some(controle.as_str()) {
            return None;
        }
        let mut l = Vec::new();
        for f in fichiers.as_array()? {
            let chemin = f["chemin"].as_str()?.to_string();
            l.push(match f["sorte"].as_str()? {
                "ini" => {
                    let mut cles = Vec::new();
                    for c in f["cles"].as_array()? {
                        cles.push(CleIniCopie {
                            section: c["section"].as_str()?.to_string(),
                            cle: c["cle"].as_str()?.to_string(),
                            avant: valeur_texte(&c["avant"]),
                            apres: c.get("apres").map(valeur_texte),
                        });
                    }
                    let sections =
                        f["sections_ajoutees"].as_array()?.iter().filter_map(|s| s.as_str().map(str::to_string));
                    Entree::Ini { chemin, cles, sections: sections.collect() }
                }
                "json" => {
                    let avant = match &f["avant"] {
                        Value::Null => None,
                        x => Some(depuis_hex(x.as_str()?)?),
                    };
                    let empreinte = match &f["empreinte"] {
                        Value::Null => None,
                        x => Some(u64::from_str_radix(x.as_str()?, 16).ok()?),
                    };
                    Entree::Json { chemin, avant, empreinte, cles: cles_lues(f)? }
                }
                "api" => Entree::Api { cles: cles_lues(f)? },
                "abimee" => Entree::Abimee { chemin },
                _ => return None,
            });
        }
        Some(Fiche {
            date: v["date"].as_str().unwrap_or_default().to_string(),
            profil: v["profil"].as_str().unwrap_or_default().to_string(),
            fichiers: l,
        })
    }

    /// Une ancienne copie (avant le 10/11/2026) : chaque fichier entier dans `n.avant`. `None` : la liste ne se lit pas.
    fn lire_ancienne(v: &Value, dossier: &Path) -> Option<Fiche> {
        let mut l = Vec::new();
        for f in v["fichiers"].as_array()? {
            let chemin = f["chemin"].as_str()?.to_string();
            let existait = f["existait"].as_bool()?;
            let basic = Path::new(&chemin).file_name().is_some_and(|n| n.eq_ignore_ascii_case("basic.ini"));
            let octets = if existait {
                match f["copie"].as_str().and_then(|n| std::fs::read(dossier.join(n)).ok()) {
                    Some(o) => Some(o),
                    None => {
                        l.push(Entree::Abimee { chemin });
                        continue;
                    }
                }
            } else {
                None
            };
            l.push(match (basic, octets) {
                // (basic.ini existe toujours avant un « Optimiser » : rien à faire sans lui)
                (true, None) => Entree::Ini { chemin, cles: Vec::new(), sections: Vec::new() },
                (true, Some(o)) => match crate::ini::Ini::lire(&o) {
                    // (un basic.ini d'OBS a toujours au moins une section : un fichier vide est une copie abîmée)
                    Ok(i) if !i.sections().is_empty() => {
                        let cles = CLES_ANCIENNES
                            .iter()
                            .map(|(s, k)| CleIniCopie {
                                section: s.to_string(),
                                cle: k.to_string(),
                                avant: i.valeur(s, k).map(str::to_string),
                                apres: None,
                            })
                            .collect();
                        let presentes = i.sections();
                        let mut sections: Vec<String> = CLES_ANCIENNES
                            .iter()
                            .map(|(s, _)| s.to_string())
                            .filter(|s| !presentes.contains(s))
                            .collect();
                        sections.dedup();
                        Entree::Ini { chemin, cles, sections }
                    }
                    _ => Entree::Abimee { chemin },
                },
                (false, None) => Entree::Json { chemin, avant: None, empreinte: None, cles: Vec::new() },
                (false, Some(o)) if crate::reglages::objet(Some(&o)).is_some() => {
                    Entree::Json { chemin, avant: Some(o), empreinte: None, cles: Vec::new() }
                }
                (false, Some(_)) => Entree::Abimee { chemin },
            });
        }
        Some(Fiche {
            date: v["date"].as_str().unwrap_or_default().to_string(),
            profil: v["profil"].as_str().unwrap_or_default().to_string(),
            fichiers: l,
        })
    }

    /// Ajoute ce que Nevermind va changer : les valeurs d'avant déjà notées sont gardées
    fn ajouter(&mut self, m: &Modif) {
        let chemin_de = |p: &Path| p.to_string_lossy().into_owned();
        let (chemin, neuve) = match m {
            Modif::Ini { chemin, cles, sections } => {
                let cles = cles
                    .iter()
                    .map(|c| CleIniCopie {
                        section: c.section.clone(),
                        cle: c.cle.clone(),
                        avant: c.avant.clone(),
                        apres: Some(c.apres.clone()),
                    })
                    .collect();
                (chemin_de(chemin), Entree::Ini { chemin: chemin_de(chemin), cles, sections: sections.clone() })
            }
            Modif::Json { chemin, avant, apres, cles } => (
                chemin_de(chemin),
                Entree::Json {
                    chemin: chemin_de(chemin),
                    avant: avant.clone(),
                    empreinte: Some(empreinte(apres)),
                    cles: cles.clone(),
                },
            ),
            Modif::Api { cles } => (
                CHEMIN_API.to_string(),
                Entree::Api {
                    cles: cles
                        .iter()
                        .map(|r| CleJson {
                            cle: r.cible.clone(),
                            avant: Some(r.avant.clone()).filter(|v| !v.is_null()),
                            apres: Some(r.apres.clone()).filter(|v| !v.is_null()),
                        })
                        .collect(),
                },
            ),
        };
        let Some(e) = self.fichiers.iter_mut().find(|e| e.chemin().eq_ignore_ascii_case(&chemin)) else {
            self.fichiers.push(neuve);
            return;
        };
        match (e, neuve) {
            (Entree::Ini { cles, sections, .. }, Entree::Ini { cles: n, sections: ns, .. }) => {
                for c in n {
                    match cles.iter_mut().find(|x| x.section == c.section && x.cle == c.cle) {
                        Some(x) => x.apres = c.apres,
                        None => cles.push(c),
                    }
                }
                for s in ns {
                    if !sections.contains(&s) {
                        sections.push(s);
                    }
                }
            }
            (Entree::Json { cles, empreinte: emp, .. }, Entree::Json { cles: n, empreinte: ne, .. }) => {
                // (une ancienne copie, sans empreinte, reste remise entière : son fichier d'avant est gardé)
                if emp.is_some() {
                    *emp = ne;
                }
                for c in n {
                    match cles.iter_mut().find(|x| x.cle == c.cle) {
                        Some(x) => x.apres = c.apres,
                        None => cles.push(c),
                    }
                }
            }
            (Entree::Api { cles }, Entree::Api { cles: n }) => {
                for c in n {
                    match cles.iter_mut().find(|x| x.cle == c.cle) {
                        Some(x) => x.apres = c.apres,
                        None => cles.push(c),
                    }
                }
            }
            // (une copie abîmée pour ce fichier : l'état d'aujourd'hui devient celui d'avant)
            (e, neuve) => *e = neuve,
        }
    }
}

/// Une erreur de partage est passagère (ERROR_SHARING_VIOLATION 32, ERROR_LOCK_VIOLATION 33) : réessayée, au plus
/// [`ESSAIS_PARTAGE`] fois, toutes les [`PAUSE_PARTAGE`] (environ une seconde en tout)
const ESSAIS_PARTAGE: u32 = 5;
const PAUSE_PARTAGE: std::time::Duration = std::time::Duration::from_millis(200);

fn erreur_de_partage(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(32 | 33))
}

/// La copie d'un logiciel
#[derive(Debug, Clone)]
pub struct Copie {
    pub dossier: PathBuf,
}

impl Copie {
    pub fn nouvelle(dossier: PathBuf) -> Copie {
        Copie { dossier }
    }

    fn fiche(&self) -> PathBuf {
        self.dossier.join("copie.json")
    }

    /// Les octets de copie.json (`None` : pas de copie). `attendre` : une erreur de partage (le fichier tenu un instant
    /// par un antivirus ou une sauvegarde) est réessayée [`ESSAIS_PARTAGE`] fois, toutes les [`PAUSE_PARTAGE`], avant de
    /// conclure ; jamais sur le fil des blocs pour un simple affichage (seulement au clic). Toute autre erreur (accès
    /// refusé...) est rendue telle quelle : le fichier n'est pas lu, il n'est pas pour autant abîmé.
    fn charger(&self, attendre: bool) -> Result<Option<Vec<u8>>, String> {
        let mut reste = if attendre { ESSAIS_PARTAGE } else { 0 };
        loop {
            match std::fs::read(self.fiche()) {
                Ok(o) => return Ok(Some(o)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) if reste > 0 && erreur_de_partage(&e) => {
                    reste -= 1;
                    std::thread::sleep(PAUSE_PARTAGE);
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    /// copie.json lu, sans attendre (pour l'affichage)
    fn lire(&self) -> Lecture {
        self.lire_avec(false)
    }

    fn lire_avec(&self, attendre: bool) -> Lecture {
        match self.charger(attendre) {
            Ok(o) => self.interpreter(o.as_deref()),
            Err(e) => Lecture::Illisible(e),
        }
    }

    /// Le contenu de copie.json (`None` : pas de copie)
    fn interpreter(&self, o: Option<&[u8]>) -> Lecture {
        let Some(o) = o else { return Lecture::Aucune };
        let Ok(v) = serde_json::from_slice::<Value>(o) else { return Lecture::Abimee };
        let f = match v.get("version") {
            None => Fiche::lire_ancienne(&v, &self.dossier),
            Some(n) if n.as_u64() == Some(VERSION) => Fiche::lire(&v),
            Some(_) => None,
        };
        f.map_or(Lecture::Abimee, Lecture::Bonne)
    }

    /// Une copie existe (« Remettre comme avant » est montré ; une copie abîmée aussi, pour le dire au clic)
    pub fn existe(&self) -> bool {
        !matches!(self.lire(), Lecture::Aucune)
    }

    /// La date de la copie (« 08/11/2026 à 23:10 »)
    pub fn date(&self) -> Option<String> {
        match self.lire() {
            Lecture::Bonne(f) => Some(f.date),
            _ => None,
        }
    }

    /// Les fichiers gardés (chemins)
    pub fn fichiers(&self) -> Vec<PathBuf> {
        match self.lire() {
            Lecture::Bonne(f) => f.fichiers.iter().map(|e| PathBuf::from(e.chemin())).collect(),
            _ => Vec::new(),
        }
    }

    /// Note ce que Nevermind va changer, AVANT de l'écrire. Erreur : rien ne doit être changé (la copie n'a pas pu être
    /// faite). Rend de quoi revenir à la copie d'avant si l'écriture rate ([`Copie::annuler`]).
    pub fn garder(&self, modifs: &[Modif], date: &str, profil: &str) -> Result<Annulation, String> {
        std::fs::create_dir_all(&self.dossier).map_err(|e| format!("dossier de la copie : {e}"))?;
        // (une copie que Windows ne laisse pas lire n'est jamais remplacée : elle garde les réglages d'origine ; lot K)
        let avant =
            self.charger(true).map_err(|e| format!("copie illisible pour l'instant ({e}), rien n'est changé"))?;
        let mut fiche = match self.interpreter(avant.as_deref()) {
            Lecture::Bonne(f) => f,
            // (une copie abîmée ne se remettra jamais : elle est remplacée par l'état d'aujourd'hui)
            Lecture::Aucune | Lecture::Abimee | Lecture::Illisible(_) => {
                Fiche { date: date.to_string(), profil: profil.to_string(), fichiers: Vec::new() }
            }
        };
        for m in modifs {
            fiche.ajouter(m);
        }
        let t = serde_json::to_vec_pretty(&fiche.valeur()).map_err(|e| e.to_string())?;
        ecrire_sur(&self.fiche(), &t).map_err(|e| format!("fiche de la copie : {e}"))?;
        // (relue : la copie doit être bonne avant qu'on touche aux réglages ; un antivirus peut l'analyser juste après)
        if !matches!(self.lire_avec(true), Lecture::Bonne(_)) {
            let _ = self.annuler(Annulation { fiche: avant });
            return Err("fiche de la copie : relue différente".into());
        }
        Ok(Annulation { fiche: avant })
    }

    /// Revient à la copie d'avant [`Copie::garder`] (le changement n'a pas pu se faire) : pas de copie s'il n'y en
    /// avait pas
    pub fn annuler(&self, a: Annulation) -> Result<(), String> {
        match a.fiche {
            Some(o) => ecrire_sur(&self.fiche(), &o).map_err(|e| e.to_string()),
            None => {
                let r = std::fs::remove_file(self.fiche());
                let _ = std::fs::remove_dir(&self.dossier);
                match r {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                    _ => Ok(()),
                }
            }
        }
    }

    /// Remet ce que Nevermind a changé, puis efface la copie. Erreur : un fichier n'a pas pu être écrit (la copie est
    /// gardée pour réessayer).
    pub fn remettre(&self) -> Result<Bilan, String> {
        self.remettre_avec(None)
    }

    /// Ce que la copie demande pour être remise : (des réglages changés par l'API de Streamlabs, qui le veut ouvert ;
    /// des fichiers, qui le veulent fermé). Une copie illisible ou abîmée ne demande rien.
    pub fn besoins(&self) -> (bool, bool) {
        match self.lire() {
            Lecture::Bonne(f) => (
                f.fichiers.iter().any(|e| matches!(e, Entree::Api { .. })),
                f.fichiers.iter().any(|e| !matches!(e, Entree::Api { .. })),
            ),
            _ => (false, false),
        }
    }

    /// [`Copie::remettre`], avec l'API de Streamlabs pour les réglages qu'elle a changés (`None` : pas d'API, ces
    /// réglages ne se remettent pas, la copie est gardée)
    pub fn remettre_avec(&self, porte: Option<&dyn crate::streamlabs::Porte>) -> Result<Bilan, String> {
        let fiche = match self.lire_avec(true) {
            Lecture::Aucune => return Err("aucune copie des anciens réglages".into()),
            // (pas lue : elle est gardée telle quelle pour réessayer, jamais effacée ; lot K)
            Lecture::Illisible(e) => return Err(format!("copie illisible pour l'instant ({e}), gardée")),
            Lecture::Abimee => {
                let _ = std::fs::remove_dir_all(&self.dossier);
                return Ok(Bilan { abimes: 1, ..Bilan::default() });
            }
            Lecture::Bonne(f) => f,
        };
        let mut bilan = Bilan::default();
        let mut erreurs = Vec::new();
        // (Streamlabs ouvert : seulement ce que l'API a changé ; fermé : seulement les fichiers. Le reste est gardé dans
        // la copie, pour la fois suivante)
        let mut restants = Vec::new();
        for e in &fiche.fichiers {
            if let Entree::Api { cles } = e {
                let Some(p) = porte else {
                    restants.push(e.clone());
                    continue;
                };
                match remettre_api(p, cles) {
                    Ok(g) => {
                        bilan.remis += 1;
                        bilan.gardes += g;
                    }
                    Err(x) => erreurs.push(format!("Streamlabs : {x}")),
                }
                continue;
            }
            if porte.is_some() && !matches!(e, Entree::Abimee { .. }) {
                restants.push(e.clone());
                continue;
            }
            let chemin = Path::new(e.chemin());
            if !chemin.parent().is_some_and(Path::is_dir) {
                bilan.disparus += 1;
                continue;
            }
            let r = match e {
                Entree::Abimee { .. } | Entree::Api { .. } => {
                    bilan.abimes += 1;
                    continue;
                }
                Entree::Ini { cles, sections, .. } => remettre_ini(chemin, cles, sections),
                Entree::Json { avant, empreinte, cles, .. } => {
                    remettre_json(chemin, avant.as_deref(), *empreinte, cles)
                }
            };
            match r {
                Ok(g) => {
                    bilan.remis += 1;
                    bilan.gardes += g;
                }
                Err(x) => erreurs.push(format!("{} : {x}", chemin.display())),
            }
        }
        if !erreurs.is_empty() {
            return Err(erreurs.join(" ; "));
        }
        if restants.is_empty() {
            let _ = std::fs::remove_dir_all(&self.dossier);
        } else {
            bilan.restants = restants.len();
            let reste = Fiche { date: fiche.date, profil: fiche.profil, fichiers: restants };
            let t = serde_json::to_vec_pretty(&reste.valeur()).map_err(|e| e.to_string())?;
            ecrire_sur(&self.fiche(), &t).map_err(|e| format!("fiche de la copie : {e}"))?;
        }
        Ok(bilan)
    }
}

/// Écrit et relit
fn ecrire_verifie(chemin: &Path, o: &[u8]) -> Result<(), String> {
    ecrire_sur(chemin, o).map_err(|e| e.to_string())?;
    if std::fs::read(chemin).ok().as_deref() != Some(o) {
        return Err("relu différent".into());
    }
    Ok(())
}

/// Le fichier actuel, `None` s'il n'existe plus
fn lire_actuel(chemin: &Path) -> Result<Option<Vec<u8>>, String> {
    match std::fs::read(chemin) {
        Ok(o) => Ok(Some(o)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Remet les clés d'un .ini qui ont encore la valeur écrite par Nevermind. Rend le nombre de clés gardées (changées par
/// l'utilisateur depuis).
fn remettre_ini(chemin: &Path, cles: &[CleIniCopie], sections: &[String]) -> Result<usize, String> {
    // (le fichier a été effacé depuis : plus rien de Nevermind dedans)
    let Some(o) = lire_actuel(chemin)? else { return Ok(0) };
    let mut i = crate::ini::Ini::lire(&o)?;
    let mut gardes = 0;
    for c in cles {
        let actuelle = i.valeur(&c.section, &c.cle).map(str::to_string);
        if actuelle == c.avant {
            continue;
        }
        if c.apres.as_ref().is_some_and(|a| *a != actuelle) {
            gardes += 1;
            continue;
        }
        match &c.avant {
            Some(v) => i.mettre(&c.section, &c.cle, v),
            None => i.retirer(&c.section, &c.cle),
        };
    }
    for s in sections {
        i.retirer_section_vide(s);
    }
    let n = i.octets();
    if n != o {
        ecrire_verifie(chemin, &n)?;
    }
    Ok(gardes)
}

/// Remet streamEncoder.json : entier s'il est encore celui écrit par Nevermind (ou ancienne copie), sinon les clés qui
/// ont encore la valeur écrite par Nevermind. Rend le nombre de clés gardées.
fn remettre_json(chemin: &Path, avant: Option<&[u8]>, emp: Option<u64>, cles: &[CleJson]) -> Result<usize, String> {
    let Some(o) = lire_actuel(chemin)? else { return Ok(0) };
    if emp.is_none_or(|e| e == empreinte(&o)) {
        match avant {
            Some(a) => ecrire_verifie(chemin, a)?,
            None => std::fs::remove_file(chemin).map_err(|e| e.to_string())?,
        }
        return Ok(0);
    }
    // (changé depuis, et illisible : OBS le lit comme vide ; Nevermind n'y touche pas)
    let Some(mut m) = crate::reglages::objet(Some(&o)) else { return Ok(1) };
    let mut gardes = 0;
    let mut change = false;
    for c in cles {
        let actuelle = m.get(&c.cle).cloned();
        if actuelle == c.avant {
            continue;
        }
        if actuelle != c.apres {
            gardes += 1;
            continue;
        }
        match &c.avant {
            Some(v) => m.insert(c.cle.clone(), v.clone()),
            None => m.remove(&c.cle),
        };
        change = true;
    }
    if change {
        if avant.is_none() && m.is_empty() {
            std::fs::remove_file(chemin).map_err(|e| e.to_string())?;
        } else {
            ecrire_verifie(chemin, &crate::reglages::json_obs(&m))?;
        }
    }
    Ok(gardes)
}

/// Remet par l'API de Streamlabs les réglages qui ont encore la valeur écrite par Nevermind, et vérifie qu'ils ont
/// pris. Rend le nombre de réglages gardés (changés par l'utilisateur depuis).
fn remettre_api(p: &dyn crate::streamlabs::Porte, cles: &[CleJson]) -> Result<usize, String> {
    use crate::streamlabs::{Reglage, actuelle, ecrire, lire, meme, pas_pris};
    let lu = lire(p)?;
    if lu.occupe {
        return Err("Streamlabs est en train de streamer ou d'enregistrer".into());
    }
    let mut gardes = 0;
    let mut a_remettre = Vec::new();
    for c in cles {
        let maintenant = actuelle(&lu, &c.cle);
        let avant = c.avant.clone().unwrap_or(Value::Null);
        let ecrit = c.apres.clone().unwrap_or(Value::Null);
        if meme(&maintenant, &avant) {
            continue;
        }
        if !meme(&maintenant, &ecrit) {
            gardes += 1;
            continue;
        }
        // (une valeur d'avant absente ne se remet pas par l'API : le réglage reste tel quel)
        if !avant.is_null() {
            a_remettre.push(Reglage { cible: c.cle.clone(), avant: maintenant, apres: avant });
        }
    }
    if a_remettre.is_empty() {
        return Ok(gardes);
    }
    ecrire(p, &a_remettre, false)?;
    let rate = pas_pris(p, &a_remettre)?;
    if !rate.is_empty() {
        return Err(format!("pas repris par Streamlabs : {}", rate.join(", ")));
    }
    Ok(gardes)
}

/// Les clés qui diffèrent entre deux objets JSON (streamEncoder.json avant et après)
pub fn cles_json(avant: &Map<String, Value>, apres: &Map<String, Value>) -> Vec<CleJson> {
    let mut l: Vec<CleJson> = Vec::new();
    for k in avant.keys().chain(apres.keys()) {
        if l.iter().any(|c| c.cle == *k) || avant.get(k) == apres.get(k) {
            continue;
        }
        l.push(CleJson { cle: k.clone(), avant: avant.get(k).cloned(), apres: apres.get(k).cloned() });
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ini::Ini;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nevermind-stream-copie-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Ce que ferait Nevermind : poser ces clés dans basic.ini, et la modif à donner à la copie
    fn changer(f: &Path, cles: &[(&str, &str, &str)]) -> (Vec<u8>, Modif) {
        let avant = Ini::lire(&std::fs::read(f).unwrap()).unwrap();
        let mut i = avant.clone();
        let mut l = Vec::new();
        for (s, k, v) in cles {
            i.mettre(s, k, v);
            l.push(CleIni {
                section: s.to_string(),
                cle: k.to_string(),
                avant: avant.valeur(s, k).map(str::to_string),
                apres: Some(v.to_string()),
            });
        }
        let sections = i.sections().into_iter().filter(|s| !avant.sections().contains(s)).collect();
        (i.octets(), Modif::Ini { chemin: f.to_path_buf(), cles: l, sections })
    }

    #[test]
    fn rend_les_valeurs_d_avant() {
        let d = dossier("valeurs");
        let a = d.join("basic.ini");
        let origine: Vec<u8> = b"\xEF\xBB\xBF[Video]\nBaseCX=1920\nOutputCX=1280\n\xC3\xA9t\xC3\xA9=1\n".to_vec();
        std::fs::write(&a, &origine).unwrap();
        let c = Copie::nouvelle(d.join("copie"));
        assert!(!c.existe());
        let (n, m) = changer(&a, &[("Video", "OutputCX", "1920"), ("SimpleOutput", "VBitrate", "6000")]);
        c.garder(&[m], "08/11/2026 à 23:00", "Sans titre").unwrap();
        std::fs::write(&a, n).unwrap();
        assert!(c.existe());
        // un second « Optimiser » garde les valeurs d'avant le premier
        let (n, m) = changer(&a, &[("Video", "OutputCX", "1600"), ("Video", "FPSCommon", "60")]);
        c.garder(&[m], "09/11/2026 à 10:00", "Sans titre").unwrap();
        std::fs::write(&a, n).unwrap();
        assert_eq!(c.date().as_deref(), Some("08/11/2026 à 23:00"));
        assert_eq!(c.remettre().unwrap(), Bilan { remis: 1, ..Bilan::default() });
        assert_eq!(std::fs::read(&a).unwrap(), origine, "les clés et la section ajoutées sont retirées");
        assert!(!c.existe());
        assert!(c.remettre().is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn json_entier_ou_cle_par_cle() {
        let d = dossier("json");
        let f = d.join("streamEncoder.json");
        let c = Copie::nouvelle(d.join("copie"));
        // il n'existait pas : retiré s'il est encore celui de Nevermind
        let apres = br#"{"bitrate": 6000}"#.to_vec();
        let m = Modif::Json {
            chemin: f.clone(),
            avant: None,
            apres: apres.clone(),
            cles: vec![CleJson { cle: "bitrate".into(), avant: None, apres: Some(json!(6000)) }],
        };
        c.garder(std::slice::from_ref(&m), "d", "p").unwrap();
        std::fs::write(&f, &apres).unwrap();
        c.remettre().unwrap();
        assert!(!f.exists());
        // l'utilisateur y a ajouté un réglage : seul celui de Nevermind part
        c.garder(&[m], "d", "p").unwrap();
        std::fs::write(&f, br#"{"bitrate": 6000, "bf": 2}"#).unwrap();
        assert_eq!(c.remettre().unwrap().gardes, 0);
        let j: Value = serde_json::from_slice(&std::fs::read(&f).unwrap()).unwrap();
        assert_eq!(j, json!({ "bf": 2 }));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn annuler_rend_la_copie_d_avant() {
        let d = dossier("annuler");
        let a = d.join("basic.ini");
        std::fs::write(&a, b"[Video]\nOutputCX=1280\n").unwrap();
        let c = Copie::nouvelle(d.join("copie"));
        let (_, m) = changer(&a, &[("Video", "OutputCX", "1920")]);
        let x = c.garder(std::slice::from_ref(&m), "d", "p").unwrap();
        c.annuler(x).unwrap();
        assert!(!c.existe() && !c.dossier.exists());
        c.garder(std::slice::from_ref(&m), "d", "p").unwrap();
        let avant = std::fs::read(c.fiche()).unwrap();
        let (_, m2) = changer(&a, &[("Video", "FPSCommon", "60")]);
        let x = c.garder(&[m2], "d2", "p").unwrap();
        assert_ne!(std::fs::read(c.fiche()).unwrap(), avant);
        c.annuler(x).unwrap();
        assert_eq!(std::fs::read(c.fiche()).unwrap(), avant);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ecriture_d_un_coup() {
        let d = dossier("ecrire");
        let f = d.join("x.ini");
        ecrire_sur(&f, b"1").unwrap();
        ecrire_sur(&f, b"22").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"22");
        assert!(!d.join("x.ini.nevermind-tmp").exists());
        // un dossier qui n'existe pas : erreur, rien ne reste
        assert!(ecrire_sur(&d.join("absent").join("y.ini"), b"1").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn hex_et_empreinte() {
        assert_eq!(depuis_hex(&hex(b"\x00\xffab")).as_deref(), Some(&b"\x00\xffab"[..]));
        assert_eq!(depuis_hex("abc"), None);
        assert_eq!(depuis_hex("zz"), None);
        assert_ne!(empreinte(b"a"), empreinte(b"b"));
        assert_eq!(empreinte(b""), 0xcbf2_9ce4_8422_2325);
    }
}
