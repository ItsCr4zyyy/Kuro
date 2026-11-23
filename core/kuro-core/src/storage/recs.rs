//! Uložené řady doporučení a pomocné dotazy pro jejich výpočet.

use std::collections::HashSet;

use rusqlite::params;

use super::catalog::{title_from_row, StoredTitle, TITLE_COLS};
use super::Db;
use crate::error::Result;

impl Db {
    /// Tituly, které nedoporučovat: rozkoukané/zhlédnuté, oblíbené, stažené.
    pub fn rec_excluded_ids(&self) -> Result<HashSet<i64>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT title_id FROM progress UNION SELECT title_id FROM watchlist UNION SELECT title_id FROM download",
            )?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            Ok(rows.collect::<rusqlite::Result<HashSet<i64>>>()?)
        })
    }

    /// Nahradí řadu doporučení (pořadí = pořadí v řadě).
    pub fn save_rec_row(&self, row_id: &str, items: &[(i64, f64)]) -> Result<()> {
        let now = self.now();
        self.with(|c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM rec_row WHERE row_id = ?1", [row_id])?;
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO rec_row (row_id, position, title_id, score, computed_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                )?;
                for (pos, (id, score)) in items.iter().enumerate() {
                    stmt.execute(params![row_id, pos as i64, id, score, now])?;
                }
            }
            // Prázdný výpočet si taky pamatuje čas (ať se nepočítá pořád dokola).
            tx.execute(
                "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![format!("rec_at:{row_id}"), now.to_string()],
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Řada doporučení a kdy se počítala (None = nikdy). Zhlédnuté
    /// a oblíbené od výpočtu se vynechají.
    pub fn rec_row(&self, row_id: &str) -> Result<(Vec<StoredTitle>, Option<i64>)> {
        let now = self.now();
        let computed_at = self.kv_get(&format!("rec_at:{row_id}"))?.and_then(|v| v.parse().ok());
        let excluded = self.rec_excluded_ids()?;
        let cols = TITLE_COLS.split(", ").map(|c| format!("t.{c}")).collect::<Vec<_>>().join(", ");
        let items = self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {cols} FROM rec_row r JOIN title t ON t.id = r.title_id WHERE r.row_id = ?1 ORDER BY r.position"
            ))?;
            let rows = stmt.query_map([row_id], |r| title_from_row(r, now))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        Ok((items.into_iter().filter(|t| !excluded.contains(&t.id)).collect(), computed_at))
    }

    /// Seriály, které uživatel sleduje (pozice za posledních 90 dní) —
    /// u nich se denně hlídají nové díly.
    pub fn series_in_progress(&self, limit: u32) -> Result<Vec<StoredTitle>> {
        let now = self.now();
        let cols = TITLE_COLS.split(", ").map(|c| format!("t.{c}")).collect::<Vec<_>>().join(", ");
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {cols} FROM title t
                 WHERE t.kind = 'series'
                   AND EXISTS (SELECT 1 FROM progress p WHERE p.title_id = t.id AND p.updated_at > ?1)
                 ORDER BY (SELECT MAX(updated_at) FROM progress p WHERE p.title_id = t.id) DESC
                 LIMIT ?2"
            ))?;
            let rows = stmt.query_map(params![now - 90 * super::DAY_MS, limit], |r| title_from_row(r, now))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}
