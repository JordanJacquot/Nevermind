//! Le vrai PC ([`PcWindows`]) : les lectures en natif, les écritures du registre de l'utilisateur comme donnees.ps1,
//! tout ce qui demande les droits par l'Aide (`ctx.aide`), les fichiers de Nevermind (sauvegarde.json, ignores.json,
//! journal du nettoyage) et les signaux communs vers l'hôte (question, occupe, historique, sauvegarde-changee).
//! En mode test (`ctx.essai`), rien n'est écrit sur le PC : chaque écriture est refusée.

use crate::corrections::Sauvegarde;
use crate::pc::{Pc, ResultatNettoyage, ValeurReg};
use crate::temps::Moment;
use nevermind_noyau::{Contexte, Effet, Permis};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const ORIGINES: &str = "Software\\Mindset\\Nevermind\\Origines";
const ORIGINES_ESSAI: &str = "Software\\MindsetTest\\Origines";

pub struct PcWindows<'a> {
    ctx: &'a Contexte,
    effets: Vec<Effet>,
    statut: String,
    /// Dans un travail de fond (audit G3) : le texte d'attente part tout de suite, sans attendre la fin du geste
    direct: Option<&'a (dyn Fn(Effet) + 'a)>,
    /// Tests (mode test seulement) : le nombre de règles du pare-feu posées par Nevermind
    /// (`Some(None)` : la lecture des règles échoue)
    regles_essai: Option<Option<usize>>,
}

/// Lire un fichier JSON de Nevermind (ou sa copie de secours)
fn lire_json(p: &Path) -> Option<Value> {
    for f in [p.to_path_buf(), PathBuf::from(format!("{}.bak", p.display()))] {
        if let Ok(t) = std::fs::read_to_string(&f)
            && let Ok(v) = serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}'))
        {
            return Some(v);
        }
    }
    None
}

/// `Save-TexteSur` : écrit à côté (.tmp), puis remplace ; l'ancien reste en copie de secours (.bak)
fn ecrire_sur(p: &Path, texte: &str, bom: bool) -> Result<(), String> {
    let tmp = PathBuf::from(format!("{}.tmp", p.display()));
    let mut o = Vec::new();
    if bom {
        o.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    o.extend_from_slice(texte.as_bytes());
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    std::fs::write(&tmp, o).map_err(|e| e.to_string())?;
    if p.exists() {
        let _ = std::fs::copy(p, format!("{}.bak", p.display()));
    }
    std::fs::rename(&tmp, p).map_err(|e| e.to_string())
}

/// `Import-Backup` : sauvegarde.json (ou sa copie), complétée par les valeurs d'origine gardées dans le registre
pub fn lire_sauvegarde(ctx: &Contexte) -> Sauvegarde {
    // (M9, audit du 08/11 : une sauvegarde abîmée ou disparue est dite comme telle. Le noyau la lit d'abord : il met un
    //  fichier abîmé de côté, reprend sa copie de secours, et note le problème, comme Import-Backup)
    let probleme = ctx.sauvegarde().probleme;
    let fichier = ctx.donnees.join("sauvegarde.json");
    let abime = fichier.exists()
        && !std::fs::read_to_string(&fichier)
            .is_ok_and(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).is_ok());
    let mut sv = lire_json(&fichier).map(|j| Sauvegarde::depuis(&j)).unwrap_or_default();
    sv.probleme = probleme.filter(|p| !p.is_empty()).or_else(|| abime.then(|| "abimee".to_string()));
    #[cfg(windows)]
    {
        let mut connues: Vec<String> = sv
            .registre
            .values()
            .map(|e| {
                format!(
                    "{}|{}",
                    crate::corrections::cle_normale(&crate::net::champ_texte(e, "Path")),
                    crate::net::champ_texte(e, "Name")
                )
            })
            .collect();
        connues.extend(sv.oublies.iter().cloned());
        let utilisateur = if ctx.essai { ORIGINES_ESSAI } else { ORIGINES };
        for chemin in
            ["HKLM:\\SOFTWARE\\Mindset\\Nevermind\\Aide\\Origines".to_string(), format!("HKCU:\\{utilisateur}")]
        {
            for n in crate::windows::reg_noms(&chemin).unwrap_or_default() {
                let Some(o) = crate::windows::reg_lire(&chemin, &n)
                    .ok()
                    .flatten()
                    .and_then(|v| serde_json::from_str::<Value>(&v.texte()).ok())
                else {
                    continue;
                };
                let (p, nom) = (crate::net::champ_texte(&o, "Path"), crate::net::champ_texte(&o, "Name"));
                if p.is_empty() || nom.is_empty() {
                    continue;
                }
                let cle = format!("{}|{nom}", crate::corrections::cle_normale(&p));
                if connues.contains(&cle) {
                    continue;
                }
                sv.registre.insert(format!("{p}|{nom}"), json!({ "Path": p, "Name": nom, "Existed": crate::net::champ_vrai(&o, "Existed"), "Value": o.get("Value"), "Kind": o.get("Kind") }));
                connues.push(cle);
            }
        }
    }
    sv
}

