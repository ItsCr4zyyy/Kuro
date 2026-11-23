package app.kuro.download

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.pm.ServiceInfo
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import app.kuro.BuildConfig
import app.kuro.KuroApp
import app.kuro.core.PlayLink
import app.kuro.R
import app.kuro.core.DownloadInfo
import app.kuro.core.DownloadStatus
import app.kuro.core.KuroException
import app.kuro.core.LinkPurpose
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.io.IOException
import java.io.RandomAccessFile
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.TimeUnit

/**
 * Stáhne jeden soubor do soukromé složky aplikace. Pokračuje od místa
 * přerušení (HTTP Range) a odkaz si pokaždé vyžádá znovu — odkazy
 * Webshare vyprší a jsou vázané na IP.
 */
class DownloadWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {

    private val app = context.applicationContext as KuroApp
    private val core = app.core
    private val id = inputData.getLong(KEY_ID, -1)

    override suspend fun getForegroundInfo(): ForegroundInfo =
        foregroundInfo(core.download(id)?.let { label(it) } ?: "Stahování", 0, 0)

    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        val d = core.download(id) ?: return@withContext Result.success() // mezitím smazáno
        if (d.status == DownloadStatus.DONE || d.status == DownloadStatus.PAUSED) return@withContext Result.success()
        val provider = d.provider ?: return@withContext fail(d, "Neznámý zdroj")

        val file = Downloads.fileFor(applicationContext, d)
        android.util.Log.i("KuroDownload", "start ${d.id} -> ${file.path}")
        file.parentFile?.mkdirs()
        val name = label(d)
        runCatching { setForeground(foregroundInfo(name, 0, 0)) }

