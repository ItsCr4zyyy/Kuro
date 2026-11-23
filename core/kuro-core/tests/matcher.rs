//! Testovací případy převzaté 1:1 z NSC (`parse.test.ts`, `languages.test.ts`).
//! Názvy souborů jsou reálné výsledky z Webshare.

use kuro_core::matcher::*;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/* ------------------------------ parse ------------------------------ */

#[test]
fn parse_interstellar() {
    let p = parse_release("Interstellar.2014.2160p.UHD.BluRay.x265.HDR.CZ.dabing-GROUP.mkv");
    assert_eq!(p.title, "Interstellar");
    assert_eq!(p.year, Some(2014));
    assert_eq!(p.height, Some(2160));
    assert_eq!(p.hdr, Some(Hdr::Hdr10));
    assert_eq!(p.video_codec, Some("h265"));
    assert_eq!(p.source, Some("bluray"));
    assert!(p.dubbed);
    assert_eq!(p.audio_langs, s(&["cs"]));
    assert_eq!(p.reject, None);
    assert_eq!(p.group.as_deref(), Some("GROUP"));
}

#[test]
fn parse_lotr_czech_name() {
    let p = parse_release("Pan.prstenu.Dve.veze.2002.EXTENDED.1080p.BluRay.x264.CZ.EN-ABC.mkv");
    assert_eq!(p.title, "Pan prstenu Dve veze");
    assert_eq!(p.year, Some(2002));
    assert_eq!(p.height, Some(1080));
    assert_eq!(p.video_codec, Some("h264"));
}

#[test]
fn parse_episode_sxxexx() {
    let p = parse_release("Breaking.Bad.S03E07.1080p.BluRay.x264.CZtit.mkv");
    assert_eq!(p.title, "Breaking Bad");
    assert_eq!((p.season, p.episode), (Some(3), Some(7)));
    assert_eq!(p.height, Some(1080));
    assert_eq!(p.sub_langs, s(&["cs"]));
}

#[test]
fn parse_episode_nxnn() {
    let p = parse_release("Sherlock 1x02 The Blind Banker 720p HDTV CZ dabing.avi");
    assert_eq!(p.title, "Sherlock");
    assert_eq!((p.season, p.episode), (Some(1), Some(2)));
    assert_eq!(p.height, Some(720));
    assert!(p.dubbed);
}

#[test]
fn parse_dune_webdl() {
    let p = parse_release("Duna.Cast.druha.2024.1080p.WEB-DL.DDP5.1.Atmos.H.264-CZ.mkv");
    assert_eq!(p.title, "Duna Cast druha");
    assert_eq!(p.year, Some(2024));
    assert_eq!(p.height, Some(1080));
    assert_eq!(p.source, Some("webdl"));
}

#[test]
fn parse_matrix_audio() {
    let p = parse_release("The.Matrix.1999.REMASTERED.1080p.BluRay.DTS-HD.MA.5.1.x264.mkv");
    assert_eq!(p.title, "The Matrix");
    assert_eq!(p.year, Some(1999));
    assert_eq!(p.audio_codec, Some("dtshd"));
    assert_eq!(p.channels, Some("5.1"));
}

#[test]
fn parse_rejects() {
    assert_eq!(
        parse_release("Avatar.2009.sample.720p.mkv").reject.as_deref(),
        Some("sample/trailer/bonus")
    );
    assert_eq!(
        parse_release("Nejaky.Film.2020.1080p.rar").reject.as_deref(),
        Some("nepodporovaná přípona .rar")
    );
    assert_eq!(
        parse_release("Friends.S05E03-E04.720p.WEB-DL.mkv").reject.as_deref(),
        Some("více epizod v jednom souboru")
    );
}

#[test]
fn parse_skyfall_is_not_slovak() {
    let p = parse_release("Skyfall.2012.1080p.BluRay.x264.YIFY.mp4");
    assert_eq!(p.title, "Skyfall");
    assert_eq!(p.year, Some(2012));
    assert!(p.audio_langs.is_empty());
}

