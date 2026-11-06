//! La page « Tests des composants » (tests.ps1 `Build-TestsTab`, lue par ecran-divers.ps1 `Get-TuilesTestsKit`) :
//! une tuile par composant, avec ses boutons, la pastille et le résumé du dernier test ; et les tests eux-mêmes
//! (vitesse des disques, santé, recherche d'erreurs, processeur, mémoire, carte graphique en direct, connexion, lag
//! en charge, pixels morts), dans `crate::tests` : le déroulé (banc), les résultats (rendus), le panneau (kit) et
//! les mesures (mesures, dans un fil à part).

use crate::net::{self, champ_liste, champ_nombre, champ_texte, taille};
use crate::tests::banc::{Banc, Ctx, Demande, Direct, Echantillon, InfoTuile, Lieu, Relance, Resultat, Travail};
use crate::tests::kit;
use crate::tests::rendus::CtxDisque;
use regex::Regex;
use serde_json::{Value, json};

/// Ce que fait un bouton d'une tuile
#[derive(Debug, Clone, PartialEq)]
pub enum Bouton {
    Vitesse,
    Sante,
    Erreurs,
    /// Test du processeur : secondes sur un cœur, secondes sur tous
    Cpu(f64, f64),
    Memoire,
    Mdsched,
    Gpu,
    TroisDMark,
    Reseau,
    Lag,
    Pixels(usize),
    Batterie,
    Voir,
}

#[derive(Debug, Clone)]
struct Tuile {
    tag: &'static str,
    titre: String,
    sous: String,
    desc: &'static str,
    boutons: Vec<(String, Bouton)>,
    ctx: Ctx,
}

/// Ce que le bloc doit faire après un geste (ce que le banc ne fait pas lui-même)
#[derive(Debug, Clone, PartialEq)]
pub enum Geste {
    Demande(Demande),
    /// Lancer un programme (mdsched.exe)
    Lancer(Vec<String>),
    /// Le rapport de batterie (powercfg /batteryreport, puis l'ouvrir)
    Batterie,
    /// Ouvrir une adresse (3DMark dans Steam)
    Ouvrir(String),
    /// Les couleurs en plein écran sur l'écran n
    Pixels(usize),
}

/// Ce que la page sait du PC (les données de l'analyse, les écrans, portable ou pas) et l'état des tests
#[derive(Debug, Default)]
pub struct Tests {
    pub donnees: Value,
    pub ecrans: Vec<String>,
    pub portable: bool,
    /// 3DMark est installé dans Steam (`Test-3DMark`)
    pub troisdmark: bool,
    boutons: Vec<(usize, Bouton)>,
    tuiles: Vec<Tuile>,
    /// Ce dont les tuiles dépendent (pour les refaire)
    cle: String,
    pub banc: Banc,
    /// Les mesures en direct utiles aux tests (`$Live`)
    pub direct: Direct,
    /// Le travail en cours dans un fil à part
    pub atelier: crate::tests::Atelier,
}

fn icone(tag: &str) -> &'static str {
    match tag {
        "SSD" | "HDD" | "USB" => "disque",
        "CPU" => "puce",
        "RAM" => "memoire",
        "GPU" | "HZ" => "ecran",
        "NET" => "ondes",
        "BAT" => "eclair",
        _ => "test",
    }
}

