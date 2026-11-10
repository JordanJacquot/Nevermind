//! Les fichiers .ini d'OBS et de Streamlabs (basic.ini, global.ini, user.ini), lus et changés SANS rien perdre : chaque
//! ligne est gardée telle quelle (sections et clés inconnues, commentaires, lignes vides, ordre), seules les clés
//! demandées changent ou s'ajoutent à la fin de leur section.
//!
//! Comme libobs (util/config-file.c) : une ligne « [Section] » ouvre une section, « clé=valeur » est une valeur (coupée
//! au premier « = », espaces autour retirés), noms sensibles aux majuscules. À l'écriture, comme OBS sous Windows :
//! UTF-8 avec BOM ; les fins de ligne du fichier sont gardées (LF pour un fichier écrit par OBS).

/// Un fichier .ini en mémoire, ligne à ligne
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ini {
    lignes: Vec<String>,
    /// La fin de ligne du fichier (« \n » ou « \r\n »)
    fin: &'static str,
    /// La dernière ligne finissait par une fin de ligne
    fin_finale: bool,
}

/// L'en-tête de section d'une ligne (« [Video] » : « Video »)
fn section_de(l: &str) -> Option<&str> {
    let t = l.trim();
    (t.len() >= 2 && t.starts_with('[') && t.ends_with(']')).then(|| t[1..t.len() - 1].trim())
}

/// La clé et la valeur d'une ligne (« OutputCX=1280 »)
fn cle_valeur(l: &str) -> Option<(&str, &str)> {
    if section_de(l).is_some() {
        return None;
    }
    let t = l.trim_start();
    if t.starts_with('#') || t.starts_with(';') {
        return None;
    }
    let (k, v) = t.split_once('=')?;
    let k = k.trim();
    (!k.is_empty()).then(|| (k, v.trim()))
}

impl Ini {
    /// Un fichier vide (nouveau)
    pub fn vide() -> Ini {
        Ini { lignes: Vec::new(), fin: "\n", fin_finale: true }
    }

