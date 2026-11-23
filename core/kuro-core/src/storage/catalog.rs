//! Tituly, epizody, události zájmu a profil vkusu.

use rusqlite::{params, OptionalExtension, Row, Transaction};

use super::{decayed, from_json, to_json, Db};
use crate::error::Result;
use crate::matcher::MediaKind;
use crate::meta::{EpisodeMeta, LocalizedTitles, TitleMeta};
use crate::text::normalize_title;

/// Co uživatel s titulem udělal. Váhy určují, jak moc to zvedne zájem
/// o titul a o jeho vlastnosti (žánr, dekádu, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// Viděl v seznamu. Titul zůstává v krátkodobé vrstvě.
    Impression,
    /// Klikl na výsledek hledání.
    SearchClick,
    /// Otevřel detail.
    Opened,
    /// Spustil přehrávání.
    PlayStarted,
    /// Dokoukal (> 90 %).
    Finished,
    /// Přidal do oblíbených.
    Watchlisted,
}

impl Event {
    fn title_weight(self) -> f64 {
        match self {
            Event::Impression => 0.1,
            Event::SearchClick => 1.5,
            Event::Opened => 1.0,
            Event::PlayStarted => 3.0,
            Event::Finished => 5.0,
            Event::Watchlisted => 4.0,
        }
    }

