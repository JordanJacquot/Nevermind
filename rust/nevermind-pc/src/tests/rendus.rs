//! Le résultat de chaque test (les `Render` de tests.ps1) : les éléments du panneau, le statut (ok, warn, bad, info)
//! et les petites cases chiffrées de la tuile. Les mêmes calculs, les mêmes seuils et les mêmes textes.

use super::kit::{self, BAD, BLANC, El, Fmt, INFO, OK, VIOLET, WARN, jauge, section, verdict};
use crate::net::{entier, n_fr, taille, texte_nombre};
use serde_json::Value;

/// Ce que rend un test : éléments, statut, cases de la tuile
pub type Rendu = (Vec<El>, String, Vec<(String, String)>);

fn r(v: &[f64], i: usize) -> f64 {
    v.get(i).copied().unwrap_or(0.0)
}

/// `Get-LoadColor` (analyse.ps1)
pub fn couleur_charge(pct: f64, warn: f64, bad: f64) -> &'static str {
    if pct >= bad {
        BAD
    } else if pct >= warn {
        WARN
    } else {
        OK
    }
}

/// Le type d'un disque (`$kind` de Build-TestsTab) : NVMe, SSD, HDD, USB
pub fn echelle_disque(sorte: &str) -> (f64, f64) {
    match sorte {
        "NVMe" => (7000.0, 1200.0),
        "SSD" => (600.0, 350.0),
        "HDD" => (250.0, 80.0),
        _ => (1000.0, 0.0),
    }
}

/// Vitesse d'un disque (Test-DiskSpeed)
pub fn disque(v: &[f64], sorte: &str) -> Rendu {
    let (echelle, min) = echelle_disque(sorte);
    let mut e = vec![
        section("RÉSULTAT"),
        El::Jauges(vec![
            jauge("Lecture", r(v, 1), Fmt::N0, "Mo/s", OK),
            jauge("Écriture", r(v, 0), Fmt::N0, "Mo/s", INFO),
            jauge("Petits fichiers", r(v, 3), Fmt::N0, "par seconde", VIOLET),
        ]),
        section("COMPARAISON (LECTURE)"),
        El::Barres(
            vec![
                ("Ton disque".into(), r(v, 1)),
                ("Disque dur".into(), 150.0),
                ("SSD classique".into(), 550.0),
                ("SSD NVMe récent".into(), 5000.0),
            ],
            "Mo/s".into(),
        ),
    ];
    let _ = echelle;
    let (statut, txt) = if min != 0.0 && r(v, 1) < min {
        (
            "warn",
            "Plus lent que la normale pour ce type de disque : disque presque plein, qui chauffe, ou SSD branché sur un port lent.",
        )
    } else {
        ("ok", "Vitesse normale pour ce type de disque.")
    };
    e.push(verdict(statut, txt));
    let chips = vec![
        ("Lecture".into(), format!("{} Mo/s", n_fr(r(v, 1), 0))),
        ("Écriture".into(), format!("{} Mo/s", n_fr(r(v, 0), 0))),
    ];
    (e, statut.into(), chips)
}

/// Recherche d'erreurs (Test-DiskErrors) : une ligne « lettre|résultat » par lecteur
pub fn erreurs(lignes: &[String]) -> Rendu {
    let mut e = vec![section("RÉSULTAT")];
    let mut mauvais = false;
    let mut tuiles = Vec::new();
    for ligne in lignes {
        let (l, v) = ligne.split_once('|').unwrap_or((ligne.as_str(), ""));
        let ok = v.eq_ignore_ascii_case("NoErrorsFound");
        let err = v.len() >= 3 && v[..3].eq_ignore_ascii_case("ERR");
        if !ok && !err {
            mauvais = true;
        }
        let libelle = if ok {
            "aucune erreur"
        } else if err {
            "vérification impossible"
        } else {
            "erreurs trouvées"
        };
        tuiles.push((
            if ok { 0.0 } else { 1.0 },
            Fmt::Fixe(if ok { "✓" } else { "!" }),
            (if ok { OK } else { WARN }).to_string(),
            format!("Lecteur {l}: {libelle}"),
        ));
    }
    e.push(El::Tuiles(tuiles));
    if mauvais {
        e.push(verdict(
            "warn",
            "Windows a trouvé des erreurs dans le système de fichiers. Redémarre le PC : Windows les répare souvent tout seul.",
        ));
        (e, "warn".into(), vec![("Erreurs".into(), "Trouvées".into())])
    } else {
        e.push(verdict("ok", "Aucune erreur trouvée sur ce disque."));
        (e, "ok".into(), vec![("Erreurs".into(), "Aucune".into())])
    }
}

