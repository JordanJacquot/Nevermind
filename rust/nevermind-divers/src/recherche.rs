//! La recherche des réglages (recherche.ps1) : la liste des réglages trouvables ($SearchEntries), l'index complet
//! (Get-SearchIndex : plus les réglages Windows et les jeux de ce PC), où se trouve un réglage (Get-SearchWhere), la
//! recherche au fil de la frappe (Find-Settings, à une faute près) et l'arrivée sur le réglage (Open-SearchEntry).

use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::UnicodeNormalization;

/// Minuscules, sans accents ni ponctuation (ConvertTo-SearchText).
pub fn texte_recherche(texte: &str) -> String {
    use GeneralCategory::*;
    // (ToLowerInvariant de .NET Framework : une lettre donne une lettre, le I à point turc reste tel quel)
    let bas: String =
        texte.chars().map(|c| if c == '\u{130}' { c } else { c.to_lowercase().next().unwrap_or(c) }).collect();
    let mut s = String::with_capacity(bas.len());
    for c in bas.nfd() {
        let cat = get_general_category(c);
        if cat == NonspacingMark {
            continue;
        }
        let lettre = matches!(
            cat,
            UppercaseLetter | LowercaseLetter | TitlecaseLetter | ModifierLetter | OtherLetter | DecimalNumber
        );
        s.push(if lettre { c } else { ' ' });
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Le geste qui suit l'arrivée sur un réglage (Do).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Geste {
    Aucun,
    /// Les restes de jeux désinstallés (s'il y en a).
    Restes,
    /// La carte du réseau (s'il y a des appareils).
    CarteReseau,
    /// Ce que Windows envoie à Microsoft.
    VieePrivee,
}

/// Un réglage trouvable.
#[derive(Debug, Clone)]
pub struct Entree {
    /// T : le texte affiché.
    pub t: String,
    /// P : la page (« 0 » à « 7 », « jeux », « reseau », « trafic », « overlay », « parametres »).
    pub p: String,
    /// S : le sous-onglet d'Optimisation gaming.
    pub s: String,
    /// Perf : l'onglet de Perf. In Game.
    pub perf: String,
    /// A : le repère du réglage sur la page.
    pub a: String,
    /// K : les autres mots qui le trouvent.
    pub k: String,
    /// Tab : l'onglet des Paramètres.
    pub tab: String,
    /// Org : réglage de l'Organizer (caché avec lui).
    pub org: bool,
    pub geste: Geste,
    /// Game : le nom d'un jeu.
    pub jeu: String,
    pub ou: String,
    titre_n: String,
    titre_mots: Vec<String>,
    mots: Vec<String>,
}

fn e(t: &str, p: &str, a: &str, k: &str) -> Entree {
    Entree {
        t: t.into(),
        p: p.into(),
        s: String::new(),
        perf: String::new(),
        a: a.into(),
        k: k.into(),
        tab: String::new(),
        org: false,
        geste: Geste::Aucun,
        jeu: String::new(),
        ou: String::new(),
        titre_n: String::new(),
        titre_mots: Vec::new(),
        mots: Vec::new(),
    }
}

impl Entree {
    fn s(mut self, s: &str) -> Self {
        self.s = s.into();
        self
    }
    fn perf(mut self, s: &str) -> Self {
        self.perf = s.into();
        self
    }
    fn tab(mut self, s: &str) -> Self {
        self.tab = s.into();
        self
    }
    fn org(mut self) -> Self {
        self.org = true;
        self
    }
    fn geste(mut self, g: Geste) -> Self {
        self.geste = g;
        self
    }
}

pub const THEME_GERE: &str = "Le thème se choisit dans Mindset, pour toutes les applis.";

/// $SearchEntries. `geree` : Nevermind géré par Mindset (raccourci, bêta et thème passent par lui).
pub fn entrees(geree: bool) -> Vec<Entree> {
    let mut v = vec![
        // Optimisation gaming
        e("Réglages Windows pour les jeux", "1", "Cocher le recommandé", "tweaks optimiser optimisation fps appliquer recommande gagner performances").s("tweaks"),
        e("Point de restauration avant les réglages", "1", "Point de restauration avant (recommandé)", "securite sauvegarde avant").s("tweaks"),
        e("Mesurer mes FPS", "jeux", "Mesurer mes FPS quand je joue", "fps images par seconde presentmon mesure partie framerate").perf("fps"),
        e("Mes parties (historique des FPS)", "jeux", "Mesurer mes FPS quand je joue", "historique parties fps moyenne bas avant apres comparaison").perf("fps"),
        e("Mes FPS ne sont pas normaux", "jeux", "Mes FPS ne sont pas normaux : trouver pourquoi", "diagnostic fps rame saccades chutes freeze lent").perf("fps"),
        e("Raccourci Ctrl+Maj+F (mesure manuelle)", "overlay", "Raccourci Ctrl + Maj + F", "raccourci clavier hotkey touche mesure manuelle autre jeu"),
        e("Compteur de FPS à l'écran (overlay)", "overlay", "Afficher le compteur pendant la partie", "overlay compteur afficher ecran osd fps"),
        e("Style du compteur : complet ou discret", "overlay", "Style du compteur", "overlay discret petit transparent semi transparence taille apparence"),
        e("Position du compteur", "overlay", "Position du compteur", "overlay coin haut bas gauche droite emplacement deplacer"),
        e("Lag en ligne", "jeux", "Mesurer ma connexion quand je joue", "lag ping latence serveur jeu connexion partie online").perf("lag"),
        e("Tester ma connexion pour le jeu (30 s)", "jeux", "Tester ma connexion maintenant (30 s)", "test lag ping latence").perf("lag"),
        e("Mode jeu : fermer des applis pendant que je joue", "1", "Fermer des applis pendant que je joue", "mode jeu fermer applis discord chrome navigateur").s("mode"),
        e("Profils par jeu", "1", "", "priorite haute carte graphique puissante gpu profil jeu").s("profiles"),
        e("Ajouter un jeu (non reconnu)", "1", "Ajouter un jeu", "ajouter jeu manquant detecte reconnu ubisoft ea gog battlenet riot xbox autre launcher itch emulateur").s("profiles"),
        e("Mes jeux reconnus", "1", "Ajouter un jeu", "liste jeux installes launchers steam epic ubisoft ea gog battlenet riot xbox").s("profiles"),
        // Jeux
        e("Mes jeux (bibliothèque)", "jeux", "Mes jeux", "jeux bibliotheque library steam liste installes"),
        e("Lancer un jeu", "jeux", "Mes jeux", "lancer jouer demarrer jeu play"),
        e("Optimiser un jeu", "jeux", "Mes jeux", "optimiser jeu tout optimiser priorite"),
        e("Restes de jeux désinstallés (place à récupérer)", "jeux", "Mes jeux", "place disque espace jeux desinstalles restes dossiers steam liberer").geste(Geste::Restes),
        e("Désinstaller un jeu", "jeux", "Mes jeux", "desinstaller supprimer enlever jeu"),
        e("Temps de jeu", "jeux", "Mes jeux", "temps heures joue derniere partie"),
        e("Organizer Dofus (passer d'un perso à l'autre)", "jeux", "Mes personnages", "organizer organiseur dofus multicompte multi compte perso personnages fenetres switch changer initiative ordre ankama").org(),
        e("Raccourcis de l'organizer Dofus", "jeux", "Raccourcis", "organizer dofus raccourci touche clavier perso suivant precedent f1").org(),
        e("Barre flottante de l'organizer Dofus", "jeux", "Barre flottante", "organizer dofus barre flottante boutons persos par dessus jeu").org(),
        // Tableau de bord
        e("Score et analyse du PC", "0", "Relancer l'analyse", "score analyse sante composants note"),
        e("Tout corriger", "0", "Tout corriger", "corriger reparer ameliorer score"),
        e("Onduleur", "0", "Onduleur", "ups onduleur batterie secours coupure courant"),
        // Démarrage, Connexion, Nettoyage
        e("Programmes au démarrage", "2", "Programmes au démarrage", "demarrage windows lancement boot startup lent"),
        e("Désactiver les programmes conseillés au démarrage", "2", "Désactiver ce qui est conseillé", "demarrage accelerer boot"),
        e("Tester ma connexion (ping)", "3", "Tester ma connexion", "ping gigue jitter connexion internet stable"),
        e("Serveur DNS", "3", "Serveur DNS", "dns cloudflare google quad9"),
        e("Vider le cache DNS", "3", "Vider le cache DNS", "dns cache flush"),
        e("Redémarrer la carte réseau (vitesse de la carte)", "3", "Ta connexion", "carte reseau ethernet cable vitesse lente bridee 100 mbps 1 gbps gigabit relancer redemarrer veille"),
        e("Nettoyage du disque", "4", "Analyser", "nettoyer nettoyage fichiers temporaires place disque espace plein"),
        // Tests
        e("Tester la vitesse du disque", "5", "Tester la vitesse", "ssd hdd nvme disque vitesse lecture ecriture"),
        e("Santé du disque", "5", "Santé", "ssd hdd disque usure temperature smart"),
        e("Test du processeur", "5", "Test rapide (30 s)", "cpu processeur stabilite puissance"),
        e("Test de la mémoire", "5", "Tester la mémoire", "ram memoire barrette erreurs"),
        e("Carte graphique en direct", "5", "Surveiller en direct", "gpu temperature carte graphique nvidia amd"),
        e("Débit Internet", "5", "Tester ma connexion", "debit vitesse internet speedtest telechargement"),
        e("Lag quand la connexion est chargée", "5", "Lag en charge", "bufferbloat lag charge telechargement"),
        e("Pixels morts de l'écran", "5", "Écrans", "pixels morts ecran moniteur couleurs"),
        e("Batterie du portable", "5", "Batterie", "batterie portable autonomie usure"),
        // Sécurité
        e("Analyse antivirus rapide", "6", "Analyse rapide", "virus defender antivirus scan malware"),
        e("Analyse antivirus complète", "6", "Analyse complète", "virus defender antivirus scan complet"),
        e("Analyser un dossier", "6", "Analyser un dossier", "virus dossier fichier scan"),
        e("Mettre à jour l'antivirus", "6", "Mettre à jour la base", "defender definitions signatures antivirus"),
        e("Niveau de protection", "6", "Points à vérifier", "protection securite score pare feu"),
        e("Approuver un point à vérifier (c'est moi)", "6", "Points à vérifier", "approuver ok confiance faux positif ignorer autoriser tache planifiee suspecte"),
        // Sauvegarde
        e("Tout restaurer", "7", "Annuler les changements de Nevermind", "tout annuler restaurer revenir arriere defaire"),
        e("Historique des changements", "7", "Historique des changements", "historique annuler changement"),
        e("Point de restauration Windows", "7", "Point de restauration Windows", "restauration systeme sauvegarde"),
        e("Rapport du PC", "7", "Rapport de ton PC", "rapport export html partager configuration"),
        e("Mises à jour de Nevermind", "parametres", "Mises à jour", "version update maj nouvelle").tab("maj"),
        e("Versions bêta", "parametres", "Mises à jour", "beta preversion avant premiere").tab("maj"),
        e("Raccourci sur le bureau", "parametres", "Raccourci et démarrage", "raccourci bureau icone").tab("general"),
        e("Lancer Nevermind au démarrage du PC", "parametres", "Raccourci et démarrage", "demarrage automatique boot windows lancement auto allumage").tab("general"),
        // Réseau
        e("Scanner le réseau", "reseau", "Scanner le réseau", "appareils wifi box scan connectes"),
        e("Carte du réseau", "reseau", "Ton réseau", "carte constellation appareils map").geste(Geste::CarteReseau),
        e("Audit de sécurité du réseau", "reseau", "Audit de sécurité", "wifi wps upnp securite box audit"),
        e("Prévenir si un nouvel appareil se connecte", "reseau", "Appareils connectés", "alerte nouvel appareil intrus surveillance notification"),
        // Trafic
        e("Ce qui sort de ton PC", "trafic", "Ce qui sort de ton PC", "trafic internet donnees envoyees programmes connexions espion"),
        e("Identifier les serveurs sans nom", "trafic", "Identifier les serveurs sans nom", "rdap annuaire serveur ip proprietaire"),
        e("Pays du serveur du jeu (Lag en ligne)", "parametres", "Pays du serveur dans Lag en ligne", "pays localisation serveur jeu lag ping ou se trouve geolocalisation ipinfo").tab("reseau"),
        e("Vérifier le pilote NVIDIA au lancement", "parametres", "Vérifier le pilote NVIDIA au lancement", "pilote nvidia geforce dernier verification internet vie privee").tab("reseau"),
        e("Thème (couleurs de Nevermind)", "parametres", "", "theme couleur couleurs apparence design da sombre terminal arcade rubis crepuscule neon skin").tab("theme"),
        e("Paramètres de Nevermind", "parametres", "", "parametres reglages options configuration roue engrenage settings").tab("general"),
        e("Signaler un problème", "parametres", "Signaler un problème", "bug souci erreur rapport envoyer probleme aide").tab("aide"),
        e("Revoir la visite guidée", "parametres", "Visite guidée", "visite tutoriel aide decouvrir").tab("aide"),
        e("Ce que Windows envoie à Microsoft", "trafic", "Ce que Windows envoie à Microsoft", "telemetrie microsoft vie privee identifiant confidentialite diagnostic").geste(Geste::VieePrivee),
        e("Bloquer Internet à un programme", "trafic", "Ce qui sort de ton PC", "pare feu bloquer internet firewall programme"),
    ];
    if geree {
        v.retain(|x| x.t != "Raccourci sur le bureau" && x.t != "Versions bêta");
        for x in v.iter_mut().filter(|x| x.tab == "theme") {
            x.t = "Thème (se choisit dans Mindset)".into();
            x.a = THEME_GERE.into();
            x.k.push_str(" mindset suite");
        }
    }
    v
}

/// Les suggestions quand la barre est vide ($SearchStarters).
pub const SUGGESTIONS: [&str; 6] = [
    "Compteur de FPS à l'écran (overlay)",
    "Mes FPS ne sont pas normaux",
    "Lag en ligne",
    "Nettoyage du disque",
    "Lancer Nevermind au démarrage du PC",
    "Ce que Windows envoie à Microsoft",
];

fn libelle_sous(s: &str) -> &'static str {
    match s {
        "tweaks" => "Réglages Windows",
        "mode" => "Mode jeu",
        "profiles" => "Profils par jeu",
        _ => "",
    }
}

