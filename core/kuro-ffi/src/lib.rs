//! UniFFI fasáda nad `kuro-core`. Kotlin vidí jediný objekt `KuroCore`.
//!
//! Zásady:
//! - Všechno síťové je `suspend` v Kotlinu (async tady).
//! - Když síť selže a v databázi něco je, vrátí se uložená data —
//!   aplikace musí fungovat i offline.
//! - Heslo k Webshare prochází jen `webshare_login`; ven jde token,
//!   který si Kotlin uloží do Keystore a při startu vrátí přes
//!   `set_webshare_token`.

use std::sync::Arc;

use kuro_core::finder::{FinderConfig, StreamFinder};
use kuro_core::matcher::MatchTarget;
use kuro_core::meta::{CatalogId, Cinemeta, Wikidata};
use kuro_core::providers::{Provider as _, Webshare};
use kuro_core::storage::{Budget, Db, Event, StoredTitle, Target, DAY_MS};

mod error;
mod types;

pub use error::{KuroError, Result};
pub use types::*;

uniffi::setup_scaffolding!();

/// Průběžné výsledky hledání zdrojů (volá se z vlákna Rustu).
#[uniffi::export(with_foreign)]
pub trait StreamListener: Send + Sync {
    fn on_partial(&self, streams: Vec<StreamInfo>);
}

/// Jak starý detail se ještě nepřenačítá. U seriálů krátce kvůli novým epizodám.
const SERIES_DETAIL_TTL_MS: i64 = DAY_MS;
const MOVIE_DETAIL_TTL_MS: i64 = 30 * DAY_MS;

#[derive(uniffi::Object)]
pub struct KuroCore {
    db: Db,
    cinemeta: Cinemeta,
    wikidata: Wikidata,
    webshare: Arc<Webshare>,
    finder: StreamFinder,
}

fn ep(v: i32) -> u16 {
    v.clamp(0, i32::from(u16::MAX)) as u16
}

fn target(title_id: i64, season: i32, episode: i32) -> Target {
    Target { title_id, season: ep(season), episode: ep(episode) }
}

#[uniffi::export(async_runtime = "tokio")]
impl KuroCore {
    /// `db_path`: soubor v soukromém úložišti aplikace.
    /// `device_id`: náhodné UUID vygenerované při instalaci (Webshare podle něj počítá zařízení).
    #[uniffi::constructor]
    pub fn new(db_path: String, device_id: String) -> Result<Arc<Self>> {
        let http = kuro_core::http::client();
        let webshare = Arc::new(Webshare::new(http.clone(), device_id));
        let finder = StreamFinder::new(vec![webshare.clone()], FinderConfig::default());
        Ok(Arc::new(Self {
            db: Db::open(db_path)?,
            cinemeta: Cinemeta::new(http.clone()),
            wikidata: Wikidata::new(http),
            webshare,
            finder,
        }))
    }

    /* ------------------------------ účet ------------------------------ */

    /// Přihlásí k Webshare a vrátí token k uložení do Keystore. Heslo se neukládá.
    pub async fn webshare_login(&self, username: String, password: String) -> Result<String> {
        Ok(self.webshare.login(&username, &password).await?)
    }

    pub fn set_webshare_token(&self, token: Option<String>) {
        self.webshare.set_token(token);
    }

    pub fn is_webshare_logged_in(&self) -> bool {
        self.webshare.token().is_some()
    }

    pub async fn webshare_account(&self) -> Result<AccountInfo> {
        let a = self.webshare.account().await?;
        Ok(AccountInfo { username: a.username, vip: a.vip, vip_until: a.vip_until })
    }

    /* ------------------------------ katalog ------------------------------ */

    /// Stránka katalogu z Cinemety (~50 položek). `skip` = kolik položek přeskočit.
    pub async fn catalog(&self, kind: MediaKind, catalog: Catalog, genre: Option<String>, skip: i32) -> Result<Vec<TitleCard>> {
        let id = match catalog {
            Catalog::Popular => CatalogId::Top,
            Catalog::TopRated => CatalogId::ImdbRating,
            Catalog::New => CatalogId::Year,
        };
        let metas = self.cinemeta.catalog(kind.into(), id, genre.as_deref(), skip.max(0) as u32).await?;
        self.store_cards(&metas)
    }