impl Tests {
    fn construire(&self) -> Vec<Tuile> {
        let d = &self.donnees;
        let mut l = Vec::new();
        for dd in champ_liste(d, "Disks") {
            let k = dd.get("Disk").cloned().unwrap_or(Value::Null);
            let (media, bus) = (champ_texte(&k, "MediaType"), champ_texte(&k, "BusType"));
            let e = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
            let sorte = if e(&bus, "NVMe") {
                "NVMe"
            } else if e(&media, "SSD") {
                "SSD"
            } else if e(&media, "HDD") {
                "HDD"
            } else if e(&bus, "USB") {
                "USB"
            } else {
                "SSD"
            };
            let lib = match sorte {
                "NVMe" => "SSD NVMe",
                "SSD" => "SSD",
                "HDD" => "Disque dur",
                _ => "Disque externe",
            };
            let lettres: Vec<String> = champ_liste(&dd, "Letters").iter().map(net::texte).collect();
            let sous = format!(
                "{lib}, {}{}",
                taille(champ_nombre(&k, "Size")),
                if lettres.is_empty() {
                    String::new()
                } else {
                    format!("  /  {}", lettres.iter().map(|x| format!("{x}:")).collect::<Vec<_>>().join(" "))
                }
            );
            let mut b = Vec::new();
            if !lettres.is_empty() {
                b.push(("Tester la vitesse".to_string(), Bouton::Vitesse));
            }
            b.push(("Santé".into(), Bouton::Sante));
            if !lettres.is_empty() {
                b.push(("Erreurs".into(), Bouton::Erreurs));
            }
            let nom: String = champ_texte(&k, "FriendlyName").trim().into();
            l.push(Tuile {
                tag: match sorte {
                    "HDD" => "HDD",
                    "USB" => "USB",
                    _ => "SSD",
                },
                titre: nom.clone(),
                sous,
                desc: "Vitesse réelle et santé du disque (usure, température, erreurs).",
                boutons: b,
                ctx: Ctx::Disque(CtxDisque {
                    id: champ_texte(&k, "DeviceId"),
                    nom,
                    sorte: sorte.into(),
                    libelle: lib.into(),
                    lettres,
                }),
            });
        }
        let cpu = d.get("CPU").cloned().unwrap_or(Value::Null);
        let espaces = Regex::new(r"\s+").unwrap();
        l.push(Tuile {
            tag: "CPU",
            titre: espaces.replace_all(&champ_texte(&cpu, "Name"), " ").trim().into(),
            sous: format!(
                "{} cœurs, {} threads",
                champ_texte(&cpu, "NumberOfCores"),
                champ_texte(&cpu, "NumberOfLogicalProcessors")
            ),
            desc: "Puissance, vitesse tenue quand il chauffe et stabilité. Ferme tes jeux avant.",
            boutons: vec![
                ("Test rapide (30 s)".into(), Bouton::Cpu(8.0, 22.0)),
                ("Stabilité (5 min)".into(), Bouton::Cpu(5.0, 295.0)),
            ],
            ctx: Ctx::Aucun,
        });
        let mem = champ_liste(d, "Mem");
        let go = net::entier(net::arrondi(
            mem.iter().map(|m| champ_nombre(m, "Capacity")).sum::<f64>() / (1024.0 * 1024.0 * 1024.0),
        ));
        l.push(Tuile {
            tag: "RAM",
            titre: "Mémoire vive".into(),
            sous: format!("{go} Go, {} barrette{}", mem.len(), if mem.len() > 1 { "s" } else { "" }),
            desc: "Vitesse et recherche d'erreurs en quelques secondes.",
            boutons: vec![
                ("Tester la mémoire".into(), Bouton::Memoire),
                ("Test complet Windows".into(), Bouton::Mdsched),
            ],
            ctx: Ctx::Aucun,
        });
        let virt = Regex::new(r"(?i)Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta|Microsoft Basic").unwrap();
        for g in champ_liste(d, "GPUs").into_iter().filter(|g| !virt.is_match(&champ_texte(g, "Name"))) {
            let mut b = vec![("Surveiller en direct".to_string(), Bouton::Gpu)];
            if self.troisdmark {
                b.push(("3DMark".into(), Bouton::TroisDMark));
            }
            l.push(Tuile {
                tag: "GPU",
                titre: champ_texte(&g, "Name"),
                sous: "Carte graphique".into(),
                desc: "Température, utilisation et consommation en direct, et ralentissements éventuels.",
                boutons: b,
                ctx: Ctx::Gpu(g),
            });
        }
        l.push(Tuile { tag: "NET", titre: "Connexion Internet".into(), sous: "Ping, téléchargement, envoi".into(),
            desc: "Réactivité et vitesse de ta connexion en 20 secondes, et « lag en charge » : ton ping monte-t-il quand quelqu'un télécharge ?",
            boutons: vec![("Tester ma connexion".into(), Bouton::Reseau), ("Lag en charge".into(), Bouton::Lag)], ctx: Ctx::Aucun });
        let n = self.ecrans.len();
        let mut b = Vec::new();
        for (i, e) in self.ecrans.iter().enumerate() {
            let principal = e.split('|').nth(6) == Some("1");
            b.push((
                format!("Écran {}{}", i + 1, if principal && n > 1 { " (principal)" } else { "" }),
                Bouton::Pixels(i),
            ));
        }
        l.push(Tuile {
            tag: "HZ",
            titre: "Écrans".into(),
            sous: format!("{n} écran{}", if n > 1 { "s" } else { "" }),
            desc: "Couleurs unies en plein écran pour repérer les pixels morts. Échap pour quitter.",
            boutons: b,
            ctx: Ctx::Aucun,
        });
        if self.portable && !champ_liste(d, "Battery").is_empty() {
            l.push(Tuile {
                tag: "BAT",
                titre: "Batterie".into(),
                sous: "Rapport de Windows".into(),
                desc: "Capacité d'origine, capacité actuelle et autonomie estimée.",
                boutons: vec![("Voir le rapport".into(), Bouton::Batterie)],
                ctx: Ctx::Aucun,
            });
        }
        l
    }

