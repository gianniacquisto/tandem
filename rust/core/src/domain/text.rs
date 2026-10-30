//! The one text comparison, shared by everything that matches a human's word
//! against stored text (`TextMatch.swift`, ADR 0008's "one rule, one place").
//!
//! The query and, later, the quick-open palette match the same way: a case- and
//! diacritic-insensitive substring, so "app" finds the same things everywhere.
//!
//! No Unicode library: the fold covers the Latin letters a title actually carries,
//! which is the rule the Swift `String.CompareOptions` call gave with a platform
//! table behind it. A letter outside the table still matches case-insensitively;
//! only its accents are not folded, and that is a narrowing, never a crash.

/// The comparison options, in one place: case-insensitive and diacritic-insensitive.
pub struct TextMatch;

impl TextMatch {
    /// Whether `haystack` contains `needle` under those options.
    pub fn contains(needle: &str, haystack: &str) -> bool {
        fold(haystack).contains(&fold(needle))
    }
}

/// `text` lowercased, with the Latin accents folded to their base letters.
fn fold(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for character in text.chars() {
        // A combining mark on its own carries no letter: the base letter already
        // stands for it.
        if ('\u{0300}'..='\u{036f}').contains(&character) {
            continue;
        }
        for lower in character.to_lowercase() {
            match lower {
                'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => folded.push('a'),
                'æ' => folded.push_str("ae"),
                'ç' | 'ć' | 'č' => folded.push('c'),
                'ď' => folded.push('d'),
                'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => folded.push('e'),
                'ĝ' | 'ğ' => folded.push('g'),
                'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'į' => folded.push('i'),
                'ł' => folded.push('l'),
                'ñ' | 'ń' | 'ň' => folded.push('n'),
                'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => folded.push('o'),
                'œ' => folded.push_str("oe"),
                'ř' => folded.push('r'),
                'ś' | 'š' | 'ş' => folded.push('s'),
                'ß' => folded.push_str("ss"),
                'ť' => folded.push('t'),
                'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ů' | 'ű' => folded.push('u'),
                'ý' | 'ÿ' => folded.push('y'),
                'ź' | 'ż' | 'ž' => folded.push('z'),
                other => folded.push(other),
            }
        }
    }
    folded
}
