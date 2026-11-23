//! Doporučení podle vkusu — počítá se v zařízení, nic se nikam neposílá.
//!
//! Vstupy: profil vkusu (slábnoucí skóre vlastností, viz `storage::taste`)
//! a kandidáti z veřejných katalogů. Postup:
//! 1. skóre kandidáta = 0.5 shoda s profilem + 0.5 kvalita (hodnocení,
//!    popularita, čerstvost), přičemž kvalita se počítá jen v míře shody —
//!    výborný titul úplně mimo vkus nepředběhne oblíbený žánr,
//! 2. pestrost (MMR): další titul musí být dobrý a zároveň jiný než už vybrané,
//! 3. objevování: zhruba každé 8. místo dostane populární titul mimo
//!    obvyklé žánry, aby se profil neuzavřel do jedné škatulky.

use std::collections::{HashMap, HashSet};

use crate::matcher::MediaKind;
use crate::meta::TitleMeta;

/// Váhy skóre kandidáta: shoda s vkusem vs. kvalita (uvnitř kvality
/// hodnocení / popularita / čerstvost).
const W_MATCH: f64 = 0.50;
const W_QUALITY: f64 = 0.50;
const Q_RATING: f64 = 0.40;
const Q_POPULARITY: f64 = 0.30;
const Q_FRESH: f64 = 0.30;

/// MMR: 1.0 = jen kvalita, 0.0 = jen odlišnost.
const MMR_LAMBDA: f64 = 0.7;
/// Každé n-té místo je objevné.
const EXPLORE_EVERY: usize = 8;
/// Objevný titul musí být dobrý — jinak to místo dostane běžné doporučení.
const EXPLORE_MIN_RATING: f32 = 7.0;

/// Profil vkusu znormalizovaný na 0..1 v každé rodině vlastností.
#[derive(Debug, Clone, Default)]
pub struct Profile {
    genres: HashMap<String, f64>,
    decades: HashMap<u16, f64>,
    countries: HashMap<String, f64>,
    movie: f64,
    series: f64,
    anime: f64,
}

fn normalize<K: std::hash::Hash + Eq>(m: HashMap<K, f64>) -> HashMap<K, f64> {
    let max = m.values().cloned().fold(0.0, f64::max);
    if max <= 0.0 {
        return HashMap::new();
    }
    m.into_iter().map(|(k, v)| (k, v / max)).collect()
}

impl Profile {
    /// Z vlastností "genre:Drama", "decade:2010", "country:Japan",
    /// "kind:movie|series|anime" s aktuálním skóre.
    pub fn from_taste(taste: &[(String, f64)]) -> Self {
        let (mut g, mut d, mut c) = (HashMap::new(), HashMap::new(), HashMap::new());
        let mut kinds: HashMap<&str, f64> = HashMap::new();
        for (f, s) in taste {
            if *s <= 0.0 {
                continue;
            }
            if let Some(v) = f.strip_prefix("genre:") {
                g.insert(v.to_owned(), *s);
            } else if let Some(v) = f.strip_prefix("decade:") {
                if let Ok(y) = v.parse() {
                    d.insert(y, *s);
                }
            } else if let Some(v) = f.strip_prefix("country:") {
                c.insert(v.to_owned(), *s);
            } else if let Some(v) = f.strip_prefix("kind:") {
                kinds.insert(match v {
                    "movie" => "movie",
                    "series" => "series",
                    "anime" => "anime",
                    _ => continue,
                }, *s);
            }
        }
        let kmax = kinds.values().cloned().fold(0.0, f64::max).max(f64::MIN_POSITIVE);
        Self {
            genres: normalize(g),
            decades: normalize(d),
            countries: normalize(c),
            movie: kinds.get("movie").copied().unwrap_or(0.0) / kmax,
            series: kinds.get("series").copied().unwrap_or(0.0) / kmax,
            anime: kinds.get("anime").copied().unwrap_or(0.0) / kmax,
        }
    }

    /// Bez dat o vkusu (nový uživatel) doporučovat nemá smysl.
    pub fn is_empty(&self) -> bool {
        self.genres.is_empty()
    }

