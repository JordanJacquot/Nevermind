//! Le déroulé d'un test, comme tests.ps1 : le panneau qui s'ouvre (`Show-TestPanel`), les étapes et la courbe en
//! direct pendant le test (la minuterie de 100 ms), le résultat (`Invoke-ComponentTest`, les `Render`), le résumé de
//! la tuile (`Set-TileSummary`), « Voir le résultat » (`Show-LastResult`), « Refaire le test », « Arrêter le test »,
//! la santé d'un disque et la carte graphique en direct.
//!
//! Rien ici ne mesure ni ne lit le PC : le bloc lance le travail (`Demande`) dans un fil à part et donne au banc ce
//! qu'il en sait (`tic` à chaque tour, `fin` à la fin). Les juges font pareil avec des mesures inventées.

use super::kit::{self, DOUX, El, Fmt, INFO, OK, VIOLET, jauge, texte_doux, verdict};
use super::rendus::{self, CtxDisque};
use crate::net::{entier, n_fr};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Le travail d'un test, fait dans un fil à part
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Travail {
    Disque,
    Erreurs,
    Cpu,
    Memoire,
    Reseau,
    Lag,
}

impl Travail {
    pub fn nom(self) -> &'static str {
        match self {
            Travail::Disque => "disque",
            Travail::Erreurs => "erreurs",
            Travail::Cpu => "cpu",
            Travail::Memoire => "memoire",
            Travail::Reseau => "reseau",
            Travail::Lag => "lag",
        }
    }
}

/// Les serveurs du débit (`$NetWork`, `Test-Bufferbloat`)
pub const SERVEURS_DESCENTE: [&str; 4] = [
    "https://speed.cloudflare.com/__down?bytes=25000000",
    "https://proof.ovh.net/files/1Gb.dat",
    "https://nbg1-speed.hetzner.com/1GB.bin",
    "https://fsn1-speed.hetzner.com/1GB.bin",
];
pub const SERVEUR_MONTEE: &str = "https://speed.cloudflare.com/__up";

/// Ce que le banc demande au bloc
#[derive(Debug, Clone, PartialEq)]
pub enum Demande {
    /// Lancer le travail d'un test (dans un fil à part), avec ses paramètres (`$Def.Arg`)
    Mesure(Travail, Value),
    /// Relire un disque (Get-PhysicalDisk et ses compteurs) pour sa santé, puis `sante_lue`
    Sante(usize, String),
    /// Lire la carte NVIDIA (nvidia-smi), puis `gpu_lu`
    Gpu(usize),
    /// Effacer le fichier d'essai laissé par un ancien test interrompu (s'il existe)
    EffacerAncien(String),
    /// Un message (`Show-Message`)
    Message(String),
    /// Le texte de la pastille « en cours » (`Set-Status` pendant le test ; vide = fini)
    Occupe(String),
    /// Le débit mesuré, pour la page Connexion (`debit-mesure`)
    Debit(Vec<f64>),
}

/// Un tour de la minuterie de 100 ms : ce que dit le travail ([OGNative]) et la dernière mesure du processeur
#[derive(Debug, Clone, Default)]
pub struct Echantillon {
    pub phase: String,
    pub progres: f64,
    pub direct: f64,
    /// `$Live.CpuPerf` (rien si pas mesuré)
    pub perf: Option<f64>,
    /// Le numéro de la dernière mesure (`$Live.Updated`)
    pub mesure: u64,
}

/// Ce que rend le travail
#[derive(Debug, Clone, PartialEq)]
pub enum Resultat {
    R(Vec<f64>),
    Lignes(Vec<String>),
    Arrete,
    Erreur(String),
    /// Rien (le travail n'a rien rendu)
    Rien,
}

/// Le contexte d'un bouton (`$Ctx`)
#[derive(Debug, Clone, Default)]
pub enum Ctx {
    Disque(CtxDisque),
    Cpu(f64, f64),
    Gpu(Value),
    #[default]
    Aucun,
}

#[derive(Debug, Clone)]
struct DefCourbe {
    unite: &'static str,
    fmt: Fmt,
    cpu: bool,
    phases: &'static [&'static str],
    /// La fréquence de base (cpu), 0 sinon
    reference: f64,
}

/// La définition d'un test (`$def`)
#[derive(Debug, Clone)]
struct Def {
    travail: Travail,
    etapes: &'static [(&'static str, &'static str)],
    courbe: Option<DefCourbe>,
}

