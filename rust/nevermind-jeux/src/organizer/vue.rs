//! Ce que montre la vue de l'Organizer (Build-OrgPanel, Show-OrgView, New-OrgKeyRow, New-OrgKeyButton) : les mêmes
//! textes et les mêmes boutons que la version PowerShell, sans le dessin (fait par `windows::organizer_vue`).

use super::veille::Veille;
use super::{MAX_SIMULEES, MAX_TOUCHES, Perso, couleur, en_ligne, envoi, id_classe, initiale, nom_touche};

pub const TITRE: &str = "Organizer Dofus";
pub const SOUS_TITRE: &str = "Passe d'un perso à l'autre au clavier ou avec la barre flottante.";
pub const PERSOS_TITRE: &str = "Mes personnages";
pub const PERSOS_NOTE: &str = "Range-les dans ton ordre d'initiative : glisse une ligne pour la déplacer.";
pub const ACTIVER: &str = "Activer l'organizer";
pub const ACTIVER_NOTE: &str = "Raccourcis clavier et barre flottante pendant que tu joues.";
pub const AUCUNE_FENETRE: &str = "Aucune fenêtre Dofus ouverte. Lance tes comptes : tes persos apparaîtront ici tout seuls, dès qu'ils sont connectés.";
pub const RACCOURCIS_TITRE: &str = "Raccourcis";
pub const RACCOURCIS_NOTE: &str =
    "Ils ne marchent que quand une fenêtre Dofus est devant : ailleurs, tes touches restent normales.";
pub const UNE_TOUCHE: &str = "Une touche par perso";
pub const DEJA_PRISE: &str = "Déjà prise par un autre programme : choisis-en une autre.";
pub const APPUIE: &str = "Appuie sur une touche...";
pub const TOUCHE_AIDE: &str = "Clique puis appuie sur la touche ou le bouton de souris voulu (bouton latéral, clic molette). Échap : annuler, Retour arrière : aucune touche";
pub const BARRE_TITRE: &str = "Barre flottante";
pub const BARRE_NOTE: &str = "Un bouton par perso connecté, par-dessus le jeu.";
pub const BARRE: &str = "Afficher la barre";
pub const BARRE_SOUS: &str = "Visible quand une fenêtre Dofus est devant.";
pub const TOUJOURS: &str = "Toujours visible";
pub const TOUJOURS_SOUS: &str = "Même quand tu es sur une autre fenêtre que Dofus.";
pub const PLEIN_ECRAN: &str = "Elle n'apparaît pas par-dessus Dofus ? Mets le jeu en mode fenêtré ou plein écran fenêtré (sans bordure) : le vrai plein écran cache tout ce qui est par-dessus, y compris Discord.";
pub const REMETTRE: &str = "Remettre la barre en haut au centre";
pub const REMISE: &str = "Barre flottante remise en haut au centre.";
pub const SIMULEES: &str = "Un bouton qui appuie sur une touche";
pub const SIMULEES_NOTE: &str = "Pour une touche du jeu que tu veux sur ta souris (suivre le chef, par exemple) : choisis le bouton, puis la touche qu'il fait. Elle part seulement dans la fenêtre du perso affiché.";
pub const ENVOI_AIDE: &str = "Clique puis appuie sur la touche du jeu que ce bouton doit faire (par exemple A). Échap : annuler, Retour arrière : aucune touche";
pub const NOTE_BAS: &str = "Nevermind change seulement de fenêtre. Il appuie sur une touche à ta place seulement avec un bouton que tu as réglé pour ça, et dans la fenêtre devant. Il ne lit rien dans le jeu, à part le titre de la fenêtre.";
pub const RETIRER_AIDE: &str = "Enlever ce perso de la liste (il reviendra s'il se reconnecte)";
pub const GLISSER_BARRE: &str = "Glisse pour déplacer la barre";
pub const AUCUN_CONNECTE: &str = "Aucun perso connecté";

/// Le bouton d'une ligne de perso.
#[derive(Debug, Clone, PartialEq)]
pub enum BoutonLigne {
    /// « Afficher » : la fenêtre du perso devant.
    Afficher(isize),
    /// « Retirer » : le perso pas connecté sort de la liste.
    Retirer(String),
    Aucun,
}

/// Une ligne de « Mes personnages ».
#[derive(Debug, Clone, PartialEq)]
pub struct Ligne {
    /// Le nom du perso (« » : une fenêtre à l'écran de connexion).
    pub nom: String,
    /// Le numéro (connectés seulement : c'est celui des touches F1, F2...).
    pub numero: String,
    pub titre: String,
    pub etat: String,
    /// L'état en vert (connecté avec un perso).
    pub etat_ok: bool,
    pub en_ligne: bool,
    pub bouton: BoutonLigne,
    pub pastille: PastilleVue,
}

/// La pastille d'un perso : l'initiale sur la couleur de sa classe.
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

/// Une ligne de « Raccourcis ».
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
    /// Le petit titre (et sa note) au-dessus de la ligne : « Une touche par perso »...
    pub titre_avant: Option<(&'static str, Option<&'static str>)>,
    /// L'infobulle du bouton.
    pub aide: &'static str,
}

/// Toute la vue.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Modele {
    pub on: bool,
    pub barre: bool,
    pub toujours: bool,
    pub lignes: Vec<Ligne>,
    pub vide: bool,
    pub touches: Vec<LigneTouche>,
}

