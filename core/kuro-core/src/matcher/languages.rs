//! Rozpoznání jazyků zvuku a titulků z názvu souboru (NSC `matcher/languages.ts`).
//!
//! Naivní hledání tokenů nefunguje: "sk sub" znamená slovenské TITULKY,
//! ne slovenský zvuk. Proto průchod tokeny se stavem — jazyky se sbírají
//! do fronty a teprve následující značka (dabing / titulky) rozhodne,
//! kam patří.

use std::sync::LazyLock;

use regex::Regex;

use crate::rx::rx;
use crate::text::collapse;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DetectedLanguages {
    /// ISO 639-1, v pořadí výskytu.
    pub audio: Vec<String>,
    pub subs: Vec<String>,
    /// Explicitně označený dabing (ne jen přítomnost českého zvuku).
    pub dubbed: bool,
    /// Víc zvukových stop — pro uživatele signál, že půjde přepínat.
    pub dual_audio: bool,
    /// false = v názvu nebyla žádná zmínka o jazyku. Rozdíl proti prázdnému
    /// poli: nevíme, versus víme, že tam nic není.
    pub explicit: bool,
}

/// Token -> ISO 639-1. Klíče jsou lowercase.
fn lang_token(token: &str) -> Option<&'static str> {
    Some(match token {
        "cz" | "cs" | "cze" | "ces" | "czech" | "cesky" | "ceske" | "cestina" => "cs",
        "sk" | "svk" | "slo" | "slovak" | "slovensky" | "slovenske" | "slovencina" => "sk",
        "en" | "eng" | "english" | "anglicky" => "en",
        "de" | "ger" | "deu" | "german" | "deutsch" | "nemecky" => "de",
        "ru" | "rus" | "russian" | "rusky" => "ru",
        "pl" | "pol" | "polish" | "polsky" => "pl",
        "hu" | "hun" | "hungarian" => "hu",
        "fr" | "fre" | "fra" | "french" => "fr",
        "es" | "spa" | "spanish" => "es",
        "it" | "ita" | "italian" => "it",
        "jp" | "jpn" | "ja" | "japanese" => "ja",
        "kr" | "kor" | "korean" => "ko",
        _ => return None,
    })
}

