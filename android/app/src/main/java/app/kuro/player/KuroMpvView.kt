package app.kuro.player

import android.content.Context
import android.util.AttributeSet
import `is`.xyz.mpv.BaseMPVView
import `is`.xyz.mpv.MPV

/**
 * Plocha, do které kreslí mpv. Volby jsou nastavené pro streamování
 * z Webshare a pro CZ/SK uživatele (dabing první, titulky jen když
 * zvuk není česky/slovensky).
 */
class KuroMpvView(context: Context, attrs: AttributeSet? = null) : BaseMPVView(context, attrs) {

    override fun initOptions() {
        setVo("gpu")
        mpv.setOptionString("gpu-context", "android")
        mpv.setOptionString("opengl-es", "yes")
        // Hardwarové dekódování s fallbackem: přímé (bez kopírování) →
        // s kopírováním → softwarové (FFmpeg). Xvid/DivX v AVI tak hraje
        // i na telefonech, které ho hardwarově neumí.
        mpv.setOptionString("hwdec", "mediacodec,mediacodec-copy,no")
        mpv.setOptionString("hwdec-codecs", "all")

        // Síť: Webshare odkazy jsou HTTPS; certifikáty z balíčku knihovny.
        mpv.setOptionString("tls-verify", "yes")
        mpv.setOptionString("tls-ca-file", "${context.filesDir.path}/cacert.pem")
        mpv.setOptionString("network-timeout", "30")
        mpv.setOptionString("cache", "yes")
        // Paměť telefonu: dopředu 96 MB, dozadu 32 MB.
        mpv.setOptionString("demuxer-max-bytes", "${96 * 1024 * 1024}")
        mpv.setOptionString("demuxer-max-back-bytes", "${32 * 1024 * 1024}")

        // Po dohrání nezavírat soubor — UI pozná konec přes eof-reached.
        mpv.setOptionString("keep-open", "yes")
        mpv.setOptionString("msg-level", if (app.kuro.BuildConfig.DEBUG) "all=v" else "all=warn")

        // Jazyky: čeština/slovenština první, pak angličtina. Titulky se
        // zapnou samy jen tehdy, když zvuk není v jazyce titulků.
        mpv.setOptionString("alang", "cze,cs,ces,slo,sk,slk,eng,en")
        mpv.setOptionString("slang", "cze,cs,ces,slo,sk,slk")
        mpv.setOptionString("subs-with-matching-audio", "no")
        mpv.setOptionString("sub-auto", "no")
        mpv.setOptionString("sub-scale-by-window", "yes")
        mpv.setOptionString("sub-font-size", "44")
        mpv.setOptionString("sub-border-size", "2.5")
    }

    override fun observeProperties() {
        val f = MPV.mpvFormat
        for ((name, format) in listOf(
            "time-pos" to f.MPV_FORMAT_DOUBLE,
            "duration" to f.MPV_FORMAT_DOUBLE,
            "pause" to f.MPV_FORMAT_FLAG,
            "paused-for-cache" to f.MPV_FORMAT_FLAG,
            "eof-reached" to f.MPV_FORMAT_FLAG,
            "demuxer-cache-time" to f.MPV_FORMAT_DOUBLE,
            "track-list" to f.MPV_FORMAT_NODE,
            "aid" to f.MPV_FORMAT_STRING,
            "sid" to f.MPV_FORMAT_STRING,
            "video-params/aspect" to f.MPV_FORMAT_DOUBLE,
        )) {
            mpv.observeProperty(name, format)
        }
    }
}
