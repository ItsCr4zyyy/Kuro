//! Překlad regexů z NSC (JavaScript) do crate `regex`.
//!
//! Rozdíl, na kterém by se to tiše rozbilo: v JS jsou `\b` a `\d` ASCII,
//! v `regex` Unicode. Název "Predátor" by pak měl hranici slova jinde
//! a "á" by se bralo jako písmeno. `rx()` proto `\b` a `\d` převádí na
//! ASCII varianty, takže vzory zůstávají čitelné 1:1 s originálem.

use regex::{Regex, RegexBuilder};

/// Zkompiluje vzor s JS sémantikou `\b`/`\d`, bez ohledu na velikost písmen.
pub(crate) fn rx(pattern: &str) -> Regex {
    build(pattern, true)
}

/// Totéž, ale citlivé na velikost písmen.
pub(crate) fn rx_cs(pattern: &str) -> Regex {
    build(pattern, false)
}

fn build(pattern: &str, ci: bool) -> Regex {
    let translated = pattern.replace(r"\b", r"(?-u:\b)").replace(r"\d", "[0-9]");
    RegexBuilder::new(&translated)
        .case_insensitive(ci)
        .build()
        .unwrap_or_else(|e| panic!("neplatný regex {pattern:?}: {e}"))
}
