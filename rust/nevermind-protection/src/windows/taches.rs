//! Les tâches planifiées (Get-ScheduledTask, Disable-ScheduledTask), par l'interface COM du Planificateur de tâches.
//! Sans droits, seules les tâches que l'utilisateur peut lire sont vues, comme dans la version PowerShell.

use super::com::creer;
use super::message;
use windows::Win32::Foundation::VARIANT_BOOL;
use windows::Win32::System::TaskScheduler::{
    IExecAction, ITaskFolder, ITaskService, TASK_ENUM_HIDDEN, TASK_STATE_DISABLED, TaskScheduler,
};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::core::{BSTR, Interface};

/// Une tâche vue : nom, dossier (« \ » final compris), désactivée, et programme et arguments de chaque action
pub struct TacheVue {
    pub nom: String,
    pub dossier: String,
    pub desactivee: bool,
    pub actions: Vec<(String, String)>,
}

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

fn dossier(f: &ITaskFolder, r: &mut Vec<TacheVue>, profondeur: u32) {
    // SAFETY : appels COM ordinaires ; une erreur saute la tâche ou le dossier
    unsafe {
        let chemin = f.Path().map(|b| b.to_string()).unwrap_or_default();
        let dossier_tache = if chemin == "\\" { chemin.clone() } else { format!("{chemin}\\") };
        if let Ok(taches) = f.GetTasks(TASK_ENUM_HIDDEN.0) {
            let n = taches.Count().unwrap_or(0);
            for i in 1..=n {
                let Ok(t) = taches.get_Item(&index(i)) else { continue };
                let Ok(def) = t.Definition() else { continue };
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

fn service() -> Result<ITaskService, String> {
    let s: ITaskService = creer(&TaskScheduler)?;
    let vide = VARIANT::default();
    // SAFETY : appel COM ordinaire
    unsafe { s.Connect(&vide, &vide, &vide, &vide) }.map_err(|e| message(&e))?;
    Ok(s)
}

/// Toutes les tâches que l'utilisateur peut voir
pub fn liste() -> Result<Vec<TacheVue>, String> {
    let s = service()?;
    let mut r = Vec::new();
    // SAFETY : appel COM ordinaire
    let racine = unsafe { s.GetFolder(&BSTR::from("\\")) }.map_err(|e| message(&e))?;
    dossier(&racine, &mut r, 0);
    Ok(r)
}

/// Désactive une tâche puis la relit : `Ok(true)` si elle est bien désactivée
pub fn desactiver(nom: &str, chemin: &str) -> Result<bool, String> {
    let s = service()?;
    let d = chemin.trim_end_matches('\\');
    let d = if d.is_empty() { "\\" } else { d };
    // SAFETY : appels COM ordinaires
    unsafe {
        let f = s.GetFolder(&BSTR::from(d)).map_err(|e| message(&e))?;
        let t = f.GetTask(&BSTR::from(nom)).map_err(|e| message(&e))?;
        t.SetEnabled(VARIANT_BOOL(0)).map_err(|e| message(&e))?;
        let t = f.GetTask(&BSTR::from(nom)).map_err(|e| message(&e))?;
        Ok(t.State().is_ok_and(|e| e == TASK_STATE_DISABLED))
    }
}