fn def(travail: Travail, base_mhz: f64) -> Def {
    let (etapes, courbe): (&'static [(&str, &str)], Option<DefCourbe>) = match travail {
        Travail::Disque => (
            &[("write", "Écriture"), ("read", "Lecture"), ("random", "Petits fichiers")],
            Some(DefCourbe { unite: "Mo/s", fmt: Fmt::N0, cpu: false, phases: &["write", "read"], reference: 0.0 }),
        ),
        Travail::Erreurs => (&[("scan", "Recherche d'erreurs (quelques minutes)")], None),
        Travail::Cpu => (
            &[("single", "Un seul cœur"), ("multi", "Tous les cœurs")],
            Some(DefCourbe {
                unite: "GHz",
                fmt: Fmt::N1,
                cpu: true,
                phases: &["single", "multi"],
                reference: base_mhz / 1000.0,
            }),
        ),
        Travail::Memoire => (
            &[
                ("alloc", "Réservation"),
                ("write", "Écriture"),
                ("read", "Vérification"),
                ("pattern", "2e passage"),
                ("copy", "Copie"),
            ],
            Some(DefCourbe {
                unite: "Go/s",
                fmt: Fmt::N1,
                cpu: false,
                phases: &["write", "read", "pattern"],
                reference: 0.0,
            }),
        ),
        Travail::Reseau => (
            &[("ping", "Ping"), ("down", "Téléchargement"), ("up", "Envoi")],
            Some(DefCourbe { unite: "Mb/s", fmt: Fmt::N0, cpu: false, phases: &["down", "up"], reference: 0.0 }),
        ),
        Travail::Lag => (
            &[("idle", "Au repos"), ("down", "Pendant un téléchargement"), ("up", "Pendant un envoi")],
            Some(DefCourbe { unite: "ms", fmt: Fmt::N0, cpu: false, phases: &["idle", "down", "up"], reference: 0.0 }),
        ),
    };
    Def { travail, etapes, courbe }
}

impl Def {
    fn libelles(&self) -> Vec<&'static str> {
        self.etapes.iter().map(|e| e.1).collect()
    }
    fn courbe_el(&self, points: &[f64]) -> Option<El> {
        let c = self.courbe.as_ref()?;
        let titre =
            if c.reference != 0.0 { format!("fréquence de base {} GHz", n_fr(c.reference, 1)) } else { String::new() };
        Some(El::Courbe(points.to_vec(), c.fmt, c.unite.into(), titre))
    }
}

/// Le test en cours (`$script:CurTest`)
#[derive(Debug, Clone)]
struct EnCours {
    def: Def,
    tuile: usize,
    ctx: Ctx,
    courante: Option<usize>,
    points: Vec<f64>,
    freq: HashMap<String, Vec<f64>>,
    dernier: Option<u64>,
    direct: String,
    libelle: String,
}

/// Le dernier résultat d'une tuile (`$Tile.Last`)
#[derive(Debug, Clone)]
enum Dernier {
    Test {
        def: Def,
        res: Resultat,
        freq: HashMap<String, Vec<f64>>,
        points: Vec<f64>,
        ctx: Ctx,
    },
    /// (santé d'un disque, carte graphique : « Voir le résultat » refait la lecture)
    Direct(Ctx),
}

/// Ce qui est relancé par « Refaire le test » (`$script:LastRun`)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Relance {
    Test(Travail),
    Sante,
    Gpu,
    /// (« Voir le résultat » : rien à relancer)
    Rien,
}

/// L'état d'une tuile de la page Tests
#[derive(Debug, Clone, Default)]
pub struct EtatTuile {
    /// Le statut du dernier test (la pastille), rien si pas encore testé
    pub statut: Option<String>,
    pub chips: Vec<(String, String)>,
    dernier: Option<Dernier>,
}

impl EtatTuile {
    /// Le bouton « Voir le résultat » est montré
    pub fn voir(&self) -> bool {
        self.statut.is_some()
    }
}

/// Les tuiles, vues du banc : titre, étiquette (`Tag`), sous-titre
#[derive(Debug, Clone, Default)]
pub struct InfoTuile {
    pub tag: String,
    pub titre: String,
    pub sous: String,
}

/// La carte graphique suivie en direct (`$script:GpuLive`)
#[derive(Debug, Clone)]
struct SuiviGpu {
    /// Les rangs, dans le corps du panneau, de la rangée de jauges et de la courbe
    jauges: usize,
    courbe: usize,
    nvidia: bool,
    dernier: Option<u64>,
}

