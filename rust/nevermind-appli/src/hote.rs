//! L'hôte des blocs : l'écran de Nevermind assemblé comme accueil-console.ps1 (`Get-EcranNevermind`,
//! `Update-EcranNevermind`, `Register-ActionsNevermind`, `Step-MinuterieKit`), le panneau de détail déplié
//! (ecran-panneau.ps1, `Update-PanneauKit`, `Close-QuitteKit`), et ce que vue.ps1 fait des messages de la page
//! (`Receive-MessageVue`, `Send-OrdreVue` et sa file pendant la veille).
//!
//! Sans Tauri ni Windows : la page est derrière le trait [`Ecran`], le temps est donné par l'appelant. Les tests (et
//! les juges, tests\juges\hote-*.json) font tourner des blocs factices.
//!
//! Ce que les blocs donnent à l'hôte, en plus de leurs pages (clés réservées de `Bloc::pages`) :
//! - `"_accueil"` : les faits des tuiles de l'accueil, que l'hôte met en mots comme `Get-EcranNevermind`
//!   (`{ "pc": { "score", "a_regler" }, "perf": { "sous" }, "protection": { "score" },
//!   "jeux": { "nombre", "dernier", "fps", "image" }, "reseau": { "appareils", "scan" } }`) ;
//! - `"panneau"` : le panneau de détail ouvert (une page du kit : titre, sous, icone, module, blocs), avec `"numero"`
//!   qui change à chaque nouvelle ouverture (`$script:Kit.PanneauNumero`) ; l'hôte y mène, pose `parent` et
//!   `retour`, et revient en arrière quand il disparaît. Ses boutons (`panneau:stop`, `panneau:again`,
//!   `panneau:close`, `panneau:b:<n>`) vont aux blocs comme toute action ;
//! - une valeur texte `"fichier:<chemin complet>"` n'importe où dans les pages : une image du PC (une jaquette),
//!   remplacée par son adresse `http://fichier.localhost/<jeton>` (`Get-ImageVue`).
//!
//! Ce que l'hôte dit aux blocs (`Bloc::signal`) : `"ecran"` `{ "id" }` à chaque écran montré, `"mindset"`
//! `{ "accroche" }` quand Nevermind est accroché dans Mindset ou décroché. Ce qu'il écoute (`Effet::Signal`) :
//! `"partie-commencee"` et `"partie-finie"` (pendant une partie, Nevermind n'est regardé que s'il est devant).

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::time::{Duration, Instant};

use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use serde_json::{Map, Value, json};

use crate::pack::Images;
use crate::pont::MessagePage;
use crate::theme::Habillage;

/// La page, vue de l'hôte.
pub trait Ecran {
    /// Un nouvel état complet (`Send-Vue`) : gardé, et envoyé si la page suit.
    fn etat(&mut self, etat: Value);
    /// Un ordre au kit (`{ kit: 'aller' | 'message' | 'mesures' | 'question' ... }`).
    fn ordre(&mut self, ordre: Value);
}

/// Ce que l'hôte demande au programme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Demande {
    /// Quitter Nevermind (« Quitter » de la question, `Effet::Quitter`)
    Quitter,
    /// « Réduire » de la question : la fenêtre se range (`Hide-ToTray`)
    Reduire,
    /// Retour au menu de Mindset (`Request-MenuMindset`)
    MenuMindset,
    /// Une adresse ou un dossier (`Open-Url`)
    OuvrirAdresse(String),
    /// Une partie commence ou finit (signal d'un bloc)
    Partie(bool),
}

/// Les pages qui ne sont préparées qu'à l'écran : refaites en y arrivant (accueil-console.ps1:18).
pub const PAGES_LOURDES: [&str; 4] = ["optimisation", "jeux", "jeu", "tests"];
/// Les pages d'avant (`page:<n>`) et leur page du kit (accueil-console.ps1:139-140).
pub const PAGES_INDEX: [(u32, &str); 12] = [
    (0, "tableau"),
    (1, "optimisation"),
    (2, "demarrage"),
    (3, "connexion"),
    (4, "nettoyage"),
    (5, "tests"),
    (6, "securite"),
    (7, "sauvegarde"),
    (9, "jeux"),
    (10, "reseau"),
    (11, "trafic"),
    (12, "overlay"),
];
/// La file des ordres pendant que la page n'est pas regardée (vue.ps1:280 : les 20 derniers).
pub const FILE_MAX: usize = 20;
/// La connexion est relue au plus toutes les 30 s (vue.ps1:383).
pub const CONNEXION: Duration = Duration::from_secs(30);
/// Le panneau ouvert est relu au plus une fois par seconde (ecran-panneau.ps1:220 : 900 ms).
pub const PANNEAU: Duration = Duration::from_millis(900);
/// Un bloc qui a planté dans son tic est rappelé au plus tôt après ce délai.
pub const APRES_ERREUR: Duration = Duration::from_secs(10);

/// Les faits des tuiles de l'accueil, donnés par les blocs (`"_accueil"`).
#[derive(Debug, Default, Clone)]
struct Accueil(Map<String, Value>);

