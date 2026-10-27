package it.cedro.android;

import android.app.NativeActivity;
import android.content.Intent;
import android.net.Uri;

/**
 * Tutto Cedro è Rust. Questa classe esiste solo perché NativeActivity non inoltra onActivityResult né onNewIntent:
 * senza, il selettore di file non può restituire la scelta e una nuova condivisione a app aperta si perde.
 * Rust legge tutto con scelto() e novita().
 */
public class CedroActivity extends NativeActivity {
    public static final int CODICE_SCELTA = 4242;
    private volatile Uri scelto;
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
        if (richiesta == CODICE_SCELTA && esito == RESULT_OK && dati != null && dati.getData() != null) {
            scelto = dati.getData();
        } else {
            super.onActivityResult(richiesta, esito, dati);
        }
    }

    /** Il file scelto dall'ultimo selettore, una volta sola (null se non c'è niente di nuovo). */
    public Uri scelto() {
        Uri u = scelto;
        scelto = null;
        return u;
    }
}