/// Le panneau ouvert (`$ui.TestOverlay`)
#[derive(Debug, Clone, Default)]
pub struct Panneau {
    pub numero: u64,
    pub tag: String,
    pub titre: String,
    pub sous: String,
    /// L'état (`Set-TestState`) : run, live, ok, warn, bad, info, et son texte
    pub etat: String,
    pub etat_texte: String,
    pub progres: f64,
    pub pct: String,
    pub arreter: bool,
    pub refaire: bool,
    pub fermer: bool,
    pub corps: Vec<El>,
    /// Les éléments qui portent les boutons trouvés à la dernière lecture (`panneau:b:<n>`)
    boutons: Vec<usize>,
}

/// Les mesures en direct utiles aux tests (`$Live`)
#[derive(Debug, Clone, Default)]
pub struct Direct {
    pub base_mhz: f64,
    pub ram_totale: Option<f64>,
    pub ram_utilisee: f64,
    pub gpu: f64,
    pub gpu_temp: Option<f64>,
    pub gpu_conso: f64,
    pub cpu_perf: Option<f64>,
    /// Le numéro de la dernière mesure reçue (`$Live.Updated`)
    pub mesure: u64,
}

/// Ce que le banc lit du PC au moment d'un clic (comme tests.ps1 le lit tout de suite)
pub trait Lieu {
    /// `Get-VolInfo` : (taille, libre) d'un lecteur, rien s'il n'est pas prêt
    fn volume(&self, lettre: &str) -> Option<(f64, f64)>;
    /// `$env:SystemDrive`
    fn lecteur_systeme(&self) -> String;
    /// `$env:TEMP`
    fn temp(&self) -> String;
    /// `$SmiPath` : nvidia-smi est là
    fn smi(&self) -> bool;
}

#[derive(Debug, Default)]
pub struct Banc {
    pub panneau: Option<Panneau>,
    numero: u64,
    cours: Option<EnCours>,
    pub tuiles: Vec<EtatTuile>,
    relance: Option<(Relance, usize, Ctx)>,
    gpu: Option<SuiviGpu>,
}

const GO: f64 = 1024.0 * 1024.0 * 1024.0;
const MO: f64 = 1024.0 * 1024.0;

fn couleur_etat(etat: &str) -> &'static str {
    match etat {
        "run" => INFO,
        "live" | "ok" => OK,
        "warn" => kit::WARN,
        "bad" => kit::BAD,
        _ => DOUX,
    }
}

impl Banc {
    /// Un test tourne (`$script:TestRunning`) : les boutons des tuiles sont grisés
    pub fn occupe(&self) -> bool {
        self.cours.is_some()
    }

    /// Le travail du test en cours
    pub fn travail(&self) -> Option<Travail> {
        self.cours.as_ref().map(|c| c.def.travail)
    }

    /// La carte graphique est suivie en direct
    pub fn suivi_gpu(&self) -> bool {
        self.gpu.is_some()
    }

    fn tuile_mut(&mut self, k: usize) -> &mut EtatTuile {
        if self.tuiles.len() <= k {
            self.tuiles.resize(k + 1, EtatTuile::default());
        }
        &mut self.tuiles[k]
    }

    /// `Show-TestPanel`
    fn montrer(&mut self, t: &InfoTuile) -> &mut Panneau {
        self.numero += 1;
        self.panneau = Some(Panneau {
            numero: self.numero,
            tag: t.tag.clone(),
            titre: t.titre.clone(),
            sous: t.sous.clone(),
            etat: "info".into(),
            ..Default::default()
        });
        let p = self.panneau.as_mut().unwrap();
        // (Set-TestButtons : les boutons restent ceux de la fois d'avant, sauf ceux que le test change)
        p.fermer = true;
        p
    }

    fn boutons(p: &mut Panneau, mode: &str) {
        p.arreter = mode == "run";
        p.refaire = mode == "done";
        p.fermer = mode != "run";
    }

    /// `Hide-TestPanel` (« Fermer », la croix, Échap, ou une autre page)
    pub fn fermer(&mut self) {
        if self.cours.is_some() {
            return;
        }
        self.gpu = None;
        self.panneau = None;
    }

