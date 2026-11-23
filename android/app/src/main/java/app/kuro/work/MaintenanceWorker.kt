package app.kuro.work

import android.content.Context
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import app.kuro.KuroApp
import java.util.concurrent.TimeUnit

/**
 * Denní údržba na pozadí (s připojením, ne při slabé baterii):
 * úklid databáze pod limit, kontrola nových dílů rozkoukaných seriálů
 * a přepočet řady "Pro tebe".
 */
class MaintenanceWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val app = applicationContext as KuroApp
        val core = app.core
        runCatching { core.prune(app.settings.maxTitles) }
        // Nové díly rozkoukaných seriálů → "další díl" v Pokračovat.
        runCatching { core.refreshSeriesInProgress() }
        // Doporučení s ověřením dostupnosti jen na neměřené síti (Wi-Fi):
        // ~20 hledání u Webshare, na datech by to bylo zbytečné.
        val cm = applicationContext.getSystemService(android.net.ConnectivityManager::class.java)
        val unmetered = cm?.isActiveNetworkMetered == false
        runCatching { core.computeRecommendations(unmetered) }
        return Result.success()
    }

    companion object {
        private const val NAME = "kuro-maintenance"

        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<MaintenanceWorker>(1, TimeUnit.DAYS)
                .setConstraints(
                    Constraints.Builder()
                        .setRequiresBatteryNotLow(true)
                        .setRequiredNetworkType(NetworkType.CONNECTED)
                        .build(),
                )
                .build()
            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.UPDATE, request)
        }
    }
}