    /// Lit les octets d'un fichier .ini (UTF-8, BOM accepté). Erreur : le fichier n'est pas du texte UTF-8 (abîmé).
    pub fn lire(octets: &[u8]) -> Result<Ini, String> {
        let o = octets.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(octets);
        if o.contains(&0) {
            return Err("le fichier contient des octets nuls (abîmé)".into());
        }
        let t = std::str::from_utf8(o).map_err(|_| "le fichier n'est pas du texte UTF-8 (abîmé)".to_string())?;
        let fin = if t.contains("\r\n") { "\r\n" } else { "\n" };
        let fin_finale = t.is_empty() || t.ends_with('\n');
        let mut lignes: Vec<String> = t.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l).to_string()).collect();
        if fin_finale {
            lignes.pop();
        }
        Ok(Ini { lignes, fin, fin_finale })
    }

    /// Les sections présentes (dans l'ordre)
    pub fn sections(&self) -> Vec<String> {
        self.lignes.iter().filter_map(|l| section_de(l)).map(str::to_string).collect()
    }

    /// Les plages de lignes de la section (ses valeurs, sans l'en-tête), dans l'ordre
    fn plages(&self, section: &str) -> Vec<(usize, usize)> {
        let mut r = Vec::new();
        let mut debut = None;
        for (i, l) in self.lignes.iter().enumerate() {
            if let Some(s) = section_de(l) {
                if let Some(d) = debut.take() {
                    r.push((d, i));
                }
                if s == section {
                    debut = Some(i + 1);
                }
            }
        }
        if let Some(d) = debut {
            r.push((d, self.lignes.len()));
        }
        r
    }

    /// Une valeur (la première trouvée), `None` si la clé n'existe pas
    pub fn valeur(&self, section: &str, cle: &str) -> Option<&str> {
        for (d, f) in self.plages(section) {
            for l in &self.lignes[d..f] {
                if let Some((k, v)) = cle_valeur(l)
                    && k == cle
                {
                    return Some(v);
                }
            }
        }
        None
    }

    /// Un entier (`None` : absent ou pas un nombre)
    pub fn entier(&self, section: &str, cle: &str) -> Option<i64> {
        self.valeur(section, cle).and_then(|v| v.parse().ok())
    }

    /// Un oui/non comme libobs (« true », « 1 »)
    pub fn booleen(&self, section: &str, cle: &str) -> Option<bool> {
        self.valeur(section, cle).map(|v| v.eq_ignore_ascii_case("true") || v == "1")
    }

    /// Pose une valeur : la ligne existante est réécrite (toutes, si la clé est en double), sinon la clé est ajoutée
    /// après la dernière valeur de la section, sinon la section est ajoutée à la fin du fichier. Rend `true` si le
    /// fichier change.
    pub fn mettre(&mut self, section: &str, cle: &str, valeur: &str) -> bool {
        let nouvelle = format!("{cle}={valeur}");
        let mut trouve = false;
        let mut change = false;
        for (d, f) in self.plages(section) {
            for i in d..f {
                if let Some((k, v)) = cle_valeur(&self.lignes[i])
                    && k == cle
                {
                    trouve = true;
                    // (une valeur identique écrite autrement, « cle = v », est gardée telle quelle)
                    if v != valeur {
                        self.lignes[i] = nouvelle.clone();
                        change = true;
                    }
                }
            }
        }
        if trouve {
            return change;
        }
        if let Some(&(d, f)) = self.plages(section).last() {
            // après la dernière ligne non vide de la section (les lignes vides restent avant la section suivante)
            let mut i = f;
            while i > d && self.lignes[i - 1].trim().is_empty() {
                i -= 1;
            }
            self.lignes.insert(i, nouvelle);
            return true;
        }
        if self.lignes.last().is_some_and(|l| !l.trim().is_empty()) {
            self.lignes.push(String::new());
        }
        self.lignes.push(format!("[{section}]"));
        self.lignes.push(nouvelle);
        true
    }

    /// Retire une clé de la section (toutes ses lignes, si elle est en double). Rend `true` si le fichier change.
    pub fn retirer(&mut self, section: &str, cle: &str) -> bool {
        let mut a_retirer = Vec::new();
        for (d, f) in self.plages(section) {
            for i in d..f {
                if cle_valeur(&self.lignes[i]).is_some_and(|(k, _)| k == cle) {
                    a_retirer.push(i);
                }
            }
        }
        for i in a_retirer.iter().rev() {
            self.lignes.remove(*i);
        }
        !a_retirer.is_empty()
    }

    /// Retire une section qui n'a plus aucune ligne (que des lignes vides), avec la ligne vide qui la précédait :
    /// l'inverse de [`Ini::mettre`] quand il a dû l'ajouter. Rend `true` si le fichier change.
    pub fn retirer_section_vide(&mut self, section: &str) -> bool {
        let mut change = false;
        while let Some((d, f)) = self
            .plages(section)
            .into_iter()
            .rev()
            .find(|(d, f)| self.lignes[*d..*f].iter().all(|l| l.trim().is_empty()))
        {
            // l'en-tête (d - 1) et la ligne vide juste avant lui
            let mut debut = d - 1;
            if debut > 0 && self.lignes[debut - 1].trim().is_empty() {
                debut -= 1;
            }
            // à la fin du fichier, ses lignes vides partent aussi ; sinon elles séparent encore la section suivante
            let fin = if f == self.lignes.len() { f } else { d };
            self.lignes.drain(debut..fin);
            change = true;
        }
        change
    }

    /// Les octets à écrire : UTF-8 avec BOM, les fins de ligne du fichier
    pub fn octets(&self) -> Vec<u8> {
        let mut t = self.lignes.join(self.fin);
        if self.fin_finale && !self.lignes.is_empty() {
            t.push_str(self.fin);
        }
        let mut o = Vec::with_capacity(t.len() + 3);
        o.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        o.extend_from_slice(t.as_bytes());
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBS: &str = "\u{feff}[General]\nName=Sans titre\n\n[Video]\nBaseCX=1920\nBaseCY=1080\nOutputCX=1280\nOutputCY=720\n\n[Panels]\nCookieId=ABCDEF\n";

    #[test]
    fn relu_tel_quel() {
        let i = Ini::lire(OBS.as_bytes()).unwrap();
        assert_eq!(i.octets(), OBS.as_bytes());
        // sans BOM : le BOM est ajouté, rien d'autre ne change
        let sans = OBS.trim_start_matches('\u{feff}');
        let o = Ini::lire(sans.as_bytes()).unwrap().octets();
        assert_eq!(&o[3..], sans.as_bytes());
        assert_eq!(&o[..3], &[0xEF, 0xBB, 0xBF]);
        // fins de ligne Windows gardées, dernière ligne sans fin gardée
        let crlf = "[A]\r\nx=1\r\ny=2";
        assert_eq!(&Ini::lire(crlf.as_bytes()).unwrap().octets()[3..], crlf.as_bytes());
        assert_eq!(Ini::lire(b"").unwrap().octets(), vec![0xEF, 0xBB, 0xBF]);
    }

    #[test]
    fn valeurs() {
        let i = Ini::lire(OBS.as_bytes()).unwrap();
        assert_eq!(i.valeur("Video", "OutputCX"), Some("1280"));
        assert_eq!(i.entier("Video", "BaseCY"), Some(1080));
        assert_eq!(i.valeur("Video", "outputcx"), None);
        assert_eq!(i.valeur("General", "Name"), Some("Sans titre"));
        assert_eq!(i.valeur("Output", "Mode"), None);
        assert_eq!(i.sections(), vec!["General", "Video", "Panels"]);
        let j = Ini::lire(b"[A]\nurl=rtmp://x/y?a=b\n b = 2 \n# c=3\n[B]\nd=true\n").unwrap();
        assert_eq!(j.valeur("A", "url"), Some("rtmp://x/y?a=b"));
        assert_eq!(j.valeur("A", "b"), Some("2"));
        assert_eq!(j.valeur("A", "# c"), None);
        assert_eq!(j.booleen("B", "d"), Some(true));
    }

    #[test]
    fn changer_garde_le_reste() {
        let mut i = Ini::lire(OBS.as_bytes()).unwrap();
        assert!(i.mettre("Video", "OutputCX", "1920"));
        assert!(!i.mettre("Video", "OutputCX", "1920"));
        assert!(i.mettre("Video", "FPSCommon", "60"));
        assert!(i.mettre("SimpleOutput", "VBitrate", "6000"));
        let t = String::from_utf8(i.octets()).unwrap();
        assert_eq!(
            t,
            "\u{feff}[General]\nName=Sans titre\n\n[Video]\nBaseCX=1920\nBaseCY=1080\nOutputCX=1920\nOutputCY=720\nFPSCommon=60\n\n[Panels]\nCookieId=ABCDEF\n\n[SimpleOutput]\nVBitrate=6000\n"
        );
    }

    #[test]
    fn retirer_defait_mettre() {
        let mut i = Ini::lire(OBS.as_bytes()).unwrap();
        i.mettre("Video", "FPSCommon", "60");
        i.mettre("SimpleOutput", "VBitrate", "6000");
        assert!(i.retirer("Video", "FPSCommon"));
        assert!(!i.retirer("Video", "FPSCommon"));
        assert!(i.retirer("SimpleOutput", "VBitrate"));
        assert!(i.retirer_section_vide("SimpleOutput"));
        assert!(!i.retirer_section_vide("Video"), "une section qui a encore des valeurs reste");
        assert_eq!(i.octets(), OBS.as_bytes());
        // une section vide au milieu : la section suivante garde sa ligne vide
        let mut j = Ini::lire(b"[A]\nx=1\n\n[S]\n\n[T]\ny=2\n").unwrap();
        assert!(j.retirer_section_vide("S"));
        assert_eq!(String::from_utf8_lossy(&j.octets()[3..]), "[A]\nx=1\n\n[T]\ny=2\n");
    }

    #[test]
    fn sections_en_double_et_cles_en_double() {
        let mut i = Ini::lire(b"[V]\na=1\n[X]\nz=0\n[V]\na=1\nb=2\n").unwrap();
        assert!(i.mettre("V", "a", "5"));
        assert!(i.mettre("V", "c", "3"));
        assert_eq!(String::from_utf8_lossy(&i.octets()[3..]), "[V]\na=5\n[X]\nz=0\n[V]\na=5\nb=2\nc=3\n");
    }

    #[test]
    fn fichiers_abimes() {
        assert!(Ini::lire(&[0xff, 0xfe, 0x41, 0x00]).is_err());
        assert!(Ini::lire(b"[Video]\nBase\0CX=1").is_err());
        let mut i = Ini::lire(b"n'importe quoi\nsans = section\n").unwrap();
        assert_eq!(i.valeur("Video", "x"), None);
        i.mettre("Video", "x", "1");
        assert_eq!(String::from_utf8_lossy(&i.octets()[3..]), "n'importe quoi\nsans = section\n\n[Video]\nx=1\n");
        let mut v = Ini::vide();
        v.mettre("Output", "Mode", "Advanced");
        assert_eq!(String::from_utf8_lossy(&v.octets()[3..]), "[Output]\nMode=Advanced\n");
    }
}
