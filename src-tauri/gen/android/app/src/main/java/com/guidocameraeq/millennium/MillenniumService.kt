/*
 * Millennium Clipboard — Android foreground service.
 *
 * Responsibilities:
 *   1. Acquire a WifiManager.MulticastLock so the kernel does NOT
 *      filter out the multicast / UDP-broadcast packets that mDNS
 *      and our own UDP discovery rely on once the screen turns off.
 *   2. Show a persistent notification so Android lets the process
 *      keep its sockets alive even when the activity is in the
 *      background (Android 8+ requires foreground-service status
 *      for any long-lived network work).
 *
 * The Rust side keeps running inside the same process; we don't bind
 * anything to this service directly. It exists purely to extend the
 * process lifetime + unfilter multicast traffic.
 */
package com.guidocameraeq.millennium

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat

class MillenniumService : Service() {

    companion object {
        private const val CHANNEL_ID = "millennium_fg"
        private const val NOTIF_ID = 7777
        private const val MULTICAST_LOCK_TAG = "MillenniumMulticast"
    }

    private var multicastLock: WifiManager.MulticastLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        ensureChannel()
        acquireMulticastLock()
        startForeground(NOTIF_ID, buildNotification())
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // START_STICKY so Android restarts us if the OS reclaims memory.
        return START_STICKY
    }

    override fun onDestroy() {
        releaseMulticastLock()
        super.onDestroy()
    }

    private fun ensureChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Millennium peer link",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Keeps Millennium reachable for peers when the app is in the background."
                setShowBadge(false)
            }
            (getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager)
                .createNotificationChannel(channel)
        }
    }

    private fun buildNotification(): Notification {
        val openIntent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        val piFlags = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M)
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        else
            PendingIntent.FLAG_UPDATE_CURRENT
        val pendingIntent = PendingIntent.getActivity(this, 0, openIntent, piFlags)

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("Millennium Clipboard")
            .setContentText("Linked — receiving transfers in the background")
            .setSmallIcon(R.mipmap.ic_launcher)
            .setOngoing(true)
            .setSilent(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setContentIntent(pendingIntent)
            .build()
    }

    private fun acquireMulticastLock() {
        try {
            val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
            multicastLock = wifi.createMulticastLock(MULTICAST_LOCK_TAG).apply {
                setReferenceCounted(false)
                acquire()
            }
        } catch (_: Exception) {
            // Lock is best-effort — discovery still works in foreground
            // even without it, just not with the screen off.
        }
    }

    private fun releaseMulticastLock() {
        try {
            multicastLock?.takeIf { it.isHeld }?.release()
        } catch (_: Exception) {
        }
        multicastLock = null
    }
}
