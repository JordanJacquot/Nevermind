//! Nevermind en Rust : le bloc « divers ». Les pages des petites icônes du haut : Rechercher, Paramètres et Aide
//! (Get-PagesDivers, Register-ActionsDivers d'ecran-divers.ps1), la recherche des réglages (recherche.ps1) et les
//! réglages de Paramètres (parametres.ps1). La page Tests des composants est au bloc « pc ».
//!
//! Ce que la page Paramètres montre de l'hôte (version, mises à jour, raccourci, démarrage, thème) arrive par le signal
//! « etat-hote » ; la liste des jeux par « jeux-liste » (bloc jeux) ; les réglages Windows par
//! « reglages-jeu » (bloc pc).

pub mod recherche;
mod tri;

use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use recherche::{Entree, Geste, JeuConnu};
use serde_json::{Map, Value, json};

/// Ce que l'hôte dit de lui-même pour la page Paramètres (les éléments de la fenêtre d'avant).
#[derive(Debug, Clone, PartialEq)]
pub struct EtatHote {
    /// $AppVersion
    pub version: String,
    /// $ui.UpdateStatus.Text
    pub maj_texte: String,
    /// $ui.BtnCheckUpdate visible
    pub maj_bouton: bool,
    /// $ui.ShortcutStatus.Text
    pub raccourci_texte: String,
    /// $ui.BtnShortcut visible
    pub raccourci_bouton: bool,
    /// $ui.AutoStartRow visible, et la case cochée
    pub demarrage_ligne: bool,
    pub demarrage_coche: bool,
    /// Le thème actuel (clé, nom) et les choix (copie autonome) : (id, titre, texte).
    pub theme_cle: Option<String>,
    pub theme_nom: Option<String>,
    pub themes: Option<Vec<(String, String, String)>>,
}

impl Default for EtatHote {
    fn default() -> Self {
        EtatHote {
            version: String::new(),
            maj_texte: String::new(),
            maj_bouton: true,
            raccourci_texte: String::new(),
            raccourci_bouton: true,
            demarrage_ligne: true,
            demarrage_coche: false,
            theme_cle: None,
            theme_nom: None,
            themes: None,
        }
    }
}

/// Les 5 thèmes de base ($AppThemes de themes.ps1) : (id, nom, description).
pub const THEMES: [(&str, &str, &str); 5] = [
    ("neon", "Néon", "Le thème de Nevermind : cyan, violet et magenta sur fond de nuit."),
    ("crepuscule", "Crépuscule", "Le style Néon recoloré : orange, rose et or, comme un coucher de soleil."),
    ("terminal", "Terminal", "Pour les geeks : vert phosphore sur fond noir, tout en police de code."),
    ("arcade", "Arcade", "Jaune, rouge et bleu vifs sur fond bleu nuit, comme une vieille borne."),
    ("rubis", "Rubis", "Rouge vif, jaune et bleu sur un gris anthracite sobre."),
];

#[derive(Default)]
pub struct BlocDivers {
    pub hote: EtatHote,
    recherche: String,
    resultats: Vec<Entree>,
    jeux: Vec<JeuConnu>,
    reglages_windows: Vec<String>,
    theme_demande: Option<String>,
}

fn vrai(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(_) => true,
    }
}

fn texte(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        _ => String::new(),
    }
}

fn champ(v: &Value, k: &str) -> String {
    texte(v.get(k).unwrap_or(&Value::Null))
}

impl BlocDivers {
    fn reglage(ctx: &Contexte, nom: &str, defaut: bool) -> bool {
        ctx.reglage(nom).map(|v| vrai(&v)).unwrap_or(defaut)
    }

