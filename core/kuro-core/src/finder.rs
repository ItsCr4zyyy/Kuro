//! Vyhledání zdrojů k titulu na zařízení.
//!
//! V NSC to dělal serverový indexer dávkově předem (desítky hodin).
//! Tady se to děje na vyžádání při otevření titulu/epizody: dotazy
//! na všechny providery souběžně → parse → score → kontrola velikosti →
//! seřazení. Výsledek si volající uloží do lokální cache.

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::sync::Semaphore;

use crate::error::Error;
use crate::matcher::{
    parse_release, score_match, size_sanity, stream_sort_key, Hdr, MatchTarget, MediaKind, SortInput,
};
use crate::providers::{Provider, ProviderId, RawFile, SearchOptions, SearchSort};

/// Nalezený a přijatý zdroj.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundStream {
    pub provider: ProviderId,
    pub ident: String,
    pub filename: String,
    pub size_bytes: u64,
    pub height: Option<u16>,
    pub hdr: Option<Hdr>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub channels: Option<String>,
    pub source: Option<String>,
    pub audio_langs: Vec<String>,
    pub sub_langs: Vec<String>,
    pub dubbed: bool,
    pub dual_audio: bool,
    pub lang_explicit: bool,
    /// 0..1, jak moc si je matcher jistý, že soubor patří k titulu.
    pub confidence: f64,
    pub positive_votes: u32,
    pub negative_votes: u32,
    /// Zpětná vazba z přehrávání (doplní úložiště; finder je nezná).
    pub ok_count: u32,
    pub fail_count: u32,
}

impl FoundStream {
    pub fn sort_key(&self) -> f64 {
        stream_sort_key(&SortInput {
            audio_langs: &self.audio_langs,
            sub_langs: &self.sub_langs,
            height: self.height,
            hdr: self.hdr.is_some(),
            size_bytes: self.size_bytes,
            ok_count: self.ok_count,
            fail_count: self.fail_count,
        })
    }
}

/// Sloučí kopie stejného souboru (stejný soubor nahraný víckrát, často
/// i u více providerů): stejná velikost (na 10 MB), rozlišení a zvuk.
/// Vstup musí být seřazený — z každé skupiny zůstane první (nejlepší
/// včetně statistik přehrání, takže nefunkční kopii nahradí funkční).
pub fn collapse_duplicates(streams: Vec<FoundStream>) -> Vec<FoundStream> {
    let mut seen = std::collections::HashSet::new();
    streams
        .into_iter()
        .filter(|s| {
            if s.size_bytes == 0 {
                return true;
            }
            let mut langs = s.audio_langs.clone();
            langs.sort();
            seen.insert((s.size_bytes / 10_000_000, s.height, langs, s.dubbed))
        })
        .collect()
}

/// Seřadí zdroje pro uživatele (nejlepší první).
pub fn sort_streams(streams: &mut [FoundStream]) {
    streams.sort_by(|a, b| {
        b.sort_key()
            .total_cmp(&a.sort_key())
            .then(b.confidence.total_cmp(&a.confidence))
    });
}

/* ------------------------------ dotazy ------------------------------ */

/// Dotazy pro film. Originální název s rokem má nejlepší poměr
/// přesnost/šum, český název pak dohledá CZ releasy.
pub fn movie_queries(t: &MatchTarget) -> Vec<String> {
    let orig = t.title_orig.trim();
    let cs = t.title_cs.trim();
    let mut out: Vec<String> = vec![];
    let mut add = |q: String| {
        if q.chars().count() >= 2 && !out.contains(&q) {
            out.push(q);
        }
    };
    if let Some(y) = t.year {
        add(format!("{orig} {y}"));
        if cs != orig {
            add(format!("{cs} {y}"));
        }
    }
    add(orig.to_owned());
    if cs != orig {
        add(cs.to_owned());
    }
    if let Some(sub) = crate::matcher::score::subtitle_of(orig) {
        add(match t.year {
            Some(y) => format!("{sub} {y}"),
            None => sub.to_owned(),
        });
    }
    for a in &t.aliases {
        add(a.trim().to_owned());
    }
    out.truncate(5);
    out
}

/// Dotazy pro epizodu: `Název S01E02`, český název, `Název 1x02`.
pub fn episode_queries(t: &MatchTarget, season: u16, episode: u16) -> Vec<String> {
    let orig = t.title_orig.trim();
    let cs = t.title_cs.trim();
    let tag = format!("S{season:02}E{episode:02}");
    let mut out = vec![format!("{orig} {tag}")];
    if cs != orig {
        out.push(format!("{cs} {tag}"));
    }
    // Soubory často nesou jen podtitul: "The Rings of Power S03E01".
    if let Some(sub) = crate::matcher::score::subtitle_of(orig) {
        out.push(format!("{sub} {tag}"));
    }
    out.push(format!("{orig} {season}x{episode:02}"));
    out
}

