package app.kuro.ui.library

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.BookmarkBorder
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.compose.runtime.LaunchedEffect
import app.kuro.core.ContinueItem
import app.kuro.core.DownloadInfo
import app.kuro.core.DownloadStatus
import app.kuro.core.TitleCard
import app.kuro.kuroApp
import app.kuro.ui.common.Message
import app.kuro.ui.common.PosterCard
import app.kuro.ui.common.SectionTitle
import app.kuro.ui.common.formatPosition

/** Knihovna je čistě lokální — funguje i bez připojení. */
@Composable
fun LibraryScreen(onOpen: (Long) -> Unit) {
    val core = kuroApp().core
    var watching by remember { mutableStateOf<List<ContinueItem>>(emptyList()) }
    var favorites by remember { mutableStateOf<List<TitleCard>>(emptyList()) }
    var downloads by remember { mutableStateOf<List<DownloadInfo>>(emptyList()) }
    val reloadDownloads = { downloads = runCatching { core.downloads() }.getOrDefault(emptyList()) }

    LifecycleResumeEffect(Unit) {
        watching = runCatching { core.continueWatching(50) }.getOrDefault(emptyList())
        favorites = runCatching { core.watchlist() }.getOrDefault(emptyList())
        reloadDownloads()
        onPauseOrDispose {}
    }
    // Průběh stahování se obnovuje, dokud něco běží.
    val active = downloads.any { it.status == DownloadStatus.RUNNING || it.status == DownloadStatus.QUEUED }
    LaunchedEffect(active) {
        while (active) {
            kotlinx.coroutines.delay(1500)
            reloadDownloads()
        }
    }

    LazyVerticalGrid(
        columns = GridCells.Adaptive(110.dp),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 24.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier.fillMaxSize(),
    ) {
        item(span = { GridItemSpan(maxLineSpan) }) { TopAppBar(title = { Text("Knihovna") }) }

        if (downloads.isNotEmpty()) {
            item(span = { GridItemSpan(maxLineSpan) }) { SectionTitle("Stažené", Modifier) }
            items(downloads, key = { "d-${it.id}" }, span = { GridItemSpan(maxLineSpan) }) { d ->
                DownloadRow(d, onOpen = onOpen, onChanged = reloadDownloads)
            }
        }
        if (watching.isEmpty() && favorites.isEmpty() && downloads.isEmpty()) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Message("Zatím prázdno. Přidej si něco do oblíbených nebo začni sledovat.", icon = Icons.Outlined.BookmarkBorder)
            }
        }
        if (watching.isNotEmpty()) {
            item(span = { GridItemSpan(maxLineSpan) }) { SectionTitle("Rozkoukané", Modifier) }
            items(watching, key = { "w-${it.title.id}" }) { c ->
                val p = c.progress
                PosterCard(
                    c.title,
                    onClick = { onOpen(c.title.id) },
                    modifier = Modifier.fillMaxWidth(),
                    progress = if (p.durationMs > 0) p.positionMs.toFloat() / p.durationMs else null,
                    caption = (if (p.season > 0) "S${p.season} E${p.episode} · " else "") + formatPosition(p.positionMs),
                )
            }
        }
        if (favorites.isNotEmpty()) {
            item(span = { GridItemSpan(maxLineSpan) }) { SectionTitle("Oblíbené", Modifier) }
            items(favorites, key = { "f-${it.id}" }) { card ->
                PosterCard(card, onClick = { onOpen(card.id) }, modifier = Modifier.fillMaxWidth())
            }
        }
    }
}