/// `Get-GHz` : la fréquence (moyenne ou maximale) des relevés d'une étape
pub fn ghz(releves: Option<&Vec<f64>>, base_mhz: f64, max: bool) -> Option<f64> {
    let s = releves.filter(|s| !s.is_empty())?;
    if base_mhz == 0.0 {
        return None;
    }
    let v =
        if max { s.iter().copied().fold(f64::NEG_INFINITY, f64::max) } else { s.iter().sum::<f64>() / s.len() as f64 };
    Some(base_mhz * v / 100.0 / 1000.0)
}

/// Processeur (Test-Cpu) : `multi` = durée demandée sur tous les cœurs (22 ou 295 s)
pub fn cpu(v: &[f64], max_single: Option<f64>, moy_multi: Option<f64>, base_mhz: f64, multi: f64) -> Rendu {
    let base2 = base_mhz / 1000.0;
    let top = 6f64.max((max_single.unwrap_or(0.0) + 0.5).ceil());
    let mut e = vec![
        section("RÉSULTAT"),
        El::Tuiles(vec![
            (r(v, 0), Fmt::N0, BLANC.into(), "points sur un cœur".into()),
            (r(v, 1), Fmt::N0, BLANC.into(), format!("points sur les {} cœurs", entier(r(v, 3)))),
            (r(v, 2), Fmt::N0, (if r(v, 2) != 0.0 { BAD } else { OK }).into(), "erreurs de calcul".into()),
        ]),
    ];
    let _ = top;
    let mut g = Vec::new();
    let ms = max_single.filter(|x| *x != 0.0);
    let am = moy_multi.filter(|x| *x != 0.0);
    if let Some(m) = ms {
        g.push(jauge("Fréquence max", m, Fmt::N1, "GHz", INFO));
    }
    if let Some(a) = am {
        g.push(jauge("En pleine charge", a, Fmt::N1, "GHz", if a < base2 * 0.95 { WARN } else { OK }));
    }
    g.push(jauge("Tous les cœurs vs un seul", r(v, 1) / 1f64.max(r(v, 0)), Fmt::N1, "fois plus", VIOLET));
    e.push(El::Jauges(g));
    let (statut, txt) = if r(v, 2) > 0.0 {
        (
            "bad",
            "Le processeur a fait des erreurs de calcul : il est instable (overclock ou undervolt trop poussé, XMP instable, surchauffe). Remets les réglages du BIOS par défaut et refais le test.",
        )
    } else if am.is_some_and(|a| a < base2 * 0.95) {
        (
            "warn",
            "Sous forte charge, le processeur passe sous sa fréquence de base : il chauffe trop ou manque d'alimentation. Vérifie le ventirad et la pâte thermique.",
        )
    } else if multi >= 120.0 {
        ("ok", "Aucune erreur pendant 5 minutes à pleine charge : ton processeur est stable.")
    } else {
        ("ok", "Tout est normal : aucune erreur et le processeur garde bien sa vitesse.")
    };
    e.push(verdict(statut, txt));
    let chips = vec![
        ("1 cœur".into(), format!("{} pts", n_fr(r(v, 0), 0))),
        ("Tous les cœurs".into(), format!("{} pts", n_fr(r(v, 1), 0))),
        ("En charge".into(), am.map_or("?".into(), |a| format!("{} GHz", n_fr(a, 1)))),
    ];
    (e, statut.into(), chips)
}

