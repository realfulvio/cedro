package it.cedro.android;

import android.app.NativeActivity;
import android.content.ClipData;
import android.content.Intent;
import android.net.Uri;

/**
 * Tutto Cedro è Rust. Questa classe esiste solo perché NativeActivity non inoltra onActivityResult né onNewIntent:
 * senza, il selettore di file non può restituire la scelta e una nuova condivisione a app aperta si perde.
 * Rust legge tutto con scelto() e novita().
 */
public class CedroActivity extends NativeActivity {
    public static final int CODICE_SCELTA = 4242;
    private volatile Uri[] scelti;
    private volatile boolean novita;

    /** Una nuova condivisione (o l'apertura dall'icona) mentre Cedro è già aperto. */
    @Override
    protected void onNewIntent(Intent intento) {
        super.onNewIntent(intento);
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
    protected void onActivityResult(int richiesta, int esito, Intent dati) {
        if (richiesta == CODICE_SCELTA && esito == RESULT_OK && dati != null) {
            // Con la scelta multipla i file arrivano nel ClipData; con uno solo, in getData().
            ClipData c = dati.getClipData();
            if (c != null && c.getItemCount() > 0) {
                Uri[] u = new Uri[c.getItemCount()];
                for (int i = 0; i < u.length; i++) u[i] = c.getItemAt(i).getUri();
                scelti = u;
            } else if (dati.getData() != null) {
                scelti = new Uri[] { dati.getData() };
            }
        } else {
            super.onActivityResult(richiesta, esito, dati);
        }
    }

    /** I file scelti nell'ultimo selettore, una volta sola (null se non c'è niente di nuovo). */
    public Uri[] scelti() {
        Uri[] u = scelti;
        scelti = null;
        return u;
    }
}
