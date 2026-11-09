//! Les gestes de « Ton PC » (tableau-de-bord.ps1, onglets.ps1, accueil-console.ps1) : l'analyse, la fiche d'un point,
//! « Tout corriger » (`Invoke-SheetRun`), « Revenir en arrière » (`Invoke-UndoLastRun`), « Ignorer » (`Set-IgnoreFinding`),
//! « Tout restaurer » (`Invoke-UndoAll`), les optimisations cochées (`Invoke-ApplyTweaks`), un programme au démarrage
//! (`Set-StartupToggle`) et le nettoyage (`Invoke-Clean`).
//!
//! Une question (Oui / Non, ou « Tes écrans s'affichent bien ? ») arrête la suite : elle est posée à l'écran, et la
//! suite reprend avec la réponse ([`Attente`]). En PowerShell, la fenêtre attendait la réponse au même endroit.

use crate::analyse::{self, Entrees};
use crate::corrections::{self, Moteur, Sauvegarde};
use crate::fiche::{self, Fiche};
use crate::modele::{Analyse, Constat, Execution};
use crate::nettoyage;
use crate::pc::Pc;
use crate::reglages::{self, Demarrage};
use serde_json::{Value, json};

/// Ce qu'une suite de gestes attend pour reprendre
#[derive(Debug, Clone, PartialEq)]
pub enum Attente {
    /// « Tout corriger » : la confirmation d'une correction (`Fix.Confirm`)
    Confirmer(Box<Suite>),
    /// « Tout corriger » : continuer sans point de restauration ?
    Restauration(Box<Suite>),
    /// « Tes écrans s'affichent bien ? » : la réponse (keep, revert, timeout) avant `fin` (secondes)
    Ecran(Box<Suite>),
    /// Les optimisations cochées : continuer sans point de restauration ?
    RestaurationReglages {
        ids: Vec<String>,
    },
    /// « Tout restaurer » : la confirmation, puis « Réessayer maintenant ? »
    ToutRestaurer,
    Reessayer,
    /// « Annuler » d'une ligne de l'historique (`Undo-HistoryEntry`) : la confirmation
    AnnulerEntree {
        id: String,
        titre: String,
        log: Vec<Value>,
    },
}

/// « Tout corriger » en cours
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Suite {
    pub items: Vec<Constat>,
    pub avant: Option<i64>,
    pub faits: Vec<Constat>,
    pub rates: Vec<String>,
    pub redemarrage: bool,
    pub journal: Vec<Value>,
}

/// L'état de « Ton PC » : la dernière analyse, la sauvegarde, les points ignorés, la fiche ouverte
#[derive(Debug, Default, Clone)]
pub struct Session {
    pub entrees: Entrees,
    /// Les programmes au démarrage tels que lus (activés ou pas : relu dans le registre à chaque analyse)
    pub demarrage: Vec<Demarrage>,
    pub analyse: Option<Analyse>,
    pub sv: Sauvegarde,
    pub ignores: Vec<String>,
    pub fiche: Option<Fiche>,
    pub attente: Option<Attente>,
    pub restauration_faite: bool,
    pub journal_resultat: Option<Vec<Value>>,
    pub historique_resultat: Option<String>,
}

/// `Get-HistoryItems`
pub fn elements_historique(lignes: &[String]) -> Vec<String> {
    lignes
        .iter()
        .filter(|l| l.starts_with('•'))
        .map(|l| l.trim_start_matches('•').trim_start().trim().to_string())
        .collect()
}

/// `Get-HistoryTitle`
pub fn titre_historique(titre: &str, lignes: &[String]) -> String {
    let e = elements_historique(lignes);
    match e.len() {
        1 => e[0].clone(),
        0 => titre.into(),
        n => format!(
            "{n} changements : {}{}",
            e.iter().take(3).cloned().collect::<Vec<_>>().join(", "),
            if n > 3 { "..." } else { "" }
        ),
    }
}

/// `Get-MessageErreur` : ce que voit l'utilisateur quand un geste échoue (`Invoke-Safe`)
pub fn message_erreur(e: &str) -> String {
    format!("Oups, quelque chose s'est mal passé :\n\n{e}")
}

