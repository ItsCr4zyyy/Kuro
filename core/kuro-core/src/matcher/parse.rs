//! Parser názvů souborů z release scény (NSC `matcher/parse.ts`).
//!
//! "Interstellar.2014.2160p.UHD.BluRay.x265.HDR.CZ.dabing-GROUP.mkv"
//!   -> title "Interstellar", year 2014, height 2160, hdr HDR10,
//!      video h265, audio ["cs"], dubbed

use std::sync::LazyLock;

use regex::Regex;

use super::languages::detect_languages;
use crate::rx::{rx, rx_cs};
use crate::text::normalize_title;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hdr {
    DolbyVision,
    Hdr10Plus,
    Hdr10,
}

impl Hdr {
    pub fn label(self) -> &'static str {
        match self {
            Hdr::DolbyVision => "DV",
            Hdr::Hdr10Plus => "HDR10+",
            Hdr::Hdr10 => "HDR10",
        }
    }

    pub fn from_label(s: &str) -> Option<Self> {
        match s {
            "DV" => Some(Hdr::DolbyVision),
            "HDR10+" => Some(Hdr::Hdr10Plus),
            "HDR10" => Some(Hdr::Hdr10),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedRelease {
    /// Název vytažený z názvu souboru, ještě nenormalizovaný.
    pub title: String,
    pub title_norm: String,
    pub year: Option<u16>,
    pub season: Option<u16>,
    pub episode: Option<u16>,
    /// Rozsah epizod v jednom souboru (E01-E03) — takové soubory nechceme.
    pub episode_to: Option<u16>,
    pub height: Option<u16>,
    pub source: Option<&'static str>,
    pub video_codec: Option<&'static str>,
    pub audio_codec: Option<&'static str>,
    pub channels: Option<&'static str>,
    pub hdr: Option<Hdr>,
    pub audio_langs: Vec<String>,
    pub sub_langs: Vec<String>,
    pub dubbed: bool,
    /// Víc zvukových stop — v přehrávači půjde přepínat.
    pub dual_audio: bool,
    /// Nesl název vůbec nějakou informaci o jazyce? false znamená
    /// "nevíme", ne "je to originál".
    pub lang_explicit: bool,
    pub group: Option<String>,
    pub extension: Option<String>,
    /// Důvod, proč soubor rovnou zahodit (sample, archiv, ...).
    pub reject: Option<String>,
}

const VIDEO_EXT: &[&str] = &[
    "mkv", "mp4", "avi", "m4v", "mov", "ts", "m2ts", "mpg", "mpeg", "wmv", "flv", "webm",
];

const REJECT_EXT: &[&str] = &[
    "exe", "rar", "zip", "7z", "iso", "nfo", "txt", "jpg", "png", "srt", "sub", "idx", "lnk", "bat",
    "cmd", "msi", "apk", "part", "r00", "r01",
];

type Table<T> = LazyLock<Vec<(Regex, T)>>;

fn table<T: Copy>(rows: &[(&str, T)]) -> Vec<(Regex, T)> {
    rows.iter().map(|(p, v)| (rx(p), *v)).collect()
}

fn first_match<T: Copy>(s: &str, table: &[(Regex, T)]) -> Option<T> {
    table.iter().find(|(re, _)| re.is_match(s)).map(|(_, v)| *v)
}

// Tokeny, které v názvu ukončují titul — všechno za nimi jsou technické údaje.
static STOP_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    rx(r"\b(\d{3,4}p|4k|uhd|bluray|blu-ray|bdrip|brrip|bdremux|remux|web-?dl|web-?rip|webhd|hdtv|dvdrip|dvdscr|hdrip|x264|x265|h\.?264|h\.?265|hevc|avc|xvid|divx|av1|hdr10?\+?|dolby|dts|truehd|atmos|ddp?5|ac3|aac|eac3|flac|mp3|cz|sk|en|cztit|cz-?dab|sk-?dab|dab|dub|dabing|titulky|tit|multi|dual|complete|proper|repack|extended|unrated|remastered|imax|directors?\.?cut)\b")
});

