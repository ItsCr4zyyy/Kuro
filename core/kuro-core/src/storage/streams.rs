//! Cache nalezených zdrojů a zpětná vazba z přehrávání.
//!
//! "Ukaž hned, pak potichu obnov": při otevření titulu se vrátí uložené
//! zdroje a `is_fresh` řekne, jestli je potřeba hledat znovu na pozadí.

use rusqlite::{params, OptionalExtension};

use super::{from_json, to_json, Db, DAY_MS};
use crate::error::Result;
use crate::finder::{sort_streams, FoundStream};
use crate::matcher::Hdr;
use crate::providers::ProviderId;

/// Kolik nejlepších zdrojů se k jednomu cíli drží.
pub const MAX_STREAMS_PER_TARGET: usize = 20;

/// Film nebo konkrétní epizoda. Film má `season = episode = 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Target {
    pub title_id: i64,
    pub season: u16,
    pub episode: u16,
}

impl Target {
    pub fn movie(title_id: i64) -> Self {
        Self { title_id, season: 0, episode: 0 }
    }
    pub fn episode(title_id: i64, season: u16, episode: u16) -> Self {
        Self { title_id, season, episode }
    }
    pub fn is_episode(&self) -> bool {
        self.season > 0 || self.episode > 0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CachedStreams {
    /// Seřazené, s ok/fail statistikou z přehrávání.
    pub streams: Vec<FoundStream>,
    /// Kdy se naposledy hledalo; None = nikdy.
    pub searched_at: Option<i64>,
    /// Není potřeba hledat znovu.
    pub is_fresh: bool,
}

/// Jak dlouho platí výsledek hledání zdrojů.
/// Epizody 3 dny, novinky do 30 dnů od vydání 1 den, ostatní filmy 7 dní.
pub fn stream_ttl_ms(is_episode: bool, release_date: Option<&str>, now: i64) -> i64 {
    if is_episode {
        return 3 * DAY_MS;
    }
    let is_new = release_date
        .and_then(days_since_epoch)
        .is_some_and(|d| now / DAY_MS - d <= 30);
    if is_new { DAY_MS } else { 7 * DAY_MS }
}

/// "YYYY-MM-DD" -> počet dnů od 1970-01-01 (proleptický gregoriánský kalendář).
fn days_since_epoch(date: &str) -> Option<i64> {
    let mut it = date.get(..10)?.split('-');
    let (y, m, d): (i64, i64, i64) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    // Howard Hinnant, days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

impl Db {
    /// Nahradí uložené zdroje k cíli novým výsledkem hledání (max. 20 nejlepších).
    ///
    /// Prázdný výsledek NEMAŽE dřív nalezené zdroje — Webshare pod zátěží
    /// občas nevrátí nic i pro živé soubory. Jen se zapíše čas hledání.
    pub fn save_streams(&self, target: Target, streams: &[FoundStream]) -> Result<()> {
        let now = self.now();
        let mut best: Vec<FoundStream> = streams.to_vec();
        sort_streams(&mut best);
        best.truncate(MAX_STREAMS_PER_TARGET);

        self.with(|c| {
            let tx = c.transaction()?;
            let key = params![target.title_id, target.season, target.episode];
            if !best.is_empty() {
                tx.execute("DELETE FROM stream WHERE title_id = ?1 AND season = ?2 AND episode = ?3", key)?;
                let mut stmt = tx.prepare(
                    "INSERT OR REPLACE INTO stream (title_id, season, episode, provider, ident, filename,
                         size_bytes, height, hdr, video_codec, audio_codec, channels, source, audio_langs,
                         sub_langs, dubbed, dual_audio, lang_explicit, confidence, positive_votes, negative_votes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
                )?;
                for s in &best {
                    stmt.execute(params![
                        target.title_id,
                        target.season,
                        target.episode,
                        s.provider.as_str(),
                        s.ident,
                        s.filename,
                        s.size_bytes as i64,
                        s.height,
                        s.hdr.map(Hdr::label),
                        s.video_codec,
                        s.audio_codec,
                        s.channels,
                        s.source,
                        to_json(&s.audio_langs),
                        to_json(&s.sub_langs),
                        s.dubbed,
                        s.dual_audio,
                        s.lang_explicit,
                        s.confidence,
                        s.positive_votes,
                        s.negative_votes,
                    ])?;
                }
            }
            tx.execute(
                "INSERT INTO stream_search (title_id, season, episode, searched_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (title_id, season, episode) DO UPDATE SET searched_at = excluded.searched_at",
                params![target.title_id, target.season, target.episode, now],
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Všechny uložené výsledky označí za staré (přibyl zdroj / přihlášení):
    /// ukážou se hned, ale na pozadí se hledá znovu.
    pub fn mark_streams_stale(&self) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE stream_search SET searched_at = 0", [])?;
            Ok(())
        })
    }

    /// Uložené zdroje k cíli, seřazené, se statistikou z přehrávání.
    pub fn cached_streams(&self, target: Target) -> Result<CachedStreams> {
        let now = self.now();
        self.with(|c| {
            let searched_at: Option<i64> = c
                .query_row(
                    "SELECT searched_at FROM stream_search WHERE title_id = ?1 AND season = ?2 AND episode = ?3",
                    params![target.title_id, target.season, target.episode],
                    |r| r.get(0),
                )
                .optional()?;
            let release: Option<String> = c
                .query_row("SELECT release_date FROM title WHERE id = ?1", [target.title_id], |r| r.get(0))
                .optional()?
                .flatten();

            let mut stmt = c.prepare(
                "SELECT s.provider, s.ident, s.filename, s.size_bytes, s.height, s.hdr, s.video_codec,
                        s.audio_codec, s.channels, s.source, s.audio_langs, s.sub_langs, s.dubbed,
                        s.dual_audio, s.lang_explicit, s.confidence, s.positive_votes, s.negative_votes,
                        COALESCE(st.ok_count, 0), COALESCE(st.fail_count, 0)
                 FROM stream s
                 LEFT JOIN stream_stats st ON st.provider = s.provider AND st.ident = s.ident
                 WHERE s.title_id = ?1 AND s.season = ?2 AND s.episode = ?3",
            )?;
            let mut streams: Vec<FoundStream> = stmt
                .query_map(params![target.title_id, target.season, target.episode], |r| {
                    Ok((r.get::<_, String>(0)?, FoundStream {
                        provider: ProviderId::Webshare, // přepíše se níž
                        ident: r.get(1)?,
                        filename: r.get(2)?,
                        size_bytes: r.get::<_, i64>(3)? as u64,
                        height: r.get(4)?,
                        hdr: r.get::<_, Option<String>>(5)?.as_deref().and_then(Hdr::from_label),
                        video_codec: r.get(6)?,
                        audio_codec: r.get(7)?,
                        channels: r.get(8)?,
                        source: r.get(9)?,
                        audio_langs: from_json(&r.get::<_, String>(10)?),
                        sub_langs: from_json(&r.get::<_, String>(11)?),
                        dubbed: r.get(12)?,
                        dual_audio: r.get(13)?,
                        lang_explicit: r.get(14)?,
                        confidence: r.get(15)?,
                        positive_votes: r.get(16)?,
                        negative_votes: r.get(17)?,
                        ok_count: r.get(18)?,
                        fail_count: r.get(19)?,
                    }))
                })?
                .filter_map(|row| match row {
                    // Zdroj od providera, kterého tahle verze nezná, se přeskočí.
                    Ok((p, mut s)) => ProviderId::parse(&p).map(|pid| {
                        s.provider = pid;
                        Ok(s)
                    }),
                    Err(e) => Some(Err(e)),
                })
                .collect::<rusqlite::Result<_>>()?;
            sort_streams(&mut streams);

            let ttl = stream_ttl_ms(target.is_episode(), release.as_deref(), now);
            let is_fresh = searched_at.is_some_and(|at| now - at < ttl);
            Ok(CachedStreams { streams, searched_at, is_fresh })
        })
    }

    /// Doplní ok/fail statistiky k čerstvě nalezeným zdrojům (finder je nezná).
    pub fn attach_stats(&self, streams: &mut [FoundStream]) -> Result<()> {
        self.with(|c| {
            let mut stmt =
                c.prepare("SELECT ok_count, fail_count FROM stream_stats WHERE provider = ?1 AND ident = ?2")?;
            for s in streams.iter_mut() {
                if let Some((ok, fail)) = stmt
                    .query_row(params![s.provider.as_str(), s.ident], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()?
                {
                    s.ok_count = ok;
                    s.fail_count = fail;
                }
            }
            Ok(())
        })?;
        sort_streams(streams);
        Ok(())
    }

    /// Výsledek přehrání zdroje. Nefunkční zdroje tím klesají v řazení.
    pub fn report_stream(&self, provider: ProviderId, ident: &str, ok: bool) -> Result<()> {
        let now = self.now();
        self.with(|c| {
            c.execute(
                "INSERT INTO stream_stats (provider, ident, ok_count, fail_count, last_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (provider, ident) DO UPDATE SET
                     ok_count = ok_count + excluded.ok_count,
                     fail_count = fail_count + excluded.fail_count,
                     last_at = excluded.last_at",
                params![provider.as_str(), ident, ok as i64, (!ok) as i64, now],
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_days() {
        assert_eq!(days_since_epoch("1970-01-01"), Some(0));
        assert_eq!(days_since_epoch("2000-03-01"), Some(11_017));
        assert_eq!(days_since_epoch("2026-11-23"), Some(20_780));
        assert_eq!(days_since_epoch("bad"), None);
    }

    #[test]
    fn ttl_policy() {
        let now = 20_780 * DAY_MS; // 2026-11-23
        assert_eq!(stream_ttl_ms(true, None, now), 3 * DAY_MS);
        assert_eq!(stream_ttl_ms(false, Some("2026-11-10"), now), DAY_MS);
        assert_eq!(stream_ttl_ms(false, Some("2014-11-07"), now), 7 * DAY_MS);
        assert_eq!(stream_ttl_ms(false, None, now), 7 * DAY_MS);
    }
}
