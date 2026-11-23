//! Párování souborů od providerů k titulům katalogu (NSC `matcher/`).

pub mod languages;
pub mod parse;
pub mod score;

pub use languages::{detect_languages, DetectedLanguages};
pub use parse::{parse_release, Hdr, ParsedRelease};
pub use score::{score_match, size_sanity, stream_sort_key, MatchResult, MatchTarget, MediaKind, SortInput};
