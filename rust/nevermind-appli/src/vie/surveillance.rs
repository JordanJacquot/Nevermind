//! Les surveillances de Nevermind, comme les minuteries de evenements.ps1 et vue.ps1, pilotées par le temps que
//! donne l'appelant ([`Vie::avancer`]) :
//!
//! - toutes les 400 ms, la veille de la page (`Update-VeilleVue`, vue.ps1:239-264) : regardée ou non
//!   (`Test-Regardee`) ; cachée, la page est mise en pause tout de suite, puis en veille profonde au bout d'une minute ;
//! - toutes les 600 ms, les demandes (`Watch-Requests`, evenements.ps1:90-111) : accrochage dans Mindset
//!   (`Update-EmbedState`), quitter.demande, afficher.demande ;
//! - toutes les 2,5 s, le thème de la suite pour une copie gérée (`Watch-SuiteTheme`, evenements.ps1:144-169).
//!
//! Ce qui concerne l'écran (les ronds Menu et Fermer, l'écran refait) est rendu à l'appelant en [`Evenement`].

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::demarrage::{AFFICHER, PAGE_NEVERMIND, QUITTER, lire_suite, ouvrir_dans_mindset};
use super::{Fenetre, Systeme, Taille};

pub const TOUR_VEILLE: Duration = Duration::from_millis(400);
pub const TOUR_DEMANDES: Duration = Duration::from_millis(600);
pub const TOUR_SUITE: Duration = Duration::from_millis(2500);
/// La page cachée depuis une minute passe en veille profonde (vue.ps1:261).
pub const AVANT_VEILLE: Duration = Duration::from_secs(60);

/// Comment l'appli finit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fin {
    /// Quitter pour de bon
    Quitter,
    /// Se relancer avec le nouveau thème (`Set-AppTheme`), avec `-Demarrage` si elle était cachée ou gérée
    Relancer { cachee: bool },
}

/// Ce que les surveillances disent à l'appelant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evenement {
    /// Accrochée dans Mindset ou décrochée (`$script:Embedded`)
    Accrochee(bool),
    /// La page est regardée ou non (`Test-Regardee`) : l'écran se refait au retour
    Regardee(bool),
    /// Le clic sur une bulle de Mindset (`notif:<id>` dans afficher.demande)
    Bulle(String),
    /// Le thème de la suite a changé : à enregistrer (`Set-Setting 'Theme'`) avant la relance
    Theme(String),
    /// Fin de l'appli
    Fin(Fin),
}

/// Ce que la vie de l'appli a besoin de savoir, venu du démarrage.
#[derive(Debug, Clone)]
pub struct Reglages {
    pub gere: bool,
    pub fichier_suite: PathBuf,
    pub dossier_mindset: PathBuf,
    pub dossier_donnees: PathBuf,
    /// Le dossier des packs (`$PacksDir`)
    pub packs: PathBuf,
    /// La taille minimale hors de Mindset (interface.xaml : 920 x 600)
    pub taille_min: Taille,
    /// Le thème actuel (`Get-CurrentThemeKey` : « pack:<id> » ou un des 5)
    pub theme: String,
    /// La date de suite.json au démarrage (`$SuiteStamp`, Nevermind.ps1:52)
    pub date_suite: Option<SystemTime>,
}

/// La vie de l'appli, de la fin du démarrage à sa fin.
#[derive(Debug)]
pub struct Vie {
    r: Reglages,
    prochain_veille: Duration,
    prochain_demandes: Duration,
    prochain_suite: Duration,
    accrochee: bool,
    regardee: Option<bool>,
    cachee_depuis: Option<Duration>,
    en_veille: bool,
    date_suite: Option<SystemTime>,
    attente_theme: Option<String>,
    pack_absent: Option<String>,
    fin: bool,
    /// Une partie est en cours (signal des blocs) : regardée seulement si Nevermind est devant
    pub partie: bool,
}

/// `Get-ThemePack` réussit (un pack.json lisible dans le dossier du pack) : comme themes.ps1:78-82.
pub fn pack_present(packs: &Path, id: &str) -> bool {
    let f = packs.join(id).join("pack.json");
    std::fs::read(&f)
        .ok()
        .and_then(|o| {
            serde_json::from_str::<serde_json::Value>(String::from_utf8_lossy(&o).trim_start_matches('\u{feff}')).ok()
        })
        .is_some()
}

