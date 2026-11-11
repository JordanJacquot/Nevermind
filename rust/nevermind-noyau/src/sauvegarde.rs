//! La sauvegarde des réglages d'origine (sauvegarde.json, modules\donnees.ps1) : `Import-Backup`, `Save-Backup`,
//! `Save-Original`, `Set-Reg`, `Clear-Reg`, `Get-TexteProblemeSauvegarde`, et les points ignorés (ignores.json).
//!
//! La sauvegarde ne dépend pas d'un seul fichier (issue #3) : sauvegarde.json, sa copie de secours (.bak, la version
//! d'avant), et les valeurs d'origine gardées dans le registre (réglages du PC : par l'Aide, dans HKLM ; réglages de
//! l'utilisateur : par Nevermind, dans HKCU). Un fichier abîmé n'est jamais remplacé en silence par une sauvegarde
//! vide : il est mis de côté, et `probleme` le dit (« Tout restaurer », désinstallation).
//!
//! Partagée par tous les blocs (un seul `$script:Backup`) : elle vit dans le `Contexte`.

use crate::Contexte;
use crate::aide::{texte_ps, vrai};
use crate::contexte::verrou;
use crate::moment;
use crate::registre;
use serde_json::{Map, Value, json};
use std::path::PathBuf;

/// `$script:Backup`
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sauvegarde {
    /// « chemin|nom » : `{ Path, Name, Existed, Value, Kind }`
    pub registre: Map<String, Value>,
    pub plan: Option<String>,
    pub plan_cree: Option<String>,
    pub curseur: Option<String>,
    pub dns: Map<String, Value>,
    pub ecrans: Map<String, Value>,
    /// Réglages déjà remis comme avant par « Tout restaurer » alors que d'autres restent à remettre
    pub oublies: Vec<String>,
    /// `$script:BackupProbleme` : « abimee », « absente », ou rien
    pub probleme: Option<String>,
}

impl Sauvegarde {
    /// Le contenu de sauvegarde.json (comme `$script:Backup | ConvertTo-Json -Depth 6`)
    pub fn en_json(&self) -> Value {
        let opt = |o: &Option<String>| o.clone().map_or(Value::Null, Value::String);
        json!({
            "Registry": self.registre,
            "PowerScheme": opt(&self.plan),
            "CreatedScheme": opt(&self.plan_cree),
            "Overlay": opt(&self.curseur),
            "Dns": self.dns,
            "Displays": self.ecrans,
            "Oublies": self.oublies,
        })
    }
}

/// Une entrée de la sauvegarde du registre (`@{ Path; Name; Existed; Value; Kind }`)
pub fn entree(chemin: &str, nom: &str, existait: bool, valeur: Value, kind: Value) -> Value {
    json!({ "Path": chemin, "Name": nom, "Existed": existait, "Value": valeur, "Kind": kind })
}

fn int_ps(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().unwrap_or_else(|| n.as_f64().map_or(0, |f| f.round() as i64)),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        Value::Bool(b) => i64::from(*b),
        _ => 0,
    }
}

/// La sauvegarde écrite dans un fichier (`Import-Backup`, la partie fichier), comme Nevermind la lit
pub fn depuis_json(j: &Map<String, Value>) -> Sauvegarde {
    let mut s = Sauvegarde::default();
    if let Some(Value::Object(r)) = j.get("Registry") {
        for (k, v) in r {
            s.registre.insert(
                k.clone(),
                entree(
                    &texte_ps(&v["Path"]),
                    &texte_ps(&v["Name"]),
                    vrai(&v["Existed"]),
                    v["Value"].clone(),
                    v["Kind"].clone(),
                ),
            );
        }
    }
    let texte = |n: &str| j.get(n).filter(|v| vrai(v)).map(texte_ps);
    s.plan = texte("PowerScheme");
    s.plan_cree = texte("CreatedScheme");
    if let Some(Value::Object(d)) = j.get("Dns") {
        for (k, v) in d {
            s.dns.insert(k.clone(), json!(texte_ps(v)));
        }
    }
    if let Some(Value::Object(d)) = j.get("Displays") {
        for (k, v) in d {
            s.ecrans.insert(k.clone(), json!(int_ps(v)));
        }
    }
    s.curseur = texte("Overlay");
    if let Some(o) = j.get("Oublies") {
        let l: Vec<Value> = match o {
            Value::Array(a) => a.clone(),
            Value::Null => Vec::new(),
            autre => vec![autre.clone()],
        };
        s.oublies = l.iter().filter(|x| vrai(x)).map(texte_ps).collect();
    }
    s
}

