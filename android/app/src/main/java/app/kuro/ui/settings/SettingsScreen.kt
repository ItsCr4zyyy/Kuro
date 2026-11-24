package app.kuro.ui.settings

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Switch
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
import app.kuro.core.Provider
import app.kuro.data.ContentKind
import app.kuro.data.Settings
import app.kuro.data.TokenVault
import app.kuro.kuroApp
import app.kuro.update.Release
import app.kuro.update.Updater
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

        Group("Zdroje") {
            var disabled by remember { mutableStateOf(app.settings.disabledProviders) }
            fun toggle(p: Provider, on: Boolean) {
                disabled = if (on) disabled - p else disabled + p
                app.settings.disabledProviders = disabled
                app.core.setDisabledProviders(disabled.toList())
            }

            ProviderRow(
                name = "Webshare",
                note = when {
                    !loggedIn -> "Potřebuje účet. Původní kvalita (4K, HDR), všechny zvukové stopy."
                    account?.vip == true -> "Přihlášen" + (account?.username?.let { " jako $it" } ?: "") +
                        " · VIP" + (account?.vipUntil?.let { " do ${it.take(10)}" } ?: "")
                    account != null -> "Přihlášen" + (account?.username?.let { " jako $it" } ?: "") +
                        " · bez VIP, přehrávání bude pomalé"
                    else -> accountError ?: "Přihlášen"
                },
                checked = Provider.WEBSHARE !in disabled,
                onChecked = { toggle(Provider.WEBSHARE, it) },
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (loggedIn) {
                    OutlinedButton(onClick = {
                        app.core.setWebshareToken(null)
                        app.vault.save(TokenVault.WEBSHARE, null)
                        loggedIn = false
                    }) { Text("Odhlásit") }
                } else {
                    Button(onClick = onLogin) { Text("Přihlásit k Webshare") }
                }
            }

            HorizontalDivider()
            ProviderRow(
                name = "Přehraj.to",
                note = "Bez účtu. Převod do 1080p, obvykle jedna zvuková stopa, titulky.",
                checked = Provider.PREHRAJ_TO !in disabled,
                onChecked = { toggle(Provider.PREHRAJ_TO, it) },
            )
            ProviderRow(
                name = "Hellspy",
                note = "Bez účtu. Převod do 1080p, titulky. Část souborů je stejná jako na Přehraj.to.",
                checked = Provider.HELLSPY !in disabled,
                onChecked = { toggle(Provider.HELLSPY, it) },
            )
            val active = Provider.entries.count { it !in disabled && (it != Provider.WEBSHARE || loggedIn) }
            if (active == 0) {
                Text(
                    "Žádný zdroj není zapnutý — nic nepůjde přehrát.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }

        Group("Obsah") {
            var hidden by remember { mutableStateOf(app.settings.hiddenContent) }
            Text(
                "Co se má ukazovat na domovské stránce, v hledání a v doporučeních. " +
                    "Rozkoukané a oblíbené zůstanou v Knihovně.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            ContentKind.entries.forEach { c ->
                val on = c !in hidden
                // Poslední zapnutý druh vypnout nejde.
                val last = on && hidden.size == ContentKind.entries.size - 1
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(c.label, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
                    Switch(checked = on, enabled = !last, onCheckedChange = { v ->
                        hidden = if (v) hidden - c else hidden + c
                        app.settings.hiddenContent = hidden
                    })
                }
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
                Switch(checked = wifiOnly, onCheckedChange = {
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

        Group("Záloha") {
            var backupMsg by remember { mutableStateOf<String?>(null) }
            val exportLauncher = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/json")) { uri ->
                uri ?: return@rememberLauncherForActivityResult
                scope.launch {
                    backupMsg = withContext(Dispatchers.IO) {
                        runCatching {
                            val json = app.core.exportBackup(app.settings.toBackup())
                            context.contentResolver.openOutputStream(uri, "wt")!!.use { it.write(json.toByteArray()) }
                            "Záloha uložena."
                        }.getOrElse { "Záloha se nepovedla: ${it.message}" }
                    }
                }
            }
            val importLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
                uri ?: return@rememberLauncherForActivityResult
                scope.launch {
                    backupMsg = withContext(Dispatchers.IO) {
                        runCatching {
                            val json = context.contentResolver.openInputStream(uri)!!.use { it.readBytes().decodeToString() }
                            val r = app.core.importBackup(json)
                            app.settings.applyBackup(r.settings)
                            app.core.setDisabledProviders(app.settings.disabledProviders.toList())
                            "Obnoveno: ${r.titles} titulů, ${r.watchlist} oblíbených, ${r.progress} pozic."
                        }.getOrElse { e ->
                            "Obnova se nepovedla: " + ((e as? KuroException.Other)?.msg ?: "soubor nejde přečíst.")
                        }
                    }
                    maxTitles = app.settings.maxTitles
                }
            }
            Text(
                "Oblíbené, rozkoukané a zhlédnuté, vkus pro doporučení a nastavení v jednom souboru. " +
                    "Hodí se při přechodu na nový telefon. Účet ani stažené soubory v záloze nejsou.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { exportLauncher.launch("kuro-zaloha-${java.time.LocalDate.now()}.json") }) { Text("Zálohovat") }
                OutlinedButton(onClick = { importLauncher.launch(arrayOf("application/json", "text/plain", "*/*")) }) {
                    Text("Obnovit")
                }
            }
            backupMsg?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
        }

        Group("Aktualizace") {
            var release by remember { mutableStateOf<Release?>(null) }
            var updateMsg by remember { mutableStateOf<String?>(null) }
            var checking by remember { mutableStateOf(false) }
            var progress by remember { mutableStateOf<Float?>(null) }
            Text("Verze ${BuildConfig.VERSION_NAME}", style = MaterialTheme.typography.bodyMedium)
            Text(
                "Kuro se samo nikam nehlásí. Novou verzi najde jen po klepnutí (GitHub Releases).",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            val r = release
            if (r == null) {
                Button(enabled = !checking, onClick = {
                    scope.launch {
                        checking = true
                        updateMsg = null
                        runCatching { Updater.check() }
                            .onSuccess { found ->
                                release = found
                                if (found == null) updateMsg = "Máš nejnovější verzi."
                            }
                            .onFailure { updateMsg = it.message ?: "Kontrola se nepovedla." }
                        checking = false
                    }
                }) { Text(if (checking) "Hledám…" else "Zkontrolovat aktualizace") }
            } else {
                Text("K dispozici je verze ${r.version}", fontWeight = FontWeight.SemiBold)
                if (r.notes.isNotBlank()) {
                    Text(r.notes, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                val p = progress
                if (p != null) {
                    androidx.compose.material3.LinearProgressIndicator(progress = { p }, modifier = Modifier.fillMaxWidth())
                } else {
                    Button(onClick = {
                        scope.launch {
                            progress = 0f
                            updateMsg = null
                            runCatching { Updater.download(context, r) { progress = it } }
                                .onSuccess { apk -> Updater.install(context, apk) }
                                .onFailure { updateMsg = it.message ?: "Stažení se nepovedlo." }
                            progress = null
                        }
                    }) { Text("Stáhnout a nainstalovat (${formatSize(r.sizeBytes)})") }
                }
            }
            updateMsg?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
        }

        Group("Soukromí") {
            Text(
                "Všechno zůstává v telefonu: historie, pozice, oblíbené i stažené soubory. Kuro nemá " +
                    "vlastní server ani cloud a nic nezálohuje. Ven jdou jen dotazy na katalog " +
                    "(Cinemeta, Wikidata, AniList) a na zapnuté zdroje.",
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
private fun ProviderRow(name: String, note: String, checked: Boolean, onChecked: (Boolean) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(name, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
            Text(note, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Switch(checked = checked, onCheckedChange = onChecked)
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
