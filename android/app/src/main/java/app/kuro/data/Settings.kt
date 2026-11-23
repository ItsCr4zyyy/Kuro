package app.kuro.data

import android.content.Context
import androidx.core.content.edit
import app.kuro.core.Provider

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

    /** Zdroje vypnuté v nastavení (výchozí žádný — všechny se prohledávají). */
    var disabledProviders: Set<Provider>
        get() = prefs.getStringSet(KEY_DISABLED_PROVIDERS, emptySet()).orEmpty()
            .mapNotNull { n -> Provider.entries.find { it.name == n } }.toSet()
        set(v) = prefs.edit { putStringSet(KEY_DISABLED_PROVIDERS, v.map { it.name }.toSet()) }

    companion object {
        const val DEFAULT_MAX_TITLES = 3000
        val MAX_TITLES_OPTIONS = listOf(1000, 3000, 10000)
        const val IMAGE_CACHE_BYTES = 150L * 1024 * 1024
        private const val KEY_MAX_TITLES = "max_titles"
        private const val KEY_BANDWIDTH = "bandwidth_bps"
        private const val KEY_WIFI_ONLY = "download_wifi_only"
        private const val KEY_DISABLED_PROVIDERS = "disabled_providers"
    }
}
