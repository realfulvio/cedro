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
    private final ActivityDelivery<Uri[]> deliveries = new ActivityDelivery<>();
    private static final String PICKER_REQUEST="cedro.picker.request";

    public float fontScale() { return getResources().getConfiguration().fontScale; }

    /** Il servizio dei lavori ha già chiamato startForeground: si può fermare senza far chiudere l'app. */
    public boolean servizioPronto() { return ServizioLavoro.inPrimoPiano; }

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
        // NativeActivity can start Rust inside super.onCreate: publish this first.
        int savedRequest=stato==null?-1:stato.getInt(PICKER_REQUEST,-1);
        deliveries.created(stato != null,ActivityDelivery.restoredRequest(savedRequest,pickerJournalRequest()));
        super.onCreate(stato);
        accessibleSurface = new AccessibleSurface(this);
        addContentView(accessibleSurface, new android.view.ViewGroup.LayoutParams(-1, -1));
        ricordaIntento(getIntent());
        registerReceiver(audioNoisy, new android.content.IntentFilter(android.media.AudioManager.ACTION_AUDIO_BECOMING_NOISY));
    }

    @Override protected void onSaveInstanceState(Bundle stato) {
        stato.putInt(PICKER_REQUEST,deliveries.expectedRequest());
        super.onSaveInstanceState(stato);
    }

    @Override public void startActivityForResult(Intent intent,int requestCode) {
        startActivityForResult(intent,requestCode,null);
    }

    @Override public synchronized void startActivityForResult(Intent intent,int requestCode,Bundle options) {
        if (requestCode != CODICE_SCELTA) {
            super.startActivityForResult(intent,requestCode,options);
            return;
        }
        // Persist before launch, including across process recreation. Never reuse a code:
        // wrapping could associate an arbitrarily late result with another source.
        android.content.SharedPreferences state=getSharedPreferences("picker-generation",MODE_PRIVATE);
        int request=ActivityDelivery.nextRequest(state.getInt("last",ActivityDelivery.FIRST_REQUEST-1));
        if (!state.edit().putInt("last",request).commit()) throw new IllegalStateException("Cannot persist picker request");
        bindPickerRequest(request);
        deliveries.arm(request);
        try { super.startActivityForResult(intent,request,options); }
        catch (RuntimeException e) { deliveries.failed(request);throw e; }
    }

    private android.util.AtomicFile pickerJournal() {
        return new android.util.AtomicFile(new java.io.File(getFilesDir(),"scelta-in-attesa.json"));
    }

    private int pickerJournalRequest() {
        try {
            byte[] bytes=pickerJournal().readFully(); // also restores an interrupted atomic bind
            return new org.json.JSONObject(new String(bytes,java.nio.charset.StandardCharsets.UTF_8)).optInt("request_code",-1);
        } catch (java.io.IOException | org.json.JSONException e) { return -1; }
    }

    private void bindPickerRequest(int request) {
        android.util.AtomicFile journal=pickerJournal();
        java.io.FileOutputStream out=null;
        try {
            org.json.JSONObject context=new org.json.JSONObject(new String(journal.readFully(),java.nio.charset.StandardCharsets.UTF_8));
            context.put("request_code",request);
            out=journal.startWrite();
            out.write(context.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
            journal.finishWrite(out);
        } catch (java.io.IOException | org.json.JSONException e) {
            if (out!=null) journal.failWrite(out);
            throw new IllegalStateException("Cannot bind picker request",e);
        }
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
        deliveries.newIntent();
    }

    /** Vero, una volta sola, se è arrivato un nuovo intento da leggere con getIntent(). */
    public boolean novita() {
        return deliveries.takeNew();
    }

    /** Restored Activities carry the old getIntent(); only a fresh launch consumes it. */
    public boolean condivisioneIniziale() { return deliveries.takeInitial(); }

    /** Called by Rust only with the currently valid journal context. */
    public synchronized void contestoScelta(boolean pending) { deliveries.discardOrphan(pending); }

    @Override
    protected synchronized void onActivityResult(int richiesta, int esito, Intent dati) {
        if (ActivityDelivery.managed(richiesta) || richiesta==CODICE_SCELTA) {
            // Reject old callbacks even if they arrive after the next request has been armed.
            if (!deliveries.expects(richiesta)) return;
            Uri[] result=new Uri[0]; // completed cancellation, distinct from no callback
            if (esito==RESULT_OK && dati!=null) {
                ClipData c=dati.getClipData();
                if (c!=null && c.getItemCount()>0) {
                    result=new Uri[c.getItemCount()];
                    for (int i=0;i<result.length;i++) result[i]=c.getItemAt(i).getUri();
                } else if (dati.getData()!=null) {
                    result=new Uri[]{dati.getData()};
                }
                for (Uri uri:result) ricordaPermesso(uri,dati.getFlags());
            }
            deliveries.picker(richiesta,result);
        } else {
            super.onActivityResult(richiesta,esito,dati);
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
        return deliveries.takePicker();
    }
    // NativeActivity espone un'unica superficie: il provider riflette i controlli Rust reali.
    private AccessibleSurface accessibleSurface;
    public void accessibilitySnapshot(final String json) {
        runOnUiThread(new Runnable() { @Override public void run() { if (accessibleSurface != null) accessibleSurface.update(json); } });
    }
    // NativeActivity's InputQueue is upstream of View dispatch: Java gestures cannot re-enter it.
    // Rust drains this queue on its UI thread and uses the same touch/effect dispatcher.
    private org.json.JSONArray accessibilityActions = new org.json.JSONArray();
    private synchronized void queueAccessibilityAction(int kind,float x,float y,float endY) {
        org.json.JSONArray item = new org.json.JSONArray();
        item.put(kind); item.put(Float.valueOf(x)); item.put(Float.valueOf(y)); item.put(Float.valueOf(endY));
        accessibilityActions.put(item);
    }
    public synchronized String takeAccessibilityActions() {
        // Rust chiede più volte al secondo: senza azioni non si crea niente.
        if (accessibilityActions.length()==0) return null;
        String result=accessibilityActions.toString();
        accessibilityActions=new org.json.JSONArray();
        return result;
    }
    private final class AccessibleSurface extends android.view.View {
        private final android.util.SparseArray<org.json.JSONObject> nodes = new android.util.SparseArray<>();
        private final java.util.ArrayList<Integer> order = new java.util.ArrayList<>();
        private int focused = -1, hovered = -1;
        private boolean scrollable;
        private String previous = "";
        AccessibleSurface(android.content.Context context) {
            super(context);
            setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_YES);
            setFocusable(false);
        }
        void update(String json) {
            if (previous.equals(json)) return;
            try {
                org.json.JSONObject data = new org.json.JSONObject(json);
                org.json.JSONArray list = data.getJSONArray("nodes");
                nodes.clear(); order.clear();
                for (int i = 0; i < list.length(); i++) {
                    org.json.JSONObject node = list.getJSONObject(i);
                    int id = node.getInt("id");
                    nodes.put(id, node); order.add(id);
                }
                scrollable = data.optBoolean("scroll");
                previous = json;
                if (nodes.get(focused) == null) focused = -1;
                sendAccessibilityEvent(android.view.accessibility.AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED);
            } catch (org.json.JSONException e) { android.util.Log.w("Cedro", "Snapshot accessibilità non valido"); }
        }
        private android.graphics.Rect nativeBounds(org.json.JSONObject node) {
            org.json.JSONArray r = node.optJSONArray("bounds");
            return r == null ? new android.graphics.Rect() : new android.graphics.Rect((int)r.optDouble(0),(int)r.optDouble(1),(int)r.optDouble(2),(int)r.optDouble(3));
        }
        private AccessibilityCoordinates coordinates() {
            int[] window = new int[2], screen = new int[2];
            getLocationInWindow(window);
            getLocationOnScreen(screen);
            return new AccessibilityCoordinates(window[0],window[1],screen[0],screen[1]);
        }
        private android.graphics.Rect localBounds(org.json.JSONObject node) {
            android.graphics.Rect r = nativeBounds(node);
            AccessibilityCoordinates c = coordinates();
            int[] local=c.localBounds(r.left,r.top,r.right,r.bottom);
            return new android.graphics.Rect(local[0],local[1],local[2],local[3]);
        }
        private void event(int id, int type) {
            android.view.accessibility.AccessibilityEvent e = android.view.accessibility.AccessibilityEvent.obtain(type);
            e.setPackageName(getPackageName()); e.setClassName("android.view.View"); e.setSource(this, id);
            org.json.JSONObject n = nodes.get(id);
            if (n != null) e.getText().add(n.optString("text"));
            getParent().requestSendAccessibilityEvent(this, e);
        }
        @Override public boolean onTouchEvent(android.view.MotionEvent e) { return false; }
        @Override public boolean onHoverEvent(android.view.MotionEvent e) {
            android.view.accessibility.AccessibilityManager manager = (android.view.accessibility.AccessibilityManager)getSystemService(ACCESSIBILITY_SERVICE);
            if (!manager.isTouchExplorationEnabled()) return false;
            int id = -1;
            if (e.getAction() != android.view.MotionEvent.ACTION_HOVER_EXIT) {
                for (int k = order.size()-1; k >= 0; k--) {
                    int candidate = order.get(k);
                    if (localBounds(nodes.get(candidate)).contains((int)e.getX(),(int)e.getY())) { id = candidate; break; }
                }
            }
            if (id != hovered) {
                if (hovered != -1) event(hovered,android.view.accessibility.AccessibilityEvent.TYPE_VIEW_HOVER_EXIT);
                hovered = id;
                if (id != -1) event(id,android.view.accessibility.AccessibilityEvent.TYPE_VIEW_HOVER_ENTER);
            }
            return true;
        }
        @Override public android.view.accessibility.AccessibilityNodeProvider getAccessibilityNodeProvider() { return provider; }
        private final android.view.accessibility.AccessibilityNodeProvider provider = new android.view.accessibility.AccessibilityNodeProvider() {
            @Override public android.view.accessibility.AccessibilityNodeInfo createAccessibilityNodeInfo(int id) {
                if (id == HOST_VIEW_ID) {
                    android.view.accessibility.AccessibilityNodeInfo info = android.view.accessibility.AccessibilityNodeInfo.obtain(AccessibleSurface.this);
                    onInitializeAccessibilityNodeInfo(info);
                    for (int child : order) info.addChild(AccessibleSurface.this,child);
                    info.setScrollable(scrollable);
                    if (scrollable) {info.addAction(4096);info.addAction(8192);}
                    return info;
                }
                org.json.JSONObject node = nodes.get(id);
                if (node == null) return null;
                android.view.accessibility.AccessibilityNodeInfo info = android.view.accessibility.AccessibilityNodeInfo.obtain();
                info.setSource(AccessibleSurface.this,id); info.setParent(AccessibleSurface.this);
                info.setPackageName(getPackageName());
                boolean click = node.optBoolean("click");
                info.setClassName(click ? "android.widget.Button" : "android.widget.TextView");
                info.setText(node.optString("text"));info.setEnabled(true);info.setVisibleToUser(true);info.setFocusable(true);
                info.setClickable(click);if (click) info.addAction(16);
                info.setSelected(node.optBoolean("selected"));info.setCheckable(node.optBoolean("checkable"));info.setChecked(node.optBoolean("checked"));
                // Rust bounds already include the system insets, in native window pixels.
                // The overlay can start below/alongside a system bar: subtract its window
                // origin for parent bounds, then add its screen origin exactly once.
                AccessibilityCoordinates c = coordinates();
                android.graphics.Rect rect = nativeBounds(node);
                int[] parent=c.localBounds(rect.left,rect.top,rect.right,rect.bottom);
                int[] screen=c.screenBounds(rect.left,rect.top,rect.right,rect.bottom);
                info.setBoundsInParent(new android.graphics.Rect(parent[0],parent[1],parent[2],parent[3]));
                info.setBoundsInScreen(new android.graphics.Rect(screen[0],screen[1],screen[2],screen[3]));
                info.setAccessibilityFocused(focused == id);info.addAction(focused == id ? 128 : 64);
                if (scrollable) {info.addAction(4096);info.addAction(8192);}
                return info;
            }
            @Override public android.view.accessibility.AccessibilityNodeInfo findFocus(int focus) {
                return focused == -1 ? null : createAccessibilityNodeInfo(focused);
            }
            @Override public boolean performAction(int id,int action,Bundle args) {
                org.json.JSONObject node = nodes.get(id);
                if (action == 64 && node != null) {
                    if (focused != -1) event(focused,65536);
                    focused = id; event(id,32768);return true;
                }
                if (action == 128 && focused == id) { focused=-1;event(id,65536);return true; }
                if (action == 16 && node != null && node.optBoolean("click")) {
                    // Queue clicks stay in native window coordinates, as physical InputQueue events.
                    android.graphics.Rect r=nativeBounds(node);
                    queueAccessibilityAction(0,r.exactCenterX(),r.exactCenterY(),r.exactCenterY());
                    event(id,1);return true;
                }
                if ((action == 4096 || action == 8192) && scrollable) {
                    float x=getWidth()*0.5f, start=getHeight()*(action==4096 ? 0.7f : 0.3f), end=getHeight()*(action==4096 ? 0.3f : 0.7f);
                    AccessibilityCoordinates c = coordinates();
                    queueAccessibilityAction(1,c.toWindowX(x),c.toWindowY(start),c.toWindowY(end));
                    return true;
                }
                return false;
            }
        };
    }

}