impl Accueil {
    fn fusionner(&mut self, v: &Value) {
        let Value::Object(o) = v else { return };
        for (tuile, champs) in o {
            let Value::Object(c) = champs else { continue };
            let e = self.0.entry(tuile.clone()).or_insert_with(|| json!({}));
            if let Value::Object(e) = e {
                for (k, x) in c {
                    e.insert(k.clone(), x.clone());
                }
            }
        }
    }
    fn champ(&self, tuile: &str, nom: &str) -> Option<&Value> {
        self.0.get(tuile).and_then(|t| t.get(nom)).filter(|v| !v.is_null())
    }
    fn entier(&self, tuile: &str, nom: &str) -> Option<i64> {
        self.champ(tuile, nom).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(arrondi_pair)))
    }
    fn texte(&self, tuile: &str, nom: &str) -> Option<String> {
        self.champ(tuile, nom).and_then(Value::as_str).map(str::to_string)
    }
}

/// `[int]` de .NET : arrondi au pair le plus proche.
fn arrondi_pair(x: f64) -> i64 {
    let r = x.round();
    (if (x - x.trunc()).abs() == 0.5 { 2.0 * (x / 2.0).round() } else { r }) as i64
}

/// `'{0:N0}' -f $x` dans la culture française de Windows : arrondi loin de zéro, milliers séparés par une espace fine
/// insécable (U+202F).
pub fn format_n0(x: f64) -> String {
    let n = x.abs().round() as u64;
    let s = n.to_string();
    let mut r = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            r.push('\u{202F}');
        }
        r.push(c);
    }
    if x < 0.0 && n > 0 { format!("-{r}") } else { r }
}

/// L'état de l'écran que l'hôte garde (`$script:Kit` et `$script:Vue` des applis PowerShell).
#[derive(Debug, Default)]
struct Kit {
    pret: bool,
    /// `$script:Vue.Ecran` : l'écran montré ("" avant le premier)
    ecran: String,
    /// La clé du dernier état envoyé (`$script:Kit.Envoye`)
    envoye: String,
    a_refaire: bool,
    focus: Option<String>,
    retour_ecran: Option<String>,
    accrochee: bool,
    regardee: bool,
    tic: u64,
    /// La question « Quitter Nevermind ? » en attente de réponse (son id)
    question: Option<String>,
    panneau: Option<Value>,
    panneau_ouvert: bool,
    panneau_de: Option<String>,
    panneau_numero: Option<Value>,
    panneau_montre: bool,
    panneau_vu: Option<Duration>,
    /// Le bloc qui donne la page « pc » (ses mesures en direct)
    bloc_pc: Option<usize>,
    file: VecDeque<Value>,
    partie: bool,
}

/// L'état d'un service : son démarrage et s'il tourne (lecture de Windows, donnée par le programme).
pub type EtatService = Box<dyn Fn(&str) -> Option<(String, bool)> + Send>;

/// Les blocs et ce qui les entoure.
pub struct Hote {
    blocs: Vec<Box<dyn Bloc>>,
    pub ctx: Contexte,
    habillage: Habillage,
    phrase: String,
    version: String,
    images: Images,
    connexion: Box<dyn FnMut() -> String + Send>,
    connexion_lue: Option<(Duration, String)>,
    k: Kit,
    tics: Vec<Option<Duration>>,
    demandes: Vec<Demande>,
    /// Construction d'essai : l'état envoyé est écrit ici (etat.json), comme `$env:ESSAI_DOSSIER`
    pub dossier_etat: Option<std::path::PathBuf>,
    /// Les questions des blocs en attente de réponse (signal « question »)
    questions: Vec<communs::QuestionBloc>,
    /// Les réponses à donner aux blocs (après le geste ou le tic en cours)
    a_router: Vec<(String, Value)>,
    /// La fiche de résultat (signal « resultat », `Show-ResultSheet`)
    fiche: communs::Fiche,
    /// Ce que l'hôte sait du PC pour « Signaler un problème » (assistance du noyau)
    infos: nevermind_noyau::assistance::InfosSignalement,
    /// Les bulles envoyées à Mindset et ce que fait leur clic (`$script:NotifyActions`)
    notifications: Vec<(String, String, Value, String)>,
    /// L'état d'un service (son démarrage comme .NET : « Automatic », « Manual »..., et s'il tourne), pour
    /// `Set-ServiceOff` ; donné par le programme (lecture seule de Windows)
    pub etat_service: EtatService,
}

#[path = "hote_communs.rs"]
mod communs;

/// Un appel à un bloc, protégé : un bloc qui plante n'arrête pas Nevermind (`Invoke-Safe`).
fn protege<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|e| {
        e.downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| e.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "erreur inconnue".into())
    })
}

/// Un id de question (10 chiffres hexadécimaux, `[guid]::NewGuid().ToString('N').Substring(0, 10)`).
fn id_question() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    format!("{:016x}", h.finish())[..10].to_string()
}

/// `-replace '":\{\}', '":null'` (vue.ps1:285, accueil-console.ps1:118) : un objet vide, valeur d'un champ, devient
/// `null` (les objets vides d'un tableau restent).
pub fn vides_en_null(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for x in o.values_mut() {
                if x.as_object().is_some_and(Map::is_empty) {
                    *x = Value::Null;
                } else {
                    vides_en_null(x);
                }
            }
        }
        Value::Array(l) => l.iter_mut().for_each(vides_en_null),
        _ => {}
    }
}

