package it.cedro.android;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.Build;
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
    /**
     * Vero dopo startForeground. Rust ferma il servizio solo quando è vero: un stopService arrivato prima di onCreate
     * (lavoro brevissimo, thread principale occupato) fa chiudere l'app ad Android 9+ con
     * «Context.startForegroundService() did not then call Service.startForeground()».
     */
    static volatile boolean inPrimoPiano;

    @Override
    public void onCreate() {
        super.onCreate();
        NotificationManager nm = getSystemService(NotificationManager.class);
        NotificationChannel c = new NotificationChannel(CANALE, "Lavori", NotificationManager.IMPORTANCE_LOW);
        c.setDescription("Avanzamento delle conversioni");
        nm.createNotificationChannel(c);
        // Il lavoro può finire prima di onStartCommand: soddisfa subito l'avvio
        // in primo piano, anche quando stopService è già in coda.
        aggiornaNotifica("Cedro", "", -1);
        PowerManager pm = getSystemService(PowerManager.class);
        wake = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "cedro:lavori");
        wake.setReferenceCounted(false);
    }

    @Override
    public int onStartCommand(Intent intento, int flags, int id) {
        if (intento == null || intento.getBooleanExtra("ferma", false)) {
            liberaRisorse();
            stopSelf(id);
            return START_NOT_STICKY;
        }
        String titolo = intento.getStringExtra("titolo");
        String testo = intento.getStringExtra("testo");
        int percento = intento.getIntExtra("percento", -1);
        aggiornaNotifica(titolo, testo, percento);
        if (!wake.isHeld()) wake.acquire(60 * 60 * 1000L);
        return START_NOT_STICKY;
    }

    private void aggiornaNotifica(String titolo, String testo, int percento) {
        Intent apri = new Intent(this, CedroActivity.class);
        apri.addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP);
        PendingIntent pi = PendingIntent.getActivity(this, 0, apri, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        Notification.Builder b = new Notification.Builder(this, CANALE)
                .setSmallIcon(android.R.drawable.stat_sys_download)
                .setContentTitle(titolo != null ? titolo : "Cedro")
                .setContentText(testo != null ? testo : "")
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setContentIntent(pi)
                .setProgress(100, Math.max(percento, 0), percento < 0);
        // Da Android 14 il servizio in primo piano deve dire che tipo di lavoro fa.
        if (Build.VERSION.SDK_INT >= 29) startForeground(ID, b.build(), ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC);
        else startForeground(ID, b.build());
        inPrimoPiano = true;
    }

    private void liberaRisorse() {
        inPrimoPiano = false;
        if (wake != null && wake.isHeld()) wake.release();
        stopForeground(true);
    }

    @Override
    public void onDestroy() {
        liberaRisorse();
        super.onDestroy();
    }

    @Override
    public IBinder onBind(Intent intento) {
        return null;
    }
}
