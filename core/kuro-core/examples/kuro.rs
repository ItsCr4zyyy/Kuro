//! Ruční živé ověření core bez Androidu (obdoba NSC `getlink.ts` / `e2e.ts`).
//!
//!   cargo run --example kuro -- search movie "duna"
//!   cargo run --example kuro -- meta series tt0903747
//!   cargo run --example kuro -- cz tt0816692 tt0903747
//!   cargo run --example kuro -- streams movie tt15239678
//!   cargo run --example kuro -- streams series tt0903747 3 7
//!   cargo run --example kuro -- account
//!   cargo run --example kuro -- link <ident>
//!
//! Přihlášení k Webshare: proměnné WEBSHARE_USER / WEBSHARE_PASS, nebo
//! soubor `core/.env` (je v .gitignore). Bez nich se hledá anonymně.

use std::sync::Arc;

use kuro_core::finder::{FinderConfig, StreamFinder};
use kuro_core::matcher::{MatchTarget, MediaKind};
use kuro_core::meta::{Cinemeta, TitleMeta, Wikidata};
use kuro_core::providers::{LinkPurpose, Provider, Webshare};

type R<T> = Result<T, Box<dyn std::error::Error>>;

fn kind(arg: &str) -> MediaKind {
    match arg {
        "movie" | "film" => MediaKind::Movie,
        "series" | "tv" | "serial" => MediaKind::Series,
        other => panic!("neznámý typ {other:?} (movie | series)"),
    }
}

fn card(t: &TitleMeta) -> String {
    format!(
        "{:<11} {:<45} {:>4}  ★{:<4} {}{}",
        t.imdb_id,
        t.display_title(),
        t.year.map_or("?".into(), |y| y.to_string()),
        t.rating.map_or("-".into(), |r| format!("{r:.1}")),
        t.genres.join(", "),
        if t.anime { "  [anime]" } else { "" }
    )
}

