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

/// Dnešní datum (UTC) jako "YYYY-MM-DD" — na porovnání s daty vysílání.
pub fn today_iso() -> String {
    iso_date(now_ms())
}

/// Datum z milisekund od epochy (algoritmus "civil from days").
pub fn iso_date(ms: i64) -> String {
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod date_tests {
    #[test]
    fn iso_dates() {
        assert_eq!(super::iso_date(0), "1970-01-01");
        assert_eq!(super::iso_date(1_795_465_474_701), "2026-11-23");
        assert_eq!(super::iso_date(951_782_400_000), "2000-02-29");
    }
}
