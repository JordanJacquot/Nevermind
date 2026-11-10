//! Les réglages d'OBS Studio et de Streamlabs Desktop : où ils sont, ce qu'ils disent, et le changement lui-même.
//!
//! OBS (recherche, partie 1) :
//! - les réglages sont dans `%APPDATA%\obs-studio\`, ou dans `<installation>\config\obs-studio\` en mode portable ;
//! - la version vient de `global.ini [General] LastVersion` ;
//! - le profil actif : `user.ini [Basic] Profile / ProfileDir` (OBS 31 et plus ; avant : `global.ini [Basic]`), vérifié
//!   par le `[General] Name` du basic.ini du profil (OBS compare les noms) ; `[Locations]` de global.ini peut déplacer
//!   les profils (31 et plus) ;
//! - les fichiers d'un profil : basic.ini (sorties, vidéo), service.json (plateforme, lue sans la clé),
//!   streamEncoder.json (réglages de l'encodeur en mode Avancé).
//!
//! Streamlabs (recherche, partie 2) : tout est dans `%APPDATA%\slobs-client\` (pas de profils) ; sa résolution et ses
//! images par seconde ne sont plus lues dans basic.ini (stockage interne de l'appli) : Nevermind ne règle que
//! l'encodeur, le débit et la vitesse de l'encodeur, et dit quoi choisir pour la résolution.
//!
//! Le changement : seulement les clés nécessaires, tout le reste du fichier intact (voir ini.rs), après avoir noté
//! les anciennes valeurs des clés changées (sauvegarde.rs), en écrivant d'un coup chaque fichier. Si un fichier ne
//! s'écrit pas, ceux déjà écrits sont remis comme ils étaient, et la copie revient comme elle était.

use crate::calcul::{Encodeur, Logiciel, Plan, Version, nombre};
use crate::ini::Ini;
use crate::sauvegarde::{CleIni, CleJson, Copie, Modif, ecrire_sur};
use crate::service::{self, Service};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// Pourquoi les réglages ne peuvent pas être lus
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probleme {
    /// Le logiciel n'a encore jamais été ouvert (aucun réglage, aucun profil)
    JamaisOuvert,
    /// Un fichier de réglages est abîmé (son nom)
    Abime(String),
}

/// Les réglages actuels, en bref
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Actuel {
    pub avance: bool,
    /// L'encodeur (sa famille), `None` : pas écrit ou autre (HEVC, AV1)
    pub encodeur: Option<Encodeur>,
    pub video_kbps: Option<u32>,
    pub sortie: Option<(u32, u32)>,
    pub ips: Option<String>,
}

/// Ce que Nevermind a lu des réglages
#[derive(Debug, Clone)]
pub struct Etat {
    pub logiciel: Logiciel,
    /// Le nom du profil (OBS), vide pour Streamlabs
    pub profil: String,
    /// Le dossier des fichiers (celui du profil pour OBS)
    pub dossier: PathBuf,
    pub nb_profils: usize,
    pub version: Option<Version>,
    pub service: Option<Service>,
    /// L'image de base (le canevas) si elle est écrite
    pub base: Option<(u32, u32)>,
    pub actuel: Actuel,
}

/// Le fichier .ini d'un chemin : `Ok(None)` s'il n'existe pas
fn lire_ini(p: &Path) -> Result<Option<Ini>, Probleme> {
    match std::fs::read(p) {
        Ok(o) => Ini::lire(&o).map(Some).map_err(|_| Probleme::Abime(nom(p))),
        Err(_) => Ok(None),
    }
}

