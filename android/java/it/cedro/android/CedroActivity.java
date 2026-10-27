package it.cedro.android;

import android.app.NativeActivity;
import android.content.Intent;
import android.net.Uri;

/**
 * Tutto Cedro è Rust. Questa classe esiste solo perché NativeActivity non inoltra onActivityResult:
 * senza, il selettore di file di Android non può restituire la scelta. Rust la legge con scelto().
 */
public class CedroActivity extends NativeActivity {
    public static final int CODICE_SCELTA = 4242;
    private volatile Uri scelto;

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
