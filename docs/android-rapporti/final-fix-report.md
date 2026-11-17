# Unica fix wave finale Android

Base `b4d03e7`. Ambito: I1/I2/M1 della review finale e I3 del finding fisico controller. Nessun subagent, comando Rust, compilazione, accesso remoto o adb eseguito dall'implementatore. Gli altri rapporti preesistenti e i file estranei restano fuori dal commit.

## Correzioni

- **I1:** «Chi parla» usa una viewport con scorrimento reale di lettore, schede e voci, anche in landscape; estensione misurata, clamp e clipping dei bersagli coincidente col disegno. Campi e azioni hanno dimensioni basate sul font e almeno 48 dp quando interamente visibili. Il testo del toggle e la spiegazione vanno a capo. Il foglio di assegnazione/unione ha un proprio offset e limite: i gesti muovono il foglio senza muovere la pagina; le righe e i relativi hit sono ritagliati sotto l'intestazione. Ogni apertura azzera lo scroll del foglio. Nessuna funzione rimossa. Il limite preesistente del motore rimane otto voci: «Voce nuova» è raggiungibile anche a otto, ma non crea una nona voce.
- **I2:** il ramo `Sorgente::Originale` richiama `storage::leggi_originale`, che apre tramite `cedro::input_file::File` e conserva il ciclo a blocchi da 64 KiB, la cancellazione e l'errore di allocazione. Il test esercita questa funzione produttiva su un originale di 32 MiB + 17 byte, con grant registrato il cui alias non può essere riaperto come pathname; verifica l'intero contenuto, il cursore logico di un secondo lettore, il cursore del descriptor originale e l'annullamento.
- **M1:** mappa MIME estratta senza altri cambiamenti in `storage` per provarla su host; tutti i consumatori in `glue` usano la medesima funzione. Aggiunti `webm → video/webm` e `mkv → video/x-matroska`.
- **I3:** il JSON Rust resta in pixel nativi/window, inclusi gli insets. La vista Java legge ogni volta `getLocationInWindow` e `getLocationOnScreen`: parent/hover = native − origine overlay nella window; screen = parent + origine overlay sullo schermo. Il click in coda mantiene il rettangolo nativo originale; il gesto scroll locale viene convertito alla window. Nessun offset di status bar fisso. La piccola classe produttiva `AccessibilityCoordinates` rende questa trasformazione verificabile senza runtime Android.

Non cambiate stringhe di UI, cataloghi delle sei lingue, temi Android/desktop, dipendenze, codec o workflow.

## Verifiche dell'implementatore

Eseguito `git diff --check`: exit 0, nessun output. Lettura diretta dei punti di chiamata e del diff. Nessuna suite compilata o prova sul telefono viene dichiarata PASS in questo rapporto.

## Checkpoint sorgenti congelati: prove affidate al controller

