//! Sdílený HTTP klient.

use std::time::Duration;

/// Obecný User-Agent bez osobních údajů. Wikidata vyžaduje "popisný" UA;
/// název a verze aplikace to splňují.
pub const USER_AGENT: &str = concat!("Kuro/", env!("CARGO_PKG_VERSION"), " (Android media player)");

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .gzip(true)
        .build()
        .expect("HTTP klient se nepodařilo sestavit")
}
