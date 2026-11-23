package app.kuro.ui.home

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.Button
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.style.TextOverflow
import app.kuro.R
import app.kuro.ui.common.subtitle
import coil3.compose.AsyncImage
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
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
import app.kuro.core.AnimeCatalog
import app.kuro.data.ContentKind
import app.kuro.data.Settings
import app.kuro.kuroApp
import app.kuro.ui.common.Message
import app.kuro.ui.common.PosterCard
import app.kuro.ui.common.PosterRow
import app.kuro.ui.common.SectionTitle
import app.kuro.ui.common.formatPosition
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.launch

/** Vše = domov (pokračování a doporučení ze všeho); ostatní = jen daný druh. */
enum class Section(val label: String) { ALL("Vše"), MOVIES("Filmy"), SERIES("Seriály"), ANIME("Anime") }

/** Druh obsahu sekce (pro skrytí v nastavení). */
private val Section.content: ContentKind?
    get() = when (this) {
        Section.ALL -> null
        Section.MOVIES -> ContentKind.MOVIES
        Section.SERIES -> ContentKind.SERIES
        Section.ANIME -> ContentKind.ANIME
    }

/** Patří titul do sekce? */
private fun Section.accepts(kind: MediaKind, anime: Boolean): Boolean = when (this) {
    Section.ALL -> true
    Section.MOVIES -> kind == MediaKind.MOVIE && !anime
    Section.SERIES -> kind == MediaKind.SERIES && !anime
    Section.ANIME -> anime
}

private data class RowSpec(
    val title: String,
    val kind: MediaKind,
    val catalog: Catalog,
    val genre: String? = null,
    /** Řada z AniListu místo Cinemety. */
    val anime: AnimeCatalog? = null,
)

/** Anime = AniList (jen tituly s protějškem na IMDb, kvůli epizodám a zdrojům). */
private fun rowsFor(section: Section): List<RowSpec> = when (section) {
    Section.ALL -> listOf(
        RowSpec("Populární filmy", MediaKind.MOVIE, Catalog.POPULAR),
        RowSpec("Populární seriály", MediaKind.SERIES, Catalog.POPULAR),
        RowSpec("Anime teď", MediaKind.SERIES, Catalog.POPULAR, anime = AnimeCatalog.TRENDING),
        RowSpec("Nové filmy", MediaKind.MOVIE, Catalog.NEW),
    )
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
        RowSpec("Teď populární", MediaKind.SERIES, Catalog.POPULAR, anime = AnimeCatalog.TRENDING),
        RowSpec("Tato sezóna", MediaKind.SERIES, Catalog.NEW, anime = AnimeCatalog.SEASON),
        RowSpec("Nejlépe hodnocené", MediaKind.SERIES, Catalog.TOP_RATED, anime = AnimeCatalog.TOP_RATED),
        RowSpec("Nejoblíbenější", MediaKind.SERIES, Catalog.POPULAR, anime = AnimeCatalog.POPULAR),
        RowSpec("Anime filmy", MediaKind.MOVIE, Catalog.POPULAR, anime = AnimeCatalog.MOVIES),
    )
}

class HomeViewModel(private val core: KuroCore, private val settings: Settings) : ViewModel() {
    /** Skryté druhy obsahu z nastavení; při změně se domov načte znovu. */
    private var hidden = settings.hiddenContent

    /** Sekce v přepínači: "Vše" a neskryté druhy. */
    val sections: List<Section>
        get() = Section.entries.filter { it.content == null || it.content !in hidden }

    private fun visible(kind: MediaKind, anime: Boolean) = settings.shows(kind, anime)

    var section by mutableStateOf(Section.ALL)
        private set
    var rows by mutableStateOf<List<Pair<String, List<TitleCard>>>>(emptyList())
        private set
    var continueItems by mutableStateOf<List<ContinueItem>>(emptyList())
        private set
    var loading by mutableStateOf(false)
        private set
    var offline by mutableStateOf(false)
        private set
    var watchlistIds by mutableStateOf<Set<Long>>(emptySet())
        private set

    /** Řada "Pro tebe" (všechny typy; filtruje se podle sekce v [forYou]). */
    private var recs by mutableStateOf<List<TitleCard>>(emptyList())