#[test]
fn parse_misc() {
    let p = parse_release("Rocky.III.1982.1080p.BluRay.x264.mkv");
    assert_eq!((p.title.as_str(), p.year), ("Rocky III", Some(1982)));

    let p = parse_release("Jak.vycvicit.draka.2010.CZ.dabing.720p.BRRip.XviD.avi");
    assert_eq!(p.title, "Jak vycvicit draka");
    assert_eq!(p.year, Some(2010));
    assert_eq!(p.height, Some(720));
    assert!(p.dubbed);
    assert_eq!(p.video_codec, Some("xvid"));

    let p = parse_release("Oppenheimer (2023) [2160p] [DV] [TrueHD Atmos] CZ tit.mkv");
    assert_eq!(p.year, Some(2023));
    assert_eq!(p.height, Some(2160));
    assert_eq!(p.hdr, Some(Hdr::DolbyVision));
    assert_eq!(p.sub_langs, s(&["cs"]));

    assert_eq!(parse_release("Nejaky.Novy.Film.2024.HDCAM.720p.mkv").source, Some("cam"));
}

#[test]
fn parse_season_only() {
    let p = parse_release("The.Office.S02.Complete.1080p.mkv");
    assert_eq!((p.season, p.episode), (Some(2), None));
    assert_eq!(p.title, "The Office");
}

/* ------------------------------ score ------------------------------ */

const MIN_CONFIDENCE: f64 = 0.55;

fn interstellar() -> MatchTarget {
    MatchTarget {
        title_cs: "Interstellar".into(),
        title_orig: "Interstellar".into(),
        aliases: vec![],
        year: Some(2014),
        runtime: Some(169),
        kind: MediaKind::Movie,
        season: None,
        episode: None,
    }
}

