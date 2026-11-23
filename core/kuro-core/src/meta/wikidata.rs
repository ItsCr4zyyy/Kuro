//! Wikidata — české a slovenské názvy podle IMDb ID (NSC `indexer/wikidata.ts`).
//!
//! Cinemeta umí jen anglicky, ale české releasy se jmenují
//! `Pan.prstenu.Dve.veze.2002.1080p` — bez českého názvu je matcher nenajde.
//! Zdarma, bez klíče; ptáme se po dávkách a s ohleduplnou prodlevou.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use serde::Deserialize;

use crate::error::{Error, Result};
use crate::rx::rx;
use crate::text::normalize_title;

const ENDPOINT: &str = "https://query.wikidata.org/sparql";

/// Kolik IMDb ID se ptá najednou — pod limitem délky URL i času dotazu.
pub const BATCH_SIZE: usize = 50;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalizedTitles {
    pub imdb_id: String,
    pub cs: Option<String>,
    pub sk: Option<String>,
}

impl LocalizedTitles {
    /// Český název k zobrazení a aliasy pro matcher (NSC `syncCzechTitles`).
    /// Alias, který se po normalizaci shoduje s originálem, nic nepřidá.
    pub fn aliases(&self, title_orig: &str) -> Vec<String> {
        let orig = normalize_title(title_orig);
        let mut out: Vec<String> = vec![];
        for name in [&self.cs, &self.sk].into_iter().flatten() {
            let n = normalize_title(name);
            if n != orig && !out.iter().any(|a| normalize_title(a) == n) {
                out.push(name.clone());
            }
        }
        out
    }
}

#[derive(Clone)]
pub struct Wikidata {
    http: reqwest::Client,
    endpoint: String,
}

fn build_query(imdb_ids: &[&str]) -> String {
    let values = imdb_ids
        .iter()
        .map(|id| format!("\"{}\"", id.chars().filter(char::is_ascii_alphanumeric).collect::<String>()))
        .collect::<Vec<_>>()
        .join(" ");
    // OPTIONAL — spousta titulů český název nemá, ale i tak chceme řádek.
    format!(
        "SELECT ?imdb ?csLabel ?skLabel WHERE {{\n  VALUES ?imdb {{ {values} }}\n  ?item wdt:P345 ?imdb .\n  OPTIONAL {{ ?item rdfs:label ?csLabel . FILTER(LANG(?csLabel) = \"cs\") }}\n  OPTIONAL {{ ?item rdfs:label ?skLabel . FILTER(LANG(?skLabel) = \"sk\") }}\n}}"
    )
}

static NAMESPACE: LazyLock<Regex> = LazyLock::new(|| rx(r"^(Dílo|Work|Film|Seriál)\s*:\s*"));
static DISAMBIG: LazyLock<Regex> = LazyLock::new(|| {
    rx(r"\s*\((?:film|movie|seriál|serial|tv series|televizní seriál)[^)]*\)\s*$")
});
static YEAR_SUFFIX: LazyLock<Regex> = LazyLock::new(|| rx(r"\s*\(\d{4}\)\s*$"));

/// Labely nesou rozlišovače, které v názvu souboru nikdy nebudou:
/// "Kolonie (film, 2026)" -> "Kolonie", "Dílo:Kolonie" -> "Kolonie".
pub fn clean_label(raw: &str) -> Option<String> {
    let s = NAMESPACE.replace(raw.trim(), "");
    let s = DISAMBIG.replace(&s, "");
    let s = YEAR_SUFFIX.replace(&s, "");
    let s = s.trim();
    (s.chars().count() >= 2).then(|| s.to_owned())
}

#[derive(Deserialize)]
struct Sparql {
    results: Bindings,
}
#[derive(Deserialize)]
struct Bindings {
    bindings: Vec<Row>,
}
#[derive(Deserialize)]
struct Row {
    imdb: Option<Val>,
    #[serde(rename = "csLabel")]
    cs: Option<Val>,
    #[serde(rename = "skLabel")]
    sk: Option<Val>,
}
#[derive(Deserialize)]
struct Val {
    value: String,
}

