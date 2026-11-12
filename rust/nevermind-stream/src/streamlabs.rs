//! Streamlabs Desktop réglé par sa porte officielle, son API (le tube `\\.\pipe\slobs`, voir tube.rs), pendant qu'il est
//! ouvert.
//!
//! Pourquoi l'API et plus ses fichiers (essai réel du 12/11/2026 sur Streamlabs Desktop 1.21.9, workflow
//! streamlabs-reel.yml) : depuis la sortie double (horizontale et verticale), Streamlabs garde sa résolution et ses images
//! par seconde dans le stockage interne de sa fenêtre (`PersistentStatefulService-DualOutputService`, dans
//! `Local Storage\leveldb`), qu'aucun fichier ne permet de changer ; basic.ini n'est plus relu pour la vidéo. Par l'API,
//! Nevermind règle tout (résolution de sortie, images par seconde, encodeur, débit, image clé), Streamlabs le prend tout
//! de suite et le garde, comme si l'utilisateur l'avait changé dans ses Paramètres.
//!
//! Ce qui est lu et changé :
//! - la vidéo (sortie horizontale) : `VideoSettingsService.state.horizontal` (outputWidth, outputHeight, fpsNum,
//!   fpsDen), changée par `VideoSettingsService.setSettings` ; l'image de base (baseWidth, baseHeight) n'est jamais
//!   changée ;
//! - la sortie : le formulaire `SettingsService.fetchSettingsFromObs("Output")` (cette catégorie seulement : jamais
//!   celle du stream, où est la clé), changé valeur par valeur par `SettingsService.setSettingValue`. Le mode
//!   Avancé est demandé d'abord, seul (ses champs n'existent qu'après), pour Kick (image clé de 2 s).
//!
//! Chaque réglage changé est noté avec sa valeur d'avant et celle écrite ([`Reglage`]) : c'est ce que garde la copie
//! (sauvegarde.rs) et ce que « Remettre comme avant » remet, seulement si la valeur est encore celle de Nevermind.

use crate::calcul::{Encodeur, Plan};
use crate::sauvegarde::{Copie, Modif};
use serde_json::{Map, Value, json};

/// La porte vers l'API de Streamlabs (le vrai tube, ou une fausse porte dans les tests)
pub trait Porte {
    fn appeler(&self, ressource: &str, methode: &str, args: &[Value]) -> Result<Value, String>;
}

/// Un réglage de Streamlabs, désigné pour l'API : `Video/<clé>` (sortie horizontale) ou `Output/<sous-partie>/<nom>`
#[derive(Debug, Clone, PartialEq)]
pub struct Reglage {
    pub cible: String,
    /// La valeur d'avant (`Null` : absente)
    pub avant: Value,
    /// La valeur écrite par Nevermind
    pub apres: Value,
}

/// Ce que Streamlabs dit de lui, lu par l'API
#[derive(Debug, Clone)]
pub struct Lu {
    /// Un stream, un enregistrement ou le replay est en cours (les réglages de sortie sont alors bloqués)
    pub occupe: bool,
    /// La sortie horizontale (outputWidth, fpsNum...)
    pub video: Map<String, Value>,
    /// Le formulaire Sortie, tel que Streamlabs le donne
    pub sortie: Value,
}

impl Lu {
    /// L'image de base (le canevas)
    pub fn base(&self) -> Option<(u32, u32)> {
        let w = self.video.get("baseWidth")?.as_u64()?;
        let h = self.video.get("baseHeight")?.as_u64()?;
        (w > 0 && h > 0).then_some((w as u32, h as u32))
    }
    pub fn avance(&self) -> bool {
        valeur(&self.sortie, "Untitled", "Mode").and_then(Value::as_str) == Some("Advanced")
    }
    /// Le débit vidéo actuel (kb/s)
    pub fn debit(&self) -> Option<u32> {
        let (s, n) = if self.avance() { ("Streaming", "bitrate") } else { ("Streaming", "VBitrate") };
        valeur(&self.sortie, s, n).and_then(nombre).map(|v| v as u32)
    }
    pub fn encodeur(&self) -> Option<Encodeur> {
        let n = if self.avance() { "Encoder" } else { "StreamEncoder" };
        valeur(&self.sortie, "Streaming", n).and_then(Value::as_str).and_then(Encodeur::famille)
    }
    pub fn sortie_video(&self) -> Option<(u32, u32)> {
        let w = self.video.get("outputWidth")?.as_u64()?;
        let h = self.video.get("outputHeight")?.as_u64()?;
        Some((w as u32, h as u32))
    }
    pub fn ips(&self) -> Option<String> {
        let n = self.video.get("fpsNum")?.as_u64()?;
        let d = self.video.get("fpsDen")?.as_u64().filter(|d| *d > 0)?;
        Some(if n % d == 0 { (n / d).to_string() } else { format!("{:.2}", n as f64 / d as f64) })
    }
}

