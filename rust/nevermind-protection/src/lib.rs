//! Nevermind en Rust : le bloc « protection » (ecran-protection.ps1, securite.ps1, la lecture de l'Aide de
//! aide-client.ps1). Les pages Protection, Sécurité et Sauvegarde (Connexion : bloc réseau), la vérification de la protection (une
//! fois au démarrage, puis à la demande : aucune boucle de mesure), les gestes des points à vérifier et les analyses
//! de Microsoft Defender.
//!
//! Signaux envoyés : « question » (une confirmation à poser),
//! « historique », « occupe », « registre-ecrire », « demarrage-desactiver » (faits par le bloc pc),
//! « historique-annuler » (bloc pc), « etat-mindset » (la note, pour etat.json : l'hôte). La note et le panneau d'une analyse sont dans les pages (« _accueil », « panneau »).
//! Signaux reçus : « sauvegarde-changee », « sauvegarde-nombre »,
//! « protection-relire », « analyse-demandee » (rend « analyse-finie »). Voir DIVERGENCES.md.

pub mod etat;
pub mod historique;
pub mod pages;
pub mod panneau;
pub mod ps;
pub mod regles;
pub mod systeme;
#[cfg(windows)]
mod windows;

use etat::{Approuve, EtatS};
use nevermind_noyau::travaux::{Rendu, Travaux};
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
    fin: Receiver<FinAnalyse>,
    arret: Arc<AtomicBool>,
    debut: Date,
    titre: String,
}

/// La fin d'une analyse, rendue par son fil : son résultat, et la lecture de l'Aide qui donne les menaces trouvées
/// (`Some` : déjà lue dans le fil, lot B ; `None` : à lire au relevé, seulement sans contexte partagé)
struct FinAnalyse {
    r: Result<(), String>,
    aide: Option<Option<Value>>,
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
    /// Les gestes longs des points à vérifier (Defender, l'Aide) : dans un fil à part (audit G3)
    travaux: Travaux<FinGeste>,
    /// La fenêtre « Choisis le dossier à analyser » : elle attend l'utilisateur sans limite, dans un fil à part
    /// (lot B du second audit, comme `CHOIX` du bloc Jeux) ; elle rend le dossier choisi, ou rien
    choix: Travaux<Option<String>>,
    verif: Option<Verification>,
    /// La dernière lecture (pour rejuger sans tout relire quand seuls les éléments approuvés changent)
    lecture: Option<Lecture>,
    note_analyses: String,
    lecture_en_cours: Option<Receiver<(Lecture, bool)>>,
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
    /// Le panneau de la dernière analyse de Defender (ouvert jusqu'à « Fermer »)
    panneau: Option<panneau::PanneauAnalyse>,
    numero_panneau: u64,
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
    question_sur(effets, id, id, texte, valeur);
}

/// Une question sur un point de la liste : `cle` (rendue avec la réponse) dit quel point, par ce qu'il est
/// ([`empreinte`]), pas par son numéro dans la liste (M11, audit du 08/11 : une relecture pendant la question décale la
/// liste, et le « Oui » faisait le geste d'un autre point)
fn question_sur(effets: &mut Effets, action: &str, cle: &str, texte: &str, valeur: &Value) {
    if valeur.get("valeur").is_some() {
        return;
    }
    effets.ajouter(Effet::Signal {
        nom: "question".into(),
        donnees: json!({ "id": cle, "titre": "Nevermind", "texte": texte, "action": action,
            "boutons": [{ "id": "oui", "texte": "Oui" }, { "id": "non", "texte": "Non", "style": "second" }] }),
    });
}

/// Ce qu'est un geste d'un point (son titre, le texte et le contenu exact du geste : les tâches, les exclusions...),
/// en court : la question le garde, et le « Oui » ne fait le geste que si le même point est toujours là
fn empreinte(c: &Constat, quoi: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    c.titre.hash(&mut h);
    c.statut.hash(&mut h);
    quoi.hash(&mut h);
    format!("point-{:016x}", h.finish())
}

