package it.cedro.android;

/** One-shot deliveries shared by the Activity and the native thread. No URI is persisted here. */
final class ActivityDelivery<T> {
    static final int FIRST_REQUEST=0x4000, LAST_REQUEST=0xffff;
    private boolean initial, fresh;
    private T picker;
    private int expected=-1;
    static boolean managed(int code) { return code>=FIRST_REQUEST && code<=LAST_REQUEST; }
    static int restoredRequest(int bundleRequest,int journalRequest) {
        return bundleRequest==journalRequest && managed(journalRequest)?journalRequest:-1;
    }
    static int nextRequest(int last) {
        if (last>=LAST_REQUEST) throw new IllegalStateException("Picker request codes exhausted");
        return Math.max(FIRST_REQUEST,last+1);
    }
    synchronized void created(boolean restored,int request) {
        initial=!restored;
        expected=restored && managed(request)?request:-1;
    }
    synchronized boolean takeInitial() { boolean value=initial;initial=false;return value; }
    synchronized void newIntent() { fresh=true; }
    synchronized boolean takeNew() { boolean value=fresh;fresh=false;return value; }
    synchronized void arm(int request) {
        if (!managed(request)) throw new IllegalArgumentException("Invalid picker request");
        picker=null;expected=request;
    }
    synchronized int expectedRequest() { return expected; }
    synchronized boolean expects(int request) { return expected==request && managed(request); }
    synchronized boolean picker(int request,T value) {
        if (!expects(request)) return false;
        picker=value;expected=-1;return true;
    }
    synchronized void failed(int request) { if (expected==request) {expected=-1;picker=null;} }
    synchronized T takePicker() { T value=picker;picker=null;return value; }
    synchronized void discardOrphan(boolean pending) { if (!pending) {picker=null;expected=-1;} }
}
