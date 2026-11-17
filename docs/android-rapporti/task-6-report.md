# Task 6 — integrazione finale Android

Stato implementazione: **DONE_WITH_CONCERNS**, checkpoint sorgente e revisione disponibili; collaudo e pubblicazione affidati al controller. Nessuna dichiarazione di release pubblicata.

## Checkpoint 1: probe SAF e navigazione

- Aggiunto probe nel worker: fd ereditati, F_GETFD, metadata/fstat, pread di 16 byte a offset zero, risultato separato di File::open del pathname. Nessun URI, nome o contenuto nei nuovi log. Il probe duplica solo per ownership temporanea e legge posizionalmente: non altera il cursore condiviso.
- Logger Android inizializzato anche nel launcher/worker (tag CedroWorker).
- Azzerato lo scroll al nuovo import/condivisione. Avvio conversione da foglio nei temi Cedro porta alla stessa route Lavori con 3 destinazioni. Le azioni nav/Impostazioni/Lavori riattivano la casa anche dopo un foglio import.
- Corretto cast signal handler tramite puntatore, stesso ABI.
- Aggiunto test regressione import con scroll precedente e completamento route.

Verifica locale: `git diff --check` PASS. Nessun Rust, build, adb o test runtime eseguito dall'implementer come concordato; controller esegue test/build da checkpoint committati. Questo checkpoint NON dichiara risolto EACCES: serve probe reale e segue fix lettura diretta.

## Checkpoint 2: ingresso posizionale

Aggiunto `src/input_file.rs`: i normali file delegano a std::fs; solo gli alias privati /proc/self/fd/N con lease registrata usano Arc<File> e read_at/pread con cursore logico per lettore. Nessun seek sul descriptor condiviso. Registro iniziale alimentato da SAF nel genitore e dall'elenco fd ereditati nel worker; il checkpoint finale aggiunge associazione esplicita percorso→lease (vedi sotto); nessuna apertura del pathname provider per leggere quei grant. Il probe continua a confrontare pread e pathname_open indipendentemente dal fix.

Collegati NDPI, SVS, ND2, LIF, CZI, microscopia e sidecar MIRAX, sniff immagini, immagini raster/HEIC/AVIF/SVG, DICOM e Symphonia/audio. La copia per MIRAX anonimo legge il grant solo quando l'utente richiede quella copia. Nessuna copia preventiva del vetrino, nessun limite nuovo, nessun algoritmo di formato modificato. NDPI mantiene il contesto nell'errore di apertura.

Test aggiunti (NON ancora eseguiti): cursori grant paralleli/interleaved, seek negativo, grant read-only, preservazione offset originale; lettura vera NDPI sintetico per livelli e regioni concorrenti usando alias registrato. `git diff --check` PASS. Necessari controller: suite core+Android, probe SAF reale e preview OS-3. L'ispezione non prova tutti i codec esterni e gli ingressi pathname di terze parti: PDF/Office/decoder nativi e programmi esterni restano da verificare separatamente per SAF >32MB.

## Checkpoint 3: misure testo, superfici accessibili e viewport

- Painter: fattore font sistema con misura/wrap coerenti, fallback Android lazy per cirillico/arabo/CJK. Nessun font aggiunto. Shaping RTL completo non implementato: limite dichiarato nel README.
- Java minimo nella CedroActivity esistente: provider semantico con focus/hover e snapshot di testo e bersagli Rust. Azioni click/scroll riusano input nativo, non stub. Stato selezionato per lingua/tema/vista/referto. Restano da misurare copertura/attivazione TalkBack e UIAutomator.
- Famiglie catalogo su righe adattive misurate. Header Aspetto legacy: hit area intersecata con viewport, nessun bersaglio sotto back. Nuovo render dopo clamp dello scroll nello stesso disegno: nessun primo frame vuoto per offset fuori limite.
- Revisione: onda compatta nel pannello combinato su telefono; con viewport corta/font grandi scorre anche la parte controlli, export fisso e testo con altezze misurate. Verifica renderer richiesta.

Verifica statica locale `git diff --check` PASS. Build Rust/Java e dispositivi non eseguiti dall'implementer. Il report non dichiara PASS accessibilità/layout prima di prove controller.

## Ingressi nativi/esterni: decisione controller e implementazione

