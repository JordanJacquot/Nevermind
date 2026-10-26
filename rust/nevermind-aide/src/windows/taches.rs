//! Tâches planifiées (Get-ScheduledTask), par l'interface COM du Planificateur de tâches. Cachées comprises.

use super::com::creer;
use super::outils::message;
use crate::systeme::TacheVue;
use windows::Win32::System::TaskScheduler::{
    IExecAction, ITaskFolder, ITaskService, TASK_ENUM_HIDDEN, TASK_STATE_DISABLED, TaskScheduler,
};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::core::{BSTR, Interface};

/// Un VARIANT nombre entier (index d'une collection)
fn index(i: i32) -> VARIANT {
    let mut v = VARIANT::default();
    // SAFETY : écriture du type et de la valeur correspondante
    unsafe {
        let a = &mut *v.Anonymous.Anonymous;
        a.vt = VT_I4;
        a.Anonymous.lVal = i;
    }
    v
}

/// Les tâches d'un dossier et de ses sous-dossiers
fn dossier(f: &ITaskFolder, r: &mut Vec<TacheVue>, profondeur: u32) {
    // SAFETY : appels COM ordinaires ; erreurs : la tâche ou le dossier est sauté
    unsafe {
        let chemin = f.Path().map(|b| b.to_string()).unwrap_or_default();
        let dossier_tache = if chemin == "\\" { chemin.clone() } else { format!("{chemin}\\") };
        if let Ok(taches) = f.GetTasks(TASK_ENUM_HIDDEN.0) {
            let n = taches.Count().unwrap_or(0);
            for i in 1..=n {
                let Ok(t) = taches.get_Item(&index(i)) else { continue };
                let Ok(def) = t.Definition() else { continue };
                let mut compte = BSTR::new();
                let mut groupe = BSTR::new();
                if let Ok(p) = def.Principal() {
                    let _ = p.UserId(&mut compte);
                    let _ = p.GroupId(&mut groupe);
                }
                let mut actions = Vec::new();
                if let Ok(a) = def.Actions() {
                    let mut na = 0i32;
                    let _ = a.Count(&mut na);
                    for j in 1..=na {
                        let Ok(x) = a.get_Item(j) else { continue };
                        let (mut exe, mut args) = (BSTR::new(), BSTR::new());
                        if let Ok(e) = x.cast::<IExecAction>() {
                            let _ = e.Path(&mut exe);
                            let _ = e.Arguments(&mut args);
                        }
                        actions.push((exe.to_string(), args.to_string()));
                    }
                }
                r.push(TacheVue {
                    nom: t.Name().map(|b| b.to_string()).unwrap_or_default(),
                    dossier: dossier_tache.clone(),
                    desactivee: t.State().is_ok_and(|e| e == TASK_STATE_DISABLED),
                    compte: compte.to_string(),
                    groupe: groupe.to_string(),
                    actions,
                });
            }
        }
        if profondeur > 16 {
            return;
        }
        if let Ok(sous) = f.GetFolders(0) {
            let n = sous.Count().unwrap_or(0);
            for i in 1..=n {
                if let Ok(s) = sous.get_Item(&index(i)) {
                    dossier(&s, r, profondeur + 1);
                }
            }
        }
    }
}

/// Toutes les tâches planifiées du PC
pub fn toutes() -> Result<Vec<TacheVue>, String> {
    let s: ITaskService = creer(&TaskScheduler)?;
    let vide = VARIANT::default();
    let mut r = Vec::new();
    // SAFETY : appels COM ordinaires
    unsafe {
        s.Connect(&vide, &vide, &vide, &vide).map_err(|e| message(&e))?;
        let racine = s.GetFolder(&BSTR::from("\\")).map_err(|e| message(&e))?;
        dossier(&racine, &mut r, 0);
    }
    Ok(r)
}
