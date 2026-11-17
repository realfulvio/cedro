### Spec Compliance

- ✅ I1 chiuso per il perimetro sorgente verificabile, base `fd3f38c`, head `f0247e8`. Ogni lancio riserva e persiste un codice distinto; binding al journal, armamento e lancio precedono l'accettazione della callback, sotto il medesimo lock Activity (`android/java/it/cedro/android/CedroActivity.java:91–104,168–188`).
- ✅ La callback precedente arrivata dopo il nuovo armamento non modifica né il risultato né la richiesta corrente: `expects` confronta l'identità e `picker` disarma dopo un solo risultato. `arm` elimina il vecchio risultato già in buffer (`android/java/it/cedro/android/ActivityDelivery.java:24–36`). Il test esercita l'interleaving esatto del finding, più duplicati e fallimento del lancio (`android/tests/java/it/cedro/android/ActivityDeliveryTest.java:18–36`).
- ✅ Il ripristino accetta solo il codice gestito concorde tra Bundle e journal, prima dell'avvio NativeActivity/Rust; una nuova sorgente scritta senza binding non viene associata al codice precedente (`android/java/it/cedro/android/CedroActivity.java:70–85,107–130`; `android/java/it/cedro/android/ActivityDelivery.java:9–20`). Test di mismatch, codice mancante e ripristino valido in `android/tests/java/it/cedro/android/ActivityDeliveryTest.java:14–16,38–46`.
- ✅ I2 rimane chiuso: le API monouso per avvio iniziale e nuovi intenti conservano il comportamento del round precedente; il test mantiene seconda ricreazione e nuovo avvio esplicito (`android/java/it/cedro/android/ActivityDelivery.java:17–23`; `android/tests/java/it/cedro/android/ActivityDeliveryTest.java:43–53`). Nessuna modifica Rust, permission, dipendenza o autostart nel delta.
- ⚠️ Cannot verify physical: il gate di pubblicazione resta il collaudo Mi9T del controller con morte deterministica durante DocumentsUI, ritorno dell'originale SAF ≥32 MiB, viewer corretto e annullamento/riavvio senza replay. I test host non verificano consegna e Bundle del framework Android reale.

### Strengths

- La correzione lega la risposta alla richiesta invece di dipendere dal momento del drain; rifiutare una callback vecchia conserva l'attesa nuova (`android/java/it/cedro/android/ActivityDelivery.java:29–34`).
- Il codice viene persistito prima dell'apertura e il binding usa `AtomicFile`; la lettura iniziale avviene prima che Rust acceda al journal (`android/java/it/cedro/android/CedroActivity.java:73–75,98–104,111–130`).
- Il test usa lo stesso stato produttivo e verifica il comportamento che mancava, senza simulare un PASS del lifecycle Android (`android/tests/java/it/cedro/android/ActivityDeliveryTest.java:18–35`).

### Issues

#### Critical (Must Fix)

- Nessuno.

#### Important (Should Fix)

- Nessuno nel delta e nel finding residuo esaminati.

#### Minor (Nice to Have)

- Nessuno nuovo. Limite tecnico esplicito valutato: 49.152 prenotazioni nell'intervallo `0x4000..0xffff`, comprese aperture fallite; esaurito l'intervallo il picker rifiuta ulteriori aperture senza riusare codici (`android/java/it/cedro/android/ActivityDelivery.java:5,13–16`; `android/java/it/cedro/android/CedroActivity.java:98–100`). Per frequenza e impatto atteso non è un blocco di questo fix circoscritto; il limite è documentato e il no-wrap è testato (`android/tests/java/it/cedro/android/ActivityDeliveryTest.java:55–57`).

### Assessment

**Task quality:** Approved — 0 Critical, 0 Important, 0 Minor nuovi. I1 e I2 chiusi; sorgente pronta per il collaudo fisico finale, senza anticiparne l'esito.

**Reasoning:** Codici distinti, associazione persistente al contesto e rifiuto delle callback non corrispondenti eliminano l'interleaving segnalato. Non sono emerse regressioni nel delta Java esaminato.

**Evidenze del controller lette, senza riesecuzione:** `~/Documenti/cedro-android-anteprima/verifiche/f0247e8/cedro-android-f0247e8-final-runner.log`: NDK con/senza YouTube exit 0 (44,86), host entrambe 56/56 ed exit 0 (126–128,168–170), clippy exit 0 (212), Java helper compilati e `AccessibilityCoordinatesTest`/`ActivityDeliveryTest` PASS, exit 0 (215–221). I warning baseline dichiarati restano distinti da nuove regressioni; compilazione/repack APK finale e prova fisica sono al controller.

**Controlli svolti:** una lettura del diff `fd3f38c..f0247e8`, append del report e soli esiti già prodotti. Nessuna nuova lettura generale, esecuzione di test/build/Rust/adb, accesso remoto, subagente o modifica produttiva.
