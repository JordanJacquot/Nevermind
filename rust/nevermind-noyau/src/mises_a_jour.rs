//! Les mises à jour de Nevermind depuis GitHub (modules\mises-a-jour.ps1), pour une copie qui n'est pas gérée par
//! Mindset (une copie gérée : c'est Mindset qui installe les versions, rien n'est cherché ici).
//!
//! - [`choisir_version`] : `$GetReleaseScript` (la dernière version publiée ; avec les bêta, la plus récente même
//!   « préversion ») ; [`plus_recente`] : `Test-NewerVersion` ;
//! - [`Contexte::verifier_mise_a_jour`] : `Invoke-UpdateCheck` (les textes de la carte Version, le bandeau) ;
//! - [`Contexte::installer_mise_a_jour`] : `Install-Update` (le zip et sa signature téléchargés, la signature
//!   vérifiée avec les clés publiques livrées, les octets vérifiés dézippés depuis la mémoire, puis les fichiers
//!   remplacés un par un) ;
//! - [`notes_version`] : `$GetNotesScript` (« Quoi de neuf »).
//!
//! Le réseau passe par un [`Transport`] : WinHTTP dans l'appli, un faux serveur dans les tests (rien n'est jamais
//! téléchargé en vrai par un test). La vérification de la signature est celle de la bibliothèque commune (la même
//! que Mindset et Mindcare), et le dézippage refuse ce que refuse `Expand-ZipVerifie` (fichier chiffré, taille ou CRC
//! faux, deflate abîmé, chemin hors du dossier).

use crate::Contexte;
use crate::aide::texte_ps;
use crate::contexte::verrou;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Le dépôt GitHub où sont publiées les mises à jour (`$UpdateRepo`)
pub const DEPOT: &str = "JordanJacquot/Nevermind";
/// L'adresse de l'API de GitHub
pub const API: &str = "https://api.github.com";

/// Le réseau : une demande GET, le code de réponse et le contenu
pub trait Transport: Send + Sync {
    fn obtenir(&self, url: &str, delai_s: u32) -> Result<(u16, Vec<u8>), String>;
}

/// Le réseau par WinHTTP (l'appli)
pub struct WinHttp;

impl Transport for WinHttp {
    fn obtenir(&self, url: &str, delai_s: u32) -> Result<(u16, Vec<u8>), String> {
        crate::windows::http::obtenir(url, delai_s)
    }
}

/// Une version publiée (`@{ Version; Url; SigUrl; Beta; Notes }`)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Publication {
    pub version: String,
    pub url: String,
    pub sig_url: String,
    pub beta: bool,
    pub notes: String,
}

/// `Invoke-RestMethod` : le JSON d'une adresse (une erreur pour une réponse autre que 2xx)
fn json_de(t: &dyn Transport, url: &str, delai_s: u32) -> Result<Value, String> {
    let (code, corps) = t.obtenir(url, delai_s)?;
    if !(200..300).contains(&code) {
        return Err(format!("Le serveur distant a retourné une erreur : ({code})."));
    }
    let texte = suite_commun::json_sur::texte_du_fichier(&corps);
    serde_json::from_str(texte.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())
}

