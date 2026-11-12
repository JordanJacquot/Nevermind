//! Les logos des classes de l'Organizer (Get-OrgClassIcon et Receive-OrgIcons de organizer.ps1) : téléchargés une
//! fois depuis DofusDB, gardés sur le PC (`<données>\organizer\classe-<numéro>.png`, le même dossier que la version
//! PowerShell), lus par Windows (WIC). Jamais dans l'appli.

use super::dessin::Toile;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, IWICPalette, WICBitmapDitherTypeNone,
    WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnLoad,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};

/// Le début de tout fichier PNG.
const SIGNATURE_PNG: [u8; 4] = [0x89, 0x50, 0x4E, 0x47];
/// Un logo plus grand que ça n'est pas un logo (on ne le lit pas).
const COTE_MAX: u32 = 1024;

/// Ce qu'on sait des logos pendant la session.
#[derive(Default)]
struct Etat {
    dossier: Option<PathBuf>,
    telecharger: bool,
    /// Les logos déjà lus (`None` : fichier illisible, on ne le relit pas).
    lus: HashMap<u32, Option<Arc<Toile>>>,
    /// Les téléchargements déjà lancés (réussis ou non : jamais deux fois par session).
    lances: HashSet<u32>,
}

/// Ce qu'il faut faire pour un logo.
enum Suite {
    /// Déjà en mémoire.
    Pret(Option<Arc<Toile>>),
    /// Le fichier est là : le lire.
    Lire(PathBuf),
    /// Le fichier manque : le télécharger dans ce dossier.
    Telecharger(PathBuf),
    /// Rien à faire.
    Rien,
}

impl Etat {
    fn suite(&mut self, id: u32) -> Suite {
        if id == 0 {
            return Suite::Rien;
        }
        if let Some(v) = self.lus.get(&id) {
            return Suite::Pret(v.clone());
        }
        let Some(d) = &self.dossier else { return Suite::Rien };
        let f = fichier(d, id);
        if f.is_file() {
            return Suite::Lire(f);
        }
        if self.telecharger && self.lances.insert(id) {
            return Suite::Telecharger(d.clone());
        }
        Suite::Rien
    }
}

static ETAT: LazyLock<Mutex<Etat>> = LazyLock::new(|| Mutex::new(Etat::default()));
static ARRIVE: AtomicBool = AtomicBool::new(false);

fn etat() -> std::sync::MutexGuard<'static, Etat> {
    ETAT.lock().unwrap_or_else(|e| e.into_inner())
}

fn fichier(dossier: &Path, id: u32) -> PathBuf {
    dossier.join(format!("classe-{id}.png"))
}

/// Le dossier des logos et si on peut les télécharger (jamais dans un essai).
pub fn regler(dossier: PathBuf, telecharger: bool) {
    let mut e = etat();
    if e.dossier.as_ref() != Some(&dossier) {
        e.lus.clear();
    }
    e.dossier = Some(dossier);
    e.telecharger = telecharger;
}

/// Le logo d'une classe (son numéro, voir `organizer::id_classe`), s'il est déjà sur le PC ; sinon `None`, et son
/// téléchargement part en arrière-plan (une fois par session).
pub fn logo(id_classe: u32) -> Option<Arc<Toile>> {
    let suite = etat().suite(id_classe);
    match suite {
        Suite::Pret(v) => v,
        Suite::Rien => None,
        Suite::Lire(f) => {
            let v = std::fs::read(&f).ok().and_then(|o| decoder_png(&o)).map(Arc::new);
            etat().lus.insert(id_classe, v.clone());
            v
        }
        Suite::Telecharger(dossier) => {
            let _ = std::thread::Builder::new().name("nevermind-logo".into()).spawn(move || {
                let net =
                    |url: &str| nevermind_noyau::windows::http::obtenir_borne(url, 15, "Nevermind", 2 * 1024 * 1024);
                if telecharger(id_classe, &dossier, &net) {
                    ARRIVE.store(true, Ordering::SeqCst);
                }
            });
            None
        }
    }
}

/// Un logo vient d'arriver depuis la dernière fois (pour redessiner la vue et la barre).
pub fn arrives() -> bool {
    ARRIVE.swap(false, Ordering::SeqCst)
}

