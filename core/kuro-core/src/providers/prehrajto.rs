//! Přehraj.to — přehrávání bez přihlášení (převody 720p/1080p + titulky).
//!
//! Nemá veřejné API, čte se HTML:
//! - hledání: `GET prehraj.to/hledej/{dotaz}` → bloky `video-wrapper`
//!   (id, odkaz, název, velikost, délka)
//! - video: `GET prehraj.to/{slug}/{hash}` → `var sources = [{file, label}]`
//!   a `var tracks = [{file, label}]` (podepsané, časově omezené odkazy)
//!
//! Když web změní HTML, parser vrátí prázdno / chybu — testy níž hlídají
//! očekávaný tvar.

use std::sync::LazyLock;

use async_trait::async_trait;
use regex::Regex;

use super::{
    subtitle_lang, Account, LinkPurpose, Provider, ProviderId, RawFile, ResolvedLink, SearchOptions, SubtitleLink,
};
use crate::error::{Error, Result};

const BASE: &str = "https://prehraj.to";

static BLOCK_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"data-video-id="(\d+)""#).unwrap());
static BLOCK_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"href="(/[^"/]+/[0-9a-f]{8,})"\s+title="([^"]*)""#).unwrap());
static BLOCK_SIZE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"video__tag--size[^>]*>\s*([\d.,]+)\s*([KMGT]B)"#).unwrap());
static BLOCK_TIME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"video__tag--time[^>]*>\s*(\d{1,2}):(\d{2})(?::(\d{2}))?"#).unwrap());
static SOURCES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)var\s+sources\s*=\s*\[(.*?)\];").unwrap());
static TRACKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)var\s+tracks\s*=\s*\[(.*?)\];").unwrap());
static ENTRY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)\{(.*?)\}").unwrap());
static FILE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"file\s*:\s*"([^"]+)""#).unwrap());
static LABEL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"label\s*:\s*["']([^"']*)["']"#).unwrap());
static EXPIRES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[?&]expires=(\d{9,})").unwrap());

pub struct PrehrajTo {
    http: reqwest::Client,
    base: String,
}

impl PrehrajTo {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http, base: BASE.into() }
    }

    async fn get_html(&self, url: &str) -> Result<String> {
        let res = self.http.get(url).header("Accept", "text/html").send().await?;
        let status = res.status();
        if status.as_u16() == 404 {
            return Err(Error::Provider { provider: "prehrajto".into(), code: "NOT_FOUND".into(), message: "soubor neexistuje".into() });
        }
        if !status.is_success() {
            return Err(Error::Http { status: status.as_u16(), context: "prehrajto".into() });
        }
        Ok(res.text().await?)
    }
}

fn unescape(s: &str) -> String {
    s.replace("&amp;", "&").replace("&quot;", "\"").replace("&#039;", "'").replace("&lt;", "<").replace("&gt;", ">")
}

fn size_bytes(num: &str, unit: &str) -> u64 {
    let n: f64 = num.replace(',', ".").parse().unwrap_or(0.0);
    let mult = match unit {
        "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        _ => 1.0,
    };
    (n * mult) as u64
}

pub(crate) fn parse_search(html: &str) -> Vec<RawFile> {
    html.split("class=\"video-wrapper\"")
        .skip(1)
        .filter_map(|block| {
            let (path, title) = BLOCK_LINK.captures(block).map(|c| (c[1].to_owned(), unescape(&c[2])))?;
            let id = BLOCK_ID.captures(block).map(|c| c[1].to_owned());
            let size = BLOCK_SIZE.captures(block).map(|c| size_bytes(&c[1], &c[2])).unwrap_or(0);
            let duration = BLOCK_TIME.captures(block).map(|c| {
                let a: u32 = c[1].parse().unwrap_or(0);
                let b: u32 = c[2].parse().unwrap_or(0);
                match c.get(3) {
                    Some(s) => a * 3600 + b * 60 + s.as_str().parse::<u32>().unwrap_or(0),
                    None => a * 60 + b,
                }
            });
            Some(RawFile {
                ident: path,
                name: super::as_filename(&title),
                size_bytes: size,
                positive_votes: 0,
                negative_votes: 0,
                password_protected: false,
                duration_sec: duration.filter(|d| *d > 0),
                max_height: Some(1080),
                dedup_key: id.map(|i| format!("prehrajto:{i}")),
            })
        })
        .collect()
}

fn entries(list: &str) -> Vec<(String, String)> {
    ENTRY
        .captures_iter(list)
        .filter_map(|e| {
            let body = &e[1];
            let file = FILE.captures(body)?[1].to_owned();
            let label = LABEL.captures(body).map(|c| c[1].to_owned()).unwrap_or_default();
            Some((unescape(&file), label))
        })
        .collect()
}