    /** Doporučení pro aktuální sekci. */
    val forYou: List<TitleCard>
        get() = recs.filter { section.accepts(it.kind, it.anime) && visible(it.kind, it.anime) }

    /** Rozkoukané pro aktuální sekci (ve "Vše" všechno). */
    val continueShown: List<ContinueItem>
        get() = continueItems.filter {
            section.accepts(it.title.kind, it.title.anime) && visible(it.title.kind, it.title.anime)
        }

    private var computingRecs = false

    private fun loadRecs() {
        val row = runCatching { core.recommendations() }.getOrNull() ?: return
        recs = row.items
        if (computingRecs) return
        // Prázdná řada (nový uživatel bez vkusu) se zkouší znovu hned,
        // jakmile něco začne sledovat; jinak jednou za den.
        val stale = row.items.isEmpty() ||
            row.computedAtMs?.let { System.currentTimeMillis() - it > 24 * 60 * 60 * 1000L } ?: true
        if (!stale) return
        // Rychlý výpočet bez ověřování dostupnosti; ověřenou řadu dodá denní úloha.
        computingRecs = true
        viewModelScope.launch {
            runCatching { core.computeRecommendations(false) }.getOrNull()?.let { fresh ->
                recs = runCatching { core.localize(fresh.items.map { it.id }) }.getOrDefault(fresh.items)
            }
            computingRecs = false
        }
    }

    /**
     * Velký titul nahoře: náhodně z prvních pár populárních, které mají
     * obrázek pozadí. Náhodnost drží VM, takže se při posouvání nemění.
     */
    val featured: TitleCard?
        get() = rows.firstOrNull()?.second
            ?.filter { it.background != null }
            ?.take(6)
            ?.let { list -> if (list.isEmpty()) null else list[featuredSeed % list.size] }

    private val featuredSeed = (0..1000).random()

    fun toggleFeaturedWatchlist() {
        val card = featured ?: return
        val on = card.id !in watchlistIds
        runCatching { core.setWatchlist(card.id, on) }.onSuccess {
            watchlistIds = if (on) watchlistIds + card.id else watchlistIds - card.id
        }
    }

    init {
        load()
    }

    fun select(s: Section) {
        // Druhé klepnutí na vybranou sekci vrátí na "Vše".
        val next = if (s == section) Section.ALL else s
        if (next == section) return
        section = next
        rows = emptyList()
        load()
    }

    /** Po návratu z nastavení: změnily se skryté druhy → znovu načíst. */
    fun checkSettings() {
        val now = settings.hiddenContent
        if (now == hidden) return
        hidden = now
        if (section !in sections) section = Section.ALL
        rows = emptyList()
        load()
    }

    fun refreshLocal() {
        loadRecs()
        continueItems = runCatching { core.continueWatching(20) }.getOrDefault(emptyList())
        watchlistIds = runCatching { core.watchlist().map { it.id }.toSet() }.getOrDefault(emptySet())
    }

