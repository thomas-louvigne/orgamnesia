//! Page names as links compare them.

/// `c` without its accent, in lower case: 'É' → 'e', 'ç' → 'c'. One char for
/// one, so positions in a text stay valid (ligatures like 'œ' are kept).
pub fn fold_char(c: char) -> char {
    match c.to_lowercase().next().unwrap_or(c) {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' | 'ħ' => 'h',
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ţ' | 'ť' | 'ŧ' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        l => l,
    }
}

/// `s` in lower case and without accents, for comparing names and searching
/// loosely: "Élody" → "elody", "Ça" → "ca", "Œuvre" → "oeuvre", "Señor" → "senor".
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        match c {
            'æ' => out.push_str("ae"),
            'œ' => out.push_str("oe"),
            'ß' => out.push_str("ss"),
            // Accents typed as separate combining marks (decomposed text)
            '\u{300}'..='\u{36f}' => {}
            c => out.push(fold_char(c)),
        }
    }
    out
}

/// `s` as page names compare: loosely (`fold`) when links ignore case and
/// accents, else as is.
pub fn page_key(s: &str, loose: bool) -> String {
    if loose { fold(s) } else { s.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_and_accents() {
        assert_eq!(fold("Élody"), "elody");
        assert_eq!(fold("élody"), fold("elody"));
        assert_eq!(fold("Ça va à l'école"), "ca va a l'ecole");
        assert_eq!(fold("Señor Müller"), "senor muller");
        assert_eq!(fold("Œuvre Æther Straße"), "oeuvre aether strasse");
        assert_eq!(fold("e\u{301}lody"), "elody");
        assert_eq!(fold("Page 2 — notes"), "page 2 — notes");
    }

    #[test]
    fn fold_char_keeps_one_char() {
        assert_eq!("Règle".chars().map(fold_char).collect::<String>(), "regle");
        assert_eq!(fold_char('Ç'), 'c');
        assert_eq!(fold_char('œ'), 'œ');
    }

    #[test]
    fn strict_keeps_the_name() {
        assert_eq!(page_key("Élody", false), "Élody");
        assert_eq!(page_key("Élody", true), "elody");
    }
}
