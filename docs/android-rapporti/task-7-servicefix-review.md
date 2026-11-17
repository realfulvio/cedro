### Spec Compliance

- ✅ Conforme per l'ambito sorgente verificabile, base `8e98780`, head `4b2f686`. L'unico file di produzione modificato è `android/java/it/cedro/android/ServizioLavoro.java`: la promozione iniziale avviene in `onCreate` prima della preparazione del WakeLock (righe 25–36); titolo, testo e percentuale ricevuti da Rust aggiornano ancora la notifica (righe 46–49, 54–68). Nessuna modifica a permessi, dipendenze, manifest o autostart nel diff.
- ✅ Compatibilità minSdk 26 preservata nel codice modificato: il builder con canale e i due rami `startForeground` restano quelli preesistenti, con il tipo `DATA_SYNC` confinato ad API >=29 (`android/java/it/cedro/android/ServizioLavoro.java:54–68`).
- ✅ Cleanup centralizzato per stop esplicito e distruzione: rilascio condizionale del WakeLock e rimozione della notifica (`android/java/it/cedro/android/ServizioLavoro.java:41–44,71–79`). Lo stop Rust continua a passare da `stopService` (`android/src/glue.rs:785–787`).
- ⚠️ Non verificabile dal diff: eliminazione del crash reale, comportamento del framework nello specifico interleaving start/stop, annullamento e prosecuzione effettiva in background. Il report distingue correttamente l'ipotesi causale dalla prova e richiede collaudo fisico prima del rilascio (`.superpowers/sdd/ESECUZIONE/task-7-servicefix-report.md:20–28,96–107`). Il controller deve ripetere apertura di `dialogo.wav`, avvii/arresti rapidi e un lavoro reale in background, verificando assenza di crash e residui di notifica/WakeLock.

### Strengths

- La promozione iniziale non dipende più dalla consegna dell'Intent e non acquisisce il WakeLock; la notifica definitiva riusa lo stesso builder senza duplicazione (`android/java/it/cedro/android/ServizioLavoro.java:33,49–54`).
- `null` e `ferma` escono senza acquisire il lock; la distruzione riusa lo stesso cleanup, con controllo null e stato del lock (`android/java/it/cedro/android/ServizioLavoro.java:41–44,71–79`).
- Le evidenze statiche sono dichiarate come tali e non presentate come test comportamentali Android (`.superpowers/sdd/ESECUZIONE/task-7-servicefix-report.md:54–87`).

### Issues

#### Critical (Must Fix)

- Nessuno.

#### Important (Should Fix)

- Nessuno individuato nel perimetro della review sorgente.

#### Minor (Nice to Have)

- Nessuno.

### Assessment

**Task quality:** Approved, limitatamente alla modifica sorgente; gate runtime ancora da completare.

**Reasoning:** La modifica anticipa la promozione, preserva i dati della notifica e raccoglie il cleanup senza introdurre nuove dipendenze o ampliare l'ambito. La lettura statica non dimostra che l'anticipo risolva lo specifico crash osservato, quindi l'approvazione non autorizza a considerare superato il collaudo fisico obbligatorio.

**Controlli svolti:** diff letto una volta; nessuna rilettura del file Java modificato, comando git, test, build, Rust o adb. Controllo aggiuntivo mirato al rischio nominato «stopService mentre startForegroundService è in coda»: ricerca dei soli riferimenti al servizio e lettura di `servizio` in `android/src/glue.rs:780–804`, che conferma stop diretto e invio invariato degli extra titolo/testo/percento. Nessun'altra esplorazione del codice.
