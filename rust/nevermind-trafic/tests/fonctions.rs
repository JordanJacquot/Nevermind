//! Les petites fonctions, rejouées sur les cas exportés de la version PowerShell (outils\exporter-juges-trafic.ps1).

mod commun;

use commun::{difference, juge, verifier};
use nevermind_trafic::lag::{diag, stats};
use nevermind_trafic::serveurs::{self, Annuaire, Entree};
use nevermind_trafic::trafic::{alertes, etat::App, types};
use nevermind_trafic::valeur::{champ, nombre, num, texte};
use nevermind_trafic::{format, ip, microsoft};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn tableau(v: &Value) -> Vec<f64> {
    v.as_array().map(|a| a.iter().map(num).collect()).unwrap_or_default()
}

fn entree(de: &str, a: &str, o: &str, n: &str, c: &str) -> Entree {
    Entree { s: ip::hex(de), e: ip::hex(a), o: o.into(), c: c.into(), n: n.into(), d: "2026-10-22".into() }
}

fn calculer(f: &str, e: &Value) -> Option<Value> {
    Some(match f {
        "Test-PrivateIp" => json!(ip::privee(&texte(e))),
        "ConvertTo-IpHex" => json!(ip::hex(&texte(e))),
        "Format-Bytes" => json!(format::octets(num(e))),
        "Format-Ms" => json!(format::ms(nombre(e))),
        "Format-PlayTime" => json!(format::duree_jeu(num(e))),
        "Format-Country" => json!(format::pays(&texte(e))),
        "Get-OwnerLabel" => {
            json!(serveurs::libelle(&texte(champ(e, "O")), &texte(champ(e, "N")), &texte(champ(e, "C"))))
        }
        "Format-ServerPlace" => json!(diag::lieu_serveur(e)),
        "Get-PingStats" => stats::ping(&tableau(e)).map(|s| s.json()).unwrap_or(Value::Null),
        "Compress-PingSeries" => json!(stats::reduire(&tableau(champ(e, "a")), num(champ(e, "step")))),
        "Get-LagSpikeLinks" => {
            let bg = champ(e, "bg").as_array().cloned().unwrap_or_default();
            // (la version PowerShell nomme parfois le mauvais programme : DIVERGENCES.md ; la version corrigée
            // garde les mêmes nombres de pics)
            let (sp, gw) = (tableau(champ(e, "sp")), tableau(champ(e, "gw")));
            let juste = stats::liens(&sp, &gw, &bg);
            let ps = stats::liens_comme_powershell(&sp, &gw, &bg);
            assert_eq!((juste.local, juste.fond), (ps.local, ps.fond));
            ps.json()
        }
        "Get-SegStatus" => json!(diag::etat_etape(champ(e, "S"), &texte(champ(e, "kind")))),
        "Test-AdresseParticulier" => {
            let p = champ(e, "proprio");
            let (o, n) = (texte(champ(p, "O")), texte(champ(p, "N")));
            let h = champ(e, "hote");
            let r = serveurs::particulier(if p.is_null() { None } else { Some((&o, &n)) }, h.as_str());
            r.map(Value::Bool).unwrap_or(Value::Null)
        }
        "Get-ServerOwner" => {
            let mut a = Annuaire {
                entrees: vec![
                    entree("35.207.0.0", "35.207.255.255", "Google LLC", "GOOGLE-CLOUD", "US"),
                    entree("63.176.0.0", "63.179.255.255", "Amazon.com, Inc.", "AMAZO-4", "US"),
                    entree("2a01:cb00::", "2a01:cbff:ffff:ffff:ffff:ffff:ffff:ffff", "Orange", "FR-ORANGE", "FR"),
                    entree("90.0.0.0", "90.127.255.255", "", "IP2000-ADSL-BAS", "FR"),
                ],
                ..Default::default()
            };
            a.proprietaire(&texte(e)).map(|x| json!(x.libelle())).unwrap_or(Value::Null)
        }
        "Get-DestType" => {
            let p = champ(e, "proprio");
            let (o, n) = (texte(champ(p, "O")), texte(champ(p, "N")));
            let outil = alertes::OUTILS_DISTANCE.is_match(&texte(champ(e, "app")));
            let t = types::sorte(
                &texte(champ(e, "nom")),
                num(champ(e, "port")) as i64,
                num(champ(e, "out")),
                num(champ(e, "in")),
                outil,
                if p.is_null() { None } else { Some((&o, &n)) },
            );
            json!(t.id)
        }
        "Get-MsService" => {
            microsoft::service(&texte(e)).map(|(l, t)| json!({ "Label": l, "Text": t })).unwrap_or(Value::Null)
        }
        "Test-GamePath" => json!(alertes::chemin_jeu(&texte(e), &[])),
        "Test-GamePath+index" => json!(alertes::chemin_jeu(&texte(e), &["c:\\jeu\\x.exe".to_string()])),
        "Test-SuitePath" => json!(alertes::chemin_suite(&texte(e))),
        "Get-AppSigLabel" => {
            let a = App {
                sig: champ(e, "Sig").as_str().map(String::from),
                editeur: texte(champ(e, "Publisher")),
                chemin: texte(champ(e, "Path")),
                ..Default::default()
            };
            let (t, c) = alertes::libelle_signature(&a);
            json!([t, c])
        }
        _ => return None,
    })
}

