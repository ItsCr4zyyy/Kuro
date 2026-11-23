//! Cinemeta — veřejný katalog metadat od Stremia (NSC `indexer/cinemeta.ts`).
//!
//! Bez API klíče a registrace. Stojí na IMDb ID, má plakáty, popisy,
//! hodnocení, žánry i seznam epizod. České názvy nemá — ty dotahuje
//! `wikidata.rs`.

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use regex::Regex;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use tokio::sync::Semaphore;

use super::{EpisodeMeta, TitleMeta};
use crate::error::{Error, Result};
use crate::matcher::MediaKind;
use crate::rx::rx;

const BASE: &str = "https://v3-cinemeta.strem.io";

/// Stejná množina jako JS `encodeURIComponent`.
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

fn enc(s: &str) -> String {
    utf8_percent_encode(s, COMPONENT).to_string()
}

pub fn type_path(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Movie => "movie",
        MediaKind::Series => "series",
    }
}

/// Katalogy, které Cinemeta nabízí.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogId {
    /// Populární.
    Top,
    /// "Featured" — doporučené (Cinemeta je řadí podle hodnocení a popularity).
    ImdbRating,
    /// Novinky: povinný parametr `genre` = rok ("2026").
    Year,
}

impl CatalogId {
    fn path(self) -> &'static str {
        match self {
            CatalogId::Top => "top",
            CatalogId::ImdbRating => "imdbRating",
            CatalogId::Year => "year",
        }
    }
}

#[derive(Clone)]
pub struct Cinemeta {
    http: reqwest::Client,
    base: String,
    /// Nemá dokumentovaný limit, ale 12 souběžných je slušné chování.
    limiter: Arc<Semaphore>,
}