Audit QA esterni archiviato separatamente. MediaExtractor AOSP riceve un fd ma fa lseek/read su dup con mutex per singola istanza. Il controller ha autorizzato un lockfile interprocesso per la stessa sorgente (dev+ino), directory privata comune via config_dir, open file description distinta per lock e guard RAII. Implementato solo per grant; file ordinari e pread non sono serializzati. Attesa LOCK_NB a 40 ms con cancellation callback; worker aggancia il flag SIGUSR1. Il decoder video rilascia esplicitamente l'extractor prima del possibile fallback audio, evitando auto-deadlock sullo stesso grant. L'attesa avviene nei thread decoder, mai nel main UI.

Fallback ffmpeg: riutilizza l'ambiente Android già usato da YouTube e inizializza le directory anche nel worker. Il binario deve essere realmente presente nell'APK; dipendenze scaricate mediante il flusso componenti esistente. Per SAF verifica il protocollo `fd` nel binario concreto con `-protocols`, timeout/cancellazione e ambiente corretto. Solo se presente usa `-fd N -i fd:` e fd ereditato senza CLOEXEC nel solo figlio, sotto lo stesso guard nativo. Nessun pipe/nonseekable e nessun fallback pathname negato per i grant. Il processo Android rimane nel gruppo worker e riceve PDEATHSIG. La disponibilità concreta del protocollo nel pacchetto va ancora provata sul telefono, non è un PASS dedotto da documentazione upstream.

Aggiunto test interprocesso: secondo processo non acquisisce la lease finché il primo la possiede, cancellazione dell'attesa, pread continua mentre il lock è occupato, release poi acquisizione del figlio. Non eseguito localmente; controller esegue Rust.

Java: controller ha eseguito javac/d8 isolati su d1aeb88 con stessa toolchain APK: PASS, classes.dex 20584 B; solo nota API deprecated. Primo errore lambda di 5f3 risolto con Runnable anonimi, nessun desugaring aggiunto.

## Lingue, risorse e documentazione

- Controllo statico `python3 android/tools/check-i18n.py`: RED iniziale 51 mancanze per lingua (incluse 5 chiamate locali `text` non UI, poi escluse correggendo il riconoscitore). GREEN reale: en/de/es/fr/pt, 122 chiavi letterali ciascuna, **0 mancanti**. Aggiunte 47 voci Android alle cinque TSV, più due errori del decoder fd. Il test copre literal tr/tr_string e metodi Painter text/text_wrap: non certifica le stringhe dinamiche o passate indirettamente.
- `python3 android/tools/check-contrast.py`: PASS 30 coppie; testo/muted/brand su frame e frame_hi delle quattro tinte: minimo 5,04:1; sei tessere attività minimo 7,28:1. Questo è un audit delle coppie dichiarate, non di ogni pixel di screenshot o colore dinamico.
- Build core produce TSV specifiche Android escludendo le righe dell'edizione ente; sorgenti desktop intatte. PNG edizione esclusi a compile-time su Android, nome/chiave/Debug Android della variante ritirata non incorporano la denominazione. Necessario audit APK effettivo del candidato (strings/assets e dimensioni).
- Corretto il caso desktop nuovo con entrambi i marker: Labs ha precedenza senza creare prima configurazione Simple; preferenze già presenti invariate. Esteso il test esistente del marker.
- Note Android 0.53.0, README riscritto nelle parti obsolete e riga Android RILASCI aggiornata come candidato. Nessuna versione desktop, workflow o metadato F-Droid modificato. Build legge 0.53.0 dal Cargo.toml; formula esistente produce5300, collisione/tag controller.
- Rapporti/review precedenti archiviati in docs/android-rapporti; README non punta più allo workspace SDD. Il controller conserva la proprietà delle prove finali e può aggiornare gli archivi dopo nuovi risultati. Nessuna fixture/audio/foto privata copiata.


## Chiusura audit SAF e geometria

