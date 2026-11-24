package app.kuro.player

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Forward10
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PictureInPictureAlt
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Replay
import androidx.compose.material.icons.filled.Replay10
import androidx.compose.material.icons.filled.SkipNext
import androidx.compose.material.icons.filled.Subtitles
import androidx.compose.material.icons.filled.ZoomOutMap
import androidx.compose.material.icons.outlined.BrightnessMedium
import androidx.compose.material.icons.outlined.FileOpen
import androidx.compose.material.icons.outlined.VolumeUp
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import app.kuro.ui.common.formatPosition
import app.kuro.ui.common.langLabel
import kotlinx.coroutines.delay
import kotlin.math.abs

private const val CONTROLS_TIMEOUT_MS = 3500L

/** Co právě dělá gesto tažením. */
private enum class DragMode { NONE, SEEK, BRIGHTNESS, VOLUME }

@Composable
fun PlayerScreen(
    view: KuroMpvView,
    state: PlayerState,
    title: String,
    subtitle: String?,
    inPip: Boolean,
    reloading: Boolean,
    extras: PlayerExtras,
    actions: PlayerActions,
) {
    var controls by remember { mutableStateOf(true) }
    var tracksOpen by remember { mutableStateOf(false) }
    var fill by remember { mutableStateOf(false) }
    var interaction by remember { mutableIntStateOf(0) }

    // Gesta
    var dragMode by remember { mutableStateOf(DragMode.NONE) }
    var seekPreview by remember { mutableStateOf<Double?>(null) }
    var level by remember { mutableFloatStateOf(0f) }
    var doubleTapHint by remember { mutableStateOf<String?>(null) }

    BackHandler { if (tracksOpen) tracksOpen = false else actions.close() }

    // Ovládání se samo schová, když se přehrává.
    LaunchedEffect(controls, interaction, state.paused, tracksOpen) {
        if (controls && !state.paused && !tracksOpen) {
            delay(CONTROLS_TIMEOUT_MS)
            controls = false
        }
    }
    LaunchedEffect(doubleTapHint) {
        if (doubleTapHint != null) {
            delay(700)
            doubleTapHint = null
        }
    }

    Box(Modifier.fillMaxSize().background(Color.Black)) {
        AndroidView(factory = { view }, modifier = Modifier.fillMaxSize())

        if (inPip) return@Box

        // Vrstva gest přes celé video.
        Box(
            Modifier.fillMaxSize()
                .pointerInput(Unit) {
                    detectTapGestures(
                        onTap = { controls = !controls },
                        onDoubleTap = { o ->
                            when {
                                o.x < size.width / 3f -> { actions.seekBy(-10.0); doubleTapHint = "−10 s" }
                                o.x > size.width * 2f / 3f -> { actions.seekBy(10.0); doubleTapHint = "+10 s" }
                                else -> actions.togglePause()
                            }
                        },
                    )
                }
                .pointerInput(Unit) {
                    var start = Offset.Zero
                    var total = Offset.Zero
                    var startPos = 0.0
                    var startLevel = 0f
                    detectDragGestures(
                        onDragStart = { o ->
                            start = o
                            total = Offset.Zero
                            dragMode = DragMode.NONE
                        },
                        onDrag = { change, amount ->
                            change.consume()
                            total += amount
                            if (dragMode == DragMode.NONE && total.getDistance() > 24f) {
                                dragMode = when {
                                    abs(total.x) > abs(total.y) -> DragMode.SEEK.also { startPos = state.position }
                                    start.x < size.width / 2f -> DragMode.BRIGHTNESS.also { startLevel = actions.brightness() }
                                    else -> DragMode.VOLUME.also { startLevel = actions.volume() }
                                }
                            }
                            when (dragMode) {
                                // Celá šířka obrazovky = 3 minuty.
                                DragMode.SEEK -> seekPreview = (startPos + total.x / size.width * 180.0)
                                    .coerceIn(0.0, state.duration.coerceAtLeast(0.0))
                                DragMode.BRIGHTNESS -> {
                                    level = (startLevel - total.y / size.height).coerceIn(0f, 1f)
                                    actions.setBrightness(level)
                                }
                                DragMode.VOLUME -> {
                                    level = (startLevel - total.y / size.height).coerceIn(0f, 1f)
                                    actions.setVolume(level)
                                }
                                DragMode.NONE -> {}
                            }
                        },
                        onDragEnd = {
                            seekPreview?.let { actions.seekTo(it) }
                            seekPreview = null
                            dragMode = DragMode.NONE
                        },
                        onDragCancel = {
                            seekPreview = null
                            dragMode = DragMode.NONE
                        },
                    )
                },
        )

        // Načítání
        if ((state.buffering || reloading) && state.failed == null) {
            Column(Modifier.align(Alignment.Center), horizontalAlignment = Alignment.CenterHorizontally) {
                CircularProgressIndicator(Modifier.size(56.dp), color = MaterialTheme.colorScheme.primary)
                if (state.loaded && !reloading) {
                    Spacer(Modifier.height(12.dp))
                    Text(
                        "Načítám ${state.bufferPercent} %" +
                            if (state.downloadSpeed > 0) " · ${formatMbit(state.downloadSpeed)}" else "",
                        color = Color.White,
                        style = MaterialTheme.typography.labelLarge,
                        modifier = Modifier.background(Color.Black.copy(alpha = 0.5f), RoundedCornerShape(8.dp)).padding(horizontal = 10.dp, vertical = 4.dp),
                    )
                }
            }
        }

        // Ukazatele gest
        doubleTapHint?.let { Hint(it, Modifier.align(Alignment.Center)) }
        seekPreview?.let {
            Hint("${formatPosition((it * 1000).toLong())} / ${formatPosition((state.duration * 1000).toLong())}", Modifier.align(Alignment.Center))
        }
        if (dragMode == DragMode.BRIGHTNESS || dragMode == DragMode.VOLUME) {
            LevelIndicator(
                if (dragMode == DragMode.BRIGHTNESS) Icons.Outlined.BrightnessMedium else Icons.Outlined.VolumeUp,
                level,
                Modifier.align(if (dragMode == DragMode.BRIGHTNESS) Alignment.CenterStart else Alignment.CenterEnd).padding(32.dp),
            )
        }

        // Chyba
        state.failed?.let { msg ->
            Column(
                Modifier.align(Alignment.Center).padding(32.dp)
                    .background(Color.Black.copy(alpha = 0.8f), RoundedCornerShape(16.dp)).padding(24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text("Video nejde přehrát", style = MaterialTheme.typography.titleMedium, color = Color.White)
                Text(msg, style = MaterialTheme.typography.bodySmall, color = Color.White.copy(alpha = 0.7f))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Button(onClick = actions::retry, enabled = !reloading) { Text("Zkusit znovu") }
                    OutlinedButton(onClick = actions::close) { Text("Jiný zdroj") }
                }
            }
        }

        // Ovládání
        AnimatedVisibility(controls && state.failed == null, enter = fadeIn(), exit = fadeOut(), modifier = Modifier.fillMaxSize()) {
            Controls(
                state = state,
                title = title,
                subtitle = subtitle,
                fill = fill,
                onInteract = { interaction++ },
                onTracks = { tracksOpen = true },
                onZoom = {
                    fill = !fill
                    actions.setZoom(fill)
                },
                actions = actions,
            )
        }

        // Panel stop vpravo — uvnitř obrazovky, ne jako samostatné okno
        // (bottom sheet otevíral vlastní okno a vracel systémové lišty).
        if (tracksOpen) {
            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.4f)).clickable(
                interactionSource = remember { androidx.compose.foundation.interaction.MutableInteractionSource() },
                indication = null,
            ) { tracksOpen = false })
        }
        AnimatedVisibility(
            tracksOpen,
            enter = slideInHorizontally { it },
            exit = slideOutHorizontally { it },
            modifier = Modifier.align(Alignment.CenterEnd),
        ) {
            TracksPanel(state, extras, actions)
        }

        // Nabídka dalšího dílu (i když je ovládání schované).
        extras.nextUp?.let { next ->
            if (!tracksOpen) {
                NextUpCard(next, actions, Modifier.align(Alignment.BottomEnd).safeDrawingPadding().padding(24.dp))
            }
        }
    }
}

