//! Les changements de Windows faits par Nevermind, et leur retour en arrière (tableau-de-bord.ps1, donnees.ps1,
//! optimisations.ps1, analyse.ps1, microsoft.ps1, systeme.ps1) :
//! - chaque changement déclaré à l'avance (`Set-Declarations`) ; l'état d'origine est noté AVANT dans sauvegarde.json
//!   (`Save-Original`, `Save-BackupOuOublier`) ou dans les réglages (`Set-Setting -Exiger` : SvcOriginal, PcfgOriginal) ;
//! - le journal de ce qui vient d'être fait (`$script:RunLog`), pour « Revenir en arrière » (`Undo-RunLog`) ;
//! - « Tout restaurer » (`Restore-AllSettings`) : tout ce qui est noté dans la sauvegarde remis comme avant.
//!
//! Le registre de la machine, les services et le point de restauration passent par l'Aide ; le registre de
//! l'utilisateur, powercfg, le curseur d'alimentation et les écrans sont faits par Nevermind (comme en PowerShell).

use crate::analyse::{self, REGLAGES_BATTERIE, SOUS_GROUPE_BATTERIE};
use crate::modele::{Changements, Declaration};
use crate::nettoyage;
use crate::pc::{Pc, ValeurReg, parametres};
use crate::reglages::{self, CURSEUR_PERF as BEST, HAUTES_PERF, NORMAL};
use serde_json::{Map, Value, json};

/// sauvegarde.json (`$script:Backup`)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sauvegarde {
    /// « chemin|nom » : { Path, Name, Existed, Value, Kind }
    pub registre: Map<String, Value>,
    pub plan: Option<String>,
    pub plan_cree: Option<String>,
    pub curseur: Option<String>,
    pub dns: Map<String, Value>,
    pub ecrans: Map<String, Value>,
    pub oublies: Vec<String>,
}

impl Sauvegarde {
    pub fn json(&self) -> Value {
        json!({
            "CreatedScheme": self.plan_cree, "Displays": Value::Object(self.ecrans.clone()), "Dns": Value::Object(self.dns.clone()),
            "Oublies": self.oublies, "Overlay": self.curseur, "PowerScheme": self.plan, "Registry": Value::Object(self.registre.clone())
        })
    }

    /// `Import-Backup` (la partie fichier)
    pub fn depuis(j: &Value) -> Sauvegarde {
        let texte = |k: &str| j.get(k).filter(|v| crate::net::vrai(v)).map(crate::net::texte);
        let mut registre = Map::new();
        if let Some(Value::Object(r)) = j.get("Registry") {
            for (k, v) in r {
                registre.insert(
                    k.clone(),
                    json!({ "Path": v.get("Path"), "Name": v.get("Name"), "Existed": crate::net::champ_vrai(v, "Existed"), "Value": v.get("Value"), "Kind": v.get("Kind") }),
                );
            }
        }
        let carte = |k: &str| match j.get(k) {
            Some(Value::Object(m)) => m.clone(),
            _ => Map::new(),
        };
        Sauvegarde {
            registre,
            plan: texte("PowerScheme"),
            plan_cree: texte("CreatedScheme"),
            curseur: texte("Overlay"),
            dns: carte("Dns"),
            ecrans: carte("Displays"),
            oublies: crate::net::champ_liste(j, "Oublies")
                .iter()
                .filter(|v| crate::net::vrai(v))
                .map(crate::net::texte)
                .collect(),
        }
    }

    /// `Get-BackupCount` (sans les réglages notés dans parametres.json)
    pub fn compte(&self) -> usize {
        self.registre.len()
            + self.dns.len()
            + self.ecrans.len()
            + usize::from(self.plan.is_some())
            + usize::from(self.curseur.is_some())
    }
}

/// `Get-CleRegNormale`
pub fn cle_normale(chemin: &str) -> String {
    let c = chemin.trim();
    let c = c.strip_prefix("Registry::").unwrap_or(c);
    let remplace = |c: &str, avant: &[&str], apres: &str| -> Option<String> {
        for a in avant {
            if c.len() >= a.len() && c[..a.len()].eq_ignore_ascii_case(a) {
                return Some(format!("{apres}{}", &c[a.len()..]));
            }
        }
        None
    };
    remplace(c, &["HKEY_LOCAL_MACHINE\\", "HKLM:\\", "HKLM\\"], "HKLM:\\")
        .or_else(|| remplace(c, &["HKEY_USERS\\", "HKU:\\", "HKU\\"], "HKU:\\"))
        .or_else(|| remplace(c, &["HKEY_CURRENT_USER\\", "HKCU:\\", "HKCU\\"], "HKCU:\\"))
        .unwrap_or_else(|| c.to_string())
}