impl Vie {
    pub fn nouvelle(r: Reglages) -> Vie {
        Vie {
            date_suite: r.date_suite,
            r,
            prochain_veille: TOUR_VEILLE,
            prochain_demandes: TOUR_DEMANDES,
            prochain_suite: TOUR_SUITE,
            accrochee: false,
            regardee: None,
            cachee_depuis: None,
            en_veille: false,
            attente_theme: None,
            pack_absent: None,
            fin: false,
            partie: false,
        }
    }

    pub fn accrochee(&self) -> bool {
        self.accrochee
    }

    /// La page est regardée (rien avant le premier tour : pas encore).
    pub fn regardee(&self) -> bool {
        self.regardee == Some(true)
    }

    pub fn reglages(&self) -> &Reglages {
        &self.r
    }

    /// Fait les tours dus jusqu'à `maintenant` (le temps depuis le début des surveillances) : un tour de chaque, au
    /// plus. En retard de plus d'un tour (le PC sort de veille, une longue pause), les tours manqués ne sont pas
    /// rattrapés un par un : chaque tour lit l'état actuel, un seul suffit, et le suivant repart de maintenant (petit
    /// de l'audit du 08/11 : après une nuit en veille, des dizaines de milliers de tours d'un coup sur le fil de la
    /// fenêtre).
    pub fn avancer(
        &mut self,
        maintenant: Duration,
        f: &mut dyn Fenetre,
        s: &mut dyn Systeme,
        journal: &mut dyn FnMut(&str),
    ) -> Vec<Evenement> {
        fn suivant(prochain: Duration, tour: Duration, maintenant: Duration) -> Duration {
            let p = prochain + tour;
            if p <= maintenant { maintenant + tour } else { p }
        }
        let mut ev = Vec::new();
        if !self.fin && maintenant >= self.prochain_veille {
            self.prochain_veille = suivant(self.prochain_veille, TOUR_VEILLE, maintenant);
            self.veille(maintenant, f, journal, &mut ev);
        }
        if !self.fin && maintenant >= self.prochain_demandes {
            self.prochain_demandes = suivant(self.prochain_demandes, TOUR_DEMANDES, maintenant);
            self.demandes(f, s, journal, &mut ev);
        }
        if !self.fin && maintenant >= self.prochain_suite {
            self.prochain_suite = suivant(self.prochain_suite, TOUR_SUITE, maintenant);
            if self.r.gere {
                self.suite(f, journal, &mut ev);
            }
        }
        ev
    }

    /// La croix, `WM_CLOSE` (le bouton « Fermer » de Mindset) ou « Quitter » : une seule fois.
    pub fn quitter(&mut self) -> Option<Fin> {
        if self.fin {
            return None;
        }
        self.fin = true;
        Some(Fin::Quitter)
    }

    /// `Test-Regardee` (vue.ps1:234-237) : à l'écran (la fenêtre et ses parents visibles, la fenêtre principale pas
    /// réduite), et pendant une partie seulement si elle est devant.
    pub fn est_regardee(&self, f: &dyn Fenetre) -> bool {
        let n = f.niveaux();
        let vue = !n.is_empty() && n.iter().all(|x| x.visible) && !n.last().is_some_and(|x| x.reduite);
        vue && !(self.partie && !f.est_devant())
    }

    /// `Update-VeilleVue` (vue.ps1:239-264).
    fn veille(
        &mut self,
        maintenant: Duration,
        f: &mut dyn Fenetre,
        journal: &mut dyn FnMut(&str),
        ev: &mut Vec<Evenement>,
    ) {
        let regardee = self.est_regardee(f);
        if regardee {
            if self.regardee == Some(true) {
                return;
            }
            self.regardee = Some(true);
            self.cachee_depuis = None;
            if self.en_veille {
                f.page_en_veille(false);
                self.en_veille = false;
            }
            f.page_visible(true);
            journal("Écran : de nouveau à l'écran");
            ev.push(Evenement::Regardee(true));
            return;
        }
        if self.regardee != Some(false) {
            self.regardee = Some(false);
            self.cachee_depuis = Some(maintenant);
            f.page_visible(false);
            journal("Écran : caché, en pause");
            ev.push(Evenement::Regardee(false));
            return;
        }
        if !self.en_veille && self.cachee_depuis.is_some_and(|d| maintenant.saturating_sub(d) >= AVANT_VEILLE) {
            f.page_en_veille(true);
            self.en_veille = true;
            journal("Écran : en veille");
        }
    }