/// Un nombre écrit en nombre ou en texte (le formulaire mélange les deux)
fn nombre(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
        .or_else(|| v.as_str()?.trim().parse().ok())
}

/// La valeur d'un champ du formulaire, dans sa sous-partie
pub fn valeur<'a>(form: &'a Value, sous: &str, nom: &str) -> Option<&'a Value> {
    champ(form, sous, nom).and_then(|c| c.get("value"))
}

fn champ<'a>(form: &'a Value, sous: &str, nom: &str) -> Option<&'a Map<String, Value>> {
    form.as_array()?
        .iter()
        .filter(|g| g["nameSubCategory"].as_str() == Some(sous))
        .flat_map(|g| g["parameters"].as_array().into_iter().flatten())
        .filter_map(Value::as_object)
        .find(|p| p.get("name").and_then(Value::as_str) == Some(nom))
}

/// La même valeur, qu'elle soit écrite en nombre ou en texte (« 3000 » et 3000)
pub fn meme(a: &Value, b: &Value) -> bool {
    a == b || matches!((nombre(a), nombre(b)), (Some(x), Some(y)) if x == y && !a.is_boolean() && !b.is_boolean())
}

/// Lit Streamlabs par son API
pub fn lire(p: &dyn Porte) -> Result<Lu, String> {
    let m = p.appeler("StreamingService", "getModel", &[])?;
    let occupe = ["streamingStatus", "recordingStatus", "replayBufferStatus"]
        .iter()
        .any(|k| m.get(*k).and_then(Value::as_str).is_some_and(|s| s != "offline"));
    let v = p.appeler("VideoSettingsService", "state", &[])?;
    let video = v.get("horizontal").and_then(Value::as_object).cloned().ok_or("réglages vidéo absents")?;
    let sortie = formulaire_sortie(p)?;
    Ok(Lu { occupe, video, sortie })
}

/// Le formulaire Sortie, relu dans le moteur de Streamlabs (seulement cette catégorie : jamais celle du stream, où est
/// la clé)
fn formulaire_sortie(p: &dyn Porte) -> Result<Value, String> {
    let r = p.appeler("SettingsService", "fetchSettingsFromObs", &[json!("Output")])?;
    let f = r.get("formData").cloned().unwrap_or(r);
    if !f.is_array() {
        return Err("formulaire Sortie illisible".into());
    }
    Ok(f)
}

/// Le préréglage de vitesse de l'encodeur dans le formulaire (champ, valeur), s'il existe pour cet encodeur
fn preset(lu: &Lu, e: Encodeur, avance: bool) -> Option<(&'static str, &'static str)> {
    let candidats: &[(&str, &str)] = match (e, avance) {
        (Encodeur::Nvenc, false) => &[("NVENCPreset2", "p5")],
        (Encodeur::Nvenc, true) => &[("preset2", "p5"), ("preset", "p5")],
        (Encodeur::Amf, false) => &[("AMDPreset", "quality")],
        (Encodeur::Amf, true) => &[("preset", "quality")],
        (Encodeur::Qsv, false) => &[("QSVPreset", "balanced")],
        (Encodeur::Qsv, true) => &[("target_usage", "balanced")],
        (Encodeur::X264, _) => &[("Preset", "veryfast"), ("preset", "veryfast")],
    };
    candidats.iter().copied().find(|(n, _)| champ(&lu.sortie, "Streaming", n).is_some())
}