    /// L'Organizer est-il proposé ? (Test-OrgVisible, organizer.ps1)
    fn organizer_visible(&self, ctx: &Contexte) -> bool {
        if let Some(r) = ctx.reglage("OrganizerAffiche").filter(|r| !r.is_null()) {
            return vrai(&r);
        }
        let on = std::fs::read(ctx.donnees.join("organizer.json"))
            .ok()
            .and_then(|o| {
                serde_json::from_str::<Value>(String::from_utf8_lossy(&o).trim_start_matches('\u{feff}')).ok()
            })
            .and_then(|j| j.get("On").cloned())
            .map(|v| vrai(&v))
            .unwrap_or(false);
        if on {
            return true;
        }
        self.jeux.iter().any(|g| {
            let b = g.nom.to_lowercase();
            b.match_indices("dofus").any(|(i, _)| {
                let avant = b[..i].chars().next_back();
                let apres = b[i + 5..].chars().next();
                !avant.map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false)
                    && !apres.map(|c| c.is_alphanumeric() || c == '_').unwrap_or(false)
            })
        })
    }

    fn index(&self, ctx: &Contexte) -> Vec<Entree> {
        recherche::index(ctx.geree, self.organizer_visible(ctx), &self.reglages_windows, &self.jeux)
    }

    /// Les réglages trouvés pour un texte (Find-Settings, 8 au plus).
    pub fn chercher(&self, ctx: &Contexte, q: &str) -> Vec<Entree> {
        recherche::chercher(&self.index(ctx), q, 8)
    }

    /// Le thème actuel (Get-CurrentThemeKey, Get-CurrentThemeName) : celui de l'hôte, sinon le réglage « Theme ».
    fn theme(&self, ctx: &Contexte) -> (String, String) {
        if let (Some(k), Some(n)) = (&self.hote.theme_cle, &self.hote.theme_nom) {
            return (k.clone(), n.clone());
        }
        let r = ctx.reglage("Theme").map(|v| texte(&v)).unwrap_or_else(|| "neon".into());
        let r = match r.as_str() {
            "retro" => "arcade".to_string(),
            "dresseur" => "rubis".to_string(),
            _ => r,
        };
        let t = THEMES.iter().find(|t| t.0 == r).unwrap_or(&THEMES[0]);
        (t.0.to_string(), t.1.to_string())
    }

    fn choix_themes(&self) -> Vec<(String, String, String)> {
        match &self.hote.themes {
            Some(l) => l.clone(),
            None => THEMES.iter().map(|t| (t.0.to_string(), t.1.to_string(), t.2.to_string())).collect(),
        }
    }

    /// Aller au réglage choisi (Open-SearchEntry) : la bonne page de l'écran, puis le geste qui suit.
    fn ouvrir(&self, x: &Entree, effets: &mut Effets) {
        effets.ajouter(Effet::OuvrirEcran(recherche::ecran(x)));
        if x.org {
            effets.ajouter(Effet::Signal { nom: "organizer-ouvrir".into(), donnees: Value::Null });
        }
        effets.ajouter(Effet::Annonce(format!("{} : {}", x.t, x.ou)));
        match x.geste {
            Geste::Restes => effets.ajouter(Effet::Signal { nom: "jeux-restes-ouvrir".into(), donnees: Value::Null }),
            Geste::CarteReseau => effets.ajouter(Effet::Signal { nom: "reseau-carte".into(), donnees: Value::Null }),
            Geste::VieePrivee => effets.ajouter(Effet::Signal { nom: "trafic-microsoft".into(), donnees: Value::Null }),
            Geste::Aucun => {}
        }
    }

    /// « Ouvrir les réglages de Mindset » (Open-MindsetSettings) : Mindset trouvé par le registre (clé écrite par un
    /// administrateur), la page à ouvrir dans afficher-page.txt, puis Mindset lancé par l'Explorateur.
    fn reglages_mindset(&self, ctx: &Contexte, page: &str, effets: &mut Effets) {
        let dir = dossier_mindset();
        let exe = if !dir.is_empty() && std::path::Path::new(&dir).is_absolute() {
            std::path::Path::new(&dir).join("Mindset.exe")
        } else {
            std::path::PathBuf::new()
        };
        if exe.as_os_str().is_empty() || !exe.is_file() {
            ctx.journal(&format!("Mindset introuvable (registre : « {dir} »)"));
            effets.ajouter(Effet::Annonce(
                "Mindset est introuvable sur ce PC. Ouvre-le depuis son raccourci pour changer ce réglage de la suite."
                    .into(),
            ));
            return;
        }
        let page = if ["general", "apparence", "son"].contains(&page) { page } else { "general" };
        match dossier_suite(ctx) {
            Some(d) => {
                if let Err(e) = std::fs::write(d.join("afficher-page.txt"), format!("parametres:{page}")) {
                    ctx.journal(&format!("Mindset, page à ouvrir : {e}"));
                }
            }
            None => ctx.journal("Mindset, page à ouvrir : copie de test, rien écrit"),
        }
        effets.ajouter(Effet::OuvrirAdresse(exe.to_string_lossy().to_string()));
        effets.ajouter(Effet::Annonce("Réglages de Mindset ouverts.".into()));
    }
}