fn egal(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// La décision de `$GetReleaseScript` sur ce que GitHub a répondu (`latest` : un objet ; avec les bêta : la liste
/// des 10 dernières). `ancien` : la copie porte l'ancien nom (OptiGame.ps1) et prend d'abord OptiGame.zip.
pub fn choisir_version(reponse: &Value, beta: bool, ancien: bool) -> Result<Publication, String> {
    let r = if beta {
        let l = match reponse {
            Value::Array(a) => a.clone(),
            Value::Null => Vec::new(),
            autre => vec![autre.clone()],
        };
        l.into_iter().find(|x| !crate::aide::vrai(&x["draft"]))
    } else {
        Some(reponse.clone()).filter(crate::aide::vrai)
    };
    let Some(r) = r else { return Err("Aucune version publiée.".into()) };
    let noms = if ancien { ["OptiGame.zip", "Nevermind.zip"] } else { ["Nevermind.zip", "OptiGame.zip"] };
    let assets = match &r["assets"] {
        Value::Array(a) => a.clone(),
        Value::Null => Vec::new(),
        autre => vec![autre.clone()],
    };
    let zip = noms.iter().find_map(|n| assets.iter().find(|a| egal(&texte_ps(&a["name"]), n)).cloned());
    let Some(zip) = zip else { return Err("Aucun fichier Nevermind.zip dans la dernière version publiée.".into()) };
    let nom_sig = format!("{}.sig", texte_ps(&zip["name"]));
    let sig = assets.iter().find(|a| egal(&texte_ps(&a["name"]), &nom_sig));
    let tag = texte_ps(&r["tag_name"]);
    let version = tag.strip_prefix(['v', 'V']).unwrap_or(&tag).to_string();
    Ok(Publication {
        version,
        url: texte_ps(&zip["browser_download_url"]),
        sig_url: sig.map(|s| texte_ps(&s["browser_download_url"])).unwrap_or_default(),
        beta: crate::aide::vrai(&r["prerelease"]),
        notes: texte_ps(&r["body"]),
    })
}

/// `Test-NewerVersion` : `[version]$distante -gt [version]$locale` (faux si l'une est illisible)
pub fn plus_recente(distante: &str, locale: &str) -> bool {
    suite_commun::version::plus_recente(distante.trim(), locale.trim())
}

/// `$GetReleaseScript` : demande à GitHub la version à prendre
pub fn chercher_version(
    t: &dyn Transport,
    api: &str,
    depot: &str,
    beta: bool,
    ancien: bool,
) -> Result<Publication, String> {
    let url = if beta {
        format!("{api}/repos/{depot}/releases?per_page=10")
    } else {
        format!("{api}/repos/{depot}/releases/latest")
    };
    choisir_version(&json_de(t, &url, 10)?, beta, ancien)
}

/// `$GetNotesScript` : les notes d'une version précise (vide si elles n'ont pas pu être lues)
pub fn notes_version(t: &dyn Transport, version: &str) -> String {
    notes_version_de(t, API, DEPOT, version)
}

/// `$GetNotesScript` avec l'adresse de l'API et le dépôt (les tests : un faux serveur)
pub fn notes_version_de(t: &dyn Transport, api: &str, depot: &str, version: &str) -> String {
    json_de(t, &format!("{api}/repos/{depot}/releases/tags/v{version}"), 10)
        .map(|j| texte_ps(&j["body"]))
        .unwrap_or_default()
}

/// Ce que montre la carte « Version » (Paramètres) et le bandeau (`$ui.UpdateStatus`, `UpdateBanner`...)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EtatMaj {
    /// Le texte sous « Version » (`$ui.UpdateStatus.Text`)
    pub statut: String,
    /// Le bandeau « Nouvelle version... » (vide : caché)
    pub bandeau: String,
    /// Le bouton de la carte dit « Mettre à jour » (une version attend)
    pub mettre_a_jour: bool,
    /// La version qui attend d'être installée (`$script:PendingUpdate`)
    pub en_attente: Option<Publication>,
}

/// Ce que la vérification demande de montrer en plus (un message, une ligne d'état)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Retour {
    /// Un message (`Show-Message`), et s'il est un avertissement
    pub message: Option<(String, bool)>,
    /// La ligne d'état (`Set-Status`)
    pub statut: Option<String>,
}

impl Contexte {
    /// L'état de la carte « Version » (avant la première vérification : le texte de `Initialize-Suite` pour une
    /// copie gérée)
    pub fn etat_mise_a_jour(&self) -> EtatMaj {
        let mut e = verrou(&self.maj).clone();
        if e.statut.is_empty() && self.geree {
            e.statut = texte_geree(&self.version());
        }
        e
    }

    /// Le bandeau est fermé (« Plus tard »)
    pub fn fermer_bandeau_mise_a_jour(&self) {
        verrou(&self.maj).bandeau.clear();
    }

    /// `Invoke-UpdateCheck` (`manuel` : le bouton « Vérifier les mises à jour »)
    pub fn verifier_mise_a_jour(&self, manuel: bool, t: &dyn Transport) -> Retour {
        self.verifier_mise_a_jour_de(manuel, t, API, DEPOT)
    }