/// L'identifiant d'encodeur à écrire, pris dans la liste que Streamlabs propose (son champ « options ») : celui de la
/// famille voulue. `None` : Streamlabs ne le propose pas sur ce PC.
fn identifiant(lu: &Lu, nom: &str, e: Encodeur) -> Option<String> {
    let c = champ(&lu.sortie, "Streaming", nom)?;
    if let Some(v) = c.get("value").and_then(Value::as_str)
        && Encodeur::famille(v) == Some(e)
    {
        return Some(v.to_string());
    }
    let options = c.get("options").and_then(Value::as_array)?;
    options
        .iter()
        .filter_map(|o| o.get("value").and_then(Value::as_str))
        .find(|v| Encodeur::famille(v) == Some(e))
        .map(str::to_string)
}

/// Ce que Nevermind va changer, sans rien écrire. `mode` : le mode Avancé est à demander d'abord (Kick).
#[derive(Debug, Clone, Default)]
pub struct Prevu {
    pub passe_en_avance: bool,
    pub reglages: Vec<Reglage>,
}

/// Les réglages de sortie à changer dans ce formulaire (déjà dans le bon mode)
fn sortie(lu: &Lu, plan: &Plan, l: &mut Vec<Reglage>) {
    let avance = lu.avance();
    let mut pose = |sous: &str, nom: &str, v: Value| {
        if let Some(c) = champ(&lu.sortie, sous, nom) {
            let avant = c.get("value").cloned().unwrap_or(Value::Null);
            if !meme(&avant, &v) {
                l.push(Reglage { cible: format!("Output/{sous}/{nom}"), avant, apres: v });
            }
        }
    };
    let e = plan.encodeur;
    let nom_enc = if avance { "Encoder" } else { "StreamEncoder" };
    if let Some(id) = identifiant(lu, nom_enc, e) {
        pose("Streaming", nom_enc, json!(id));
    }
    if avance {
        pose("Streaming", "rate_control", json!("CBR"));
        pose("Streaming", "bitrate", json!(plan.video_kbps));
        pose("Streaming", "keyint_sec", json!(plan.image_cle_s));
        let piste = valeur(&lu.sortie, "Streaming", "TrackIndex").and_then(nombre).filter(|t| (1..=6).contains(t));
        let piste = piste.unwrap_or(1);
        let sous = format!("Audio - Track {piste}");
        pose(&sous, &format!("Track{piste}Bitrate"), json!(plan.son_kbps));
    } else {
        pose("Streaming", "VBitrate", json!(plan.video_kbps));
        pose("Streaming", "ABitrate", json!(plan.son_kbps));
    }
    // (le préréglage seulement si l'encodeur choisi est déjà celui-là : ses champs n'existent qu'après le changement)
    if lu.encodeur() == Some(e)
        && let Some((n, v)) = preset(lu, e, avance)
    {
        pose("Streaming", n, json!(v));
    }
}

/// La largeur de sortie que Streamlabs garde : un multiple de 4 (essai réel du 12/11/2026 : 854 écrit devient 852 quand
/// il se relance, comme dans sa propre liste de résolutions)
pub fn largeur(l: u32) -> u32 {
    (l & !3).max(4)
}

/// La vidéo à changer : résolution de sortie et images par seconde (l'image de base n'est pas touchée)
fn video(lu: &Lu, plan: &Plan, l: &mut Vec<Reglage>) {
    for (k, v) in
        [("outputWidth", largeur(plan.sortie.0)), ("outputHeight", plan.sortie.1), ("fpsNum", plan.ips), ("fpsDen", 1)]
    {
        let avant = lu.video.get(k).cloned().unwrap_or(Value::Null);
        if !meme(&avant, &json!(v)) {
            l.push(Reglage { cible: format!("Video/{k}"), avant, apres: json!(v) });
        }
    }
}

/// Ce qui sera changé (sur ce qui est lu maintenant). Kick en mode Simple : d'abord seulement le passage en mode
/// Avancé (avec la vidéo) ; la sortie se règle au tour suivant, sur le formulaire du mode Avancé.
pub fn prevoir(lu: &Lu, plan: &Plan) -> Prevu {
    let mut l = Vec::new();
    video(lu, plan, &mut l);
    let passe = plan.plateforme.image_cle_a_ecrire() && !lu.avance();
    if passe {
        let avant = valeur(&lu.sortie, "Untitled", "Mode").cloned().unwrap_or(Value::Null);
        l.push(Reglage { cible: "Output/Untitled/Mode".into(), avant, apres: json!("Advanced") });
    } else {
        sortie(lu, plan, &mut l);
    }
    Prevu { passe_en_avance: passe, reglages: l }
}

