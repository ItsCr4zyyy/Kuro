package app.kuro.ui.detail

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.kuro.core.DownloadInfo
import app.kuro.core.DownloadStatus
import app.kuro.core.Episode
import app.kuro.core.KuroCore
import app.kuro.core.KuroException
import app.kuro.core.LinkPurpose
import app.kuro.core.MediaKind
import app.kuro.core.PlayLink
import app.kuro.core.Provider
import app.kuro.core.ProgressInfo
import app.kuro.core.StreamInfo
import app.kuro.core.StreamListener
import app.kuro.core.TitleDetail
import app.kuro.data.TokenVault
import app.kuro.download.Downloads
import app.kuro.ui.common.isSuspicious
import app.kuro.ui.common.isTranscode
import app.kuro.ui.common.BANDWIDTH_HEADROOM
import app.kuro.ui.common.estimatedBitrate
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

/** Film = (0, 0), epizoda = (sezóna, díl). */
data class PlayTarget(val season: Int, val episode: Int, val label: String)

/** Co má UI udělat po výběru zdroje. */
sealed interface PlayEvent {
    data object NeedLogin : PlayEvent
    data class Ready(val link: PlayLink, val target: PlayTarget, val stream: StreamInfo) : PlayEvent

    /** Staženo — přehrát soubor z telefonu, bez internetu. */
    data class Local(val file: java.io.File, val target: PlayTarget, val download: DownloadInfo) : PlayEvent

    data class Message(val text: String) : PlayEvent
}

