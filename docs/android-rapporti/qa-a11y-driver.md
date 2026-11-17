# Driver QA accessibilità senza installare un APK

2026-11-17. Ricerca iniziale in sola lettura, seguita da autorizzazione esplicita del controller a creare e compilare soltanto un helper QA fuori repository. Nessun sorgente di produzione modificato, nessun commit, nessun test/action ADB eseguito dall'agente. La sessione ADB operativa resta al controller.

## Esito concreto

**Route disponibile e helper DEX compilato:** runner legacy pubblico `adb shell uiautomator runtest` + `UiAutomatorTestCase` + sottoclasse di `UiObject` che espone il metodo **protected documentato** `findAccessibilityNodeInfo(long)`. L'helper ottiene veri `AccessibilityNodeInfo` e usa esclusivamente `node.performAction(ACTION_CLICK / ACTION_SCROLL_FORWARD)` per le azioni UI sotto test.

Questa route usa il runner shell UI Automator già presente e non richiede una nuova package APK, firma, nuova installazione o attivazione persistente di un servizio accessibilità. Non è un tentativo di aggirare l'installazione rifiutata dall'OS: è il meccanismo JAR documentato del runner disponibile. Il tentativo APK separato riportato dal controller (`INSTALL_FAILED_USER_RESTRICTED`) non è stato ripetuto né diagnosticato come policy specifica.

**Verifica runtime delle action: ancora da eseguire dal controller.** La compilazione e la presenza del runner non dimostrano che CLICK/SCROLL funzionino già nel bridge Cedro. L'eventuale oracle RED sulla build attuale può essere eseguito dal controller con lo stesso artifact.

## Inventario verificato

Telefono Mi 9T `2f6d6ba7`, soli comandi di inventario/help:

- `uiautomator help` elenca `runtest`, opzione `-c Class#method`, JAR da `/data/local/tmp`, `-s` output semplice e runner alternativo opzionale.
- `/system/framework/uiautomator.jar`: presente, 366758 byte.
- `/system/framework/android.test.runner.jar`: presente, 106974 byte.
- `/system/framework/android.test.base.jar`: presente, 33134 byte.

Mini-PC, SDK effettivo:

```text
/home/decok/android-sdk/platforms/android-11/android.jar
/home/decok/android-sdk/platforms/android-11/uiautomator.jar
/home/decok/android-sdk/platforms/android-11/optional/android.test.base.jar
/home/decok/android-sdk/platforms/android-11/optional/android.test.runner.jar
/home/decok/android-sdk/jdk/bin/javac
/home/decok/android-sdk/jdk/bin/jar
/home/decok/android-sdk/build-tools/android-14/d8
```

`javap` sul JAR SDK installato conferma direttamente:

```text
public UiObject(UiSelector);
protected AccessibilityNodeInfo findAccessibilityNodeInfo(long);
public class UiAutomatorTestCase extends junit.framework.TestCase
```

La classe `junit/framework/TestCase.class` è presente nell'optional `android.test.base.jar`. La build usa **tre JAR SDK**: android.jar come boot/lib, uiautomator.jar e android.test.base.jar come classpath. Il framework runner presente sul telefono fornisce l'esecuzione; non viene incluso nel DEX dell'helper.

