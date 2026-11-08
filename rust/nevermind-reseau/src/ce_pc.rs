//! « Ce PC » dans l'audit du réseau (`$NetAuditWork`, audit-reseau.ps1:125-141) : ce que la sonde a lu, mis en forme
//! comme la version PowerShell. La lecture elle-même (registre, gestionnaire des réseaux de Windows) est dans
//! src/windows ; ici seulement la règle, sans Windows, pour les tests.

use serde_json::{Value, json};

/// Un partage lu dans le registre (`SYSTEM\CurrentControlSet\Services\LanmanServer\Shares`)
#[derive(Debug, Clone, Default)]
pub struct PartageLu {
    /// Le nom de la valeur : le nom du partage
    pub nom: String,
    /// Ses lignes « Clé=valeur » (Path, Type, Remark...)
    pub lignes: Vec<String>,
    /// Son descripteur de sécurité (`Shares\Security\<nom>`) ; `None` : aucun d'écrit ; vide : illisible
    pub securite: Option<Vec<u8>>,
}

/// Ce que la sonde a lu sur ce PC
#[derive(Debug, Clone, Default)]
pub struct LecturePc {
    pub smb1: bool,
    pub smb1_client: bool,
    pub rdp: bool,
    /// Les profils du pare-feu coupés (« Domain », « Private », « Public »)
    pub pare_feu_coupe: Vec<String>,
    /// La catégorie du réseau de la connexion (`NLM_NETWORK_CATEGORY` : 0 public, 1 privé, 2 domaine) ; `None` :
    /// illisible
    pub categorie: Option<i32>,
    pub partages: Vec<PartageLu>,
}

/// Le bloc « Pc » du résultat de l'audit (mêmes champs que `$pc`)
pub fn bloc_pc(l: &LecturePc) -> Value {
    json!({ "Smb1": l.smb1, "Smb1Client": l.smb1_client, "Rdp": l.rdp, "FirewallOff": l.pare_feu_coupe,
        "Category": categorie(l.categorie), "Shares": partages(&l.partages) })
}

/// `[string]$profil.NetworkCategory` ; vide si illisible
pub fn categorie(n: Option<i32>) -> &'static str {
    match n {
        Some(0) => "Public",
        Some(1) => "Private",
        Some(2) => "DomainAuthenticated",
        _ => "",
    }
}

/// `$pc.Shares` : « Nom|Chemin|Droits de Tout le monde », sans les partages qui finissent par « $ » (print$, et les
/// partages d'administration, qui ne sont d'ailleurs pas dans le registre)
pub fn partages(l: &[PartageLu]) -> Vec<String> {
    l.iter()
        .filter(|p| !p.nom.is_empty() && !p.nom.ends_with('$'))
        .map(|p| {
            let chemin =
                p.lignes.iter().find_map(|x| x.strip_prefix("Path=").or_else(|| x.strip_prefix("path="))).unwrap_or("");
            format!("{}|{chemin}|{}", p.nom, droits_tout_le_monde(p.securite.as_deref()).join(","))
        })
        .collect()
}

const SID_TOUT_LE_MONDE: [u8; 12] = [1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0];

/// Les droits « Allow » de Tout le monde (S-1-1-0), dans l'ordre des entrées, comme `Get-SmbShareAccess` les écrit
/// (Full, Change, Read, sinon Custom). Sans descripteur : la lecture pour tout le monde (ce que Windows applique à un
/// partage sans sécurité écrite). Une liste d'accès absente ou nulle : tous les droits. Un descripteur illisible :
/// aucun droit lu.
pub fn droits_tout_le_monde(sd: Option<&[u8]>) -> Vec<&'static str> {
    let Some(sd) = sd else { return vec!["Read"] };
    let u16_a = |i: usize| sd.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_a = |i: usize| sd.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let (Some(controle), Some(dacl)) = (u16_a(2), u32_a(16)) else { return Vec::new() };
    // (SE_DACL_PRESENT absent, ou liste nulle : rien ne limite l'accès)
    if sd.first() != Some(&1) || controle & 0x8000 == 0 {
        return Vec::new();
    }
    if controle & 0x0004 == 0 || dacl == 0 {
        return vec!["Full"];
    }
    let debut = dacl as usize;
    let (Some(taille_acl), Some(nombre)) = (u16_a(debut + 2), u16_a(debut + 4)) else { return Vec::new() };
    let fin_acl = debut + taille_acl as usize;
    if fin_acl > sd.len() {
        return Vec::new();
    }
    let mut r = Vec::new();
    let mut i = debut + 8;
    for _ in 0..nombre {
        let (Some(&sorte), Some(taille)) = (sd.get(i), u16_a(i + 2)) else { return Vec::new() };
        let taille = taille as usize;
        if taille < 8 || i + taille > fin_acl {
            return Vec::new();
        }
        // (ACCESS_ALLOWED_ACE : masque, puis le SID)
        if sorte == 0 && sd.get(i + 8..i + taille) == Some(&SID_TOUT_LE_MONDE[..]) {
            r.push(match u32_a(i + 4) {
                Some(0x001F_01FF) => "Full",
                Some(0x0013_01BF) => "Change",
                Some(0x0012_00A9) => "Read",
                _ => "Custom",
            });
        }
        i += taille;
    }
    r
}