/// Le dossier de suite.json (%LOCALAPPDATA%\Mindset) ; en copie de test, seulement si elle simule Mindset
/// (NEVERMIND_SUITE), jamais le vrai.
fn dossier_suite(ctx: &Contexte) -> Option<std::path::PathBuf> {
    if ctx.essai {
        let s = std::env::var("NEVERMIND_SUITE").ok().filter(|s| !s.is_empty())?;
        return std::path::Path::new(&s).parent().map(|p| p.to_path_buf());
    }
    std::env::var("LOCALAPPDATA").ok().map(|l| std::path::Path::new(&l).join("Mindset"))
}

#[cfg(windows)]
fn dossier_mindset() -> String {
    windows_registry::LOCAL_MACHINE
        .open(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Mindset")
        .and_then(|k| k.get_string("InstallLocation"))
        .map(|s| s.trim().trim_matches('"').to_string())
        .unwrap_or_default()
}

#[cfg(not(windows))]
fn dossier_mindset() -> String {
    String::new()
}

impl Bloc for BlocDivers {
    fn nom(&self) -> &'static str {
        "divers"
    }

    fn pages(&mut self, ctx: &Contexte, _ecran: &str) -> Map<String, Value> {
        let mut pages = Map::new();

        // ---- Rechercher
        let q = self.recherche.clone();
        let res = &self.resultats;
        let mut blocs = vec![
            json!({ "type": "champ", "id": "recherche", "recherche": true, "valeur": q, "indice": "Par exemple : mode jeu" }),
        ];
        if !q.is_empty() {
            let t = if res.is_empty() {
                "Rien trouvé".to_string()
            } else {
                format!("{} résultat{}", res.len(), if res.len() > 1 { "s" } else { "" })
            };
            blocs.push(json!({ "type": "titre", "texte": t }));
        }
        for (k, r) in res.iter().enumerate() {
            blocs.push(json!({ "type": "ligne", "id": format!("res:{k}"), "icone": "loupe", "couleur": "S", "texte": r.t, "note": r.ou, "valeur": "Y aller" }));
        }
        if !q.is_empty() && res.is_empty() {
            blocs.push(json!({ "type": "texte", "texte": "Essaie avec un autre mot." }));
        }
        if q.is_empty() {
            blocs.push(json!({ "type": "raccourcis", "elements": [
                { "id": "optimisation", "nom": "Mode jeu", "icone": "manette", "couleur": "P" },
                { "id": "connexion", "nom": "Serveur DNS", "icone": "ondes", "couleur": "P" },
                { "id": "overlay", "nom": "Compteur de FPS", "icone": "jauge", "couleur": "vert" },
                { "id": "sauvegarde", "nom": "Tout restaurer", "icone": "sauve", "couleur": "S" }
            ] }));
        }
        pages.insert("recherche".into(), json!({ "titre": "Rechercher", "sous": "Un réglage, un outil, un jeu.", "icone": "loupe", "couleur": "S",
            "module": { "icone": "loupe", "couleur": "S", "titre": "Que cherches-tu ?", "texte": "Tape un mot : Nevermind te montre où c'est et t'y emmène. Raccourci : Ctrl + K." },
            "blocs": blocs }));

        // ---- Paramètres
        let (cle, nom) = self.theme(ctx);
        let theme = if ctx.geree {
            json!([{ "type": "reglages", "rangs": [
                { "texte": "Thème actuel", "note": recherche::THEME_GERE, "valeur": nom },
                { "texte": "Changer de thème", "note": "Mindset s'ouvre sur Paramètres, Apparence. Le thème choisi là-bas va sur Nevermind et sur les autres applis.",
                    "bouton": { "id": "theme-mindset", "texte": "Ouvrir les réglages de Mindset", "style": "second" } }
            ] }])
        } else {
            let options: Vec<Value> =
                self.choix_themes().into_iter().map(|(id, t, d)| json!({ "id": id, "titre": t, "texte": d })).collect();
            json!([{ "type": "texte", "texte": "Change les couleurs et l'ambiance de Nevermind. Nevermind redémarre pour appliquer le thème." },
                { "type": "cartes", "id": "theme", "valeur": cle, "options": options }])
        };
        let h = &self.hote;
        let mut general = vec![json!({ "texte": "Raccourci sur le bureau", "note": h.raccourci_texte,
            "bouton": if h.raccourci_bouton { json!({ "id": "clic:BtnShortcut", "texte": "Créer le raccourci", "style": "second" }) } else { Value::Null } })];
        if h.demarrage_ligne {
            general.push(json!({ "texte": "Lancer Nevermind au démarrage du PC", "note": "Il reste réduit près de l'horloge, sans demande d'autorisation, pour le mode jeu et la mesure des FPS.",
                "bascule": { "id": "coche:ChkAutoStart", "oui": h.demarrage_coche } }));
        }
        general.push(json!({ "texte": "Visite guidée", "note": "Revoir en 3 étapes comment marche Nevermind.", "bouton": { "id": "visite", "texte": "Revoir la visite", "style": "second" } }));
        let mesure_fps = match ctx.reglage("FpsMeasure") {
            Some(v) => vrai(&v),
            None => Self::reglage(ctx, "FpsOverlay", false),
        };
        let mut maj = vec![
            json!({ "texte": "Version", "note": h.maj_texte, "valeur": format!("Nevermind {}", h.version) }),
            json!({ "texte": "Recevoir les versions bêta", "note": "Les nouveautés en avant-première ; elles peuvent contenir des erreurs.", "bascule": { "id": "coche:ChkBeta", "oui": Self::reglage(ctx, "Beta", false) } }),
        ];
        if !ctx.geree {
            maj.push(json!({ "texte": "Chercher une nouvelle version au lancement", "note": "Demande à GitHub (api.github.com) le numéro de la dernière version. Coupé : seulement quand tu cliques sur « Vérifier les mises à jour ».", "bascule": { "id": "maj-auto", "oui": Self::reglage(ctx, "MajAuto", true) } }));
        }
        maj.push(json!({ "texte": "Lire les nouveautés sur Internet", "note": "Après une mise à jour : la liste des nouveautés est lue sur GitHub (api.github.com). Coupé : Nevermind dit seulement qu'il a été mis à jour.", "bascule": { "id": "nouveautes-en-ligne", "oui": Self::reglage(ctx, "NouveautesEnLigne", true) } }));
        let boutons_module: Vec<Value> = if h.maj_bouton {
            vec![json!({ "id": "clic:BtnCheckUpdate", "texte": "Vérifier les mises à jour", "style": "second" })]
        } else {
            Vec::new()
        };
        pages.insert("parametres".into(), json!({ "titre": "Paramètres", "sous": "Les réglages de Nevermind lui-même.", "icone": "reglages", "couleur": "doux",
            "module": { "icone": "reglages", "couleur": "P", "titre": format!("Nevermind {}", h.version), "texte": h.maj_texte, "boutons": boutons_module },
            "onglets": [
                { "id": "general", "nom": "Général", "blocs": [{ "type": "reglages", "rangs": general }] },
                { "id": "jeux", "nom": "Jeux", "blocs": [{ "type": "reglages", "rangs": [
                    { "texte": "Mesurer mes FPS quand je joue", "note": "Moyenne, chutes et courbe de chaque partie, gardées dans Jeux, Perf. In Game.", "bascule": { "id": "fps-mesure", "oui": mesure_fps } },
                    { "texte": "Compteur de FPS à l'écran", "note": "Affichage, style et position du compteur par-dessus le jeu.", "lien": { "id": "overlay", "texte": "Ouvrir Overlay" } },
                    { "texte": "Mesurer ma connexion quand je joue", "note": "Box, Internet et serveur du jeu pendant la partie, pour savoir d'où vient le lag.", "bascule": { "id": "lag-mesure", "oui": Self::reglage(ctx, "LagMeasure", true) } },
                    { "texte": "Mode jeu : fermer des applis pendant que je joue", "note": "Les applis choisies (Optimisation gaming, Mode jeu) sont fermées puis relancées après la partie.", "bascule": { "id": "modejeu", "oui": Self::reglage(ctx, "GameMode", false) } },
                    { "texte": "Organizer Dofus", "note": "Pour passer d'un perso à l'autre quand tu joues à plusieurs comptes. Coupé : le bouton disparaît de la page Jeux.", "bascule": { "id": "org-affiche", "oui": self.organizer_visible(ctx) } },
                    { "texte": "Jaquettes depuis Internet", "note": "Cherche les images des jeux qui n'en ont pas sur la boutique Steam puis Wikipédia (seul le nom du jeu est envoyé).", "bascule": { "id": "coche:ChkLibCovers", "oui": Self::reglage(ctx, "LibCoversOnline", false) } }
                ] }] },
                { "id": "reseau", "nom": "Réseau et vie privée", "blocs": [
                    { "type": "reglages", "rangs": [
                        { "texte": "Me prévenir quand un nouvel appareil se connecte", "note": "Vérifié toutes les 10 minutes sur ton réseau, tant que Nevermind est ouvert.", "bascule": { "id": "coche:ChkNetWatch", "oui": Self::reglage(ctx, "NetWatch", false) } },
                        { "texte": "Identifier les serveurs sans nom", "note": "Page Trafic et Lag en ligne : cherche à qui appartient une adresse inconnue (Valve, Riot, Amazon...) dans l'annuaire public rdap.org. Seule l'adresse du serveur est envoyée.", "bascule": { "id": "trafic-annuaire", "oui": Self::reglage(ctx, "TrafficLookup", false) } },
                        { "texte": "Pays du serveur dans Lag en ligne", "note": "Lag en ligne : cherche dans quel pays se trouve le serveur du jeu (ipinfo.io). Seule l'adresse du serveur est envoyée.", "bascule": { "id": "lag-pays", "oui": Self::reglage(ctx, "LagPays", false) } },
                        { "texte": "Vérifier le pilote NVIDIA au lancement", "note": "Si le PC a une carte NVIDIA : demande à NVIDIA (geforce.com) le numéro du dernier pilote. Rien sur ton PC n'est envoyé. Coupé : Nevermind regarde seulement l'âge de ton pilote.", "bascule": { "id": "pilote-nvidia", "oui": Self::reglage(ctx, "PiloteNvidia", true) } },
                        { "texte": "Ce que Windows envoie à Microsoft", "note": "Identifiants, réglages qui envoient plus que le minimum et envois vus en direct.", "bouton": { "id": "trafic-microsoft", "texte": "Voir", "style": "second" } }
                    ] },
                    { "type": "note", "texte": "Nevermind ne déchiffre jamais tes connexions et n'envoie rien sur toi : il regarde seulement qui parle à qui depuis ton PC." }
                ] },
                { "id": "theme", "nom": "Thème", "blocs": theme },
                { "id": "maj", "nom": "Mises à jour", "blocs": [{ "type": "reglages", "rangs": maj }] }
            ] }));

        // ---- Aide
        pages.insert("aide".into(), json!({ "titre": "Aide", "sous": "Les réponses aux questions qu'on se pose souvent.", "icone": "aide", "couleur": "T",
            "module": { "icone": "aide", "couleur": "T", "titre": "Bienvenue dans Nevermind", "texte": "Nevermind analyse ton PC et le règle pour que tes jeux tournent au mieux.", "boutons": [{ "id": "visite", "texte": "Revoir la visite" }] },
            "blocs": [
                { "type": "faq", "elements": [
                    { "question": "Est-ce que Nevermind peut abîmer mon PC ?", "icone": "bouclier", "couleur": "T", "reponse": "Non. Rien n'est modifié sans ton accord, et chaque changement est sauvegardé avant d'être fait : tu peux l'annuler à tout moment depuis la page Sauvegarde (dans Protection). Nevermind ne touche jamais à tes documents." },
                    { "question": "Comment revenir à l'accueil ?", "icone": "maison", "couleur": "P", "reponse": "Clique sur le logo ou le nom Nevermind en haut à gauche, ou appuie sur Échap (ou le bouton Retour en bas à droite) jusqu'à revenir à l'accueil." },
                    { "question": "Que se passe-t-il quand je clique sur la croix ?", "icone": "horloge", "couleur": "P", "reponse": "Nevermind te demande si tu veux quitter ou réduire. « Quitter » le ferme complètement. « Réduire » le garde ouvert près de l'horloge : il continue en arrière-plan pour le mode jeu, le compteur de FPS et la mesure des FPS." },
                    { "question": "Mes FPS ne sont pas mesurés dans un jeu", "icone": "manette", "couleur": "S", "reponse": "Vérifie dans Paramètres, Jeux, que « Mesurer mes FPS quand je joue » est activé. Si Nevermind ne reconnaît pas le jeu, appuie sur Ctrl + Maj + F pendant la partie, ou ajoute le jeu dans la page Jeux." },
                    { "question": "À quoi sert la page Réseau ?", "icone": "radar", "couleur": "S", "reponse": "Elle trouve les appareils de ta maison (téléphones, consoles, TV, objets connectés) et vérifie leur sécurité. Un nouvel appareil inconnu est signalé." }
                ] },
                { "type": "titre", "texte": "Toujours bloqué ?" },
                { "type": "ligne", "icone": "rapport", "couleur": "P", "texte": "Signaler un problème", "note": "Crée un fichier sur ton bureau avec le journal de Nevermind et la configuration de ton PC, sans tes fichiers ni ton nom. Envoie-le à la personne qui t'a donné Nevermind.", "bouton": { "id": "clic:BtnReportProblem", "texte": "Créer le fichier", "style": "second" } },
                { "type": "raccourcis", "elements": [{ "id": "recherche", "nom": "Rechercher un réglage", "icone": "loupe", "couleur": "S" }] }
            ] }));
        pages
    }

    fn action(&mut self, ctx: &Contexte, id: &str, valeur: &Value, effets: &mut Effets) -> bool {
        match id {
            "recherche" => {
                let q = texte(valeur).trim().to_string();
                self.resultats = if q.is_empty() { Vec::new() } else { self.chercher(ctx, &q) };
                self.recherche = q;
            }
            "visite" => effets.ajouter(Effet::Signal { nom: "visite".into(), donnees: Value::Null }),
            "theme-mindset" => self.reglages_mindset(ctx, "apparence", effets),
            "pilote-nvidia" | "maj-auto" | "nouveautes-en-ligne" => {
                let nom = match id {
                    "pilote-nvidia" => "PiloteNvidia",
                    "maj-auto" => "MajAuto",
                    _ => "NouveautesEnLigne",
                };
                if let Err(e) = ctx.ecrire_reglage(nom, Value::Bool(vrai(valeur)), false) {
                    ctx.journal(&e);
                }
            }
            "theme" => {
                let t = texte(valeur);
                if ctx.geree {
                    ctx.journal(&format!(
                        "Thème « {t} » demandé depuis l'écran : refusé (copie gérée, le thème se choisit dans Mindset)"
                    ));
                    return true;
                }
                if t == self.theme(ctx).0 {
                    return true;
                }
                self.theme_demande = Some(t);
                effets.ajouter(Effet::Signal {
                    nom: "question".into(),
                    donnees: json!({ "id": "divers-theme", "titre": "", "texte": "Changer de thème ? Nevermind va redémarrer pour l'appliquer.", "action": "divers-reponse",
                        "boutons": [{ "id": "oui", "texte": "Oui" }, { "id": "non", "texte": "Non", "style": "second" }] }),
                });
            }
            "divers-reponse" => {
                if champ(valeur, "id") != "divers-theme" {
                    return false;
                }
                if let Some(t) = self.theme_demande.take()
                    && champ(valeur, "valeur") == "oui"
                {
                    effets.ajouter(Effet::Signal { nom: "theme-choisi".into(), donnees: json!({ "id": t }) });
                }
            }
            _ => {
                if let Some(k) = id.strip_prefix("res:").and_then(|x| x.parse::<usize>().ok()) {
                    if let Some(r) = self.resultats.get(k).cloned() {
                        self.ouvrir(&r, effets);
                    }
                    return true;
                }
                return false;
            }
        }
        true
    }

    fn signal(&mut self, _ctx: &Contexte, nom: &str, donnees: &Value, _effets: &mut Effets) {
        match nom {
            "jeux-liste" => {
                let l = donnees.get("jeux").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                self.jeux = l.iter().map(|g| JeuConnu { nom: champ(g, "nom"), source: champ(g, "source") }).collect();
            }
            // (le bloc pc : `$script:TweakRows`, toutes les lignes de l'onglet Optimisation gaming, recherche.ps1:127)
            "reglages-jeu" => {
                let l = donnees.get("lignes").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                self.reglages_windows = l.iter().map(|r| champ(r, "titre")).filter(|t| !t.is_empty()).collect();
            }
            "etat-hote" => {
                let h = &mut self.hote;
                let d = donnees;
                if let Some(v) = d.get("version") {
                    h.version = texte(v);
                }
                if let Some(v) = d.get("maj_texte") {
                    h.maj_texte = texte(v);
                }
                if let Some(v) = d.get("maj_bouton") {
                    h.maj_bouton = vrai(v);
                }
                if let Some(v) = d.get("raccourci_texte") {
                    h.raccourci_texte = texte(v);
                }
                if let Some(v) = d.get("raccourci_bouton") {
                    h.raccourci_bouton = vrai(v);
                }
                if let Some(v) = d.get("demarrage_ligne") {
                    h.demarrage_ligne = vrai(v);
                }
                if let Some(v) = d.get("demarrage_coche") {
                    h.demarrage_coche = vrai(v);
                }
                if let Some(v) = d.get("theme_cle") {
                    h.theme_cle = Some(texte(v));
                }
                if let Some(v) = d.get("theme_nom") {
                    h.theme_nom = Some(texte(v));
                }
                if let Some(Value::Array(a)) = d.get("themes") {
                    h.themes = Some(a.iter().map(|t| (champ(t, "id"), champ(t, "titre"), champ(t, "texte"))).collect());
                }
            }
            _ => {}
        }
    }
}