impl Wikidata {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http, endpoint: ENDPOINT.into() }
    }

    /// Jedna dávka; přechodnou chybu (5xx/429, síť) zkusí jednou znovu —
    /// SPARQL endpoint občas vrátí 502 a hned nato odpoví normálně.
    async fn fetch_batch(&self, ids: &[&str]) -> Result<Vec<LocalizedTitles>> {
        match self.fetch_batch_once(ids).await {
            Err(e) if e.is_retryable() => {
                tokio::time::sleep(Duration::from_secs(2)).await;
                self.fetch_batch_once(ids).await
            }
            r => r,
        }
    }

    async fn fetch_batch_once(&self, ids: &[&str]) -> Result<Vec<LocalizedTitles>> {
        let res = self
            .http
            .get(&self.endpoint)
            .query(&[("query", build_query(ids).as_str()), ("format", "json")])
            .header("Accept", "application/sparql-results+json")
            .timeout(Duration::from_secs(45))
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(Error::Http { status: res.status().as_u16(), context: "wikidata".into() });
        }
        Ok(merge_rows(res.json::<Sparql>().await?.results.bindings))
    }

    /// Lokalizované názvy pro seznam IMDb ID. Selhání dávky nezastaví
    /// zbytek — bez českého názvu titul funguje, jen se hůř páruje.
    pub async fn localized_titles(&self, imdb_ids: &[String]) -> HashMap<String, LocalizedTitles> {
        let mut unique: Vec<&str> = imdb_ids.iter().map(String::as_str).filter(|s| !s.is_empty()).collect();
        unique.sort_unstable();
        unique.dedup();

        let mut out = HashMap::new();
        for (i, batch) in unique.chunks(BATCH_SIZE).enumerate() {
            if i > 0 {
                // Veřejná služba zdarma; nebušit do ní bez pauzy.
                tokio::time::sleep(Duration::from_millis(1200)).await;
            }
            if let Ok(rows) = self.fetch_batch(batch).await {
                for row in rows {
                    out.insert(row.imdb_id.clone(), row);
                }
            }
        }
        out
    }

    /// Jeden titul — při otevření detailu.
    pub async fn localized_title(&self, imdb_id: &str) -> Result<Option<LocalizedTitles>> {
        Ok(self.fetch_batch(&[imdb_id]).await?.into_iter().next())
    }
}

/// Kam AniList anime patří na IMDb: seriál (nebo film) a číslo sezóny.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AniMapping {
    pub imdb_id: String,
    /// `None` = položka sama má IMDb ID (film, seriál bez dělení na sezóny).
    pub season: Option<u16>,
}