/// Un programme de Windows qui ne s'est pas ouvert (M8, audit du 08/11 : le bouton ne faisait rien, sans le dire)
pub fn message_lancement(args: &[String], raison: &str) -> String {
    let exe = args.first().map(|e| e.rsplit(['\\', '/']).next().unwrap_or(e).to_lowercase()).unwrap_or_default();
    let quoi = match exe.as_str() {
        "mdsched.exe" => "Le test de la mémoire de Windows".to_string(),
        "perfmon.exe" => "L'historique de fiabilité de Windows".to_string(),
        _ => format!("Le programme « {exe} »"),
    };
    format!("{quoi} ne s'est pas ouvert : {}.", raison.trim().trim_end_matches('.'))
}

impl Session {
    /// Le nom du geste qui attend une réponse (« Attends la fin de « Tout restaurer » (Ton PC) »)
    pub fn geste_en_attente(&self) -> Option<&'static str> {
        Some(match self.attente.as_ref()? {
            Attente::Confirmer(_) | Attente::Restauration(_) => "Tout corriger",
            Attente::Ecran(_) => "Fréquence de l'écran",
            Attente::RestaurationReglages { .. } => "Appliquer",
            Attente::ToutRestaurer | Attente::Reessayer => "Tout restaurer",
            Attente::AnnulerEntree { .. } => "Annuler",
        })
    }

    fn moteur<'a>(&'a mut self, pc: &'a mut dyn Pc) -> Moteur<'a> {
        let portable = self.analyse.as_ref().is_some_and(|a| a.portable);
        Moteur::nouveau(pc, &mut self.sv, portable)
    }

    /// `Invoke-Analysis` (avec ce qui a été lu du PC ; les programmes au démarrage relus dans le registre)
    pub fn analyser(&mut self, pc: &mut dyn Pc) {
        pc.statut("Analyse de ton PC en cours...");
        let mut e = self.entrees.clone();
        e.demarrage = self
            .demarrage
            .iter()
            .map(|d| {
                let mut x = d.clone();
                x.actif = reglages::demarrage_actif(pc, &d.approuve, &d.nom_valeur);
                x
            })
            .collect();
        e.ignores = self.ignores.clone();
        e.plan_cree = self.sv.plan_cree.clone();
        let a = analyse::analyser(pc, &e);
        pc.statut(&format!("Analyse terminée: score de {} sur 100.", a.score));
        self.analyse = Some(a);
    }

    fn score(&self) -> Option<i64> {
        self.analyse.as_ref().map(|a| a.score)
    }

    /// `Open-Sheet` d'un point
    pub fn ouvrir(&mut self, pc: &mut dyn Pc, f: &Constat) {
        let ignores = self.ignores.clone();
        let mut m = self.moteur(pc);
        let fi = fiche::ouvrir(&mut m, f, &ignores);
        self.fiche = Some(fi);
    }

    /// `Open-FixAll` : tous les points que Nevermind corrige, du plus grand gain au plus petit
    pub fn ouvrir_tout(&mut self, pc: &mut dyn Pc) {
        let Some(a) = &self.analyse else { return };
        let mut auto: Vec<Constat> =
            a.actifs.iter().filter(|f| f.statut != "ok" && f.gain > 0.0 && f.auto()).cloned().collect();
        crate::net::trier(&mut auto, |x, y| y.gain.total_cmp(&x.gain));
        if auto.is_empty() {
            return;
        }
        // (un seul point : sa fiche à lui, comme Open-Sheet avec un seul élément)
        let ignores = self.ignores.clone();
        let mut m = self.moteur(pc);
        let fi = if auto.len() == 1 {
            fiche::ouvrir(&mut m, &auto[0], &ignores)
        } else {
            fiche::tout_corriger(&mut m, &auto)
        };
        self.fiche = Some(fi);
    }

    /// Le bouton Exécuter de la fiche (`SheetRun`)
    pub fn executer(&mut self, pc: &mut dyn Pc) {
        let Some(fi) = self.fiche.clone() else { return };
        match fi.mode.as_str() {
            "display" => self.repondre_ecran(pc, "keep"),
            "question" => self.repondre(pc, true),
            "fix" => {
                let items: Vec<Constat> = fi
                    .items
                    .iter()
                    .filter(|f| f.correction.as_ref().is_some_and(|c| c.execution.is_some()))
                    .cloned()
                    .collect();
                if items.is_empty() {
                    return;
                }
                let suite = Suite { items: items.clone(), ..Default::default() };
                if items.len() == 1
                    && let Some(q) = items[0].correction.as_ref().and_then(|c| c.confirmer.clone())
                {
                    match pc.question(&q) {
                        None => {
                            self.fiche = Some(fiche::question(&q));
                            self.attente = Some(Attente::Confirmer(Box::new(suite)));
                            return;
                        }
                        Some(false) => return,
                        Some(true) => {}
                    }
                }
                self.fiche = None;
                self.attente = None;
                self.etape_restauration(pc, suite);
            }
            _ => {}
        }
    }

    /// La réponse à une question posée à l'écran (Oui : `true`)
    pub fn repondre(&mut self, pc: &mut dyn Pc, oui: bool) {
        let Some(a) = self.attente.take() else {
            self.fiche = None;
            return;
        };
        self.fiche = None;
        match a {
            Attente::Confirmer(s) => {
                if oui {
                    self.etape_restauration(pc, *s);
                }
            }
            Attente::Restauration(s) => {
                if oui {
                    self.etape_corrections(pc, *s);
                } else {
                    pc.statut("Annulé.");
                }
            }
            Attente::RestaurationReglages { ids } => {
                if oui {
                    self.appliquer_reglages_suite(pc, &ids);
                } else {
                    pc.statut("Annulé.");
                }
            }
            Attente::ToutRestaurer | Attente::Reessayer => {
                if oui {
                    self.restaurer_tout_suite(pc);
                }
            }
            Attente::Ecran(s) => {
                self.attente = Some(Attente::Ecran(s));
            }
            Attente::AnnulerEntree { id, titre, log } => {
                if oui {
                    self.annuler_entree_suite(pc, &id, &titre, &log);
                }
            }
        }
    }

    /// `Undo-HistoryEntry` (assistance.ps1:196) : « Annuler » d'une ligne de l'historique (page Sauvegarde, signal
    /// `historique-annuler` du bloc protection). L'entrée est celle de historique.json.
    pub fn annuler_entree(&mut self, pc: &mut dyn Pc, h: &Value) {
        use nevermind_noyau::assistance::{historique_definitif, historique_note};
        let vrai = |v: &Value| v.as_bool().unwrap_or(false);
        if h.is_null() || vrai(&h["Annule"]) || historique_definitif(h) || historique_note(h) {
            return;
        }
        let texte = |k: &str| h.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let (id, titre) = (texte("Id"), texte("Titre"));
        let log = match h.get("Log") {
            Some(Value::Array(l)) => l.clone(),
            Some(o @ Value::Object(_)) => vec![o.clone()],
            _ => Vec::new(),
        };
        let q = format!(
            "Annuler ce changement ?\n\n« {titre} »\n{}\n\nLes réglages concernés reviennent à ce qu'ils étaient juste avant.",
            texte("Date")
        );
        match pc.question(&q) {
            None => {
                self.fiche = Some(fiche::question(&q));
                self.attente = Some(Attente::AnnulerEntree { id, titre, log });
            }
            Some(true) => self.annuler_entree_suite(pc, &id, &titre, &log),
            Some(false) => {}
        }
    }

    fn annuler_entree_suite(&mut self, pc: &mut dyn Pc, id: &str, titre: &str, log: &[Value]) {
        pc.statut("Retour en arrière...");
        let erreurs = {
            let mut m = self.moteur(pc);
            corrections::annuler_journal(&mut m, log)
        };
        pc.historique_annule(id);
        self.analyser(pc);
        let mut lignes = vec![format!("« {titre} » est annulé.")];
        if !erreurs.is_empty() {
            lignes.push("Pas pu être restauré :".into());
            lignes.extend(erreurs);
        }
        pc.statut("Changement annulé.");
        self.resultat(pc, "Changement annulé", &lignes, None, None);
    }

    fn etape_restauration(&mut self, pc: &mut dyn Pc, mut suite: Suite) {
        suite.avant = self.score();
        if suite.items.iter().any(|f| f.correction.as_ref().is_some_and(|c| c.restauration)) && !self.restauration_faite
        {
            let (r, q) = {
                let mut m = self.moteur(pc);
                let r = m.point_de_restauration();
                (r, m.question.take().unwrap_or_default())
            };
            match r {
                Err(e) => {
                    pc.message(&message_erreur(&e));
                    pc.statut("Une erreur est survenue.");
                    return;
                }
                Ok(None) => {
                    self.fiche = Some(fiche::question(&q));
                    self.attente = Some(Attente::Restauration(Box::new(suite)));
                    return;
                }
                Ok(Some(false)) => {
                    pc.statut("Annulé.");
                    return;
                }
                Ok(Some(true)) => self.restauration_faite = true,
            }
        }
        self.etape_corrections(pc, suite);
    }

    fn etape_corrections(&mut self, pc: &mut dyn Pc, mut suite: Suite) {
        let items = suite.items.clone();
        let mut m = self.moteur(pc);
        m.journal = Some(Vec::new());
        for f in &items {
            m.pc.statut(&format!("En cours : {}...", f.titre));
            let Some(c) = &f.correction else { continue };
            let r = executer_correction(&mut m, c.execution.as_ref(), &c.changements);
            match r {
                Ok(()) => {
                    suite.faits.push(f.clone());
                    if c.redemarrage {
                        suite.redemarrage = true;
                    }
                }
                Err(e) => {
                    m.pc.journal(&format!("Échec correction {}: {e}", f.id));
                    suite.rates.push(format!("{} : {e}", f.titre));
                }
            }
        }
        suite.journal = m.journal.take().unwrap_or_default();
        if suite.faits.len() == 1 && suite.faits[0].correction.as_ref().is_some_and(|c| c.sans_analyse) {
            let msg = suite.faits[0]
                .correction
                .as_ref()
                .and_then(|c| c.fait.clone())
                .unwrap_or_else(|| "C'est lancé.".into());
            pc.statut(&msg);
            pc.message(&msg);
            return;
        }
        // Un écran a changé de fréquence : on vérifie qu'il affiche toujours quelque chose
        if suite.journal.iter().any(|e| e["Type"] == "display") {
            match pc.choix_ecran() {
                None => {
                    self.fiche = Some(fiche::ecran(15));
                    self.attente = Some(Attente::Ecran(Box::new(suite)));
                }
                Some(c) => self.etape_fin(pc, suite, &c),
            }
            return;
        }
        self.etape_fin(pc, suite, "keep");
    }

    /// La réponse à « Tes écrans s'affichent bien ? » (keep, revert, timeout)
    pub fn repondre_ecran(&mut self, pc: &mut dyn Pc, choix: &str) {
        if let Some(Attente::Ecran(s)) = self.attente.take() {
            self.fiche = None;
            self.etape_fin(pc, *s, choix);
        }
    }

    fn etape_fin(&mut self, pc: &mut dyn Pc, mut suite: Suite, choix: &str) {
        // Pas de réponse, ou « Revenir en arrière » : l'ancienne fréquence revient, et le point est ignoré
        let mut revenus: Vec<String> = Vec::new();
        if choix != "keep" {
            // (la réponse à « Tes écrans s'affichent bien ? » passe en priorité, jamais refusée : le bloc tient le
            //  verrou des gestes pour elle)
            if let Err(e) = pc.exiger_geste() {
                pc.journal(&format!("Fréquence pas remise : {e}"));
            }
            let ecrans: Vec<Value> = suite.journal.iter().filter(|e| e["Type"] == "display").cloned().collect();
            for d in ecrans {
                let dev = crate::net::champ_texte(&d, "Device");
                pc.ecran_frequence(&dev, crate::net::champ_entier(&d, "Hz"));
                if let Some(i) = suite.journal.iter().position(|e| *e == d) {
                    suite.journal.remove(i);
                }
                revenus.push(dev.clone());
                let id = format!("display:{dev}");
                if !self.ignores.iter().any(|x| x.eq_ignore_ascii_case(&id)) {
                    self.ignores.push(id);
                }
            }
            pc.ignores_ecrire(&self.ignores);
            pc.journal(&format!("Fréquence annulée ({choix}) pour: {}", revenus.join(", ")));
        }
        self.analyser(pc);
        let apres = self.score().unwrap_or(0);
        let gardes: Vec<&Constat> = suite
            .faits
            .iter()
            .filter(|f| !(f.id.starts_with("display:") && revenus.iter().any(|d| *d == f.id[8..])))
            .collect();
        let mut lignes = Vec::new();
        let n = gardes.len();
        if n > 0 {
            let s = if n > 1 { "s" } else { "" };
            lignes.push(format!("{n} correction{s} appliquée{s} :"));
            for f in &gardes {
                lignes.push(format!("•  {}", f.titre));
            }
        }
        if !revenus.is_empty() {
            lignes.push("L'écran est revenu à son ancienne fréquence car la nouvelle ne s'affichait pas. Ce point est maintenant ignoré : il ne compte plus dans ton score.".into());
        }
        if let Some(av) = suite.avant
            && n > 0
        {
            lignes.push(format!("Score : {av} → {apres}"));
        }
        if !suite.rates.is_empty() {
            lignes.push("Non appliqué :".into());
            lignes.extend(suite.rates.iter().cloned());
        }
        if suite.redemarrage && n > 0 {
            lignes.push("Redémarre ton PC pour que tout soit pris en compte.".into());
        }
        let note = gardes.iter().any(|f| f.id == "disk-space").then_some(
            "Les fichiers supprimés par le nettoyage ne peuvent pas être récupérés. Tout le reste peut être annulé.",
        );
        let titre = if n > 0 {
            "C'est fait !"
        } else if !revenus.is_empty() {
            "Retour à l'ancien réglage"
        } else {
            "Rien n'a été appliqué"
        };
        pc.statut(titre);
        self.resultat(pc, titre, &lignes, Some(suite.journal), note);
    }

    /// `Show-ResultSheet`
    fn resultat(
        &mut self,
        pc: &mut dyn Pc,
        titre: &str,
        lignes: &[String],
        journal: Option<Vec<Value>>,
        note: Option<&str>,
    ) {
        let peut = journal.as_ref().is_some_and(|j| !j.is_empty());
        self.historique_resultat = if peut {
            pc.historique_ajouter(
                &titre_historique(titre, lignes),
                &elements_historique(lignes),
                journal.as_deref().unwrap_or(&[]),
            )
        } else {
            None
        };
        self.journal_resultat = journal;
        self.fiche = Some(fiche::resultat(titre, lignes, peut, note));
    }

    /// Le bouton du bas de la fiche (`SheetIgnore`) : Ignorer, Revenir en arrière, ou l'écran à l'ancienne fréquence
    pub fn bouton_ignorer(&mut self, pc: &mut dyn Pc) {
        let Some(fi) = self.fiche.clone() else { return };
        match fi.mode.as_str() {
            "display" => self.repondre_ecran(pc, "revert"),
            "result" => self.revenir(pc),
            "fix" => {
                if let Some(f) = fi.items.first() {
                    let ignore = !self.ignores.iter().any(|x| x.eq_ignore_ascii_case(&f.id));
                    self.ignorer(pc, f, ignore);
                }
            }
            _ => {}
        }
    }

    /// `Invoke-UndoLastRun`
    pub fn revenir(&mut self, pc: &mut dyn Pc) {
        let journal = self.journal_resultat.clone();
        let id = self.historique_resultat.clone();
        self.fiche = None;
        self.revenir_journal(pc, journal, id);
    }

    /// `Invoke-UndoLastRun` pour une fiche de résultat donnée (celle d'un autre bloc, montrée par l'hôte : action
    /// `fiche:annuler` {log, historique})
    pub fn revenir_journal(&mut self, pc: &mut dyn Pc, journal: Option<Vec<Value>>, id: Option<String>) {
        let Some(journal) = journal.filter(|j| !j.is_empty()) else { return };
        pc.statut("Retour en arrière...");
        let avant = self.score();
        let erreurs = {
            let mut m = self.moteur(pc);
            corrections::annuler_journal(&mut m, &journal)
        };
        self.journal_resultat = None;
        if let Some(id) = id {
            pc.historique_annule(&id);
        }
        self.analyser(pc);
        let mut lignes = vec!["Les réglages sont revenus exactement comme avant.".to_string()];
        if let Some(av) = avant {
            lignes.push(format!("Score : {av} → {}", self.score().unwrap_or(0)));
        }
        if !erreurs.is_empty() {
            lignes.push("Pas pu être restauré :".into());
            lignes.extend(erreurs);
        }
        pc.statut("Retour en arrière effectué.");
        self.resultat(pc, "Retour en arrière effectué", &lignes, None, None);
    }

    /// `Set-IgnoreFinding`
    pub fn ignorer(&mut self, pc: &mut dyn Pc, f: &Constat, ignore: bool) {
        if ignore {
            if !self.ignores.iter().any(|x| x.eq_ignore_ascii_case(&f.id)) {
                self.ignores.push(f.id.clone());
            }
        } else {
            self.ignores.retain(|x| !x.eq_ignore_ascii_case(&f.id));
        }
        pc.ignores_ecrire(&self.ignores);
        self.fiche = None;
        self.analyser(pc);
        pc.statut(&if ignore {
            format!("« {} » est ignoré et ne compte plus dans le score.", f.titre)
        } else {
            format!("« {} » compte de nouveau dans le score.", f.titre)
        });
    }

    /// `Invoke-UndoAll` : « Tout restaurer » (avec sa question, puis « Réessayer » si des réglages restent)
    pub fn restaurer_tout(&mut self, pc: &mut dyn Pc, definitifs: &[String]) {
        // (Get-TexteProblemeSauvegarde, onglets.ps1:870-873 : une sauvegarde abîmée ou disparue est dite, avec le point
        //  de restauration de Windows ; M9, audit du 08/11)
        let probleme = nevermind_noyau::sauvegarde::texte_probleme(self.sv.probleme.as_deref());
        if corrections::compte_total(&self.sv, pc) == 0 {
            pc.message(if probleme.is_empty() { "Il n'y a aucun changement à annuler." } else { &probleme });
            return;
        }
        let q = format!(
            "Remettre les réglages modifiés par Nevermind comme ils étaient avant ?{}{}",
            texte_definitifs(definitifs),
            if probleme.is_empty() { String::new() } else { format!("\n\nAttention : {probleme}") }
        );
        match pc.question(&q) {
            None => {
                self.fiche = Some(fiche::question(&q));
                self.attente = Some(Attente::ToutRestaurer);
            }
            Some(true) => self.restaurer_tout_suite(pc),
            Some(false) => {}
        }
    }

    fn restaurer_tout_suite(&mut self, pc: &mut dyn Pc) {
        loop {
            pc.statut("Restauration des réglages...");
            let erreurs = {
                let mut m = self.moteur(pc);
                corrections::tout_restaurer(&mut m)
            };
            pc.historique_tout_annule(&self.sv);
            if erreurs.is_empty() {
                pc.message(
                    "Les réglages ont été remis comme avant. Redémarre le PC pour que tout soit pris en compte.",
                );
                break;
            }
            let n = corrections::compte_total(&self.sv, pc);
            let mut msg = format!(
                "Certains réglages n'ont pas pu être remis comme avant :\n{}",
                erreurs.iter().take(12).map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n")
            );
            if erreurs.len() > 12 {
                msg.push_str(&format!("\n  et {} autre(s).", erreurs.len() - 12));
            }
            msg.push_str(&format!(
                "\n\nRien n'est perdu : {}tu pourras réessayer plus tard avec « Tout restaurer ». Les autres réglages sont bien remis.\n\nRéessayer maintenant ?",
                if n > 0 { format!("Nevermind garde ces {n} réglage(s) d'origine, ") } else { "Nevermind les garde, ".into() }
            ));
            match pc.question(&msg) {
                Some(true) => continue,
                Some(false) => break,
                None => {
                    self.fiche = Some(fiche::question(&msg));
                    self.attente = Some(Attente::Reessayer);
                    return;
                }
            }
        }
        self.analyser(pc);
    }

    /// `Invoke-TweakFix` (diagnostic-fps.ps1:290) : les réglages Windows demandés par la fiche d'un jeu ou d'une partie
    /// (signal `appliquer-reglages-windows` {ids} du bloc jeux), sans point de restauration, ceux déjà faits laissés
    pub fn corriger_reglages(&mut self, pc: &mut dyn Pc, ids: &[String]) {
        let build = self.analyse.as_ref().map(|a| a.build).unwrap_or(0);
        let cree = self.sv.plan_cree.clone();
        let sel: Vec<&'static reglages::Reglage> = reglages::disponibles(build)
            .into_iter()
            .filter(|t| ids.iter().any(|i| i == t.id) && !reglages::test(pc, t.id, cree.as_deref()))
            .collect();
        if sel.is_empty() {
            pc.message("C'est déjà réglé.");
            return;
        }
        let mut faits = Vec::new();
        let journal = {
            let mut m = self.moteur(pc);
            m.journal = Some(Vec::new());
            for t in &sel {
                match m.appliquer_reglage(t.id) {
                    Ok(()) => faits.push(t.titre.to_string()),
                    Err(e) => m.pc.journal(&format!("Échec {}: {e}", t.id)),
                }
            }
            m.journal.take().unwrap_or_default()
        };
        self.analyser(pc);
        let mut lignes = vec!["Réglage appliqué :".to_string()];
        lignes.extend(faits.iter().map(|d| format!("•  {d}")));
        lignes.push("Rejoue une partie : Nevermind comparera tes FPS avant / après.".into());
        self.resultat(pc, "C'est fait !", &lignes, Some(journal), None);
    }

    /// `Invoke-ApplyTweaks` : les optimisations cochées (page Optimisation gaming), avec le point de restauration si
    /// la case est cochée
    pub fn appliquer_reglages(&mut self, pc: &mut dyn Pc, ids: &[String], restauration: bool) {
        if ids.is_empty() {
            pc.message("Aucune optimisation cochée.");
            return;
        }
        if restauration {
            let (r, q) = {
                let mut m = self.moteur(pc);
                let r = m.point_de_restauration();
                (r, m.question.take().unwrap_or_default())
            };
            match r {
                Err(e) => {
                    pc.message(&message_erreur(&e));
                    pc.statut("Une erreur est survenue.");
                    return;
                }
                Ok(None) => {
                    self.fiche = Some(fiche::question(&q));
                    self.attente = Some(Attente::RestaurationReglages { ids: ids.to_vec() });
                    return;
                }
                Ok(Some(false)) => {
                    pc.statut("Annulé.");
                    return;
                }
                Ok(Some(true)) => {}
            }
        }
        self.appliquer_reglages_suite(pc, ids);
    }

    fn appliquer_reglages_suite(&mut self, pc: &mut dyn Pc, ids: &[String]) {
        let avant = self.score();
        let (mut faits, mut rates, mut redemarrage) = (Vec::new(), Vec::new(), false);
        let journal = {
            let mut m = self.moteur(pc);
            m.journal = Some(Vec::new());
            for id in ids {
                let Some(t) = reglages::reglage(id) else { continue };
                m.pc.statut(&format!("Application : {}...", t.titre));
                match m.appliquer_reglage(id) {
                    Ok(()) => {
                        faits.push(t.titre.to_string());
                        redemarrage |= t.redemarrage;
                    }
                    Err(e) => {
                        m.pc.journal(&format!("Échec {id}: {e}"));
                        rates.push(format!("{} : {e}", t.titre));
                    }
                }
            }
            m.journal.take().unwrap_or_default()
        };
        self.analyser(pc);
        let mut lignes = Vec::new();
        if !faits.is_empty() {
            let s = if faits.len() > 1 { "s" } else { "" };
            lignes.push(format!("{} optimisation{s} appliquée{s} :", faits.len()));
            for d in &faits {
                lignes.push(format!("•  {d}"));
            }
            if let Some(av) = avant {
                lignes.push(format!("Score : {av} → {}", self.score().unwrap_or(0)));
            }
        }
        if !rates.is_empty() {
            lignes.push("Non appliqué :".into());
            lignes.extend(rates);
        }
        if redemarrage && !faits.is_empty() {
            lignes.push("Redémarre ton PC pour que tout soit pris en compte.".into());
        }
        let titre = if faits.is_empty() { "Rien n'a été appliqué" } else { "C'est fait !" };
        pc.statut(titre);
        self.resultat(pc, titre, &lignes, Some(journal), None);
    }

    /// `Set-StartupToggle` : un programme au démarrage activé ou désactivé (noté dans l'historique)
    pub fn demarrage(&mut self, pc: &mut dyn Pc, d: &Demarrage, nom: &str, activer: bool) -> Result<(), String> {
        let journal = {
            let mut m = self.moteur(pc);
            m.journal = Some(Vec::new());
            let r = m.appliquer(&[reglages::changement_demarrage(d, activer)]);
            let j = m.journal.take().unwrap_or_default();
            r.map(|_| j)
        }?;
        pc.historique_ajouter(
            &if activer {
                format!("Démarrage : « {nom} » réactivé")
            } else {
                format!("Démarrage : « {nom} » ne se lance plus")
            },
            &[],
            &journal,
        );
        pc.statut(&if activer {
            format!("« {nom} » se lancera de nouveau quand tu allumes ton PC.")
        } else {
            format!("« {nom} » ne se lancera plus quand tu allumes ton PC.")
        });
        Ok(())
    }

    /// Des changements déclarés par un autre bloc (profil d'un jeu, `Set-GameProfile`), notés dans l'historique
    pub fn declarations(
        &mut self,
        pc: &mut dyn Pc,
        l: &[crate::modele::Declaration],
        titre: &str,
    ) -> Result<(), String> {
        let journal = {
            let mut m = self.moteur(pc);
            m.journal = Some(Vec::new());
            let r = m.appliquer(l);
            let j = m.journal.take().unwrap_or_default();
            r.map(|_| j)
        }?;
        pc.historique_ajouter(titre, &[], &journal);
        Ok(())
    }

    /// `Invoke-Clean` : les catégories cochées (page Nettoyage)
    pub fn nettoyer(&mut self, pc: &mut dyn Pc, ids: &[String]) {
        let cibles: Vec<nettoyage::Cible> =
            nettoyage::cibles(pc).into_iter().filter(|c| ids.iter().any(|i| i == c.id)).collect();
        if cibles.is_empty() {
            pc.message("Clique d'abord sur Analyser, puis active ce que tu veux nettoyer.");
            return;
        }
        let r = nettoyage::nettoyer(pc, &cibles);
        pc.statut(&format!("{} libérés.", crate::net::taille(r.libere)));
        pc.message(&texte_nettoyage(&r));
    }
}

