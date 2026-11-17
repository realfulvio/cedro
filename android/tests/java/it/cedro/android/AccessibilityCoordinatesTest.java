package it.cedro.android;

/** Host regression of the production transform; no Android runtime or test dependency. */
public final class AccessibilityCoordinatesTest {
    private static void equal(int[] actual, int... expected) {
        if (!java.util.Arrays.equals(actual,expected)) {
            throw new AssertionError(java.util.Arrays.toString(actual)+" != "+java.util.Arrays.toString(expected));
        }
    }
    public static void main(String[] args) {
        // Recorded Mi9T window bounds: physical screen must not receive the bar twice.
        AccessibilityCoordinates portrait=new AccessibilityCoordinates(0,95,0,95);
        equal(portrait.localBounds(551,1403,1036,1582),551,1308,1036,1487);
        equal(portrait.screenBounds(551,1403,1036,1582),551,1403,1036,1582);
        // Landscape left inset and a moved window (e.g. split screen), no fixed bar size.
        for (int[] origin : new int[][] {{0,0,0,0},{83,0,83,0},{17,61,217,361},{0,113,0,113}}) {
            AccessibilityCoordinates c=new AccessibilityCoordinates(origin[0],origin[1],origin[2],origin[3]);
            int[] parent=c.localBounds(300,500,480,680);
            int[] screen=c.screenBounds(300,500,480,680);
            equal(screen,parent[0]+origin[2],parent[1]+origin[3],parent[2]+origin[2],parent[3]+origin[3]);
            // A physical node-center hover maps into the same local hit rectangle.
            float hoverX=(screen[0]+screen[2])*0.5f-origin[2];
            float hoverY=(screen[1]+screen[3])*0.5f-origin[3];
            if (!(hoverX>=parent[0] && hoverX<parent[2] && hoverY>=parent[1] && hoverY<parent[3])) throw new AssertionError("hover");
            // Queue receives native coordinates; only local scroll gestures need conversion.
            if (c.toWindowX(hoverX)!=390 || c.toWindowY(hoverY)!=590) throw new AssertionError("native action");
            if (c.toWindowY(700)-c.toWindowY(300)!=400) throw new AssertionError("scroll distance");
        }
        System.out.println("AccessibilityCoordinatesTest: PASS");
    }
}