impl Cinemeta {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http, base: BASE.into(), limiter: Arc::new(Semaphore::new(12)) }
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let _permit = self.limiter.acquire().await.expect("semafor se nezavírá");
        let url = format!("{}{path}", self.base);
        for attempt in 0..3u32 {
            let res = match self.http.get(&url).header("Accept", "application/json").send().await {
                Ok(r) => r,
                Err(e) if attempt < 2 => {
                    let _ = e;
                    tokio::time::sleep(Duration::from_millis(1000 << attempt)).await;
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            let status = res.status().as_u16();
            if status == 429 || status >= 500 {
                tokio::time::sleep(Duration::from_millis(1000 << attempt)).await;
                continue;
            }
            if status == 404 {
                return Err(Error::NotFound);
            }
            if !res.status().is_success() {
                return Err(Error::Http { status, context: format!("cinemeta {path}") });
            }
            return Ok(res.json::<T>().await?);
        }
        Err(Error::Http { status: 503, context: format!("cinemeta nedostupná po 3 pokusech: {path}") })
    }

    /// Stránka katalogu (~50 položek). Stránkuje se `skip`, ne číslem stránky.
    pub async fn catalog(
        &self,
        kind: MediaKind,
        catalog: CatalogId,
        genre: Option<&str>,
        skip: u32,
    ) -> Result<Vec<TitleMeta>> {
        let mut extras = vec![];
        if let Some(g) = genre {
            extras.push(format!("genre={}", enc(g)));
        }
        if skip > 0 {
            extras.push(format!("skip={skip}"));
        }
        let suffix = if extras.is_empty() { String::new() } else { format!("/{}", extras.join("&")) };
        let path = format!("/catalog/{}/{}{suffix}.json", type_path(kind), catalog.path());
        let data: Metas = self.get(&path).await?;
        Ok(data.metas.into_iter().filter_map(|m| m.normalize(kind)).collect())
    }

    /// Fulltextové hledání v Cinemetě.
    pub async fn search(&self, kind: MediaKind, query: &str) -> Result<Vec<TitleMeta>> {
        let path = format!("/catalog/{}/top/search={}.json", type_path(kind), enc(query));
        let data: Metas = self.get(&path).await?;
        Ok(data.metas.into_iter().filter_map(|m| m.normalize(kind)).collect())
    }

    /// Detail včetně epizod. Neznámé ID vrací `Ok(None)`.
    ///
    /// Pozor: na neznámé IMDb ID Cinemeta nevrací 404, ale HTTP 200
    /// s prázdným záznamem bez názvu.
    pub async fn meta(&self, kind: MediaKind, imdb_id: &str) -> Result<Option<TitleMeta>> {
        let path = format!("/meta/{}/{}.json", type_path(kind), enc(imdb_id));
        match self.get::<MetaEnvelope>(&path).await {
            Ok(env) => Ok(env.meta.and_then(|m| m.normalize(kind))),
            Err(Error::NotFound) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/* ------------------------------ JSON tvary ------------------------------ */

#[derive(Deserialize)]
struct Metas {
    #[serde(default)]
    metas: Vec<RawMeta>,
}

#[derive(Deserialize)]
struct MetaEnvelope {
    meta: Option<RawMeta>,
}

/// Cinemeta posílá čísla jednou jako číslo, jindy jako řetězec. Všechno
/// riskantní se proto čte tolerantně.
#[derive(Deserialize, Default)]
#[serde(default)]
pub(crate) struct RawMeta {
    #[serde(deserialize_with = "stringish")]
    id: Option<String>,
    #[serde(deserialize_with = "stringish")]
    imdb_id: Option<String>,
    #[serde(deserialize_with = "stringish")]
    name: Option<String>,
    #[serde(deserialize_with = "stringish")]
    poster: Option<String>,
    #[serde(deserialize_with = "stringish")]
    background: Option<String>,
    #[serde(deserialize_with = "stringish")]
    logo: Option<String>,
    #[serde(deserialize_with = "stringish")]
    description: Option<String>,
    #[serde(rename = "releaseInfo", deserialize_with = "stringish")]
    release_info: Option<String>,
    #[serde(deserialize_with = "stringish")]
    released: Option<String>,
    #[serde(deserialize_with = "stringish")]
    year: Option<String>,
    #[serde(deserialize_with = "stringish")]
    runtime: Option<String>,
    #[serde(rename = "imdbRating", deserialize_with = "stringish")]
    imdb_rating: Option<String>,
    #[serde(deserialize_with = "string_list")]
    genre: Vec<String>,
    #[serde(deserialize_with = "string_list")]
    genres: Vec<String>,
    #[serde(deserialize_with = "stringish")]
    country: Option<String>,
    #[serde(deserialize_with = "number")]
    popularity: Option<f64>,
    videos: Vec<RawVideo>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawVideo {
    #[serde(deserialize_with = "stringish")]
    name: Option<String>,
    #[serde(deserialize_with = "number")]
    season: Option<f64>,
    #[serde(deserialize_with = "number")]
    number: Option<f64>,
    #[serde(deserialize_with = "number")]
    episode: Option<f64>,
    #[serde(rename = "firstAired", deserialize_with = "stringish")]
    first_aired: Option<String>,
    #[serde(deserialize_with = "stringish")]
    released: Option<String>,
    #[serde(deserialize_with = "stringish")]
    overview: Option<String>,
    #[serde(deserialize_with = "stringish")]
    description: Option<String>,
    #[serde(deserialize_with = "stringish")]
    thumbnail: Option<String>,
}

fn stringish<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<String>, D::Error> {
    Ok(match Option::<Value>::deserialize(d)? {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    })
}

fn number<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<f64>, D::Error> {
    Ok(match Option::<Value>::deserialize(d)? {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    })
}

fn string_list<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<String>, D::Error> {
    Ok(match Option::<Value>::deserialize(d)? {
        Some(Value::Array(a)) => {
            a.into_iter().filter_map(|v| v.as_str().map(str::to_owned)).filter(|s| !s.is_empty()).collect()
        }
        Some(Value::String(s)) => s.split(',').map(|x| x.trim().to_owned()).filter(|x| !x.is_empty()).collect(),
        _ => vec![],
    })
}

/* ------------------------------ převody ------------------------------ */

static YEAR4: LazyLock<Regex> = LazyLock::new(|| rx(r"(\d{4})"));
static HOURS: LazyLock<Regex> = LazyLock::new(|| rx(r"(\d+)\s*h"));
static MINUTES: LazyLock<Regex> = LazyLock::new(|| rx(r"(\d+)\s*min"));
static BARE: LazyLock<Regex> = LazyLock::new(|| rx(r"(\d+)"));

/// "148 min" → 148, "1h 30min" → 90.
pub fn parse_runtime(raw: Option<&str>) -> Option<u32> {
    let raw = raw?;
    let h = HOURS.captures(raw).and_then(|c| c[1].parse::<u32>().ok());
    let m = MINUTES.captures(raw).and_then(|c| c[1].parse::<u32>().ok());
    if h.is_some() || m.is_some() {
        return Some(h.unwrap_or(0) * 60 + m.unwrap_or(0));
    }
    BARE.captures(raw).and_then(|c| c[1].parse().ok())
}

/// Rok vydání. `releaseInfo` bývá "2008–2013" nebo "2008–" — bere se první.
fn parse_year(candidates: &[Option<&str>]) -> Option<u16> {
    let max = crate::time::current_year() + 5;
    candidates.iter().flatten().find_map(|c| {
        YEAR4
            .captures(c)
            .and_then(|m| m[1].parse::<u16>().ok())
            .filter(|y| (1870..=max).contains(y))
    })
}

/// Prvních `n` znaků (ne bajtů — řez uprostřed UTF-8 znaku by spadl).
fn prefix(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map_or(s, |(i, _)| &s[..i])
}

impl RawMeta {
    pub(crate) fn normalize(self, kind: MediaKind) -> Option<TitleMeta> {
        let imdb_id = self.imdb_id.or(self.id).filter(|id| id.starts_with("tt"))?;
        let name = self.name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty())?;

        let year = parse_year(&[
            self.year.as_deref(),
            self.release_info.as_deref(),
            self.released.as_deref().map(|r| prefix(r, 4)),
        ]);

        let genres: Vec<String> =
            if self.genre.is_empty() { self.genres } else { self.genre }.into_iter().take(8).collect();
        let countries: Vec<String> = self
            .country
            .as_deref()
            .map(|c| c.split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).take(5).collect())
            .unwrap_or_default();

        // Animovaný titul z Japonska. Samotné "Animation" by chytlo
        // Simpsonovy, samotné "Japan" zase hrané filmy.
        let anime = genres.iter().any(|g| g == "Animation") && countries.iter().any(|c| c.contains("Japan"));

        let release_date = self
            .released
            .as_deref()
            .map(|r| prefix(r, 10))
            .filter(|d| d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-')
            .map(str::to_owned)
            .or_else(|| year.map(|y| format!("{y}-01-01")));

        let mut episodes: Vec<EpisodeMeta> = self
            .videos
            .into_iter()
            .filter_map(|v| {
                let season = v.season? as u16;
                let episode = v.number.or(v.episode)? as u16;
                // Speciály (sezóna 0) se párují mizerně — přeskočit.
                if season == 0 {
                    return None;
                }
                Some(EpisodeMeta {
                    season,
                    episode,
                    name: v.name,
                    overview: v.overview.or(v.description),
                    still: v.thumbnail,
                    air_date: v.first_aired.or(v.released).map(|d| d.chars().take(10).collect()),
                })
            })
            .collect();
        episodes.sort_by_key(|e| (e.season, e.episode));
        episodes.dedup_by_key(|e| (e.season, e.episode));

        Some(TitleMeta {
            imdb_id,
            kind,
            title_orig: name,
            title_cs: None,
            aliases: vec![],
            year,
            overview: self.description,
            poster: self.poster,
            background: self.background,
            logo: self.logo,
            runtime_min: parse_runtime(self.runtime.as_deref()),
            rating: self.imdb_rating.and_then(|r| r.parse().ok()).filter(|r: &f32| *r > 0.0),
            genres,
            countries,
            anime,
            release_date,
            popularity: self.popularity.unwrap_or(0.0),
            episodes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime() {
        assert_eq!(parse_runtime(Some("148 min")), Some(148));
        assert_eq!(parse_runtime(Some("1h 30min")), Some(90));
        assert_eq!(parse_runtime(Some("2h")), Some(120));
        assert_eq!(parse_runtime(Some("47")), Some(47));
        assert_eq!(parse_runtime(None), None);
    }

    #[test]
    fn encodes_like_js() {
        assert_eq!(enc("Pán prstenů: Dvě věže"), "P%C3%A1n%20prsten%C5%AF%3A%20Dv%C4%9B%20v%C4%9B%C5%BEe");
    }

    #[test]
    fn normalizes_series() {
        let json = r#"{"meta":{"id":"tt0903747","type":"series","name":"Breaking Bad",
            "releaseInfo":"2008–2013","imdbRating":"9.5","runtime":"49 min",
            "genre":["Crime","Drama"],"country":"United States","popularity":12.5,
            "videos":[
              {"season":0,"number":1,"name":"Special"},
              {"season":1,"number":2,"name":"Cat's in the Bag...","released":"2008-01-27T05:00:00.000Z"},
              {"season":1,"episode":1,"name":"Pilot","firstAired":"2008-01-20T00:00:00.000Z"}
            ]}}"#;
        let env: MetaEnvelope = serde_json::from_str(json).unwrap();
        let m = env.meta.unwrap().normalize(MediaKind::Series).unwrap();
        assert_eq!(m.imdb_id, "tt0903747");
        assert_eq!(m.year, Some(2008));
        assert_eq!(m.rating, Some(9.5));
        assert_eq!(m.runtime_min, Some(49));
        assert!(!m.anime);
        assert_eq!(m.episodes.len(), 2);
        assert_eq!((m.episodes[0].season, m.episodes[0].episode), (1, 1));
        assert_eq!(m.episodes[0].air_date.as_deref(), Some("2008-01-20"));
    }

    #[test]
    fn empty_meta_is_none_and_anime_detected() {
        let env: MetaEnvelope = serde_json::from_str(r#"{"meta":{"id":"tt1","type":"movie"}}"#).unwrap();
        assert!(env.meta.unwrap().normalize(MediaKind::Movie).is_none());

        let json = r#"{"id":"tt0245429","name":"Spirited Away","year":2001,
            "genres":["Animation","Family"],"country":"Japan"}"#;
        let raw: RawMeta = serde_json::from_str(json).unwrap();
        let m = raw.normalize(MediaKind::Movie).unwrap();
        assert!(m.anime);
        assert_eq!(m.year, Some(2001));
        assert_eq!(m.release_date.as_deref(), Some("2001-01-01"));
    }
}
