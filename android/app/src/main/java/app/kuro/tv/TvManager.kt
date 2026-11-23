package app.kuro.tv

import android.content.Context
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.kuro.core.KuroCore
import app.kuro.core.Provider
import app.kuro.core.SubtitleInfo
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/** Televize v síti. */
data class TvDevice(val id: String, val name: String, val kind: Kind) {
    enum class Kind { CAST, DLNA }
}

/** Co pustit na televizi + k čemu uložit pozici. */
data class TvMedia(
    /** http(s) odkaz nebo cesta ke staženému souboru. */
    val url: String,
    val title: String,
    val subtitle: String?,
    val subtitles: List<SubtitleInfo>,
    val startSec: Long,
    val titleId: Long,
    val season: Int,
    val episode: Int,
    val provider: Provider,
    val ident: String,
)

data class TvStatus(val positionSec: Double, val durationSec: Double, val playing: Boolean, val ended: Boolean = false)

/** Jedna technologie (Google Cast / DLNA). */
interface TvBackend {
    fun startDiscovery(onFound: (List<TvDevice>) -> Unit)
    fun stopDiscovery()
    suspend fun connect(device: TvDevice)
    suspend fun load(media: TvMedia)
    suspend fun setPaused(paused: Boolean)
    suspend fun seek(sec: Double)
    suspend fun status(): TvStatus?
    suspend fun stop()
    fun disconnect()
}

/**
 * Přehrávání na televizi (jako tlačítko Cast v YouTube): připojit se k TV,
 * pak se všechno pouští tam. Telefon slouží jako ovladač a průběžně ukládá
 * pozici, takže "Pokračovat" funguje i po sledování na TV.
 */
class TvManager(context: Context, private val core: KuroCore) {
    private val app = context.applicationContext
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private val relay = RelayServer(app)
    private val backends: Map<TvDevice.Kind, TvBackend> = buildMap {
        // Cast jen tam, kde jsou Google Play služby.
        CastBackend.createOrNull(app, relay)?.let { put(TvDevice.Kind.CAST, it) }
        put(TvDevice.Kind.DLNA, DlnaBackend(app, relay))
    }

    var devices by mutableStateOf<List<TvDevice>>(emptyList())
        private set
    var connected by mutableStateOf<TvDevice?>(null)
        private set
    var connecting by mutableStateOf(false)
        private set
    var nowPlaying by mutableStateOf<TvMedia?>(null)
        private set
    var status by mutableStateOf<TvStatus?>(null)
        private set
    var error by mutableStateOf<String?>(null)

    private val found = mutableMapOf<TvDevice.Kind, List<TvDevice>>()
    private var poll: Job? = null

    fun startDiscovery() {
        backends.forEach { (kind, b) ->
            b.startDiscovery { list ->
                scope.launch {
                    found[kind] = list
                    devices = found.values.flatten().sortedBy { it.name.lowercase() }
                }
            }
        }
    }

    fun stopDiscovery() = backends.values.forEach { it.stopDiscovery() }

    fun connect(device: TvDevice) {
        scope.launch {
            connecting = true
            error = null
            runCatching {
                if (connected != null) disconnect()
                backends.getValue(device.kind).connect(device)
                connected = device
            }.onFailure {
                Log.w(TAG, "connect", it)
                error = "K televizi ${device.name} se nejde připojit."
            }
            connecting = false
        }
    }

    fun play(media: TvMedia) {
        val d = connected ?: return
        scope.launch {
            error = null
            savePosition()
            nowPlaying = media
            status = null
            runCatching { backends.getValue(d.kind).load(media) }
                .onFailure {
                    Log.w(TAG, "load", it)
                    nowPlaying = null
                    error = it.message ?: "Televize video nepřehrála."
                }
            startPolling()
        }
    }

    fun togglePause() {
        val d = connected ?: return
        val s = status ?: return
        scope.launch {
            runCatching { backends.getValue(d.kind).setPaused(s.playing) }
            status = s.copy(playing = !s.playing)
        }
    }

    fun seekTo(sec: Double) {
        val d = connected ?: return
        scope.launch {
            runCatching { backends.getValue(d.kind).seek(sec.coerceAtLeast(0.0)) }
            status = status?.copy(positionSec = sec)
        }
    }

    fun seekBy(sec: Double) = status?.let { seekTo(it.positionSec + sec) }

    /** Zastavit přehrávání (TV zůstane připojená). */
    fun stopPlayback() {
        val d = connected ?: return
        scope.launch {
            savePosition()
            runCatching { backends.getValue(d.kind).stop() }
            nowPlaying = null
            status = null
            relay.stopServing()
        }
    }

    fun disconnect() {
        val d = connected ?: return
        savePosition()
        poll?.cancel()
        scope.launch { runCatching { backends.getValue(d.kind).stop() } }
        backends.getValue(d.kind).disconnect()
        relay.stopServing()
        connected = null
        nowPlaying = null
        status = null
    }

    private fun startPolling() {
        poll?.cancel()
        poll = scope.launch {
            var tick = 0
            while (isActive) {
                delay(2_000)
                val d = connected ?: break
                val s = runCatching { backends.getValue(d.kind).status() }.getOrNull() ?: continue
                status = s
                // Pozice každých ~10 s, jako v přehrávači v telefonu.
                if (++tick % 5 == 0 || s.ended) savePosition()
                if (s.ended) {
                    nowPlaying = null
                    relay.stopServing()
                    break
                }
            }
        }
    }

    private fun savePosition() {
        val m = nowPlaying ?: return
        val s = status ?: return
        if (s.durationSec <= 0) return
        val pos = if (s.ended) s.durationSec else s.positionSec
        runCatching {
            core.saveProgress(
                m.titleId, m.season, m.episode,
                (pos * 1000).toLong(), (s.durationSec * 1000).toLong(), m.provider, m.ident,
            )
        }
    }

    companion object {
        private const val TAG = "KuroTv"
    }
}
