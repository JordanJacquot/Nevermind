//! Les touches comme les voit la version PowerShell : le nom d'une touche WPF (System.Windows.Input.Key) et son code
//! Windows (KeyInterop.VirtualKeyFromKey), la lecture d'un nom par `[System.Windows.Input.Key]"texte"` (le KeyConverter
//! de WPF, puis Enum.Parse de .NET Framework), et le nom d'une touche appuyée (KeyInterop.KeyFromVirtualKey).
//! Tables relevées sur .NET Framework 4.8 (outils\exporter-juges-organizer.ps1 les vérifie toutes).

/// Tous les noms de l'énumération Key (alias compris), avec leur valeur.
const NOMS: &[(&str, i64)] = &[
    ("None", 0),
    ("Cancel", 1),
    ("Back", 2),
    ("Tab", 3),
    ("LineFeed", 4),
    ("Clear", 5),
    ("Return", 6),
    ("Enter", 6),
    ("Pause", 7),
    ("Capital", 8),
    ("CapsLock", 8),
    ("KanaMode", 9),
    ("HangulMode", 9),
    ("JunjaMode", 10),
    ("FinalMode", 11),
    ("HanjaMode", 12),
    ("KanjiMode", 12),
    ("Escape", 13),
    ("ImeConvert", 14),
    ("ImeNonConvert", 15),
    ("ImeAccept", 16),
    ("ImeModeChange", 17),
    ("Space", 18),
    ("Prior", 19),
    ("PageUp", 19),
    ("Next", 20),
    ("PageDown", 20),
    ("End", 21),
    ("Home", 22),
    ("Left", 23),
    ("Up", 24),
    ("Right", 25),
    ("Down", 26),
    ("Select", 27),
    ("Print", 28),
    ("Execute", 29),
    ("Snapshot", 30),
    ("PrintScreen", 30),
    ("Insert", 31),
    ("Delete", 32),
    ("Help", 33),
    ("D0", 34),
    ("D1", 35),
    ("D2", 36),
    ("D3", 37),
    ("D4", 38),
    ("D5", 39),
    ("D6", 40),
    ("D7", 41),
    ("D8", 42),
    ("D9", 43),
    ("A", 44),
    ("B", 45),
    ("C", 46),
    ("D", 47),
    ("E", 48),
    ("F", 49),
    ("G", 50),
    ("H", 51),
    ("I", 52),
    ("J", 53),
    ("K", 54),
    ("L", 55),
    ("M", 56),
    ("N", 57),
    ("O", 58),
    ("P", 59),
    ("Q", 60),
    ("R", 61),
    ("S", 62),
    ("T", 63),
    ("U", 64),
    ("V", 65),
    ("W", 66),
    ("X", 67),
    ("Y", 68),
    ("Z", 69),
    ("LWin", 70),
    ("RWin", 71),
    ("Apps", 72),
    ("Sleep", 73),
    ("NumPad0", 74),
    ("NumPad1", 75),
    ("NumPad2", 76),
    ("NumPad3", 77),
    ("NumPad4", 78),
    ("NumPad5", 79),
    ("NumPad6", 80),
    ("NumPad7", 81),
    ("NumPad8", 82),
    ("NumPad9", 83),
    ("Multiply", 84),
    ("Add", 85),
    ("Separator", 86),
    ("Subtract", 87),
    ("Decimal", 88),
    ("Divide", 89),
    ("F1", 90),
    ("F2", 91),
    ("F3", 92),
    ("F4", 93),
    ("F5", 94),
    ("F6", 95),
    ("F7", 96),
    ("F8", 97),
    ("F9", 98),
    ("F10", 99),
    ("F11", 100),
    ("F12", 101),
    ("F13", 102),
    ("F14", 103),
    ("F15", 104),
    ("F16", 105),
    ("F17", 106),
    ("F18", 107),
    ("F19", 108),
    ("F20", 109),
    ("F21", 110),
    ("F22", 111),
    ("F23", 112),
    ("F24", 113),
    ("NumLock", 114),
    ("Scroll", 115),
    ("LeftShift", 116),
    ("RightShift", 117),
    ("LeftCtrl", 118),
    ("RightCtrl", 119),
    ("LeftAlt", 120),
    ("RightAlt", 121),
    ("BrowserBack", 122),
    ("BrowserForward", 123),
    ("BrowserRefresh", 124),
    ("BrowserStop", 125),
    ("BrowserSearch", 126),
    ("BrowserFavorites", 127),
    ("BrowserHome", 128),
    ("VolumeMute", 129),
    ("VolumeDown", 130),
    ("VolumeUp", 131),
    ("MediaNextTrack", 132),
    ("MediaPreviousTrack", 133),
    ("MediaStop", 134),
    ("MediaPlayPause", 135),
    ("LaunchMail", 136),
    ("SelectMedia", 137),
    ("LaunchApplication1", 138),
    ("LaunchApplication2", 139),
    ("Oem1", 140),
    ("OemSemicolon", 140),
    ("OemPlus", 141),
    ("OemComma", 142),
    ("OemMinus", 143),
    ("OemPeriod", 144),
    ("Oem2", 145),
    ("OemQuestion", 145),
    ("Oem3", 146),
    ("OemTilde", 146),
    ("AbntC1", 147),
    ("AbntC2", 148),
    ("Oem4", 149),
    ("OemOpenBrackets", 149),
    ("Oem5", 150),
    ("OemPipe", 150),
    ("Oem6", 151),
    ("OemCloseBrackets", 151),
    ("Oem7", 152),
    ("OemQuotes", 152),
    ("Oem8", 153),
    ("Oem102", 154),
    ("OemBackslash", 154),
    ("ImeProcessed", 155),
    ("System", 156),
    ("OemAttn", 157),
    ("DbeAlphanumeric", 157),
    ("OemFinish", 158),
    ("DbeKatakana", 158),
    ("OemCopy", 159),
    ("DbeHiragana", 159),
    ("OemAuto", 160),
    ("DbeSbcsChar", 160),
    ("OemEnlw", 161),
    ("DbeDbcsChar", 161),
    ("OemBackTab", 162),
    ("DbeRoman", 162),
    ("Attn", 163),
    ("DbeNoRoman", 163),
    ("CrSel", 164),
    ("DbeEnterWordRegisterMode", 164),
    ("ExSel", 165),
    ("DbeEnterImeConfigureMode", 165),
    ("EraseEof", 166),
    ("DbeFlushString", 166),
    ("Play", 167),
    ("DbeCodeInput", 167),
    ("Zoom", 168),
    ("DbeNoCodeInput", 168),
    ("NoName", 169),
    ("DbeDetermineString", 169),
    ("Pa1", 170),
    ("DbeEnterDialogConversionMode", 170),
    ("OemClear", 171),
    ("DeadCharProcessed", 172),
];

