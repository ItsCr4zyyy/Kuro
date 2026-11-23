//! Čas bez závislosti na chrono — stačí unixové milisekundy a aktuální rok.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Aktuální rok. Průměrná délka gregoriánského roku; na rozhodování
/// o roce v názvu souboru nebo katalogu stačí.
pub fn current_year() -> u16 {
    (1970 + now_ms() / 1000 / 31_556_952) as u16
}