/// Le message après un nettoyage (`Invoke-Clean`)
pub fn texte_nettoyage(r: &nettoyage::Total) -> String {
    let p = |n: i64| if n > 1 { "s" } else { "" };
    let mut m = format!(
        "{} libérés : {} fichier{} supprimé{}",
        crate::net::taille(r.libere),
        r.supprimes,
        p(r.supprimes),
        p(r.supprimes)
    );
    if r.laisses != 0 {
        m.push_str(&format!(
            ", {} laissé{} car utilisé{} par un programme (c'est normal)",
            r.laisses,
            p(r.laisses),
            p(r.laisses)
        ));
    }
    if r.refuses != 0 {
        m.push_str(&format!(", {} refusé{} par Windows", r.refuses, p(r.refuses)));
    }
    m.push_str(".\n\nLe détail fichier par fichier est dans le journal (bouton « Ouvrir le journal » sous la liste).");
    m
}

/// Les protections du PC qui restent en place (« Tout restaurer » ne les annule pas)
pub fn texte_definitifs(d: &[String]) -> String {
    if d.is_empty() {
        return String::new();
    }
    format!(
        "\n\nCes protections du PC restent en place (elles ne s'annulent pas, l'historique dit comment revenir en arrière à la main) :\n{}",
        d.iter().map(|x| format!("  - {x}")).collect::<Vec<_>>().join("\n")
    )
}

