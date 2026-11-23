package app.kuro.player

import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import `is`.xyz.mpv.MPV
import `is`.xyz.mpv.MPVNode

/** Zvuková stopa nebo titulky tak, jak je hlásí mpv (`track-list`). */
data class Track(
    val id: Int,
    val type: String,
    val lang: String?,
    val title: String?,
    val codec: String?,
    val channels: Int?,
    val external: Boolean,
    val selected: Boolean,
)

/**
 * Stav přehrávače pro Compose. mpv volá observer ze svého vlákna, takže
 * se všechno přehazuje na hlavní vlákno.
 */
class PlayerState : MPV.EventObserver {
    var position by mutableDoubleStateOf(0.0)
        private set
    var duration by mutableDoubleStateOf(0.0)
        private set
    var cachedUntil by mutableDoubleStateOf(0.0)
        private set
    var paused by mutableStateOf(false)
        private set
    var buffering by mutableStateOf(true)
        private set
    var loaded by mutableStateOf(false)
        private set
    var ended by mutableStateOf(false)
        private set
    var aspect by mutableDoubleStateOf(16.0 / 9.0)
        private set
    var tracks by mutableStateOf<List<Track>>(emptyList())
        private set

    /** Soubor se nepodařilo otevřít / přehrávání spadlo. */
    var failed by mutableStateOf<String?>(null)
        private set

    /** Volá se po načtení souboru (jednou) a po chybě. */
    var onLoaded: (() -> Unit)? = null
    var onFailed: ((String) -> Unit)? = null

    private val main = Handler(Looper.getMainLooper())

    val audioTracks: List<Track> get() = tracks.filter { it.type == "audio" }
    val subtitleTracks: List<Track> get() = tracks.filter { it.type == "sub" }

    fun clearFailure() {
        failed = null
        buffering = true
    }

    override fun eventProperty(property: String) {}

    override fun eventProperty(property: String, value: Long) {}

    override fun eventProperty(property: String, value: Boolean) {
        main.post {
            when (property) {
                "pause" -> paused = value
                "paused-for-cache" -> buffering = value
                "eof-reached" -> ended = value
            }
        }
    }

    override fun eventProperty(property: String, value: String) {}

    override fun eventProperty(property: String, value: Double) {
        main.post {
            when (property) {
                "time-pos" -> position = value
                "duration" -> duration = value
                "demuxer-cache-time" -> cachedUntil = value
                "video-params/aspect" -> if (value > 0) aspect = value
            }
        }
    }

    override fun eventProperty(property: String, value: MPVNode) {
        if (property != "track-list") return
        val parsed = value.asArray().orEmpty().mapNotNull { n ->
            Track(
                id = n["id"]?.asInt()?.toInt() ?: return@mapNotNull null,
                type = n["type"]?.asString() ?: return@mapNotNull null,
                lang = n["lang"]?.asString(),
                title = n["title"]?.asString(),
                codec = n["codec"]?.asString(),
                channels = n["demux-channel-count"]?.asInt()?.toInt(),
                external = n["external"]?.asBoolean() == true,
                selected = n["selected"]?.asBoolean() == true,
            )
        }
        main.post { tracks = parsed }
    }

    override fun event(eventId: Int, data: MPVNode) {
        val e = MPV.mpvEvent
        when (eventId) {
            e.MPV_EVENT_FILE_LOADED -> main.post {
                loaded = true
                buffering = false
                failed = null
                onLoaded?.invoke()
            }
            e.MPV_EVENT_PLAYBACK_RESTART -> main.post { buffering = false }
            e.MPV_EVENT_END_FILE -> {
                val reason = data["reason"]?.asString()
                if (reason == "error") {
                    val msg = data["error"]?.asString() ?: "přehrávání selhalo"
                    main.post {
                        failed = msg
                        buffering = false
                        onFailed?.invoke(msg)
                    }
                }
            }
        }
    }
}
