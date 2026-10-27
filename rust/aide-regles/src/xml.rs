//! Lecture d'un XML bien formé, assez pour la description d'une tâche planifiée (`schtasks /Query /XML`).
//!
//! Comme `[xml]` de PowerShell : un texte mal formé (balises mal fermées, plusieurs racines, texte hors de la racine)
//! est refusé. Sont sautés : la déclaration `<?xml ...?>`, les instructions `<?...?>`, les commentaires ; les attributs
//! sont lus puis ignorés. Une déclaration de type (`<!DOCTYPE`) est refusée. Entités : `&lt; &gt; &amp; &quot; &apos;`
//! et `&#...;`.

/// Un élément : son nom (avec son préfixe éventuel), son texte (morceaux mis bout à bout) et ses enfants
pub struct Element {
    pub nom: String,
    pub texte: String,
    pub enfants: Vec<Element>,
}

impl Element {
    /// Les éléments au bout du chemin `noms` sous cet élément (chaque étape : tous les enfants de ce nom), comme
    /// `$x.A.B.C` de PowerShell qui énumère les listes
    pub fn chercher(&self, noms: &[&str]) -> Vec<&Element> {
        let mut courants = vec![self];
        for n in noms {
            courants = courants.iter().flat_map(|e| e.enfants.iter().filter(|c| c.nom == *n)).collect();
        }
        courants
    }

    /// Le texte des éléments au bout du chemin, séparés par une espace (`[string]` d'une liste en PowerShell)
    pub fn texte_de(&self, noms: &[&str]) -> String {
        self.chercher(noms).iter().map(|e| e.texte.as_str()).collect::<Vec<_>>().join(" ")
    }
}

/// L'élément racine du document, ou `None` s'il n'est pas bien formé
pub fn lire(t: &str) -> Option<Element> {
    let mut l = Lecteur { t, i: 0 };
    l.passer_divers()?;
    if !l.reste().starts_with('<') {
        return None;
    }
    let racine = l.element()?;
    l.passer_divers()?;
    if !l.reste().is_empty() {
        return None;
    }
    Some(racine)
}

struct Lecteur<'a> {
    t: &'a str,
    i: usize,
}

impl Lecteur<'_> {
    fn reste(&self) -> &str {
        &self.t[self.i..]
    }

    fn espaces(&mut self) {
        let r = self.reste();
        self.i += r.len() - r.trim_start_matches([' ', '\t', '\r', '\n', '\u{feff}']).len();
    }

    /// Espaces, déclarations et commentaires hors des éléments
    fn passer_divers(&mut self) -> Option<()> {
        loop {
            self.espaces();
            if self.reste().starts_with("<?") {
                let f = self.reste().find("?>")?;
                self.i += f + 2;
            } else if self.reste().starts_with("<!--") {
                let f = self.reste()[4..].find("-->")?;
                self.i += f + 7;
            } else if self.reste().starts_with("<!") {
                return None;
            } else {
                return Some(());
            }
        }
    }

    fn nom(&mut self) -> Option<String> {
        let r = self.reste();
        let n = r.find(|c: char| c.is_whitespace() || c == '>' || c == '/' || c == '=').unwrap_or(r.len());
        if n == 0 {
            return None;
        }
        let nom = r[..n].to_string();
        if nom.contains(['<', '"', '\'', '&']) {
            return None;
        }
        self.i += n;
        Some(nom)
    }

    /// Un élément, depuis son « < »
    fn element(&mut self) -> Option<Element> {
        self.i += 1;
        let nom = self.nom()?;
        // Attributs : nom="valeur" ou nom='valeur'
        loop {
            self.espaces();
            let r = self.reste();
            if r.starts_with("/>") {
                self.i += 2;
                return Some(Element { nom, texte: String::new(), enfants: Vec::new() });
            }
            if r.starts_with('>') {
                self.i += 1;
                break;
            }
            self.nom()?;
            self.espaces();
            if !self.reste().starts_with('=') {
                return None;
            }
            self.i += 1;
            self.espaces();
            let q = self.reste().chars().next()?;
            if q != '"' && q != '\'' {
                return None;
            }
            let f = self.reste()[1..].find(q)?;
            let valeur = &self.reste()[1..1 + f];
            if valeur.contains('<') {
                return None;
            }
            decoder(valeur)?;
            self.i += f + 2;
        }
        // Contenu : texte, enfants, commentaires, jusqu'à la balise fermante du même nom
        let mut texte = String::new();
        let mut enfants = Vec::new();
        loop {
            let r = self.reste();
            if r.is_empty() {
                return None;
            }
            if let Some(apres) = r.strip_prefix("</") {
                let f = apres.find('>')?;
                if apres[..f].trim_end() != nom {
                    return None;
                }
                self.i += 2 + f + 1;
                return Some(Element { nom, texte, enfants });
            }
            if let Some(c) = r.strip_prefix("<!--") {
                let f = c.find("-->")?;
                self.i += f + 7;
            } else if let Some(c) = r.strip_prefix("<![CDATA[") {
                let f = c.find("]]>")?;
                texte.push_str(&c[..f]);
                self.i += 9 + f + 3;
            } else if r.starts_with("<?") {
                let f = r.find("?>")?;
                self.i += f + 2;
            } else if r.starts_with('<') {
                enfants.push(self.element()?);
            } else {
                let f = r.find('<').unwrap_or(r.len());
                texte.push_str(&decoder(&r[..f])?);
                self.i += f;
            }
        }
    }
}

/// Les entités d'un texte
fn decoder(s: &str) -> Option<String> {
    let mut r = String::with_capacity(s.len());
    let mut reste = s;
    while let Some(i) = reste.find('&') {
        r.push_str(&reste[..i]);
        let apres = &reste[i + 1..];
        let f = apres.find(';')?;
        let ent = &apres[..f];
        let c = match ent {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let n = if let Some(h) = ent.strip_prefix("#x") {
                    u32::from_str_radix(h, 16).ok()?
                } else {
                    ent.strip_prefix('#')?.parse().ok()?
                };
                char::from_u32(n)?
            }
        };
        r.push(c);
        reste = &apres[f + 1..];
    }
    r.push_str(reste);
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bien_ou_mal_forme() {
        let x = lire("<?xml version=\"1.0\"?><A x='1'><B>un &amp; deux</B><B>trois</B><C/></A>").unwrap();
        assert_eq!(x.texte_de(&["B"]), "un & deux trois");
        assert_eq!(x.chercher(&["C"]).len(), 1);
        assert!(lire("pas du xml").is_none());
        assert!(lire("<A><B></A>").is_none());
        assert!(lire("<A></A><B></B>").is_none());
        assert!(lire("").is_none());
        assert!(lire("<A>&inconnue;</A>").is_none());
    }
}
