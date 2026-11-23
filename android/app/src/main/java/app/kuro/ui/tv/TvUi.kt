package app.kuro.ui.tv

import android.widget.Toast
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CastConnected
import androidx.compose.material.icons.filled.Forward10
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Replay10
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material.icons.outlined.Cast
import androidx.compose.material.icons.outlined.Tv
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.kuro.kuroApp
import app.kuro.tv.TvDevice
import app.kuro.ui.common.formatPosition

/** Tlačítko "Přehrát na TV" (jako Cast v YouTube). Připojeno = plná ikona. */
@Composable
fun TvButton(modifier: Modifier = Modifier) {
    val tv = kuroApp().tv
    var picker by remember { mutableStateOf(false) }
    IconButton(
        onClick = { picker = true },
        modifier = modifier,
        colors = IconButtonDefaults.iconButtonColors(containerColor = Color.Black.copy(alpha = 0.45f)),
    ) {
        if (tv.connected != null) {
            Icon(Icons.Filled.CastConnected, "Televize připojená", tint = MaterialTheme.colorScheme.primary)
        } else {
            Icon(Icons.Outlined.Cast, "Přehrát na TV", tint = Color.White)
        }
    }
    if (picker) TvPicker(onDismiss = { picker = false })
}

@Composable
private fun TvPicker(onDismiss: () -> Unit) {
    val tv = kuroApp().tv
    DisposableEffect(Unit) {
        tv.startDiscovery()
        onDispose { tv.stopDiscovery() }
    }
    // Po připojení dialog zavřít.
    var wasConnecting by remember { mutableStateOf(false) }
    LaunchedEffect(tv.connecting, tv.connected) {
        if (wasConnecting && !tv.connecting && tv.connected != null) onDismiss()
        wasConnecting = tv.connecting
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Přehrát na televizi") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                tv.connected?.let { c ->
                    Text("Připojeno: ${c.name}", fontWeight = FontWeight.SemiBold)
                    Text(
                        "Co teď pustíš, poběží na televizi.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Spacer(Modifier.size(8.dp))
                }
                val others = tv.devices.filter { it.id != tv.connected?.id }
                if (others.isEmpty() && tv.connected == null) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                        Spacer(Modifier.width(12.dp))
                        Text("Hledám televize ve Wi-Fi…")
                    }
                    Text(
                        "Telefon i televize musí být ve stejné síti.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                others.forEach { d ->
                    Row(
                        Modifier.fillMaxWidth().clickable(enabled = !tv.connecting) { tv.connect(d) }.padding(vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(if (d.kind == TvDevice.Kind.CAST) Icons.Outlined.Cast else Icons.Outlined.Tv, null)
                        Spacer(Modifier.width(12.dp))
                        Column(Modifier.weight(1f)) {
                            Text(d.name)
                            Text(
                                if (d.kind == TvDevice.Kind.CAST) "Google Cast" else "DLNA",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
                if (tv.connecting) Text("Připojuji…", style = MaterialTheme.typography.bodySmall)
                tv.error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("Zavřít") } },
        dismissButton = {
            if (tv.connected != null) {
                TextButton(onClick = { tv.disconnect(); onDismiss() }) { Text("Odpojit") }
            }
        },
    )
}

/**
 * Ovladač dole nad navigací, když je připojená televize.
 * [aboveSystemBar]: pod lištou není navigace aplikace (detail) → odsadit
 * od systémových tlačítek / gesta, jinak by "Zastavit" padlo na "Domů".
 */
@Composable
fun TvMiniBar(aboveSystemBar: Boolean = false) {
    val tv = kuroApp().tv
    val context = LocalContext.current
    LaunchedEffect(tv.error) {
        tv.error?.let { Toast.makeText(context, it, Toast.LENGTH_LONG).show() }
    }
    val device = tv.connected ?: return
    val media = tv.nowPlaying
    Surface(tonalElevation = 6.dp, modifier = Modifier.fillMaxWidth()) {
        val insets = if (aboveSystemBar) Modifier.windowInsetsPadding(WindowInsets.navigationBars) else Modifier
        Column(insets.padding(horizontal = 12.dp, vertical = 6.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Filled.CastConnected, null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(10.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        media?.let { listOfNotNull(it.title, it.subtitle).joinToString(" · ") } ?: "Připojeno",
                        style = MaterialTheme.typography.bodyMedium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    val s = tv.status
                    Text(
                        if (media != null && s != null && s.durationSec > 0) {
                            "${device.name} · ${formatPosition((s.positionSec * 1000).toLong())} / " +
                                formatPosition((s.durationSec * 1000).toLong())
                        } else if (media != null) {
                            "${device.name} · načítá…"
                        } else {
                            "${device.name} · vyber, co pustit"
                        },
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (media != null) {
                    IconButton(onClick = { tv.seekBy(-10.0) }) { Icon(Icons.Filled.Replay10, "Zpět 10 s") }
                    IconButton(onClick = tv::togglePause) {
                        Icon(
                            if (tv.status?.playing == false) Icons.Filled.PlayArrow else Icons.Filled.Pause,
                            if (tv.status?.playing == false) "Přehrát" else "Pauza",
                        )
                    }
                    IconButton(onClick = { tv.seekBy(10.0) }) { Icon(Icons.Filled.Forward10, "Vpřed 10 s") }
                    IconButton(onClick = tv::stopPlayback) { Icon(Icons.Filled.Stop, "Zastavit") }
                }
            }
            val s = tv.status
            if (media != null && s != null && s.durationSec > 0) {
                var drag by remember { mutableStateOf<Float?>(null) }
                Slider(
                    value = drag ?: (s.positionSec / s.durationSec).toFloat().coerceIn(0f, 1f),
                    onValueChange = { drag = it },
                    onValueChangeFinished = {
                        drag?.let { tv.seekTo(it * s.durationSec) }
                        drag = null
                    },
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 4.dp),
                )
            }
        }
    }
}
