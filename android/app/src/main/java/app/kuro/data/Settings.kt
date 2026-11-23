package app.kuro.data

import android.content.Context
import androidx.core.content.edit
import app.kuro.core.MediaKind
import app.kuro.core.Provider

/** Druh obsahu, který jde v nastavení skrýt. */
enum class ContentKind(val label: String) { MOVIES("Filmy"), SERIES("Seriály"), ANIME("Anime") }

/** Nastavení aplikace (lokálně, nikam se nesynchronizuje). */
class Settings(context: Context) {
    private val prefs = context.getSharedPreferences("kuro", Context.MODE_PRIVATE)

    /** Kolik titulů smí databáze držet (připnuté se nepočítají do mazání). */
    var maxTitles: Int
        get() = prefs.getInt(KEY_MAX_TITLES, DEFAULT_MAX_TITLES)
        set(v) = prefs.edit { putInt(KEY_MAX_TITLES, v) }

    /**
     * Odhad rychlosti připojení (bajty/s) z posledních přehrávání; 0 = neznámá.
     * Podle ní se u zdrojů ukazuje "může sekat" a takové zdroje jdou níž.
     */
    var bandwidthBps: Long
        get() = prefs.getLong(KEY_BANDWIDTH, 0L)
        set(v) = prefs.edit { putLong(KEY_BANDWIDTH, v) }

    /** Započítá nové měření (průměr se starým, ať jedno pomalé Wi-Fi nepřepíše vše). */
    fun recordBandwidth(measured: Long) {
        if (measured <= 0) return
        val old = bandwidthBps
        bandwidthBps = if (old == 0L) measured else (old + measured) / 2
    }

    /** Stahovat jen přes Wi-Fi (nebo jinou neměřenou síť). Výchozí ano. */
    var downloadWifiOnly: Boolean
        get() = prefs.getBoolean(KEY_WIFI_ONLY, true)
        set(v) = prefs.edit { putBoolean(KEY_WIFI_ONLY, v) }

    /** Zdroje vypnuté v nastavení. */
    var disabledProviders: Set<Provider>
        // Výchozí: Webshare vypnutý (potřebuje účet) — zapne se v nastavení nebo přihlášením.
        get() = prefs.getStringSet(KEY_DISABLED_PROVIDERS, setOf(Provider.WEBSHARE.name)).orEmpty()
            .mapNotNull { n -> Provider.entries.find { it.name == n } }.toSet()
        set(v) = prefs.edit { putStringSet(KEY_DISABLED_PROVIDERS, v.map { it.name }.toSet()) }

    /** Druhy obsahu, které uživatel nechce vidět (domov, hledání, doporučení). */
    var hiddenContent: Set<ContentKind>
        // Výchozí: anime skryté — kdo chce, zapne si ho v nastavení.
        get() = prefs.getStringSet(KEY_HIDDEN_CONTENT, setOf(ContentKind.ANIME.name)).orEmpty()
            .mapNotNull { n -> ContentKind.entries.find { it.name == n } }.toSet()
        set(v) = prefs.edit { putStringSet(KEY_HIDDEN_CONTENT, v.map { it.name }.toSet()) }

    /** Ukázat titul? Anime se řídí přepínačem Anime, ne Filmy/Seriály. */
    fun shows(kind: MediaKind, anime: Boolean): Boolean {
        val c = when {
            anime -> ContentKind.ANIME
            kind == MediaKind.MOVIE -> ContentKind.MOVIES
            else -> ContentKind.SERIES
        }
        return c !in hiddenContent
    }

    /** Nastavení do zálohy (bez účtu, bez měření rychlosti — to patří k zařízení). */
    fun toBackup(): Map<String, String> = mapOf(
        KEY_MAX_TITLES to maxTitles.toString(),
        KEY_WIFI_ONLY to downloadWifiOnly.toString(),
        KEY_DISABLED_PROVIDERS to disabledProviders.joinToString(",") { it.name },
        KEY_HIDDEN_CONTENT to hiddenContent.joinToString(",") { it.name },
    )

    /** Obnova ze zálohy; neznámé nebo vadné hodnoty se přeskočí. */
    fun applyBackup(map: Map<String, String>) {
        map[KEY_MAX_TITLES]?.toIntOrNull()?.takeIf { it in MAX_TITLES_OPTIONS }?.let { maxTitles = it }
        map[KEY_WIFI_ONLY]?.toBooleanStrictOrNull()?.let { downloadWifiOnly = it }
        map[KEY_HIDDEN_CONTENT]?.let { raw ->
            val hidden = raw.split(',').mapNotNull { n -> ContentKind.entries.find { it.name == n.trim() } }.toSet()
            // Všechno skryté nedává smysl.
            if (hidden.size < ContentKind.entries.size) hiddenContent = hidden
        }
        map[KEY_DISABLED_PROVIDERS]?.let { raw ->
            disabledProviders = raw.split(',').mapNotNull { n -> Provider.entries.find { it.name == n.trim() } }.toSet()
        }
    }

    companion object {
        const val DEFAULT_MAX_TITLES = 3000
        val MAX_TITLES_OPTIONS = listOf(1000, 3000, 10000)
        const val IMAGE_CACHE_BYTES = 150L * 1024 * 1024
        private const val KEY_MAX_TITLES = "max_titles"
        private const val KEY_BANDWIDTH = "bandwidth_bps"
        private const val KEY_WIFI_ONLY = "download_wifi_only"
        private const val KEY_DISABLED_PROVIDERS = "disabled_providers"
        private const val KEY_HIDDEN_CONTENT = "hidden_content"
    }
}
