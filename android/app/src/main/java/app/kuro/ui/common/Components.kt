package app.kuro.ui.common

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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.unit.Dp
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.CloudOff
import androidx.compose.material.icons.outlined.Movie
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.kuro.core.TitleCard

import coil3.compose.AsyncImage
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.runtime.getValue
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.HazeStyle
import dev.chrisbanes.haze.HazeTint
import dev.chrisbanes.haze.hazeEffect
import androidx.compose.material.icons.filled.PlayArrow

/** Široký displej: rozložený ohebný telefon, tablet, telefon na šířku. */
const val WIDE_DP = 600

@Composable
fun isWideScreen(): Boolean = LocalConfiguration.current.screenWidthDp >= WIDE_DP

/**
 * Výška velkého obrázku nahoře: [ratio] šířky, ale nejvýš [maxOfHeight]
 * výšky displeje (jinak by na rozloženém/otočeném displeji zabral celou obrazovku).
 */
@Composable
fun heroHeight(ratio: Float, maxOfHeight: Float, cap: Dp = Dp.Infinity): Dp {
    val c = LocalConfiguration.current
    val h = minOf(c.screenWidthDp * ratio, c.screenHeightDp * maxOfHeight).dp
    return if (h > cap) cap else h
}

/** Omezí šířku obsahu a vycentruje ho (na telefonu se nic nemění). */
fun Modifier.readableWidth(max: Dp = 720.dp): Modifier =
    fillMaxWidth().wrapContentWidth(Alignment.CenterHorizontally).widthIn(max = max).fillMaxWidth()

/**
 * Rozmazaný pruh pod stavovou lištou (hodiny, Wi-Fi), když pod ní odjede
 * obsah — jinak ikony splývají s plakáty. Obsah musí mít `hazeSource(haze)`.
 * Na Androidu < 12 je místo rozmazání poloprůhledná barva pozadí.
 */
@Composable
fun BoxScope.StatusBarBlur(haze: HazeState, visible: Boolean) {
    val alpha by animateFloatAsState(if (visible) 1f else 0f, label = "statusBarBlur")
    if (alpha <= 0f) return
    val bg = MaterialTheme.colorScheme.background
    Box(
        Modifier.align(Alignment.TopCenter).fillMaxWidth()
            .hazeEffect(
                haze,
                HazeStyle(
                    backgroundColor = bg,
                    tints = listOf(HazeTint(bg.copy(alpha = 0.55f))),
                    blurRadius = 24.dp,
                    noiseFactor = 0f,
                    fallbackTint = HazeTint(bg.copy(alpha = 0.92f)),
                ),
            ) {
                this.alpha = alpha
                // Spodní okraj se jemně ztrácí, ať to není ostrá hrana.
                mask = Brush.verticalGradient(0f to Color.Black, 0.6f to Color.Black, 1f to Color.Transparent)
            }
            .windowInsetsPadding(WindowInsets.statusBars)
            .height(14.dp),
    )
}

private val PosterShape = RoundedCornerShape(10.dp)