fn nom(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Un dossier de `[Locations]` (OBS 31 et plus), s'il existe : `<valeur>\obs-studio`
fn emplacement(global: Option<&Ini>, cle: &str) -> Option<PathBuf> {
    let v = global?.valeur("Locations", cle)?;
    let p = Path::new(v).join("obs-studio");
    (Path::new(v).is_absolute() && p.is_dir()).then_some(p)
}

/// Les profils d'OBS : (dossier, nom écrit dans son basic.ini), triés par dossier
fn profils(dossier: &Path) -> Vec<(PathBuf, String)> {
    let mut l: Vec<(PathBuf, String)> = std::fs::read_dir(dossier)
        .map(|it| {
            it.flatten()
                .map(|e| e.path())
                .filter(|p| p.join("basic.ini").is_file())
                .map(|p| {
                    let n = lire_ini(&p.join("basic.ini"))
                        .ok()
                        .flatten()
                        .and_then(|i| i.valeur("General", "Name").map(str::to_string))
                        .unwrap_or_default();
                    (p, n)
                })
                .collect()
        })
        .unwrap_or_default();
    l.sort();
    l
}

/// Le profil actif d'OBS dans ce dossier de réglages (`...\obs-studio`) : (dossier, nom, nombre de profils, version)
pub fn profil_obs(racine: &Path) -> Result<(PathBuf, String, usize, Option<Version>), Probleme> {
    let global = lire_ini(&racine.join("global.ini"))?;
    let version = global.as_ref().and_then(|g| g.valeur("General", "LastVersion")).and_then(Version::lire);
    let conf = emplacement(global.as_ref(), "Configuration").unwrap_or_else(|| racine.to_path_buf());
    let user = lire_ini(&conf.join("user.ini"))?;
    // (31 et plus : user.ini seul ; l'ancien global.ini garde souvent un profil périmé)
    let choix = user.as_ref().or(global.as_ref());
    let voulu = choix.and_then(|c| c.valeur("Basic", "Profile")).unwrap_or("").to_string();
    let voulu_dossier = choix.and_then(|c| c.valeur("Basic", "ProfileDir")).unwrap_or("").to_string();
    let base = emplacement(global.as_ref(), "Profiles").unwrap_or_else(|| racine.to_path_buf());
    let liste = profils(&base.join("basic").join("profiles"));
    if liste.is_empty() {
        return Err(Probleme::JamaisOuvert);
    }
    let dossier_de = |p: &PathBuf| nom(p);
    let trouve = liste
        .iter()
        .find(|(p, n)| !voulu.is_empty() && *n == voulu && dossier_de(p) == voulu_dossier)
        .or_else(|| liste.iter().find(|(_, n)| !voulu.is_empty() && *n == voulu))
        .or_else(|| liste.iter().find(|(p, _)| !voulu_dossier.is_empty() && dossier_de(p) == voulu_dossier))
        .or_else(|| (liste.len() == 1).then(|| &liste[0]));
    let Some((p, n)) = trouve else {
        // (plusieurs profils et aucun ne correspond : on ne devine pas)
        return Err(Probleme::JamaisOuvert);
    };
    let n = if n.is_empty() { dossier_de(p) } else { n.clone() };
    Ok((p.clone(), n, liste.len(), version))
}

fn taille(i: &Ini, s: &str, x: &str, y: &str) -> Option<(u32, u32)> {
    let w = i.entier(s, x)?;
    let h = i.entier(s, y)?;
    (w > 0 && h > 0 && w < 100_000 && h < 100_000).then_some((w as u32, h as u32))
}

/// Les images par seconde écrites dans [Video] (texte), `None` si rien n'est écrit
fn ips(i: &Ini) -> Option<String> {
    match i.entier("Video", "FPSType").unwrap_or(0) {
        1 => i.valeur("Video", "FPSInt").map(str::to_string),
        2 => {
            let n = i.entier("Video", "FPSNum")?;
            let d = i.entier("Video", "FPSDen").filter(|d| *d > 0)?;
            Some(if n % d == 0 { (n / d).to_string() } else { format!("{:.2}", n as f64 / d as f64) })
        }
        _ => i.valeur("Video", "FPSCommon").map(str::to_string),
    }
}

/// Le JSON d'un objet (streamEncoder.json), `None` : absent ou abîmé (OBS prend alors un objet vide)
pub(crate) fn objet(o: Option<&[u8]>) -> Option<Map<String, Value>> {
    let o = o?;
    let o = o.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(o);
    match serde_json::from_slice::<Value>(o).ok()? {
        Value::Object(m) => Some(m),
        _ => None,
    }
}

fn actuel(i: &Ini, encodeur_json: Option<&[u8]>) -> Actuel {
    let avance = i.valeur("Output", "Mode") == Some("Advanced");
    let (encodeur, video) = if avance {
        let e = i.valeur("AdvOut", "Encoder").unwrap_or("obs_x264");
        let b = objet(encodeur_json).and_then(|m| m.get("bitrate").and_then(Value::as_u64)).map(|b| b as u32);
        (Encodeur::famille(e), b)
    } else {
        let e = i.valeur("SimpleOutput", "StreamEncoder").and_then(Encodeur::famille);
        (e, i.entier("SimpleOutput", "VBitrate").filter(|b| *b > 0).map(|b| b as u32))
    };
    Actuel { avance, encodeur, video_kbps: video, sortie: taille(i, "Video", "OutputCX", "OutputCY"), ips: ips(i) }
}

/// Lit les réglages d'un logiciel dans son dossier de réglages (`...\obs-studio` ou `...\slobs-client`)
pub fn lire(logiciel: Logiciel, racine: &Path) -> Result<Etat, Probleme> {
    let (dossier, profil, nb, version) = match logiciel {
        Logiciel::Obs => {
            let (d, n, nb, v) = profil_obs(racine)?;
            (d, n, nb, v)
        }
        Logiciel::Streamlabs => (racine.to_path_buf(), String::new(), 1, None),
    };
    let basic = lire_ini(&dossier.join("basic.ini"))?.ok_or(Probleme::JamaisOuvert)?;
    let enc = std::fs::read(dossier.join("streamEncoder.json")).ok();
    Ok(Etat {
        logiciel,
        profil,
        service: service::lire(&dossier),
        base: taille(&basic, "Video", "BaseCX", "BaseCY"),
        actuel: actuel(&basic, enc.as_deref()),
        dossier,
        nb_profils: nb,
        version,
    })
}

/// Les nouveaux fichiers : basic.ini, et streamEncoder.json s'il change (`None` : pas touché)
#[derive(Debug, Clone)]
pub struct Nouveau {
    pub basic: Vec<u8>,
    pub encodeur: Option<Vec<u8>>,
    /// Le mode Avancé a été activé (pour l'image clé de Kick)
    pub passe_en_avance: bool,
    /// Les clés de basic.ini changées (valeur d'avant, valeur écrite), pour la copie
    pub cles: Vec<CleIni>,
    /// Les sections de basic.ini ajoutées
    pub sections: Vec<String>,
    /// Les clés de streamEncoder.json changées
    pub cles_json: Vec<CleJson>,
}

/// Pose une valeur et note la clé (pour savoir ensuite ce que Nevermind a changé)
fn mettre(i: &mut Ini, touchees: &mut Vec<(String, String)>, section: &str, cle: &str, valeur: &str) {
    if !touchees.iter().any(|(s, k)| s == section && k == cle) {
        touchees.push((section.to_string(), cle.to_string()));
    }
    i.mettre(section, cle, valeur);
}

/// L'identifiant d'encodeur du mode Avancé pour Streamlabs : les anciens noms, reconnus par toutes ses versions
/// (NVENC : jim_nvenc, avec la couche de compatibilité des versions récentes)
fn avance_streamlabs(e: Encodeur) -> &'static str {
    match e {
        Encodeur::Nvenc => "jim_nvenc",
        Encodeur::Amf => "h264_texture_amf",
        Encodeur::Qsv => "obs_qsv11_v2",
        Encodeur::X264 => "obs_x264",
    }
}