/// La clé de comparaison d'un état (`Update-EcranNevermind`, accueil-console.ps1:114-119) : les mesures en direct
/// (blocs « mesures ») ne comptent que par leurs ids.
pub fn cle_etat(etat: &Value) -> String {
    let mut e = etat.clone();
    if let Some(Value::Object(pages)) = e.get_mut("pages") {
        for p in pages.values_mut() {
            let Value::Object(p) = p else { continue };
            let mut listes: Vec<&mut Value> = Vec::new();
            for (k, v) in p.iter_mut() {
                if k == "blocs" {
                    listes.push(v);
                } else if k == "sections"
                    && let Value::Array(sections) = v
                {
                    for s in sections.iter_mut() {
                        if let Some(b) = s.get_mut("blocs") {
                            listes.push(b);
                        }
                    }
                }
            }
            for l in listes {
                let Value::Array(l) = l else { continue };
                for b in l.iter_mut() {
                    if b.get("type").and_then(Value::as_str) == Some("mesures")
                        && let Some(Value::Array(el)) = b.get_mut("elements")
                    {
                        *el = el.iter().map(|x| x.get("id").cloned().unwrap_or(Value::Null)).collect();
                    }
                }
            }
        }
    }
    vides_en_null(&mut e);
    e.to_string()
}

/// La page `jeux-lag` du bloc trafic (le lag d'une partie) va dans l'onglet « lag » de l'onglet « perf » de la page
/// Jeux, puis elle est retirée de l'état (si le bloc jeux ne l'a pas déjà placée).
pub fn placer_lag(pages: &mut Map<String, Value>) {
    let Some(Value::Object(lag)) = pages.remove("jeux-lag") else { return };
    let cible = pages
        .get_mut("jeux")
        .and_then(|j| j.get_mut("onglets"))
        .and_then(Value::as_array_mut)
        .and_then(|l| l.iter_mut().find(|o| o.get("id").and_then(Value::as_str) == Some("perf")))
        .and_then(|p| p.get_mut("onglets"))
        .and_then(Value::as_array_mut)
        .and_then(|l| l.iter_mut().find(|o| o.get("id").and_then(Value::as_str) == Some("lag")))
        .and_then(Value::as_object_mut);
    if let Some(o) = cible {
        for (k, v) in lag {
            o.insert(k, v);
        }
    }
}

impl Hote {
    pub fn nouveau(
        blocs: Vec<Box<dyn Bloc>>,
        ctx: Contexte,
        habillage: Habillage,
        phrase: String,
        version: &str,
        images: Images,
        connexion: Box<dyn FnMut() -> String + Send>,
    ) -> Hote {
        let n = blocs.len();
        Hote {
            blocs,
            ctx,
            habillage,
            phrase,
            version: version.to_string(),
            images,
            connexion,
            connexion_lue: None,
            k: Kit { regardee: true, ..Kit::default() },
            tics: vec![Some(Duration::ZERO); n],
            demandes: Vec::new(),
            dossier_etat: None,
            questions: Vec::new(),
            a_router: Vec::new(),
            fiche: communs::Fiche::default(),
            infos: Default::default(),
            notifications: Vec::new(),
            etat_service: Box::new(|_| None),
        }
    }

    pub fn accrochee(&self) -> bool {
        self.k.accrochee
    }
    pub fn pret(&self) -> bool {
        self.k.pret
    }
    pub fn ecran_montre(&self) -> &str {
        &self.k.ecran
    }
    pub fn partie(&self) -> bool {
        self.k.partie
    }
    /// L'écran où aller à l'arrivée de la page (`$script:Kit.RetourEcran`).
    pub fn retour_ecran(&self) -> Option<&str> {
        self.k.retour_ecran.as_deref()
    }

    fn demandes(&mut self) -> Vec<Demande> {
        std::mem::take(&mut self.demandes)
    }

    // -----------------------------------------------------------------------------------------------------------
    // Ce que montre l'écran
    // -----------------------------------------------------------------------------------------------------------

    /// La connexion (`Get-ConnexionVue`), relue au plus toutes les 30 s.
    fn connexion(&mut self, maintenant: Duration) -> String {
        if let Some((t, v)) = &self.connexion_lue
            && maintenant.saturating_sub(*t) < CONNEXION
        {
            return v.clone();
        }
        let v = (self.connexion)();
        self.connexion_lue = Some((maintenant, v.clone()));
        v
    }

    /// Les chemins `fichier:` des pages remplacés par leur adresse (`Get-ImageVue`).
    fn images_des_pages(&self, v: &mut Value) {
        match v {
            Value::String(s) => {
                if let Some(c) = s.strip_prefix("fichier:")
                    && Path::new(c).is_absolute()
                {
                    *s = self.images.adresse(Path::new(c));
                }
            }
            Value::Array(l) => l.iter_mut().for_each(|x| self.images_des_pages(x)),
            Value::Object(o) => o.values_mut().for_each(|x| self.images_des_pages(x)),
            _ => {}
        }
    }

