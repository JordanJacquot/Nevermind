//! Les périphériques (casque, souris, clavier) de la page « Ton PC » : leur batterie, leur état de charge et un bouton
//! vers le logiciel de la marque. Nouveau : pas dans la version PowerShell (voir DIVERGENCES.md).
//!
//! La lecture elle-même (HID, seulement des questions, jamais un réglage) est dans le module Windows
//! (`windows::peripheriques`) et passe par [`crate::pc::Pc::peripheriques`] : les tests et les juges ont un faux PC
//! (aucun appareil par défaut, donc aucune section). Ici : le décodage des réponses (fonctions pures, testées avec les
//! octets lus sur un vrai PC) et les blocs des pages.

use serde_json::{Value, json};

/// L'identifiant de logiciel mis dans chaque question (G Hub prend 0x0D) : seules les réponses à nos questions sont
/// gardées.
pub const SWID: u8 = 0x0C;

/// La batterie est relue au plus une fois par minute, et seulement quand la page « Ton PC » (ou le détail d'un
/// appareil) est regardée.
pub const RELECTURE: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sorte {
    Casque,
    Souris,
    Clavier,
}

impl Sorte {
    /// L'icône du kit (window.kitIcones)
    pub fn icone(self) -> &'static str {
        match self {
            Sorte::Casque => "casque",
            Sorte::Souris => "souris",
            Sorte::Clavier => "clavier",
        }
    }
    pub fn genre(self) -> &'static str {
        match self {
            Sorte::Casque => "Casque",
            Sorte::Souris => "Souris",
            Sorte::Clavier => "Clavier",
        }
    }
    /// « Ton casque », « Ta souris »
    pub fn ton_nom(self) -> &'static str {
        match self {
            Sorte::Casque => "Ton casque",
            Sorte::Souris => "Ta souris",
            Sorte::Clavier => "Ton clavier",
        }
    }
    /// « le brancher », « la brancher »
    fn le(self) -> &'static str {
        if self == Sorte::Souris { "la" } else { "le" }
    }
    /// Ce que règle le logiciel de la marque
    fn reglages(self) -> &'static str {
        match self {
            Sorte::Casque => "Pour régler le son, le micro et les lumières",
            Sorte::Souris => "Pour régler la vitesse, les boutons et les lumières",
            Sorte::Clavier => "Pour régler les touches et les lumières",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Charge {
    /// Sur batterie (ou un état de charge inconnu)
    Batterie,
    EnCharge,
    /// Branché, charge terminée
    Pleine,
}

/// Le logiciel de la marque installé : son nom (« G Hub ») et son programme
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logiciel {
    pub nom: String,
    pub chemin: String,
}

/// Un appareil trouvé
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peripherique {
    pub sorte: Sorte,
    /// Le nom du produit (« PRO X 2 LIGHTSPEED »)
    pub nom: String,
    pub marque: String,
    /// Relié sans fil (récepteur) ; sinon par câble
    pub sans_fil: bool,
    /// La batterie en %, `None` : pas de batterie (filaire)
    pub batterie: Option<u8>,
    pub charge: Charge,
    /// Le logiciel de la marque, s'il est installé (rempli par la lecture, `Pc::logiciel_marque`)
    pub logiciel: Option<Logiciel>,
}

impl Peripherique {
    fn en_charge(&self) -> bool {
        self.batterie.is_some() && self.charge != Charge::Batterie
    }

    /// Le ton : vert en charge, rouge sous 20 %, orange sous 35 %
    pub fn ton(&self) -> &'static str {
        match self.batterie {
            None => "",
            Some(_) if self.en_charge() => "ok",
            Some(b) if b < 20 => "urgent",
            Some(b) if b < 35 => "attention",
            Some(_) => "",
        }
    }

    /// L'état en quelques mots
    pub fn etat(&self) -> String {
        match (self.batterie, self.charge) {
            (None, _) => "Branché par câble".into(),
            (Some(_), Charge::EnCharge) => "En charge".into(),
            (Some(_), Charge::Pleine) => "Chargé".into(),
            (Some(b), Charge::Batterie) if b < 20 => format!("Presque vide, pense à {} brancher", self.sorte.le()),
            (Some(_), Charge::Batterie) => "Sur batterie".into(),
        }
    }

    /// La page de détail (une par appareil : sa clé ne change pas d'une lecture à l'autre)
    pub fn page(&self) -> String {
        let mut s = String::from("perif:");
        s.push_str(self.sorte.icone());
        let mut tiret = true;
        for c in self.nom.chars().flat_map(char::to_lowercase) {
            if c.is_ascii_alphanumeric() {
                if tiret {
                    s.push('-');
                }
                s.push(c);
                tiret = false;
            } else {
                tiret = true;
            }
        }
        s
    }
}

