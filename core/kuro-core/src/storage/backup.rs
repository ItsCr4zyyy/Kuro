//! Záloha a obnova knihovny (přechod na nový telefon) jako JSON soubor.
//!
//! Obsahuje: tituly, na které uživatel sáhl (oblíbené, rozkoukané,
//! otevírané), oblíbené, pozice přehrávání, vkus (doporučení) a poslední
//! hledání. Nikdy přihlašovací údaje ani odkazy na soubory.
//!
//! Tituly se párují podle (druh, IMDb ID), ne podle interních id.
//! Obnova jen doplňuje — nic nemaže, novější pozice vyhrává.

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{from_json, to_json, Db};
use crate::error::{Error, Result};

pub const BACKUP_FORMAT: &str = "kuro-backup";
const BACKUP_VERSION: u32 = 1;
/// Kolik "jen otevřených" titulů (bez oblíbených/pozic) přibalit — kvůli
/// doporučením; víc nemá smysl, stejně by časem vyprchaly.
const MAX_INTEREST_TITLES: u32 = 500;

#[derive(Debug, Serialize, Deserialize)]
pub struct Backup {
    pub format: String,
    pub version: u32,
    pub created_at: i64,
    #[serde(default)]
    pub titles: Vec<BackupTitle>,
    #[serde(default)]
    pub watchlist: Vec<BackupWatch>,
    #[serde(default)]
    pub progress: Vec<BackupProgress>,
    #[serde(default)]
    pub taste: Vec<BackupTaste>,
    #[serde(default)]
    pub searches: Vec<BackupSearch>,
    /// Nastavení aplikace (doplňuje Android vrstva; nikdy token).
    #[serde(default)]
    pub settings: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupTitle {
    pub kind: String,
    pub imdb_id: String,
    pub title_orig: String,
    #[serde(default)]
    pub title_cs: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub year: Option<u16>,
    #[serde(default)]
    pub poster: Option<String>,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub rating: Option<f32>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub anime: bool,
    #[serde(default)]
    pub tier: u8,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub score_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupWatch {
    pub kind: String,
    pub imdb_id: String,
    pub added_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupProgress {
    pub kind: String,
    pub imdb_id: String,
    pub season: u16,
    pub episode: u16,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub finished: bool,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupTaste {
    pub feature: String,
    pub score: f64,
    pub score_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupSearch {
    pub query: String,
    pub at: i64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub titles: u32,
    pub watchlist: u32,
    pub progress: u32,
}

fn valid_kind(k: &str) -> bool {
    k == "movie" || k == "series"
}

impl Db {
    /// Celá knihovna jako JSON.
    pub fn export_backup(&self, settings: HashMap<String, String>) -> Result<String> {
        let now = self.now();
        let backup = self.with(|c| {
            // Oblíbené, rozkoukané, stažené + nejzajímavější otevírané.
            let mut stmt = c.prepare(&format!(
                "SELECT kind, imdb_id, title_orig, title_cs, aliases, year, poster, background, rating, genres,
                        anime, tier, score, score_at
                 FROM title t
                 WHERE EXISTS (SELECT 1 FROM watchlist w WHERE w.title_id = t.id)
                    OR EXISTS (SELECT 1 FROM progress p WHERE p.title_id = t.id)
                    OR EXISTS (SELECT 1 FROM download d WHERE d.title_id = t.id)
                    OR t.id IN (SELECT id FROM title WHERE tier >= 1 ORDER BY score DESC LIMIT {MAX_INTEREST_TITLES})"
            ))?;
            let titles = stmt
                .query_map([], |r| {
                    Ok(BackupTitle {
                        kind: r.get(0)?,
                        imdb_id: r.get(1)?,
                        title_orig: r.get(2)?,
                        title_cs: r.get(3)?,
                        aliases: from_json(&r.get::<_, String>(4)?),
                        year: r.get(5)?,
                        poster: r.get(6)?,
                        background: r.get(7)?,
                        rating: r.get(8)?,
                        genres: from_json(&r.get::<_, String>(9)?),
                        anime: r.get(10)?,
                        tier: r.get(11)?,
                        score: r.get(12)?,
                        score_at: r.get(13)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            let watchlist = c
                .prepare("SELECT t.kind, t.imdb_id, w.added_at FROM watchlist w JOIN title t ON t.id = w.title_id")?
                .query_map([], |r| Ok(BackupWatch { kind: r.get(0)?, imdb_id: r.get(1)?, added_at: r.get(2)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            let progress = c
                .prepare(
                    "SELECT t.kind, t.imdb_id, p.season, p.episode, p.position_ms, p.duration_ms, p.finished, p.updated_at
                     FROM progress p JOIN title t ON t.id = p.title_id",
                )?
                .query_map([], |r| {
                    Ok(BackupProgress {
                        kind: r.get(0)?,
                        imdb_id: r.get(1)?,
                        season: r.get(2)?,
                        episode: r.get(3)?,
                        position_ms: r.get(4)?,
                        duration_ms: r.get(5)?,
                        finished: r.get(6)?,
                        updated_at: r.get(7)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            let taste = c
                .prepare("SELECT feature, score, score_at FROM taste")?
                .query_map([], |r| Ok(BackupTaste { feature: r.get(0)?, score: r.get(1)?, score_at: r.get(2)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            let searches = c
                .prepare("SELECT query, at FROM search_recent ORDER BY at DESC LIMIT 50")?
                .query_map([], |r| Ok(BackupSearch { query: r.get(0)?, at: r.get(1)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(Backup {
                format: BACKUP_FORMAT.into(),
                version: BACKUP_VERSION,
                created_at: now,
                titles,
                watchlist,
                progress,
                taste,
                searches,
                settings,
            })
        })?;
        serde_json::to_string(&backup).map_err(|e| Error::BadResponse(format!("záloha: {e}")))
    }

    /// Načte zálohu a doplní ji do knihovny. Vrací souhrn a nastavení ze zálohy.
    pub fn import_backup(&self, json: &str) -> Result<(RestoreReport, HashMap<String, String>)> {
        let b: Backup =
            serde_json::from_str(json).map_err(|e| Error::BadResponse(format!("tohle není záloha Kura ({e})")))?;
        if b.format != BACKUP_FORMAT {
            return Err(Error::BadResponse("tohle není záloha Kura".into()));
        }
        if b.version > BACKUP_VERSION {
            return Err(Error::BadResponse("záloha je z novější verze Kura — aktualizuj aplikaci".into()));
        }
        let now = self.now();
        let report = self.with(|c| {
            let tx = c.transaction()?;
            let mut report = RestoreReport::default();
            let mut ids: HashMap<(String, String), i64> = HashMap::new();

            for t in b.titles.iter().filter(|t| valid_kind(&t.kind) && t.imdb_id.starts_with("tt")) {
                let existing: Option<i64> = tx
                    .query_row("SELECT id FROM title WHERE kind = ?1 AND imdb_id = ?2", params![t.kind, t.imdb_id], |r| {
                        r.get(0)
                    })
                    .optional()?;
                let id = match existing {
                    Some(id) => {
                        // Zájem ze zálohy přidat, nic nepřepsat.
                        tx.execute(
                            "UPDATE title SET tier = MAX(tier, ?2),
                                 score = CASE WHEN ?3 > score THEN ?3 ELSE score END,
                                 score_at = CASE WHEN ?3 > score THEN ?4 ELSE score_at END,
                                 anime = MAX(anime, ?5)
                             WHERE id = ?1",
                            params![id, t.tier, t.score, t.score_at, t.anime],
                        )?;
                        id
                    }
                    None => {
                        // Detail (epizody, popis) se dotáhne při otevření.
                        tx.execute(
                            "INSERT INTO title (kind, imdb_id, title_orig, title_cs, aliases, year, poster, background,
                                 rating, genres, anime, tier, score, score_at, seen_at)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                            params![
                                t.kind,
                                t.imdb_id,
                                t.title_orig,
                                t.title_cs,
                                to_json(&t.aliases),
                                t.year,
                                t.poster,
                                t.background,
                                t.rating,
                                to_json(&t.genres),
                                t.anime,
                                t.tier,
                                t.score,
                                t.score_at,
                                now
                            ],
                        )?;
                        report.titles += 1;
                        tx.last_insert_rowid()
                    }
                };
                ids.insert((t.kind.clone(), t.imdb_id.clone()), id);
            }

            for w in &b.watchlist {
                let Some(id) = ids.get(&(w.kind.clone(), w.imdb_id.clone())) else { continue };
                report.watchlist +=
                    tx.execute("INSERT OR IGNORE INTO watchlist (title_id, added_at) VALUES (?1, ?2)", params![id, w.added_at])?
                        as u32;
            }

            for p in &b.progress {
                let Some(id) = ids.get(&(p.kind.clone(), p.imdb_id.clone())) else { continue };
                // Novější pozice vyhrává (záloha ze starého telefonu nepřepíše novou).
                report.progress += tx.execute(
                    "INSERT INTO progress (title_id, season, episode, position_ms, duration_ms, finished, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT (title_id, season, episode) DO UPDATE SET
                         position_ms = excluded.position_ms, duration_ms = excluded.duration_ms,
                         finished = excluded.finished, updated_at = excluded.updated_at
                     WHERE excluded.updated_at > progress.updated_at",
                    params![id, p.season, p.episode, p.position_ms, p.duration_ms, p.finished, p.updated_at],
                )? as u32;
            }

            for t in &b.taste {
                tx.execute(
                    "INSERT INTO taste (feature, score, score_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT (feature) DO UPDATE SET score = excluded.score, score_at = excluded.score_at
                     WHERE excluded.score_at > taste.score_at",
                    params![t.feature, t.score, t.score_at],
                )?;
            }

            for s in &b.searches {
                tx.execute("INSERT OR IGNORE INTO search_recent (query, at) VALUES (?1, ?2)", params![s.query, s.at])?;
            }

            tx.commit()?;
            Ok(report)
        })?;
        Ok((report, b.settings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::MediaKind;
    use crate::meta::TitleMeta;
    use crate::storage::Target;

    fn meta(id: &str) -> TitleMeta {
        TitleMeta {
            imdb_id: id.into(),
            kind: MediaKind::Series,
            title_orig: format!("Titul {id}"),
            title_cs: Some("Český".into()),
            aliases: vec!["Alias".into()],
            year: Some(2020),
            overview: None,
            poster: Some("https://p".into()),
            background: None,
            logo: None,
            runtime_min: None,
            rating: Some(8.0),
            genres: vec!["Drama".into()],
            countries: vec![],
            anime: true,
            release_date: None,
            popularity: 0.0,
            episodes: vec![],
        }
    }

    #[test]
    fn roundtrip_into_empty_db() {
        let a = Db::in_memory_with_clock(|| 1_000).unwrap();
        let ids = a.upsert_cards(&[meta("tt1"), meta("tt2")]).unwrap();
        a.set_watchlist(ids[0], true).unwrap();
        a.save_progress(Target::episode(ids[1], 2, 3), 600_000, 1_400_000, None).unwrap();
        let mut settings = HashMap::new();
        settings.insert("max_titles".to_string(), "3000".to_string());
        let json = a.export_backup(settings).unwrap();
        assert!(json.contains("kuro-backup"));

        let b = Db::in_memory_with_clock(|| 2_000).unwrap();
        let (r, s) = b.import_backup(&json).unwrap();
        assert_eq!(r, RestoreReport { titles: 2, watchlist: 1, progress: 1 });
        assert_eq!(s.get("max_titles").map(String::as_str), Some("3000"));
        let t = b.title_by_imdb(MediaKind::Series, "tt2").unwrap().unwrap();
        let p = b.progress(Target::episode(t.id, 2, 3)).unwrap().unwrap();
        assert_eq!(p.position_ms, 600_000);
        assert!(t.meta.anime);
        assert_eq!(t.meta.aliases, vec!["Alias"]);

        // Podruhé nic nového a starší pozice nepřepíše novější.
        b.save_progress(Target::episode(t.id, 2, 3), 900_000, 1_400_000, None).unwrap();
        let (r2, _) = b.import_backup(&json).unwrap();
        assert_eq!(r2, RestoreReport::default());
        assert_eq!(b.progress(Target::episode(t.id, 2, 3)).unwrap().unwrap().position_ms, 900_000);
    }

    #[test]
    fn rejects_foreign_json() {
        let db = Db::in_memory_with_clock(|| 0).unwrap();
        assert!(db.import_backup(r#"{"format":"vibe-backup","version":1,"created_at":0}"#).is_err());
        assert!(db.import_backup("nesmysl").is_err());
    }
}
