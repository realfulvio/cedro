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
