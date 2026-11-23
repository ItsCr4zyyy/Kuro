package app.kuro.tv

import android.content.Context
import android.net.ConnectivityManager
import android.util.Log
import java.io.BufferedInputStream
import java.io.File
import java.io.InputStream
import java.io.OutputStream
import java.io.RandomAccessFile
import java.net.Inet4Address
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.net.HttpURLConnection
import java.net.URL
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

/**
 * Malý HTTP server v telefonu pro televizi v domácí síti:
 * - stažené soubory (TV na ně jinak nedosáhne),
 * - HTTPS odkazy pro DLNA televize, které HTTPS neumí (telefon data přeposílá),
 * - titulky pro Chromecast (potřebuje CORS hlavičku, kterou CDN nemá).
 *
 * Poslouchá jen po dobu přehrávání na TV; adresy jsou náhodné tokeny,
 * žádný seznam souborů. Range požadavky (posun ve videu) se předávají dál.
 */
class RelayServer(private val context: Context) {
    private sealed interface Source {
        val mime: String
        val cors: Boolean
        data class Remote(val url: String, override val mime: String, override val cors: Boolean) : Source
        data class Local(val file: File, override val mime: String, override val cors: Boolean) : Source
    }

    private val sources = ConcurrentHashMap<String, Source>()
    private var server: ServerSocket? = null

    /** Adresa, na které TV dostane [url] (http/https odkaz nebo cesta k souboru). */
    @Synchronized
    fun serve(url: String, mime: String, cors: Boolean = false): String? {
        val ip = localIp() ?: return null
        val port = ensureStarted() ?: return null
        val token = UUID.randomUUID().toString().replace("-", "")
        sources[token] = if (url.startsWith("/")) Source.Local(File(url), mime, cors) else Source.Remote(url, mime, cors)
        val ext = when {
            mime.contains("vtt") -> "vtt"
            mime.contains("matroska") -> "mkv"
            else -> "mp4"
        }
        return "http://$ip:$port/r/$token.$ext"
    }

    @Synchronized
    fun stopServing() {
        sources.clear()
        runCatching { server?.close() }
        server = null
    }

    private fun ensureStarted(): Int? {
        server?.let { if (!it.isClosed) return it.localPort }
        return runCatching {
            val s = ServerSocket()
            s.reuseAddress = true
            s.bind(InetSocketAddress(0))
            server = s
            Thread({ acceptLoop(s) }, "kuro-relay").apply { isDaemon = true }.start()
            s.localPort
        }.onFailure { Log.w(TAG, "start", it) }.getOrNull()
    }

    private fun acceptLoop(s: ServerSocket) {
        while (!s.isClosed) {
            val client = runCatching { s.accept() }.getOrNull() ?: break
            Thread({ handle(client) }, "kuro-relay-conn").apply { isDaemon = true }.start()
        }
    }

    /** IPv4 adresa telefonu v aktuální (Wi-Fi) síti. */
    private fun localIp(): String? {
        val cm = context.getSystemService(ConnectivityManager::class.java) ?: return null
        val props = cm.getLinkProperties(cm.activeNetwork) ?: return null
        return props.linkAddresses.map { it.address }.firstOrNull { it is Inet4Address && !it.isLoopbackAddress }?.hostAddress
    }

    private fun handle(socket: Socket) {
        socket.use { sock -> try {
            sock.soTimeout = 30_000
            val input = BufferedInputStream(sock.getInputStream())
            val requestLine = readLine(input) ?: return
            val headers = mutableMapOf<String, String>()
            while (true) {
                val line = readLine(input) ?: break
                if (line.isEmpty()) break
                val i = line.indexOf(':')
                if (i > 0) headers[line.substring(0, i).trim().lowercase()] = line.substring(i + 1).trim()
            }
            val parts = requestLine.split(' ')
            val method = parts.getOrNull(0) ?: return
            val token = parts.getOrNull(1)?.removePrefix("/r/")?.substringBefore('.')?.substringBefore('?')
            val out = sock.getOutputStream()
            val src = token?.let { sources[it] }
            if (src == null || (method != "GET" && method != "HEAD")) {
                out.write("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".toByteArray())
                return
            }
            when (src) {
                is Source.Local -> serveFile(src, method, headers["range"], out)
                is Source.Remote -> serveRemote(src, method, headers["range"], out)
            }
        } catch (e: Exception) {
            // Televize spojení při posunu běžně zavírá — to není chyba.
            if (e !is java.net.SocketException) Log.d(TAG, "conn", e)
        } }
    }

