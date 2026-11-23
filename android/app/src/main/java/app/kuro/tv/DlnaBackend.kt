package app.kuro.tv

import android.content.Context
import android.net.wifi.WifiManager
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.HttpURLConnection
import java.net.InetAddress
import java.net.SocketTimeoutException
import java.net.URL
import java.util.concurrent.ConcurrentHashMap

/**
 * DLNA / UPnP "MediaRenderer" — umí ho většina chytrých televizí (LG webOS,
 * Samsung, Sony…). Telefon televizi pošle adresu videa a pak ji ovládá
 * (AVTransport: přehrát, pauza, posun, pozice).
 */
class DlnaBackend(context: Context, private val relay: RelayServer) : TvBackend {
    private val app = context.applicationContext
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var discovery: Job? = null
    private val renderers = ConcurrentHashMap<String, Renderer>()
    private var current: Renderer? = null

    data class Renderer(val id: String, val name: String, val avTransportUrl: String)

    override fun startDiscovery(onFound: (List<TvDevice>) -> Unit) {
        discovery?.cancel()
        discovery = scope.launch {
            val wifi = app.getSystemService(WifiManager::class.java)
            // Bez zámku Android multicast odpovědi na Wi-Fi zahazuje.
            val lock = wifi?.createMulticastLock("kuro-dlna")?.apply { setReferenceCounted(false); acquire() }
            try {
                repeat(3) {
                    if (!isActive) return@launch
                    search { r ->
                        if (renderers.putIfAbsent(r.id, r) == null) {
                            onFound(renderers.values.map { TvDevice(it.id, it.name, TvDevice.Kind.DLNA) })
                        }
                    }
                    delay(4_000)
                }
            } finally {
                runCatching { lock?.release() }
            }
        }
    }

    override fun stopDiscovery() {
        discovery?.cancel()
    }

    /** SSDP M-SEARCH; každá odpověď → popis zařízení → AVTransport. */
    private suspend fun search(onRenderer: (Renderer) -> Unit) = withContext(Dispatchers.IO) {
        val msg = (
            "M-SEARCH * HTTP/1.1\r\n" +
                "HOST: 239.255.255.250:1900\r\n" +
                "MAN: \"ssdp:discover\"\r\n" +
                "MX: 2\r\n" +
                "ST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n"
            ).toByteArray()
        DatagramSocket().use { sock ->
            sock.soTimeout = 3_000
            sock.send(DatagramPacket(msg, msg.size, InetAddress.getByName("239.255.255.250"), 1900))
            val seen = mutableSetOf<String>()
            val buf = ByteArray(4096)
            while (true) {
                val p = DatagramPacket(buf, buf.size)
                try {
                    sock.receive(p)
                } catch (_: SocketTimeoutException) {
                    break
                }
                val location = Dlna.header(String(p.data, 0, p.length), "location") ?: continue
                if (!seen.add(location)) continue
                runCatching { describe(location) }.onSuccess { r -> r?.let(onRenderer) }
                    .onFailure { Log.d(TAG, "popis $location", it) }
            }
        }
    }

    private fun describe(location: String): Renderer? {
        val xml = httpGet(location)
        val name = Dlna.tag(xml, "friendlyName") ?: return null
        val udn = Dlna.tag(xml, "UDN") ?: location
        val control = Dlna.avTransportControl(xml) ?: return null
        return Renderer(udn, name, URL(URL(location), control).toString())
    }

    override suspend fun connect(device: TvDevice) {
        current = renderers[device.id] ?: error("Televize už není v síti.")
    }

    override suspend fun load(media: TvMedia) = withContext(Dispatchers.IO) {
        val r = current ?: error("Není připojená televize.")
        val mime = guessMime(media.url, media.provider)
        // Stažené soubory a HTTPS (spousta televizí HTTPS neumí) přes telefon.
        val needsRelay = media.url.startsWith("/") || media.url.startsWith("https://")
        val direct = media.url.takeUnless { needsRelay }
        val relayed = if (needsRelay) relay.serve(media.url, mime) else null
        val url = direct ?: relayed ?: error("Telefon není ve Wi-Fi síti s televizí.")

        runCatching { soap(r, "Stop", "") }
        soap(
            r, "SetAVTransportURI",
            "<CurrentURI>${Dlna.xml(url)}</CurrentURI>" +
                "<CurrentURIMetaData>${Dlna.xml(Dlna.didl(url, media.title, mime))}</CurrentURIMetaData>",
        )
        soap(r, "Play", "<Speed>1</Speed>")
        if (media.startSec > 5) {
            // Posun až když TV opravdu hraje (dřív ho většina ignoruje).
            repeat(20) {
                delay(1_000)
                if (transportState(r) == "PLAYING") {
                    runCatching { seekTo(r, media.startSec.toDouble()) }
                    return@withContext
                }
            }
        }
    }

    override suspend fun setPaused(paused: Boolean) {
        val r = current ?: return
        withContext(Dispatchers.IO) { soap(r, if (paused) "Pause" else "Play", if (paused) "" else "<Speed>1</Speed>") }
    }

    override suspend fun seek(sec: Double) {
        val r = current ?: return
        withContext(Dispatchers.IO) { seekTo(r, sec) }
    }

    private fun seekTo(r: Renderer, sec: Double) =
        soap(r, "Seek", "<Unit>REL_TIME</Unit><Target>${Dlna.hms(sec)}</Target>")

    private fun transportState(r: Renderer): String? =
        runCatching { Dlna.tag(soap(r, "GetTransportInfo", ""), "CurrentTransportState") }.getOrNull()