fn is_sub_marker(t: &str) -> bool {
    matches!(t, "tit" | "tits" | "titulky" | "titule" | "sub" | "subs" | "subbed" | "subtitles")
        || t.strip_prefix("titulk")
            .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

fn is_dub_marker(t: &str) -> bool {
    matches!(
        t,
        "dab" | "dabing" | "dabbing" | "dabovano" | "dabovany" | "dabovane" | "dub" | "dubbed"
            | "rychlodabing"
    )
}

fn is_multi_marker(t: &str) -> bool {
    matches!(
        t,
        "dual" | "duálni" | "dualaudio" | "multi" | "multiaudio" | "multilang" | "multisub"
    )
}

static UNGLUE_MARKER: LazyLock<Regex> = LazyLock::new(|| {
    rx(r"\b(cz|sk|en|de|ru|pl|hu|fr|es|it)[-_.]?(tit(?:ulky)?|subs?|dab(?:ing)?|dub(?:bed)?)\b")
});
static UNGLUE_COUNT: LazyLock<Regex> = LazyLock::new(|| rx(r"\b(\d)\s*x\s*(cz|sk|en)\b"));
static SEPARATORS: LazyLock<Regex> = LazyLock::new(|| rx(r"[.,_\-\[\]()+/\\]+"));

/// Rozdělí slepené tvary: "cztit" -> "cz tit", "CZdab" -> "CZ dab", "2xCZ" -> "dual CZ".
fn unglue(s: &str) -> String {
    let s = UNGLUE_MARKER.replace_all(s, "${1} ${2}");
    UNGLUE_COUNT.replace_all(&s, "dual ${2}").into_owned()
}

/// Množina se zachovaným pořadím vložení (jako JS `Set`).
#[derive(Default)]
struct OrderedSet(Vec<String>);

impl OrderedSet {
    fn add(&mut self, v: &str) {
        if !self.has(v) {
            self.0.push(v.to_owned());
        }
    }
    fn has(&self, v: &str) -> bool {
        self.0.iter().any(|x| x == v)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Marker {
    Sub,
    Dub,
}

/// Vytáhne jazyky z technické části názvu souboru.
///
/// `tail` je část názvu ZA titulem. Hledat v celém názvu nejde —
/// "Skyfall" obsahuje "sk" a "Denní hlídka" obsahuje "de".
pub fn detect_languages(tail: &str) -> DetectedLanguages {
    let normalized = collapse(&SEPARATORS.replace_all(&unglue(tail), " ")).to_lowercase();

    let mut audio = OrderedSet::default();
    let mut subs = OrderedSet::default();
    let mut dubbed = false;
    let mut dual_audio = false;
    let mut explicit = false;

    // Jazyky čekající na značku, která řekne, jestli jsou zvuk nebo titulky.
    let mut pending: Vec<&'static str> = Vec::new();
    // Jazyky, u kterých žádná značka nepřišla.
    let mut ambiguous: Vec<&'static str> = Vec::new();
    // Značka, která zatím nemá jazyk ("dab.CZ"). Nastavuje se JEN když před
    // ní žádný jazyk nestál — jinak by "CZ EN dabing CZ SK titulky"
    // přiřadilo i SK ke zvuku.
    let mut open_marker: Option<Marker> = None;

    // Značka zůstala bez jazyka — v CZ scéně to znamená češtinu.
    let close_marker = |open: &mut Option<Marker>, audio: &mut OrderedSet, subs: &mut OrderedSet| {
        match open.take() {
            Some(Marker::Sub) => subs.add("cs"),
            Some(Marker::Dub) => audio.add("cs"),
            None => {}
        }
    };

    for token in normalized.split(' ').filter(|t| !t.is_empty()) {
        if let Some(lang) = lang_token(token) {
            // Značka stála před jazykem ("dab.CZ") — přiřadit rovnou.
            if let Some(marker) = open_marker {
                match marker {
                    Marker::Sub => subs.add(lang),
                    Marker::Dub => audio.add(lang),
                }
                explicit = true;
                continue;
            }
            pending.push(lang);
            continue;
        }

        if is_sub_marker(token) {
            // "CZ SK titulky" -> oba jazyky jsou titulky.
            open_marker = if pending.is_empty() { Some(Marker::Sub) } else { None };
            for l in pending.drain(..) {
                subs.add(l);
            }
            explicit = true;
            continue;
        }

        if is_dub_marker(token) {
            open_marker = if pending.is_empty() { Some(Marker::Dub) } else { None };
            for l in pending.drain(..) {
                audio.add(l);
            }
            dubbed = true;
            explicit = true;
            continue;
        }

        if is_multi_marker(token) {
            // "multisub" mluví o titulcích, ne o zvuku.
            if !token.contains("sub") {
                dual_audio = true;
            }
            explicit = true;
            ambiguous.append(&mut pending);
            close_marker(&mut open_marker, &mut audio, &mut subs);
            continue;
        }

        // Jakýkoli jiný token ukončuje sekvenci jazyků i platnost značky.
        ambiguous.append(&mut pending);
        close_marker(&mut open_marker, &mut audio, &mut subs);
    }
    ambiguous.append(&mut pending);
    close_marker(&mut open_marker, &mut audio, &mut subs);

    // Nezařazené jazyky beru jako zvuk — "Film 1080p CZ" znamená český zvuk.
    // Výjimka: pokud už je jazyk mezi titulky, druhá zmínka zvuk nedělá.
    for lang in ambiguous {
        if !subs.has(lang) {
            audio.add(lang);
            explicit = true;
        }
    }

    if audio.0.len() > 1 {
        dual_audio = true;
    }

    let explicit = explicit || !audio.0.is_empty() || !subs.0.is_empty();
    DetectedLanguages { audio: audio.0, subs: subs.0, dubbed, dual_audio, explicit }
}