/// Ce que Nevermind a fait
#[derive(Debug, Clone, Default)]
pub struct Fait {
    pub reglages: Vec<Reglage>,
    pub passe_en_avance: bool,
}

/// Ajoute des réglages à ceux déjà faits : la valeur d'avant reste la première notée
fn fusionner(fait: &mut Vec<Reglage>, l: &[Reglage]) {
    for r in l {
        match fait.iter_mut().find(|x| x.cible == r.cible) {
            Some(x) => x.apres = r.apres.clone(),
            None => fait.push(r.clone()),
        }
    }
}

/// Règle Streamlabs par son API : chaque tour relit Streamlabs, note dans la copie ce qui va changer (AVANT de
/// l'écrire), l'écrit ; jusqu'à ce qu'il n'y ait plus rien à changer (au plus 4 tours : le mode Avancé, puis la sortie,
/// puis le préréglage du nouvel encodeur, dont le champ n'existe qu'une fois l'encodeur changé). Puis tout est relu :
/// si un réglage n'a pas pris, tout est remis comme avant (et la copie aussi) et c'est une erreur.
pub fn appliquer(p: &dyn Porte, lu: &Lu, plan: &Plan, copie: &Copie, date: &str) -> Result<Fait, String> {
    let mut fait = Fait::default();
    let mut premiere = None;
    // (les enregistrements restent où ils allaient quand le mode Avancé est pris : son dossier est à part)
    let dossier_simple = (!lu.avance()).then(|| valeur(&lu.sortie, "Recording", "FilePath").cloned()).flatten();
    let mut lu = lu.clone();
    let r = (|| {
        for _ in 0..4 {
            let mut prevu = prevoir(&lu, plan);
            if lu.avance()
                && fait.passe_en_avance
                && let Some(d) = &dossier_simple
                && let Some(c) = champ(&lu.sortie, "Recording", "RecFilePath")
            {
                let avant = c.get("value").cloned().unwrap_or(Value::Null);
                if !meme(&avant, d) && !fait.reglages.iter().any(|r| r.cible == "Output/Recording/RecFilePath") {
                    prevu.reglages.push(Reglage {
                        cible: "Output/Recording/RecFilePath".into(),
                        avant,
                        apres: d.clone(),
                    });
                }
            }
            if prevu.reglages.is_empty() {
                return Ok(());
            }
            fait.passe_en_avance |= prevu.passe_en_avance;
            let a = copie.garder(&[Modif::Api { cles: prevu.reglages.clone() }], date, "")?;
            premiere.get_or_insert(a);
            fusionner(&mut fait.reglages, &prevu.reglages);
            ecrire(p, &prevu.reglages, false)?;
            lu = lire(p)?;
        }
        Ok(())
    })()
    .and_then(|()| {
        let rate = pas_pris(p, &fait.reglages)?;
        if rate.is_empty() { Ok(()) } else { Err(format!("pas pris par Streamlabs : {}", rate.join(", "))) }
    });
    if let Err(e) = r {
        // (rien n'est à moitié fait : Streamlabs et la copie reviennent comme avant)
        let _ = ecrire(p, &fait.reglages, true);
        if let Some(a) = premiere {
            let _ = copie.annuler(a);
        }
        return Err(e);
    }
    Ok(fait)
}

/// Ce qui a changé, en mots simples (pour la page)
pub fn changements(lu: &Lu, plan: &Plan, fait: &Fait) -> Vec<String> {
    let mut l = Vec::new();
    let avant = match (lu.sortie_video(), lu.ips()) {
        (Some((_, h)), Some(i)) => format!(" (avant : {h}p à {i} images par seconde)"),
        _ => String::new(),
    };
    l.push(format!("Qualité : {}{avant}", plan.qualite()));
    let avant = lu.debit().map(|v| format!(" (avant : {} kb/s)", crate::calcul::nombre(v))).unwrap_or_default();
    l.push(format!("Débit vidéo : {}{avant}", plan.debit()));
    let avant = match lu.encodeur() {
        Some(e) if e != plan.encodeur => format!(" (avant : {})", e.par()),
        _ => String::new(),
    };
    l.push(format!("Encodé par {}{avant}", plan.encodeur.par()));
    l.push(format!("Son : {} kb/s", plan.son_kbps));
    if fait.reglages.iter().any(|r| r.cible == "Output/Streaming/keyint_sec") {
        l.push("Une image clé toutes les 2 secondes".into());
    }
    if fait.passe_en_avance {
        l.push("Sortie de Streamlabs en mode Avancé (Kick demande une image clé toutes les 2 secondes)".into());
    }
    l
}