        try {
            var lastError: String? = null
            // Pár pokusů s novým odkazem (vypršel, Webshare dočasně odmítl).
            repeat(3) { attempt ->
                if (isStopped) return@withContext Result.retry()
                val link = try {
                    // Jen debug build: ident = přímá URL (test bez Webshare účtu).
                    if (BuildConfig.DEBUG && d.ident.startsWith("http")) {
                        PlayLink(d.ident, emptyMap(), null, emptyList())
                    } else {
                        core.resolveStream(provider, d.ident, LinkPurpose.DOWNLOAD)
                    }
                } catch (e: KuroException) {
                    if (e is KuroException.AuthExpired || e is KuroException.NotLoggedIn) {
                        return@withContext fail(d, "Přihlášení k Webshare vypršelo")
                    }
                    lastError = e.message
                    return@repeat
                }
                when (val r = transfer(d, link.url, link.headers, file, name)) {
                    TransferResult.Done -> {
                        saveSubtitles(link.subtitles, file)
                        core.updateDownload(id, DownloadStatus.DONE, file.length(), file.length(), file.path, null)
                        notifyDone(name)
                        return@withContext Result.success()
                    }
                    TransferResult.Stopped -> return@withContext Result.retry()
                    is TransferResult.LinkRejected -> lastError = "HTTP ${r.code} (pokus ${attempt + 1})"
                }
            }
            fail(d, lastError ?: "Stažení se nepodařilo")
        } catch (e: IOException) {
            android.util.Log.w("KuroDownload", "download ${d.id} přerušeno", e)
            // Výpadek sítě: zkusit později (WorkManager s odstupem), soubor zůstává.
            core.updateDownload(id, DownloadStatus.QUEUED, file.length(), null, file.path, "Přerušeno: ${e.message}")
            if (runAttemptCount < 8) Result.retry() else fail(d, "Opakovaně přerušeno: ${e.message}")
        }
    }

    private sealed interface TransferResult {
        data object Done : TransferResult
        data object Stopped : TransferResult
        data class LinkRejected(val code: Int) : TransferResult
    }

    private suspend fun transfer(d: DownloadInfo, url: String, headers: Map<String, String>, file: File, name: String): TransferResult {
        var have = if (file.exists()) file.length() else 0L
        val conn = (URL(url).openConnection() as HttpURLConnection).apply {
            connectTimeout = 20_000
            readTimeout = 30_000
            headers.forEach { (k, v) -> setRequestProperty(k, v) }
            if (have > 0) setRequestProperty("Range", "bytes=$have-")
        }
        try {
            val code = conn.responseCode
            when {
                code == 416 && have > 0 -> return TransferResult.Done // už je celé
                code == 200 && have > 0 -> have = 0 // server Range ignoroval → od začátku
                code == 403 || code == 404 || code == 410 -> return TransferResult.LinkRejected(code)
                code !in 200..299 -> throw IOException("HTTP $code")
            }
            val total = have + conn.contentLengthLong.coerceAtLeast(0)
            core.updateDownload(id, DownloadStatus.RUNNING, have, total.takeIf { it > have }, file.path, null)

            RandomAccessFile(file, "rw").use { out ->
                out.setLength(have)
                out.seek(have)
                conn.inputStream.use { input ->
                    val buf = ByteArray(256 * 1024)
                    var lastReport = 0L
                    while (true) {
                        if (isStopped) {
                            core.updateDownload(id, DownloadStatus.QUEUED, have, null, null, null)
                            return TransferResult.Stopped
                        }
                        val n = input.read(buf)
                        if (n < 0) break
                        out.write(buf, 0, n)
                        have += n
                        val now = System.currentTimeMillis()
                        if (now - lastReport > 1000) {
                            lastReport = now
                            core.updateDownload(id, DownloadStatus.RUNNING, have, null, null, null)
                            runCatching { setForeground(foregroundInfo(name, have, total)) }
                        }
                    }
                }
            }
            if (total > 0 && have < total) throw IOException("spojení skončilo předčasně")
            return TransferResult.Done
        } finally {
            conn.disconnect()
        }
    }

    /** Titulky od zdroje vedle videa ("<video>.<jazyk>.<n>.vtt") pro offline přehrání. */
    private fun saveSubtitles(subs: List<app.kuro.core.SubtitleInfo>, video: File) {
        subs.forEachIndexed { i, s ->
            runCatching {
                val ext = s.url.substringBefore('?').substringAfterLast('.', "vtt").take(4)
                val out = File(video.parentFile, "${video.nameWithoutExtension}.${s.lang}.$i.$ext")
                (URL(s.url).openConnection() as HttpURLConnection).run {
                    connectTimeout = 15_000
                    readTimeout = 30_000
                    inputStream.use { input -> out.outputStream().use { input.copyTo(it) } }
                    disconnect()
                }
            }
        }
    }

    private fun fail(d: DownloadInfo, msg: String): Result {
        core.updateDownload(id, DownloadStatus.FAILED, d.bytesDone, null, null, msg)
        return Result.failure()
    }

    private fun label(d: DownloadInfo): String {
        val t = runCatching { core.titleCard(d.titleId)?.title }.getOrNull() ?: "Kuro"
        return if (d.season > 0) "$t · S${d.season} E${d.episode}" else t
    }

    private fun foregroundInfo(title: String, done: Long, total: Long): ForegroundInfo {
        ensureChannel(applicationContext)
        val pct = if (total > 0) (done * 100 / total).toInt() else 0
        val n = NotificationCompat.Builder(applicationContext, CHANNEL)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle(title)
            .setContentText(if (total > 0) "$pct % · ${done / 1_000_000} / ${total / 1_000_000} MB" else "Připravuji…")
            .setProgress(100, pct, total <= 0)
            .setOngoing(true)
            .setSilent(true)
            .build()
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ForegroundInfo(id.toInt(), n, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            ForegroundInfo(id.toInt(), n)
        }
    }

    private fun notifyDone(title: String) {
        ensureChannel(applicationContext)
        val n = NotificationCompat.Builder(applicationContext, CHANNEL)
            .setSmallIcon(android.R.drawable.stat_sys_download_done)
            .setContentTitle(title)
            .setContentText("Staženo — jde přehrát bez internetu")
            .setAutoCancel(true)
            .build()
        val nm = applicationContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        runCatching { nm.notify(DONE_NOTIFICATION_BASE + id.toInt(), n) }
    }

    companion object {
        const val KEY_ID = "download_id"
        private const val CHANNEL = "downloads"
        private const val DONE_NOTIFICATION_BASE = 100_000

        fun ensureChannel(context: Context) {
            val nm = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            if (nm.getNotificationChannel(CHANNEL) == null) {
                nm.createNotificationChannel(
                    NotificationChannel(CHANNEL, context.getString(R.string.channel_downloads), NotificationManager.IMPORTANCE_LOW),
                )
            }
        }
    }
}

