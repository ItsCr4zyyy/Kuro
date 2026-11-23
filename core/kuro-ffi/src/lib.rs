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
use kuro_core::meta::{AniList, CatalogId, Cinemeta, Wikidata};
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

/// Id řady doporučení "Pro tebe".
const FOR_YOU: &str = "for_you";
const MOVIE_DETAIL_TTL_MS: i64 = 30 * DAY_MS;

#[derive(uniffi::Object)]
pub struct KuroCore {
    db: Db,
    cinemeta: Cinemeta,
    wikidata: Wikidata,
    anilist: AniList,
    webshare: Arc<Webshare>,
    /// Všichni provideři (Webshare, Přehraj.to, Hellspy) pro přehrání podle id.
    providers: Vec<Arc<dyn kuro_core::providers::Provider>>,
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
        // Zdroje bez přihlášení: Přehraj.to (HTML) a Hellspy (JSON API).
        let providers: Vec<Arc<dyn kuro_core::providers::Provider>> = vec![
            webshare.clone(),
            Arc::new(kuro_core::providers::PrehrajTo::new(http.clone())),
            Arc::new(kuro_core::providers::Hellspy::new(http.clone())),
        ];
        let finder = StreamFinder::new(providers.clone(), FinderConfig::default());
        Ok(Arc::new(Self {
            db: Db::open(db_path)?,
            cinemeta: Cinemeta::new(http.clone()),
            anilist: AniList::new(http.clone()),
            wikidata: Wikidata::new(http),
            webshare,
            providers,
            finder,
        }))
    }

    /* ------------------------------ účet ------------------------------ */

    /// Přihlásí k Webshare a vrátí token k uložení do Keystore. Heslo se neukládá.
    pub async fn webshare_login(&self, username: String, password: String) -> Result<String> {
        Ok(self.webshare.login(&username, &password).await?)
    }

    pub fn set_webshare_token(&self, token: Option<String>) {
        let login = token.is_some() && self.webshare.token().is_none();
        self.webshare.set_token(token);
        if login {
            // Po přihlášení hledat znovu i u Webshare.
            let _ = self.db.mark_streams_stale();
        }
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
        // Katalog "year" (Novinky) má povinný parametr = rok, ne žánr.
        let genre = match (catalog, genre) {
            (Catalog::New, None) => Some(kuro_core::time::current_year().to_string()),
            (_, g) => g,
        };
        let metas = self.cinemeta.catalog(kind.into(), id, genre.as_deref(), skip.max(0) as u32).await?;
        self.store_cards(&metas)
    }

    /// Řada záložky Anime z AniListu (30 položek na stránku, `page` od 1).
    /// Jen anime, které má protějšek na IMDb (kvůli epizodám a zdrojům).
    pub async fn anime_catalog(&self, catalog: AnimeCatalog, page: i32) -> Result<Vec<TitleCard>> {
        let medias = self.anilist.list(catalog.into(), page.max(1) as u32).await?;
        let metas = kuro_core::meta::resolve_anime(&medias, &self.wikidata, &self.cinemeta).await;
        self.store_anime(metas)
    }

    /// Hledání přes Cinemetu (oba typy, když `kind` chybí). Výsledky se uloží
    /// jako krátkodobé karty; české názvy doplní `localize`.
    pub async fn search(&self, query: String, kind: Option<MediaKind>) -> Result<Vec<TitleCard>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(vec![]);
        }
        self.db.add_recent_search(q)?;

        // AniList souběžně: najde i romaji ("sousou no frieren") a doplní
        // anime názvy pro párování. Pomalé Wikidata nesmí brzdit hledání.
        let anime = async {
            let fut = async {
                let medias = self.anilist.search(q).await.ok()?;
                Some(kuro_core::meta::resolve_anime(&medias, &self.wikidata, &self.cinemeta).await)
            };
            tokio::time::timeout(std::time::Duration::from_secs(4), fut).await.ok().flatten().unwrap_or_default()
        };
        let cinemeta = async {
            Ok::<_, KuroError>(match kind {
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
            })
        };
        let (metas, anime) = tokio::join!(cinemeta, anime);
        let anime: Vec<_> = anime.into_iter().filter(|m| kind.is_none_or(|k| m.kind == k.into())).collect();
        let metas = match metas {
            Ok(m) => m,
            Err(e) if anime.is_empty() => return Err(e),
            Err(_) => vec![],
        };
        let mut cards = self.store_cards(&metas)?;
        // Anime z AniListu: aliasy uložit vždy, kartu přidat, jen když chybí.
        for c in self.store_anime(anime)? {
            if !cards.iter().any(|x| x.id == c.id) {
                cards.push(c);
            }
        }
        Ok(rank_by_name(cards, q))
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
        // Anime: romaji a další názvy z AniListu (soubory se tak často jmenují).
        let is_anime = self.db.title(title_id)?.is_some_and(|t| t.meta.anime);
        if is_anime && (stale || stored.localized_at.is_none()) {
            if let Ok(ids) = self.wikidata.anilist_ids(&stored.meta.imdb_id).await {
                if let Ok(medias) = self.anilist.by_ids(&ids).await {
                    let names: Vec<String> = medias
                        .iter()
                        .flat_map(|m| std::iter::once(kuro_core::meta::anilist::strip_season(m.title())).chain(m.names("")))
                        .collect();
                    self.db.add_aliases(title_id, &names)?;
                }
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

    /// Karta titulu z databáze (bez sítě, bez zápisu události).
    pub fn title_card(&self, title_id: i64) -> Result<Option<TitleCard>> {
        Ok(self.db.title(title_id)?.as_ref().map(TitleCard::from))
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
            streams: c.streams.iter().filter(|s| self.finder.is_active(s.provider)).map(StreamInfo::from).collect(),
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

        // Statistiky přehrání dřív než slučování: z kopií stejného souboru
        // má zůstat ta, která jde pustit.
        let mut streams = report.streams;
        self.db.attach_stats(&mut streams)?;
        let streams = kuro_core::finder::collapse_duplicates(streams);
        self.db.save_streams(tgt, &streams)?;
        let cached = self.db.cached_streams(tgt)?;
        Ok(cached.streams.iter().filter(|s| self.finder.is_active(s.provider)).map(StreamInfo::from).collect())
    }

    /// Zdroje vypnuté v nastavení (výchozí: žádné). Při zapnutí zdroje se
    /// uložené výsledky označí za staré, ať se dohledají.
    pub fn set_disabled_providers(&self, providers: Vec<Provider>) -> Result<()> {
        let ids = providers.into_iter().map(Into::into).collect();
        if self.finder.set_disabled(ids) {
            self.db.mark_streams_stale()?;
        }
        Ok(())
    }

    /// Přímý odkaz pro přehrávač / stahování. Nikdy ho neukládat — je
    /// vázaný na účet a IP a vyprší.
    pub async fn resolve_stream(&self, provider: Provider, ident: String, purpose: LinkPurpose) -> Result<PlayLink> {
        let p: kuro_core::providers::ProviderId = provider.into();
        let prov = self.providers.iter().find(|x| x.id() == p).ok_or(KuroError::NotFound)?;
        match prov.resolve(&ident, purpose.into()).await {
            Ok(link) => Ok(PlayLink {
                url: link.url,
                headers: link.headers.into_iter().collect(),
                expires_at_ms: link.expires_at_ms,
                subtitles: link
                    .subtitles
                    .into_iter()
                    .map(|s| SubtitleInfo { lang: s.lang, label: s.label, url: s.url })
                    .collect(),
            }),
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
    #[allow(clippy::too_many_arguments)] // plochý podpis pro Kotlin
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

    /// Pozice v titulu (u seriálu po epizodách) — levné, bez sítě a bez
    /// zápisu události (na rozdíl od `title_detail`).
    pub fn title_progress(&self, title_id: i64) -> Result<Vec<ProgressInfo>> {
        Ok(self.db.title_progress(title_id)?.iter().map(ProgressInfo::from).collect())
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

    /* ------------------------------ stahování ------------------------------ */

    /// Zařadí stažení zdroje. Vrací id; když nahrazuje rozpracované stažení
    /// jiného zdroje, smaže jeho soubor (`stale_file`) volající.
    #[allow(clippy::too_many_arguments)] // plochý podpis pro Kotlin
    pub fn enqueue_download(
        &self,
        title_id: i64,
        season: i32,
        episode: i32,
        provider: Provider,
        ident: String,
        filename: String,
        size_bytes: i64,
    ) -> Result<EnqueueResult> {
        let (id, stale) = self.db.enqueue_download(target(title_id, season, episode), provider.into(), &ident, &filename, size_bytes)?;
        Ok(EnqueueResult { id, stale_file: stale })
    }

    pub fn downloads(&self) -> Result<Vec<DownloadInfo>> {
        Ok(self
            .db
            .downloads()?
            .iter()
            .map(|d| DownloadInfo { title: Some(TitleCard::from(&d.title)), ..DownloadInfo::from(&d.download) })
            .collect())
    }

    pub fn download(&self, id: i64) -> Result<Option<DownloadInfo>> {
        Ok(self.db.download(id)?.as_ref().map(DownloadInfo::from))
    }

    pub fn download_for(&self, title_id: i64, season: i32, episode: i32) -> Result<Option<DownloadInfo>> {
        Ok(self.db.download_for(target(title_id, season, episode))?.as_ref().map(DownloadInfo::from))
    }

    /// Stažení, která mají běžet (po restartu).
    pub fn pending_downloads(&self) -> Result<Vec<DownloadInfo>> {
        Ok(self.db.pending_downloads()?.iter().map(DownloadInfo::from).collect())
    }

    /// Průběh z workeru. Prázdná volitelná pole = beze změny.
    pub fn update_download(
        &self,
        id: i64,
        status: DownloadStatus,
        bytes_done: i64,
        bytes_total: Option<i64>,
        local_path: Option<String>,
        error: Option<String>,
    ) -> Result<()> {
        Ok(self.db.update_download(id, status.into(), bytes_done, bytes_total, local_path.as_deref(), error.as_deref())?)
    }

    /// Smaže záznam a vrátí cestu k souboru, který má smazat volající.
    pub fn delete_download(&self, id: i64) -> Result<Option<String>> {
        Ok(self.db.delete_download(id)?)
    }

    /* ------------------------------ doporučení ------------------------------ */

    /// Uložená řada "Pro tebe" (okamžitě, bez sítě).
    pub fn recommendations(&self) -> Result<RecRow> {
        let (items, at) = self.db.rec_row(FOR_YOU)?;
        Ok(RecRow { items: items.iter().map(TitleCard::from).collect(), computed_at_ms: at })
    }

    /// Spočítá řadu "Pro tebe" z profilu vkusu a veřejných katalogů.
    /// `check_availability`: ověřit u Webshare, že k titulu je zdroj
    /// (pomalejší, ~20 dotazů; volá denní úloha na Wi-Fi).
    pub async fn compute_recommendations(&self, check_availability: bool) -> Result<RecRow> {
        use kuro_core::matcher::MediaKind as K;
        use kuro_core::recommend::{rank, Profile};

        let profile = Profile::from_taste(&self.db.taste("", 500)?);
        if profile.is_empty() {
            self.db.save_rec_row(FOR_YOU, &[])?;
            return self.recommendations();
        }

        // Kandidáti: oblíbené žánry (populární + doporučené) a novinky.
        let year = kuro_core::time::current_year().to_string();
        let mut requests: Vec<(K, CatalogId, Option<String>)> = vec![];
        for kind in [K::Movie, K::Series] {
            if profile.kind_weight(kind) < 0.15 {
                continue;
            }
            for g in profile.top_genres(3) {
                requests.push((kind, CatalogId::Top, Some(g.clone())));
                requests.push((kind, CatalogId::ImdbRating, Some(g)));
            }
            requests.push((kind, CatalogId::Year, Some(year.clone())));
        }
        let pages = futures_util::future::join_all(
            requests.iter().map(|(k, c, g)| self.cinemeta.catalog(*k, *c, g.as_deref(), 0)),
        )
        .await;

        let mut seen = std::collections::HashSet::new();
        let metas: Vec<kuro_core::meta::TitleMeta> = pages
            .into_iter()
            .filter_map(|p| p.ok())
            .flatten()
            .filter(|m| seen.insert((m.kind, m.imdb_id.clone())))
            .collect();
        if metas.is_empty() {
            // Bez sítě: nechat starou řadu, nepřepisovat prázdnou.
            return self.recommendations();
        }

        let ids = self.db.upsert_cards(&metas)?;
        let excluded = self.db.rec_excluded_ids()?;
        let candidates: Vec<(i64, kuro_core::meta::TitleMeta)> =
            ids.into_iter().zip(metas).filter(|(id, _)| !excluded.contains(id)).collect();
        let ranked = rank(&candidates, &profile, kuro_core::time::current_year(), 40);

        let chosen: Vec<(i64, f64)> = if check_availability {
            // Do řady jen to, co jde pustit. Strop na počet ověření šetří Webshare.
            let mut ok = vec![];
            for (id, score) in ranked.iter().take(24) {
                let is_series = candidates.iter().any(|(cid, m)| cid == id && m.kind == K::Series);
                let (s, e) = if is_series { (1, 1) } else { (0, 0) };
                let cached = self.db.cached_streams(target(*id, s, e))?;
                let available = if cached.is_fresh {
                    cached.streams.iter().any(|s| self.finder.is_active(s.provider))
                } else {
                    self.find_streams(*id, s, e, None).await.map(|v| !v.is_empty()).unwrap_or(false)
                };
                if available {
                    ok.push((*id, *score));
                }
                if ok.len() >= 15 {
                    break;
                }
            }
            ok
        } else {
            ranked.into_iter().take(20).collect()
        };

        self.db.save_rec_row(FOR_YOU, &chosen)?;
        self.recommendations()
    }

    /// Denní kontrola nových dílů rozkoukaných seriálů (bez zápisu
    /// "otevřeno" do vkusu). Vrací počet obnovených seriálů.
    pub async fn refresh_series_in_progress(&self) -> Result<i32> {
        let mut n = 0;
        for t in self.db.series_in_progress(15)? {
            if let Ok(Some(meta)) = self.cinemeta.meta(t.meta.kind, &t.meta.imdb_id).await {
                self.db.upsert_detail(&meta)?;
                n += 1;
            }
        }
        Ok(n)
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
    /// Uloží anime z AniListu. U titulu, který už známe z Cinemety, nechá
    /// jeho název a pozadí (AniList banner je úzký pruh) a jen přidá aliasy.
    fn store_anime(&self, mut metas: Vec<kuro_core::meta::TitleMeta>) -> Result<Vec<TitleCard>> {
        for m in &mut metas {
            if let Some(t) = self.db.title_by_imdb(m.kind, &m.imdb_id)? {
                if t.detail_at.is_some() {
                    let ani = std::mem::replace(&mut m.title_orig, t.meta.title_orig.clone());
                    m.aliases.push(ani);
                    if t.meta.background.is_some() {
                        m.background = None;
                    }
                    if t.meta.year.is_some() {
                        m.year = None;
                    }
                }
            }
        }
        self.store_cards(&metas)
    }

    fn store_cards(&self, metas: &[kuro_core::meta::TitleMeta]) -> Result<Vec<TitleCard>> {
        let ids = self.db.upsert_cards(metas)?;
        Ok(ids.iter().filter_map(|id| self.db.title(*id).ok().flatten()).map(|t| TitleCard::from(&t)).collect())
    }
}

/// Přesná shoda názvu první, pak názvy začínající dotazem, pak zbytek
/// (stabilně, v pořadí Cinemety). Bez toho "Breaking Bad" vrátí nejdřív
/// film El Camino a "Ozark" vánoční film, protože se střídá film/seriál.
fn rank_by_name(cards: Vec<TitleCard>, query: &str) -> Vec<TitleCard> {
    use kuro_core::text::normalize_title;
    let q = normalize_title(query);
    let class = |c: &TitleCard| {
        let names = [normalize_title(&c.title), normalize_title(&c.original_title)];
        if names.contains(&q) {
            0
        } else if names.iter().any(|n| n.starts_with(&q)) {
            1
        } else {
            2
        }
    };
    let mut indexed: Vec<(usize, TitleCard)> = cards.into_iter().enumerate().collect();
    indexed.sort_by_key(|(i, c)| (class(c), *i));
    indexed.into_iter().map(|(_, c)| c).collect()
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

    fn card(title: &str, orig: &str) -> TitleCard {
        TitleCard {
            id: 0,
            imdb_id: String::new(),
            kind: MediaKind::Movie,
            title: title.into(),
            original_title: orig.into(),
            year: None,
            poster: None,
            background: None,
            rating: None,
            genres: vec![],
            anime: false,
        }
    }

    #[test]
    fn exact_name_first() {
        let cards = vec![card("El Camino", "El Camino: A Breaking Bad Movie"), card("Perníkový táta", "Breaking Bad"), card("Breaking Bad Recap", "Breaking Bad Recap")];
        let out: Vec<String> = rank_by_name(cards, "breaking bad").into_iter().map(|c| c.title).collect();
        assert_eq!(out, vec!["Perníkový táta", "Breaking Bad Recap", "El Camino"]);
    }

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

        // Webshare bez přihlášení neaktivní, zdroje bez účtu ano — dokud se nevypnou.
        use kuro_core::providers::ProviderId as P;
        assert!(!core.finder.is_active(P::Webshare));
        assert!(core.finder.is_active(P::Hellspy));
        core.set_disabled_providers(vec![Provider::Hellspy]).unwrap();
        assert!(!core.finder.is_active(P::Hellspy));
        assert!(core.finder.is_active(P::PrehrajTo));
        core.set_disabled_providers(vec![]).unwrap();
        assert!(core.finder.is_active(P::Hellspy));
        drop(core);
        let _ = std::fs::remove_dir_all(dir);
    }
}