Fonti primarie per il percorso legacy: [documentazione AOSP del test JAR](https://android.googlesource.com/platform/frameworks/base/+/32cfe2bbc71091a1ab282d5c7e4df7db5f6051e6/docs/html/tools/testing/testing_ui.jd), [UiObject API legacy: metodo protected](https://android.googlesource.com/platform/frameworks/base/+/84bf8073a8a80cf464eba1dada0eb7585f9943d5/docs/html/tools/help/uiautomator/UiObject.jd), [implementazione AOSP UiObject](https://android.googlesource.com/platform/frameworks/uiautomator/+/1626fb3acf5b4502b5b24429458da11c3a2e72ff/src/com/android/uiautomator/core/UiObject.java).

## Helper prodotto

Locale:

- Sorgente: `/home/accorsiluca/prove-cedro-android/bridge-jar/src/it/cedro/android/probe/LegacyBridgeProbe.java`
- Build ripetibile: `/home/accorsiluca/prove-cedro-android/bridge-jar/build.sh`
- Artifact eseguibile: `/home/accorsiluca/prove-cedro-android/bridge-jar/out/cedro-bridge-probe.jar`
- Classi JVM intermedie: `/home/accorsiluca/prove-cedro-android/bridge-jar/out/classes.jar`

Mini-PC:

- Directory isolata: `/srv/dev/scratch/cedro-a11y-driver-20261117/`
- Sorgente e build.sh nello stesso layout.
- Artifact: `/srv/dev/scratch/cedro-a11y-driver-20261117/out/cedro-bridge-probe.jar`

Build Java + D8 completata con exit0; unico avviso javac: uso di API legacy deprecata. Prima esecuzione D8 mancava java nel PATH: corretto il solo build.sh QA impostando JAVA_HOME **e** PATH del JDK SDK, poi build completata. Nessuna installazione di dipendenze.

Artifact locale 4011 byte; archivio contiene soltanto `classes.dex`, header `dex\n038\0`, min-api26.

```text
SHA256 759921b57a42ee04879cc009ed7cb28345cb356d117f5651c3194168b77b75a7
```

Lo stesso hash è stato prodotto sul mini-PC prima del trasferimento e verificato sul file locale.

## Cosa prova il test quando eseguito

Classe/metodo: `it.cedro.android.probe.LegacyBridgeProbe#testBridgeActions`.

1. Cerca un nodo del package Cedro con la API legacy protetta, risale al root e verifica che appartenga a Cedro. Controlla catena genitori e BFS figli senza duplicazioni/cicli usando **AccessibilityNodeInfo.equals/hashCode** tramite HashSet. Limite di guardia2048 nodi; almeno10 per escludere un semplice contenitore senza controlli virtuali.
2. Cerca un solo pulsante **visibile e clickable** con text o contentDescription esattamente `Lavori`. Esegue `AccessibilityNodeInfo.performAction(ACTION_CLICK)`, verifica `accepted=true` e poi attende contenuto realmente visibile `Svuota`. **Svuota viene soltanto cercato: non viene mai cliccato.** Rilegge il pulsante e registra `isSelected()` come diagnostica, senza assumerne il valore.
3. Ripete il click accessibile su `Impostazioni`, richiedendo il contenuto reale visibile `Tinta di Cedro`, e ricontrolla unicità.
4. Cerca un nodo visibile scrollable che esponga `ACTION_SCROLL_FORWARD`, chiama quella action e confronta la firma dell'albero prima/dopo. La firma contiene testi, contentDescription, bounds e visibilità; non usa nuovi id/hash come scorciatoia per dichiarare un cambiamento di contenuto. Richiede **accepted=true e firma cambiata**, poi ricontrolla unicità.

I log iniziano con `CEDRO_A11Y`; l'ultima riga su successo è `PASS routes=Lavori,Impostazioni scroll=true`. Il runner JUnit mostra gli assert/stack trace in caso di fallimento.

Non usa `UiObject.click()`, `UiScrollable.scrollForward()`, coordinate, touch inject, DPAD, reflection, API nascoste o modifica impostazioni. La distinzione è sostanziale: `UiObject.click()` nella sorgente AOSP prende il centro dei bounds e passa a `InteractionController.clickAndSync`, che non verifica lo stesso percorso `performAction` del bridge. [AOSP UiObject.click](https://android.googlesource.com/platform/frameworks/uiautomator/+/1626fb3acf5b4502b5b24429458da11c3a2e72ff/src/com/android/uiautomator/core/UiObject.java#387), [API AccessibilityNodeInfo.performAction](https://developer.android.com/reference/android/view/accessibility/AccessibilityNodeInfo#performAction(int)).

## Comandi pronti per il controller, non eseguiti da questo agente

Il controller porta prima Cedro in primo piano e riserva il dispositivo al runner, senza altre sessioni UiAutomation concorrenti. Poi:

```bash
adb -s 2f6d6ba7 push \
  /home/accorsiluca/prove-cedro-android/bridge-jar/out/cedro-bridge-probe.jar \
  /data/local/tmp/cedro-bridge-probe.jar

adb -s 2f6d6ba7 shell uiautomator runtest \
  /data/local/tmp/cedro-bridge-probe.jar \
  -c it.cedro.android.probe.LegacyBridgeProbe#testBridgeActions -s
```

Per ricompilare sul mini-PC, senza Rust/build di Cedro:

```bash
ssh dev 'bash /srv/dev/scratch/cedro-a11y-driver-20261117/build.sh'
```

L'helper non avvia l'app e non recupera automaticamente da un'altra finestra: se Cedro non è in primo piano, il root manca o l'UI iniziale non contiene la navigazione attesa, fallisce chiaramente. Il test termina su Impostazioni dopo lo scroll; lo scroll richiede spazio effettivo per scorrere. Non interpreta un `accepted=true` isolato come successo.

## Alternativa TalkBack già presente: capacità e limite

Inventario readonly: `com.google.android.marvin.talkback` è installato, aggiornamento17.1 e versione sistema9.1; `accessibility_enabled=0`, `enabled_accessibility_services` vuoto al momento della lettura. Non è stato attivato o configurato.

TalkBack può quindi essere provato tramite la normale attivazione utente nelle impostazioni, senza installare un'altra package; sarebbe però una modifica dello stato del servizio e non è stata autorizzata/eseguita in questo subtask. La documentazione ufficiale prevede navigazione tastiera e un comando di click con **TalkBack key + Enter** nella keymap predefinita; altre keymap usano, per esempio, modifier+Space. **DPAD/Enter semplici non costituiscono da soli prova di AccessibilityNodeInfo.performAction**: possono attivare il normale percorso tastiera dell'app. Versione/keymap effettiva vanno osservate prima di trattare le scorciatoie come equivalenti al driver.

[Attivazione TalkBack](https://support.google.com/accessibility/android/answer/6007100), [scorciatoie TalkBack ufficiali](https://support.google.com/accessibility/android/answer/6110948?hl=en).

## Handoff

Artifact, hash, API verificate e comandi forniti al controller. L'helper è pronto per l'oracle RED/GREEN del controller; nessun esito runtime CLICK/SCROLL è stato dichiarato senza esecuzione. Nessuna interferenza con l'ADB operativo del controller oltre alle letture inventory/help esplicitamente consentite.
