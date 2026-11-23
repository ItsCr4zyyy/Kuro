//! Úložiště: vrstvy, slábnoucí skóre, profil vkusu, cache zdrojů, úklid.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use kuro_core::finder::FoundStream;
use kuro_core::matcher::MediaKind;
use kuro_core::meta::{EpisodeMeta, LocalizedTitles, TitleMeta};
use kuro_core::providers::ProviderId;
use kuro_core::storage::{Budget, Db, Event, Target, DAY_MS, HALF_LIFE_MS};

/// Databáze s ručně posouvanými hodinami.
fn db() -> (Db, Arc<AtomicI64>) {
    let clock = Arc::new(AtomicI64::new(20_780 * DAY_MS)); // 2026-11-23
    let c = clock.clone();
    (Db::in_memory_with_clock(move || c.load(Ordering::SeqCst)).unwrap(), clock)
}

fn advance(clock: &AtomicI64, ms: i64) {
    clock.fetch_add(ms, Ordering::SeqCst);
}

fn meta(imdb: &str, name: &str, kind: MediaKind, genres: &[&str], year: u16) -> TitleMeta {
    TitleMeta {
        imdb_id: imdb.into(),
        kind,
        title_orig: name.into(),
        title_cs: None,
        aliases: vec![],
        year: Some(year),
        overview: Some(format!("{name} popis")),
        poster: None,
        background: None,
        logo: None,
        runtime_min: Some(120),
        rating: Some(7.5),
        genres: genres.iter().map(|g| g.to_string()).collect(),
        countries: vec!["United States".into()],
        anime: false,
        release_date: Some(format!("{year}-01-01")),
        popularity: 1.0,
        episodes: vec![],
    }
}

fn stream(ident: &str, audio: &[&str], height: u16) -> FoundStream {
    FoundStream {
        provider: ProviderId::Webshare,
        ident: ident.into(),
        filename: format!("{ident}.mkv"),
        size_bytes: 4_000_000_000,
        height: Some(height),
        hdr: None,
        video_codec: Some("h264".into()),
        audio_codec: None,
        channels: None,
        source: None,
        audio_langs: audio.iter().map(|a| a.to_string()).collect(),
        sub_langs: vec![],
        dubbed: false,
        dual_audio: false,
        lang_explicit: !audio.is_empty(),
        confidence: 0.9,
        positive_votes: 0,
        negative_votes: 0,
        ok_count: 0,
        fail_count: 0,
    }
}

#[test]
fn cards_do_not_overwrite_localized_fields() {
    let (db, _) = db();
    let id = db.upsert_cards(&[meta("tt0903747", "Breaking Bad", MediaKind::Series, &["Crime"], 2008)]).unwrap()[0];
    db.apply_localized(
        id,
        Some(&LocalizedTitles { imdb_id: "tt0903747".into(), cs: Some("Perníkový táta".into()), sk: None }),
    )
    .unwrap();

    // Nová karta ze seznamu (bez českého názvu) ho nesmí smazat.
    db.upsert_cards(&[meta("tt0903747", "Breaking Bad", MediaKind::Series, &["Crime"], 2008)]).unwrap();
    let t = db.title(id).unwrap().unwrap();
    assert_eq!(t.meta.title_cs.as_deref(), Some("Perníkový táta"));
    assert_eq!(t.meta.aliases, vec!["Perníkový táta".to_string()]);
    assert!(t.localized_at.is_some());
    assert_eq!(t.tier, 0);
}

#[test]
fn detail_stores_episodes() {
    let (db, _) = db();
    let mut m = meta("tt0903747", "Breaking Bad", MediaKind::Series, &["Crime"], 2008);
    m.episodes = (1..=3)
        .map(|e| EpisodeMeta { season: 1, episode: e, name: Some(format!("E{e}")), overview: None, still: None, air_date: None })
        .collect();
    let id = db.upsert_detail(&m).unwrap();
    assert_eq!(db.episodes(id).unwrap().len(), 3);
    assert!(db.title(id).unwrap().unwrap().detail_at.is_some());
}

