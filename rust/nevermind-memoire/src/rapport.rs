//! Le rapport (`Format-RapportMemoire`, `Save-RapportMemoire`) : un fichier texte sur le Bureau, à envoyer à
//! quelqu'un qui aide à distance.

use crate::constat::{Constat, part_cachee};
use crate::texte::taille;
use std::path::{Path, PathBuf};

const MO: f64 = 1024.0 * 1024.0;

/// `'{0,10}' -f $x` : aligné à droite sur 10 caractères
fn d10(t: &str) -> String {
    let n = t.encode_utf16().count();
    if n >= 10 { t.to_string() } else { format!("{}{t}", " ".repeat(10 - n)) }
}

/// Le texte du rapport (lignes séparées par « \r\n »)
pub fn texte(c: &Constat) -> String {
    let mut l: Vec<String> = Vec::new();
    l.push("Rapport mémoire de Nevermind".into());
    l.push(format!("PC : {}    Date : {}", c.pc, c.date.jour_et_heure()));
    l.push(String::new());
    l.push("EN BREF".into());
    l.push(format!(
        "  Mémoire installée : {}{}",
        taille(c.installee),
        if c.installee_lue && c.reservee >= MO {
            format!(" (Windows en voit {})", taille(c.visible))
        } else {
            String::new()
        }
    ));
    l.push(format!("  Utilisée : {} ({} %)", taille(c.utilisee), c.pourcent));
    l.push(format!("  En cache : {} (ne compte pas : Windows la rend dès qu'une appli en a besoin)", taille(c.cache)));
    l.push(format!("  Libre tout de suite : {}", taille(c.disponible)));
    l.push(String::new());
    if !c.alertes.is_empty() {
        l.push("A REGARDER".into());
        for a in &c.alertes {
            l.push(format!("  * {}", a.titre));
            l.push(format!("    {}", a.texte));
            l.push(format!("    Que faire : {}", a.quoi));
        }
        l.push(String::new());
    }
    l.push("PAR APPLI (du plus gros au plus petit)".into());
    for g in &c.applis {
        if g.octets < MO {
            continue;
        }
        l.push(format!(
            "  {}  {}{}  [{}]",
            d10(&taille(g.octets)),
            g.nom,
            if g.nb > 1 { format!(" ({} processus)", g.nb) } else { String::new() },
            g.etiquette
        ));
        let programmes = if g.exes.is_empty() {
            String::new()
        } else {
            format!(
                " Programmes : {}{}.",
                g.exes.iter().take(6).cloned().collect::<Vec<_>>().join(", "),
                if g.exes.len() > 6 { "..." } else { "" }
            )
        };
        l.push(format!("              {}{programmes}", g.note));
    }
    l.push(String::new());
    l.push("LA PART CACHEE".into());
    for x in part_cachee(c) {
        l.push(format!("  {}  {}", d10(&taille(x.octets)), x.nom));
        l.push(format!("              {}", x.note));
    }
    if !c.etiquettes.is_empty() {
        l.push(String::new());
        l.push("ETIQUETTES DE LA MEMOIRE DES PILOTES (les plus grosses)".into());
        for e in &c.etiquettes {
            let p: Vec<String> = e
                .pilotes
                .iter()
                .map(|p| {
                    if p.description.is_empty() {
                        p.fichier.clone()
                    } else {
                        format!(
                            "{} ({}{})",
                            p.fichier,
                            p.description,
                            if p.societe.is_empty() { String::new() } else { format!(", {}", p.societe) }
                        )
                    }
                })
                .collect();
            l.push(format!(
                "  {}  {}  {}",
                d10(&taille(e.octets)),
                e.tag,
                if p.is_empty() {
                    "pilote pas retrouvé".to_string()
                } else {
                    format!("pilote probable : {}", p.join(" ou "))
                }
            ));
        }
    }
    l.push(String::new());
    l.push("CE QUE NEVERMIND PEUT COUPER SANS RISQUE".into());
    if c.corrections.is_empty() {
        l.push("  Rien : ce qui tourne est utile, ou se coupe à la main (voir chaque appli).".into());
    } else {
        for x in &c.corrections {
            l.push(format!("  * {}", x.nom));
        }
    }
    l.push(String::new());
    l.push("Rien n'a été changé sur le PC pour faire ce rapport.".into());
    l.join("\r\n")
}

/// Le nom du fichier du rapport
pub fn nom_fichier(c: &Constat) -> String {
    format!("Nevermind, rapport mémoire du {} à {}.txt", c.date.jour_tirets(), c.date.heure_h())
}

/// Pose le rapport dans un dossier (UTF-8 avec BOM, pour le Bloc-notes) ; rend le chemin du fichier
pub fn poser(c: &Constat, dossier: &Path) -> std::io::Result<PathBuf> {
    let f = dossier.join(nom_fichier(c));
    let mut o = vec![0xEF, 0xBB, 0xBF];
    o.extend_from_slice(texte(c).as_bytes());
    std::fs::write(&f, o)?;
    Ok(f)
}