pub fn lire_ignores(ctx: &Contexte) -> Vec<String> {
    lire_json(&ctx.donnees.join("ignores.json"))
        .map(|v| crate::net::liste(&v).iter().filter(|x| crate::net::vrai(x)).map(crate::net::texte).collect())
        .unwrap_or_default()
}

/// `Get-ChangementsDefinitifs` : lu dans historique.json (tenu par le bloc « protection »)
pub fn changements_definitifs(pc: &mut dyn Pc) -> Vec<String> {
    pc.definitifs()
}

/// `Add-AideLecturePc` : ce que seule l'Aide sait lire (Secure Boot, puce TPM, santé des disques)
pub fn completer_par_aide(pc: &mut dyn Pc, e: &mut crate::analyse::Entrees) {
    // (la lecture vient de finir, personne n'a cliqué : jamais de demande de Windows ; audit G5)
    let Some(l) = pc.aide_lecture("lecture.pc", json!({})) else { return };
    let d = &mut e.data;
    if d.get("SecureBoot").is_none_or(|x| x.is_null())
        && let Some(sb) = l.get("SecureBoot").filter(|x| !x.is_null())
    {
        d["SecureBoot"] = json!(crate::net::vrai(sb));
    }
    if !crate::net::champ_vrai(d, "TpmOk") && crate::net::champ_vrai(&l, "TpmOk") {
        d["TpmOk"] = json!(true);
        d["Tpm"] = l.get("Tpm").cloned().unwrap_or(Value::Null);
    }
    if let Some(Value::Array(disques)) = d.get_mut("Disks") {
        for dd in disques {
            if crate::net::champ_vrai(dd, "Rel") {
                continue;
            }
            let id = dd.get("Disk").map(|x| crate::net::champ_texte(x, "DeviceId")).unwrap_or_default();
            if let Some(f) = l.get("Fiabilite").and_then(|f| f.get(&id)) {
                dd["Rel"] = f.clone();
            }
        }
    }
}

/// La question posée à NVIDIA (la même que la version PowerShell, interface.ps1) : le dernier pilote « Game Ready »
/// WHQL de Windows 10 et 11 64 bits, le même pour toutes les GeForce récentes
pub const ADRESSE_PILOTE_NVIDIA: &str = "https://gfwsl.geforce.com/services_toolkit/services/com/nvidia/services/AjaxDriverService.php?func=DriverManualLookup&psid=127&pfid=995&osID=57&languageCode=1033&beta=0&isWHQL=1&dltype=-1&dch=1&upCRD=0&qnf=0&sort1=0&numberOfResults=1";