fn libelle_perf(s: &str) -> &'static str {
    match s {
        "fps" => "FPS",
        "lag" => "Lag en ligne",
        _ => "",
    }
}

pub fn libelle_onglet(s: &str) -> &'static str {
    match s {
        "general" => "Général",
        "jeux" => "Jeux",
        "reseau" => "Réseau et vie privée",
        "theme" => "Thème",
        "maj" => "Mises à jour",
        "aide" => "Aide",
        _ => "",
    }
}

/// Où se trouve un réglage, avec les noms de la console (Get-SearchWhere).
pub fn ou(x: &Entree) -> String {
    let sous = if !x.s.is_empty() {
        format!(" › {}", libelle_sous(&x.s))
    } else if !x.perf.is_empty() {
        format!(" › Perf. In Game › {}", libelle_perf(&x.perf))
    } else {
        String::new()
    };
    match x.p.as_str() {
        "reseau" => "Réseau".into(),
        "jeux" => "Jeux".into(),
        "trafic" => "Trafic".into(),
        "overlay" => "Overlay".into(),
        "parametres" => {
            if x.tab == "aide" {
                "Aide".into()
            } else {
                format!("Paramètres › {}", libelle_onglet(&x.tab))
            }
        }
        p => {
            let i: i64 = p.parse().unwrap_or(0);
            let famille = match i {
                1 | 2 | 4 => "Performances",
                3 | 6 | 7 => "Protection",
                _ => "",
            };
            let nom = match i {
                0 => "Tableau de bord",
                1 => "Optimisation gaming",
                2 => "Démarrage",
                3 => "Connexion",
                4 => "Nettoyage",
                5 => "Tests",
                6 => "Sécurité",
                7 => "Sauvegarde",
                _ => "",
            };
            if i == 0 {
                format!("Ton PC{sous}")
            } else if i == 5 {
                format!("Ton PC › Tests des composants{sous}")
            } else {
                format!("{famille} › {nom}{sous}")
            }
        }
    }
}

