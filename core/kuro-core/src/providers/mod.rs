//! Zdroje souborů (Webshare, později Přehraj.to, ...).
//!
//! Provider nikdy nic nestahuje ani neproxuje. Vrací metadata souborů
//! a `ident`; přímé URL vzniká až v `resolve()` a nikam se neukládá —
//! je vázané na účet a IP a za pár hodin vyprší.

use async_trait::async_trait;

use crate::error::Result;

pub mod hellspy;
pub mod md5crypt;
pub mod prehrajto;
pub mod webshare;

pub use hellspy::Hellspy;
pub use prehrajto::PrehrajTo;
pub use webshare::Webshare;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderId {
    Webshare,
    PrehrajTo,
    Hellspy,
}

impl ProviderId {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderId::Webshare => "webshare",
            ProviderId::PrehrajTo => "prehrajto",
            ProviderId::Hellspy => "hellspy",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "webshare" => Some(ProviderId::Webshare),
            "prehrajto" => Some(ProviderId::PrehrajTo),
            "hellspy" => Some(ProviderId::Hellspy),
            _ => None,
        }
    }
}

/// Soubor tak, jak ho vrátí hledání u providera.
#[derive(Debug, Clone, PartialEq)]
pub struct RawFile {
    pub ident: String,
    pub name: String,
    pub size_bytes: u64,
    pub positive_votes: u32,
    pub negative_votes: u32,
    /// Chráněno heslem — takové přeskakujeme.
    pub password_protected: bool,
    /// Délka v sekundách, pokud ji provider hlásí.
    pub duration_sec: Option<u32>,
    /// Nejvyšší rozlišení, které provider skutečně streamuje (převody
    /// u Přehraj.to/Hellspy jsou max. 1080p, i když se soubor jmenuje 2160p).
    pub max_height: Option<u16>,
    /// Klíč pro sloučení stejného souboru u různých providerů
    /// (Hellspy zrcadlí soubory z Přehraj.to se stejným id).
    pub dedup_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub username: Option<String>,
    pub vip: bool,
    /// Konec VIP tak, jak ho provider poslal (Webshare: "YYYY-MM-DD HH:MM:SS").
    pub vip_until: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchSort {
    Relevance,
    Recent,
    Rating,
    Largest,
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub limit: u32,
    pub offset: u32,
    pub sort: SearchSort,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self { limit: 50, offset: 0, sort: SearchSort::Relevance }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkPurpose {
    /// Přehrání v přehrávači.
    Stream,
    /// Stažení do zařízení.
    Download,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLink {
    pub url: String,
    /// Hlavičky, které musí přehrávač/stahování poslat (Referer, cookies, ...).
    pub headers: Vec<(String, String)>,
    /// Unix ms, po kterém je odkaz nejspíš mrtvý. Pak si říct znovu.
    pub expires_at_ms: Option<i64>,
    /// Externí titulky od providera (VTT/SRT).
    pub subtitles: Vec<SubtitleLink>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubtitleLink {
    /// ISO 639-2/1 nebo popisek providera ("cze", "eng").
    pub lang: String,
    pub label: String,
    pub url: String,
}

/// Přehraj.to/Hellspy dávají titulek, ne název souboru. Bez přípony by
/// parser bral "Film.2014.v2" jako soubor s příponou ".v2" a zahodil ho.
/// Stream je MP4 převod, takže ".mp4" je pravda.
pub(crate) fn as_filename(title: &str) -> String {
    const VIDEO: &[&str] = &["mkv", "mp4", "avi", "m4v", "mov", "ts", "m2ts", "mpg", "mpeg", "wmv", "flv", "webm"];
    let ext = title.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    if VIDEO.contains(&ext.as_str()) { title.to_owned() } else { format!("{title}.mp4") }
}

/// "cze", "cze1", "CZE" → "cs"; neznámé vrátí, jak je (malými).
pub fn subtitle_lang(raw: &str) -> String {
    let l = raw.trim().to_lowercase();
    let base: String = l.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    match base.as_str() {
        "cze" | "ces" | "cs" | "cz" => "cs".into(),
        "slo" | "slk" | "sk" => "sk".into(),
        "eng" | "en" => "en".into(),
        "ger" | "deu" | "de" => "de".into(),
        "jpn" | "ja" => "ja".into(),
        _ => base,
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;

    /// Kolik souběžných dotazů provider snese bez omezení.
    fn max_concurrency(&self) -> usize;

    /// Umí teď vydat přehratelný odkaz? (Webshare bez přihlášení ne —
    /// pak nemá smysl jeho zdroje vůbec ukazovat.)
    fn can_resolve(&self) -> bool {
        true
    }

    /// Přihlásí se a vrátí token. Token si provider zároveň drží.
    /// Heslo se nikam neukládá.
    async fn login(&self, username: &str, password: &str) -> Result<String>;

    /// Nastaví token uložený z dřívějška (Kotlin ho drží v Keystore).
    fn set_token(&self, token: Option<String>);

    fn token(&self) -> Option<String>;

    async fn account(&self) -> Result<Account>;

    async fn search(&self, query: &str, opts: &SearchOptions) -> Result<Vec<RawFile>>;

    async fn resolve(&self, ident: &str, purpose: LinkPurpose) -> Result<ResolvedLink>;
}
