//! Shoda souboru s titulem a řazení streamů (NSC `matcher/score.ts`).
//!
//! Nejde jen o podobnost názvu. Klíčové jsou rok a kontrola velikosti —
//! bez nich projde každý soubor, co má v názvu podobné slovo.

use super::parse::ParsedRelease;
use crate::text::{normalize_loose, normalize_numerals, normalize_title, similarity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Movie,
    Series,
}

#[derive(Debug, Clone)]
pub struct MatchTarget {
    pub title_cs: String,
    pub title_orig: String,
    /// Další známé názvy (české z Wikidat, romaji z AniListu, ...).
    pub aliases: Vec<String>,
    pub year: Option<u16>,
    /// Minuty. Slouží ke kontrole velikosti souboru.
    pub runtime: Option<u32>,
    pub kind: MediaKind,
    pub season: Option<u16>,
    pub episode: Option<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchResult {
    pub confidence: f64,
    /// Lidsky čitelné důvody — k ladění.
    pub reasons: Vec<String>,
    pub rejected: Option<String>,
}

impl MatchResult {
    fn reject(reasons: Vec<String>, why: String) -> Self {
        Self { confidence: 0.0, reasons, rejected: Some(why) }
    }
}

pub fn score_match(parsed: &ParsedRelease, target: &MatchTarget) -> MatchResult {
    let mut reasons = Vec::new();

    if let Some(r) = &parsed.reject {
        return MatchResult::reject(reasons, r.clone());
    }

    // --- 1. Název --------------------------------------------------
    let names: Vec<&str> = [&target.title_orig, &target.title_cs]
        .into_iter()
        .chain(target.aliases.iter())
        .map(String::as_str)
        .filter(|t| !t.is_empty())
        .collect();
    let candidates: Vec<String> = names
        .iter()
        .copied()
        .chain(names.iter().filter_map(|t| subtitle_of(t)))
        .flat_map(|t| {
            let norm = normalize_title(t);
            [normalize_numerals(&norm), normalize_loose(&norm), norm]
        })
        .filter(|c| !c.is_empty())
        .collect();

    let parsed_variants: Vec<String> = [
        parsed.title_norm.clone(),
        normalize_numerals(&parsed.title_norm),
        normalize_loose(&parsed.title_norm),
    ]
    .into_iter()
    .filter(|p| !p.is_empty())
    .collect();

    let mut title_score: f64 = 0.0;
    let mut sequel_conflict = false;
    for c in &candidates {
        for p in &parsed_variants {
            // Jiný díl série: "Duna Part One" není "Dune Part Two", i když se
            // názvy liší o pár písmen. Platí jen když číslo dílu nesou oba.
            let (pn, cn) = (sequel_numbers(p), sequel_numbers(c));
            if !pn.is_empty() && !cn.is_empty() && pn != cn {
                sequel_conflict = true;
                continue;
            }
            // Film bez čísla = první díl: "Hunger Games-02" je pokračování,
            // ne "The Hunger Games". (U seriálů ne — čísla tam bývají sezóny.)
            if target.kind == MediaKind::Movie && cn.is_empty() && pn.iter().any(|n| *n >= 2) {
                sequel_conflict = true;
                continue;
            }
            let mut s = similarity(p, c);
            // Soubor občas nese zkrácený název. Prefixovou shodu netrestáme
            // jako překlep. Opačný směr NE: "Interstellar Documentary"
            // není "Interstellar".
            if s < 0.95 && c.starts_with(p.as_str()) && p.len() >= 6 {
                let ratio = p.len() as f64 / c.len() as f64;
                if ratio >= 0.6 {
                    s = s.max(0.75 + 0.2 * ratio);
                }
            }
            // Krátké názvy: pár rozdílných písmen je velký rozdíl. "our boys"
            // vs "the boys" vychází na 0.63, ale je to jiný seriál.
            if s < SHORT_TITLE_MIN && p.chars().count().max(c.chars().count()) <= SHORT_TITLE_LEN {
                s = 0.0;
            }
            title_score = title_score.max(s);
        }
    }

    if title_score < 0.62 {
        if sequel_conflict {
            return MatchResult::reject(vec![], "jiný díl série".into());
        }
        return MatchResult::reject(vec![], format!("název nesedí ({title_score:.2})"));
    }
    reasons.push(format!("název {title_score:.2}"));

    // --- 2. Rok ----------------------------------------------------
    // Nejsilnější rozlišovací signál. Remaky se jmenují úplně stejně.
    let mut year_score = 0.5; // neutrální, když rok v názvu není
    if let (Some(py), Some(ty)) = (parsed.year, target.year) {
        match py.abs_diff(ty) {
            0 => {
                year_score = 1.0;
                reasons.push("rok přesně".into());
            }
            1 => {
                // Datum vydání se liší podle regionu, ±1 rok je běžné.
                year_score = 0.8;
                reasons.push("rok ±1".into());
            }
            2 if target.kind == MediaKind::Series => {
                year_score = 0.6;
                reasons.push("rok ±2 (seriál)".into());
            }
            // Epizoda seriálu se správným SxxExx: rok v názvu bývá rok
            // vysílání epizody (3. sezóna z roku 2026 seriálu z 2022),
            // ne rok seriálu. Neutrální, ne důvod zamítnout.
            _ if target.kind == MediaKind::Series && target.episode.is_some() && parsed.episode.is_some() => {
                reasons.push("rok epizody".into());
            }
            _ => return MatchResult::reject(reasons, format!("rok nesedí ({py} vs {ty})")),
        }
    }

    // --- 3. Epizoda ------------------------------------------------
    let mut episode_score = 1.0;
    match target.kind {
        MediaKind::Series => {
            if let Some(want_e) = target.episode {
                if parsed.season != target.season || parsed.episode != Some(want_e) {
                    return MatchResult::reject(
                        reasons,
                        format!(
                            "epizoda nesedí (S{}E{} vs S{}E{})",
                            fmt_opt(parsed.season),
                            fmt_opt(parsed.episode),
                            fmt_opt(target.season),
                            want_e
                        ),
                    );
                }
                reasons.push(format!("S{}E{want_e}", fmt_opt(target.season)));
            } else if parsed.episode.is_some() {
                episode_score = 0.9;
            }
        }
        MediaKind::Movie => {
            if parsed.episode.is_some() {
                return MatchResult::reject(reasons, "film, ale soubor vypadá jako epizoda".into());
            }
        }
    }

    // --- 4. Kvalita zdroje ----------------------------------------
    // CAM/TS rip je skoro vždycky zklamání; nezahazuj ho, ale sraž dolů.
    let mut quality_score = 1.0;
    if parsed.source == Some("cam") {
        quality_score = 0.5;
        reasons.push("CAM/TS".into());
    }

    let confidence =
        (title_score * 0.55 + year_score * 0.3 + episode_score * 0.1 + quality_score * 0.05)
            .clamp(0.0, 1.0);

    MatchResult { confidence, reasons, rejected: None }
}

/// Do kolika znaků je název "krátký" a jakou podobnost musí mít.
const SHORT_TITLE_LEN: usize = 12;
const SHORT_TITLE_MIN: f64 = 0.8;

/// Podtitul za dvojtečkou jako samostatný název: soubory se často jmenují
/// jen "The Rings of Power" místo "The Lord of the Rings: The Rings of Power".
/// Jen dostatečně dlouhý — "Part Two" nebo "Impossible" by matchovaly
/// cokoli.
pub(crate) fn subtitle_of(title: &str) -> Option<&str> {
    let (_, sub) = title.split_once(':')?;
    let sub = sub.trim();
    let norm = normalize_title(sub);
    (norm.split(' ').count() >= 3 || norm.len() >= 12).then_some(sub)
}

/// Čísla dílů v normalizovaném názvu: číslice, anglické a české
/// číslovky ("two", "druha"), římské číslice (přes `normalize_numerals`).
fn sequel_numbers(norm: &str) -> Vec<u8> {
    let mut out: Vec<u8> = normalize_numerals(norm)
        .split(' ')
        .filter_map(|w| match w {
            "one" | "prvni" | "prvy" => Some(1),
            "two" | "druha" | "druhy" | "druhe" => Some(2),
            "three" | "treti" => Some(3),
            "four" | "ctvrta" | "ctvrty" | "ctvrte" => Some(4),
            "five" | "pata" | "paty" | "pate" => Some(5),
            "six" | "sesta" | "sesty" => Some(6),
            "seven" | "sedma" | "sedmy" => Some(7),
            "eight" | "osma" | "osmy" => Some(8),
            "nine" | "devata" | "devaty" => Some(9),
            w if (1..=2).contains(&w.len()) && w.bytes().all(|b| b.is_ascii_digit()) => w.parse().ok(),
            _ => None,
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn fmt_opt(v: Option<u16>) -> String {
    v.map_or_else(|| "null".into(), |v| v.to_string())
}

/// Minimální rozumný bitrate v bitech/s podle výšky obrazu.
const MIN_BITRATE: &[(u16, f64)] =
    &[(2160, 6_000_000.0), (1080, 1_500_000.0), (720, 800_000.0), (480, 300_000.0)];

/// Kontrola velikosti proti délce. Vrací multiplikátor confidence 0..1 —
/// 0 znamená evidentní fake (40MB "film").
pub fn size_sanity(size_bytes: u64, runtime_min: Option<u32>, height: Option<u16>) -> (f64, String) {
    if size_bytes == 0 {
        return (0.9, "velikost neznámá".into());
    }
    let Some(runtime) = runtime_min.filter(|r| *r > 0) else {
        return (1.0, "runtime neznámý".into());
    };

    let bitrate = size_bytes as f64 * 8.0 / (runtime as f64 * 60.0);
    let h = height.unwrap_or(480);
    let min = MIN_BITRATE.iter().find(|(mh, _)| h >= *mh).map_or(250_000.0, |(_, b)| *b);

    if bitrate < min * 0.4 {
        (0.0, format!("bitrate {}kbps je nesmysl", (bitrate / 1000.0).round()))
    } else if bitrate < min {
        (0.7, format!("nízký bitrate {}kbps", (bitrate / 1000.0).round()))
    } else if bitrate > 200_000_000.0 {
        (0.5, "podezřele velký soubor".into())
    } else {
        (1.0, format!("bitrate {}Mbps", (bitrate / 1_000_000.0).round()))
    }
}

/// Vstup pro řazení streamů.
#[derive(Debug, Clone, Default)]
pub struct SortInput<'a> {
    pub audio_langs: &'a [String],
    pub sub_langs: &'a [String],
    pub height: Option<u16>,
    pub hdr: bool,
    pub size_bytes: u64,
    pub ok_count: u32,
    pub fail_count: u32,
}

/// Řazení streamů pro uživatele (vyšší = lepší). V CZ/SK je dabing
/// a titulky nadřazený rozlišení — 720p s dabingem lidi chtějí víc než 4K bez.
pub fn stream_sort_key(s: &SortInput<'_>) -> f64 {
    let has = |v: &[String], l: &str| v.iter().any(|x| x == l);
    let mut score = 0.0;

    if has(s.audio_langs, "cs") {
        score += 10_000.0;
    } else if has(s.audio_langs, "sk") {
        score += 9_000.0;
    } else if has(s.sub_langs, "cs") {
        score += 5_000.0;
    } else if has(s.sub_langs, "sk") {
        score += 4_500.0;
    }

    score += f64::from(s.height.unwrap_or(0)) * 1.5;
    if s.hdr {
        score += 300.0;
    }

    // Ověřená funkčnost přebíjí technické parametry.
    score += f64::from(s.ok_count.min(20)) * 200.0;
    score -= f64::from(s.fail_count.min(20)) * 400.0;

    // Na zařízení statistiky sbírá jediný uživatel: dvě selhání bez
    // jediného úspěchu = mrtvý odkaz. Musí spadnout pod všechno funkční,
    // i pod zdroje bez dabingu — jinak by navrchu visel zdroj, který
    // nejde pustit, a aplikace by vypadala rozbitě.
    if s.fail_count >= 2 && s.ok_count == 0 {
        score -= 20_000.0;
    }

    // Při shodě všeho ostatního větší soubor = lepší kvalita, ale slabá váha.
    score += (s.size_bytes as f64 / 1_000_000_000.0).min(60.0) * 5.0;

    score
}