    /// Lancer un test (`Invoke-ComponentTest`) : le panneau, les étapes, la courbe
    #[allow(clippy::too_many_arguments)]
    fn lancer(
        &mut self,
        k: usize,
        t: &InfoTuile,
        travail: Travail,
        ctx: Ctx,
        arg: Value,
        base_mhz: f64,
        statut: String,
    ) -> Vec<Demande> {
        if self.cours.is_some() {
            return vec![Demande::Message("Un test est déjà en cours : attends qu'il se termine.".into())];
        }
        self.relance = Some((Relance::Test(travail), k, ctx.clone()));
        self.gpu = None;
        let d = def(travail, base_mhz);
        let p = self.montrer(t);
        p.etat = "run".into();
        p.etat_texte = "Test en cours".into();
        Self::boutons(p, "run");
        self.cours = Some(EnCours {
            def: d,
            tuile: k,
            ctx,
            courante: None,
            points: Vec::new(),
            freq: HashMap::new(),
            dernier: None,
            direct: "0".into(),
            libelle: "Préparation...".into(),
        });
        self.corps_en_cours();
        vec![Demande::Occupe(statut), Demande::Mesure(travail, arg)]
    }

    /// Le corps du panneau pendant le test : étapes, libellé, valeur en direct, courbe
    fn corps_en_cours(&mut self) {
        let (Some(c), Some(p)) = (&self.cours, &mut self.panneau) else { return };
        let mut corps = kit::etapes(&c.def.libelles(), c.courante, false);
        if let Some(dc) = &c.def.courbe {
            corps.push(texte_doux(&c.libelle));
            corps.push(El::Texte(c.direct.clone(), 44.0, kit::BLANC.into(), true));
            corps.push(El::Texte(dc.unite.into(), 16.0, DOUX.into(), true));
            corps.extend(c.def.courbe_el(&c.points));
        }
        p.corps = corps;
    }

    /// La minuterie de 100 ms pendant un test
    pub fn tic(&mut self, e: &Echantillon, base_mhz: f64) {
        let Some(c) = self.cours.as_mut() else { return };
        let prog = 100f64.min(e.progres);
        if let Some(p) = self.panneau.as_mut() {
            p.progres = prog.clamp(0.0, 100.0);
            p.pct = format!("{} %", n_fr(prog, 0));
        }
        let ph = e.phase.as_str();
        if !ph.is_empty()
            && let Some(i) = c.def.etapes.iter().position(|x| x.0 == ph)
            && c.courante != Some(i)
        {
            c.courante = Some(i);
        }
        if !ph.is_empty()
            && let Some(perf) = e.perf.filter(|x| *x != 0.0)
        {
            c.freq.entry(ph.to_string()).or_default().push(perf);
        }
        if let Some(dc) = c.def.courbe.clone()
            && !ph.is_empty()
            && dc.phases.contains(&ph)
        {
            let v = if dc.cpu {
                if c.dernier == Some(e.mesure) {
                    self.corps_en_cours();
                    return;
                }
                c.dernier = Some(e.mesure);
                match e.perf.filter(|x| *x != 0.0) {
                    Some(perf) if base_mhz != 0.0 => base_mhz * perf / 100.0 / 1000.0,
                    _ => 0.0,
                }
            } else {
                e.direct
            };
            c.points.push(v);
            if c.points.len() > 160 {
                c.points.remove(0);
            }
            c.direct = dc.fmt.f(v);
            c.libelle = c.def.etapes.iter().find(|x| x.0 == ph).map(|x| x.1).unwrap_or("").into();
        }
        self.corps_en_cours();
    }

    /// « Arrêter le test » : le travail est prévenu par le bloc
    pub fn arreter(&mut self) -> bool {
        let Some(p) = self.panneau.as_mut().filter(|p| p.arreter) else { return false };
        p.etat = "info".into();
        p.etat_texte = "Arrêt en cours...".into();
        true
    }