    /// Les tuiles (faites une fois, comme Build-TestsTab ; refaites quand ce qu'elles montrent arrive après coup, les
    /// écrans par exemple, tant qu'aucun test n'a été fait : les numéros des tuiles restent ceux des résultats gardés)
    fn tuiles(&mut self) -> usize {
        let n = |k: &str| self.donnees.get(k).and_then(Value::as_array).map_or(0, Vec::len);
        let cle = format!(
            "{}|{}|{}|{}|{}|{}",
            self.ecrans.len(),
            self.portable,
            self.troisdmark,
            n("Disks"),
            n("GPUs"),
            n("Battery")
        );
        if cle != self.cle && self.banc.tuiles.iter().all(|t| t.statut.is_none()) && !self.banc.occupe() {
            self.tuiles.clear();
            self.cle = cle;
        }
        if self.tuiles.is_empty() && !self.donnees.is_null() {
            self.tuiles = self.construire();
        }
        self.tuiles.len()
    }

    fn info(&self, k: usize) -> InfoTuile {
        self.tuiles.get(k).map_or_else(InfoTuile::default, |t| InfoTuile {
            tag: t.tag.into(),
            titre: t.titre.clone(),
            sous: t.sous.clone(),
        })
    }

    /// La fréquence de base du processeur (`$Live.BaseMHz`, lue par l'analyse)
    pub fn base_mhz(&self) -> f64 {
        net::entier(champ_nombre(self.donnees.get("CPU").unwrap_or(&Value::Null), "MaxClockSpeed")) as f64
    }

