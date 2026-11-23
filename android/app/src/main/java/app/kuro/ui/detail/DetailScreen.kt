package app.kuro.ui.detail

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Bookmark
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.outlined.BookmarkBorder
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import app.kuro.core.Episode
import app.kuro.core.StreamInfo
import app.kuro.core.TitleDetail
import app.kuro.kuroApp
import app.kuro.player.PlayerActivity
import app.kuro.ui.common.BadgeRow
import app.kuro.ui.common.Loading
import app.kuro.ui.common.Message
import app.kuro.ui.common.formatPosition
import app.kuro.ui.common.formatRuntime
import app.kuro.ui.common.formatSize
import app.kuro.ui.common.isSuspicious
import app.kuro.ui.common.kindLabel
import app.kuro.ui.common.languageLine
import app.kuro.ui.common.qualityBadges
import coil3.compose.AsyncImage
import java.util.Locale

@Composable
fun DetailScreen(titleId: Long, onBack: () -> Unit, onLogin: () -> Unit) {
    val app = kuroApp()
    val vm: DetailViewModel = viewModel(key = "detail-$titleId") { DetailViewModel(app.core, app.vault, titleId) }
    val context = LocalContext.current

    LifecycleResumeEffect(Unit) {
        vm.refreshProgress()
        onPauseOrDispose {}
    }

    LaunchedEffect(vm.event) {
        when (val e = vm.event) {
            PlayEvent.NeedLogin -> onLogin()
            is PlayEvent.Ready -> {
                val card = vm.detail?.card
                PlayerActivity.start(
                    context,
                    titleId = titleId,
                    season = e.target.season,
                    episode = e.target.episode,
                    provider = e.stream.provider,
                    ident = e.stream.ident,
                    url = e.link.url,
                    title = card?.title.orEmpty(),
                    subtitle = e.target.label.takeIf { e.target.season > 0 },
                )
                vm.closeStreams()
            }
            null -> {}
        }
        if (vm.event != null) vm.consumeEvent()
    }

    val d = vm.detail
    when {
        d == null && vm.error != null -> Column(Modifier.statusBarsPadding()) {
            BackButton(onBack)
            Message(vm.error!!, action = "Zkusit znovu" to vm::load)
        }
        d == null -> Loading()
        else -> DetailContent(d, vm, onBack)
    }

    if (vm.target != null) {
        StreamsSheet(vm)
    }
}

@Composable
private fun BackButton(onBack: () -> Unit, modifier: Modifier = Modifier) {
    IconButton(
        onClick = onBack,
        modifier = modifier.padding(8.dp),
        colors = IconButtonDefaults.iconButtonColors(containerColor = Color.Black.copy(alpha = 0.45f)),
    ) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Zpět", tint = Color.White) }
}