/// Une demande GET : le code de réponse et le contenu (les essais donnent un faux Internet).
type Obtenir<'a> = dyn Fn(&str) -> Result<(u16, Vec<u8>), String> + 'a;

/// Télécharge le logo `id` dans le dossier (d'abord dans `.part`, puis renommé : jamais de fichier à moitié écrit).
/// Seule une vraie image PNG est gardée.
fn telecharger(id: u32, dossier: &Path, obtenir: &Obtenir<'_>) -> bool {
    let url = format!("https://api.dofusdb.fr/img/breeds/symbol_{id}.png");
    let Ok((code, octets)) = obtenir(&url) else { return false };
    if code != 200 || octets.len() <= 100 || !octets.starts_with(&SIGNATURE_PNG) {
        return false;
    }
    if std::fs::create_dir_all(dossier).is_err() {
        return false;
    }
    let f = fichier(dossier, id);
    let part = dossier.join(format!("classe-{id}.png.part"));
    if std::fs::write(&part, &octets).is_err() || std::fs::rename(&part, &f).is_err() {
        let _ = std::fs::remove_file(&part);
        return false;
    }
    true
}

/// COM ouvert sur ce fil le temps d'une lecture (refermé seulement si c'est nous qui l'avons ouvert).
struct Com(bool);

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY : CoInitializeEx a réussi sur ce fil (S_OK ou S_FALSE), on équilibre
            unsafe { CoUninitialize() };
        }
    }
}

/// Une image PNG lue par Windows : ses pixels en BGRA prémultipliés (le format de la Toile).
fn decoder_png(octets: &[u8]) -> Option<Toile> {
    if !octets.starts_with(&SIGNATURE_PNG) {
        return None;
    }
    // SAFETY : COM ouvert pour ce fil ; un fil déjà ouvert autrement (RPC_E_CHANGED_MODE) convient aussi à WIC
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if hr.is_err() && hr != RPC_E_CHANGED_MODE {
        return None;
    }
    let _com = Com(hr.is_ok());
    // (les objets de WIC sont tous libérés à la fin de wic(), avant que COM soit refermé)
    wic(octets)
}