/// L'action « Ouvrir » le logiciel de la marque
pub fn action_logiciel(marque: &str) -> String {
    format!("perif-logiciel:{marque}")
}

// ---------------------------------------------------------------------------------------------------------------
// Le décodage des réponses (HID++ 2.0 des souris et claviers Logitech, « Centurion » des casques PRO X 2)
// ---------------------------------------------------------------------------------------------------------------

/// HID++ long (0x11) : la question « où est la fonction `fonction` » à l'appareil `appareil` (1 à 6 sur un récepteur,
/// 0xFF branché directement)
pub fn hidpp_question_fonction(appareil: u8, fonction: u16) -> [u8; 6] {
    let [h, l] = fonction.to_be_bytes();
    [0x11, appareil, 0x00, SWID, h, l]
}

/// HID++ long : l'appel `numero` de la fonction rangée à `index`, avec ses paramètres
pub fn hidpp_appel(appareil: u8, index: u8, numero: u8, parametres: &[u8]) -> Vec<u8> {
    let mut q = vec![0x11, appareil, index, (numero << 4) | SWID];
    q.extend_from_slice(parametres);
    q
}

/// Ce que dit une trame reçue pour notre question (`appareil`, `index`, `numero`)
#[derive(Debug, PartialEq, Eq)]
pub enum Reponse<'a> {
    /// Pas pour nous (une autre question, celle de G Hub...) : on attend encore
    Autre,
    /// Refusée (erreur HID++, appareil absent ou éteint)
    Refus,
    /// Les données de la réponse
    Donnees(&'a [u8]),
}

/// Une trame HID++ reçue : la réponse à notre question, une erreur, ou autre chose
pub fn hidpp_reponse(r: &[u8], appareil: u8, index: u8, numero: u8) -> Reponse<'_> {
    if r.len() < 5 || r[1] != appareil {
        return Reponse::Autre;
    }
    // (erreur HID++ 2.0 : 11 dev FF index fonction code ; HID++ 1.0 : 10 dev 8F index fonction code)
    if (r[0] == 0x11 && r[2] == 0xFF || r[0] == 0x10 && r[2] == 0x8F) && r[3] == index && r[4] == (numero << 4) | SWID {
        return Reponse::Refus;
    }
    if r[0] == 0x11 && r[2] == index && r[3] == (numero << 4) | SWID {
        return Reponse::Donnees(&r[4..]);
    }
    Reponse::Autre
}

/// La batterie d'une réponse à « getStatus » (fonction 0x1004 UNIFIED_BATTERY, appel 1) : %, niveau, charge
pub fn hidpp_batterie(d: &[u8]) -> Option<(u8, Charge)> {
    let (&pourcent, &niveau, &charge) = (d.first()?, d.get(1)?, d.get(2)?);
    // (un appareil qui ne donne que des niveaux : 1 critique, 2 bas, 4 bon, 8 plein)
    let p = if pourcent > 0 {
        pourcent.min(100)
    } else if niveau & 8 != 0 {
        100
    } else if niveau & 4 != 0 {
        50
    } else if niveau & 2 != 0 {
        20
    } else if niveau & 1 != 0 {
        5
    } else {
        0
    };
    let c = match charge {
        1 | 2 => Charge::EnCharge,
        3 => Charge::Pleine,
        // (0 sur batterie ; autre : une erreur de charge, montrée comme sur batterie)
        _ => Charge::Batterie,
    };
    Some((p, c))
}

