//! Les relevés du Trafic (Update-Traffic) et les pages de l'écran (Get-PagesJeux : page Trafic, onglet Lag en
//! ligne), rejoués sur les cas exportés de la version PowerShell.

mod commun;

use commun::{difference, juge, verifier};
use nevermind_trafic::pages;
use nevermind_trafic::trafic::alertes;
use nevermind_trafic::trafic::etat::{Processus, Releve, Service};
use nevermind_trafic::valeur::{champ, liste, num, texte};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// Un faux Windows : les emplacements et les services du cas, aucun processus vivant
struct Faux {
    /// Le numéro de PowerShell pendant l'export : son vrai nom (les autres numéros n'existent pas)
    moi: u32,
    chemins: HashMap<u32, String>,
    services: HashMap<u32, Vec<Service>>,
}

impl Processus for Faux {
    fn noms(&self) -> HashMap<u32, String> {
        HashMap::from([(self.moi, "powershell".to_string())])
    }
    fn chemin(&self, pid: u32) -> String {
        self.chemins.get(&pid).cloned().unwrap_or_default()
    }
    fn services(&self, pid: u32) -> Vec<Service> {
        self.services.get(&pid).cloned().unwrap_or_default()
    }
}

/// Rejoue un cas : les passages de 2 s, puis la page
fn rejouer(c: &Value) -> (Releve, Vec<alertes::Alerte>, f64) {
    let mut f = Faux { moi: num(champ(c, "moi")) as u32, chemins: HashMap::new(), services: HashMap::new() };
    for x in liste(champ(c, "chemins")) {
        f.chemins.insert(num(champ(&x, "Pid")) as u32, texte(champ(&x, "Chemin")));
    }
    for x in liste(champ(c, "services")) {
        let l = liste(champ(&x, "Services"))
            .iter()
            .map(|s| Service { nom: texte(champ(s, "Name")), titre: texte(champ(s, "Title")) })
            .collect();
        f.services.insert(num(champ(&x, "Pid")) as u32, l);
    }
    let sigs = champ(c, "signatures").as_object().cloned().unwrap_or_default();
    let dns = champ(c, "dns").as_object().cloned().unwrap_or_default();
    let udp: Vec<String> = liste(champ(c, "udp")).iter().map(texte).collect();
    let minutes = num(champ(c, "minutes"));
    let mut r = Releve::nouveau(num(champ(c, "moi")) as u32, 0.0);
    let mut t = minutes * 60.0 - 2.0 * liste(champ(c, "passages")).len() as f64;
    r.debut = 0.0;
    let mut prevenus = HashSet::new();
    let mut al = Vec::new();
    for p in liste(champ(c, "passages")) {
        t += 2.0;
        r.passages += 1;
        if r.passages.is_multiple_of(15) {
            r.oublier_morts(&HashSet::new());
        }
        for (k, v) in &dns {
            r.dns.insert(k.clone(), texte(v));
        }
        let lignes: Vec<String> = liste(&p).iter().map(texte).collect();
        r.passage(&lignes, &f, t);
        if r.passages % 5 == 1 {
            r.udp(&udp, &f);
        }
        // (signatures : comme le faux de l'export, celles connues tout de suite)
        while let Some(k) = r.a_verifier.pop_front() {
            if let Some(s) = sigs.get(&k)
                && let Some(a) = r.apps.get_mut(&k)
            {
                a.sig = Some(texte(&s[0]));
                a.editeur = texte(&s[1]);
            }
        }
        al = alertes::alertes(r.apps.values(), &[], &[], &[]);
        for a in &al {
            if a.niveau != "info" {
                prevenus.insert(a.cle.clone());
            }
        }
    }
    r.prevenus = prevenus;
    (r, al, minutes * 60.0 + 0.5)
}