/** Plakát s názvem pod ním. `progress` 0..1 = rozkoukáno. */
@Composable
fun PosterCard(
    card: TitleCard,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    progress: Float? = null,
    caption: String? = null,
) {
    Column(modifier.width(120.dp).clickable(onClick = onClick)) {
        Box(
            Modifier.fillMaxWidth().aspectRatio(2f / 3f).clip(PosterShape)
                .background(MaterialTheme.colorScheme.surfaceVariant),
        ) {
            if (card.poster != null) {
                AsyncImage(
                    model = card.poster,
                    contentDescription = card.title,
                    contentScale = ContentScale.Crop,
                    modifier = Modifier.fillMaxSize(),
                )
            } else {
                Icon(
                    Icons.Outlined.Movie, null,
                    Modifier.align(Alignment.Center).size(32.dp),
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            if (progress != null) {
                LinearProgressIndicator(
                    progress = { progress.coerceIn(0f, 1f) },
                    modifier = Modifier.align(Alignment.BottomCenter).fillMaxWidth().height(3.dp),
                    drawStopIndicator = {},
                    gapSize = 0.dp,
                )
            }
        }
        Spacer(Modifier.height(6.dp))
        Text(
            card.title,
            style = MaterialTheme.typography.bodyMedium,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            caption ?: card.subtitle(),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
fun SectionTitle(text: String, modifier: Modifier = Modifier, horizontalPadding: Dp = 16.dp) {
    Text(
        text,
        style = MaterialTheme.typography.titleMedium,
        fontWeight = FontWeight.SemiBold,
        modifier = modifier.padding(horizontal = horizontalPadding, vertical = 8.dp),
    )
}

/** Vodorovná řada plakátů s nadpisem. */
@Composable
fun PosterRow(title: String, cards: List<TitleCard>, onOpen: (TitleCard) -> Unit) {
    if (cards.isEmpty()) return
    Column {
        SectionTitle(title)
        LazyRow(
            contentPadding = PaddingValues(horizontal = 16.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            items(cards, key = { it.id }) { PosterCard(it, onClick = { onOpen(it) }) }
        }
    }
}

@Composable
fun Loading(modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
}

@Composable
fun Message(
    text: String,
    modifier: Modifier = Modifier,
    icon: ImageVector = Icons.Outlined.CloudOff,
    action: Pair<String, () -> Unit>? = null,
) {
    Column(
        modifier.fillMaxWidth().padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Icon(icon, null, tint = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.size(40.dp))
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodyMedium)
        action?.let { (label, onClick) -> Button(onClick = onClick) { Text(label) } }
    }
}

/** Malý štítek (kvalita, jazyk). */
@Composable
fun Badge(text: String, highlight: Boolean = false) {
    Surface(
        shape = RoundedCornerShape(6.dp),
        color = if (highlight) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceVariant,
    ) {
        Text(
            text,
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.SemiBold,
            color = if (highlight) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
        )
    }
}

@Composable
fun BadgeRow(badges: List<String>, highlightFirst: Boolean = false) {
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        badges.forEachIndexed { i, b -> Badge(b, highlight = highlightFirst && i == 0) }
    }
}

/**
 * Rozkoukaný titul: široká karta s obrázkem z filmu, průběhem a tím,
 * kolik zbývá (odlišená od běžných plakátů).
 */
@Composable
fun ContinueCard(item: app.kuro.core.ContinueItem, onClick: () -> Unit) {
    val card = item.title
    val p = item.progress
    val fraction = if (p.durationMs > 0) (p.positionMs.toFloat() / p.durationMs).coerceIn(0f, 1f) else 0f
    val episode = if (p.season > 0) "S${p.season} E${p.episode}" else null
    val detail = when {
        p.season > 0 && p.positionMs == 0L -> "další díl"
        p.durationMs > 0 -> "zbývá ${formatRemaining(p.durationMs - p.positionMs)}"
        else -> formatPosition(p.positionMs)
    }
    Column(Modifier.width(240.dp).clickable(onClick = onClick)) {
        Box(
            Modifier.fillMaxWidth().aspectRatio(16f / 9f).clip(PosterShape)
                .background(MaterialTheme.colorScheme.surfaceVariant),
        ) {
            AsyncImage(
                model = card.background ?: card.poster,
                contentDescription = card.title,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
            Box(Modifier.fillMaxSize().background(Brush.verticalGradient(0.5f to Color.Transparent, 1f to Color.Black.copy(alpha = 0.7f))))
            Surface(
                shape = androidx.compose.foundation.shape.CircleShape,
                color = Color.Black.copy(alpha = 0.55f),
                contentColor = Color.White,
                modifier = Modifier.align(Alignment.Center).size(44.dp),
            ) {
                Icon(Icons.Filled.PlayArrow, "Pokračovat", Modifier.padding(10.dp))
            }
            if (fraction > 0f) {
                LinearProgressIndicator(
                    progress = { fraction },
                    modifier = Modifier.align(Alignment.BottomCenter).fillMaxWidth().height(3.dp),
                    trackColor = Color.White.copy(alpha = 0.25f),
                    drawStopIndicator = {},
                    gapSize = 0.dp,
                )
            }
        }
        Spacer(Modifier.height(6.dp))
        Text(card.title, style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(
            listOfNotNull(episode, detail).joinToString(" · "),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
        )
    }
}

/** 1 260 000 ms → "21 min", 4 000 000 → "1 h 7 min" */
private fun formatRemaining(ms: Long): String {
    val min = ((ms + 59_999) / 60_000).coerceAtLeast(1)
    return if (min >= 60) "${min / 60} h ${min % 60} min" else "$min min"
}
