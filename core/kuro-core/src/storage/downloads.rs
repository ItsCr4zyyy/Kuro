//! Fronta stahování pro offline sledování. Samotné stahování dělá Android
//! (WorkManager); tady je jen stav, aby přežil restart a byl vidět v UI.
//! Titul se staženým souborem je "připnutý" — úklid ho nesmaže.

use rusqlite::{params, OptionalExtension, Row};

use super::catalog::{title_from_row, StoredTitle, TITLE_COLS};
use super::streams::Target;
use super::Db;
use crate::error::Result;
use crate::providers::ProviderId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadState {
    Queued,
    Running,
    Paused,
    Done,
    Failed,
}

impl DownloadState {
    fn as_str(self) -> &'static str {
        match self {
            DownloadState::Queued => "queued",
            DownloadState::Running => "running",
            DownloadState::Paused => "paused",
            DownloadState::Done => "done",
            DownloadState::Failed => "failed",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "running" => DownloadState::Running,
            "paused" => DownloadState::Paused,
            "done" => DownloadState::Done,
            "failed" => DownloadState::Failed,
            _ => DownloadState::Queued,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Download {
    pub id: i64,
    pub target: Target,
    pub provider: Option<ProviderId>,
    pub ident: String,
    pub filename: String,
    pub local_path: Option<String>,
    pub state: DownloadState,
    pub bytes_done: i64,
    pub bytes_total: i64,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadWithTitle {
    pub download: Download,
    pub title: StoredTitle,
}

const COLS: &str = "d.id, d.title_id, d.season, d.episode, d.provider, d.ident, d.filename, d.local_path, d.state, \
     d.bytes_done, d.bytes_total, d.error, d.created_at, d.updated_at";

fn from_row(r: &Row<'_>, off: usize) -> rusqlite::Result<Download> {
    Ok(Download {
        id: r.get(off)?,
        target: Target { title_id: r.get(off + 1)?, season: r.get(off + 2)?, episode: r.get(off + 3)? },
        provider: ProviderId::parse(&r.get::<_, String>(off + 4)?),
        ident: r.get(off + 5)?,
        filename: r.get(off + 6)?,
        local_path: r.get(off + 7)?,
        state: DownloadState::parse(&r.get::<_, String>(off + 8)?),
        bytes_done: r.get(off + 9)?,
        bytes_total: r.get(off + 10)?,
        error: r.get(off + 11)?,
        created_at: r.get(off + 12)?,
        updated_at: r.get(off + 13)?,
    })
}

impl Db {
    /// Zařadí stažení. Pro jeden cíl drží jediné stažení — nový zdroj
    /// nahradí rozpracované/selhané (staženého souboru se nedotkne, vrátí
    /// jeho id). Vrací (id, cestu ke starému souboru ke smazání).
    pub fn enqueue_download(
        &self,
        target: Target,
        provider: ProviderId,
        ident: &str,
        filename: &str,
        size_bytes: i64,
    ) -> Result<(i64, Option<String>)> {
        let now = self.now();
        self.with(|c| {
            let tx = c.transaction()?;
            let existing: Option<(i64, String, String, Option<String>)> = tx
                .query_row(
                    "SELECT id, ident, state, local_path FROM download WHERE title_id = ?1 AND season = ?2 AND episode = ?3",
                    params![target.title_id, target.season, target.episode],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()?;

            let mut stale_file = None;
            if let Some((id, old_ident, state, path)) = existing {
                if old_ident == ident || state == "done" {
                    // Stejný zdroj nebo už stažené: jen znovu rozběhnout, pokud selhalo.
                    tx.execute(
                        "UPDATE download SET state = CASE WHEN state IN ('failed', 'paused') THEN 'queued' ELSE state END,
                             error = NULL, updated_at = ?2 WHERE id = ?1",
                        params![id, now],
                    )?;
                    tx.commit()?;
                    return Ok((id, None));
                }
                stale_file = path;
                tx.execute("DELETE FROM download WHERE id = ?1", [id])?;
            }

            tx.execute(
                "INSERT INTO download (title_id, season, episode, provider, ident, filename, state, bytes_total, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'queued', ?7, ?8, ?8)",
                params![target.title_id, target.season, target.episode, provider.as_str(), ident, filename, size_bytes, now],
            )?;
            let id = tx.last_insert_rowid();
            tx.commit()?;
            Ok((id, stale_file))
        })
    }

    pub fn download(&self, id: i64) -> Result<Option<Download>> {
        self.with(|c| {
            Ok(c.query_row(&format!("SELECT {COLS} FROM download d WHERE d.id = ?1"), [id], |r| from_row(r, 0))
                .optional()?)
        })
    }

    /// Stažení k filmu/epizodě (jakýkoli stav), pokud existuje.
    pub fn download_for(&self, target: Target) -> Result<Option<Download>> {
        self.with(|c| {
            Ok(c.query_row(
                &format!("SELECT {COLS} FROM download d WHERE d.title_id = ?1 AND d.season = ?2 AND d.episode = ?3"),
                params![target.title_id, target.season, target.episode],
                |r| from_row(r, 0),
            )
            .optional()?)
        })
    }

    /// Všechna stažení s titulem, nejnovější první.
    pub fn downloads(&self) -> Result<Vec<DownloadWithTitle>> {
        let now = self.now();
        let tcols = TITLE_COLS.split(", ").map(|c| format!("t.{c}")).collect::<Vec<_>>().join(", ");
        let n = TITLE_COLS.split(", ").count();
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {tcols}, {COLS} FROM download d JOIN title t ON t.id = d.title_id
                 ORDER BY d.created_at DESC, d.season, d.episode"
            ))?;
            let rows = stmt.query_map([], |r| {
                Ok(DownloadWithTitle { title: title_from_row(r, now)?, download: from_row(r, n)? })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Průběh z workeru. `None` u volitelných polí = beze změny.
    pub fn update_download(
        &self,
        id: i64,
        state: DownloadState,
        bytes_done: i64,
        bytes_total: Option<i64>,
        local_path: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let now = self.now();
        self.with(|c| {
            c.execute(
                "UPDATE download SET state = ?2, bytes_done = ?3,
                     bytes_total = COALESCE(?4, bytes_total),
                     local_path = COALESCE(?5, local_path),
                     error = ?6, updated_at = ?7
                 WHERE id = ?1",
                params![id, state.as_str(), bytes_done, bytes_total, local_path, error, now],
            )?;
            Ok(())
        })
    }

    /// Smaže záznam; vrátí cestu k souboru, který má volající smazat.
    pub fn delete_download(&self, id: i64) -> Result<Option<String>> {
        self.with(|c| {
            let path: Option<String> = c
                .query_row("SELECT local_path FROM download WHERE id = ?1", [id], |r| r.get(0))
                .optional()?
                .flatten();
            c.execute("DELETE FROM download WHERE id = ?1", [id])?;
            Ok(path)
        })
    }

    /// Stažení, která mají běžet (po restartu aplikace / telefonu).
    pub fn pending_downloads(&self) -> Result<Vec<Download>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {COLS} FROM download d WHERE d.state IN ('queued', 'running') ORDER BY d.created_at"
            ))?;
            let rows = stmt.query_map([], |r| from_row(r, 0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}
