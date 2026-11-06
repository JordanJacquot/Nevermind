//! Nevermind en Rust : le bloc « protection » (ecran-protection.ps1, securite.ps1, la lecture de l'Aide de
//! aide-client.ps1). Les pages Protection, Sécurité et Sauvegarde (Connexion : bloc réseau), la vérification de la protection (une
//! fois au démarrage, puis à la demande : aucune boucle de mesure), les gestes des points à vérifier et les analyses
//! de Microsoft Defender.
//!
//! Signaux envoyés : « protection-note » (la note sur 100), « question » (une confirmation à poser), « choisir-dossier »,
//! « panneau-analyse » (l'analyse en cours), « historique », « occupe », « registre-ecrire », « demarrage-desactiver »,
//! « historique-annuler ».
//! Signaux reçus : « sauvegarde-changee », « sauvegarde-nombre »,
//! « protection-relire », « analyse-demandee » (rend « analyse-finie »). Voir DIVERGENCES.md.

pub mod etat;
pub mod historique;
pub mod pages;
pub mod ps;
pub mod regles;
pub mod systeme;
#[cfg(windows)]
mod windows;

use etat::{Approuve, EtatS};
use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use pages::Verification;
use ps::Date;
use regles::{Constat, ContexteTaches, Sorte};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, SystemTime};
use systeme::{Lecture, Systeme, TypeAnalyse};

/// Le vrai système de Windows (pour les essais de lecture sans droits)
#[cfg(windows)]
pub fn systeme_windows() -> Arc<dyn Systeme> {
    Arc::new(windows::Vrai)
}

/// Une analyse de Defender en cours
struct AnalyseEnCours {
    fin: Receiver<Result<(), String>>,
    arret: Arc<AtomicBool>,
    debut: Date,
    titre: String,
}

/// Un fichier de données relu seulement quand il change
#[derive(Default)]
struct Cache {
    cle: Option<(Option<SystemTime>, Option<SystemTime>)>,
    valeur: Option<Value>,
}