/// `Test-RegUtilisateur` : un réglage de l'utilisateur (Nevermind l'écrit lui-même)
pub fn reg_utilisateur(chemin: &str) -> bool {
    let p = chemin.strip_prefix("Registry::").unwrap_or(chemin).to_ascii_uppercase();
    p.starts_with("HKCU:") || p.starts_with("HKEY_CURRENT_USER")
}

/// Une valeur de HKCU n'est écrite ou effacée que si c'est un des réglages que Nevermind change lui-même (liste fermée
/// du noyau, audit du 08/11, M5) : « Tout restaurer » et « Revenir en arrière » relisent des fichiers que le compte
/// peut modifier, et le désinstalleur les rejoue en administrateur.
fn reglage_permis(chemin: &str, nom: &str) -> Result<(), String> {
    if nevermind_noyau::registre::utilisateur_permis(chemin, nom) {
        Ok(())
    } else {
        Err("ce n'est pas un réglage que Nevermind change : il n'est pas touché".into())
    }
}

/// Ce que le moteur sait du PC au moment d'agir
pub struct Moteur<'a> {
    pub pc: &'a mut dyn Pc,
    pub sv: &'a mut Sauvegarde,
    /// `$script:RunLog` : `Some` pendant une correction (Revenir en arrière)
    pub journal: Option<Vec<Value>>,
    pub portable: bool,
    /// Le registre a pu changer : la liste des programmes au démarrage est à relire
    pub demarrage_a_relire: bool,
    /// La question à poser à l'écran quand `Pc::question` n'a pas pu répondre tout de suite
    pub question: Option<String>,
}

fn erreur_reg(e: String) -> String {
    e
}

impl<'a> Moteur<'a> {
    pub fn nouveau(pc: &'a mut dyn Pc, sv: &'a mut Sauvegarde, portable: bool) -> Self {
        Moteur { pc, sv, journal: None, portable, demarrage_a_relire: false, question: None }
    }

    fn noter(&mut self, e: Value) {
        if let Some(j) = self.journal.as_mut() {
            j.push(e);
        }
    }

    /// `Save-Backup`
    pub fn sauver(&mut self) -> Result<(), String> {
        let j = self.sv.json();
        self.pc.sauvegarde_ecrire(&j)
    }

    fn sauver_ou(&mut self, oublier: impl FnOnce(&mut Sauvegarde)) -> Result<(), String> {
        if let Err(e) = self.sauver() {
            oublier(self.sv);
            return Err(format!(
                "Nevermind n'a pas pu noter l'état d'origine (sauvegarde.json : {e}) : rien n'a été changé."
            ));
        }
        Ok(())
    }

    /// `Save-Original` : la valeur d'origine notée avant la première modification
    pub fn noter_origine(&mut self, chemin: &str, nom: &str) -> Result<(), String> {
        let cle = format!("{chemin}|{nom}");
        if self.sv.registre.contains_key(&cle) {
            return Ok(());
        }
        let st = self.pc.reg_lire(chemin, nom).map_err(erreur_reg)?;
        let e = json!({
            "Path": chemin, "Name": nom, "Existed": st.is_some(),
            "Value": st.as_ref().map(|v| v.json()).unwrap_or(Value::Null),
            "Kind": st.as_ref().map(|v| Value::from(v.sorte())).unwrap_or(Value::Null)
        });
        self.sv.registre.insert(cle.clone(), e.clone());
        let c2 = cle.clone();
        self.sauver_ou(move |sv| {
            sv.registre.remove(&c2);
        })?;
        if reg_utilisateur(chemin) {
            self.pc.origine_utilisateur(&cle, &serde_json::to_string(&e).unwrap_or_default());
        }
        Ok(())
    }

    /// `Write-RegValue` : le registre de la machine par l'Aide, celui de l'utilisateur par Nevermind
    pub fn ecrire_reg(&mut self, chemin: &str, nom: &str, v: &ValeurReg) -> Result<(), String> {
        if !reg_utilisateur(chemin) {
            self.pc.aide(
                "reg.ecrire",
                parametres(&[
                    ("Cle", json!(chemin)),
                    ("Nom", json!(nom)),
                    ("Valeur", v.json()),
                    ("Type", json!(v.sorte())),
                ]),
                120000,
            )?;
            return Ok(());
        }
        reglage_permis(chemin, nom)?;
        self.pc.reg_utilisateur_ecrire(chemin, nom, v)
    }

