# Task 7 — recupero selettore dopo ricreazione del processo

Base `e9b6a72`. Correzione circoscritta al KO runtime comunicato dal controller: DocumentsUI restituisce l'originale SAF di circa 33 MiB a una nuova NativeActivity, ma `attesa_scelta` riparte da `None` e la callback rimane senza consumatore. Nessun altro finding o refactoring aggiunto.

## Modifica

`scelta-in-attesa.json` nella directory interna dell'app registra la sorgente prima di `startActivityForResult`, mediante la stessa scrittura atomica e fsync dei journal Android. Per `Originale` registra anche il percorso della firma e la data di verifica (oggi, firma, data personalizzata).

All'avvio il journal viene letto prima del ciclo che consuma `scelti()`. Il file della firma viene protetto dal cleanup; si ripristina il grant se necessario e si riapre il viewer, usando le API asincrone esistenti della verifica. Il contesto pendente evita anche di ripetere l'intento di condivisione iniziale mentre si aspetta il risultato del picker. Non si modificano API del core né si cancellano file privati/grant.

Java distingue `null` (nessuna callback) da array URI vuoto (annullamento o risultato senza URI); quest'ultimo chiude l'attesa senza creare un lavoro d'importazione. Successo consumato, annullamento e fallimento di apertura del picker scrivono una tombstone atomica `null`, impedendo il recupero di un contesto già concluso. Le URI restano consumate una sola volta tramite il metodo Java sincronizzato esistente.

Su richiesta del controller, i due helper Android/test `mime_da_percorso` e `leggi_originale`, e il nuovo modulo picker, sono compilati soltanto su Android o nei test: rimossi così i due nuovi `dead_code` del rendering host senza `allow` generali. Nessun cambio a permessi, temi, traduzioni, dipendenze o workflow.

## Verifiche e checkpoint

Implementatore: sola lettura/modifica locale e `git diff --check`, exit 0. Nessuna compilazione Rust/Java, comando remoto o adb eseguito.

Nuova regressione host produttiva: `picker::tests::picker_context_survives_recreation_and_clears_without_removing_inputs`. Esercita tutte le nove sorgenti del picker, le tre modalità della data, salvataggio/lettura indipendente da disco, identità del file firma, clear idempotente senza replay, conservazione degli input e rifiuto di un contesto Originale incompleto.

Da eseguire sul PC di collaudo a cura del controller, dalla copia sincronizzata e con il suo ambiente di build già validato:

```sh
cargo test --manifest-path android/Cargo.toml --lib picker::tests::picker_context_survives_recreation_and_clears_without_removing_inputs -- --exact
cargo test --manifest-path android/Cargo.toml --lib
cargo test --manifest-path android/Cargo.toml --lib --features youtube
```

Pendenti al checkpoint: suite host, NDK entrambe le configurazioni, clippy senza nuovi avvisi, Java/d8/APK e prova deterministica sul Mi9T con morte del processo mentre DocumentsUI è aperto, ritorno dell'originale SAF ≥32 MiB nel viewer corretto, annullamento e successivo riavvio senza replay. Nessun PASS di queste prove è inferito dal codice.

## Fix round 1 dopo la review circoscritta I1/I2

Letta integralmente `task-7-picker-recovery-review.md`. Base `5580f0f`; il controller comunica su quella base 56 test host riusciti con e senza YouTube e NDK/clippy/Java riusciti. Questi esiti precedono il presente delta e non certificano il nuovo checkpoint.

- **I1:** quando manca un journal valido, Rust fa scartare l'eventuale risultato Java nel ciclo principale (coprendo anche le consegne tardive dopo l'inizializzazione) e immediatamente prima di aprire un nuovo selettore. Il ramo con attesa valida continua invece a consumare il risultato corrispondente senza scartarlo.
- **I2:** `onCreate` registra se il Bundle indica una ricreazione **prima** di `super.onCreate`, che può già avviare Rust. Il metodo JNI iniziale restituisce una disponibilità monouso solo per un avvio nuovo. Una Activity ricreata non importa nuovamente il vecchio `getIntent()`, anche quando il journal picker è ormai `null`. `onNewIntent` ha uno stato monouso distinto e continua a importare nuove condivisioni; anche un nuovo avvio esplicito resta ammesso. Non si usa la presenza della tombstone come divieto permanente degli intenti futuri.

Lo stato produttivo Java `ActivityDelivery` è privo di dipendenze Android e coperto da `ActivityDeliveryTest`: callback orfana iniziale e tardiva, conservazione della callback con contesto valido, consumo singolo, cancel/success → seconda ricreazione senza replay, nuovo `onNewIntent` e nuovo avvio esplicito dopo la tombstone.

