//! Knihovna uživatele: oblíbené, rozkoukané pozice (= historie), nedávná hledání.

use rusqlite::{params, OptionalExtension};

use super::catalog::{title_from_row, StoredTitle, TITLE_COLS};
use super::streams::Target;
use super::Db;
use crate::error::Result;
use crate::providers::ProviderId;

/// Od kolika procent se titul/epizoda bere jako dokoukaná.
pub const FINISHED_RATIO: f64 = 0.9;

/// Kolik nedávných hledání se pamatuje.
const RECENT_SEARCHES: i64 = 50;

#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub target: Target,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub finished: bool,
    /// Zdroj, ze kterého se přehrávalo — pro navázání stejným souborem.
    pub provider: Option<ProviderId>,
    pub ident: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContinueItem {
    pub title: StoredTitle,
    pub progress: Progress,
}

fn progress_from_row(r: &rusqlite::Row<'_>, off: usize) -> rusqlite::Result<Progress> {
    Ok(Progress {
        target: Target { title_id: r.get(off)?, season: r.get(off + 1)?, episode: r.get(off + 2)? },
        position_ms: r.get(off + 3)?,
        duration_ms: r.get(off + 4)?,
        finished: r.get(off + 5)?,
        provider: r.get::<_, Option<String>>(off + 6)?.as_deref().and_then(ProviderId::parse),
        ident: r.get(off + 7)?,
        updated_at: r.get(off + 8)?,
    })
}

const PROGRESS_COLS: &str = "p.title_id, p.season, p.episode, p.position_ms, p.duration_ms, p.finished, p.provider, p.ident, p.updated_at";

impl Db {
    /* ------------------------------ oblíbené ------------------------------ */

    pub fn set_watchlist(&self, title_id: i64, on: bool) -> Result<()> {
        let now = self.now();
        self.with(|c| {
            if on {
                c.execute("INSERT OR IGNORE INTO watchlist (title_id, added_at) VALUES (?1, ?2)", params![title_id, now])?;
            } else {
                c.execute("DELETE FROM watchlist WHERE title_id = ?1", [title_id])?;
            }
            Ok(())
        })?;
        if on {
            self.record_event(title_id, super::Event::Watchlisted)?;
        }
        Ok(())
    }

    pub fn in_watchlist(&self, title_id: i64) -> Result<bool> {
        self.with(|c| {
            Ok(c.query_row("SELECT 1 FROM watchlist WHERE title_id = ?1", [title_id], |_| Ok(()))
                .optional()?
                .is_some())
        })
    }

