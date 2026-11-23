package app.kuro.tv

import android.content.Context
import android.util.Log
import androidx.mediarouter.media.MediaRouteSelector
import androidx.mediarouter.media.MediaRouter
import com.google.android.gms.cast.CastMediaControlIntent
import com.google.android.gms.cast.MediaInfo
import com.google.android.gms.cast.MediaLoadRequestData
import com.google.android.gms.cast.MediaMetadata
import com.google.android.gms.cast.MediaSeekOptions
import com.google.android.gms.cast.MediaStatus
import com.google.android.gms.cast.MediaTrack
import com.google.android.gms.cast.framework.CastContext
import com.google.android.gms.cast.framework.CastOptions
import com.google.android.gms.cast.framework.OptionsProvider
import com.google.android.gms.cast.framework.SessionProvider
import com.google.android.gms.cast.framework.media.RemoteMediaClient
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailability
import com.google.android.gms.common.api.PendingResult
import com.google.android.gms.common.api.Result
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeout
import kotlin.coroutines.resume

/** Google Cast nastavení: výchozí přijímač Googlu (bez vlastní registrace). */
class CastOptionsProvider : OptionsProvider {
    override fun getCastOptions(context: Context): CastOptions =
        CastOptions.Builder()
            .setReceiverApplicationId(CastMediaControlIntent.DEFAULT_MEDIA_RECEIVER_APPLICATION_ID)
            .setStopReceiverApplicationWhenEndingSession(true)
            .build()

    override fun getAdditionalSessionProviders(context: Context): List<SessionProvider>? = null
}

/**
 * Google Cast (Chromecast, Google TV, Android TV) — stejné jako tlačítko
 * Cast v YouTube. Televize si video stáhne sama, telefon jen ovládá.
 * Přijímač Googlu umí MP4 (převody z Přehraj.to/Hellspy) a WebM;
 * MKV s AC3/DTS zvukem často ne.
 */
