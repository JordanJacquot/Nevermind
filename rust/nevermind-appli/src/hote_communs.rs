//! Les signaux communs que l'hôte exécute pour les blocs (D:\Projets\_plans\signaux-communs-nevermind-rust.md) :
//!
//! - `question` `{ id, titre, texte, boutons, details?, action }` : la question du kit (`Read-QuestionVue`,
//!   `Confirm-Action`) ; la réponse revient au bloc par l'action `action`, valeur `{ id, valeur }` ; une question qui
//!   ne peut pas être posée (écran pas prêt ou pas regardé), ou abandonnée (la page s'est rechargée), reçoit le
//!   dernier bouton (vue.ps1:336) ;
//! - `occupe` `{ texte }` : la pastille « en cours » (`Set-OccupeVue`) ;
//! - `resultat` `{ titre, lignes, log?, note? }` : la fiche de résultat (`Show-ResultSheet`, tableau-de-bord.ps1:514),
//!   montrée comme `Get-FicheKit` la lit (ecran-pc.ps1:165) et suivie comme `Update-FicheKit` ; « OK » la ferme,
//!   « Revenir en arrière » la ferme et donne l'action `fiche:annuler` `{ log, historique }` aux blocs
//!   (`Invoke-UndoLastRun`, que fait le bloc qui sait remettre les réglages) ;
//! - `historique` `{ titre, details?, log? }` : une ligne de historique.json (`Add-History`, par le contexte du noyau,
//!   qui garde l'historique de tous les blocs), puis le signal `sauvegarde-changee` aux blocs (`Update-HistoryList`) ;
//! - `sauvegarde-changee` : l'écran refait, et le signal passé aux autres blocs (hote.rs).

use serde_json::{Value, json};

use super::{Ecran, Hote, id_question};

/// Une question d'un bloc posée à l'écran.
#[derive(Debug, Clone)]
pub struct QuestionBloc {
    /// L'id de la question dans le kit (choisi par l'hôte : deux blocs ne se gênent pas)
    kit: String,
    /// L'id donné par le bloc
    id: String,
    action: String,
    dernier: String,
}

/// La fiche de résultat ouverte.
#[derive(Debug, Default)]
pub struct Fiche {
    /// La page « fiche » montrée (`$script:Kit.Fiche`)
    pub page: Option<Value>,
    log: Option<Value>,
    historique: Option<String>,
    /// Une nouvelle fiche à montrer (`FicheNumero` changé)
    nouvelle: bool,
    ouverte: bool,
    de: Option<String>,
    montree: bool,
}

