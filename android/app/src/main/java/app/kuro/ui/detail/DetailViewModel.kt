package app.kuro.ui.detail

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import app.kuro.core.Episode
import app.kuro.core.KuroCore
import app.kuro.core.KuroException
import app.kuro.core.LinkPurpose
import app.kuro.core.MediaKind
import app.kuro.core.PlayLink
import app.kuro.core.ProgressInfo
import app.kuro.core.StreamInfo
import app.kuro.core.StreamListener
import app.kuro.core.TitleDetail
import app.kuro.data.TokenVault
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

/** Film = (0, 0), epizoda = (sezóna, díl). */
data class PlayTarget(val season: Int, val episode: Int, val label: String)

/** Co má UI udělat po výběru zdroje. */
sealed interface PlayEvent {
    data object NeedLogin : PlayEvent
    data class Ready(val link: PlayLink, val target: PlayTarget, val stream: StreamInfo) : PlayEvent
}

class DetailViewModel(
    private val core: KuroCore,
    private val vault: TokenVault,
    val titleId: Long,
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
    var searching by mutableStateOf(false)
        private set
    var streamError by mutableStateOf<String?>(null)
        private set
    var resolving by mutableStateOf<String?>(null)
        private set
    var event by mutableStateOf<PlayEvent?>(null)
        private set

    private var findJob: Job? = null

    val seasons: List<Int> get() = detail?.episodes?.map { it.season }?.distinct().orEmpty()
    val isSeries: Boolean get() = detail?.card?.kind == MediaKind.SERIES

    init {
        load()
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

    fun openMovie() = openStreams(PlayTarget(0, 0, detail?.card?.title ?: ""))

    fun openEpisode(e: Episode) = openStreams(
        PlayTarget(e.season, e.episode, "S${e.season} E${e.episode}" + (e.name?.let { " · $it" } ?: "")),
    )

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
        if (!core.isWebshareLoggedIn()) {
            event = PlayEvent.NeedLogin
            return
        }
        viewModelScope.launch {
            resolving = stream.ident
            try {
                val link = core.resolveStream(stream.provider, stream.ident, LinkPurpose.STREAM)
                core.playbackStarted(titleId, t.season, t.episode, stream.provider, stream.ident)
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
