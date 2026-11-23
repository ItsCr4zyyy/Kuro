package app.kuro

import android.app.Application
import androidx.core.content.edit
import app.kuro.core.KuroCore
import app.kuro.data.Settings
import app.kuro.data.TokenVault
import app.kuro.work.MaintenanceWorker
import coil3.ImageLoader
import coil3.PlatformContext
import coil3.SingletonImageLoader
import coil3.disk.DiskCache
import coil3.request.crossfade
import okio.Path.Companion.toOkioPath
import java.util.UUID

/**
 * Drží jedinou instanci Rust core. Databáze je v soukromém úložišti
 * aplikace a nikam se nezálohuje (viz data_extraction_rules.xml).
 */
class KuroApp : Application(), SingletonImageLoader.Factory {

    lateinit var core: KuroCore
        private set
    lateinit var vault: TokenVault
        private set
    lateinit var settings: Settings
        private set

    override fun onCreate() {
        super.onCreate()
        settings = Settings(this)
        vault = TokenVault(this)

        val prefs = getSharedPreferences("kuro", MODE_PRIVATE)
        // Náhodné ID instalace — Webshare podle něj rozlišuje zařízení.
        // Není odvozené od hardwaru ani účtu.
        val deviceId = prefs.getString(KEY_DEVICE_ID, null)
            ?: UUID.randomUUID().toString().also { prefs.edit { putString(KEY_DEVICE_ID, it) } }

        val dbFile = getDatabasePath("kuro.db").apply { parentFile?.mkdirs() }
        core = KuroCore(dbFile.absolutePath, deviceId)
        core.setWebshareToken(vault.load(TokenVault.WEBSHARE))

        MaintenanceWorker.schedule(this)
    }

    /** Plakáty: disková cache s pevným limitem, Android ji smí kdykoli promazat. */
    override fun newImageLoader(context: PlatformContext): ImageLoader =
        ImageLoader.Builder(context)
            .diskCache {
                DiskCache.Builder()
                    .directory(cacheDir.resolve("images").toOkioPath())
                    .maxSizeBytes(Settings.IMAGE_CACHE_BYTES)
                    .build()
            }
            .crossfade(true)
            .build()

    private companion object {
        const val KEY_DEVICE_ID = "device_id"
    }
}
