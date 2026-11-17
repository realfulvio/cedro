# Audit mirato SAF: PDF, Office, decoder e processi esterni

Data 2026-11-17. Baseline HEAD verificato all'inizio: `4c67b2c`; l'implementer Task 6 ha iniziato modifiche WIP durante l'audit. I riferimenti iniziali di seguito descrivono quella baseline; non sono una nuova review del WIP. Nessun ADB, accesso remoto, build, sync o modifica di produzione effettuato. Il registry Cargo del notebook non contiene i sorgenti delle crate richieste: le firme API sono state verificate nelle fonti ufficiali sotto indicate.

## Risultati concreti comunicati all'implementer

### PDF: nessun Pdfium nell'ingresso analizzato

Il rendering usa **Hayro 0.9 / hayro-syntax 0.9.0**; la lettura/manipolazione PDF usa **lopdf 0.47.0**. I decoder ricevono già byte, quindi è l'ingresso per pathname prima del decoder a bypassare il nuovo registry SAF.

Punti baseline `4c67b2c`:

- `src/ui/media.rs:146`, `:201`, `:224`: `std::fs::read(path)` per miniature, pagina PDF e informazioni. Dopo la lettura, Hayro riceve i byte; non riapre il pathname esterno.
- `src/engine.rs:1398`, `:1452`, `:1512`, `:1520`, `:1527`: split/edit/compressione/info/sblocco PDF leggono con `std::fs::read`.
- `src/engine.rs:1902`, `:1979`, `:2018`, `:2105`, `:2124`: PDF→immagini/Word/Excel, fallback OCR e anonimizzazione usano anch'essi l'ingresso standard per pathname.
- `src/doctext.rs:367`: `lopdf::Document::load(path)` apre il file dentro la libreria, senza passare dal registry.

**Minimo intervento per correggere l'autorizzazione:** nei punti che già richiedono una sequenza completa di byte, sostituire solo la lettura dell'input con `input_file::read(path)`; questo mantiene il comportamento memoria esistente, senza introdurre una materializzazione privata su disco o una nuova copia di fallback del file. Per `doctext`, `Document::load_from(input_file::File::open(path)?)` è una vera alternativa API; `load_mem(&input_file::read(path)?)` evita analogamente la riapertura esterna e corrisponde al WIP comunicato dall'implementer.

