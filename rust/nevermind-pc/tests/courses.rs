//! Lot A du second audit (09/11) : deux gestes de deux pages en même temps. Les essais de l'audit
//! (`audit2-nevermind-fond\essai\tests\courses.rs`) reproduisaient le défaut ; ici le défaut ne doit plus arriver.
//!
//! Rien ne touche le vrai PC : contexte d'essai, fausse Aide (les réglages du PC « écrits » sont seulement notés),
//! dossier d'essai.

use nevermind_noyau::travaux::{Rendu, Travaux};
use nevermind_noyau::{Bloc, Contexte, Effet, Effets};
use nevermind_pc::corrections::{Moteur, Sauvegarde, annuler_journal};
use nevermind_pc::systeme::PcWindows;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn dossier(nom: &str) -> PathBuf {
    let d = nevermind_noyau::dossier_essai(&format!("courses-{nom}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("a")).unwrap();
    std::fs::create_dir_all(d.join("d")).unwrap();
    d
}

fn attendre_fin<R: Send + 'static>(t: &mut Travaux<R>) -> Result<R, String> {
    let fin = Instant::now() + Duration::from_secs(20);
    loop {
        for r in t.relever() {
            if let Rendu::Fini { resultat, .. } = r {
                return resultat;
            }
        }
        assert!(Instant::now() < fin, "le travail ne finit pas");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Les demandes reçues par la fausse Aide : (action, paramètres)
type Demandes = Arc<Mutex<Vec<(String, Value)>>>;

fn fausse_aide(ctx: &Contexte) -> Demandes {
    let d: Demandes = Default::default();
    let d2 = d.clone();
    ctx.fausse_aide(Arc::new(move |a: &str, p: &Value| {
        d2.lock().unwrap().push((a.to_string(), p.clone()));
        Ok(json!("OK"))
    }));
    d
}

/// Un réglage du PC (écrit par la fausse Aide, jamais pour de vrai)
const CLE_RDP: &str = r"HKLM:\SOFTWARE\MindsetEssaiLotA\TerminalServer";
const CLE_ENVOIS: &str = r"HKLM:\SOFTWARE\MindsetEssaiLotA\DataCollection";

/// M-C : « Couper les envois » (Trafic) tient le verrou et son journal. Pendant ce temps :
/// - « Couper le Bureau à distance » (Réseau) est REFUSÉ, avec le texte exact ;
/// - même un geste qui passe en priorité (le mode jeu) n'entre pas dans le journal du Trafic ;
/// - « Annuler » la ligne du Trafic ne remet que ce que le Trafic a changé : le Bureau à distance n'est pas rouvert.
///
/// Avant (audit) : le journal était commun à tout Nevermind, `fDenyTSConnections` entrait dans la ligne du Trafic et
/// « Annuler » rouvrait le Bureau à distance.
#[test]
fn le_journal_d_un_geste_ne_ramasse_plus_le_geste_d_une_autre_page() {
    let d = dossier("journal");
    let ctx = Contexte::nouveau(d.join("a"), d.join("d"), false, true).partage();
    let demandes = fausse_aide(&ctx);

    // Le travail de l'hôte (« Couper les envois ») : le verrou pris au clic, puis son journal, l'Aide lente
    let (ouvert, journal_ouvert) = channel::<()>();
    let (feu, attente) = channel::<()>();
    let mut hote: Travaux<Vec<Value>> = Travaux::default();
    let g = ctx.prendre_geste("Couper les envois", "Trafic").unwrap();
    hote.lancer_geste(&ctx, "sauvegarde-appliquer", g, move |f| {
        f.ctx.commencer_changements();
        f.ctx.changer_registre(CLE_ENVOIS, "AllowTelemetry", &json!(0), "DWord").unwrap();
        ouvert.send(()).unwrap();
        let _ = attente.recv_timeout(Duration::from_secs(15));
        f.ctx.finir_changements()
    });
    journal_ouvert.recv_timeout(Duration::from_secs(10)).unwrap();

    // Le Réseau : le clic est refusé, avec le texte exact, et rien n'est demandé à l'Aide
    assert_eq!(
        ctx.prendre_geste("Couper le Bureau à distance", "Réseau").unwrap_err(),
        "Attends la fin de « Couper les envois » (Trafic)"
    );
    // Un fil sans geste ne peut rien changer (la vérification de chaque écriture)
    let c2 = ctx.clone();
    let r = std::thread::spawn(move || c2.changer_registre(CLE_RDP, "fDenyTSConnections", &json!(1), "DWord"))
        .join()
        .unwrap();
    assert!(r.is_err(), "un changement sans geste est passé");
    // Même un geste prioritaire, sur son fil, a son propre journal
    let mut prio: Travaux<Result<(), String>> = Travaux::default();
    let c3 = ctx.clone();
    prio.lancer(&ctx, "prio", move |_| {
        let _g = c3.geste_prioritaire("Essai prioritaire", "Mode jeu");
        c3.changer_registre(CLE_RDP, "fDenyTSConnections", &json!(1), "DWord")
    });
    attendre_fin(&mut prio).unwrap().unwrap();

    feu.send(()).unwrap();
    let log = attendre_fin(&mut hote).unwrap();
    assert_eq!(log.len(), 1, "le journal du Trafic : seulement son geste : {log:?}");
    assert_eq!(log[0]["Name"], "AllowTelemetry");
    assert!(!log.iter().any(|e| e["Name"] == "fDenyTSConnections"), "le geste d'une autre page est entré : {log:?}");

    // « Annuler » la ligne du Trafic : seulement AllowTelemetry, jamais le Bureau à distance
    demandes.lock().unwrap().clear();
    let _g = ctx.prendre_geste("Annuler", "Ton PC").unwrap();
    let mut p = PcWindows::nouveau(&ctx);
    let mut sv = Sauvegarde::default();
    let erreurs = {
        let mut m = Moteur::nouveau(&mut p, &mut sv, false);
        annuler_journal(&mut m, &log)
    };
    assert!(erreurs.is_empty(), "{erreurs:?}");
    let vus = demandes.lock().unwrap().clone();
    assert!(vus.iter().any(|(a, p)| a == "reg.effacer" && p["Nom"] == "AllowTelemetry"), "{vus:?}");
    assert!(
        !vus.iter().any(|(_, p)| p["Nom"] == "fDenyTSConnections"),
        "« Annuler » a touché le Bureau à distance : {vus:?}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// Ajustement B : « Appliquer » rend le verrou pendant la création du point de restauration (jusqu'à 10 min, rien de
/// noté) : les autres pages restent utilisables. Il le reprend ensuite, à son tour, avant de changer quoi que ce soit.
#[test]
fn le_point_de_restauration_laisse_les_autres_pages_utilisables() {
    let d = dossier("restauration");
    let ctx = Contexte::nouveau(d.join("a"), d.join("d"), false, true).partage();
    let (appele, aide_appelee) = channel::<()>();
    let (feu, attente) = channel::<()>();
    let attente = Mutex::new(Some(attente));
    let appele = Mutex::new(appele);
    ctx.fausse_aide(Arc::new(move |a: &str, _p: &Value| {
        if a == "restauration.creer" {
            let _ = appele.lock().unwrap().send(());
            if let Some(r) = attente.lock().unwrap().take() {
                let _ = r.recv_timeout(Duration::from_secs(20));
            }
        }
        Ok(json!("OK"))
    }));
    let mut appliquer: Travaux<(Result<Option<bool>, String>, bool)> = Travaux::default();
    let g = ctx.prendre_geste("Appliquer", "Ton PC").unwrap();
    appliquer.lancer_geste(&ctx, "pc", g, move |f| {
        let mut p = PcWindows::nouveau(f.ctx);
        let mut sv = Sauvegarde::default();
        let mut m = Moteur::nouveau(&mut p, &mut sv, false);
        let r = m.point_de_restauration();
        // (la suite d'« Appliquer » : le verrou est de nouveau à lui)
        (r, f.ctx.exiger_geste().is_ok())
    });
    aide_appelee.recv_timeout(Duration::from_secs(10)).expect("le point de restauration est demandé");
    // Pendant le point de restauration : le DNS du Réseau passe, et note son DNS d'origine
    let dns = ctx.prendre_geste("Changer le DNS", "Réseau").expect("les autres pages restent utilisables");
    ctx.changer_sauvegarde(|s| {
        s.dns.insert("{CARTE}".into(), json!("192.168.1.1"));
    });
    ctx.enregistrer_sauvegarde().unwrap();
    // Le point de restauration finit pendant que le DNS tient encore le verrou : « Appliquer » attend son tour
    feu.send(()).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(appliquer.relever().is_empty(), "« Appliquer » a continué sans le verrou");
    drop(dns);
    let (r, tient) = attendre_fin(&mut appliquer).unwrap();
    assert_eq!(r, Ok(Some(true)));
    assert!(tient, "« Appliquer » a repris le verrou avant de continuer");
    assert!(ctx.prendre_geste("Bloquer", "Trafic").is_ok(), "rendu à la fin");
    let _ = std::fs::remove_dir_all(&d);
}

/// Ajustement A : un profil de jeu (automatique) n'est jamais refusé : pendant le geste d'une autre page, il attend son
/// tour, puis il est fait, sans un nouveau clic. Pendant qu'il attend, il passe avant les nouveaux clics.
#[test]
fn un_profil_de_jeu_attend_son_tour_sans_etre_refuse() {
    let d = dossier("profil");
    let ctx = Contexte::nouveau(d.join("a"), d.join("d"), false, true).partage();
    let demandes = fausse_aide(&ctx);
    let mut b = nevermind_pc::BlocPc::default();
    let ram = ctx.prendre_geste("Couper ce qui est sûr", "RAM").unwrap();
    let ifeo = r"HKLM:\SOFTWARE\MindsetEssaiLotA\IFEO\jeu.exe\PerfOptions";
    let mut e = Effets::default();
    b.signal(
        &ctx,
        "declarations",
        &json!({ "changements": [{ "Type": "reg", "Path": ifeo, "Name": "CpuPriorityClass", "Value": 3 }],
                 "historique": "Profil de Jeu : priorité haute activée" }),
        &mut e,
    );
    assert!(!e.0.iter().any(|x| matches!(x, Effet::Annonce(_))), "le profil a été refusé : {:?}", e.0);
    let mut tout = e.0;
    let fin = Instant::now() + Duration::from_secs(10);
    let attente = Effet::Signal {
        nom: "occupe".into(),
        donnees: json!({ "texte": "« Profil de jeu » se fera juste après « Couper ce qui est sûr » (RAM)..." }),
    };
    while !tout.contains(&attente) {
        assert!(Instant::now() < fin, "l'attente n'est pas montrée : {tout:?}");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(200));
    assert!(demandes.lock().unwrap().is_empty(), "le profil n'a pas attendu son tour");
    drop(ram);
    // Il passe avant un nouveau clic
    let fin = Instant::now() + Duration::from_secs(10);
    while !demandes.lock().unwrap().iter().any(|(a, p)| a == "reg.ecrire" && p["Nom"] == "CpuPriorityClass") {
        assert!(Instant::now() < fin, "le profil n'est pas fait : {:?}", demandes.lock().unwrap());
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        std::thread::sleep(Duration::from_millis(10));
    }
    while b.travail_en_cours() {
        assert!(Instant::now() < fin + Duration::from_secs(5), "le profil ne finit pas");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(ctx.historique().iter().any(|h| h["Titre"] == "Profil de Jeu : priorité haute activée"));
    assert!(ctx.prendre_geste("Nouveau", "Essai").is_ok(), "le verrou est rendu");
    let _ = std::fs::remove_dir_all(&d);
}
