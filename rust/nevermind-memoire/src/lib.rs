//! Nevermind en Rust : le bloc « memoire », la page RAM de Performances (« Qu'est-ce qui prend ma mémoire ? »,
//! memoire.ps1 et la page `memoire` de ecran-perf.ps1).
//!
//! La mémoire regroupée par appli (les « Hôte de service » rattachés à leurs services), une étiquette pour chacune,
//! la part cachée, les alertes des pilotes et du matériel, les corrections sûres (services passés en « manuel », par
//! l'Aide), le rapport sur le Bureau, et le bouton « Fermer » (la règle « fermable » d'aide-regles, la même que celle
//! de l'Aide et de NevermindPont.dll).
//!
//! Ce dont le bloc a besoin de l'hôte (pas encore dans `Effet`, voir le rapport) passe par `Effet::Signal` :
//! - envoyés : « question » (une question à l'écran, réponse par l'action « memoire-reponse »), « occupe » (la pastille
//!   « en cours »), « resultat » (la fiche de résultat avec « Revenir en arrière »), « historique » (une note de
//!   l'historique), « sauvegarde-changee », « memoire-constat » (pour la tuile RAM de la page Performances) ;
//! - attendus : « demarrage-programmes », « jeux-bibliotheque », « partie-commencee », « partie-finie ».

pub mod constat;
pub mod donnees;
pub mod fermeture;
pub mod listes;
pub mod page;
pub mod pc;
pub mod rapport;
pub mod texte;
#[cfg(windows)]
pub mod windows;

use constat::{Appli, Cible, Constat};
use donnees::{Donnees, Jeux, Pilote};
use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use pc::{Aide, AideDuContexte, Lecture, Pc};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use texte::taille;

/// Une lecture en tâche de fond
struct Travail {
    fil: JoinHandle<Resultat>,
    debut: Instant,
    etape: String,
}

/// Ce que rend une lecture (`$TravailMemoire`)
enum Resultat {
    Donnees(Box<Lecture>),
    Pilotes(HashMap<String, Vec<Pilote>>),
}

/// Une appli en train de se fermer (`$script:MemoireFermeture`)
struct Fermeture {
    nom: String,
    chemins: Vec<String>,
    cibles: Vec<Cible>,
    mode: String,
    /// « travail », « attente » ou « question » (on attend la réponse à « Forcer la fermeture ? »)
    etape: String,
    faits: Vec<Cible>,
    refuses: Vec<fermeture::Refuse>,
    fil: Option<JoinHandle<Result<fermeture::Resultat, String>>>,
    /// `None` : rien n'a été envoyé, pas d'attente (`[datetime]::MinValue`)
    debut: Option<Instant>,
    restants: Vec<Cible>,
}

/// Une question posée à l'écran, en attente de sa réponse
enum Question {
    Couper,
    Fermer(Box<Appli>),
    Forcer,
    Info,
}

pub struct BlocMemoire {
    pc: Option<Arc<dyn Pc>>,
    aide: Arc<dyn Aide>,
    /// Le dernier constat (`$script:Kit.Memoire`) et ses données
    constat: Option<Constat>,
    donnees: Option<Donnees>,
    /// « donnees », « pilotes » ou vide (`$script:Kit.MemoireEnCours`)
    en_cours: String,
    travail: Option<Travail>,
    /// Ce qui attend la fin de la lecture : « rapport », « relire »
    apres: Vec<String>,
    /// Les pilotes déjà retrouvés pour une étiquette (la recherche n'est faite qu'une fois par séance)
    pilotes_connus: HashMap<String, Vec<Pilote>>,
    fermeture: Option<Fermeture>,
    questions: HashMap<String, Question>,
    n_question: u64,
    /// Les programmes de la page Démarrage (signal « demarrage-programmes »)
    demarrage: Vec<String>,
    /// Les programmes des jeux de la bibliothèque (signal « jeux-bibliotheque »)
    jeux_exes: Vec<String>,
    /// Le jeu en cours (signal « partie-commencee »), 0 sinon
    partie: i64,
    /// Les effets demandés hors d'un appel qui en reçoit (rendus au prochain appel)
    en_attente: Vec<Effet>,
}