/// Le dernier pilote dans la réponse de NVIDIA (`IDS[0].downloadInfo`) : `{ Version, Date, Url }` comme la version
/// PowerShell, `None` si la réponse n'en donne pas un lisible (page d'erreur, portail d'un Wi-Fi public)
pub fn pilote_nvidia_depuis_reponse(texte: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(texte.trim_start_matches('\u{feff}').trim()).ok()?;
    let info = v.get("IDS")?.get(0)?.get("downloadInfo")?;
    let version = crate::net::champ_texte(info, "Version").trim().to_string();
    let parties: Vec<&str> = version.split('.').collect();
    if parties.len() != 2 || parties.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    let url = crate::net::champ_texte(info, "DetailsURL");
    let url =
        if url.starts_with("https://www.nvidia.com/") { url } else { "https://www.nvidia.com/fr-fr/drivers/".into() };
    Some(json!({ "Version": version, "Date": crate::net::champ_texte(info, "ReleaseDateTime"), "Url": url }))
}

/// Le dernier pilote NVIDIA (`$r.NvLatest` de `$AnalysisDataWork`) : demandé à NVIDIA seulement s'il y a une carte
/// NVIDIA et si le réglage « Vérifier le pilote NVIDIA au lancement » est mis (par défaut). Sans réponse, rien ne
/// change : l'analyse regarde l'âge du pilote installé. Avant, la version Rust ne le demandait jamais : un pilote de
/// moins de 6 mois passait pour « à jour » même avec un plus récent sorti (PC de Jordan, 08/11 : 617.42 contre 621.14).
/// Seulement pour une carte que ce pilote prend encore en charge (second audit du 09/11) : une GTX 1060 restait sinon
/// sur « Nouveau pilote NVIDIA disponible » pour toujours, vers un pilote qui ne s'installe pas sur elle.
pub fn completer_pilote_nvidia(pc: &mut dyn Pc, e: &mut crate::analyse::Entrees) {
    if !pc.reglage("PiloteNvidia").is_none_or(|r| r.is_null() || crate::net::vrai(&r)) {
        return;
    }
    let nvidia = crate::net::champ_liste(&e.data, "GPUs")
        .iter()
        .any(|g| crate::analyse::nvidia_suivie(&crate::net::champ_texte(g, "Name")));
    if !nvidia {
        return;
    }
    if let Some(d) = pc.reponse_pilote_nvidia().as_deref().and_then(pilote_nvidia_depuis_reponse) {
        e.data["NvLatest"] = d;
    }
}

impl<'a> PcWindows<'a> {
    pub fn nouveau(ctx: &'a Contexte) -> Self {
        PcWindows { ctx, effets: Vec::new(), statut: String::new(), direct: None, regles_essai: None }
    }

    /// Le texte d'attente (`occupe`) envoyé tout de suite par `envoi` (un travail de fond), pas à la fin du geste
    pub fn en_direct(mut self, envoi: &'a (dyn Fn(Effet) + 'a)) -> Self {
        self.direct = Some(envoi);
        self
    }

    /// Tests (mode test seulement) : le nombre de règles du pare-feu posées par Nevermind, au lieu de celles du PC
    pub fn avec_regles_pare_feu_essai(mut self, n: usize) -> Self {
        self.regles_essai = Some(Some(n));
        self
    }

    /// Tests (mode test seulement) : la lecture des règles du pare-feu échoue (COM refusé, service arrêté...)
    pub fn avec_regles_pare_feu_illisibles(mut self) -> Self {
        self.regles_essai = Some(None);
        self
    }

    /// Ce qui est à montrer ou à dire à l'hôte après le geste
    pub fn annonces(mut self) -> Vec<Effet> {
        if !self.statut.is_empty() {
            self.effets.push(Effet::Signal { nom: "occupe".into(), donnees: json!({ "texte": "" }) });
            let s = std::mem::take(&mut self.statut);
            self.effets.push(Effet::Annonce(s));
        }
        self.effets
    }

    fn essai(&self) -> Result<(), String> {
        if self.ctx.essai { Err("mode test : rien n'est changé sur le PC".into()) } else { Ok(()) }
    }
}