/// JSON écrit comme OBS (sans BOM, retrait de 4 espaces)
pub(crate) fn json_obs(m: &Map<String, Value>) -> Vec<u8> {
    let mut o = Vec::new();
    let f = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut s = serde_json::Serializer::with_formatter(&mut o, f);
    let _ = serde::Serialize::serialize(m, &mut s);
    o
}

/// Le changement lui-même, sans rien écrire : les nouveaux octets des fichiers d'après les anciens. Seules les clés
/// nécessaires changent. Erreur : basic.ini est abîmé.
pub fn preparer(
    logiciel: Logiciel,
    basic: &[u8],
    encodeur_json: Option<&[u8]>,
    plan: &Plan,
    version: Option<Version>,
) -> Result<Nouveau, String> {
    let origine = Ini::lire(basic)?;
    let mut i = origine.clone();
    let mut t = Vec::new();
    let mut avance = i.valeur("Output", "Mode") == Some("Advanced");
    let mut passe = false;
    let e = plan.encodeur;
    let obs = logiciel == Logiciel::Obs;
    // Kick : l'image clé à 2 s ne s'écrit qu'en mode Avancé (le mode Simple la laisse à OBS, qui ne la fixe pas pour
    // un serveur personnalisé). Streamlabs garde son mode.
    if obs && plan.plateforme.image_cle_a_ecrire() && !avance {
        mettre(&mut i, &mut t, "Output", "Mode", "Advanced");
        avance = true;
        passe = true;
        // Les enregistrements restent où ils allaient : le mode Avancé a son propre dossier et son propre format
        // (restés souvent ceux d'origine d'OBS), repris de ceux du mode Simple (« RecFormat » avant OBS 30)
        for (simple, avance_cle) in
            [("FilePath", "RecFilePath"), ("RecFormat2", "RecFormat2"), ("RecFormat", "RecFormat")]
        {
            if let Some(v) = i.valeur("SimpleOutput", simple).map(str::to_string) {
                mettre(&mut i, &mut t, "AdvOut", avance_cle, &v);
            }
        }
    }
    if obs {
        mettre(&mut i, &mut t, "Video", "OutputCX", &plan.sortie.0.to_string());
        mettre(&mut i, &mut t, "Video", "OutputCY", &plan.sortie.1.to_string());
        mettre(&mut i, &mut t, "Video", "FPSType", "0");
        mettre(&mut i, &mut t, "Video", "FPSCommon", &plan.ips.to_string());
    }
    let mut encodeur = None;
    let mut cles_json = Vec::new();
    if avance {
        let avant = i.valeur("AdvOut", "Encoder").unwrap_or("obs_x264").to_string();
        let id = if obs { e.avance(version) } else { avance_streamlabs(e) };
        // (Streamlabs : un encodeur déjà de la bonne famille est gardé tel quel)
        let garder = !obs && Encodeur::famille(&avant) == Some(e);
        let meme_famille = Encodeur::famille(&avant) == Some(e);
        if !garder {
            mettre(&mut i, &mut t, "AdvOut", "Encoder", id);
        }
        let piste = i.entier("AdvOut", "TrackIndex").filter(|t| (1..=6).contains(t)).unwrap_or(1);
        mettre(&mut i, &mut t, "AdvOut", &format!("Track{piste}Bitrate"), &plan.son_kbps.to_string());
        // (la sortie du stream est déjà à la bonne taille : pas de seconde mise à l'échelle)
        if i.entier("AdvOut", "RescaleFilter").is_some_and(|f| f != 0) {
            mettre(&mut i, &mut t, "AdvOut", "RescaleFilter", "0");
        }
        if i.booleen("AdvOut", "Rescale") == Some(true) {
            mettre(&mut i, &mut t, "AdvOut", "Rescale", "false");
        }
        // streamEncoder.json : repris tel quel si l'encodeur ne change pas de famille, sinon vidé (comme OBS quand on
        // change d'encodeur : les réglages d'un autre encodeur n'ont pas de sens pour celui-ci)
        let mut m = if meme_famille { objet(encodeur_json).unwrap_or_default() } else { Map::new() };
        m.insert("rate_control".into(), Value::from("CBR"));
        m.insert("bitrate".into(), Value::from(plan.video_kbps));
        m.insert("keyint_sec".into(), Value::from(plan.image_cle_s));
        let preset = if obs {
            e.preset_avance(version)
        } else {
            match e {
                Encodeur::Nvenc => Some(("preset2", "p5")),
                _ => e.preset_avance(version),
            }
        };
        if let Some((k, v)) = preset {
            m.insert(k.into(), Value::from(v));
        }
        cles_json = crate::sauvegarde::cles_json(&objet(encodeur_json).unwrap_or_default(), &m);
        encodeur = Some(json_obs(&m));
    } else {
        mettre(&mut i, &mut t, "SimpleOutput", "StreamEncoder", e.simple());
        mettre(&mut i, &mut t, "SimpleOutput", "VBitrate", &plan.video_kbps.to_string());
        mettre(&mut i, &mut t, "SimpleOutput", "ABitrate", &plan.son_kbps.to_string());
        if let Some((k, v)) = e.preset_simple() {
            mettre(&mut i, &mut t, "SimpleOutput", k, v);
        }
    }
    let cles = t
        .iter()
        .filter_map(|(s, k)| {
            let avant = origine.valeur(s, k).map(str::to_string);
            let apres = i.valeur(s, k).map(str::to_string);
            (avant != apres).then(|| CleIni { section: s.clone(), cle: k.clone(), avant, apres })
        })
        .collect();
    let deja = origine.sections();
    let mut sections: Vec<String> = i.sections().into_iter().filter(|s| !deja.contains(s)).collect();
    sections.dedup();
    Ok(Nouveau { basic: i.octets(), encodeur, passe_en_avance: passe, cles, sections, cles_json })
}