impl Default for BlocMemoire {
    fn default() -> Self {
        Self {
            pc: None,
            aide: Arc::new(AideDuContexte),
            constat: None,
            donnees: None,
            en_cours: String::new(),
            travail: None,
            apres: Vec::new(),
            pilotes_connus: HashMap::new(),
            fermeture: None,
            questions: HashMap::new(),
            n_question: 0,
            demarrage: Vec::new(),
            jeux_exes: Vec::new(),
            partie: 0,
            en_attente: Vec::new(),
        }
    }
}

/// Le vrai PC (lectures de Windows)
fn pc_natif() -> Arc<dyn Pc> {
    #[cfg(windows)]
    {
        Arc::new(windows::PcWindows)
    }
    #[cfg(not(windows))]
    {
        Arc::new(pc::PcAbsent)
    }
}

impl BlocMemoire {
    /// Un bloc sur un PC et une Aide donnés (les tests : un faux PC, une fausse Aide)
    pub fn avec(pc: Arc<dyn Pc>, aide: Arc<dyn Aide>) -> Self {
        Self { pc: Some(pc), aide, ..Default::default() }
    }

    fn pc(&mut self) -> Arc<dyn Pc> {
        self.pc.get_or_insert_with(pc_natif).clone()
    }

    /// Le constat montré (pour les tests et l'hôte)
    pub fn constat(&self) -> Option<&Constat> {
        self.constat.as_ref()
    }

    /// Une lecture est en cours (`$script:MemoireJob`)
    pub fn lecture_en_cours(&self) -> bool {
        self.travail.is_some()
    }

    /// Une appli est en train de se fermer
    pub fn fermeture_en_cours(&self) -> bool {
        self.fermeture.is_some()
    }

    /// Essais : fait comme si `secondes` étaient passées depuis le début de l'attente de la fermeture
    #[doc(hidden)]
    pub fn vieillir_fermeture(&mut self, secondes: u64) {
        if let Some(f) = &mut self.fermeture
            && let Some(d) = f.debut
        {
            f.debut = d.checked_sub(Duration::from_secs(secondes)).or(Some(d));
        }
    }

