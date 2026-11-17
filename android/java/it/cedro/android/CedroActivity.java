package it.cedro.android;

import android.app.NativeActivity;
import android.content.ClipData;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;

/**
 * Tutto Cedro è Rust. Questa classe esiste solo perché NativeActivity non inoltra onActivityResult né onNewIntent:
 * senza, il selettore di file non può restituire la scelta e una nuova condivisione a app aperta si perde.
 * Rust legge tutto con scelto() e novita().
 */
public class CedroActivity extends NativeActivity implements android.media.AudioManager.OnAudioFocusChangeListener {
    public static final int CODICE_SCELTA = 4242;
    private volatile Uri[] scelti;
    private volatile boolean novita;

    private android.media.AudioManager audioManager;
    private android.media.AudioFocusRequest audioRequest;
    private boolean audioOwned;
    private volatile boolean audioStop;
    private final android.content.BroadcastReceiver audioNoisy = new android.content.BroadcastReceiver() {
        @Override public void onReceive(android.content.Context context, Intent intent) { audioStop = true; }
    };

    // NativeActivity/AAudio non consegnano a Rust la callback Java di AudioManager.
    // Questo ponte non riproduce audio: Rust continua a gestire player, PCM e posizione.
    @Override public void onAudioFocusChange(int focus) {
        if (focus <= 0) audioStop = true; // anche duck: il parlato si mette in pausa
    }

    public synchronized boolean audioFocus(boolean playing) {
        if (audioManager == null) {
            audioManager = (android.media.AudioManager)getSystemService(AUDIO_SERVICE);
            audioRequest = new android.media.AudioFocusRequest.Builder(android.media.AudioManager.AUDIOFOCUS_GAIN)
                .setAudioAttributes(new android.media.AudioAttributes.Builder()
                    .setUsage(android.media.AudioAttributes.USAGE_MEDIA)
                    .setContentType(android.media.AudioAttributes.CONTENT_TYPE_SPEECH).build())
                .setOnAudioFocusChangeListener(this, new android.os.Handler(getMainLooper())).build();
        }
        boolean stop = audioStop;
        audioStop = false;
        if ((!playing || stop) && audioOwned) {
            audioManager.abandonAudioFocusRequest(audioRequest);
            audioOwned = false;
        }
        if (playing && !stop && !audioOwned) {
            audioOwned = audioManager.requestAudioFocus(audioRequest) == android.media.AudioManager.AUDIOFOCUS_REQUEST_GRANTED;
        }
        return !playing || (!stop && audioOwned);
    }

    @Override public boolean dispatchKeyEvent(android.view.KeyEvent event) {
        int key = event.getKeyCode();
        if (key == android.view.KeyEvent.KEYCODE_MEDIA_PAUSE || key == android.view.KeyEvent.KEYCODE_MEDIA_STOP) {
            if (event.getAction() == android.view.KeyEvent.ACTION_DOWN) audioStop = true;
            return true;
        }
        return super.dispatchKeyEvent(event);
    }

    @Override protected void onDestroy() {
        unregisterReceiver(audioNoisy);
        audioFocus(false);
        super.onDestroy();
    }

    @Override
    protected void onCreate(Bundle stato) {
        super.onCreate(stato);
        ricordaIntento(getIntent());
        registerReceiver(audioNoisy, new android.content.IntentFilter(android.media.AudioManager.ACTION_AUDIO_BECOMING_NOISY));
    }

    private void ricordaIntento(Intent intento) {
        if (intento == null) return;
        int flags = intento.getFlags();
        if (intento.getData() != null) ricordaPermesso(intento.getData(), flags);
        ClipData clip = intento.getClipData();
        if (clip != null) for (int i = 0; i < clip.getItemCount(); i++) {
            Uri uri = clip.getItemAt(i).getUri();
            if (uri != null) ricordaPermesso(uri, flags);
        }
        android.os.Parcelable stream = intento.getParcelableExtra(Intent.EXTRA_STREAM);
        if (stream instanceof Uri) ricordaPermesso((Uri) stream, flags);
    }

    /** Una nuova condivisione (o l'apertura dall'icona) mentre Cedro è già aperto. */
    @Override
    protected void onNewIntent(Intent intento) {
        super.onNewIntent(intento);
        ricordaIntento(intento);
        setIntent(intento);
        novita = true;
    }

    /** Vero, una volta sola, se è arrivato un nuovo intento da leggere con getIntent(). */
    public boolean novita() {
        boolean n = novita;
        novita = false;
        return n;
    }

    @Override
    protected synchronized void onActivityResult(int richiesta, int esito, Intent dati) {
        if (richiesta == CODICE_SCELTA && esito == RESULT_OK && dati != null) {
            // Con la scelta multipla i file arrivano nel ClipData; con uno solo, in getData().
            ClipData c = dati.getClipData();
            if (c != null && c.getItemCount() > 0) {
                Uri[] u = new Uri[c.getItemCount()];
                for (int i = 0; i < u.length; i++) u[i] = c.getItemAt(i).getUri();
                for (Uri uri : u) ricordaPermesso(uri, dati.getFlags());
                scelti = u;
            } else if (dati.getData() != null) {
                ricordaPermesso(dati.getData(), dati.getFlags());
                scelti = new Uri[] { dati.getData() };
            }
        } else {
            super.onActivityResult(richiesta, esito, dati);
        }
    }

    private void ricordaPermesso(Uri uri, int flags) {
        if ((flags & Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION) == 0) return;
        int grant = flags & Intent.FLAG_GRANT_READ_URI_PERMISSION;
        if (grant == 0) return;
        try { getContentResolver().takePersistableUriPermission(uri, grant); }
        catch (SecurityException e) { android.util.Log.w("Cedro", "Il provider non conserva il permesso", e); }
    }

    /** I file scelti nell'ultimo selettore, una volta sola (null se non c'è niente di nuovo). */
    public synchronized Uri[] scelti() {
        Uri[] u = scelti;
        scelti = null;
        return u;
    }
}
