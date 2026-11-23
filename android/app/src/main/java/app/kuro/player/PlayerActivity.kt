package app.kuro.player

import android.app.PictureInPictureParams
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.media.AudioManager
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import android.util.Rational
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.lifecycleScope
import app.kuro.KuroApp
import app.kuro.core.KuroException
import app.kuro.core.LinkPurpose
import app.kuro.core.Provider
import app.kuro.ui.theme.KuroTheme
import `is`.xyz.mpv.Utils
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

/**
 * Přehrávač (mpv). Spouští se s už vyřešeným odkazem; když odkaz
 * vyprší nebo spojení spadne, vyžádá si nový přes core a naváže.
 */
class PlayerActivity : ComponentActivity() {

    private lateinit var view: KuroMpvView
    private val state = PlayerState()
    private val core get() = (application as KuroApp).core

    private var titleId = 0L
    private var season = 0
    private var episode = 0
    private lateinit var provider: Provider
    private lateinit var ident: String
    private lateinit var url: String

    var inPip by mutableStateOf(false)
        private set
    var reloading by mutableStateOf(false)
        private set

    private val audio by lazy { getSystemService(Context.AUDIO_SERVICE) as AudioManager }

    private val pickSubtitle = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let { addSubtitle(it) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        titleId = intent.getLongExtra(EXTRA_TITLE_ID, 0)
        season = intent.getIntExtra(EXTRA_SEASON, 0)
        episode = intent.getIntExtra(EXTRA_EPISODE, 0)
        provider = Provider.valueOf(intent.getStringExtra(EXTRA_PROVIDER) ?: Provider.WEBSHARE.name)
        ident = intent.getStringExtra(EXTRA_IDENT).orEmpty()
        url = intent.getStringExtra(EXTRA_URL).orEmpty()
        val title = intent.getStringExtra(EXTRA_TITLE).orEmpty()
        val subtitle = intent.getStringExtra(EXTRA_SUBTITLE)

        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        hideSystemBars()

        Utils.copyAssets(this) // cacert.pem + fonts.conf pro libass
        // Knihovna fonts.conf zapíše, ale fontconfig ho najde jen přes tuhle
        // proměnnou. Bez ní libass nemá fonty a ASS titulky (anime) se
        // nevykreslí vůbec. Musí být nastavená před vytvořením mpv.
        android.system.Os.setenv("FONTCONFIG_FILE", File(filesDir, "fonts.conf").path, true)
        view = KuroMpvView(this)
        view.initialize(filesDir.path, cacheDir.path)
        view.mpv.addObserver(state)

        // Knihovna startuje mpv s pause=true — po načtení souboru rozběhnout.
        state.onLoaded = { view.mpv.setPropertyBoolean("pause", false) }
        state.onFailed = {
            // Nerozjelo se vůbec → zdroj v řazení klesne.
            if (!state.loaded) runCatching { core.reportStream(provider, ident, false) }
        }

        resumePositionSec()?.let { view.mpv.setOptionString("start", it.toString()) }
        view.playFile(url)

        setContent {
            KuroTheme {
                PlayerScreen(
                    view = view,
                    state = state,
                    title = title,
                    subtitle = subtitle,
                    inPip = inPip,
                    reloading = reloading,
                    actions = actions,
                )
            }
        }

        // Pozice se ukládá průběžně — při pádu aplikace se o moc nepřijde.
        lifecycleScope.launch {
            while (isActive) {
                delay(10_000)
                if (!state.paused) saveProgress()
            }
        }
    }

    /** Navázat tam, kde se skončilo (ne u dokoukaného a ne u prvních 30 s). */
    private fun resumePositionSec(): Long? {
        val p = runCatching { core.progress(titleId, season, episode) }.getOrNull() ?: return null
        if (p.finished || p.positionMs < 30_000) return null
        return (p.positionMs / 1000 - 3).coerceAtLeast(0)
    }

    private fun saveProgress() {
        if (!state.loaded || state.duration <= 0) return
        val pos = if (state.ended) state.duration else state.position
        runCatching {
            core.saveProgress(titleId, season, episode, (pos * 1000).toLong(), (state.duration * 1000).toLong(), provider, ident)
        }
    }

    val actions = object : PlayerActions {
        override fun togglePause() {
            if (state.ended) {
                view.mpv.command("seek", "0", "absolute")
            }
            view.mpv.setPropertyBoolean("pause", !state.paused)
        }

        override fun seekTo(seconds: Double) {
            view.mpv.command("seek", seconds.coerceAtLeast(0.0).toString(), "absolute")
        }

        override fun seekBy(seconds: Double) {
            view.mpv.command("seek", seconds.toString(), "relative")
        }

        override fun selectAudio(id: Int) {
            view.mpv.setPropertyString("aid", id.toString())
        }

        override fun selectSubtitle(id: Int?) {
            view.mpv.setPropertyString("sid", id?.toString() ?: "no")
        }

        override fun loadSubtitleFile() {
            pickSubtitle.launch(arrayOf("*/*"))
        }

        override fun setZoom(fill: Boolean) {
            // panscan 1.0 = vyplnit obrazovku (ořízne okraje), 0 = celý obraz.
            view.mpv.setPropertyDouble("panscan", if (fill) 1.0 else 0.0)
        }

        override fun enterPip() {
            enterPipIfPossible()
        }

        override fun retry() {
            reload()
        }

        override fun close() {
            saveProgress()
            finish()
        }

        override fun brightness(): Float =
            window.attributes.screenBrightness.takeIf { it >= 0 } ?: 0.5f

        override fun setBrightness(value: Float) {
            window.attributes = window.attributes.apply { screenBrightness = value.coerceIn(0.01f, 1f) }
        }

        override fun volume(): Float =
            audio.getStreamVolume(AudioManager.STREAM_MUSIC).toFloat() / audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC)