/// Mémoire vive (Test-Memory)
pub fn memoire(v: &[f64]) -> Rendu {
    let mut e = vec![
        section("RÉSULTAT"),
        El::Jauges(vec![
            jauge("Lecture", r(v, 1), Fmt::N1, "Go/s", OK),
            jauge("Écriture", r(v, 0), Fmt::N1, "Go/s", INFO),
            jauge("Copie", r(v, 2), Fmt::N1, "Go/s", VIOLET),
        ]),
        El::Tuiles(vec![
            (r(v, 4), Fmt::N1, BLANC.into(), "Go vérifiés".into()),
            (r(v, 3), Fmt::N0, (if r(v, 3) != 0.0 { BAD } else { OK }).into(), "erreurs trouvées".into()),
        ]),
    ];
    let (statut, txt) = if r(v, 3) > 0.0 {
        (
            "bad",
            "Des erreurs ont été trouvées. Désactive le profil XMP / EXPO dans le BIOS et refais le test. Si ça continue, une barrette est défectueuse : lance le test complet de Windows.",
        )
    } else {
        (
            "ok",
            "Aucune erreur. Ce test rapide ne vérifie qu'une partie de la mémoire : en cas de plantages, lance le test complet de Windows.",
        )
    };
    e.push(verdict(statut, txt));
    let chips = vec![("Lecture".into(), format!("{} Go/s", n_fr(r(v, 1), 1))), ("Erreurs".into(), n_fr(r(v, 3), 0))];
    (e, statut.into(), chips)
}

/// Connexion Internet (Test-NetSpeed) : ping moyen, téléchargement, envoi, pings perdus
pub fn reseau(v: &[f64]) -> Rendu {
    let (p, d, u) = (r(v, 0), r(v, 1), r(v, 2));
    let echelle = if d.max(u) > 1000.0 { 2500.0 } else { 1000.0 };
    let _ = echelle;
    let mut e = vec![section("RÉSULTAT")];
    let mut g = Vec::new();
    if d >= 0.0 {
        g.push(jauge("Téléchargement", d, Fmt::N0, "Mb/s", OK));
    }
    if u >= 0.0 {
        g.push(jauge("Envoi", u, Fmt::N0, "Mb/s", INFO));
    }
    if p >= 0.0 {
        g.push(jauge("Ping", p, Fmt::N0, "ms", if p > 60.0 { WARN } else { OK }));
    }
    e.push(El::Jauges(g));
    if d > 0.0 {
        let min = 50.0 * 8000.0 / d / 60.0;
        e.push(El::Tuiles(vec![(min, Fmt::N0, BLANC.into(), "minutes pour télécharger un jeu de 50 Go".into())]));
    }
    let (statut, txt) = if d < 0.0 || u < 0.0 {
        (
            "info",
            "Les serveurs de test n'ont pas répondu (trop de tests d'affilée ou pas de connexion). Réessaie dans quelques minutes.",
        )
    } else if d < 10.0 {
        ("warn", "Connexion lente : les téléchargements seront longs. Pour jouer, c'est surtout le ping qui compte.")
    } else if p > 60.0 {
        ("warn", "Le débit est correct mais le ping est élevé : en Wi-Fi, rapproche-toi de la box ou branche un câble.")
    } else {
        ("ok", "Bonne connexion pour jouer et télécharger.")
    };
    e.push(verdict(statut, txt));
    let chips = vec![
        ("Téléchargement".into(), if d >= 0.0 { format!("{} Mb/s", n_fr(d, 0)) } else { "?".into() }),
        ("Ping".into(), if p >= 0.0 { format!("{} ms", n_fr(p, 0)) } else { "?".into() }),
    ];
    (e, statut.into(), chips)
}

/// `Get-BloatGrade` : la note du lag en charge d'après les millisecondes en plus
pub fn note_lag(extra: f64) -> (&'static str, &'static str) {
    if extra < 5.0 {
        ("A+", "ok")
    } else if extra < 30.0 {
        ("A", "ok")
    } else if extra < 60.0 {
        ("B", "ok")
    } else if extra < 200.0 {
        ("C", "warn")
    } else if extra < 400.0 {
        ("D", "bad")
    } else {
        ("F", "bad")
    }
}