    /// Les tuiles de l'accueil (accueil-console.ps1:33-95) d'après les faits des blocs.
    fn accueil(&self, a: &Accueil) -> Value {
        let score = a.entier("pc", "score");
        let todo = a.entier("pc", "a_regler").unwrap_or(0).max(0);
        let resume = match (score, todo) {
            (None, _) => "Analyse en cours...".to_string(),
            (_, 1) => "1 point à régler".to_string(),
            (_, n) if n > 1 => format!("{n} points à régler"),
            _ => "Au top pour jouer".to_string(),
        };
        let sec = a.entier("protection", "score");
        let mut jeux =
            json!({ "id": "jeux", "nom": "Jeux", "icone": "manette", "sous": "Ta bibliothèque", "couleur": "S" });
        if let Some(n) = a.entier("jeux", "nombre") {
            jeux["sous"] = json!(format!("{n} jeu{}", if n > 1 { "x" } else { "" }));
            if let Some(nom) = a.texte("jeux", "dernier") {
                if let Some(f) = a.texte("jeux", "image").filter(|f| Path::new(f).is_absolute()) {
                    jeux["fond"] = json!(self.images.adresse(Path::new(&f)));
                }
                let mut sous = nom;
                if let Some(fps) = a.champ("jeux", "fps").and_then(Value::as_f64).filter(|f| *f != 0.0) {
                    sous.push_str(&format!(" · {} FPS", format_n0(fps)));
                }
                jeux["sous"] = json!(sous);
            }
        }
        let nb = a.entier("reseau", "appareils").unwrap_or(0);
        let reseau = if nb > 0 {
            format!("{nb} appareil{}", if nb > 1 { "s" } else { "" })
        } else if a.champ("reseau", "scan").and_then(Value::as_bool) == Some(true) {
            "Scan en cours".to_string()
        } else {
            "Pas encore scanné".to_string()
        };
        let perf = a.texte("perf", "sous").filter(|s| !s.is_empty()).unwrap_or_else(|| "Pour gagner des FPS".into());
        let mut ronds = vec![
            json!({ "id": "recherche", "nom": "Rechercher", "icone": "loupe", "couleur": "S" }),
            json!({ "id": "parametres", "nom": "Paramètres", "icone": "reglages", "couleur": "doux" }),
            json!({ "id": "aide", "nom": "Aide", "icone": "aide", "couleur": "T" }),
        ];
        if self.k.accrochee {
            ronds.push(json!({ "id": "menu", "nom": "Menu de Mindset", "icone": "maison", "couleur": "S" }));
            ronds.push(json!({ "id": "fermer", "nom": "Fermer Nevermind", "icone": "croix", "couleur": "doux" }));
        }
        json!({
            "tuiles": [
                { "id": "pc", "nom": "Ton PC", "gros": score.map_or(json!("..."), |s| json!(s)),
                  "petit": if score.is_some() { "/100" } else { "" }, "sous": resume, "couleur": "P", "alerte": todo > 0 },
                { "id": "perf", "nom": "Performances", "icone": "eclair", "sous": perf, "couleur": "P" },
                { "id": "protection", "nom": "Protection", "gros": sec, "petit": sec.map(|_| "/100"), "icone": "bouclier",
                  "sous": "Pour rester tranquille", "couleur": "T", "alerte": sec.is_some_and(|s| s < 50) },
                jeux,
                { "id": "reseau", "nom": "Réseau", "icone": "radar", "sous": reseau, "couleur": "S" },
                { "id": "trafic", "nom": "Trafic", "icone": "courbe", "sous": "Ce qui sort de ton PC", "couleur": "T" },
                { "id": "overlay", "nom": "Overlay", "icone": "jauge", "sous": "Tes FPS en jeu", "couleur": "vert" },
            ],
            "ronds": ronds,
        })
    }

    /// Le panneau donné par un bloc (ou plus de panneau) : `Update-PanneauKit` (ecran-panneau.ps1:214-236). Rend
    /// l'écran où aller.
    fn suivre_panneau(&mut self, donne: Option<(usize, Value)>) -> Option<String> {
        let Some((_, mut p)) = donne else {
            if !self.k.panneau_ouvert {
                return None;
            }
            self.k.panneau_ouvert = false;
            self.k.panneau = None;
            self.k.panneau_vu = None;
            self.k.panneau_montre = false;
            self.k.panneau_numero = None;
            if self.k.ecran == "panneau" {
                let de = self.k.panneau_de.clone().filter(|d| d != "panneau" && d != "fiche");
                return Some(de.unwrap_or_else(|| "accueil".into()));
            }
            return None;
        };
        let numero = p.as_object_mut().and_then(|o| o.remove("numero"));
        let nouveau = numero != self.k.panneau_numero;
        let aller = !self.k.panneau_ouvert || (nouveau && self.k.ecran != "panneau");
        if aller {
            self.k.panneau_ouvert = true;
            self.k.panneau_de = Some(if self.k.ecran.is_empty() { "accueil".into() } else { self.k.ecran.clone() });
        }
        self.k.panneau_numero = numero;
        let parent = self.k.panneau_de.clone().filter(|d| !["accueil", "panneau", "fiche"].contains(&d.as_str()));
        if let Value::Object(o) = &mut p {
            o.insert("retour".into(), json!("appli"));
            o.insert("parent".into(), parent.map_or(Value::Null, Value::String));
        }
        self.k.panneau = Some(p);
        aller.then(|| "panneau".to_string())
    }

