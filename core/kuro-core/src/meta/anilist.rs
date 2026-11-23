//! AniList (GraphQL, bez přihlášení) — katalog anime a názvy pro párování.
//!
//! AniList nezná IMDb ID. Převod AniList → (IMDb seriál, sezóna) dělá
//! Wikidata (`Wikidata::anilist_map`), epizody a zdroje pak jdou stejnou
//! cestou jako ostatní tituly (Cinemeta + providery).
//!
//! Limit API je ~90 dotazů/min; Kuro dělá pár dotazů na otevření záložky.

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use super::TitleMeta;
use crate::error::{Error, Result};
use crate::matcher::MediaKind;

const ENDPOINT: &str = "https://graphql.anilist.co";

/// Řady v záložce Anime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimeList {
    /// Teď populární (trending).
    Trending,
    /// Běžící sezóna.
    Season,
    /// Nejpopulárnější vůbec.
    Popular,
    /// Nejlépe hodnocené.
    TopRated,
    /// Filmy.
    Movies,
}

/// Jedno anime z AniListu.
#[derive(Debug, Clone, PartialEq)]
pub struct AniMedia {
    pub id: u64,
    pub movie: bool,
    pub romaji: Option<String>,
    pub english: Option<String>,
    pub synonyms: Vec<String>,
    pub year: Option<u16>,
    pub episodes: Option<u32>,
    pub duration_min: Option<u32>,
    /// 0–100
    pub score: Option<u32>,
    pub genres: Vec<String>,
    pub description: Option<String>,
    pub cover: Option<String>,
    pub banner: Option<String>,
}

impl AniMedia {
    /// Hlavní název: anglický, jinak romaji.
    pub fn title(&self) -> &str {
        self.english.as_deref().or(self.romaji.as_deref()).unwrap_or("")
    }

    /// Názvy pro matcher a dotazy: romaji, anglický, synonyma v latince
    /// (japonské/korejské písmo v názvech souborů nebývá). Bez `skip`.
    pub fn names(&self, skip: &str) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        let candidates = self.romaji.iter().chain(self.english.iter()).chain(self.synonyms.iter());
        for n in candidates {
            let stripped = strip_season(n);
            let n = stripped.trim();
            if n.chars().count() > 80 {
                continue; // celé věty z light novel v názvu souboru nebudou
            }
            let latin = n.chars().all(|c| c.is_ascii() || matches!(c, 'À'..='ž' | '–' | '’'));
            if n.chars().count() >= 2 && latin && !n.eq_ignore_ascii_case(skip) && !out.iter().any(|o| o.eq_ignore_ascii_case(n)) {
                out.push(n.to_owned());
            }
        }
        out.truncate(6);
        out
    }

    /// Titul pro databázi. `imdb_id` a `season` dodá Wikidata. U pozdější
    /// sezóny (2+) se nevyplňuje rok ani popis — karta patří celému seriálu.
    pub fn to_meta(&self, imdb_id: &str, season: Option<u16>) -> TitleMeta {
        let first = season.unwrap_or(1) <= 1;
        let title = strip_season(self.title());
        TitleMeta {
            imdb_id: imdb_id.to_owned(),
            kind: if self.movie { MediaKind::Movie } else { MediaKind::Series },
            aliases: self.names(&title),
            title_orig: title,
            title_cs: None,
            year: self.year.filter(|_| first),
            overview: self.description.clone().filter(|_| first),
            poster: self.cover.clone(),
            background: self.banner.clone(),
            logo: None,
            runtime_min: self.duration_min,
            rating: self.score.map(|s| s as f32 / 10.0),
            genres: self.genres.clone(),
            countries: vec![],
            anime: true,
            release_date: None,
            popularity: 0.0,
            episodes: vec![],
        }
    }
}

pub struct AniList {
    http: reqwest::Client,
    endpoint: String,
}

const FIELDS: &str = "id format title { romaji english } synonyms startDate { year } seasonYear episodes duration \
    averageScore genres description(asHtml: false) coverImage { extraLarge large } bannerImage";

