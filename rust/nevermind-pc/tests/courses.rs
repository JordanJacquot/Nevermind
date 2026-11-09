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
    let attente = "« Profil de jeu » se fera juste après « Couper ce qui est sûr » (RAM)...";
    let montre =
        |x: &Effet| matches!(x, Effet::Signal { nom, donnees } if nom == "occupe" && donnees["texte"] == attente);
    while !tout.iter().any(montre) {
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

// -----------------------------------------------------------------------------------------------------------------
// Étape 2 : un seul écrivain de sauvegarde.json, qui relit le fichier et y fusionne seulement ce que le geste a changé
// -----------------------------------------------------------------------------------------------------------------

/// Le contenu de sauvegarde.json
fn fichier_sauvegarde(ctx: &Contexte) -> Value {
    let t = std::fs::read_to_string(ctx.fichier_sauvegarde()).unwrap_or_default();
    serde_json::from_str(t.trim_start_matches('\u{feff}')).unwrap_or(Value::Null)
}

/// G-A, le cas DNS de l'audit : Ton PC a pris sa copie de la sauvegarde au début d'« Appliquer » ; pendant le point de
/// restauration (verrou rendu, ajustement B), le Réseau note le DNS d'origine ; Ton PC note ensuite sa valeur et
/// écrit. Avant : sa copie écrasait le fichier, le DNS disparaissait pour toujours. Maintenant : le fichier relu, seul
/// ce que Ton PC a changé y est ajouté ; le DNS reste, dans le fichier, dans le noyau et dans la copie de Ton PC.
#[test]
fn la_copie_de_ton_pc_n_efface_plus_le_dns_note_pendant_son_travail() {
    let d = dossier("sauvegarde-dns");
    let ctx = Contexte::nouveau(d.join("a"), d.join("d"), false, true);
    let _ = fausse_aide(&ctx);
    // Au début du travail de Ton PC : sa copie
    let mut copie_pc = Sauvegarde::depuis(&fichier_sauvegarde(&ctx));
    // Pendant le point de restauration : « Changer le DNS » (Réseau)
    {
        let _g = ctx.prendre_geste("Changer le DNS", "Réseau").unwrap();
        ctx.changer_sauvegarde(|s| {
            s.dns.insert("{GUID-CARTE}".into(), json!("192.168.1.1"));
        });
        ctx.enregistrer_sauvegarde().unwrap();
    }
    // La suite d'« Appliquer » : une valeur d'origine notée, puis l'écriture
    let _g = ctx.prendre_geste("Appliquer", "Ton PC").unwrap();
    let mut p = PcWindows::nouveau(&ctx);
    {
        let mut m = Moteur::nouveau(&mut p, &mut copie_pc, false);
        m.noter_origine(r"HKLM:\SOFTWARE\MindsetEssaiLotA", "X").unwrap();
    }
    let f = fichier_sauvegarde(&ctx);
    assert_eq!(f["Dns"]["{GUID-CARTE}"], "192.168.1.1", "le DNS d'origine a disparu du fichier : {f}");
    assert!(f["Registry"].get(r"HKLM:\SOFTWARE\MindsetEssaiLotA|X").is_some(), "la valeur de Ton PC manque : {f}");
    assert!(ctx.sauvegarde().dns.contains_key("{GUID-CARTE}"), "le DNS d'origine a disparu du noyau");
    assert!(copie_pc.dns.contains_key("{GUID-CARTE}"), "la copie de Ton PC ne voit pas le DNS noté entre-temps");
    let _ = std::fs::remove_dir_all(&d);
}

/// À la fusion, la valeur d'origine la plus ancienne gagne : une copie prise avant que le Réseau note le DNS d'origine
/// (« 192.168.1.1 ») ne le remplace jamais par ce qu'elle croit être l'origine (le DNS déjà changé).
#[test]
fn la_valeur_d_origine_la_plus_ancienne_gagne() {
    let d = dossier("sauvegarde-ancienne");
    let ctx = Contexte::nouveau(d.join("a"), d.join("d"), false, true);
    let _ = fausse_aide(&ctx);
    let mut copie_pc = Sauvegarde::depuis(&fichier_sauvegarde(&ctx));
    {
        let _g = ctx.prendre_geste("Changer le DNS", "Réseau").unwrap();
        ctx.changer_sauvegarde(|s| {
            s.dns.insert("{GUID-CARTE}".into(), json!("192.168.1.1"));
        });
        ctx.enregistrer_sauvegarde().unwrap();
    }
    let _g = ctx.prendre_geste("Annuler", "Ton PC").unwrap();
    let mut p = PcWindows::nouveau(&ctx);
    {
        let mut m = Moteur::nouveau(&mut p, &mut copie_pc, false);
        m.sv.dns.insert("{GUID-CARTE}".into(), json!("1.1.1.1"));
        m.sauver().unwrap();
    }
    let f = fichier_sauvegarde(&ctx);
    assert_eq!(f["Dns"]["{GUID-CARTE}"], "192.168.1.1", "la plus ancienne valeur d'origine a été écrasée : {f}");
    assert_eq!(ctx.sauvegarde().dns.get("{GUID-CARTE}"), Some(&json!("192.168.1.1")));
    let _ = std::fs::remove_dir_all(&d);
}

/// G-A, le cas des services : « Tout restaurer » lit SvcOriginal au début, remet chaque service par l'Aide (jusqu'à
/// 120 s chacun), puis écrivait SvcOriginal avec seulement ses échecs. Un service noté entre-temps disparaissait. La
/// fausse Aide le note pendant que « Tout restaurer » attend (ce qui se passerait sans le verrou) : il reste noté.
#[test]
fn tout_restaurer_garde_un_service_note_pendant_son_travail() {
    let d = dossier("sauvegarde-services");
    let ctx = Contexte::nouveau(d.join("a"), d.join("d"), false, true).partage();
    {
        let _g = ctx.prendre_geste("Mise en place", "Essai").unwrap();
        ctx.ecrire_reglage("SvcOriginal", json!(["ServiceA|Automatic|True"]), true).unwrap();
    }
    let c = Arc::downgrade(&ctx);
    ctx.fausse_aide(Arc::new(move |a: &str, p: &Value| {
        if a == "service.regler"
            && p["Nom"] == "ServiceA"
            && let Some(c) = c.upgrade()
        {
            // (pendant ce temps : « Couper ce qui est sûr » note un autre service)
            let mut l = c.reglage("SvcOriginal").and_then(|v| v.as_array().cloned()).unwrap_or_default();
            l.push(json!("AdobeARMservice|Automatic|True"));
            c.ecrire_reglage("SvcOriginal", Value::Array(l), true).unwrap();
        }
        Ok(json!("OK"))
    }));
    let _g = ctx.prendre_geste("Tout restaurer", "Ton PC").unwrap();
    let mut p = PcWindows::nouveau(&ctx);
    let mut sv = Sauvegarde::default();
    let erreurs = {
        let mut m = Moteur::nouveau(&mut p, &mut sv, false);
        nevermind_pc::corrections::tout_restaurer(&mut m)
    };
    assert!(erreurs.is_empty(), "{erreurs:?}");
    assert_eq!(
        ctx.reglage("SvcOriginal"),
        Some(json!(["AdobeARMservice|Automatic|True"])),
        "le service remis est retiré, celui noté pendant ce temps est gardé"
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// Un seul écrivain : Ton PC et le noyau écrivent sauvegarde.json au même moment, chacun depuis ce qu'il sait. Avant,
/// ils passaient par le même `sauvegarde.json.tmp` sans verrou (une écriture pouvait échouer) et la copie de Ton PC
/// effaçait ce que le noyau venait de noter. Maintenant : aucune erreur, et tout ce qui a été noté est dans le fichier.
#[test]
fn deux_ecrivains_au_meme_moment_ne_perdent_rien() {
    let d = dossier("sauvegarde-ecrivains");
    let ctx = Contexte::nouveau(d.join("a"), d.join("d"), false, true).partage();
    let _ = fausse_aide(&ctx);
    const N: usize = 25;
    let c1 = ctx.clone();
    let pc = std::thread::spawn(move || {
        let _g = c1.prendre_geste("Appliquer", "Ton PC").unwrap();
        let mut copie = Sauvegarde::default();
        let mut p = PcWindows::nouveau(&c1);
        let mut m = Moteur::nouveau(&mut p, &mut copie, false);
        let mut erreurs = Vec::new();
        for i in 0..N {
            let e = json!({ "Path": "HKLM:\\Essai", "Name": format!("pc{i}"), "Existed": false });
            m.sv.registre.insert(format!("HKLM:\\Essai|pc{i}"), e);
            if let Err(e) = m.sauver() {
                erreurs.push(e);
            }
        }
        erreurs
    });
    let c2 = ctx.clone();
    let noyau = std::thread::spawn(move || {
        let _g = c2.geste_prioritaire("Essai", "Essai");
        let mut erreurs = Vec::new();
        for i in 0..N {
            c2.changer_sauvegarde(|s| {
                s.dns.insert(format!("carte{i}"), json!("9.9.9.9"));
            });
            if let Err(e) = c2.enregistrer_sauvegarde() {
                erreurs.push(e);
            }
        }
        erreurs
    });
    let (e1, e2) = (pc.join().unwrap(), noyau.join().unwrap());
    assert!(e1.is_empty() && e2.is_empty(), "des écritures ont échoué : {e1:?} {e2:?}");
    let f = fichier_sauvegarde(&ctx);
    let manquent: Vec<String> = (0..N)
        .map(|i| format!("pc{i}"))
        .filter(|k| f["Registry"].get(format!("HKLM:\\Essai|{k}")).is_none())
        .chain((0..N).map(|i| format!("carte{i}")).filter(|k| f["Dns"].get(k).is_none()))
        .collect();
    assert!(manquent.is_empty(), "perdu dans sauvegarde.json : {manquent:?}");
    let restes: Vec<String> = std::fs::read_dir(&ctx.donnees)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(restes.is_empty(), "fichiers temporaires laissés : {restes:?}");
    let _ = std::fs::remove_dir_all(&d);
}

// -----------------------------------------------------------------------------------------------------------------
// Étape 3 : les petits liés
// -----------------------------------------------------------------------------------------------------------------

/// Une panique dans un geste de Ton PC ne jette plus ce qu'il avait noté : la sauvegarde de Ton PC est relue (avant,
/// elle gardait sa copie d'avant le geste, sans la valeur d'origine déjà notée) ; « sauvegarde-changee » est dit aux
/// autres blocs ; et le verrou est rendu (ajustement C) : un NOUVEAU geste passe ensuite.
#[test]
fn une_panique_dans_un_geste_de_ton_pc_ne_jette_pas_ce_qu_il_a_note() {
    let d = dossier("panique");
    let ctx = Contexte::nouveau(d.join("a"), d.join("d"), false, true).partage();
    ctx.fausse_aide(Arc::new(move |a: &str, p: &Value| {
        if a == "reg.ecrire" && p["Nom"] == "Deux" {
            panic!("erreur imprévue d'essai");
        }
        Ok(json!("OK"))
    }));
    let mut b = nevermind_pc::BlocPc::default();
    let cle = r"HKLM:\SOFTWARE\MindsetEssaiLotA\Panique";
    let mut e = Effets::default();
    b.signal(
        &ctx,
        "declarations",
        &json!({ "changements": [
            { "Type": "reg", "Path": cle, "Name": "Un", "Value": 1 },
            { "Type": "reg", "Path": cle, "Name": "Deux", "Value": 2 }
        ], "historique": "Profil de Jeu : priorité haute activée" }),
        &mut e,
    );
    let mut tout = e.0;
    let fin = Instant::now() + Duration::from_secs(10);
    while b.travail_en_cours() {
        assert!(Instant::now() < fin, "le geste ne finit pas");
        let mut e = Effets::default();
        b.travaux_finis(&ctx, &mut e);
        tout.extend(e.0);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        tout.iter().any(|x| matches!(x, Effet::Annonce(t) if t.contains("erreur imprévue"))),
        "la panique n'est pas dite : {tout:?}"
    );
    let k = format!("{cle}|Un");
    assert!(ctx.sauvegarde().registre.contains_key(&k), "la valeur d'origine n'est pas dans sauvegarde.json");
    assert!(b.session().sv.registre.contains_key(&k), "Ton PC a jeté ce que le geste avait noté");
    assert!(
        tout.iter().any(|x| matches!(x, Effet::Signal { nom, .. } if nom == "sauvegarde-changee")),
        "les autres blocs ne savent pas que la sauvegarde a changé : {tout:?}"
    );
    assert!(ctx.prendre_geste("Nouveau", "Essai").is_ok(), "après la panique, un nouveau geste ne passe pas");
    let _ = std::fs::remove_dir_all(&d);
}

/// Petit du lot A (reste de M9) : « Tout restaurer » marquait annulées les lignes « pare-feu » de l'historique même
/// quand le déblocage avait échoué (systeme.rs:519) : plus de « Annuler » pour un programme toujours bloqué.
/// Maintenant elles restent à annuler ; une ligne bien remise est marquée annulée.
#[test]
fn tout_restaurer_garde_les_lignes_pare_feu_si_le_deblocage_echoue() {
    let d = dossier("pare-feu");
    let ctx = Contexte::nouveau(d.join("a"), d.join("d"), false, true);
    ctx.fausse_aide(Arc::new(|a: &str, _p: &Value| {
        if a == "pare-feu.debloquer-tout" {
            return Err("refusé par Windows".into());
        }
        Ok(json!("OK"))
    }));
    let _g = ctx.prendre_geste("Tout restaurer", "Ton PC").unwrap();
    let cle = r"HKLM:\SOFTWARE\MindsetEssaiLotA";
    ctx.changer_registre(cle, "X", &json!(1), "DWord").unwrap();
    ctx.ajouter_historique(
        "Réglage X",
        &[],
        vec![json!({ "Type": "reg", "Path": cle, "Name": "X", "Existed": false })],
    );
    ctx.ajouter_historique(
        "Internet bloqué pour Jeu",
        &[],
        vec![json!({ "Type": "fw", "Name": "Nevermind : bloque Jeu" })],
    );
    let mut p = PcWindows::nouveau(&ctx).avec_regles_pare_feu_essai(1);
    let mut s =
        nevermind_pc::session::Session { sv: Sauvegarde::depuis(&fichier_sauvegarde(&ctx)), ..Default::default() };
    s.restaurer_tout(&mut p, &[]);
    s.repondre(&mut p, true);
    let h = ctx.historique();
    let ligne = |t: &str| h.iter().find(|x| x["Titre"] == t).cloned().unwrap_or(Value::Null);
    assert_eq!(ligne("Internet bloqué pour Jeu")["Annule"], false, "la règle du pare-feu est toujours là : {h:?}");
    assert_eq!(ligne("Réglage X")["Annule"], true, "la ligne bien remise est marquée annulée : {h:?}");
    let _ = std::fs::remove_dir_all(&d);
}