    fun load() {
        refreshLocal()
        val current = section
        viewModelScope.launch {
            loading = true
            val results = rowsFor(current).filter { spec ->
                // Ve "Vše" jen řady neskrytých druhů.
                visible(spec.kind, spec.anime != null)
            }.map { spec ->
                async {
                    try {
                        spec.title to (
                            spec.anime?.let { core.animeCatalog(it, 1) }
                                ?: core.catalog(spec.kind, spec.catalog, spec.genre, 0)
                            )
                    } catch (e: KuroException) {
                        null
                    }
                }
            }.awaitAll()
            if (section == current) {
                // Cinemeta řady filmů/seriálů občas obsahují anime — skrýt podle nastavení.
                rows = results.filterNotNull()
                    .map { (t, cards) -> t to cards.filter { visible(it.kind, it.anime) } }
                    .filter { it.second.isNotEmpty() }
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
    val settings = kuroApp().settings
    val vm: HomeViewModel = viewModel { HomeViewModel(core, settings) }

    LifecycleResumeEffect(Unit) {
        vm.checkSettings()
        vm.refreshLocal()
        onPauseOrDispose {}
    }

    PullToRefreshBox(isRefreshing = vm.loading, onRefresh = vm::load, modifier = Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = 24.dp)) {
            item {
                Hero(
                    card = vm.featured,
                    section = vm.section,
                    sections = vm.sections,
                    inWatchlist = vm.featured?.id?.let { it in vm.watchlistIds } == true,
                    onSelect = vm::select,
                    onOpen = onOpen,
                    onToggleWatchlist = vm::toggleFeaturedWatchlist,
                )
            }
            if (vm.continueShown.isNotEmpty()) {
                item {
                    SectionTitle("Pokračovat ve sledování")
                    LazyRow(
                        contentPadding = PaddingValues(horizontal = 16.dp),
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        items(vm.continueShown, key = { it.title.id }) { c ->
                            val p = c.progress
                            PosterCard(
                                c.title,
                                onClick = { onOpen(c.title.id) },
                                progress = if (p.durationMs > 0) p.positionMs.toFloat() / p.durationMs else null,
                                caption = when {
                                    p.season > 0 && p.positionMs == 0L -> "S${p.season} E${p.episode} · další díl"
                                    p.season > 0 -> "S${p.season} E${p.episode} · ${formatPosition(p.positionMs)}"
                                    else -> formatPosition(p.positionMs)
                                },
                            )
                        }
                    }
                }
            }
            if (vm.forYou.isNotEmpty()) {
                item(key = "for-you-${vm.section}") {
                    PosterRow("Pro tebe", vm.forYou) { onOpen(it.id) }
                    Spacer(Modifier.height(8.dp))
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
                        "Anime katalog z AniListu.",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(16.dp),
                    )
                }
            }
        }
    }
}

/** Velký doporučený titul nahoře s logem a přepínačem sekcí přes obrázek. */
@Composable
private fun Hero(
    card: TitleCard?,
    section: Section,
    sections: List<Section>,
    inWatchlist: Boolean,
    onSelect: (Section) -> Unit,
    onOpen: (Long) -> Unit,
    onToggleWatchlist: () -> Unit,
) {
    val bg = MaterialTheme.colorScheme.background
    Box(Modifier.fillMaxWidth().height(if (card != null) 480.dp else 120.dp)) {
        if (card != null) {
            AsyncImage(
                model = card.background ?: card.poster,
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize().clickable { onOpen(card.id) },
            )
            Box(
                Modifier.fillMaxSize().background(
                    Brush.verticalGradient(
                        0f to Color.Black.copy(alpha = 0.6f),
                        0.25f to Color.Transparent,
                        0.55f to bg.copy(alpha = 0.35f),
                        1f to bg,
                    ),
                ),
            )
        }
        Row(
            Modifier.statusBarsPadding().padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Image(painterResource(R.drawable.ic_kuro_mark), contentDescription = "Kuro", modifier = Modifier.size(44.dp))
            sections.forEach { s ->
                FilterChip(
                    selected = section == s,
                    onClick = { onSelect(s) },
                    label = { Text(s.label) },
                    colors = FilterChipDefaults.filterChipColors(
                        containerColor = Color.Black.copy(alpha = 0.35f),
                        labelColor = Color.White,
                    ),
                    border = FilterChipDefaults.filterChipBorder(
                        enabled = true,
                        selected = section == s,
                        borderColor = Color.White.copy(alpha = 0.35f),
                    ),
                )
            }
        }
        if (card != null) {
            Column(Modifier.align(Alignment.BottomStart).padding(horizontal = 16.dp, vertical = 12.dp)) {
                Text(
                    card.title,
                    style = MaterialTheme.typography.headlineMedium,
                    fontWeight = FontWeight.Bold,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    listOfNotNull(card.subtitle(), card.genres.take(3).joinToString(" · ").ifEmpty { null }).joinToString("  ·  "),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                )
                Spacer(Modifier.height(12.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(onClick = { onOpen(card.id) }) {
                        Icon(Icons.Filled.PlayArrow, null)
                        Spacer(Modifier.width(6.dp))
                        Text("Sledovat")
                    }
                    OutlinedButton(onClick = onToggleWatchlist) {
                        Icon(if (inWatchlist) Icons.Filled.Check else Icons.Filled.Add, null)
                        Spacer(Modifier.width(6.dp))
                        Text(if (inWatchlist) "V oblíbených" else "Oblíbené")
                    }
                }
            }
        }
    }
}