static RESOLUTION: Table<u16> = LazyLock::new(|| {
    table(&[
        // "4KHDR" / "4KUHD" bývají slepené bez oddělovače.
        (r"\b(4k|4khdr|4kuhd|uhd|2160p?)\b", 2160),
        (r"\b1440p\b", 1440),
        (r"\b(fullhd|fhd|1080[pi]?)\b", 1080),
        (r"\b(hd|720[pi]?)\b", 720),
        (r"\b(576[pi]?)\b", 576),
        (r"\b(480[pi]?|sd)\b", 480),
    ])
});

static SOURCE: Table<&'static str> = LazyLock::new(|| {
    table(&[
        (r"\b(bd)?remux\b", "remux"),
        (r"\b(blu-?ray|bdrip|brrip|bd-?rip)\b", "bluray"),
        (r"\b(web-?dl|webdl)\b", "webdl"),
        (r"\b(web-?rip|webrip|webhd|web)\b", "webrip"),
        (r"\bhdtv\b", "hdtv"),
        (r"\b(dvdrip|dvd-?r|dvd5|dvd9)\b", "dvd"),
        (r"\b(hdrip)\b", "hdrip"),
        (r"\b(ts|telesync|cam|hdcam|camrip|ts-?rip|screener|dvdscr|scr)\b", "cam"),
    ])
});

static VCODEC: Table<&'static str> = LazyLock::new(|| {
    table(&[
        (r"\b(x265|h\.?265|hevc)\b", "h265"),
        (r"\b(x264|h\.?264|avc)\b", "h264"),
        (r"\bav1\b", "av1"),
        (r"\b(xvid|divx|mpeg-?4)\b", "xvid"),
        (r"\bvp9\b", "vp9"),
    ])
});

static ACODEC: Table<&'static str> = LazyLock::new(|| {
    table(&[
        (r"\b(truehd|true-?hd)\b", "truehd"),
        (r"\batmos\b", "atmos"),
        (r"\b(dts-?hd|dtshd|dts-?ma)\b", "dtshd"),
        (r"\bdts\b", "dts"),
        (r"\b(e-?ac-?3|eac3|ddp|dd\+|dolby-?digital-?plus)\b", "eac3"),
        (r"\b(ac-?3|ac3|dd5|dd2|dolby-?digital)\b", "ac3"),
        (r"\b(aac)\b", "aac"),
        (r"\b(flac)\b", "flac"),
        (r"\b(mp3)\b", "mp3"),
        (r"\b(opus)\b", "opus"),
    ])
});

static REJECT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    rx(r"\b(sample|trailer|ukazka|upoutavka|teaser|behind-?the-?scenes|featurette|making-?of|extras?|bonus-?disc|soundtrack|ost)\b")
});

static YEAR: LazyLock<Regex> = LazyLock::new(|| rx(r"\b(19[0-9]{2}|20[0-4][0-9])\b"));

static EPISODE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // S01E02, S01E02E03, S01E02-E03; skupina 3 = "e" před druhým číslem.
        rx(r"\bs(\d{1,2})[\s._-]*e(\d{1,3})(?:[\s._-]*(e)?(\d{1,3}))?\b"),
        rx(r"\b(\d{1,2})x(\d{1,3})\b"),                                 // 1x02
        rx(r"\bseason[\s._-]*(\d{1,2})[\s._-]*episode[\s._-]*(\d{1,3})\b"),
        rx(r"\bserie[\s._-]*(\d{1,2})[\s._-]*(?:dil|epizoda)[\s._-]*(\d{1,3})\b"),
    ]
});