fn empreinte_geste(c: &Constat, a: &regles::Geste) -> String {
    empreinte(c, &format!("geste|{}|{:?}", a.texte, a.sorte))
}

fn empreinte_confiance(c: &Constat) -> String {
    empreinte(c, &format!("moi|{}", c.confiance.iter().map(|x| x.cle.as_str()).collect::<Vec<_>>().join("\n")))
}

/// Le point a changé pendant la question : rien n'est fait
const POINT_CHANGE: &str = "La liste des points à vérifier a changé pendant la question : rien n'a été fait. Regarde la liste à jour et réessaie.";

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
            travaux: Travaux::default(),
            choix: Travaux::default(),
            marque: None,
            panneau: None,
            numero_panneau: 0,
        }
    }

    /// Le panneau d'une analyse (`Show-TestPanel` d'Invoke-DefenderScan) : nouveau numéro au début d'une analyse
    fn montrer_panneau(&mut self, donnees: Value) {
        let debut = donnees.get("etat").and_then(Value::as_str) == Some("en-cours");
        if debut || self.panneau.is_none() {
            self.numero_panneau += 1;
        }
        let t0 = if debut {
            std::time::Instant::now()
        } else {
            self.panneau.as_ref().map(|p| p.debut).unwrap_or_else(std::time::Instant::now)
        };
        self.panneau = Some(panneau::PanneauAnalyse { donnees, debut: t0, numero: self.numero_panneau });
    }

    /// Ce que dit le panneau de l'analyse (pour les essais)
    pub fn panneau_donnees(&self) -> Option<&Value> {
        self.panneau.as_ref().map(|p| &p.donnees)
    }

    /// Le panneau de l'analyse (le temps qui passe est relu par l'hôte une fois par seconde)
    pub fn page_panneau(&self) -> Option<Value> {
        self.panneau.as_ref().map(|p| p.page())
    }

    /// La note sur 100 de la dernière vérification (`$script:SecurityScore`)
    pub fn note(&self) -> Option<i64> {
        self.verif.as_ref().map(|v| v.note)
    }

    /// Un travail de fond attend sa fin (lecture, analyse, mise à jour de la base)
    pub fn occupe(&self) -> bool {
        self.lecture_en_cours.is_some()
            || self.analyse.is_some()
            || self.maj_base.is_some()
            || self.travaux.occupe()
            || self.choix.occupe()
    }

    /// `Update-SecurityTab` : la lecture lente part dans un fil à part, avec ce que l'Aide y ajoute (lot B du second
    /// audit : `lecture.securite` et `lecture.taches`, jusqu'à 60 s chacune, ne sont plus attendues sur le fil des
    /// blocs) ; la suite se fait au prochain tic
    fn relire(&mut self, ctx: &Contexte) {
        if self.lecture_en_cours.is_some() {
            return;
        }
        let Some(sys) = self.sys.clone() else { return };
        // (sans contexte partagé, seulement les tests d'un bloc seul et les juges : l'Aide est lue au tic, comme avant)
        let partage = ctx.partage_actuel();
        let (tx, rx) = channel();
        let ok = std::thread::Builder::new().name("protection-lecture".into()).spawn(move || {
            let mut l = sys.lire();
            let complete = match &partage {
                Some(c) => {
                    completer_par_aide(c, sys.as_ref(), &mut l);
                    true
                }
                None => false,
            };
            let _ = tx.send((l, complete));
        });
        if ok.is_ok() {
            self.lecture_en_cours = Some(rx);
        }
    }

    /// Une lecture finie (déjà complétée par l'Aide dans son fil, sauf sans contexte partagé), puis elle est jugée
    fn finir_lecture(&mut self, ctx: &Contexte, mut l: Lecture, complete: bool, effets: &mut Effets) {
        if !complete && let Some(sys) = self.sys.clone() {
            completer_par_aide(ctx, sys.as_ref(), &mut l);
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
        // (la note va à la tuile de l'accueil par « _accueil » dans les pages, et à la carte de Nevermind dans Mindset :
        //  `$script:SecurityScore` de `Write-EtatMindset`, écrit par l'hôte dans etat.json)
        effets.ajouter(Effet::Rafraichir);
        effets.ajouter(Effet::Signal { nom: "etat-mindset".into(), donnees: json!({ "protection": note }) });
    }

    fn defender_ok(&self) -> bool {
        self.verif.as_ref().is_some_and(|v| v.s.defender_ok())
    }

    /// `Invoke-DefenderScan`
    fn analyser(
        &mut self,
        ctx: &Contexte,
        effets: &mut Effets,
        sorte: TypeAnalyse,
        chemins: Vec<String>,
        titre: Option<String>,
    ) {
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
        // (les menaces trouvées sont lues par l'Aide dans ce fil aussi, jusqu'à 60 s : pas sur le fil des blocs, lot B)
        let partage = ctx.partage_actuel();
        let lance = std::thread::Builder::new().name("protection-analyse".into()).spawn(move || {
            let r = sys.analyser(sorte, &chemins, &a);
            let aide = match &partage {
                Some(c) if r.is_ok() && !a.load(Ordering::SeqCst) && !sys.est_admin() => {
                    Some(c.aide_lecture("lecture.securite", &json!({})))
                }
                Some(_) => Some(None),
                None => None,
            };
            let _ = tx.send(FinAnalyse { r, aide });
        });
        if lance.is_err() {
            return;
        }
        self.analyse = Some(AnalyseEnCours { fin: rx, arret, debut, titre: titre.clone() });
        self.montrer_panneau(
            json!({ "etat": "en-cours", "titre": titre, "sous": "Microsoft Defender", "texte": texte }),
        );
    }

    /// La fin d'une analyse : le verdict (jamais « Aucune menace » si le résultat n'a pas été lu)
    fn finir_analyse(&mut self, ctx: &Contexte, a: AnalyseEnCours, fin: FinAnalyse, effets: &mut Effets) {
        let FinAnalyse { r, aide: lue } = fin;
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
            let aide = match lue {
                _ if admin => None,
                Some(v) => v,
                None => ctx.aide_lecture("lecture.securite", &json!({})),
            };
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
        self.montrer_panneau(
            json!({ "etat": etat, "titre": a.titre, "sous": "Microsoft Defender", "resultat": resultat,
            "temps": format!("{:02}:{:02}", duree / 60, duree % 60), "lignes": lignes, "boutons": boutons }),
        );
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
            self.relire(ctx);
        }
    }

    /// Un geste d'un point à vérifier (le Script de son action), déjà confirmé
    fn geste(&mut self, ctx: &Contexte, c: &Constat, j: usize, effets: &mut Effets) {
        let Some(a) = c.actions.get(j).cloned() else { return };
        let ttl = c.titre.clone();
        let sys = self.sys.clone();
        let titre = format!("{} ({ttl})", a.texte);
        let mut rafraichir = !a.sans_rafraichir;
        // (Defender, l'Aide jusqu'à 60 s, le Planificateur : dans un travail de fond, audit G3 ; un autre geste en
        //  cours : rien n'est lancé, un double clic ne refait rien)
        if matches!(
            a.sorte,
            Sorte::SupprimerMenaces
                | Sorte::RetirerExclusions(_)
                | Sorte::ReactiverPareFeu
                | Sorte::DesactiverTaches(_)
        ) {
            if self.travaux.occupe() {
                ctx.journal(&format!("Sécurité : {titre} pas lancé, un geste est encore en cours"));
                return;
            }
            // (un geste qui change le PC : le verrou commun des gestes, sinon le clic est refusé ; lot A du second audit)
            let geste = match ctx.prendre_geste(nom_geste(&a.sorte), PAGE) {
                Ok(g) => g,
                Err(refus) => {
                    effets.ajouter(Effet::Annonce(refus));
                    return;
                }
            };
            let sorte = a.sorte.clone();
            self.travaux.lancer_geste(ctx, GESTE, geste, move |fond| {
                let (definitif, erreur, annonces) = geste_long(fond.ctx, sys.as_deref(), &sorte);
                FinGeste { titre, definitif, erreur, annonces, rafraichir }
            });
            self.relever_travaux(ctx, effets);
            return;
        }
        match &a.sorte {
            Sorte::Ouvrir(u) => effets.ajouter(Effet::OuvrirAdresse(u.clone())),
            Sorte::Planificateur => effets.ajouter(Effet::OuvrirAdresse(nevermind_noyau::programme_windows("taskschd.msc"))),
            Sorte::MettreAJourBase => {
                self.mettre_a_jour_base();
                rafraichir = false;
            }
            Sorte::ReactiverUac => effets.ajouter(Effet::Signal {
                nom: "registre-ecrire".into(),
                donnees: json!({ "Path": "HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System", "Name": "EnableLUA",
                    "Value": 1, "Kind": "DWord", "titre": titre }),
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
                self.analyser(ctx, effets, TypeAnalyse::Personnalisee, chemins.clone(), Some(titre.clone()));
                rafraichir = false;
            }
            Sorte::DesactiverDemarrage(l) => effets.ajouter(Effet::Signal {
                nom: "demarrage-desactiver".into(),
                donnees: json!({ "elements": l.iter().map(|d| json!({ "Nom": d.nom, "Exe": d.exe })).collect::<Vec<_>>(),
                    "titre": titre }),
            }),
            Sorte::SupprimerMenaces | Sorte::RetirerExclusions(_) | Sorte::ReactiverPareFeu | Sorte::DesactiverTaches(_) => {}
        }
        self.fin_geste(
            ctx,
            effets,
            FinGeste { titre, definitif: None, erreur: None, annonces: Vec::new(), rafraichir },
        );
    }

    /// La fin d'un geste (sur le fil des blocs) : le message d'erreur, ou l'historique ; puis la protection relue
    fn fin_geste(&mut self, ctx: &Contexte, effets: &mut Effets, f: FinGeste) {
        for a in f.annonces {
            effets.ajouter(a);
        }
        if let Some(e) = f.erreur {
            // (Invoke-Safe : un message écrit pour l'utilisateur, montré tel quel après cette phrase)
            effets.ajouter(Effet::Annonce(format!(
                "Oups, quelque chose s'est mal passé :

{e}"
            )));
            return;
        }
        if let Some((quoi, retour)) = f.definitif {
            // (Add-ChangementDefinitif puis Add-History : écrit par le bloc qui tient l'historique)
            effets.ajouter(Effet::Signal {
                nom: "historique".into(),
                donnees: json!({ "titre": f.titre, "details": [],
                    "log": [{ "Type": "definitif", "Quoi": quoi, "Retour": retour }] }),
            });
        }
        if f.rafraichir && self.analyse.is_none() {
            self.relire(ctx);
        }
    }

    /// Ce que les gestes longs ont rendu (sur le fil des blocs)
    fn relever_travaux(&mut self, ctx: &Contexte, effets: &mut Effets) {
        for r in self.travaux.relever() {
            match r {
                Rendu::Effet(e) => effets.ajouter(e),
                Rendu::Fini { resultat: Ok(f), .. } => self.fin_geste(ctx, effets, f),
                Rendu::Fini { resultat: Err(e), .. } => {
                    effets.ajouter(Effet::Annonce(format!("Oups, quelque chose s'est mal passé :\n\n{e}")))
                }
            }
        }
        // (le dossier choisi dans la fenêtre : son analyse, comme si l'utilisateur l'avait donné tout de suite)
        for r in self.choix.relever() {
            match r {
                Rendu::Effet(e) => effets.ajouter(e),
                Rendu::Fini { resultat: Ok(Some(d)), .. } => {
                    self.action(ctx, "sec-analyser-dossier", &json!(d), effets);
                }
                Rendu::Fini { resultat: Ok(None), .. } => {}
                Rendu::Fini { resultat: Err(e), .. } => {
                    effets.ajouter(Effet::Annonce(format!("Oups, quelque chose s'est mal passé :\n\n{e}")))
                }
            }
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
        // La tuile « Protection » de l'accueil ($script:SecurityScore)
        let mut p = p;
        if let Some(pa) = self.page_panneau() {
            p.insert("panneau".into(), pa);
        }
        p.insert("_accueil".into(), json!({ "protection": { "score": self.verif.as_ref().map(|v| v.note) } }));
        p
    }

    fn action(&mut self, ctx: &Contexte, id: &str, valeur: &Value, effets: &mut Effets) -> bool {
        // Les boutons du panneau d'une analyse (seulement quand il est ouvert : sinon ceux du bloc qui l'a)
        if let Some(p) = &self.panneau
            && id.starts_with("panneau:")
        {
            match id {
                "panneau:stop" => {
                    if let Some(a) = &self.analyse {
                        a.arret.store(true, Ordering::SeqCst);
                    }
                }
                "panneau:close" => {
                    if !p.en_cours() {
                        self.panneau = None;
                        effets.ajouter(Effet::Rafraichir);
                    }
                }
                _ => {
                    let b =
                        id.strip_prefix("panneau:b:").and_then(|n| n.parse::<usize>().ok()).and_then(|n| p.bouton(n));
                    match b {
                        Some(b) => return self.action(ctx, &b, &Value::Null, effets),
                        None => return false,
                    }
                }
            }
            return true;
        }
        match id {
            "clic:BtnSecRefresh" => {
                self.relire(ctx);
                return true;
            }
            "clic:BtnScanQuick" | "clic:BtnScanFull" | "clic:BtnScanFolder" | "clic:BtnScanUpdate" => {
                // (les boutons d'analyse ne marchent qu'avec Microsoft Defender, et pas pendant une analyse)
                if !self.defender_ok() || self.analyse.is_some() {
                    effets.ajouter(Effet::Annonce(OCCUPE.into()));
                    return true;
                }
                match id {
                    "clic:BtnScanQuick" => self.analyser(ctx, effets, TypeAnalyse::Rapide, Vec::new(), None),
                    "clic:BtnScanFull" => {
                        if confirme(valeur) {
                            self.analyser(ctx, effets, TypeAnalyse::Complete, Vec::new(), None);
                        } else {
                            question(
                                effets,
                                id,
                                "L'analyse complète vérifie tous les fichiers du PC : elle peut durer une heure ou plus. Tu peux continuer à utiliser ton PC pendant ce temps. Lancer l'analyse ?",
                                valeur,
                            );
                        }
                    }
                    // (Invoke-FolderScan : la fenêtre de Windows pour choisir le dossier, puis son analyse)
                    // (la fenêtre attend l'utilisateur sans limite : dans un fil à part, jamais sur le fil des blocs ;
                    //  la suite vient au relevé avec le dossier choisi. Déjà ouverte : pas une deuxième.)
                    "clic:BtnScanFolder" => {
                        if self.choix.en_cours(CHOIX) {
                            effets.ajouter(Effet::Annonce(CHOIX_OUVERT.into()));
                            return true;
                        }
                        let Some(sys) = self.sys.clone() else { return true };
                        self.choix.lancer(ctx, CHOIX, move |_| sys.choisir_dossier("Choisis le dossier à analyser"));
                        self.relever_travaux(ctx, effets);
                    }
                    _ => self.mettre_a_jour_base(),
                }
                return true;
            }
            "sec-analyser-dossier" => {
                let d = regles::dossier_a_analyser(valeur.as_str().unwrap_or_default());
                if !d.is_empty() && self.defender_ok() {
                    let nom = regles::nom_du_dossier(&d);
                    self.analyser(
                        ctx,
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
                    let _geste = match ctx.prendre_geste("Supprimer les menaces", PAGE) {
                        Ok(g) => g,
                        Err(refus) => {
                            effets.ajouter(Effet::Annonce(refus));
                            return true;
                        }
                    };
                    match self.sys.as_ref().map(|s| s.supprimer_menaces()) {
                        Some(Err(e)) => effets.ajouter(Effet::Annonce(e)),
                        _ => {
                            effets.ajouter(Effet::Annonce("Menaces supprimées.".into()));
                            self.relire(ctx);
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
            // (le « Oui » d'une question : le point retrouvé par ce qu'il est, où qu'il soit dans la liste à jour)
            if confirme(valeur) {
                let cle = valeur.get("id").and_then(Value::as_str).unwrap_or_default();
                let trouve = self.points.iter().find_map(|c| {
                    c.actions
                        .iter()
                        .position(|a| a.confirmer.is_some() && empreinte_geste(c, a) == cle)
                        .map(|j| (c.clone(), j))
                });
                match trouve {
                    Some((c, j)) => self.geste(ctx, &c, j, effets),
                    None => {
                        ctx.journal(&format!("Sécurité : {id} pas fait, le point a changé pendant la question"));
                        effets.ajouter(Effet::Annonce(POINT_CHANGE.into()));
                    }
                }
                return true;
            }
            let Some(c) = self.points.get(k).cloned() else { return true };
            let Some(a) = c.actions.get(j) else { return true };
            if let Some(t) = &a.confirmer {
                question_sur(effets, id, &empreinte_geste(&c, a), t, valeur);
                return true;
            }
            self.geste(ctx, &c, j, effets);
            return true;
        }
        if let Some(k) = id.strip_prefix("sec-moi:").and_then(|x| x.parse::<usize>().ok()) {
            if confirme(valeur) {
                let cle = valeur.get("id").and_then(Value::as_str).unwrap_or_default();
                match self.points.iter().find(|c| !c.confiance.is_empty() && empreinte_confiance(c) == cle).cloned() {
                    Some(c) => self.approuver(ctx, &c.confiance, effets),
                    None => {
                        ctx.journal(&format!("Sécurité : {id} pas fait, le point a changé pendant la question"));
                        effets.ajouter(Effet::Annonce(POINT_CHANGE.into()));
                    }
                }
                return true;
            }
            let Some(c) = self.points.get(k).cloned() else { return true };
            let n = c.confiance.len();
            if n == 0 {
                return true;
            }
            let t = if n > 1 {
                format!(
                    "Marquer ces {n} éléments comme sûrs ? Ils ne compteront plus dans le niveau de protection. S'ils changent (autre commande, autre fichier), Nevermind les signalera de nouveau."
                )
            } else {
                "Marquer cet élément comme sûr ? Il ne comptera plus dans le niveau de protection. S'il change (autre commande, autre fichier), Nevermind le signalera de nouveau.".to_string()
            };
            question_sur(effets, id, &empreinte_confiance(&c), &t, valeur);
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

    fn travaux_finis(&mut self, ctx: &Contexte, effets: &mut Effets) {
        self.relever_travaux(ctx, effets);
    }

    fn tic(&mut self, ctx: &Contexte, _cache: bool, effets: &mut Effets) -> Option<Duration> {
        self.relever_travaux(ctx, effets);
        if let Some(t) = self.occupe_texte.take() {
            effets.ajouter(Effet::Signal { nom: "occupe".into(), donnees: json!({ "texte": t }) });
        }
        if !self.demarre {
            self.demarre = true;
            self.relire(ctx);
        }
        if let Some(rx) = &self.lecture_en_cours {
            match rx.try_recv() {
                Ok((l, complete)) => {
                    self.lecture_en_cours = None;
                    self.finir_lecture(ctx, l, complete, effets);
                }
                Err(TryRecvError::Disconnected) => self.lecture_en_cours = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(a) = &self.analyse {
            let r = match a.fin.try_recv() {
                Ok(r) => Some(r),
                Err(TryRecvError::Disconnected) => {
                    Some(FinAnalyse { r: Err("l'analyse s'est arrêtée".into()), aide: None })
                }
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
                    self.relire(ctx);
                }
                Err(TryRecvError::Disconnected) => self.maj_base = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        // (rien ne tourne en continu : un coup d'œil deux fois par seconde seulement quand un travail de fond attend)
        if self.occupe() { Some(Duration::from_millis(500)) } else { Some(Duration::from_secs(60)) }
    }

    fn signal(&mut self, ctx: &Contexte, nom: &str, donnees: &Value, effets: &mut Effets) {
        match nom {
            // (sauvegarde.json est relu quand il change ; ce signal commun redessine la page)
            "sauvegarde-changee" => {}
            "sauvegarde-nombre" => {
                self.sauvegarde_nombre = donnees.get("n").unwrap_or(donnees).as_u64().map(|n| n as usize)
            }
            "protection-relire" => self.relire(ctx),
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
                self.analyser(ctx, effets, sorte, chemins, titre);
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

/// Ce que rend un geste long d'un point à vérifier (audit G3)
struct FinGeste {
    titre: String,
    /// Le changement définitif à noter dans l'historique (quoi, comment revenir)
    definitif: Option<(String, String)>,
    erreur: Option<String>,
    annonces: Vec<Effet>,
    rafraichir: bool,
}

/// La clé des gestes longs : un seul à la fois
const GESTE: &str = "geste";

/// La clé de la fenêtre « Choisis le dossier à analyser »
const CHOIX: &str = "choix-dossier";

/// Un deuxième clic sur « Analyser un dossier » pendant que la fenêtre est ouverte
const CHOIX_OUVERT: &str =
    "La fenêtre pour choisir le dossier est déjà ouverte : choisis le dossier dedans, ou ferme-la.";

/// La page, dans « Attends la fin de « Réactiver le pare-feu » (Protection) »
const PAGE: &str = "Protection";

/// Le nom d'un geste long, dans « Attends la fin de ... »
fn nom_geste(s: &Sorte) -> &'static str {
    match s {
        Sorte::SupprimerMenaces => "Supprimer les menaces",
        Sorte::RetirerExclusions(_) => "Retirer les exclusions",
        Sorte::ReactiverPareFeu => "Réactiver le pare-feu",
        Sorte::DesactiverTaches(_) => "Désactiver les tâches",
        _ => "Un réglage de la sécurité",
    }
}

/// Ce que l'Aide ajoute à une lecture (sans droits) : `lecture.securite`, puis `lecture.taches`. Jusqu'à 60 s chacune
/// (plus le réveil de sa tâche) : appelée dans le fil de la lecture, jamais sur le fil des blocs (lot B).
fn completer_par_aide(ctx: &Contexte, sys: &dyn Systeme, l: &mut Lecture) {
    if sys.est_admin() {
        return;
    }
    let lecture = ctx.aide_lecture("lecture.securite", &json!({}));
    if lecture.is_none() {
        return;
    }
    let taches = ctx.aide_sans_demande("lecture.taches", &json!({}), 60_000);
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

/// Un geste long (Defender, l'Aide, le Planificateur), fait dans un travail de fond : le changement définitif,
/// l'erreur, les messages
fn geste_long(
    ctx: &Contexte,
    sys: Option<&dyn Systeme>,
    sorte: &Sorte,
) -> (Option<(String, String)>, Option<String>, Vec<Effet>) {
    if let Err(e) = ctx.exiger_geste() {
        return (None, Some(e), Vec::new());
    }
    let mut definitif: Option<(String, String)> = None;
    let mut erreur: Option<String> = None;
    let mut annonces = Vec::new();
    match sorte {
        Sorte::SupprimerMenaces => {
            if let Some(s) = sys
                && let Err(e) = s.supprimer_menaces()
            {
                erreur = Some(e);
            }
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
            Ok(_) => definitif = Some((
                "Pare-feu réactivé sur tous les réseaux".into(),
                "Sécurité Windows, « Pare-feu et protection du réseau » : choisir le réseau et désactiver le pare-feu."
                    .into(),
            )),
            Err(e) => erreur = Some(e),
        },
        Sorte::DesactiverTaches(l) => {
            let (mut faites, mut ratees) = (Vec::new(), Vec::new());
            if let Some(s) = sys {
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
                Ok(Some(m)) => annonces.push(Effet::Annonce(m)),
                Ok(None) => {}
                Err(m) => erreur = Some(m),
            }
        }
        _ => {}
    }
    (definitif, erreur, annonces)
}