    /// Hledání přes Cinemetu (oba typy, když `kind` chybí). Výsledky se uloží
    /// jako krátkodobé karty; české názvy doplní `localize`.
    pub async fn search(&self, query: String, kind: Option<MediaKind>) -> Result<Vec<TitleCard>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(vec![]);
        }
        self.db.add_recent_search(q)?;

        let metas = match kind {
            Some(k) => self.cinemeta.search(k.into(), q).await?,
            None => {
                let (movies, series) = tokio::join!(
                    self.cinemeta.search(kuro_core::matcher::MediaKind::Movie, q),
                    self.cinemeta.search(kuro_core::matcher::MediaKind::Series, q),
                );
                // Selhání jednoho typu nezahodí druhý.
                let (movies, series) = match (movies, series) {
                    (Err(e), Err(_)) => return Err(e.into()),
                    (m, s) => (m.unwrap_or_default(), s.unwrap_or_default()),
                };
                interleave(movies, series)
            }
        };
        self.store_cards(&metas)
    }

    /// Okamžité napovídání z lokální databáze (bez sítě).
    pub fn search_local(&self, query: String, kind: Option<MediaKind>, limit: i32) -> Result<Vec<TitleCard>> {
        Ok(self.db.search_local(&query, kind.map(Into::into), limit.max(0) as u32)?.iter().map(TitleCard::from).collect())
    }

    /// Doplní české názvy z Wikidat k titulům, které je ještě nemají.
    /// Vrací aktualizované karty ve stejném pořadí.
    pub async fn localize(&self, title_ids: Vec<i64>) -> Result<Vec<TitleCard>> {
        let titles: Vec<StoredTitle> = title_ids.iter().filter_map(|id| self.db.title(*id).ok().flatten()).collect();
        let pending: Vec<String> =
            titles.iter().filter(|t| t.localized_at.is_none()).map(|t| t.meta.imdb_id.clone()).collect();
        if !pending.is_empty() {
            let found = self.wikidata.localized_titles(&pending).await;
            // Prázdná mapa = Wikidata neodpověděla; pak nic nezapisovat,
            // ať se příště zeptáme znovu.
            if !found.is_empty() {
                for t in titles.iter().filter(|t| t.localized_at.is_none()) {
                    self.db.apply_localized(t.id, found.get(&t.meta.imdb_id))?;
                }
            }
        }
        Ok(title_ids.iter().filter_map(|id| self.db.title(*id).ok().flatten()).map(|t| TitleCard::from(&t)).collect())
    }

    /// Detail titulu. Z databáze, když je čerstvý; jinak z Cinemety
    /// (+ Wikidata). Bez sítě vrátí, co je uložené. Zapíše "otevřeno".
    pub async fn title_detail(&self, title_id: i64) -> Result<TitleDetail> {
        let stored = self.db.title(title_id)?.ok_or(KuroError::NotFound)?;
        let now = kuro_core::time::now_ms();
        let ttl = match stored.meta.kind {
            kuro_core::matcher::MediaKind::Series => SERIES_DETAIL_TTL_MS,
            kuro_core::matcher::MediaKind::Movie => MOVIE_DETAIL_TTL_MS,
        };
        let stale = stored.detail_at.is_none_or(|at| now - at > ttl);

        if stale {
            match self.cinemeta.meta(stored.meta.kind, &stored.meta.imdb_id).await {
                Ok(Some(meta)) => {
                    self.db.upsert_detail(&meta)?;
                }
                Ok(None) => {}
                // Offline: když už detail máme, stačí uložený.
                Err(e) if stored.detail_at.is_some() => {
                    let _ = e;
                }
                Err(e) => return Err(e.into()),
            }
        }
        if stored.localized_at.is_none() {
            if let Ok(loc) = self.wikidata.localized_title(&stored.meta.imdb_id).await {
                self.db.apply_localized(title_id, loc.as_ref())?;
            }
        }

        self.db.record_event(title_id, Event::Opened)?;
        let t = self.db.title(title_id)?.ok_or(KuroError::NotFound)?;
        Ok(TitleDetail {
            overview: t.meta.overview.clone(),
            logo: t.meta.logo.clone(),
            runtime_min: t.meta.runtime_min.map(|r| r as i32),
            countries: t.meta.countries.clone(),
            release_date: t.meta.release_date.clone(),
            episodes: self.db.episodes(title_id)?.iter().map(Episode::from).collect(),
            in_watchlist: self.db.in_watchlist(title_id)?,
            progress: self.db.title_progress(title_id)?.iter().map(ProgressInfo::from).collect(),
            card: TitleCard::from(&t),
        })
    }

    pub fn record_event(&self, title_id: i64, event: UserEvent) -> Result<()> {
        Ok(self.db.record_event(title_id, event.into())?)
    }

    /* ------------------------------ zdroje ------------------------------ */

    /// Uložené zdroje (okamžitě, bez sítě). Když `is_fresh` je false,
    /// UI je ukáže a zavolá `find_streams`.
    pub fn cached_streams(&self, title_id: i64, season: i32, episode: i32) -> Result<StreamList> {
        let c = self.db.cached_streams(target(title_id, season, episode))?;
        Ok(StreamList {
            streams: c.streams.iter().map(StreamInfo::from).collect(),
            is_fresh: c.is_fresh,
            searched_at_ms: c.searched_at,
        })
    }

    /// Vyhledá zdroje u providerů, uloží je a vrátí seřazené.
    /// Film: `season = episode = 0`.
    pub async fn find_streams(
        &self,
        title_id: i64,
        season: i32,
        episode: i32,
        listener: Option<Arc<dyn StreamListener>>,
    ) -> Result<Vec<StreamInfo>> {
        let t = self.db.title(title_id)?.ok_or(KuroError::NotFound)?;
        let tgt = target(title_id, season, episode);
        let m = &t.meta;
        let match_target = MatchTarget {
            title_cs: m.display_title().to_owned(),
            title_orig: m.title_orig.clone(),
            aliases: m.aliases.clone(),
            year: m.year,
            runtime: m.runtime_min,
            kind: m.kind,
            season: tgt.is_episode().then_some(tgt.season),
            episode: tgt.is_episode().then_some(tgt.episode),
        };

        let report = self
            .finder
            .find(&match_target, &|partial| {
                if let Some(l) = &listener {
                    l.on_partial(partial.iter().map(StreamInfo::from).collect());
                }
            })
            .await;

        // Nic nenalezeno a všechny dotazy selhaly → chyba (jinak by to
        // vypadalo jako "k titulu nic není").
        if report.streams.is_empty() && !report.errors.is_empty() && report.errors.len() as u32 == report.queries {
            if report.auth_expired().is_some() {
                return Err(KuroError::AuthExpired);
            }
            return Err(report.errors[0].1.clone().into());
        }

        self.db.save_streams(tgt, &report.streams)?;
        let cached = self.db.cached_streams(tgt)?;
        Ok(cached.streams.iter().map(StreamInfo::from).collect())
    }

    /// Přímý odkaz pro přehrávač / stahování. Nikdy ho neukládat — je
    /// vázaný na účet a IP a vyprší.
    pub async fn resolve_stream(&self, provider: Provider, ident: String, purpose: LinkPurpose) -> Result<PlayLink> {
        let p: kuro_core::providers::ProviderId = provider.into();
        match self.webshare.resolve(&ident, purpose.into()).await {
            Ok(link) => Ok(PlayLink { url: link.url, headers: link.headers.into_iter().collect(), expires_at_ms: link.expires_at_ms }),
            Err(e) => {
                // Soubor zmizel → započítat selhání, ať v řazení klesne.
                if matches!(e, kuro_core::Error::Provider { .. }) {
                    self.db.report_stream(p, &ident, false)?;
                }
                Err(e.into())
            }
        }
    }

    /// Výsledek přehrání z přehrávače (rozjelo se / nerozjelo).
    pub fn report_stream(&self, provider: Provider, ident: String, ok: bool) -> Result<()> {
        Ok(self.db.report_stream(provider.into(), &ident, ok)?)
    }

    /// Přehrávání se rozjelo: zvedne zájem o titul a zapíše jazykovou
    /// preferenci podle zvoleného zdroje (dabing / titulky / originál).
    pub fn playback_started(&self, title_id: i64, season: i32, episode: i32, provider: Provider, ident: String) -> Result<()> {
        self.db.record_event(title_id, Event::PlayStarted)?;
        self.db.report_stream(provider.into(), &ident, true)?;
        let cached = self.db.cached_streams(target(title_id, season, episode))?;
        if let Some(s) = cached.streams.iter().find(|s| s.ident == ident) {
            let has = |v: &[String], l: &str| v.iter().any(|x| x == l);
            let feature = if has(&s.audio_langs, "cs") || has(&s.audio_langs, "sk") {
                "lang:cs-dub"
            } else if has(&s.sub_langs, "cs") || has(&s.sub_langs, "sk") {
                "lang:cs-sub"
            } else {
                "lang:orig"
            };
            self.db.record_taste(feature, 1.0)?;
        }
        Ok(())
    }

    /* ------------------------------ knihovna ------------------------------ */

    pub fn set_watchlist(&self, title_id: i64, on: bool) -> Result<()> {
        Ok(self.db.set_watchlist(title_id, on)?)
    }

    pub fn watchlist(&self) -> Result<Vec<TitleCard>> {
        Ok(self.db.watchlist()?.iter().map(TitleCard::from).collect())
    }

    /// Uloží pozici (volat každých ~10 s a při zavření přehrávače).
    pub fn save_progress(
        &self,
        title_id: i64,
        season: i32,
        episode: i32,
        position_ms: i64,
        duration_ms: i64,
        provider: Option<Provider>,
        ident: Option<String>,
    ) -> Result<()> {
        let source = match (provider, ident.as_deref()) {
            (Some(p), Some(i)) => Some((p.into(), i)),
            _ => None,
        };
        Ok(self.db.save_progress(target(title_id, season, episode), position_ms, duration_ms, source)?)
    }

    pub fn progress(&self, title_id: i64, season: i32, episode: i32) -> Result<Option<ProgressInfo>> {
        Ok(self.db.progress(target(title_id, season, episode))?.as_ref().map(ProgressInfo::from))
    }

    pub fn continue_watching(&self, limit: i32) -> Result<Vec<ContinueItem>> {
        Ok(self
            .db
            .continue_watching(limit.max(0) as u32)?
            .iter()
            .map(|c| ContinueItem { title: TitleCard::from(&c.title), progress: ProgressInfo::from(&c.progress) })
            .collect())
    }

    pub fn recent_searches(&self, limit: i32) -> Result<Vec<String>> {
        Ok(self.db.recent_searches(limit.max(0) as u32)?)
    }

    pub fn clear_recent_searches(&self) -> Result<()> {
        Ok(self.db.clear_recent_searches()?)
    }

    /* ------------------------------ údržba ------------------------------ */

    /// Běžný úklid (denně na pozadí).
    pub fn prune(&self, max_titles: i32) -> Result<PruneInfo> {
        Ok(self.db.prune(Budget { max_titles: max_titles.max(0) as u32, ..Budget::default() })?.into())
    }

    /// Tlačítko "Pročistit paměť" v nastavení.
    pub fn deep_clean(&self, max_titles: i32) -> Result<PruneInfo> {
        Ok(self.db.deep_clean(Budget { max_titles: max_titles.max(0) as u32, ..Budget::default() })?.into())
    }
}

