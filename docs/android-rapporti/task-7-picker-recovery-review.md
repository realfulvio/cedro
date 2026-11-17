### Spec Compliance

- ❌ Issues found: il recupero con journal valido è implementato, ma restano due percorsi che possono rigiocare URI precedenti: callback orfana dopo journal mancante/corrotto e intento iniziale dopo chiusura del journal. Dettagli I1–I2 sotto.
- ✅ Il contesto viene salvato prima dell'apertura esterna (`android/src/glue.rs:2048–2051`); `Originale` conserva path e modalità/data (`android/src/picker.rs:8–30`). La lettura precede il consumo della callback, ripristina il grant prima del viewer e protegge il path dal cleanup (`android/src/glue.rs:1740–1758`).
- ✅ L'array vuoto chiude l'attesa prima di creare un lavoro (`android/src/glue.rs:411–412,2104–2108`); Java lo produce per annullamento e risultato senza URI (`android/java/it/cedro/android/CedroActivity.java:110–115`). Nessuna nuova dipendenza, permission o autostart nel diff, nessuna nuova cancellazione di input personali.
- ⚠️ Cannot verify physical: build host/NDK/Java e prove Mi9T sono del controller e non erano incluse nel report disponibile. Restano da associare all'head `5580f0f` gli esiti di morte deterministica del processo durante DocumentsUI, ritorno originale SAF ≥32 MiB, validazione nel viewer e cancel/riavvio senza replay (`.superpowers/sdd/ESECUZIONE/task-7-picker-recovery-report.md:17–29`).

### Strengths

- Il journal usa la primitiva atomica produttiva, con fsync prima e dopo rename, e la tombstone non cancella gli input (`android/src/picker.rs:32–45`; `android/src/storage.rs:121–136`). Il test percorre tutte le sorgenti selezionabili e tutte le date, leggendo realmente dal disco (`android/src/picker.rs:52–76`).
- Il ripristino dell'originale rispetta le dipendenze: riapertura SAF, creazione sincrona del viewer, impostazione della data, poi consumo della callback (`android/src/glue.rs:1747–1752,2104–2115`). La riverifica conserva la data e le generazioni impediscono che un risultato della verifica iniziale sostituisca quello con l'originale (`android/src/ui/firma.rs:92–101`; `src/ui/firma.rs:283–288,324–327`).
- La protezione del path agisce sul cleanup già esistente, che rispetta i riferimenti protetti prima di eliminare cache vecchie (`android/src/glue.rs:1757–1758`; `android/src/glue/saf.rs:131–135`).

### Issues

#### Critical (Must Fix)

- Nessuno.

#### Important (Should Fix)

- **I1 — Una callback orfana può essere assegnata al picker successivo.** `android/src/glue.rs:1741–1745,2051,2104–2105`. Se il processo ricreato riceve un risultato ma il journal manca o è corrotto, `attesa_scelta` resta `None` e nessuno legge/scarta il risultato Java. Il campo Java conserva l'array fino alla chiamata sincronizzata `scelti()` (`android/java/it/cedro/android/CedroActivity.java:140–143`). Quando l'utente avvia un nuovo picker, `Some(s)` permette di consumare subito quel vecchio array con la nuova sorgente; viene chiuso anche il nuovo journal, mentre il risultato reale successivo resta senza consumatore. È una lacuna nella gestione espressamente richiesta del journal mancante/corrotto e nel vincolo di non rigiocare vecchie URI. Scartare le callback prive di contesto valido, anche se consegnate dopo l'inizializzazione, prima di associare una nuova richiesta; coprire il caso con una regressione mirata.
- **I2 — La tombstone non impedisce il replay dell'intento iniziale dopo cancel/success.** `android/src/glue.rs:1787,2106–2107`; `android/src/picker.rs:36–45`. Il blocco dell'importazione iniziale vale solo con `pending=Some`. Dopo annullamento o consumo, il journal diventa `null`; a una successiva morte/ricreazione del processo `pending=None` riattiva `importa_condivisione`, che importa nuovamente il `getIntent()` originale se ACTION_VIEW/SEND/SEND_MULTIPLE (`android/src/glue.rs:115–125`). Per esempio, aprire una firma via ACTION_VIEW, scegliere l'originale, annullare e ricreare nuovamente il processo rigioca l'URI iniziale. Il codice non marca quell'intento come consumato né lo sostituisce. Conservare/distinguere l'identità dell'intento già gestito oppure consumarlo esplicitamente, lasciando passare intenti davvero nuovi; includere la sequenza cancel → seconda ricreazione nel test. Il test attuale verifica soltanto che la tombstone non restituisca un `Attesa`, non che il chiamante eviti il replay dell'intento.

#### Minor (Nice to Have)

- Nessuno.

### Assessment

**Task quality:** Needs fixes — 0 Critical, 2 Important, 0 Minor.

**Reasoning:** La sequenza principale con journal valido recupera correttamente il viewer e la data prima dell'originale. I percorsi senza contesto valido e successivi alla sua chiusura non garantiscono ancora l'assenza di replay richiesta dal task.

**Controlli svolti:** diff letto una volta; nessun test, compilazione, comando Rust, adb, accesso remoto o modifica produttiva. Letture aggiuntive mirate ai rischi richiesti: `storage::atomico` per atomicità; `saf::ripristina/pulisci/ripristina_tutti` per grant e cleanup; API viewer/reverify/pump per ordine e generazioni; `importa_condivisione`, `onNewIntent` e `scelti` per replay e consumo singolo. Il corpo di `importa_scelti`, troncato dal hunk, è stato letto per verificare la consegna asincrona dell'originale. Nessuna review generale del ramo ripetuta.
