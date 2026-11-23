package app.kuro.ui.search

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.SearchOff
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import app.kuro.core.KuroCore
import app.kuro.core.KuroException
import app.kuro.core.MediaKind
import app.kuro.core.TitleCard
import app.kuro.core.UserEvent
import app.kuro.kuroApp
import app.kuro.ui.common.Message
import app.kuro.ui.common.PosterCard
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class SearchViewModel(private val core: KuroCore) : ViewModel() {
    var query by mutableStateOf("")
        private set
    var kind by mutableStateOf<MediaKind?>(null)
        private set
    var results by mutableStateOf<List<TitleCard>>(emptyList())
        private set
    var recent by mutableStateOf(core.recentSearches(10))
        private set
    var searching by mutableStateOf(false)
        private set
    var error by mutableStateOf<String?>(null)
        private set
    var searchedFor by mutableStateOf<String?>(null)
        private set

    private var job: Job? = null

    fun onQuery(q: String) {
        query = q
        job?.cancel()
        if (q.isBlank()) {
            results = emptyList()
            searchedFor = null
            recent = core.recentSearches(10)
            return
        }
        // Okamžitě z lokální databáze, po chvíli psaní z Cinemety.
        results = runCatching { core.searchLocal(q, kind, 20) }.getOrDefault(emptyList())
        job = viewModelScope.launch {
            delay(450)
            remote(q)
        }
    }

    fun submit() {
        job?.cancel()
        if (query.isNotBlank()) job = viewModelScope.launch { remote(query) }
    }

    fun selectKind(k: MediaKind?) {
        kind = k
        if (query.isNotBlank()) onQuery(query)
    }

    fun clearRecent() {
        core.clearRecentSearches()
        recent = emptyList()
    }

    fun clicked(card: TitleCard) {
        runCatching { core.recordEvent(card.id, UserEvent.SEARCH_CLICK) }
    }

    private suspend fun remote(q: String) {
        searching = true
        error = null
        try {
            val found = core.search(q, kind)
            // Lokální napovídání, které Cinemeta nevrátila (třeba české názvy), nechat navrchu.
            val local = results.filter { l -> found.none { it.id == l.id } }
            results = local + found
            searchedFor = q
            results = core.localize(results.map { it.id })
        } catch (e: KuroException) {
            error = if (results.isEmpty()) "Hledání selhalo (${e.message})." else null
        } finally {
            searching = false
        }
    }
}

@Composable
fun SearchScreen(onOpen: (Long) -> Unit) {
    val core = kuroApp().core
    val vm: SearchViewModel = viewModel { SearchViewModel(core) }
    val focus = LocalFocusManager.current

    Column(Modifier.fillMaxSize().statusBarsPadding()) {
        OutlinedTextField(
            value = vm.query,
            onValueChange = vm::onQuery,
            placeholder = { Text("Film, seriál, anime…") },
            leadingIcon = { Icon(Icons.Outlined.Search, null) },
            trailingIcon = {
                if (vm.query.isNotEmpty()) {
                    IconButton(onClick = { vm.onQuery("") }) { Icon(Icons.Outlined.Close, "Smazat") }
                }
            },
            singleLine = true,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
            keyboardActions = KeyboardActions(onSearch = { vm.submit(); focus.clearFocus() }),
            modifier = Modifier.fillMaxWidth().padding(16.dp, 12.dp, 16.dp, 4.dp),
        )
        Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilterChip(vm.kind == null, { vm.selectKind(null) }, { Text("Vše") })
            FilterChip(vm.kind == MediaKind.MOVIE, { vm.selectKind(MediaKind.MOVIE) }, { Text("Filmy") })
            FilterChip(vm.kind == MediaKind.SERIES, { vm.selectKind(MediaKind.SERIES) }, { Text("Seriály") })
        }
        if (vm.searching) {
            LinearProgressIndicator(Modifier.fillMaxWidth().padding(top = 4.dp))
        }

        when {
            vm.query.isBlank() -> RecentSearches(vm.recent, onPick = { vm.onQuery(it); vm.submit() }, onClear = vm::clearRecent)
            vm.error != null -> Message(vm.error!!)
            vm.results.isEmpty() && vm.searchedFor == vm.query && !vm.searching ->
                Message("Nic nenalezeno. Zkus originální název.", icon = Icons.Outlined.SearchOff)
            else -> LazyVerticalGrid(
                columns = GridCells.Adaptive(110.dp),
                contentPadding = PaddingValues(16.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
                modifier = Modifier.fillMaxSize(),
            ) {
                items(vm.results, key = { it.id }) { card ->
                    PosterCard(card, onClick = {
                        vm.clicked(card)
                        onOpen(card.id)
                    }, modifier = Modifier.fillMaxWidth())
                }
            }
        }
    }
}

@Composable
private fun RecentSearches(recent: List<String>, onPick: (String) -> Unit, onClear: () -> Unit) {
    if (recent.isEmpty()) {
        Message("Hledej podle českého i originálního názvu.", icon = Icons.Outlined.Search)
        return
    }
    Column(Modifier.padding(top = 8.dp)) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Nedávno hledané", style = MaterialTheme.typography.titleSmall, modifier = Modifier.weight(1f))
            TextButton(onClick = onClear) { Text("Smazat") }
        }
        recent.forEach { q ->
            Row(
                Modifier.fillMaxWidth().clickable { onPick(q) }.padding(horizontal = 16.dp, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Icon(Icons.Outlined.History, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(q)
            }
        }
    }
}
