//! La page « Tests des composants » (tests.ps1 `Build-TestsTab`, lue par ecran-divers.ps1 `Get-TuilesTestsKit`) :
//! une tuile par composant, avec ses boutons. Les tests eux-mêmes (vitesse des disques, processeur, mémoire, carte
//! graphique en direct, connexion, pixels morts) ne sont pas encore portés : voir DIVERGENCES.md. Le test complet de
//! la mémoire (mdsched.exe) et le rapport de batterie sont lancés comme avant.

use crate::net::{self, champ_liste, champ_nombre, champ_texte, taille};
use regex::Regex;
use serde_json::{Value, json};

#[derive(Debug, Clone)]
struct Tuile {
    tag: &'static str,
    titre: String,
    sous: String,
    desc: &'static str,
    boutons: Vec<(String, &'static str)>,
}

/// Ce que la page sait du PC (les données de l'analyse, les écrans, portable ou pas)
#[derive(Debug, Clone, Default)]
pub struct Tests {
    pub donnees: Value,
    pub ecrans: Vec<String>,
    pub portable: bool,
    boutons: Vec<&'static str>,
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
    fn tuiles(&self) -> Vec<Tuile> {
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
                b.push(("Tester la vitesse".to_string(), "disque-vitesse"));
            }
            b.push(("Santé".into(), "disque-sante"));
            if !lettres.is_empty() {
                b.push(("Erreurs".into(), "disque-erreurs"));
            }
            l.push(Tuile {
                tag: match sorte {
                    "HDD" => "HDD",
                    "USB" => "USB",
                    _ => "SSD",
                },
                titre: champ_texte(&k, "FriendlyName").trim().into(),
                sous,
                desc: "Vitesse réelle et santé du disque (usure, température, erreurs).",
                boutons: b,
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
            boutons: vec![("Test rapide (30 s)".into(), "cpu"), ("Stabilité (5 min)".into(), "cpu")],
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
            boutons: vec![("Tester la mémoire".into(), "memoire"), ("Test complet Windows".into(), "mdsched")],
        });
        let virt = Regex::new(r"(?i)Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta|Microsoft Basic").unwrap();
        for g in champ_liste(d, "GPUs").into_iter().filter(|g| !virt.is_match(&champ_texte(g, "Name"))) {
            l.push(Tuile {
                tag: "GPU",
                titre: champ_texte(&g, "Name"),
                sous: "Carte graphique".into(),
                desc: "Température, utilisation et consommation en direct, et ralentissements éventuels.",
                boutons: vec![("Surveiller en direct".into(), "gpu")],
            });
        }
        l.push(Tuile { tag: "NET", titre: "Connexion Internet".into(), sous: "Ping, téléchargement, envoi".into(),
            desc: "Réactivité et vitesse de ta connexion en 20 secondes, et « lag en charge » : ton ping monte-t-il quand quelqu'un télécharge ?", boutons: vec![("Tester ma connexion".into(), "reseau"), ("Lag en charge".into(), "reseau")] });
        let n = self.ecrans.len();
        let mut b = Vec::new();
        for (i, e) in self.ecrans.iter().enumerate() {
            let principal = e.split('|').nth(6) == Some("1");
            b.push((format!("Écran {}{}", i + 1, if principal && n > 1 { " (principal)" } else { "" }), "pixels"));
        }
        l.push(Tuile {
            tag: "HZ",
            titre: "Écrans".into(),
            sous: format!("{n} écran{}", if n > 1 { "s" } else { "" }),
            desc: "Couleurs unies en plein écran pour repérer les pixels morts. Échap pour quitter.",
            boutons: b,
        });
        if self.portable && !champ_liste(d, "Battery").is_empty() {
            l.push(Tuile {
                tag: "BAT",
                titre: "Batterie".into(),
                sous: "Rapport de Windows".into(),
                desc: "Capacité d'origine, capacité actuelle et autonomie estimée.",
                boutons: vec![("Voir le rapport".into(), "batterie")],
            });
        }
        l
    }

    /// `$pages.tests` (ecran-divers.ps1)
    pub fn page(&mut self) -> Value {
        self.boutons.clear();
        let slash = Regex::new(r"\s+/\s+").unwrap();
        let mut blocs = Vec::new();
        let tuiles = if self.donnees.is_null() { Vec::new() } else { self.tuiles() };
        for t in &tuiles {
            let note = [slash.replace_all(&t.sous, ", disque ").to_string(), t.desc.to_string()]
                .into_iter()
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            let mut bs = Vec::new();
            for (k, (texte, sorte)) in t.boutons.iter().enumerate() {
                self.boutons.push(sorte);
                bs.push(json!({ "id": format!("test-b:{}", self.boutons.len() - 1), "texte": texte, "style": if k > 0 { Some("second") } else { None } }));
            }
            blocs.push(json!({ "type": "ligne", "icone": icone(t.tag), "couleur": "P", "texte": t.titre, "note": note, "etiquette": "Pas encore testé", "ton": "", "boutons": bs }));
        }
        let blocs = if blocs.is_empty() {
            vec![json!({ "type": "texte", "texte": "Lecture de tes composants..." })]
        } else {
            std::iter::once(json!({ "type": "titre", "texte": "Tes composants" })).chain(blocs).collect()
        };
        json!({ "titre": "Tests des composants", "parent": "pc", "sous": "Vérifie que chaque pièce de ton PC fonctionne bien et à la bonne vitesse.", "icone": "test", "couleur": "S",
            "module": { "icone": "test", "couleur": "S", "titre": "Aucun test pour le moment", "texte": "Choisis un composant : chaque test dure de quelques secondes à quelques minutes. Ferme tes jeux avant." },
            "blocs": blocs })
    }

    /// Ce que fait un bouton d'une tuile (`test-b:N`)
    pub fn sorte(&self, id: &str) -> Option<&'static str> {
        id.strip_prefix("test-b:").and_then(|n| n.parse::<usize>().ok()).and_then(|k| self.boutons.get(k).copied())
    }
}