@Composable
private fun DetailContent(d: TitleDetail, vm: DetailViewModel, onBack: () -> Unit) {
    val card = d.card
    val bg = MaterialTheme.colorScheme.background
    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = 32.dp)) {
        item {
            Box(Modifier.fillMaxWidth().aspectRatio(16f / 10f)) {
                AsyncImage(
                    model = card.background ?: card.poster,
                    contentDescription = null,
                    contentScale = ContentScale.Crop,
                    modifier = Modifier.fillMaxSize(),
                )
                Box(
                    Modifier.fillMaxSize().background(
                        Brush.verticalGradient(0f to Color.Black.copy(alpha = 0.35f), 0.5f to Color.Transparent, 1f to bg),
                    ),
                )
                BackButton(onBack, Modifier.statusBarsPadding())
            }
        }
        item {
            Row(Modifier.padding(horizontal = 16.dp).offset(y = (-48).dp)) {
                AsyncImage(
                    model = card.poster,
                    contentDescription = null,
                    contentScale = ContentScale.Crop,
                    modifier = Modifier.width(104.dp).aspectRatio(2f / 3f).clip(RoundedCornerShape(10.dp))
                        .background(MaterialTheme.colorScheme.surfaceVariant),
                )
                Spacer(Modifier.width(16.dp))
                Column(Modifier.padding(top = 48.dp)) {
                    Text(card.title, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
                    if (card.originalTitle != card.title) {
                        Text(card.originalTitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Spacer(Modifier.height(4.dp))
                    Text(
                        listOfNotNull(
                            card.year?.toString(),
                            if (card.anime) "Anime" else kindLabel(card.kind),
                            d.runtimeMin?.let { formatRuntime(it) },
                            card.rating?.let { String.format(Locale.ROOT, "★ %.1f", it) },
                        ).joinToString(" · "),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
        item {
            Column(Modifier.padding(horizontal = 16.dp).offset(y = (-32).dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    val next = if (vm.isSeries) vm.nextEpisode() else null
                    Button(onClick = { if (vm.isSeries) next?.let(vm::openEpisode) else vm.openMovie() }, enabled = !vm.isSeries || next != null) {
                        Icon(Icons.Filled.PlayArrow, null)
                        Spacer(Modifier.width(6.dp))
                        Text(
                            when {
                                !vm.isSeries -> vm.progressFor(0, 0)?.takeIf { !it.finished && it.positionMs > 60_000 }
                                    ?.let { "Pokračovat ${formatPosition(it.positionMs)}" } ?: "Přehrát"
                                next != null -> "S${next.season} E${next.episode}"
                                else -> "Přehrát"
                            },
                        )
                    }
                    OutlinedButton(onClick = vm::toggleWatchlist) {
                        Icon(if (d.inWatchlist) Icons.Filled.Bookmark else Icons.Outlined.BookmarkBorder, null)
                        Spacer(Modifier.width(6.dp))
                        Text(if (d.inWatchlist) "V oblíbených" else "Oblíbené")
                    }
                }
                if (card.genres.isNotEmpty()) {
                    Spacer(Modifier.height(12.dp))
                    Text(card.genres.joinToString(" · "), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                }
                d.overview?.let {
                    Spacer(Modifier.height(8.dp))
                    Text(it, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
        if (vm.isSeries && vm.seasons.isNotEmpty()) {
            item {
                LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    items(vm.seasons) { s ->
                        FilterChip(vm.season == s, { vm.selectSeason(s) }, { Text("Sezóna $s") })
                    }
                }
                Spacer(Modifier.height(8.dp))
            }
            val episodes = d.episodes.filter { it.season == vm.season }
            items(episodes, key = { "${it.season}-${it.episode}" }) { e -> EpisodeRow(e, vm) { vm.openEpisode(e) } }
        }
    }
}

@Composable
private fun EpisodeRow(e: Episode, vm: DetailViewModel, onClick: () -> Unit) {
    val p = vm.progressFor(e.season, e.episode)
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.width(128.dp).aspectRatio(16f / 9f).clip(RoundedCornerShape(8.dp)).background(MaterialTheme.colorScheme.surfaceVariant)) {
            AsyncImage(model = e.still, contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
            if (p != null && p.durationMs > 0) {
                LinearProgressIndicator(
                    progress = { (p.positionMs.toFloat() / p.durationMs).coerceIn(0f, 1f) },
                    modifier = Modifier.align(Alignment.BottomCenter).fillMaxWidth().height(3.dp),
                    drawStopIndicator = {},
                    gapSize = 0.dp,
                )
            }
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text("${e.episode}. ${e.name ?: "Epizoda ${e.episode}"}", style = MaterialTheme.typography.titleSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
            e.airDate?.let { Text(it, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
        if (p?.finished == true) {
            Icon(Icons.Filled.CheckCircle, "Zhlédnuto", tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp))
        }
    }
}

@Composable
private fun StreamsSheet(vm: DetailViewModel) {
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = false)
    ModalBottomSheet(onDismissRequest = vm::closeStreams, sheetState = state) {
        Column(Modifier.padding(horizontal = 16.dp)) {
            Text(vm.target?.label ?: "", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    when {
                        vm.searching -> "Hledám zdroje…"
                        else -> "${vm.streams.size} zdrojů"
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (vm.searching) {
                    Spacer(Modifier.width(8.dp))
                    CircularProgressIndicator(Modifier.size(12.dp), strokeWidth = 2.dp)
                }
            }
            vm.streamError?.let {
                Row(Modifier.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Outlined.ErrorOutline, null, tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(18.dp))
                    Spacer(Modifier.width(8.dp))
                    Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
                }
            }
        }
        LazyColumn(contentPadding = PaddingValues(bottom = 24.dp)) {
            items(vm.streams, key = { "${it.provider}-${it.ident}" }) { s ->
                StreamRow(s, resolving = vm.resolving == s.ident) { vm.play(s) }
                HorizontalDivider(Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.surfaceVariant)
            }
        }
    }
}

@Composable
private fun StreamRow(s: StreamInfo, resolving: Boolean, onClick: () -> Unit) {
    val czech = s.audioLangs.any { it == "cs" || it == "sk" }
    Row(
        Modifier.fillMaxWidth().clickable(enabled = !resolving, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .alpha(if (s.isSuspicious) 0.5f else 1f),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            BadgeRow(s.qualityBadges())
            Spacer(Modifier.height(4.dp))
            Text(
                s.languageLine() + " · " + formatSize(s.sizeBytes),
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = if (czech) FontWeight.SemiBold else FontWeight.Normal,
                color = if (czech) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
            )
            Text(s.filename, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (s.isSuspicious) {
                Text("Opakovaně nešel přehrát", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.error)
            }
        }
        Box(Modifier.size(40.dp).clip(CircleShape), contentAlignment = Alignment.Center) {
            if (resolving) {
                CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
            } else {
                Icon(Icons.Filled.PlayArrow, "Přehrát", tint = MaterialTheme.colorScheme.primary)
            }
        }
    }
}
