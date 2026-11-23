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
| Lokální databáze (malá, plní se podle zájmu), napojení na Android (UniFFI) | ✅ hotovo, testy |
| UI (Jetpack Compose) | ⏳ další krok |
| Přehrávač (mpv) | — |
| Doporučení podle vkusu (na zařízení) | — |
| Stahování offline | — |
| Přehraj.to, anime (AniList, české titulky) | — |

## Plánované funkce
- Oddělené sekce **Filmy**, **Seriály** (série → epizody) a **Anime**, hledání s diakritikou i bez.
- Detail titulu: popis, hodnocení, výběr zdroje podle kvality, kodeku, jazyka zvuku a titulků.
  Zdroje s českým dabingem a titulky mají přednost před rozlišením.
- Přehrávač postavený na **mpv**: MKV, MP4, AVI (Xvid/DivX), HLS, hardwarové dekódování, zvuk AC3/EAC3/DTS/TrueHD,
  přepínání zvukových stop, interní i externí titulky včetně plně stylovaných ASS pro anime, gesta (jas,
  hlasitost, posun), navázání tam, kde jsi skončil.
- Malá databáze, která se plní jen tím, co tě zajímá, a sama se uklízí. V nastavení „Pročistit paměť“.
- Doporučení podle vkusu počítaná v telefonu (žánry, období, dabing vs. titulky), bez sdílení dat.
- Stahování filmů a celých sérií pro offline sledování.

## Architektura

```
core/                   Rust – logika bez závislosti na Androidu (testuje se `cargo test`)
  kuro-core/
    src/matcher/          parser názvů souborů, detekce jazyků, párování a řazení zdrojů
    src/providers/        úložiště (Webshare, …)
    src/meta/             Cinemeta, Wikidata
    src/finder.rs         vyhledání zdrojů k titulu/epizodě
    src/storage/          SQLite: katalog ve vrstvách, zájem, vkus, cache zdrojů, knihovna, úklid
    examples/kuro.rs      CLI pro ruční ověření
  kuro-ffi/               UniFFI fasáda (`KuroCore`) → libkuro_ffi.so + Kotlin bindingy
android/                Kotlin – Jetpack Compose UI, přehrávač, stahování
```

Gradle při buildu sám přeloží Rust přes `cargo-ndk` (`arm64-v8a`, `armeabi-v7a`, `x86_64`) a vygeneruje
Kotlin bindingy (`app.kuro.core`).

## Sestavení

Potřeba: Rust (`rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`),
`cargo install cargo-ndk`, Android SDK s NDK 29.0.14206865, Android Studio.

```bash
cd core && cargo test                 # logika, offline
cd android && ./gradlew assembleDebug # APK (Rust se přeloží automaticky)
```

Nebo otevři složku `android/` v Android Studiu a dej **Run**.

## Poznámky
- Kuro není spojené s Webshare, Stremiem, Wikidaty, AniListem ani jinou službou.
- Logika párování souborů vychází z projektu NSC.
- K přehrávání je potřeba vlastní účet u úložiště. Kuro nic nehostuje ani neproxuje.

## Licence

[GPL-3.0](LICENSE)
