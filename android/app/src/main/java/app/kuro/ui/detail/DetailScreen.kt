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
import app.kuro.ui.common.heroHeight
import app.kuro.ui.common.isWideScreen
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.widthIn
import androidx.compose.ui.platform.LocalConfiguration
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
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.filled.DownloadDone
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
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import app.kuro.core.Provider
import app.kuro.core.StreamInfo
import app.kuro.core.TitleDetail
import app.kuro.kuroApp
import app.kuro.player.PlayerActivity
import app.kuro.tv.TvMedia
import app.kuro.ui.common.BadgeRow
import app.kuro.ui.common.Loading
import app.kuro.ui.common.Message
import app.kuro.ui.common.estimatedBitrate
import app.kuro.ui.common.formatBitrate
import app.kuro.ui.common.formatPosition
import app.kuro.ui.common.formatRuntime
import app.kuro.ui.common.formatSize
import app.kuro.ui.common.isSuspicious
import app.kuro.ui.common.isTranscode
import app.kuro.ui.common.kindLabel
import app.kuro.ui.common.providerLabel
import app.kuro.ui.common.languageLine
import app.kuro.ui.common.qualityBadges
import coil3.compose.AsyncImage
import java.util.Locale
import dev.chrisbanes.haze.hazeSource
import dev.chrisbanes.haze.rememberHazeState
import app.kuro.ui.common.StatusBarBlur
import androidx.compose.runtime.derivedStateOf
import androidx.compose.foundation.lazy.rememberLazyListState

