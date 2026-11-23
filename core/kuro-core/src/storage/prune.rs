//! Udržování databáze malé.
//!
//! Běžný úklid (denně na pozadí) i tlačítko "Pročistit paměť" v nastavení.
//! Připnuté tituly (oblíbené, rozkoukané, stažené) se nemažou nikdy.

use rusqlite::params;

use super::{decayed, Db, DAY_MS};
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    /// Kolik titulů se smí držet (připnuté se nepočítají do mazání).
    pub max_titles: u32,
    /// Jak dlouho žije titul, který byl jen vidět v seznamu.
    pub ephemeral_ttl_ms: i64,
}

impl Default for Budget {
    fn default() -> Self {
        Self { max_titles: 3000, ephemeral_ttl_ms: 3 * DAY_MS }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PruneReport {
    /// Krátkodobé tituly po TTL.
    pub expired: u32,
    /// Tituly smazané kvůli limitu / pročištění.
    pub evicted: u32,
    pub titles_left: u32,
}

/// Při "Pročistit paměť" se smažou i pracovní tituly, jejichž zájem
/// zeslábl pod tuhle hranici (~jedno otevření před víc než měsícem).
const DEEP_CLEAN_MIN_SCORE: f64 = 0.5;

const PINNED: &str = "(EXISTS (SELECT 1 FROM watchlist w WHERE w.title_id = title.id)
     OR EXISTS (SELECT 1 FROM progress p WHERE p.title_id = title.id)
     OR EXISTS (SELECT 1 FROM download d WHERE d.title_id = title.id))";

impl Db {
    /// Běžný úklid: krátkodobé tituly po TTL, pak nejméně zajímavé nad limit.
    pub fn prune(&self, budget: Budget) -> Result<PruneReport> {
        self.prune_inner(budget, false)
    }

    /// "Pročistit paměť": jako `prune`, navíc smaže všechny krátkodobé
    /// a pracovní tituly se zeslabeným zájmem, i když je místa dost.
    pub fn deep_clean(&self, budget: Budget) -> Result<PruneReport> {
        self.prune_inner(budget, true)
    }

    fn prune_inner(&self, budget: Budget, deep: bool) -> Result<PruneReport> {
        let now = self.now();
        self.with(|c| {
            let tx = c.transaction()?;
            let mut report = PruneReport::default();

            // 1. Krátkodobá vrstva (při pročištění celá).
            let cutoff = if deep { i64::MAX } else { now - budget.ephemeral_ttl_ms };
            report.expired = tx.execute(
                &format!("DELETE FROM title WHERE tier = 0 AND seen_at < ?1 AND NOT {PINNED}"),
                [cutoff],
            )? as u32;

            // 2. Pracovní vrstva podle slábnoucího skóre. Mocnina se počítá
            // tady, ne v SQL — přibalené SQLite nemusí mít matematické funkce.
            let mut candidates: Vec<(i64, f64)> = {
                let mut stmt = tx.prepare(&format!("SELECT id, score, score_at FROM title WHERE NOT {PINNED}"))?;
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, decayed(r.get(1)?, r.get(2)?, now))))?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            candidates.sort_by(|a, b| a.1.total_cmp(&b.1));

            let total: i64 = tx.query_row("SELECT COUNT(*) FROM title", [], |r| r.get(0))?;
            let over = (total - i64::from(budget.max_titles)).max(0) as usize;
            let weak = if deep { candidates.iter().take_while(|(_, s)| *s < DEEP_CLEAN_MIN_SCORE).count() } else { 0 };
            let remove = over.max(weak).min(candidates.len());

            {
                let mut del = tx.prepare("DELETE FROM title WHERE id = ?1")?;
                for (id, _) in &candidates[..remove] {
                    del.execute(params![id])?;
                }
            }
            report.evicted = remove as u32;

            // 3. Statistiky zdrojů, které nikde nefigurují a půl roku se nepoužily.
            tx.execute(
                "DELETE FROM stream_stats WHERE last_at < ?1
                 AND NOT EXISTS (SELECT 1 FROM stream s WHERE s.provider = stream_stats.provider AND s.ident = stream_stats.ident)",
                [now - 180 * DAY_MS],
            )?;

            report.titles_left = tx.query_row("SELECT COUNT(*) FROM title", [], |r| r.get(0))?;
            tx.commit()?;

            // Vrátit uvolněné stránky souborovému systému.
            c.execute_batch("PRAGMA incremental_vacuum;")?;
            Ok(report)
        })
    }
}
