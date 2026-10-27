//! Les demandes, une fois vérifiées par `aide_regles::demandes::verifier`, faites avec les appels de Windows
//! (`Invoke-AideAction` sans `-Verifier`, aide-actions.ps1). Rend le résultat que l'Aide renvoie à Nevermind en JSON,
//! ou le message d'erreur montré tel quel.

use crate::mesures::Mesures;
use crate::systeme::{ErreurEffacement, Systeme, ValeurLue};
use aide_regles::demandes::{self, ContexteProgrammes, Demande, REGLE, REGLE_ANCIENNE, VERSION};
use aide_regles::fermeture::{self, Contexte};
use aide_regles::nettoyage::{self, Categorie};
use aide_regles::registre::{self, Ruche, Valeur};
use aide_regles::{restauration, taches_compte, texte};
use serde_json::{Value, json};

/// Ce que l'Aide retient elle-même (HKLM, modifiable seulement en administrateur)
const CLE_AIDE: &str = "SOFTWARE\\Mindset\\Nevermind\\Aide";
/// Valeurs d'origine des réglages, gardées avant leur première modification (une seconde copie de sauvegarde.json)
const CLE_ORIGINES: &str = "SOFTWARE\\Mindset\\Nevermind\\Aide\\Origines";
/// Serveurs DNS d'origine de chaque carte
const CLE_DNS: &str = "SOFTWARE\\Mindset\\Nevermind\\Aide\\Dns";

/// Ce que l'Aide sait d'elle-même, pour « aide.bonjour »
pub struct EtatAide {
    pub admin: bool,
    pub compte: String,
    pub pid: u32,
    /// Le dossier aide-temp, « \ » final compris (vide s'il n'a pas pu être préparé)
    pub temp: String,
    /// Les mesures continues en cours (PresentMon, trace réseau, compteurs TCP)
    pub mesures: Mesures,
}