fn ligne(ch: &Perso, numero: &mut usize) -> Ligne {
    let num = if ch.en_ligne && !ch.nom.is_empty() {
        *numero += 1;
        numero.to_string()
    } else {
        String::new()
    };
    let etat = if ch.nom.is_empty() {
        "Pas encore de perso choisi".to_string()
    } else if ch.en_ligne {
        format!("{}Connecté", if ch.classe.is_empty() { String::new() } else { format!("{}  ·  ", ch.classe) })
    } else {
        "Pas connecté".to_string()
    };
    Ligne {
        nom: ch.nom.clone(),
        numero: num,
        titre: if ch.nom.is_empty() { "Écran de connexion".into() } else { ch.nom.clone() },
        etat,
        etat_ok: ch.en_ligne && !ch.nom.is_empty(),
        en_ligne: ch.en_ligne,
        bouton: if ch.en_ligne {
            BoutonLigne::Afficher(ch.hwnd)
        } else if !ch.nom.is_empty() {
            BoutonLigne::Retirer(ch.nom.clone())
        } else {
            BoutonLigne::Aucun
        },
        pastille: PastilleVue::de(&ch.nom, &ch.classe),
    }
}

/// La vue d'après la liste des persos (Get-OrgList, déjà lue par la veille) et l'état de la veille.
pub fn modele(v: &Veille, list: &[Perso]) -> Modele {
    let mut n = 0;
    let lignes: Vec<Ligne> = list.iter().map(|ch| ligne(ch, &mut n)).collect();
    let on = en_ligne(list);
    let mut touches = Vec::new();
    let mut ajouter = |libelle: String, action: &str, titre_avant: Option<(&'static str, Option<&'static str>)>| {
        let en_choix = v.capture.as_deref() == Some(action);
        // (« E1 » : la touche faite dans le jeu par la touche simulée « S1 »)
        let t = if envoi(action) { v.config.envoi(&format!("S{}", &action[1..])) } else { v.config.touche(action) };
        touches.push(LigneTouche {
            action: action.to_string(),
            libelle,
            bouton: if en_choix { APPUIE.to_string() } else { nom_touche(&t) },
            en_choix,
            vide: t.is_empty() && !en_choix,
            refusee: !envoi(action) && v.refusee(action),
            titre_avant,
            aide: if envoi(action) { ENVOI_AIDE } else { TOUCHE_AIDE },
        });
    };
    ajouter("Perso suivant".into(), "Next", None);
    ajouter("Perso précédent".into(), "Prev", None);
    for i in 1..=MAX_TOUCHES {
        let lib = if i <= on.len() { format!("{i}.  {}", on[i - 1].nom) } else { format!("Perso {i}") };
        ajouter(lib, &format!("P{i}"), (i == 1).then_some((UNE_TOUCHE, None)));
    }
    for i in 1..=MAX_SIMULEES {
        ajouter(format!("Bouton {i}"), &format!("S{i}"), (i == 1).then_some((SIMULEES, Some(SIMULEES_NOTE))));
        ajouter("    appuie sur".into(), &format!("E{i}"), None);
    }
    Modele {
        on: v.config.on,
        barre: v.config.barre,
        toujours: v.config.barre_toujours,
        vide: list.is_empty(),
        lignes,
        touches,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organizer::Config;

    #[test]
    fn textes() {
        let mut v = Veille::nouvelle(Config::default());
        v.capture = Some("P2".into());
        let l = vec![
            Perso { nom: "Bob".into(), classe: "Iop".into(), hwnd: 5, pid: 1, en_ligne: true },
            Perso { nom: "Zed".into(), classe: String::new(), hwnd: 0, pid: 0, en_ligne: false },
            Perso { nom: String::new(), classe: String::new(), hwnd: 9, pid: 2, en_ligne: true },
        ];
        let m = modele(&v, &l);
        assert_eq!(m.lignes[0].etat, "Iop  ·  Connecté");
        assert_eq!(m.lignes[0].numero, "1");
        assert_eq!(m.lignes[1].bouton, BoutonLigne::Retirer("Zed".into()));
        assert_eq!(m.lignes[2].titre, "Écran de connexion");
        assert_eq!(m.touches[2].libelle, "1.  Bob");
        assert_eq!(m.touches[3].bouton, APPUIE);
        assert_eq!(m.touches[4].libelle, "Perso 3");
        assert_eq!(m.touches[0].bouton, "Ctrl + Tab");
        // (les touches simulées : le bouton, puis la touche qu'il fait dans le jeu)
        assert_eq!(m.touches[10].libelle, "Bouton 1");
        assert_eq!(m.touches[10].titre_avant.map(|t| t.0), Some(SIMULEES));
        assert_eq!(m.touches[11].action, "E1");
        assert_eq!(m.touches[11].bouton, "Aucune");
        assert_eq!(m.touches.len(), 16);
    }

    #[test]
    fn touche_simulee_choisie() {
        let mut v = Veille::nouvelle(Config::default());
        v.config.envois.push(("S1".into(), "A".into()));
        v.config.touches.push(("S1".into(), "Souris5".into()));
        let m = modele(&v, &[]);
        assert_eq!(m.touches[10].bouton, "Souris 5 (avant)");
        assert_eq!(m.touches[11].bouton, "A");
        assert!(!m.touches[11].vide);
    }
}
