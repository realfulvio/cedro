# Task 4 — Video dal web Android

Stato implementazione: DONE_WITH_CONCERNS. Il candidato finale richiede compilazione, test e review del controller; nessun PASS del checkpoint intermedio è attribuito automaticamente al candidato finale.

## Cambiamenti

- `android/src/web.rs`: snapshot Scelte (risoluzione, qualità audio, formati, sottotitoli, copertina, capitoli, metadati, divisione, sponsor, normalizzazione, raccolta, duplicati), distinto da qualità media e Voce. Request effettiva condivisa con il core.
- `android/src/ui/web.rs` e integrazione nella variante A: qualità a tessere con consigliata, formati e tre gruppi; campo link/Incolla già presenti conservati e aggiunto Svuota. Altezza delle tessere qualità calcolata sul testo tradotto. Le opzioni del provider non disponibili rimangono visibili ma disabilitate con spiegazione. Per i generici il motore applica `video_height`/conversione audio dopo lo scarico; Originale mantiene i byte e il formato del sito. M4A/MKV/WebM non vengono promessi per i generici perché quel percorso core espone MP4 e MP3/Opus/WAV/FLAC.
- `Request.metadata` nel core, default true invariato sul desktop; flag yt-dlp effettivo. Recupero 429 tramite il percorso core esistente e nota conservata nel risultato Android. Il test fake yt-dlp esistente è esteso per provare il retry senza sottotitoli.
- Osservatore dei componenti senza thread di polling core, eventi IPC verso il genitore e stato per ID; errori con scadenza 12 secondi, reset fine/cancel/crash. UI e notifica leggono lo stesso stato; il mero lavoro attivo non richiede più redraw continuo. Progressi atomici confrontati nel loop utile; notifica aggiornata solo quando testo/percentuale cambiano.
- `glue/web.rs`: JNI per rete a consumo e spazio, senza nuovo Java. `ACCESS_NETWORK_STATE` giustificato nel manifest/README. Default Chiedi una volta su rete mobile, scelta salvata atomicamente; Solo Wi-Fi e Consenti anche rete mobile. Il worker usa IPC duplex per la policy. Per 100+ MB, dimensione e spazio sono informativi (nessun tocco extra); spazio insufficiente segnala quantità e richiesta. Ricontrollo durante i componenti al massimo ogni secondo; attesa cancellabile, senza busy polling, quando la rete a consumo non è consentita.
- Maven Python/ffmpeg alimentano la riga componenti; Range riparte dal byte mancante anche se la risposta si interrompe a metà. yt-dlp Android riusa il downloader dei modelli per retry/Range/checksum. Un errore definitivo Maven non conserva un indice ZIP persistente tra riavvii: la ripresa aggiunta copre il ritorno della rete durante il download; i `.part` dei modelli/yt-dlp conservano la ripresa già offerta dal core.
- Cancel worker: SIGUSR1 con solo AtomicBool nel signal handler, grazia 1,5 secondi, poi terminazione del gruppo. I nuovi parziali sono reclamati con create_new e registrati via IPC con ACK; cleanup soltanto sui percorsi registrati e device/inode corrispondenti. Nessuna scansione per differenza, nessuna rimozione di modelli o parziali preesistenti/altrui. Scratch e audio di revisione restano distinti.
- Naming del core applicato una sola volta anche al download passthrough e a tutti i risultati yt-dlp, inclusi sidecar/capitoli, preservando le sottocartelle.
- «Evita copie già salvate»: firma privata `.web-request` con link/scelte/caso, esclusi flag duplicati e modalità Exists. Snapshot mode per lavoro, globale invariato. I journal `.pronti` costituiscono l'indice: nomi relativi e forma devono corrispondere prima di riusare namespace/URI; il provider verifica esistenza e `is_pending=0`. Prenota/completato precedono la rimozione della copia privata anche nel riuso. Un file eliminato si ripubblica. Nessun archive yt-dlp cieco, nessuna promessa di risparmio dati: il download/trascrizione possono avvenire comunque.
- Log genitore: avvio effettivo e fine dopo journal/pubblicazione, correlati per ID e azione; esito completo/parziale/errore/annullato e numero uscite. Nessun nome privato, contenuto o URI nei nuovi eventi diagnostici.
- Traduzioni en/de/es/fr/pt, incluso fix di integrazione `Anteprima` richiesto dal controller. Nessun layout Labs o tema desktop cambiato.