    /// La fin du travail : le résultat, ou l'échec, ou l'arrêt
    pub fn fin(&mut self, res: Resultat, base_mhz: f64) -> Vec<Demande> {
        if self.cours.is_none() {
            return Vec::new();
        }
        let mut demandes = vec![Demande::Occupe(String::new())];
        // (le corps tel qu'il est à la fin du travail : étapes, valeur en direct, courbe)
        self.corps_en_cours();
        let Some(c) = self.cours.take() else { return demandes };
        let Some(p) = self.panneau.as_mut() else { return demandes };
        Self::boutons(p, "done");
        match &res {
            Resultat::Erreur(_) | Resultat::Rien => {
                let msg = if let Resultat::Erreur(m) = &res { m.clone() } else { "erreur inconnue".into() };
                p.etat = "bad".into();
                p.etat_texte = "Échec".into();
                p.corps.push(verdict("bad", &format!("Le test n'a pas pu aller au bout : {msg}")));
                return demandes;
            }
            Resultat::Arrete => {
                p.etat = "info".into();
                p.etat_texte = "Arrêté".into();
                p.corps.push(verdict("info", "Test arrêté."));
                return demandes;
            }
            _ => {}
        }
        p.progres = 100.0;
        p.pct = "100 %".into();
        let mut corps = kit::etapes(&c.def.libelles(), None, true);
        if c.def.courbe.is_some() {
            corps.push(texte_doux("Courbe du test"));
            corps.extend(c.def.courbe_el(&c.points));
        }
        let (els, statut, chips) = rendre(&c.def, &res, &c.freq, &c.ctx, base_mhz);
        corps.extend(els);
        p.corps = corps;
        p.etat = statut.clone();
        p.etat_texte = match statut.as_str() {
            "warn" => "Terminé : à surveiller",
            "bad" => "Problème détecté",
            _ => "Terminé",
        }
        .into();
        if c.def.travail == Travail::Reseau
            && let Resultat::R(r) = &res
        {
            demandes.push(Demande::Debit(r.clone()));
        }
        let e = self.tuile_mut(c.tuile);
        e.dernier = Some(Dernier::Test {
            def: c.def.clone(),
            res,
            freq: c.freq.clone(),
            points: c.points.clone(),
            ctx: c.ctx.clone(),
        });
        e.statut = Some(statut);
        e.chips = chips;
        demandes
    }

    /// « Voir le résultat » (`Show-LastResult`)
    pub fn voir(&mut self, k: usize, t: &InfoTuile, base_mhz: f64, lieu: &dyn Lieu) -> Vec<Demande> {
        let Some(d) = self.tuiles.get(k).and_then(|e| e.dernier.clone()) else { return Vec::new() };
        match d {
            Dernier::Direct(ctx) => match ctx {
                Ctx::Gpu(_) => self.gpu_demande(k, ctx),
                Ctx::Disque(c) => self.sante_demande(k, c),
                _ => Vec::new(),
            },
            Dernier::Test { def, res, freq, points, ctx } => {
                let _ = lieu;
                let p = self.montrer(t);
                Self::boutons(p, "done");
                let mut corps = kit::etapes(&def.libelles(), None, true);
                if def.courbe.is_some() && !points.is_empty() {
                    corps.push(texte_doux("Courbe du test"));
                    corps.extend(def.courbe_el(&points));
                }
                let p = self.panneau.as_mut().unwrap();
                p.progres = 100.0;
                p.pct = String::new();
                let (els, statut, _) = rendre(&def, &res, &freq, &ctx, base_mhz);
                corps.extend(els);
                p.corps = corps;
                p.etat_texte = (if statut == "ok" {
                    "Terminé"
                } else if statut == "bad" {
                    "Problème détecté"
                } else {
                    "Terminé : à surveiller"
                })
                .into();
                p.etat = statut;
                self.relance = Some((Relance::Rien, k, ctx));
                self.gpu = None;
                Vec::new()
            }
        }
    }

    /// « Refaire le test » : ce qu'il faut relancer
    pub fn relance(&self) -> Option<(Relance, usize, Ctx)> {
        self.panneau.as_ref().filter(|p| p.refaire)?;
        self.relance.clone()
    }

    // ---- Les tests à lancer (Test-DiskSpeed, Test-DiskErrors, Test-Cpu, Test-Memory, Test-NetSpeed, Test-Bufferbloat)