    pub fn watchlist(&self) -> Result<Vec<StoredTitle>> {
        let now = self.now();
        let cols = prefixed(TITLE_COLS, "t");
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {cols} FROM watchlist w JOIN title t ON t.id = w.title_id ORDER BY w.added_at DESC"
            ))?;
            let rows = stmt.query_map([], |r| title_from_row(r, now))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /* ------------------------------ pozice ------------------------------ */

    /// Uloží pozici přehrávání. Volá se průběžně (každých pár sekund)
    /// a při zavření přehrávače. Přechod na dokoukáno zapíše událost.
    pub fn save_progress(
        &self,
        target: Target,
        position_ms: i64,
        duration_ms: i64,
        source: Option<(ProviderId, &str)>,
    ) -> Result<()> {
        let now = self.now();
        let finished = duration_ms > 0 && position_ms as f64 >= duration_ms as f64 * FINISHED_RATIO;
        let was_finished = self.with(|c| {
            let was: Option<bool> = c
                .query_row(
                    "SELECT finished FROM progress WHERE title_id = ?1 AND season = ?2 AND episode = ?3",
                    params![target.title_id, target.season, target.episode],
                    |r| r.get(0),
                )
                .optional()?;
            c.execute(
                "INSERT INTO progress (title_id, season, episode, position_ms, duration_ms, finished, provider, ident, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT (title_id, season, episode) DO UPDATE SET
                     position_ms = excluded.position_ms, duration_ms = excluded.duration_ms,
                     finished = excluded.finished,
                     provider = COALESCE(excluded.provider, progress.provider),
                     ident = COALESCE(excluded.ident, progress.ident),
                     updated_at = excluded.updated_at",
                params![
                    target.title_id,
                    target.season,
                    target.episode,
                    position_ms,
                    duration_ms,
                    finished,
                    source.map(|(p, _)| p.as_str()),
                    source.map(|(_, i)| i),
                    now
                ],
            )?;
            Ok(was.unwrap_or(false))
        })?;
        if finished && !was_finished {
            self.record_event(target.title_id, super::Event::Finished)?;
        }
        Ok(())
    }

    pub fn progress(&self, target: Target) -> Result<Option<Progress>> {
        self.with(|c| {
            Ok(c.query_row(
                &format!("SELECT {PROGRESS_COLS} FROM progress p WHERE p.title_id = ?1 AND p.season = ?2 AND p.episode = ?3"),
                params![target.title_id, target.season, target.episode],
                |r| progress_from_row(r, 0),
            )
            .optional()?)
        })
    }

    /// Všechny pozice v titulu (u seriálu po epizodách) — pro odškrtnutí
    /// zhlédnutých epizod v detailu.
    pub fn title_progress(&self, title_id: i64) -> Result<Vec<Progress>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {PROGRESS_COLS} FROM progress p WHERE p.title_id = ?1 ORDER BY p.season, p.episode"
            ))?;
            let rows = stmt.query_map([title_id], |r| progress_from_row(r, 0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// "Pokračovat ve sledování": u každého titulu jen poslední pozice,
    /// rozkoukané (ne dokoukané) a s aspoň minutou zhlédnutou.
    pub fn continue_watching(&self, limit: u32) -> Result<Vec<ContinueItem>> {
        let now = self.now();
        let cols = prefixed(TITLE_COLS, "t");
        let n = TITLE_COLS.split(", ").count();
        self.with(|c| {
            // Poslední pozice každého titulu: rozkoukané (aspoň minuta),
            // nebo dokoukaný díl seriálu — pak se nabídne další díl.
            let mut stmt = c.prepare(&format!(
                "SELECT {cols}, {PROGRESS_COLS}
                 FROM progress p JOIN title t ON t.id = p.title_id
                 WHERE p.updated_at = (SELECT MAX(updated_at) FROM progress q WHERE q.title_id = p.title_id)
                   AND ((p.finished = 0 AND p.position_ms >= 60000) OR (p.finished = 1 AND p.episode > 0))
                 ORDER BY p.updated_at DESC"
            ))?;
            let candidates = stmt
                .query_map([], |r| Ok(ContinueItem { title: title_from_row(r, now)?, progress: progress_from_row(r, n)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            let mut next_ep = c.prepare(
                "SELECT e.season, e.episode FROM episode e
                 WHERE e.title_id = ?1 AND (e.season > ?2 OR (e.season = ?2 AND e.episode > ?3))
                   AND NOT EXISTS (SELECT 1 FROM progress p WHERE p.title_id = e.title_id
                                   AND p.season = e.season AND p.episode = e.episode AND p.finished = 1)
                 ORDER BY e.season, e.episode LIMIT 1",
            )?;
            let mut out = Vec::new();
            for item in candidates {
                if out.len() as u32 >= limit {
                    break;
                }
                if !item.progress.finished {
                    out.push(item);
                    continue;
                }
                // Dokoukaný díl → další díl (pokud ho katalog zná), od začátku.
                let t = item.progress.target;
                let next: Option<(u16, u16)> = next_ep
                    .query_row(params![t.title_id, t.season, t.episode], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()?;
                if let Some((season, episode)) = next {
                    out.push(ContinueItem {
                        progress: Progress {
                            target: Target { title_id: t.title_id, season, episode },
                            position_ms: 0,
                            duration_ms: 0,
                            finished: false,
                            provider: None,
                            ident: None,
                            updated_at: item.progress.updated_at,
                        },
                        title: item.title,
                    });
                }
            }
            Ok(out)
        })
    }

    /* ------------------------------ hledání ------------------------------ */

    pub fn add_recent_search(&self, query: &str) -> Result<()> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(());
        }
        let now = self.now();
        self.with(|c| {
            c.execute(
                "INSERT INTO search_recent (query, at) VALUES (?1, ?2)
                 ON CONFLICT (query) DO UPDATE SET at = excluded.at",
                params![q, now],
            )?;
            c.execute(
                "DELETE FROM search_recent WHERE query NOT IN
                     (SELECT query FROM search_recent ORDER BY at DESC LIMIT ?1)",
                [RECENT_SEARCHES],
            )?;
            Ok(())
        })
    }

    pub fn recent_searches(&self, limit: u32) -> Result<Vec<String>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT query FROM search_recent ORDER BY at DESC LIMIT ?1")?;
            let rows = stmt.query_map([limit], |r| r.get(0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    pub fn clear_recent_searches(&self) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM search_recent", [])?;
            Ok(())
        })
    }
}

fn prefixed(cols: &str, alias: &str) -> String {
    cols.split(", ").map(|c| format!("{alias}.{c}")).collect::<Vec<_>>().join(", ")
}