    /// `Remove-RegValue`
    pub fn effacer_reg(&mut self, chemin: &str, nom: &str) -> Result<(), String> {
        if !reg_utilisateur(chemin) {
            self.pc.aide("reg.effacer", parametres(&[("Cle", json!(chemin)), ("Nom", json!(nom))]), 120000)?;
            return Ok(());
        }
        reglage_permis(chemin, nom)?;
        self.pc.reg_utilisateur_effacer(chemin, nom)
    }

    /// `Set-Reg`
    pub fn set_reg(&mut self, chemin: &str, nom: &str, v: &ValeurReg) -> Result<(), String> {
        self.noter_origine(chemin, nom)?;
        if self.journal.is_some() {
            let st = self.pc.reg_lire(chemin, nom).map_err(erreur_reg)?;
            self.noter(json!({
                "Type": "reg", "Path": chemin, "Name": nom, "Existed": st.is_some(),
                "Value": st.as_ref().map(|x| x.json()).unwrap_or(Value::Null), "Kind": st.as_ref().map(|x| Value::from(x.sorte())).unwrap_or(Value::Null),
                "Nouveau": v.json(), "NouveauType": v.sorte()
            }));
        }
        self.ecrire_reg(chemin, nom, v)
    }

    /// `Clear-Reg`
    pub fn clear_reg(&mut self, chemin: &str, nom: &str) -> Result<(), String> {
        let Some(st) = self.pc.reg_lire(chemin, nom).map_err(erreur_reg)? else { return Ok(()) };
        self.noter_origine(chemin, nom)?;
        self.noter(json!({ "Type": "reg", "Path": chemin, "Name": nom, "Existed": true, "Value": st.json(), "Kind": st.sorte(), "Efface": true }));
        self.effacer_reg(chemin, nom)
    }

    /// `Set-Declarations`
    pub fn appliquer(&mut self, liste: &[Declaration]) -> Result<(), String> {
        for d in liste {
            match d.sorte.as_str() {
                "reg" => {
                    if d.efface {
                        self.clear_reg(&d.chemin, &d.nom)?;
                    } else if let Some(v) = &d.valeur {
                        let sorte = if d.type_reg.is_empty() { "DWord" } else { d.type_reg.as_str() };
                        let v = ValeurReg::depuis(&v.json(), sorte);
                        self.set_reg(&d.chemin, &d.nom, &v)?;
                    }
                }
                "pcfg" => self.set_batterie(&d.cle, d.nombre)?,
                "display" => self.set_frequence(&d.ecran, d.hz)?,
                "svc" => {
                    let dem = if d.demarrage.is_empty() { "Disabled" } else { d.demarrage.as_str() };
                    self.service_off(&d.nom, dem)?
                }
                _ => {}
            }
        }
        self.demarrage_a_relire = true;
        Ok(())
    }

    /// Les changements d'une correction, calculés maintenant (`Resolve-Declarations`)
    pub fn resoudre(&mut self, c: &Changements) -> Result<Vec<Declaration>, String> {
        Ok(match c {
            Changements::Aucun => Vec::new(),
            Changements::Liste(l) => l.clone(),
            Changements::Alimentation => {
                reglages::changement_alimentation(self.pc, self.portable, self.sv.plan_cree.as_deref())
            }
            Changements::Raccourcis => reglages::changements_raccourcis(self.pc)?,
            Changements::Fenetre => reglages::changement_fenetre(self.pc),
            Changements::Nettoyage => nettoyage::changements(self.pc),
        })
    }

    /// `Set-DisplayRate`
    pub fn set_frequence(&mut self, ecran: &str, hz: i64) -> Result<(), String> {
        let actuel = self.pc.ecrans().into_iter().find(|d| d.split('|').next() == Some(ecran));
        let freq = actuel.as_ref().map(|d| d.split('|').nth(4).and_then(|x| x.trim().parse::<i64>().ok()).unwrap_or(0));
        if let Some(f) = freq
            && !self.sv.ecrans.contains_key(ecran)
        {
            self.sv.ecrans.insert(ecran.into(), json!(f));
            let e = ecran.to_string();
            self.sauver_ou(move |sv| {
                sv.ecrans.remove(&e);
            })?;
        }
        let r = self.pc.ecran_frequence(ecran, hz);
        if r != 0 {
            return Err(format!("Windows a refusé le changement de fréquence (code {r})."));
        }
        if let Some(f) = freq {
            self.noter(json!({ "Type": "display", "Device": ecran, "Hz": f, "Nouveau": hz }));
        }
        Ok(())
    }

