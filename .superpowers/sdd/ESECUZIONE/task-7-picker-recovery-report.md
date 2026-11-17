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
