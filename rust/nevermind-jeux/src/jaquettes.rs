//! Les images des jeux (bibliotheque.ps1) : celles que Steam garde sur le PC (Get-SteamArt), celles du launcher
//! (Ankama), celles déjà téléchargées (jaquettes\index.json), et le même jeu sur un autre launcher. Plus l'adresse
//! d'une image pour l'écran (Get-ImageVue : « fichier:<chemin> », servi par l'hôte).

use crate::biblio::Jeu;
use crate::systeme::{Ruche, Systeme};
use crate::texte::{joindre, sans_marques, texte_recherche};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Une jaquette téléchargée (index.json du dossier « jaquettes »).
#[derive(Debug, Clone, Default)]
pub struct Telechargee {
    pub cover: String,
    pub hero: String,
    pub logo: String,
    pub date: String,
}

#[derive(Default)]
pub struct Jaquettes {
    /// Le dossier des images de Steam (appcache\librarycache), « » si absent ; `None` : pas encore cherché.
    cache_steam: Option<String>,
    art: HashMap<String, Option<String>>,
    index: Option<HashMap<String, Telechargee>>,
}

fn existe(p: &str) -> bool {
    !p.is_empty() && Path::new(p).exists()
}

impl Jaquettes {
    pub fn oublier(&mut self) {
        self.art.clear();
        self.index = None;
        self.cache_steam = None;
    }

    fn cache_steam(&mut self, sys: &dyn Systeme) -> String {
        if self.cache_steam.is_none() {
            let mut c = String::new();
            if let Some(sp) =
                sys.reg_texte(Ruche::Utilisateur, r"Software\Valve\Steam", "SteamPath").filter(|s| !s.is_empty())
            {
                let p = joindre(&sp.replace('/', "\\"), r"appcache\librarycache");
                if Path::new(&p).exists() {
                    c = p;
                }
            }
            self.cache_steam = Some(c);
        }
        self.cache_steam.clone().unwrap_or_default()
    }

    /// Une image de Steam pour ce jeu (Get-SteamArt) : dans son dossier, ou rangée dans un sous-dossier (Steam récent).
    pub fn art_steam(&mut self, sys: &dyn Systeme, g: &Jeu, sorte: &str) -> Option<String> {
        if g.appid.is_empty() {
            return None;
        }
        let cle = format!("{}|{sorte}", g.appid);
        if let Some(v) = self.art.get(&cle) {
            return v.clone();
        }
        let cache = self.cache_steam(sys);
        let v = if cache.is_empty() {
            None
        } else {
            let dir = joindre(&cache, &g.appid);
            let f = joindre(&dir, sorte);
            if Path::new(&f).exists() { Some(f) } else { chercher(&dir, sorte) }
        };
        self.art.insert(cle, v.clone());
        v
    }

    pub fn index(&mut self, donnees: &Path) -> &HashMap<String, Telechargee> {
        if self.index.is_none() {
            let mut m = HashMap::new();
            let f = donnees.join("jaquettes").join("index.json");
            if let (Some(Value::Object(o)), _) = crate::json::lire_json_sur(None, &f) {
                for (k, v) in o {
                    m.insert(
                        k.to_lowercase(),
                        Telechargee {
                            cover: crate::json::texte(&v, "Cover"),
                            hero: crate::json::texte(&v, "Hero"),
                            logo: crate::json::texte(&v, "Logo"),
                            date: crate::json::texte(&v, "Date"),
                        },
                    );
                }
            }
            self.index = Some(m);
        }
        self.index.as_ref().unwrap()
    }

    /// La jaquette d'un jeu (Get-CoverFile). `tous` : la bibliothèque, pour le même jeu sur un autre launcher.
    pub fn jaquette(
        &mut self,
        sys: &dyn Systeme,
        donnees: &Path,
        g: &Jeu,
        tous: &[Jeu],
        jumeau: bool,
    ) -> Option<String> {
        if let Some(a) = &g.art
            && existe(&a.cover)
        {
            return Some(a.cover.clone());
        }
        let mut f = self.art_steam(sys, g, "library_600x900.jpg");
        if f.is_none() {
            f = self.art_steam(sys, g, "library_capsule.jpg");
        }
        if f.is_none()
            && let Some(c) = self.index(donnees).get(&g.nom.to_lowercase())
            && existe(&c.cover)
        {
            f = Some(c.cover.clone());
        }
        if f.is_none() && !jumeau {
            let moi = texte_recherche(&sans_marques(&g.nom));
            for o in tous {
                if o.nom != g.nom && texte_recherche(&sans_marques(&o.nom)) == moi {
                    f = self.jaquette(sys, donnees, o, tous, true);
                    if f.is_some() {
                        break;
                    }
                }
            }
        }
        f
    }

