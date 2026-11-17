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

Da eseguire sul minipc a cura del controller, dalla copia sincronizzata e con il suo ambiente di build già validato:

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

Comandi aggiuntivi **da eseguire dal controller sul minipc**:

```sh
mkdir -p /srv/dev/scratch/cedro-picker-delivery
javac -d /srv/dev/scratch/cedro-picker-delivery android/java/it/cedro/android/ActivityDelivery.java android/tests/java/it/cedro/android/ActivityDeliveryTest.java
java -cp /srv/dev/scratch/cedro-picker-delivery it.cedro.android.ActivityDeliveryTest
```

Implementatore: `git diff --check` exit 0; nessuna esecuzione Rust/Java/adb/remota. Ancora pendenti sul nuovo checkpoint build/test, re-review mirata e retest fisico del controller.