    /// `Watch-Requests` (evenements.ps1:90-111).
    fn demandes(
        &mut self,
        f: &mut dyn Fenetre,
        s: &mut dyn Systeme,
        journal: &mut dyn FnMut(&str),
        ev: &mut Vec<Evenement>,
    ) {
        self.accrochage(f, journal, ev);
        let quitter = self.r.dossier_donnees.join(QUITTER);
        if quitter.is_file() {
            let _ = std::fs::remove_file(&quitter);
            journal("Fermeture demandée par Mindset");
            self.fin = true;
            ev.push(Evenement::Fin(Fin::Quitter));
            return;
        }
        let afficher = self.r.dossier_donnees.join(AFFICHER);
        if afficher.is_file() {
            let demande = std::fs::read_to_string(&afficher).unwrap_or_default().trim().to_string();
            let _ = std::fs::remove_file(&afficher);
            // (le clic sur une bulle : seul un id de 12 chiffres hexadécimaux est accepté, evenements.ps1:103). La
            //  fenêtre n'est pas montrée : Mindset, qui a montré la bulle, se montre et ouvre lui-même Nevermind
            //  (Open-BulleAppli, zone.ps1:157-167) ; une bulle que Nevermind n'a pas donnée ne fait donc rien du tout
            //  (contrat de la suite, partie 5 ; DIVERGENCES.md)
            if let Some(id) = demande.strip_prefix("notif:")
                && id.len() == 12
                && id.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                ev.push(Evenement::Bulle(id.to_string()));
            } else if !self.accrochee {
                self.montrer(f, s);
            }
        }
    }

    /// `Show-MainWindow` (assistance.ps1:239-251).
    pub fn montrer(&mut self, f: &mut dyn Fenetre, s: &mut dyn Systeme) {
        if self.r.gere && !self.accrochee && ouvrir_dans_mindset(&self.r.dossier_mindset, s) {
            return;
        }
        if self.accrochee {
            f.montrer_sans_activer();
            return;
        }
        f.montrer_et_activer();
    }

    /// `Update-EmbedState` (evenements.ps1:61-87).
    fn accrochage(&mut self, f: &mut dyn Fenetre, journal: &mut dyn FnMut(&str), ev: &mut Vec<Evenement>) {
        if !f.existe() {
            return;
        }
        let on = f.est_accrochee();
        if on == self.accrochee {
            return;
        }
        self.accrochee = on;
        f.poser_barre_propre(on);
        if on {
            f.poser_taille_minimale(None);
            if !f.est_visible() {
                f.montrer_sans_activer();
            }
        } else {
            f.poser_taille_minimale(Some(self.r.taille_min));
        }
        journal(if on { "Fenêtre affichée dans Mindset" } else { "Fenêtre décrochée de Mindset" });
        ev.push(Evenement::Accrochee(on));
    }

    /// `Test-ThemeSansGener` (evenements.ps1:123-131) : on regarde Nevermind (ou Mindset qui l'affiche), pas le jeu.
    fn theme_sans_gener(&self, f: &dyn Fenetre) -> bool {
        f.premier_plan_est_a_moi() || (self.accrochee && f.est_visible() && f.est_devant())
    }

    /// `Watch-SuiteTheme` (evenements.ps1:144-169).
    fn suite(&mut self, f: &mut dyn Fenetre, journal: &mut dyn FnMut(&str), ev: &mut Vec<Evenement>) {
        let date = std::fs::metadata(&self.r.fichier_suite).ok().and_then(|m| m.modified().ok()).or(self.date_suite);
        if date == self.date_suite {
            return;
        }
        let t = lire_suite(&self.r.fichier_suite).theme;
        let Some(t) = t.filter(|t| *t != self.r.theme) else {
            self.date_suite = date;
            return;
        };
        if self.partie && !self.theme_sans_gener(f) {
            if self.attente_theme.as_deref() != Some(&t) {
                journal(&format!(
                    "Thème de la suite changé dans Mindset : {t}, appliqué après la partie en cours (ou dès que Nevermind ou Mindset est à l'écran)"
                ));
                self.attente_theme = Some(t);
            }
            return;
        }
        self.date_suite = date;
        if let Some(id) = t.strip_prefix("pack:")
            && !pack_present(&self.r.packs, id)
        {
            if self.pack_absent.as_deref() != Some(&t) {
                journal(&format!(
                    "Thème de la suite changé dans Mindset : {t} absent ou illisible dans {}, thème actuel gardé",
                    self.r.packs.display()
                ));
                self.pack_absent = Some(t);
            }
            self.date_suite = None;
            return;
        }
        journal(&format!("Thème de la suite changé dans Mindset : {t}"));
        journal(&format!("Thème : {t}"));
        self.fin = true;
        ev.push(Evenement::Theme(t));
        // (géré par Mindset : relancé caché, Mindset le raccroche ; Nevermind.ps1:488)
        ev.push(Evenement::Fin(Fin::Relancer { cachee: true }));
    }

    /// La relance (Nevermind.ps1:481-491) : les verrous rendus, puis Mindset.exe du dossier (Nevermind.exe s'il
    /// manque), avec `-Demarrage` (caché ou géré).
    pub fn relancer(&self, cachee: bool, programme: Option<&Path>, s: &mut dyn Systeme, journal: &mut dyn FnMut(&str)) {
        s.liberer_verrous();
        let args: &[&str] = if cachee || self.r.gere { &["-Demarrage"] } else { &[] };
        match programme.filter(|p| p.is_file()) {
            Some(exe) => {
                if let Err(e) = s.lancer(exe, args, None) {
                    journal(&format!("Relance : {e}"));
                }
            }
            None => journal("Relance : programme introuvable"),
        }
    }

    /// Le chemin de la page de Nevermind dans Mindset (pour les tests).
    pub fn page_mindset() -> &'static str {
        PAGE_NEVERMIND
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Niveau, PriseVerrou};
    use super::*;
    use std::io;

    #[derive(Default)]
    struct FausseFenetre {
        accrochee: bool,
        visible: bool,
        reduite: bool,
        devant: bool,
        a_moi: bool,
        min: Vec<Option<Taille>>,
        barre: Vec<bool>,
        page: Vec<bool>,
        veille: Vec<bool>,
        montree: u32,
        activee: u32,
        /// Les lectures de la fenêtre (une par tour de surveillance au moins)
        lectures: std::cell::Cell<u32>,
    }
    impl Fenetre for FausseFenetre {
        fn existe(&self) -> bool {
            true
        }
        fn est_accrochee(&self) -> bool {
            self.lectures.set(self.lectures.get() + 1);
            self.accrochee
        }
        fn est_visible(&self) -> bool {
            self.visible
        }
        fn montrer_sans_activer(&mut self) {
            self.visible = true;
            self.montree += 1;
        }
        fn montrer_et_activer(&mut self) {
            self.visible = true;
            self.activee += 1;
        }
        fn cacher(&mut self) {
            self.visible = false;
        }
        fn reduire(&mut self) {
            self.reduite = true;
        }
        fn poser_taille_minimale(&mut self, t: Option<Taille>) {
            self.min.push(t);
        }
        fn poser_barre_propre(&mut self, oui: bool) {
            self.barre.push(oui);
        }
        fn niveaux(&self) -> Vec<Niveau> {
            vec![Niveau { visible: self.visible, reduite: self.reduite }]
        }
        fn est_devant(&self) -> bool {
            self.devant
        }
        fn premier_plan_est_a_moi(&self) -> bool {
            self.a_moi
        }
        fn page_visible(&mut self, v: bool) {
            self.page.push(v);
        }
        fn page_en_veille(&mut self, v: bool) {
            self.veille.push(v);
        }
    }

    #[derive(Default)]
    struct FauxSysteme {
        mindset: Option<String>,
        lances: Vec<(String, Vec<String>)>,
        liberes: u32,
        ouvertures: Vec<String>,
    }
    impl Systeme for FauxSysteme {
        fn verrou_existe(&mut self, _: &str) -> bool {
            false
        }
        fn prendre_verrou(&mut self, _: &str, _: Duration) -> PriseVerrou {
            PriseVerrou::Pris
        }
        fn liberer_verrous(&mut self) {
            self.liberes += 1;
        }
        fn emplacement_mindset(&mut self) -> Option<String> {
            self.mindset.clone()
        }
        fn lancer(&mut self, exe: &Path, a: &[&str], _: Option<&Path>) -> io::Result<()> {
            self.lances.push((exe.display().to_string(), a.iter().map(|s| s.to_string()).collect()));
            Ok(())
        }
        fn ouvrir_par_explorateur(&mut self, c: &str) -> io::Result<()> {
            self.ouvertures.push(c.into());
            Ok(())
        }
        fn permettre_premier_plan(&mut self) {}
        fn lever_signal(&mut self, _: &str) {}
        fn dormir(&mut self, _: Duration) {}
        fn demander_mindset(&mut self, _: bool) -> bool {
            false
        }
    }

    struct Essai(PathBuf);
    impl Drop for Essai {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn reglages(nom: &str, gere: bool) -> (Essai, Reglages) {
        let d = nevermind_noyau::dossier_essai(&format!("hote-vie-{nom}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("Donnees")).unwrap();
        std::fs::create_dir_all(d.join("Mindset")).unwrap();
        let r = Reglages {
            gere,
            fichier_suite: d.join(r"Mindset\suite.json"),
            dossier_mindset: d.join("Mindset"),
            dossier_donnees: d.join("Donnees"),
            packs: d.join(r"Donnees\packs"),
            taille_min: Taille { largeur: 920.0, hauteur: 600.0 },
            theme: "neon".into(),
            date_suite: None,
        };
        (Essai(d), r)
    }

    /// Petit de l'audit du 08/11 : au réveil du PC (une nuit en veille), les tours manqués étaient tous rattrapés d'un
    /// coup sur le fil de la fenêtre (toutes les 400 ms, 600 ms et 2,5 s : des dizaines de milliers de tours, avec
    /// des lectures de fichiers) : une pause visible. Un seul tour est fait, puis tout repart de maintenant.
    #[test]
    fn au_reveil_les_tours_manques_ne_sont_pas_rattrapes_un_par_un() {
        let (_e, r) = reglages("reveil", true);
        let mut v = Vie::nouvelle(r);
        let (mut f, mut s) = (FausseFenetre::default(), FauxSysteme::default());
        v.avancer(Duration::from_secs(1), &mut f, &mut s, &mut |_| {});
        let avant = f.lectures.get();
        // Huit heures plus tard
        let nuit = Duration::from_secs(8 * 3600);
        v.avancer(nuit, &mut f, &mut s, &mut |_| {});
        let tours = f.lectures.get() - avant;
        assert!(tours <= 3, "{tours} tours rattrapés d'un coup au réveil");
        // Et la suite reprend au rythme normal
        let avant = f.lectures.get();
        v.avancer(nuit + Duration::from_millis(1200), &mut f, &mut s, &mut |_| {});
        assert!((1..=3).contains(&(f.lectures.get() - avant)), "{}", f.lectures.get() - avant);
    }

    #[test]
    fn accrochage_comme_update_embedstate() {
        let (_e, r) = reglages("accroche", true);
        let mut v = Vie::nouvelle(r);
        let (mut f, mut s) = (FausseFenetre { accrochee: true, ..Default::default() }, FauxSysteme::default());
        let mut j = Vec::new();
        let ev = v.avancer(Duration::from_millis(600), &mut f, &mut s, &mut |t| j.push(t.to_string()));
        assert!(ev.contains(&Evenement::Accrochee(true)));
        assert_eq!((f.min.clone(), f.barre.clone(), f.montree), (vec![None], vec![true], 1));
        assert!(j.contains(&"Fenêtre affichée dans Mindset".to_string()));
        // Rien ne change : rien de refait
        let ev = v.avancer(Duration::from_millis(1200), &mut f, &mut s, &mut |_| {});
        assert!(!ev.iter().any(|e| matches!(e, Evenement::Accrochee(_))));
        f.accrochee = false;
        let ev = v.avancer(Duration::from_millis(1800), &mut f, &mut s, &mut |_| {});
        assert!(ev.contains(&Evenement::Accrochee(false)));
        assert_eq!(f.min.last(), Some(&Some(Taille { largeur: 920.0, hauteur: 600.0 })));
        assert_eq!(f.barre.last(), Some(&false));
    }

    #[test]
    fn veille_de_la_page() {
        let (_e, r) = reglages("veille", true);
        let mut v = Vie::nouvelle(r);
        let (mut f, mut s) = (FausseFenetre::default(), FauxSysteme::default());
        let ev = v.avancer(Duration::from_millis(400), &mut f, &mut s, &mut |_| {});
        assert_eq!(ev, vec![Evenement::Regardee(false)]);
        assert_eq!(f.page, vec![false]);
        v.avancer(Duration::from_secs(59), &mut f, &mut s, &mut |_| {});
        assert!(f.veille.is_empty(), "pas avant une minute");
        v.avancer(Duration::from_secs(61), &mut f, &mut s, &mut |_| {});
        assert_eq!(f.veille, vec![true], "une minute cachée : en veille");
        f.visible = true;
        let ev = v.avancer(Duration::from_millis(61_400), &mut f, &mut s, &mut |_| {});
        assert!(ev.contains(&Evenement::Regardee(true)));
        assert_eq!((f.veille.clone(), f.page.clone()), (vec![true, false], vec![false, true]));
        // Pendant une partie : regardée seulement si elle est devant
        v.partie = true;
        let ev = v.avancer(Duration::from_millis(61_800), &mut f, &mut s, &mut |_| {});
        assert!(ev.contains(&Evenement::Regardee(false)), "un jeu devant : pas regardée");
        f.devant = true;
        let ev = v.avancer(Duration::from_millis(62_200), &mut f, &mut s, &mut |_| {});
        assert!(ev.contains(&Evenement::Regardee(true)));
        // Réduite : pas regardée
        f.reduite = true;
        let ev = v.avancer(Duration::from_millis(62_600), &mut f, &mut s, &mut |_| {});
        assert!(ev.contains(&Evenement::Regardee(false)));
    }

    #[test]
    fn quitter_demande_par_mindset() {
        let (_e, r) = reglages("quitter", true);
        std::fs::write(r.dossier_donnees.join(QUITTER), "1234").unwrap();
        let q = r.dossier_donnees.join(QUITTER);
        let mut v = Vie::nouvelle(r);
        let (mut f, mut s) = (FausseFenetre::default(), FauxSysteme::default());
        let mut j = Vec::new();
        let ev = v.avancer(Duration::from_millis(600), &mut f, &mut s, &mut |t| j.push(t.to_string()));
        assert_eq!(ev.last(), Some(&Evenement::Fin(Fin::Quitter)));
        assert!(!q.exists() && j.contains(&"Fermeture demandée par Mindset".to_string()));
        assert!(v.avancer(Duration::from_secs(5), &mut f, &mut s, &mut |_| {}).is_empty(), "une seule fois");
        assert_eq!(v.quitter(), None);
    }

    #[test]
    fn demande_d_affichage() {
        // Gérée, pas accrochée, Mindset introuvable : sa propre fenêtre au premier plan
        let (_e, r) = reglages("afficher", true);
        let a = r.dossier_donnees.join(AFFICHER);
        std::fs::write(&a, "4242").unwrap();
        let mut v = Vie::nouvelle(r.clone());
        let (mut f, mut s) = (FausseFenetre::default(), FauxSysteme::default());
        v.avancer(Duration::from_millis(600), &mut f, &mut s, &mut |_| {});
        assert!(!a.exists());
        assert_eq!(f.activee, 1);
        // Accrochée dans Mindset : on ne touche pas à sa fenêtre
        let mut v = Vie::nouvelle(r);
        let mut f = FausseFenetre { accrochee: true, visible: true, ..Default::default() };
        v.avancer(Duration::from_millis(600), &mut f, &mut s, &mut |_| {});
        std::fs::write(&a, "notif:0123456789ab").unwrap();
        v.avancer(Duration::from_millis(1200), &mut f, &mut s, &mut |_| {});
        assert!(!a.exists());
        assert_eq!(f.activee, 0);
    }

    #[test]
    fn bulle_cliquee_sans_rien_montrer() {
        // (contrat de la suite, partie 5 : « notif:000000000000 », une bulle qu'elle n'a pas donnée : lue, supprimée,
        //  rien ne se montre ; une bulle à elle est rendue à l'hôte, Mindset se montre lui-même)
        let (_e, r) = reglages("bulle", true);
        let a = r.dossier_donnees.join(AFFICHER);
        let mut v = Vie::nouvelle(r);
        let (mut f, mut s) = (FausseFenetre::default(), FauxSysteme::default());
        std::fs::write(&a, "notif:000000000000").unwrap();
        let ev = v.avancer(Duration::from_millis(600), &mut f, &mut s, &mut |_| {});
        assert!(!a.exists());
        assert_eq!((f.activee, f.montree), (0, 0));
        assert!(ev.contains(&Evenement::Bulle("000000000000".into())));
        // Mal écrite (majuscules, trop courte) : comme un numéro de processus, la fenêtre est montrée
        std::fs::write(&a, "notif:0123456789AB").unwrap();
        let ev = v.avancer(Duration::from_millis(1200), &mut f, &mut s, &mut |_| {});
        assert!(!a.exists() && !ev.iter().any(|e| matches!(e, Evenement::Bulle(_))));
        assert_eq!(f.activee, 1);
    }

    #[test]
    fn theme_de_la_suite_suivi() {
        let (_e, mut r) = reglages("theme", true);
        std::fs::write(&r.fichier_suite, r#"{"theme":"neon"}"#).unwrap();
        r.date_suite = std::fs::metadata(&r.fichier_suite).unwrap().modified().ok();
        let fs = r.fichier_suite.clone();
        let packs = r.packs.clone();
        let mut v = Vie::nouvelle(r);
        let (mut f, mut s) = (FausseFenetre::default(), FauxSysteme::default());
        assert!(
            !v.avancer(Duration::from_millis(2500), &mut f, &mut s, &mut |_| {})
                .iter()
                .any(|e| matches!(e, Evenement::Fin(_)))
        );
        // Un pack absent : gardé, noté une fois
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&fs, r#"{"theme":"pack:absent"}"#).unwrap();
        let mut j = Vec::new();
        v.avancer(Duration::from_millis(5000), &mut f, &mut s, &mut |t| j.push(t.to_string()));
        v.avancer(Duration::from_millis(7500), &mut f, &mut s, &mut |t| j.push(t.to_string()));
        assert_eq!(j.iter().filter(|l| l.contains("absent ou illisible")).count(), 1, "{j:?}");
        // Pendant une partie (jeu devant) : il attend
        v.partie = true;
        std::fs::write(&fs, r#"{"theme":"rubis"}"#).unwrap();
        let ev = v.avancer(Duration::from_millis(10_000), &mut f, &mut s, &mut |_| {});
        assert!(!ev.iter().any(|e| matches!(e, Evenement::Fin(_))));
        // On regarde Nevermind : appliqué, relance cachée
        f.a_moi = true;
        let ev = v.avancer(Duration::from_millis(12_500), &mut f, &mut s, &mut |_| {});
        assert!(ev.contains(&Evenement::Theme("rubis".into())), "{ev:?}");
        assert!(ev.contains(&Evenement::Fin(Fin::Relancer { cachee: true })));
        // Un pack présent est accepté
        let (_e2, mut r2) = reglages("theme-pack", true);
        std::fs::create_dir_all(r2.packs.join("mon-pack")).unwrap();
        std::fs::write(r2.packs.join(r"mon-pack\pack.json"), "{\"Name\":\"x\"}").unwrap();
        std::fs::write(&r2.fichier_suite, r#"{"theme":"pack:mon-pack"}"#).unwrap();
        r2.date_suite = None;
        let mut v = Vie::nouvelle(r2);
        let ev = v.avancer(Duration::from_millis(2500), &mut f, &mut s, &mut |_| {});
        assert!(ev.contains(&Evenement::Theme("pack:mon-pack".into())));
        let _ = packs;
    }

    #[test]
    fn relance_avec_demarrage() {
        let (e, r) = reglages("relance", true);
        let exe = e.0.join("Mindset.exe");
        std::fs::write(&exe, "x").unwrap();
        let v = Vie::nouvelle(r);
        let mut s = FauxSysteme::default();
        v.relancer(false, Some(&exe), &mut s, &mut |_| {});
        assert_eq!(s.liberes, 1);
        assert_eq!(s.lances, vec![(exe.display().to_string(), vec!["-Demarrage".to_string()])]);
    }
}