/// Ce que fait le bouton Exécuter d'une correction (`& $f.Fix.Run $f.Fix.Args`)
pub fn executer_correction(
    m: &mut Moteur,
    ex: Option<&Execution>,
    changements: &crate::modele::Changements,
) -> Result<(), String> {
    match ex {
        None => Ok(()),
        Some(Execution::Declarations) => {
            let l = m.resoudre(changements)?;
            m.appliquer(&l)
        }
        Some(Execution::Reglage(id)) => m.appliquer_reglage(id),
        Some(Execution::Nettoyage) => {
            let c = nettoyage::cibles_en_un_clic(m.pc);
            nettoyage::nettoyer(m.pc, &c);
            Ok(())
        }
        Some(Execution::TestMemoire) => {
            let a = ["mdsched.exe".to_string()];
            m.pc.lancer(&a).map_err(|e| message_lancement(&a, &e))
        }
        Some(Execution::Bios) => {
            m.pc.exiger_geste()?;
            let code = m.pc.commande("shutdown.exe", &["/r".into(), "/fw".into(), "/t".into(), "10".into()]);
            if code != 0 {
                return Err(format!(
                    "Ce PC ne permet pas de redémarrer directement dans le BIOS (code {code}). Redémarre et appuie sur Suppr ou F2 pendant le démarrage."
                ));
            }
            Ok(())
        }
    }
}

/// Le résumé de l'analyse pour les autres blocs (tuile « Ton PC » de l'accueil, Mindset)
pub fn resume(a: Option<&Analyse>) -> Value {
    let (i, m) = crate::score::a_regler(a);
    match a {
        Some(a) => {
            json!({ "score": a.score, "potentiel": a.potentiel, "appreciation": a.appreciation, "aRegler": i.len() + m.len() })
        }
        None => json!({ "score": null }),
    }
}