/// Une sauvegarde donnée en JSON (celle de Ton PC)
fn depuis_valeur(v: &Value) -> Sauvegarde {
    match v {
        Value::Object(m) => depuis_json(m),
        _ => Sauvegarde::default(),
    }
}

fn fusionner_carte(r: &mut Map<String, Value>, base: &Map<String, Value>, voulue: &Map<String, Value>) {
    // (ajouté par le geste : seulement si rien n'est déjà noté, la plus ancienne valeur d'origine gagne)
    for (k, v) in voulue {
        if !base.contains_key(k) && !r.contains_key(k) {
            r.insert(k.clone(), v.clone());
        }
    }
    // (retiré par le geste : « Tout restaurer » l'a remis comme avant)
    for k in base.keys() {
        if !voulue.contains_key(k) {
            r.remove(k);
        }
    }
}

fn fusionner_texte(r: &mut Option<String>, base: &Option<String>, voulue: &Option<String>) {
    if base == voulue {
        return;
    }
    match voulue {
        Some(v) => {
            if r.is_none() {
                *r = Some(v.clone());
            }
        }
        None => {
            if r == base {
                *r = None;
            }
        }
    }
}

/// La fusion d'un seul écrivain (lot A, étape 2) : sur ce qui est noté (`disque`), seulement ce que le geste a changé
/// depuis sa copie (`base` devenue `voulue`). Une valeur d'origine déjà notée n'est jamais remplacée (la plus ancienne
/// gagne) ; ce que le geste a retiré (« Tout restaurer ») est retiré ; tout le reste (noté par un autre geste) reste.
pub fn fusionner(disque: &Sauvegarde, base: &Sauvegarde, voulue: &Sauvegarde) -> Sauvegarde {
    let mut r = disque.clone();
    fusionner_carte(&mut r.registre, &base.registre, &voulue.registre);
    fusionner_carte(&mut r.dns, &base.dns, &voulue.dns);
    fusionner_carte(&mut r.ecrans, &base.ecrans, &voulue.ecrans);
    fusionner_texte(&mut r.plan, &base.plan, &voulue.plan);
    fusionner_texte(&mut r.plan_cree, &base.plan_cree, &voulue.plan_cree);
    fusionner_texte(&mut r.curseur, &base.curseur, &voulue.curseur);
    for o in &voulue.oublies {
        if !base.oublies.contains(o) && !r.oublies.contains(o) {
            r.oublies.push(o.clone());
        }
    }
    r.oublies.retain(|o| !(base.oublies.contains(o) && !voulue.oublies.contains(o)));
    r
}

/// Un nom de fichier temporaire propre à chaque écriture (jamais partagé entre deux écrivains)
fn tmp_unique(fichier: &std::path::Path) -> PathBuf {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    crate::fichiers::a_cote(fichier, &format!(".{}-{n}.tmp", std::process::id()))
}

/// Un nom de fichier temporaire laissé par une écriture de la sauvegarde : `sauvegarde.json.<pid>-<n>.tmp` (voir
/// [`tmp_unique`]), sans tenir compte des majuscules
fn est_tmp_de_sauvegarde(nom: &str) -> bool {
    let n = nom.to_ascii_lowercase();
    let Some(milieu) = n.strip_prefix("sauvegarde.json.").and_then(|r| r.strip_suffix(".tmp")) else { return false };
    let Some((pid, rang)) = milieu.split_once('-') else { return false };
    let chiffres = |s: &str| !s.is_empty() && s.bytes().all(|o| o.is_ascii_digit());
    chiffres(pid) && chiffres(rang)
}

/// Une écriture de la sauvegarde dure quelques millisecondes : un fichier temporaire plus récent que cela peut être
/// celui d'une écriture en cours (jamais effacé)
const TMP_RESTE_APRES: std::time::Duration = std::time::Duration::from_secs(120);