Comandi da eseguire sul PC di collaudo, dalla copia sincronizzata `/srv/dev/projects/cedro-android-fase1`, con lo stesso ambiente di build già validato dal controller (non eseguiti dall'implementatore):

```sh
cargo test --manifest-path android/Cargo.toml --lib
cargo test --manifest-path android/Cargo.toml --lib --features youtube
mkdir -p /srv/dev/scratch/cedro-final-geometry
javac -d /srv/dev/scratch/cedro-final-geometry android/java/it/cedro/android/AccessibilityCoordinates.java android/tests/java/it/cedro/android/AccessibilityCoordinatesTest.java
java -cp /srv/dev/scratch/cedro-final-geometry it.cedro.android.AccessibilityCoordinatesTest
```

Nuove regressioni nelle suite Android:

- `ui::rivedi::tests::all_voice_controls_are_reachable_by_scrolling_and_clipped_with_large_fonts`: gesti nel dispatcher reale, tutte le azioni di tre/otto voci, rename ultimo nome e toggle finale; 360×640, 640×360 e viewport 360×320, font 100%/200%, Onda e testo/Solo testo.
- `ui::rivedi::tests::voice_sheets_scroll_to_new_voice_and_last_merge_destination_without_moving_page`: tutte le destinazioni, nuova voce e ultima unione, clipping, 48 dp e attivazione reale, con le stesse dimensioni/font.
- `storage::tests::detached_original_uses_registered_grant_and_independent_cursor`.
- `storage::tests::web_video_mime_is_used_for_publication_and_intents`.
- `ui::accessibilita::tests::native_semantic_centers_dispatch_with_density_and_system_insets`: il centro JSON continua ad attivare Lavori con densità 3 e insets sopra/a sinistra.
- `AccessibilityCoordinatesTest`: geometria del caso Mi9T, landscape, finestra spostata, hover al centro e conversione scroll/click.

Gate ancora **pendenti**, da registrare nel rapporto controller: compilazione Rust host/NDK con e senza YouTube; Java/d8/APK; re-review del delta; installazione candidata e tap fisico al centro del nodo, performAction, hover e rotazione; raggiungibilità reale delle voci/fogli con font grandi e tastiera; originale SAF ≥32 MiB con firma staccata; MIME pubblicato e Intent per WebM/MKV. Il checkpoint non costituisce approvazione automatica della release.

## Correzione fixture dopo il primo gate compilato

Il controller ha eseguito il checkpoint `5a86bf6`: NDK con/senza YouTube riuscito (16,45 s / 3,22 s, comunicazione controller); suite Android host 53 riusciti e 2 falliti. Log esaminato direttamente: `~/Documenti/cedro-android-anteprima/verifiche/5a86bf6/cedro-android-5a86bf6-task6-aac3-host-test.log`.

I due fallimenti riguardavano la fixture `voices_sample`: passava nomi vuoti a `Review::new`, che per contratto elimina gli speaker non presenti nei nomi; impostare i nomi dopo il costruttore non ripristina gli speaker. Perciò la pagina conteneva solo il toggle e il foglio solo «Voce nuova». Corretto l'ingresso del costruttore, con nuove precondizioni che verificano esattamente tutte le voci attive e gli speaker delle righe. Le verifiche di raggiungibilità, gesti, clipping, 48 dp e azioni restano invariate. Nessuna modifica alla produzione in questo follow-up della stessa wave. `git diff --check` ripetuto: exit 0. Rilancio delle suite e tutti i gate fisici ancora pendenti al controller.

## Controller: covering tests eseguiti e9b6a72, 17novembre03:23CET

- host-test: exit0 TSV reale; test result: ok. 55 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.61s.
- host-test-youtube: exit0 TSV reale; test result: ok. 55 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.59s.
- ndk-youtube: exit0 TSV reale;     Finished `release` profile [optimized] target(s) in 2.82s.
- ndk-senza-youtube: exit0 TSV reale;     Finished `release` profile [optimized] target(s) in 2.92s.
- clippy: exit0 TSV reale;     Finished `release` profile [optimized] target(s) in 2.71s.

Comandi reali dal runner verifica-task6.py: cargo --config profile.test.package.fdk-aac-sys.opt-level=3 test -j2 -p cedro-android --lib [--features youtube]; RUST_TEST_THREADS2, CARGO_TARGET_DIRscratchcedro-android-target-test, TESTopt1/debug0/LTOoff/codegen16/incrementaltrue. NDK bash /srv/dev/scratch/cedro-check-ndk.sh [--features youtube], cargo checkreleasealltargets; clippy cargo clippy --release -j2 -p cedro-android --all-targets --features youtube --no-deps. Vendor/baseline warning restano, nessunclaimoutputpristine. Testnuoviovering55suite includonovoci/sheet/font2/viewport320/640,originalegrant32MB,cancel,MIME,geometrianativecenters.

Javahost: SDKjdk javac -d /srv/dev/scratch/cedro-final-geometry android/java/it/cedro/android/AccessibilityCoordinates.java android/tests/java/it/cedro/android/AccessibilityCoordinatesTest.java exit0; java -cp /srv/dev/scratch/cedro-final-geometry it.cedro.android.AccessibilityCoordinatesTest: AccessibilityCoordinatesTest: PASS,exit0. Logcedro-android-e9b6a72-final-runner.log in stessacartella locale. APKbuild inprogress, nonPASS fisico/Javaframework.
