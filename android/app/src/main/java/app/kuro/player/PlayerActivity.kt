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
import android.widget.Toast
import app.kuro.core.Episode
import app.kuro.core.Provider
import app.kuro.core.StreamInfo
import app.kuro.ui.common.isSuspicious
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

    private var title by mutableStateOf("")
    private var subtitle by mutableStateOf<String?>(null)
    private var startedRecorded = false

    /** Rychlost, časovač vypnutí, nabídka dalšího dílu. */
    val extras = PlayerExtras()
    private var media: PlayerMediaSession? = null

    /** Další díl a zdroj pro něj (připravené dopředu, ať přechod netrvá). */
    private var preparedNext: Pair<Episode, StreamInfo>? = null
    private var preparing = false
    private var nextDismissed = false
    private var hasNext = false

    /** Co přehrát — z intentu při spuštění i při novém videu v běžícím přehrávači. */
    private fun readIntent(i: Intent) {
        titleId = i.getLongExtra(EXTRA_TITLE_ID, 0)
        season = i.getIntExtra(EXTRA_SEASON, 0)
        episode = i.getIntExtra(EXTRA_EPISODE, 0)
        provider = Provider.valueOf(i.getStringExtra(EXTRA_PROVIDER) ?: Provider.WEBSHARE.name)
        ident = i.getStringExtra(EXTRA_IDENT).orEmpty()
        url = i.getStringExtra(EXTRA_URL).orEmpty()
        title = i.getStringExtra(EXTRA_TITLE).orEmpty()
        subtitle = i.getStringExtra(EXTRA_SUBTITLE)
        externalSubtitles = i.getStringArrayListExtra(EXTRA_SUBS).orEmpty().mapNotNull { line ->
            line.split('|', limit = 3).takeIf { it.size == 3 }?.let { (lang, label, u) -> Triple(lang, label, u) }
        } + localSubtitles(url)
        subtitlesAdded = false
        startedRecorded = false
        creditsStartSec = null
    }

    /**
     * Přehrávač je singleTask: když běží (třeba zmenšený v obraze v obraze)
     * a uživatel pustí jiné video, přijde sem. Uložit pozici starého a
     * načíst nové — jinak by dál hrál původní soubor.
     */
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        if (intent.getStringExtra(EXTRA_URL).isNullOrEmpty()) return
        saveProgress()
        setIntent(intent)
        readIntent(intent)
        loadCurrent()
    }

    /** Načte [url] do běžícího přehrávače (nové video nebo další díl). */
    private fun loadCurrent() {
        state.reset()
        extras.nextUp = null
        preparedNext = null
        preparing = false
        nextDismissed = false
        hasNext = false
        if (extras.sleep == SleepTimer.END) extras.sleep = SleepTimer.OFF
        view.mpv.setPropertyString("start", resumePositionSec()?.toString() ?: "none")
        view.mpv.command("loadfile", url)
    }

    private var externalSubtitles: List<Triple<String, String, String>> = emptyList()
    private var subtitlesAdded = false

    /** Titulky stažené vedle videa: "<video>.<jazyk>.<n>.vtt". */
    private fun localSubtitles(path: String): List<Triple<String, String, String>> {
        if (!path.startsWith("/")) return emptyList()
        val video = File(path)
        val base = video.nameWithoutExtension
        return video.parentFile?.listFiles { f -> f.name.startsWith("$base.") && f.extension in setOf("vtt", "srt", "ass") }
            ?.sortedBy { it.name }
            ?.map { f -> val lang = f.name.removePrefix("$base.").substringBefore('.'); Triple(lang, lang.uppercase(), f.path) }
            .orEmpty()
    }
    var reloading by mutableStateOf(false)
        private set

    private val audio by lazy { getSystemService(Context.AUDIO_SERVICE) as AudioManager }

    private val pickSubtitle = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let { addSubtitle(it) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        readIntent(intent)

        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        // Video i do oblasti výřezu kamery — jinak je na šířku na jedné
        // straně černý pruh a obraz i ovládání jsou posunuté.
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.P) {
            window.attributes = window.attributes.apply {
                layoutInDisplayCutoutMode = if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R) {
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
                } else {
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
                }
            }
        }
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
        state.onLoaded = {
            view.mpv.setPropertyBoolean("pause", false)
            creditsStartSec = findCreditsChapter()
            hasNext = season > 0 && titleId > 0 &&
                runCatching { core.nextEpisode(titleId, season, episode) }.getOrNull() != null
            media?.setMetadata(title, subtitle, view.mpv.getPropertyDouble("duration") ?: 0.0, hasNext)
            // Titulky od zdroje (Přehraj.to/Hellspy) nebo stažené vedle videa.
            // Jen nabídnout ("auto"), zapne je výběr jazyka (slang) nebo uživatel.
            if (!subtitlesAdded) {
                subtitlesAdded = true
                externalSubtitles.forEach { (lang, label, u) ->
                    view.mpv.command("sub-add", u, "auto", label.ifBlank { lang }, lang)
                }
            }
            // Přehrávání se rozjelo (online i ze staženého souboru): zájem
            // o titul + jazyková preference + "zdroj funguje". Jednou.
            if (!startedRecorded) {
                startedRecorded = true
                runCatching { core.playbackStarted(titleId, season, episode, provider, ident) }
            }
        }
        state.onFailed = {
            // Nerozjelo se vůbec → zdroj v řazení klesne.
            if (!state.loaded) runCatching { core.reportStream(provider, ident, false) }
        }

        resumePositionSec()?.let { view.mpv.setOptionString("start", it.toString()) }
        view.playFile(url)

        media = runCatching {
            PlayerMediaSession(this, object : PlayerMediaSession.Callbacks {
                override fun play() { view.mpv.setPropertyBoolean("pause", false) }
                override fun pause() { view.mpv.setPropertyBoolean("pause", true) }
                override fun seekBy(seconds: Double) = actions.seekBy(seconds)
                override fun seekTo(seconds: Double) = actions.seekTo(seconds)
                override fun next() = playNext(finishedCurrent = false)
                override fun stop() = actions.close()
            })
        }.getOrNull()

        setContent {
            KuroTheme {
                PlayerScreen(
                    view = view,
                    state = state,
                    title = title,
                    subtitle = subtitle,
                    inPip = inPip,
                    reloading = reloading,
                    extras = extras,
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

        // Jednou za sekundu: notifikace, časovač vypnutí, další díl.
        lifecycleScope.launch {
            var lastPlaying: Boolean? = null
            var tick = 0
            while (isActive) {
                delay(1_000)
                val playing = state.loaded && !state.paused
                if (playing != lastPlaying || ++tick % 10 == 0) {
                    lastPlaying = playing
                    media?.update(playing, state.position, extras.speed)
                }
                tickSleepTimer()
                tickNextEpisode()
            }
        }
    }

    private fun tickSleepTimer() {
        val at = extras.sleepAtMs
        if (at > 0 && System.currentTimeMillis() >= at) {
            view.mpv.setPropertyBoolean("pause", true)
            extras.sleepAtMs = 0
            extras.sleep = SleepTimer.OFF
            saveProgress()
        }
        // "Na konci dílu": mpv na konci zůstane stát; časovač tím splnil úkol.
        if (extras.sleep == SleepTimer.END && state.ended) extras.sleep = SleepTimer.OFF
    }

    /**
     * Další díl: ~4 min před koncem se najde zdroj, na titulcích (kapitola,
     * jinak posledních 45 s) naskočí nabídka s odpočtem. Ne při časovači
     * "na konci dílu" a ne když ji uživatel zavřel.
     */
    private fun tickNextEpisode() {
        if (season <= 0 || titleId <= 0 || !state.loaded || state.duration <= 0) return
        val remaining = state.duration - state.position
        if (!preparing && preparedNext == null && remaining <= 240) {
            lifecycleScope.launch { prepareNext() }
        }
        if (nextDismissed || extras.sleep == SleepTimer.END) {
            extras.nextUp = null
            return
        }
        val ep = preparedNext?.first ?: return
        val atEnd = state.ended || (creditsStartSec?.let { state.position >= it } ?: (remaining <= 45))
        if (!atEnd) {
            extras.nextUp = null // posun zpět před titulky
            return
        }
        val cur = extras.nextUp
        when {
            cur == null -> extras.nextUp = NextUp(ep.season, ep.episode, episodeLabel(ep), NEXT_COUNTDOWN)
            state.paused && !state.ended -> {} // při pauze odpočet stojí
            (cur.secondsLeft ?: 0) <= 1 -> playNext(finishedCurrent = true)
            else -> extras.nextUp = cur.copy(secondsLeft = (cur.secondsLeft ?: NEXT_COUNTDOWN) - 1)
        }
    }

    private fun episodeLabel(e: Episode) = "S${e.season} E${e.episode}" + (e.name?.let { " · $it" } ?: "")

    /** Najde další díl a zdroj co nejpodobnější tomu současnému (dabing/titulky, zdroj). */
    private suspend fun prepareNext(): Pair<Episode, StreamInfo>? {
        preparedNext?.let { return it }
        if (preparing) return null
        preparing = true
        try {
            val next = runCatching { core.nextEpisode(titleId, season, episode) }.getOrNull() ?: return null
            val current = runCatching { core.cachedStreams(titleId, season, episode) }.getOrNull()
                ?.streams?.firstOrNull { it.ident == ident }
            val cached = runCatching { core.cachedStreams(titleId, next.season, next.episode) }.getOrNull()
            val list = if (cached != null && cached.isFresh && cached.streams.isNotEmpty()) {
                cached.streams
            } else {
                runCatching { core.findStreams(titleId, next.season, next.episode, null) }.getOrNull()
                    ?: cached?.streams.orEmpty()
            }
            fun similarity(s: StreamInfo): Int {
                var score = if (s.provider == provider) 1 else 0
                if (current != null) {
                    if (s.dubbed == current.dubbed) score += 2
                    if (s.audioLangs.toSet() == current.audioLangs.toSet()) score += 1
                    if (s.subLangs.any { it in current.subLangs }) score += 1
                }
                return score
            }
            // Seznam je seřazený od nejlepšího; při shodě vyhraje dřívější.
            val pick = list.filter { !it.isSuspicious }.withIndex()
                .maxWithOrNull(compareBy<IndexedValue<StreamInfo>>({ similarity(it.value) }, { -it.index }))
                ?.value ?: return null
            return (next to pick).also { preparedNext = it }
        } finally {
            preparing = false
        }
    }

    /** Pustí další díl ve stejném přehrávači. */
    private fun playNext(finishedCurrent: Boolean) {
        if (season <= 0 || titleId <= 0) return
        extras.nextUp = null
        nextDismissed = true
        lifecycleScope.launch {
            val prepared = prepareNext()
            if (prepared == null) {
                Toast.makeText(this@PlayerActivity, "Další díl se nepodařilo najít.", Toast.LENGTH_SHORT).show()
                return@launch
            }
            val (ep, stream) = prepared
            reloading = true
            try {
                val link = core.resolveStream(stream.provider, stream.ident, LinkPurpose.STREAM)
                // Dokoukaný díl (došlo na titulky) se uloží jako dokoukaný.
                if (finishedCurrent && state.duration > 0) {
                    val d = (state.duration * 1000).toLong()
                    runCatching { core.saveProgress(titleId, season, episode, d, d, provider, ident) }
                } else {
                    saveProgress()
                }
                season = ep.season
                episode = ep.episode
                provider = stream.provider
                ident = stream.ident
                url = link.url
                subtitle = episodeLabel(ep)
                externalSubtitles = link.subtitles.map { Triple(it.lang, it.label, it.url) }
                subtitlesAdded = false
                startedRecorded = false
                creditsStartSec = null
                loadCurrent()
            } catch (e: KuroException) {
                nextDismissed = false
                Toast.makeText(this@PlayerActivity, "Další díl nejde pustit (${e.message}).", Toast.LENGTH_SHORT).show()
            } finally {
                reloading = false
            }
        }
    }

    /** Navázat tam, kde se skončilo (ne u dokoukaného a ne u prvních 30 s). */
    private fun resumePositionSec(): Long? {
        val p = runCatching { core.progress(titleId, season, episode) }.getOrNull() ?: return null
        if (p.finished || p.positionMs < 30_000) return null
        return (p.positionMs / 1000 - 3).coerceAtLeast(0)
    }

    /**
     * Začátek titulků podle kapitol souboru ("Credits", "End Credits",
     * "Titulky", "Ending"…), jen v poslední čtvrtině. Kdo dojde sem, má
     * dokoukáno — stejně jako Jellyfin s kapitolami.
     */
    private fun findCreditsChapter(): Double? = runCatching {
        val duration = view.mpv.getPropertyDouble("duration") ?: return null
        val count = view.mpv.getPropertyInt("chapter-list/count") ?: 0
        (0 until count).firstNotNullOfOrNull { i ->
            val title = view.mpv.getPropertyString("chapter-list/$i/title").orEmpty()
            val time = view.mpv.getPropertyDouble("chapter-list/$i/time") ?: return@firstNotNullOfOrNull null
            time.takeIf { it >= duration * 0.75 && CREDITS.containsMatchIn(title) }
        }
    }.getOrNull()

    private var creditsStartSec: Double? = null

    private fun saveProgress() {
        if (!state.loaded || state.duration <= 0) return
        val inCredits = creditsStartSec?.let { state.position >= it } == true
        val pos = if (state.ended || inCredits) state.duration else state.position
        runCatching {
            core.saveProgress(titleId, season, episode, (pos * 1000).toLong(), (state.duration * 1000).toLong(), provider, ident)
        }
    }

    val actions = object : PlayerActions {
        override fun setSpeed(speed: Double) {
            extras.speed = speed
            view.mpv.setPropertyDouble("speed", speed)
        }

        override fun setSleepTimer(timer: SleepTimer) {
            extras.sleep = timer
            extras.sleepAtMs = timer.minutes?.let { System.currentTimeMillis() + it * 60_000L } ?: 0L
        }

        override fun playNextNow() = playNext(finishedCurrent = true)

        override fun cancelNext() {
            nextDismissed = true
            extras.nextUp = null
        }

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
        // Stažený soubor: nic nevyřizovat přes síť, jen otevřít znovu.
        if (url.startsWith("/")) {
            val at = state.position
            state.clearFailure()
            view.mpv.setPropertyString("start", at.toLong().toString())
            view.mpv.command("loadfile", url)
            return
        }
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

    override fun onResume() {
        super.onResume()
        hideSystemBars()
    }

    override fun onStop() {
        super.onStop()
        saveProgress()
        (application as KuroApp).settings.recordBandwidth(state.measuredBandwidth)
        // Zavřené PiP okno nebo odchod z aplikace → pauza.
        if (!isInPictureInPictureMode) view.mpv.setPropertyBoolean("pause", true)
    }

    override fun onDestroy() {
        saveProgress()
        media?.release()
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
        private const val EXTRA_SUBS = "subs"
        private const val NEXT_COUNTDOWN = 10
        private val CREDITS = Regex(
            """^(end\s*)?credits|^(closing|ending)\b|^titulky|^závěrečné titulky|^outro\b|^ed\b""",
            RegexOption.IGNORE_CASE,
        )

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
            subtitles: List<app.kuro.core.SubtitleInfo> = emptyList(),
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
                    .putExtra(EXTRA_SUBTITLE, subtitle)
                    .putStringArrayListExtra(
                        EXTRA_SUBS,
                        ArrayList(subtitles.map { "${it.lang}|${it.label.replace('|', ' ')}|${it.url}" }),
                    ),
            )
        }
    }
}

/** Co UI přehrávače smí s přehrávačem dělat. */
interface PlayerActions {
    fun setSpeed(speed: Double)
    fun setSleepTimer(timer: SleepTimer)
    fun playNextNow()
    fun cancelNext()
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