#[test]
fn releves_et_page_trafic() {
    let cas = juge("releves.json");
    let cas = cas.as_array().expect("liste");
    let mut diffs = Vec::new();
    for (i, c) in cas.iter().enumerate() {
        let (r, al, maintenant) = rejouer(c);
        // Les programmes et leurs destinations
        let mut attendues = liste(champ(c, "apps"));
        attendues.sort_by_key(|a| texte(champ(a, "Key")));
        let mut obtenues: Vec<Value> = r
            .apps
            .values()
            .map(|a| {
                let mut d: Vec<Value> = a.dest.values().map(|d| json!({ "Remote": d.distante, "Port": d.port, "Out": d.out, "In": d.entrant, "Live": d.vivante, "Private": d.privee })).collect();
                d.sort_by_key(|x| format!("{}|{}", texte(champ(x, "Remote")), texte(champ(x, "Port"))));
                json!({ "Key": a.cle, "Name": a.nom, "Title": a.titre, "Path": a.chemin, "Out": a.out, "In": a.entrant, "Rate": a.debit, "Live": a.vivantes, "Udp": a.udp, "Sig": a.sig, "Dest": d })
            })
            .collect();
        obtenues.sort_by_key(|a| texte(champ(a, "Key")));
        for a in attendues.iter_mut() {
            let mut d = liste(champ(a, "Dest"));
            d.sort_by_key(|x| format!("{}|{}", texte(champ(x, "Remote")), texte(champ(x, "Port"))));
            a["Dest"] = json!(d);
        }
        for a in attendues.iter_mut() {
            // (un programme vu seulement en UDP n'a jamais eu de « Live » en PowerShell : 0 ici, jamais montré)
            if a["Live"].is_null() {
                a["Live"] = json!(0);
            }
        }
        if let Some(x) = difference(&json!(obtenues), &json!(attendues), "apps") {
            diffs.push(format!("cas {i} : {x}"));
            continue;
        }
        let actif = champ(c, "actif").as_bool().unwrap_or(false);
        // À gravité égale, PowerShell range les alertes dans l'ordre de sa table (au hasard des clés) : on prend son
        // ordre ; et les ports d'une alerte dans l'ordre des destinations (idem) : comparés triés (DIVERGENCES.md)
        let attendue = champ(c, "page").clone();
        let ordre: Vec<String> = liste(champ(&attendue, "blocs"))
            .iter()
            .filter(|b| texte(champ(b, "icone")) == "alerte")
            .map(|b| texte(champ(b, "texte")))
            .collect();
        let mut al = al;
        al.sort_by_key(|a| {
            let t = r.apps.get(&a.cle).map(|x| x.titre.clone()).unwrap_or_default();
            ordre.iter().position(|o| o.starts_with(&format!("{t} : "))).unwrap_or(usize::MAX)
        });
        let page =
            pages::page_trafic(Some(&r), actif, &al, champ(c, "annuaire").as_bool().unwrap_or(false), maintenant);
        if let Some(x) = difference(&ports_tries(&page), &ports_tries(&attendue), "page") {
            diffs.push(format!("cas {i} : {x}"));
        }
        let kit: Vec<String> = pages::apps_montrees(&r).iter().map(|a| a.cle.clone()).collect();
        if let Some(x) = difference(&json!(kit), champ(c, "kitApps"), "kitApps") {
            diffs.push(format!("cas {i} : {x}"));
        }
        let bulles = liste(champ(c, "bulles")).len();
        if bulles != r.prevenus.len() {
            diffs.push(format!("cas {i} : {} bulles au lieu de {bulles}", r.prevenus.len()));
        }
    }
    verifier("Relevés et page Trafic", diffs, cas.len());
}

#[test]
fn onglet_lag_et_page_trafic_vide() {
    let cas = juge("onglet-lag.json");
    let cas = cas.as_array().expect("liste");
    let mut diffs = Vec::new();
    for (i, c) in cas.iter().enumerate() {
        let sessions = liste(champ(c, "sessions"));
        let o = pages::onglet_lag(
            &sessions,
            champ(c, "mesure").as_bool().unwrap_or(false),
            champ(c, "enCours").as_bool().unwrap_or(false),
        );
        if let Some(x) = difference(champ(&o, "module"), champ(c, "module"), "module") {
            diffs.push(format!("cas {i} : {x}"));
        }
        if let Some(x) = difference(champ(&o, "blocs"), champ(c, "blocs"), "blocs") {
            diffs.push(format!("cas {i} : {x}"));
        }
        let t = pages::page_trafic(None, false, &[], false, 0.0);
        // (l'export garde le texte du bouton laissé par le cas d'avant : « Reprendre » ; jamais démarrée, c'est
        // « Démarrer la surveillance »)
        let mut attendu = champ(c, "trafic").clone();
        attendu["module"]["boutons"][0]["texte"] = json!("Démarrer la surveillance");
        if let Some(x) = difference(&t, &attendu, "trafic") {
            diffs.push(format!("cas {i} : {x}"));
        }
    }
    verifier("Onglet Lag en ligne", diffs, cas.len());
}

/// Les ports d'une alerte « il utilise un port inhabituel (a, b) », triés
fn ports_tries(v: &Value) -> Value {
    match v {
        Value::String(s) if s.contains("il utilise un port inhabituel (") => {
            let (avant, reste) = s.split_once("il utilise un port inhabituel (").unwrap_or((s, ""));
            let (ports, apres) = reste.split_once(')').unwrap_or((reste, ""));
            let mut l: Vec<&str> = ports.split(", ").collect();
            l.sort_unstable();
            json!(format!("{avant}il utilise un port inhabituel ({}){apres}", l.join(", ")))
        }
        Value::Array(a) => Value::Array(a.iter().map(ports_tries).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), ports_tries(x))).collect()),
        x => x.clone(),
    }
}
