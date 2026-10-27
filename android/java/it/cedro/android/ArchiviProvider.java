package it.cedro.android;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import android.webkit.MimeTypeMap;

import java.io.File;
import java.io.FileNotFoundException;

/**
 * Serve i risultati in Documenti con un indirizzo che finisce col nome del file
 * (content://it.cedro.android.files/Documents/vocale.txt): il foglio di condivisione di Android lo mostra com'è,
 * mentre l'indirizzo di MediaStore finisce con un numero («3212»). È un FileProvider fatto a mano, senza androidx.
 */
public class ArchiviProvider extends ContentProvider {
    private static final String RADICE = "/storage/emulated/0/Documents/";

    private File file(Uri uri) throws FileNotFoundException {
        // Niente getCanonicalFile(): /storage/emulated/0 è un collegamento e il percorso «vero» non comincia più per
        // RADICE (il file risultava non trovato e Telegram non inviava niente). Si normalizza e basta (toglie «..»).
        String voluto = "/storage/emulated/0" + uri.getPath();
        String pulito = java.nio.file.Paths.get(voluto).normalize().toString();
        File f = new File(pulito);
        if (!pulito.startsWith(RADICE) || !f.isFile()) throw new FileNotFoundException(uri.toString());
        return f;
    }

    @Override public boolean onCreate() { return true; }

    @Override public String getType(Uri uri) {
        String nome = uri.getLastPathSegment();
        String ext = nome == null ? "" : MimeTypeMap.getFileExtensionFromUrl(nome.replace(' ', '_')).toLowerCase();
        String m = MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext);
        return m != null ? m : "application/octet-stream";
    }

    @Override public Cursor query(Uri uri, String[] proiezione, String sel, String[] args, String ord) {
        File f;
        try { f = file(uri); } catch (FileNotFoundException e) { return null; }
        String[] colonne = proiezione != null ? proiezione : new String[] { OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE };
        MatrixCursor c = new MatrixCursor(colonne, 1);
        Object[] riga = new Object[colonne.length];
        for (int i = 0; i < colonne.length; i++) {
            if (OpenableColumns.DISPLAY_NAME.equals(colonne[i])) riga[i] = f.getName();
            else if (OpenableColumns.SIZE.equals(colonne[i])) riga[i] = f.length();
        }
        c.addRow(riga);
        return c;
    }

    @Override public ParcelFileDescriptor openFile(Uri uri, String modo) throws FileNotFoundException {
        return ParcelFileDescriptor.open(file(uri), ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override public Uri insert(Uri uri, ContentValues v) { throw new UnsupportedOperationException(); }
    @Override public int delete(Uri uri, String s, String[] a) { throw new UnsupportedOperationException(); }
    @Override public int update(Uri uri, ContentValues v, String s, String[] a) { throw new UnsupportedOperationException(); }
}
