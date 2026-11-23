//! Hellspy.to — veřejné JSON API, přehrávání bez přihlášení.
//!
//! - hledání: `GET api.hellspy.to/gw/search?query=…&offset=0&limit=64`
//! - detail:  `GET api.hellspy.to/gw/video/{id}/{fileHash}` → `conversions`
//!   (720/1080 → přímé MP4) a `subtitles` (VTT)
//!
//! Streamuje převody (max. 1080p), ne původní soubor. Část výsledků jsou
//! soubory z Přehraj.to (16znakový hash) — mají stejné id jako tam, takže
//! se slučují přes `dedup_key`.

use async_trait::async_trait;
use serde::Deserialize;

use super::{
    subtitle_lang, Account, LinkPurpose, Provider, ProviderId, RawFile, ResolvedLink, SearchOptions, SubtitleLink,
};
use crate::error::{Error, Result};

const API: &str = "https://api.hellspy.to";

/// Platnost odkazu neznáme; po dvou hodinách si říct znovu.
const LINK_TTL_MS: i64 = 2 * 60 * 60 * 1000;

pub struct Hellspy {
    http: reqwest::Client,
    api: String,
}

impl Hellspy {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http, api: API.into() }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let res = self.http.get(url).header("Accept", "application/json").send().await?;
        let status = res.status();
        if status.as_u16() == 404 {
            return Err(Error::Provider { provider: "hellspy".into(), code: "NOT_FOUND".into(), message: "soubor neexistuje".into() });
        }
        if !status.is_success() {
            return Err(Error::Http { status: status.as_u16(), context: "hellspy".into() });
        }
        Ok(res.json::<T>().await?)
    }
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    items: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    id: u64,
    #[serde(default)]
    title: String,
    #[serde(rename = "fileHash", default)]
    file_hash: String,
    #[serde(default)]
    size: u64,
    duration: Option<f64>,
}

#[derive(Deserialize)]
struct VideoResponse {
    #[serde(default)]
    conversions: std::collections::HashMap<String, String>,
    #[serde(default)]
    subtitles: Vec<Subtitle>,
}

#[derive(Deserialize)]
struct Subtitle {
    #[serde(default)]
    title: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    url: String,
}

pub(crate) fn parse_search(json: &str) -> Result<Vec<RawFile>> {
    let r: SearchResponse = serde_json::from_str(json).map_err(|e| Error::BadResponse(format!("hellspy: {e}")))?;
    Ok(r.items.into_iter().filter(|i| !i.file_hash.is_empty()).map(to_raw).collect())
}

fn to_raw(i: SearchItem) -> RawFile {
    // 16znakový hash = soubor z Přehraj.to (stejné id) → společný klíč.
    let dedup = if i.file_hash.len() == 16 { format!("prehrajto:{}", i.id) } else { format!("hellspy:{}", i.id) };
    RawFile {
        ident: format!("{}/{}", i.id, i.file_hash),
        name: super::as_filename(&i.title),
        size_bytes: i.size,
        positive_votes: 0,
        negative_votes: 0,
        password_protected: false,
        duration_sec: i.duration.filter(|d| *d > 0.0).map(|d| d as u32),
        max_height: Some(1080),
        dedup_key: Some(dedup),
    }
}

pub(crate) fn parse_video(json: &str) -> Result<ResolvedLink> {
    let v: VideoResponse = serde_json::from_str(json).map_err(|e| Error::BadResponse(format!("hellspy: {e}")))?;
    // Nejvyšší dostupný převod.
    let url = v
        .conversions
        .iter()
        .filter_map(|(k, u)| k.parse::<u32>().ok().map(|h| (h, u)))
        .max_by_key(|(h, _)| *h)
        .map(|(_, u)| u.clone())
        .ok_or_else(|| Error::Provider {
            provider: "hellspy".into(),
            code: "NO_CONVERSION".into(),
            message: "video zatím není připravené k přehrání".into(),
        })?;
    let subtitles = v
        .subtitles
        .into_iter()
        .filter(|s| !s.url.is_empty())
        .map(|s| {
            let raw = if s.language.is_empty() { &s.title } else { &s.language };
            SubtitleLink { lang: subtitle_lang(raw), label: s.title, url: s.url }
        })
        .collect();
    Ok(ResolvedLink { url, headers: vec![], expires_at_ms: Some(crate::time::now_ms() + LINK_TTL_MS), subtitles })
}