/// Ce qui a changé, en mots simples (pour la page)
pub fn changements(etat: &Etat, plan: &Plan, nouveau: &Nouveau) -> Vec<String> {
    let mut l = Vec::new();
    let a = &etat.actuel;
    if etat.logiciel == Logiciel::Obs {
        let avant = match (a.sortie, &a.ips) {
            (Some((_, h)), Some(i)) => format!(" (avant : {h}p à {i} images par seconde)"),
            _ => String::new(),
        };
        l.push(format!("Qualité : {}{avant}", plan.qualite()));
    }
    let avant = a.video_kbps.map(|v| format!(" (avant : {} kb/s)", nombre(v))).unwrap_or_default();
    l.push(format!("Débit vidéo : {}{avant}", plan.debit()));
    let avant = match a.encodeur {
        Some(e) if e != plan.encodeur => format!(" (avant : {})", e.par()),
        _ => String::new(),
    };
    l.push(format!("Encodé par {}{avant}", plan.encodeur.par()));
    l.push(format!("Son : {} kb/s", plan.son_kbps));
    if nouveau.encodeur.is_some() {
        l.push("Une image clé toutes les 2 secondes".into());
    }
    if nouveau.passe_en_avance {
        l.push("Sortie d'OBS en mode Avancé (Kick demande une image clé toutes les 2 secondes)".into());
    }
    l
}

