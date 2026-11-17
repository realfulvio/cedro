- **I1 — Chi parla e fogli delle voci irraggiungibili: ADDRESSED.** `android/src/ui/rivedi.rs:537–586` fa scorrere lettore, schede e voci anche in landscape, misura il contenuto e ritaglia gli hit nella stessa viewport del disegno; `:822–886` dimensiona i controlli in base al font e restituisce l'estensione completa. `:777–814` assegna al foglio offset/limite indipendenti e instrada il gesto al foglio senza muovere la pagina. Le regressioni `:1158` e `:1201` verificano tutte le voci, rename/toggle e ultima destinazione con tre/otto voci, tre viewport, font 100%/200% e dispatcher reale.
- **I2 — Originale SAF di firma staccata riaperto tramite pathname: ADDRESSED.** `android/src/glue.rs:431` usa ora `storage::leggi_originale`; `android/src/storage.rs:60–74` apre mediante `cedro::input_file::File`, conservando lettura a blocchi, cancellazione ed errore di allocazione. Il test `:150–174` legge 32 MiB + 17 byte tramite registro con pathname non riapribile, verifica contenuto e indipendenza di entrambi i cursori e il caso annullato.
- **M1 — MIME WebM/MKV generico: ADDRESSED.** `android/src/storage.rs:29–30` assegna i due MIME video; `android/src/glue.rs:112`, `:536` e `:1084` conservano un unico mapping per intent e pubblicazione. Test `storage.rs:141–146` include i due formati, maiuscole e fallback; il resto della mappa è preservato.
- **I3 — Bounds accessibili spostati di una status bar: ADDRESSED nel codice.** `android/java/it/cedro/android/AccessibilityCoordinates.java:13–22` separa coordinate window, overlay locale e schermo; `CedroActivity.java:192–202`, `:219`, `:253–258` applica la stessa trasformazione a parent/screen bounds e hover usando le origini correnti. `:275–282` mantiene il click nelle coordinate native originali e converte soltanto il gesto scroll locale, evitando la doppia sottrazione. Il test Java copre il caso Mi9T, inset laterale, finestra spostata e conversione hover/scroll; `android/src/ui/accessibilita.rs:82` conserva la prova del centro semantico nel dispatcher Rust con densità e insets. Il retest fisico sul nuovo APK resta un gate, non un PASS implicito di questa verifica statica.

## New Breakage in the Fix Diff

- **Nessuna individuata.** Controllati anche bilanciamento clip, ridisegno dopo clamp, priorità del foglio sugli hit sottostanti, reset dello scroll all'apertura e inclusione della nuova classe Java tramite il glob produttivo di `android/build.sh:50`.

## Out-of-Scope Observations

- **Nessuna.** La verifica resta circoscritta ai quattro finding e al delta `b4d03e7..e9b6a72`, senza ripetere la review generale.

## Checks

- **Diff letto integralmente in quattro tranche:** `.superpowers/sdd/ESECUZIONE/review-b4d03e7..e9b6a72.diff`, 1.007 righe; confrontati brief Task 7, finding del controller e rapporto finale della fix wave.
- **Host con/senza YouTube confermato dai log e dal TSV in `/home/accorsiluca/Documenti/cedro-android-anteprima/verifiche/e9b6a72`:** 55/55, zero fallimenti in entrambe le esecuzioni; presenti e `ok` i cinque test nuovi per voci/fogli, originale SAF, MIME e centri semantici.
- **NDK release all-targets in entrambe le configurazioni e clippy confermati:** exit 0 nel TSV e completamento nei rispettivi log; non interpretato come assenza di warning preesistenti.
- **Test Java confermato nel `cedro-android-e9b6a72-final-runner.log:213–217`:** javac exit 0; `AccessibilityCoordinatesTest: PASS`, java exit 0. È un test host della trasformazione produttiva, non un test del framework Android sul telefono.
- **Correzione della fixture coerente con l'oracolo:** `rivedi.rs:1136–1144` passa i nomi al costruttore e verifica le voci attive/speaker prima delle asserzioni di raggiungibilità; non sostituisce o indebolisce tali asserzioni. I precedenti due fallimenti restano dichiarati nel report.
- **Nessuna suite rieseguita, nessun Rust/adb/altro agente, nessuna modifica di produzione.** Scritto soltanto questo rapporto autorizzato.

## Verdict

- **Fix round: All findings addressed, no new Critical/Important breakage.** Aperti nel perimetro della review: zero.
- **Spec: Approved per il delta. Quality: Approved.** Resta condizionata la pubblicazione alla build/installazione del candidato corretto e ai gate fisici assegnati al controller: centri dei nodi/tap, performAction, hover/rotazione, voci/fogli con font grandi/tastiera, originale SAF ≥32 MiB e MIME/intent effettivi. Il benchmark ancora eseguito sull'APK precedente non costituisce verifica fisica del fix.