/// Lag en charge (Test-Bufferbloat)
pub fn lag(v: &[f64]) -> Rendu {
    let mut e = vec![section("RÉSULTAT")];
    if r(v, 0) < 0.0 {
        e.push(verdict(
            "info",
            "Le ping ne répond pas (pas de connexion, ou pare-feu qui bloque le ping). Réessaie plus tard.",
        ));
        return (e, "info".into(), vec![("Note".into(), "?".into())]);
    }
    let charge = r(v, 1).max(r(v, 2));
    let extra = 0f64.max(charge - r(v, 0));
    let (note, statut) = note_lag(extra);
    let c = kit::couleur_statut(statut).to_string();
    let couleur = |x: f64| if x - r(v, 0) >= 60.0 { WARN } else { OK };
    e.push(El::Rangee(vec![
        El::Encadre(vec![(note.into(), c), ("Note".into(), kit::DOUX.into())]),
        El::Jauges(vec![
            jauge("Au repos", r(v, 0), Fmt::N0, "ms", OK),
            jauge("Téléchargement", r(v, 1), Fmt::N0, "ms", couleur(r(v, 1))),
            jauge("Envoi", r(v, 2), Fmt::N0, "ms", couleur(r(v, 2))),
        ]),
    ]));
    e.push(El::Tuiles(vec![
        (extra, Fmt::N0, BLANC.into(), "ms de plus quand la connexion est chargée".into()),
        (0f64.max(r(v, 6)), Fmt::N0, BLANC.into(), "Mb/s en téléchargement pendant le test".into()),
    ]));
    let ms = entier(extra);
    let txt = match note {
        "A+" => {
            "Parfait : ton ping ne bouge pas, même quand quelqu'un télécharge ou regarde une vidéo. Idéal pour jouer."
                .to_string()
        }
        "A" => format!(
            "Très bien : ton ping ne monte que de {ms} ms quand la connexion est pleine. Pas de lag à craindre."
        ),
        "B" => format!(
            "Correct : ton ping monte de {ms} ms quand la connexion est pleine. Un petit lag est possible si quelqu'un télécharge pendant que tu joues."
        ),
        _ => format!(
            "Ton ping monte de {ms} ms dès que quelqu'un télécharge ou regarde une vidéo : c'est ce qui fait laguer en jeu. Solutions : active la « QoS » ou la priorité jeux dans ta box (ou un routeur avec SQM), limite la vitesse de téléchargement de Steam pendant que tu joues, et préfère le câble au Wi-Fi."
        ),
    };
    e.push(verdict(statut, &txt));
    if r(v, 4) >= 10.0 || r(v, 5) >= 10.0 {
        e.push(verdict(
            "warn",
            &format!(
                "Des pings se perdent quand la connexion est chargée ({} %) : en jeu, ça se traduit par des téléportations.",
                entier(r(v, 4).max(r(v, 5)))
            ),
        ));
    }
    e.push(El::Details(
        vec![
            ("Ping au repos".into(), format!("{} ms (gigue {} ms)", n_fr(r(v, 0), 0), n_fr(r(v, 8), 1))),
            (
                "Ping pendant un téléchargement".into(),
                format!("{} ms, {} % de pertes", n_fr(r(v, 1), 0), n_fr(r(v, 4), 0)),
            ),
            ("Ping pendant un envoi".into(), format!("{} ms, {} % de pertes", n_fr(r(v, 2), 0), n_fr(r(v, 5), 0))),
            (
                "Débit pendant le test".into(),
                format!(
                    "{} Mb/s en téléchargement, {} Mb/s en envoi",
                    n_fr(0f64.max(r(v, 6)), 0),
                    n_fr(0f64.max(r(v, 7)), 0)
                ),
            ),
            ("Serveur de ping".into(), "1.1.1.1 (Cloudflare)".into()),
        ],
        false,
    ));
    let chips = vec![("Note".into(), note.into()), ("En charge".into(), format!("+{} ms", n_fr(extra, 0)))];
    (e, statut.into(), chips)
}

// ---------------------------------------------------------------------------
// Santé d'un disque (Show-DiskHealth)
// ---------------------------------------------------------------------------

/// Un nombre d'un objet JSON, `None` s'il est absent ou nul (`$null -ne $rel.X`)
fn champ(o: &Value, k: &str) -> Option<f64> {
    match o.get(k)? {
        Value::Null => None,
        v => Some(crate::net::nombre_de(v)),
    }
}

