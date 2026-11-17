### Spec Compliance

- ❌ I1 corretto parzialmente: le callback orfane già presenti e quelle arrivate durante l'attesa senza richiesta vengono drenate; rimane la finestra durante l'avvio della richiesta successiva descritta sotto.
- ✅ I2 risolto nel perimetro richiesto: la ricreazione viene registrata prima di `super.onCreate`, la consegna iniziale è monouso e distinta da `onNewIntent` (`android/java/it/cedro/android/CedroActivity.java:72–75,96–110`; `android/java/it/cedro/android/ActivityDelivery.java:7–10`). Rust consulta questo stato anche quando il journal è tombstone, evitando il vecchio intento sulla seconda ricreazione (`android/src/glue.rs:1794–1797`). Il test copre cancel/success, seconda ricreazione, nuovo intento e nuovo avvio (`android/tests/java/it/cedro/android/ActivityDeliveryTest.java:22–36`).
- ⚠️ Cannot verify physical: morte del processo mentre DocumentsUI è aperto, ritorno dell'originale SAF ≥32 MiB e comportamento del Bundle sul Mi9T restano al collaudo del controller. Nessun PASS fisico dedotto dai test host.

### Strengths

- Il ramo con journal valido non drena la sua callback; la consegna è consumata una volta sola e l'array vuoto rimane distinto dall'assenza di callback (`android/src/glue.rs:2115–2126`; `android/java/it/cedro/android/ActivityDelivery.java:11–13`; `android/tests/java/it/cedro/android/ActivityDeliveryTest.java:18–28`).
- La sincronizzazione della consegna `newIntent` elimina la precedente coppia read/reset non atomica del flag; il nuovo helper è piccolo, privo di dipendenze Android e usato dal codice produttivo (`android/java/it/cedro/android/ActivityDelivery.java:4–13`).

### Issues

#### Critical (Must Fix)

- Nessuno.

#### Important (Should Fix)

- **I1 residuo — Callback orfana durante l'avvio del nuovo picker.** `android/src/glue.rs:2058–2062`; `android/java/it/cedro/android/ActivityDelivery.java:11–13`. Il drain avviene prima di salvare il journal e prima di `scegli`; non è atomico con `onActivityResult`. La sequenza ancora ammessa è: drain dell'orfano → arriva la callback vecchia e `picker(value)` la memorizza → avvio del nuovo picker → `attesa_scelta=Some(s)` → `importa_scelti` consuma quell'array come risultato nuovo. Il ramo di pulizia del ciclo non interviene più perché ora esiste un'attesa. `con_attivita` esegue JNI sul thread Rust corrente (`android/src/glue.rs:311–315`), quindi il lock delle singole operazioni Java non serializza l'intera transizione con la callback. Il codice usa ancora il medesimo request code 4242 per tutte le richieste (`android/src/glue.rs:397`), senza identità che consenta di respingere una consegna precedente arrivata dopo l'avvio nuovo. Identificare la richiesta corrente anche nella callback e nel contesto persistente, oppure adottare un protocollo equivalente che escluda questo interleaving. Aggiungere il caso vecchia callback dopo l'ultimo drain/durante il nuovo avvio: il test attuale consegna sempre l'orfano prima del drain (`android/tests/java/it/cedro/android/ActivityDeliveryTest.java:14–17`) e pertanto non esercita il residuo. Il problema resta attribuire un file alla sorgente sbagliata e chiudere il journal della nuova richiesta.

#### Minor (Nice to Have)

- Nessun nuovo finding nel delta. I log contengono warning preesistenti dichiarati dal controller; non sono output pristine e non vengono riclassificati come regressioni di questo fix.

### Assessment

**Task quality:** Needs fixes — 0 Critical, 1 Important residuo, 0 Minor nuovi. I2 chiuso.

**Reasoning:** La separazione fra intento iniziale e nuovo intento risolve il replay dopo tombstone. Il drain periodico riduce I1, ma non assicura l'associazione corretta se la consegna orfana si inserisce nella transizione verso un nuovo picker.

**Evidenze del controller lette, senza riesecuzione:** `/home/accorsiluca/Documenti/cedro-android-anteprima/verifiche/fd3f38c/cedro-android-fd3f38c-final-runner.log`: NDK entrambe le configurazioni exit 0 (righe 44,86), host entrambe 56/56 ed exit 0 (126–128,168–170), clippy exit 0 con warning (210–212), compilazione helper Java e due test PASS/exit 0 (214–221). Queste prove non coprono l'interleaving del finding.

**Controlli svolti:** sola review di `5580f0f..fd3f38c`, report aggiornato e risultati già prodotti. Unica lettura aggiuntiva di sorgente fuori dal delta: `con_attivita`, per il rischio concreto di concorrenza fra drain/apertura e callback. Nessun test, Rust, build, adb, accesso remoto, modifica produttiva, subagente o nuova review generale.