/// La sorte d'appareil d'après « getDeviceType » (fonction 0x0005, appel 2)
pub fn hidpp_sorte(d: &[u8]) -> Option<Sorte> {
    match *d.first()? {
        0 => Some(Sorte::Clavier),
        3..=5 => Some(Sorte::Souris),
        8 => Some(Sorte::Casque),
        _ => None,
    }
}

/// Une trame « Centurion » : 51, longueur, 00, puis les données
pub fn centurion_trame(donnees: &[u8]) -> Vec<u8> {
    let mut f = vec![0x51, (donnees.len() + 1) as u8, 0x00];
    f.extend_from_slice(donnees);
    f
}

/// La question au récepteur du casque : où est son pont vers le casque
pub fn centurion_question_pont() -> Vec<u8> {
    centurion_trame(&[0x00, SWID, 0x00, 0x03])
}

/// Une question passée au casque par le pont
pub fn centurion_par_pont(pont: u8, sous: &[u8]) -> Vec<u8> {
    let mut p = vec![pont, 0x10 | SWID, 0x00, sous.len() as u8];
    p.extend_from_slice(sous);
    centurion_trame(&p)
}

/// La réponse du récepteur sur le pont : `Donnees([pont])`
pub fn centurion_pont(r: &[u8]) -> Reponse<'_> {
    if r.len() < 6 || r[0] != 0x51 {
        return Reponse::Autre;
    }
    // (un récepteur qui ne connaît pas la question : 51 05 00 FF ...)
    if r[3] == 0xFF {
        return Reponse::Refus;
    }
    if r[3] == 0x00 && r[4] == SWID {
        return if r[5] == 0 { Reponse::Refus } else { Reponse::Donnees(&r[5..6]) };
    }
    Reponse::Autre
}

/// La réponse du casque par le pont à une question sur la fonction rangée à `index` (0 : la racine). L'accusé de
/// réception (51 03 00 pont 1C ...) qui arrive avant n'est pas la réponse.
pub fn centurion_reponse(r: &[u8], pont: u8, index: u8) -> Reponse<'_> {
    if r.len() <= 12 || r[0] != 0x51 || r[3] != pont || r[4] != 0x10 || r[7] != 0x00 {
        return Reponse::Autre;
    }
    if r[8] == 0xFF {
        return Reponse::Refus;
    }
    if r[8] == index { Reponse::Donnees(&r[8..]) } else { Reponse::Autre }
}

/// La question « où est la fonction BATTERY_SOC (0x0104) » du casque
pub fn centurion_question_batterie(pont: u8) -> Vec<u8> {
    centurion_par_pont(pont, &[0x00, 0x00, SWID, 0x01, 0x04])
}

/// L'index de BATTERY_SOC dans la réponse (`None` : fonction absente)
pub fn centurion_index_batterie(d: &[u8]) -> Option<u8> {
    d.get(2).copied().filter(|i| *i != 0)
}

/// La question de la batterie du casque
pub fn centurion_question_etat(pont: u8, index: u8) -> Vec<u8> {
    centurion_par_pont(pont, &[0x00, index, SWID])
}

/// La batterie du casque : % et charge (1 ou 2 en charge, 3 terminée, sinon sur batterie)
pub fn centurion_batterie(d: &[u8]) -> Option<(u8, Charge)> {
    let (&p, &e) = (d.get(2)?, d.get(4)?);
    let c = match e {
        1 | 2 => Charge::EnCharge,
        3 => Charge::Pleine,
        _ => Charge::Batterie,
    };
    Some((p.min(100), c))
}

/// Un récepteur Logitech (Unifying, Nano, LIGHTSPEED, Bolt) : les appareils sont derrière lui (1 à 6), pas à 0xFF
pub fn est_recepteur(pid: u16, nom: &str) -> bool {
    (0xC517..=0xC5FF).contains(&pid) || nom.to_lowercase().contains("receiver")
}

// ---------------------------------------------------------------------------------------------------------------
// Le logiciel de la marque
// ---------------------------------------------------------------------------------------------------------------

