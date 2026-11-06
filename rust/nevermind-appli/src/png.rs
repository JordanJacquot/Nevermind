//! Un fichier PNG à partir de pixels RVBA, sans bibliothèque de plus (construction d'essai : la capture de la
//! fenêtre). Les données ne sont pas compressées (blocs « stockés » de zlib) : le fichier est plus gros, mais
//! n'importe quel lecteur d'images l'ouvre.

/// La table du CRC-32 des blocs PNG (polynôme 0xEDB88320).
fn table_crc() -> [u32; 256] {
    let mut t = [0u32; 256];
    for (n, case) in t.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *case = c;
    }
    t
}

/// Le CRC-32 d'une suite d'octets.
pub fn crc32(octets: &[u8]) -> u32 {
    let t = table_crc();
    !octets.iter().fold(0xFFFF_FFFFu32, |c, b| t[((c ^ u32::from(*b)) & 0xFF) as usize] ^ (c >> 8))
}

/// La somme Adler-32 de zlib.
pub fn adler32(octets: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for x in octets {
        a = (a + u32::from(*x)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Un bloc PNG : longueur, type, données, CRC du type et des données.
fn bloc(sortie: &mut Vec<u8>, genre: &[u8; 4], donnees: &[u8]) {
    sortie.extend_from_slice(&(donnees.len() as u32).to_be_bytes());
    let debut = sortie.len();
    sortie.extend_from_slice(genre);
    sortie.extend_from_slice(donnees);
    let crc = crc32(&sortie[debut..]);
    sortie.extend_from_slice(&crc.to_be_bytes());
}

/// Le PNG d'une image `largeur` x `hauteur` en RVBA (8 bits par couleur, de haut en bas). `None` si la taille des
/// pixels ne correspond pas.
pub fn encoder(largeur: u32, hauteur: u32, rvba: &[u8]) -> Option<Vec<u8>> {
    let ligne = largeur as usize * 4;
    if largeur == 0 || hauteur == 0 || rvba.len() != ligne * hauteur as usize {
        return None;
    }
    // Chaque ligne précédée de son filtre (0 : aucun)
    let mut brut = Vec::with_capacity((ligne + 1) * hauteur as usize);
    for l in rvba.chunks_exact(ligne) {
        brut.push(0);
        brut.extend_from_slice(l);
    }
    // zlib : en-tête, blocs stockés de 65535 octets au plus, somme Adler-32
    let mut z = vec![0x78, 0x01];
    let morceaux: Vec<&[u8]> = brut.chunks(65535).collect();
    for (i, m) in morceaux.iter().enumerate() {
        z.push(u8::from(i + 1 == morceaux.len()));
        let n = m.len() as u16;
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&(!n).to_le_bytes());
        z.extend_from_slice(m);
    }
    z.extend_from_slice(&adler32(&brut).to_be_bytes());

    let mut sortie = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut entete = Vec::with_capacity(13);
    entete.extend_from_slice(&largeur.to_be_bytes());
    entete.extend_from_slice(&hauteur.to_be_bytes());
    // 8 bits, RVBA (6), compression 0, filtres 0, pas d'entrelacement
    entete.extend_from_slice(&[8, 6, 0, 0, 0]);
    bloc(&mut sortie, b"IHDR", &entete);
    bloc(&mut sortie, b"IDAT", &z);
    bloc(&mut sortie, b"IEND", &[]);
    Some(sortie)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sommes_connues() {
        // Valeurs de référence (zlib, et le CRC du bloc IEND que tout PNG porte)
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn png_bien_forme() {
        // Une image de 300 x 120 (plus de 65535 octets : plusieurs blocs zlib)
        let (l, h) = (300u32, 120u32);
        let px: Vec<u8> = (0..l * h * 4).map(|i| (i % 251) as u8).collect();
        let png = encoder(l, h, &px).expect("png");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        // Les blocs se suivent, chacun avec un CRC juste ; on reprend les données de IDAT
        let mut i = 8;
        let mut genres = Vec::new();
        let mut idat = Vec::new();
        while i < png.len() {
            let n = u32::from_be_bytes(png[i..i + 4].try_into().unwrap()) as usize;
            let genre = &png[i + 4..i + 8];
            let donnees = &png[i + 8..i + 8 + n];
            let crc = u32::from_be_bytes(png[i + 8 + n..i + 12 + n].try_into().unwrap());
            assert_eq!(crc, crc32(&png[i + 4..i + 8 + n]), "CRC du bloc {}", String::from_utf8_lossy(genre));
            if genre == b"IHDR" {
                assert_eq!(u32::from_be_bytes(donnees[0..4].try_into().unwrap()), l);
                assert_eq!(u32::from_be_bytes(donnees[4..8].try_into().unwrap()), h);
                assert_eq!(&donnees[8..], &[8, 6, 0, 0, 0]);
            }
            if genre == b"IDAT" {
                idat.extend_from_slice(donnees);
            }
            genres.push(String::from_utf8_lossy(genre).to_string());
            i += 12 + n;
        }
        assert_eq!(genres, ["IHDR", "IDAT", "IEND"]);
        // zlib stocké : on relit les blocs et on retrouve chaque ligne, précédée de son filtre 0
        assert_eq!(&idat[..2], &[0x78, 0x01]);
        let mut j = 2;
        let mut brut = Vec::new();
        loop {
            let dernier = idat[j] == 1;
            let n = u16::from_le_bytes([idat[j + 1], idat[j + 2]]);
            assert_eq!(u16::from_le_bytes([idat[j + 3], idat[j + 4]]), !n);
            brut.extend_from_slice(&idat[j + 5..j + 5 + n as usize]);
            j += 5 + n as usize;
            if dernier {
                break;
            }
        }
        assert_eq!(u32::from_be_bytes(idat[j..j + 4].try_into().unwrap()), adler32(&brut));
        assert_eq!(brut.len(), ((l * 4 + 1) * h) as usize);
        for (k, ligne) in brut.chunks_exact((l * 4 + 1) as usize).enumerate() {
            assert_eq!(ligne[0], 0);
            assert_eq!(&ligne[1..], &px[k * (l as usize) * 4..(k + 1) * (l as usize) * 4]);
        }
        // Une taille qui ne correspond pas : rien
        assert!(encoder(2, 2, &[0; 15]).is_none());
        assert!(encoder(0, 2, &[]).is_none());
    }
}