    /// Nejsilnější žánry — z nich se berou katalogy kandidátů.
    pub fn top_genres(&self, n: usize) -> Vec<String> {
        let mut v: Vec<(&String, &f64)> = self.genres.iter().collect();
        v.sort_by(|a, b| b.1.total_cmp(a.1).then(a.0.cmp(b.0)));
        v.into_iter().take(n).map(|(g, _)| g.clone()).collect()
    }

    /// Jak moc má uživatel rád daný typ (0..1).
    pub fn kind_weight(&self, kind: MediaKind) -> f64 {
        match kind {
            MediaKind::Movie => self.movie,
            MediaKind::Series => self.series,
        }
    }

    /// Shoda titulu s profilem, 0..1.
    pub fn match_score(&self, t: &TitleMeta) -> f64 {
        // Žánry: průměr dvou nejlépe hodnocených žánrů titulu (jeden
        // oblíbený žánr ve směsi pěti nemá titul úplně přebít).
        let mut gs: Vec<f64> = t.genres.iter().map(|g| self.genres.get(g).copied().unwrap_or(0.0)).collect();
        gs.sort_by(|a, b| b.total_cmp(a));
        let genre = if gs.is_empty() { 0.0 } else { gs.iter().take(2).sum::<f64>() / gs.len().min(2) as f64 };

        let decade = t.year.map(|y| self.decades.get(&(y / 10 * 10)).copied().unwrap_or(0.0)).unwrap_or(0.0);
        let country = t.countries.iter().map(|c| self.countries.get(c).copied().unwrap_or(0.0)).fold(0.0, f64::max);
        let kind = if t.anime { self.anime.max(self.kind_weight(t.kind) * 0.5) } else { self.kind_weight(t.kind) };

        0.6 * genre + 0.15 * decade + 0.1 * country + 0.15 * kind
    }

    fn is_familiar(&self, t: &TitleMeta, top: &HashSet<String>) -> bool {
        t.genres.iter().any(|g| top.contains(g))
    }
}

/// Podobnost dvou titulů pro pestrost: Jaccard žánrů (+ stejný typ).
fn similarity(a: &TitleMeta, b: &TitleMeta) -> f64 {
    let ga: HashSet<&String> = a.genres.iter().collect();
    let gb: HashSet<&String> = b.genres.iter().collect();
    let union = ga.union(&gb).count();
    let j = if union == 0 { 0.0 } else { ga.intersection(&gb).count() as f64 / union as f64 };
    0.8 * j + if a.kind == b.kind { 0.2 } else { 0.0 }
}