Comandi aggiuntivi **da eseguire dal controller sul PC di collaudo**:

```sh
mkdir -p /srv/dev/scratch/cedro-picker-delivery
javac -d /srv/dev/scratch/cedro-picker-delivery android/java/it/cedro/android/ActivityDelivery.java android/tests/java/it/cedro/android/ActivityDeliveryTest.java
java -cp /srv/dev/scratch/cedro-picker-delivery it.cedro.android.ActivityDeliveryTest
```

Implementatore: `git diff --check` exit 0; nessuna esecuzione Rust/Java/adb/remota. Ancora pendenti sul nuovo checkpoint build/test, re-review mirata e retest fisico del controller.

## Controller: covering tests reali fd3f38c

NDKYT/noYT exit0 (20.67s/4.18s). Androidhostlibcon/senzaYT exit0:
host-test: test result: ok. 56 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.82s
host-test-youtube: test result: ok. 56 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.91s
Comandi come precedente runner finale: cargo --config profile.test.package.fdk-aac-sys.opt-level=3 test -j2 -pcedro-android --lib [--features youtube], TESTopt1/debug0/LTOoff/codegen16/incrementaltrue,targetscratchcache,RUST_TEST_THREADS2. ClippyreleasealltargetsYTno-deps exit0, baselinewarningrestano.
JavaSDKjavac -d /srv/dev/scratch/cedro-final-geometry AccessibilityCoordinates.java+Test.java e ActivityDelivery.java+Test.java exit0. java -cp ... it.cedro.android.AccessibilityCoordinatesTest: PASSexit0; ActivityDeliveryTest: PASSexit0. Testorphanvalidpending/secondrestore/newIntentprodhelper confermato. Logrunner/TSV e filecompleti in ~/Documenti/cedro-android-anteprima/verifiche/fd3f38c.
APKnativebuild inprogress, phonevalidationnonancorafatta, nessunPASSfisicoinferito.

## Fix round 2 — identità della richiesta, solo Java

Letta integralmente `task-7-picker-recovery-fix1-review.md`: I2 resta chiuso, questo delta affronta soltanto I1 residuo. I sorgenti Rust e quindi il binario nativo `fd3f38c` restano identici.

`startActivityForResult(4242)` viene intercettato da CedroActivity e associato a un request code gestito distinto. Il contatore viene salvato con `SharedPreferences.commit()` prima dell'avvio; la callback e l'armamento sono serializzati sullo stesso lock Activity. Armare una richiesta azzera il risultato precedente; `onActivityResult` accetta esclusivamente il codice atteso e lo consuma una sola volta. Una callback precedente arrivata dopo l'armamento viene rifiutata senza chiudere la richiesta nuova. Un fallimento dell'avvio disarma il codice.

Il codice atteso viene salvato nel Bundle. Prima di `super.onCreate`, il ripristino richiede anche la corrispondenza con `request_code` nel journal Rust corrente. Java aggiunge questo campo con `AtomicFile` prima di armare/avviare; serde Rust ignora il campo aggiuntivo senza cambiare il formato già consumato. Questo lega il codice alla sorgente del journal e chiude anche la fessura «Rust ha scritto la nuova sorgente, Bundle conserva ancora la vecchia richiesta»: un campo assente/diverso rifiuta l'identità vecchia. La lettura AtomicFile precedente all'avvio Rust recupera anche un binding interrotto.

Il contatore usa l'intervallo app a 16 bit `0x4000..0xffff`, senza wrap: dopo 49.152 aperture esterne l'avvio viene rifiutato esplicitamente, anziché riutilizzare un codice e poter accettare una risposta arbitrariamente vecchia. Anche le aperture fallite consumano il codice riservato. Nessuna nuova permission o dipendenza.

`ActivityDeliveryTest` ora esercita la callback vecchia dopo armamento/durante il nuovo avvio (sequenza della review), risultato vecchio già in buffer, callback duplicata, fallimento lancio, rifiuto senza journal valido, ripristino del codice corrente, mismatch Bundle/journal, cancel/success e nuovi intenti, contatore attraverso ricreazioni ed esaurimento senza wrap. I comandi Java del round 1 restano applicabili. Implementatore: `git diff --check` exit 0; nessuna compilazione né comando Rust/adb/remoto. Compilazione Java/d8, test, repack APK e review circoscritta I1 pendenti al controller.