    /// `Invoke-UpdateCheck` avec l'adresse de l'API (les tests : un faux serveur local)
    pub fn verifier_mise_a_jour_de(&self, manuel: bool, t: &dyn Transport, api: &str, depot: &str) -> Retour {
        let v = self.version();
        let mut r = Retour::default();
        if self.geree {
            verrou(&self.maj).statut = texte_geree(&v);
            return r;
        }
        if !manuel && !self.reglage_bool("MajAuto", true) {
            verrou(&self.maj).statut =
                format!("Version {v}. Clique sur « Vérifier les mises à jour » pour chercher une nouvelle version.");
            return r;
        }
        verrou(&self.maj).statut = "Recherche d'une nouvelle version...".into();
        let ancien = false;
        match chercher_version(t, api, depot, self.reglage_bool("Beta", false), ancien) {
            Err(e) => {
                verrou(&self.maj).statut =
                    format!("Version {v}. Impossible de vérifier les mises à jour (pas de connexion ?).");
                self.journal(&format!("Mise à jour: {e}"));
                if manuel {
                    r.message = Some((
                        "Impossible de vérifier les mises à jour pour le moment.\n\nVérifie ta connexion Internet et réessaie.".into(),
                        true,
                    ));
                }
            }
            Ok(p) if plus_recente(&p.version, &v) => {
                let mut e = verrou(&self.maj);
                e.bandeau = format!(
                    "Nouvelle version {}{} disponible (tu as la {v}).",
                    if p.beta { "bêta " } else { "" },
                    p.version
                );
                e.statut = format!("Version {v}. La version {} est disponible.", p.version);
                e.mettre_a_jour = true;
                e.en_attente = Some(p);
            }
            Ok(_) => {
                verrou(&self.maj).statut = format!("Version {v} : tu as la dernière version.");
                if manuel {
                    r.statut = Some("Nevermind est à jour.".into());
                }
            }
        }
        r
    }

    /// `Set-BetaChannel` : les versions bêta activées ou non, puis une vérification
    pub fn choisir_beta(&self, oui: bool, t: &dyn Transport) -> Retour {
        let _ = self.ecrire_reglage("Beta", Value::Bool(oui), false);
        let mut r = self.verifier_mise_a_jour(true, t);
        if r.statut.is_none() {
            r.statut =
                Some(if oui { "Versions bêta activées.".into() } else { "Versions bêta désactivées.".into() });
        }
        r
    }
}

fn texte_geree(v: &str) -> String {
    format!("Version {v}. Les mises à jour de Nevermind passent par Mindset : ouvre Mindset pour les installer.")
}

/// Ce que l'installation a donné
#[derive(Debug, Clone, PartialEq)]
pub enum FinInstallation {
    /// Rien n'a été fait (pas de version en attente, copie gérée)
    Rien,
    /// Refusée ou ratée : le message à montrer, et la ligne d'état
    Refus { message: String, statut: Option<String> },
    /// Installée : Nevermind se ferme et relance ce programme
    Relancer(PathBuf),
}

/// Le message d'une signature refusée (« absente », « fausse ou illisible »)
pub fn message_signature(version: &str, quoi: &str) -> String {
    format!(
        "La version {version} n'a pas été installée : sa signature numérique est {quoi}.\n\nPar sécurité, Nevermind n'installe que les versions signées par son auteur. Rien n'a été modifié sur ton PC.\n\nRéessaie plus tard, ou installe Mindset pour recevoir les mises à jour."
    )
}

const ZIP_ABIME: &str = "Le zip est abîmé : rien n'a été installé.";
const ZIP_CHIFFRE: &str = "Le zip contient un fichier chiffré : rien n'a été installé.";