/// Lot K (audit3 du lot A, défaut 6) : efface les `sauvegarde.json.<pid>-<n>.tmp` laissés dans `dossier` par un arrêt
/// brutal (Nevermind arrêté net entre l'écriture du fichier temporaire et le remplacement). Seulement les fichiers
/// ordinaires à ce nom exact, vieux de plus de [`TMP_RESTE_APRES`] : jamais un dossier, un lien ou un point
/// d'analyse (le type est lu sur l'entrée elle-même, rien n'est suivi ; un lien serait d'ailleurs retiré seul), jamais
/// l'écriture en cours d'un autre fil. Rend le nombre de fichiers effacés.
fn effacer_tmp_restes(dossier: &std::path::Path) -> usize {
    #[cfg(windows)]
    use std::os::windows::fs::MetadataExt;
    let Ok(l) = std::fs::read_dir(dossier) else { return 0 };
    let mut n = 0;
    for e in l.flatten() {
        if !est_tmp_de_sauvegarde(&e.file_name().to_string_lossy()) {
            continue;
        }
        // (le type et l'heure de l'entrée elle-même, sans suivre de lien)
        let Ok(m) = std::fs::symlink_metadata(e.path()) else { continue };
        if !m.file_type().is_file() {
            continue;
        }
        #[cfg(windows)]
        if m.file_attributes() & 0x400 != 0 {
            continue; // (FILE_ATTRIBUTE_REPARSE_POINT)
        }
        let vieux = m.modified().ok().and_then(|t| t.elapsed().ok()).is_some_and(|a| a >= TMP_RESTE_APRES);
        if vieux && std::fs::remove_file(e.path()).is_ok() {
            n += 1;
        }
    }
    n
}

/// `Read-FichierSauvegarde` : `None` s'il n'existe pas ; une erreur s'il est abîmé ou n'a pas « Registry »
fn lire_fichier(f: &std::path::Path) -> Result<Option<Map<String, Value>>, String> {
    if !f.exists() {
        return Ok(None);
    }
    let o = std::fs::read(f).map_err(|e| e.to_string())?;
    lire_octets(&o).map(Some)
}