async fn webshare(http: reqwest::Client, require_login: bool) -> R<Arc<Webshare>> {
    // Stabilní pro CLI, ať Webshare nevidí pokaždé "nové zařízení".
    let ws = Arc::new(Webshare::new(http, "kuro-cli-0000-0000-0000-000000000001"));
    // KURO_NO_LOGIN=1: chovat se jako uživatel bez Webshare účtu.
    let skip = std::env::var("KURO_NO_LOGIN").is_ok();
    match (std::env::var("WEBSHARE_USER"), std::env::var("WEBSHARE_PASS")) {
        (Ok(u), Ok(p)) if !u.is_empty() && !skip => {
            ws.login(&u, &p).await?;
            eprintln!("· přihlášeno k Webshare");
        }
        _ if require_login => return Err("chybí WEBSHARE_USER / WEBSHARE_PASS".into()),
        _ => eprintln!("· Webshare anonymně (bez přihlášení najde méně)"),
    }
    Ok(ws)
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

#[tokio::main]
async fn main() -> R<()> {
    let _ = dotenvy::from_filename(concat!(env!("CARGO_MANIFEST_DIR"), "/../.env"));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();

    let http = kuro_core::http::client();
    let cinemeta = Cinemeta::new(http.clone());
    let wikidata = Wikidata::new(http.clone());

    match a.as_slice() {
        ["search", k, q @ ..] => {
            for t in cinemeta.search(kind(k), &q.join(" ")).await? {
                println!("{}", card(&t));
            }
        }

        ["top", k] => {
            for t in cinemeta.catalog(kind(k), kuro_core::meta::CatalogId::Top, None, 0).await? {
                println!("{}", card(&t));
            }
        }

        ["meta", k, id] => {
            let Some(mut t) = cinemeta.meta(kind(k), id).await? else {
                return Err("Cinemeta titul nezná".into());
            };
            if let Ok(Some(loc)) = wikidata.localized_title(id).await {
                t.apply_localized(&loc);
            }
            println!("{}", card(&t));
            println!("originál: {} | aliasy: {:?} | délka: {:?} min", t.title_orig, t.aliases, t.runtime_min);
            println!("{}", t.overview.as_deref().unwrap_or(""));
            let seasons: std::collections::BTreeMap<u16, usize> =
                t.episodes.iter().fold(Default::default(), |mut m, e| {
                    *m.entry(e.season).or_default() += 1;
                    m
                });
            for (s, n) in seasons {
                println!("  sezóna {s}: {n} epizod");
            }
        }

        ["cz", ids @ ..] => {
            let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
            for (id, t) in wikidata.localized_titles(&ids).await {
                println!("{id}: cs={:?} sk={:?}", t.cs, t.sk);
            }
        }

        ["streams", k, id, rest @ ..] => {
            let k = kind(k);
            let (season, episode) = match rest {
                [s, e] => (Some(s.parse()?), Some(e.parse()?)),
                [] => (None, None),
                _ => return Err("u seriálu zadej sezónu a epizodu".into()),
            };

            let Some(mut t) = cinemeta.meta(k, id).await? else {
                return Err("Cinemeta titul nezná".into());
            };
            if let Ok(Some(loc)) = wikidata.localized_title(id).await {
                t.apply_localized(&loc);
            }
            eprintln!("· {} ({:?}), aliasy {:?}", t.display_title(), t.year, t.aliases);

            let target = MatchTarget {
                title_cs: t.display_title().to_owned(),
                title_orig: t.title_orig.clone(),
                aliases: t.aliases.clone(),
                year: t.year,
                runtime: t.runtime_min,
                kind: k,
                season,
                episode,
            };

            let ws = webshare(http.clone(), false).await?;
            let providers: Vec<Arc<dyn Provider>> = vec![
                ws,
                Arc::new(kuro_core::providers::PrehrajTo::new(http.clone())),
                Arc::new(kuro_core::providers::Hellspy::new(http.clone())),
            ];
            let finder = StreamFinder::new(providers, FinderConfig::default());
            let started = std::time::Instant::now();
            let report = finder
                .find(&target, &|partial| eprintln!("  … průběžně {} zdrojů", partial.len()))
                .await;

            println!(
                "\n{} zdrojů za {:.1} s ({} dotazů, {} souborů prošlo hledáním)\n",
                report.streams.len(),
                started.elapsed().as_secs_f64(),
                report.queries,
                report.files_seen
            );
            for s in report.streams.iter().take(20) {
                let langs = if s.lang_explicit {
                    format!("zvuk {:?} tit {:?}{}", s.audio_langs, s.sub_langs, if s.dubbed { " dabing" } else { "" })
                } else {
                    "zvuk neuveden".into()
                };
                println!(
                    "{:<9} {:>5}p {:<6} {:>8}  conf {:.2}  {:<38} {}\n        {}",
                    s.provider.as_str(),
                    s.height.map_or("?".into(), |h| h.to_string()),
                    s.hdr.map_or("", |h| h.label()),
                    gb(s.size_bytes),
                    s.confidence,
                    langs,
                    s.ident,
                    s.filename
                );
            }
            let mut rejected: Vec<_> = report.rejected.iter().collect();
            rejected.sort_by(|a, b| b.1.cmp(a.1));
            println!("\nzamítnuto:");
            for (why, n) in rejected.iter().take(10) {
                println!("  {n:>4}× {why}");
            }
            for (p, e) in &report.errors {
                println!("chyba {}: {e}", p.as_str());
            }
        }

        // Surové výsledky hledání na Webshare + jak je vidí parser a párovač.
        ["ws", q @ ..] => {
            let ws = webshare(http.clone(), false).await?;
            let opts = kuro_core::providers::SearchOptions {
                limit: 40,
                // KURO_SORT=relevance|largest|recent|rating (výchozí largest)
                sort: match std::env::var("KURO_SORT").as_deref() {
                    Ok("relevance") => kuro_core::providers::SearchSort::Relevance,
                    Ok("recent") => kuro_core::providers::SearchSort::Recent,
                    Ok("rating") => kuro_core::providers::SearchSort::Rating,
                    _ => kuro_core::providers::SearchSort::Largest,
                },
                ..Default::default()
            };
            for f in ws.search(&q.join(" "), &opts).await? {
                let p = kuro_core::matcher::parse_release(&f.name);
                println!("{:>8}  {:<70} → {:?} S{:?}E{:?}", gb(f.size_bytes), f.name, p.title_norm, p.season, p.episode);
            }
        }

        // Odkaz od zdroje bez přihlášení: link-free prehrajto|hellspy <ident>
        ["link-free", provider, ident] => {
            let p: Arc<dyn Provider> = match *provider {
                "prehrajto" => Arc::new(kuro_core::providers::PrehrajTo::new(http)),
                _ => Arc::new(kuro_core::providers::Hellspy::new(http)),
            };
            let link = p.resolve(ident, LinkPurpose::Stream).await?;
            let host = link.url.split('/').take(3).collect::<Vec<_>>().join("/");
            println!("odkaz: {host}/… (zkráceno), platnost do {:?}", link.expires_at_ms);
            for s in &link.subtitles {
                println!("titulky: {} ({})", s.lang, s.label);
            }
        }

        ["account"] => {
            let ws = webshare(http, true).await?;
            println!("{:?}", ws.account().await?);
        }

        ["link", ident] => {
            let ws = webshare(http, true).await?;
            let link = ws.resolve(ident, LinkPurpose::Stream).await?;
            println!("{}", link.url);
        }

        _ => {
            eprintln!("použití: viz hlavička examples/kuro.rs");
            std::process::exit(2);
        }
    }
    Ok(())
}