    /// Le logo d'un jeu (Get-LogoFile).
    pub fn logo(&mut self, sys: &dyn Systeme, donnees: &Path, g: &Jeu) -> Option<String> {
        if let Some(a) = &g.art
            && existe(&a.logo)
        {
            return Some(a.logo.clone());
        }
        let mut f = self.art_steam(sys, g, "logo.png");
        if f.is_none()
            && let Some(c) = self.index(donnees).get(&g.nom.to_lowercase())
            && existe(&c.logo)
        {
            f = Some(c.logo.clone());
        }
        f
    }

    /// La grande image d'un jeu (Get-HeroFile).
    pub fn banniere(&mut self, sys: &dyn Systeme, donnees: &Path, g: &Jeu) -> Option<String> {
        if let Some(a) = &g.art
            && existe(&a.hero)
        {
            return Some(a.hero.clone());
        }
        for k in ["library_hero.jpg", "header.jpg", "library_header.jpg"] {
            if let Some(f) = self.art_steam(sys, g, k) {
                return Some(f);
            }
        }
        let c = self.index(donnees).get(&g.nom.to_lowercase())?;
        if existe(&c.hero) { Some(c.hero.clone()) } else { None }
    }

    /// L'image d'un fichier du PC pour l'écran (Get-ImageVue) : la valeur « fichier:<chemin> », que l'hôte remplace
    /// par son adresse servie (`http://fichier.localhost/<jeton>`, règles Tauri de la suite).
    pub fn adresse(&mut self, chemin: &str) -> String {
        format!("fichier:{chemin}")
    }
}

/// Le premier fichier de ce nom dans les sous-dossiers (Get-ChildItem -Recurse -Filter).
fn chercher(dir: &str, nom: &str) -> Option<String> {
    let mut pile = vec![dir.to_string()];
    while let Some(d) = pile.pop() {
        let Ok(it) = std::fs::read_dir(&d) else { continue };
        let mut sous = Vec::new();
        let mut entrees: Vec<_> = it.flatten().collect();
        entrees.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
        for e in entrees {
            let p = joindre(&d, &e.file_name().to_string_lossy());
            match e.file_type() {
                Ok(t) if t.is_dir() => sous.push(p),
                Ok(_) if e.file_name().to_string_lossy().eq_ignore_ascii_case(nom) => return Some(p),
                _ => {}
            }
        }
        pile.extend(sous.into_iter().rev());
    }
    None
}

/// La taille d'une image JPEG ou PNG (largeur, hauteur), lue dans son en-tête.
pub fn taille_image(chemin: &str) -> Option<(u32, u32)> {
    let o = std::fs::read(chemin).ok()?;
    if o.len() > 24 && &o[..8] == b"\x89PNG\r\n\x1a\n" {
        let w = u32::from_be_bytes([o[16], o[17], o[18], o[19]]);
        let h = u32::from_be_bytes([o[20], o[21], o[22], o[23]]);
        return Some((w, h));
    }
    if o.len() < 4 || o[0] != 0xFF || o[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 9 < o.len() {
        if o[i] != 0xFF {
            i += 1;
            continue;
        }
        let m = o[i + 1];
        if m == 0xD8 || m == 0x01 || (0xD0..=0xD7).contains(&m) || m == 0xFF {
            i += if m == 0xFF { 1 } else { 2 };
            continue;
        }
        let long = u16::from_be_bytes([o[i + 2], o[i + 3]]) as usize;
        if matches!(m, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            let h = u16::from_be_bytes([o[i + 5], o[i + 6]]) as u32;
            let w = u16::from_be_bytes([o[i + 7], o[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + long;
    }
    None
}