    /// `Get-EcranNevermind` (accueil-console.ps1:33-103), puis ce que `Send-Vue` ajoute. Rend aussi l'écran où aller
    /// (le panneau qui s'ouvre ou se ferme).
    fn construire(&mut self, maintenant: Duration) -> (Value, Option<String>) {
        let chrono = Instant::now();
        let mut pages = Map::new();
        let mut accueil = Accueil::default();
        let mut panneau = None;
        let ecran = self.k.ecran.clone();
        for i in 0..self.blocs.len() {
            let debut = Instant::now();
            let (b, ctx) = (&mut self.blocs[i], &self.ctx);
            let nom = b.nom();
            match protege(|| b.pages(ctx, &ecran)) {
                Ok(p) => {
                    for (k, v) in p {
                        match k.as_str() {
                            "_accueil" => accueil.fusionner(&v),
                            "panneau" => {
                                if panneau.is_none() {
                                    panneau = Some((i, v));
                                }
                            }
                            _ => {
                                if k == "pc" {
                                    self.k.bloc_pc = Some(i);
                                }
                                pages.insert(k, v);
                            }
                        }
                    }
                }
                Err(e) => self.ctx.journal(&format!("Écran, {nom} : {e}")),
            }
            let ms = debut.elapsed().as_millis();
            if ms > 120 {
                self.ctx.journal(&format!("Écran : {nom} en {ms} ms"));
            }
        }
        // L'assistance du noyau (visite guidée, nouveautés, « Signaler un problème ») : sa fiche passe avant celle
        // des blocs, son panneau avant les leurs (c'est l'utilisateur qui vient de l'ouvrir)
        let assistance = nevermind_noyau::assistance::pages_assistance(&self.ctx, &ecran);
        if let Some(f) = assistance.get("fiche") {
            pages.insert("fiche".into(), f.clone());
        }
        if let Some(p) = assistance.get("panneau") {
            panneau = Some((usize::MAX, p.clone()));
        }
        self.infos.score = accueil.entier("pc", "score").map(|s| s.to_string());
        self.infos.protection = accueil.entier("protection", "score");
        self.infos.page = ecran.clone();
        placer_lag(&mut pages);
        let mut pages = Value::Object(pages);
        self.images_des_pages(&mut pages);
        let aller = self.suivre_panneau(panneau);
        if let (Value::Object(p), Some(pa)) = (&mut pages, &self.k.panneau) {
            let mut pa = pa.clone();
            self.images_des_pages(&mut pa);
            p.insert("panneau".into(), pa);
        }
        let aller = self.suivre_fiche().or(aller);
        if let (Value::Object(p), Some(f)) = (&mut pages, &self.fiche.page) {
            p.insert("fiche".into(), f.clone());
        }
        let mut e = Map::new();
        e.insert(
            "app".into(),
            json!({ "nom": "Nevermind", "version": self.version, "logo": "logo.png", "phrase": self.phrase }),
        );
        e.insert("accueil".into(), self.accueil(&accueil));
        e.insert("pages".into(), pages);
        let pret = self.k.pret;
        e.insert("focus".into(), if pret { Value::Null } else { json!(self.k.focus) });
        e.insert("ecran".into(), if pret { Value::Null } else { json!(self.k.retour_ecran) });
        let ms = chrono.elapsed().as_millis();
        if ms > 300 {
            self.ctx.journal(&format!("Écran : préparé en {ms} ms"));
        }
        let _ = maintenant;
        (Value::Object(e), aller)
    }

