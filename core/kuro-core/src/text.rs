//! Normalizace názvů a podobnost řetězců (NSC `lib/text.ts`).

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::rx::rx_cs;

/// Odstraní diakritiku: "Měsíční Údolí" -> "Mesicni Udoli".
pub fn strip_diacritics(s: &str) -> String {
    s.nfd()
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
        .collect()
}

static NON_ALNUM: LazyLock<Regex> = LazyLock::new(|| rx_cs(r"[^a-z0-9]+"));
static ARTICLES: LazyLock<Regex> =
    LazyLock::new(|| rx_cs(r"\b(the|a|an|le|la|les|der|die|das|el|los)\b"));

/// Kanonický tvar názvu pro porovnávání: lowercase, bez diakritiky,
/// bez interpunkce, jednoduché mezery.
///
/// "The Lord of the Rings: The Two Towers" -> "the lord of the rings the two towers"
pub fn normalize_title(s: &str) -> String {
    let lowered = strip_diacritics(s)
        .to_lowercase()
        .replace('&', " and ")
        .replace(['\'', '’', '`'], "");
    collapse(&NON_ALNUM.replace_all(&lowered, " "))
}

/// Jako [`normalize_title`], ale zahazuje i členy — poslední záchrana při matchování.
pub fn normalize_loose(s: &str) -> String {
    let n = normalize_title(s);
    collapse(&ARTICLES.replace_all(&n, " "))
}

/// "rocky iii" -> "rocky 3". Sjednocuje sequely, kde se release scéna
/// a katalog neshodnou na zápisu čísla dílu.
pub fn normalize_numerals(s: &str) -> String {
    s.split(' ')
        .map(|w| match w {
            "i" => "1",
            "ii" => "2",
            "iii" => "3",
            "iv" => "4",
            "v" => "5",
            "vi" => "6",
            "vii" => "7",
            "viii" => "8",
            "ix" => "9",
            "x" => "10",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Levenshteinova vzdálenost, iterativní se dvěma řádky.
pub fn levenshtein(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];

    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (curr[j] + 1).min(prev[j + 1] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Podobnost 0..1 na už normalizovaných řetězcích.
pub fn similarity(a: &str, b: &str) -> f64 {
    let max = a.chars().count().max(b.chars().count());
    if max == 0 {
        return 1.0;
    }
    1.0 - levenshtein(a, b) as f64 / max as f64
}

/// Ořízne a sloučí mezery.
pub(crate) fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes() {
        assert_eq!(
            normalize_title("The Lord of the Rings: The Two Towers"),
            "the lord of the rings the two towers"
        );
        assert_eq!(normalize_title("Měsíční Údolí"), "mesicni udoli");
        assert_eq!(normalize_title("Tom & Jerry"), "tom and jerry");
        assert_eq!(normalize_title("Ocean's Eleven"), "oceans eleven");
        assert_eq!(normalize_loose("The Matrix"), "matrix");
        assert_eq!(normalize_numerals("rocky iii"), "rocky 3");
    }

    #[test]
    fn distance() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert!((similarity("interstelar", "interstellar") - (1.0 - 1.0 / 12.0)).abs() < 1e-9);
    }
}