// V JS je první vzor `\bs(\d{1,2})(?![\s._-]*e\d)\b`. `regex` lookahead
// nemá, takže se negativní podmínka kontroluje ručně v `season_only()`.
static SEASON_S: LazyLock<Regex> = LazyLock::new(|| rx(r"\bs(\d{1,2})\b"));
static FOLLOWED_BY_EPISODE: LazyLock<Regex> = LazyLock::new(|| rx(r"^[\s._-]*e\d"));
static SEASON_ONLY: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![rx(r"\bseason[\s._-]*(\d{1,2})\b"), rx(r"\b(\d{1,2})[\s._-]*serie\b")]
});

static EXT: LazyLock<Regex> = LazyLock::new(|| rx(r"\.([a-z0-9]{2,4})$"));
static DOTS: LazyLock<Regex> = LazyLock::new(|| rx_cs(r"[._]+"));
static LEADING_TAG: LazyLock<Regex> = LazyLock::new(|| rx_cs(r"^[\[(][^\])]*[\])]\s*"));
static TRAILING_DASH: LazyLock<Regex> = LazyLock::new(|| rx_cs(r"[-\s]+$"));
static TRAILING_BRACKET: LazyLock<Regex> = LazyLock::new(|| rx_cs(r"\[[^\]]*\]$"));
static GROUP: LazyLock<Regex> = LazyLock::new(|| rx(r"-([a-z0-9]{2,20})$"));
static CHANNELS: LazyLock<Regex> = LazyLock::new(|| rx(r"\b([578])[\s._-]?1\b"));
static STEREO: LazyLock<Regex> = LazyLock::new(|| rx(r"\b2[\s._-]?0\b"));
static DV: LazyLock<Regex> = LazyLock::new(|| rx(r"\b(dv|dovi|dolby[\s-]?vision)\b"));
static HDR10P: LazyLock<Regex> = LazyLock::new(|| rx(r"\bhdr10\+|\bhdr\+\b"));
static HDR10: LazyLock<Regex> = LazyLock::new(|| rx(r"\b(4k)?hdr(10)?\b"));

fn num(m: Option<regex::Match<'_>>) -> Option<u16> {
    m.and_then(|m| m.as_str().parse().ok())
}

/// Sezóna bez epizody ("S02", "Season 2", "2 serie") a pozice nálezu.
fn season_only(spaced: &str) -> Option<(u16, usize)> {
    for c in SEASON_S.captures_iter(spaced) {
        let whole = c.get(0).unwrap();
        if !FOLLOWED_BY_EPISODE.is_match(&spaced[whole.end()..]) {
            return Some((num(c.get(1))?, whole.start()));
        }
    }
    SEASON_ONLY
        .iter()
        .find_map(|re| re.captures(spaced).and_then(|c| Some((num(c.get(1))?, c.get(0)?.start()))))
}

