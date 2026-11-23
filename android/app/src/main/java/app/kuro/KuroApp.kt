package app.kuro

import android.app.Application
import androidx.core.content.edit
import app.kuro.core.KuroCore
import java.util.UUID

/**
 * Drží jedinou instanci Rust core. Databáze je v soukromém úložišti
 * aplikace a nikam se nezálohuje (viz data_extraction_rules.xml).
 */
class KuroApp : Application() {

    lateinit var core: KuroCore
        private set

    override fun onCreate() {
        super.onCreate()
        val prefs = getSharedPreferences("kuro", MODE_PRIVATE)
        // Náhodné ID instalace — Webshare podle něj rozlišuje zařízení.
        // Není odvozené od hardwaru ani účtu.
        val deviceId = prefs.getString(KEY_DEVICE_ID, null)
            ?: UUID.randomUUID().toString().also { prefs.edit { putString(KEY_DEVICE_ID, it) } }

        val dbFile = getDatabasePath("kuro.db").apply { parentFile?.mkdirs() }
        core = KuroCore(dbFile.absolutePath, deviceId)
    }

    private companion object {
        const val KEY_DEVICE_ID = "device_id"
    }
}