impl AniList {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http, endpoint: ENDPOINT.into() }
    }

    async fn page(&self, args: &str, vars: serde_json::Value) -> Result<Vec<AniMedia>> {
        // AniList odmítá deklarované a nepoužité proměnné → jen ty poslané.
        let decl = vars
            .as_object()
            .map(|o| {
                o.keys()
                    .filter_map(|k| {
                        let ty = match k.as_str() {
                            "page" | "perPage" | "seasonYear" => "Int",
                            "sort" => "[MediaSort]",
                            "season" => "MediaSeason",
                            "formats" => "[MediaFormat]",
                            "search" => "String",
                            "ids" => "[Int]",
                            _ => return None,
                        };
                        Some(format!("${k}: {ty}"))
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let query = format!(
            "query ({decl}) {{ Page(page: $page, perPage: $perPage) {{ \
             media(type: ANIME, isAdult: false, {args}) {{ {FIELDS} }} }} }}"
        );
        let res = self
            .http
            .post(&self.endpoint)
            .json(&json!({ "query": query, "variables": vars }))
            .header("Accept", "application/json")
            .timeout(Duration::from_secs(20))
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(Error::Http { status: res.status().as_u16(), context: "anilist".into() });
        }
        parse_page(&res.text().await?)
    }

    /// Řada katalogu. `page` od 1, 30 položek.
    pub async fn list(&self, list: AnimeList, page: u32) -> Result<Vec<AniMedia>> {
        let series = json!(["TV", "TV_SHORT", "ONA"]);
        let base = |sort: &str, formats: serde_json::Value| {
            json!({ "page": page.max(1), "perPage": 30, "sort": [sort], "formats": formats })
        };
        let common = "sort: $sort, format_in: $formats";
        match list {
            AnimeList::Trending => self.page(common, base("TRENDING_DESC", series)).await,
            AnimeList::Popular => self.page(common, base("POPULARITY_DESC", series)).await,
            AnimeList::TopRated => self.page(common, base("SCORE_DESC", series)).await,
            AnimeList::Movies => self.page(common, base("POPULARITY_DESC", json!(["MOVIE"]))).await,
            AnimeList::Season => {
                let (season, year) = current_season(crate::time::now_ms());
                let mut v = base("POPULARITY_DESC", series);
                v["season"] = json!(season);
                v["seasonYear"] = json!(year);
                self.page(&format!("{common}, season: $season, seasonYear: $seasonYear"), v).await
            }
        }
    }

    /// Hledání podle názvu (romaji, anglicky, synonyma).
    pub async fn search(&self, query: &str) -> Result<Vec<AniMedia>> {
        let v = json!({ "page": 1, "perPage": 15, "sort": ["SEARCH_MATCH"], "search": query,
                        "formats": ["TV", "TV_SHORT", "ONA", "MOVIE", "OVA"] });
        self.page("sort: $sort, search: $search, format_in: $formats", v).await
    }

    /// Konkrétní anime podle id (např. sezóny seriálu z Wikidat).
    pub async fn by_ids(&self, ids: &[u64]) -> Result<Vec<AniMedia>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let v = json!({ "page": 1, "perPage": 50, "ids": ids });
        self.page("id_in: $ids", v).await
    }
}

static SEASON_SUFFIX: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    crate::rx::rx(r"(?i)[\s:\-–]*(?:season\s*\d+|\d+(?:st|nd|rd|th)\s+season|part\s*\d+|cour\s*\d+)(?:\s*part\s*\d+)?\s*$")
});

/// "Blue Box Season 2" → "Blue Box": karta patří celému seriálu.
pub fn strip_season(name: &str) -> String {
    let s = SEASON_SUFFIX.replace(name.trim(), "");
    if s.trim().chars().count() >= 2 { s.trim().to_owned() } else { name.trim().to_owned() }
}