/// Le code Windows de chaque valeur de Key, de 0 à 172 (KeyInterop.VirtualKeyFromKey ; 0 : aucun).
const VK: [u8; 173] = [
    0, 3, 8, 9, 0, 12, 13, 19, 20, 21, 23, 24, 25, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43,
    44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79,
    80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107,
    108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130,
    131, 132, 133, 134, 135, 144, 145, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 175,
    176, 177, 178, 179, 180, 181, 182, 183, 186, 187, 188, 189, 190, 191, 192, 193, 194, 219, 220, 221, 222, 223, 226,
    229, 0, 240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 0,
];

/// Le nom donné par `[string]` à chaque valeur (le premier nom que .NET choisit pour un alias).
fn nom_de_valeur(v: i64) -> Option<&'static str> {
    // (pour les alias, .NET Framework rend le nom relevé dans la table KeyFromVirtualKey ci-dessous)
    const CHOIX: &[(i64, &str)] = &[
        (6, "Return"),
        (8, "Capital"),
        (9, "KanaMode"),
        (12, "HanjaMode"),
        (19, "PageUp"),
        (20, "Next"),
        (30, "Snapshot"),
        (140, "Oem1"),
        (145, "OemQuestion"),
        (146, "Oem3"),
        (149, "OemOpenBrackets"),
        (150, "Oem5"),
        (151, "Oem6"),
        (152, "OemQuotes"),
        (154, "OemBackslash"),
        (157, "OemAttn"),
        (158, "OemFinish"),
        (159, "OemCopy"),
        (160, "DbeSbcsChar"),
        (161, "OemEnlw"),
        (162, "OemBackTab"),
        (163, "DbeNoRoman"),
        (164, "DbeEnterWordRegisterMode"),
        (165, "DbeEnterImeConfigureMode"),
        (166, "EraseEof"),
        (167, "Play"),
        (168, "DbeNoCodeInput"),
        (169, "NoName"),
        (170, "Pa1"),
    ];
    if let Some((_, n)) = CHOIX.iter().find(|(x, _)| *x == v) {
        return Some(n);
    }
    NOMS.iter().find(|(_, x)| *x == v).map(|(n, _)| *n)
}