@Composable
private fun NextUpCard(next: NextUp, actions: PlayerActions, modifier: Modifier) {
    androidx.compose.material3.Surface(
        modifier = modifier.width(300.dp),
        shape = RoundedCornerShape(16.dp),
        color = Color.Black.copy(alpha = 0.85f),
        contentColor = Color.White,
    ) {
        Column(Modifier.padding(16.dp)) {
            Text(
                next.secondsLeft?.let { "Další díl za $it s" } ?: "Další díl",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.primary,
            )
            Text(next.label, style = MaterialTheme.typography.titleSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Spacer(Modifier.height(12.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                androidx.compose.material3.Button(onClick = actions::playNextNow) {
                    Icon(Icons.Filled.SkipNext, null)
                    Spacer(Modifier.width(6.dp))
                    Text("Přehrát")
                }
                androidx.compose.material3.OutlinedButton(onClick = actions::cancelNext) { Text("Zrušit", color = Color.White) }
            }
        }
    }
}

@Composable
private fun Controls(
    state: PlayerState,
    title: String,
    subtitle: String?,
    fill: Boolean,
    onInteract: () -> Unit,
    onTracks: () -> Unit,
    onZoom: () -> Unit,
    actions: PlayerActions,
) {
    var scrubbing by remember { mutableStateOf<Float?>(null) }
    Box(
        Modifier.fillMaxSize().background(
            Brush.verticalGradient(
                0f to Color.Black.copy(alpha = 0.7f),
                0.25f to Color.Transparent,
                0.7f to Color.Transparent,
                1f to Color.Black.copy(alpha = 0.8f),
            ),
        ).safeDrawingPadding(),
    ) {
        // Nahoře: zpět, název, PiP
        Row(Modifier.fillMaxWidth().align(Alignment.TopCenter).padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = actions::close) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Zpět", tint = Color.White) }
            Column(Modifier.weight(1f)) {
                Text(title, color = Color.White, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                subtitle?.let { Text(it, color = Color.White.copy(alpha = 0.7f), style = MaterialTheme.typography.bodySmall, maxLines = 1) }
            }
            IconButton(onClick = actions::enterPip) { Icon(Icons.Filled.PictureInPictureAlt, "Obraz v obraze", tint = Color.White) }
        }

        // Uprostřed: −10 / play / +10
        Row(
            Modifier.align(Alignment.Center),
            horizontalArrangement = Arrangement.spacedBy(40.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            RoundButton(Icons.Filled.Replay10, "Zpět 10 s", 48.dp) { actions.seekBy(-10.0); onInteract() }
            RoundButton(
                when {
                    state.ended -> Icons.Filled.Replay
                    state.paused -> Icons.Filled.PlayArrow
                    else -> Icons.Filled.Pause
                },
                "Přehrát / pauza",
                72.dp,
            ) { actions.togglePause(); onInteract() }
            RoundButton(Icons.Filled.Forward10, "Vpřed 10 s", 48.dp) { actions.seekBy(10.0); onInteract() }
        }

        // Dole: čas, posuvník, stopy, zoom
        Column(Modifier.align(Alignment.BottomCenter).fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp)) {
            val dur = state.duration.coerceAtLeast(0.0)
            Box(contentAlignment = Alignment.Center) {
                if (dur > 0) {
                    // Načteno do cache (tenký pruh pod posuvníkem).
                    LinearProgressIndicator(
                        progress = { (state.cachedUntil / dur).toFloat().coerceIn(0f, 1f) },
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 10.dp).height(2.dp),
                        color = Color.White.copy(alpha = 0.35f),
                        trackColor = Color.Transparent,
                        drawStopIndicator = {},
                        gapSize = 0.dp,
                    )
                }
                Slider(
                    value = scrubbing ?: if (dur > 0) (state.position / dur).toFloat() else 0f,
                    onValueChange = {
                        scrubbing = it
                        onInteract()
                    },
                    onValueChangeFinished = {
                        scrubbing?.let { actions.seekTo(it * dur) }
                        scrubbing = null
                    },
                    enabled = dur > 0,
                    colors = SliderDefaults.colors(inactiveTrackColor = Color.White.copy(alpha = 0.2f)),
                )
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                val shown = scrubbing?.let { it * dur } ?: state.position
                Text(
                    "${formatPosition((shown * 1000).toLong())} / ${formatPosition((dur * 1000).toLong())}",
                    color = Color.White,
                    style = MaterialTheme.typography.labelLarge,
                    modifier = Modifier.weight(1f),
                )
                IconButton(onClick = onTracks) { Icon(Icons.Filled.Subtitles, "Zvuk a titulky", tint = Color.White) }
                IconButton(onClick = onZoom) {
                    Icon(Icons.Filled.ZoomOutMap, if (fill) "Celý obraz" else "Vyplnit", tint = if (fill) MaterialTheme.colorScheme.primary else Color.White)
                }
            }
        }
    }
}

@Composable
private fun RoundButton(icon: ImageVector, desc: String, size: androidx.compose.ui.unit.Dp, onClick: () -> Unit) {
    Box(
        Modifier.size(size).clip(RoundedCornerShape(50)).background(Color.Black.copy(alpha = 0.35f)).clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) { Icon(icon, desc, tint = Color.White, modifier = Modifier.size(size * 0.6f)) }
}

@Composable
private fun Hint(text: String, modifier: Modifier) {
    Text(
        text,
        color = Color.White,
        style = MaterialTheme.typography.titleLarge,
        fontWeight = FontWeight.SemiBold,
        modifier = modifier.background(Color.Black.copy(alpha = 0.6f), RoundedCornerShape(12.dp)).padding(horizontal = 20.dp, vertical = 10.dp),
    )
}

@Composable
private fun LevelIndicator(icon: ImageVector, level: Float, modifier: Modifier) {
    Column(
        modifier.background(Color.Black.copy(alpha = 0.6f), RoundedCornerShape(12.dp)).padding(12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Icon(icon, null, tint = Color.White)
        Spacer(Modifier.height(8.dp))
        Text("${(level * 100).toInt()} %", color = Color.White, style = MaterialTheme.typography.labelLarge)
    }
}

/** Bajty/s → "12 Mbit/s". */
private fun formatMbit(bytesPerSec: Long): String = "%.0f Mbit/s".format(java.util.Locale.ROOT, bytesPerSec * 8 / 1e6)

private fun trackLabel(t: Track): String {
    val lang = t.lang?.let { langLabel(isoFromMpv(it)) }
    val parts = listOfNotNull(
        lang,
        t.title?.takeIf { it.isNotBlank() && it != t.lang },
        t.codec?.uppercase(),
        t.channels?.let { ch -> if (ch >= 6) "${ch - 1}.1" else if (ch == 2) "stereo" else null },
        "externí".takeIf { t.external },
    )
    return parts.joinToString(" · ").ifEmpty { "Stopa ${t.id}" }
}

/** mpv hlásí jazyky ISO 639-2 ("cze", "eng") — převod na krátké štítky. */
private fun isoFromMpv(lang: String): String = when (lang.lowercase()) {
    "cze", "ces", "cs" -> "cs"
    "slo", "slk", "sk" -> "sk"
    "eng", "en" -> "en"
    "jpn", "ja" -> "ja"
    "ger", "deu", "de" -> "de"
    "kor", "ko" -> "ko"
    else -> lang
}

@Composable
private fun TracksPanel(state: PlayerState, extras: PlayerExtras, actions: PlayerActions) {
    androidx.compose.material3.Surface(
        modifier = Modifier.fillMaxHeight().width(360.dp),
        color = MaterialTheme.colorScheme.surface.copy(alpha = 0.97f),
        // Průhledná barva není ve schématu, takže contentColorFor() by
        // vrátil tmavý text — nastavit výslovně.
        contentColor = MaterialTheme.colorScheme.onSurface,
        shape = RoundedCornerShape(topStart = 16.dp, bottomStart = 16.dp),
    ) {
        LazyColumn(Modifier.safeDrawingPadding().padding(vertical = 12.dp)) {
            item { SheetHeader("Zvuk") }
            if (state.audioTracks.isEmpty()) item { SheetNote("Žádná zvuková stopa") }
            items(state.audioTracks, key = { "a${it.id}" }) { t ->
                TrackRow(trackLabel(t), t.selected) { actions.selectAudio(t.id) }
            }
            item {
                HorizontalDivider(Modifier.padding(vertical = 8.dp))
                SheetHeader("Titulky")
                TrackRow("Vypnuto", state.subtitleTracks.none { it.selected }) { actions.selectSubtitle(null) }
            }
            items(state.subtitleTracks, key = { "s${it.id}" }) { t ->
                TrackRow(trackLabel(t), t.selected) { actions.selectSubtitle(t.id) }
            }
            item {
                Row(
                    Modifier.fillMaxWidth().clickable { actions.loadSubtitleFile() }.padding(horizontal = 24.dp, vertical = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(Icons.Outlined.FileOpen, null, tint = MaterialTheme.colorScheme.primary)
                    Spacer(Modifier.width(16.dp))
                    Text("Načíst titulky ze souboru (SRT, ASS…)")
                }
            }
            item {
                HorizontalDivider(Modifier.padding(vertical = 8.dp))
                SheetHeader("Rychlost")
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = 20.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    PlayerExtras.SPEEDS.forEach { sp ->
                        androidx.compose.material3.FilterChip(
                            selected = extras.speed == sp,
                            onClick = { actions.setSpeed(sp) },
                            label = { Text(if (sp == 1.0) "1×" else "%s×".format(sp.toString().trimEnd('0').trimEnd('.').replace('.', ','))) },
                        )
                    }
                }
                HorizontalDivider(Modifier.padding(vertical = 8.dp))
                SheetHeader("Časovač vypnutí")
                if (extras.sleepAtMs > 0) {
                    val left = ((extras.sleepAtMs - System.currentTimeMillis()) / 60_000L).coerceAtLeast(0) + 1
                    SheetNote("Pozastaví se asi za $left min")
                }
            }
            items(SleepTimer.entries, key = { "t${it.name}" }) { t ->
                TrackRow(t.label, extras.sleep == t) { actions.setSleepTimer(t) }
            }
        }
    }
}

@Composable
private fun SheetHeader(text: String) {
    Text(text, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp))
}

@Composable
private fun SheetNote(text: String) {
    Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp))
}

@Composable
private fun TrackRow(label: String, selected: Boolean, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 24.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, modifier = Modifier.weight(1f), fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal)
        if (selected) Icon(Icons.Filled.Check, "Vybráno", tint = MaterialTheme.colorScheme.primary)
    }
}