/// Anime z AniListu → tituly v databázi (IMDb ID kvůli epizodám a
/// zdrojům). Převod přes Wikidata; co tam není, zkusí Cinemeta podle
/// názvu a roku (jen první sezóny a filmy — pozdější sezóny mají jiný rok).
/// Víc sezón stejného seriálu → jedna karta (aliasy se sloučí). Pořadí
/// zůstává podle AniListu; anime bez IMDb protějšku se vynechá.
pub async fn resolve_titles(
    medias: &[AniMedia],
    wikidata: &super::Wikidata,
    cinemeta: &super::Cinemeta,
) -> Vec<TitleMeta> {
    let ids: Vec<u64> = medias.iter().map(|m| m.id).collect();
    let map = wikidata.anilist_map(&ids).await.unwrap_or_default();

    // Chybějící dohledat v Cinemetě souběžně (max. 8, ať to netrvá).
    let missing: Vec<&AniMedia> = medias.iter().filter(|m| !map.contains_key(&m.id)).take(8).collect();
    let found = futures_util::future::join_all(missing.iter().map(|m| cinemeta_match(m, cinemeta))).await;
    let fallback: std::collections::HashMap<u64, String> =
        missing.iter().zip(found).filter_map(|(m, f)| Some((m.id, f?))).collect();

    let mut out: Vec<TitleMeta> = vec![];
    for m in medias {
        let (imdb, season) = match (map.get(&m.id), fallback.get(&m.id)) {
            (Some(x), _) => (x.imdb_id.clone(), x.season),
            (None, Some(imdb)) => (imdb.clone(), None),
            _ => continue,
        };
        let meta = m.to_meta(&imdb, season);
        if let Some(prev) = out.iter_mut().find(|t| t.imdb_id == meta.imdb_id && t.kind == meta.kind) {
            // Další sezóna téhož seriálu: jen přidat názvy.
            for a in std::iter::once(meta.title_orig.clone()).chain(meta.aliases) {
                if a != prev.title_orig && !prev.aliases.contains(&a) {
                    prev.aliases.push(a);
                }
            }
            // První sezóna nese rok a popis celého seriálu.
            if season == Some(1) {
                prev.year = prev.year.or(m.year);
            }
            continue;
        }
        out.push(meta);
    }
    out
}

/// IMDb ID z Cinemety pro anime, které Wikidata nezná: stejný typ, rok ±1.
async fn cinemeta_match(m: &AniMedia, cinemeta: &super::Cinemeta) -> Option<String> {
    let kind = if m.movie { MediaKind::Movie } else { MediaKind::Series };
    let year = m.year?;
    for q in m.english.iter().chain(m.romaji.iter()) {
        let Ok(list) = cinemeta.search(kind, q).await else { continue };
        if let Some(t) = list.into_iter().take(5).find(|t| t.year.is_some_and(|y| y.abs_diff(year) <= 1)) {
            return Some(t.imdb_id);
        }
    }
    None
}

/// Aktuální anime sezóna podle data (zima = leden–březen).
pub fn current_season(now_ms: i64) -> (&'static str, i32) {
    let (y, m) = year_month(now_ms);
    let s = match m {
        1..=3 => "WINTER",
        4..=6 => "SPRING",
        7..=9 => "SUMMER",
        _ => "FALL",
    };
    (s, y)
}

/// Rok a měsíc (UTC) z milisekund od epochy (algoritmus "civil from days").
fn year_month(ms: i64) -> (i32, u32) {
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = (yoe + era * 400 + i64::from(m <= 2)) as i32;
    (y, m)
}

#[derive(Deserialize)]
struct Resp {
    data: Option<Data>,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}