/// Le code Windows d'une valeur de Key (0 pour une valeur hors de l'énumération).
pub fn vk_de_cle(v: i64) -> u32 {
    if (0..VK.len() as i64).contains(&v) { u32::from(VK[v as usize]) } else { 0 }
}

/// La touche WPF d'un code Windows (KeyInterop.KeyFromVirtualKey), par son nom (`[string]`).
pub fn nom_de_vk(vk: u32) -> &'static str {
    let v = match vk {
        16 => 116, // (Maj, Ctrl et Alt sans côté : ceux de gauche)
        17 => 118,
        18 => 120,
        _ => VK.iter().position(|&x| x != 0 && u32::from(x) == vk).map(|p| p as i64).unwrap_or(0),
    };
    nom_de_valeur(v).unwrap_or("None")
}

/// `ToUpper` de .NET (correspondance simple, lettre par lettre).
fn majuscule(c: char) -> char {
    // (le i sans point turc reste tel quel pour .NET)
    if c == '\u{131}' {
        return c;
    }
    let mut u = c.to_uppercase();
    match (u.next(), u.next()) {
        (Some(x), None) => x,
        _ => c,
    }
}

/// `char.IsLetterOrDigit` et `char.IsDigit` de .NET.
fn lettre_ou_chiffre(c: char) -> bool {
    crate::texte::lettre_ou_chiffre(c)
}

/// `Enum.Parse(typeof(Key), texte, ignoreCase: true)` de .NET Framework 4.8 : `None` si .NET lève une erreur.
fn enum_parse(texte: &str) -> Option<i64> {
    let t = texte.trim();
    let premier = t.chars().next()?;
    if is_nd(premier) || premier == '-' || premier == '+' {
        // (Convert.ChangeType vers Int32 : un nombre trop grand est une erreur, un texte qui n'est pas un nombre
        // passe aux noms)
        match entier_dotnet(t) {
            Ok(n) => return Some(n),
            Err(true) => return None,
            Err(false) => {}
        }
    }
    let mut r: i64 = 0;
    for part in t.split(',') {
        let p = part.trim();
        let v = NOMS.iter().find(|(n, _)| egal_sans_casse(n, p))?;
        r |= v.1;
    }
    Some(r)
}

fn is_nd(c: char) -> bool {
    use unicode_general_category::{GeneralCategory, get_general_category};
    get_general_category(c) == GeneralCategory::DecimalNumber
}

/// Int32.Parse(texte, NumberStyles.Integer, Invariant) : `Err(true)` trop grand, `Err(false)` pas un nombre.
fn entier_dotnet(t: &str) -> Result<i64, bool> {
    let (signe, chiffres) = match t.as_bytes().first() {
        Some(b'-') => (-1i64, &t[1..]),
        Some(b'+') => (1, &t[1..]),
        _ => (1, t),
    };
    if chiffres.is_empty() || !chiffres.bytes().all(|b| b.is_ascii_digit()) {
        return Err(false);
    }
    let mut n: i64 = 0;
    for b in chiffres.bytes() {
        n = n * 10 + i64::from(b - b'0');
        if n > 2_147_483_648 {
            return Err(true);
        }
    }
    let n = signe * n;
    if n > i64::from(i32::MAX) || n < i64::from(i32::MIN) {
        return Err(true);
    }
    Ok(n)
}