fn preparer(mut x: Entree) -> Entree {
    if x.ou.is_empty() {
        x.ou = ou(&x);
    }
    x.titre_n = texte_recherche(&x.t);
    x.titre_mots = x.titre_n.split(' ').map(String::from).collect();
    let mut mots: Vec<String> = Vec::new();
    for m in texte_recherche(&format!("{} {} {}", x.t, x.k, x.ou)).split(' ') {
        if !mots.iter().any(|y| y == m) {
            mots.push(m.to_string());
        }
    }
    x.mots = mots;
    x
}

/// Un jeu de ce PC, pour la recherche (nom et launcher).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JeuConnu {
    pub nom: String,
    pub source: String,
}

/// L'index complet (Get-SearchIndex) : réglages fixes (l'Organizer seulement s'il est proposé), réglages Windows de ce
/// PC, et les 80 premiers jeux (le jeu et son profil).
pub fn index(geree: bool, organizer: bool, reglages: &[String], jeux: &[JeuConnu]) -> Vec<Entree> {
    let mut v: Vec<Entree> = entrees(geree).into_iter().filter(|x| organizer || !x.org).collect();
    for t in reglages.iter().filter(|t| !t.is_empty()) {
        v.push(e(t, "1", t, "reglage windows optimisation").s("tweaks"));
    }
    for g in jeux.iter().filter(|g| !g.nom.is_empty()).take(80) {
        let mut j = e(&g.nom, "jeux", "", &format!("jeu lancer jouer {}", g.source));
        j.jeu = g.nom.clone();
        v.push(j);
        v.push(e(&format!("Profil de {}", g.nom), "1", &g.nom, "jeu profil priorite").s("profiles"));
    }
    v.into_iter().map(preparer).collect()
}