- Il ripristino URI avviene in background: il registro iniziale solo per numero fd poteva associare un vecchio symlink a un grant diverso dopo riavvio. Ora il registro contiene percorso→(alias_fd, Arc<File>) e il launcher trasmette soltanto le associazioni delle lease ereditate. Un alias `/proc/self/fd` privo di registrazione per quel percorso è rifiutato, mai riaperto via fallback. Regressione aggiunta `stale_alias_cannot_select_another_paths_reused_fd`. La sostituzione atomica del symlink durante ripristino può produrre un errore transitorio, non la lettura di un altro documento.
- Audit ulteriore: preview Office/PDF/audio/video, Matroska, lettore whisper Android, miniature raster, riconoscimento firme e hash consulto MRXS usano lo stesso ingresso. `mp4_reader` è generico Read+Seek, senza cambiare parsing. Il percorso faststart scrive un risultato locale e mantiene il File ordinario. Aperture font/config/test restano ordinarie.
- Prima del protocollo ffmpeg fd, reset del cursore nativo a zero sotto guard: un precedente MediaExtractor può averlo lasciato a fine file. Nessun reset dei cursori logici pread.
- Condivisione MediaStore usa `_display_name` per EXTRA_TITLE, senza cambiare MIME o flusso di pubblicazione.
- Visto realmente il PNG baseline Task5 `it-360x640-referto`: tutti i controlli occupavano la viewport del testo. Il fix riduce l'onda combinata a 48dp nei viewport corti e consente lo scorrimento dei controlli insieme alle frasi. A font200 crescono le righe player e filtri, le altezze delle frasi vengono ricalcolate, follow/edit include l'offset dei controlli. Un clamp dello scroll revisione ridisegna nello stesso frame. Test aggiunto `large_system_fonts_remeasure_rows_and_allow_reaching_the_phrase`; ancora da eseguire dal controller.
- Esempio host `vista_a`: `CEDRO_RENDER_FONT_SCALE=2` produce la scala OS reale, distinta dall'opzione interna Grandi. Questa variabile appartiene soltanto all'esempio, non all'app di rilascio.
- Controllo lingue esteso agli helper testo_a/testo_centrato_a: ultimo PASS reale **134 chiavi × 5 lingue, zero mancanti**. Restano esclusi i testi costruiti indirettamente/dinamicamente; il renderer e il telefono verificano quelli.

## Evidenze effettive ricevute dal controller/QA

Queste prove riguardano checkpoint intermedi specifici, non il candidato finale:

| Checkpoint | Prova | Esito osservato |
| --- | --- | --- |
| 4c67b2c, APK 5299 | SAF reale NDPI 1.473.064.608 B, Panoramica | PASS: tessuto visibile, dimensioni 122880×77824 |
| 4c67b2c, APK 5299 | Centro livello 1 e ritorno Panoramica | PASS: dettaglio distinto, avviso area centrale, ritorno riuscito |
| 4c67b2c, worker | Probe fd87 | fstat valido, pread16 valido, pathname_open EACCES: causa distinta e confermata |
| 4c67b2c | PSS UI dopo preview | ~55,6 MB panoramica / ~70,4 MB centro; non è il picco aggregato UI+worker |
| 4c67b2c | Test cursori grant concorrenti | Controller: 1/1 PASS |
| d1aeb88 | javac e d8 stessa pipeline APK | PASS; classes.dex 20.584 B, solo nota API deprecated |
| d539cae | cargo check core controller | FAIL: tre aperture doctext e tipo ZipArchive; corretti in 6e0b422, non occultato |
| sorgente finale | check-i18n.py | PASS 134 chiavi per lingua, cinque lingue |
| sorgente finale | check-contrast.py | PASS 30 coppie, minimo 5,04:1 |
| ogni checkpoint | git diff --check locale | PASS |

QA distingue il primo campione centro completo entro 2,4 secondi dal tempo assoluto non misurato. Il limite lato1024 è verificato nel codice Labs, non misurando il PNG privato. Report completo runtime a cura QA: `qa-saf-4c67b2c-report.md` (archiviazione finale controller). Il filtro errato `ndpi::tests` restituiva zero test e NON è contato come PASS; test corretto `ndpi::prove::ndpi_grant_legge_anteprima_e_regioni_senza_spostare_fd`.

## File condivisi modificati e motivazione

Nessuna versione desktop, workflow, dipendenza Cargo o metadato F-Droid cambiato.