    fn pc_args(&mut self, a: &[&str]) -> (i32, String) {
        let v: Vec<String> = a.iter().map(|s| s.to_string()).collect();
        self.pc.powercfg(&v)
    }

    /// `Set-BatterySetting`
    pub fn set_batterie(&mut self, cle: &str, valeur: i64) -> Result<(), String> {
        let guid = REGLAGES_BATTERIE.iter().find(|(k, _)| *k == cle).map(|(_, g)| *g).unwrap_or("");
        let p = analyse::reglages_batterie(self.pc);
        let cur = analyse::batterie_get(&p, cle);
        let plan = reglages::plan_actif(self.pc).map(|(g, _)| g).unwrap_or_default();
        if let Some(cur) = cur {
            self.noter(json!({ "Type": "pcfg", "Guid": guid, "Key": cle, "Ac": cur[0], "Dc": cur[1], "Nouveau": valeur, "Plan": plan }));
            let orig = liste_textes(self.pc.reglage("PcfgOriginal"));
            let deja = orig.iter().any(|l| {
                let x: Vec<&str> = l.split('|').collect();
                x[0].eq_ignore_ascii_case(guid)
                    && (x.get(3).is_none_or(|p| p.is_empty())
                        || x.get(3).is_some_and(|p| p.eq_ignore_ascii_case(&plan)))
            });
            if !deja {
                let mut n = orig.clone();
                let t = |v: Option<i64>| v.map(|x| x.to_string()).unwrap_or_default();
                n.push(format!("{guid}|{}|{}|{plan}", t(cur[0]), t(cur[1])));
                self.pc.ecrire_reglage("PcfgOriginal", reglage_liste(&n), true)?;
            }
        }
        let v = valeur.to_string();
        self.pc_args(&["/setacvalueindex", "SCHEME_CURRENT", SOUS_GROUPE_BATTERIE, guid, &v]);
        let (code, _) = self.pc_args(&["/setdcvalueindex", "SCHEME_CURRENT", SOUS_GROUPE_BATTERIE, guid, &v]);
        if code != 0 {
            return Err(format!("Windows a refusé le réglage (code {code})."));
        }
        self.pc_args(&["/setactive", "SCHEME_CURRENT"]);
        Ok(())
    }

    /// `Set-ServiceOff` : service arrêté et désactivé (ou passé en manuel), par l'Aide
    pub fn service_off(&mut self, nom: &str, demarrage: &str) -> Result<(), String> {
        let Some((debut, tourne)) = self.pc.service(nom) else {
            return Err(format!("Impossible de trouver un service assorti du nom « {nom} »."));
        };
        let orig = liste_textes(self.pc.reglage("SvcOriginal"));
        if !orig.iter().any(|l| l.split('|').next().is_some_and(|n| n.eq_ignore_ascii_case(nom))) {
            let mut n = orig.clone();
            n.push(format!("{nom}|{debut}|{}", if tourne { "True" } else { "False" }));
            self.pc.ecrire_reglage("SvcOriginal", reglage_liste(&n), true)?;
        }
        self.noter(json!({ "Type": "svc", "Name": nom, "StartType": debut, "Running": tourne, "Nouveau": demarrage }));
        self.pc.aide(
            "service.regler",
            parametres(&[("Nom", json!(nom)), ("Demarrage", json!(demarrage)), ("Arreter", json!(tourne))]),
            120000,
        )?;
        Ok(())
    }

    /// `Set-BestPerfOverlay` : le curseur de Windows sur « Meilleures performances » (plan Utilisation normale)
    pub fn meilleur_curseur(&mut self) -> Result<bool, String> {
        let s = reglages::plan_actif(self.pc);
        if !s.as_ref().is_some_and(|(g, _)| g == NORMAL) {
            return Ok(false);
        }
        let Some(avant) = self.pc.curseur().filter(|c| !c.is_empty()) else { return Ok(false) };
        if self.pc.curseur_ecrire(BEST) != 0 {
            return Ok(false);
        }
        if self.sv.curseur.is_none() {
            self.sv.curseur = Some(avant.clone());
            self.sauver()?;
        }
        self.noter(json!({ "Type": "overlay", "Guid": avant, "Nouveau": BEST }));
        Ok(true)
    }