## Handoff Task 5 — raccolta → trascrizioni

`web::Batch` conserva Options e Voce originali. `Figlio.identita.chiave` è `<chiave batch>/audio-<indice>`, indipendente dall'ID UI. Root/.batch viene committato prima di qualsiasi rename; ogni figlio root/.figli/N contiene `.lavoro`, `.batch-child`, `.revisione-source` e `.revisione/audio`. Il marker `.web-request` dei figli deriva dalla firma del batch e dall'indice.

`Completamento.batch` trasporta insieme stato download e figli. Il main assegna soltanto ID/stato in memoria; I/O, materializzazione e avvio seriale avvengono nei thread di lavoro con il permit esistente. Nel recupero, `Batch.ricevute` (solo in memoria, escluso dalla serializzazione) porta le ricevute recuperate nello stesso evento dei figli: non ci sono due code che possono sovrascrivere uno stato figlio con un risultato vecchio. Le ricevute sono applicate prima della decisione di avvio. La doppia consegna non avvia due motori, i tombstone impediscono reimportazione dopo rimozione cronologia. Un annullamento del figlio non elimina gli audio degli altri. Il marker `.batch-child` protegge l'audio anche prima del primo salvataggio whisper.

Correzione della prima diagnosi sul permit: `resources::acquire` è rientrante per thread; non è dimostrato un deadlock nel vecchio passaggio YouTube→trascrivi sullo stesso thread. Il rilascio prima dell'handoff è necessario per il nuovo passaggio ad altri thread. Nessun bug precedente non provato viene rivendicato.

## Verifiche eseguite dall'implementer

Vincolo rispettato: nessun cargo, rustfmt, fmt, build Rust, adb o carico remoto avviato dall'implementer. Nessun pm clear/disinstallazione/cancellazione modelli. Non modificati PROMPT-CODEX-tracciadoc.md e foto-riga/.

- `git diff --check` e `git diff --cached --check`: exit 0, nessun output (controllo finale ripetuto prima del commit).
- Python `xml.etree.ElementTree.parse('android/AndroidManifest.xml')`: `Manifest XML: valido`.
- Python estrae dalla sorgente la stringa della fixture 429 e invoca `/bin/sh -n`: `Fixture shell 429: sintassi valida (nessun yt-dlp eseguito)`. Questo NON è il test Rust del retry.
- Controllo Python delle nuove chiavi di integrazione nelle cinque TSV: per en/de/es/fr/pt, `chiavi integrazione presenti, Anteprima unica`.
- Self-review del diff: corretti altezza tessere, selezione Originale generica, raccolta di sidecar/capitoli, ownership parziali, errori cancel prematuro, priorità delle ricevute durante recovery batch e selezione di namespace compatibile.

## Evidenza riferita dal controller, solo checkpoint aefda8d

- NDK all-targets con YouTube: PASS, 19,42 s. Log `/srv/dev/scratch/cedro-android-aefda8d-ndk.log`.
- Host Android: 33/33 PASS, build 7m32s, test 0,86s. Il controller ha comunicato il risultato, non l'invocazione completa; nessuna invocazione viene inventata qui.
- Core standalone senza feature android-worker: FAIL compilazione E0433 su `src/componenti.rs:7`, serde opzionale non abilitato. Log `/srv/dev/scratch/cedro-android-aefda8d-core-gruppi.log`, righe 743–753. Corretto nel candidato finale con `cfg_attr(feature="android-worker", derive(...))`, stesso schema di engine.rs, senza dipendenze nuove. Correzione NON ancora compilata dall'implementer.
- Warning del test batch iniziale sovrascritto: ripulito nel candidato finale.
- Nessun PASS senza YouTube, core MRXS/i18n, renderer Task4 o telefono è attribuito senza comunicazione/verifica. I 156 PNG af4 appartengono alla base Task3, non sono evidenza del nuovo pannello web.

## Test implementati e da eseguire sul candidato finale