| File | Ragione limitata alla correzione/integrazione |
| --- | --- |
| build.rs | Generazione TSV Android prive delle risorse dell'edizione ritirata |
| src/archive.rs | ZIP ingresso da grant e firma concreta ZipArchive coerente |
| src/audio.rs | Sniff/lettura audio dal grant |
| src/avi.rs | AVI ingresso dal grant |
| src/czi.rs | Lettore CZI posizionale |
| src/dicom/mod.rs | Lettura bytes DICOM autorizzata; algoritmo/memoria invariati |
| src/doctext.rs | ZIP/CFB/RTF/PDF da ingresso autorizzato |
| src/edizione.rs | Esclusione risorse dell'edizione dal target Android |
| src/engine.rs | Ingressi PDF/Office/testo/immagini/ZIP e copie richieste; test PDF/Office |
| src/firma/verify.rs | Bytes delle firme e originali dal grant |
| src/i18n.rs | Tabelle generate Android, originali desktop |
| src/imgfmt.rs | Reader raster/JXL e bytes dei formati speciali |
| src/input_file.rs | Registry per percorso, pread/seek logico, guard nativo, test |
| src/lib.rs | Esporta ingresso condiviso per il crate Android |
| src/lif.rs | Lettore LIF posizionale |
| src/linux_video.rs | Reader ISO autorizzato; release extractor prima audio |
| src/mediadec.rs | Descriptor autorizzato e guard per lifetime extractor |
| src/microscopia.rs | Ingressi stack posizionali |
| src/mkv.rs | Matroska da Read+Seek autorizzato |
| src/mp4fast.rs | Generalizza solo mp4_reader a Read+Seek |
| src/mrxs.rs | Header/sidecar/blocchi MIRAX autorizzati |
| src/nd2.rs | Lettore ND2 posizionale |
| src/ndpi.rs | Reader diretto, contesto errore e regressione livello/regione |
| src/ocr.rs | ImageReader autorizzato |
| src/pacchetto.rs | Bytes ingresso pacchetti da grant |
| src/settings.rs | Nuova directory con entrambi marker privilegia Labs; test |
| src/softvideo.rs | Sniff video dal grant |
| src/svs.rs | Lettore SVS posizionale |
| src/svs_lab.rs | Hash consulto e sidecar MIRAX dal grant |
| src/symph.rs | MediaSource autorizzato |
| src/trascrizione.rs | Fallback fd seekable verificato, cancellazione e guard |
| src/ui/firmare.rs | Riconoscimento buste dal grant |
| src/ui/gfx.rs | Scala testo, fallback sistema lazy, raccolta semantica opzionale |
| src/ui/media.rs | Thumbnail raster/PDF e metadata PDF da grant |
| src/ui/pdfwork.rs | Anteprima PDF timbrato dal grant |
| src/ui/preview.rs | Anteprime documenti e probe audio/video dal grant |
| src/ui/theme.rs | Debug/denominazioni Android senza edizione ritirata |
| src/whisper_android/decode.rs | Ingresso audio grande autorizzato |
| src/youtube.rs | Riusa ambiente ffmpeg Android e cattura cancellabile protocolli |

## Self-review e gate residui

Controllati lifetime delle lease, rifiuto alias non registrati, assenza di seek nei lettori pread, release guard nativo, trasporto fd child-only, assenza di nuove copie integrali su disco, comportamento desktop ordinario del wrapper, metadati condivisi invariati, nessun nuovo permesso/dipendenza e nessun file privato aggiunto. Gli errori di compilazione comunicati dal controller sono stati corretti senza reintrodurre pathname open SAF.

Resta indispensabile prima della pubblicazione:

- check/clippy e suite host/core/NDK sul checkpoint finale, incluse regressioni per grant, lock interprocesso/cancel, PDF/Office, import e semantica;
- APK aggiornato e prova TalkBack/UIAutomator con azione vera, font130/200 e sei lingue, header dopo scroll, intent/completamento nella navbar3, referto leggibile;
- prova fd protocol nel ffmpeg concreto e lettori native concorrenti; il supporto documentato upstream non è prova del binario nell'APK;
- audit APK assets/strings/licenze/dimensioni e misure base vs finale con PSS aggregato, CPU/UI/render5Hz, coldstart, lavori e batteria;
- review finale, aggiornamento archivi report con prove controller, collisione versione/tag, release Android autorizzata.

Limiti dichiarati: fallback arabo a glifi non significa shaping RTL completo; il provider semantico deve ancora essere verificato per copertura/ordine/azioni sul dispositivo; API native della stessa sorgente sono serializzate (le letture pread e gli altri file no); PDF/Office/DICOM mantengono eventuali letture in RAM già proprie del motore. Nessun falso PASS su Xiaomi13, esplicitamente escluso dal mandato. Nessun comando Rust/build/ADB o remoto eseguito dall'implementer.


## Fix round 1 — base 3d1629b, rilievi I1–I5