    pub fn disque_vitesse(&mut self, k: usize, t: &InfoTuile, c: &CtxDisque, lieu: &dyn Lieu) -> Vec<Demande> {
        let lettre = c.lettres.first().cloned().unwrap_or_default();
        let vol = lieu.volume(&lettre);
        let Some((_, libre)) = vol.filter(|(_, l)| *l >= GO) else {
            return vec![Demande::Message(format!(
                "Il faut au moins 1 Go de libre sur le lecteur {lettre}: pour faire ce test."
            ))];
        };
        let taille = if libre > 40.0 * GO {
            8.0 * GO
        } else if libre > 10.0 * GO {
            2.0 * GO
        } else {
            256.0 * MO
        };
        let fichier = if format!("{lettre}:").eq_ignore_ascii_case(&lieu.lecteur_systeme()) {
            joindre(&lieu.temp(), "Nevermind-test-disque.tmp")
        } else {
            format!("{lettre}:\\Nevermind-test-disque.tmp")
        };
        let dossier = match fichier.rfind('\\') {
            Some(i) if i > 0 => {
                let d = &fichier[..i];
                if d.ends_with(':') { format!("{d}\\") } else { d.to_string() }
            }
            _ => String::new(),
        };
        let ancien = joindre(&dossier, "OptiGame-test-disque.tmp");
        let mut l = vec![Demande::EffacerAncien(ancien)];
        l.extend(self.lancer(
            k,
            t,
            Travail::Disque,
            Ctx::Disque(c.clone()),
            json!({ "File": fichier, "Size": taille as i64 }),
            0.0,
            format!("Test de vitesse : {}...", c.nom),
        ));
        l
    }

    pub fn disque_erreurs(&mut self, k: usize, t: &InfoTuile, c: &CtxDisque) -> Vec<Demande> {
        self.lancer(
            k,
            t,
            Travail::Erreurs,
            Ctx::Disque(c.clone()),
            json!({ "Letters": c.lettres }),
            0.0,
            format!("Recherche d'erreurs : {}...", c.nom),
        )
    }

    pub fn cpu(&mut self, k: usize, t: &InfoTuile, single: f64, multi: f64, base_mhz: f64) -> Vec<Demande> {
        self.lancer(
            k,
            t,
            Travail::Cpu,
            Ctx::Cpu(single, multi),
            json!({ "Single": single as i64, "Multi": multi as i64 }),
            base_mhz,
            "Test du processeur...".into(),
        )
    }

    pub fn memoire(&mut self, k: usize, t: &InfoTuile, live: &Direct) -> Vec<Demande> {
        let libre = match live.ram_totale.filter(|x| *x != 0.0) {
            Some(tot) => tot - live.ram_utilisee,
            None => 2.0 * GO,
        };
        let octets = entier((2.0 * GO).min((256.0 * MO).max(libre * 0.5)));
        self.lancer(k, t, Travail::Memoire, Ctx::Aucun, json!({ "Bytes": octets }), 0.0, "Test de la mémoire...".into())
    }

    pub fn reseau(&mut self, k: usize, t: &InfoTuile) -> Vec<Demande> {
        self.lancer(k, t, Travail::Reseau, Ctx::Aucun, json!({}), 0.0, "Test de la connexion...".into())
    }

    pub fn lag(&mut self, k: usize, t: &InfoTuile) -> Vec<Demande> {
        self.lancer(
            k,
            t,
            Travail::Lag,
            Ctx::Aucun,
            json!({ "Down": SERVEURS_DESCENTE, "Up": [SERVEUR_MONTEE] }),
            0.0,
            "Mesure du lag en charge...".into(),
        )
    }

    // ---- Santé d'un disque (Show-DiskHealth)

    pub fn sante_demande(&mut self, k: usize, c: CtxDisque) -> Vec<Demande> {
        vec![Demande::Sante(k, c.id)]
    }

    /// Le disque relu : `d` (rien = introuvable), `rel` ses compteurs (Get-StorageReliabilityCounter, souvent refusé
    /// sans les droits), `rel_aide` ceux de l'Aide, `vols` la taille et la place libre de chaque lettre
    #[allow(clippy::too_many_arguments)]
    pub fn sante_lue(
        &mut self,
        k: usize,
        t: &InfoTuile,
        c: &CtxDisque,
        d: Option<&Value>,
        rel: Option<&Value>,
        rel_aide: Option<&Value>,
        vols: &[Option<(f64, f64)>],
    ) -> Vec<Demande> {
        let Some(d) = d.filter(|d| !d.is_null()) else { return vec![Demande::Message("Disque introuvable.".into())] };
        self.relance = Some((Relance::Sante, k, Ctx::Disque(c.clone())));
        self.gpu = None;
        let p = self.montrer(t);
        Self::boutons(p, "done");
        p.refaire = false;
        let rel = rel.filter(|r| !r.is_null()).or(rel_aide.filter(|r| !r.is_null()));
        let (corps, statut, chips, etat_texte) = rendus::sante(c, d, rel, vols);
        let p = self.panneau.as_mut().unwrap();
        p.corps = corps;
        p.etat = statut.clone();
        p.etat_texte = etat_texte;
        p.progres = 100.0;
        p.pct = String::new();
        let e = self.tuile_mut(k);
        if e.dernier.is_none() {
            e.dernier = Some(Dernier::Direct(Ctx::Disque(c.clone())));
        }
        e.statut = Some(statut);
        e.chips = chips;
        Vec::new()
    }