    override suspend fun status(): TvStatus? = withContext(Dispatchers.IO) {
        val r = current ?: return@withContext null
        val pos = soap(r, "GetPositionInfo", "")
        val state = transportState(r)
        val position = Dlna.seconds(Dlna.tag(pos, "RelTime"))
        val duration = Dlna.seconds(Dlna.tag(pos, "TrackDuration"))
        TvStatus(
            positionSec = position,
            durationSec = duration,
            playing = state == "PLAYING" || state == "TRANSITIONING",
            // Po dohrání TV hlásí STOPPED a pozici 0 / konec.
            ended = state == "STOPPED" && duration > 0 && (position == 0.0 || position >= duration - 2),
        )
    }

    override suspend fun stop() {
        val r = current ?: return
        withContext(Dispatchers.IO) { runCatching { soap(r, "Stop", "") } }
    }

    override fun disconnect() {
        current = null
    }

    private fun soap(r: Renderer, action: String, args: String): String {
        val body = """<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
<s:Body><u:$action xmlns:u="$AVT"><InstanceID>0</InstanceID>$args</u:$action></s:Body></s:Envelope>"""
        val conn = (URL(r.avTransportUrl).openConnection() as HttpURLConnection).apply {
            connectTimeout = 5_000
            readTimeout = 10_000
            requestMethod = "POST"
            doOutput = true
            setRequestProperty("Content-Type", "text/xml; charset=\"utf-8\"")
            setRequestProperty("SOAPACTION", "\"$AVT#$action\"")
        }
        try {
            conn.outputStream.use { it.write(body.toByteArray()) }
            val code = conn.responseCode
            val text = (if (code < 400) conn.inputStream else conn.errorStream)?.use { it.readBytes().decodeToString() }.orEmpty()
            if (code >= 400) {
                val desc = Dlna.tag(text, "errorDescription") ?: "HTTP $code"
                throw IllegalStateException(
                    if (action == "SetAVTransportURI") "Televize video odmítla ($desc). Zkus jiný zdroj." else "$action: $desc",
                )
            }
            return text
        } finally {
            conn.disconnect()
        }
    }

    private fun httpGet(url: String): String {
        val conn = (URL(url).openConnection() as HttpURLConnection).apply {
            connectTimeout = 4_000
            readTimeout = 6_000
        }
        return try {
            conn.inputStream.use { it.readBytes().decodeToString() }
        } finally {
            conn.disconnect()
        }
    }

    companion object {
        private const val TAG = "KuroDlna"
        private const val AVT = "urn:schemas-upnp-org:service:AVTransport:1"
    }
}

/** Čisté pomocné funkce DLNA (bez sítě). */
object Dlna {
    fun header(response: String, name: String): String? =
        response.lineSequence().firstOrNull { it.substringBefore(':').trim().equals(name, ignoreCase = true) }
            ?.substringAfter(':')?.trim()?.takeIf { it.isNotEmpty() }

    /** Obsah prvního elementu (bez ohledu na prefix jmenného prostoru), odescapovaný. */
    fun tag(xml: String, name: String): String? =
        Regex("""<(?:\w+:)?$name(?:\s[^>]*)?>([\s\S]*?)</(?:\w+:)?$name>""").find(xml)?.groupValues?.get(1)?.trim()
            ?.let(::unxml)

    /** controlURL služby AVTransport z popisu zařízení. */
    fun avTransportControl(description: String): String? =
        Regex("""<service>([\s\S]*?)</service>""").findAll(description)
            .map { it.groupValues[1] }
            .firstOrNull { (tag(it, "serviceType") ?: "").contains("AVTransport") }
            ?.let { tag(it, "controlURL") }

    fun xml(s: String) = s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\"", "&quot;")

    private fun unxml(s: String) = s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")

    fun didl(url: String, title: String, mime: String) =
        """<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" """ +
            """xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/"><item id="0" parentID="-1" restricted="1">""" +
            "<dc:title>${xml(title)}</dc:title><upnp:class>object.item.videoItem</upnp:class>" +
            """<res protocolInfo="http-get:*:$mime:${RelayServer.DLNA_FEATURES}">${xml(url)}</res></item></DIDL-Lite>"""

    fun hms(sec: Double): String {
        val s = sec.toLong().coerceAtLeast(0)
        return "%d:%02d:%02d".format(s / 3600, (s % 3600) / 60, s % 60)
    }

    /** "1:02:03" / "01:02:03.500" → sekundy; "NOT_IMPLEMENTED" → 0. */
    fun seconds(hms: String?): Double {
        val parts = hms?.split(':') ?: return 0.0
        if (parts.size != 3) return 0.0
        val h = parts[0].toDoubleOrNull() ?: return 0.0
        val m = parts[1].toDoubleOrNull() ?: return 0.0
        val s = parts[2].toDoubleOrNull() ?: return 0.0
        return h * 3600 + m * 60 + s
    }
}

/** MIME pro televizi: převody z Přehraj.to/Hellspy jsou MP4, jinak podle přípony. */
fun guessMime(url: String, provider: app.kuro.core.Provider): String {
    val path = url.substringBefore('?').lowercase()
    return when {
        path.endsWith(".mkv") -> "video/x-matroska"
        path.endsWith(".avi") -> "video/x-msvideo"
        path.endsWith(".m3u8") -> "application/x-mpegURL"
        path.endsWith(".webm") -> "video/webm"
        path.endsWith(".mp4") || path.endsWith(".m4v") -> "video/mp4"
        provider != app.kuro.core.Provider.WEBSHARE -> "video/mp4"
        else -> "video/x-matroska"
    }
}
