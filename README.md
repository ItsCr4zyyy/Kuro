# Kuro – filmy, seriály a anime pro Android

> **Soukromý projekt, ve vývoji.** Aplikace vzniká pro vlastní potřebu a pár kamarádů. Zatím neexistuje
> žádná vydaná verze. Používání je na vlastní riziko.

Nativní Android přehrávač nad **vlastním účtem u úložiště** (Webshare, další zdroje se připravují).
Katalog z veřejných metadat (Cinemeta, Wikidata, AniList), vyhledání zdrojů, přehrávání a stahování
pro offline sledování.

**Všechno běží v zařízení.** Kuro nemá žádný vlastní server ani cloud. Historie, rozkoukané pozice,
oblíbené, cache i stažené soubory zůstávají jen v telefonu. Ven jdou jen dotazy, bez kterých to nejde:
katalog (Cinemeta, Wikidata, AniList) a přihlášení a hledání u zvoleného úložiště. Heslo se neukládá,
v zařízení zůstává jen přihlašovací token zašifrovaný přes Android Keystore.

## Stav

| Část | Stav |
|---|---|
| Rust core: parser názvů, jazyky, párování, Webshare login hash | ✅ hotovo, testy |
| Webshare, Cinemeta, Wikidata, vyhledávání zdrojů | ✅ hledání ověřeno živě, login čeká na test |
| Lokální databáze, napojení na Android (UniFFI) | ⏳ další krok |
| UI (Jetpack Compose) | — |
| Přehrávač (Media3 + FFmpeg) | — |
| Stahování offline | — |
| Přehraj.to, anime (AniList, české titulky) | — |

## Plánované funkce
- Oddělené sekce **Filmy**, **Seriály** (série → epizody) a **Anime**, hledání s diakritikou i bez.
- Detail titulu: popis, hodnocení, výběr zdroje podle kvality, kodeku, jazyka zvuku a titulků.
  Zdroje s českým dabingem a titulky mají přednost před rozlišením.
- Přehrávač: MKV, HLS, DASH, hardwarové dekódování, FFmpeg pro AC3/EAC3/DTS/TrueHD, přepínání zvukových
  stop, interní i externí titulky (SRT, ASS/SSA), gesta (jas, hlasitost, posun), navázání tam, kde jsi skončil.
- Stahování filmů a celých sérií pro offline sledování.

## Architektura

```
core/                 Rust – logika bez závislosti na Androidu (testuje se `cargo test`)
  kuro-core/
    src/text.rs         normalizace názvů, podobnost
    src/matcher/        parser názvů souborů, detekce jazyků, párování a řazení zdrojů
    src/providers/      úložiště (Webshare, …)
android/              Kotlin – Jetpack Compose UI, Media3 přehrávač, stahování (připravuje se)
```

Rust core se do aplikace dostane přes **UniFFI** a `cargo-ndk` (`arm64-v8a`, `armeabi-v7a`, `x86_64`).

## Sestavení

```bash
cd core
cargo test
```

Android část zatím neexistuje.

## Poznámky
- Kuro není spojené s Webshare, Stremiem, Wikidaty, AniListem ani jinou službou.
- Logika párování souborů vychází z projektu NSC.
- K přehrávání je potřeba vlastní účet u úložiště. Kuro nic nehostuje ani neproxuje.

## Licence

[GPL-3.0](LICENSE)