/// Seřadí kandidáty (id, titul) a vybere `limit` doporučení se skóre.
/// Kandidáti už nemají obsahovat viděné/oblíbené — to filtruje volající.
pub fn rank(candidates: &[(i64, TitleMeta)], profile: &Profile, current_year: u16, limit: usize) -> Vec<(i64, f64)> {
    if candidates.is_empty() || limit == 0 {
        return vec![];
    }
    let max_pop = candidates.iter().map(|(_, t)| t.popularity).fold(0.0, f64::max).max(f64::MIN_POSITIVE);

    let base: Vec<f64> = candidates
        .iter()
        .map(|(_, t)| {
            let rating = t.rating.map(|r| f64::from(r) / 10.0).unwrap_or(0.5);
            let pop = (t.popularity / max_pop).clamp(0.0, 1.0);
            let fresh = t
                .year
                .map(|y| (1.0 - f64::from(current_year.saturating_sub(y)) / 20.0).clamp(0.0, 1.0))
                .unwrap_or(0.3);
            let m = profile.match_score(t);
            let quality = Q_RATING * rating + Q_POPULARITY * pop + Q_FRESH * fresh;
            W_MATCH * m + W_QUALITY * quality * (0.4 + 0.6 * m)
        })
        .collect();

    let top: HashSet<String> = profile.top_genres(3).into_iter().collect();
    let mut picked: Vec<usize> = vec![];
    let mut used = vec![false; candidates.len()];

    while picked.len() < limit.min(candidates.len()) {
        let slot = picked.len() + 1;
        let explore = slot.is_multiple_of(EXPLORE_EVERY) && !top.is_empty();

        let mut best: Option<(usize, f64)> = None;
        for (i, (_, t)) in candidates.iter().enumerate() {
            if used[i] {
                continue;
            }
            if explore && (profile.is_familiar(t, &top) || t.rating.is_none_or(|r| r < EXPLORE_MIN_RATING)) {
                continue;
            }
            let score = if explore {
                // Objevné místo: populární a dobře hodnocené mimo obvyklé žánry.
                (t.popularity / max_pop) * 0.6 + t.rating.map(|r| f64::from(r) / 10.0).unwrap_or(0.5) * 0.4
            } else {
                let max_sim = picked.iter().map(|&j| similarity(t, &candidates[j].1)).fold(0.0, f64::max);
                MMR_LAMBDA * base[i] - (1.0 - MMR_LAMBDA) * max_sim
            };
            if best.is_none_or(|(_, b)| score > b) {
                best = Some((i, score));
            }
        }
        // Nenašel se objevný kandidát → obsadit běžným.
        let Some((i, _)) = best.or_else(|| {
            (0..candidates.len()).filter(|&i| !used[i]).map(|i| (i, base[i])).max_by(|a, b| a.1.total_cmp(&b.1))
        }) else {
            break;
        };
        used[i] = true;
        picked.push(i);
    }

    picked.into_iter().map(|i| (candidates[i].0, base[i])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, kind: MediaKind, genres: &[&str], year: u16, rating: f32, pop: f64) -> TitleMeta {
        TitleMeta {
            imdb_id: format!("tt{name}"),
            kind,
            title_orig: name.into(),
            title_cs: None,
            aliases: vec![],
            year: Some(year),
            overview: None,
            poster: None,
            background: None,
            logo: None,
            runtime_min: None,
            rating: Some(rating),
            genres: genres.iter().map(|g| g.to_string()).collect(),
            countries: vec![],
            anime: false,
            release_date: None,
            popularity: pop,
            episodes: vec![],
        }
    }

    fn crime_fan() -> Profile {
        Profile::from_taste(&[
            ("genre:Crime".into(), 6.0),
            ("genre:Drama".into(), 4.0),
            ("genre:Thriller".into(), 2.0),
            ("decade:2010".into(), 3.0),
            ("kind:series".into(), 5.0),
            ("kind:movie".into(), 1.0),
        ])
    }

    #[test]
    fn empty_profile() {
        assert!(Profile::from_taste(&[]).is_empty());
        assert!(rank(&[], &crime_fan(), 2026, 10).is_empty());
    }

    #[test]
    fn profile_prefers_matching_titles() {
        let p = crime_fan();
        assert_eq!(p.top_genres(2), vec!["Crime".to_string(), "Drama".to_string()]);
        let crime = t("a", MediaKind::Series, &["Crime", "Drama"], 2015, 8.0, 10.0);
        let romcom = t("b", MediaKind::Movie, &["Romance", "Comedy"], 2015, 8.0, 10.0);
        assert!(p.match_score(&crime) > 0.75);
        assert!(p.match_score(&romcom) < 0.2);
    }

    #[test]
    fn ranking_matches_taste_diversifies_and_explores() {
        let p = crime_fan();
        let mut cands: Vec<(i64, TitleMeta)> = (0..10)
            .map(|i| (i, t(&format!("c{i}"), MediaKind::Series, &["Crime", "Drama"], 2018, 8.0, 10.0 - i as f64)))
            .collect();
        cands.push((100, t("thr", MediaKind::Series, &["Thriller", "Mystery"], 2019, 7.8, 5.0)));
        cands.push((200, t("doc", MediaKind::Movie, &["Documentary"], 2020, 8.5, 20.0)));
        cands.push((300, t("rom", MediaKind::Movie, &["Romance"], 1995, 5.0, 1.0)));

        let ranked = rank(&cands, &p, 2026, 8);
        let ids: Vec<i64> = ranked.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids.len(), 8);
        assert_eq!(ids[0], 0, "nejlepší shoda první");
        assert!(ids[..4].contains(&100), "pestrost: thriller mezi prvními čtyřmi, ne až za všemi krimi: {ids:?}");
        assert!(ids[..3].iter().filter(|id| **id < 10).count() >= 2, "nahoře převládá oblíbené krimi: {ids:?}");
        assert!(ids.contains(&200), "dobrý titul mimo obvyklé žánry se objeví (objevování)");
        assert!(!ids.contains(&300), "slabý nesouvisející titul ne, ani na objevném místě");
    }
}