fn texte_champ(o: &Value, k: &str) -> String {
    crate::net::champ_texte(o, k)
}

/// Le disque de la tuile (`$Ctx`) : identifiant, sorte (NVMe, SSD, HDD, USB), libellé, lettres
#[derive(Debug, Clone, Default)]
pub struct CtxDisque {
    pub id: String,
    pub nom: String,
    pub sorte: String,
    pub libelle: String,
    pub lettres: Vec<String>,
}

/// La santé : `d` = le disque relu (Get-PhysicalDisk), `rel` = ses compteurs de fiabilité (ou rien),
/// `vols` = (taille, libre) de chaque lettre (`Get-VolInfo`), ou rien si le lecteur n'est pas prêt
pub fn sante(
    c: &CtxDisque,
    d: &Value,
    rel: Option<&Value>,
    vols: &[Option<(f64, f64)>],
) -> (Vec<El>, String, Vec<(String, String)>, String) {
    let mut statut = "ok";
    let mut notes: Vec<String> = Vec::new();
    let etat = texte_champ(d, "HealthStatus");
    let etat_l = etat.to_ascii_lowercase();
    match etat_l.as_str() {
        "warning" => {
            statut = "warn";
            notes.push("Le disque signale lui-même un problème.".into());
        }
        "unhealthy" => {
            statut = "bad";
            notes.push("Le disque annonce une panne proche.".into());
        }
        _ => {}
    }
    let pire = |s: &mut &str| {
        if *s == "ok" {
            *s = "warn";
        }
    };
    let mut gauges = Vec::new();
    let usure = rel.and_then(|r| champ(r, "Wear"));
    if let (Some(w), true) = (usure, c.sorte != "HDD") {
        let vie = 100 - entier(w);
        let vf = vie as f64;
        gauges.push(jauge(
            "Durée de vie restante",
            vf,
            Fmt::N0,
            "%",
            if vie <= 10 {
                BAD
            } else if vie <= 30 {
                WARN
            } else {
                OK
            },
        ));
        if vie <= 10 {
            statut = "bad";
            notes.push(format!("Il reste environ {vie} % de durée de vie."));
        } else if vie <= 30 {
            pire(&mut statut);
            notes.push(format!("Il reste environ {vie} % de durée de vie."));
        }
    } else {
        let sh = match etat_l.as_str() {
            "healthy" => 100.0,
            "warning" => 50.0,
            "unhealthy" => 10.0,
            _ => 0.0,
        };
        gauges.push(jauge(
            "État SMART",
            sh,
            Fmt::N0,
            "%",
            if sh >= 100.0 {
                OK
            } else if sh >= 50.0 {
                WARN
            } else {
                BAD
            },
        ));
    }
    let temp = rel.and_then(|r| champ(r, "Temperature")).filter(|t| *t > 0.0);
    if let Some(t) = temp {
        let wt = if c.sorte == "HDD" { 50.0 } else { 70.0 };
        gauges.push(jauge("Température", t, Fmt::N0, "°C", couleur_charge(t, wt, wt + 10.0)));
        if t >= wt {
            pire(&mut statut);
            notes.push("Le disque chauffe : vérifie la ventilation.".into());
        }
    }
    let rempli = vols.iter().flatten().find(|(t, _)| *t != 0.0).map(|(t, l)| 100.0 * (t - l) / t);
    if let Some(p) = rempli {
        gauges.push(jauge("Rempli", p, Fmt::N0, "%", couleur_charge(p, 80.0, 90.0)));
        if p >= 90.0 {
            pire(&mut statut);
            notes.push("Le disque est presque plein.".into());
        }
    }
    let mut e = vec![section("SANTÉ DU DISQUE"), El::Jauges(gauges)];
    let mut tuiles = Vec::new();
    if let Some(h) = rel.and_then(|r| champ(r, "PowerOnHours")).filter(|h| *h > 0.0) {
        tuiles.push((h, Fmt::N0, BLANC.to_string(), "heures allumé au total".to_string()));
    }
    if let Some(re) = rel.and_then(|r| champ(r, "ReadErrorsUncorrected")) {
        let errs = re + rel.and_then(|r| champ(r, "WriteErrorsUncorrected")).unwrap_or(0.0);
        tuiles.push((errs, Fmt::N0, (if errs != 0.0 { WARN } else { OK }).into(), "erreurs non réparées".into()));
        if errs != 0.0 {
            pire(&mut statut);
            notes.push(format!("{} erreur(s) de lecture ou d'écriture.", texte_nombre(errs)));
        }
    }
    if let Some(s) = rel.and_then(|r| champ(r, "StartStopCycleCount")).filter(|s| *s > 0.0) {
        tuiles.push((s, Fmt::N0, BLANC.to_string(), "démarrages".to_string()));
    }
    if !tuiles.is_empty() {
        e.push(El::Tuiles(tuiles));
    }
    let serie = texte_champ(d, "SerialNumber");
    let mut rangs: Vec<(String, String)> = vec![
        ("Modèle".into(), texte_champ(d, "FriendlyName").trim().into()),
        ("Numéro de série".into(), serie.trim().trim_end_matches('.').into()),
        ("Version du micrologiciel".into(), texte_champ(d, "FirmwareVersion")),
        ("Type".into(), format!("{}, branché en {}", c.libelle, texte_champ(d, "BusType"))),
        ("Capacité".into(), taille(crate::net::champ_nombre(d, "Size"))),
        (
            "État SMART".into(),
            match etat_l.as_str() {
                "healthy" => "Bon",
                "warning" => "Avertissement",
                "unhealthy" => "Défaillant",
                _ => "Inconnu",
            }
            .into(),
        ),
    ];
    if let Some(rl) = rel {
        if let Some(w) = champ(rl, "Wear") {
            rangs.push(("Usure".into(), format!("{} %", entier(w))));
        }
        if let Some(t) = champ(rl, "TemperatureMax").filter(|t| *t > 0.0) {
            rangs.push(("Température limite du fabricant".into(), format!("{} °C", entier(t))));
        }
        if let Some(x) = champ(rl, "ReadErrorsCorrected") {
            rangs.push(("Erreurs de lecture réparées".into(), n_fr(x, 0)));
        }
        if let Some(x) = champ(rl, "ReadErrorsUncorrected") {
            rangs.push(("Erreurs de lecture non réparées".into(), n_fr(x, 0)));
        }
        if let Some(x) = champ(rl, "WriteErrorsUncorrected") {
            rangs.push(("Erreurs d'écriture non réparées".into(), n_fr(x, 0)));
        }
        if let Some(x) = champ(rl, "ReadLatencyMax").filter(|x| *x > 0.0) {
            rangs.push(("Temps de réponse max en lecture".into(), format!("{} ms", texte_nombre(x))));
        }
        if let Some(x) = champ(rl, "WriteLatencyMax").filter(|x| *x > 0.0) {
            rangs.push(("Temps de réponse max en écriture".into(), format!("{} ms", texte_nombre(x))));
        }
    } else {
        rangs.push(("Détails SMART".into(), "Non fournis par ce disque".into()));
    }
    for (l, v) in c.lettres.iter().zip(vols) {
        if let Some((t, libre)) = v.filter(|(t, _)| *t != 0.0) {
            rangs.push((format!("Lecteur {l}:"), format!("{} libres sur {}", taille(libre), taille(t))));
        }
    }
    e.push(El::Details(rangs, false));
    let txt = match statut {
        "ok" => "Ce disque est en bonne santé.".to_string(),
        "warn" => format!("À surveiller : {} Pense à sauvegarder tes fichiers importants.", notes.join(" ")),
        _ => format!(
            "Attention : {} Sauvegarde tes fichiers maintenant et prévois de remplacer ce disque.",
            notes.join(" ")
        ),
    };
    e.push(verdict(statut, &txt));
    let etat_texte = match statut {
        "ok" => "Bonne santé",
        "warn" => "À surveiller",
        _ => "Problème détecté",
    };
    let mut chips = vec![(
        "Santé".to_string(),
        match statut {
            "ok" => "Bonne",
            "warn" => "À surveiller",
            _ => "Problème",
        }
        .to_string(),
    )];
    if let Some(t) = temp {
        chips.push(("Température".into(), format!("{} °C", entier(t))));
    }
    (e, statut.into(), chips, etat_texte.into())
}