#[derive(Deserialize)]
struct Data {
    #[serde(rename = "Page")]
    page: PageData,
}
#[derive(Deserialize)]
struct PageData {
    #[serde(default)]
    media: Vec<Media>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Media {
    id: u64,
    format: Option<String>,
    title: Titles,
    #[serde(default)]
    synonyms: Vec<String>,
    start_date: Option<Date>,
    season_year: Option<u16>,
    episodes: Option<u32>,
    duration: Option<u32>,
    average_score: Option<u32>,
    #[serde(default)]
    genres: Vec<String>,
    description: Option<String>,
    cover_image: Option<Cover>,
    banner_image: Option<String>,
}
#[derive(Deserialize)]
struct Titles {
    romaji: Option<String>,
    english: Option<String>,
}
#[derive(Deserialize)]
struct Date {
    year: Option<u16>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cover {
    extra_large: Option<String>,
    large: Option<String>,
}

/// Popis z AniListu: bez HTML značek a poznámek "(Source: …)".
fn clean_description(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    for c in raw.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out.replace("&quot;", "\"").replace("&amp;", "&").replace("&#039;", "'");
    let out = match out.find("(Source:") {
        Some(i) => out[..i].to_owned(),
        None => out,
    };
    let out = out.trim();
    (!out.is_empty()).then(|| out.to_owned())
}

pub(crate) fn parse_page(body: &str) -> Result<Vec<AniMedia>> {
    let r: Resp = serde_json::from_str(body).map_err(|e| Error::BadResponse(format!("anilist: {e}")))?;
    let Some(data) = r.data else {
        return Err(Error::BadResponse(format!("anilist: {:?}", r.errors.first())));
    };
    Ok(data
        .page
        .media
        .into_iter()
        .map(|m| AniMedia {
            id: m.id,
            movie: m.format.as_deref() == Some("MOVIE"),
            romaji: m.title.romaji.filter(|s| !s.is_empty()),
            english: m.title.english.filter(|s| !s.is_empty()),
            synonyms: m.synonyms,
            year: m.start_date.and_then(|d| d.year).or(m.season_year),
            episodes: m.episodes,
            duration_min: m.duration,
            score: m.average_score,
            genres: m.genres,
            description: m.description.as_deref().and_then(clean_description),
            cover: m.cover_image.and_then(|c| c.extra_large.or(c.large)),
            banner: m.banner_image,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_media_and_names() {
        let body = r#"{"data":{"Page":{"media":[{"id":154587,"format":"TV",
            "title":{"romaji":"Sousou no Frieren","english":"Frieren: Beyond Journey’s End"},
            "synonyms":["Frieren at the Funeral","장송의 프리렌","Frieren"],
            "startDate":{"year":2023},"seasonYear":2023,"episodes":28,"duration":24,"averageScore":91,
            "genres":["Adventure","Drama"],"description":"The adventure is over.<br><br>(Source: Crunchyroll)",
            "coverImage":{"extraLarge":"https://img/x.jpg","large":null},"bannerImage":"https://img/b.jpg"}]}}}"#;
        let m = &parse_page(body).unwrap()[0];
        assert_eq!(m.title(), "Frieren: Beyond Journey’s End");
        assert_eq!(m.description.as_deref(), Some("The adventure is over."));
        assert_eq!(
            m.names(m.title()),
            vec!["Sousou no Frieren", "Frieren at the Funeral", "Frieren"],
            "bez hangulu a bez hlavního názvu"
        );
        let t = m.to_meta("tt22248376", Some(2));
        assert!(t.anime && t.year.is_none() && t.overview.is_none());
        assert_eq!(t.rating, Some(9.1));
        assert_eq!(m.to_meta("tt22248376", Some(1)).year, Some(2023));
    }

    #[test]
    fn strips_season_suffix() {
        assert_eq!(strip_season("Blue Box Season 2"), "Blue Box");
        assert_eq!(strip_season("Ao no Hako 2nd Season"), "Ao no Hako");
        assert_eq!(strip_season("Attack on Titan Final Season Part 2"), "Attack on Titan Final Season");
        assert_eq!(strip_season("Spy x Family"), "Spy x Family");
        assert_eq!(strip_season("Season 2"), "Season 2");
    }

    #[test]
    fn graphql_error_is_error() {
        assert!(parse_page(r#"{"data":null,"errors":[{"message":"x"}]}"#).is_err());
    }

    #[test]
    fn seasons() {
        assert_eq!(current_season(1_795_465_474_701), ("FALL", 2026)); // 23. 11. 2026
        assert_eq!(year_month(0), (1970, 1));
        assert_eq!(current_season(1_767_225_600_000), ("WINTER", 2026)); // 1. 1. 2026
    }
}