    /// `$pages.tests` (ecran-divers.ps1)
    pub fn page(&mut self) -> Value {
        self.boutons.clear();
        let slash = Regex::new(r"\s+/\s+").unwrap();
        let mut blocs = Vec::new();
        let (mut faits, mut total) = (0, 0);
        let n = self.tuiles();
        for k in 0..n {
            let t = self.tuiles[k].clone();
            let etat = self.banc.tuiles.get(k).cloned().unwrap_or_default();
            let ton = etat.statut.as_deref().map_or("", |s| kit::ton(kit::couleur_statut(s)));
            total += 1;
            if !ton.is_empty() {
                faits += 1;
            }
            let note = [slash.replace_all(&t.sous, ", disque ").to_string(), t.desc.to_string()]
                .into_iter()
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            let mut bs = Vec::new();
            let mut liste = t.boutons.clone();
            if etat.voir() {
                liste.push(("Voir le résultat".into(), Bouton::Voir));
            }
            for (j, (texte, sorte)) in liste.into_iter().enumerate() {
                self.boutons.push((k, sorte));
                bs.push(json!({ "id": format!("test-b:{}", self.boutons.len() - 1), "texte": texte, "style": if j > 0 { Some("second") } else { None } }));
            }
            let (couleur, etiquette) = match ton {
                "ok" => ("vert", "Bon"),
                "attention" => ("orange", "À surveiller"),
                "urgent" => ("rouge", "Problème"),
                _ => ("P", "Pas encore testé"),
            };
            blocs.push(json!({ "type": "ligne", "icone": icone(t.tag), "couleur": couleur, "texte": t.titre, "note": note, "etiquette": etiquette, "ton": ton, "boutons": bs }));
            if !etat.chips.is_empty() {
                blocs.push(json!({ "type": "chiffres", "elements": etat.chips.iter().map(|(a, b)| json!({ "titre": a, "valeur": b })).collect::<Vec<_>>() }));
            }
        }
        let blocs = if blocs.is_empty() {
            vec![json!({ "type": "texte", "texte": "Lecture de tes composants..." })]
        } else {
            std::iter::once(json!({ "type": "titre", "texte": "Tes composants" })).chain(blocs).collect()
        };
        let module = if total > 0 && faits > 0 {
            json!({ "anneau": { "valeur": faits, "sur": total, "petit": format!("/{total}") },
                "titre": format!("{faits} composant{} testé{}", if faits > 1 { "s" } else { "" }, if faits > 1 { "s" } else { "" }),
                "texte": "Ferme tes jeux avant de lancer un test : il prend toute la puissance du composant." })
        } else {
            json!({ "icone": "test", "couleur": "S", "titre": "Aucun test pour le moment", "texte": "Choisis un composant : chaque test dure de quelques secondes à quelques minutes. Ferme tes jeux avant." })
        };
        json!({ "titre": "Tests des composants", "parent": "pc", "sous": "Vérifie que chaque pièce de ton PC fonctionne bien et à la bonne vitesse.", "icone": "test", "couleur": "S",
            "module": module, "blocs": blocs })
    }

    /// Ce que fait un bouton d'une tuile (`test-b:N`) : la tuile et le bouton
    pub fn sorte(&self, id: &str) -> Option<(usize, Bouton)> {
        id.strip_prefix("test-b:").and_then(|n| n.parse::<usize>().ok()).and_then(|k| self.boutons.get(k).cloned())
    }

    /// Le panneau ouvert, pour l'écran
    pub fn panneau(&mut self) -> Option<Value> {
        self.banc.page()
    }

    /// Le panneau des tests est ouvert
    pub fn panneau_ouvert(&self) -> bool {
        self.banc.panneau.is_some()
    }

    /// Un bouton d'une tuile (`test-b:N`) : `None` si l'identifiant n'est pas un bouton
    pub fn clic(&mut self, id: &str, lieu: &dyn Lieu) -> Option<Vec<Geste>> {
        let (k, b) = self.sorte(id)?;
        // (pendant un test, les boutons des tuiles sont grisés ; « Voir le résultat » aussi, voir DIVERGENCES.md)
        if self.banc.occupe() {
            return Some(Vec::new());
        }
        Some(self.bouton(k, b, lieu))
    }

    fn bouton(&mut self, k: usize, b: Bouton, lieu: &dyn Lieu) -> Vec<Geste> {
        let t = self.info(k);
        let ctx = self.tuiles.get(k).map(|t| t.ctx.clone()).unwrap_or_default();
        let base = self.base_mhz();
        let d = |l: Vec<Demande>| l.into_iter().map(Geste::Demande).collect::<Vec<_>>();
        match (b, ctx) {
            (Bouton::Vitesse, Ctx::Disque(c)) => d(self.banc.disque_vitesse(k, &t, &c, lieu)),
            (Bouton::Sante, Ctx::Disque(c)) => d(self.banc.sante_demande(k, c)),
            (Bouton::Erreurs, Ctx::Disque(c)) => d(self.banc.disque_erreurs(k, &t, &c)),
            (Bouton::Cpu(s, m), _) => d(self.banc.cpu(k, &t, s, m, base)),
            (Bouton::Memoire, _) => {
                let live = self.direct.clone();
                d(self.banc.memoire(k, &t, &live))
            }
            (Bouton::Mdsched, _) => vec![Geste::Lancer(vec!["mdsched.exe".into()])],
            (Bouton::Gpu, c @ Ctx::Gpu(_)) => d(self.banc.gpu_demande(k, c)),
            (Bouton::TroisDMark, _) => vec![Geste::Ouvrir("steam://rungameid/223850".into())],
            (Bouton::Reseau, _) => d(self.banc.reseau(k, &t)),
            (Bouton::Lag, _) => d(self.banc.lag(k, &t)),
            (Bouton::Pixels(i), _) => vec![Geste::Pixels(i)],
            (Bouton::Batterie, _) => vec![Geste::Batterie],
            (Bouton::Voir, _) => d(self.banc.voir(k, &t, base, lieu)),
            _ => Vec::new(),
        }
    }