/// Fait la demande. `client` : le processus qui la fait (Nevermind), lu par Windows sur le canal (0 : inconnu).
pub fn faire(d: Demande, s: &dyn Systeme, client: u32, etat: &EtatAide) -> Result<Value, String> {
    let ok = || Ok(json!("ok"));
    match d {
        Demande::Bonjour => Ok(json!({
            "Version": VERSION, "Admin": etat.admin, "Pid": etat.pid, "Compte": etat.compte, "Temp": etat.temp
        })),

        Demande::RegEcrire { cle, nom, valeur } => {
            let (ruche, sous) = registre::decouper(&cle).ok_or_else(|| format!("Clé non prise en charge : {cle}"))?;
            garder_origine(s, &cle, &nom);
            let v = match valeur {
                Valeur::DWord(n) => ValeurLue::DWord(n as u32),
                Valeur::Binaire(o) => ValeurLue::Binaire(o),
            };
            s.reg_ecrire(ruche, sous, &nom, &v)?;
            ok()
        }

        Demande::RegEffacer { cle, nom } => {
            let (ruche, sous) = registre::decouper(&cle).ok_or_else(|| format!("Clé non prise en charge : {cle}"))?;
            garder_origine(s, &cle, &nom);
            s.reg_effacer(ruche, sous, &nom)?;
            ok()
        }

        Demande::ServiceRegler { nom, demarrage, lancer, arreter } => {
            s.service_regler(&nom, demarrage, lancer, arreter)?;
            ok()
        }

        Demande::DnsRegler { carte, serveurs, origine_connue } => {
            if !origine_connue {
                // Premier changement sur cette carte : l'Aide retient les serveurs d'origine (seule elle peut les remettre)
                let avant = match s.reg_lire(Ruche::Machine, &tcpip(&carte.guid), "NameServer") {
                    Some(ValeurLue::Texte(t)) | Some(ValeurLue::TexteEtendu(t)) => t,
                    Some(ValeurLue::Multi(l)) => l.join(" "),
                    _ => String::new(),
                };
                s.reg_ecrire(Ruche::Machine, CLE_DNS, &carte.guid, &ValeurLue::Texte(avant))?;
            }
            s.dns_appliquer(&carte, &serveurs)?;
            s.dns_vider_cache();
            ok()
        }

        // La carte de la connexion, éteinte puis rallumée par Windows (Restart-NetAdapter) : elle renégocie sa vitesse
        Demande::CarteRelancer { index } => {
            s.carte_relancer(index).map_err(|e| format!("Windows n'a pas pu relancer la carte réseau : {e}"))?;
            ok()
        }

        Demande::PareFeuBloquer { nom, programme, services } => {
            let cibles: Vec<Option<&str>> =
                if services.is_empty() { vec![None] } else { services.iter().map(|x| Some(x.as_str())).collect() };
            for service in cibles {
                for entrant in [false, true] {
                    s.pare_feu_ajouter(&nom, &programme, service, entrant)?;
                }
            }
            ok()
        }

        Demande::PareFeuDebloquer { suite } => {
            // (un blocage noté sous l'ancien nom a pu être renommé depuis : les deux noms sont cherchés)
            let noms = [format!("{REGLE}{suite}"), format!("{REGLE_ANCIENNE}{suite}")];
            for r in regles_nevermind(s)? {
                if noms.iter().any(|n| texte::egal(n, &r)) {
                    s.pare_feu_retirer(&r)?;
                }
            }
            ok()
        }

        Demande::PareFeuDebloquerTout => {
            for r in regles_nevermind(s)? {
                s.pare_feu_retirer(&r)?;
            }
            ok()
        }

        Demande::OriginesOublier => {
            // (après une restauration complète : ces valeurs ne doivent pas resservir des mois plus tard)
            let _ = s.reg_effacer_arbre(Ruche::Machine, CLE_ORIGINES);
            ok()
        }

        Demande::PareFeuActiver => {
            s.pare_feu_activer()?;
            ok()
        }

        Demande::RestaurationCreer { description } => {
            // Un par 24 h au plus : déjà un de moins de 24 h, c'est lui qui sert (« recent »)
            let derniere = match s.reg_lire(Ruche::Machine, CLE_AIDE, "DerniereRestauration") {
                Some(ValeurLue::Texte(t)) => aide_regles::ps::lire_entier(&t).unwrap_or(0),
                Some(ValeurLue::QWord(n)) => n as i64,
                Some(ValeurLue::DWord(n)) => i64::from(n),
                _ => 0,
            };
            if restauration::recente(derniere, s.ticks_utc()) {
                return Ok(json!("recent"));
            }
            s.point_restauration(&description)?;
            let maintenant = ValeurLue::Texte(s.ticks_utc().to_string());
            s.reg_ecrire(Ruche::Machine, CLE_AIDE, "DerniereRestauration", &maintenant)?;
            ok()
        }

        Demande::AntivirusRetirerExclusions { elements } => {
            for x in &elements {
                s.retirer_exclusion(x);
            }
            ok()
        }

        Demande::LecturePc => Ok(s.lecture_pc()),
        Demande::LectureSecurite => Ok(s.lecture_securite()),

        Demande::LectureTaches => {
            // Seulement celles du compte qui demande et des comptes du système, jamais celles des autres comptes
            if client == 0 {
                return Err("Demande refusée : programme qui fait la demande inconnu.".into());
            }
            let sid_client = s.fiche(i64::from(client), "").compte;
            if sid_client.is_empty() {
                return Err("Demande refusée : compte du programme qui fait la demande illisible.".into());
            }
            let traduire = |n: &str| s.traduire_compte(n);
            let mut r = Vec::new();
            for t in s.taches() {
                if texte::commence(&t.dossier, "\\Microsoft\\") || t.desactivee {
                    continue;
                }
                if !taches_compte::montree(&t.compte, &t.groupe, &sid_client, &traduire) {
                    continue;
                }
                for (exe, args) in &t.actions {
                    r.push(json!({ "Nom": t.nom, "Chemin": t.dossier, "Exe": exe, "Args": args }));
                }
            }
            Ok(Value::Array(r))
        }

        Demande::LectureChemins { pids } => {
            let mut r = serde_json::Map::new();
            for (pid, chemin) in s.chemins_processus() {
                if pids.contains(&pid) && !chemin.is_empty() {
                    r.insert(pid.to_string(), Value::String(chemin));
                }
            }
            Ok(Value::Object(r))
        }

        Demande::WifiManuel { profils } => {
            for p in &profils {
                s.wifi_manuel(p);
            }
            ok()
        }

        Demande::Smb1Couper { client } => {
            s.smb1_couper(client);
            ok()
        }

        Demande::NettoyageLister { categorie, top } => Ok(lister(s, categorie, top)),
        Demande::NettoyageLancer { categorie } => Ok(nettoyer(s, categorie)),

        Demande::ProcessusFermer { nom, chemin, contexte } => fermer(s, client, &nom, chemin.as_deref(), contexte),

        // --- Mesures en continu : l'Aide les fait tourner, Nevermind lit les résultats
        // FPS d'un jeu : PresentMon (celui du dossier de Nevermind) sur ce processus seulement ; ses lignes sont
        // relayées telles quelles
        Demande::FpsDemarrer { pid } => {
            let m = &etat.mesures;
            // Jeu guetté par la veille (lancée avant lui) : la mesure continue sur celle-là
            let nom = s.processus().into_iter().find(|p| p.pid == pid).map(|p| nom_processus(&p.nom));
            if let Some(nom) = nom
                && m.fps.rattacher(&nom)
            {
                return Ok(json!("rattachee"));
            }
            let pm = s.presentmon()?;
            m.fps.demarrer(s, &pm, pid)?;
            ok()
        }
        // Veille avant le lancement de jeux dont l'anti-triche refuse une mesure démarrée après eux
        Demande::FpsVeiller { noms } => {
            let pm = s.presentmon()?;
            etat.mesures.fps.veiller(s, &pm, &noms)?;
            ok()
        }
        Demande::FpsLire => Ok(json!(etat.mesures.fps.lire())),
        Demande::FpsArreter => {
            etat.mesures.fps.arreter(s);
            ok()
        }

        // Connexions TCP avec les octets envoyés et reçus de chacune (Windows ne compte qu'à la demande d'un
        // administrateur)
        Demande::TraficTcp => {
            let mut c = etat.mesures.tcp.lock().unwrap_or_else(|e| e.into_inner());
            let lignes = c.lignes(s);
            Ok(json!({ "Lignes": lignes, "CompteursOk": c.ok }))
        }

        // Trace réseau de Windows (chaque échange, UDP compris) : le vrai serveur du jeu pendant « Lag en ligne »
        Demande::FluxDemarrer => {
            etat.mesures.flux.demarrer(s).map_err(trace_refusee)?;
            ok()
        }
        // Veille avant un jeu dont l'anti-triche refuse une trace démarrée après lui : la trace tourne déjà quand la
        // partie commence (flux.demarrer la reprend). Redemandée toutes les 30 s : les vieilles connexions sont oubliées.
        Demande::FluxVeiller => {
            let f = &etat.mesures.flux;
            if f.tourne() {
                f.oublier(s, 120);
            } else {
                f.demarrer(s).map_err(trace_refusee)?;
            }
            ok()
        }
        Demande::FluxLire => {
            let f = &etat.mesures.flux;
            if !f.tourne() {
                return Err("La trace réseau ne tourne pas.".into());
            }
            Ok(json!(f.lignes(s)))
        }
        Demande::FluxArreter => {
            etat.mesures.flux.arreter();
            ok()
        }
    }
}