/// Fait le changement : les anciennes valeurs notées dans la copie, puis écriture. Rend les nouveaux fichiers écrits.
pub fn appliquer(etat: &Etat, plan: &Plan, copie: &Copie, date: &str) -> Result<Nouveau, String> {
    let fb = etat.dossier.join("basic.ini");
    let fe = etat.dossier.join("streamEncoder.json");
    let basic = std::fs::read(&fb).map_err(|e| format!("basic.ini : {e}"))?;
    let enc = std::fs::read(&fe).ok();
    let n = preparer(etat.logiciel, &basic, enc.as_deref(), plan, etat.version)?;
    let mut modifs = vec![Modif::Ini { chemin: fb.clone(), cles: n.cles.clone(), sections: n.sections.clone() }];
    if let Some(j) = &n.encodeur {
        modifs.push(Modif::Json {
            chemin: fe.clone(),
            avant: enc.clone(),
            apres: j.clone(),
            cles: n.cles_json.clone(),
        });
    }
    let annulation = copie.garder(&modifs, date, &etat.profil)?;
    // (rien n'a pu être écrit : la copie revient comme elle était, « Remettre comme avant » n'apparaît pas pour rien)
    if let Err(e) = ecrire_sur(&fb, &n.basic) {
        let _ = copie.annuler(annulation);
        return Err(format!("basic.ini : {e}"));
    }
    if let Some(j) = &n.encodeur
        && let Err(e) = ecrire_sur(&fe, j)
    {
        // (basic.ini est remis comme il était juste avant : rien n'est à moitié fait)
        let _ = ecrire_sur(&fb, &basic);
        let _ = copie.annuler(annulation);
        return Err(format!("streamEncoder.json : {e}"));
    }
    Ok(n)
}