    /// L'état complet tel qu'envoyé (`Send-Vue` : thème, couleurs, connexion, pack), objets vides en `null`.
    fn habiller(&mut self, etat: Value, maintenant: Duration) -> Value {
        let mut e = match etat {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        let connexion = self.connexion(maintenant);
        self.habillage.poser(&mut e);
        e.entry("connexion").or_insert_with(|| json!(connexion));
        let mut e = Value::Object(e);
        vides_en_null(&mut e);
        e
    }

    /// L'état de départ (`Initialize-AccueilConsole` appelle `Update-EcranNevermind` une fois) : envoyé même si la
    /// fenêtre n'est pas encore regardée.
    pub fn etat_initial(&mut self, maintenant: Duration) -> Value {
        let (e, _) = self.construire(maintenant);
        self.k.envoye = cle_etat(&e);
        self.habiller(e, maintenant)
    }

    /// `Update-EcranNevermind` (accueil-console.ps1:109-125) : seulement s'il a changé, et seulement s'il est regardé
    /// (caché, il est refait au retour).
    pub fn mettre_a_jour(&mut self, maintenant: Duration, ecran: &mut dyn Ecran) {
        if !self.k.regardee {
            self.k.envoye.clear();
            self.k.a_refaire = true;
            return;
        }
        let (e, aller) = self.construire(maintenant);
        let cle = cle_etat(&e);
        if cle != self.k.envoye {
            self.k.envoye = cle;
            if let Some(d) = &self.dossier_etat {
                let _ = std::fs::write(d.join("etat.json"), &self.k.envoye);
            }
            let e = self.habiller(e, maintenant);
            ecran.etat(e);
        }
        if let Some(a) = aller {
            self.aller(&a, ecran);
        }
    }

    /// `Send-OrdreVue` : rien tant que la page n'est pas prête ; pas regardée, l'ordre attend son retour.
    fn ordre(&mut self, o: Value, ecran: &mut dyn Ecran) {
        if !self.k.pret {
            return;
        }
        if !self.k.regardee {
            self.k.file.push_back(o);
            while self.k.file.len() > FILE_MAX {
                self.k.file.pop_front();
            }
            return;
        }
        let mut o = o;
        vides_en_null(&mut o);
        ecran.ordre(o);
    }

    /// `Open-EcranVue`
    fn aller(&mut self, id: &str, ecran: &mut dyn Ecran) {
        self.ordre(json!({ "kit": "aller", "ecran": id }), ecran);
    }

    // -----------------------------------------------------------------------------------------------------------
    // Les effets demandés par les blocs
    // -----------------------------------------------------------------------------------------------------------

    fn appliquer(
        &mut self,
        source: Option<usize>,
        effets: Effets,
        maintenant: Duration,
        ecran: &mut dyn Ecran,
        profondeur: u32,
    ) -> bool {
        let mut refaire = false;
        for e in effets.0 {
            match e {
                Effet::OuvrirEcran(id) => {
                    if self.k.pret {
                        self.aller(&id, ecran);
                    } else {
                        self.k.retour_ecran = Some(id);
                    }
                }
                Effet::Rafraichir => refaire = true,
                Effet::OuvrirAdresse(a) => self.demandes.push(Demande::OuvrirAdresse(a)),
                Effet::Annonce(t) => self.ordre(json!({ "kit": "message", "texte": t, "bravo": false }), ecran),
                Effet::Quitter => self.demandes.push(Demande::Quitter),
                Effet::Signal { nom, donnees } => {
                    match nom.as_str() {
                        "partie-commencee" => {
                            self.k.partie = true;
                            self.demandes.push(Demande::Partie(true));
                        }
                        "partie-finie" => {
                            self.k.partie = false;
                            self.demandes.push(Demande::Partie(false));
                        }
                        "question"
                        | "occupe"
                        | "resultat"
                        | "historique"
                        | "sauvegarde-appliquer"
                        | "notification"
                        | "bulle" => {
                            refaire |= self.signal_commun(&nom, &donnees, ecran);
                            continue;
                        }
                        "sauvegarde-changee" => refaire = true,
                        // (fait ici, puis passé aux blocs aussi)
                        "veille-jeu-ajouter" => refaire |= self.signal_commun(&nom, &donnees, ecran),
                        _ => {}
                    }
                    if profondeur >= 8 {
                        self.ctx.journal(&format!("Signal {nom} : trop de signaux en chaîne, arrêté"));
                        continue;
                    }
                    refaire |= self.signaler(source, &nom, &donnees, maintenant, ecran, profondeur + 1);
                }
            }
        }
        refaire
    }

    /// Un signal donné à tous les blocs sauf celui qui l'envoie.
    fn signaler(
        &mut self,
        source: Option<usize>,
        nom: &str,
        donnees: &Value,
        maintenant: Duration,
        ecran: &mut dyn Ecran,
        profondeur: u32,
    ) -> bool {
        let mut refaire = false;
        for j in 0..self.blocs.len() {
            if Some(j) == source {
                continue;
            }
            let mut eff = Effets::default();
            let (b, ctx) = (&mut self.blocs[j], &self.ctx);
            let n = b.nom();
            if let Err(e) = protege(|| b.signal(ctx, nom, donnees, &mut eff)) {
                self.ctx.journal(&format!("Signal {nom} pour {n} : {e}"));
            }
            refaire |= self.appliquer(Some(j), eff, maintenant, ecran, profondeur);
        }
        refaire
    }

    /// Une action donnée aux blocs, dans l'ordre, jusqu'à ce que l'un la prenne (`Invoke-ActionX`). `true` si prise.
    fn router(&mut self, id: &str, valeur: &Value, maintenant: Duration, ecran: &mut dyn Ecran) -> bool {
        // L'assistance du noyau d'abord (ses boutons de fiche et de panneau ne valent que quand elle en a d'ouverts)
        let mut eff = Effets::default();
        let infos = self.infos.clone();
        let ctx = &self.ctx;
        let pris = protege(|| nevermind_noyau::assistance::action(ctx, id, valeur, &infos, &mut eff)).unwrap_or(false);
        if self.appliquer(None, eff, maintenant, ecran, 0) {
            self.mettre_a_jour(maintenant, ecran);
        }
        if pris {
            return true;
        }
        for i in 0..self.blocs.len() {
            let mut eff = Effets::default();
            let (b, ctx) = (&mut self.blocs[i], &self.ctx);
            let n = b.nom();
            let pris = match protege(|| b.action(ctx, id, valeur, &mut eff)) {
                Ok(p) => p,
                Err(e) => {
                    self.ctx.journal(&format!("Geste : {id} en erreur ({n} : {e})"));
                    true
                }
            };
            let refaire = self.appliquer(Some(i), eff, maintenant, ecran, 0);
            if refaire {
                self.mettre_a_jour(maintenant, ecran);
            }
            if pris {
                return true;
            }
        }
        false
    }

    // -----------------------------------------------------------------------------------------------------------
    // Les messages de la page
    // -----------------------------------------------------------------------------------------------------------

    /// `Receive-MessageVue` (vue.ps1:403-454) et les actions de `Register-ActionsNevermind`.
    pub fn recevoir(&mut self, m: MessagePage, maintenant: Duration, ecran: &mut dyn Ecran) -> Vec<Demande> {
        match m {
            MessagePage::Pret => {
                self.k.pret = true;
                self.ctx.journal("Écran prêt");
                self.abandonner_questions();
                self.action_pret();
                self.mettre_a_jour(maintenant, ecran);
                #[cfg(feature = "essai")]
                if let Ok(e) = std::env::var("NEVERMIND_ECRAN")
                    && crate::pont::id_valable(&e)
                {
                    self.aller(&e, ecran);
                }
            }
            MessagePage::Erreur { texte } => self.ctx.journal(&format!("Écran : erreur ({texte})")),
            MessagePage::Reponse { id, valeur } => {
                self.repondre(&id, &valeur);
                if self.k.question.as_deref() == Some(id.as_str()) {
                    self.k.question = None;
                    self.ctx.journal(&format!("Question : réponse « {valeur} »"));
                    match valeur.as_str() {
                        "quit" => self.demandes.push(Demande::Quitter),
                        "tray" => self.demandes.push(Demande::Reduire),
                        _ => {}
                    }
                }
            }
            MessagePage::Ecran { id } => self.ecran_montre_change(id, maintenant, ecran),
            MessagePage::Menu => {
                if self.k.accrochee {
                    self.demandes.push(Demande::MenuMindset);
                }
            }
            MessagePage::Action { id, valeur, ecran: de } => self.geste(&id, &valeur, &de, maintenant, ecran),
        }
        self.vider_reponses(maintenant, ecran);
        self.demandes()
    }

    /// La page (re)créée a reçu l'écran, avec la page et la tuile du retour : elles ne servent qu'une fois.
    fn action_pret(&mut self) {
        self.k.focus = None;
        self.k.retour_ecran = None;
        self.k.envoye.clear();
        self.k.question = None;
    }

    /// Le message « ecran » (vue.ps1:429-432) et l'action « ecran » (accueil-console.ps1:195-199).
    fn ecran_montre_change(&mut self, id: String, maintenant: Duration, ecran: &mut dyn Ecran) {
        self.k.ecran = id.clone();
        // Close-QuitteKit (ecran-panneau.ps1:242-257)
        if id == "panneau" {
            self.k.panneau_montre = true;
        } else if id != "fiche" && self.k.panneau_montre {
            self.k.panneau_montre = false;
            if self.k.panneau_ouvert {
                self.ctx.journal(&format!("Écran : panneau quitté par la page {id}, fermé"));
                self.router("panneau:close", &Value::Null, maintenant, ecran);
            }
        }
        let fermee = self.fiche_quittee(&id);
        let refaire = self.signaler(None, "ecran", &json!({ "id": id }), maintenant, ecran, 0) || fermee;
        if refaire || PAGES_LOURDES.contains(&id.as_str()) || id == "memoire" {
            self.mettre_a_jour(maintenant, ecran);
        }
    }

    /// Un geste (vue.ps1:437-452) : noté au journal, donné à qui le prend, puis l'écran à jour (`apres`).
    fn geste(&mut self, id: &str, valeur: &Value, de: &str, maintenant: Duration, ecran: &mut dyn Ecran) {
        let note = !matches!(id, "recherche" | "jeux-cherche" | "jeux-filtre" | "pret");
        let chrono = Instant::now();
        if note {
            self.ctx.journal(&format!("Geste : {id} (écran {de})"));
        }
        match id {
            "pret" => self.action_pret(),
            "menu" => {
                if self.k.accrochee {
                    self.demandes.push(Demande::MenuMindset);
                }
            }
            "fermer" => self.demander_quitter(ecran),
            "fiche:close" | "fiche:ignore" if self.fiche.page.is_some() => self.bouton_fiche(id),
            "retour" => {
                if de == "fiche" && self.fiche.page.is_some() {
                    self.bouton_fiche("fiche:close");
                } else if de == "fiche" {
                    self.router("retour", &json!("fiche"), maintenant, ecran);
                } else if de == "panneau" && self.k.panneau_ouvert {
                    self.router("panneau:close", &Value::Null, maintenant, ecran);
                }
            }
            _ => {
                if !self.router(id, valeur, maintenant, ecran) {
                    let page = id.strip_prefix("page:").and_then(|n| n.parse::<u32>().ok());
                    match page.and_then(|n| PAGES_INDEX.iter().find(|(i, _)| *i == n)) {
                        Some((_, p)) => {
                            if self.k.pret {
                                self.aller(p, ecran);
                            } else {
                                self.k.retour_ecran = Some(p.to_string());
                            }
                        }
                        None if page.is_some() => {}
                        None => self.ctx.journal(&format!("Écran : action sans suite ({id})")),
                    }
                }
            }
        }
        if note {
            self.ctx.journal(&format!("Geste : {id} traité en {} ms", chrono.elapsed().as_millis()));
        }
        self.mettre_a_jour(maintenant, ecran);
    }

    /// « Fermer Nevermind » (le rond de l'accueil dans Mindset) : la question « Quitter ou Réduire »
    /// (`Show-QuitDialog`, assistance.ps1:275-322), dans l'écran. Sans écran regardé : quitte.
    pub fn demander_quitter(&mut self, ecran: &mut dyn Ecran) {
        if self.k.question.is_some() {
            return;
        }
        if !self.k.pret || !self.k.regardee {
            self.demandes.push(Demande::Quitter);
            return;
        }
        let id = id_question();
        self.k.question = Some(id.clone());
        self.ctx.journal("Question : posée à l'écran");
        self.ordre(
            json!({ "kit": "question", "id": id, "titre": "Quitter Nevermind ?",
                    "texte": "Voulez-vous vraiment quitter ?\n\nEn réduisant, Nevermind reste ouvert près de l'horloge : mode jeu, compteur de FPS et organizer continuent.",
                    "boutons": [ { "id": "quit", "texte": "Quitter" }, { "id": "tray", "texte": "Réduire", "style": "second" } ] }),
            ecran,
        );
    }

    // -----------------------------------------------------------------------------------------------------------
    // Le temps qui passe
    // -----------------------------------------------------------------------------------------------------------

    /// Regardée ou non (`Update-VeilleVue`) : de retour, l'écran est refait, puis les ordres qui ont attendu.
    pub fn regardee(&mut self, oui: bool, maintenant: Duration, ecran: &mut dyn Ecran) -> Vec<Demande> {
        self.k.regardee = oui;
        if oui {
            if self.k.question.is_none() {
                self.k.a_refaire = false;
                self.mettre_a_jour(maintenant, ecran);
            }
            let file: Vec<Value> = self.k.file.drain(..).collect();
            for o in file {
                self.ordre(o, ecran);
            }
        }
        self.vider_reponses(maintenant, ecran);
        self.demandes()
    }

    /// Accrochée dans Mindset ou décrochée (`Update-EmbedState`) : les ronds changent, les blocs le savent.
    pub fn accrocher(&mut self, oui: bool, maintenant: Duration, ecran: &mut dyn Ecran) -> Vec<Demande> {
        self.k.accrochee = oui;
        self.signaler(None, "mindset", &json!({ "accroche": oui }), maintenant, ecran, 0);
        self.mettre_a_jour(maintenant, ecran);
        self.demandes()
    }

    /// `Send-MesuresKit` (ecran-pc.ps1:68-72) : les mesures en direct de la page Ton PC, sans la redessiner.
    fn envoyer_mesures(&mut self, ecran: &mut dyn Ecran) {
        if !self.k.pret || self.k.ecran != "pc" || !self.k.regardee {
            return;
        }
        let Some(i) = self.k.bloc_pc.filter(|i| *i < self.blocs.len()) else { return };
        let (b, ctx) = (&mut self.blocs[i], &self.ctx);
        let Ok(p) = protege(|| b.pages(ctx, "pc")) else { return };
        let Some(page) = p.get("pc") else { return };
        let mut valeurs = Map::new();
        let mut blocs: Vec<&Value> =
            page.get("blocs").and_then(Value::as_array).map(|l| l.iter().collect()).unwrap_or_default();
        if let Some(Value::Array(s)) = page.get("sections") {
            for x in s {
                blocs.extend(x.get("blocs").and_then(Value::as_array).into_iter().flatten());
            }
        }
        for b in blocs.into_iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("mesures")) {
            for m in b.get("elements").and_then(Value::as_array).into_iter().flatten() {
                let Some(id) = m.get("id").and_then(Value::as_str) else { continue };
                let ton = m.get("ton").and_then(Value::as_str).unwrap_or("");
                valeurs.insert(
                    id.to_string(),
                    json!({ "valeur": m.get("valeur"), "texte": m.get("texte"), "note": m.get("note"), "ton": ton }),
                );
            }
        }
        if valeurs.is_empty() {
            return;
        }
        self.ordre(json!({ "kit": "mesures", "valeurs": valeurs }), ecran);
    }