- UI→snapshot→Request, risoluzione selezionata, Voce/qualità media indipendenti, delega senza YouTube.
- Stato serializzato componente→genitore, errore/scadenza/fine/cancel e percentuale usata anche dalla notifica.
- Naming caso del download e sottocartelle.
- Batch due audio, piano durevole prima dei rename, crash prima del marker, materializzazione ripetuta, associazione sorgente, isolamento fine/cancel, doppia consegna UI, restart e tombstone.
- Policy rete/spazio; cleanup preserva modello completo, parziale preesistente e un file creato da altro job, anche se sostituisce il path originariamente registrato.
- Due lavori reali di pubblicazione legacy con playlist/capitoli/sidecar, primo con switch spento e secondo acceso: stessi path/namespace, tre file totali; dopo eliminazione di uno, terzo lavoro lo ricrea. Provider simulato pending/non leggibile non è riusato; firma mancante non produce falso skip. Il controllo URI MediaStore reale resta al Mi9T.
- Fake yt-dlp 429 senza rete, fallback senza sottotitoli e output presente; flag metadata disattivato.
- `vista_a` esteso con nove scene web YouTube/generico, qualità/opzioni/raccolte, componente progresso/errore e policy. Fixture esclusivamente nell'example. Scene in IT/DE/PT alle tre dimensioni già previste (compresi 360×640 e 800×400).

Comandi proposti al controller, NON eseguiti dall'implementer:

```sh
cargo check --release -j2 -p cedro --lib
cargo test --release -j2 -p cedro-android --lib --features youtube
cargo test --release -j2 -p cedro-android --lib
cargo test --release -j2 -p cedro --lib youtube::tests::runs_a_fake_ytdlp
cargo run --release -j2 -p cedro-android --features youtube --example vista_a -- /srv/dev/scratch/vista-task4
```

NDK con/senza YouTube e comandi specifici del corpus/range-loopback restano sotto coordinamento del controller. Prove richieste prima della release: risoluzioni ffprobe, web→Testo singolo/raccolta, componenti presenti/primo uso isolato senza cancellare dati, ritorno Wi-Fi, cancel CPU idle, notifica background, MP4 troncato/durata impossibile, URI eliminato/pending e ripetizione download. Tutte NON ESEGUITE dall'implementer.

## File e limiti

File principali: android/src/web.rs, android/src/ui/web.rs, android/src/glue/web.rs; integrazione in ui.rs, ui/casa.rs, ui/casa/telefono.rs, ui/labs.rs (solo literal Completamento nel test), glue.rs, worker.rs e lib.rs. Core: componenti.rs, youtube.rs, trascrizione.rs, lib.rs (export choose_output). Manifest, cinque TSV, README, note-release-0.53.0.md ed example vista_a pertinenti.

Rischi/gate: codice JNI, provider MediaStore e protocollo nuovo richiedono runtime; le prove sul candidato finale sono ancora da fare. Il riuso dei journal è una scansione dei journal privati, senza archivio generale nuovo; non è stato misurato con una cronologia molto grande. Maven riprende la risposta interrotta durante il lavoro, ma non conserva uno stato ZIP persistente dopo terminazione definitiva. La disponibilità dei sottotitoli reali dipende dal sito; il caso 429 deterministico è coperto dalla fixture da eseguire. Nessuna garanzia dichiarata su RAM/tempo/frame/telefono prima dei test del controller.

## Fix round 1 — I1, I2, I3 e I4 (base 46ec8ea)

Implementazione completata e sottoposta a self-review statica. I test Rust seguenti sono scritti ma NON eseguiti dall'implementer; il controller li esegue dopo questo commit e fornirà l'evidenza prima della review circoscritta. Nessun cargo/rustfmt/fmt, adb o carico remoto avviato. M2 rimane assegnato al Task6.

### Correzioni e copertura