    /// `Set-PowerPlan`
    pub fn set_plan(&mut self) -> Result<(), String> {
        if self.portable && self.meilleur_curseur()? {
            return Ok(());
        }
        let actuel = reglages::plan_actif(self.pc);
        let mut cible: Option<String> = None;
        let liste = reglages::liste_plans(self.pc);
        if liste.to_lowercase().contains(HAUTES_PERF) {
            cible = Some(HAUTES_PERF.into());
        } else if let Some(c) = self
            .sv
            .plan_cree
            .clone()
            .filter(|c| reglages::liste_plans(self.pc).to_lowercase().contains(&c.to_lowercase()))
        {
            cible = Some(c);
        } else {
            let (_, out) = self.pc_args(&["-duplicatescheme", HAUTES_PERF]);
            if let Some(g) = reglages::GUID.find(&out) {
                let g = g.as_str().to_lowercase();
                self.sv.plan_cree = Some(g.clone());
                cible = Some(g);
            }
        }
        let Some(cible) = cible else {
            if self.meilleur_curseur()? {
                return Ok(());
            }
            return Err("Ce PC ne propose pas le plan Performances élevées. Va dans Paramètres > Système > Alimentation et choisis le mode « Meilleures performances ».".into());
        };
        if let Some((g, _)) = &actuel
            && self.sv.plan.is_none()
        {
            self.sv.plan = Some(g.clone());
        }
        self.sauver()?;
        if let Some((g, n)) = &actuel {
            self.noter(json!({ "Type": "power", "Guid": g, "Nom": n, "Nouveau": cible }));
        }
        self.pc_args(&["/setactive", &cible]);
        Ok(())
    }

    /// `Sync-Mouse` : Windows relit tout de suite les réglages de la souris
    pub fn sync_souris(&mut self) {
        let n = |pc: &dyn Pc, k: &str| reglages::valeur(pc, reglages::SOURIS, k).and_then(|v| v.entier()).unwrap_or(0);
        let (v, t1, t2) = (n(self.pc, "MouseSpeed"), n(self.pc, "MouseThreshold1"), n(self.pc, "MouseThreshold2"));
        self.pc.souris_relire(t1, t2, v);
    }

    /// `Invoke-TweakApply` : les changements déclarés, puis ce que le réglage fait en plus
    pub fn appliquer_reglage(&mut self, id: &str) -> Result<(), String> {
        let ch = self.resoudre(&reglages::changements(id))?;
        self.appliquer(&ch)?;
        match id {
            "power" => self.set_plan()?,
            "mouse" => self.sync_souris(),
            _ => {}
        }
        Ok(())
    }

    /// `New-RestorePoint` : `Ok(Some(true))` créé (ou un de moins de 24 h sert), `Ok(None)` : la question est posée à
    /// l'écran (continuer sans point de restauration ?), `Ok(Some(false))` : annulé.
    pub fn point_de_restauration(&mut self) -> Result<Option<bool>, String> {
        self.pc.statut("Création du point de restauration (ça peut prendre une minute)...");
        self.set_reg(
            "HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\SystemRestore",
            "SystemRestorePointCreationFrequency",
            &ValeurReg::DWord(0),
        )?;
        let r = match self.pc.aide("restauration.creer", parametres(&[("Description", json!("Nevermind"))]), 600000) {
            Ok(v) => {
                if crate::net::texte(&v).eq_ignore_ascii_case("recent") {
                    "RECENT".to_string()
                } else {
                    "OK".to_string()
                }
            }
            Err(e) => e,
        };
        if r == "OK" {
            self.pc.statut("Point de restauration créé.");
            return Ok(Some(true));
        }
        if r == "RECENT" {
            self.pc.statut("Un point de restauration de moins de 24 h existe déjà : il sert pour ces changements.");
            return Ok(Some(true));
        }
        self.pc.journal(&format!("Point de restauration: {r}"));
        let q = texte_sans_restauration(&r);
        let rep = self.pc.question(&q);
        if rep.is_none() {
            self.question = Some(q);
        }
        Ok(rep)
    }
}