/// Un champ, sans tenir compte des majuscules (`Titre` du bloc trafic, `titre` des autres)
fn champ<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    let o = v.as_object()?;
    o.get(k).or_else(|| o.iter().find(|(x, _)| x.eq_ignore_ascii_case(k)).map(|(_, y)| y))
}
fn texte(v: &Value, k: &str) -> String {
    champ(v, k).and_then(Value::as_str).unwrap_or("").to_string()
}
fn lignes(v: &Value, k: &str) -> Vec<String> {
    match champ(v, k) {
        Some(Value::Array(l)) => l.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}
/// `$Log -and $Log.Count` : une liste non vide.
fn log_non_vide(v: Option<&Value>) -> Option<Value> {
    match v {
        Some(Value::Array(l)) if !l.is_empty() => Some(Value::Array(l.clone())),
        Some(o @ Value::Object(_)) => Some(json!([o])),
        _ => None,
    }
}

use nevermind_noyau::assistance::{elements_historique, titre_historique};

/// La fiche de résultat telle que `Get-FicheKit` lit celle de `Show-ResultSheet`.
pub fn page_resultat(titre: &str, l: &[String], note: &str, annulable: bool) -> Value {
    let mut blocs: Vec<Value> = Vec::new();
    let mut liste: Vec<String> = Vec::new();
    let mut sorte = "";
    let finir = |blocs: &mut Vec<Value>, liste: &mut Vec<String>, sorte: &str| {
        if !liste.is_empty() {
            blocs.push(if sorte == "etapes" {
                json!({ "type": "etapes", "elements": liste.iter().map(|t| json!({ "texte": t })).collect::<Vec<_>>() })
            } else {
                json!({ "type": "liste", "elements": liste })
            });
        }
        liste.clear();
    };
    for t in l {
        // `^\d+\.\s+(.*)$` et `^•\s+(.*)$`
        let etape = t.find('.').filter(|&i| i > 0 && t[..i].bytes().all(|b| b.is_ascii_digit())).and_then(|i| {
            let r = &t[i + 1..];
            (r.starts_with(char::is_whitespace)).then(|| r.trim_start().to_string())
        });
        let puce =
            t.strip_prefix('•').filter(|r| r.starts_with(char::is_whitespace)).map(|r| r.trim_start().to_string());
        if let Some(e) = etape {
            if sorte != "etapes" {
                finir(&mut blocs, &mut liste, sorte);
                sorte = "etapes";
            }
            liste.push(e);
        } else if let Some(p) = puce {
            if sorte != "liste" {
                finir(&mut blocs, &mut liste, sorte);
                sorte = "liste";
            }
            liste.push(p);
        } else {
            finir(&mut blocs, &mut liste, sorte);
            blocs.push(json!({ "type": "texte", "texte": t }));
        }
    }
    finir(&mut blocs, &mut liste, sorte);
    if !note.is_empty() {
        blocs.push(json!({ "type": "note", "texte": note }));
    }
    if annulable {
        blocs.push(json!({ "type": "note", "texte": "Si quelque chose ne va pas, « Revenir en arrière » annule exactement ces changements." }));
        blocs.push(json!({ "type": "bouton", "id": "fiche:ignore", "texte": "Revenir en arrière", "style": "second" }));
    }
    blocs.push(json!({ "type": "bouton", "id": "fiche:close", "texte": "OK", "style": "second" }));
    json!({
        "titre": if titre.is_empty() { "Nevermind" } else { titre }, "sous": "", "icone": "ok", "couleur": "vert",
        "retour": "appli", "parent": null,
        "module": { "icone": "ok", "couleur": "vert", "titre": "", "texte": "", "boutons": [] },
        "blocs": blocs,
    })
}

impl Hote {
    /// Un signal commun d'un bloc, exécuté ici. `true` : l'écran est à refaire.
    pub(super) fn signal_commun(&mut self, nom: &str, d: &Value, ecran: &mut dyn Ecran) -> bool {
        match nom {
            "occupe" => {
                self.ordre(json!({ "kit": "occupe", "texte": texte(d, "texte") }), ecran);
                false
            }
            "question" => {
                self.poser_question(d, ecran);
                false
            }
            "historique" => {
                let log = log_non_vide(d.get("log")).and_then(|l| l.as_array().cloned()).unwrap_or_default();
                if self.ctx.ajouter_historique(&texte(d, "titre"), &lignes(d, "details"), log).is_some() {
                    self.a_signaler_sauvegarde();
                }
                true
            }
            "resultat" => {
                let (t, l) = (texte(d, "titre"), lignes(d, "lignes"));
                let log = log_non_vide(d.get("log"));
                let liste = log.as_ref().and_then(|l| l.as_array().cloned()).unwrap_or_default();
                let hist = self.ctx.ajouter_historique(&titre_historique(&t, &l), &elements_historique(&l), liste);
                if hist.is_some() {
                    self.a_signaler_sauvegarde();
                }
                self.fiche.page = Some(page_resultat(&t, &l, &texte(d, "note"), log.is_some()));
                self.fiche.log = log;
                self.fiche.historique = hist;
                self.fiche.nouvelle = true;
                true
            }
            "sauvegarde-appliquer" => {
                self.appliquer_sauvegarde(d, ecran);
                true
            }
            "notification" | "bulle" => {
                self.notifier(d);
                false
            }
            "veille-jeu-ajouter" => {
                let p = texte(d, "Processus");
                if !p.is_empty() {
                    self.ctx.ajouter_fps_veille(&p);
                }
                false
            }
            _ => false,
        }
    }

    /// `sauvegarde-appliquer` `{ Titre, Details, Ecritures, Message }` (envoyé par le bloc trafic) : chaque écriture
    /// passe par la sauvegarde du noyau (`Set-Reg` : origine notée, changement noté, l'Aide pour un réglage du PC ;
    /// `Set-ServiceOff` : origine dans SvcOriginal, changement noté, `service.regler` par l'Aide), puis une ligne
    /// d'historique (`Add-History`) : c'est ce qui permet « Revenir en arrière » (microsoft.ps1:158-168, lag.ps1:497).
    fn appliquer_sauvegarde(&mut self, d: &Value, ecran: &mut dyn Ecran) {
        self.ctx.commencer_changements();
        let mut erreurs = Vec::new();
        for e in d.get("Ecritures").and_then(Value::as_array).cloned().unwrap_or_default() {
            let r = match texte(&e, "Type").as_str() {
                "reg" => {
                    let kind = texte(&e, "Kind");
                    self.ctx.changer_registre(
                        &texte(&e, "Path"),
                        &texte(&e, "Name"),
                        e.get("Value").unwrap_or(&Value::Null),
                        if kind.is_empty() { "DWord" } else { &kind },
                    )
                }
                "svc" => self.couper_service(&texte(&e, "Name"), &texte(&e, "Demarrage")),
                t => Err(format!("écriture inconnue ({t})")),
            };
            if let Err(x) = r {
                erreurs.push(format!("{} : {x}", texte(&e, "Name")));
            }
        }
        let log = self.ctx.finir_changements();
        if self.ctx.ajouter_historique(&texte(d, "Titre"), &lignes(d, "Details"), log).is_some() {
            self.a_signaler_sauvegarde();
        }
        if !erreurs.is_empty() {
            let t = format!("Certains réglages n'ont pas pu être changés :\n\n{}", erreurs.join("\n"));
            self.ordre(json!({ "kit": "message", "texte": t, "bravo": false }), ecran);
        }
        let m = texte(d, "Message");
        if !m.is_empty() {
            self.ordre(json!({ "kit": "message", "texte": m, "bravo": false }), ecran);
        }
    }

    /// `Set-ServiceOff` (microsoft.ps1:68-75).
    fn couper_service(&mut self, nom: &str, demarrage: &str) -> Result<(), String> {
        let demarrage = if demarrage.is_empty() { "Disabled" } else { demarrage };
        let (debut, tourne) = (self.etat_service)(nom).ok_or_else(|| format!("service {nom} introuvable"))?;
        let orig: Vec<Value> = match self.ctx.reglage("SvcOriginal") {
            Some(Value::Array(l)) => l,
            Some(Value::String(s)) => vec![Value::String(s)],
            _ => Vec::new(),
        };
        let deja = orig.iter().any(|x| x.as_str().and_then(|s| s.split('|').next()) == Some(nom));
        if !deja {
            let mut n = orig.clone();
            n.push(json!(format!("{nom}|{debut}|{}", if tourne { "True" } else { "False" })));
            self.ctx.ecrire_reglage("SvcOriginal", Value::Array(n), true)?;
        }
        self.ctx.noter_changement(
            json!({ "Type": "svc", "Name": nom, "StartType": debut, "Running": tourne, "Nouveau": demarrage }),
        );
        self.ctx
            .aide("service.regler", &json!({ "Nom": nom, "Demarrage": demarrage, "Arreter": tourne }), 60_000)
            .map(|_| ())
    }

    /// `notification` (`Show-Notify`) : `{ Titre, Texte, Action, Valeur, Ecran }` (bloc trafic) ou
    /// `{ titre, texte, action }` (bloc perf), ou `bulle` `{ titre, texte, ecran }` (bloc réseau) ; la bulle part vers Mindset (notification.demande) ; son clic
    /// (`notif:<id>` dans afficher.demande) fait l'action et mène à l'écran.
    fn notifier(&mut self, d: &Value) {
        let id = self.ctx.notifier(&texte(d, "Titre"), &texte(d, "Texte"));
        let valeur = champ(d, "Valeur").cloned().unwrap_or(Value::Null);
        self.notifications.push((id, texte(d, "Action"), valeur, texte(d, "Ecran")));
        while self.notifications.len() > 50 {
            self.notifications.remove(0);
        }
    }

    /// Le clic sur une bulle (`notif:<id>`) : son action donnée aux blocs, puis son écran. `false` si elle est
    /// inconnue (la fenêtre est seulement montrée).
    pub fn bulle_cliquee(&mut self, id: &str, maintenant: std::time::Duration, ecran: &mut dyn Ecran) -> bool {
        let Some(i) = self.notifications.iter().position(|n| n.0 == id) else { return false };
        let (_, action, valeur, vers) = self.notifications.remove(i);
        if !action.is_empty() {
            self.a_router.push((action, valeur));
        }
        if !vers.is_empty() {
            if self.k.pret {
                self.aller(&vers, ecran);
            } else {
                self.k.retour_ecran = Some(vers);
            }
        }
        self.vider_reponses(maintenant, ecran);
        true
    }

    /// Après une ligne d'historique : les blocs le savent (le signal part au prochain passage, avec les réponses).
    fn a_signaler_sauvegarde(&mut self) {
        self.a_router.push(("\u{0}sauvegarde-changee".into(), json!({})));
    }

    fn poser_question(&mut self, d: &Value, ecran: &mut dyn Ecran) {
        let boutons = match d.get("boutons") {
            // (des textes simples, comme `signaux::question` du noyau : le texte sert aussi d'id)
            Some(Value::Array(b)) if !b.is_empty() => b
                .iter()
                .map(|x| match x {
                    Value::String(t) => json!({ "id": t, "texte": t }),
                    o => o.clone(),
                })
                .collect(),
            _ => {
                vec![json!({ "id": "oui", "texte": "Oui" }), json!({ "id": "non", "texte": "Non", "style": "second" })]
            }
        };
        let dernier = boutons.last().and_then(|b| b.get("id")).and_then(Value::as_str).unwrap_or("").to_string();
        let action = texte(d, "action");
        let id_bloc = d.get("id").cloned().unwrap_or(Value::Null);
        let q = QuestionBloc {
            kit: format!("q{}", id_question()),
            id: id_bloc.as_str().unwrap_or("").to_string(),
            action,
            dernier,
        };
        if !self.k.pret || !self.k.regardee || !crate::pont::id_valable(&q.action) {
            self.ctx.journal(&format!(
                "Question : pas posée à l'écran ({})",
                if !self.k.pret {
                    "écran pas prêt"
                } else if !self.k.regardee {
                    "écran caché"
                } else {
                    "action mal écrite"
                }
            ));
            self.donner_reponse(&q, &q.dernier.clone());
            return;
        }
        self.ctx.journal("Question : posée à l'écran");
        let mut o = json!({ "kit": "question", "id": q.kit, "titre": texte(d, "titre"), "texte": texte(d, "texte"), "boutons": boutons });
        match d.get("details") {
            Some(Value::Array(l)) if l.iter().any(|x| x.as_str().is_some_and(|s| !s.is_empty())) => {
                o["details"] = json!({ "titre": "Détails techniques", "lignes": l });
            }
            Some(x @ Value::Object(_)) => o["details"] = x.clone(),
            _ => {}
        }
        self.questions.push(q);
        self.ordre(o, ecran);
    }

    fn donner_reponse(&mut self, q: &QuestionBloc, valeur: &str) {
        if !q.action.is_empty() {
            self.a_router.push((q.action.clone(), json!({ "id": q.id, "valeur": valeur })));
        }
    }

    /// La réponse du kit à une question d'un bloc.
    pub(super) fn repondre(&mut self, id: &str, valeur: &str) {
        if let Some(i) = self.questions.iter().position(|q| q.kit == id) {
            let q = self.questions.remove(i);
            self.ctx.journal(&format!("Question : réponse « {valeur} »"));
            self.donner_reponse(&q, valeur);
        }
    }

    /// La page s'est rechargée : les questions posées sont abandonnées (le dernier bouton, vue.ps1:335-336).
    pub(super) fn abandonner_questions(&mut self) {
        for q in std::mem::take(&mut self.questions) {
            self.ctx.journal("Question : abandonnée (l'écran a disparu)");
            let d = q.dernier.clone();
            self.donner_reponse(&q, &d);
        }
    }

    /// Les réponses et signaux en attente, donnés aux blocs.
    pub(super) fn vider_reponses(&mut self, maintenant: std::time::Duration, ecran: &mut dyn Ecran) {
        for _ in 0..16 {
            if self.a_router.is_empty() {
                return;
            }
            let mut refaire = false;
            for (id, v) in std::mem::take(&mut self.a_router) {
                if let Some(nom) = id.strip_prefix('\u{0}') {
                    refaire |= self.signaler(None, nom, &v, maintenant, ecran, 0);
                } else if !self.router(&id, &v, maintenant, ecran) {
                    self.ctx.journal(&format!("Écran : action sans suite ({id})"));
                }
            }
            if refaire {
                self.mettre_a_jour(maintenant, ecran);
            }
        }
    }

    /// `Update-FicheKit` (ecran-pc.ps1:244-264) pour la fiche de résultat : rend l'écran où aller.
    pub(super) fn suivre_fiche(&mut self) -> Option<String> {
        let f = &mut self.fiche;
        if let Some(p) = f.page.as_mut() {
            let nouvelle = std::mem::take(&mut f.nouvelle);
            let aller = !f.ouverte || (nouvelle && self.k.ecran != "fiche");
            if aller {
                f.ouverte = true;
                f.de = Some(if self.k.ecran.is_empty() { "accueil".into() } else { self.k.ecran.clone() });
            }
            let parent = f.de.clone().filter(|d| d != "accueil" && d != "fiche");
            p["parent"] = parent.map_or(Value::Null, Value::String);
            return aller.then(|| "fiche".to_string());
        }
        if !f.ouverte {
            return None;
        }
        f.ouverte = false;
        f.montree = false;
        (self.k.ecran == "fiche").then(|| f.de.clone().filter(|d| d != "fiche").unwrap_or_else(|| "accueil".into()))
    }

    /// « OK » ou « Revenir en arrière » de la fiche de résultat (`Close-Sheet`, `Invoke-UndoLastRun`).
    pub(super) fn bouton_fiche(&mut self, id: &str) {
        let log = self.fiche.log.take();
        let hist = self.fiche.historique.take();
        self.fiche.page = None;
        if id == "fiche:ignore"
            && let Some(log) = log
        {
            self.a_router.push(("fiche:annuler".into(), json!({ "log": log, "historique": hist })));
        }
    }

    /// `Close-QuitteKit` (ecran-panneau.ps1:249-256) : la fiche quittée par une autre page (pas le panneau) est fermée.
    pub(super) fn fiche_quittee(&mut self, id: &str) -> bool {
        if id == "fiche" {
            self.fiche.montree = true;
            return false;
        }
        if id == "panneau" || !self.fiche.montree {
            return false;
        }
        self.fiche.montree = false;
        if self.fiche.page.is_some() {
            self.ctx.journal(&format!("Écran : fiche quittée par la page {id}"));
            self.bouton_fiche("fiche:close");
            return true;
        }
        false
    }

    /// Les questions en attente (pour les tests).
    #[cfg(test)]
    pub(super) fn questions_en_attente(&self) -> usize {
        self.questions.len()
    }
}