fn sparql_values(ids: impl Iterator<Item = String>) -> String {
    ids.map(|id| format!("\"{}\"", id.chars().filter(char::is_ascii_alphanumeric).collect::<String>()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Řádky SPARQL výsledku jako mapy proměnná → hodnota.
fn rows_of(v: &serde_json::Value) -> Vec<HashMap<String, String>> {
    v["results"]["bindings"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r.as_object())
                .map(|r| r.iter().filter_map(|(k, v)| Some((k.clone(), v["value"].as_str()?.to_owned()))).collect())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn parse_anilist_map(v: &serde_json::Value) -> HashMap<u64, AniMapping> {
    let mut out: HashMap<u64, AniMapping> = HashMap::new();
    for r in rows_of(v) {
        let Some(a) = r.get("a").and_then(|a| a.parse::<u64>().ok()) else { continue };
        let tt = |k: &str| r.get(k).filter(|s| s.starts_with("tt")).cloned();
        // Vlastní IMDb ID má přednost před "část seriálu".
        let m = if let Some(imdb) = tt("imdb") {
            AniMapping { imdb_id: imdb, season: None }
        } else if let Some(series) = tt("simdb") {
            let season = r.get("ord").and_then(|o| o.trim().parse::<u16>().ok());
            AniMapping { imdb_id: series, season }
        } else {
            continue;
        };
        match out.get(&a) {
            Some(prev) if prev.season.is_none() || m.season.is_some() && prev.season.is_some() => {}
            _ => {
                out.insert(a, m);
            }
        }
    }
    out
}

impl Wikidata {
    async fn sparql(&self, query: &str) -> Result<serde_json::Value> {
        let once = || async {
            let res = self
                .http
                .get(&self.endpoint)
                .query(&[("query", query), ("format", "json")])
                .header("Accept", "application/sparql-results+json")
                .timeout(Duration::from_secs(45))
                .send()
                .await?;
            if !res.status().is_success() {
                return Err(Error::Http { status: res.status().as_u16(), context: "wikidata".into() });
            }
            Ok(res.json::<serde_json::Value>().await?)
        };
        match once().await {
            Err(e) if e.is_retryable() => {
                tokio::time::sleep(Duration::from_secs(2)).await;
                once().await
            }
            r => r,
        }
    }

    /// AniList id → IMDb (film / seriál + sezóna). Anime, které Wikidata
    /// nezná, v mapě chybí.
    pub async fn anilist_map(&self, anilist_ids: &[u64]) -> Result<HashMap<u64, AniMapping>> {
        if anilist_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let values = sparql_values(anilist_ids.iter().map(u64::to_string));
        let q = format!(
            "SELECT ?a ?imdb ?simdb ?ord WHERE {{ VALUES ?a {{ {values} }} ?item wdt:P8729 ?a .              OPTIONAL {{ ?item wdt:P345 ?imdb }}              OPTIONAL {{ ?item p:P179 ?st . ?st ps:P179 ?series . ?series wdt:P345 ?simdb . OPTIONAL {{ ?st pq:P1545 ?ord }} }} }}"
        );
        Ok(parse_anilist_map(&self.sparql(&q).await?))
    }

    /// IMDb → AniList id (titul sám nebo jeho sezóny). Pro romaji názvy
    /// u anime, které přišlo z Cinemety.
    pub async fn anilist_ids(&self, imdb_id: &str) -> Result<Vec<u64>> {
        let values = sparql_values(std::iter::once(imdb_id.to_owned()));
        let q = format!(
            "SELECT DISTINCT ?a WHERE {{ VALUES ?imdb {{ {values} }} ?s wdt:P345 ?imdb .              {{ ?s wdt:P8729 ?a }} UNION {{ ?item wdt:P179 ?s . ?item wdt:P8729 ?a }} }} LIMIT 20"
        );
        let mut ids: Vec<u64> = rows_of(&self.sparql(&q).await?).iter().filter_map(|r| r.get("a")?.parse().ok()).collect();
        ids.sort_unstable();
        Ok(ids)
    }
}

fn merge_rows(rows: Vec<Row>) -> Vec<LocalizedTitles> {
    let mut by_id: Vec<LocalizedTitles> = vec![];
    for row in rows {
        let Some(id) = row.imdb.map(|v| v.value) else { continue };
        let idx = match by_id.iter().position(|t| t.imdb_id == id) {
            Some(i) => i,
            None => {
                by_id.push(LocalizedTitles { imdb_id: id, ..Default::default() });
                by_id.len() - 1
            }
        };
        let entry = &mut by_id[idx];
        // Titul může mít víc řádků — bereme první použitelný label.
        if entry.cs.is_none() {
            entry.cs = row.cs.and_then(|v| clean_label(&v.value));
        }
        if entry.sk.is_none() {
            entry.sk = row.sk.and_then(|v| clean_label(&v.value));
        }
    }
    by_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_anilist_to_imdb() {
        let v: serde_json::Value = serde_json::from_str(r#"{"results":{"bindings":[
            {"a":{"value":"21"},"imdb":{"value":"tt0388629"}},
            {"a":{"value":"182255"},"simdb":{"value":"tt22248376"},"ord":{"value":"2"}},
            {"a":{"value":"5"},"simdb":{"value":"nm123"}}
        ]}}"#).unwrap();
        let m = parse_anilist_map(&v);
        assert_eq!(m[&21], AniMapping { imdb_id: "tt0388629".into(), season: None });
        assert_eq!(m[&182255], AniMapping { imdb_id: "tt22248376".into(), season: Some(2) });
        assert!(!m.contains_key(&5), "jen tt id");
    }

    #[test]
    fn cleans_labels() {
        assert_eq!(clean_label("Kolonie (film, 2026)").as_deref(), Some("Kolonie"));
        assert_eq!(clean_label("Dílo:Kolonie").as_deref(), Some("Kolonie"));
        assert_eq!(clean_label("Kolonie (2026)").as_deref(), Some("Kolonie"));
        assert_eq!(clean_label("Perníkový táta (televizní seriál)").as_deref(), Some("Perníkový táta"));
        assert_eq!(clean_label(" X "), None);
    }

    #[test]
    fn aliases_skip_original() {
        let t = LocalizedTitles {
            imdb_id: "tt1".into(),
            cs: Some("Interstellar".into()),
            sk: Some("Medzi hviezdami".into()),
        };
        assert_eq!(t.aliases("Interstellar"), vec!["Medzi hviezdami".to_string()]);
    }

    #[test]
    fn merges_sparql_rows() {
        let json = r#"{"results":{"bindings":[
            {"imdb":{"value":"tt0903747"},"csLabel":{"value":"Perníkový táta"}},
            {"imdb":{"value":"tt0903747"},"skLabel":{"value":"Perníkový tatko"}},
            {"imdb":{"value":"tt0816692"}}
        ]}}"#;
        let rows = merge_rows(serde_json::from_str::<Sparql>(json).unwrap().results.bindings);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].cs.as_deref(), Some("Perníkový táta"));
        assert_eq!(rows[0].sk.as_deref(), Some("Perníkový tatko"));
        assert_eq!(rows[1].cs, None);
    }

    #[test]
    fn query_sanitizes_ids() {
        let q = build_query(&["tt123\" }", "tt456"]);
        assert!(q.contains("\"tt123\" \"tt456\""));
    }
}