/// Le contenu d'un fichier de sauvegarde : une erreur s'il est abîmé ou n'a pas « Registry »
fn lire_octets(o: &[u8]) -> Result<Map<String, Value>, String> {
    let t = suite_commun::json_sur::texte_du_fichier(o);
    let j: Value = serde_json::from_str(t.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    match j {
        Value::Object(m) if m.contains_key("Registry") => Ok(m),
        _ => Err("contenu inattendu".into()),
    }
}

impl Contexte {
    /// Le fichier de la sauvegarde
    pub fn fichier_sauvegarde(&self) -> PathBuf {
        self.donnees.join("sauvegarde.json")
    }

    /// `Import-Backup` : relit la sauvegarde (fichier, copie de secours, valeurs d'origine du registre). Sans disque
    /// ([`Contexte::sans_disque`]) : aucun fichier n'est lu, seules les valeurs d'origine du registre sont reprises.
    pub fn importer_sauvegarde(&self) {
        if self.sans_disque {
            self.importer_sauvegarde_de(Ok(None), None, false);
            return;
        }
        // (au démarrage, la première lecture : les fichiers temporaires laissés par un arrêt brutal ; lot K)
        let restes = effacer_tmp_restes(&self.donnees);
        if restes > 0 {
            self.journal(&format!("Sauvegarde : {restes} fichier(s) temporaire(s) laissé(s) par un arrêt brutal, effacé(s)"));
        }
        let fichier = self.fichier_sauvegarde();
        let principal = lire_fichier(&fichier);
        let secours = if matches!(principal, Ok(Some(_))) {
            None
        } else {
            lire_fichier(&crate::fichiers::a_cote(&fichier, ".bak")).ok().flatten()
        };
        self.importer_sauvegarde_de(principal, secours, true);
    }

    /// `Import-Backup` sur des fichiers lus ailleurs (lot C : le désinstalleur en administrateur les lit lui-même, sans
    /// suivre de lien) : `principal`, le contenu de sauvegarde.json (`Ok(None)` s'il n'existe pas, une erreur s'il n'a
    /// pas pu être lu), `secours` celui de sa copie de secours. Rien n'est lu ni écrit ici : un fichier abîmé n'est pas
    /// mis de côté, il est seulement dit (« abimee »).
    pub fn importer_sauvegarde_lue(&self, principal: Result<Option<Vec<u8>>, String>, secours: Option<Vec<u8>>) {
        let principal = principal.and_then(|o| o.map(|o| lire_octets(&o)).transpose());
        let secours = secours.and_then(|o| lire_octets(&o).ok());
        self.importer_sauvegarde_de(principal, secours, false);
    }

    /// `Import-Backup`, une fois les fichiers lus. `mettre_de_cote` : une sauvegarde abîmée est renommée à côté
    /// (jamais sans disque, jamais en administrateur).
    fn importer_sauvegarde_de(
        &self,
        principal: Result<Option<Map<String, Value>>, String>,
        secours: Option<Map<String, Value>>,
        mettre_de_cote: bool,
    ) {
        let mut s = Sauvegarde::default();
        let fichier = self.fichier_sauvegarde();
        let mut notes: Vec<String> = Vec::new();
        let mut j = match principal {
            Ok(j) => j,
            Err(e) => {
                s.probleme = Some("abimee".into());
                if mettre_de_cote && !self.sans_disque {
                    notes.push(format!("Sauvegarde abîmée ({e}) : mise de côté"));
                    let dest =
                        self.donnees.join(format!("sauvegarde-abimee-{}.json", moment::compacte(&self.maintenant())));
                    let _ = std::fs::rename(&fichier, dest);
                } else {
                    notes.push(format!("Sauvegarde abîmée ({e})"));
                }
                None
            }
        };
        if j.is_none()
            && let Some(b) = secours
        {
            notes.push("Sauvegarde reprise de sa copie de secours".into());
            j = Some(b);
        }
        if let Some(j) = &j {
            let probleme = s.probleme.take();
            s = depuis_json(j);
            s.probleme = probleme;
        }
        // Les valeurs d'origine du registre complètent ce que le fichier n'a plus (fichier effacé, abîmé, ou plus ancien)
        let mut connues: std::collections::HashSet<String> = s
            .registre
            .values()
            .map(|e| format!("{}|{}", registre::cle_normale(&texte_ps(&e["Path"])), texte_ps(&e["Name"])))
            .collect();
        connues.extend(s.oublies.iter().cloned());
        let mut reprises = 0;
        for o in self.origines_registre() {
            if !vrai(&o) || !vrai(&o["Path"]) || !vrai(&o["Name"]) {
                continue;
            }
            let (p, n) = (texte_ps(&o["Path"]), texte_ps(&o["Name"]));
            let cle = format!("{}|{n}", registre::cle_normale(&p));
            if connues.contains(&cle) {
                continue;
            }
            s.registre
                .insert(format!("{p}|{n}"), entree(&p, &n, vrai(&o["Existed"]), o["Value"].clone(), o["Kind"].clone()));
            connues.insert(cle);
            reprises += 1;
        }
        if reprises > 0 {
            notes.push(format!("Sauvegarde : {reprises} valeur(s) d'origine reprise(s) du registre"));
        }
        // Plus rien alors que l'historique montre des changements pas annulés : la sauvegarde a disparu
        if s.probleme.is_none() && s.registre.is_empty() {
            let h = self.historique();
            let reste = h.iter().any(|e| {
                vrai(e)
                    && !vrai(&e["Annule"])
                    && e["Log"].as_array().is_some_and(|l| l.iter().any(|x| x["Type"].as_str() == Some("reg")))
            });
            if reste {
                s.probleme = Some("absente".into());
            }
        }
        *verrou(&self.sauvegarde) = Some(s);
        for n in notes {
            self.journal(&n);
        }
    }

    /// `$script:Backup` (importée au premier appel)
    pub fn sauvegarde(&self) -> Sauvegarde {
        if verrou(&self.sauvegarde).is_none() {
            self.importer_sauvegarde();
        }
        verrou(&self.sauvegarde).clone().unwrap_or_default()
    }

    /// Change la sauvegarde en mémoire (sans l'écrire : voir `enregistrer_sauvegarde`)
    pub fn changer_sauvegarde<R>(&self, f: impl FnOnce(&mut Sauvegarde) -> R) -> R {
        if verrou(&self.sauvegarde).is_none() {
            self.importer_sauvegarde();
        }
        let mut g = verrou(&self.sauvegarde);
        f(g.get_or_insert_with(Sauvegarde::default))
    }

    /// `Save-Backup` : ce que la sauvegarde en mémoire a de plus que sauvegarde.json y est ajouté (jamais un retrait,
    /// jamais une valeur d'origine remplacée), par l'écrivain unique ([`Contexte::ecrire_sauvegarde_fusion`])
    pub fn enregistrer_sauvegarde(&self) -> Result<(), String> {
        self.exiger_geste()?;
        let voulue = self.sauvegarde();
        self.ecrire_sauvegarde_fusion(&Sauvegarde::default(), &voulue).map(|_| ())
    }

    /// L'écrivain unique de sauvegarde.json (lot A, étape 2) : sous le verrou du geste, et un seul à la fois, il relit
    /// le fichier et y fusionne seulement ce que le geste a changé depuis sa copie (`base` devenue `voulue`), au lieu
    /// d'écrire une copie prise au début du geste. Écrit par un fichier temporaire propre à cette écriture, remplacé
    /// d'un coup (l'ancien en copie de secours). La sauvegarde en mémoire suit. Rend la copie à jour de l'écrivain :
    /// sa `voulue`, avec ce qu'un autre geste a noté entre-temps.
    pub fn ecrire_sauvegarde_fusion(&self, base: &Sauvegarde, voulue: &Sauvegarde) -> Result<Sauvegarde, String> {
        self.exiger_geste()?;
        if self.sans_disque {
            return Err(crate::contexte::SANS_DISQUE.into());
        }
        let _seul = verrou(&self.ecrivain_sauvegarde);
        let fichier = self.fichier_sauvegarde();
        let disque = match lire_fichier(&fichier) {
            Ok(Some(m)) => depuis_json(&m),
            Ok(None) => Sauvegarde::default(),
            // (illisible : ce que Nevermind en sait, jamais un fichier vide à la place)
            Err(_) => self.sauvegarde(),
        };
        let fus = fusionner(&disque, base, voulue);
        let texte = crate::json_ps::lisible(&fus.en_json());
        let tmp = tmp_unique(&fichier);
        if let Err(e) = crate::fichiers::ecrire_texte_sur_par(&fichier, &texte, true, &tmp) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.to_string());
        }
        // (la mémoire suit, d'un seul coup : ce qu'un autre fil y a noté entre-temps reste)
        let _ = self.sauvegarde();
        {
            let mut g = verrou(&self.sauvegarde);
            let memoire = g.clone().unwrap_or_default();
            let mut m = fusionner(&memoire, base, voulue);
            m.probleme = memoire.probleme;
            *g = Some(m);
        }
        // (la copie de l'écrivain : le fichier, qui a toujours la plus ancienne valeur d'origine, et ce qu'elle seule
        //  connaît, comme les valeurs reprises du registre)
        let mut copie = fusionner(&fus, &Sauvegarde::default(), voulue);
        copie.probleme = voulue.probleme.clone();
        Ok(copie)
    }

    /// [`Contexte::ecrire_sauvegarde_fusion`] pour la sauvegarde de Ton PC, donnée en JSON : rend sa copie à jour
    pub fn ecrire_sauvegarde_fusion_json(&self, base: &Value, voulue: &Value) -> Result<Value, String> {
        self.exiger_geste()?;
        self.ecrire_sauvegarde_fusion(&depuis_valeur(base), &depuis_valeur(voulue)).map(|s| s.en_json())
    }

    /// La fin de « Tout restaurer » : sauvegarde.json et ses copies effacés, par l'écrivain unique ; la sauvegarde
    /// relue (les valeurs d'origine encore gardées dans le registre)
    pub fn effacer_sauvegarde(&self) -> Result<(), String> {
        self.exiger_geste()?;
        if !self.sans_disque {
            let _seul = verrou(&self.ecrivain_sauvegarde);
            let f = self.fichier_sauvegarde();
            for fin in ["", ".bak", ".tmp"] {
                let _ = std::fs::remove_file(crate::fichiers::a_cote(&f, fin));
            }
            effacer_tmp_restes(&self.donnees);
        }
        self.importer_sauvegarde();
        Ok(())
    }

    /// `Save-BackupOuOublier` : l'état d'origine noté AVANT le changement ; s'il n'a pas pu être écrit, il est
    /// oublié (`oublier`) et l'erreur remonte, sans rien changer.
    pub fn enregistrer_sauvegarde_ou_oublier(&self, oublier: impl FnOnce(&mut Sauvegarde)) -> Result<(), String> {
        self.exiger_geste()?;
        match self.enregistrer_sauvegarde() {
            Ok(()) => Ok(()),
            Err(e) => {
                self.changer_sauvegarde(oublier);
                Err(format!(
                    "Nevermind n'a pas pu noter l'état d'origine (sauvegarde.json : {e}) : rien n'a été changé."
                ))
            }
        }
    }

    /// `Get-TexteProblemeSauvegarde` : ce que « Tout restaurer » et la désinstallation disent quand la sauvegarde a
    /// un problème (vide sinon)
    pub fn texte_probleme_sauvegarde(&self) -> String {
        texte_probleme(self.sauvegarde().probleme.as_deref())
    }

    /// `Save-Original` : mémorise la valeur d'origine avant la première modification (et une copie dans le
    /// registre de l'utilisateur pour un réglage de l'utilisateur ; jamais en mode test).
    pub fn noter_origine(&self, chemin: &str, nom: &str) -> Result<(), String> {
        self.exiger_geste()?;
        let cle = format!("{chemin}|{nom}");
        if self.sauvegarde().registre.contains_key(&cle) {
            return Ok(());
        }
        let st = self.registre_etat(chemin, nom);
        let e = entree(chemin, nom, vrai(&st["Existed"]), st["Value"].clone(), st["Kind"].clone());
        self.changer_sauvegarde(|s| {
            s.registre.insert(cle.clone(), e.clone());
        });
        let k = cle.clone();
        self.enregistrer_sauvegarde_ou_oublier(move |s| {
            s.registre.remove(&k);
        })?;
        if registre::utilisateur(chemin) && !self.essai {
            let copie = format!("HKCU:\\{}", self.cle_origines_utilisateur());
            if crate::registre::valeur(&copie, &cle).is_none() {
                let _ =
                    crate::registre::ecrire_utilisateur(&copie, &cle, &json!(crate::json_ps::compact(&e)), "String");
            }
        }
        Ok(())
    }

    /// `Set-Reg` : note l'origine, le changement (action en cours), puis écrit (l'Aide pour un réglage du PC)
    pub fn changer_registre(&self, chemin: &str, nom: &str, valeur: &Value, kind: &str) -> Result<(), String> {
        self.exiger_geste()?;
        self.noter_origine(chemin, nom)?;
        if self.changements_en_cours() {
            let st = self.registre_etat(chemin, nom);
            self.noter_changement(json!({ "Type": "reg", "Path": chemin, "Name": nom, "Existed": st["Existed"], "Value": st["Value"], "Kind": st["Kind"], "Nouveau": valeur, "NouveauType": kind }));
        }
        self.ecrire_registre(chemin, nom, valeur, kind)
    }

    /// `Clear-Reg` : une valeur qui existe est effacée (origine et changement notés)
    pub fn vider_registre(&self, chemin: &str, nom: &str) -> Result<(), String> {
        self.exiger_geste()?;
        let st = self.registre_etat(chemin, nom);
        if !vrai(&st["Existed"]) {
            return Ok(());
        }
        self.noter_origine(chemin, nom)?;
        self.noter_changement(json!({ "Type": "reg", "Path": chemin, "Name": nom, "Existed": true, "Value": st["Value"], "Kind": st["Kind"], "Efface": true }));
        self.effacer_registre(chemin, nom)
    }

    // -----------------------------------------------------------------------------------------------------------
    // Les points ignorés (ignores.json)
    // -----------------------------------------------------------------------------------------------------------

    /// `Import-Ignored` / `$script:Ignored` : les points que l'utilisateur a choisi d'ignorer
    pub fn ignores(&self) -> Vec<String> {
        if let Some(l) = verrou(&self.ignores).clone() {
            return l;
        }
        let l = match self.lire_json(&self.donnees.join("ignores.json")) {
            Some(Value::Array(a)) => a.iter().filter(|x| vrai(x)).map(texte_ps).collect(),
            Some(v) if vrai(&v) => vec![texte_ps(&v)],
            _ => Vec::new(),
        };
        *verrou(&self.ignores) = Some(l.clone());
        l
    }

    /// `Save-Ignored` : enregistre la liste des points ignorés
    pub fn enregistrer_ignores(&self, l: Vec<String>) -> Result<(), String> {
        *verrou(&self.ignores) = Some(l.clone());
        self.ecrire_json(&self.donnees.join("ignores.json"), &json!(l), false, false)
    }
}