/// Une faute de frappe : lettre en trop, en moins, changée ou inversée (Test-OneTypo).
pub fn une_faute(a: &[char], b: &[char]) -> bool {
    if a == b {
        return true;
    }
    if a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    let mut i = 0;
    while i < a.len() && i < b.len() && a[i] == b[i] {
        i += 1;
    }
    let (ta, tb) = (&a[i..], &b[i..]);
    if ta.len() == tb.len() {
        if ta.is_empty() {
            return true;
        }
        if ta[1..] == tb[1..] {
            return true;
        }
        return ta.len() >= 2 && ta[0] == tb[1] && ta[1] == tb[0] && ta[2..] == tb[2..];
    }
    if ta.len() > tb.len() {
        return ta[1..] == *tb;
    }
    *ta == tb[1..]
}

/// Les réglages qui correspondent (Find-Settings) : chaque mot tapé doit se retrouver (début de mot, dedans, ou à une
/// faute près). Les meilleurs d'abord, puis les plus courts.
pub fn chercher(idx: &[Entree], requete: &str, max: usize) -> Vec<Entree> {
    let q: Vec<String> = texte_recherche(requete).split(' ').filter(|w| !w.is_empty()).map(String::from).collect();
    if q.is_empty() {
        return Vec::new();
    }
    let qn = q.join(" ");
    let mut touches: Vec<(f64, &Entree)> = Vec::new();
    'entrees: for x in idx {
        let mut score = 0.0;
        for w in &q {
            let wc: Vec<char> = w.chars().collect();
            let mut mieux: f64 = 0.0;
            for m in &x.mots {
                let mc: Vec<char> = m.chars().collect();
                let mut s = if m.starts_with(w.as_str()) {
                    3.0
                } else if wc.len() >= 3 && m.contains(w.as_str()) {
                    1.5
                } else if wc.len() >= 4
                    && (une_faute(&wc, &mc) || (mc.len() > wc.len() && une_faute(&wc, &mc[..wc.len()])))
                {
                    1.0
                } else {
                    0.0
                };
                if s > 0.0 && x.titre_mots.iter().any(|t| t == m) {
                    s += 1.5;
                }
                if s > mieux {
                    mieux = s;
                }
            }
            if mieux == 0.0 {
                continue 'entrees;
            }
            score += mieux;
        }
        if x.titre_n.starts_with(&qn) {
            score += 5.0;
        } else if x.titre_n.contains(&qn) {
            score += 2.0;
        }
        touches.push((score, x));
    }
    crate::tri::trier_ps(&mut touches, |a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.t.chars().count().cmp(&b.1.t.chars().count()))
    });
    touches.into_iter().take(max).map(|(_, x)| x.clone()).collect()
}

