//! Le vrai PC ([`PcWindows`]) : les lectures en natif, les écritures du registre de l'utilisateur comme donnees.ps1,
//! tout ce qui demande les droits par l'Aide (`ctx.aide`), les fichiers de Nevermind (sauvegarde.json, ignores.json,
//! journal du nettoyage) et les signaux communs vers l'hôte (question, occupe, historique, sauvegarde-changee).
//! En mode test (`ctx.essai`), rien n'est écrit sur le PC : chaque écriture est refusée.

use crate::corrections::Sauvegarde;
use crate::pc::{Pc, ResultatNettoyage, ValeurReg};
use crate::temps::Moment;
use nevermind_noyau::{Contexte, Effet};
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

impl<'a> PcWindows<'a> {
    pub fn nouveau(ctx: &'a Contexte) -> Self {
        PcWindows { ctx, effets: Vec::new(), statut: String::new(), direct: None }
    }

    /// Le texte d'attente (`occupe`) envoyé tout de suite par `envoi` (un travail de fond), pas à la fin du geste
    pub fn en_direct(mut self, envoi: &'a (dyn Fn(Effet) + 'a)) -> Self {
        self.direct = Some(envoi);
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
    fn reg_utilisateur_ecrire(&mut self, chemin: &str, nom: &str, valeur: &ValeurReg) -> Result<(), String> {
        self.essai()?;
        #[cfg(windows)]
        {
            crate::windows::reg_utilisateur_ecrire(chemin, nom, valeur)
        }
        #[cfg(not(windows))]
        {
            let _ = (chemin, nom, valeur);
            Err("pas de registre".into())
        }
    }
    fn reg_utilisateur_effacer(&mut self, chemin: &str, nom: &str) -> Result<(), String> {
        self.essai()?;
        #[cfg(windows)]
        {
            crate::windows::reg_utilisateur_effacer(chemin, nom)
        }
        #[cfg(not(windows))]
        {
            let _ = (chemin, nom);
            Ok(())
        }
    }
    fn origine_utilisateur(&mut self, cle: &str, json: &str) {
        if self.ctx.essai {
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
    fn origine_utilisateur_oublier(&mut self, cle: &str) {
        if self.ctx.essai {
            return;
        }
        #[cfg(windows)]
        if let Some(k) = crate::windows::reg_utilisateur_cle(ORIGINES) {
            let _ = k.remove_value(cle);
        }
        #[cfg(not(windows))]
        let _ = cle;
    }
    fn origines_utilisateur_effacer(&mut self) {
        if self.ctx.essai {
            return;
        }
        #[cfg(windows)]
        crate::windows::reg_utilisateur_effacer_cle(ORIGINES);
    }
    fn origines_machine_effacer(&mut self) -> Result<(), String> {
        // (la clé de la machine demande les droits : seule l'Aide l'efface)
        Err("droits administrateur".into())
    }
    fn aide(&mut self, action: &str, parametres: Value, delai_ms: u64) -> Result<Value, String> {
        // (en test, seulement une fausse Aide : jamais la vraie)
        if self.ctx.essai && !action.starts_with("lecture.") && !self.ctx.a_fausse_aide() {
            return Err("mode test : l'Aide n'est pas appelée".into());
        }
        self.ctx.aide(action, &parametres, delai_ms)
    }
    fn aide_lecture(&mut self, action: &str, parametres: Value) -> Option<Value> {
        self.ctx.aide_lecture(action, &parametres)
    }
    fn powercfg(&mut self, args: &[String]) -> (i32, String) {
        let lecture = matches!(args.first().map(|s| s.as_str()), Some("/getactivescheme" | "/list" | "/q"));
        if !lecture && self.ctx.essai {
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
    fn curseur_ecrire(&mut self, guid: &str) -> i32 {
        if self.ctx.essai {
            return 1;
        }
        #[cfg(windows)]
        {
            crate::windows::curseur_ecrire(guid)
        }
        #[cfg(not(windows))]
        {
            let _ = guid;
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
    fn ecran_frequence(&mut self, appareil: &str, hz: i64) -> i32 {
        if self.ctx.essai {
            return 1;
        }
        #[cfg(windows)]
        {
            crate::windows::ecran_frequence(appareil, hz)
        }
        #[cfg(not(windows))]
        {
            let _ = (appareil, hz);
            1
        }
    }
    fn souris_relire(&mut self, seuil1: i64, seuil2: i64, vitesse: i64) {
        if self.ctx.essai {
            return;
        }
        #[cfg(windows)]
        crate::windows::souris(seuil1, seuil2, vitesse);
        #[cfg(not(windows))]
        let _ = (seuil1, seuil2, vitesse);
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
        if self.ctx.essai {
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
        #[cfg(windows)]
        {
            crate::windows::regles_pare_feu()
        }
        #[cfg(not(windows))]
        {
            0
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
    fn nettoyer_dossier(&mut self, _id: &str, chemins: &[String], exclus: &[String]) -> ResultatNettoyage {
        if self.ctx.essai {
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
    fn sauvegarde_ecrire(&mut self, sauvegarde: &Value) -> Result<(), String> {
        let t = serde_json::to_string_pretty(sauvegarde).unwrap_or_default();
        ecrire_sur(&self.ctx.donnees.join("sauvegarde.json"), &t, true)?;
        // (le noyau garde aussi sauvegarde.json en mémoire, pour les écritures des autres blocs : il le relit)
        self.ctx.importer_sauvegarde();
        self.effets.push(Effet::Signal { nom: "sauvegarde-changee".into(), donnees: json!({}) });
        Ok(())
    }
    fn sauvegarde_effacer(&mut self) {
        for n in ["sauvegarde.json", "sauvegarde.json.bak", "sauvegarde.json.tmp"] {
            let _ = std::fs::remove_file(self.ctx.donnees.join(n));
        }
        self.ctx.importer_sauvegarde();
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
    fn historique_tout_annule(&mut self, _sauvegarde: &crate::corrections::Sauvegarde) {
        // (Set-HistoryAllUndone ; la remise du pare-feu n'est pas suivie ici : `pare_feu_echoue` faux)
        self.ctx.marquer_historique_tout_annule(false);
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