    /// Un bouton du panneau (`panneau:stop`, `panneau:again`, `panneau:close`, `panneau:b:<n>`)
    pub fn panneau_action(&mut self, id: &str, lieu: &dyn Lieu) -> Vec<Geste> {
        match id {
            "panneau:stop" => {
                self.banc.arreter();
                Vec::new()
            }
            "panneau:close" => {
                self.banc.fermer();
                Vec::new()
            }
            "panneau:again" => {
                let Some((r, k, ctx)) = self.banc.relance() else { return Vec::new() };
                let b = match (r, ctx) {
                    (Relance::Test(Travail::Disque), _) => Bouton::Vitesse,
                    (Relance::Test(Travail::Erreurs), _) => Bouton::Erreurs,
                    (Relance::Test(Travail::Cpu), Ctx::Cpu(s, m)) => Bouton::Cpu(s, m),
                    (Relance::Test(Travail::Memoire), _) => Bouton::Memoire,
                    (Relance::Test(Travail::Reseau), _) => Bouton::Reseau,
                    (Relance::Test(Travail::Lag), _) => Bouton::Lag,
                    (Relance::Sante, _) => Bouton::Sante,
                    (Relance::Gpu, _) => Bouton::Gpu,
                    _ => return Vec::new(),
                };
                self.bouton(k, b, lieu)
            }
            _ => {
                if let Some(n) = id.strip_prefix("panneau:b:").and_then(|n| n.parse::<usize>().ok()) {
                    self.banc.bouton(n);
                }
                Vec::new()
            }
        }
    }

    /// La santé d'un disque relue (après `Demande::Sante`)
    pub fn sante_lue(
        &mut self,
        k: usize,
        d: Option<&Value>,
        rel: Option<&Value>,
        rel_aide: Option<&Value>,
        lieu: &dyn Lieu,
    ) -> Vec<Demande> {
        let t = self.info(k);
        let Some(Ctx::Disque(c)) = self.tuiles.get(k).map(|t| t.ctx.clone()) else { return Vec::new() };
        let vols: Vec<Option<(f64, f64)>> = c.lettres.iter().map(|l| lieu.volume(l)).collect();
        self.banc.sante_lue(k, &t, &c, d, rel, rel_aide, &vols)
    }

    /// La carte graphique lue (après `Demande::Gpu`)
    pub fn gpu_lu(&mut self, k: usize, smi: Option<&str>, lieu: &dyn Lieu) -> Vec<Demande> {
        let t = self.info(k);
        let Some(Ctx::Gpu(g)) = self.tuiles.get(k).map(|t| t.ctx.clone()) else { return Vec::new() };
        let live = self.direct.clone();
        self.banc.gpu_lu(k, &t, &g, smi, lieu.smi(), &live)
    }

    /// Le disque d'une tuile (pour relire sa santé)
    pub fn disque(&self, k: usize) -> Option<CtxDisque> {
        match self.tuiles.get(k).map(|t| &t.ctx) {
            Some(Ctx::Disque(c)) => Some(c.clone()),
            _ => None,
        }
    }

    /// Un tour de la minuterie pendant un test
    pub fn tic(&mut self, e: &Echantillon) {
        let base = self.base_mhz();
        self.banc.tic(e, base);
    }

    /// La fin du travail d'un test
    pub fn fin(&mut self, r: Resultat) -> Vec<Demande> {
        let base = self.base_mhz();
        self.banc.fin(r, base)
    }

