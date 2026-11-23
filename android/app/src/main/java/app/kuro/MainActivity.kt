package app.kuro

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import app.kuro.ui.KuroRoot
import app.kuro.ui.theme.KuroTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent { KuroTheme { KuroRoot() } }
        debugPlay()
    }

    /**
     * Jen debug build: přehrávač s libovolnou URL bez přihlášení, pro test
     * přes adb:  adb shell am start -n app.kuro.debug/app.kuro.MainActivity
     *            --es kuro_debug_play_url <url>
     */
    private fun debugPlay() {
        if (!BuildConfig.DEBUG) return
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