/// StringComparison.OrdinalIgnoreCase (les noms de Key sont en ASCII).
fn egal_sans_casse(a: &str, b: &str) -> bool {
    a.chars().count() == b.chars().count() && a.chars().zip(b.chars()).all(|(x, y)| majuscule(x) == majuscule(y))
}

/// `[System.Windows.Input.Key]"texte"` de PowerShell : la valeur de la touche, `None` si PowerShell lève une erreur.
/// C'est le KeyConverter de WPF : une lettre ou un chiffre seuls, quelques noms courts (« Esc », « PgUp »...), sinon
/// Enum.Parse (sans tenir compte de la casse, avec les nombres et les noms séparés par des virgules).
pub fn cle_wpf(texte: &str) -> Option<i64> {
    let t = texte.trim();
    if t.is_empty() {
        return Some(0);
    }
    let haut: String = t.chars().map(majuscule).collect();
    let mut cs = haut.chars();
    if let (Some(c), None) = (cs.next(), cs.next())
        && lettre_ou_chiffre(c)
    {
        if c.is_ascii_digit() {
            return Some(34 + i64::from(c as u8 - b'0'));
        }
        if c.is_ascii_uppercase() {
            return Some(44 + i64::from(c as u8 - b'A'));
        }
        return None;
    }
    let v = match haut.as_str() {
        "ENTER" => 6,
        "ESC" => 13,
        "PGUP" => 19,
        "PGDN" => 20,
        "PRTSC" => 30,
        "INS" => 31,
        "DEL" => 32,
        "WINDOWS" | "WIN" | "LEFTWINDOWS" => 70,
        "RIGHTWINDOWS" => 71,
        "APPS" | "APPLICATION" => 72,
        "BREAK" => 1,
        "BACKSPACE" | "BKSP" | "BS" => 2,
        "SHIFT" | "LEFTSHIFT" => 116,
        "RIGHTSHIFT" => 117,
        "CONTROL" | "CTRL" | "LEFTCTRL" => 118,
        "RIGHTCTRL" => 119,
        "ALT" | "LEFTALT" => 120,
        "RIGHTALT" => 121,
        "SEMICOLON" => 140,
        "PLUS" => 141,
        "COMMA" => 142,
        "MINUS" => 143,
        "PERIOD" => 144,
        "QUESTION" => 145,
        "TILDE" => 146,
        "OPENBRACKETS" => 149,
        "PIPE" => 150,
        "CLOSEBRACKETS" => 151,
        "QUOTES" => 152,
        "BACKSLASH" => 154,
        "FINISH" => 158,
        "ATTN" => 163,
        "CRSEL" => 164,
        "EXSEL" => 165,
        "ERASEEOF" => 166,
        "PLAY" => 167,
        "ZOOM" => 168,
        "PA1" => 170,
        _ => enum_parse(&haut)?,
    };
    // (KeyConverter refuse la valeur -1)
    (v != -1).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noms() {
        assert_eq!(cle_wpf("5"), Some(39));
        assert_eq!(cle_wpf("05"), Some(5));
        assert_eq!(cle_wpf("a,b"), Some(45));
        assert_eq!(cle_wpf("-1"), None);
        assert_eq!(cle_wpf("é"), None);
        assert_eq!(cle_wpf(""), Some(0));
        assert_eq!(vk_de_cle(cle_wpf("Tab").unwrap()), 9);
        assert_eq!(nom_de_vk(34), "Next");
        assert_eq!(nom_de_vk(192), "Oem3");
        assert_eq!(nom_de_vk(16), "LeftShift");
    }
}
