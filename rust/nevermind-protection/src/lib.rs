//! Nevermind en Rust : le bloc « protection ». Vide pour l'instant (base posée par l'intégration).

use nevermind_noyau::{Bloc, Contexte, Effets};
use serde_json::{Map, Value};

#[derive(Default)]
pub struct BlocProtection {}

impl Bloc for BlocProtection {
    fn nom(&self) -> &'static str {
        "protection"
    }
    fn pages(&mut self, _ctx: &Contexte, _ecran: &str) -> Map<String, Value> {
        Map::new()
    }
    fn action(&mut self, _ctx: &Contexte, _id: &str, _valeur: &Value, _effets: &mut Effets) -> bool {
        false
    }
}
