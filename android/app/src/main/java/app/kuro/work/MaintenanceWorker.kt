package app.kuro.work

import android.content.Context
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import app.kuro.KuroApp
import java.util.concurrent.TimeUnit

/**
 * Denní údržba na pozadí: úklid databáze pod limit. Později sem přibudou
 * doporučení a kontrola nových epizod (Wi-Fi + nabíjení).
 */
class MaintenanceWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result {
        val app = applicationContext as KuroApp
        return runCatching { app.core.prune(app.settings.maxTitles) }
            .fold({ Result.success() }, { Result.retry() })
    }

    companion object {
        private const val NAME = "kuro-maintenance"

        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<MaintenanceWorker>(1, TimeUnit.DAYS)
                .setConstraints(Constraints.Builder().setRequiresBatteryNotLow(true).build())
                .build()
            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork(NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }
    }
}