Letta integralmente la review `task-6-review.md`, incluse limitazioni Cannot verify. Controller ha inoltre comunicato core suite **592 PASS, 2 FAIL, 15 ignored** su 3d1629b: `engine::format_tests::photo_formats_roundtrip` (TGA non riconosciuto) e `mrxs::prove::copia_anonima_con_alias_fd_e_dati_annidati` (alias Unix non registrato). Le quattro verifiche di compilazione precedenti erano PASS; il job si è fermato sui test core, nessuna suite successiva viene contata come eseguita.

### Correzioni

- **I1, ponte accessibile:** rimosso dispatchTouchEvent Java sintetico. `CedroActivity` accoda `[kind,x,y,endY]` in una JSONArray sincronizzata; `takeAccessibilityActions` svuota la coda. Il thread UI Rust la legge nel ciclo eventi esistente, converte pixel/dp e chiama touch/hit-test reali. Gli Effetto prodotti percorrono lo stesso match dei tocchi fisici, spostato fuori dalla callback InputQueue senza alterarne i rami. Hover e altri MotionAction ignorati restituiscono Unhandled, così raggiungono il provider Java. Il poll idle già esistente resta 150 ms; nessun loop nuovo a 60 Hz. Nessun nuovo JNI export, permesso o libreria.
- **I2, cancellazione player:** `prepare_audio` legge lo stop del singolo Player; la closure arriva a `audio::decode_cancellable`, al fallback nativo e a `AudioDecoder::open_cancellable`. L'acquisizione combina la cancellazione del consumatore con quella del worker e ricontrolla dopo il lock, prima di creare MediaExtractor. Lo stop viene controllato anche durante i chunk Symphonia/nativi e prima di avviare l'uscita audio. `Player::drop` rimane la sorgente del token. Il confine decoder nativo è iniettabile nel test host per provare il collegamento senza dispositivi audio; la produzione Android chiama sempre MediaExtractor reale.
- **I3, ID duplicati:** ogni regione dello stesso Azione ha un ordinal stabile nel frame e un ID distinto. Gli ordinal sono contati prima dei filtri di visibilità; collisioni hash vengono risolte senza entrare nello spazio ID del testo statico. Timestamp e testo mantengono entrambi bounds/label/click; non vengono fusi in un rettangolo il cui centro potrebbe attivare un altro comando.
- **I4, TGA:** `imgfmt::reader` ripristina il formato dedotto dall'estensione prima di with_guessed_format; magic riconosciuto mantiene precedenza. Lo stesso helper serve conversione, OCR upright e miniatura, sempre usando input_file. Non reintroduce File::open del provider.
- **I5, alias Unix:** la policy obbligatoria path→grant rimane attiva per Android. Su Unix desktop gli alias non registrati conservano le normali semantiche std::fs; i grant esplicitamente registrati continuano a usare pread. Il test stale-alias verifica esplicitamente la stessa policy stretta selezionata da cfg Android e aggiunge una lettura con policy desktop. L'esistente test MIRAX con alias e dati annidati resta invariato: nessun adattamento della fixture per nascondere la regressione.

### Copertura aggiunta/aggiornata, ancora da eseguire sul controller

- `ui::accessibilita::tests::semantic_jobs_action_is_the_real_native_hit_target`: usa ora il JSON realmente prodotto dal ponte, con scala2 pixel/dp, e verifica route Lavori.
- `ui::accessibilita::tests::review_regions_have_unique_ids_and_bridge_scroll_moves_content`: revisione reale con timestamp/testo, almeno due regioni Riga(0), unicità di tutti gli ID e comando scroll JSON che modifica l'offset.
- `ui::player::tests::closing_player_cancels_native_wait_before_other_process_releases_lock` con helper child: un processo separato tiene lo stesso lock inode; la pipeline di preparazione del player entra nel fallback nativo e attende; drop(Player) deve terminarla prima del rilascio del processo holder, senza acquisizione né ripartenza successiva. Non apre dispositivi audio.
- `imgfmt::tests::tga_extension_survives_grant_reader_and_magic_still_wins`: TGA ordinario e grant, conversione/upright/thumbnail, PNG con estensione TGA per verificare precedenza magic.
- `input_file::tests::stale_alias_cannot_select_another_paths_reused_fd`: policy Android stretta contro alias estraneo, più semantica desktop ordinaria.
- Rieseguire i due test esistenti falliti e le suite core/Android appropriate.

### Verifiche effettivamente eseguite dall'implementer