class DetailViewModel(
    private val app: android.app.Application,
    private val core: KuroCore,
    private val vault: TokenVault,
    val titleId: Long,
    /** Naměřená rychlost připojení v bajtech/s (0 = neznámá). */
    private val bandwidthBps: Long,
) : ViewModel() {
    var detail by mutableStateOf<TitleDetail?>(null)
        private set
    var error by mutableStateOf<String?>(null)
        private set
    var season by mutableStateOf<Int?>(null)
        private set

    /** Otevřený panel se zdroji. */
    var target by mutableStateOf<PlayTarget?>(null)
        private set
    var streams by mutableStateOf<List<StreamInfo>>(emptyList())
        private set

    /** Rychlost linky v bitech/s, nebo null když ji ještě neznáme. */
    val bandwidthBits: Long? get() = (bandwidthBps * 8).takeIf { it > 0 }

    /** Zdroj má vyšší datový tok, než linka nejspíš zvládne. */
    fun tooHeavy(s: StreamInfo): Boolean {
        if (s.isTranscode) return false // převody mají pár Mbit/s
        val bw = bandwidthBits ?: return false
        val rate = s.estimatedBitrate(detail?.runtimeMin) ?: return false
        return rate > bw * BANDWIDTH_HEADROOM
    }

    /**
     * Zdroje k zobrazení: pořadí z core (čeština, kvalita, funkčnost),
     * ale ty, které by na téhle lince sekaly, až za těmi, které ne.
     */
    val displayStreams: List<StreamInfo>
        get() = streams.partition { !tooHeavy(it) }.let { (ok, heavy) -> ok + heavy }
    var searching by mutableStateOf(false)
        private set
    var streamError by mutableStateOf<String?>(null)
        private set
    var resolving by mutableStateOf<String?>(null)
        private set
    var event by mutableStateOf<PlayEvent?>(null)
        private set

    private var findJob: Job? = null

    /** Stažení tohoto titulu podle (sezóna, díl); film = (0, 0). */
    var downloads by mutableStateOf<Map<Pair<Int, Int>, DownloadInfo>>(emptyMap())
        private set

    fun downloadFor(season: Int, episode: Int): DownloadInfo? = downloads[season to episode]

    val hasActiveDownloads: Boolean
        get() = downloads.values.any { it.status == DownloadStatus.QUEUED || it.status == DownloadStatus.RUNNING }

    fun refreshDownloads() {
        downloads = runCatching { core.downloads() }.getOrDefault(emptyList())
            .filter { it.titleId == titleId }
            .associateBy { it.season to it.episode }
    }

    /** Stáhnout konkrétní zdroj pro otevřený film/díl. */
    fun download(stream: StreamInfo) {
        val t = target ?: return
        if (stream.provider == Provider.WEBSHARE && !core.isWebshareLoggedIn()) {
            event = PlayEvent.NeedLogin
            return
        }
        enqueue(t.season, t.episode, stream)
        refreshDownloads()
        event = PlayEvent.Message("Stahování zařazeno")
    }

    private fun enqueue(season: Int, episode: Int, s: StreamInfo) {
        val r = core.enqueueDownload(titleId, season, episode, s.provider, s.ident, s.filename, s.sizeBytes)
        r.staleFile?.let { java.io.File(it).delete() }
        Downloads.start(app, r.id)
    }

    /** Stáhne celou sezónu: ke každému nestaženému dílu nejlepší zdroj. */
    fun downloadSeason(season: Int) {
        val d = detail ?: return
        viewModelScope.launch {
            seasonProgress = "Hledám zdroje…"
            var queued = 0
            var missing = 0
            for (e in d.episodes.filter { it.season == season }) {
                if (downloadFor(e.season, e.episode) != null) continue
                seasonProgress = "Hledám zdroje… S${e.season} E${e.episode}"
                val cached = runCatching { core.cachedStreams(titleId, e.season, e.episode) }.getOrNull()
                val list = if (cached?.isFresh == true) cached.streams
                else runCatching { core.findStreams(titleId, e.season, e.episode, null) }.getOrDefault(cached?.streams.orEmpty())
                // Webshare bez přihlášení stáhnout nejde → první jiný zdroj.
                val best = list.firstOrNull {
                    !it.isSuspicious && (it.provider != Provider.WEBSHARE || core.isWebshareLoggedIn())
                }
                if (best == null) missing++ else { enqueue(e.season, e.episode, best); queued++ }
            }
            seasonProgress = null
            refreshDownloads()
            event = PlayEvent.Message(
                "Zařazeno $queued dílů" + if (missing > 0) " · $missing bez zdroje" else "",
            )
        }
    }

    /** Průběh hledání zdrojů pro "Stáhnout sezónu". */
    var seasonProgress by mutableStateOf<String?>(null)
        private set

    val seasons: List<Int> get() = detail?.episodes?.map { it.season }?.distinct().orEmpty()
    val isSeries: Boolean get() = detail?.card?.kind == MediaKind.SERIES

    init {
        load()
        refreshDownloads()
    }

    fun load() {
        viewModelScope.launch {
            error = null
            try {
                val d = core.titleDetail(titleId)
                detail = d
                if (season == null) season = resumeSeason(d)
            } catch (e: KuroException) {
                error = when (e) {
                    is KuroException.Network -> "Bez připojení a titul ještě není uložený."
                    else -> "Detail se nepodařilo načíst (${e.message})."
                }
            }
        }
    }

    /** Po návratu z přehrávače: obnovit pozice a oblíbené. */
    fun refreshProgress() {
        refreshDownloads()
        val d = detail ?: return
        detail = d.copy(progress = runCatching { core.titleProgress(titleId) }.getOrDefault(d.progress))
    }

    fun selectSeason(s: Int) {
        season = s
    }

    fun toggleWatchlist() {
        val d = detail ?: return
        val on = !d.inWatchlist
        runCatching { core.setWatchlist(titleId, on) }.onSuccess { detail = d.copy(inWatchlist = on) }
    }

    fun progressFor(season: Int, episode: Int): ProgressInfo? =
        detail?.progress?.firstOrNull { it.season == season && it.episode == episode }

    /** Další díl k přehrání: první nedokoukaný po posledním sledovaném. */
    fun nextEpisode(): Episode? {
        val d = detail ?: return null
        val last = d.progress.maxByOrNull { it.updatedAtMs } ?: return d.episodes.firstOrNull()
        val idx = d.episodes.indexOfFirst { it.season == last.season && it.episode == last.episode }
        return if (!last.finished) d.episodes.getOrNull(idx) else d.episodes.getOrNull(idx + 1) ?: d.episodes.getOrNull(idx)
    }

    fun openMovie() = openOrPlayLocal(PlayTarget(0, 0, detail?.card?.title ?: ""))

    fun openEpisode(e: Episode) = openOrPlayLocal(
        PlayTarget(e.season, e.episode, "S${e.season} E${e.episode}" + (e.name?.let { " · $it" } ?: "")),
    )

    /** Zdroje i u staženého (třeba pro jinou kvalitu). */
    fun openStreamsFor(t: PlayTarget) = openStreams(t)

    /** Stažené se pustí rovnou ze souboru — bez internetu a bez výběru zdroje. */
    private fun openOrPlayLocal(t: PlayTarget) {
        val d = downloadFor(t.season, t.episode)
        val file = Downloads.playableFile(d)
        if (d != null && file != null) {
            event = PlayEvent.Local(file, t, d)
        } else {
            openStreams(t)
        }
    }

    fun closeStreams() {
        findJob?.cancel()
        target = null
        streams = emptyList()
        streamError = null
    }

    private fun openStreams(t: PlayTarget) {
        target = t
        streamError = null
        findJob?.cancel()
        // Uložené hned, nové hledání jen když jsou zastaralé.
        val cached = runCatching { core.cachedStreams(titleId, t.season, t.episode) }.getOrNull()
        streams = cached?.streams.orEmpty()
        if (cached?.isFresh == true) return
        findJob = viewModelScope.launch {
            searching = true
            try {
                streams = core.findStreams(titleId, t.season, t.episode, object : StreamListener {
                    override fun onPartial(streams: List<StreamInfo>) {
                        if (target == t) this@DetailViewModel.streams = streams
                    }
                })
                if (streams.isEmpty()) streamError = "Žádný zdroj nenalezen."
            } catch (e: KuroException) {
                if (e is KuroException.AuthExpired) logout()
                if (streams.isEmpty()) streamError = "Hledání zdrojů selhalo (${e.message})."
            } finally {
                searching = false
            }
        }
    }

    fun play(stream: StreamInfo) {
        val t = target ?: return
        // Přihlášení potřebuje jen Webshare; Přehraj.to a Hellspy hrají bez účtu.
        if (stream.provider == Provider.WEBSHARE && !core.isWebshareLoggedIn()) {
            event = PlayEvent.NeedLogin
            return
        }
        viewModelScope.launch {
            resolving = stream.ident
            try {
                val link = core.resolveStream(stream.provider, stream.ident, LinkPurpose.STREAM)
                // "Přehrávání začalo" zapisuje přehrávač, až se video opravdu rozjede.
                event = PlayEvent.Ready(link, t, stream)
            } catch (e: KuroException) {
                when (e) {
                    is KuroException.AuthExpired, is KuroException.NotLoggedIn -> {
                        logout()
                        event = PlayEvent.NeedLogin
                    }
                    else -> {
                        streamError = "Zdroj nejde přehrát (${e.message}). Zkus jiný."
                        // Seznam znovu z databáze — nefunkční zdroj spadl níž.
                        streams = runCatching { core.cachedStreams(titleId, t.season, t.episode).streams }.getOrDefault(streams)
                    }
                }
            } finally {
                resolving = null
            }
        }
    }

    fun consumeEvent() {
        event = null
    }

    private fun logout() {
        core.setWebshareToken(null)
        vault.save(TokenVault.WEBSHARE, null)
    }

    private fun resumeSeason(d: TitleDetail): Int? {
        if (d.episodes.isEmpty()) return null
        val last = d.progress.maxByOrNull { it.updatedAtMs }
        return last?.season?.takeIf { it > 0 } ?: d.episodes.first().season
    }
}
