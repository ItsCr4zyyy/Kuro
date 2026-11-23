package app.kuro

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.SystemBarStyle
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import app.kuro.ui.KuroRoot
import app.kuro.ui.theme.KuroTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Kuro je vždy tmavé → světlé ikony stavového řádku i přes obrázky.
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
        setContent { KuroTheme { KuroRoot() } }
        debugPlay(intent)
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        debugPlay(intent)
    }

    /**
     * Jen debug build: přehrávač s libovolnou URL bez přihlášení, pro test
     * přes adb:  adb shell am start -n app.kuro.debug/app.kuro.MainActivity
     *            --es kuro_debug_play_url <url>
     */
    private fun debugPlay(intent: android.content.Intent) {
        if (!BuildConfig.DEBUG) return
        intent.getStringExtra("kuro_debug_download_url")?.let { url ->
            // Stáhnout URL k prvnímu populárnímu filmu (jen test workeru).
            val core = (application as KuroApp).core
            lifecycleScope.launch {
                val card = core.catalog(app.kuro.core.MediaKind.MOVIE, app.kuro.core.Catalog.POPULAR, null, 0).first()
                val r = core.enqueueDownload(card.id, 0, 0, app.kuro.core.Provider.WEBSHARE, url, "debug.mkv", 0)
                app.kuro.download.Downloads.start(this@MainActivity, r.id)
            }
            return
        }
        val url = intent.getStringExtra("kuro_debug_play_url") ?: return
        app.kuro.player.PlayerActivity.start(
            this, titleId = -1, season = 0, episode = 0,
            provider = app.kuro.core.Provider.WEBSHARE, ident = "debug", url = url,
            title = "Debug", subtitle = url.substringAfterLast('/'),
        )
    }
}

/** Přístup k aplikaci z Compose (core, trezor, nastavení). */
@Composable
fun kuroApp(): KuroApp = LocalContext.current.applicationContext as KuroApp
