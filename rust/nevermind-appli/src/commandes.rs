//! Les commandes que la page de Nevermind peut appeler : les trois seules, chacune déclarée dans le manifeste de
//! build.rs et autorisée à la fenêtre « principale » seulement (capabilities\principale.json). Elles ne lisent ni
//! n'écrivent rien sur le disque et ne lancent rien : elles passent l'état à la page (le kit), notent ce qu'elle voit,
//! et donnent ses messages vérifiés au receveur de l'appli. Reprises de Mindcare en Rust (mindcare-appli).

use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::Value;
use tauri::State;
use tauri::ipc::Channel;

use crate::pont::{self, MessagePage};

/// Le journal de l'appli, vu d'ici (une ligne de texte)
type Journal = Box<dyn Fn(&str) + Send + Sync>;

/// Ce que l'appli fait d'un message de la page (appelé sur le fil de la fenêtre, pendant la commande `action`).
pub type Receveur = Arc<dyn Fn(MessagePage) + Send + Sync>;

/// L'état partagé entre l'appli et sa page : le dernier état envoyé (`$script:Vue.Etat`), le canal ouvert par la page
/// (un seul : une page rechargée reprend sa place), ce que la page a dit de sa visibilité, et le receveur.
pub struct Page {
    etat: Mutex<Value>,
    canal: Mutex<Option<Channel<Value>>>,
    visible: Mutex<Option<bool>>,
    receveur: Mutex<Option<Receveur>>,
    journal: Journal,
}

fn tenir<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Page {
    pub fn new(etat: Value, journal: impl Fn(&str) + Send + Sync + 'static) -> Page {
        Page {
            etat: Mutex::new(etat),
            canal: Mutex::new(None),
            visible: Mutex::new(None),
            receveur: Mutex::new(None),
            journal: Box::new(journal),
        }
    }

    /// Le dernier état (ce que la page reçoit en arrivant).
    pub fn etat(&self) -> Value {
        tenir(&self.etat).clone()
    }

    fn envoyer(&self, message: Value) {
        if let Some(canal) = tenir(&self.canal).as_ref()
            && let Err(e) = canal.send(message)
        {
            (self.journal)(&format!("Écran : envoi impossible ({e})"));
        }
    }

    /// Un nouvel état : gardé, et envoyé à la page si elle suit déjà (`Send-Vue`).
    pub fn changer(&self, etat: Value) {
        *tenir(&self.etat) = etat.clone();
        self.envoyer(etat);
    }

    /// Un ordre au kit, sans changer l'état. Autre chose qu'un ordre n'est pas envoyé.
    pub fn ordre(&self, ordre: Value) {
        if ordre.get("kit").is_some_and(Value::is_string) {
            self.envoyer(ordre);
        }
    }

    /// Le receveur des messages de la page ; sans lui, chaque message est noté au journal.
    pub fn recevoir(&self, receveur: impl Fn(MessagePage) + Send + Sync + 'static) {
        *tenir(&self.receveur) = Some(Arc::new(receveur));
    }

    /// Ce que la page a dit en dernier : visible, cachée, ou rien encore.
    pub fn visible(&self) -> Option<bool> {
        *tenir(&self.visible)
    }
}

/// La page commence à suivre l'état de l'appli : il lui est envoyé tout de suite, puis à chaque changement.
#[tauri::command]
pub fn suivre_page(canal: Channel<Value>, page: State<'_, Page>) -> Result<(), String> {
    canal.send(page.etat()).map_err(|e| format!("état non envoyé ({e})"))?;
    *tenir(&page.canal) = Some(canal);
    Ok(())
}

/// La page est devenue visible ou cachée (document.visibilityState), noté par la page elle-même : rien d'autre (la
/// veille est décidée par l'appli, qui l'écrit au journal).
#[tauri::command]
pub fn noter_visibilite(visible: bool, page: State<'_, Page>) {
    *tenir(&page.visible) = Some(visible);
}

/// Un message du kit : vérifié (taille, type, champs), puis donné au receveur de l'appli. Un message refusé est noté
/// au journal, et la page reçoit seulement « refusé ».
#[tauri::command]
pub fn action(message: Value, page: State<'_, Page>) -> Result<(), String> {
    let m = match pont::lire_message(&message) {
        Ok(m) => m,
        Err(r) => {
            (page.journal)(&format!("Écran : action refusée ({})", r.raison()));
            return Err("refusé".into());
        }
    };
    let receveur = tenir(&page.receveur).clone();
    match receveur {
        Some(r) => r(m),
        None => (page.journal)(&pont::ligne_journal(&m)),
    }
    Ok(())
}
