package app.kuro.update

import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.content.FileProvider
import app.kuro.BuildConfig
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/** Vydaná verze na GitHubu. */
data class Release(val version: String, val notes: String, val apkUrl: String, val sizeBytes: Long)

/**
 * Ruční aktualizace z GitHub Releases. Nic se nekontroluje samo — jen po
 * klepnutí v nastavení (jediný dotaz na GitHub, bez přihlášení).
 */
object Updater {
    private const val LATEST = "https://api.github.com/repos/ItsCr4zyyy/Kuro/releases/latest"

    /** APK pro procesor telefonu; `kuro.apk` = arm64 (skoro všechny telefony). */
    private fun assetName(): String = when (Build.SUPPORTED_ABIS.firstOrNull()) {
        "armeabi-v7a" -> "kuro-armeabi-v7a.apk"
        "x86_64" -> "kuro-x86_64.apk"
        else -> "kuro.apk"
    }

    /** Novější verze, nebo null, když je aplikace aktuální. */
    suspend fun check(): Release? = withContext(Dispatchers.IO) {
        val conn = (URL(LATEST).openConnection() as HttpURLConnection).apply {
            connectTimeout = 10_000
            readTimeout = 15_000
            setRequestProperty("Accept", "application/vnd.github+json")
        }
        val json = try {
            if (conn.responseCode == 404) error("Na GitHubu zatím není žádné vydání.")
            if (conn.responseCode != 200) error("GitHub neodpověděl (HTTP ${conn.responseCode}).")
            JSONObject(conn.inputStream.use { it.readBytes().decodeToString() })
        } finally {
            conn.disconnect()
        }
        val version = json.optString("tag_name").removePrefix("v")
        if (!isNewer(version, BuildConfig.VERSION_NAME)) return@withContext null
        val assets = json.optJSONArray("assets") ?: error("Vydání $version nemá APK.")
        val all = (0 until assets.length()).map { assets.getJSONObject(it) }
        val asset = all.firstOrNull { it.optString("name") == assetName() }
            ?: all.firstOrNull { it.optString("name") == "kuro.apk" }
            ?: error("Vydání $version nemá APK pro tenhle telefon.")
        Release(
            version = version,
            notes = json.optString("body").trim(),
            apkUrl = asset.getString("browser_download_url"),
            sizeBytes = asset.optLong("size"),
        )
    }

    /** "1.0.10" > "1.0.9"; neznámý formát se bere jako nenovější. */
    fun isNewer(candidate: String, current: String): Boolean {
        fun parts(v: String): List<Int>? = v.substringBefore('-').split('.').map { it.toIntOrNull() ?: return null }
        val a = parts(candidate) ?: return false
        val b = parts(current) ?: return false
        for (i in 0 until maxOf(a.size, b.size)) {
            val x = a.getOrElse(i) { 0 }
            val y = b.getOrElse(i) { 0 }
            if (x != y) return x > y
        }
        return false
    }

    /** Stáhne APK do cache; `onProgress` 0–1. */
    suspend fun download(context: Context, release: Release, onProgress: (Float) -> Unit): File = withContext(Dispatchers.IO) {
        val dir = File(context.cacheDir, "update").apply { mkdirs() }
        dir.listFiles()?.forEach { it.delete() }
        val out = File(dir, "kuro-${release.version}.apk")
        val conn = (URL(release.apkUrl).openConnection() as HttpURLConnection).apply {
            connectTimeout = 15_000
            readTimeout = 30_000
            instanceFollowRedirects = true
        }
        try {
            if (conn.responseCode != 200) error("Stažení selhalo (HTTP ${conn.responseCode}).")
            val total = conn.contentLengthLong.takeIf { it > 0 } ?: release.sizeBytes
            conn.inputStream.use { input ->
                out.outputStream().use { output ->
                    val buf = ByteArray(256 * 1024)
                    var done = 0L
                    while (true) {
                        val n = input.read(buf)
                        if (n < 0) break
                        output.write(buf, 0, n)
                        done += n
                        if (total > 0) onProgress((done.toFloat() / total).coerceIn(0f, 1f))
                    }
                }
            }
        } finally {
            conn.disconnect()
        }
        out
    }

    /** Otevře systémovou instalaci (Android se zeptá na povolení a potvrzení). */
    fun install(context: Context, apk: File) {
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", apk)
        context.startActivity(
            Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, "application/vnd.android.package-archive")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK),
        )
    }
}