/// Écrit des réglages par l'API (valeur `apres`, ou `avant` si `retour`) : la vidéo d'un coup, puis le formulaire Sortie
/// d'un coup
pub fn ecrire(p: &dyn Porte, reglages: &[Reglage], retour: bool) -> Result<(), String> {
    let val = |r: &Reglage| if retour { r.avant.clone() } else { r.apres.clone() };
    let mut video = Map::new();
    for r in reglages {
        if let Some(k) = r.cible.strip_prefix("Video/") {
            let v = val(r);
            if !v.is_null() {
                video.insert(k.to_string(), v);
            }
        }
    }
    if !video.is_empty() {
        p.appeler("VideoSettingsService", "setSettings", &[Value::Object(video), json!("horizontal")])?;
    }
    // (le passage en mode Avancé d'abord, seul : ses champs n'existent qu'après ; le retour au mode Simple en dernier,
    // une fois les champs du mode Avancé remis)
    let (mode, mut autres): (Vec<&Reglage>, Vec<&Reglage>) =
        reglages.iter().filter(|r| r.cible.starts_with("Output/")).partition(|r| r.cible == "Output/Untitled/Mode");
    let vers_avance = mode.iter().any(|r| val(r).as_str() == Some("Advanced"));
    // (le préréglage avant l'encodeur : ses champs dépendent de l'encodeur choisi ; à la remise, celui de l'encodeur de
    // Nevermind est remis avant que l'encodeur d'avant revienne)
    autres.sort_by_key(|r| {
        let nom = r.cible.rsplit('/').next().unwrap_or_default();
        if nom.to_ascii_lowercase().contains("preset") || nom == "target_usage" {
            0
        } else if nom == "Encoder" || nom == "StreamEncoder" {
            2
        } else {
            1
        }
    });
    let groupes = if vers_avance { [mode, autres] } else { [autres, mode] };
    // (chaque valeur par `setSettingValue`, comme Streamlabs le fait lui-même : il relit son formulaire, change ce
    // champ, et l'enregistre ; les noms des champs de la Sortie sont uniques, ceux de l'enregistrement commencent par
    // « Rec »)
    for r in groupes.into_iter().flatten() {
        let mut c = r.cible["Output/".len()..].splitn(2, '/');
        let (Some(_), Some(nom)) = (c.next(), c.next()) else { continue };
        p.appeler("SettingsService", "setSettingValue", &[json!("Output"), json!(nom), val(r)])?;
    }
    Ok(())
}

/// La valeur actuelle d'un réglage
pub fn actuelle(lu: &Lu, cible: &str) -> Value {
    if let Some(k) = cible.strip_prefix("Video/") {
        return lu.video.get(k).cloned().unwrap_or(Value::Null);
    }
    let Some(r) = cible.strip_prefix("Output/") else { return Value::Null };
    let mut c = r.splitn(2, '/');
    match (c.next(), c.next()) {
        (Some(s), Some(n)) => valeur(&lu.sortie, s, n).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// Les réglages qui n'ont pas pris (relus après l'écriture)
pub fn pas_pris(p: &dyn Porte, reglages: &[Reglage]) -> Result<Vec<String>, String> {
    let lu = lire(p)?;
    // (un champ du mode Avancé n'existe plus dans le formulaire une fois revenu au mode Simple : il n'est pas compté)
    Ok(reglages
        .iter()
        .filter(|r| existe(&lu, &r.cible) && !meme(&actuelle(&lu, &r.cible), &r.apres))
        .map(|r| r.cible.clone())
        .collect())
}

/// Le réglage existe dans ce qui est lu
fn existe(lu: &Lu, cible: &str) -> bool {
    if cible.starts_with("Video/") {
        return true;
    }
    let mut c = cible.trim_start_matches("Output/").splitn(2, '/');
    matches!((c.next(), c.next()), (Some(s), Some(n)) if champ(&lu.sortie, s, n).is_some())
}