#[test]
fn petites_fonctions() {
    let cas = juge("fonctions.json");
    let cas = cas.as_array().expect("liste");
    let mut par_fonction: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    for c in cas {
        let f = texte(champ(c, "f"));
        let e = champ(c, "e");
        let attendu = champ(c, "s");
        let x = par_fonction.entry(f.clone()).or_default();
        x.0 += 1;
        match calculer(&f, e) {
            None => x.1.push(format!("{f} : pas porté")),
            Some(obtenu) => {
                if let Some(d) = difference(&obtenu, attendu, "") {
                    x.1.push(format!("{f}({e}) : {d}"));
                }
            }
        }
    }
    let mut toutes = Vec::new();
    let mut total = 0;
    for (f, (n, d)) in par_fonction {
        eprintln!("{f} : {} / {n}", n - d.len());
        total += n;
        toutes.extend(d);
    }
    verifier("Petites fonctions", toutes, total);
}

#[test]
fn diagnostics_du_lag() {
    let cas = juge("diagnostics.json");
    let cas = cas.as_array().expect("liste");
    let mut diffs = Vec::new();
    for (i, c) in cas.iter().enumerate() {
        let r = champ(c, "e");
        let d = diag::diagnostic(r);
        let obtenu = json!({
            "Level": d.niveau, "Title": d.titre,
            "Findings": d.constats.iter().map(|k| json!({
                "Status": k.statut, "Title": k.titre, "Detail": k.detail,
                "Tips": match &k.conseils { Some(t) => json!(t), None => json!([null]) },
                "Actions": k.actions,
            })).collect::<Vec<_>>(),
        });
        if let Some(x) = difference(&obtenu, champ(c, "s"), "") {
            diffs.push(format!("cas {i} : {x}"));
        }
        let ligne = diag::ligne_mesure(r);
        if let Some(x) = difference(&json!(ligne), champ(c, "ligne"), "ligne") {
            diffs.push(format!("cas {i} : {x}"));
        }
    }
    verifier("Diagnostics du lag", diffs, cas.len());
}

#[test]
fn alertes_du_trafic() {
    let cas = juge("alertes.json");
    let cas = cas.as_array().expect("liste");
    let mut diffs = Vec::new();
    for (i, c) in cas.iter().enumerate() {
        let mut apps = Vec::new();
        for a in champ(c, "apps").as_array().cloned().unwrap_or_default() {
            let mut x = App {
                cle: texte(champ(&a, "Key")),
                nom: texte(champ(&a, "Name")),
                chemin: texte(champ(&a, "Path")),
                sig: champ(&a, "Sig").as_str().map(String::from),
                moi: champ(&a, "IsSelf").as_bool().unwrap_or(false),
                out: num(champ(&a, "Out")),
                entrant: num(champ(&a, "In")),
                parent: texte(champ(&a, "Parent")),
                parent_sig: texte(champ(&a, "ParentSig")),
                parent_pub: texte(champ(&a, "ParentPub")),
                ..Default::default()
            };
            for d in champ(&a, "Dest").as_array().cloned().unwrap_or_default() {
                let dist = texte(champ(&d, "Remote"));
                let port = num(champ(&d, "Port")) as i64;
                x.dest.insert(
                    format!("{dist}|{port}"),
                    nevermind_trafic::trafic::etat::Dest {
                        distante: dist,
                        port,
                        privee: champ(&d, "Private").as_bool().unwrap_or(false),
                        ..Default::default()
                    },
                );
            }
            for ipa in champ(&a, "IpsAnciennes").as_array().cloned().unwrap_or_default() {
                x.ips_anciennes.insert(texte(&ipa));
            }
            apps.push(x);
        }
        let confiance = alertes::marques(Some(champ(c, "trusted").clone()));
        let analyses = alertes::marques(Some(champ(c, "scanned").clone()));
        let mut al = alertes::alertes(apps.iter(), &confiance, &analyses, &[]);
        al.sort_by(|a, b| (rang(a.niveau), &a.cle).cmp(&(rang(b.niveau), &b.cle)));
        let obtenu: Vec<Value> =
            al.iter().map(|a| json!({ "Key": a.cle, "Level": a.niveau, "Why": a.pourquoi })).collect();
        let mut attendu = champ(c, "s").as_array().cloned().unwrap_or_default();
        attendu.sort_by_key(|a| (rang(&texte(champ(a, "Level"))), texte(champ(a, "Key"))));
        if let Some(x) = difference(&json!(obtenu), &json!(attendu), "") {
            diffs.push(format!("cas {i} : {x}"));
        }
    }
    verifier("Alertes du trafic", diffs, cas.len());
}

fn rang(n: &str) -> u8 {
    match n {
        "bad" => 0,
        "warn" => 1,
        _ => 2,
    }
}

/// M5 (audit du 08/11) : chaque valeur de HKCU que « Couper » écrit est dans la liste fermée du noyau, sinon « Tout
/// restaurer » et le désinstalleur refuseraient de la remettre comme avant
#[test]
fn ce_que_couper_ecrit_dans_hkcu_est_dans_la_liste_fermee() {
    struct Rien;
    impl microsoft::LecturesWindows for Rien {
        fn reg_nombre(&self, _: &str, _: &str) -> Option<i64> {
            Some(1)
        }
        fn reg_texte(&self, _: &str, _: &str) -> String {
            String::new()
        }
        fn service(&self, _: &str) -> Option<(bool, String)> {
            Some((true, "Automatic".into()))
        }
        fn compte_microsoft(&self) -> bool {
            true
        }
    }
    let mut n = 0;
    for r in microsoft::reglages(&Rien) {
        for e in &r.couper {
            if let microsoft::Ecriture::Registre { chemin, nom, .. } = e
                && nevermind_noyau::registre::utilisateur(chemin)
            {
                n += 1;
                assert!(nevermind_noyau::registre::utilisateur_permis(chemin, nom), "{chemin}|{nom} hors de la liste");
            }
        }
    }
    assert!(n >= 14, "{n}");
}
