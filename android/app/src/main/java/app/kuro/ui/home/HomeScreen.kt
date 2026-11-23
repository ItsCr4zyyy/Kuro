package app.kuro.ui.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import app.kuro.core.Catalog
import app.kuro.core.ContinueItem
import app.kuro.core.KuroCore
import app.kuro.core.KuroException
import app.kuro.core.MediaKind
import app.kuro.core.TitleCard
import app.kuro.kuroApp
import app.kuro.ui.common.Message
import app.kuro.ui.common.PosterCard
import app.kuro.ui.common.PosterRow
import app.kuro.ui.common.SectionTitle
import app.kuro.ui.common.formatPosition
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.launch

enum class Section(val label: String) { MOVIES("Filmy"), SERIES("Seriály"), ANIME("Anime") }

private data class RowSpec(val title: String, val kind: MediaKind, val catalog: Catalog, val genre: String? = null)

/**
 * Anime zatím = žánr "Animation" z Cinemety (zahrnuje i neanime animaci).
 * Přesné anime přijde s AniListem.
 */
private fun rowsFor(section: Section): List<RowSpec> = when (section) {
    Section.MOVIES -> listOf(
        RowSpec("Populární filmy", MediaKind.MOVIE, Catalog.POPULAR),
        RowSpec("Novinky", MediaKind.MOVIE, Catalog.NEW),
        RowSpec("Doporučené", MediaKind.MOVIE, Catalog.TOP_RATED),
    )
    Section.SERIES -> listOf(
        RowSpec("Populární seriály", MediaKind.SERIES, Catalog.POPULAR),
        RowSpec("Novinky", MediaKind.SERIES, Catalog.NEW),
        RowSpec("Doporučené", MediaKind.SERIES, Catalog.TOP_RATED),
    )
    Section.ANIME -> listOf(
        RowSpec("Animované seriály", MediaKind.SERIES, Catalog.POPULAR, "Animation"),
        RowSpec("Animované filmy", MediaKind.MOVIE, Catalog.POPULAR, "Animation"),
        RowSpec("Doporučené", MediaKind.SERIES, Catalog.TOP_RATED, "Animation"),
    )
}

class HomeViewModel(private val core: KuroCore) : ViewModel() {
    var section by mutableStateOf(Section.MOVIES)
        private set
    var rows by mutableStateOf<List<Pair<String, List<TitleCard>>>>(emptyList())
        private set
    var continueItems by mutableStateOf<List<ContinueItem>>(emptyList())
        private set
    var loading by mutableStateOf(false)
        private set
    var offline by mutableStateOf(false)
        private set

    init {
        load()
    }

    fun select(s: Section) {
        if (s == section) return
        section = s
        rows = emptyList()
        load()
    }

    fun refreshLocal() {
        continueItems = runCatching { core.continueWatching(20) }.getOrDefault(emptyList())
    }

    fun load() {
        refreshLocal()
        val current = section
        viewModelScope.launch {
            loading = true
            val results = rowsFor(current).map { spec ->
                async {
                    try {
                        spec.title to core.catalog(spec.kind, spec.catalog, spec.genre, 0)
                    } catch (e: KuroException) {
                        null
                    }
                }
            }.awaitAll()
            if (section == current) {
                rows = results.filterNotNull()
                offline = results.all { it == null }
            }
            loading = false
            // České názvy dodatečně, ať plakáty nečekají na Wikidata.
            val ids = rows.flatMap { r -> r.second.map { it.id } }.distinct()
            runCatching { core.localize(ids) }.getOrNull()?.associateBy { it.id }?.let { byId ->
                if (section == current) {
                    rows = rows.map { (t, cards) -> t to cards.map { byId[it.id] ?: it } }
                }
            }
        }
    }
}

@Composable
fun HomeScreen(onOpen: (Long) -> Unit) {
    val core = kuroApp().core
    val vm: HomeViewModel = viewModel { HomeViewModel(core) }

    LifecycleResumeEffect(Unit) {
        vm.refreshLocal()
        onPauseOrDispose {}
    }

    PullToRefreshBox(isRefreshing = vm.loading, onRefresh = vm::load, modifier = Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = 24.dp)) {
            item {
                CenterAlignedTopAppBar(
                    title = {
                        Text("KURO", fontWeight = FontWeight.Black, color = MaterialTheme.colorScheme.primary)
                    },
                )
            }
            item {
                LazyRow(
                    contentPadding = PaddingValues(horizontal = 16.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    items(Section.entries) { s ->
                        FilterChip(selected = vm.section == s, onClick = { vm.select(s) }, label = { Text(s.label) })
                    }
                }
                Spacer(Modifier.height(8.dp))
            }
            if (vm.continueItems.isNotEmpty()) {
                item {
                    SectionTitle("Pokračovat ve sledování")
                    LazyRow(
                        contentPadding = PaddingValues(horizontal = 16.dp),
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        items(vm.continueItems, key = { it.title.id }) { c ->
                            val p = c.progress
                            PosterCard(
                                c.title,
                                onClick = { onOpen(c.title.id) },
                                progress = if (p.durationMs > 0) p.positionMs.toFloat() / p.durationMs else null,
                                caption = if (p.season > 0) {
                                    "S${p.season} E${p.episode} · ${formatPosition(p.positionMs)}"
                                } else {
                                    formatPosition(p.positionMs)
                                },
                            )
                        }
                    }
                }
            }
            if (vm.offline && !vm.loading) {
                item {
                    Message(
                        "Bez připojení. Uložené tituly najdeš v Knihovně.",
                        action = "Zkusit znovu" to vm::load,
                    )
                }
            }
            vm.rows.forEach { (title, cards) ->
                item(key = "${vm.section}-$title") {
                    PosterRow(title, cards) { onOpen(it.id) }
                    Spacer(Modifier.height(8.dp))
                }
            }
            if (vm.section == Section.ANIME && vm.rows.isNotEmpty()) {
                item {
                    Text(
                        "Zatím podle žánru Animace. Přesný anime katalog přijde s AniListem.",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(16.dp),
                    )
                }
            }
        }
    }
}
