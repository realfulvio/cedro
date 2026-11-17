package it.cedro.android;

/** Host checks of the same delivery state used by CedroActivity's JNI methods. */
public final class ActivityDeliveryTest {
    private static void check(boolean value,String message) { if (!value) throw new AssertionError(message); }
    public static void main(String[] args) {
        ActivityDelivery<String[]> launch=new ActivityDelivery<>();
        launch.created(false);
        check(launch.takeInitial(),"fresh ACTION_VIEW/SEND lost");
        check(!launch.takeInitial(),"initial intent replay");
        launch.picker(new String[]{"orphan-before-startup"});
        launch.discardOrphan(false);
        check(launch.takePicker()==null,"missing/corrupt journal callback replayed");
        // A late callback after startup is drained again before the next picker.
        launch.picker(new String[]{"late-orphan"});
        launch.discardOrphan(false);
        check(launch.takePicker()==null,"old URI assigned to next request");
        String[] current={"current-original"};
        launch.picker(current);launch.discardOrphan(true);
        check(launch.takePicker()==current,"valid pending callback discarded");
        check(launch.takePicker()==null,"result consumed twice");
        for (boolean cancel:new boolean[]{true,false}) {
            ActivityDelivery<String[]> recovered=new ActivityDelivery<>();
            recovered.created(true);
            check(!recovered.takeInitial(),"initial intent replayed with pending picker");
            recovered.picker(cancel?new String[0]:current);
            String[] result=recovered.takePicker();
            check(result!=null && result.length==(cancel?0:1),"cancel/success delivery lost");
            // Journal was cleared; the next recreation must still reject the old initial intent.
            ActivityDelivery<String[]> second=new ActivityDelivery<>();
            second.created(true);
            check(!second.takeInitial(),"cancel/success then recreation replayed initial intent");
            second.newIntent();
            check(second.takeNew(),"genuine onNewIntent dropped after tombstone");
            check(!second.takeNew(),"new intent replayed");
            second.created(false);
            check(second.takeInitial(),"new explicit launch dropped after tombstone");
        }
        System.out.println("ActivityDeliveryTest: PASS");
    }
}
