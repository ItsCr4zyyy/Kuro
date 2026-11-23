//! Typy, které vidí Kotlin. Záměrně ploché a jen se znaménkovými čísly
//! (Kotlin s UInt/ULong pracuje nepohodlně).

use kuro_core::finder::FoundStream;
use kuro_core::matcher::MediaKind as CoreKind;
use kuro_core::meta::EpisodeMeta;
use kuro_core::providers::{LinkPurpose as CoreLinkPurpose, ProviderId};
use kuro_core::storage::{self, StoredTitle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MediaKind {
    Movie,
    Series,
}

impl From<MediaKind> for CoreKind {
    fn from(k: MediaKind) -> Self {
        match k {
            MediaKind::Movie => CoreKind::Movie,
            MediaKind::Series => CoreKind::Series,
        }
    }
}

impl From<CoreKind> for MediaKind {
    fn from(k: CoreKind) -> Self {
        match k {
            CoreKind::Movie => MediaKind::Movie,
            CoreKind::Series => MediaKind::Series,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Catalog {
    Popular,
    TopRated,
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Provider {
    Webshare,
}

impl From<Provider> for ProviderId {
    fn from(p: Provider) -> Self {
        match p {
            Provider::Webshare => ProviderId::Webshare,
        }
    }
}

impl From<ProviderId> for Provider {
    fn from(p: ProviderId) -> Self {
        match p {
            ProviderId::Webshare => Provider::Webshare,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LinkPurpose {
    Stream,
    Download,
}

impl From<LinkPurpose> for CoreLinkPurpose {
    fn from(p: LinkPurpose) -> Self {
        match p {
            LinkPurpose::Stream => CoreLinkPurpose::Stream,
            LinkPurpose::Download => CoreLinkPurpose::Download,
        }
    }
}

/// Co uživatel s titulem udělal (pro skóre zájmu a profil vkusu).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum UserEvent {
    Impression,
    SearchClick,
    Opened,
}

impl From<UserEvent> for storage::Event {
    fn from(e: UserEvent) -> Self {
        match e {
            UserEvent::Impression => storage::Event::Impression,
            UserEvent::SearchClick => storage::Event::SearchClick,
            UserEvent::Opened => storage::Event::Opened,
        }
    }
}

/// Titul v seznamu.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TitleCard {
    pub id: i64,
    pub imdb_id: String,
    pub kind: MediaKind,
    /// Český název, když ho známe, jinak originál.
    pub title: String,
    pub original_title: String,
    pub year: Option<i32>,
    pub poster: Option<String>,
    pub background: Option<String>,
    pub rating: Option<f32>,
    pub genres: Vec<String>,
    pub anime: bool,
}

impl From<&StoredTitle> for TitleCard {
    fn from(t: &StoredTitle) -> Self {
        let m = &t.meta;
        Self {
            id: t.id,
            imdb_id: m.imdb_id.clone(),
            kind: m.kind.into(),
            title: m.display_title().to_owned(),
            original_title: m.title_orig.clone(),
            year: m.year.map(i32::from),
            poster: m.poster.clone(),
            background: m.background.clone(),
            rating: m.rating,
            genres: m.genres.clone(),
            anime: m.anime,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Episode {
    pub season: i32,
    pub episode: i32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub still: Option<String>,
    pub air_date: Option<String>,
}

impl From<&EpisodeMeta> for Episode {
    fn from(e: &EpisodeMeta) -> Self {
        Self {
            season: e.season.into(),
            episode: e.episode.into(),
            name: e.name.clone(),
            overview: e.overview.clone(),
            still: e.still.clone(),
            air_date: e.air_date.clone(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ProgressInfo {
    pub title_id: i64,
    /// 0 u filmu.
    pub season: i32,
    pub episode: i32,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub finished: bool,
    pub provider: Option<Provider>,
    pub ident: Option<String>,
    pub updated_at_ms: i64,
}

impl From<&storage::Progress> for ProgressInfo {
    fn from(p: &storage::Progress) -> Self {
        Self {
            title_id: p.target.title_id,
            season: p.target.season.into(),
            episode: p.target.episode.into(),
            position_ms: p.position_ms,
            duration_ms: p.duration_ms,
            finished: p.finished,
            provider: p.provider.map(Into::into),
            ident: p.ident.clone(),
            updated_at_ms: p.updated_at,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct TitleDetail {
    pub card: TitleCard,
    pub overview: Option<String>,
    pub logo: Option<String>,
    pub runtime_min: Option<i32>,
    pub countries: Vec<String>,
    pub release_date: Option<String>,
    /// Seřazené podle (sezóna, epizoda); prázdné u filmu.
    pub episodes: Vec<Episode>,
    pub in_watchlist: bool,
    /// Pozice v titulu (u seriálu po epizodách).
    pub progress: Vec<ProgressInfo>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct StreamInfo {
    pub provider: Provider,
    pub ident: String,
    pub filename: String,
    pub size_bytes: i64,
    pub height: Option<i32>,
    /// "DV" | "HDR10+" | "HDR10"
    pub hdr: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub channels: Option<String>,
    pub source: Option<String>,
    /// ISO 639-1
    pub audio_langs: Vec<String>,
    pub sub_langs: Vec<String>,
    pub dubbed: bool,
    pub dual_audio: bool,
    /// false = název o jazyce neříká nic → UI ukáže "zvuk neuveden", ne "originál".
    pub lang_explicit: bool,
    pub confidence: f64,
    pub ok_count: i32,
    pub fail_count: i32,
}

impl From<&FoundStream> for StreamInfo {
    fn from(s: &FoundStream) -> Self {
        Self {
            provider: s.provider.into(),
            ident: s.ident.clone(),
            filename: s.filename.clone(),
            size_bytes: s.size_bytes as i64,
            height: s.height.map(i32::from),
            hdr: s.hdr.map(|h| h.label().to_owned()),
            video_codec: s.video_codec.clone(),
            audio_codec: s.audio_codec.clone(),
            channels: s.channels.clone(),
            source: s.source.clone(),
            audio_langs: s.audio_langs.clone(),
            sub_langs: s.sub_langs.clone(),
            dubbed: s.dubbed,
            dual_audio: s.dual_audio,
            lang_explicit: s.lang_explicit,
            confidence: s.confidence,
            ok_count: s.ok_count as i32,
            fail_count: s.fail_count as i32,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct StreamList {
    pub streams: Vec<StreamInfo>,
    /// Není potřeba hledat znovu (jinak UI ukáže tyhle a zavolá `find_streams`).
    pub is_fresh: bool,
    pub searched_at_ms: Option<i64>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PlayLink {
    pub url: String,
    /// Hlavičky, které musí přehrávač poslat.
    pub headers: std::collections::HashMap<String, String>,
    /// Po tomhle čase si říct o nový odkaz. Odkaz NIKDY neukládat.
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ContinueItem {
    pub title: TitleCard,
    pub progress: ProgressInfo,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AccountInfo {
    pub username: Option<String>,
    pub vip: bool,
    pub vip_until: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PruneInfo {
    pub expired: i32,
    pub evicted: i32,
    pub titles_left: i32,
}

impl From<storage::PruneReport> for PruneInfo {
    fn from(r: storage::PruneReport) -> Self {
        Self { expired: r.expired as i32, evicted: r.evicted as i32, titles_left: r.titles_left as i32 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DownloadStatus {
    Queued,
    Running,
    Paused,
    Done,
    Failed,
}

impl From<storage::DownloadState> for DownloadStatus {
    fn from(s: storage::DownloadState) -> Self {
        use storage::DownloadState as S;
        match s {
            S::Queued => DownloadStatus::Queued,
            S::Running => DownloadStatus::Running,
            S::Paused => DownloadStatus::Paused,
            S::Done => DownloadStatus::Done,
            S::Failed => DownloadStatus::Failed,
        }
    }
}

impl From<DownloadStatus> for storage::DownloadState {
    fn from(s: DownloadStatus) -> Self {
        use storage::DownloadState as S;
        match s {
            DownloadStatus::Queued => S::Queued,
            DownloadStatus::Running => S::Running,
            DownloadStatus::Paused => S::Paused,
            DownloadStatus::Done => S::Done,
            DownloadStatus::Failed => S::Failed,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DownloadInfo {
    pub id: i64,
    pub title_id: i64,
    /// 0 u filmu.
    pub season: i32,
    pub episode: i32,
    pub provider: Option<Provider>,
    pub ident: String,
    pub filename: String,
    pub local_path: Option<String>,
    pub status: DownloadStatus,
    pub bytes_done: i64,
    pub bytes_total: i64,
    pub error: Option<String>,
    /// Vyplněné jen ve výpisu `downloads()`.
    pub title: Option<TitleCard>,
}

impl From<&storage::Download> for DownloadInfo {
    fn from(d: &storage::Download) -> Self {
        Self {
            id: d.id,
            title_id: d.target.title_id,
            season: d.target.season.into(),
            episode: d.target.episode.into(),
            provider: d.provider.map(Into::into),
            ident: d.ident.clone(),
            filename: d.filename.clone(),
            local_path: d.local_path.clone(),
            status: d.state.into(),
            bytes_done: d.bytes_done,
            bytes_total: d.bytes_total,
            error: d.error.clone(),
            title: None,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct EnqueueResult {
    pub id: i64,
    /// Soubor rozpracovaného stažení, které nový zdroj nahradil — smazat.
    pub stale_file: Option<String>,
}