    /// Toutes les 200 ms (`Step-MinuterieKit`, accueil-console.ps1:426-434), puis les tics des blocs dus.
    pub fn minuterie(&mut self, maintenant: Duration, ecran: &mut dyn Ecran) -> Vec<Demande> {
        self.k.tic += 1;
        if !self.k.regardee {
            self.k.a_refaire = true;
        } else if self.k.a_refaire {
            if self.k.question.is_none() {
                self.k.a_refaire = false;
                self.mettre_a_jour(maintenant, ecran);
            }
        } else {
            // (le panneau ouvert est suivi, au plus une fois par seconde : un test en cours, un ping)
            if self.k.panneau_ouvert
                && self.k.question.is_none()
                && self.k.panneau_vu.is_none_or(|v| maintenant.saturating_sub(v) >= PANNEAU)
            {
                self.k.panneau_vu = Some(maintenant);
                self.mettre_a_jour(maintenant, ecran);
            }
            if self.k.tic.is_multiple_of(8) {
                self.envoyer_mesures(ecran);
            }
            if self.k.tic.is_multiple_of(20) && self.k.question.is_none() {
                self.mettre_a_jour(maintenant, ecran);
            }
        }
        self.tics(maintenant, ecran);
        // Les bulles pour Mindset (notification.demande), une à la fois, vérifiées toutes les secondes
        if self.k.tic.is_multiple_of(5) && self.ctx.bulles_en_attente() {
            self.ctx.envoyer_notification_suivante();
        }
        self.vider_reponses(maintenant, ecran);
        self.demandes()
    }