- **I1, ricevute batch e timestamp:** `Batch::recupera_ricevute` legge esplicitamente le directory dei figli definite dal manifest e controlla la chiave stabile. Non dipende dal cutoff della scansione generale, anche dopo i rename atomici di `materializza`. Il callback di produzione è `glue::recupera_registro`, estratto dalla riconciliazione già esistente (stessi commit legacy/MediaStore). Le ricevute generali dei figli sono rimosse dalla coda separata; continuano a viaggiare nello stesso completamento batch. Il ramo `.pronti` fra registrazione e avvio ora invia sempre `Completamento::recuperato` o un errore esplicito se il registro è invalido. La costruzione del completamento è condivisa con `App::recupera`, senza I/O nel loop UI. Test `ui::web::tests::recupero_batch_con_mtime_riscritto_e_ricevuta_pronta`: due audio reali; pubblicazione legacy e commit `.pronti`; snapshot UI precedente al commit; cutoff SystemTime reale, materializzazione e mtime successivo al cutoff; recupero esplicito, URI/path e sorgente corretti, solo il secondo figlio riparte, doppia consegna e completamento tardivo non restano InCorso.
- **I2, omonimi non compatibili:** `Salvataggio::nuovo` conserva sempre la policy `Options.exists` nello snapshot `mode`. Il toggle controlla soltanto `riusi`, che verifica firma, relativi e provider; non abilita più Skip per nome nel fallback. Test `web::salvataggio_tests::richieste_diverse_con_stesso_nome_pubblicano_i_propri_byte`: tre pubblicazioni reali dello stesso nome, prima 480p con toggle spento, poi 1080p con toggle acceso, poi altro link omonimo. Verifica tre path distinti e i byte di ciascuno, senza sostituzione della prima/seconda uscita. Rimangono i test precedenti di richiesta identica, raccolte, sidecar, URI/file eliminato e pending. Replace e Skip espliciti dell'utente restano rispettati.
- **I3, audio generico → Testo:** `worker::prepara_audio_web` chiama davvero `cedro::perform_with` prima della trascrizione. Il formato scelto, la qualità mappata su 95/82/55 e normalize agiscono sul file conservato in `.revisione`; il marker `.revisione-source` viene committato prima della trascrizione. La copia di Options per la preparazione rimuove soltanto naming/caso, override bitrate e conflitti del privato; le Options originali rimangono per il testo finale. Nessun codec o dipendenza nuova. Test `worker::tests::audio_web_testo_preparato_rispetta_formato_qualita_volume`: PCM sintetico effettivo, WAV con picco ~2000 o ~32000 secondo normalize, MP3 massima/leggera decodificabili dal core e di dimensione diversa, marker corretto, nome caso applicato una volta, cleanup scratch senza rimuovere gli audio preparati. Questo test sostituisce la vecchia fixture che spostava soltanto byte finti con `sposta_download`; helper obsoleto rimosso.
- **I4, cancel Maven su connessione aperta:** `src/youtube/range.rs` contiene la sola lettura Remote estratta, compilata per Android e per i test host. L'agent desktop generale è invariato. Connect e durata globale di ogni richiesta Maven sono limitati a 2 s; il token è verificato prima/dopo HEAD e dopo gli errori GET/read, con risposta `Annullato`. Le attese fra tentativi controllano il token ogni 100 ms. Il prefisso ricevuto rimane nel buffer e il Range successivo parte dal primo byte mancante; se sono arrivati byte si azzera il budget degli errori senza progresso, evitando l'esaurimento dei tentativi su una rete lenta. Nessun thread di download staccato. Test reali sul loopback: `youtube::range::tests::maven_annulla_head_senza_risposta`, `maven_annulla_get_senza_headers`, `maven_annulla_body_fermo` misurano <5 s dall'annullamento mantenendo aperto il socket fino al risultato. `maven_range_riprende_byte_e_non_esaurisce_retry_se_avanza` tronca nove risposte successive, verifica dieci Range esatti, contenuto finale e seek sulla cache. I server fixture hanno attese finite e i thread sono ricongiunti.

### Comandi e risultati

Eseguito localmente: `git diff --check`, exit 0, nessun output. `git diff --cached --check` dopo lo staging puntuale: exit 0, nessun output. Self-review: letti diff completo dei sei file sorgente modificati e del modulo nuovo, controllati gli ownership dei byte, la riconciliazione condivisa e il collegamento worker → preparazione → marker → trascrizione. Non è una prova di compilazione.

Comandi proposti al controller sul candidato di questo round, NON ESEGUITI dall'implementer (il controller può usare il profilo host già predisposto con override O3 di fdk-aac-sys):

