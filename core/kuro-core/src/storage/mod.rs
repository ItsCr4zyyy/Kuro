//! Lokální úložiště (SQLite). Všechno zůstává v zařízení.
//!
//! Princip: databáze je malá a plní se jen tím, na co uživatel sáhne.
//! Zájem o titul i o vlastnosti (žánr, dekáda, …) se drží jako
//! slábnoucí skóre: `score * 0.5^(stáří / poločas) + váha nové akce`.
//! Jedno číslo a jeden čas na řádek, žádná historie událostí.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::{Error, Result};

mod catalog;
mod downloads;
mod library;
mod prune;
mod streams;

pub use catalog::{Event, StoredTitle};
pub use downloads::{Download, DownloadState, DownloadWithTitle};
pub use library::{ContinueItem, Progress};
pub use prune::{Budget, PruneReport};
pub use streams::{CachedStreams, Target};

const SCHEMA: &str = include_str!("schema.sql");
const SCHEMA_VERSION: i32 = 1;

pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Poločas zájmu: po 30 dnech bez kontaktu má titul/vlastnost poloviční skóre.
pub const HALF_LIFE_MS: i64 = 30 * DAY_MS;

/// Skóre přepočtené na okamžik `now`.
pub fn decayed(score: f64, at: i64, now: i64) -> f64 {
    let age = (now - at).max(0) as f64;
    score * 0.5f64.powf(age / HALF_LIFE_MS as f64)
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NotFound,
            other => Error::Db(other.to_string()),
        }
    }
}

type Clock = Box<dyn Fn() -> i64 + Send + Sync>;

pub struct Db {
    conn: Mutex<Connection>,
    clock: Clock,
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::init(Connection::open(path)?, Box::new(crate::time::now_ms))
    }

    /// Databáze v paměti s vlastními hodinami — pro testy.
    pub fn in_memory_with_clock(clock: impl Fn() -> i64 + Send + Sync + 'static) -> Result<Self> {
        Self::init(Connection::open_in_memory()?, Box::new(clock))
    }

    fn init(conn: Connection, clock: Clock) -> Result<Self> {
        // auto_vacuum se musí nastavit před vytvořením tabulek; na existující
        // databázi nic neudělá, takže je bezpečné ho posílat vždycky.
        conn.execute_batch(
            "PRAGMA auto_vacuum = INCREMENTAL;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )?;

        let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        match version {
            0 => {
                conn.execute_batch(SCHEMA)?;
                conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            }
            SCHEMA_VERSION => {}
            v if v > SCHEMA_VERSION => {
                return Err(Error::Db(format!("databáze je z novější verze aplikace (schéma {v})")));
            }
            // Budoucí migrace: postupně v -> v+1.
            v => return Err(Error::Db(format!("chybí migrace ze schématu {v}"))),
        }

        let db = Self { conn: Mutex::new(conn), clock };
        db.invalidate_streams_on_matcher_change()?;
        Ok(db)
    }

    /// Po změně pravidel párování zahodit uložené zdroje (ne statistiky
    /// přehrávání — ty patří ke konkrétním souborům a platí dál).
    fn invalidate_streams_on_matcher_change(&self) -> Result<()> {
        let want = crate::matcher::MATCHER_VERSION.to_string();
        if self.kv_get("matcher_version")?.as_deref() == Some(want.as_str()) {
            return Ok(());
        }
        self.with(|c| {
            c.execute_batch("DELETE FROM stream; DELETE FROM stream_search;")?;
            Ok(())
        })?;
        self.kv_set("matcher_version", &want)
    }

    pub(crate) fn now(&self) -> i64 {
        (self.clock)()
    }

    pub(crate) fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut conn = self.conn.lock().map_err(|_| Error::Db("zámek databáze je otrávený".into()))?;
        f(&mut conn)
    }

    /// Hodnota z key-value tabulky (nastavení core, kurzory, …).
    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        self.with(|c| {
            Ok(c.query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0))
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })?)
        })
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO kv (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                [key, value],
            )?;
            Ok(())
        })
    }
}

/* ------------------------------ JSON pomocníci ------------------------------ */

pub(crate) fn to_json(v: &[String]) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "[]".into())
}

pub(crate) fn from_json(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decay_halves_after_half_life() {
        assert!((decayed(4.0, 0, HALF_LIFE_MS) - 2.0).abs() < 1e-9);
        assert!((decayed(4.0, 0, 2 * HALF_LIFE_MS) - 1.0).abs() < 1e-9);
        assert_eq!(decayed(4.0, 100, 50), 4.0, "budoucí čas neroste");
    }

    #[test]
    fn opens_and_migrates() {
        let db = Db::in_memory_with_clock(|| 0).unwrap();
        db.kv_set("a", "1").unwrap();
        db.kv_set("a", "2").unwrap();
        assert_eq!(db.kv_get("a").unwrap().as_deref(), Some("2"));
        assert_eq!(db.kv_get("b").unwrap(), None);
    }
}