/// Les programmes connus du logiciel de chaque marque, sous Program Files (puis Program Files (x86))
fn programmes(marque: &str) -> (&'static str, &'static [&'static str]) {
    match marque {
        "Logitech" => ("G Hub", &["LGHUB\\lghub.exe"]),
        "Corsair" => (
            "iCUE",
            &[
                "Corsair\\Corsair iCUE5 Software\\iCUE.exe",
                "Corsair\\CORSAIR iCUE 5 Software\\iCUE.exe",
                "Corsair\\CORSAIR iCUE 4 Software\\iCUE.exe",
            ],
        ),
        "Razer" => (
            "Synapse",
            &[
                "Razer\\RazerAppEngine\\RazerAppEngine.exe",
                "Razer\\Synapse3\\WPFUI\\Framework\\Razer Synapse 3 Host\\Razer Synapse 3.exe",
            ],
        ),
        "SteelSeries" => ("GG", &["SteelSeries\\GG\\SteelSeriesGG.exe"]),
        _ => ("", &[]),
    }
}

/// Le logiciel de la marque, s'il est installé (`existe` : le fichier est là)
pub fn logiciel_installe(marque: &str, dossiers: &[String], existe: impl Fn(&str) -> bool) -> Option<Logiciel> {
    let (nom, l) = programmes(marque);
    dossiers.iter().filter(|d| !d.is_empty()).find_map(|d| {
        l.iter()
            .map(|p| format!("{}\\{p}", d.trim_end_matches('\\')))
            .find(|c| existe(c))
            .map(|chemin| Logiciel { nom: nom.into(), chemin })
    })
}

/// Le nom que montre G Hub quand l'appareil s'annonce sous un nom court (la PRO X SUPERLIGHT 2 dit « PRO X 2 »)
pub fn nom_connu(sorte: Sorte, nom: String) -> String {
    match (sorte, nom.as_str()) {
        (Sorte::Souris, "PRO X 2") => "PRO X SUPERLIGHT 2".into(),
        _ => nom,
    }
}

/// Les appareils montrés en mode test (`ctx.essai`) : jamais les vrais
pub fn exemple() -> Vec<Peripherique> {
    let p = |sorte, nom: &str, sans_fil, batterie, charge| Peripherique {
        sorte,
        nom: nom.into(),
        marque: "Logitech".into(),
        sans_fil,
        batterie,
        charge,
        logiciel: None,
    };
    vec![
        p(Sorte::Casque, "PRO X 2 LIGHTSPEED", true, Some(64), Charge::EnCharge),
        p(Sorte::Souris, "PRO X SUPERLIGHT 2", true, Some(18), Charge::Batterie),
        p(Sorte::Clavier, "PRO X TKL RAPID", false, None, Charge::Batterie),
    ]
}

/// Les appareils dans l'ordre de la page : casque, souris, clavier ; un même appareil vu deux fois (par son
/// récepteur et par son câble de charge) n'est gardé qu'une fois, avec sa batterie
pub fn ranger(mut l: Vec<Peripherique>) -> Vec<Peripherique> {
    let rang = |s: Sorte| match s {
        Sorte::Casque => 0,
        Sorte::Souris => 1,
        Sorte::Clavier => 2,
    };
    l.sort_by_key(|p| (rang(p.sorte), p.batterie.is_none()));
    let mut r: Vec<Peripherique> = Vec::new();
    for p in l {
        if !r.iter().any(|x| x.sorte == p.sorte && x.nom == p.nom) {
            r.push(p);
        }
    }
    r
}

// ---------------------------------------------------------------------------------------------------------------
// Les pages
// ---------------------------------------------------------------------------------------------------------------

fn texte_valeur(p: &Peripherique) -> String {
    p.batterie.map(|b| format!("{b} %")).unwrap_or_else(|| "Filaire".into())
}

/// Les blocs de la page « Ton PC », juste après « Ton PC maintenant » : rien du tout sans appareil
pub fn blocs(l: &[Peripherique]) -> Vec<Value> {
    if l.is_empty() {
        return Vec::new();
    }
    let mut b = vec![json!({ "type": "titre", "texte": "Mes périphériques" })];
    for p in l {
        let mut x = json!({ "type": "ligne", "id": p.page(), "icone": p.sorte.icone(),
            "couleur": if p.ton() == "urgent" { "rouge" } else { "P" },
            "texte": p.nom, "note": format!("{} · {}", p.sorte.genre(), p.etat()),
            "valeur": texte_valeur(p), "ton": p.ton() });
        if let Some(v) = p.batterie {
            x["barre"] = json!(v);
        }
        // (la ligne mène au détail de l'appareil ; son bouton ouvre le logiciel de la marque, maquette B)
        if let Some(lg) = &p.logiciel {
            x["bouton"] =
                json!({ "id": action_logiciel(&p.marque), "texte": format!("Ouvrir {}", lg.nom), "style": "second" });
        }
        b.push(x);
    }
    b
}

/// La page de détail d'un appareil (parent : « Ton PC »)
pub fn page_detail(p: &Peripherique) -> Value {
    let bouton = p
        .logiciel
        .as_ref()
        .map(|lg| json!({ "id": action_logiciel(&p.marque), "texte": format!("Ouvrir {}", lg.nom) }));
    let boutons: Vec<Value> = bouton.iter().cloned().collect();
    let module = match p.batterie {
        Some(b) => {
            let titre = match p.charge {
                Charge::EnCharge => "En charge",
                Charge::Pleine => "Chargé",
                Charge::Batterie if b < 20 => "Presque vide",
                Charge::Batterie => "Sur batterie",
            };
            json!({ "anneau": { "valeur": b, "sur": 100, "ton": p.ton() }, "titre": titre,
                "texte": format!("Batterie à {b} %. Nevermind la relit toutes les minutes."), "boutons": boutons })
        }
        None => json!({ "icone": p.sorte.icone(), "titre": "Branché par câble",
            "texte": "Pas de batterie : il marche tant qu'il est branché.", "boutons": boutons }),
    };
    let mut blocs = vec![json!({ "type": "titre", "texte": p.sorte.ton_nom() })];
    if let Some(b) = p.batterie {
        blocs.push(json!({ "type": "ligne", "icone": "eclair",
            "couleur": if p.en_charge() { "vert" } else if p.ton() == "urgent" { "rouge" } else { "P" },
            "texte": "Batterie", "note": p.etat(), "barre": b, "valeur": format!("{b} %"), "ton": p.ton() }));
    }
    blocs.push(json!({ "type": "ligne", "icone": if p.sans_fil { "wifi" } else { "cable" }, "couleur": "P", "texte": "Connexion",
        "valeur": if p.sans_fil { "Sans fil" } else { "Câble USB" } }));
    blocs.push(
        json!({ "type": "ligne", "icone": p.sorte.icone(), "couleur": "P", "texte": "Marque", "valeur": p.marque }),
    );
    if let Some(lg) = &p.logiciel {
        blocs.push(
            json!({ "type": "ligne", "icone": "app", "couleur": "S", "texte": "Logiciel", "note": p.sorte.reglages(),
            "valeur": format!("{} {}", p.marque, lg.nom) }),
        );
        blocs.push(
            json!({ "type": "note", "texte": format!("{} peut rester ouvert : ça ne gêne pas Nevermind.", lg.nom) }),
        );
    }
    let sous = format!(
        "{} {}, {}.",
        p.sorte.ton_nom(),
        p.marque,
        if p.sans_fil {
            "sans fil"
        } else if p.sorte == Sorte::Souris {
            "branchée par câble"
        } else {
            "branché par câble"
        }
    );
    json!({ "titre": p.nom, "parent": "pc", "sous": sous, "icone": p.sorte.icone(), "couleur": "P",
        "actions": boutons, "module": module, "blocs": blocs })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(t: &str) -> Vec<u8> {
        t.split_whitespace().map(|x| u8::from_str_radix(x, 16).unwrap()).collect()
    }

    /// Les octets lus sur un vrai PC (souris par le récepteur LIGHTSPEED C54D : 58 %, sur batterie)
    #[test]
    fn nom_montre_par_g_hub() {
        assert_eq!(nom_connu(Sorte::Souris, "PRO X 2".into()), "PRO X SUPERLIGHT 2");
        assert_eq!(nom_connu(Sorte::Casque, "PRO X 2".into()), "PRO X 2");
        assert_eq!(nom_connu(Sorte::Souris, "G502 X".into()), "G502 X");
    }

    #[test]
    fn souris_par_le_recepteur() {
        assert_eq!(hidpp_question_fonction(1, 0x1004).to_vec(), h("11 01 00 0C 10 04"));
        let r = h("11 01 00 0C 06 00 05 00 00 00 00 00 00 00 00 00 00 00 00 00");
        let Reponse::Donnees(d) = hidpp_reponse(&r, 1, 0, 0) else { panic!("réponse attendue") };
        assert_eq!(d[0], 6);
        assert_eq!(hidpp_appel(1, 6, 1, &[]), h("11 01 06 1C"));
        let r = h("11 01 06 1C 3A 04 00 00 00 00 00 00 00 00 00 00 00 00 00 00");
        let Reponse::Donnees(d) = hidpp_reponse(&r, 1, 6, 1) else { panic!("réponse attendue") };
        assert_eq!(hidpp_batterie(d), Some((58, Charge::Batterie)));
        // (la réponse à une question de G Hub, identifiant 0x0D : pas pour nous)
        assert_eq!(hidpp_reponse(&h("11 01 06 1D 3A 04 00"), 1, 6, 1), Reponse::Autre);
        // (une autre case du récepteur)
        assert_eq!(hidpp_reponse(&r, 2, 6, 1), Reponse::Autre);
        // (erreurs : appareil éteint, fonction refusée)
        assert_eq!(hidpp_reponse(&h("10 01 8F 00 0C 09 00"), 1, 0, 0), Reponse::Refus);
        assert_eq!(hidpp_reponse(&h("11 01 FF 06 1C 05 00 00"), 1, 6, 1), Reponse::Refus);
    }

    /// Le clavier filaire (C35B) : la fonction de batterie est à l'index 0, donc absente
    #[test]
    fn clavier_filaire() {
        let r = h("11 FF 00 0C 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00");
        assert_eq!(hidpp_reponse(&r, 0xFF, 0, 0), Reponse::Donnees(&r[4..]));
        assert_eq!(r[4], 0, "index 0 : pas de batterie");
    }

    #[test]
    fn etats_de_charge_et_niveaux() {
        assert_eq!(hidpp_batterie(&[90, 8, 1]), Some((90, Charge::EnCharge)));
        assert_eq!(hidpp_batterie(&[90, 8, 2]), Some((90, Charge::EnCharge)));
        assert_eq!(hidpp_batterie(&[100, 8, 3]), Some((100, Charge::Pleine)));
        assert_eq!(hidpp_batterie(&[40, 4, 7]), Some((40, Charge::Batterie)));
        // (seulement un niveau)
        assert_eq!(hidpp_batterie(&[0, 2, 0]), Some((20, Charge::Batterie)));
        assert_eq!(hidpp_batterie(&[0, 1]), None);
        assert_eq!(hidpp_sorte(&[0]), Some(Sorte::Clavier));
        assert_eq!(hidpp_sorte(&[3]), Some(Sorte::Souris));
        assert_eq!(hidpp_sorte(&[7]), None);
    }

    /// Le casque PRO X 2 (0AF7), octets lus sur un vrai PC : pont 3, BATTERY_SOC à l'index 4, 96 % en charge
    #[test]
    fn casque_centurion() {
        assert_eq!(centurion_question_pont(), h("51 05 00 00 0C 00 03"));
        let r = h("51 06 00 00 0C 03 00 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00");
        assert_eq!(centurion_pont(&r), Reponse::Donnees(&[3]));
        assert_eq!(centurion_question_batterie(3), h("51 0A 00 03 1C 00 05 00 00 0C 01 04"));
        // (l'accusé de réception, à ignorer)
        let accuse = h("51 03 00 03 1C 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00");
        assert_eq!(centurion_reponse(&accuse, 3, 0), Reponse::Autre);
        let r = h("51 0B 00 03 10 00 06 00 00 0C 04 00 02 00 00 00 00 00 00 00 00 00 00 00");
        let Reponse::Donnees(d) = centurion_reponse(&r, 3, 0) else { panic!("réponse attendue") };
        assert_eq!(centurion_index_batterie(d), Some(4));
        assert_eq!(centurion_question_etat(3, 4), h("51 08 00 03 1C 00 03 00 04 0C"));
        assert_eq!(centurion_reponse(&accuse, 3, 4), Reponse::Autre);
        let r = h("51 0B 00 03 10 00 06 00 04 0C 60 60 02 00 00 00 00 00 00 00 00 00 00 00");
        let Reponse::Donnees(d) = centurion_reponse(&r, 3, 4) else { panic!("réponse attendue") };
        assert_eq!(centurion_batterie(d), Some((96, Charge::EnCharge)));
        // (le deuxième récepteur 0AF8 refuse : appareil non affiché)
        assert_eq!(centurion_pont(&h("51 05 00 FF 00 0C 05 00 00 00 00 00 00")), Reponse::Refus);
        // (refusé par le casque)
        let r = h("51 0B 00 03 10 00 06 00 FF 0C 04 00 02 00");
        assert_eq!(centurion_reponse(&r, 3, 4), Reponse::Refus);
        assert_eq!(centurion_batterie(&[4, 0x0C, 50, 50, 0]), Some((50, Charge::Batterie)));
        assert_eq!(centurion_batterie(&[4, 0x0C, 100, 100, 3]), Some((100, Charge::Pleine)));
    }

    #[test]
    fn recepteurs() {
        assert!(est_recepteur(0xC54D, "USB Receiver"));
        assert!(est_recepteur(0xC548, ""));
        assert!(!est_recepteur(0xC35B, "PRO X RAPID"));
        assert!(!est_recepteur(0x0AF7, "PRO X 2 LIGHTSPEED"));
    }

    #[test]
    fn logiciels() {
        let pf = vec!["C:\\Program Files".to_string(), "C:\\Program Files (x86)".to_string()];
        let l = logiciel_installe("Logitech", &pf, |c| c == "C:\\Program Files\\LGHUB\\lghub.exe");
        assert_eq!(l, Some(Logiciel { nom: "G Hub".into(), chemin: "C:\\Program Files\\LGHUB\\lghub.exe".into() }));
        assert_eq!(logiciel_installe("Logitech", &pf, |_| false), None);
        let s = logiciel_installe("SteelSeries", &pf, |c| c.ends_with("SteelSeriesGG.exe")).unwrap();
        assert_eq!(s.nom, "GG");
        assert_eq!(logiciel_installe("Inconnue", &pf, |_| true), None);
    }

    #[test]
    fn pages_et_tons() {
        let l = exemple();
        assert_eq!(l[0].page(), "perif:casque-pro-x-2-lightspeed");
        assert_eq!(l[0].ton(), "ok");
        assert_eq!(l[1].ton(), "urgent");
        assert_eq!(l[1].etat(), "Presque vide, pense à la brancher");
        assert_eq!(l[2].ton(), "");
        assert_eq!(l[2].etat(), "Branché par câble");
        let mut p = l[1].clone();
        p.batterie = Some(30);
        assert_eq!(p.ton(), "attention");
        p.batterie = Some(35);
        assert_eq!(p.ton(), "");
        assert!(blocs(&[]).is_empty());
        let b = blocs(&l);
        assert_eq!(b.len(), 4, "le titre et trois lignes, aucun bouton sans logiciel");
        assert_eq!(b[3]["valeur"], "Filaire");
        assert!(b[3].get("barre").is_none());
        assert_eq!(b[1]["barre"], 64);
        assert_eq!(b[1]["note"], "Casque · En charge");
    }

    #[test]
    fn rangement() {
        let mut l = exemple();
        l.reverse();
        let mut double = l[1].clone();
        double.sans_fil = false;
        double.batterie = None;
        l.push(double);
        let r = ranger(l);
        assert_eq!(r.iter().map(|p| p.sorte).collect::<Vec<_>>(), vec![Sorte::Casque, Sorte::Souris, Sorte::Clavier]);
        assert_eq!(r[1].batterie, Some(18));
    }

    /// Aucun tiret long dans les textes montrés
    #[test]
    fn textes_sans_tiret_long() {
        let mut l = exemple();
        l[0].logiciel = Some(Logiciel { nom: "G Hub".into(), chemin: "x".into() });
        let mut t = serde_json::to_string(&blocs(&l)).unwrap();
        for p in &l {
            t.push_str(&page_detail(p).to_string());
        }
        assert!(!t.contains('\u{2014}') && !t.contains('\u{2013}'));
    }
}