```sh
cargo check --release -j2 -p cedro --lib
cargo test --release -j2 -p cedro-android --lib --features youtube recupero_batch_con_mtime_riscritto_e_ricevuta_pronta
cargo test --release -j2 -p cedro-android --lib --features youtube richieste_diverse_con_stesso_nome_pubblicano_i_propri_byte
cargo test --release -j2 -p cedro-android --lib --features youtube audio_web_testo_preparato_rispetta_formato_qualita_volume
cargo test --release -j2 -p cedro --lib youtube::range::tests
```

Nessun output Rust o PASS di questi comandi è disponibile al momento della consegna. Rimangono NDK con/senza YouTube e il controllo host del candidato sotto coordinamento del controller. Risultati comunicati per la SOLA base 46ec8ea: core standalone check PASS 142,32 s; NDK con YouTube PASS 20,27 s e senza PASS 3,97 s; host all-targets YouTube check PASS 18,26 s. Il primo tentativo test opt1 ha fallito nella compilazione C di fdk-aac-sys (GCC always_inline/O1), prima di eseguire test applicativi; il controller ha comunicato la ripartenza con override O3 del package. Queste evidenze non convalidano le modifiche del round.

### Limiti e interfacce conservate

Il limite temporale Maven riguarda socket già aperti; il DNS nativo di ureq 2.12.1 non può essere interrotto dalle API disponibili e può eccedere il timeout globale (documentazione `AgentBuilder::timeout`, sorgente ufficiale `https://github.com/algesten/ureq/blob/2.12.1/src/agent.rs`). Nessun claim di cancel ≤5 s nel pre-connect/DNS non misurato; limite esplicitamente accettato dal controller per questo round. I test di stallo e Range sono ancora da eseguire. JNI/provider reale, preparazione audio e annullamento sul Mi9T restano gate runtime.

Handoff Task5 invariato: manifest batch, identità stabile per figlio, Options/Voce e marker di revisione. Nuova API in memoria `Completamento::recuperato` consente al worker di consegnare una ricevuta già pronta senza ripartire o lasciare un lavoro attivo. `Batch::recupera_ricevute` va chiamato nel worker con riconciliazione del provider, mai dal loop UI.

File del round: `android/src/glue.rs`, `android/src/ui.rs`, `android/src/ui/web.rs`, `android/src/web.rs`, `android/src/worker.rs`, `src/youtube.rs`, nuovo `src/youtube/range.rs`, questo rapporto. Il report Task3 e gli untracked dell'utente sono lasciati fuori dallo staging.

## Validazione riferita dal controller su db3a1a5 — 17 novembre 2026

Aggiornamento del solo rapporto: nessuna modifica ai sorgenti, nessun commit o comando cargo eseguito dall'implementer. Le esecuzioni seguenti sono state effettuate dal controller sul mini-PC, sulla copia immutabile di `db3a1a5`.

- Core standalone, inclusa compilazione dei test Range: `cargo check --release -j2 -p cedro --tests`, **PASS**, 19,87 s. È compilazione, non esecuzione della regressione I4.
- NDK all-targets con YouTube: **PASS**, 21,47 s.
- NDK all-targets senza YouTube: **PASS**, 3,87 s.
- Host Android all-targets con YouTube: **PASS**, 19,71 s.
- Test Android senza YouTube: **PASS 40/40**, 0 falliti; compilazione 5m00s, test 1,64 s, step totale 302,33 s. Log `/srv/dev/scratch/cedro-android-db3a1a5-task4-aac3-host-test.log`.
- Test Android con YouTube: **PASS 40/40**, 0 falliti; compilazione 18,30 s, test 1,70 s, step totale 20,12 s. Log `/srv/dev/scratch/cedro-android-db3a1a5-task4-aac3-host-test-youtube.log`.

Comandi effettivi dei test comunicati dal controller (override O3 limitato a fdk-aac-sys nel profilo di test; profilo release di produzione invariato):

