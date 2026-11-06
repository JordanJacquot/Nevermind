//! Lecture réelle du PC (lecture seule) pour la comparer avec la version PowerShell : écrit les données et le constat
//! en JSON, avec le temps et le processeur pris par la lecture.
//!
//!   cargo run --offline -p nevermind-memoire --example lecture-reelle -- <fichier.json> [répétitions]
//!
//! Rien n'est changé sur le PC : les poignées gardées (comme la page RAM) sont rendues à la fin.

fn main() {
    #[cfg(windows)]
    {
        use nevermind_memoire::pc::Pc;
        use nevermind_memoire::{constat, page, windows};
        use serde_json::json;
        let a: Vec<String> = std::env::args().collect();
        let sortie = a.get(1).cloned().unwrap_or_else(|| "lecture-reelle.json".into());
        let n: usize = a.get(2).and_then(|x| x.parse().ok()).unwrap_or(1);
        let mut temps = Vec::new();
        let mut l = None;
        for _ in 0..n.max(1) {
            let cpu0 = cpu();
            let t0 = std::time::Instant::now();
            l = Some(windows::lire(&[], &Default::default()));
            temps.push(json!({ "ms": t0.elapsed().as_secs_f64() * 1000.0, "cpuMs": (cpu() - cpu0) * 1000.0 }));
        }
        let l = l.unwrap();
        let d = l.donnees.unwrap_or_default();
        let c = constat::constat(&d);
        // (troisième argument « pilotes » : la recherche des pilotes des étiquettes, mesurée)
        let pilotes = if a.get(3).map(String::as_str) == Some("pilotes") {
            let tags: Vec<String> = d.etiquettes.iter().map(|e| e.tag.clone()).collect();
            let t0 = std::time::Instant::now();
            let p = windows::PcWindows.chercher_pilotes(&tags);
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            json!({ "ms": ms, "trouves": p.iter().map(|(k, v)| (k.clone(), json!(v.iter().map(|x| format!("{} ({}, {})", x.fichier, x.description, x.societe)).collect::<Vec<_>>()))).collect::<serde_json::Map<_, _>>() })
        } else {
            serde_json::Value::Null
        };
        let cle: Vec<_> = c
            .applis
            .iter()
            .map(|g| {
                json!({ "Cle": g.cle, "Nom": g.nom, "Kind": g.kind, "Octets": g.octets, "Nb": g.nb, "Exes": g.exes,
                "Services": g.services, "Fermable": g.fermable, "Refus": g.refus })
            })
            .collect();
        let o = json!({ "temps": temps, "pilotes": pilotes, "journal": l.journal, "donnees": d.en_json(), "applis": cle,
            "pourcent": c.pourcent, "corrections": c.corrections.iter().map(|x| x.id.clone()).collect::<Vec<_>>(),
            "page": page::page(Some(&c), "", false) });
        std::fs::write(&sortie, serde_json::to_string_pretty(&o).unwrap()).unwrap();
        windows::relacher();
        println!(
            "{} processus, {} services, {} applis : {}",
            d.processus.len(),
            d.services.len(),
            c.applis.len(),
            sortie
        );
    }
}

#[cfg(windows)]
fn cpu() -> f64 {
    nevermind_memoire::windows::temps_processeur()
}