    /// Essais : attend que le travail de fond (lecture ou fermeture) soit fini
    #[doc(hidden)]
    pub fn attendre_fond(&self) {
        let fini = || {
            self.travail.as_ref().is_none_or(|t| t.fil.is_finished())
                && self.fermeture.as_ref().and_then(|f| f.fil.as_ref()).is_none_or(|f| f.is_finished())
        };
        let limite = Instant::now() + Duration::from_secs(30);
        while !fini() && Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn occupe(effets: &mut Effets, texte: &str) {
        effets.ajouter(Effet::Signal { nom: "occupe".into(), donnees: json!({ "texte": texte }) });
    }

    fn message(effets: &mut Effets, texte: &str) {
        effets.ajouter(Effet::Annonce(texte.into()));
    }

    /// Une question à l'écran (`Read-QuestionVue`) : la réponse revient par l'action « memoire-reponse »
    fn question(&mut self, effets: &mut Effets, q: Question, texte: &str, boutons: Value, details: &[String]) {
        self.n_question += 1;
        let id = format!("memoire-{}", self.n_question);
        let mut d = json!({ "id": id, "titre": "", "texte": texte, "boutons": boutons, "action": "memoire-reponse" });
        if !details.is_empty() {
            d["details"] = json!({ "titre": "Détails techniques", "lignes": details });
        }
        self.questions.insert(id, q);
        effets.ajouter(Effet::Signal { nom: "question".into(), donnees: d });
    }

    fn oui_non() -> Value {
        json!([{ "id": "oui", "texte": "Oui" }, { "id": "non", "texte": "Non", "style": "second" }])
    }

    // -----------------------------------------------------------------------
    // La lecture (Start-ConstatMemoire, Update-ConstatMemoireFond, Receive-ResultatMemoire, Complete-ConstatMemoire)
    // -----------------------------------------------------------------------

    /// Lance la lecture (ou la recherche du pilote). `ensuite` : ce qu'il faut faire quand tout est fini (« rapport »).
    fn lancer(&mut self, ctx: &Contexte, effets: &mut Effets, ensuite: &str, etape: &str, tags: Vec<String>) {
        if !ensuite.is_empty() && !self.apres.iter().any(|x| x == ensuite) {
            self.apres.push(ensuite.into());
        }
        if self.travail.is_some() || self.fermeture.is_some() {
            return; // (déjà en cours : le résultat servira aussi à ce geste ; une appli se ferme : relue après)
        }
        let pc = self.pc();
        let fil = if etape == "pilotes" {
            std::thread::Builder::new()
                .name("memoire-pilotes".into())
                .spawn(move || Resultat::Pilotes(pc.chercher_pilotes(&tags)))
        } else {
            let dem = self.demarrage.clone();
            let connus = self.pilotes_connus.clone();
            std::thread::Builder::new()
                .name("memoire-lecture".into())
                .spawn(move || Resultat::Donnees(Box::new(pc.lire(&dem, &connus))))
        };
        match fil {
            Ok(fil) => {
                self.travail = Some(Travail { fil, debut: Instant::now(), etape: etape.into() });
                self.en_cours = etape.into();
                Self::occupe(
                    effets,
                    if etape == "pilotes" {
                        "Je cherche quel pilote garde la mémoire..."
                    } else {
                        "Je regarde ce qui prend la mémoire..."
                    },
                );
            }
            Err(e) => {
                ctx.journal(&format!("Mémoire : {e}"));
                self.en_cours.clear();
                self.apres.clear();
            }
        }
    }

    /// Relève le résultat de la lecture quand il est prêt (toutes les 300 ms)
    fn relever(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let Some(t) = &self.travail else { return };
        if !t.fil.is_finished() {
            if t.debut.elapsed() < Duration::from_secs(90) {
                return;
            }
            let etape = t.etape.clone();
            self.travail = None; // (le fil est abandonné : il finira seul)
            ctx.journal(&format!("Mémoire : lecture abandonnée au bout de 90 s ({etape})"));
            self.finir(ctx, effets);
            return;
        }
        let Some(t) = self.travail.take() else { return };
        match t.fil.join() {
            Ok(r) => self.recevoir(ctx, effets, Some(r)),
            Err(_) => self.recevoir(ctx, effets, None),
        }
    }

    /// Le résultat d'une étape (`Receive-ResultatMemoire`)
    fn recevoir(&mut self, ctx: &Contexte, effets: &mut Effets, r: Option<Resultat>) {
        match r {
            None => {
                ctx.journal("Mémoire : lecture sans résultat");
                self.finir(ctx, effets);
            }
            Some(Resultat::Pilotes(p)) => {
                for (k, v) in p {
                    self.pilotes_connus.insert(k, v);
                }
                if let Some(d) = &mut self.donnees {
                    for e in &mut d.etiquettes {
                        if let Some(p) = self.pilotes_connus.get(&e.tag) {
                            e.pilotes = p.clone();
                        }
                    }
                    self.constat = Some(constat::constat(d));
                }
                self.finir(ctx, effets);
            }
            Some(Resultat::Donnees(l)) => {
                for x in &l.journal {
                    ctx.journal(x);
                }
                if let Some(e) = &l.erreur {
                    ctx.journal(&format!("Mémoire, donnees : {e}"));
                }
                let Some(mut d) = l.donnees else {
                    self.finir(ctx, effets);
                    return;
                };
                d.jeux = Some(self.jeux(ctx));
                let c = constat::constat(&d);
                ctx.journal(&format!(
                    "Mémoire : {} utilisés sur {}, {} applis, {} correction(s) sûre(s), {} alerte(s)",
                    taille(c.utilisee),
                    taille(c.visible),
                    c.applis.len(),
                    c.corrections.len(),
                    c.alertes.len()
                ));
                let tags: Vec<String> = d
                    .etiquettes
                    .iter()
                    .filter(|e| !self.pilotes_connus.contains_key(&e.tag))
                    .map(|e| e.tag.clone())
                    .collect();
                self.donnees = Some(d);
                self.constat = Some(c);
                self.signaler_constat(effets);
                // Les étiquettes dont le pilote n'est pas encore connu : leur recherche, à part
                if !tags.is_empty() {
                    effets.ajouter(Effet::Rafraichir); // (le constat s'affiche déjà)
                    self.lancer(ctx, effets, "", "pilotes", tags);
                    return;
                }
                self.finir(ctx, effets);
            }
        }
    }

    /// La tuile RAM de la page Performances (bloc « perf ») : le pourcentage et s'il y a une alerte
    fn signaler_constat(&self, effets: &mut Effets) {
        if let Some(c) = &self.constat {
            effets.ajouter(Effet::Signal {
                nom: "memoire-constat".into(),
                donnees: json!({ "pourcent": c.pourcent, "alertes": c.alertes.len() }),
            });
        }
    }

    /// Tout est lu (`Complete-ConstatMemoire`)
    fn finir(&mut self, ctx: &Contexte, effets: &mut Effets) {
        self.en_cours.clear();
        Self::occupe(effets, "");
        effets.ajouter(Effet::Rafraichir);
        let apres = std::mem::take(&mut self.apres);
        if apres.iter().any(|x| x == "relire") {
            let ensuite = if apres.iter().any(|x| x == "rapport") { "rapport" } else { "" };
            self.lancer(ctx, effets, ensuite, "donnees", Vec::new());
            return;
        }
        if apres.iter().any(|x| x == "rapport") {
            if self.constat.is_some() {
                self.rapport(ctx, effets);
            } else {
                Self::message(effets, "Le rapport n'a pas pu être fait : la mémoire du PC n'a pas pu être lue.");
            }
        }
    }

    /// Ce que Nevermind sait des jeux (`Get-JeuxMemoire`)
    fn jeux(&self, ctx: &Contexte) -> Jeux {
        let mut noms: Vec<String> = Vec::new();
        let appris: Vec<String> = ctx
            .reglage("FpsVeille")
            .map(|v| match v {
                Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
                Value::String(s) => vec![s],
                _ => vec![],
            })
            .unwrap_or_default();
        for x in listes::FPS_VEILLE_CONNUS.iter().map(|s| s.to_string()).chain(appris) {
            if x.is_empty() {
                continue;
            }
            let n = constat::sans_exe(&x.to_lowercase()).to_string();
            if !noms.contains(&n) {
                noms.push(n);
            }
        }
        Jeux {
            exes: self.jeux_exes.iter().filter(|e| !e.is_empty()).map(|e| e.to_lowercase()).collect(),
            noms,
            en_cours: self.partie,
        }
    }

    // -----------------------------------------------------------------------
    // Le rapport (New-RapportMemoire)
    // -----------------------------------------------------------------------
    fn rapport(&mut self, ctx: &Contexte, effets: &mut Effets) {
        if self.travail.is_some() || !self.en_cours.is_empty() || self.constat.is_none() {
            self.lancer(ctx, effets, "rapport", "donnees", Vec::new());
            Self::message(effets, "Je finis de regarder la mémoire, puis je pose le rapport sur le Bureau.");
            return;
        }
        // (essais : jamais sur le vrai Bureau)
        let dossier = if ctx.essai {
            std::env::var_os("ESSAI_DOSSIER").map(std::path::PathBuf::from).unwrap_or_else(|| ctx.donnees.clone())
        } else {
            match self.pc().bureau() {
                Some(b) => b,
                None => {
                    Self::message(effets, "Le rapport n'a pas pu être posé : le Bureau est introuvable.");
                    return;
                }
            }
        };
        let Some(c) = &self.constat else { return };
        match rapport::poser(c, &dossier) {
            Ok(f) => {
                ctx.journal(&format!("Mémoire : rapport posé ({})", f.display()));
                Self::message(
                    effets,
                    &format!(
                        "Rapport posé sur le Bureau : « {} ». Tu peux l'envoyer à la personne qui t'aide.",
                        texte::nom_fichier(&f.to_string_lossy())
                    ),
                );
            }
            Err(e) => {
                ctx.journal(&format!("Mémoire : rapport impossible ({e})"));
                Self::message(effets, &format!("Le rapport n'a pas pu être posé : {e}"));
            }
        }
    }

    // -----------------------------------------------------------------------
    // « Couper ce qui est sûr » (Invoke-CorrectionMemoire)
    // -----------------------------------------------------------------------
    fn demander_couper(&mut self, effets: &mut Effets) {
        let pc = self.pc();
        let Some(c) = &self.constat else { return };
        if c.corrections.is_empty() {
            return;
        }
        let noms: Vec<String> = c.corrections.iter().map(|x| format!("•  {}", x.nom)).collect();
        let texte = format!(
            "Nevermind va couper ce qui ne sert pas au quotidien :\n\n{}\n\nRien n'est désinstallé ni fermé de force. « Revenir en arrière », ou « Tout restaurer » dans la page Sauvegarde, remettent tout comme avant. Continuer ?",
            noms.join("\n")
        );
        // Les détails techniques (`Get-DetailsAvant`), lus sans rien changer
        let details: Vec<String> = c
            .corrections
            .iter()
            .map(|x| match pc.service(&x.service) {
                None => format!("Service {} : absent de ce PC, rien à faire", x.service),
                Some((demarrage, tourne)) => format!(
                    "Service {} : démarrage maintenant {}{}, après {}{}{}",
                    x.service,
                    format_demarrage(&demarrage),
                    if tourne { ", il tourne" } else { "" },
                    format_demarrage(&x.demarrage),
                    if tourne { ", arrêté" } else { "" },
                    if demarrage == x.demarrage { " (déjà comme ça)" } else { "" }
                ),
            })
            .collect();
        self.question(effets, Question::Couper, &texte, Self::oui_non(), &details);
    }

    fn couper(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let Some(c) = &self.constat else { return };
        let corrections = c.corrections.clone();
        if corrections.is_empty() {
            return;
        }
        let pc = self.pc();
        let mut faits = Vec::new();
        let mut rates = Vec::new();
        let mut log: Vec<Value> = Vec::new();
        Self::occupe(effets, "Un instant...");
        for x in &corrections {
            match self.service_manuel(ctx, pc.as_ref(), &x.service, &x.demarrage, &mut log) {
                Ok(()) => faits.push(x.clone()),
                Err(e) => {
                    rates.push(format!("{} : {e}", x.nom));
                    ctx.journal(&format!("Mémoire, correction {} : {e}", x.id));
                }
            }
        }
        Self::occupe(effets, "");
        effets.ajouter(Effet::Signal { nom: "sauvegarde-changee".into(), donnees: Value::Null });
        self.constat = None;
        self.donnees = None;
        self.lancer(ctx, effets, "", "donnees", Vec::new()); // (relu en tâche de fond : la page montre ce qui reste)
        let n = faits.len();
        let s = if n > 1 { "s" } else { "" };
        let mut lignes: Vec<String> = Vec::new();
        if n > 0 {
            lignes.push(format!("{n} chose{s} coupée{s} :"));
        }
        lignes.extend(faits.iter().map(|x| format!("•  {}", x.nom)));
        if !rates.is_empty() {
            lignes.push("Pas pu être fait :".into());
            lignes.extend(rates.iter().map(|r| format!("   {r}")));
        }
        Self::message(effets, "Mémoire : ce qui est sûr est coupé.");
        effets.ajouter(Effet::Signal {
            nom: "resultat".into(),
            donnees: json!({
                "titre": if n > 0 { "C'est fait !" } else { "Rien n'a pu être coupé" },
                "lignes": lignes,
                "log": log,
                "note": "La mémoire revient tout de suite pour ce qui tournait, et au prochain allumage du PC pour le reste.",
            }),
        });
    }

    /// `Set-ServiceOff -Demarrage Manual` : l'état d'origine noté (SvcOriginal), puis le changement par l'Aide
    fn service_manuel(
        &self,
        ctx: &Contexte,
        pc: &dyn Pc,
        nom: &str,
        demarrage: &str,
        log: &mut Vec<Value>,
    ) -> Result<(), String> {
        let (depart, tourne) =
            pc.service(nom).ok_or_else(|| format!("Impossible de trouver un service assorti du nom « {nom} »."))?;
        let mut orig: Vec<Value> = match ctx.reglage("SvcOriginal") {
            Some(Value::Array(a)) => a,
            Some(Value::Null) | None => Vec::new(),
            Some(x) => vec![x],
        };
        let deja = orig.iter().any(|x| x.as_str().unwrap_or("").split('|').next() == Some(nom));
        if !deja {
            orig.push(Value::String(format!("{nom}|{depart}|{}", if tourne { "True" } else { "False" })));
            ctx.ecrire_reglage("SvcOriginal", Value::Array(orig), true)?;
        }
        log.push(json!({ "Type": "svc", "Name": nom, "StartType": depart, "Running": tourne, "Nouveau": demarrage }));
        self.aide.demander(ctx, "service.regler", &json!({ "Nom": nom, "Demarrage": demarrage, "Arreter": tourne }))?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // « Fermer » une appli (Start-FermetureMemoire, Update-FermetureMemoire, Complete-FermetureMemoire)
    // -----------------------------------------------------------------------
    fn demander_fermer(&mut self, effets: &mut Effets, index: usize) {
        let Some(g) = self.constat.as_ref().and_then(|c| c.applis.get(index)).cloned() else { return };
        if !g.fermable {
            return;
        }
        if let Some(f) = &self.fermeture {
            Self::message(effets, &format!("Je ferme déjà {} : attends un instant.", f.nom));
            return;
        }
        if self.travail.is_some() {
            Self::message(effets, "Je relis la mémoire : réessaie dans un instant.");
            return;
        }
        if g.lanceur && self.partie != 0 {
            // (jeu lancé depuis la lecture)
            Self::message(
                effets,
                &format!("{} sert à ton jeu en cours : Nevermind ne le ferme pas pendant la partie.", g.nom),
            );
            return;
        }
        let octets: f64 = g.cibles.iter().map(|x| x.octets).sum();
        let texte = format!(
            "Fermer {} ? Environ {} libérés.\n\nIl se ferme comme avec la croix : s'il a quelque chose à enregistrer, il te le demandera.",
            g.nom,
            taille(octets)
        );
        self.question(effets, Question::Fermer(Box::new(g)), &texte, Self::oui_non(), &[]);
    }

    fn lancer_travail_fermeture(
        &mut self,
        cibles: Vec<Cible>,
        mode: &str,
    ) -> Option<JoinHandle<Result<fermeture::Resultat, String>>> {
        let pc = self.pc();
        let mode = mode.to_string();
        std::thread::Builder::new()
            .name("memoire-fermeture".into())
            .spawn(move || {
                let ctx = pc.contexte_fermeture()?;
                Ok(fermeture::fermer(pc.as_ref(), &cibles, &ctx, &mode))
            })
            .ok()
    }

    fn commencer_fermeture(&mut self, ctx: &Contexte, effets: &mut Effets, g: Appli) {
        if self.fermeture.is_some() || self.travail.is_some() {
            Self::message(effets, "Je relis la mémoire : réessaie dans un instant.");
            return;
        }
        let mut chemins: Vec<String> = Vec::new();
        for c in &g.cibles {
            if !chemins.contains(&c.chemin) {
                chemins.push(c.chemin.clone());
            }
        }
        let fil = self.lancer_travail_fermeture(g.cibles.clone(), "poli");
        ctx.journal(&format!(
            "Mémoire : fermeture polie de {} ({} processus, {})",
            g.nom,
            g.cibles.len(),
            chemins.join(", ")
        ));
        Self::occupe(effets, &format!("Je ferme {}...", g.nom));
        self.fermeture = Some(Fermeture {
            nom: g.nom.clone(),
            chemins,
            cibles: g.cibles.clone(),
            mode: "poli".into(),
            etape: "travail".into(),
            faits: Vec::new(),
            refuses: Vec::new(),
            fil,
            debut: Some(Instant::now()),
            restants: Vec::new(),
        });
        effets.ajouter(Effet::Rafraichir);
    }

    fn vivant(&mut self, c: &Cible) -> bool {
        self.pc().vivant(c.pid) == 1
    }

    /// Toutes les 250 ms : le travail de fond, puis l'attente de la fermeture
    fn suivre_fermeture(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let Some(f) = &mut self.fermeture else { return };
        match f.etape.as_str() {
            "travail" => {
                let fini = f.fil.as_ref().is_none_or(|x| x.is_finished());
                if !fini && f.debut.is_some_and(|d| d.elapsed() < Duration::from_secs(60)) {
                    return;
                }
                let r = if fini {
                    match f.fil.take().map(|x| x.join()) {
                        Some(Ok(r)) => r,
                        _ => Err("pas de résultat".into()),
                    }
                } else {
                    f.fil = None;
                    Err("pas de résultat".into())
                };
                // Receive-TravailFermeture
                let nom = f.nom.clone();
                let mut rien = true;
                match r {
                    Err(e) => ctx.journal(&format!("Mémoire, fermeture de {nom} : {e}")),
                    Ok(x) => {
                        for r in &x.refuses {
                            ctx.journal(&format!("Mémoire : processus {} pas fermé, il {}", r.pid, r.raison));
                        }
                        f.refuses.extend(x.refuses.iter().cloned());
                        rien = x.faits.is_empty();
                        if f.mode == "poli" {
                            f.faits = x.faits.iter().chain(x.deja.iter()).cloned().collect();
                        }
                    }
                }
                f.etape = "attente".into();
                f.debut = if rien { None } else { Some(Instant::now()) }; // (rien d'envoyé : pas d'attente)
            }
            "attente" => {
                let faits = f.faits.clone();
                let mode = f.mode.clone();
                let debut = f.debut;
                let nom = f.nom.clone();
                let restants: Vec<Cible> = faits.into_iter().filter(|c| self.vivant(c)).collect();
                if restants.is_empty() {
                    self.finir_fermeture(ctx, effets);
                    return;
                }
                let delai = Duration::from_secs(if mode == "force" { 3 } else { 5 });
                if debut.is_some_and(|d| d.elapsed() < delai) {
                    return;
                }
                if mode == "force" {
                    self.finir_fermeture(ctx, effets);
                    return;
                }
                let etat = self.pc().etat_fermeture(&restants);
                let (forcer, texte) = fermeture::message(&nom, etat);
                ctx.journal(&format!(
                    "Mémoire : {nom} pas fermé au bout de 5 s ({} processus restent)",
                    restants.len()
                ));
                Self::occupe(effets, "");
                if !forcer {
                    self.question(effets, Question::Info, &texte, json!([{ "id": "ok", "texte": "OK" }]), &[]);
                    self.finir_fermeture(ctx, effets);
                    return;
                }
                if let Some(f) = &mut self.fermeture {
                    f.etape = "question".into();
                    f.restants = restants;
                }
                self.question(
                    effets,
                    Question::Forcer,
                    &texte,
                    json!([{ "id": "forcer", "texte": "Forcer la fermeture" }, { "id": "non", "texte": "Laisser ouvert", "style": "second" }]),
                    &[],
                );
            }
            _ => {} // (« question » : on attend la réponse)
        }
    }

    fn forcer(&mut self, ctx: &Contexte, effets: &mut Effets, oui: bool) {
        let Some(f) = &self.fermeture else { return };
        if !oui || f.etape != "question" {
            self.finir_fermeture(ctx, effets);
            return;
        }
        let restants = f.restants.clone();
        let nom = f.nom.clone();
        let fil = self.lancer_travail_fermeture(restants.clone(), "force");
        if let Some(f) = &mut self.fermeture {
            f.mode = "force".into();
            f.etape = "travail".into();
            f.debut = Some(Instant::now());
            f.fil = fil;
        }
        ctx.journal(&format!("Mémoire : fermeture forcée de {nom} ({} processus)", restants.len()));
        Self::occupe(effets, &format!("Je force la fermeture de {nom}..."));
    }

    /// La fin : ce qui est fermé est noté dans l'historique, le message, puis la liste est relue
    fn finir_fermeture(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let Some(f) = self.fermeture.take() else { return };
        Self::occupe(effets, "");
        let fermes: Vec<Cible> = f.cibles.iter().filter(|c| !self.vivant(c)).cloned().collect();
        let reste = f.cibles.len() - fermes.len();
        let octets: f64 = fermes.iter().map(|x| x.octets).sum();
        if !fermes.is_empty() {
            let mode = if f.mode == "force" { "fermeture forcée" } else { "fermeture polie (comme la croix)" };
            let titre = if reste > 0 { format!("Fermé en partie : {}", f.nom) } else { format!("Fermé : {}", f.nom) };
            let mut details = f.chemins.clone();
            details.push(mode.into());
            details.push(format!("environ {} libérés (estimation)", taille(octets)));
            effets.ajouter(Effet::Signal {
                nom: "historique".into(),
                donnees: json!({ "titre": titre, "details": details,
                    "log": [{ "Type": "note", "Quoi": format!("{} fermé ({mode})", f.nom), "Chemins": f.chemins,
                        "Mode": f.mode, "Octets": octets, "Processus": fermes.len() }] }),
            });
            ctx.journal(&format!(
                "Mémoire : {} fermé ({mode}), {} processus, environ {}{}",
                f.nom,
                fermes.len(),
                taille(octets),
                if reste > 0 { format!(", {reste} restent") } else { String::new() }
            ));
            if reste > 0 {
                Self::message(effets, &format!("{} est fermé en partie : {reste} processus tournent encore.", f.nom));
            } else {
                Self::message(
                    effets,
                    &format!("{} est fermé. Environ {} de mémoire libérés (estimation).", f.nom, taille(octets)),
                );
            }
        } else if !f.refuses.is_empty() && f.faits.is_empty() {
            Self::message(
                effets,
                &format!(
                    "{} n'a pas été fermé : il a changé depuis que Nevermind a regardé la mémoire. La liste est relue.",
                    f.nom
                ),
            );
        }
        // Request-RelectureMemoire
        if self.travail.is_some() {
            if !self.apres.iter().any(|x| x == "relire") {
                self.apres.push("relire".into());
            }
        } else {
            self.lancer(ctx, effets, "", "donnees", Vec::new());
        }
    }

    fn repondre(&mut self, ctx: &Contexte, effets: &mut Effets, valeur: &Value) {
        let id = valeur.get("id").and_then(Value::as_str).unwrap_or("");
        let choix = valeur.get("valeur").and_then(Value::as_str).unwrap_or("");
        let Some(q) = self.questions.remove(id) else { return };
        match q {
            Question::Couper => {
                if choix == "oui" {
                    self.couper(ctx, effets);
                }
            }
            Question::Fermer(g) => {
                if choix == "oui" {
                    self.commencer_fermeture(ctx, effets, *g);
                }
            }
            Question::Forcer => self.forcer(ctx, effets, choix == "forcer"),
            Question::Info => {}
        }
    }

    fn rendre_en_attente(&mut self, effets: &mut Effets) {
        for e in self.en_attente.drain(..) {
            effets.ajouter(e);
        }
    }
}

/// `Format-DemarrageService`
fn format_demarrage(t: &str) -> String {
    match t {
        "Automatic" => "automatique".into(),
        "Manual" => "manuel".into(),
        "Disabled" => "désactivé".into(),
        _ => t.into(),
    }
}

impl Bloc for BlocMemoire {
    fn nom(&self) -> &'static str {
        "memoire"
    }

    fn pages(&mut self, ctx: &Contexte, ecran: &str) -> Map<String, Value> {
        // (la page RAM lance son constat en y arrivant la première fois : accueil-console.ps1, action « ecran »)
        if ecran == "memoire" && self.constat.is_none() && self.en_cours.is_empty() {
            let mut e = Effets::default();
            self.lancer(ctx, &mut e, "", "donnees", Vec::new());
            self.en_attente.extend(e.0);
        }
        let mut m = Map::new();
        m.insert("memoire".into(), page::page(self.constat.as_ref(), &self.en_cours, self.fermeture.is_some()));
        m
    }

    fn action(&mut self, ctx: &Contexte, id: &str, valeur: &Value, effets: &mut Effets) -> bool {
        self.rendre_en_attente(effets);
        match id {
            "memoire:analyser" => self.lancer(ctx, effets, "", "donnees", Vec::new()),
            "memoire:couper" => self.demander_couper(effets),
            "memoire:rapport" => self.rapport(ctx, effets),
            "memoire-reponse" => self.repondre(ctx, effets, valeur),
            // (l'arrivée sur un écran : la page RAM lance son constat la première fois ; les autres blocs la voient aussi)
            "ecran" => {
                if valeur.as_str() == Some("memoire") && self.constat.is_none() && self.en_cours.is_empty() {
                    self.lancer(ctx, effets, "", "donnees", Vec::new());
                    effets.ajouter(Effet::Rafraichir);
                }
                return false;
            }
            _ => {
                if let Some(k) = id.strip_prefix("memoire-fermer:").and_then(|k| k.parse::<usize>().ok()) {
                    self.demander_fermer(effets, k);
                } else {
                    return false;
                }
            }
        }
        true
    }

    fn tic(&mut self, ctx: &Contexte, _cache: bool, effets: &mut Effets) -> Option<Duration> {
        self.rendre_en_attente(effets);
        self.relever(ctx, effets);
        self.suivre_fermeture(ctx, effets);
        if self.fermeture.is_some() {
            Some(Duration::from_millis(250))
        } else if self.travail.is_some() {
            Some(Duration::from_millis(300))
        } else {
            // (rien en cours : un coup d'oeil de temps en temps, sans rien lire)
            Some(Duration::from_secs(2))
        }
    }

    fn signal(&mut self, _ctx: &Contexte, nom: &str, donnees: &Value, _effets: &mut Effets) {
        let liste = |v: &Value| -> Vec<String> {
            match v {
                Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
                _ => Vec::new(),
            }
        };
        match nom {
            "demarrage-programmes" => self.demarrage = liste(&donnees["programmes"]),
            "jeux-bibliotheque" => self.jeux_exes = liste(&donnees["exes"]),
            "partie-commencee" => {
                self.partie = donnees.get("pid").or_else(|| donnees.get("Pid")).and_then(Value::as_i64).unwrap_or(0)
            }
            "partie-finie" => self.partie = 0,
            _ => {}
        }
    }

    fn fermer(&mut self, _ctx: &Contexte) {
        // (les poignées gardées sont rendues ; le fil de fond en cours finit seul)
        #[cfg(windows)]
        windows::relacher();
    }
}