**Non confondere autorizzazione e streaming:** `lopdf::load_from<R: Read>` legge internamente fino a EOF in un buffer prima del parsing (`reader.rs`, `load_internal`), quindi non rende il motore lazy o a memoria limitata. La correzione SAF non deve essere presentata come tale. [API lopdf 0.47.0](https://docs.rs/lopdf/latest/lopdf/struct.Document.html#method.load_from), [sorgente load_internal](https://docs.rs/lopdf/latest/src/lopdf/reader.rs.html#86-98)

Hayro `Pdf::new(data: impl Into<PdfData>)` richiede dati ad accesso casuale come byte, non un generico `Read+Seek`. `PdfData` accetta `Vec<u8>` oppure `Arc<T>` con `T: AsRef<[u8]> + Send + Sync + 'static` (feature std): non è stato trovato un ingresso Hayro di pathname da correggere dopo l'acquisizione dei byte. Una futura sorgente mmap sul descriptor autorizzato potrebbe usare l'API Arc senza copia di input, ma non è il minimo fix di autorizzazione e non è stata implementata/validata. [API PdfData 0.9.0](https://docs.rs/hayro-syntax/latest/hayro_syntax/struct.PdfData.html)

### Office: librerie reader disponibili, nessun renderer esterno da aggirare

Baseline:

- `engine.rs:509` legge il sorgente Office per il rendering PDF con `std::fs::read(path)`; `:1766` per fogli→testo/CSV/Excel ecc.; `:1884` per DOCX/ODT/HTML/MD/TXT.
- `ufficio::leggi_documento` / `leggi_foglio` accettano `&[u8]`. DOCX/XLSX/ODT/ODS creano già `ZipArchive<Cursor<&[u8]>>`; DOC usa CFB su Cursor. Una volta ricevuti questi byte, il rendering Office interno non apre nuovamente l'alias SAF. Correggere l'acquisizione dei byte è sufficiente per questa specifica barriera di autorizzazione.
- La lettura testuale legacy `doctext.rs:446`, `:462` usa `std::fs::File::open(path)` prima di `ZipArchive::new(file)`; `:670` usa lo stesso open prima di `cfb::CompoundFile::open(file)`. RTF usa `fs::read` (`:15`, `:54`).

**Minimo:** `input_file::File::open` nei tre ingressi legacy ZIP/CFB, `input_file::read` dove la funzione esistente opera su byte. Nessun file temporaneo privato richiesto, nessun cap aggiuntivo.

Le API sono reali e adatte al wrapper indipendente `Read+Seek`: `ZipArchive<R>::new(reader)` richiede `R: Read + Seek`; `cfb::CompoundFile<F>::open(inner)` richiede `F: Read + Seek`. L'attuale codice legacy passa il file senza annotazioni concrete incompatibili, quindi il cambio del tipo del reader è circoscritto. L'API cfb consultata è esattamente 0.16.0; la documentazione zip consultata è 9.6.1, mentre Cargo.lock blocca 9.5.1: la compatibilità di questa firma è coerente con l'uso già compilato `ZipArchive::new(file)` ma non è stata compilata la sostituzione. [ZipArchive API](https://docs.rs/zip/latest/zip/read/struct.ZipArchive.html#method.new), [cfb 0.16.0 API](https://docs.rs/cfb/latest/cfb/struct.CompoundFile.html#method.open)

### Audio/video Android: distinguere apertura iniziale e consumo diretto del fd

Baseline `src/mediadec.rs:84` e `:298`: decoder video e audio iniziano con `std::fs::File::open(path)`, poi chiamano già `AMediaExtractor_setDataSourceFd(ex, fd, 0, len)`. **Il primo open è il bypass**, non l'API MediaExtractor fd.

**Minimo:** acquisire il reader/lease tramite `input_file::File::open`; esporre in modo esplicito un fd preso in prestito per questa API nativa, mantenendo il proprietario vivo (`_file` esiste già). Non trasformare quel fd in pathname `/proc/self/fd/N` prima di passarlo alla libreria. L'implementer ha comunicato il corrispondente WIP `AsRawFd` diretto.

La distinzione è verificata nei sorgenti AOSP: `AMediaExtractor_setDataSourceFd` passa il fd numerico a `NuMediaExtractor::setDataSource`; quest'ultimo crea `FileSource(dup(fd), offset, size)`. Il costruttore fd di FileSource conserva quel fd e usa fstat; non esegue `open(filename)`. È quindi un consumo del descriptor già autorizzato, non una riapertura FUSE per pathname. La firma NDK è disponibile da API21, compatibile con minSdk26. [NDK API](https://developer.android.com/ndk/reference/group/media#amediaextractor_setdatasourcefd), [NdkMediaExtractor.cpp](https://android.googlesource.com/platform/frameworks/av/+/refs/heads/main/media/ndk/NdkMediaExtractor.cpp#85), [NuMediaExtractor.cpp](https://android.googlesource.com/platform/frameworks/av/+/refs/heads/main/media/libstagefright/NuMediaExtractor.cpp#131), [FileSource.cpp](https://android.googlesource.com/platform/frameworks/av/+/refs/heads/main/media/libdatasource/FileSource.cpp#55)

**Caveat API dimostrato, non guasto runtime osservato:** FileSource AOSP implementa readAt con lseek64+read e mutex per singolo FileSource, non pread. I `dup` condividono l'offset della stessa open file description; due extractor contemporanei costruiti dalla medesima lease richiedono attenzione, mentre i reader Rust del wrapper usano pread e non dipendono dall'offset. Non dichiarare che il solo AsRawFd fornisca cursori nativi indipendenti. Nel percorso Android→MP4 locale controllato (`linux_video.rs:36–73`) video e audio sono letti in sequenza; non è stata verificata ogni possibile combinazione di preview/lavori concorrenti. [FileSource::readAt_l](https://android.googlesource.com/platform/frameworks/av/+/refs/heads/main/media/libdatasource/FileSource.cpp#128)

L'API alternativa `AMediaExtractor_setDataSourceCustom` accetta AMediaDataSource e callback posizionali, ma parte da API28: non è una sostituzione incondizionata per minSdk26 e non è necessaria per eliminare il primo open negato. [NDK custom source](https://developer.android.com/ndk/reference/group/media#amediaextractor_setdatasourcecustom)

`symph::open` nella baseline usa già `input_file::File` e `MediaSourceStream`; quindi audio/container Symphonia mantengono Read+Seek del wrapper. Il probe WebM `softvideo.rs:573` era un ulteriore `std::fs::File::open` di pochi MB, correggibile con il wrapper; l'implementer ha comunicato la correzione WIP. Gli encoder MediaCodec ricevono frame/sample e non è emersa una riapertura dell'input in `mediacodec.rs`.

### Processi esterni: residuo concreto nel fallback trascrizione

`src/trascrizione.rs:1506–1547`, funzione `system_decode`:

- ramo `#[cfg(not(windows))]` **include Android**;
- `:1524`: `Command::new(ffmpeg_bin)... -i input` consegna il pathname al processo esterno;
- `:1537`: fallback GStreamer `filesrc location=<input>` consegna anch'esso il pathname;
- `:1906`: il percorso è realmente raggiungibile dopo `Outcome::Unsupported` nel flusso trascrizione.

Questi figli non conoscono il registry Rust del processo chiamante. L'alias `/proc/self/fd/N` rimane una nuova apertura per pathname anche se il fd N è ereditato: non risolve la barriera FUSE dimostrata nel test Labs. Il ramo YouTube usa ffmpeg tramite yt-dlp per URL e file scaricati nella cartella privata: non è stato individuato lì un ingresso arbitrario del fixture SAF locale. Non proporre di cambiare quel flusso senza un caso concreto.

**Alternativa esterna reale:** FFmpeg dispone del protocollo `fd:` con opzione `-fd N`, che consuma un fd già aperto e conserva seek sui file regolari; `pipe:` non fornisce la stessa proprietà. Perciò non raccomandare come fix universale `-i pipe:0`, né presumere che MP4/MOV o PDF funzionino in stdin non seekable. [FFmpeg fd protocol](https://ffmpeg.org/ffmpeg-protocols.html#fd), [FFmpeg pipe protocol](https://ffmpeg.org/ffmpeg-protocols.html#pipe)

**Proposta minima condizionata:** il chiamante deve conservare la lease e passare un descriptor autorizzato ereditabile al figlio, scegliendo l'ingresso `fd:` soltanto se il binario specifico include quel protocollo. Il protocollo è una capacità del build FFmpeg: la documentazione upstream non prova quella del pacchetto Android fissato da Cedro, che qui non è stato eseguito. L'eventuale fd trasferito come stdin deve rimanere un file regolare ed essere usato da protocollo `fd`, non trasformato in una pipe; il helper attuale `wait_or_cancel` forza stdin a null e andrebbe adattato intenzionalmente. Questo evita copie complete, ma occorre inoltre gestire l'offset condiviso di dup/lseek per lettori esterni concorrenti: il wrapper pread non si estende automaticamente ai processi esterni. Non è stato identificato nel GStreamer `filesrc` esistente un ingresso diretto equivalente già configurato.

Non è proposta una copia privata completa come fallback, né la rimozione della funzione. L'assenza di un FFmpeg fd protocol verificato o di un trasporto con offset sicuro resta un requisito concreto per completare questo ramo, non un motivo per dichiararlo già corretto.

## Verifica API aggiuntiva richiesta dall'implementer: JPEG XL

`imgfmt.rs:296` baseline chiamava `JxlImage::builder().open(path)`. **Alternativa verificata in jxl-oxide 0.12.6:** `JxlImage::builder().read(reader)` accetta un reader `Read`; usare direttamente `input_file::File::open(path)` senza materializzare una copia privata. La documentazione dello stesso crate mostra esplicitamente sia l'ingresso pathname sia quello reader. Non usare riferimenti ai fork `ps-jxl-oxide` o `malmal-jxl-oxide` come prova dell'API upstream di Cedro. [jxl-oxide 0.12.6](https://docs.rs/jxl-oxide/latest/jxl_oxide/)

## Limite e handoff

I punti sopra sono stati comunicati direttamente a `/root/android_finish` e al controller. L'implementer ha dichiarato WIP su engine/doctext/mediadec/ui/media/softvideo e altri ingressi; questo audit non ne certifica compilazione o runtime e non duplica la review generale. Non sono state eseguite conversioni, nuove prove sul telefono, comandi remoti o test. Restano da provare runtime il consumo fd MediaExtractor nel build concreto e il ramo esterno FFmpeg se si sceglie di modificarlo.