#[test]
fn events_promote_and_decay() {
    let (db, clock) = db();
    let id = db.upsert_cards(&[meta("tt1", "A", MediaKind::Movie, &["Drama"], 2015)]).unwrap()[0];

    db.record_event(id, Event::Impression).unwrap();
    assert_eq!(db.title(id).unwrap().unwrap().tier, 0, "zobrazení nepovyšuje");

    db.record_event(id, Event::PlayStarted).unwrap();
    let t = db.title(id).unwrap().unwrap();
    assert_eq!(t.tier, 1);
    assert!((t.score - 3.1).abs() < 1e-9);

    advance(&clock, HALF_LIFE_MS);
    assert!((db.title(id).unwrap().unwrap().score - 1.55).abs() < 1e-9, "po poločase polovina");
}

#[test]
fn taste_profile_follows_events() {
    let (db, clock) = db();
    let ids = db
        .upsert_cards(&[
            meta("tt1", "Krimi", MediaKind::Movie, &["Crime", "Drama"], 2015),
            meta("tt2", "Komedie", MediaKind::Movie, &["Comedy"], 1995),
        ])
        .unwrap();

    db.record_event(ids[1], Event::Finished).unwrap();
    advance(&clock, 3 * HALF_LIFE_MS); // komedie dávno
    db.record_event(ids[0], Event::PlayStarted).unwrap();

    let genres = db.taste("genre:", 5).unwrap();
    assert_eq!(genres[0].0, "genre:Crime");
    assert!(genres.iter().any(|(f, _)| f == "genre:Comedy"));
    let decades = db.taste("decade:", 1).unwrap();
    assert_eq!(decades[0].0, "decade:2010");
}

#[test]
fn local_search_ignores_diacritics_and_prefixes() {
    let (db, _) = db();
    let id = db.upsert_cards(&[meta("tt15239678", "Dune: Part Two", MediaKind::Movie, &["Sci-Fi"], 2024)]).unwrap()[0];
    db.apply_localized(id, Some(&LocalizedTitles { imdb_id: "tt15239678".into(), cs: Some("Duna: Část druhá".into()), sk: None }))
        .unwrap();
    db.upsert_cards(&[meta("tt0903747", "Breaking Bad", MediaKind::Series, &["Crime"], 2008)]).unwrap();

    let hits = db.search_local("cast dru", None, 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, id);
    assert_eq!(db.search_local("dune", Some(MediaKind::Series), 10).unwrap().len(), 0);
    assert_eq!(db.search_local("breaking", Some(MediaKind::Series), 10).unwrap().len(), 1);
    assert!(db.search_local("  ", None, 10).unwrap().is_empty());
}

#[test]
fn stream_cache_keeps_best_and_survives_empty_search() {
    let (db, clock) = db();
    let id = db.upsert_cards(&[meta("tt1", "A", MediaKind::Movie, &["Drama"], 2015)]).unwrap()[0];
    let t = Target::movie(id);

    assert!(!db.cached_streams(t).unwrap().is_fresh);

    let many: Vec<FoundStream> = (0..30).map(|i| stream(&format!("x{i}"), &[], 720)).chain([stream("cz", &["cs"], 480)]).collect();
    db.save_streams(t, &many).unwrap();
    let cached = db.cached_streams(t).unwrap();
    assert_eq!(cached.streams.len(), 20);
    assert_eq!(cached.streams[0].ident, "cz", "český zvuk první");
    assert!(cached.is_fresh);

    // Prázdný výsledek nesmí smazat dřív nalezené.
    db.save_streams(t, &[]).unwrap();
    assert_eq!(db.cached_streams(t).unwrap().streams.len(), 20);

    // Nefunkční zdroj klesne.
    for _ in 0..3 {
        db.report_stream(ProviderId::Webshare, "cz", false).unwrap();
    }
    assert_ne!(db.cached_streams(t).unwrap().streams[0].ident, "cz");

    advance(&clock, 8 * DAY_MS);
    assert!(!db.cached_streams(t).unwrap().is_fresh, "starý film po 7 dnech");
}

