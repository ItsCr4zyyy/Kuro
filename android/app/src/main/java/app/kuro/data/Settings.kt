package app.kuro.data

import android.content.Context
import androidx.core.content.edit

/** Nastavení aplikace (lokálně, nikam se nesynchronizuje). */
class Settings(context: Context) {
    private val prefs = context.getSharedPreferences("kuro", Context.MODE_PRIVATE)

    /** Kolik titulů smí databáze držet (připnuté se nepočítají do mazání). */
    var maxTitles: Int
        get() = prefs.getInt(KEY_MAX_TITLES, DEFAULT_MAX_TITLES)
        set(v) = prefs.edit { putInt(KEY_MAX_TITLES, v) }

    companion object {
        const val DEFAULT_MAX_TITLES = 3000
        val MAX_TITLES_OPTIONS = listOf(1000, 3000, 10000)
        const val IMAGE_CACHE_BYTES = 150L * 1024 * 1024
        private const val KEY_MAX_TITLES = "max_titles"
    }
}