// ---------------------------------------------------------------------------
// Carte graphique en direct (Show-GpuMonitor)
// ---------------------------------------------------------------------------

/// `$num` : un nombre de nvidia-smi (« [N/A] » et le reste donnent 0)
pub fn nombre_smi(s: &str) -> f64 {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.') { s.parse().unwrap_or(0.0) } else { 0.0 }
}

/// Les champs demandés à nvidia-smi
pub const CHAMPS_SMI: &str = "driver_version,vbios_version,pstate,clocks.gr,clocks.max.gr,clocks.mem,clocks.max.mem,temperature.gpu,fan.speed,power.draw,power.limit,pcie.link.gen.current,pcie.link.gen.max,pcie.link.width.current,pcie.link.width.max,utilization.gpu,memory.used,memory.total,clocks_throttle_reasons.active";

/// Le corps du panneau de la carte graphique (sans la courbe ni les jauges en direct, mises à part) :
/// (éléments avant la courbe : rien ; éléments après : détails et verdict), statut, et la consommation lue
pub fn gpu_details(gpu: &Value, info: Option<&[String]>) -> (Vec<El>, &'static str) {
    let t = |i: usize| info.and_then(|x| x.get(i)).cloned().unwrap_or_default();
    match info {
        Some(_) => {
            let bits = u64::from_str_radix(t(18).trim_start_matches("0x").trim_start_matches("0X"), 16).unwrap_or(0);
            let mut why = Vec::new();
            if bits & 0x4 != 0 {
                why.push("limite de consommation (normal en pleine charge)");
            }
            if bits & 0x20 != 0 {
                why.push("chauffe");
            }
            if bits & 0x40 != 0 {
                why.push("surchauffe");
            }
            if bits & 0x8 != 0 {
                why.push("ralentissement matériel");
            }
            if bits & 0x80 != 0 {
                why.push("alimentation insuffisante");
            }
            let rangs = vec![
                ("Fréquence du GPU".into(), format!("{} MHz (max {} MHz)", t(3), t(4))),
                ("Fréquence de la mémoire".into(), format!("{} MHz (max {} MHz)", t(5), t(6))),
                (
                    "Mémoire vidéo utilisée".into(),
                    format!(
                        "{} Go sur {} Go",
                        n_fr(nombre_smi(&t(16)) / 1024.0, 1),
                        n_fr(nombre_smi(&t(17)) / 1024.0, 0)
                    ),
                ),
                ("Ventilateurs".into(), format!("{} %", t(8))),
                ("Liaison PCIe".into(), format!("Gen {} x{}  (max Gen {} x{})", t(11), t(13), t(12), t(14))),
                ("Ralentissements".into(), if why.is_empty() { "Aucun".into() } else { why.join(", ") }),
                ("Pilote".into(), t(0)),
                ("BIOS de la carte".into(), t(1)),
            ];
            let mut e = vec![El::Details(rangs, false)];
            if bits & 0xE8 != 0 {
                e.push(verdict(
                    "warn",
                    "La carte ralentit à cause de la chaleur ou de l'alimentation : dépoussière le PC et vérifie la ventilation du boîtier.",
                ));
                (e, "warn")
            } else {
                e.push(verdict(
                    "ok",
                    "Aucun ralentissement. À vide, la carte baisse sa vitesse pour économiser l'énergie : c'est normal.",
                ));
                (e, "ok")
            }
        }
        None => {
            let rangs = vec![
                ("Modèle".into(), texte_champ(gpu, "Name")),
                ("Pilote".into(), texte_champ(gpu, "DriverVersion")),
                (
                    "Résolution".into(),
                    format!(
                        "{} x {}",
                        texte_champ(gpu, "CurrentHorizontalResolution"),
                        texte_champ(gpu, "CurrentVerticalResolution")
                    ),
                ),
            ];
            (
                vec![
                    El::Details(rangs, false),
                    verdict(
                        "info",
                        "Température et consommation ne sont lisibles que sur les cartes NVIDIA. Pour une carte AMD : AMD Software > Performances.",
                    ),
                ],
                "ok",
            )
        }
    }
}
