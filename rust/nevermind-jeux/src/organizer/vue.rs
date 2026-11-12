//! Ce que montre la vue de l'Organizer, sans le dessin (fait par `windows::organizer_vue`). Depuis le 12/11 (maquette
//! « E1 » choisie par Jordan), tout tient sur un écran, en trois colonnes :
//! - « Mes personnages » : seulement les persos connectés (et les fenêtres à l'écran de connexion), chacun avec son logo
//!   de classe et sa touche (F1, F2...) ; l'ordre d'initiative des autres est gardé sans être montré ;
//! - « Raccourcis » (perso suivant et précédent) et « Barre flottante » ;
//! - « Boutons de souris » : les touches simulées réglées en pastilles, « + Ajouter » et la création en deux cases
//!   (le bouton de souris, puis la touche du jeu).

use super::veille::Veille;
use super::{MAX_SIMULEES, Perso, couleur, en_ligne, id_classe, initiale, nom_touche, simulee, simulees};

pub const TITRE: &str = "Organizer Dofus";
pub const SOUS_TITRE: &str = "Passe d'un perso à l'autre au clavier, à la souris ou avec la barre flottante.";
pub const PERSOS_TITRE: &str = "Mes personnages";
pub const PERSOS_NOTE: &str = "Glisse une ligne pour l'ordre d'initiative. Clique sur une touche pour la changer.";
pub const ACTIVER: &str = "Organizer activé";
pub const ACTIVER_NOTE: &str = "Pendant que tu joues";
pub const AUCUNE_FENETRE: &str = "Aucune fenêtre Dofus ouverte. Lance tes comptes : tes persos apparaîtront ici tout seuls, dès qu'ils sont connectés.";
pub const RACCOURCIS_TITRE: &str = "Raccourcis";
pub const RACCOURCIS_NOTE: &str = "Seulement quand Dofus est devant : ailleurs, tes touches restent normales.";
pub const DEJA_PRISE: &str = "Déjà prise par un autre programme : choisis-en une autre.";
pub const APPUIE: &str = "Appuie sur une touche...";
pub const TOUCHE_AIDE: &str = "Clique puis appuie sur la touche ou le bouton de souris voulu (bouton latéral, clic molette). Échap : annuler, Retour arrière : aucune touche";
pub const BARRE_TITRE: &str = "Barre flottante";
pub const BARRE_NOTE: &str = "Un bouton par perso connecté, par-dessus le jeu.";
pub const BARRE: &str = "Afficher la barre";
pub const BARRE_SOUS: &str = "Quand Dofus est devant";
pub const TOUJOURS: &str = "Toujours visible";
pub const TOUJOURS_SOUS: &str = "Même sur une autre fenêtre";
pub const PLEIN_ECRAN: &str = "Elle n'apparaît pas par-dessus Dofus ? Mets le jeu en mode fenêtré ou plein écran fenêtré (sans bordure) : le vrai plein écran cache tout ce qui est par-dessus, y compris Discord.";
pub const REMETTRE: &str = "Remettre la barre en haut au centre";
pub const REMISE: &str = "Barre flottante remise en haut au centre.";
pub const SOURIS_TITRE: &str = "Boutons de souris";
pub const SOURIS_NOTE: &str = "Une touche du jeu sur ta souris (suivre le chef, par exemple). ✕ pour en enlever un.";
pub const AJOUTER: &str = "+ Ajouter un bouton";
pub const PLEIN: &str = "20 boutons réglés : enlèves-en un pour en ajouter un autre.";
pub const CASE_SOURIS: &str = "Bouton de souris";
pub const CASE_TOUCHE: &str = "Touche du jeu";
pub const ETAPE_SOURIS: &str =
    "Appuie sur le bouton de ta souris que tu veux utiliser (bouton sur le côté, clic molette).";
pub const ETAPE_TOUCHE: &str = "Maintenant, appuie sur la touche du jeu qu'il doit faire (par exemple A).";
pub const ANNULER: &str = "Annuler";
pub const EN_ATTENTE: &str = "Appuie...";
pub const PUCE_AIDE: &str = "Clique sur le bouton ou sur la touche pour le changer";
pub const SOURIS_AIDE: &str = "Clique puis appuie sur le bouton de ta souris voulu. Échap : annuler";
pub const ENVOI_AIDE: &str =
    "Clique puis appuie sur la touche du jeu que ce bouton doit faire (par exemple A). Échap : annuler";
pub const ENLEVER_AIDE: &str = "Enlever ce bouton";
pub const NOTE_BAS: &str = "Nevermind change seulement de fenêtre. Il appuie sur une touche à ta place seulement avec un bouton de souris que tu as réglé pour ça, et dans la fenêtre devant. Il ne lit rien dans le jeu, à part le titre de la fenêtre.";
pub const GLISSER_BARRE: &str = "Glisse pour déplacer la barre";
pub const AUCUN_CONNECTE: &str = "Aucun perso connecté";