#[async_trait]
impl Provider for Hellspy {
    fn id(&self) -> ProviderId {
        ProviderId::Hellspy
    }

    fn max_concurrency(&self) -> usize {
        3
    }

    async fn login(&self, _: &str, _: &str) -> Result<String> {
        Err(Error::BadResponse("Hellspy přihlášení nepotřebuje".into()))
    }

    fn set_token(&self, _: Option<String>) {}

    fn token(&self) -> Option<String> {
        None
    }

    async fn account(&self) -> Result<Account> {
        Ok(Account { username: None, vip: false, vip_until: None })
    }

    async fn search(&self, query: &str, opts: &SearchOptions) -> Result<Vec<RawFile>> {
        let url = reqwest::Url::parse_with_params(
            &format!("{}/gw/search", self.api),
            &[("query", query), ("offset", &opts.offset.to_string()), ("limit", &opts.limit.min(64).to_string())],
        )
        .map_err(|e| Error::BadResponse(e.to_string()))?;
        let body = self.http.get(url).header("Accept", "application/json").send().await?;
        if !body.status().is_success() {
            return Err(Error::Http { status: body.status().as_u16(), context: "hellspy search".into() });
        }
        parse_search(&body.text().await?)
    }

    async fn resolve(&self, ident: &str, _: LinkPurpose) -> Result<ResolvedLink> {
        let (id, hash) = ident.split_once('/').ok_or_else(|| Error::BadResponse(format!("hellspy ident {ident}")))?;
        let json: serde_json::Value = self.get_json(&format!("{}/gw/video/{id}/{hash}", self.api)).await?;
        parse_video(&json.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_and_dedup_keys() {
        let json = r#"{"items":[
            {"id":6589465,"title":"INTERSTELLAR,CZdab","fileHash":"13b22b073ab630abde6aba321ba1a59c","size":1988887990,"duration":10144},
            {"id":31276859,"title":"Interstellar - 2014 CZ EN DABING","fileHash":"70f0b6452586f015","size":13170763085,"duration":10143},
            {"id":1,"title":"bez hashe","fileHash":"","size":1}
        ],"offset":0,"limit":64}"#;
        let files = parse_search(json).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].ident, "6589465/13b22b073ab630abde6aba321ba1a59c");
        assert_eq!(files[0].name, "INTERSTELLAR,CZdab.mp4");
        assert_eq!(files[0].dedup_key.as_deref(), Some("hellspy:6589465"));
        assert_eq!(files[1].dedup_key.as_deref(), Some("prehrajto:31276859"));
        assert_eq!(files[1].duration_sec, Some(10143));
        assert_eq!(files[1].max_height, Some(1080));
    }

    #[test]
    fn picks_best_conversion_and_subtitles() {
        let json = r#"{"conversions":{"720":"https://cdn/a720.mp4","1080":"https://cdn/a1080.mp4"},
            "subtitles":[{"title":"cze","language":"cze","url":"https://cdn/c.vtt"},{"title":"eng","language":"eng","url":"https://cdn/e.vtt"}]}"#;
        let l = parse_video(json).unwrap();
        assert_eq!(l.url, "https://cdn/a1080.mp4");
        assert_eq!(l.subtitles.iter().map(|s| s.lang.as_str()).collect::<Vec<_>>(), vec!["cs", "en"]);

        assert!(parse_video(r#"{"conversions":{},"subtitles":[]}"#).is_err(), "bez převodu nejde přehrát");
    }
}
