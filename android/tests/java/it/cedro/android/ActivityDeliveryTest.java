package it.cedro.android;

/** Host checks of the same delivery state used by CedroActivity's JNI methods. */
public final class ActivityDeliveryTest {
    private static void check(boolean value,String message) { if (!value) throw new AssertionError(message); }
    public static void main(String[] args) {
        ActivityDelivery<String[]> launch=new ActivityDelivery<>();
        launch.created(false,-1);
        check(launch.takeInitial(),"fresh ACTION_VIEW/SEND lost");
        check(!launch.takeInitial(),"initial intent replay");
        int old=ActivityDelivery.nextRequest(ActivityDelivery.FIRST_REQUEST-1);
        int current=ActivityDelivery.nextRequest(old);
        String[] previous={"old-original"}, result={"current-original"};
        check(ActivityDelivery.restoredRequest(old,current)==-1,"old Bundle bound to new journal source");
        check(ActivityDelivery.restoredRequest(old,-1)==-1,"old Bundle bound to unarmed journal");
        check(ActivityDelivery.restoredRequest(current,current)==current,"matching Bundle/journal binding lost");
        check(!launch.picker(old,previous),"orphan before request accepted");
        launch.arm(old);check(launch.picker(old,previous),"old matching request lost");
        // Old delivery is already buffered; arm itself clears it, independently of drain timing.
        launch.arm(current);
        check(launch.takePicker()==null,"buffered old result retained during launch");
        // Exact reviewer interleaving: old callback arrives AFTER new arm / during super launch.
        check(!launch.picker(old,previous),"old callback assigned to the new source");
        check(launch.expectedRequest()==current,"old callback closed the current request");
        check(launch.takePicker()==null,"old URI exposed to Rust");
        launch.discardOrphan(true);
        check(launch.picker(current,result),"valid pending callback discarded");
        check(!launch.picker(current,previous),"duplicate replaced the accepted result");
        check(launch.takePicker()==result,"matching result lost");
        check(launch.takePicker()==null,"result consumed twice");
        launch.arm(ActivityDelivery.nextRequest(current));launch.discardOrphan(false);
        check(!launch.picker(current+1,result),"invalid-journal late callback accepted");
        launch.arm(current+2);launch.failed(current+2);
        check(!launch.picker(current+2,result),"failed launch retained expectation");
        for (boolean cancel:new boolean[]{true,false}) {
            int savedRequest=ActivityDelivery.nextRequest(current+2);
            ActivityDelivery<String[]> before=new ActivityDelivery<>();before.created(false,-1);before.arm(savedRequest);
            int savedBundleRequest=before.expectedRequest();
            // A new object models restoration BEFORE NativeActivity can start the Rust consumer.
            ActivityDelivery<String[]> recovered=new ActivityDelivery<>();recovered.created(true,ActivityDelivery.restoredRequest(savedBundleRequest,savedRequest));
            check(!recovered.takeInitial(),"initial intent replayed with pending picker");
            check(!recovered.picker(old,previous),"stale result accepted after recreation");
            check(recovered.picker(savedRequest,cancel?new String[0]:result),"restored expected callback rejected");
            String[] value=recovered.takePicker();
            check(value!=null && value.length==(cancel?0:1),"cancel/success delivery lost");
            ActivityDelivery<String[]> second=new ActivityDelivery<>();second.created(true,recovered.expectedRequest());
            check(!second.takeInitial(),"cancel/success then recreation replayed initial intent");
            check(!second.picker(savedRequest,result),"completed callback replayed after recreation");
            second.newIntent();check(second.takeNew(),"genuine onNewIntent dropped after tombstone");
            check(!second.takeNew(),"new intent replayed");
            second.created(false,-1);check(second.takeInitial(),"new explicit launch dropped after tombstone");
            int next=ActivityDelivery.nextRequest(savedRequest);second.arm(next);
            check(next!=savedRequest && !second.picker(savedRequest,previous),"request code reused across process recreation");
        }
        boolean exhausted=false;
        try {ActivityDelivery.nextRequest(ActivityDelivery.LAST_REQUEST);} catch (IllegalStateException e) {exhausted=true;}
        check(exhausted,"16-bit request code wrapped and enabled stale replay");
        System.out.println("ActivityDeliveryTest: PASS");
    }
}