- `git diff --check`: PASS.
- `python3 android/tools/check-i18n.py`: PASS 134 chiavi per ciascuna delle cinque lingue, zero mancanze. Nessuna nuova stringa visibile.
- Confronto Python del match Effetto precedente e nuovo normalizzando solo whitespace: PASS, rami invariati.
- Self-review dei callsite player→audio→mediadec, policy alias per target, precedenza immagine, coda sincronizzata Java, parser Rust, coordinate e ID.

**Non eseguiti qui:** cargo check/test/clippy, rustc, fmt, build Java/APK, remoto, ADB, UIAutomator/TalkBack. Le prove host del protocollo non sono una prova del trasporto JNI fisico né dell'esplorazione TalkBack: restano obbligatori ACTION_CLICK Lavori, ACTION_SCROLL e hover sul candidato aggiornato. Il cold Play di 4c funziona nella prova controller; l'errore durante cattura scrcpy non è diagnosticato come bug applicativo e questo round non modifica quel comportamento per ipotesi.

Stato round: **correzioni implementate, verifica controller e scoped review dei cinque rilievi pendenti**. Nessun FAIL trasformato in PASS prima dell'esecuzione.

## Controller — copertura fix1 eseguita su8e98780

MiniPC,profilotestopt1/debug0/LTOoff/codegen16/incrementaltrue,targetdircedro-android-target-test,overridefdk-aac-sysO3,nice10,j2,testthreads2. Comandi esatti ~/prove-cedro-android/verifica-task6.py o /srv/dev/scratch/cedro-verifica-task6.py. Logarchiviati ~/Documenti/cedro-android-anteprima/verifiche/8e98780. NonAndroidfisico/nonprofilorelease.

### fix-player
```text
running 1 test
running 1 test
test ui::player::tests::player_lock_holder_child ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 611 filtered out; finished in 0.02s
test ui::player::tests::closing_player_cancels_native_wait_before_other_process_releases_lock ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 611 filtered out; finished in 0.03s
```

### fix-img
```text
running 1 test
test imgfmt::tests::tga_extension_survives_grant_reader_and_magic_still_wins ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 611 filtered out; finished in 0.00s
```

### fix-photo
```text
running 1 test
test engine::format_tests::photo_formats_roundtrip ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 611 filtered out; finished in 0.02s
```

### fix-mrxs
```text
running 1 test
test mrxs::prove::copia_anonima_con_alias_fd_e_dati_annidati ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 611 filtered out; finished in 0.01s
```

### fix-a11y
```text
running 3 tests
test ui::accessibilita::tests::semantic_jobs_action_is_the_real_native_hit_target ... ok
test ui::accessibilita::tests::system_text_scale_changes_measured_layout ... ok
test ui::accessibilita::tests::review_regions_have_unique_ids_and_bridge_scroll_moves_content ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 47 filtered out; finished in 0.22s
```

### input-io
```text
running 4 tests
test input_file::tests::native_lock_child_entry ... ok
test input_file::tests::grant_readers_have_independent_positions_in_parallel ... ok
test input_file::tests::stale_alias_cannot_select_another_paths_reused_fd ... ok
running 1 test
test input_file::tests::native_lock_child_entry ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 611 filtered out; finished in 0.20s
test input_file::tests::native_lock_is_interprocess_cancellable_and_leaves_pread_free ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 608 filtered out; finished in 0.22s
```

Tutti6stepEXIT0. Callbacknativecancel, TGA/MRXSactualregressions, JSON+IDs+scrollUIhostPASS. Java→Rusttransport/UIAutomation/TalkBackfisiciancoraPENDINGsucandidatoaggiornato, non PASS daquestihosttest.


## Triage clippy finale — log 8e98780 rispetto a baseline 0a3a64b

Log effettivo letto dal notebook: `Documenti/cedro-android-anteprima/verifiche/8e98780/cedro-android-8e98780-task6-aac3-clippy.log`, prodotto dal controller, exit0. Il controller comunica completata PASS la pipeline 8e98780 (core, Android, NDK, i18n, renderer scala1/200, clippy con warning). Questo non equivale a clippy senza nuovi warning. Nessun comando Rust è stato eseguito dall'implementer per il triage.

