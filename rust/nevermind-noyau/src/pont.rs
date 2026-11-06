//! Ce que NevermindPont.dll donnait à la version PowerShell (modules\pont.ps1) : en Rust, les règles de l'Aide
//! (aide-regles, LE même code que NevermindAide.exe) sont appelées directement, sans DLL. Il n'y a donc plus de cas
//! « DLL absente » : la règle est toujours là.
//!
//! - [`refus_fermeture_liste`] / [`refus_fermeture`] : la règle « fermable » (bulle RAM, mode jeu, « Fermer ») ;
//! - [`suffixe_canal`] : le suffixe du nom du canal de l'Aide pour un dossier ;
//! - [`programmes_commandes`] : le programme de chaque commande écrite par Windows ;
//! - [`dossiers_nettoyage`] : les dossiers de Windows que l'Aide vide (montrés dans la page Nettoyage) ;
//! - [`decider_fermeture`] : la décision de `Stop-ProcessusRegle` (fermer ici, laisser à l'Aide, ou refuser).

pub use aide_regles::fermeture::{Contexte as ContexteFermeture, Fiche, Parente, Reference};
use aide_regles::{canal, dossiers::Dossiers, fermeture, nettoyage};
use std::collections::BTreeMap;

/// La règle « fermable » sur plusieurs processus : une raison par fiche, dans le même ordre (`None` : fermable)
pub fn refus_fermeture_liste(ctx: &ContexteFermeture, fiches: &[Fiche], eleve_permis: bool) -> Vec<Option<String>> {
    fiches.iter().map(|f| fermeture::refus(ctx, f, eleve_permis).map(str::to_string)).collect()
}

/// La même règle pour un seul processus
pub fn refus_fermeture(ctx: &ContexteFermeture, fiche: &Fiche, eleve_permis: bool) -> Option<String> {
    fermeture::refus(ctx, fiche, eleve_permis).map(str::to_string)
}

/// `Get-SuffixeCanal` : 12 caractères, `None` si le dossier n'est pas un chemin « X:\ »
pub fn suffixe_canal(dossier: &str) -> Option<String> {
    canal::suffixe(dossier)
}

/// Les dossiers de Windows (lus par Windows, jamais dans les variables de l'utilisateur)
pub fn dossiers_windows() -> Dossiers {
    Dossiers {
        windows: dossier_connu("windows"),
        programmes: dossier_connu("programmes"),
        programmes_x86: dossier_connu("programmesX86"),
        donnees_communes: dossier_connu("donneesCommunes"),
    }
}

fn dossier_connu(quoi: &str) -> String {
    use windows::Win32::UI::Shell::{
        FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86, FOLDERID_Windows,
    };
    let id = match quoi {
        "windows" => FOLDERID_Windows,
        "programmes" => FOLDERID_ProgramFiles,
        "programmesX86" => FOLDERID_ProgramFilesX86,
        _ => FOLDERID_ProgramData,
    };
    crate::windows::dossier_connu(&id)
}

/// `Get-ProgrammesCommandes` : le chemin complet du programme de chaque commande, ou `None`
pub fn programmes_commandes(commandes: &[String]) -> Vec<Option<String>> {
    let d = dossiers_windows();
    commandes.iter().map(|c| d.programme_de_commande(c)).collect()
}

/// `Get-DossiersNettoyage` : les dossiers des catégories « wu », « do » et « dump » du nettoyage
pub fn dossiers_nettoyage() -> BTreeMap<String, Vec<String>> {
    let windows = dossier_connu("windows");
    let mut r = BTreeMap::new();
    for id in ["wu", "do", "dump"] {
        let l = nettoyage::categorie(id).map(|c| nettoyage::dossiers(c, &windows)).unwrap_or_default();
        r.insert(id.to_string(), l);
    }
    r
}

/// Ce que `Stop-ProcessusRegle` décide pour des processus déjà choisis
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DecisionFermeture {
    /// Les index des fiches que Nevermind ferme lui-même
    pub fermer: Vec<usize>,
    /// Combien sont laissés à l'Aide (lancés en administrateur, chemin illisible)
    pub aide: usize,
    /// Les raisons des refus
    pub refus: Vec<String>,
}

/// La décision de `Stop-ProcessusRegle` : `chemin` (non vide) limite à ce programme-là (un chemin illisible part à
/// l'Aide) ; puis la règle commune, et pour ceux lancés en administrateur, la règle de l'Aide (`elevePermis`).
/// L'appelant ferme ceux de `fermer` ; un échec de fermeture compte aussi pour l'Aide.
pub fn decider_fermeture(ctx: &ContexteFermeture, fiches: &[Fiche], chemin: &str) -> DecisionFermeture {
    let mut d = DecisionFermeture::default();
    let mut cibles = Vec::new();
    for (i, f) in fiches.iter().enumerate() {
        if !chemin.is_empty() && f.chemin != chemin {
            if f.chemin.is_empty() {
                d.aide += 1;
            }
            continue;
        }
        cibles.push(i);
    }
    for i in cibles {
        let f = &fiches[i];
        match fermeture::refus(ctx, f, false) {
            None => d.fermer.push(i),
            Some(r) => {
                if f.eleve.unwrap_or(0) != 0 && fermeture::refus(ctx, f, true).is_none() {
                    d.aide += 1;
                } else {
                    d.refus.push(r.to_string());
                }
            }
        }
    }
    d
}

/// Le message de `Stop-ProgrammeNom` quand rien n'a pu être fermé (`None` : rien à dire)
pub fn message_refus(nom: &str, d: &DecisionFermeture, fermes: usize) -> Option<String> {
    if fermes == 0 && d.aide == 0 && !d.refus.is_empty() {
        Some(format!("Nevermind ne ferme pas {nom} (raison : {}).", d.refus[0]))
    } else {
        None
    }
}

/// `Stop-ProgrammeNom` refuse toujours la suite : Mindset, Nevermind ou NevermindAide (avec ou sans « .exe »)
pub fn nom_de_la_suite(nom: &str) -> bool {
    let n = nom.trim().to_lowercase();
    let n = n.strip_suffix(".exe").unwrap_or(&n);
    matches!(n, "mindset" | "nevermind" | "nevermindaide")
}