pub(crate) fn parse_video(html: &str) -> Result<ResolvedLink> {
    let sources = SOURCES.captures(html).map(|c| entries(&c[1])).unwrap_or_default();
    // Nejvyšší kvalita podle popisku "720p"/"1080p".
    let (url, _) = sources
        .into_iter()
        .max_by_key(|(_, label)| label.trim_end_matches('p').parse::<u32>().unwrap_or(0))
        .ok_or_else(|| Error::Provider {
            provider: "prehrajto".into(),
            code: "NO_SOURCE".into(),
            message: "stránka neobsahuje zdroj videa".into(),
        })?;
    let subtitles = TRACKS
        .captures(html)
        .map(|c| entries(&c[1]))
        .unwrap_or_default()
        .into_iter()
        .map(|(url, label)| {
            // Popisek "CZE - 14323905 - cze" → jazyk z prvního slova.
            let first = label.split(['-', ' ']).find(|s| !s.is_empty()).unwrap_or("");
            SubtitleLink { lang: subtitle_lang(first), label, url }
        })
        .collect();
    let expires = EXPIRES.captures(&url).and_then(|c| c[1].parse::<i64>().ok()).map(|s| s * 1000);
    Ok(ResolvedLink { url, headers: vec![], expires_at_ms: expires, subtitles })
}

#[async_trait]
impl Provider for PrehrajTo {
    fn id(&self) -> ProviderId {
        ProviderId::PrehrajTo
    }

    fn max_concurrency(&self) -> usize {
        3
    }

    async fn login(&self, _: &str, _: &str) -> Result<String> {
        Err(Error::BadResponse("Přehraj.to bez účtu".into()))
    }

    fn set_token(&self, _: Option<String>) {}

    fn token(&self) -> Option<String> {
        None
    }

    async fn account(&self) -> Result<Account> {
        Ok(Account { username: None, vip: false, vip_until: None })
    }

    async fn search(&self, query: &str, _: &SearchOptions) -> Result<Vec<RawFile>> {
        let q = percent_encoding::utf8_percent_encode(query, percent_encoding::NON_ALPHANUMERIC).to_string();
        let html = self.get_html(&format!("{}/hledej/{q}", self.base)).await?;
        Ok(parse_search(&html))
    }

    async fn resolve(&self, ident: &str, _: LinkPurpose) -> Result<ResolvedLink> {
        if !ident.starts_with('/') || ident.contains("..") {
            return Err(Error::BadResponse(format!("prehrajto ident {ident}")));
        }
        let html = self.get_html(&format!("{}{ident}", self.base)).await?;
        parse_video(&html)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Umělé vzorky se stejnou strukturou jako stránky Přehraj.to.
    const SEARCH: &str = r#"
<div class="video-listing">
 <div class="video-wrapper">
  <div class="video__picture--container" data-video-id="31276859">
   <a class="video video--small video--link" href="/interstellar-2014-cz-en-dabing/22298b5c2e8dec4a" title="Interstellar - 2014 CZ EN DABING">
    <div class="video__tag video__tag--time">02:49:03</div>
    <div class="video__tag video__tag--size video__tag--size-alone"> 13.76 GB </div>
   </a>
  </div>
 </div>
 <div class="video-wrapper">
  <div class="video__picture--container" data-video-id="42">
   <a class="video video--small video--link" href="/kratke-video/abcdef0123456789" title="Krátké &amp; video">
    <div class="video__tag video__tag--time">05:10</div>
    <div class="video__tag video__tag--size"> 120 MB </div>
   </a>
  </div>
 </div>
</div>"#;

    const VIDEO: &str = r#"<script>
 var sources = [ { file: "https://pf-storage1.example/a.mp4?token=x&expires=1795551138&signature=s", label: '720p' } ,
                 { file: "https://pf-storage5.example/b.mp4?token=x&expires=1795551138&signature=s", label: '1080p' } ];
 var tracks = [ { file: "https://pf-storage4.example/c.vtt?token=x", "default": true , label: "CZE - 14323905 - cze", kind: "captions" },
                { file: "https://pf-storage3.example/e.vtt?token=x", label: "ENG - 14323919 - eng", kind: "captions" } ];
</script>"#;

    #[test]
    fn parses_search_blocks() {
        let f = parse_search(SEARCH);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].ident, "/interstellar-2014-cz-en-dabing/22298b5c2e8dec4a");
        assert_eq!(f[0].name, "Interstellar - 2014 CZ EN DABING.mp4");
        assert_eq!(f[0].size_bytes, 13_760_000_000);
        assert_eq!(f[0].duration_sec, Some(2 * 3600 + 49 * 60 + 3));
        assert_eq!(f[0].dedup_key.as_deref(), Some("prehrajto:31276859"));
        assert_eq!(f[1].name, "Krátké & video.mp4");
        assert_eq!(f[1].duration_sec, Some(310));
        assert!(parse_search("<html>nic</html>").is_empty());
    }

    #[test]
    fn parses_video_sources_and_tracks() {
        let l = parse_video(VIDEO).unwrap();
        assert!(l.url.contains("b.mp4"), "nejvyšší kvalita");
        assert_eq!(l.expires_at_ms, Some(1_795_551_138_000));
        assert_eq!(l.subtitles.len(), 2);
        assert_eq!((l.subtitles[0].lang.as_str(), l.subtitles[1].lang.as_str()), ("cs", "en"));
        assert!(parse_video("<html></html>").is_err());
    }
}
