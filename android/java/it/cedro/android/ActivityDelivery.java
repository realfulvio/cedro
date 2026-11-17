package it.cedro.android;

/** One-shot deliveries shared by the Activity and the native thread. No URI is persisted here. */
final class ActivityDelivery<T> {
    private boolean initial, fresh;
    private T picker;
    synchronized void created(boolean restored) { initial=!restored; }
    synchronized boolean takeInitial() { boolean value=initial;initial=false;return value; }
    synchronized void newIntent() { fresh=true; }
    synchronized boolean takeNew() { boolean value=fresh;fresh=false;return value; }
    synchronized void picker(T value) { picker=value; }
    synchronized T takePicker() { T value=picker;picker=null;return value; }
    synchronized void discardOrphan(boolean pending) { if (!pending) picker=null; }
}