impl KuroCore {
    fn store_cards(&self, metas: &[kuro_core::meta::TitleMeta]) -> Result<Vec<TitleCard>> {
        let ids = self.db.upsert_cards(metas)?;
        Ok(ids.iter().filter_map(|id| self.db.title(*id).ok().flatten()).map(|t| TitleCard::from(&t)).collect())
    }
}

/// Střídavě film/seriál — Cinemeta řadí každý typ podle relevance zvlášť.
fn interleave<T>(a: Vec<T>, b: Vec<T>) -> Vec<T> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut a, mut b) = (a.into_iter(), b.into_iter());
    loop {
        match (a.next(), b.next()) {
            (None, None) => break,
            (x, y) => out.extend(x.into_iter().chain(y)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interleaves() {
        assert_eq!(interleave(vec![1, 3, 5, 7], vec![2, 4]), vec![1, 2, 3, 4, 5, 7]);
    }

    #[test]
    fn opens_core_offline() {
        let dir = std::env::temp_dir().join(format!("kuro-ffi-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let core = KuroCore::new(dir.join("k.db").to_string_lossy().into(), "test-device".into()).unwrap();
        assert!(!core.is_webshare_logged_in());
        assert!(core.watchlist().unwrap().is_empty());
        assert!(core.search_local("x".into(), None, 5).unwrap().is_empty());
        drop(core);
        let _ = std::fs::remove_dir_all(dir);
    }
}