    private fun commonHeaders(src: Source) = buildString {
        append("Content-Type: ${src.mime}\r\n")
        append("Accept-Ranges: bytes\r\n")
        append("Connection: close\r\n")
        // DLNA televize (LG, Samsung) chtějí vědět, že jde o stream s posunem.
        append("transferMode.dlna.org: Streaming\r\n")
        append("contentFeatures.dlna.org: $DLNA_FEATURES\r\n")
        if (src.cors) append("Access-Control-Allow-Origin: *\r\n")
    }

    private fun serveFile(src: Source.Local, method: String, range: String?, out: OutputStream) {
        val len = src.file.length()
        val (start, end) = parseRange(range, len)
        val partial = range != null
        val head = buildString {
            append(if (partial) "HTTP/1.1 206 Partial Content\r\n" else "HTTP/1.1 200 OK\r\n")
            append(commonHeaders(src))
            append("Content-Length: ${end - start + 1}\r\n")
            if (partial) append("Content-Range: bytes $start-$end/$len\r\n")
            append("\r\n")
        }
        out.write(head.toByteArray())
        if (method == "HEAD") return
        RandomAccessFile(src.file, "r").use { f ->
            f.seek(start)
            val buf = ByteArray(256 * 1024)
            var left = end - start + 1
            while (left > 0) {
                val n = f.read(buf, 0, minOf(buf.size.toLong(), left).toInt())
                if (n < 0) break
                out.write(buf, 0, n)
                left -= n
            }
        }
    }

    private fun serveRemote(src: Source.Remote, method: String, range: String?, out: OutputStream) {
        val conn = (URL(src.url).openConnection() as HttpURLConnection).apply {
            connectTimeout = 15_000
            readTimeout = 30_000
            requestMethod = if (method == "HEAD") "HEAD" else "GET"
            range?.let { setRequestProperty("Range", it) }
        }
        try {
            val code = conn.responseCode
            val head = buildString {
                append("HTTP/1.1 $code ${if (code == 206) "Partial Content" else if (code == 200) "OK" else "Error"}\r\n")
                append(commonHeaders(src))
                conn.getHeaderField("Content-Length")?.let { append("Content-Length: $it\r\n") }
                conn.getHeaderField("Content-Range")?.let { append("Content-Range: $it\r\n") }
                append("\r\n")
            }
            out.write(head.toByteArray())
            if (method == "HEAD" || code >= 400) return
            conn.inputStream.use { it.copyTo(out, 256 * 1024) }
        } finally {
            conn.disconnect()
        }
    }

    private fun parseRange(range: String?, len: Long): Pair<Long, Long> {
        val m = range?.let { Regex("""bytes=(\d*)-(\d*)""").find(it) } ?: return 0L to len - 1
        val (a, b) = m.destructured
        return when {
            a.isEmpty() && b.isNotEmpty() -> (len - b.toLong()).coerceAtLeast(0) to len - 1
            else -> a.toLong().coerceIn(0, len - 1) to (b.toLongOrNull() ?: (len - 1)).coerceAtMost(len - 1)
        }
    }

    private fun readLine(input: InputStream): String? {
        val sb = StringBuilder()
        while (true) {
            val c = input.read()
            if (c < 0) return if (sb.isEmpty()) null else sb.toString()
            if (c == '\n'.code) return sb.toString().trimEnd('\r')
            sb.append(c.toChar())
            if (sb.length > 8192) return null
        }
    }

    companion object {
        private const val TAG = "KuroRelay"
        const val DLNA_FEATURES = "DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000"
    }
}
