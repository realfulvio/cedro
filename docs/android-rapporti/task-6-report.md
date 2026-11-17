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