    /// Les mesures en direct (signal « mesures » du bloc perf) : `$Live`
    pub fn mesures(&mut self, live: &Value) {
        let opt = |k: &str| live.get(k).filter(|v| !v.is_null()).map(net::nombre_de);
        self.direct.ram_totale = opt("RamTotal");
        self.direct.ram_utilisee = opt("RamUsed").unwrap_or(0.0);
        self.direct.gpu = opt("Gpu").unwrap_or(0.0);
        self.direct.gpu_temp = opt("GpuTemp");
        self.direct.gpu_conso = opt("GpuPower").unwrap_or(0.0);
        self.direct.cpu_perf = opt("CpuPerf");
        self.direct.mesure += 1;
    }

    /// Le bouton « Tester ma connexion » (signal `test-debit` de la page Connexion)
    pub fn bouton_debit(&mut self) -> Option<String> {
        self.page();
        let k = self
            .boutons
            .iter()
            .position(|(t, b)| *b == Bouton::Reseau && self.tuiles.get(*t).is_some_and(|x| x.tag == "NET"))?;
        Some(format!("test-b:{k}"))
    }
}

// ---------------------------------------------------------------------------
// Le lien avec le bloc : les fils de travail, les effets
// ---------------------------------------------------------------------------

use nevermind_noyau::{Contexte, Effet, Effets};
use std::time::{Duration, Instant};

impl Tests {
    /// Ce que le PC donne au banc : le vrai, ou rien en mode test
    fn lieu(ctx: &Contexte) -> Box<dyn Lieu> {
        if ctx.essai { Box::new(crate::tests::LieuEssai) } else { Box::new(crate::tests::LieuWindows) }
    }

