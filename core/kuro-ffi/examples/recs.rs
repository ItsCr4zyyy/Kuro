//! Živé ověření doporučení: simulace sledování pár titulů v dočasné DB
//! a výpis řady "Pro tebe".
//!
//!   cargo run -p kuro-ffi --example recs -- "Breaking Bad" "Ozark" "Narcos"

use kuro_ffi::{KuroCore, Provider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("kuro-recs-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let core = KuroCore::new(dir.join("k.db").to_string_lossy().into(), "kuro-recs".into())?;

    let names: Vec<String> = std::env::args().skip(1).collect();
    for name in &names {
        let Some(card) = core.search(name.clone(), None).await?.into_iter().next() else {
            eprintln!("· {name}: nenalezeno");
            continue;
        };
        let d = core.title_detail(card.id).await?;
        // "Sledoval": spuštění přehrávání 3× (vkus se posílí jako u skutečného sledování).
        for _ in 0..3 {
            core.playback_started(card.id, 1, 1, Provider::Webshare, "x".into())?;
        }
        eprintln!("· sledováno: {} ({:?}) {:?}", d.card.title, d.card.year, d.card.genres);
    }

    let started = std::time::Instant::now();
    let row = core.compute_recommendations(std::env::var("CHECK").is_ok()).await?;
    let row = core.localize(row.items.iter().map(|c| c.id).collect()).await?;
    println!("\nPro tebe ({} titulů, {:.1} s):", row.len(), started.elapsed().as_secs_f64());
    for (i, c) in row.iter().enumerate() {
        println!("{:>2}. {:<40} {:>4} {:?} ★{:?}", i + 1, c.title, c.year.unwrap_or(0), c.genres, c.rating);
    }
    drop(core);
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