    /// Jak moc událost vypovídá o vkusu. Zobrazení v seznamu nic.
    fn taste_weight(self) -> f64 {
        match self {
            Event::Impression => 0.0,
            Event::SearchClick | Event::Opened => 0.3,
            Event::PlayStarted => 1.0,
            Event::Finished => 2.0,
            Event::Watchlisted => 1.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredTitle {
    pub id: i64,
    pub meta: TitleMeta,
    /// Kdy se stáhl plný detail s epizodami; None = známe jen kartu.
    pub detail_at: Option<i64>,
    /// Kdy se ptalo Wikidat na český název; None = nikdy.
    pub localized_at: Option<i64>,
    pub tier: u8,
    /// Aktuální (slábnoucí) skóre zájmu.
    pub score: f64,
}

pub(crate) fn kind_str(k: MediaKind) -> &'static str {
    match k {
        MediaKind::Movie => "movie",
        MediaKind::Series => "series",
    }
}

fn kind_from(s: &str) -> MediaKind {
    if s == "series" { MediaKind::Series } else { MediaKind::Movie }
}

pub(crate) const TITLE_COLS: &str = "id, kind, imdb_id, title_orig, title_cs, aliases, year, overview, poster, \
     background, logo, runtime_min, rating, genres, countries, anime, release_date, popularity, \
     detail_at, localized_at, tier, score, score_at";

pub(crate) fn title_from_row(r: &Row<'_>, now: i64) -> rusqlite::Result<StoredTitle> {
    Ok(StoredTitle {
        id: r.get(0)?,
        meta: TitleMeta {
            kind: kind_from(&r.get::<_, String>(1)?),
            imdb_id: r.get(2)?,
            title_orig: r.get(3)?,
            title_cs: r.get(4)?,
            aliases: from_json(&r.get::<_, String>(5)?),
            year: r.get(6)?,
            overview: r.get(7)?,
            poster: r.get(8)?,
            background: r.get(9)?,
            logo: r.get(10)?,
            runtime_min: r.get(11)?,
            rating: r.get(12)?,
            genres: from_json(&r.get::<_, String>(13)?),
            countries: from_json(&r.get::<_, String>(14)?),
            anime: r.get(15)?,
            release_date: r.get(16)?,
            popularity: r.get(17)?,
            episodes: vec![],
        },
        detail_at: r.get(18)?,
        localized_at: r.get(19)?,
        tier: r.get(20)?,
        score: decayed(r.get(21)?, r.get(22)?, now),
    })
}

/// Vložení/aktualizace titulu. Pole, která nový zdroj nezná (český název,
/// aliasy), se nepřepisují prázdnem. Vrací id.
fn upsert(tx: &Transaction<'_>, m: &TitleMeta, now: i64) -> Result<i64> {
    // Aliasy se slučují (Wikidata, AniList i Cinemeta je dodávají zvlášť).
    let stored: Option<String> = tx
        .query_row("SELECT aliases FROM title WHERE kind = ?1 AND imdb_id = ?2", params![kind_str(m.kind), m.imdb_id], |r| r.get(0))
        .optional()?;
    let aliases = merge_aliases(stored.as_deref().map(from_json).unwrap_or_default(), &m.aliases, &m.title_orig);
    tx.execute(
        "INSERT INTO title (kind, imdb_id, title_orig, title_cs, aliases, year, overview, poster,
             background, logo, runtime_min, rating, genres, countries, anime, release_date,
             popularity, seen_at, score_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?18)
         ON CONFLICT (kind, imdb_id) DO UPDATE SET
             title_orig   = excluded.title_orig,
             title_cs     = COALESCE(excluded.title_cs, title.title_cs),
             aliases      = excluded.aliases,
             year         = COALESCE(excluded.year, title.year),
             overview     = COALESCE(excluded.overview, title.overview),
             poster       = COALESCE(excluded.poster, title.poster),
             background   = COALESCE(excluded.background, title.background),
             logo         = COALESCE(excluded.logo, title.logo),
             runtime_min  = COALESCE(excluded.runtime_min, title.runtime_min),
             rating       = COALESCE(excluded.rating, title.rating),
             genres       = CASE WHEN excluded.genres = '[]' THEN title.genres ELSE excluded.genres END,
             countries    = CASE WHEN excluded.countries = '[]' THEN title.countries ELSE excluded.countries END,
             anime        = MAX(excluded.anime, title.anime),
             release_date = COALESCE(excluded.release_date, title.release_date),
             popularity   = MAX(excluded.popularity, title.popularity),
             seen_at      = excluded.seen_at",
        params![
            kind_str(m.kind),
            m.imdb_id,
            m.title_orig,
            m.title_cs,
            to_json(&aliases),
            m.year,
            m.overview,
            m.poster,
            m.background,
            m.logo,
            m.runtime_min,
            m.rating,
            to_json(&m.genres),
            to_json(&m.countries),
            m.anime,
            m.release_date,
            m.popularity,
            now,
        ],
    )?;
    Ok(tx.query_row(
        "SELECT id FROM title WHERE kind = ?1 AND imdb_id = ?2",
        params![kind_str(m.kind), m.imdb_id],
        |r| r.get(0),
    )?)
}

/// Přidá nové aliasy (bez duplicit po normalizaci a bez hlavního názvu).
fn merge_aliases(mut list: Vec<String>, new: &[String], title_orig: &str) -> Vec<String> {
    for a in new {
        let n = normalize_title(a);
        if n != normalize_title(title_orig) && !list.iter().any(|x| normalize_title(x) == n) {
            list.push(a.clone());
        }
    }
    list
}

impl Db {
    /// Doplní aliasy k titulu (např. romaji z AniListu).
    pub fn add_aliases(&self, title_id: i64, new: &[String]) -> Result<()> {
        self.with(|c| {
            let (orig, aliases): (String, String) =
                c.query_row("SELECT title_orig, aliases FROM title WHERE id = ?1", [title_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let merged = merge_aliases(from_json(&aliases), new, &orig);
            c.execute("UPDATE title SET aliases = ?2 WHERE id = ?1", params![title_id, to_json(&merged)])?;
            Ok(())
        })
    }

    /// Uloží karty ze seznamu/hledání (krátkodobá vrstva). Vrací id ve stejném pořadí.
    pub fn upsert_cards(&self, metas: &[TitleMeta]) -> Result<Vec<i64>> {
        let now = self.now();
        self.with(|c| {
            let tx = c.transaction()?;
            let ids = metas.iter().map(|m| upsert(&tx, m, now)).collect::<Result<Vec<_>>>()?;
            tx.commit()?;
            Ok(ids)
        })
    }

    /// Uloží plný detail včetně epizod.
    pub fn upsert_detail(&self, meta: &TitleMeta) -> Result<i64> {
        let now = self.now();
        self.with(|c| {
            let tx = c.transaction()?;
            let id = upsert(&tx, meta, now)?;
            tx.execute("UPDATE title SET detail_at = ?2 WHERE id = ?1", params![id, now])?;
            if !meta.episodes.is_empty() {
                let mut stmt = tx.prepare(
                    "INSERT INTO episode (title_id, season, episode, name, overview, still, air_date)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT (title_id, season, episode) DO UPDATE SET
                         name = excluded.name, overview = excluded.overview,
                         still = excluded.still, air_date = excluded.air_date",
                )?;
                for e in &meta.episodes {
                    stmt.execute(params![id, e.season, e.episode, e.name, e.overview, e.still, e.air_date])?;
                }
            }
            tx.commit()?;
            Ok(id)
        })
    }

    /// Zapíše výsledek dotazu na Wikidata (i prázdný, ať se neptáme pořád).
    pub fn apply_localized(&self, title_id: i64, loc: Option<&LocalizedTitles>) -> Result<()> {
        let now = self.now();
        self.with(|c| {
            let tx = c.transaction()?;
            let (orig, mut aliases): (String, String) = tx.query_row(
                "SELECT title_orig, aliases FROM title WHERE id = ?1",
                [title_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let mut cs = None;
            if let Some(loc) = loc {
                cs = loc.cs.clone();
                let mut list = from_json(&aliases);
                for a in loc.aliases(&orig) {
                    if !list.iter().any(|x| normalize_title(x) == normalize_title(&a)) {
                        list.push(a);
                    }
                }
                aliases = to_json(&list);
            }
            tx.execute(
                "UPDATE title SET title_cs = COALESCE(?2, title_cs), aliases = ?3, localized_at = ?4 WHERE id = ?1",
                params![title_id, cs, aliases, now],
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn title(&self, id: i64) -> Result<Option<StoredTitle>> {
        let now = self.now();
        self.with(|c| {
            Ok(c.query_row(&format!("SELECT {TITLE_COLS} FROM title WHERE id = ?1"), [id], |r| {
                title_from_row(r, now)
            })
            .optional()?)
        })
    }

    pub fn title_by_imdb(&self, kind: MediaKind, imdb_id: &str) -> Result<Option<StoredTitle>> {
        let now = self.now();
        self.with(|c| {
            Ok(c.query_row(
                &format!("SELECT {TITLE_COLS} FROM title WHERE kind = ?1 AND imdb_id = ?2"),
                params![kind_str(kind), imdb_id],
                |r| title_from_row(r, now),
            )
            .optional()?)
        })
    }

    pub fn episodes(&self, title_id: i64) -> Result<Vec<EpisodeMeta>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT season, episode, name, overview, still, air_date FROM episode
                 WHERE title_id = ?1 ORDER BY season, episode",
            )?;
            let rows = stmt.query_map([title_id], |r| {
                Ok(EpisodeMeta {
                    season: r.get(0)?,
                    episode: r.get(1)?,
                    name: r.get(2)?,
                    overview: r.get(3)?,
                    still: r.get(4)?,
                    air_date: r.get(5)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Zapíše zájem o titul: zvedne skóre titulu, povýší ho z krátkodobé
    /// vrstvy a posílí vlastnosti v profilu vkusu.
    pub fn record_event(&self, title_id: i64, event: Event) -> Result<()> {
        let now = self.now();
        self.with(|c| {
            let tx = c.transaction()?;
            let (score, at, genres, countries, year, kind, anime): (f64, i64, String, String, Option<u16>, String, bool) =
                tx.query_row(
                    "SELECT score, score_at, genres, countries, year, kind, anime FROM title WHERE id = ?1",
                    [title_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
                )?;

            let new_score = decayed(score, at, now) + event.title_weight();
            let promote = event != Event::Impression;
            tx.execute(
                "UPDATE title SET score = ?2, score_at = ?3, seen_at = ?3,
                     tier = CASE WHEN ?4 THEN MAX(tier, 1) ELSE tier END
                 WHERE id = ?1",
                params![title_id, new_score, now, promote],
            )?;

            let w = event.taste_weight();
            if w > 0.0 {
                let mut features: Vec<String> = from_json(&genres).into_iter().map(|g| format!("genre:{g}")).collect();
                features.extend(from_json(&countries).into_iter().take(2).map(|c| format!("country:{c}")));
                if let Some(y) = year {
                    features.push(format!("decade:{}", y / 10 * 10));
                }
                features.push(format!("kind:{kind}"));
                if anime {
                    features.push("kind:anime".into());
                }
                for f in features {
                    bump_taste(&tx, &f, w, now)?;
                }
            }
            tx.commit()?;
            Ok(())
        })
    }

    /// Posílí vlastnost profilu přímo — např. "lang:cs-dub" podle zdroje,
    /// který si uživatel pustil.
    pub fn record_taste(&self, feature: &str, weight: f64) -> Result<()> {
        let now = self.now();
        self.with(|c| {
            let tx = c.transaction()?;
            bump_taste(&tx, feature, weight, now)?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Nejsilnější vlastnosti profilu (aktuální skóre), sestupně.
    pub fn taste(&self, prefix: &str, limit: usize) -> Result<Vec<(String, f64)>> {
        let now = self.now();
        self.with(|c| {
            let mut stmt = c.prepare("SELECT feature, score, score_at FROM taste WHERE feature LIKE ?1 || '%'")?;
            let mut all: Vec<(String, f64)> = stmt
                .query_map([prefix], |r| Ok((r.get::<_, String>(0)?, decayed(r.get(1)?, r.get(2)?, now))))?
                .collect::<rusqlite::Result<_>>()?;
            all.sort_by(|a, b| b.1.total_cmp(&a.1));
            all.truncate(limit);
            Ok(all)
        })
    }

    /// Fulltext v uložených titulech — okamžité napovídání při psaní,
    /// než odpoví Cinemeta. Diakritika je jedno, poslední slovo je prefix.
    pub fn search_local(&self, query: &str, kind: Option<MediaKind>, limit: u32) -> Result<Vec<StoredTitle>> {
        let tokens: Vec<String> = normalize_title(query).split(' ').filter(|t| !t.is_empty()).map(str::to_owned).collect();
        if tokens.is_empty() {
            return Ok(vec![]);
        }
        // Tokeny jsou po normalizaci jen [a-z0-9], takže uvozovky stačí.
        let fts = tokens.iter().map(|t| format!("\"{t}\"*")).collect::<Vec<_>>().join(" ");
        let now = self.now();
        self.with(|c| {
            let cols = TITLE_COLS.split(", ").map(|col| format!("t.{col}")).collect::<Vec<_>>().join(", ");
            let mut stmt = c.prepare(&format!(
                "SELECT {cols} FROM title_fts f JOIN title t ON t.id = f.rowid
                 WHERE title_fts MATCH ?1 AND (?2 IS NULL OR t.kind = ?2)
                 ORDER BY bm25(title_fts) - t.score * 0.5 - t.popularity * 0.01
                 LIMIT ?3"
            ))?;
            let rows = stmt.query_map(params![fts, kind.map(kind_str), limit], |r| title_from_row(r, now))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }
}

fn bump_taste(tx: &Transaction<'_>, feature: &str, weight: f64, now: i64) -> Result<()> {
    let old: Option<(f64, i64)> = tx
        .query_row("SELECT score, score_at FROM taste WHERE feature = ?1", [feature], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let score = old.map_or(0.0, |(s, at)| decayed(s, at, now)) + weight;
    tx.execute(
        "INSERT INTO taste (feature, score, score_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (feature) DO UPDATE SET score = excluded.score, score_at = excluded.score_at",
        params![feature, score, now],
    )?;
    Ok(())
}