    // ---- Carte graphique en direct (Show-GpuMonitor)

    pub fn gpu_demande(&mut self, k: usize, _ctx: Ctx) -> Vec<Demande> {
        vec![Demande::Gpu(k)]
    }

    /// nvidia-smi lu (`smi` = sa première ligne, rien si pas de carte NVIDIA ou pas de réponse)
    pub fn gpu_lu(
        &mut self,
        k: usize,
        t: &InfoTuile,
        g: &Value,
        smi: Option<&str>,
        a_smi: bool,
        live: &Direct,
    ) -> Vec<Demande> {
        self.relance = Some((Relance::Gpu, k, Ctx::Gpu(g.clone())));
        let p = self.montrer(t);
        Self::boutons(p, "done");
        p.refaire = false;
        p.etat = "live".into();
        p.etat_texte = "En direct".into();
        let nom = crate::net::champ_texte(g, "Name");
        let nvidia = a_smi && {
            let n = nom.to_lowercase();
            n.contains("nvidia") || n.contains("geforce")
        };
        let info: Option<Vec<String>> = if nvidia {
            smi.filter(|o| !o.is_empty()).map(|o| o.split(',').map(|x| x.trim().to_string()).collect())
        } else {
            None
        };
        let mut jauges = vec![jauge("Utilisation", live.gpu, Fmt::N0, "%", INFO)];
        if nvidia {
            jauges.push(jauge("Température", live.gpu_temp.unwrap_or(0.0), Fmt::N0, "°C", OK));
            jauges.push(jauge("Consommation", live.gpu_conso, Fmt::N0, "W", kit::WARN));
        }
        let mut corps = vec![
            texte_doux("Lance un jeu, puis reviens ici avec Alt + Tab : tout se met à jour en direct."),
            El::Jauges(jauges),
            texte_doux("Utilisation de la carte graphique"),
            El::Courbe(Vec::new(), Fmt::N0, "%".into(), String::new()),
        ];
        let (details, statut) = rendus::gpu_details(g, info.as_deref());
        corps.extend(details);
        let p = self.panneau.as_mut().unwrap();
        p.corps = corps;
        p.progres = 0.0;
        p.pct = String::new();
        self.gpu = Some(SuiviGpu { jauges: 1, courbe: 3, nvidia, dernier: None });
        let mut chips = vec![(
            "Température".to_string(),
            match live.gpu_temp.filter(|x| *x != 0.0) {
                Some(x) => format!("{} °C", entier(x)),
                None => "?".into(),
            },
        )];
        if let Some(i) = &info {
            chips.push((
                "Consommation".into(),
                format!("{} W", entier(rendus::nombre_smi(i.get(9).map(String::as_str).unwrap_or("")))),
            ));
        }
        let e = self.tuile_mut(k);
        e.dernier = Some(Dernier::Direct(Ctx::Gpu(g.clone())));
        e.statut = Some(statut.into());
        e.chips = chips;
        Vec::new()
    }

    /// La surveillance de la carte graphique, toutes les 500 ms : une nouvelle mesure fait bouger les jauges et la courbe
    pub fn tic_gpu(&mut self, live: &Direct) -> bool {
        let (Some(g), Some(p)) = (self.gpu.as_mut(), self.panneau.as_mut()) else { return false };
        if g.dernier == Some(live.mesure) {
            return false;
        }
        g.dernier = Some(live.mesure);
        if let Some(El::Jauges(js)) = p.corps.get_mut(g.jauges) {
            if let Some(j) = js.get_mut(0) {
                j.valeur = live.gpu;
            }
            if g.nvidia {
                if let Some(j) = js.get_mut(1) {
                    j.valeur = live.gpu_temp.unwrap_or(0.0);
                }
                if let Some(j) = js.get_mut(2) {
                    j.valeur = live.gpu_conso;
                }
            }
        }
        if let Some(El::Courbe(pts, ..)) = p.corps.get_mut(g.courbe) {
            pts.push(live.gpu);
            if pts.len() > 160 {
                pts.remove(0);
            }
        }
        true
    }