/// Rozparsuje název souboru z release scény na strukturovaná data.
pub fn parse_release(raw_name: &str) -> ParsedRelease {
    let name = raw_name.rsplit(['/', '\\']).next().unwrap_or(raw_name);

    let (extension, stem) = match EXT.captures(name) {
        Some(c) => (
            Some(c[1].to_lowercase()),
            &name[..c.get(0).unwrap().start()],
        ),
        None => (None, name),
    };

    // Tečky/podtržítka na mezery, ale pomlčku nech — je v ní release group
    // a v "dts-hd" apod.
    let spaced = crate::text::collapse(&DOTS.replace_all(stem, " "));

    let mut reject = match extension.as_deref() {
        Some(e) if REJECT_EXT.contains(&e) => Some(format!("nepodporovaná přípona .{e}")),
        Some(e) if !VIDEO_EXT.contains(&e) => Some(format!("neznámá přípona .{e}")),
        _ => None,
    };
    if reject.is_none() && REJECT_PATTERN.is_match(&spaced) {
        reject = Some("sample/trailer/bonus".into());
    }

    // --- epizoda ---------------------------------------------------
    let mut season = None;
    let mut episode = None;
    let mut episode_to = None;
    let mut episode_idx = None;

    for re in EPISODE.iter() {
        if let Some(c) = re.captures(&spaced) {
            season = num(c.get(1));
            episode = num(c.get(2));
            // Druhé číslo jen u prvního vzoru (S01E02…). Rozsah dílů je to,
            // když před ním stojí "E" (S05E03-E04) nebo navazuje (E01-02).
            // "S01E01-08" bez E je česká konvence "díl 1 z 8", ne 8 dílů.
            let to = num(c.get(4));
            let has_e = c.get(3).is_some();
            episode_to = match (episode, to) {
                (Some(e), Some(t)) if has_e || t == e + 1 => Some(t),
                _ => None,
            };
            episode_idx = Some(c.get(0).unwrap().start());
            break;
        }
    }
    if season.is_none() {
        if let Some((s, idx)) = season_only(&spaced) {
            season = Some(s);
            episode_idx = Some(idx);
        }
    }

    // --- rok -------------------------------------------------------
    // První výskyt — pozdější bývá součást release tagu.
    let max_year = crate::time::current_year() + 2;
    let mut year = None;
    let mut year_idx = None;
    for m in YEAR.find_iter(&spaced) {
        let val: u16 = m.as_str().parse().unwrap_or(0);
        if (1900..=max_year).contains(&val) {
            year = Some(val);
            year_idx = Some(m.start());
            break;
        }
    }

    // --- titul -----------------------------------------------------
    // Uříznout na prvním "hraničním" tokenu: epizoda, rok, technický tag.
    let cut = [episode_idx, year_idx, STOP_TOKEN.find(&spaced).map(|m| m.start())]
        .into_iter()
        .flatten()
        .min();

    let title_raw = cut.map_or(spaced.as_str(), |c| &spaced[..c]).trim();
    let title = LEADING_TAG.replace(title_raw, "");
    let title = TRAILING_DASH.replace(title.trim(), "").trim().to_owned();

    // --- release group --------------------------------------------
    let stem_for_group = TRAILING_BRACKET.replace(stem, "");
    let group = GROUP.captures(stem_for_group.trim()).map(|c| c[1].to_owned());

    // --- technické atributy ---------------------------------------
    let height = first_match(&spaced, &RESOLUTION);
    let source = first_match(&spaced, &SOURCE);
    let video_codec = first_match(&spaced, &VCODEC);
    let audio_codec = first_match(&spaced, &ACODEC);

    let channels = match CHANNELS.captures(&spaced) {
        Some(c) => Some(match &c[1] {
            "5" => "5.1",
            "7" => "7.1",
            _ => "8.1",
        }),
        None if STEREO.is_match(&spaced) => Some("2.0"),
        None => None,
    };

    let hdr = if DV.is_match(&spaced) {
        Some(Hdr::DolbyVision)
    } else if HDR10P.is_match(&spaced) {
        Some(Hdr::Hdr10Plus)
    } else if HDR10.is_match(&spaced) {
        Some(Hdr::Hdr10)
    } else {
        None
    };

    // Jazyky jen v technické části (za titulem), jinak "Skyfall"
    // nebo "Den nezavislosti" nasekají falešné shody v samotném názvu.
    let tail = cut.map_or("", |c| &spaced[c..]);
    let langs = detect_languages(tail);

    if reject.is_none() && episode_to.is_some() && episode_to != episode {
        reject = Some("více epizod v jednom souboru".into());
    }

    ParsedRelease {
        title_norm: normalize_title(&title),
        title,
        year,
        season,
        episode,
        episode_to,
        height,
        source,
        video_codec,
        audio_codec,
        channels,
        hdr,
        audio_langs: langs.audio,
        sub_langs: langs.subs,
        dubbed: langs.dubbed,
        dual_audio: langs.dual_audio,
        lang_explicit: langs.explicit,
        group,
        extension,
        reject,
    }
}
