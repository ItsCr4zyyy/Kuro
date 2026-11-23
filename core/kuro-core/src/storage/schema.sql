-- Kuro — lokální databáze. Jen v zařízení, nikam se nesynchronizuje.
--
-- Databáze je záměrně malá: katalog žije online (Cinemeta) a sem se
-- ukládá jen to, na co uživatel sáhl. Vrstvy titulů:
--   připnuté  = má záznam ve watchlist / progress / download (nikdy se nemaže)
--   pracovní  = tier 1, otevřený/kliknutý; maže se podle slábnoucího skóre
--   krátkodobé = tier 0, jen viděný v seznamu; maže se po pár dnech

CREATE TABLE title (
    id              INTEGER PRIMARY KEY,
    kind            TEXT    NOT NULL CHECK (kind IN ('movie', 'series')),
    imdb_id         TEXT    NOT NULL,
    title_orig      TEXT    NOT NULL,
    title_cs        TEXT,
    aliases         TEXT    NOT NULL DEFAULT '[]',   -- JSON pole
    year            INTEGER,
    overview        TEXT,
    poster          TEXT,
    background      TEXT,
    logo            TEXT,
    runtime_min     INTEGER,
    rating          REAL,
    genres          TEXT    NOT NULL DEFAULT '[]',   -- JSON pole
    countries       TEXT    NOT NULL DEFAULT '[]',   -- JSON pole
    anime           INTEGER NOT NULL DEFAULT 0,
    release_date    TEXT,
    popularity      REAL    NOT NULL DEFAULT 0,

    -- Kdy se stáhl plný detail (epizody); NULL = známe jen kartu ze seznamu.
    detail_at       INTEGER,
    -- Kdy se ptalo Wikidat; NULL = nikdy (i prázdná odpověď se zapíše).
    localized_at    INTEGER,

    tier            INTEGER NOT NULL DEFAULT 0,
    -- Slábnoucí skóre zájmu: skutečná hodnota = score * 0.5^((now - score_at) / poločas).
    score           REAL    NOT NULL DEFAULT 0,
    score_at        INTEGER NOT NULL DEFAULT 0,
    -- Poslední kontakt (i jen zobrazení v seznamu) — pro TTL krátkodobé vrstvy.
    seen_at         INTEGER NOT NULL DEFAULT 0,

    UNIQUE (kind, imdb_id)
);

CREATE TABLE episode (
    title_id    INTEGER NOT NULL REFERENCES title(id) ON DELETE CASCADE,
    season      INTEGER NOT NULL,
    episode     INTEGER NOT NULL,
    name        TEXT,
    overview    TEXT,
    still       TEXT,
    air_date    TEXT,
    PRIMARY KEY (title_id, season, episode)
) WITHOUT ROWID;

-- Cache nalezených zdrojů. Film má season = episode = 0.
CREATE TABLE stream (
    title_id        INTEGER NOT NULL REFERENCES title(id) ON DELETE CASCADE,
    season          INTEGER NOT NULL,
    episode         INTEGER NOT NULL,
    provider        TEXT    NOT NULL,
    ident           TEXT    NOT NULL,
    filename        TEXT    NOT NULL,
    size_bytes      INTEGER NOT NULL,
    height          INTEGER,
    hdr             TEXT,
    video_codec     TEXT,
    audio_codec     TEXT,
    channels        TEXT,
    source          TEXT,
    audio_langs     TEXT    NOT NULL DEFAULT '[]',
    sub_langs       TEXT    NOT NULL DEFAULT '[]',
    dubbed          INTEGER NOT NULL DEFAULT 0,
    dual_audio      INTEGER NOT NULL DEFAULT 0,
    lang_explicit   INTEGER NOT NULL DEFAULT 0,
    confidence      REAL    NOT NULL,
    positive_votes  INTEGER NOT NULL DEFAULT 0,
    negative_votes  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (title_id, season, episode, provider, ident)
) WITHOUT ROWID;

-- Kdy se k cíli naposledy hledaly zdroje (i když se nic nenašlo).
CREATE TABLE stream_search (
    title_id    INTEGER NOT NULL REFERENCES title(id) ON DELETE CASCADE,
    season      INTEGER NOT NULL,
    episode     INTEGER NOT NULL,
    searched_at INTEGER NOT NULL,
    PRIMARY KEY (title_id, season, episode)
) WITHOUT ROWID;