/// L'image de base pour le calcul : celle d'OBS si elle est écrite, sinon l'écran, sinon 1920 x 1080
pub fn base(etat: Option<&Etat>, ecran: Option<(u32, u32)>) -> (u32, u32) {
    etat.and_then(|e| e.base).or(ecran).unwrap_or((1920, 1080))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calcul::{Carte, Entree, Fabricant, Plateforme, calculer};

    fn plan(p: Plateforme, envoi: u32, nvidia: bool) -> Plan {
        let cartes = if nvidia {
            vec![Carte { nom: "NVIDIA GeForce RTX 3060".into(), fabricant: Fabricant::Nvidia }]
        } else {
            vec![]
        };
        calculer(&Entree {
            envoi_kbps: envoi,
            cartes,
            base: (1920, 1080),
            plateforme: p,
            version: Version::lire("33.0.1"),
        })
    }

    #[test]
    fn mode_simple_seulement_les_cles_utiles() {
        let avant = "\u{feff}[General]\nName=Sans titre\n\n[Video]\nBaseCX=1920\nBaseCY=1080\nOutputCX=1280\nOutputCY=720\n\n[Panels]\nCookieId=X\n\n[SimpleOutput]\nVBitrate=2500\nStreamEncoder=x264\nRecQuality=Stream\n";
        let p = plan(Plateforme::Twitch, 20_000, true);
        let n = preparer(Logiciel::Obs, avant.as_bytes(), None, &p, Version::lire("33.0.1")).unwrap();
        assert!(n.encodeur.is_none() && !n.passe_en_avance);
        let t = String::from_utf8(n.basic).unwrap();
        assert_eq!(
            t,
            "\u{feff}[General]\nName=Sans titre\n\n[Video]\nBaseCX=1920\nBaseCY=1080\nOutputCX=1920\nOutputCY=1080\nFPSType=0\nFPSCommon=60\n\n[Panels]\nCookieId=X\n\n[SimpleOutput]\nVBitrate=6000\nStreamEncoder=nvenc\nRecQuality=Stream\nABitrate=160\nNVENCPreset2=p5\n"
        );
    }

    #[test]
    fn kick_passe_en_avance_avec_l_image_cle() {
        let avant = "\u{feff}[General]\nName=P\n\n[Output]\nMode=Simple\n\n[AdvOut]\nTrackIndex=2\nRescaleFilter=3\nRescaleRes=1280x720\n";
        let p = plan(Plateforme::Kick, 20_000, true);
        let n = preparer(Logiciel::Obs, avant.as_bytes(), None, &p, Version::lire("30.2.3")).unwrap();
        assert!(n.passe_en_avance);
        let t = String::from_utf8(n.basic).unwrap();
        assert!(t.contains("[Output]\nMode=Advanced\n"), "{t}");
        assert!(t.contains("Encoder=jim_nvenc"), "{t}");
        assert!(t.contains("Track2Bitrate=160"), "{t}");
        assert!(t.contains("RescaleFilter=0") && t.contains("RescaleRes=1280x720"), "{t}");
        let j: Value = serde_json::from_slice(&n.encodeur.unwrap()).unwrap();
        assert_eq!(j["keyint_sec"], 2);
        assert_eq!(j["rate_control"], "CBR");
        assert_eq!(j["bitrate"], 8000);
        assert_eq!(j["preset2"], "p5");
        // OBS 31 et plus : le nouvel identifiant de NVENC et la clé « preset »
        let n = preparer(Logiciel::Obs, avant.as_bytes(), None, &p, Version::lire("31.0.0")).unwrap();
        assert!(String::from_utf8(n.basic).unwrap().contains("Encoder=obs_nvenc_h264_tex"));
        let j: Value = serde_json::from_slice(&n.encodeur.unwrap()).unwrap();
        assert_eq!(j["preset"], "p5");
    }

    #[test]
    fn kick_les_enregistrements_restent_dans_le_meme_dossier() {
        // mode Simple : l'utilisateur enregistre dans D:\Vidéos en MKV ; le mode Avancé garde encore les valeurs
        // d'origine d'OBS (comme le basic.ini d'OBS 33)
        let avant = "\u{feff}[Output]\nMode=Simple\n\n[SimpleOutput]\nFilePath=D:\\\\Vidéos\nRecFormat2=mkv\n\n[AdvOut]\nRecFilePath=C:\\\\Users\\\\Joueur\\\\Videos\nRecFormat2=hybrid_mp4\n";
        let p = plan(Plateforme::Kick, 20_000, true);
        let n = preparer(Logiciel::Obs, avant.as_bytes(), None, &p, Version::lire("33.0.1")).unwrap();
        assert!(n.passe_en_avance);
        let i = Ini::lire(&n.basic).unwrap();
        assert_eq!(i.valeur("AdvOut", "RecFilePath"), Some("D:\\\\Vidéos"));
        assert_eq!(i.valeur("AdvOut", "RecFormat2"), Some("mkv"));
        // déjà en mode Avancé : ses réglages d'enregistrement ne sont pas touchés
        let avance = avant.replace("Mode=Simple", "Mode=Advanced");
        let n = preparer(Logiciel::Obs, avance.as_bytes(), None, &p, Version::lire("33.0.1")).unwrap();
        let i = Ini::lire(&n.basic).unwrap();
        assert_eq!(i.valeur("AdvOut", "RecFilePath"), Some("C:\\\\Users\\\\Joueur\\\\Videos"));
        // OBS d'avant la 30 : « RecFormat »
        let ancien = "[SimpleOutput]\nFilePath=E:\\\\Clips\nRecFormat=flv\n";
        let n = preparer(Logiciel::Obs, ancien.as_bytes(), None, &p, Version::lire("28.1.2")).unwrap();
        let i = Ini::lire(&n.basic).unwrap();
        assert_eq!(
            (i.valeur("AdvOut", "RecFilePath"), i.valeur("AdvOut", "RecFormat")),
            (Some("E:\\\\Clips"), Some("flv"))
        );
    }

    #[test]
    fn encodeur_json_garde_ses_cles_ou_repart_de_zero() {
        let avant = "[Output]\nMode=Advanced\n[AdvOut]\nEncoder=obs_nvenc_h264_tex\n";
        let json = br#"{"bitrate": 2500, "multipass": "qres", "tune": "hq", "bf": 2}"#;
        let p = plan(Plateforme::Twitch, 20_000, true);
        let n = preparer(Logiciel::Obs, avant.as_bytes(), Some(json), &p, Version::lire("32.0.0")).unwrap();
        let j: Value = serde_json::from_slice(&n.encodeur.unwrap()).unwrap();
        assert_eq!(
            (j["bitrate"].as_u64(), j["multipass"].as_str(), j["bf"].as_u64()),
            (Some(6000), Some("qres"), Some(2))
        );
        // l'encodeur change de famille (x264 vers NVENC) : les réglages de x264 ne sont pas gardés
        let avant = "[Output]\nMode=Advanced\n[AdvOut]\nEncoder=obs_x264\n";
        let json = br#"{"bitrate": 2500, "tune": "zerolatency", "x264opts": "keyint=60"}"#;
        let n = preparer(Logiciel::Obs, avant.as_bytes(), Some(json), &p, Version::lire("32.0.0")).unwrap();
        let j: Value = serde_json::from_slice(&n.encodeur.unwrap()).unwrap();
        assert!(j.get("tune").is_none() && j.get("x264opts").is_none());
        // un streamEncoder.json abîmé : OBS le lit comme vide, Nevermind aussi
        let avant = "[Output]\nMode=Advanced\n[AdvOut]\nEncoder=obs_nvenc_h264_tex\n";
        let n = preparer(Logiciel::Obs, avant.as_bytes(), Some(b"{abime"), &p, None).unwrap();
        let j: Value = serde_json::from_slice(&n.encodeur.unwrap()).unwrap();
        assert_eq!(j["bitrate"], 6000);
    }

    #[test]
    fn streamlabs_ne_touche_pas_la_video() {
        let avant =
            "\u{feff}[Video]\nOutputCX=1920\nOutputCY=1080\n[SimpleOutput]\nStreamEncoder=x264\nVBitrate=2500\n";
        let p = plan(Plateforme::Twitch, 6_000, true);
        let n = preparer(Logiciel::Streamlabs, avant.as_bytes(), None, &p, None).unwrap();
        let t = String::from_utf8(n.basic).unwrap();
        assert!(t.contains("OutputCX=1920\nOutputCY=1080\n"));
        assert!(t.contains("StreamEncoder=nvenc") && t.contains("VBitrate=4000"));
        assert!(!t.contains("FPSCommon"));
        // mode Avancé : un encodeur déjà NVIDIA est gardé tel quel
        let avant = "[Output]\nMode=Advanced\n[AdvOut]\nEncoder=ffmpeg_nvenc\n";
        let n = preparer(Logiciel::Streamlabs, avant.as_bytes(), None, &plan(Plateforme::Kick, 20_000, true), None)
            .unwrap();
        let t = String::from_utf8(n.basic).unwrap();
        assert!(t.contains("Encoder=ffmpeg_nvenc"));
        let avant = "[Output]\nMode=Advanced\n[AdvOut]\nEncoder=obs_x264\n";
        let n = preparer(Logiciel::Streamlabs, avant.as_bytes(), None, &plan(Plateforme::Twitch, 20_000, true), None)
            .unwrap();
        assert!(String::from_utf8(n.basic).unwrap().contains("Encoder=jim_nvenc"));
        assert!(!n.passe_en_avance);
    }

    #[test]
    fn basic_ini_abime() {
        let p = plan(Plateforme::Twitch, 20_000, true);
        assert!(preparer(Logiciel::Obs, &[0xC3, 0x28, 0x00, 0xFF], None, &p, None).is_err());
    }
}