```sh
env RUST_TEST_THREADS=2 CARGO_TARGET_DIR=/srv/dev/scratch/cedro-android-target-test CARGO_PROFILE_TEST_OPT_LEVEL=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_LTO=false CARGO_PROFILE_TEST_CODEGEN_UNITS=16 CARGO_PROFILE_TEST_INCREMENTAL=true nice -n10 cargo --config profile.test.package.fdk-aac-sys.opt-level=3 test -j2 -p cedro-android --lib
env RUST_TEST_THREADS=2 CARGO_TARGET_DIR=/srv/dev/scratch/cedro-android-target-test CARGO_PROFILE_TEST_OPT_LEVEL=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_LTO=false CARGO_PROFILE_TEST_CODEGEN_UNITS=16 CARGO_PROFILE_TEST_INCREMENTAL=true nice -n10 cargo --config profile.test.package.fdk-aac-sys.opt-level=3 test -j2 -p cedro-android --lib --features youtube
```

Estratti nominali dell'output comunicato, presenti in entrambe le esecuzioni:

```text
ui::web::tests::recupero_batch_con_mtime_riscritto_e_ricevuta_pronta ... ok
web::salvataggio_tests::richieste_diverse_con_stesso_nome_pubblicano_i_propri_byte ... ok
worker::tests::audio_web_testo_preparato_rispetta_formato_qualita_volume ... ok
40 passed; 0 failed
```

Questi risultati coprono le regressioni host I1–I3 sul candidato, senza sostituire la review circoscritta o la prova sul Mi9T. **I4 rimane aperto**: compilati i test, ma esecuzione Maven HEAD/headers/body/Range ancora PENDING. Anche core MRXS, alias, 429, i18n e renderer sono PENDING al momento di questo aggiornamento. Nessun PASS viene attribuito loro per inferenza.

Invocazioni compile effettive confermate dal controller dal suo helper `/home/accorsiluca/prove-cedro-android/verifica-task4.py`, con `RUST_TEST_THREADS=2` e cwd remoto isolato:

```sh
nice -n 10 cargo check --release -j2 -p cedro --tests
nice -n 10 bash /srv/dev/scratch/cedro-check-ndk.sh --features youtube
nice -n 10 bash /srv/dev/scratch/cedro-check-ndk.sh
nice -n 10 cargo check --release -j2 -p cedro-android --all-targets --features youtube
```

Il wrapper NDK remoto corrisponde a `/home/accorsiluca/prove-cedro-android/check-ndk.sh` secondo il controller; le impostazioni NDK e l'ulteriore nice interno sono lasciati al wrapper effettivamente eseguito, senza ricostruire variabili non osservate. Controllo locale del solo rapporto: `git diff --check -- .superpowers/sdd/ESECUZIONE/task-4-report.md`, exit 0, nessun output.

### Esiti core del fix db3a1a5 — aggiornamento del controller

