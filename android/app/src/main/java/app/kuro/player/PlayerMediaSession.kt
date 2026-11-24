package app.kuro.player

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.IntentFilter
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.support.v4.media.MediaMetadataCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.media.app.NotificationCompat.MediaStyle
import app.kuro.R

/**
 * Ovládání mimo přehrávač: notifikace s tlačítky, zamčená obrazovka,
 * sluchátka a Bluetooth (play/pauza, ±10 s, další díl). Žije jen po
 * dobu přehrávání.
 */
class PlayerMediaSession(private val context: Context, private val callbacks: Callbacks) {
    interface Callbacks {
        fun play()
        fun pause()
        fun seekBy(seconds: Double)
        fun seekTo(seconds: Double)
        fun next()
        fun stop()
    }

    private val session = MediaSessionCompat(context, "Kuro").apply {
        setCallback(object : MediaSessionCompat.Callback() {
            override fun onPlay() = callbacks.play()
            override fun onPause() = callbacks.pause()
            override fun onFastForward() = callbacks.seekBy(10.0)
            override fun onRewind() = callbacks.seekBy(-10.0)
            override fun onSeekTo(pos: Long) = callbacks.seekTo(pos / 1000.0)
            override fun onSkipToNext() = callbacks.next()
            override fun onStop() = callbacks.stop()
        })
        // Klepnutí na notifikaci → zpět do přehrávače.
        setSessionActivity(
            PendingIntent.getActivity(
                context, 0,
                Intent(context, PlayerActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
            ),
        )
        isActive = true
    }

    private var title = ""
    private var subtitle: String? = null
    private var hasNext = false

    init {
        val nm = context.getSystemService(NotificationManager::class.java)
        if (nm.getNotificationChannel(CHANNEL) == null) {
            nm.createNotificationChannel(
                NotificationChannel(CHANNEL, "Přehrávání", NotificationManager.IMPORTANCE_LOW).apply {
                    description = "Ovládání přehrávání v notifikaci a na zamčené obrazovce"
                    setShowBadge(false)
                },
            )
        }
    }

    fun setMetadata(title: String, subtitle: String?, durationSec: Double, hasNext: Boolean) {
        this.title = title
        this.subtitle = subtitle
        this.hasNext = hasNext
        session.setMetadata(
            MediaMetadataCompat.Builder()
                .putString(MediaMetadataCompat.METADATA_KEY_TITLE, title)
                .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, subtitle ?: "Kuro")
                .putLong(MediaMetadataCompat.METADATA_KEY_DURATION, (durationSec * 1000).toLong())
                .build(),
        )
    }

    fun update(playing: Boolean, positionSec: Double, speed: Double) {
        var actions = PlaybackStateCompat.ACTION_PLAY or PlaybackStateCompat.ACTION_PAUSE or
            PlaybackStateCompat.ACTION_PLAY_PAUSE or PlaybackStateCompat.ACTION_SEEK_TO or
            PlaybackStateCompat.ACTION_FAST_FORWARD or PlaybackStateCompat.ACTION_REWIND or PlaybackStateCompat.ACTION_STOP
        if (hasNext) actions = actions or PlaybackStateCompat.ACTION_SKIP_TO_NEXT
        session.setPlaybackState(
            PlaybackStateCompat.Builder()
                .setActions(actions)
                .setState(
                    if (playing) PlaybackStateCompat.STATE_PLAYING else PlaybackStateCompat.STATE_PAUSED,
                    (positionSec * 1000).toLong(),
                    if (playing) speed.toFloat() else 0f,
                )
                .build(),
        )
        showNotification(playing)
    }

    /** Tlačítka notifikace → broadcast jen pro tuhle aplikaci → callbacky. */
    private val receiver = object : BroadcastReceiver() {
        override fun onReceive(c: Context, intent: Intent) {
            when (intent.getStringExtra(EXTRA_CMD)) {
                CMD_PLAY -> callbacks.play()
                CMD_PAUSE -> callbacks.pause()
                CMD_REW -> callbacks.seekBy(-10.0)
                CMD_FF -> callbacks.seekBy(10.0)
                CMD_NEXT -> callbacks.next()
            }
        }
    }.also {
        ContextCompat.registerReceiver(context, it, IntentFilter(ACTION), ContextCompat.RECEIVER_NOT_EXPORTED)
    }

    private fun action(icon: Int, label: String, cmd: String) = NotificationCompat.Action(
        icon, label,
        PendingIntent.getBroadcast(
            context, cmd.hashCode(),
            Intent(ACTION).setPackage(context.packageName).putExtra(EXTRA_CMD, cmd),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        ),
    )

    private fun showNotification(playing: Boolean) {
        if (Build.VERSION.SDK_INT >= 33 &&
            context.checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        val b = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_kuro_mark)
            .setContentTitle(title)
            .setContentText(subtitle)
            .setContentIntent(session.controller.sessionActivity)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setOngoing(playing)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .addAction(action(android.R.drawable.ic_media_rew, "Zpět 10 s", CMD_REW))
            .addAction(
                if (playing) {
                    action(android.R.drawable.ic_media_pause, "Pauza", CMD_PAUSE)
                } else {
                    action(android.R.drawable.ic_media_play, "Přehrát", CMD_PLAY)
                },
            )
            .addAction(action(android.R.drawable.ic_media_ff, "Vpřed 10 s", CMD_FF))
        if (hasNext) b.addAction(action(android.R.drawable.ic_media_next, "Další díl", CMD_NEXT))
        b.setStyle(MediaStyle().setMediaSession(session.sessionToken).setShowActionsInCompactView(0, 1, 2))
        runCatching { NotificationManagerCompat.from(context).notify(NOTIFICATION_ID, b.build()) }
    }

    fun release() {
        runCatching { context.unregisterReceiver(receiver) }
        NotificationManagerCompat.from(context).cancel(NOTIFICATION_ID)
        session.isActive = false
        session.release()
    }

    companion object {
        private const val CHANNEL = "playback"
        private const val NOTIFICATION_ID = 4242
        private const val ACTION = "app.kuro.PLAYER_CONTROL"
        private const val EXTRA_CMD = "cmd"
        private const val CMD_PLAY = "play"
        private const val CMD_PAUSE = "pause"
        private const val CMD_REW = "rew"
        private const val CMD_FF = "ff"
        private const val CMD_NEXT = "next"
    }
}