/// Le refus de Windows de démarrer la trace, tel que Nevermind l'affiche
fn trace_refusee(e: String) -> String {
    format!("Trace réseau refusée par Windows : {e}")
}

/// Le nom d'un programme tel que .NET le donne pour un processus, plus « .exe » (« bf6.exe ») : `ProcessName` retire
/// « .exe » (quelle que soit la casse), puis la version PowerShell le remet en minuscules
fn nom_processus(nom: &str) -> String {
    let court = if texte::finit(nom, ".exe") { &nom[..nom.len() - 4] } else { nom };
    format!("{court}.exe")
}

/// La clé des réglages TCP/IP d'une carte
fn tcpip(guid: &str) -> String {
    format!("SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces\\{{{guid}}}")
}

/// Les règles du pare-feu créées par Nevermind (« Nevermind : bloque ... », ou l'ancien nom avant la 1.3.6)
fn regles_nevermind(s: &dyn Systeme) -> Result<Vec<String>, String> {
    let mut r: Vec<String> = s.pare_feu_regles()?.into_iter().filter(|n| demandes::suite_regle(n).is_some()).collect();
    r.sort();
    r.dedup();
    Ok(r)
}

/// Une fois, au démarrage de l'Aide : les anciennes règles prennent le nom de Nevermind
pub fn renommer_anciennes_regles(s: &dyn Systeme) -> Result<(), String> {
    for r in s.pare_feu_regles()? {
        if let Some(suite) = texte::reste_apres(&r, REGLE_ANCIENNE) {
            let _ = s.pare_feu_renommer(&r, &format!("{REGLE}{suite}"));
        }
    }
    Ok(())
}

