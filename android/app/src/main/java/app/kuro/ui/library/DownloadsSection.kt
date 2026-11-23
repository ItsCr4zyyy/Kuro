package app.kuro.ui.library

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.kuro.core.DownloadInfo
import app.kuro.core.DownloadStatus
import app.kuro.core.Provider
import app.kuro.download.Downloads
import app.kuro.player.PlayerActivity
import app.kuro.ui.common.formatSize
import coil3.compose.AsyncImage

/** Řádek staženého/stahovaného titulu v Knihovně. */
@Composable
fun DownloadRow(d: DownloadInfo, onOpen: (Long) -> Unit, onChanged: () -> Unit) {
    val context = LocalContext.current
    var confirmDelete by remember { mutableStateOf(false) }
    val title = d.title?.title ?: "Titul"
    val episode = if (d.season > 0) "S${d.season} E${d.episode}" else null
    val file = Downloads.playableFile(d)

    Row(
        Modifier.fillMaxWidth().clickable { onOpen(d.titleId) }.padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AsyncImage(
            model = d.title?.poster,
            contentDescription = null,
            contentScale = ContentScale.Crop,
            modifier = Modifier.width(52.dp).aspectRatio(2f / 3f).clip(RoundedCornerShape(6.dp)),
        )
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(listOfNotNull(title, episode).joinToString(" · "), style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val pct = if (d.bytesTotal > 0) (d.bytesDone * 100 / d.bytesTotal).toInt() else 0
            Text(
                when (d.status) {
                    DownloadStatus.DONE -> "Staženo · ${formatSize(d.bytesTotal)}"
                    DownloadStatus.RUNNING -> "Stahuji $pct % · ${formatSize(d.bytesDone)} z ${formatSize(d.bytesTotal)}"
                    DownloadStatus.QUEUED -> if (d.bytesDone > 0) "Čeká (síť / Wi-Fi) · $pct %" else "Čeká na stažení"
                    DownloadStatus.PAUSED -> "Pozastaveno · $pct %"
                    DownloadStatus.FAILED -> "Selhalo: ${d.error ?: "neznámá chyba"}"
                },
                style = MaterialTheme.typography.bodySmall,
                color = if (d.status == DownloadStatus.FAILED) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 2,
            )
            if (d.status != DownloadStatus.DONE && d.bytesTotal > 0) {
                Spacer(Modifier.height(4.dp))
                LinearProgressIndicator(
                    progress = { (d.bytesDone.toFloat() / d.bytesTotal).coerceIn(0f, 1f) },
                    modifier = Modifier.fillMaxWidth().height(3.dp),
                    drawStopIndicator = {},
                    gapSize = 0.dp,
                )
            }
        }
        when (d.status) {
            DownloadStatus.DONE -> if (file != null) {
                IconButton(onClick = {
                    PlayerActivity.start(
                        context, d.titleId, d.season, d.episode, d.provider ?: Provider.WEBSHARE, d.ident,
                        file.path, title, listOfNotNull(episode, "staženo").joinToString(" · "),
                    )
                }) { Icon(Icons.Filled.PlayArrow, "Přehrát", tint = MaterialTheme.colorScheme.primary) }
            }
            DownloadStatus.RUNNING, DownloadStatus.QUEUED -> IconButton(onClick = { Downloads.pause(context, d); onChanged() }) {
                Icon(Icons.Filled.Pause, "Pozastavit")
            }
            DownloadStatus.PAUSED, DownloadStatus.FAILED -> IconButton(onClick = { Downloads.resume(context, d); onChanged() }) {
                Icon(Icons.Outlined.Refresh, "Pokračovat")
            }
        }
        IconButton(onClick = { confirmDelete = true }) { Icon(Icons.Outlined.Delete, "Smazat") }
    }

    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Smazat stažené?") },
            text = { Text(listOfNotNull(title, episode).joinToString(" · ") + " se smaže z telefonu.") },
            confirmButton = {
                TextButton(onClick = {
                    confirmDelete = false
                    Downloads.delete(context, d)
                    onChanged()
                }) { Text("Smazat") }
            },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("Zrušit") } },
        )
    }
}