/// Une ligne de « Mes personnages » (un perso connecté, ou une fenêtre à l'écran de connexion).
#[derive(Debug, Clone, PartialEq)]
pub struct Ligne {
    /// Le nom du perso (« » : une fenêtre à l'écran de connexion).
    pub nom: String,
    /// Le numéro dans l'ordre d'initiative (c'est celui de sa touche).
    pub numero: String,
    pub titre: String,
    pub etat: String,
    /// La fenêtre du perso (un clic sur la ligne la met devant).
    pub hwnd: isize,
    /// Sa fenêtre est devant.
    pub devant: bool,
    pub pastille: PastilleVue,
    /// Sa touche (F1...), `None` à l'écran de connexion.
    pub touche: Option<LigneTouche>,
}

/// La pastille d'un perso : le logo de sa classe, sinon l'initiale sur la couleur de sa classe.
#[derive(Debug, Clone, PartialEq)]
pub struct PastilleVue {
    pub initiale: String,
    pub couleur: &'static str,
    pub classe: String,
    pub id_classe: u32,
}

impl PastilleVue {
    pub fn de(nom: &str, classe: &str) -> Self {
        PastilleVue {
            initiale: initiale(nom),
            couleur: couleur(classe, nom),
            classe: classe.to_string(),
            id_classe: id_classe(classe),
        }
    }
}

/// Une touche à choisir (un raccourci, la touche d'un perso).
#[derive(Debug, Clone, PartialEq)]
pub struct LigneTouche {
    pub action: String,
    pub libelle: String,
    /// Le texte du bouton : la touche, « Aucune », ou « Appuie sur une touche... » pendant le choix.
    pub bouton: String,
    pub en_choix: bool,
    /// Pas de touche (texte grisé).
    pub vide: bool,
    /// Refusée par Windows : « Déjà prise par un autre programme ».
    pub refusee: bool,
}

/// Une touche simulée réglée, en pastille : « 🖱 bouton ➜ touche du jeu ✕ ».
#[derive(Debug, Clone, PartialEq)]
pub struct Puce {
    /// « S4 »
    pub id: String,
    pub bouton: String,
    pub touche: String,
    /// Le bouton ou la touche est en train d'être changé.
    pub choix_bouton: bool,
    pub choix_touche: bool,
}

/// La création guidée d'une touche simulée : les deux grandes cases.
#[derive(Debug, Clone, PartialEq)]
pub struct Creation {
    /// 1 : le bouton de souris ; 2 : la touche du jeu.
    pub etape: u8,
    /// Le bouton déjà choisi (étape 2).
    pub bouton: String,
}

/// Toute la vue.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Modele {
    pub on: bool,
    pub barre: bool,
    pub toujours: bool,
    pub lignes: Vec<Ligne>,
    /// Aucune fenêtre du jeu ouverte.
    pub vide: bool,
    /// Perso suivant et précédent.
    pub touches: Vec<LigneTouche>,
    pub puces: Vec<Puce>,
    pub creation: Option<Creation>,
    /// Les 20 touches simulées sont prises.
    pub plein: bool,
}

fn ligne_touche(v: &Veille, libelle: String, action: &str) -> LigneTouche {
    let en_choix = v.capture.as_deref() == Some(action);
    let t = v.config.touche(action);
    LigneTouche {
        action: action.to_string(),
        libelle,
        bouton: if en_choix { APPUIE.to_string() } else { nom_touche(&t) },
        en_choix,
        vide: t.is_empty() && !en_choix,
        refusee: v.refusee(action),
    }
}

/// Le nom d'un bouton de souris pour une pastille (« Souris 5 », « Ctrl + Molette »).
pub fn nom_court(t: &str) -> String {
    nom_touche(t).replace(" (avant)", "").replace(" (arrière)", "").replace("Clic molette", "Molette")
}