/// Garde la valeur d'origine d'un réglage avant sa première modification (`Save-AideOrigine`). Rien n'arrête la
/// demande si ça ne marche pas.
fn garder_origine(s: &dyn Systeme, cle: &str, nom: &str) {
    let id = format!("{cle}|{nom}");
    if s.reg_lire(Ruche::Machine, CLE_ORIGINES, &id).is_some() {
        return; // (la toute première valeur seulement)
    }
    let Some((ruche, sous)) = registre::decouper(cle) else { return };
    let (existe, valeur, sorte) = match s.reg_lire(ruche, sous, nom) {
        None => (false, Value::Null, Value::Null),
        Some(v) => {
            let (valeur, sorte) = match v {
                ValeurLue::Texte(t) => (json!(t), "String"),
                ValeurLue::TexteEtendu(t) => (json!(t), "ExpandString"),
                ValeurLue::Binaire(o) => (json!(o), "Binary"),
                // (comme .NET : un DWORD est un entier signé)
                ValeurLue::DWord(n) => (json!(n as i32), "DWord"),
                ValeurLue::QWord(n) => (json!(n as i64), "QWord"),
                ValeurLue::Multi(l) => (json!(l), "MultiString"),
                ValeurLue::Autre(_, o) => (json!(o), "Unknown"),
            };
            (true, valeur, json!(sorte))
        }
    };
    // Mêmes champs, dans le même ordre, que la version PowerShell (Nevermind les relit)
    let texte = format!(
        "{{\"Path\":{},\"Name\":{},\"Existed\":{},\"Value\":{},\"Kind\":{}}}",
        json!(cle),
        json!(nom),
        existe,
        valeur,
        sorte
    );
    let _ = s.reg_ecrire(Ruche::Machine, CLE_ORIGINES, &id, &ValeurLue::Texte(texte));
}

/// Un fichier à nettoyer
struct AEffacer {
    chemin: String,
    taille: u64,
    ecrit: i64,
    lecture_seule: bool,
}

/// Les fichiers d'un dossier et de ses sous-dossiers, sans suivre les liens : on ne sort jamais du dossier prévu.
/// Seulement les fichiers de plus de 24 h (une installation ou un programme en cours peut encore s'en servir).
fn parcourir(s: &dyn Systeme, dossier: &str, avant: i64, fichiers: &mut Vec<AEffacer>, dossiers: &mut Vec<String>) {
    for e in s.entrees(dossier) {
        if e.dossier {
            if e.lien {
                continue;
            }
            dossiers.push(e.chemin.clone());
            parcourir(s, &e.chemin, avant, fichiers, dossiers);
        } else if e.ecrit < avant {
            fichiers.push(AEffacer {
                chemin: e.chemin,
                taille: e.taille,
                ecrit: e.ecrit,
                lecture_seule: e.lecture_seule,
            });
        }
    }
}

/// Analyse d'une catégorie : nombre de fichiers, taille totale et les plus gros (pour « Voir les fichiers »)
fn lister(s: &dyn Systeme, c: Categorie, top: usize) -> Value {
    let avant = s.ticks_utc() - 24 * restauration::TICKS_HEURE;
    let mut fichiers = Vec::new();
    for d in nettoyage::dossiers(c, &s.dossiers().windows) {
        if s.dossier_existe(&d) {
            parcourir(s, &d, avant, &mut fichiers, &mut Vec::new());
        }
    }
    let total = fichiers.iter().fold(0f64, |t, f| t + f.taille as f64);
    let nombre = fichiers.len();
    fichiers.sort_by_key(|a| std::cmp::Reverse(a.taille));
    // Dates en ticks (heure locale, comme LastWriteTime de .NET) : elles traversent le JSON sans changer de forme
    let liste: Vec<Value> =
        fichiers.iter().take(top).map(|f| json!([f.chemin, f.taille as f64, s.ticks_local(f.ecrit)])).collect();
    json!({ "Size": total, "Count": nombre, "Top": liste })
}

