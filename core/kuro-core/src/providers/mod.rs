//! Zdroje souborů (Webshare, později Přehraj.to, ...).
//!
//! Provider nikdy nic nestahuje ani neproxuje. Vrací metadata souborů
//! a `ident`; přímé URL vzniká až v `resolve()` a nikam se neukládá —
//! je vázané na účet a IP a za pár hodin vyprší.

use async_trait::async_trait;

use crate::error::Result;

pub mod md5crypt;
pub mod webshare;

pub use webshare::Webshare;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderId {
    Webshare,
}

impl ProviderId {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderId::Webshare => "webshare",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "webshare" => Some(ProviderId::Webshare),
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
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;

    /// Kolik souběžných dotazů provider snese bez omezení.
    fn max_concurrency(&self) -> usize;

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