impl Cache {
    fn lire(&mut self, f: &Path) -> Option<&Value> {
        let date = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let bak = f.with_file_name(format!("{}.bak", f.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
        let cle = (date(f), date(&bak));
        if self.cle != Some(cle) {
            self.cle = Some(cle);
            self.valeur = historique::lire_json(f);
        }
        self.valeur.as_ref()
    }
}

pub struct BlocProtection {
    sys: Option<Arc<dyn Systeme>>,
    verif: Option<Verification>,
    /// La dernière lecture (pour rejuger sans tout relire quand seuls les éléments approuvés changent)
    lecture: Option<Lecture>,
    note_analyses: String,
    lecture_en_cours: Option<Receiver<Lecture>>,
    demarre: bool,
    /// `$script:Kit.SecChecks` et `$script:Kit.SecApprouves` : ce que numérotent les boutons de la page
    points: Vec<Constat>,
    approuves_vus: Vec<Approuve>,
    sauvegarde_nombre: Option<usize>,
    historique: Cache,
    sauvegarde: Cache,
    analyse: Option<AnalyseEnCours>,
    maj_base: Option<Receiver<Result<(), String>>>,
    /// La pastille « en cours » à montrer au prochain tic (signal commun « occupe »)
    occupe_texte: Option<String>,
    /// La marque de l'analyse demandée par un autre bloc (signal « analyse-demandee »), rendue à sa fin
    marque: Option<Value>,
}

impl Default for BlocProtection {
    fn default() -> Self {
        #[cfg(windows)]
        let sys: Option<Arc<dyn Systeme>> = Some(systeme_windows());
        #[cfg(not(windows))]
        let sys: Option<Arc<dyn Systeme>> = None;
        BlocProtection::avec_systeme(sys)
    }
}

/// L'action est confirmée : l'hôte la renvoie avec `{ id, valeur: "oui" }` (signaux communs : « question »)
fn confirme(valeur: &Value) -> bool {
    valeur.get("valeur").and_then(Value::as_str) == Some("oui")
}

/// `Confirm-Action` : la question est posée par l'hôte (kit.question), qui renvoie l'action confirmée
/// (`valeur` porte déjà une réponse autre que « oui » : l'utilisateur a dit non, rien n'est fait ni redemandé)
fn question(effets: &mut Effets, id: &str, texte: &str, valeur: &Value) {
    if valeur.get("valeur").is_some() {
        return;
    }
    effets.ajouter(Effet::Signal {
        nom: "question".into(),
        donnees: json!({ "id": id, "titre": "Nevermind", "texte": texte, "action": id,
            "boutons": [{ "id": "oui", "texte": "Oui" }, { "id": "non", "texte": "Non", "style": "second" }] }),
    });
}

const OCCUPE: &str = "Un instant : Nevermind finit ce qu'il est en train de faire.";

impl BlocProtection {
    /// Le bloc avec un système donné (les essais passent un faux)
    pub fn avec_systeme(sys: Option<Arc<dyn Systeme>>) -> BlocProtection {
        BlocProtection {
            sys,
            verif: None,
            lecture: None,
            note_analyses: String::new(),
            lecture_en_cours: None,
            demarre: false,
            points: Vec::new(),
            approuves_vus: Vec::new(),
            sauvegarde_nombre: None,
            historique: Cache::default(),
            sauvegarde: Cache::default(),
            analyse: None,
            maj_base: None,
            occupe_texte: None,
            marque: None,
        }
    }

    /// La note sur 100 de la dernière vérification (`$script:SecurityScore`)
    pub fn note(&self) -> Option<i64> {
        self.verif.as_ref().map(|v| v.note)
    }

    /// Un travail de fond attend sa fin (lecture, analyse, mise à jour de la base)
    pub fn occupe(&self) -> bool {
        self.lecture_en_cours.is_some() || self.analyse.is_some() || self.maj_base.is_some()
    }

    /// `Update-SecurityTab` : la lecture lente part dans un fil à part ; la suite se fait au prochain tic
    fn relire(&mut self) {
        if self.lecture_en_cours.is_some() {
            return;
        }
        let Some(sys) = self.sys.clone() else { return };
        let (tx, rx) = channel();
        let ok = std::thread::Builder::new().name("protection-lecture".into()).spawn(move || {
            let _ = tx.send(sys.lire());
        });
        if ok.is_ok() {
            self.lecture_en_cours = Some(rx);
        }
    }

    /// Une lecture finie : l'Aide la complète (sans droits), puis elle est jugée
    fn finir_lecture(&mut self, ctx: &Contexte, mut l: Lecture, effets: &mut Effets) {
        if let Some(sys) = self.sys.clone()
            && !sys.est_admin()
        {
            let lecture = ctx.aide("lecture.securite", &json!({}), 60_000).ok();
            if lecture.is_some() {
                let taches = ctx.aide("lecture.taches", &json!({}), 60_000);
                if let Err(e) = &taches {
                    ctx.journal(&format!("Aide : tâches planifiées non lues ({e})"));
                }
                let (temp, public) = (sys.temp(), sys.public());
                let var = |n: &str| sys.variable(n);
                let prot = |c: &str| sys.script_protege(c);
                let existe = |c: &str| sys.existe(c);
                let c = ContexteTaches {
                    temp: &temp,
                    public: &public,
                    variable: &var,
                    script_protege: &prot,
                    existe: &existe,
                    propre_tache: None,
                };
                regles::completer_par_aide(&mut l.s, lecture.as_ref(), taches.as_ref().map_err(|e| e.clone()), &c);
            }
        }
        self.lecture = Some(l);
        self.juger(ctx, effets);
    }

    /// Les points, la note et les menaces depuis la dernière lecture
    fn juger(&mut self, ctx: &Contexte, effets: &mut Effets) {
        let Some(l) = &self.lecture else { return };
        let approuves = etat::approuves(ctx.reglage("SecuriteApprouves").as_ref());
        let hosts = self.sys.as_ref().map(|s| s.hosts()).unwrap_or_default();
        let v = Verification::faire(l.s.clone(), &l.env, &approuves, &hosts);
        self.note_analyses = regles::note_analyses(&v.s);
        let note = v.note;
        self.verif = Some(v);
        effets.ajouter(Effet::Signal { nom: "protection-note".into(), donnees: json!(note) });
        effets.ajouter(Effet::Rafraichir);
    }

    fn defender_ok(&self) -> bool {
        self.verif.as_ref().is_some_and(|v| v.s.defender_ok())
    }

    /// `Invoke-DefenderScan`
    fn analyser(&mut self, effets: &mut Effets, sorte: TypeAnalyse, chemins: Vec<String>, titre: Option<String>) {
        if self.analyse.is_some() {
            effets.ajouter(Effet::Annonce("Une analyse ou un test est déjà en cours.".into()));
            return;
        }
        let Some(sys) = self.sys.clone() else { return };
        let titre = titre.unwrap_or_else(|| {
            match sorte {
                TypeAnalyse::Rapide => "Analyse rapide",
                TypeAnalyse::Complete => "Analyse complète",
                TypeAnalyse::Personnalisee => "Analyse personnalisée",
            }
            .into()
        });
        let texte = match sorte {
            TypeAnalyse::Rapide => "Analyse des endroits où se cachent les virus. Ça prend en général quelques minutes, tu peux continuer à utiliser ton PC.".to_string(),
            TypeAnalyse::Complete => "Analyse de tous les fichiers du PC. Ça peut prendre une heure ou plus : tu peux continuer à utiliser ton PC.".to_string(),
            TypeAnalyse::Personnalisee => {
                format!("Analyse de {} élément{}.", chemins.len(), if chemins.len() > 1 { "s" } else { "" })
            }
        };
        let arret = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let a = arret.clone();
        let debut = sys.maintenant();
        let lance = std::thread::Builder::new().name("protection-analyse".into()).spawn(move || {
            let _ = tx.send(sys.analyser(sorte, &chemins, &a));
        });
        if lance.is_err() {
            return;
        }
        self.analyse = Some(AnalyseEnCours { fin: rx, arret, debut, titre: titre.clone() });
        effets.ajouter(Effet::Signal {
            nom: "panneau-analyse".into(),
            donnees: json!({ "etat": "en-cours", "titre": titre, "sous": "Microsoft Defender", "texte": texte,
                "arreter": "sec-analyse-arreter" }),
        });
    }

    /// La fin d'une analyse : le verdict (jamais « Aucune menace » si le résultat n'a pas été lu)
    fn finir_analyse(&mut self, ctx: &Contexte, a: AnalyseEnCours, r: Result<(), String>, effets: &mut Effets) {
        let Some(sys) = self.sys.clone() else { return };
        let duree = (sys.maintenant().ticks() - a.debut.ticks()).max(0) / 10_000_000;
        let pluriel = |n: usize| if n > 1 { "s" } else { "" };
        let (etat, resultat, lignes, boutons): (&str, String, Vec<String>, Vec<Value>) = if a
            .arret
            .load(Ordering::SeqCst)
        {
            ("arretee", "Arrêtée".into(), vec!["Analyse arrêtée avant la fin.".into()], Vec::new())
        } else if let Err(e) = r {
            ("echec", "Échec".into(), vec![format!("L'analyse n'a pas pu se faire : {e}")], Vec::new())
        } else {
            let admin = sys.est_admin();
            let aide = if admin { None } else { ctx.aide("lecture.securite", &json!({}), 60_000).ok() };
            let (lu, journal) = regles::menaces_analyse(a.debut, admin, aide.as_ref(), &|| sys.detections());
            if let Some(j) = journal {
                ctx.journal(&j);
            }
            if !lu.lu {
                ("inconnu", "Résultat inconnu".into(), vec!["L'analyse est terminée, mais je n'ai pas pu lire son résultat. Pour le voir, ouvre Sécurité Windows, puis « Protection contre les virus et menaces ».".into()], Vec::new())
            } else if lu.menaces.is_empty() {
                let quand = if duree < 60 {
                    "moins d'une minute.".to_string()
                } else {
                    format!("{} min.", ((duree as f64) / 60.0).round_ties_even() as i64)
                };
                (
                    "aucune",
                    "Aucune menace".into(),
                    vec![format!("Aucune menace trouvée. Analyse terminée en {quand}")],
                    Vec::new(),
                )
            } else {
                let n = lu.menaces.len();
                (
                    "menaces",
                    format!("{n} menace{} trouvée{}", pluriel(n), pluriel(n)),
                    lu.menaces.iter().map(|m| format!("{}   {}", m.nom, m.fichier)).collect(),
                    vec![json!({ "id": "sec-supprimer-trouvees", "texte": "Supprimer les menaces" })],
                )
            }
        };
        effets.ajouter(Effet::Signal {
            nom: "panneau-analyse".into(),
            donnees: json!({ "etat": etat, "titre": a.titre, "sous": "Microsoft Defender", "resultat": resultat,
                "temps": format!("{:02}:{:02}", duree / 60, duree % 60), "lignes": lignes, "boutons": boutons }),
        });
        effets.ajouter(Effet::Annonce(format!("{} : {}", a.titre, lignes.join(" "))));
        // Une analyse demandée par un autre bloc (trafic.ps1 : Invoke-TrafficScan) : son résultat ($script:LastScanResult)
        if let Some(m) = self.marque.take() {
            let r = match etat {
                "arretee" => "cancel",
                "echec" => "error",
                "inconnu" => "unknown",
                "aucune" => "clean",
                _ => "threats",
            };
            effets
                .ajouter(Effet::Signal { nom: "analyse-finie".into(), donnees: json!({ "Marque": m, "Resultat": r }) });
        }
        // (une analyse qui a échoué ne relit pas la protection, comme la version PowerShell)
        if etat != "echec" {
            self.relire();
        }
    }

    /// Un geste d'un point à vérifier (le Script de son action), déjà confirmé
    fn geste(&mut self, ctx: &Contexte, c: &Constat, j: usize, effets: &mut Effets) {
        let Some(a) = c.actions.get(j).cloned() else { return };
        let ttl = c.titre.clone();
        let sys = self.sys.clone();
        let mut definitif: Option<(String, String)> = None;
        let mut erreur: Option<String> = None;
        let mut rafraichir = !a.sans_rafraichir;
        match &a.sorte {
            Sorte::SupprimerMenaces => {
                if let Some(s) = &sys
                    && let Err(e) = s.supprimer_menaces()
                {
                    erreur = Some(e);
                }
            }
            Sorte::Ouvrir(u) => effets.ajouter(Effet::OuvrirAdresse(u.clone())),
            Sorte::Planificateur => effets.ajouter(Effet::OuvrirAdresse("taskschd.msc".into())),
            Sorte::MettreAJourBase => {
                self.mettre_a_jour_base();
                rafraichir = false;
            }
            Sorte::RetirerExclusions(l) => {
                match ctx.aide("antivirus.retirer-exclusions", &json!({ "Elements": l }), 60_000) {
                    Ok(_) => {
                        definitif = Some((
                            format!("Exclusions retirées : {}", l.join(", ")),
                            "Sécurité Windows, Protection contre les virus et menaces, « Gérer les paramètres », Exclusions : ajouter de nouveau l'élément.".into(),
                        ))
                    }
                    Err(e) => erreur = Some(e),
                }
            }
            Sorte::ReactiverPareFeu => match ctx.aide("pare-feu.activer", &json!({}), 60_000) {
                Ok(_) => {
                    definitif = Some((
                        "Pare-feu réactivé sur tous les réseaux".into(),
                        "Sécurité Windows, « Pare-feu et protection du réseau » : choisir le réseau et désactiver le pare-feu.".into(),
                    ))
                }
                Err(e) => erreur = Some(e),
            },
            Sorte::ReactiverUac => effets.ajouter(Effet::Signal {
                nom: "registre-ecrire".into(),
                donnees: json!({ "Path": "HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System", "Name": "EnableLUA",
                    "Value": 1, "Kind": "DWord", "titre": format!("{} ({ttl})", a.texte) }),
            }),
            Sorte::OuvrirFichier(f) => {
                if let Some(s) = &sys {
                    s.bloc_notes(f);
                }
            }
            Sorte::OuvrirEmplacement(f) => {
                if let Some(s) = &sys {
                    s.montrer(f);
                }
            }
            Sorte::Analyser { chemins, titre } => {
                self.analyser(effets, TypeAnalyse::Personnalisee, chemins.clone(), Some(titre.clone()));
                rafraichir = false;
            }
            Sorte::DesactiverDemarrage(l) => effets.ajouter(Effet::Signal {
                nom: "demarrage-desactiver".into(),
                donnees: json!({ "elements": l.iter().map(|d| json!({ "Nom": d.nom, "Exe": d.exe })).collect::<Vec<_>>(),
                    "titre": format!("{} ({ttl})", a.texte) }),
            }),
            Sorte::DesactiverTaches(l) => {
                let (mut faites, mut ratees) = (Vec::new(), Vec::new());
                if let Some(s) = &sys {
                    for t in l {
                        match s.desactiver_tache(&t.nom, &t.chemin) {
                            Ok(true) => faites.push(t.nom.clone()),
                            Ok(false) => ratees.push(t.nom.clone()),
                            Err(e) => {
                                ctx.journal(&format!("Sécurité : tâche « {}{} » non désactivée ({e})", t.chemin, t.nom));
                                ratees.push(t.nom.clone());
                            }
                        }
                    }
                }
                match regles::message_taches(&faites, &ratees) {
                    Ok(Some(m)) => effets.ajouter(Effet::Annonce(m)),
                    Ok(None) => {}
                    Err(m) => erreur = Some(m),
                }
            }
        }
        if let Some(e) = erreur {
            // (Invoke-Safe : un message écrit pour l'utilisateur, montré tel quel après cette phrase)
            effets.ajouter(Effet::Annonce(format!(
                "Oups, quelque chose s'est mal passé :

{e}"
            )));
            return;
        }
        if let Some((quoi, retour)) = definitif {
            // (Add-ChangementDefinitif puis Add-History : écrit par le bloc qui tient l'historique)
            effets.ajouter(Effet::Signal {
                nom: "historique".into(),
                donnees: json!({ "titre": format!("{} ({ttl})", a.texte), "details": [],
                    "log": [{ "Type": "definitif", "Quoi": quoi, "Retour": retour }] }),
            });
        }
        if rafraichir && self.analyse.is_none() {
            self.relire();
        }
    }

    /// `Update-Definitions`
    fn mettre_a_jour_base(&mut self) {
        if self.maj_base.is_some() {
            return;
        }
        let Some(sys) = self.sys.clone() else { return };
        let (tx, rx) = channel();
        self.occupe_texte = Some("Mise à jour de la base de virus...".into());
        if std::thread::Builder::new()
            .name("protection-base".into())
            .spawn(move || {
                let _ = tx.send(sys.mettre_a_jour_base());
            })
            .is_ok()
        {
            self.maj_base = Some(rx);
        }
    }

    /// `Add-SecTrusted` : les éléments marqués sûrs, gardés dans les préférences
    fn approuver(&mut self, ctx: &Contexte, l: &[Approuve], effets: &mut Effets) {
        let mut tous = etat::approuves(ctx.reglage("SecuriteApprouves").as_ref());
        for e in l {
            if !tous.iter().any(|t| ps::egal(&t.cle, &e.cle)) {
                tous.push(e.clone());
            }
        }
        self.enregistrer(ctx, &tous, effets);
        ctx.journal(&format!(
            "Sécurité : approuvé par l'utilisateur : {}",
            l.iter().map(|e| e.texte.as_str()).collect::<Vec<_>>().join(" | ")
        ));
    }

    /// `Save-SecTrusted`, puis les points jugés de nouveau
    fn enregistrer(&mut self, ctx: &Contexte, l: &[Approuve], effets: &mut Effets) {
        let v = Value::Array(l.iter().map(Approuve::json).collect());
        if let Err(e) = ctx.ecrire_reglage("SecuriteApprouves", v, false) {
            effets.ajouter(Effet::Annonce(e));
        }
        if self.analyse.is_none() {
            self.juger(ctx, effets);
        }
    }

    fn fichier(ctx: &Contexte, nom: &str) -> PathBuf {
        ctx.donnees.join(nom)
    }
}

impl Bloc for BlocProtection {
    fn nom(&self) -> &'static str {
        "protection"
    }

    fn pages(&mut self, ctx: &Contexte, _ecran: &str) -> Map<String, Value> {
        let approuves = etat::approuves(ctx.reglage("SecuriteApprouves").as_ref());
        let hist = historique::importer(self.historique.lire(&Self::fichier(ctx, "historique.json")));
        let j = self.sauvegarde.lire(&Self::fichier(ctx, "sauvegarde.json")).cloned();
        // (Read-FichierSauvegarde : un fichier sans « Registry » ne compte pas)
        let j = j.filter(|v| v.get("Registry").is_some());
        let mut sauve = historique::sauvegarde(
            j.as_ref(),
            ctx.reglage("SvcOriginal").as_ref(),
            ctx.reglage("PcfgOriginal").as_ref(),
        );
        if let Some(n) = self.sauvegarde_nombre {
            sauve.nombre = n;
        }
        let e = pages::EntreePages {
            verif: self.verif.as_ref(),
            note_analyses: &self.note_analyses,
            approuves: &approuves,
            sauvegarde: &sauve,
            historique: &hist,
        };
        let (p, points) = pages::pages(&e);
        self.points = points;
        self.approuves_vus = approuves;
        p
    }

    fn action(&mut self, ctx: &Contexte, id: &str, valeur: &Value, effets: &mut Effets) -> bool {
        match id {
            "clic:BtnSecRefresh" => {
                self.relire();
                return true;
            }
            "clic:BtnScanQuick" | "clic:BtnScanFull" | "clic:BtnScanFolder" | "clic:BtnScanUpdate" => {
                // (les boutons d'analyse ne marchent qu'avec Microsoft Defender, et pas pendant une analyse)
                if !self.defender_ok() || self.analyse.is_some() {
                    effets.ajouter(Effet::Annonce(OCCUPE.into()));
                    return true;
                }
                match id {
                    "clic:BtnScanQuick" => self.analyser(effets, TypeAnalyse::Rapide, Vec::new(), None),
                    "clic:BtnScanFull" => {
                        if confirme(valeur) {
                            self.analyser(effets, TypeAnalyse::Complete, Vec::new(), None);
                        } else {
                            question(
                                effets,
                                id,
                                "L'analyse complète vérifie tous les fichiers du PC : elle peut durer une heure ou plus. Tu peux continuer à utiliser ton PC pendant ce temps. Lancer l'analyse ?",
                                valeur,
                            );
                        }
                    }
                    "clic:BtnScanFolder" => effets.ajouter(Effet::Signal {
                        nom: "choisir-dossier".into(),
                        donnees: json!({ "texte": "Choisis le dossier à analyser", "retour": "sec-analyser-dossier" }),
                    }),
                    _ => self.mettre_a_jour_base(),
                }
                return true;
            }
            "sec-analyser-dossier" => {
                let d = valeur.as_str().map(str::trim).unwrap_or_default().trim_end_matches('\\').to_string();
                if !d.is_empty() && self.defender_ok() {
                    let nom = d.rsplit('\\').next().unwrap_or(&d).to_string();
                    self.analyser(
                        effets,
                        TypeAnalyse::Personnalisee,
                        vec![d.clone()],
                        Some(format!("Analyse de {nom}")),
                    );
                }
                return true;
            }
            "sec-analyse-arreter" => {
                if let Some(a) = &self.analyse {
                    a.arret.store(true, Ordering::SeqCst);
                }
                return true;
            }
            "sec-supprimer-trouvees" => {
                if confirme(valeur) {
                    match self.sys.as_ref().map(|s| s.supprimer_menaces()) {
                        Some(Err(e)) => effets.ajouter(Effet::Annonce(e)),
                        _ => {
                            effets.ajouter(Effet::Annonce("Menaces supprimées.".into()));
                            self.relire();
                        }
                    }
                } else {
                    question(effets, id, "Supprimer toutes les menaces trouvées par Microsoft Defender ?", valeur);
                }
                return true;
            }
            _ => {}
        }
        // Invoke-ActionProtection : les gestes à numéro
        if let Some(r) = id.strip_prefix("sec:") {
            let mut p = r.splitn(2, ':');
            let k = p.next().and_then(|x| x.parse::<usize>().ok());
            let j = p.next().and_then(|x| x.parse::<usize>().ok());
            let (Some(k), Some(j)) = (k, j) else { return false };
            let Some(c) = self.points.get(k).cloned() else { return true };
            let Some(a) = c.actions.get(j) else { return true };
            if let Some(t) = &a.confirmer
                && !confirme(valeur)
            {
                question(effets, id, t, valeur);
                return true;
            }
            self.geste(ctx, &c, j, effets);
            return true;
        }
        if let Some(k) = id.strip_prefix("sec-moi:").and_then(|x| x.parse::<usize>().ok()) {
            let Some(c) = self.points.get(k).cloned() else { return true };
            let n = c.confiance.len();
            if n == 0 {
                return true;
            }
            if !confirme(valeur) {
                let t = if n > 1 {
                    format!(
                        "Marquer ces {n} éléments comme sûrs ? Ils ne compteront plus dans le niveau de protection. S'ils changent (autre commande, autre fichier), Nevermind les signalera de nouveau."
                    )
                } else {
                    "Marquer cet élément comme sûr ? Il ne comptera plus dans le niveau de protection. S'il change (autre commande, autre fichier), Nevermind le signalera de nouveau.".to_string()
                };
                question(effets, id, &t, valeur);
                return true;
            }
            self.approuver(ctx, &c.confiance, effets);
            return true;
        }
        if let Some(k) = id.strip_prefix("sec-retirer:").and_then(|x| x.parse::<usize>().ok()) {
            if let Some(t) = self.approuves_vus.get(k).cloned() {
                let reste: Vec<Approuve> = etat::approuves(ctx.reglage("SecuriteApprouves").as_ref())
                    .into_iter()
                    .filter(|x| !ps::egal(&x.cle, &t.cle))
                    .collect();
                self.enregistrer(ctx, &reste, effets);
            }
            return true;
        }
        if let Some(h) = id.strip_prefix("hist:")
            && (6..=40).contains(&h.len())
            && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            // (l'annulation, avec sa question, est faite par le bloc qui tient la sauvegarde : Undo-HistoryEntry)
            effets.ajouter(Effet::Signal { nom: "historique-annuler".into(), donnees: json!({ "id": h }) });
            return true;
        }
        false
    }

