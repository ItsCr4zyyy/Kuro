package app.kuro.player

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** Časovač vypnutí. */
enum class SleepTimer(val label: String, val minutes: Int?) {
    OFF("Vypnuto", null),
    M15("15 min", 15),
    M30("30 min", 30),
    M60("1 h", 60),
    M90("1,5 h", 90),

    /** Po dohrání aktuálního filmu/dílu (bez automatického dalšího dílu). */
    END("Na konci dílu", null),
}

/** Nabídka dalšího dílu na konci epizody. */
data class NextUp(
    val season: Int,
    val episode: Int,
    /** "S1 E2 · Název" */
    val label: String,
    /** Odpočet do spuštění; null = čeká na uživatele (např. při pauze). */
    val secondsLeft: Int?,
)

/** Stav přehrávače mimo mpv (rychlost, časovač, další díl) pro Compose. */
class PlayerExtras {
    var speed by mutableDoubleStateOf(1.0)
    var sleep by mutableStateOf(SleepTimer.OFF)

    /** Kdy časovač pozastaví přehrávání (ms, 0 = neběží). */
    var sleepAtMs by mutableLongStateOf(0L)
    var nextUp by mutableStateOf<NextUp?>(null)

    companion object {
        val SPEEDS = listOf(0.75, 1.0, 1.25, 1.5, 2.0)
    }
}