/// `Expand-ZipVerifie` : dézippe dans `dest` les octets dont la signature vient d'être vérifiée (jamais le fichier
/// relu sur le disque), entrée par entrée : une entrée chiffrée, une taille ou un CRC faux, un deflate illisible, un
/// chemin hors du dossier : tout est refusé.
pub fn dezipper_verifie(octets: &[u8], dest: &Path) -> Result<(), String> {
    use std::io::Read;
    let mut a = zip::ZipArchive::new(std::io::Cursor::new(octets)).map_err(|_| ZIP_ABIME.to_string())?;
    let racine_txt = suite_commun::chemins::chemin_complet(&dest.to_string_lossy()).map_err(|e| e.to_string())?;
    let racine = format!("{}\\", racine_txt.trim_end_matches('\\'));
    std::fs::create_dir_all(&racine).map_err(|e| e.to_string())?;
    for i in 0..a.len() {
        let mut e = match a.by_index(i) {
            Ok(e) => e,
            Err(zip::result::ZipError::UnsupportedArchive(m)) if m.to_lowercase().contains("password") => {
                return Err(ZIP_CHIFFRE.into());
            }
            Err(_) => return Err(ZIP_ABIME.into()),
        };
        if e.encrypted() {
            return Err(ZIP_CHIFFRE.into());
        }
        let nom = e.name().map_err(|_| ZIP_ABIME.to_string())?.into_owned();
        let cible = suite_commun::chemins::chemin_complet(&format!("{racine}{}", nom.replace('/', "\\")))
            .map_err(|_| format!("Chemin refusé dans le zip : {nom}"))?;
        if !cible.to_lowercase().starts_with(&racine.to_lowercase()) {
            return Err(format!("Chemin refusé dans le zip : {nom}"));
        }
        if nom.ends_with('/') || nom.ends_with('\\') {
            std::fs::create_dir_all(&cible).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = Path::new(&cible).parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let taille = e.size();
        let mut contenu = Vec::new();
        // (le CRC est vérifié par la lecture jusqu'au bout : un CRC faux ou un deflate abîmé donne une erreur)
        e.read_to_end(&mut contenu).map_err(|_| ZIP_ABIME.to_string())?;
        if contenu.len() as u64 != taille {
            return Err(ZIP_ABIME.into());
        }
        std::fs::write(&cible, &contenu).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn fichiers_de(d: &Path) -> Vec<PathBuf> {
    let mut r = Vec::new();
    let mut a_voir = vec![d.to_path_buf()];
    while let Some(x) = a_voir.pop() {
        let Ok(l) = std::fs::read_dir(&x) else { continue };
        for e in l.flatten() {
            let p = e.path();
            if p.is_dir() {
                a_voir.push(p);
            } else {
                r.push(p);
            }
        }
    }
    r.sort();
    r
}

impl Contexte {
    /// `Install-Update` (une fois l'accord donné par l'utilisateur) : la version en attente est téléchargée,
    /// vérifiée, dézippée, puis ses fichiers remplacent ceux de cette copie (`racine`). `cles` : le dossier des clés
    /// publiques livrées (fichiers\cles-publiques).
    pub fn installer_mise_a_jour(&self, t: &dyn Transport, cles: &Path) -> FinInstallation {
        let Some(rel) = verrou(&self.maj).en_attente.clone() else { return FinInstallation::Rien };
        if self.geree {
            return FinInstallation::Rien;
        }
        let annulee = Some("Mise à jour annulée.".to_string());
        if rel.sig_url.is_empty() {
            self.journal(&format!(
                "Mise à jour {} refusée : pas de signature (Nevermind.zip.sig) dans la version publiée",
                rel.version
            ));
            return FinInstallation::Refus { message: message_signature(&rel.version, "absente"), statut: None };
        }
        verrou(&self.maj).bandeau.clear();
        let tmp = std::env::temp_dir().join(format!("Nevermind-maj-{}", crate::assistance::identifiant()));
        let telecharger = || -> Result<(Vec<u8>, Vec<u8>), String> {
            let (c, zip) = t.obtenir(&rel.url, 120)?;
            if !(200..300).contains(&c) {
                return Err(format!("Le serveur distant a retourné une erreur : ({c})."));
            }
            let (c, sig) = t.obtenir(&rel.sig_url, 30)?;
            if !(200..300).contains(&c) {
                return Err(format!("Le serveur distant a retourné une erreur : ({c})."));
            }
            Ok((zip, sig))
        };
        let res = match telecharger() {
            Ok((zip, sig)) => {
                if !suite_commun::signature::verifier_signature(&zip, &sig, cles) {
                    self.journal(&format!("Mise à jour {} refusée : signature invalide", rel.version));
                    let _ = std::fs::remove_dir_all(&tmp);
                    return FinInstallation::Refus {
                        message: message_signature(&rel.version, "fausse ou illisible"),
                        statut: annulee,
                    };
                }
                match dezipper_verifie(&zip, &tmp.join("x")) {
                    Ok(()) => "OK".to_string(),
                    Err(e) if e.starts_with("Le zip ") && e.ends_with("rien n'a été installé.") => {
                        self.journal(&format!("Mise à jour {} refusée : {e}", rel.version));
                        let _ = std::fs::remove_dir_all(&tmp);
                        return FinInstallation::Refus {
                            message: format!(
                                "La version {} n'a pas été installée.\n\n{e} Ton Nevermind reste tel quel.\n\nInstalle Mindset pour recevoir les mises à jour.",
                                rel.version
                            ),
                            statut: annulee,
                        };
                    }
                    Err(e) => e,
                }
            }
            Err(e) => e,
        };
        // (dossier « Nevermind » et script Nevermind.ps1 ; « OptiGame » et OptiGame.ps1, leurs anciens noms)
        let src = ["Nevermind", "OptiGame"].iter().map(|n| tmp.join("x").join(n)).find(|p| p.exists());
        let script = src.as_ref().and_then(|s| {
            ["Nevermind.ps1", "OptiGame.ps1"].iter().map(|n| s.join("fichiers").join(n)).find(|p| p.exists())
        });
        let (Some(src), Some(script), "OK") = (src, script, res.as_str()) else {
            self.journal(&format!("Échec de la mise à jour: {res}"));
            let _ = std::fs::remove_dir_all(&tmp);
            return FinInstallation::Refus {
                message: format!("La mise à jour n'a pas pu être téléchargée.\n\n{res}\n\nRéessaie plus tard."),
                statut: annulee,
            };
        };
        let racine = self.racine();
        let mut erreurs = Vec::new();
        for f in fichiers_de(&src) {
            let rel_ = f.strip_prefix(&src).unwrap_or(&f);
            let dest = racine.join(rel_);
            if dest.exists() && std::fs::read(&dest).ok() == std::fs::read(&f).ok() {
                continue;
            }
            let r = (|| -> Result<(), String> {
                if let Some(p) = dest.parent() {
                    std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
                }
                // (un .exe en cours d'exécution ne peut pas être écrasé, mais il peut être renommé : l'ancien est mis
                // de côté en .old, effacé au prochain lancement)
                if dest.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) && dest.exists() {
                    let old = crate::fichiers::a_cote(&dest, ".old");
                    let _ = std::fs::remove_file(&old);
                    let _ = std::fs::rename(&dest, &old);
                }
                std::fs::copy(&f, &dest).map_err(|e| e.to_string())?;
                Ok(())
            })();
            if let Err(e) = r {
                erreurs.push(format!("{}: {e}", crate::fichiers::nom(&f)));
            }
        }
        // Fichiers de l'ancienne version qui n'existent plus dans la nouvelle (seulement dans « fichiers »)
        let nouveaux: std::collections::HashSet<String> = fichiers_de(&src.join("fichiers"))
            .iter()
            .map(|f| f.strip_prefix(&src).unwrap_or(f).to_string_lossy().to_lowercase())
            .collect();
        for f in fichiers_de(&racine.join("fichiers")) {
            let r = f.strip_prefix(&racine).unwrap_or(&f).to_string_lossy().to_lowercase();
            if !nouveaux.contains(&r) && std::fs::remove_file(&f).is_ok() {
                self.journal(&format!("Fichier obsolète supprimé: {r}"));
            }
        }
        let _ = std::fs::remove_dir_all(&tmp);
        if !erreurs.is_empty() {
            self.journal(&format!("Mise à jour incomplète: {}", erreurs.join(" | ")));
            return FinInstallation::Refus {
                message: format!("La mise à jour n'a pas pu remplacer tous les fichiers :\n\n{}", erreurs.join("\n")),
                statut: None,
            };
        }
        self.journal(&format!("Mise à jour installée: {} -> {}", self.version(), rel.version));
        FinInstallation::Relancer(racine.join("fichiers").join(crate::fichiers::nom(&script)))
    }
}