fn dune() -> MatchTarget {
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

fn breaking_bad_s03e07() -> MatchTarget {
    MatchTarget {
        title_cs: "Perníkový táta".into(),
        title_orig: "Breaking Bad".into(),
        aliases: vec![],
        year: Some(2008),
        runtime: Some(47),
        kind: MediaKind::Series,
        season: Some(3),
        episode: Some(7),
    }
}

#[test]
fn score_cases() {
    let cases: Vec<(&str, MatchTarget, bool, &str)> = vec![
        ("Interstellar.2014.1080p.BluRay.x264.CZ.mkv", interstellar(), true, "přesná shoda"),
        ("Interstellar.Documentary.2015.1080p.mkv", interstellar(), false, "jiný rok"),
        ("Interstelar.2014.1080p.BluRay.mkv", interstellar(), true, "překlep v názvu"),
        ("Interceptor.2022.1080p.WEB.mkv", interstellar(), false, "podobný název"),
        ("Dune.Part.Two.2024.2160p.mkv", dune(), true, "originální název"),
        ("Duna.Cast.druha.2024.1080p.CZ.dabing.mkv", dune(), true, "český bez diakritiky"),
        ("Breaking.Bad.S03E07.1080p.mkv", breaking_bad_s03e07(), true, "správná epizoda"),
        ("Breaking.Bad.S03E08.1080p.mkv", breaking_bad_s03e07(), false, "jiná epizoda"),
    ];

    let mut failures = vec![];
    for (file, target, should_accept, note) in cases {
        let res = score_match(&parse_release(file), &target);
        let accepted = res.rejected.is_none() && res.confidence >= MIN_CONFIDENCE;
        if accepted != should_accept {
            failures.push(format!("{note}: {file} -> {res:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn size_sanity_cases() {
    let gb = 1_000_000_000.0;
    for (size_gb, runtime, height, should_pass) in [
        (12.0, 169, 2160, true),
        (2.5, 169, 1080, true),
        (0.04, 169, 1080, false),
        (0.3, 45, 720, true),
    ] {
        let (factor, reason) = size_sanity((size_gb * gb) as u64, Some(runtime), Some(height));
        assert_eq!(factor > 0.5, should_pass, "{size_gb} GB / {height}p / {runtime} min: {reason}");
    }
}

#[test]
fn sort_prefers_czech_audio_over_resolution() {
    let cs = s(&["cs"]);
    let none: Vec<String> = vec![];
    let dub_720 = SortInput { audio_langs: &cs, height: Some(720), ..Default::default() };
    let orig_4k = SortInput { audio_langs: &none, height: Some(2160), hdr: true, ..Default::default() };
    assert!(stream_sort_key(&dub_720) > stream_sort_key(&orig_4k));
}

/* ---------------------------- languages ---------------------------- */

struct LangCase {
    file: &'static str,
    audio: Option<&'static [&'static str]>,
    subs: Option<&'static [&'static str]>,
    dubbed: Option<bool>,
    dual: Option<bool>,
    explicit: Option<bool>,
    note: &'static str,
}

const fn lc(file: &'static str, note: &'static str) -> LangCase {
    LangCase { file, audio: None, subs: None, dubbed: None, dual: None, explicit: None, note }
}

#[test]
fn languages_real_names() {
    let cases = [
        LangCase { audio: Some(&[]), subs: Some(&["sk"]), dubbed: Some(false),
            ..lc("Queen Of Hearts 2019 sk sub 1080p BluRay H264 .mkv", "sk sub = titulky") },
        LangCase { audio: Some(&[]), subs: Some(&["cs"]),
            ..lc("Mortal.Kombat.2021.1080p.cz.sub.WEB-DL.x265.mkv", "cz sub") },
        LangCase { audio: Some(&[]), subs: Some(&["cs"]),
            ..lc("Dragon Ball Super 71 cz tit.mkv", "cz tit") },
        LangCase { audio: Some(&[]), subs: Some(&["cs"]),
            ..lc("Breaking.Bad.S03E07.1080p.BluRay.x264.CZtit.mkv", "slepené CZtit") },
        LangCase { audio: Some(&["cs"]), subs: Some(&[]), dubbed: Some(true),
            ..lc("Interstellar.2014.2160p.BluRay.x265.CZ.dabing-GROUP.mkv", "CZ dabing") },
        LangCase { audio: Some(&["cs"]), dubbed: Some(false),
            ..lc("Jeepers Creepers (2001) 1080p, cz.mp4", "holé cz = zvuk") },
        LangCase { audio: Some(&["cs", "en"]), subs: Some(&["cs", "sk"]), dubbed: Some(true), dual: Some(true),
            ..lc("Supergirl (2026) [1080p] [CZ EN Dabing] [CZ SK Titulky].mkv", "audio + titulky") },
        LangCase { audio: Some(&["cs", "en"]), dubbed: Some(true), dual: Some(true),
            ..lc("Predátor (1987) CZ.EN rychlodabing - 2160p HDR.mkv", "rychlodabing") },
        LangCase { audio: Some(&["cs", "sk"]), dubbed: Some(true), dual: Some(true),
            ..lc("Chappie (2015) 1080p cz,sk dab.mkv", "cz,sk dab") },
        LangCase { subs: Some(&["cs"]), dual: Some(true),
            ..lc("The.Trip.2021.1080p.NF.WEB-DL.DUAL.DDP5.1.x264-TEPES (cz tit).mkv", "DUAL") },
        LangCase { audio: Some(&["en", "ja"]), dual: Some(true),
            ..lc("Pluto 07 WebRip 1080p x265 EN JP.mkv", "dvojjazyčné anime") },
        LangCase { dual: Some(true),
            ..lc("Nobody 2021 MULTi VF2 1080p HDLight AC3 H264-LiHDL.mkv", "MULTi") },
        LangCase { audio: Some(&[]), subs: Some(&[]), explicit: Some(false),
            ..lc("The.Black.Phone.2021.WEBRip.x264-ION10.mp4", "bez zmínky o jazyku") },
        LangCase { audio: Some(&[]), explicit: Some(false),
            ..lc("Skyfall.2012.1080p.BluRay.x264.YIFY.mp4", "Skyfall") },
        LangCase { audio: Some(&[]), explicit: Some(false),
            ..lc("Army.Of.Darkness.DC.1992.DVDRiP.XViD-LiViDiTY.avi", "DC/DVDRiP") },
        LangCase { audio: Some(&["cs"]), dubbed: Some(true),
            ..lc("Star Wars Mandalorian a Grogu dab.CZ.mkv", "dab před jazykem") },
        LangCase { audio: Some(&["cs"]), dubbed: Some(true), dual: Some(true),
            ..lc("Film.2020.1080p.2xCZ.dabing.mkv", "2xCZ") },
        LangCase { audio: Some(&[]), subs: Some(&["cs"]),
            ..lc("Neco.2019.1080p.CZ.titulky.mkv", "CZ titulky") },
    ];

    let mut failures = vec![];
    for c in &cases {
        let p = parse_release(c.file);
        let mut problems = vec![];
        if let Some(a) = c.audio {
            if sorted(p.audio_langs.clone()) != sorted(s(a)) {
                problems.push(format!("audio {:?} != {a:?}", p.audio_langs));
            }
        }
        if let Some(x) = c.subs {
            if sorted(p.sub_langs.clone()) != sorted(s(x)) {
                problems.push(format!("subs {:?} != {x:?}", p.sub_langs));
            }
        }
        if c.dubbed.is_some_and(|d| d != p.dubbed) {
            problems.push(format!("dubbed {}", p.dubbed));
        }
        if c.dual.is_some_and(|d| d != p.dual_audio) {
            problems.push(format!("dual {}", p.dual_audio));
        }
        if c.explicit.is_some_and(|e| e != p.lang_explicit) {
            problems.push(format!("explicit {}", p.lang_explicit));
        }
        if !problems.is_empty() {
            failures.push(format!("{} ({}): {}", c.note, c.file, problems.join(", ")));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn languages_direct() {
    let d = detect_languages("cz dabing");
    assert_eq!((d.audio, d.dubbed), (s(&["cs"]), true));

    let d = detect_languages("dabing");
    assert_eq!((d.audio, d.dubbed), (s(&["cs"]), true), "samotné dabing = české");

    assert_eq!(detect_languages("titulky").subs, s(&["cs"]));

    let d = detect_languages("cz en dabing cz sk titulky");
    assert_eq!(sorted(d.audio), s(&["cs", "en"]));
    assert_eq!(sorted(d.subs), s(&["cs", "sk"]));

    let d = detect_languages("1080p x265");
    assert!(d.audio.is_empty() && d.subs.is_empty() && !d.explicit);
}

/* -------------------- opravy z živých dat (Kuro) -------------------- */

#[test]
fn other_part_of_series_is_rejected() {
    // Živě: "Duna - Part One" prošla k Části druhé s confidence 0.68.
    let res = score_match(&parse_release("Duna - Part One CZ (4KHDR).mkv"), &dune());
    assert_eq!(res.rejected.as_deref(), Some("jiný díl série"), "{res:?}");

    // Stejný díl dál prochází, i česky slovy i číslem.
    for f in ["Duna - Part Two CZ (4KHDR).mkv", "Duna.Cast.druha.2024.1080p.mkv", "Duna 2 (2024) CZ.mkv"] {
        let res = score_match(&parse_release(f), &dune());
        assert!(res.rejected.is_none() && res.confidence >= MIN_CONFIDENCE, "{f}: {res:?}");
    }

    // Titul bez čísla dílu se číslem v souboru nenechá zmást (rozhoduje rok).
    let res = score_match(&parse_release("Interstellar.2014.Part.1.1080p.mkv"), &interstellar());
    assert_ne!(res.rejected.as_deref(), Some("jiný díl série"));
}

#[test]
fn glued_4khdr() {
    let p = parse_release("Duna - Part Two CZ (4KHDR).mkv");
    assert_eq!(p.height, Some(2160));
    assert_eq!(p.hdr, Some(Hdr::Hdr10));
}

#[test]
fn cs_is_czech() {
    let p = parse_release("Dune.Part.Two.2024.1080p.CS.dabing.mkv");
    assert_eq!(p.audio_langs, s(&["cs"]));
}

#[test]
fn subtitle_only_file_names_match_series() {
    // Živě: soubory "The Rings of Power S03E01 …" k titulu
    // "The Lord of the Rings: The Rings of Power" (2022), epizody z roku 2026.
    let target = MatchTarget {
        title_cs: "Pán prstenů: Prsteny moci".into(),
        title_orig: "The Lord of the Rings: The Rings of Power".into(),
        aliases: vec![],
        year: Some(2022),
        runtime: Some(67),
        kind: MediaKind::Series,
        season: Some(3),
        episode: Some(1),
    };
    for f in [
        "The Rings of Power S03E01 Marnûkh (2026) CZ.avi",
        "The.Lord.of.the.Rings.The.Rings.of.Power.S03E01.Marnukh.AMZN.SDR.WEB-DL.2160p.by.AKTEP.mkv",
        "Prsteny moci S03E01 CZ.mkv",
    ] {
        let res = score_match(&parse_release(f), &target);
        assert!(res.rejected.is_none() && res.confidence >= MIN_CONFIDENCE, "{f}: {res:?}");
    }
    // Jiná epizoda dál neprojde.
    assert!(score_match(&parse_release("The Rings of Power S03E02 (2026) CZ.avi"), &target).rejected.is_some());
}

#[test]
fn short_subtitles_are_not_candidates() {
    // "Part Two" ani "Impossible" nesmí samy o sobě matchovat.
    let mi = MatchTarget {
        title_cs: "Mission: Impossible".into(),
        title_orig: "Mission: Impossible".into(),
        aliases: vec![],
        year: Some(1996),
        runtime: Some(110),
        kind: MediaKind::Movie,
        season: None,
        episode: None,
    };
    assert!(score_match(&parse_release("Impossible.1996.1080p.mkv"), &mi).rejected.is_some());
    assert!(score_match(&parse_release("Part.Two.2024.1080p.mkv"), &dune()).rejected.is_some());
}

#[test]
fn movie_year_mismatch_still_rejects() {
    let res = score_match(&parse_release("Interstellar.2019.1080p.mkv"), &interstellar());
    assert!(res.rejected.as_deref().is_some_and(|r| r.starts_with("rok nesedí")));
}

#[test]
fn short_titles_need_close_match() {
    // Živě: "Our Boys S01E01" prošlo jako "The Boys" (podobnost 0.63).
    let boys = MatchTarget {
        title_cs: "The Boys".into(),
        title_orig: "The Boys".into(),
        aliases: vec![],
        year: Some(2019),
        runtime: Some(60),
        kind: MediaKind::Series,
        season: Some(1),
        episode: Some(1),
    };
    assert!(score_match(&parse_release("Our Boys S01E01 Out of the Depth.mkv"), &boys).rejected.is_some());

    let ok = score_match(&parse_release("The.Boys.2019.S01E01.1080p.WEBRip.x265.mkv"), &boys);
    assert!(ok.rejected.is_none() && ok.confidence >= MIN_CONFIDENCE, "{ok:?}");
    let ok = score_match(&parse_release("The Boys S01E01 CZ dabing.mkv"), &boys);
    assert!(ok.rejected.is_none(), "{ok:?}");
}

#[test]
fn episode_of_total_is_not_a_range() {
    // Živě: "Banda S01E01-08 … Karty jsou rozdaný" = díl 1 z 8, 1,1 GB.
    let p = parse_release("Banda S01E01-08 2019 CZ dabing 1080p FHD - Karty jsou rozdaný.mkv");
    assert_eq!((p.season, p.episode, p.episode_to), (Some(1), Some(1), None));
    assert_eq!(p.reject, None);
    assert_eq!(p.title, "Banda");

    // Skutečné rozsahy dál pryč.
    assert!(parse_release("Friends.S05E03-E04.720p.WEB-DL.mkv").reject.is_some());
    assert!(parse_release("Show.S01E01E02.720p.mkv").reject.is_some());
    assert!(parse_release("Show.S01E01-02.720p.mkv").reject.is_some());

    let boys = MatchTarget {
        title_cs: "Banda".into(),
        title_orig: "The Boys".into(),
        aliases: vec!["Banda".into()],
        year: Some(2019),
        runtime: Some(60),
        kind: MediaKind::Series,
        season: Some(1),
        episode: Some(1),
    };
    let res = score_match(&p, &boys);
    assert!(res.rejected.is_none() && res.confidence >= MIN_CONFIDENCE, "{res:?}");
}