        override fun setVolume(value: Float) {
            val max = audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC)
            audio.setStreamVolume(AudioManager.STREAM_MUSIC, (value.coerceIn(0f, 1f) * max).toInt(), 0)
        }
    }

    /** Nový odkaz (starý mohl vypršet) a navázání na aktuální pozici. */
    private fun reload() {
        if (reloading) return
        lifecycleScope.launch {
            reloading = true
            try {
                val link = core.resolveStream(provider, ident, LinkPurpose.STREAM)
                url = link.url
                val at = state.position
                state.clearFailure()
                view.mpv.setPropertyString("start", at.toLong().toString())
                view.mpv.command("loadfile", url)
            } catch (e: KuroException) {
                // Necháme chybovou hlášku; uživatel může zpět a vybrat jiný zdroj.
            } finally {
                reloading = false
            }
        }
    }

    private fun addSubtitle(uri: Uri) {
        lifecycleScope.launch {
            // mpv neumí content:// — zkopírovat do cache a přidat jako soubor.
            val file = withContext(Dispatchers.IO) {
                runCatching {
                    val name = contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
                        ?.use { c -> if (c.moveToFirst()) c.getString(0) else null } ?: "titulky.srt"
                    val dir = File(cacheDir, "subs").apply { mkdirs() }
                    File(dir, name.replace('/', '_')).also { out ->
                        contentResolver.openInputStream(uri)!!.use { input -> out.outputStream().use { input.copyTo(it) } }
                    }
                }.getOrNull()
            }
            file?.let { view.mpv.command("sub-add", it.path, "select") }
        }
    }

    private fun enterPipIfPossible() {
        if (!packageManager.hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE)) return
        val ratio = state.aspect.coerceIn(1 / 2.39, 2.39)
        val params = PictureInPictureParams.Builder()
            .setAspectRatio(Rational((ratio * 1000).toInt(), 1000))
            .build()
        runCatching { enterPictureInPictureMode(params) }
    }

    override fun onUserLeaveHint() {
        super.onUserLeaveHint()
        if (state.loaded && !state.paused) enterPipIfPossible()
    }

    override fun onPictureInPictureModeChanged(isInPictureInPictureMode: Boolean, newConfig: Configuration) {
        super.onPictureInPictureModeChanged(isInPictureInPictureMode, newConfig)
        inPip = isInPictureInPictureMode
    }

    override fun onStop() {
        super.onStop()
        saveProgress()
        // Zavřené PiP okno nebo odchod z aplikace → pauza.
        if (!isInPictureInPictureMode) view.mpv.setPropertyBoolean("pause", true)
    }

    override fun onDestroy() {
        saveProgress()
        view.mpv.removeObserver(state)
        view.destroy()
        super.onDestroy()
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) hideSystemBars()
    }

    private fun hideSystemBars() {
        WindowCompat.setDecorFitsSystemWindows(window, false)
        WindowInsetsControllerCompat(window, window.decorView).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
    }

    companion object {
        private const val EXTRA_TITLE_ID = "title_id"
        private const val EXTRA_SEASON = "season"
        private const val EXTRA_EPISODE = "episode"
        private const val EXTRA_PROVIDER = "provider"
        private const val EXTRA_IDENT = "ident"
        private const val EXTRA_URL = "url"
        private const val EXTRA_TITLE = "title"
        private const val EXTRA_SUBTITLE = "subtitle"

        fun start(
            context: Context,
            titleId: Long,
            season: Int,
            episode: Int,
            provider: Provider,
            ident: String,
            url: String,
            title: String,
            subtitle: String?,
        ) {
            context.startActivity(
                Intent(context, PlayerActivity::class.java)
                    .putExtra(EXTRA_TITLE_ID, titleId)
                    .putExtra(EXTRA_SEASON, season)
                    .putExtra(EXTRA_EPISODE, episode)
                    .putExtra(EXTRA_PROVIDER, provider.name)
                    .putExtra(EXTRA_IDENT, ident)
                    .putExtra(EXTRA_URL, url)
                    .putExtra(EXTRA_TITLE, title)
                    .putExtra(EXTRA_SUBTITLE, subtitle),
            )
        }
    }
}

/** Co UI přehrávače smí s přehrávačem dělat. */
interface PlayerActions {
    fun togglePause()
    fun seekTo(seconds: Double)
    fun seekBy(seconds: Double)
    fun selectAudio(id: Int)
    fun selectSubtitle(id: Int?)
    fun loadSubtitleFile()
    fun setZoom(fill: Boolean)
    fun enterPip()
    fun retry()
    fun close()
    fun brightness(): Float
    fun setBrightness(value: Float)
    fun volume(): Float
    fun setVolume(value: Float)
}