class CastBackend private constructor(
    private val context: Context,
    private val cast: CastContext,
    private val relay: RelayServer,
) : TvBackend {
    private val router = MediaRouter.getInstance(context)
    private val selector = MediaRouteSelector.Builder()
        .addControlCategory(CastMediaControlIntent.categoryForCast(CastMediaControlIntent.DEFAULT_MEDIA_RECEIVER_APPLICATION_ID))
        .build()
    private var callback: MediaRouter.Callback? = null

    private fun routes() = router.routes.filter { !it.isDefault && it.matchesSelector(selector) }

    override fun startDiscovery(onFound: (List<TvDevice>) -> Unit) {
        stopDiscovery()
        val cb = object : MediaRouter.Callback() {
            fun emit() = onFound(routes().map { TvDevice(it.id, it.name, TvDevice.Kind.CAST) })
            override fun onRouteAdded(router: MediaRouter, route: MediaRouter.RouteInfo) = emit()
            override fun onRouteChanged(router: MediaRouter, route: MediaRouter.RouteInfo) = emit()
            override fun onRouteRemoved(router: MediaRouter, route: MediaRouter.RouteInfo) = emit()
        }
        callback = cb
        router.addCallback(selector, cb, MediaRouter.CALLBACK_FLAG_PERFORM_ACTIVE_SCAN)
        cb.emit()
    }

    override fun stopDiscovery() {
        callback?.let { router.removeCallback(it) }
        callback = null
    }

    private fun client(): RemoteMediaClient? = cast.sessionManager.currentCastSession?.remoteMediaClient

    override suspend fun connect(device: TvDevice) {
        val route = routes().firstOrNull { it.id == device.id } ?: error("Televize už není v síti.")
        // Výběr trasy spustí Cast relaci (framework sleduje MediaRouter).
        router.selectRoute(route)
        withTimeout(20_000) {
            while (cast.sessionManager.currentCastSession?.isConnected != true) delay(300)
        }
    }

    override suspend fun load(media: TvMedia) {
        val c = client() ?: error("Televize se odpojila.")
        val mime = guessMime(media.url, media.provider)
        // Stažený soubor přes telefon; online odkaz si TV stáhne sama.
        val url = if (media.url.startsWith("/")) relay.serve(media.url, mime) ?: error("Telefon není ve Wi-Fi.") else media.url
        val meta = MediaMetadata(if (media.season > 0) MediaMetadata.MEDIA_TYPE_TV_SHOW else MediaMetadata.MEDIA_TYPE_MOVIE).apply {
            putString(MediaMetadata.KEY_TITLE, media.title)
            media.subtitle?.let { putString(MediaMetadata.KEY_SUBTITLE, it) }
        }
        // Titulky přes telefon — přijímač je bez CORS hlavičky nenačte.
        val tracks = media.subtitles.mapIndexedNotNull { i, s ->
            val sub = relay.serve(s.url, "text/vtt", cors = true) ?: return@mapIndexedNotNull null
            MediaTrack.Builder(i + 1L, MediaTrack.TYPE_TEXT)
                .setSubtype(MediaTrack.SUBTYPE_SUBTITLES)
                .setContentId(sub)
                .setContentType("text/vtt")
                .setLanguage(s.lang)
                .setName(s.label.ifBlank { s.lang.uppercase() })
                .build()
        }
        val info = MediaInfo.Builder(url)
            .setStreamType(MediaInfo.STREAM_TYPE_BUFFERED)
            .setContentType(mime)
            .setMetadata(meta)
            .setMediaTracks(tracks)
            .build()
        val czech = tracks.firstOrNull { it.language == "cs" || it.language == "sk" }
        val request = MediaLoadRequestData.Builder()
            .setMediaInfo(info)
            .setAutoplay(true)
            .setCurrentTime(media.startSec * 1000)
            .apply { czech?.let { setActiveTrackIds(longArrayOf(it.id)) } }
            .build()
        val result = c.load(request).await()
        if (!result.status.isSuccess) {
            Log.w(TAG, "load ${result.status}")
            error("Televize tenhle soubor neumí přehrát. Zkus zdroj Přehraj.to nebo Hellspy (MP4).")
        }
    }

    override suspend fun setPaused(paused: Boolean) {
        val c = client() ?: return
        if (paused) c.pause().await() else c.play().await()
    }

    override suspend fun seek(sec: Double) {
        client()?.seek(MediaSeekOptions.Builder().setPosition((sec * 1000).toLong()).build())?.await()
    }

    override suspend fun status(): TvStatus? {
        val c = client() ?: return null
        val s = c.mediaStatus ?: return null
        val ended = s.playerState == MediaStatus.PLAYER_STATE_IDLE && s.idleReason == MediaStatus.IDLE_REASON_FINISHED
        return TvStatus(
            positionSec = c.approximateStreamPosition / 1000.0,
            durationSec = c.streamDuration / 1000.0,
            playing = c.isPlaying || c.isBuffering,
            ended = ended,
        )
    }

    override suspend fun stop() {
        client()?.stop()?.await()
    }

    override fun disconnect() {
        cast.sessionManager.endCurrentSession(true)
        router.unselect(MediaRouter.UNSELECT_REASON_STOPPED)
    }

    companion object {
        private const val TAG = "KuroCast"

        /** null = zařízení bez Google Play služeb (Cast nejde). */
        @Suppress("DEPRECATION")
        fun createOrNull(context: Context, relay: RelayServer): CastBackend? {
            val ok = GoogleApiAvailability.getInstance().isGooglePlayServicesAvailable(context) == ConnectionResult.SUCCESS
            if (!ok) return null
            return runCatching { CastBackend(context, CastContext.getSharedInstance(context), relay) }
                .onFailure { Log.w(TAG, "Cast nejde", it) }
                .getOrNull()
        }
    }
}

private suspend fun <R : Result> PendingResult<R>.await(): R = suspendCancellableCoroutine { cont ->
    setResultCallback { cont.resume(it) }
}