Esecuzioni riferite dal controller sul medesimo candidato e con lo stesso ambiente test opt1/AAC3 già riportato: `RUST_TEST_THREADS=2`, `CARGO_TARGET_DIR=/srv/dev/scratch/cedro-android-target-test`, `CARGO_PROFILE_TEST_OPT_LEVEL=1`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_PROFILE_TEST_LTO=false`, `CARGO_PROFILE_TEST_CODEGEN_UNITS=16`, `CARGO_PROFILE_TEST_INCREMENTAL=true`. Nessun test, modifica ai sorgenti o commit eseguito dall'implementer in questo aggiornamento.

```sh
nice -n10 cargo --config profile.test.package.fdk-aac-sys.opt-level=3 test -j2 -p cedro --lib youtube::range::tests
nice -n10 cargo --config profile.test.package.fdk-aac-sys.opt-level=3 test -j2 -p cedro --lib youtube::tests::runs_a_fake_ytdlp
nice -n10 cargo --config profile.test.package.fdk-aac-sys.opt-level=3 test -j2 -p cedro --lib i18n::tests
```

- `youtube::range::tests`: **PASS 4/4**, 0 falliti, 591 filtrati; test 11,12 s, build 0,26 s, step 11,49 s. Log `/srv/dev/scratch/cedro-android-db3a1a5-task4-aac3-core-range.log`.
- `youtube::tests::runs_a_fake_ytdlp`: **PASS 1/1**, 0 falliti, 594 filtrati; test 0,03 s, build 0,25 s, step 0,37 s. Log `/srv/dev/scratch/cedro-android-db3a1a5-task4-aac3-core-youtube.log`. Copre la fixture yt-dlp con retry 429 già descritta.
- `i18n::tests`: **PASS 5/5**, 0 falliti, 590 filtrati; test 0,02 s, build 0,25 s, step 0,37 s. Log `/srv/dev/scratch/cedro-android-db3a1a5-task4-aac3-core-i18n.log`.

Esiti nominali Range comunicati:

```text
youtube::range::tests::maven_annulla_get_senza_headers ... ok
youtube::range::tests::maven_annulla_body_fermo ... ok
youtube::range::tests::maven_annulla_head_senza_risposta ... ok
youtube::range::tests::maven_range_riprende_byte_e_non_esaurisce_retry_se_avanza ... ok
4 passed; 0 failed; 591 filtered out
```

**I4: copertura host PASS.** I tre test di stallo verificano il ritorno entro 5 s dall'annullamento con socket loopback già aperto; il quarto verifica ripresa Range con progresso e contenuto finale. Il risultato non si estende alla risoluzione DNS nativa o al telefono. La review circoscritta del fix rimane da svolgere; integrazione `i18n_inglese` e renderer sono ancora in coda, senza esito disponibile in questo aggiornamento. Nessuna chiusura implicita degli altri gate runtime.

### Ultimi gate host e rendering — controller, db3a1a5

Esecuzioni completate: integrazione `i18n_inglese` PASS (step132,36s) e renderer `vista_a` PASS (step26,83s). Il renderer usa profilo dev opt1/debug0/LTOoff/codegen16/incrementaltrue e override fdk-aac-sys O3, con cache separata: i tempi di disegno non sono misure delle prestazioni dell'APK. La coda seriale termina con exit0; TSV e log archiviati in `~/Documenti/cedro-android-anteprima/verifiche/db3a1a5`.

Prodotti237PNG per12.097.215byte, conservati in `~/Documenti/cedro-android-anteprima/render-rust/task4-db3a1a5`. Controller ha ispezionato: IT360×800 web-youtube e web-youtube-opzioni; DE360×640 web-youtube-qualita e componente-errore-rete; PT800×400 web-generico-opzioni; IT360×640 web-youtube-raccolte e componente-progresso; PT360×640 web-generico-qualita. Verificati nel campione wrapping, qualità, gruppi, spiegazione della conversione generica, policy, stato componente con35%/byte e messaggio errore. La scena raccolte a offset1820 mostra soprattutto il piede nel primo frame e non prova la visibilità dei due toggle: cattura da rifinire e gesto da verificare sul telefono (Task6).

Re-review round1 approvata, I1–I4 addressed, nessun nuovo finding nel fix. Nessun PASS host/render sostituisce i gate Mi9T, MediaStore, Wi-Fi, PSS/CPU o MP4 corrotto. APK intermedio in build sul mini-PC al momento di questo aggiornamento, non ancora installato.

## Runtime intermedio controller17novembre db3a1a5/5299

GET localhost/fermo.mp4: serverinvio65536Bpoistallo27.28s, Annulla→workeruscito2.014sdalcomandoADB, logjob34web-videoannullatouscite0; peercloserilevato, reverse/serverterminati. Non è prova Maven/DNS. URLpubblicoW3C https://media.w3.org/2010/05/sintel/trailer.mp4 viaSENDtext/appaperta→Video: job36completo1output9.777s,PSSaggregatocampionato219941KiB. Outputtrailer.mp4 pubblicatoMediaStoremimevideo/mp4/_size6248964; ffprobeh264864x480+AAC52.208333s, valido. Opzioniprecedentipersistite, non claimbyteidenticoalsorgente né qualitàoriginale. Evidenze in Documenti/cedro-android-anteprima/prove-mi9t/anteprima-db3a1a5/web-w3c-video eweb-stall-cancel.json. UIAregressioneoldnav/scroll assegnataTask6; YouTube/playlist/RangeWiFiphonerealiancorapendenti.

YouTubeintermedio: SENDMeatthezoo(jNQXAC9IVRw)→VideoesitoFatto, aggiornatoreyt-dlpvisibileprima, outputpubblicato964169B, filevalidatoinyoutub-video/ffprobe.json. Nessuntempo/PSSmisuratoperquestogiro. Nome-2numeraedevitacollisioneconfilepreesistente. Nonprovaaltrequalità/testo/playlist.
