//! La lecture du vrai PC, en lecture seule (WMI, registre, journaux, écrans) : à lancer à la main pour comparer avec
//! la version PowerShell (`cargo test -p nevermind-pc --test lecture_reelle -- --ignored --nocapture`). Rien n'est écrit.

#[test]
#[ignore]
fn lecture_du_vrai_pc() {
    let t = std::time::Instant::now();
    let lu = nevermind_pc::lecture::lire();
    let d = &lu.entrees.data;
    let mut r = serde_json::Map::new();
    for k in [
        "OS",
        "CPU",
        "GPUs",
        "Mem",
        "MemDiag",
        "SysDisk",
        "LogicalC",
        "BaseBoard",
        "BIOS",
        "Bsod",
        "Crash",
        "WheaErr",
        "WheaWarn",
        "Net",
        "Chassis",
        "PCType",
        "Battery",
        "UpsHints",
    ] {
        r.insert(k.into(), d.get(k).cloned().unwrap_or_default());
    }
    let disques: Vec<serde_json::Value> = nevermind_pc::net::champ_liste(d, "Disks")
        .iter()
        .map(|x| serde_json::json!({ "Disk": x["Disk"], "Letters": x["Letters"], "Vols": x["Vols"] }))
        .collect();
    r.insert("Disks".into(), serde_json::json!(disques));
    r.insert("uefi".into(), serde_json::json!(lu.entrees.uefi));
    r.insert("vram".into(), serde_json::json!(lu.entrees.vram));
    r.insert(
        "demarrage".into(),
        serde_json::json!(
            lu.demarrage.iter().map(|x| format!("{} | {} | {}", x.nom, x.source, x.exe)).collect::<Vec<_>>()
        ),
    );
    println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(r)).unwrap());
    println!("lecture en {} ms", t.elapsed().as_millis());
}

/// Le dernier pilote NVIDIA demandé à NVIDIA (curl de Windows, sans fenêtre ; rien n'est écrit) : à lancer à la main
/// (`cargo test -p nevermind-pc --test lecture_reelle dernier_pilote_nvidia -- --ignored --nocapture`)
#[test]
#[ignore]
fn dernier_pilote_nvidia() {
    use nevermind_pc::pc::Pc;
    let d = nevermind_noyau::dossier_essai("pc-nvidia-reel");
    let ctx = nevermind_noyau::Contexte::nouveau(d.clone(), d.clone(), false, false);
    let mut pc = nevermind_pc::systeme::PcWindows::nouveau(&ctx);
    let r = pc.reponse_pilote_nvidia();
    let lu = r.as_deref().and_then(nevermind_pc::systeme::pilote_nvidia_depuis_reponse);
    println!("{lu:?}");
    assert!(lu.is_some(), "pas de réponse de NVIDIA : {r:?}");
    let _ = std::fs::remove_dir_all(&d);
}
