### Finding Verdicts

- **I1 — Il ponte accessibile usa il percorso Java che salta l'InputQueue nativa — ADDRESSED.** `android/java/it/cedro/android/CedroActivity.java:149` introduce una coda sincronizzata e `:254`/`:259` accodano click e scroll; la dispatchTouchEvent sintetica è rimossa. `android/src/glue.rs:1949` drena la coda via JNI sul thread Rust e unisce gli effetti al dispatcher dei tocchi fisici. `android/src/ui/accessibilita.rs:7` interpreta il protocollo, converte pixel/dp e usa i veri touch/hit-test; `android/src/glue.rs:1895` restituisce Unhandled per gli eventi Motion non gestiti, consentendo agli hover di raggiungere la View. Il test aggiornato `android/src/ui/accessibilita.rs:82` attraversa il protocollo con scala2; il nuovo test `:98` verifica che uno scroll JSON modifichi l'offset. Copertura host 3/3 PASS; trasporto JNI, ACTION_CLICK/SCROLL e hover/TalkBack fisici rimangono da provare sul candidato.

- **I2 — Il player genitore non può annullare l'attesa del lock nativo — ADDRESSED.** `src/ui/player.rs:209` lega la preparazione audio al token `Shared.stop` e `:217` usa il decoder cancellabile, interrompendo prima dell'uscita audio se il player è stato chiuso. `src/audio.rs:317`/`:327` propagano la closure fino al fallback Android; `src/mediadec.rs:301` combina cancellazione del consumatore e del worker nell'acquisizione e ricontrolla dopo il lock, prima di creare MediaExtractor. `src/ui/player.rs:427` aggiunge la prova con processo holder separato, drop del Player e nessuna acquisizione/ripartenza successiva; risultato 1/1 PASS. Il confine MediaExtractor è sostituito nel test host, mentre il token attraversa la preparazione e il dispatch audio di produzione: non viene presentato come collaudo del decoder fisico.

- **I3 — Più bersagli della stessa azione producono nodi virtuali con lo stesso ID — ADDRESSED.** `android/src/ui/accessibilita.rs:25` conta le regioni per azione prima dei filtri e `:65` include tale ordinal nell'identità, risolvendo anche collisioni senza occupare lo spazio del testo statico. Timestamp e testo conservano regioni separate. Il test `:98` costruisce la revisione reale con almeno due hit per Riga(0) e richiede unicità di tutti gli ID semantici; incluso nel PASS host 3/3.

- **I4 — TGA non riconosciuto dopo la sostituzione di ImageReader::open — ADDRESSED.** `src/imgfmt.rs:177` conserva il formato dall'estensione prima di `with_guessed_format`, che mantiene la precedenza del magic riconosciuto. Conversione, upright/OCR (`src/ocr.rs:24`) e miniature (`src/ui/media.rs:189`) usano lo stesso helper, sempre sopra input_file. Il nuovo test `src/imgfmt.rs:371` verifica TGA normale e grant nei tre percorsi, più PNG con estensione TGA: 1/1 PASS. L'esistente `engine::format_tests::photo_formats_roundtrip`, precedentemente fallito, è invariato nel fix e ora PASS 1/1.

- **I5 — Gli alias fd Unix ordinari rompono la copia MIRAX con dati annidati — ADDRESSED.** `src/input_file.rs:44` seleziona la policy obbligatoria di registrazione soltanto su Android; su Unix desktop un alias non registrato torna all'apertura standard, mentre quelli registrati conservano pread. Il test `:196` esercita esplicitamente il rifiuto della policy stretta e la semantica desktop. Non è stata alterata la fixture MIRAX per nascondere il guasto: l'esistente `mrxs::prove::copia_anonima_con_alias_fd_e_dati_annidati` è PASS 1/1; input-io è PASS 4/4, compresi stale alias, cursori indipendenti e lock/cancel interprocesso.

### New Breakage in the Fix Diff

- **Nessuna nuova rottura Critical/Important/Minor dimostrata nel fixdiff.** Il dispatcher Effetto di `android/src/glue.rs:1954` è trasferito dopo la raccolta eventi e condiviso fra le due sorgenti; i rami sono preservati. Il fix immagine non riapre il pathname del provider; la policy permissiva desktop non modifica la selezione stretta Android; la cancellazione non rimuove il guard RAII né la sua durata.

### Verification Evidence

- **Controllo eseguito: lettura del solo fix package `review-3d1629b..8e98780.diff`, 12 file, dei finding I1–I5 e delle appendici del report.** Nessun comando git, test, build, Rust, fmt, ADB, accesso remoto o subagente; nessuna modifica ai sorgenti. Scritto soltanto questo rapporto.
- **Controllo eseguito: riscontro dei sei log locali di 8e98780 e del TSV/exit del controller.** Directory `~/Documenti/cedro-android-anteprima/verifiche/8e98780/`, prefisso dei log `cedro-android-8e98780-task6-aac3-`: `fix-player.log:745–747` PASS1, `fix-img.log:738–740` PASS1, `fix-photo.log:738–740` PASS1, `fix-mrxs.log:738–740` PASS1, `fix-a11y.log:740–744` PASS3, `input-io.log:738–749` PASS4. Tutti i sei step e l'orchestrazione hanno exit0. Le prove helper avviate nei figli sono distinte dai conteggi principali.
- **Controllo eseguito: comandi e profilo nel solo script di verifica locale** `~/prove-cedro-android/verifica-task6.py:8–15`, `:43`: cargo test mirati ai test sopra, `-j2`, core/Android `--lib`, profilo test opt1/debug0/LTOoff/codegen16/incrementaltrue, fdk-aac-sys O3. Non profilo release e non prove Android fisiche. Nessuna suite è stata rieseguita dalla review.
- **Rumore dichiarato:** i log conservano 35 warning vendor rav1d (`fix-player.log:733`, `fix-a11y.log:733`); non è output privo di warning e non è una nuova regressione del fix. Il check/clippy finale rimane nel lavoro del controller.

### Out-of-Scope Observations

- Nessun nuovo difetto esterno al fix segnalato. Restano i gate già aperti: APK/NDK/Java aggiornati, UIAutomator/TalkBack reali, geometria e lingue, SAF/nativi sul dispositivo, misure e confezionamento finali. Il helper JAR preparato dal controller per performAction non è stato eseguito sul nuovo APK e non costituisce un PASS. Il successo dell'APK intermedio4c non è trasferito al nuovo ponte.

### Verdict

**Fix round: All findings addressed, no new Critical/Important breakage — I1, I2, I3, I4, I5 ADDRESSED; 0 finding aperti, 0 nuovi finding.**

**Spec compliance e task quality: Approved per questo fix round circoscritto.** L'approvazione statica e la copertura host non dichiarano completato il collaudo del candidato né autorizzano a omettere i gate fisici e di pubblicazione già assegnati al controller.