@Composable
fun DetailScreen(titleId: Long, onBack: () -> Unit, onLogin: () -> Unit) {
    val app = kuroApp()
    val vm: DetailViewModel = viewModel(key = "detail-$titleId") { DetailViewModel(app, app.core, app.vault, titleId, app.settings.bandwidthBps) }
    val context = LocalContext.current

    LifecycleResumeEffect(Unit) {
        vm.refreshProgress()
        onPauseOrDispose {}
    }

    // Průběh stahování, dokud něco běží.
    LaunchedEffect(vm.hasActiveDownloads) {
        while (vm.hasActiveDownloads) {
            kotlinx.coroutines.delay(1500)
            vm.refreshDownloads()
        }
    }

    // Android 13+: notifikace s průběhem stahování potřebují svolení.
    val notifPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}
    val askNotifications = {
        if (android.os.Build.VERSION.SDK_INT >= 33 &&
            context.checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) !=
            android.content.pm.PackageManager.PERMISSION_GRANTED
        ) {
            notifPermission.launch(android.Manifest.permission.POST_NOTIFICATIONS)
        }
    }

    val core = kuroApp().core
    val tv = kuroApp().tv
    /** Kde na TV navázat (stejně jako přehrávač v telefonu). */
    fun resumeSec(t: PlayTarget): Long {
        val p = runCatching { core.progress(titleId, t.season, t.episode) }.getOrNull() ?: return 0
        return if (p.finished || p.positionMs < 30_000) 0 else (p.positionMs / 1000 - 3).coerceAtLeast(0)
    }

    LaunchedEffect(vm.event) {
        when (val e = vm.event) {
            PlayEvent.NeedLogin -> onLogin()
            is PlayEvent.Ready if tv.connected != null -> {
                // Připojená televize: pustit tam místo telefonu.
                tv.play(
                    TvMedia(
                        url = e.link.url,
                        title = vm.detail?.card?.title.orEmpty(),
                        subtitle = e.target.label.takeIf { e.target.season > 0 },
                        subtitles = e.link.subtitles,
                        startSec = resumeSec(e.target),
                        titleId = titleId,
                        season = e.target.season,
                        episode = e.target.episode,
                        provider = e.stream.provider,
                        ident = e.stream.ident,
                    ),
                )
                vm.closeStreams()
            }
            is PlayEvent.Local if tv.connected != null -> {
                tv.play(
                    TvMedia(
                        url = e.file.path,
                        title = vm.detail?.card?.title.orEmpty(),
                        subtitle = e.target.label.takeIf { e.target.season > 0 },
                        subtitles = emptyList(),
                        startSec = resumeSec(e.target),
                        titleId = titleId,
                        season = e.target.season,
                        episode = e.target.episode,
                        provider = e.download.provider ?: Provider.WEBSHARE,
                        ident = e.download.ident,
                    ),
                )
            }
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
                    subtitles = e.link.subtitles,
                )
                vm.closeStreams()
            }
            is PlayEvent.Local -> {
                PlayerActivity.start(
                    context,
                    titleId = titleId,
                    season = e.target.season,
                    episode = e.target.episode,
                    provider = e.download.provider ?: Provider.WEBSHARE,
                    ident = e.download.ident,
                    url = e.file.path,
                    title = vm.detail?.card?.title.orEmpty(),
                    subtitle = (e.target.label.takeIf { e.target.season > 0 }?.let { "$it · " } ?: "") + "staženo",
                )
            }
            is PlayEvent.Message -> android.widget.Toast.makeText(context, e.text, android.widget.Toast.LENGTH_SHORT).show()
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
        else -> DetailContent(d, vm, onBack, onDownloadSeason = { askNotifications(); vm.downloadSeason(it) })
    }

    if (vm.target != null) {
        StreamsSheet(vm, onDownload = { askNotifications(); vm.download(it) })
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
private fun DetailContent(d: TitleDetail, vm: DetailViewModel, onBack: () -> Unit, onDownloadSeason: (Int) -> Unit) {
    val wide = isWideScreen()
    // Na širokém displeji díly ve dvou sloupcích a s větším odsazením.
    val columns = if (wide) 2 else 1
    val side = if (wide) 16.dp else 0.dp
    val haze = rememberHazeState()
    val listState = rememberLazyListState()
    val scrolled by remember { derivedStateOf { listState.firstVisibleItemIndex > 0 || listState.firstVisibleItemScrollOffset > 300 } }
    Box(Modifier.fillMaxSize()) {
    LazyColumn(Modifier.fillMaxSize().hazeSource(haze), state = listState, contentPadding = PaddingValues(bottom = 32.dp)) {
        if (wide) {
            item { WideHeader(d, vm, onBack) }
        } else {
            item { PhoneHeader(d, vm, onBack) }
        }
        if (vm.isSeries && vm.seasons.isNotEmpty()) {
            item {
                Column(Modifier.fillMaxWidth().padding(horizontal = side)) {
                    LazyRow(contentPadding = PaddingValues(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        items(vm.seasons) { s ->
                            FilterChip(vm.season == s, { vm.selectSeason(s) }, { Text("Sezóna $s") })
                        }
                    }
                    Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        androidx.compose.material3.TextButton(
                            onClick = { vm.season?.let(onDownloadSeason) },
                            enabled = vm.seasonProgress == null,
                        ) {
                            Icon(Icons.Outlined.Download, null, modifier = Modifier.size(18.dp))
                            Spacer(Modifier.width(6.dp))
                            Text("Stáhnout sezónu")
                        }
                        vm.seasonProgress?.let {
                            CircularProgressIndicator(Modifier.size(14.dp), strokeWidth = 2.dp)
                            Spacer(Modifier.width(8.dp))
                            Text(it, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
            }
            val episodes = d.episodes.filter { it.season == vm.season }
            items(episodes.chunked(columns), key = { row -> row.joinToString { "${it.season}-${it.episode}" } }) { row ->
                Row(Modifier.fillMaxWidth().padding(horizontal = side)) {
                    row.forEach { e -> Box(Modifier.weight(1f)) { EpisodeRow(e, vm) { vm.openEpisode(e) } } }
                    repeat(columns - row.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
        item {
            Facts(
                d,
                columns = if (wide) 3 else 2,
                modifier = Modifier.padding(horizontal = if (wide) 32.dp else 16.dp, vertical = if (wide) 16.dp else 0.dp),
            )
        }
    }
    StatusBarBlur(haze, scrolled)
    }
}

/** Telefon: pozadí, pod ním plakát s názvem, tlačítka a popis. */
@Composable
private fun PhoneHeader(d: TitleDetail, vm: DetailViewModel, onBack: () -> Unit) {
    val card = d.card
    val bg = MaterialTheme.colorScheme.background
    Column {
        Box(Modifier.fillMaxWidth().height(heroHeight(10f / 16f, 0.5f))) {
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
            app.kuro.ui.tv.TvButton(Modifier.align(Alignment.TopEnd).statusBarsPadding().padding(8.dp))
        }
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
                MetaLine(d)
            }
        }
        Column(Modifier.padding(horizontal = 16.dp).offset(y = (-32).dp)) {
            ActionButtons(d, vm)
            if (card.genres.isNotEmpty()) {
                Spacer(Modifier.height(12.dp))
                Genres(d)
            }
            d.overview?.let {
                Spacer(Modifier.height(8.dp))
                Text(it, style = MaterialTheme.typography.bodyMedium)
            }
        }
    }
}

/**
 * Široký displej (rozložený ohebný telefon, tablet): pozadí přes skoro celou
 * obrazovku a vlevo přes něj logo/název, údaje, popis a tlačítka.
 */
@Composable
private fun WideHeader(d: TitleDetail, vm: DetailViewModel, onBack: () -> Unit) {
    val card = d.card
    val bg = MaterialTheme.colorScheme.background
    val screenH = LocalConfiguration.current.screenHeightDp
    Box(Modifier.fillMaxWidth().heightIn(min = (screenH * 0.72f).dp)) {
        AsyncImage(
            model = card.background ?: card.poster,
            contentDescription = null,
            contentScale = ContentScale.Crop,
            alignment = Alignment.TopEnd,
            modifier = Modifier.matchParentSize(),
        )
        // Zleva tma pod text, dole přechod do pozadí stránky.
        Box(
            Modifier.matchParentSize().background(
                Brush.horizontalGradient(0f to bg, 0.38f to bg.copy(alpha = 0.85f), 0.7f to Color.Transparent),
            ),
        )
        Box(
            Modifier.matchParentSize().background(
                Brush.verticalGradient(0f to Color.Black.copy(alpha = 0.35f), 0.25f to Color.Transparent, 0.75f to Color.Transparent, 1f to bg),
            ),
        )
        BackButton(onBack, Modifier.statusBarsPadding())
        app.kuro.ui.tv.TvButton(Modifier.align(Alignment.TopEnd).statusBarsPadding().padding(8.dp))

        Column(
            Modifier.align(Alignment.BottomStart).statusBarsPadding()
                .padding(start = 32.dp, end = 32.dp, top = 72.dp, bottom = 16.dp)
                .widthIn(max = 520.dp),
        ) {
            var logoFailed by remember(d.logo) { mutableStateOf(d.logo == null) }
            if (!logoFailed) {
                AsyncImage(
                    model = d.logo,
                    contentDescription = card.title,
                    contentScale = ContentScale.Fit,
                    alignment = Alignment.CenterStart,
                    onError = { logoFailed = true },
                    modifier = Modifier.height(110.dp).widthIn(max = 380.dp).fillMaxWidth(),
                )
                // Logo bývá v originále; český název pod ním, když se liší.
                if (card.title != card.originalTitle) {
                    Spacer(Modifier.height(8.dp))
                    Text(card.title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
                }
            } else {
                Text(card.title, style = MaterialTheme.typography.displaySmall, fontWeight = FontWeight.Bold)
                if (card.originalTitle != card.title) {
                    Text(card.originalTitle, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            Spacer(Modifier.height(12.dp))
            MetaLine(d)
            if (card.genres.isNotEmpty()) {
                Spacer(Modifier.height(6.dp))
                Genres(d)
            }
            d.overview?.let {
                Spacer(Modifier.height(12.dp))
                Text(it, style = MaterialTheme.typography.bodyLarge, maxLines = 6, overflow = TextOverflow.Ellipsis)
            }
            Spacer(Modifier.height(20.dp))
            ActionButtons(d, vm)
        }
    }
}

@Composable
private fun MetaLine(d: TitleDetail) {
    val card = d.card
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

@Composable
private fun Genres(d: TitleDetail) {
    Text(d.card.genres.joinToString(" · "), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
}

@Composable
private fun ActionButtons(d: TitleDetail, vm: DetailViewModel) {
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
    if (!vm.isSeries) {
        vm.downloadFor(0, 0)?.let { dl ->
            Spacer(Modifier.height(8.dp))
            DownloadLine(dl)
        }
    }
}

/** Podrobnosti pod hlavičkou: jen to, co v ní není (a co opravdu víme). */
@Composable
private fun Facts(d: TitleDetail, columns: Int, modifier: Modifier = Modifier) {
    val card = d.card
    val facts = listOfNotNull(
        d.releaseDate?.let { "Premiéra" to formatDate(it) },
        d.countries.takeIf { it.isNotEmpty() }?.let { "Země" to it.joinToString(", ") },
        d.episodes.map { it.season }.distinct().size.takeIf { it > 0 }?.let { "Sezóny" to "$it" },
        d.episodes.size.takeIf { it > 0 }?.let { "Díly" to "$it" },
    )
    if (facts.isEmpty()) return
    Column(modifier) {
        HorizontalDivider(Modifier.padding(bottom = 16.dp))
        Text("Podrobnosti", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        Spacer(Modifier.height(12.dp))
        facts.chunked(columns).forEach { row ->
            Row(Modifier.fillMaxWidth().padding(bottom = 12.dp)) {
                row.forEach { (label, value) ->
                    Column(Modifier.weight(1f).padding(end = 16.dp)) {
                        Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Text(value, style = MaterialTheme.typography.bodyMedium)
                    }
                }
                repeat(columns - row.size) { Spacer(Modifier.weight(1f)) }
            }
        }
    }
}

/** "2026-07-17" → "17. 7. 2026" (jiný tvar se nechá, jak je). */
private fun formatDate(iso: String): String {
    val p = iso.take(10).split('-').mapNotNull { it.toIntOrNull() }
    return if (p.size == 3) "${p[2]}. ${p[1]}. ${p[0]}" else iso
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
        vm.downloadFor(e.season, e.episode)?.let { DownloadIcon(it) }
        if (p?.finished == true) {
            Spacer(Modifier.width(6.dp))
            Icon(Icons.Filled.CheckCircle, "Zhlédnuto", tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp))
        }
    }
}

@Composable
private fun StreamsSheet(vm: DetailViewModel, onDownload: (StreamInfo) -> Unit) {
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
            items(vm.displayStreams, key = { "${it.provider}-${it.ident}" }) { s ->
                StreamRow(
                    s,
                    resolving = vm.resolving == s.ident,
                    bitrate = if (s.isTranscode) null else s.estimatedBitrate(vm.detail?.runtimeMin),
                    tooHeavy = vm.tooHeavy(s),
                    bandwidth = vm.bandwidthBits,
                    downloaded = vm.target?.let { t -> vm.downloadFor(t.season, t.episode)?.ident == s.ident } == true,
                    onDownload = { onDownload(s) },
                ) { vm.play(s) }
                HorizontalDivider(Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.surfaceVariant)
            }
        }
    }
}

@Composable
private fun StreamRow(
    s: StreamInfo,
    resolving: Boolean,
    bitrate: Long?,
    tooHeavy: Boolean,
    bandwidth: Long?,
    downloaded: Boolean,
    onDownload: () -> Unit,
    onClick: () -> Unit,
) {
    val czech = s.audioLangs.any { it == "cs" || it == "sk" }
    Row(
        Modifier.fillMaxWidth().clickable(enabled = !resolving, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 10.dp)
            .alpha(if (s.isSuspicious) 0.5f else 1f),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                BadgeRow(s.qualityBadges())
                Spacer(Modifier.width(6.dp))
                Text(
                    providerLabel(s.provider),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Spacer(Modifier.height(4.dp))
            Text(
                if (s.isTranscode) {
                    // Převod má jen jednu zvukovou stopu a jinou velikost než původní soubor.
                    s.languageLine() + " · převod" + if (s.audioLangs.size > 1) " · jen 1 zvuková stopa" else ""
                } else {
                    s.languageLine() + " · " + formatSize(s.sizeBytes) + (bitrate?.let { " · ≈ " + formatBitrate(it) } ?: "")
                },
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = if (czech) FontWeight.SemiBold else FontWeight.Normal,
                color = if (czech) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
            )
            Text(s.filename, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (s.isSuspicious) {
                Text("Opakovaně nešel přehrát", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.error)
            }
            if (tooHeavy && bandwidth != null) {
                Text(
                    "Může sekat — tvoje připojení zvládne ≈ ${formatBitrate(bandwidth)}",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
        IconButton(onClick = onDownload, enabled = !downloaded) {
            Icon(
                if (downloaded) Icons.Filled.DownloadDone else Icons.Outlined.Download,
                if (downloaded) "Staženo" else "Stáhnout",
                tint = if (downloaded) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
            )
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

/** Malá ikona stavu stažení u epizody. */
@Composable
private fun DownloadIcon(d: app.kuro.core.DownloadInfo) {
    when (d.status) {
        app.kuro.core.DownloadStatus.DONE ->
            Icon(Icons.Filled.DownloadDone, "Staženo", tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp))
        app.kuro.core.DownloadStatus.FAILED ->
            Icon(Icons.Outlined.ErrorOutline, "Stažení selhalo", tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(20.dp))
        app.kuro.core.DownloadStatus.PAUSED ->
            Icon(Icons.Outlined.Download, "Pozastaveno", tint = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.size(20.dp))
        else -> CircularProgressIndicator(
            progress = { if (d.bytesTotal > 0) d.bytesDone.toFloat() / d.bytesTotal else 0f },
            modifier = Modifier.size(18.dp),
            strokeWidth = 2.dp,
        )
    }
}

/** Řádek se stavem stažení filmu. */
@Composable
private fun DownloadLine(d: app.kuro.core.DownloadInfo) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        DownloadIcon(d)
        Spacer(Modifier.width(8.dp))
        Text(
            when (d.status) {
                app.kuro.core.DownloadStatus.DONE -> "Staženo · ${formatSize(d.bytesTotal)} · přehraje se bez internetu"
                app.kuro.core.DownloadStatus.FAILED -> "Stažení selhalo: ${d.error ?: ""}"
                app.kuro.core.DownloadStatus.PAUSED -> "Stahování pozastaveno"
                else -> if (d.bytesTotal > 0) {
                    "Stahuji ${d.bytesDone * 100 / d.bytesTotal} % · ${formatSize(d.bytesDone)} z ${formatSize(d.bytesTotal)}"
                } else {
                    "Čeká na stažení"
                }
            },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
