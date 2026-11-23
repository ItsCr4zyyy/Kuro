package app.kuro.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import app.kuro.BuildConfig
import app.kuro.core.AccountInfo
import app.kuro.core.DownloadStatus
import app.kuro.ui.common.formatSize
import app.kuro.core.KuroException
import app.kuro.data.Settings
import app.kuro.data.TokenVault
import app.kuro.kuroApp
import coil3.SingletonImageLoader
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
fun SettingsScreen(onLogin: () -> Unit) {
    val app = kuroApp()
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    var loggedIn by remember { mutableStateOf(app.core.isWebshareLoggedIn()) }
    var account by remember { mutableStateOf<AccountInfo?>(null) }
    var accountError by remember { mutableStateOf<String?>(null) }
    var maxTitles by remember { mutableIntStateOf(app.settings.maxTitles) }
    var cleanResult by remember { mutableStateOf<String?>(null) }
    var cleaning by remember { mutableStateOf(false) }

    LifecycleResumeEffect(Unit) {
        loggedIn = app.core.isWebshareLoggedIn()
        onPauseOrDispose {}
    }
    LaunchedEffect(loggedIn) {
        account = null
        accountError = null
        if (loggedIn) {
            try {
                account = app.core.webshareAccount()
            } catch (e: KuroException) {
                if (e is KuroException.AuthExpired) {
                    app.core.setWebshareToken(null)
                    app.vault.save(TokenVault.WEBSHARE, null)
                    loggedIn = false
                } else {
                    accountError = "Stav účtu teď nejde zjistit."
                }
            }
        }
    }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
        TopAppBar(title = { Text("Nastavení") })

        Group("Účet Webshare") {
            if (loggedIn) {
                Text(account?.username?.let { "Přihlášen jako $it" } ?: "Přihlášen", fontWeight = FontWeight.SemiBold)
                account?.let { a ->
                    Text(
                        if (a.vip) "VIP" + (a.vipUntil?.let { " do ${it.take(10)}" } ?: "") else "Bez VIP — přehrávání bude pomalé",
                        style = MaterialTheme.typography.bodySmall,
                        color = if (a.vip) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.error,
                    )
                }
                accountError?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                OutlinedButton(onClick = {
                    app.core.setWebshareToken(null)
                    app.vault.save(TokenVault.WEBSHARE, null)
                    loggedIn = false
                }) { Text("Odhlásit") }
            } else {
                Text("Nepřihlášeno. Hledat jde i bez účtu, přehrávat ne.", style = MaterialTheme.typography.bodySmall)
                Button(onClick = onLogin) { Text("Přihlásit") }
            }
        }

        Group("Paměť") {
            Text("Kolik titulů si Kuro pamatuje", style = MaterialTheme.typography.bodyMedium)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Settings.MAX_TITLES_OPTIONS.forEach { n ->
                    FilterChip(maxTitles == n, {
                        maxTitles = n
                        app.settings.maxTitles = n
                    }, { Text("%,d".format(n).replace(',', ' ')) })
                }
            }
            Text(
                "Oblíbené, rozkoukané a stažené se nemažou nikdy. Ostatní tituly ubývají podle toho, " +
                    "jak dlouho tě nezajímaly.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Button(enabled = !cleaning, onClick = {
                scope.launch {
                    cleaning = true
                    val r = withContext(Dispatchers.IO) {
                        runCatching { app.core.deepClean(maxTitles) }.getOrNull()
                    }
                    SingletonImageLoader.get(context).let { loader ->
                        loader.memoryCache?.clear()
                        withContext(Dispatchers.IO) { loader.diskCache?.clear() }
                    }
                    cleanResult = r?.let {
                        "Smazáno ${it.expired + it.evicted} titulů a plakáty z cache. Zbývá ${it.titlesLeft}."
                    } ?: "Pročištění se nepovedlo."
                    cleaning = false
                }
            }) { Text("Pročistit paměť") }
            cleanResult?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
        }

        Group("Stahování") {
            var wifiOnly by remember { mutableStateOf(app.settings.downloadWifiOnly) }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Jen přes Wi-Fi", style = MaterialTheme.typography.bodyMedium)
                    Text(
                        "Na mobilních datech stahování počká.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                androidx.compose.material3.Switch(checked = wifiOnly, onCheckedChange = {
                    wifiOnly = it
                    app.settings.downloadWifiOnly = it
                })
            }
            val used = remember(cleanResult) {
                runCatching { app.core.downloads() }.getOrDefault(emptyList())
                    .filter { it.status == DownloadStatus.DONE }.sumOf { it.bytesTotal }
            }
            Text(
                "Stažené zabírají ${formatSize(used).replace("—", "0 MB")}. Smazat je jde v Knihovně.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        Group("Soukromí") {
            Text(
                "Všechno zůstává v telefonu: historie, pozice, oblíbené i stažené soubory. Kuro nemá " +
                    "vlastní server ani cloud a nic nezálohuje. Ven jdou jen dotazy na katalog " +
                    "(Cinemeta, Wikidata) a na tvé úložiště.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        Text(
            "Kuro ${BuildConfig.VERSION_NAME} · soukromý projekt · GPL-3.0",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(16.dp).align(Alignment.CenterHorizontally),
        )
    }
}

@Composable
private fun Group(title: String, content: @Composable () -> Unit) {
    Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(title, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
            content()
        }
    }
}
