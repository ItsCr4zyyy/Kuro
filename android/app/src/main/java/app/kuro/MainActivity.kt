package app.kuro

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import app.kuro.core.KuroCore
import app.kuro.core.KuroException
import app.kuro.core.StreamInfo
import app.kuro.core.StreamListener
import app.kuro.core.TitleCard
import app.kuro.ui.theme.KuroTheme
import coil3.compose.AsyncImage
import kotlinx.coroutines.launch

/**
 * Kouřová obrazovka F3: ověří celý řetězec Kotlin → Rust → Cinemeta /
 * Webshare → SQLite. Skutečné UI přijde ve F4.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val core = (application as KuroApp).core
        setContent {
            KuroTheme {
                val vm: SmokeViewModel = viewModel { SmokeViewModel(core) }
                SmokeScreen(vm)
            }
        }
    }
}

class SmokeViewModel(private val core: KuroCore) : ViewModel() {
    var query by mutableStateOf("")
    var busy by mutableStateOf(false)
        private set
    var status by mutableStateOf<String?>(null)
        private set
    var results by mutableStateOf<List<TitleCard>>(emptyList())
        private set
    var selected by mutableStateOf<TitleCard?>(null)
        private set
    var streams by mutableStateOf<List<StreamInfo>>(emptyList())
        private set

    fun search() = launchBusy {
        selected = null
        results = core.search(query, null)
        status = "${results.size} titulů"
        // České názvy se doplní dodatečně, ať výsledky nečekají na Wikidata.
        results = core.localize(results.map { it.id })
    }

    fun open(card: TitleCard) = launchBusy {
        selected = card
        val detail = core.titleDetail(card.id)
        selected = detail.card
        // U seriálu zatím zkusíme S01E01 — výběr epizod přijde ve F4.
        val (s, e) = if (detail.episodes.isNotEmpty()) 1 to 1 else 0 to 0
        streams = core.cachedStreams(card.id, s, e).streams
        val started = System.currentTimeMillis()
        streams = core.findStreams(card.id, s, e, object : StreamListener {
            override fun onPartial(streams: List<StreamInfo>) {
                // Volá se z vlákna Rustu; stav Compose je thread-safe pro zápis.
                this@SmokeViewModel.streams = streams
            }
        })
        status = "${streams.size} zdrojů za ${System.currentTimeMillis() - started} ms" +
            if (s > 0) " (S01E01)" else ""
    }

    private fun launchBusy(block: suspend () -> Unit) {
        viewModelScope.launch {
            busy = true
            try {
                block()
            } catch (e: KuroException) {
                status = "Chyba: ${e.message}"
            } finally {
                busy = false
            }
        }
    }
}

@Composable
private fun SmokeScreen(vm: SmokeViewModel) {
    Scaffold(
        topBar = { TopAppBar(title = { Text(vm.selected?.title ?: "Kuro") }) },
    ) { pad ->
        Column(Modifier.padding(pad).fillMaxSize().padding(horizontal = 16.dp)) {
            OutlinedTextField(
                value = vm.query,
                onValueChange = { vm.query = it },
                label = { Text("Hledat film nebo seriál") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                keyboardActions = KeyboardActions(onSearch = { vm.search() }),
                modifier = Modifier.fillMaxWidth(),
            )
            Row(Modifier.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                if (vm.busy) {
                    CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp)
                    Spacer(Modifier.width(8.dp))
                }
                Text(vm.status ?: "", style = MaterialTheme.typography.bodySmall)
            }
            LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (vm.selected == null) {
                    items(vm.results, key = { it.id }) { card -> CardRow(card) { vm.open(card) } }
                } else {
                    items(vm.streams, key = { it.ident }) { StreamRow(it) }
                }
            }
        }
    }
}

@Composable
private fun CardRow(card: TitleCard, onClick: () -> Unit) {
    Row(Modifier.fillMaxWidth().clickable(onClick = onClick), verticalAlignment = Alignment.CenterVertically) {
        AsyncImage(
            model = card.poster,
            contentDescription = null,
            modifier = Modifier.size(48.dp, 72.dp).clip(RoundedCornerShape(6.dp)),
        )
        Spacer(Modifier.width(12.dp))
        Column {
            Text(card.title, style = MaterialTheme.typography.titleMedium)
            Text(
                listOfNotNull(card.year?.toString(), card.kind.name.lowercase(), card.rating?.let { "★ $it" })
                    .joinToString(" · "),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun StreamRow(s: StreamInfo) {
    val langs = if (s.langExplicit) {
        buildList {
            if (s.audioLangs.isNotEmpty()) add("zvuk " + s.audioLangs.joinToString("+"))
            if (s.subLangs.isNotEmpty()) add("tit. " + s.subLangs.joinToString("+"))
        }.joinToString(", ")
    } else {
        "zvuk neuveden"
    }
    Column(Modifier.fillMaxWidth()) {
        Text(
            listOfNotNull(s.height?.let { "${it}p" }, s.hdr, "%.1f GB".format(s.sizeBytes / 1e9), langs)
                .joinToString(" · "),
            style = MaterialTheme.typography.titleSmall,
        )
        Text(s.filename, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
