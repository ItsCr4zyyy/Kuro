package app.kuro.ui.common

import app.kuro.core.MediaKind
import app.kuro.core.StreamInfo
import app.kuro.core.TitleCard
import java.util.Locale

/** ISO 639-1 → krátký štítek. */
fun langLabel(iso: String): String = when (iso) {
    "cs" -> "CZ"
    "sk" -> "SK"
    "en" -> "EN"
    "ja" -> "JP"
    "ko" -> "KR"
    else -> iso.uppercase(Locale.ROOT)
}

fun formatSize(bytes: Long): String = when {
    bytes >= 1_000_000_000 -> String.format(Locale.ROOT, "%.1f GB", bytes / 1e9)
    bytes >= 1_000_000 -> String.format(Locale.ROOT, "%.0f MB", bytes / 1e6)
    else -> "—"
}

fun formatRuntime(min: Int): String = if (min >= 60) "${min / 60} h ${min % 60} min" else "$min min"

fun formatPosition(ms: Long): String {
    val s = ms / 1000
    return if (s >= 3600) {
        String.format(Locale.ROOT, "%d:%02d:%02d", s / 3600, s / 60 % 60, s % 60)
    } else {
        String.format(Locale.ROOT, "%d:%02d", s / 60, s % 60)
    }
}

fun kindLabel(kind: MediaKind): String = when (kind) {
    MediaKind.MOVIE -> "Film"
    MediaKind.SERIES -> "Seriál"
}

fun TitleCard.subtitle(): String = listOfNotNull(
    year?.toString(),
    if (anime) "Anime" else kindLabel(kind),
    rating?.let { String.format(Locale.ROOT, "★ %.1f", it) },
).joinToString(" · ")

/** Jazyk zdroje lidsky. "zvuk neuveden" ≠ "originál" — nevíme, tak to neříkáme. */
fun StreamInfo.languageLine(): String {
    if (!langExplicit) return "zvuk neuveden"
    val parts = buildList {
        if (audioLangs.isNotEmpty()) {
            add((if (dubbed) "dabing " else "zvuk ") + audioLangs.joinToString("+") { langLabel(it) })
        }
        if (subLangs.isNotEmpty()) add("titulky " + subLangs.joinToString("+") { langLabel(it) })
        if (dualAudio && audioLangs.size < 2) add("více stop")
    }
    return parts.joinToString(" · ").ifEmpty { "zvuk neuveden" }
}

/** Štítky kvality: rozlišení, HDR, kodek, zvuk. */
fun StreamInfo.qualityBadges(): List<String> = buildList {
    height?.let { add(if (it >= 2160) "4K" else "${it}p") }
    hdr?.let { add(it) }
    videoCodec?.let { add(it.uppercase(Locale.ROOT).replace("H26", "H.26")) }
    audioCodec?.let { c -> add(c.uppercase(Locale.ROOT) + (channels?.let { " $it" } ?: "")) }
}

/**
 * Zdroj streamuje převod (Přehraj.to/Hellspy): max. 1080p, jedna zvuková
 * stopa, velikost/tok původního souboru pro stream neplatí.
 */
val StreamInfo.isTranscode: Boolean get() = provider != app.kuro.core.Provider.WEBSHARE

fun providerLabel(p: app.kuro.core.Provider): String = when (p) {
    app.kuro.core.Provider.WEBSHARE -> "Webshare"
    app.kuro.core.Provider.PREHRAJ_TO -> "Přehraj.to"
    app.kuro.core.Provider.HELLSPY -> "Hellspy"
}

/** Zdroj, kterému se dá věřit méně (opakovaně nešel pustit). */
val StreamInfo.isSuspicious: Boolean get() = failCount >= 2 && okCount == 0

/**
 * Odhad datového toku zdroje (bity/s) z velikosti a délky titulu.
 * null = délku neznáme.
 */
fun StreamInfo.estimatedBitrate(runtimeMin: Int?): Long? {
    if (runtimeMin == null || runtimeMin <= 0 || sizeBytes <= 0) return null
    return sizeBytes * 8 / (runtimeMin * 60L)
}

/** Bity/s → "18 Mbit/s". */
fun formatBitrate(bitsPerSec: Long): String = String.format(Locale.ROOT, "%.0f Mbit/s", bitsPerSec / 1e6)

/** Rezerva: zdroj s tokem nad 85 % rychlosti linky bude nejspíš sekat. */
const val BANDWIDTH_HEADROOM = 0.85