fn wic(octets: &[u8]) -> Option<Toile> {
    // SAFETY : appels WIC ordinaires ; `octets` vit plus longtemps que le flux et le décodeur
    unsafe {
        let usine: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let flux = usine.CreateStream().ok()?;
        flux.InitializeFromMemory(octets).ok()?;
        let decodeur = usine.CreateDecoderFromStream(&flux, std::ptr::null(), WICDecodeMetadataCacheOnLoad).ok()?;
        let image = decodeur.GetFrame(0).ok()?;
        let conv = usine.CreateFormatConverter().ok()?;
        conv.Initialize(
            &image,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None::<&IWICPalette>,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
        .ok()?;
        let (mut l, mut h) = (0u32, 0u32);
        conv.GetSize(&mut l, &mut h).ok()?;
        if l == 0 || h == 0 || l > COTE_MAX || h > COTE_MAX {
            return None;
        }
        let mut brut = vec![0u8; (l * h * 4) as usize];
        conv.CopyPixels(std::ptr::null(), l * 4, &mut brut).ok()?;
        // (B, G, R, A en mémoire = 0xAARRGGBB en u32, déjà prémultipliés)
        let px = brut.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
        Some(Toile { l: l as i32, h: h as i32, px })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crc32(o: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in o {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
            }
        }
        !c
    }

    fn morceau(png: &mut Vec<u8>, genre: &[u8; 4], donnees: &[u8]) {
        png.extend_from_slice(&(donnees.len() as u32).to_be_bytes());
        let debut = png.len();
        png.extend_from_slice(genre);
        png.extend_from_slice(donnees);
        let c = crc32(&png[debut..]);
        png.extend_from_slice(&c.to_be_bytes());
    }

    /// Un petit PNG (RVBA, non compressé) fabriqué à la main : `px` en (R, V, B, A), ligne du haut d'abord.
    fn png(l: u32, h: u32, px: &[[u8; 4]]) -> Vec<u8> {
        let mut brut = Vec::new();
        for y in 0..h {
            brut.push(0);
            for x in 0..l {
                brut.extend_from_slice(&px[(y * l + x) as usize]);
            }
        }
        let (mut a, mut b) = (1u32, 0u32);
        for &o in &brut {
            a = (a + u32::from(o)) % 65521;
            b = (b + a) % 65521;
        }
        let mut z = vec![0x78, 0x01, 0x01];
        z.extend_from_slice(&(brut.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(brut.len() as u16)).to_le_bytes());
        z.extend_from_slice(&brut);
        z.extend_from_slice(&((b << 16) | a).to_be_bytes());
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&l.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        let mut p = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        morceau(&mut p, b"IHDR", &ihdr);
        morceau(&mut p, b"IDAT", &z);
        morceau(&mut p, b"IEND", &[]);
        p
    }

    fn png_essai() -> Vec<u8> {
        png(2, 2, &[[255, 0, 0, 255], [0, 255, 0, 128], [0, 0, 0, 0], [0, 0, 255, 255]])
    }

    fn dossier_essai(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nevermind-logos-essai-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn verifier(t: &Toile) {
        assert_eq!((t.l, t.h), (2, 2));
        assert_eq!(t.px[0], 0xFFFF_0000);
        // (vert à moitié transparent : prémultiplié)
        let p = t.px[1];
        assert_eq!(p >> 24, 128);
        assert!((((p >> 8) & 255) as i32 - 128).abs() <= 1, "{p:08X}");
        assert_eq!(p & 0x00FF_00FF, 0);
        assert_eq!(t.px[2], 0);
        assert_eq!(t.px[3], 0xFF00_00FF);
    }

    #[test]
    fn logos_decoder_un_png() {
        verifier(&decoder_png(&png_essai()).expect("PNG lu"));
        assert!(decoder_png(b"pas une image").is_none());
        assert!(decoder_png(&[0x89, 0x50, 0x4E, 0x47, 0, 0, 0]).is_none());
    }

    #[test]
    fn logos_lire_le_fichier_du_dossier() {
        let d = dossier_essai("lire");
        std::fs::write(fichier(&d, 8), png_essai()).unwrap();
        let mut e = Etat { dossier: Some(d.clone()), ..Default::default() };
        let Suite::Lire(f) = e.suite(8) else { panic!("le fichier devrait être lu") };
        verifier(&decoder_png(&std::fs::read(f).unwrap()).unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn logos_zero_est_sans_logo() {
        assert!(logo(0).is_none());
        let mut e = Etat { telecharger: true, ..Default::default() };
        assert!(matches!(e.suite(0), Suite::Rien));
        assert!(e.lances.is_empty());
    }

    #[test]
    fn logos_telechargement_interdit_aucun_fil() {
        let d = dossier_essai("interdit");
        let mut e = Etat { dossier: Some(d.clone()), telecharger: false, ..Default::default() };
        assert!(matches!(e.suite(8), Suite::Rien));
        assert!(matches!(e.suite(8), Suite::Rien));
        assert!(e.lances.is_empty());
        // Permis : lancé une seule fois par session
        e.telecharger = true;
        assert!(matches!(e.suite(8), Suite::Telecharger(ref x) if *x == d));
        assert!(matches!(e.suite(8), Suite::Rien));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn logos_telecharger_avec_un_faux_internet() {
        let d = dossier_essai("faux");
        let bon = png_essai();
        let ok = |url: &str| {
            assert_eq!(url, "https://api.dofusdb.fr/img/breeds/symbol_9.png");
            let mut v = bon.clone();
            v.resize(200, 0);
            Ok((200, v))
        };
        assert!(telecharger(9, &d, &ok));
        assert!(fichier(&d, 9).is_file());
        assert!(!d.join("classe-9.png.part").exists());
        // Refusés : mauvais code, trop petit, pas un PNG, réseau en panne
        assert!(!telecharger(3, &d, &|_| Ok((404, vec![0x89; 500]))));
        assert!(!telecharger(3, &d, &|_| Ok((200, bon[..50].to_vec()))));
        assert!(!telecharger(3, &d, &|_| Ok((200, vec![b'<'; 500]))));
        assert!(!telecharger(3, &d, &|_| Err("pas de réseau".into())));
        assert!(!fichier(&d, 3).exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