#[test]
fn progress_and_continue_watching() {
    let (db, clock) = db();
    let ids = db
        .upsert_cards(&[
            meta("tt1", "Film", MediaKind::Movie, &["Drama"], 2015),
            meta("tt2", "Seriál", MediaKind::Series, &["Crime"], 2008),
        ])
        .unwrap();

    db.save_progress(Target::movie(ids[0]), 30 * 60_000, 120 * 60_000, Some((ProviderId::Webshare, "abc"))).unwrap();
    advance(&clock, 1000);
    db.save_progress(Target::episode(ids[1], 1, 1), 50 * 60_000, 50 * 60_000, None).unwrap(); // dokoukáno
    advance(&clock, 1000);
    db.save_progress(Target::episode(ids[1], 1, 2), 5 * 60_000, 50 * 60_000, None).unwrap();

    let cont = db.continue_watching(10).unwrap();
    assert_eq!(cont.len(), 2);
    assert_eq!(cont[0].title.id, ids[1]);
    assert_eq!(cont[0].progress.target.episode, 2, "u seriálu poslední epizoda");
    assert_eq!(cont[1].progress.ident.as_deref(), Some("abc"));

    let ep = db.progress(Target::episode(ids[1], 1, 1)).unwrap().unwrap();
    assert!(ep.finished);
    // Dokoukání se propsalo do skóre (Finished = 5).
    assert!(db.title(ids[1]).unwrap().unwrap().score >= 4.99);
}

#[test]
fn recent_searches_are_capped_and_deduped() {
    let (db, clock) = db();
    for i in 0..60 {
        db.add_recent_search(&format!("q{i}")).unwrap();
        advance(&clock, 1);
    }
    db.add_recent_search("q10").unwrap();
    let r = db.recent_searches(100).unwrap();
    assert_eq!(r.len(), 50);
    assert_eq!(r[0], "q10");
}

#[test]
fn prune_respects_pins_ttl_and_budget() {
    let (db, clock) = db();
    let metas: Vec<TitleMeta> =
        (0..10).map(|i| meta(&format!("tt{i}"), &format!("T{i}"), MediaKind::Movie, &["Drama"], 2010)).collect();
    let ids = db.upsert_cards(&metas).unwrap();

    // 0 = oblíbený, 1 = rozkoukaný, 2..=5 pracovní s rostoucím zájmem, 6..9 jen zobrazené.
    db.set_watchlist(ids[0], true).unwrap();
    db.save_progress(Target::movie(ids[1]), 60_000, 7_200_000, None).unwrap();
    for (n, &id) in ids[2..=5].iter().enumerate() {
        for _ in 0..=n {
            db.record_event(id, Event::Opened).unwrap();
        }
    }
    db.save_streams(Target::movie(ids[2]), &[stream("s", &["cs"], 1080)]).unwrap();

    advance(&clock, 4 * DAY_MS);
    let r = db.prune(Budget { max_titles: 5, ..Budget::default() }).unwrap();
    assert_eq!(r.expired, 4, "zobrazené po TTL");
    assert_eq!(r.evicted, 1, "6 zbylých - limit 5");
    assert_eq!(r.titles_left, 5);
    assert!(db.title(ids[2]).unwrap().is_none(), "nejslabší pracovní titul pryč");
    assert!(db.cached_streams(Target::movie(ids[2])).unwrap().streams.is_empty(), "zdroje kaskádou");
    assert!(db.title(ids[0]).unwrap().is_some() && db.title(ids[1]).unwrap().is_some());

    // Pročištění: pracovní tituly se zájmem pod hranicí pryč i bez překročení limitu.
    advance(&clock, 3 * HALF_LIFE_MS);
    let r = db.deep_clean(Budget::default()).unwrap();
    assert_eq!(r.titles_left, 2, "zůstanou jen připnuté");
}