/// Le texte de `Get-TexteProblemeSauvegarde` pour ce problème
pub fn texte_probleme(probleme: Option<&str>) -> String {
    match probleme {
        None | Some("") => String::new(),
        Some(p) => format!(
            "{} : certains réglages changés par Nevermind ne pourront peut-être pas être remis comme avant. Pour tout remettre, utilise le point de restauration de Windows créé par Nevermind (page Sauvegarde, « Restauration du système »).",
            if p == "abimee" {
                "Le fichier qui garde les réglages d'origine est abîmé"
            } else {
                "Le fichier qui garde les réglages d'origine a disparu"
            }
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sv(registre: &[(&str, i64)], dns: &[(&str, &str)], plan: Option<&str>, oublies: &[&str]) -> Sauvegarde {
        let mut s = Sauvegarde::default();
        for (k, v) in registre {
            s.registre.insert((*k).into(), json!({ "Value": v }));
        }
        for (k, v) in dns {
            s.dns.insert((*k).into(), json!(v));
        }
        s.plan = plan.map(str::to_string);
        s.oublies = oublies.iter().map(|x| x.to_string()).collect();
        s
    }

    /// La fusion de l'écrivain unique : seul ce que le geste a changé, la plus ancienne valeur d'origine gagne, ce
    /// qu'un autre geste a noté reste, ce que le geste a retiré est retiré
    #[test]
    fn fusion_de_l_ecrivain_unique() {
        // (le fichier : le DNS noté par le Réseau pendant le geste de Ton PC, et une valeur déjà là)
        let disque = sv(&[("a", 1)], &[("carte", "192.168.1.1")], None, &[]);
        // (la copie de Ton PC au début : « a » seulement ; puis il note « b », croit noter le DNS, retire « a »)
        let base = sv(&[("a", 1)], &[], None, &[]);
        let voulue = sv(&[("b", 2)], &[("carte", "1.1.1.1")], Some("plan"), &["a"]);
        let f = fusionner(&disque, &base, &voulue);
        assert_eq!(f, sv(&[("b", 2)], &[("carte", "192.168.1.1")], Some("plan"), &["a"]));
        // La copie de Ton PC, à jour : avec le DNS noté par le Réseau
        let copie = fusionner(&f, &Sauvegarde::default(), &voulue);
        assert_eq!(
            copie.dns.get("carte"),
            Some(&json!("192.168.1.1")),
            "la plus ancienne valeur d'origine, aussi dans sa copie"
        );
        assert!(copie.registre.contains_key("b") && !copie.registre.contains_key("a"));
        // Un plan déjà noté n'est jamais remplacé ; retiré seulement s'il est celui que le geste a retiré
        let disque = sv(&[], &[], Some("ancien"), &[]);
        assert_eq!(
            fusionner(&disque, &sv(&[], &[], None, &[]), &sv(&[], &[], Some("nouveau"), &[])).plan.as_deref(),
            Some("ancien")
        );
        assert_eq!(fusionner(&disque, &sv(&[], &[], Some("ancien"), &[]), &sv(&[], &[], None, &[])).plan, None);
        assert_eq!(
            fusionner(&disque, &sv(&[], &[], Some("autre"), &[]), &sv(&[], &[], None, &[])).plan.as_deref(),
            Some("ancien")
        );
    }
}
