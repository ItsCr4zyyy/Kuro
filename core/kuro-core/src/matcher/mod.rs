//! Párování souborů od providerů k titulům katalogu (NSC `matcher/`).

pub mod languages;
pub mod parse;
pub mod score;

pub use languages::{detect_languages, DetectedLanguages};
pub use parse::{parse_release, Hdr, ParsedRelease};
pub use score::{score_match, size_sanity, stream_sort_key, MatchResult, MatchTarget, MediaKind, SortInput};

/// Verze pravidel párování. Zvýšit při každé změně, která mění, KTERÉ
/// soubory k titulu patří — úložiště pak zahodí uložené zdroje (jinak by
/// v cache zůstaly špatně spárované, protože prázdné hledání je nemaže).
pub const MATCHER_VERSION: u32 = 3;