    // ---- Ce que l'écran reçoit

    /// `Get-PanneauKit` : la page du panneau (avec `numero`, pour l'hôte)
    pub fn page(&mut self) -> Option<Value> {
        let p = self.panneau.as_mut()?;
        let tr = kit::traduire(&p.corps);
        p.boutons = tr.boutons;
        let ton = kit::ton(couleur_etat(&p.etat));
        // ($IconesPanneau d'ecran-panneau.ps1, d'après l'étiquette du panneau)
        let icone = match p.tag.as_str() {
            "FPS" => "jauge",
            "LAG" => "ondes",
            "AV" | "SÉCU" => "bouclier",
            "TMP" => "balai",
            "NET" => "courbe",
            "WIN" => "windows",
            "DEL" => "poubelle",
            "!" => "aide",
            _ => "test",
        };
        let en_cours = p.arreter;
        let mut boutons = Vec::new();
        if en_cours {
            boutons.push(json!({ "id": "panneau:stop", "texte": "Arrêter le test", "style": "danger" }));
        }
        if p.refaire {
            boutons.push(json!({ "id": "panneau:again", "texte": "Refaire le test" }));
        }
        if p.fermer && !en_cours {
            boutons.push(json!({ "id": "panneau:close", "texte": "Fermer", "style": "second" }));
        }
        let module = if en_cours && p.progres > 0.0 && p.progres < 100.0 && !p.pct.is_empty() {
            json!({ "anneau": { "valeur": entier(p.progres), "sur": 100, "petit": "%" },
                "titre": if p.etat_texte.is_empty() { "En cours".to_string() } else { p.etat_texte.clone() }, "texte": p.sous, "boutons": boutons })
        } else {
            json!({ "icone": match ton { "ok" => "ok", "attention" | "urgent" => "alerte", _ => icone },
                "couleur": match ton { "ok" => "vert", "attention" => "orange", "urgent" => "rouge", _ => "P" },
                "titre": if p.etat_texte.is_empty() { p.titre.clone() } else { p.etat_texte.clone() }, "texte": p.sous, "boutons": boutons })
        };
        Some(
            json!({ "titre": p.titre, "sous": p.sous, "icone": icone, "couleur": "P", "module": module, "blocs": tr.blocs, "numero": p.numero }),
        )
    }

    /// Un bouton du panneau (`panneau:b:<n>`) : les détails repliables s'ouvrent ou se ferment
    pub fn bouton(&mut self, n: usize) {
        let Some(p) = self.panneau.as_mut() else { return };
        if let Some(i) = p.boutons.get(n).copied()
            && let Some(El::Details(_, ouvert)) = p.corps.get_mut(i)
        {
            *ouvert = !*ouvert;
        }
    }
}

/// `Join-Path` : un dossier et un nom
fn joindre(dossier: &str, nom: &str) -> String {
    if dossier.ends_with('\\') { format!("{dossier}{nom}") } else { format!("{dossier}\\{nom}") }
}

/// Le `Render` du test : éléments, statut, cases de la tuile
fn rendre(def: &Def, res: &Resultat, freq: &HashMap<String, Vec<f64>>, ctx: &Ctx, base_mhz: f64) -> rendus::Rendu {
    let vide = Vec::new();
    let r = match res {
        Resultat::R(r) => r,
        _ => &vide,
    };
    match def.travail {
        Travail::Disque => {
            let sorte = if let Ctx::Disque(c) = ctx { c.sorte.as_str() } else { "" };
            rendus::disque(r, sorte)
        }
        Travail::Erreurs => {
            let lignes = match res {
                Resultat::Lignes(l) => l.clone(),
                Resultat::R(r) => r.iter().map(|x| crate::net::texte_nombre(*x)).collect(),
                _ => Vec::new(),
            };
            rendus::erreurs(&lignes)
        }
        Travail::Cpu => {
            let multi = if let Ctx::Cpu(_, m) = ctx { *m } else { 0.0 };
            rendus::cpu(
                r,
                rendus::ghz(freq.get("single"), base_mhz, true),
                rendus::ghz(freq.get("multi"), base_mhz, false),
                base_mhz,
                multi,
            )
        }
        Travail::Memoire => rendus::memoire(r),
        Travail::Reseau => rendus::reseau(r),
        Travail::Lag => rendus::lag(r),
    }
}

#[allow(dead_code)]
const _VIOLET: &str = VIOLET;
