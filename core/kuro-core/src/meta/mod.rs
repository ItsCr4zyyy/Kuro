//! Metadata katalogu: Cinemeta (katalog, detail, epizody) a Wikidata
//! (české/slovenské názvy). AniList pro anime přibude později.

pub mod cinemeta;
pub mod wikidata;

pub use cinemeta::{CatalogId, Cinemeta};
pub use wikidata::{LocalizedTitles, Wikidata};

use crate::matcher::MediaKind;

/// Titul v doménovém tvaru, nezávislý na zdroji metadat.
#[derive(Debug, Clone, PartialEq)]
pub struct TitleMeta {
    pub imdb_id: String,
    pub kind: MediaKind,
    pub title_orig: String,
    /// Český název z Wikidat, pokud ho známe.
    pub title_cs: Option<String>,
    /// Další názvy pro matcher (cs/sk z Wikidat, romaji z AniListu).
    pub aliases: Vec<String>,
    pub year: Option<u16>,
    pub overview: Option<String>,
    pub poster: Option<String>,
    pub background: Option<String>,
    pub logo: Option<String>,
    /// Minuty; u seriálů délka epizody.
    pub runtime_min: Option<u32>,
    pub rating: Option<f32>,
    pub genres: Vec<String>,
    pub countries: Vec<String>,
    pub anime: bool,
    /// YYYY-MM-DD
    pub release_date: Option<String>,
    pub popularity: f64,
    /// Jen u detailu seriálu; seřazené podle (sezóna, epizoda), bez speciálů.
    pub episodes: Vec<EpisodeMeta>,
}

impl TitleMeta {
    /// Název k zobrazení: český, když ho máme.
    pub fn display_title(&self) -> &str {
        self.title_cs.as_deref().unwrap_or(&self.title_orig)
    }

    /// Doplní české názvy z Wikidat.
    pub fn apply_localized(&mut self, loc: &LocalizedTitles) {
        if let Some(cs) = &loc.cs {
            self.title_cs = Some(cs.clone());
        }
        for a in loc.aliases(&self.title_orig) {
            if !self.aliases.contains(&a) {
                self.aliases.push(a);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EpisodeMeta {
    pub season: u16,
    pub episode: u16,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub still: Option<String>,
    /// YYYY-MM-DD
    pub air_date: Option<String>,
}
