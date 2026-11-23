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
    }
}

/** Přístup k aplikaci z Compose (core, trezor, nastavení). */
@Composable
fun kuroApp(): KuroApp = LocalContext.current.applicationContext as KuroApp
