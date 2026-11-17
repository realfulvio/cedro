package it.cedro.android;

/** Native buffer/window pixels and overlay-local pixels have different origins.
 * Locations come from the view on every use, so rotation, insets and window moves
 * do not leave a cached status-bar offset. Rust's snapshot stays in window pixels.
 */
final class AccessibilityCoordinates {
    final int windowX, windowY, screenX, screenY;
    AccessibilityCoordinates(int windowX, int windowY, int screenX, int screenY) {
        this.windowX=windowX; this.windowY=windowY;
        this.screenX=screenX; this.screenY=screenY;
    }
    int[] localBounds(int left, int top, int right, int bottom) {
        return new int[] {left-windowX,top-windowY,right-windowX,bottom-windowY};
    }
    int[] screenBounds(int left, int top, int right, int bottom) {
        int[] r=localBounds(left,top,right,bottom);
        r[0]+=screenX; r[1]+=screenY; r[2]+=screenX; r[3]+=screenY;
        return r;
    }
    float toWindowX(float localX) { return localX+windowX; }
    float toWindowY(float localY) { return localY+windowY; }
}