    /// Les tics des blocs dus (`Bloc::tic`), avec `cache` quand l'écran n'est pas regardé.
    fn tics(&mut self, maintenant: Duration, ecran: &mut dyn Ecran) {
        let cache = !self.k.regardee;
        let mut refaire = false;
        for i in 0..self.blocs.len() {
            if !self.tics[i].is_some_and(|t| maintenant >= t) {
                continue;
            }
            let mut eff = Effets::default();
            let (b, ctx) = (&mut self.blocs[i], &self.ctx);
            let n = b.nom();
            match protege(|| b.tic(ctx, cache, &mut eff)) {
                Ok(d) => self.tics[i] = d.map(|d| maintenant + d.max(Duration::from_millis(50))),
                Err(e) => {
                    self.ctx.journal(&format!("Tic de {n} : {e}"));
                    self.tics[i] = Some(maintenant + APRES_ERREUR);
                }
            }
            refaire |= self.appliquer(Some(i), eff, maintenant, ecran, 0);
        }
        if refaire {
            self.mettre_a_jour(maintenant, ecran);
        }
    }

    /// Le prochain tic de bloc dû (pour que le programme dorme jusque-là quand rien d'autre n'est à faire).
    pub fn prochain_tic(&self) -> Option<Duration> {
        self.tics.iter().flatten().min().copied()
    }

    /// Nevermind se ferme : chaque bloc remet ce qui doit l'être (`Add_Closed`, evenements.ps1:184-202).
    pub fn fermer(&mut self) {
        for i in 0..self.blocs.len() {
            let (b, ctx) = (&mut self.blocs[i], &self.ctx);
            let n = b.nom();
            if let Err(e) = protege(|| b.fermer(ctx)) {
                self.ctx.journal(&format!("Fermeture de {n} : {e}"));
            }
        }
    }
}

#[cfg(test)]
#[path = "hote_tests.rs"]
mod tests;