/// La vue d'après la liste des persos (Get-OrgList, déjà lue par la veille), la fenêtre devant, et la veille.
pub fn modele(v: &Veille, list: &[Perso], devant: isize) -> Modele {
    let mut n = 0;
    let lignes: Vec<Ligne> = list
        .iter()
        .filter(|ch| ch.en_ligne)
        .map(|ch| {
            let numero = if ch.nom.is_empty() {
                String::new()
            } else {
                n += 1;
                n.to_string()
            };
            Ligne {
                nom: ch.nom.clone(),
                titre: if ch.nom.is_empty() { "Écran de connexion".into() } else { ch.nom.clone() },
                etat: if ch.nom.is_empty() {
                    "Pas encore de perso choisi".into()
                } else if ch.classe.is_empty() {
                    "Connecté".into()
                } else {
                    ch.classe.clone()
                },
                hwnd: ch.hwnd,
                devant: ch.hwnd != 0 && ch.hwnd == devant,
                pastille: PastilleVue::de(&ch.nom, &ch.classe),
                touche: (!ch.nom.is_empty() && n <= super::MAX_TOUCHES)
                    .then(|| ligne_touche(v, String::new(), &format!("P{n}"))),
                numero,
            }
        })
        .collect();
    let touches =
        vec![ligne_touche(v, "Perso suivant".into(), "Next"), ligne_touche(v, "Perso précédent".into(), "Prev")];
    let creation = v.creation.as_deref().and_then(|s| {
        let etape = match v.capture.as_deref() {
            Some(c) if c == s => 1,
            Some(c) if c.strip_prefix('E') == s.strip_prefix('S') => 2,
            _ => return None,
        };
        Some(Creation { etape, bouton: nom_court(&v.config.touche(s)) })
    });
    let puces: Vec<Puce> = simulees(&v.config)
        .into_iter()
        .filter(|s| v.creation.as_deref() != Some(s.as_str()))
        .map(|s| {
            let e = format!("E{}", &s[1..]);
            let choix_bouton = v.capture.as_deref() == Some(s.as_str());
            let choix_touche = v.capture.as_deref() == Some(e.as_str());
            Puce {
                bouton: if choix_bouton { EN_ATTENTE.into() } else { nom_court(&v.config.touche(&s)) },
                touche: if choix_touche { EN_ATTENTE.into() } else { nom_touche(&v.config.envoi(&s)) },
                choix_bouton,
                choix_touche,
                id: s,
            }
        })
        .collect();
    let nb = simulees(&v.config).len();
    let _ = simulee;
    Modele {
        on: v.config.on,
        barre: v.config.barre,
        toujours: v.config.barre_toujours,
        vide: lignes.is_empty(),
        lignes,
        touches,
        puces,
        creation,
        plein: nb >= MAX_SIMULEES && v.creation.is_none(),
    }
}

/// Les persos connectés, pour l'aperçu de la barre (le même ordre que la liste).
pub fn connectes(list: &[Perso]) -> Vec<Perso> {
    en_ligne(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organizer::Config;

    fn persos() -> Vec<Perso> {
        vec![
            Perso { nom: "Bob".into(), classe: "Iop".into(), hwnd: 5, pid: 1, en_ligne: true },
            Perso { nom: "Zed".into(), classe: String::new(), hwnd: 0, pid: 0, en_ligne: false },
            Perso { nom: "Ana".into(), classe: "Cra".into(), hwnd: 7, pid: 3, en_ligne: true },
            Perso { nom: String::new(), classe: String::new(), hwnd: 9, pid: 2, en_ligne: true },
        ]
    }

    #[test]
    fn seulement_les_connectes_avec_leur_touche() {
        let mut v = Veille::nouvelle(Config::default());
        v.capture = Some("P2".into());
        let m = modele(&v, &persos(), 7);
        // (Zed, pas connecté, n'est plus montré : pas d'historique)
        assert_eq!(m.lignes.iter().map(|l| l.titre.as_str()).collect::<Vec<_>>(), ["Bob", "Ana", "Écran de connexion"]);
        assert_eq!(m.lignes[0].touche.as_ref().unwrap().bouton, "F1");
        assert_eq!(m.lignes[1].touche.as_ref().unwrap().bouton, APPUIE);
        assert!(m.lignes[1].devant && !m.lignes[0].devant);
        assert!(m.lignes[2].touche.is_none());
        assert_eq!(m.lignes[0].pastille.id_classe, 8);
        assert_eq!(m.touches.len(), 2);
        assert_eq!(m.touches[0].bouton, "Ctrl + Tab");
        assert!(m.puces.is_empty() && m.creation.is_none() && !m.plein);
    }

    #[test]
    fn creation_d_un_bouton_en_deux_cases() {
        let mut v = Veille::nouvelle(Config::default());
        v.config.touches.push(("S1".into(), "Souris5".into()));
        v.config.envois.push(("S1".into(), "A".into()));
        let m = modele(&v, &[], 0);
        assert_eq!((m.puces[0].bouton.as_str(), m.puces[0].touche.as_str()), ("Souris 5", "A"));
        // (+ Ajouter : la case du bouton s'allume ; la pastille en création n'est pas encore montrée)
        v.creation = Some("S2".into());
        v.capture = Some("S2".into());
        let m = modele(&v, &[], 0);
        assert_eq!(m.creation, Some(Creation { etape: 1, bouton: "Aucune".into() }));
        assert_eq!(m.puces.len(), 1);
        v.config.touches.push(("S2".into(), "Molette".into()));
        v.capture = Some("E2".into());
        let m = modele(&v, &[], 0);
        assert_eq!(m.creation, Some(Creation { etape: 2, bouton: "Molette".into() }));
    }
}