impl Pc for PcWindows<'_> {
    fn exiger_geste(&self) -> Result<Permis, String> {
        self.ctx.exiger_geste()
    }
    fn suspendre_geste(&mut self) -> Option<u64> {
        self.ctx.suspendre_geste()
    }
    fn reprendre_geste(&mut self, id: u64) -> Result<(), String> {
        self.ctx.reprendre_geste(id)
    }
    fn reg_lire(&self, chemin: &str, nom: &str) -> Result<Option<ValeurReg>, String> {
        #[cfg(windows)]
        {
            crate::windows::reg_lire(chemin, nom)
        }
        #[cfg(not(windows))]
        {
            let _ = (chemin, nom);
            Ok(None)
        }
    }
    fn reg_noms(&self, chemin: &str) -> Option<Vec<String>> {
        #[cfg(windows)]
        {
            crate::windows::reg_noms(chemin)
        }
        #[cfg(not(windows))]
        {
            let _ = chemin;
            None
        }
    }
    fn reg_utilisateur_ecrire(
        &mut self,
        p: &Permis,
        chemin: &str,
        nom: &str,
        valeur: &ValeurReg,
    ) -> Result<(), String> {
        self.essai()?;
        self.ctx.exiger_geste()?;
        #[cfg(windows)]
        {
            crate::windows::reg_utilisateur_ecrire(p, chemin, nom, valeur)
        }
        #[cfg(not(windows))]
        {
            let _ = (p, chemin, nom, valeur);
            Err("pas de registre".into())
        }
    }
    fn reg_utilisateur_effacer(&mut self, p: &Permis, chemin: &str, nom: &str) -> Result<(), String> {
        self.essai()?;
        self.ctx.exiger_geste()?;
        #[cfg(windows)]
        {
            crate::windows::reg_utilisateur_effacer(p, chemin, nom)
        }
        #[cfg(not(windows))]
        {
            let _ = (p, chemin, nom);
            Ok(())
        }
    }
    fn origine_utilisateur(&mut self, _p: &Permis, cle: &str, json: &str) {
        if self.ctx.essai || self.ctx.exiger_geste().is_err() {
            return;
        }
        #[cfg(windows)]
        if let Some(k) = crate::windows::reg_utilisateur_cle(ORIGINES)
            && k.get_value(cle).is_err()
        {
            let _ = k.set_string(cle, json);
        }
        #[cfg(not(windows))]
        let _ = (cle, json);
    }
    fn origine_utilisateur_oublier(&mut self, _p: &Permis, cle: &str) {
        if self.ctx.essai || self.ctx.exiger_geste().is_err() {
            return;
        }
        #[cfg(windows)]
        if let Some(k) = crate::windows::reg_utilisateur_cle(ORIGINES) {
            let _ = k.remove_value(cle);
        }
        #[cfg(not(windows))]
        let _ = cle;
    }
    fn origines_utilisateur_effacer(&mut self, p: &Permis) {
        if self.ctx.essai || self.ctx.exiger_geste().is_err() {
            return;
        }
        #[cfg(windows)]
        crate::windows::reg_utilisateur_effacer_cle(p, ORIGINES);
        #[cfg(not(windows))]
        let _ = p;
    }
    fn origines_machine_effacer(&mut self, _p: &Permis) -> Result<(), String> {
        // (la clé de la machine demande les droits : seule l'Aide l'efface)
        Err("droits administrateur".into())
    }
    fn aide(&mut self, action: &str, parametres: Value, delai_ms: u64) -> Result<Value, String> {
        // (en test, seulement une fausse Aide : jamais la vraie)
        if self.ctx.essai && !action.starts_with("lecture.") && !self.ctx.a_fausse_aide() {
            return Err("mode test : l'Aide n'est pas appelée".into());
        }
        // (une action qui change le PC : par aide_qui_change, avec le permis ; ici, le noyau la refuse, lot J)
        self.ctx.aide(action, &parametres, delai_ms)
    }
    fn aide_qui_change(&mut self, p: &Permis, action: &str, parametres: Value, delai_ms: u64) -> Result<Value, String> {
        // (seulement sous le verrou des gestes, lot A : le permis le prouve, et le noyau le revérifie)
        self.ctx.exiger_geste()?;
        if self.ctx.essai && !self.ctx.a_fausse_aide() {
            return Err("mode test : l'Aide n'est pas appelée".into());
        }
        self.ctx.aide_qui_change(p, action, &parametres, delai_ms)
    }
    fn aide_lecture(&mut self, action: &str, parametres: Value) -> Option<Value> {
        self.ctx.aide_lecture(action, &parametres)
    }
    fn relire(&mut self) -> Option<crate::lecture::Lu> {
        Some(crate::lecture::lire())
    }
    fn reponse_pilote_nvidia(&mut self) -> Option<String> {
        // (en mode test, rien ne sort sur Internet)
        if self.ctx.essai {
            return None;
        }
        #[cfg(windows)]
        {
            // (le curl de Windows, sans fenêtre ; 6 secondes au plus comme la version PowerShell ; une réponse
            //  d'erreur n'est pas lue, `--fail`)
            let args: Vec<String> =
                ["-s", "--fail", "-L", "-m", "6", "-A", "Mozilla/5.0 Nevermind", ADRESSE_PILOTE_NVIDIA]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
            let (code, texte) = crate::windows::commande("curl.exe", &args);
            (code == 0 && !texte.trim().is_empty()).then_some(texte)
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
    fn powercfg(&mut self, args: &[String]) -> (i32, String) {
        // (lot J : seulement une lecture ; un changement passe par powercfg_changer, avec le permis)
        let lecture = matches!(args.first().map(|s| s.as_str()), Some("/getactivescheme" | "/list" | "/q"));
        if !lecture {
            self.ctx
                .journal(&format!("powercfg {} refusé : un changement passe par le verrou des gestes", args.join(" ")));
            return (1, String::new());
        }
        #[cfg(windows)]
        {
            crate::windows::powercfg(args)
        }
        #[cfg(not(windows))]
        {
            (1, String::new())
        }
    }
    fn powercfg_changer(&mut self, _p: &Permis, args: &[String]) -> (i32, String) {
        if self.ctx.essai || self.ctx.exiger_geste().is_err() {
            return (1, String::new());
        }
        #[cfg(windows)]
        {
            crate::windows::powercfg(args)
        }
        #[cfg(not(windows))]
        {
            (1, String::new())
        }
    }
    fn curseur(&self) -> Option<String> {
        #[cfg(windows)]
        {
            crate::windows::curseur()
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
    fn curseur_ecrire(&mut self, p: &Permis, guid: &str) -> i32 {
        if self.ctx.essai || self.ctx.exiger_geste().is_err() {
            return 1;
        }
        #[cfg(windows)]
        {
            crate::windows::curseur_ecrire(p, guid)
        }
        #[cfg(not(windows))]
        {
            let _ = (p, guid);
            1
        }
    }
    fn ecrans(&self) -> Vec<String> {
        #[cfg(windows)]
        {
            crate::windows::ecrans()
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    }
    fn ecran_frequence(&mut self, p: &Permis, appareil: &str, hz: i64) -> i32 {
        if self.ctx.essai || self.ctx.exiger_geste().is_err() {
            return 1;
        }
        #[cfg(windows)]
        {
            crate::windows::ecran_frequence(p, appareil, hz)
        }
        #[cfg(not(windows))]
        {
            let _ = (p, appareil, hz);
            1
        }
    }
    fn souris_relire(&mut self, p: &Permis, seuil1: i64, seuil2: i64, vitesse: i64) {
        if self.ctx.essai || self.ctx.exiger_geste().is_err() {
            return;
        }
        #[cfg(windows)]
        crate::windows::souris(p, seuil1, seuil2, vitesse);
        #[cfg(not(windows))]
        let _ = (p, seuil1, seuil2, vitesse);
    }
    fn service(&self, nom: &str) -> Option<(String, bool)> {
        #[cfg(windows)]
        {
            crate::windows::service(nom)
        }
        #[cfg(not(windows))]
        {
            let _ = nom;
            None
        }
    }
    fn lancer(&mut self, args: &[String]) -> Result<(), String> {
        if self.ctx.essai {
            self.ctx.journal(&format!("Mode test : {} n'est pas lancé", args.join(" ")));
            return Ok(());
        }
        #[cfg(windows)]
        return crate::windows::lancer(args);
        #[cfg(not(windows))]
        Err("pas sous Windows".into())
    }
    fn ouvrir(&mut self, adresse: &str) {
        self.effets.push(Effet::OuvrirAdresse(adresse.into()));
    }
    fn commande(&mut self, exe: &str, args: &[String]) -> i32 {
        if self.ctx.essai || (exe.eq_ignore_ascii_case("shutdown.exe") && self.ctx.exiger_geste().is_err()) {
            return 1;
        }
        #[cfg(windows)]
        {
            crate::windows::commande(exe, args).0
        }
        #[cfg(not(windows))]
        {
            let _ = (exe, args);
            1
        }
    }
    fn regles_pare_feu(&self) -> usize {
        self.regles_pare_feu_lues().unwrap_or(0)
    }
    fn regles_pare_feu_lues(&self) -> Option<usize> {
        if self.ctx.essai
            && let Some(n) = self.regles_essai
        {
            return n;
        }
        #[cfg(windows)]
        {
            crate::windows::regles_pare_feu()
        }
        #[cfg(not(windows))]
        {
            Some(0)
        }
    }
    fn index_carte(&self, guid: &str) -> Option<i64> {
        #[cfg(windows)]
        {
            crate::windows::index_carte(guid)
        }
        #[cfg(not(windows))]
        {
            let _ = guid;
            None
        }
    }
    fn dossier_temp(&self) -> String {
        std::env::var("TEMP").unwrap_or_default().trim_end_matches('\\').to_string()
    }
    fn dossier_windows(&self) -> String {
        std::env::var("windir").unwrap_or_else(|_| "C:\\Windows".into())
    }
    fn hiberfil(&self) -> bool {
        Path::new(&format!("{}\\hiberfil.sys", std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()))).exists()
    }
    fn nettoyer_dossier(&mut self, _p: &Permis, _id: &str, chemins: &[String], exclus: &[String]) -> ResultatNettoyage {
        if self.ctx.essai || self.ctx.exiger_geste().is_err() {
            return ResultatNettoyage::default();
        }
        crate::nettoyage::nettoyer_chemins(chemins, exclus, std::time::SystemTime::now())
    }
    fn reglage(&self, nom: &str) -> Option<Value> {
        self.ctx.reglage(nom)
    }
    fn ecrire_reglage(&mut self, nom: &str, valeur: Value, exiger: bool) -> Result<(), String> {
        self.ctx.ecrire_reglage(nom, valeur, exiger)
    }
    fn ecrire_reglage_d_origine(&mut self, p: &Permis, nom: &str, valeur: Value, exiger: bool) -> Result<(), String> {
        self.ctx.ecrire_reglage_d_origine(p, nom, valeur, exiger)
    }
    fn sauvegarde_ecrire(&mut self, p: &Permis, sauvegarde: &Value) -> Result<(), String> {
        // (sans la copie de départ du geste : seulement des ajouts, jamais un retrait)
        self.ctx.exiger_geste()?;
        self.sauvegarde_fusionner(p, &json!({}), sauvegarde).map(|_| ())
    }
    fn sauvegarde_fusionner(&mut self, p: &Permis, base: &Value, voulue: &Value) -> Result<Option<Value>, String> {
        // (l'écrivain unique du noyau, lot A étape 2 : le fichier relu, seul ce que ce geste a changé y est fusionné,
        //  par un fichier temporaire propre à cette écriture ; la sauvegarde du noyau suit)
        self.ctx.exiger_geste()?;
        let copie = self.ctx.ecrire_sauvegarde_fusion_json(p, base, voulue)?;
        self.effets.push(Effet::Signal { nom: "sauvegarde-changee".into(), donnees: json!({}) });
        Ok(Some(copie))
    }
    fn sauvegarde_effacer(&mut self, p: &Permis) {
        if self.ctx.exiger_geste().is_err() || self.ctx.effacer_sauvegarde(p).is_err() {
            return;
        }
        self.effets.push(Effet::Signal { nom: "sauvegarde-changee".into(), donnees: json!({}) });
    }
    fn ignores_ecrire(&mut self, ignores: &[String]) {
        let _ = ecrire_sur(
            &self.ctx.donnees.join("ignores.json"),
            &serde_json::to_string_pretty(&json!(ignores)).unwrap_or_default(),
            false,
        );
    }
    fn historique_ajouter(&mut self, titre: &str, details: &[String], journal: &[Value]) -> Option<String> {
        if journal.is_empty() {
            return None;
        }
        // (l'historique est celui du noyau, commun à tous les blocs : son vrai identifiant sert à « Revenir en
        //  arrière », Set-HistoryUndone)
        let id = self.ctx.ajouter_historique(titre, details, journal.to_vec());
        if id.is_some() {
            self.effets.push(Effet::Signal { nom: "sauvegarde-changee".into(), donnees: json!({}) });
        }
        id
    }
    fn historique_annule(&mut self, id: &str) {
        self.ctx.marquer_historique_annule(id);
        self.effets.push(Effet::Signal { nom: "sauvegarde-changee".into(), donnees: json!({}) });
    }
    fn historique_tout_annule(&mut self, _sauvegarde: &crate::corrections::Sauvegarde, pare_feu_echoue: bool) {
        // (Set-HistoryAllUndone : les lignes « pare-feu » seulement si le déblocage a réussi ; lot A, avant c'était
        //  toujours le cas, même quand l'Aide avait refusé)
        self.ctx.marquer_historique_tout_annule(pare_feu_echoue);
        self.effets.push(Effet::Signal { nom: "sauvegarde-changee".into(), donnees: json!({}) });
    }
    fn journal_nettoyage(&mut self, nom: &str, lignes: &[String]) -> String {
        let d = self.ctx.donnees.join("nettoyage");
        let _ = std::fs::create_dir_all(&d);
        let f = d.join(nom);
        let mut o = vec![0xEF, 0xBB, 0xBF];
        o.extend_from_slice(lignes.join("\r\n").as_bytes());
        o.extend_from_slice(b"\r\n");
        let chemin = if std::fs::write(&f, o).is_ok() { f.to_string_lossy().to_string() } else { String::new() };
        let mut vieux: Vec<PathBuf> = std::fs::read_dir(&d)
            .map(|r| {
                r.flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name().is_some_and(|n| {
                            let n = n.to_string_lossy();
                            n.starts_with("nettoyage-") && n.ends_with(".log")
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        vieux.sort();
        vieux.reverse();
        for v in vieux.into_iter().skip(10) {
            let _ = std::fs::remove_file(v);
        }
        chemin
    }
    fn journal(&mut self, ligne: &str) {
        self.ctx.journal(ligne);
    }
    fn message(&mut self, texte: &str) {
        self.effets.push(Effet::Annonce(texte.into()));
    }
    fn question(&mut self, texte: &str) -> Option<bool> {
        self.effets.push(Effet::Signal {
            nom: "question".into(),
            donnees: json!({ "id": "pc", "titre": "Nevermind", "texte": texte, "boutons": [{ "id": "oui", "texte": "Oui" }, { "id": "non", "texte": "Non", "style": "second" }], "action": "pc:reponse" }),
        });
        None
    }
    fn statut(&mut self, texte: &str) {
        self.statut = texte.into();
        let e = Effet::Signal { nom: "occupe".into(), donnees: json!({ "texte": texte }) };
        match self.direct {
            Some(envoi) => envoi(e),
            None => self.effets.push(e),
        }
    }
    fn definitifs(&mut self) -> Vec<String> {
        let h = lire_json(&self.ctx.donnees.join("historique.json")).unwrap_or(Value::Null);
        let mut l: Vec<String> = Vec::new();
        for e in crate::net::liste(&h) {
            for x in crate::net::champ_liste(&e, "Log") {
                if crate::net::champ_texte(&x, "Type") == "definitif" {
                    let q = crate::net::champ_texte(&x, "Quoi");
                    if !l.contains(&q) {
                        l.push(q);
                    }
                }
            }
        }
        l
    }
    fn maintenant(&self) -> Moment {
        Moment::lire(&nevermind_noyau::contexte::horodatage()).unwrap_or(Moment(0))
    }
}
