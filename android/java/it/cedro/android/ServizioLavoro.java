package it.cedro.android;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.os.IBinder;
import android.os.PowerManager;

/**
 * Servizio in primo piano: tiene vivo Cedro (e il processore sveglio) mentre convertono i lavori lunghi,
 * con la notifica che mostra l'avanzamento. Non ha logica: titolo, testo e percentuale arrivano da Rust
 * negli extra dell'intento; con "ferma" il servizio si chiude.
 */
public class ServizioLavoro extends Service {
    private static final String CANALE = "lavori";
    private static final int ID = 1;
    private PowerManager.WakeLock wake;

    @Override
    public void onCreate() {
        super.onCreate();
        NotificationManager nm = getSystemService(NotificationManager.class);
        NotificationChannel c = new NotificationChannel(CANALE, "Lavori", NotificationManager.IMPORTANCE_LOW);
        c.setDescription("Avanzamento delle conversioni");
        nm.createNotificationChannel(c);
        PowerManager pm = getSystemService(PowerManager.class);
        wake = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "cedro:lavori");
        wake.setReferenceCounted(false);
    }

    @Override
    public int onStartCommand(Intent intento, int flags, int id) {
        if (intento != null && intento.getBooleanExtra("ferma", false)) {
            if (wake.isHeld()) wake.release();
            stopForeground(true);
            stopSelf();
            return START_NOT_STICKY;
        }
        String titolo = intento != null ? intento.getStringExtra("titolo") : null;
        String testo = intento != null ? intento.getStringExtra("testo") : null;
        int percento = intento != null ? intento.getIntExtra("percento", -1) : -1;
        Intent apri = new Intent(this, CedroActivity.class);
        apri.addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP);
        PendingIntent pi = PendingIntent.getActivity(this, 0, apri, PendingIntent.FLAG_UPDATE_CURRENT);
        Notification.Builder b = new Notification.Builder(this, CANALE)
                .setSmallIcon(android.R.drawable.stat_sys_download)
                .setContentTitle(titolo != null ? titolo : "Cedro")
                .setContentText(testo != null ? testo : "")
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setContentIntent(pi)
                .setProgress(100, Math.max(percento, 0), percento < 0);
        if (!wake.isHeld()) wake.acquire(60 * 60 * 1000L);
        startForeground(ID, b.build());
        return START_NOT_STICKY;
    }

    @Override
    public void onDestroy() {
        if (wake != null && wake.isHeld()) wake.release();
        super.onDestroy();
    }

    @Override
    public IBinder onBind(Intent intento) {
        return null;
    }
}