-- Zpětná vazba z přehrávání. Trvalá a malá — zdroje, které nejdou
-- pustit, klesají v řazení i po novém hledání.
CREATE TABLE stream_stats (
    provider    TEXT    NOT NULL,
    ident       TEXT    NOT NULL,
    ok_count    INTEGER NOT NULL DEFAULT 0,
    fail_count  INTEGER NOT NULL DEFAULT 0,
    last_at     INTEGER NOT NULL,
    PRIMARY KEY (provider, ident)
) WITHOUT ROWID;

/* ------------------------------ knihovna ------------------------------ */

CREATE TABLE watchlist (
    title_id    INTEGER PRIMARY KEY REFERENCES title(id) ON DELETE CASCADE,
    added_at    INTEGER NOT NULL
);

-- Rozkoukané pozice a zároveň historie. Film má season = episode = 0.
CREATE TABLE progress (
    title_id    INTEGER NOT NULL REFERENCES title(id) ON DELETE CASCADE,
    season      INTEGER NOT NULL,
    episode     INTEGER NOT NULL,
    position_ms INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    finished    INTEGER NOT NULL DEFAULT 0,
    provider    TEXT,
    ident       TEXT,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (title_id, season, episode)
) WITHOUT ROWID;
CREATE INDEX progress_recent ON progress (updated_at DESC);

CREATE TABLE download (
    id          INTEGER PRIMARY KEY,
    title_id    INTEGER NOT NULL REFERENCES title(id) ON DELETE CASCADE,
    season      INTEGER NOT NULL,
    episode     INTEGER NOT NULL,
    provider    TEXT    NOT NULL,
    ident       TEXT    NOT NULL,
    filename    TEXT    NOT NULL,
    local_path  TEXT,
    state       TEXT    NOT NULL DEFAULT 'queued',  -- queued|running|paused|done|failed
    bytes_done  INTEGER NOT NULL DEFAULT 0,
    bytes_total INTEGER NOT NULL DEFAULT 0,
    error       TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    UNIQUE (title_id, season, episode, provider, ident)
);

/* ------------------------------ vkus a doporučení ------------------------------ */

-- Profil vkusu: "genre:Drama", "decade:2010", "country:Japan", "kind:anime",
-- "lang:cs-dub" … se slábnoucím skóre stejně jako u titulů.
CREATE TABLE taste (
    feature     TEXT PRIMARY KEY,
    score       REAL    NOT NULL,
    score_at    INTEGER NOT NULL
) WITHOUT ROWID;

-- Předpočítané řady doporučení ("for_you", "new_in_genres", …).
CREATE TABLE rec_row (
    row_id      TEXT    NOT NULL,
    position    INTEGER NOT NULL,
    title_id    INTEGER NOT NULL REFERENCES title(id) ON DELETE CASCADE,
    score       REAL    NOT NULL,
    computed_at INTEGER NOT NULL,
    PRIMARY KEY (row_id, position)
) WITHOUT ROWID;

CREATE TABLE search_recent (
    query       TEXT PRIMARY KEY,
    at          INTEGER NOT NULL
) WITHOUT ROWID;

CREATE TABLE kv (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL
) WITHOUT ROWID;

/* ------------------------------ fulltext ------------------------------ */

-- Diakritika je jedno ("mesic" najde "Měsíc"), prefixy pro napovídání.
CREATE VIRTUAL TABLE title_fts USING fts5(
    title_cs, title_orig, aliases,
    content='title', content_rowid='id',
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER title_ai AFTER INSERT ON title BEGIN
    INSERT INTO title_fts (rowid, title_cs, title_orig, aliases)
    VALUES (new.id, new.title_cs, new.title_orig, new.aliases);
END;
CREATE TRIGGER title_ad AFTER DELETE ON title BEGIN
    INSERT INTO title_fts (title_fts, rowid, title_cs, title_orig, aliases)
    VALUES ('delete', old.id, old.title_cs, old.title_orig, old.aliases);
END;
CREATE TRIGGER title_au AFTER UPDATE OF title_cs, title_orig, aliases ON title BEGIN
    INSERT INTO title_fts (title_fts, rowid, title_cs, title_orig, aliases)
    VALUES ('delete', old.id, old.title_cs, old.title_orig, old.aliases);
    INSERT INTO title_fts (rowid, title_cs, title_orig, aliases)
    VALUES (new.id, new.title_cs, new.title_orig, new.aliases);
END;