    /// Une action de l'écran pour les tests (`test-b:N`, `panneau:*`) : `None` si elle n'est pas pour eux ; sinon
    /// les gestes que le bloc fait avec le PC (programme à lancer, rapport de batterie)
    pub fn action(&mut self, ctx: &Contexte, id: &str, effets: &mut Effets) -> Option<Vec<Geste>> {
        let lieu = Self::lieu(ctx);
        let gestes = if id.starts_with("test-b:") {
            self.clic(id, lieu.as_ref())?
        } else if id.starts_with("panneau:") && self.panneau_ouvert() {
            let arret = id == "panneau:stop" && self.banc.occupe();
            let g = self.panneau_action(id, lieu.as_ref());
            if arret && let Some((m, _)) = &self.atelier.mesure {
                m.annuler.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            g
        } else {
            return None;
        };
        effets.ajouter(Effet::Rafraichir);
        Some(self.executer(ctx, gestes, effets))
    }

    /// Le signal `test-debit` (le bouton « Mesurer mon débit » de la page Connexion) : le test de la connexion
    pub fn test_debit(&mut self, ctx: &Contexte, effets: &mut Effets) {
        if let Some(id) = self.bouton_debit() {
            let _ = self.action(ctx, &id, effets);
        }
    }

    /// Les demandes du banc : les fils de travail, les messages, les signaux
    fn executer(&mut self, ctx: &Contexte, gestes: Vec<Geste>, effets: &mut Effets) -> Vec<Geste> {
        let mut reste = Vec::new();
        for g in gestes {
            match g {
                Geste::Demande(d) => self.demande(ctx, d, effets),
                Geste::Ouvrir(a) => effets.ajouter(Effet::OuvrirAdresse(a)),
                Geste::Pixels(i) => {
                    #[cfg(windows)]
                    if !ctx.essai
                        && let Some(e) = self.ecrans.get(i)
                    {
                        crate::tests::windows::pixels(e.split('|').next().unwrap_or("").to_string());
                    }
                    let _ = i;
                }
                g => reste.push(g),
            }
        }
        reste
    }

    fn demande(&mut self, ctx: &Contexte, d: Demande, effets: &mut Effets) {
        let signal = |nom: &str, donnees: Value| Effet::Signal { nom: nom.into(), donnees };
        match d {
            Demande::Mesure(travail, arg) => {
                if ctx.essai {
                    // (mode test : rien n'est mesuré sur le vrai PC)
                    let r = self.fin(Resultat::Erreur("mode test : rien n'est mesuré".into()));
                    for d in r {
                        self.demande(ctx, d, effets);
                    }
                    return;
                }
                self.atelier.mesure = Some(crate::tests::lancer(travail, &arg));
                self.atelier.redessine = Some(Instant::now());
            }
            Demande::Sante(k, id) => {
                if ctx.essai {
                    return;
                }
                #[cfg(windows)]
                {
                    self.atelier.sante = Some((k, std::thread::spawn(move || crate::tests::windows::disque(&id))));
                }
                #[cfg(not(windows))]
                let _ = (k, id);
            }
            Demande::Gpu(k) => {
                let smi = !ctx.essai && Self::lieu(ctx).smi();
                #[cfg(windows)]
                {
                    self.atelier.gpu =
                        Some((k, std::thread::spawn(move || if smi { crate::tests::windows::smi() } else { None })));
                }
                #[cfg(not(windows))]
                let _ = (k, smi);
            }
            Demande::EffacerAncien(f) => {
                if !ctx.essai && std::path::Path::new(&f).exists() {
                    let _ = std::fs::remove_file(&f);
                }
            }
            Demande::Message(m) => effets.ajouter(Effet::Annonce(m)),
            Demande::Occupe(t) => effets.ajouter(signal("occupe", json!({ "texte": t }))),
            Demande::Debit(r) => effets.ajouter(signal("debit-mesure", json!({ "R": r }))),
        }
    }

    /// Le tour du bloc : la progression du test (toutes les 100 ms), les lectures finies, la carte graphique
    pub fn tic_bloc(&mut self, ctx: &Contexte, effets: &mut Effets) -> Duration {
        let lieu = Self::lieu(ctx);
        let mut dessiner = false;
        if let Some((m, h)) = &self.atelier.mesure {
            let (phase, progres, direct) = m.lire();
            let fini = h.is_finished();
            let e = Echantillon { phase, progres, direct, perf: self.direct.cpu_perf, mesure: self.direct.mesure };
            self.tic(&e);
            if fini {
                let (_, h) = self.atelier.mesure.take().unwrap();
                let r = h.join().unwrap_or(Resultat::Erreur("le test s'est arrêté brusquement".into()));
                if let Resultat::Erreur(x) = &r {
                    ctx.journal(&format!("Test: {x}"));
                }
                for d in self.fin(r) {
                    self.demande(ctx, d, effets);
                }
                dessiner = true;
            } else if self.atelier.redessine.is_none_or(|t| t.elapsed() >= Duration::from_millis(300)) {
                self.atelier.redessine = Some(Instant::now());
                dessiner = true;
            }
        }
        if self.atelier.sante.as_ref().is_some_and(|(_, h)| h.is_finished()) {
            let (k, h) = self.atelier.sante.take().unwrap();
            let d = h.join().ok().flatten();
            // (les compteurs de fiabilité : ceux que l'Aide a donnés à l'analyse, Get-AideFiabiliteDisque)
            let id = self.disque(k).map(|c| c.id);
            let rel = champ_liste(&self.donnees, "Disks")
                .into_iter()
                .find(|x| x.get("Disk").map(|d| champ_texte(d, "DeviceId")) == id)
                .and_then(|x| x.get("Rel").cloned())
                .filter(|r| !r.is_null());
            for d in self.sante_lue(k, d.as_ref(), None, rel.as_ref(), lieu.as_ref()) {
                self.demande(ctx, d, effets);
            }
            dessiner = true;
        }
        if self.atelier.gpu.as_ref().is_some_and(|(_, h)| h.is_finished()) {
            let (k, h) = self.atelier.gpu.take().unwrap();
            let smi = h.join().ok().flatten();
            for d in self.gpu_lu(k, smi.as_deref(), lieu.as_ref()) {
                self.demande(ctx, d, effets);
            }
            dessiner = true;
        }
        if self.banc.suivi_gpu() {
            let live = self.direct.clone();
            dessiner |= self.banc.tic_gpu(&live);
        }
        if dessiner {
            effets.ajouter(Effet::Rafraichir);
        }
        if self.atelier.mesure.is_some() || self.atelier.sante.is_some() || self.atelier.gpu.is_some() {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(500)
        }
    }

    /// Nevermind se ferme : le test en cours est arrêté (son fichier d'essai est effacé par le fil lui-même)
    pub fn arreter_tout(&mut self) {
        if let Some((m, _)) = &self.atelier.mesure {
            m.annuler.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}
