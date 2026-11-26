# Kuro – filmy, seriály a anime pro Android

> **Soukromý projekt.** Aplikace vzniká pro vlastní potřebu a pár kamarádů. Používání je na vlastní riziko.

Kuro najde k filmu, seriálu nebo anime soubory na úložištích, seřadí je podle jazyka a kvality a přehraje
je ve vestavěném přehrávači. Bez účtu hraje ze zdrojů **Přehraj.to** a **Hellspy**, s vlastním účtem
**Webshare** i v původní kvalitě (4K, HDR, všechny zvukové stopy).

**Všechno běží v zařízení.** Kuro nemá žádný vlastní server ani cloud. Historie, rozkoukané pozice,
oblíbené, cache i stažené soubory zůstávají jen v telefonu. Ven jdou jen dotazy, bez kterých to nejde:
katalog (Cinemeta, Wikidata, AniList) a hledání u zapnutých zdrojů. Heslo k Webshare se neukládá,
v zařízení zůstává jen přihlašovací token zašifrovaný přes Android Keystore.

## Stažení

APK je v [GitHub Releases](https://github.com/ItsCr4zyyy/Kuro/releases/latest) (`kuro.apk`, Android 8+).

## Co umí

**Katalog a hledání**
- Domovská stránka se sekcemi Vše, Filmy, Seriály a Anime; řady populárních, nových a nejlépe hodnocených titulů.
- Anime z AniListu: trendy, aktuální sezóna, nejlépe hodnocené, nejoblíbenější, filmy.
- Hledání s diakritikou i bez, filtr Filmy / Seriály / Anime, české názvy z Wikidat, anime i podle japonského názvu (romaji).
- Druhy obsahu jde v nastavení zapnout a skrýt; anime je ve výchozím stavu vypnuté.
- Rozvržení i pro ohebné telefony a tablety: na rozloženém displeji velký detail s logem titulu,
  díly ve dvou sloupcích a plovoucí lišta dole.

**Zdroje**
- Přehraj.to a Hellspy bez účtu (převody do 1080p, titulky od zdroje), Webshare s účtem.
- Každý zdroj jde v nastavení zapnout a vypnout. Webshare je ve výchozím stavu vypnutý, zapne se přihlášením.
- Párování souborů podle názvu, roku, sezóny a dílu (i české a romaji názvy), kontrola délky,
  detekce dabingu a titulků. Stejné soubory nahrané víckrát se slučují.
- Zdroje, které opakovaně nejdou pustit, klesají v pořadí.

**Přehrávač** ([mpv](https://mpv.io))
- MKV, MP4, AVI (Xvid/DivX), hardwarové dekódování, zvuk AC3/EAC3/DTS/TrueHD.
- Přepínání zvukových stop a titulků, plně stylované ASS titulky pro anime, titulky ze souboru.
- Gesta: jas, hlasitost, posun, dvojklepnutí ±10 s. Obraz v obraze.
- Rychlost přehrávání (0,75–2×) a časovač vypnutí (15 min – 1,5 h nebo na konci dílu).
- Další díl: na titulcích epizody nabídka s odpočtem, pustí podobný zdroj (dabing/titulky) jako předchozí díl.
- Ovládání v notifikaci, na zamčené obrazovce a tlačítky sluchátek / Bluetooth.
- Navázání tam, kde jsi skončil. Konec filmu se pozná podle titulků (kapitola titulků nebo posledních
  8 minut), takže dokoukaný film nezůstane v „Pokračovat“.

**Knihovna**
- Pokračovat ve sledování (u seriálů nabídne další díl), rozkoukané, oblíbené.
- Stahování filmů i celých sezón na pozadí, navázání po výpadku, přehrávání bez internetu,
  titulky se stáhnou s videem. Volitelně jen přes Wi-Fi.
- Doporučení podle vkusu počítaná v telefonu (žánry, období, dabing vs. titulky).
- Ruční aktualizace z GitHub Releases v nastavení (nic se nekontroluje samo).
- Záloha a obnova do JSON souboru (oblíbené, pozice, vkus, nastavení) — pro přechod na nový telefon.
- Malá databáze, která se plní jen tím, co tě zajímá, a sama se uklízí; v nastavení „Pročistit paměť“.

**Televize**
- Přehrávání na TV v domácí síti: Google Cast (Chromecast, Google TV) a DLNA (LG, Samsung a další).
  Telefon slouží jako ovladač a ukládá pozici.

## Architektura

```
core/                   Rust – logika bez závislosti na Androidu (testuje se `cargo test`)
  kuro-core/
    src/matcher/          parser názvů souborů, detekce jazyků, párování a řazení zdrojů
    src/providers/        zdroje (Webshare, Přehraj.to, Hellspy)
    src/meta/             Cinemeta, Wikidata, AniList
    src/finder.rs         vyhledání zdrojů k titulu/epizodě
    src/recommend.rs      doporučení podle vkusu
    src/storage/          SQLite: katalog ve vrstvách, zájem, vkus, cache zdrojů, knihovna, záloha, úklid
    examples/kuro.rs      CLI pro ruční ověření
  kuro-ffi/               UniFFI fasáda (`KuroCore`) → libkuro_ffi.so + Kotlin bindingy
android/                Kotlin – Jetpack Compose UI, přehrávač mpv, stahování, televize
```

Gradle při buildu sám přeloží Rust přes `cargo-ndk` (`arm64-v8a`, `armeabi-v7a`, `x86_64`) a vygeneruje
Kotlin bindingy (`app.kuro.core`).

## Sestavení

Potřeba: Rust (`rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`),
`cargo install cargo-ndk`, Android SDK s NDK 29.0.14206865, Android Studio.

```bash
cd core && cargo test                   # logika, offline
cd android && ./gradlew assembleRelease # APK do app/build/outputs/apk/release/
```

Nebo otevři složku `android/` v Android Studiu a dej **Run**.

## Poznámky
- Přehrávač je [mpv](https://mpv.io) přes knihovnu [mpv-android-lib](https://github.com/abdallahmehiz/mpv-android) (MIT) s FFmpeg a libass.
- Přehrávání na TV přes Google Cast používá knihovnu Google Play služeb.
- Kuro není spojené s Webshare, Přehraj.to, Hellspy, Stremiem, Wikidaty, AniListem ani jinou službou
  a nic nehostuje. Logika párování souborů vychází z projektu NSC.

## Licence

[GPL-3.0](LICENSE)