/// Dotazy podle typu cíle.
pub fn queries_for(t: &MatchTarget) -> Vec<String> {
    match (t.kind, t.season, t.episode) {
        (MediaKind::Series, Some(s), Some(e)) => episode_queries(t, s, e),
        _ => movie_queries(t),
    }
}

/* ------------------------------ finder ------------------------------ */

#[derive(Debug, Clone)]
pub struct FinderConfig {
    /// Práh, pod kterým se soubor k titulu nepřiřadí (NSC: 0.55).
    pub min_confidence: f64,
    pub search_limit: u32,
}

impl Default for FinderConfig {
    fn default() -> Self {
        Self { min_confidence: 0.55, search_limit: 60 }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FindReport {
    /// Seřazené, bez duplicit.
    pub streams: Vec<FoundStream>,
    pub queries: u32,
    pub files_seen: u32,
    /// Důvod zamítnutí → počet. K ladění matcheru.
    pub rejected: HashMap<String, u32>,
    /// Chyby providerů (dotaz selhal). Prázdné = všechno odpovědělo.
    pub errors: Vec<(ProviderId, Error)>,
}

impl FindReport {
    /// Některý provider hlásí propadlé přihlášení.
    pub fn auth_expired(&self) -> Option<ProviderId> {
        self.errors.iter().find(|(_, e)| matches!(e, Error::AuthExpired)).map(|(p, _)| *p)
    }
}

pub struct StreamFinder {
    providers: Vec<(Arc<dyn Provider>, Arc<Semaphore>)>,
    cfg: FinderConfig,
    /// Zdroje vypnuté uživatelem v nastavení.
    disabled: std::sync::RwLock<Vec<ProviderId>>,
}

impl StreamFinder {
    pub fn new(providers: Vec<Arc<dyn Provider>>, cfg: FinderConfig) -> Self {
        let providers = providers
            .into_iter()
            .map(|p| {
                let sem = Arc::new(Semaphore::new(p.max_concurrency().max(1)));
                (p, sem)
            })
            .collect();
        Self { providers, cfg, disabled: Default::default() }
    }

    /// Vypne / zapne zdroje (nastavení). Vrací true, když přibyl zapnutý
    /// zdroj — uložené výsledky pak nejsou úplné.
    pub fn set_disabled(&self, ids: Vec<ProviderId>) -> bool {
        let mut d = self.disabled.write().expect("zámek");
        let newly_enabled = d.iter().any(|p| !ids.contains(p));
        *d = ids;
        newly_enabled
    }

    /// Zdroj se prohledává a jeho výsledky se ukazují: je zapnutý a umí
    /// vydat odkaz (Webshare jen s přihlášením).
    pub fn is_active(&self, id: ProviderId) -> bool {
        !self.disabled.read().expect("zámek").contains(&id)
            && self.providers.iter().any(|(p, _)| p.id() == id && p.can_resolve())
    }

    /// Najde zdroje k cíli. `on_partial` dostává průběžně seřazený seznam
    /// pokaždé, když dorazí odpověď na další dotaz — UI může ukazovat
    /// zdroje hned, ne až po posledním provideru.
    pub async fn find(
        &self,
        target: &MatchTarget,
        on_partial: &(dyn Fn(&[FoundStream]) + Send + Sync),
    ) -> FindReport {
        let queries = queries_for(target);
        let opts = SearchOptions { limit: self.cfg.search_limit, offset: 0, sort: SearchSort::Largest };

        let mut tasks = FuturesUnordered::new();
        // Provider, který teď neumí vydat odkaz (Webshare bez přihlášení),
        // se vůbec neptá — jeho zdroje by nešly pustit.
        for (provider, sem) in self.providers.iter().filter(|(p, _)| self.is_active(p.id())) {
            for q in &queries {
                let (provider, sem, opts) = (provider.clone(), sem.clone(), opts.clone());
                let q = q.clone();
                tasks.push(async move {
                    let _permit = sem.acquire_owned().await.expect("semafor se nezavírá");
                    (provider.id(), provider.search(&q, &opts).await)
                });
            }
        }

        let mut report = FindReport::default();
        let mut found: HashMap<(ProviderId, String), FoundStream> = HashMap::new();
        // Stejný soubor u více providerů (Hellspy zrcadlí Přehraj.to) jen jednou.
        let mut dedup: HashMap<String, (ProviderId, String)> = HashMap::new();

        while let Some((pid, result)) = tasks.next().await {
            report.queries += 1;
            let files = match result {
                Ok(f) => f,
                Err(e) => {
                    *report.rejected.entry(format!("provider: {e}")).or_default() += 1;
                    report.errors.push((pid, e));
                    continue;
                }
            };
            report.files_seen += files.len() as u32;

            let mut changed = false;
            for file in files {
                match self.evaluate(pid, &file, target) {
                    Ok(s) => {
                        let key = (pid, s.ident.clone());
                        if let Some(dk) = &file.dedup_key {
                            match dedup.get(dk) {
                                Some(owner) if *owner != key => continue,
                                Some(_) => {}
                                None => {
                                    dedup.insert(dk.clone(), key.clone());
                                }
                            }
                        }
                        let better = found.get(&key).is_none_or(|old| old.confidence < s.confidence);
                        if better {
                            found.insert(key, s);
                            changed = true;
                        }
                    }
                    Err(reason) => *report.rejected.entry(reason).or_default() += 1,
                }
            }

            if changed {
                let mut partial: Vec<FoundStream> = found.values().cloned().collect();
                sort_streams(&mut partial);
                on_partial(&partial);
            }
        }

        report.streams = found.into_values().collect();
        sort_streams(&mut report.streams);
        report
    }

    /// Rozhodne o jednom souboru. Err = důvod zamítnutí (sjednocený do kategorie).
    fn evaluate(&self, pid: ProviderId, file: &RawFile, target: &MatchTarget) -> Result<FoundStream, String> {
        if file.password_protected {
            return Err("chráněno heslem".into());
        }

        let parsed = parse_release(&file.name);
        let m = score_match(&parsed, target);
        if let Some(r) = m.rejected {
            return Err(category(&r));
        }

        // Délka souboru (když ji provider zná) proti délce titulu: odhalí
        // ukázky, celé sezóny v jednom souboru i úplně jiná videa.
        if let (Some(dur), Some(rt)) = (file.duration_sec, target.runtime) {
            let ratio = f64::from(dur) / (f64::from(rt) * 60.0);
            if !(0.6..=1.6).contains(&ratio) {
                return Err("délka nesedí".into());
            }
        }

        let (factor, reason) = size_sanity(file.size_bytes, target.runtime, parsed.height);
        let confidence = m.confidence * factor;
        if confidence < self.cfg.min_confidence {
            return Err(format!("nízká confidence — {}", category(&reason)));
        }

        // Hlasování uživatelů je slabý, ale zadarmo dostupný signál fake souboru.
        if file.negative_votes > 5 && file.positive_votes < file.negative_votes {
            return Err("negativní hlasy".into());
        }

        Ok(FoundStream {
            provider: pid,
            ident: file.ident.clone(),
            filename: file.name.clone(),
            size_bytes: file.size_bytes,
            // Provider streamuje převod (Přehraj.to/Hellspy): rozlišení max.
            // jeho strop, HDR a kodek z názvu souboru pro převod neplatí.
            height: match file.max_height {
                Some(cap) => Some(parsed.height.map_or(cap, |h| h.min(cap))),
                None => parsed.height,
            },
            hdr: if file.max_height.is_some() { None } else { parsed.hdr },
            video_codec: if file.max_height.is_some() { None } else { parsed.video_codec.map(str::to_owned) },
            audio_codec: parsed.audio_codec.map(str::to_owned),
            channels: parsed.channels.map(str::to_owned),
            source: parsed.source.map(str::to_owned),
            audio_langs: parsed.audio_langs,
            sub_langs: parsed.sub_langs,
            dubbed: parsed.dubbed,
            dual_audio: parsed.dual_audio,
            lang_explicit: parsed.lang_explicit,
            confidence,
            positive_votes: file.positive_votes,
            negative_votes: file.negative_votes,
            ok_count: 0,
            fail_count: 0,
        })
    }
}

/// "rok nesedí (2015 vs 2014)" -> "rok nesedí".
fn category(reason: &str) -> String {
    reason.split('(').next().unwrap_or(reason).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use crate::providers::{Account, LinkPurpose, ResolvedLink};
    use async_trait::async_trait;

    fn target() -> MatchTarget {
        MatchTarget {
            title_cs: "Duna: Část druhá".into(),
            title_orig: "Dune: Part Two".into(),
            aliases: vec!["Duna 2".into()],
            year: Some(2024),
            runtime: Some(166),
            kind: MediaKind::Movie,
            season: None,
            episode: None,
        }
    }

    #[test]
    fn duplicates_collapse_to_best_copy() {
        let mk = |ident: &str, gb: f64, h: u16, langs: &[&str], fails: u32| FoundStream {
            provider: ProviderId::PrehrajTo,
            ident: ident.into(),
            filename: format!("{ident}.mp4"),
            size_bytes: (gb * 1e9) as u64,
            height: Some(h),
            hdr: None,
            video_codec: None,
            audio_codec: None,
            channels: None,
            source: None,
            audio_langs: langs.iter().map(|l| l.to_string()).collect(),
            sub_langs: vec![],
            dubbed: true,
            dual_audio: false,
            lang_explicit: true,
            confidence: 1.0,
            positive_votes: 0,
            negative_votes: 0,
            ok_count: 0,
            fail_count: fails,
        };
        let mut v = vec![
            mk("a", 13.76, 1080, &["cs", "en"], 2), // mrtvá kopie
            mk("b", 13.76, 1080, &["en", "cs"], 0),
            mk("c", 13.76, 1080, &["cs", "en"], 0),
            mk("d", 7.9, 1080, &["cs"], 0),
            mk("e", 13.76, 720, &["cs", "en"], 0), // jiné rozlišení = jiný zdroj
        ];
        sort_streams(&mut v);
        let out: Vec<String> = collapse_duplicates(v).into_iter().map(|s| s.ident).collect();
        assert_eq!(out.len(), 3, "{out:?}");
        assert!(!out.contains(&"a".to_string()), "mrtvou kopii nahradí funkční: {out:?}");
    }

    #[test]
    fn movie_query_order() {
        assert_eq!(
            movie_queries(&target()),
            vec!["Dune: Part Two 2024", "Duna: Část druhá 2024", "Dune: Part Two", "Duna: Část druhá", "Duna 2"]
        );
    }

    #[test]
    fn episode_query_shape() {
        let mut t = target();
        t.kind = MediaKind::Series;
        assert_eq!(
            episode_queries(&t, 1, 2),
            vec!["Dune: Part Two S01E02", "Duna: Část druhá S01E02", "Dune: Part Two 1x02"]
        );
    }

    /// Provider s pevnými výsledky — ověří celý řetězec bez sítě.
    struct Fake(Vec<RawFile>);

    #[async_trait]
    impl Provider for Fake {
        fn id(&self) -> ProviderId {
            ProviderId::Webshare
        }
        fn max_concurrency(&self) -> usize {
            3
        }
        async fn login(&self, _: &str, _: &str) -> Result<String> {
            unreachable!()
        }
        fn set_token(&self, _: Option<String>) {}
        fn token(&self) -> Option<String> {
            None
        }
        async fn account(&self) -> Result<Account> {
            unreachable!()
        }
        async fn search(&self, _: &str, _: &SearchOptions) -> Result<Vec<RawFile>> {
            Ok(self.0.clone())
        }
        async fn resolve(&self, _: &str, _: LinkPurpose) -> Result<ResolvedLink> {
            unreachable!()
        }
    }

    fn file(ident: &str, name: &str, gb: f64) -> RawFile {
        RawFile {
            ident: ident.into(),
            name: name.into(),
            size_bytes: (gb * 1e9) as u64,
            positive_votes: 0,
            negative_votes: 0,
            password_protected: false,
            duration_sec: None,
            max_height: None,
            dedup_key: None,
        }
    }

    #[tokio::test]
    async fn finds_dedups_and_ranks_czech_first() {
        let files = vec![
            file("a", "Dune.Part.Two.2024.2160p.UHD.BluRay.x265.HDR.mkv", 25.0),
            file("b", "Duna.Cast.druha.2024.720p.CZ.dabing.mkv", 3.0),
            file("c", "Dune.Part.Two.2024.1080p.sample.mkv", 0.1),
            file("d", "Dune.1984.1080p.mkv", 8.0),
            file("e", "Dune.Part.Two.2024.1080p.mkv", 0.03), // fake: 30 MB
        ];
        let finder = StreamFinder::new(vec![Arc::new(Fake(files))], FinderConfig::default());
        let calls = std::sync::atomic::AtomicU32::new(0);
        let report = finder
            .find(&target(), &|_| {
                calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            })
            .await;

        let idents: Vec<&str> = report.streams.iter().map(|s| s.ident.as_str()).collect();
        assert_eq!(idents, vec!["b", "a"], "{:#?}", report.rejected);
        assert_eq!(report.queries, 5);
        assert!(report.errors.is_empty());
        assert!(calls.load(std::sync::atomic::Ordering::Relaxed) >= 1);
    }
}