/// L'écran du kit où mène un réglage (Show-Page, Show-Settings d'accueil-console.ps1).
pub fn ecran(x: &Entree) -> String {
    match x.p.as_str() {
        "parametres" => if x.tab == "aide" { "aide" } else { "parametres" }.into(),
        "reseau" | "trafic" | "overlay" | "jeux" => x.p.clone(),
        p => match p.parse::<i64>().unwrap_or(-1) {
            0 => "tableau",
            1 => "optimisation",
            2 => "demarrage",
            3 => "connexion",
            4 => "nettoyage",
            5 => "tests",
            6 => "securite",
            7 => "sauvegarde",
            _ => "accueil",
        }
        .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn fautes() {
        assert!(une_faute(&c("overlay"), &c("overlya")));
        assert!(une_faute(&c("overlay"), &c("overly")));
        assert!(une_faute(&c("overlay"), &c("overlays")));
        assert!(une_faute(&c("overlay"), &c("averlay")));
        assert!(!une_faute(&c("overlay"), &c("ovrly")));
    }

    #[test]
    fn trouve() {
        let i = index(false, false, &[], &[]);
        let r = chercher(&i, "compteur", 8);
        assert!(r.iter().any(|x| x.t == "Compteur de FPS à l'écran (overlay)"));
        assert!(chercher(&i, "organizer", 8).is_empty());
        assert!(!chercher(&index(false, true, &[], &[]), "organizer", 8).is_empty());
    }
}