/** Řízení stahování z UI. */
object Downloads {
    fun dir(context: Context): File = File(context.getExternalFilesDir(null) ?: context.filesDir, "downloads")

    fun fileFor(context: Context, d: DownloadInfo): File {
        val ext = d.filename.substringAfterLast('.', "mkv").lowercase().take(5)
        val ep = if (d.season > 0) "_S${d.season}E${d.episode}" else ""
        // Ident do názvu souboru jen bezpečnými znaky (jiný provider může mít lomítka apod.).
        val safe = d.ident.replace(Regex("[^A-Za-z0-9_-]"), "_").takeLast(40)
        return File(dir(context), "${d.titleId}${ep}_$safe.$ext")
    }

    fun start(context: Context, id: Long) {
        val app = context.applicationContext as KuroApp
        val request = OneTimeWorkRequestBuilder<DownloadWorker>()
            .setInputData(workDataOf(DownloadWorker.KEY_ID to id))
            .setConstraints(
                Constraints.Builder()
                    .setRequiredNetworkType(if (app.settings.downloadWifiOnly) NetworkType.UNMETERED else NetworkType.CONNECTED)
                    .setRequiresStorageNotLow(true)
                    .build(),
            )
            .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)
            .addTag(TAG)
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork(name(id), ExistingWorkPolicy.KEEP, request)
    }

    fun pause(context: Context, d: DownloadInfo) {
        WorkManager.getInstance(context).cancelUniqueWork(name(d.id))
        (context.applicationContext as KuroApp).core.updateDownload(d.id, DownloadStatus.PAUSED, d.bytesDone, null, null, null)
    }

    fun resume(context: Context, d: DownloadInfo) {
        (context.applicationContext as KuroApp).core.updateDownload(d.id, DownloadStatus.QUEUED, d.bytesDone, null, null, null)
        start(context, d.id)
    }

    fun delete(context: Context, d: DownloadInfo) {
        WorkManager.getInstance(context).cancelUniqueWork(name(d.id))
        val app = context.applicationContext as KuroApp
        val path = app.core.deleteDownload(d.id)
        val video = path?.let(::File) ?: fileFor(context, d)
        // I stažené titulky vedle videa.
        video.parentFile?.listFiles { f -> f.name.startsWith(video.nameWithoutExtension + ".") }?.forEach { it.delete() }
        video.delete()
    }

    /** Po startu aplikace: rozjet, co mělo běžet (WorkManager to většinou drží sám). */
    fun resumePending(context: Context) {
        val app = context.applicationContext as KuroApp
        runCatching { app.core.pendingDownloads() }.getOrDefault(emptyList()).forEach { start(context, it.id) }
    }

    /** Hotové stažení s existujícím souborem, nebo null. */
    fun playableFile(d: DownloadInfo?): File? =
        d?.takeIf { it.status == DownloadStatus.DONE }?.localPath?.let(::File)?.takeIf { it.exists() }

    private fun name(id: Long) = "download-$id"
    private const val TAG = "kuro-download"
}