/// Nettoyage d'une catégorie, fichier par fichier, avec une ligne de journal pour chacun
fn nettoyer(s: &dyn Systeme, c: Categorie) -> Value {
    let avant = s.ticks_utc() - 24 * restauration::TICKS_HEURE;
    let mut journal = Vec::new();
    let (mut libere, mut effaces, mut laisses, mut erreurs) = (0f64, 0u64, 0u64, 0u64);
    for d in nettoyage::dossiers(c, &s.dossiers().windows) {
        if !s.dossier_existe(&d) {
            continue;
        }
        let mut fichiers = Vec::new();
        let mut dossiers = Vec::new();
        parcourir(s, &d, avant, &mut fichiers, &mut dossiers);
        for f in &fichiers {
            let taille = nettoyage::taille(f.taille as f64);
            match s.effacer_fichier(&f.chemin, f.lecture_seule) {
                Ok(()) => {
                    libere += f.taille as f64;
                    effaces += 1;
                    journal.push(format!("[SUPPRIMÉ] {} - {taille}", f.chemin));
                }
                Err(ErreurEffacement::Refuse) => {
                    erreurs += 1;
                    journal.push(format!("[REFUSÉ]   {} - {taille} - Accès refusé", f.chemin));
                }
                Err(ErreurEffacement::Utilise) => {
                    laisses += 1;
                    journal.push(format!("[LAISSÉ]   {} - {taille} - Fichier utilisé par un programme", f.chemin));
                }
            }
        }
        // Dossiers restés vides : du plus profond au moins profond (le dossier de départ reste)
        dossiers.sort_by_key(|x| std::cmp::Reverse(x.chars().count()));
        for x in &dossiers {
            s.effacer_dossier_vide(x);
        }
    }
    json!({ "Freed": libere, "Deleted": effaces, "Skipped": laisses, "Errors": erreurs, "Log": journal })
}

/// Ferme les processus de ce programme (`processus.fermer`) : seulement un programme de l'utilisateur, dans sa
/// session (la règle « fermable »), jamais un service, l'antivirus, un autre compte. Rend le nombre de fermés.
fn fermer(
    s: &dyn Systeme,
    client: u32,
    nom: &str,
    chemin: Option<&str>,
    c: ContexteProgrammes,
) -> Result<Value, String> {
    if client == 0 {
        return Err("Demande refusée : programme qui fait la demande inconnu.".into());
    }
    let windows = s.dossiers().windows.clone();
    let reference = s.fiche(i64::from(client), "");
    let ctx = Contexte {
        windows: windows.clone(),
        programmes: s.dossiers().programmes.clone(),
        antivirus: c.antivirus,
        services_pid: Some(c.services_pid),
        reference: Some(fermeture::Reference {
            id: i64::from(client),
            session: reference.session,
            compte: reference.compte,
            eleve: reference.eleve,
        }),
        processus: Default::default(),
    };
    let mut n = 0;
    // (par son nom exact : « [ » ou « * » dans un nom de programme ne sont pas des jokers)
    for p in s.processus() {
        let nom_processus = if texte::finit(&p.nom, ".exe") { &p.nom[..p.nom.len() - 4] } else { p.nom.as_str() };
        if !texte::egal(nom_processus, nom) {
            continue;
        }
        let pc = s.chemin_processus(p.pid);
        if pc.is_empty() || s.dossiers().dans_windows(&pc) {
            continue;
        }
        if let Some(c) = chemin
            && !texte::egal(&pc, c)
        {
            continue;
        }
        if fermeture::refus(&ctx, &s.fiche(p.pid, &pc), true).is_some() {
            continue;
        }
        if s.fermer_processus(p.pid) {
            n += 1;
        }
    }
    Ok(json!(n))
}