/// La question quand le point de restauration n'a pas pu être créé
pub fn texte_sans_restauration(motif: &str) -> String {
    format!(
        "Impossible de créer le point de restauration.\n\nMotif : {motif}\n\nLa protection du système est peut-être désactivée sur ce PC. Tu peux quand même continuer : Nevermind garde une sauvegarde de chaque réglage modifié et peut l'annuler depuis la page Sauvegarde (dans Protection).\n\nContinuer ?"
    )
}

/// Une liste notée dans les réglages (SvcOriginal, PcfgOriginal : textes « a|b|c »)
pub fn liste_textes(v: Option<Value>) -> Vec<String> {
    match v {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a.iter().map(crate::net::texte).collect(),
        Some(x) => vec![crate::net::texte(&x)],
    }
}

/// Une liste écrite dans les réglages (toujours une liste, comme `@($orig + ...)` dans parametres.json)
fn reglage_liste(l: &[String]) -> Value {
    json!(l)
}

/// `Undo-RunLog` : les changements du journal annulés, du plus récent au plus ancien. Renvoie les erreurs.
pub fn annuler_journal(m: &mut Moteur, journal: &[Value]) -> Vec<String> {
    use crate::net::{champ_texte, champ_vrai};
    let mut erreurs = Vec::new();
    for e in journal.iter().rev() {
        let r: Result<(), String> = (|| {
            match champ_texte(e, "Type").as_str() {
                "reg" => {
                    let (p, n) = (champ_texte(e, "Path"), champ_texte(e, "Name"));
                    if champ_vrai(e, "Existed") {
                        let v = ValeurReg::depuis(e.get("Value").unwrap_or(&Value::Null), &champ_texte(e, "Kind"));
                        m.ecrire_reg(&p, &n, &v)?;
                    } else {
                        m.effacer_reg(&p, &n)?;
                    }
                }
                "power" => {
                    let g = champ_texte(e, "Guid");
                    m.pc_args(&["/setactive", &g]);
                }
                "overlay" => {
                    m.pc.curseur_ecrire(&champ_texte(e, "Guid"));
                }
                "display" => {
                    let d = champ_texte(e, "Device");
                    let r = m.pc.ecran_frequence(&d, crate::net::champ_entier(e, "Hz"));
                    if r != 0 {
                        return Err(format!("Écran {d}: fréquence non restaurée (code {r})"));
                    }
                }
                "fw" => {
                    m.pc.aide("pare-feu.debloquer", parametres(&[("Nom", json!(champ_texte(e, "Name")))]), 120000)?;
                }
                "pcfg" => {
                    let plan = champ_texte(e, "Plan");
                    let plan = if est_guid(&plan) { plan } else { "SCHEME_CURRENT".into() };
                    let g = champ_texte(e, "Guid");
                    if e.get("Ac").is_some_and(|x| !x.is_null()) {
                        let v = crate::net::champ_entier(e, "Ac").to_string();
                        m.pc_args(&["/setacvalueindex", &plan, SOUS_GROUPE_BATTERIE, &g, &v]);
                    }
                    if e.get("Dc").is_some_and(|x| !x.is_null()) {
                        let v = crate::net::champ_entier(e, "Dc").to_string();
                        m.pc_args(&["/setdcvalueindex", &plan, SOUS_GROUPE_BATTERIE, &g, &v]);
                    }
                    m.pc_args(&["/setactive", "SCHEME_CURRENT"]);
                }
                "svc" => {
                    m.pc.aide(
                        "service.regler",
                        parametres(&[
                            ("Nom", json!(champ_texte(e, "Name"))),
                            ("Demarrage", json!(champ_texte(e, "StartType"))),
                            ("Lancer", json!(champ_vrai(e, "Running"))),
                        ]),
                        120000,
                    )?;
                }
                "dns" => {
                    let mut idx = crate::net::champ_entier(e, "IfIndex");
                    let g = champ_texte(e, "Guid");
                    if !g.is_empty() {
                        idx =
                            m.pc.index_carte(&g)
                                .ok_or_else(|| "La carte réseau de ce réglage n'existe plus sur ce PC.".to_string())?;
                    }
                    let serveurs: Vec<Value> =
                        crate::net::champ_liste(e, "Servers").into_iter().filter(crate::net::vrai).collect();
                    m.pc.aide(
                        "dns.regler",
                        parametres(&[("Index", json!(idx)), ("Serveurs", Value::Array(serveurs))]),
                        120000,
                    )?;
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(x) = r {
            erreurs.push(x);
        }
    }
    m.sync_souris();
    erreurs
}

fn est_guid(s: &str) -> bool {
    s.len() == 36 && reglages::GUID.find(s).is_some_and(|m| m.start() == 0 && m.end() == 36)
}

/// `Restore-AllSettings` : tout ce qui est noté remis comme avant. Ce qui n'a pas pu l'être reste noté (pour
/// « Réessayer »). Renvoie la liste de ce qui n'a pas pu être remis.
pub fn tout_restaurer(m: &mut Moteur) -> Vec<String> {
    use crate::net::entier;
    let mut erreurs: Vec<String> = Vec::new();
    if m.pc.regles_pare_feu() > 0
        && let Err(e) = m.pc.aide("pare-feu.debloquer-tout", json!({}), 120000)
    {
        erreurs.push(format!("Pare-feu: {e}"));
    }
    let mut oublies: Vec<String> = Vec::new();
    let cles: Vec<String> = m.sv.registre.keys().cloned().collect();
    for cle in cles {
        let e = m.sv.registre.get(&cle).cloned().unwrap_or(Value::Null);
        let (p, n) = (crate::net::champ_texte(&e, "Path"), crate::net::champ_texte(&e, "Name"));
        let r: Result<(), String> = (|| {
            if crate::net::champ_vrai(&e, "Existed") {
                let k = crate::net::champ_texte(&e, "Kind");
                let v = ValeurReg::depuis(e.get("Value").unwrap_or(&Value::Null), &k);
                m.ecrire_reg(&p, &n, &v)?;
            } else {
                m.effacer_reg(&p, &n)?;
            }
            Ok(())
        })();
        match r {
            Ok(()) => {
                m.sv.registre.remove(&cle);
                oublies.push(format!("{}|{n}", cle_normale(&p)));
                if reg_utilisateur(&p) {
                    m.pc.origine_utilisateur_oublier(&cle);
                }
            }
            Err(x) => erreurs.push(format!("{p}\\{n}: {x}")),
        }
    }
    // Services arrêtés par Nevermind : leur état d'origine ; ceux qui n'ont pas pu être remis restent notés
    let svc = liste_textes(m.pc.reglage("SvcOriginal"));
    let mut svc_reste = Vec::new();
    for l in &svc {
        let x: Vec<&str> = l.split('|').collect();
        let g = |i: usize| x.get(i).copied().unwrap_or("");
        if let Err(e) = m.pc.aide(
            "service.regler",
            parametres(&[
                ("Nom", json!(g(0))),
                ("Demarrage", json!(g(1))),
                ("Lancer", json!(g(2).eq_ignore_ascii_case("True"))),
            ]),
            120000,
        ) {
            erreurs.push(format!("Service {}: {e}", g(0)));
            svc_reste.push(l.clone());
        }
    }
    if !svc.is_empty() {
        let _ = m.pc.ecrire_reglage("SvcOriginal", json!(svc_reste), false);
    }
    // Le plan d'alimentation d'origine d'abord (les réglages « batterie » vont ensuite dans leur plan)
    if let Some(ps) = m.sv.plan.clone() {
        let (code, _) = m.pc_args(&["/setactive", &ps]);
        if code != 0 {
            erreurs.push(format!("Plan d'alimentation: pas remis (powercfg a répondu {code})"));
        } else {
            if let Some(c) = m.sv.plan_cree.clone()
                && c != ps
            {
                m.pc_args(&["/delete", &c]);
            }
            m.sv.plan = None;
            m.sv.plan_cree = None;
        }
    } else if let Some(c) = m.sv.plan_cree.clone() {
        m.pc_args(&["/delete", &c]);
        m.sv.plan_cree = None;
    }
    // Réglages « batterie » changés pour l'onduleur : chaque valeur dans le plan où elle a été changée
    let pcfg = liste_textes(m.pc.reglage("PcfgOriginal"));
    let mut pcfg_reste = Vec::new();
    let plans = if pcfg.is_empty() { String::new() } else { reglages::liste_plans(m.pc) };
    let a_des_plans = plans
        .chars()
        .collect::<Vec<_>>()
        .windows(9)
        .any(|w| w[..8].iter().all(|c| c.is_ascii_hexdigit()) && w[8] == '-');
    for l in &pcfg {
        let x: Vec<&str> = l.split('|').collect();
        let plan = if x.len() >= 4 && est_guid(x[3]) { x[3].to_string() } else { "SCHEME_CURRENT".into() };
        if plan != "SCHEME_CURRENT" && a_des_plans && !plans.to_lowercase().contains(&plan.to_lowercase()) {
            continue;
        }
        let g = |i: usize| x.get(i).copied();
        let r: Result<(), String> = (|| {
            if g(1) != Some("") {
                let v = entier(g(1).unwrap_or("").trim().parse::<f64>().unwrap_or(0.0)).to_string();
                let (c, _) = m.pc_args(&["/setacvalueindex", &plan, SOUS_GROUPE_BATTERIE, x[0], &v]);
                if c != 0 {
                    return Err(format!("powercfg a répondu {c}"));
                }
            }
            if g(2) != Some("") {
                let v = entier(g(2).unwrap_or("").trim().parse::<f64>().unwrap_or(0.0)).to_string();
                let (c, _) = m.pc_args(&["/setdcvalueindex", &plan, SOUS_GROUPE_BATTERIE, x[0], &v]);
                if c != 0 {
                    return Err(format!("powercfg a répondu {c}"));
                }
            }
            Ok(())
        })();
        if let Err(e) = r {
            erreurs.push(format!("Onduleur: {e}"));
            pcfg_reste.push(l.clone());
        }
    }
    if !pcfg.is_empty() {
        m.pc_args(&["/setactive", "SCHEME_CURRENT"]);
        let _ = m.pc.ecrire_reglage("PcfgOriginal", json!(pcfg_reste), false);
    }
    if let Some(o) = m.sv.curseur.clone() {
        let r = m.pc.curseur_ecrire(&o);
        if r != 0 {
            erreurs.push(format!("Mode d'alimentation: code {r}"));
        } else {
            m.sv.curseur = None;
        }
    }
    let dns: Vec<(String, Value)> = m.sv.dns.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    for (k, v) in dns {
        let r: Result<(), String> = (|| {
            let t = k.trim_matches(['{', '}']);
            let idx = if est_guid(t) {
                m.pc.index_carte(t)
                    .ok_or_else(|| "La carte réseau de ce réglage n'existe plus sur ce PC.".to_string())?
            } else {
                k.trim().parse::<i64>().unwrap_or(0)
            };
            let serveurs: Vec<Value> =
                crate::net::texte(&v).split([',', ' ', '\t']).filter(|s| !s.is_empty()).map(Value::from).collect();
            m.pc.aide(
                "dns.regler",
                parametres(&[("Index", json!(idx)), ("Serveurs", Value::Array(serveurs))]),
                120000,
            )?;
            Ok(())
        })();
        match r {
            Ok(()) => {
                m.sv.dns.remove(&k);
            }
            Err(e) => erreurs.push(format!("DNS: {e}")),
        }
    }
    let ecrans: Vec<(String, Value)> = m.sv.ecrans.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    for (k, hz) in ecrans {
        let r = m.pc.ecran_frequence(&k, entier(crate::net::nombre_de(&hz)));
        if r != 0 {
            erreurs.push(format!("Écran {k}: fréquence non restaurée (code {r})"));
        } else {
            m.sv.ecrans.remove(&k);
        }
    }
    m.sync_souris();
    let mut origines_oubliees = false;
    if m.sv.registre.is_empty() {
        m.pc.origines_utilisateur_effacer();
        if m.pc.aide("origines.oublier", json!({}), 120000).is_ok() || m.pc.origines_machine_effacer().is_ok() {
            origines_oubliees = true;
        }
    }
    let reste = m.sv.compte();
    if reste == 0 && origines_oubliees {
        m.pc.sauvegarde_effacer();
    } else {
        let mut o = m.sv.oublies.clone();
        for x in oublies {
            if !x.is_empty() && !o.contains(&x) {
                o.push(x);
            }
        }
        m.sv.oublies = o;
        if let Err(e) = m.sauver() {
            erreurs.push(format!("Sauvegarde: {e}"));
        }
        if reste > 0 {
            m.pc.journal(&format!("Tout restaurer : {reste} réglage(s) pas remis, gardé(s) dans la sauvegarde"));
        }
    }
    erreurs
}

/// `Get-BackupCount`
pub fn compte_total(sv: &Sauvegarde, pc: &dyn Pc) -> usize {
    sv.compte() + liste_textes(pc.reglage("SvcOriginal")).len() + liste_textes(pc.reglage("PcfgOriginal")).len()
}