**Conteggio:** le 33 righe `warning:` riferite al package Android comprendono **29 diagnostiche + 4 riepiloghi** lib/lib-test/esempi. Fra le diagnostiche: **12 preesistenti**, **9 nuove meccaniche**, **8 nuove legate a target/firme intenzionali**. I warning vendor rav1d restano esclusi da questa classificazione, senza sopprimerli. Il confronto è sul sorgente effettivo `git show 0a3a64b:<file>` / diff fino a8e98780, non su una presunta esecuzione clippy della baseline.

| Diagnostica nel log 8e98780 | Stato rispetto a 0a3a64b | Intervento |
| --- | --- | --- |
| ui/web.rs:32 suspicious_assignment_formatting | Nuova, file nuovo | `*v = !*v`, significato invariato |
| ui/rivedi.rs:289 suspicious_assignment_formatting | Nuova, fine_trascrizione nuova | `rv.live = !success` |
| ui/casa/telefono.rs:258 collapsible_match | Nuova nel layout A | Guardia sul ramo Att::Web/Att::Yt |
| ui/web.rs:70 manual_div_ceil | Nuova | `formats.len().div_ceil(2)` |
| ui.rs:1357 e1358 needless_return | Nuove azioni Labs/Pacchetto | Espressioni del match senza return |
| web.rs:50 manual_checked_ops | Nuova, modulo nuovo | checked_div conserva caso totale0 e saturating_mul |
| worker.rs:386 chunks_exact_to_as_chunks | Nuova, test nuovo | as_chunks::<2>().0.iter(), stessa esclusione resto |
| worker.rs:418 field_reassign_with_default | Nuova, test nuovo | Inizializza password/slide_level/micro_all_t nella struct; opzioni identiche |
| worker.rs:4/40/129/133/145/150 dead_code (6) | Nuovi item worker, intenzionalmente usati solo Android/test | cfg(any(target_os="android",test)) sui singoli item e impl Drop; host test conservati, niente allow(dead_code) globale |
| ui/web.rs:36 scelta_web too_many_arguments | Nuova firma helper UI intenzionale | allow locale esclusivamente sulla funzione, commento geometria/stato/hit-test espliciti; nessun refactoring |
| ui/labs.rs:152 lab_bottone too_many_arguments | Nuova firma helper UI intenzionale | allow locale esclusivamente sulla funzione, commento coordinate/etichetta/hit-test; nessun refactoring |
| ui/casa.rs:906 info_voce_riga too_many_arguments | Preesistente identica a baseline:823 | Nessuna modifica/soppressione |
| ui/casa.rs:937 principali type_complexity | Preesistente identica a baseline:854 | Nessuna modifica/soppressione |
| ui/casa.rs:966 riga_interruttore too_many_arguments | Preesistente identica a baseline:883 | Nessuna modifica/soppressione |
| ui/casa.rs:1326 pillola_piccola too_many_arguments | Preesistente identica a baseline:1243 | Nessuna modifica/soppressione |
| ui/m3.rs:86 pillola e:110 tonale too_many_arguments (2) | File integralmente invariato nel diff baseline→8e98780 | Nessuna modifica/soppressione |
| ui.rs:1596 if_same_then_else | Identico già a baseline:1464 | Nessuna modifica/soppressione |
| examples/vista.rs:44/61/92 ptr_arg (3) | Stesse firme &PathBuf alla baseline, stesse righe | Nessuna modifica/soppressione |
| examples/vista_cedro.rs:10 ptr_arg e:16 unused m (2) | Già presenti alla baseline, stesse righe | Nessuna modifica/soppressione |

Il mandato iniziale report-only è stato esteso esplicitamente dal controller dopo la copia del log: unica patch contenuta sui nuovi avvisi, nessun refactoring firme, nessuna modifica Java/servicefix4b2f686. Patch produzione: sette file Android, 21 righe aggiunte/14 rimosse, oltre al rapporto. Nessuna modifica di logica formati/SAF/player.

Verifiche locali dopo patch: `git diff --check` PASS; `python3 android/tools/check-i18n.py` PASS134×5, zero mancanze. Self-review dei cfg: CANCEL/emetti/Scratch e Drop/prepara_audio_web/converti_web/esegui restano compilati su Android e nelle regressioni host, esclusi solo dalla libreria host di rendering dove risultavano inutilizzati. **Clippy/check/test non rieseguiti localmente**; il controller deve verificare il nuovo checkpoint. Non si dichiara rimosso un warning prima della nuova esecuzione. Nessun blanket ignore, nessuna soppressione dei warning della baseline/vendor.