#[test]
fn matcher_version_change_drops_cached_streams() {
    let dir = std::env::temp_dir().join(format!("kuro-mv-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("k.db");
    {
        let db = Db::open(&path).unwrap();
        let id = db.upsert_cards(&[meta("tt1", "A", MediaKind::Movie, &["Drama"], 2015)]).unwrap()[0];
        db.save_streams(Target::movie(id), &[stream("x", &["cs"], 1080)]).unwrap();
        db.report_stream(ProviderId::Webshare, "x", true).unwrap();
        // Simulace staré verze párovače.
        db.kv_set("matcher_version", "0").unwrap();
    }
    let db = Db::open(&path).unwrap();
    let id = db.title_by_imdb(MediaKind::Movie, "tt1").unwrap().unwrap().id;
    let cached = db.cached_streams(Target::movie(id)).unwrap();
    assert!(cached.streams.is_empty() && cached.searched_at.is_none(), "staré zdroje pryč");
    // Při stejné verzi se nic nemaže.
    db.save_streams(Target::movie(id), &[stream("y", &[], 720)]).unwrap();
    drop(db);
    let db = Db::open(&path).unwrap();
    assert_eq!(db.cached_streams(Target::movie(id)).unwrap().streams.len(), 1);
    drop(db);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn downloads_lifecycle_and_pinning() {
    use kuro_core::storage::DownloadState;
    let (db, clock) = db();
    let ids = db
        .upsert_cards(&[
            meta("tt1", "Film", MediaKind::Movie, &["Drama"], 2015),
            meta("tt2", "Seriál", MediaKind::Series, &["Crime"], 2008),
        ])
        .unwrap();

    let (d1, stale) = db.enqueue_download(Target::movie(ids[0]), ProviderId::Webshare, "a", "a.mkv", 1000).unwrap();
    assert!(stale.is_none());
    let (d2, _) = db.enqueue_download(Target::episode(ids[1], 1, 1), ProviderId::Webshare, "b", "b.mkv", 500).unwrap();
    assert_eq!(db.pending_downloads().unwrap().len(), 2);

    // Stejný zdroj znovu = stejné id; jiný zdroj nahradí rozpracované.
    assert_eq!(db.enqueue_download(Target::movie(ids[0]), ProviderId::Webshare, "a", "a.mkv", 1000).unwrap().0, d1);
    db.update_download(d1, DownloadState::Running, 300, None, Some("/x/a.mkv"), None).unwrap();
    let (d1b, stale) = db.enqueue_download(Target::movie(ids[0]), ProviderId::Webshare, "c", "c.mkv", 2000).unwrap();
    assert_ne!(d1b, d1);
    assert_eq!(stale.as_deref(), Some("/x/a.mkv"), "starý rozpracovaný soubor ke smazání");

    db.update_download(d2, DownloadState::Done, 500, Some(500), Some("/x/b.mkv"), None).unwrap();
    let got = db.download_for(Target::episode(ids[1], 1, 1)).unwrap().unwrap();
    assert_eq!((got.state, got.local_path.as_deref()), (DownloadState::Done, Some("/x/b.mkv")));
    // Hotové stažení se jiným zdrojem nepřepíše.
    assert_eq!(db.enqueue_download(Target::episode(ids[1], 1, 1), ProviderId::Webshare, "z", "z.mkv", 1).unwrap().0, d2);

    let all = db.downloads().unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(db.pending_downloads().unwrap().len(), 1);

    // Stažené je připnuté: pročištění ho nesmaže.
    clock.fetch_add(10 * HALF_LIFE_MS, Ordering::SeqCst);
    db.deep_clean(Budget::default()).unwrap();
    assert!(db.title(ids[1]).unwrap().is_some());

    assert_eq!(db.delete_download(d2).unwrap().as_deref(), Some("/x/b.mkv"));
    assert!(db.download_for(Target::episode(ids[1], 1, 1)).unwrap().is_none());
}