    fn tic(&mut self, ctx: &Contexte, _cache: bool, effets: &mut Effets) -> Option<Duration> {
        if let Some(t) = self.occupe_texte.take() {
            effets.ajouter(Effet::Signal { nom: "occupe".into(), donnees: json!({ "texte": t }) });
        }
        if !self.demarre {
            self.demarre = true;
            self.relire();
        }
        if let Some(rx) = &self.lecture_en_cours {
            match rx.try_recv() {
                Ok(l) => {
                    self.lecture_en_cours = None;
                    self.finir_lecture(ctx, l, effets);
                }
                Err(TryRecvError::Disconnected) => self.lecture_en_cours = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(a) = &self.analyse {
            let r = match a.fin.try_recv() {
                Ok(r) => Some(r),
                Err(TryRecvError::Disconnected) => Some(Err("l'analyse s'est arrêtée".into())),
                Err(TryRecvError::Empty) => None,
            };
            if let Some(r) = r
                && let Some(a) = self.analyse.take()
            {
                self.finir_analyse(ctx, a, r, effets);
            }
        }
        if let Some(rx) = &self.maj_base {
            match rx.try_recv() {
                Ok(r) => {
                    self.maj_base = None;
                    effets.ajouter(Effet::Signal { nom: "occupe".into(), donnees: json!({ "texte": "" }) });
                    if let Err(e) = r {
                        effets.ajouter(Effet::Annonce(format!("La mise à jour n'a pas pu se faire :\n\n{e}")));
                    }
                    self.relire();
                }
                Err(TryRecvError::Disconnected) => self.maj_base = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        // (rien ne tourne en continu : un coup d'œil deux fois par seconde seulement quand un travail de fond attend)
        if self.occupe() { Some(Duration::from_millis(500)) } else { Some(Duration::from_secs(60)) }
    }

    fn signal(&mut self, _ctx: &Contexte, nom: &str, donnees: &Value, effets: &mut Effets) {
        match nom {
            // (sauvegarde.json est relu quand il change ; ce signal commun redessine la page)
            "sauvegarde-changee" => {}
            "sauvegarde-nombre" => self.sauvegarde_nombre = donnees.as_u64().map(|n| n as usize),
            "protection-relire" => self.relire(),
            // (Invoke-DefenderScan appelé par trafic.ps1 : une analyse personnalisée, sans vérifier Defender avant)
            "analyse-demandee" => {
                let en_cours = self.analyse.is_some();
                let sorte = match donnees.get("Sorte").and_then(Value::as_str).unwrap_or("CustomScan") {
                    "QuickScan" => TypeAnalyse::Rapide,
                    "FullScan" => TypeAnalyse::Complete,
                    _ => TypeAnalyse::Personnalisee,
                };
                let chemins = donnees.get("Chemins").map(ps::textes_vrais).unwrap_or_default();
                let titre = donnees.get("Titre").and_then(Value::as_str).filter(|t| !t.is_empty()).map(str::to_string);
                self.analyser(effets, sorte, chemins, titre);
                if !en_cours && self.analyse.is_some() {
                    self.marque = Some(donnees.get("Marque").cloned().unwrap_or(Value::Null));
                }
                return;
            }
            _ => return,
        }
        effets.ajouter(Effet::Rafraichir);
    }

    fn fermer(&mut self, _ctx: &Contexte) {
        if let Some(a) = &self.analyse {
            a.arret.store(true, Ordering::SeqCst);
        }
    }
}

/// Ce que rejouent les juges : l'état d'un PC, comme le voient `Update-SecurityTab` puis `Get-PagesProtection`
pub struct CasPages<'a> {
    pub s: Option<EtatS>,
    pub env: &'a etat::Environnement,
    pub approuves: &'a [Approuve],
    pub sauvegarde: &'a historique::Sauvegarde,
    pub historique: &'a [historique::Entree],
}

/// Les pages d'un cas (sans Windows), la note et les points numérotés
pub fn pages_pour(c: CasPages) -> (Map<String, Value>, Option<i64>, Vec<Constat>) {
    let hosts = "C:\\WINDOWS\\System32\\drivers\\etc\\hosts";
    let verif = c.s.map(|s| Verification::faire(s, c.env, c.approuves, hosts));
    let note = verif.as_ref().map(|v| regles::note_analyses(&v.s)).unwrap_or_default();
    let e = pages::EntreePages {
        verif: verif.as_ref(),
        note_analyses: &note,
        approuves: c.approuves,
        sauvegarde: c.sauvegarde,
        historique: c.historique,
    };
    let (p, points) = pages::pages(&e);
    (p, verif.map(|v| v.note), points)
}
