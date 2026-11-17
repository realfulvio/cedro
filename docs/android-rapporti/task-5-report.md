# Task 5 — Trascrizione, onda e referti Android

Stato implementazione: **DONE_WITH_CONCERNS**. Sorgenti completati, verifica statica locale positiva; il controller esegue build, suite e collaudo reale. Questo rapporto non certifica i gate Mi 9T né autorizza a saltarli prima della pubblicazione.

## Consegna

- `c77c2aa` — Collega onda, referti e segmenti al worker Android.
- `cdfeec2` — Conserva le conferme per frase e compatibilità desktop senza serde.
- `08c5280` — Completa controlli onda, traduzioni e regressioni dei referti.
- Il commit del presente rapporto include anche le ultime correzioni di self-review descritte sotto.

## Implementazione

Il detector Android include direttamente `trascrivi/src/parlato.rs`, senza duplicazione dell'algoritmo. È eseguito dopo la decodifica e prima di diarizzazione/riconoscimento lingua/Whisper. Il prompt usa `rifinitura::prompt` e il glossario veterinario effettivo. Il detector ora gestisce anche la sonda vuota senza panic. Silenzio e suoni stazionari producono zero segmenti; il worker traduce questo risultato in un completamento neutro con durata, suggerimento e percorso vuoto, senza pubblicare file vuoti. Una revisione precedente dello stesso sorgente non viene scambiata per il risultato della nuova trascrizione senza parlato.

`Voce` ha `referto` con default serde retrocompatibile. `Compito` e `Effetto::Converti` portano la `Voce` tipizzata; qualità media, modello, numero di voci e flag referti non sono più sovrapposti in un byte. `Scelte`/`Batch` conservano questi campi insieme a opzioni/caso. I figli continuano a usare identità, ricevute, `.revisione-source`, `.batch-child` e audio distinti del Task4. Nessuna seconda coda, nessun `files[0]`, nessun cambiamento alla materializzazione/idempotenza delle ricevute.

Eventi core `Audio { secs }` e `Segment` attraversano il protocollo worker con sorgente, tempi e voce. I consumer desktop/CLI ignorano i nuovi eventi senza cambiare il workflow. La UI conserva tracce separate per sorgente, aggiorna il testo mentre il motore lavora, apre la vista in corso dai Lavori e carica la revisione completa alla fine. Eventi tardivi di un lavoro già terminato sono ignorati. Un errore o annullamento non rende esportabili segmenti incompleti.

Onda preparata in thread a 600 campioni, poi aggregata alla larghezza effettiva del telefono. La coda legge le onde in sequenza, evitando un decoder per ogni file simultaneo. La stessa UI player serve anteprima prima dell'avvio e revisione/live: tre viste ricordate, tocco preciso, trascinamento della finestra ingrandita, pulsanti zoom −/+ e spostamento visibili, play/pausa, ±10 s, 0,75×/1×/1,25×/1,5×/2×, posizione/durata. Volume affidato ai tasti del telefono e dichiarato nella UI. In orizzontale comandi e testo stanno affiancati. Le barre trascritte si colorano, la frase attiva è evidenziata, lo scorrimento manuale disattiva il seguire e il comando visibile lo riattiva. Un tocco sulla frase avvia l'ascolto da quel punto.

Il player resta `cedro::ui::player::Player`, cpal/AAudio su Android. Non sono stati introdotti codec o dipendenze. L'orologio virtuale su errore audio è disattivato su Android: il player si ferma e mostra l'indisponibilità. Il callback PCM non sovrascrive più un seek concorrente. Il timer audio ridisegna al massimo ogni 200 ms e non tiene attivo il ciclo animazioni a 60 Hz. Restano possibili ridisegni su input/eventi UI; il costo effettivo durante Whisper è un gate di misura, non un risultato dichiarato.

`CedroActivity` aggiunge soltanto il ponte per AudioManager, callback perdita focus, MEDIA_PAUSE/STOP e distacco cuffie. Motivo documentato in `android/README.md`: NativeActivity/AAudio non espongono a Rust il listener Java. Nessuna nuova classe o dipendenza. La riproduzione si mette in pausa quando l'attività va in pausa (anche schermo spento); trascrizione e riassunto continuano nel servizio di lavoro già esistente. Nessun nuovo servizio/autostart, nessuna ripresa automatica dopo chiamata. La velocità ricampiona e cambia anche l'altezza della voce, come il player condiviso.

Referti visibili nel pannello A. Revisione con numeri/parole decisive, filtro, conteggio, riascolto, conferma e correzione con tastiera sul posto. Le conferme persistono con il documento e sono legate a tipo, frase e contenuto: confermare o correggere una frase non conferma le altre contenute nello stesso segmento Whisper. Riassunto e nomi restano nella revisione. TXT/DOCX/SRT/VTT usano `Review::export` condiviso: stesso nome `- bozza`/`- rivisto`, stessa riga di bozza. Per SRT la riga è il primo cue, per VTT è una NOTE, conservando i tempi delle frasi. Questa piccola correzione condivisa interessa anche gli export desktop, senza modificarne workflow/temi/release.

Riassunto Qwen3.5-4B conservato integralmente, `Failure::LowMemory` invariato, annullamento visibile anche nella scheda Riassunto. Nessuna soglia inventata né fallback/rimozione del modello.

## Verifiche effettuate

### Locale (solo sorgenti, nessun Rust/build/fmt/adb)

- `git diff --check`: PASS senza output.
- Parser tree-sitter Rust via Python/uv: **22 file Rust del diff, zero errori sintattici**. È parsing statico, non compilazione.
- Copertura delle nuove chiavi: aggiunte a tutte le cinque TSV (en/de/es/fr/pt); allineamento chiavi verificato con script Python.
- Self-review di protocollo, conservazione sorgenti/ricevute, separazione qualità/Voce, esportazioni e persistenza.
- Nessun `cargo`, `rustc`, `rustfmt`, build remota o comando adb eseguito da questo implementer.

### Evidenze riferite dal controller

Sul checkpoint `c77c2aa`, primo core check fallito: serde era opzionale nel core desktop (E0433/E0277). Corretto in `cdfeec2`: JSON manuale per ReviewState; derive dei Segment soltanto con feature `android-worker`. Non è stata resa obbligatoria una dipendenza.

Il controller comunica **quattro check PASS su `cdfeec2`**:

- core default: 23,73 s;
- NDK all-targets con YouTube: 29,24 s;
- NDK all-targets senza YouTube: 4,93 s;
- Android host: 26,03 s.

Queste prove non attestano i commit successivi. Le suite e i render del commit finale sono richiesti al controller; aggiungere i loro esiti al rapporto dopo l'esecuzione. Nessuna evidenza TDD RED/GREEN runtime prodotta dall'implementer, dato il vincolo esplicito source-only.

## Test aggiunti / comandi per il controller

- `cargo test --lib ui::review::tests -- --test-threads=1`: persistenza reale del JSON, riapertura, conferma di una sola frase dello stesso segmento, correzione parziale, completamento, riassunto che conserva audit; draft/nome/contenuto dei quattro export, incluso XML DOCX.
- `cargo test --test parlato_android`: stesso detector Android/desktop; 30 s di silenzio, rumore bianco, tono 440 Hz, ronzio 50 Hz; sillabe sintetiche anche deboli, parola breve in silenzio, input corto/vuoto. Nessuna fixture privata richiesta.
- `CEDRO_SPEECH_FIXTURE=/percorso/fleurs-breve.wav cargo test --test parlato_android real_voice_passes_shared_detector -- --ignored`: opt-in **voce reale**, PCM mono 16 kHz/16 bit. Non è un test di diarizzazione né sostituisce il dialogo lungo. Usare la fixture autorizzata SHA256 `9afb1037c1e557157322fb488a3bd9514c3ccee7907c52812a79055a27868ae5`.
- `cargo test -p cedro-android --lib ui::rivedi::tests -- --test-threads=1`: segmenti di sorgenti distinte, stop/ripresa del follow, export incompleto vietato, controlli e formati raggiungibili a 360×800, 360×640, 800×400, 800×360 e tre viste.
- `cargo test -p cedro-android --lib worker::tests -- --test-threads=1`: compatibilità JSON Voce vecchia, referto/voci/modello, roundtrip eventi con sorgente/tempi/parlante; più regressioni Task4 esistenti.
- Suite core/Android e test i18n appropriati dopo i test mirati, senza ripeterli in assenza di nuovi cambiamenti/fallimenti.
- `cargo run -p cedro-android --release --example vista_a -- /tmp/vista-a-task5`: sette scene aggiuntive `onda-testo`, `referto`, `solo-testo`, `solo-onda`, `onda-live`, `onda-prima`, `nessun-parlato`, già comprese nel ciclo IT/DE/PT e tre geometrie. Tutti i dati di scena sono esclusivamente nell'example host.

## Self-review e correzioni

- Identificata/corretta la serializzazione core opzionale dopo il check del controller.
- Conferme rese specifiche per frase/snippet, non soltanto per indice o segmento; ripristinate anche quando arriva l'onda dopo il documento.
- Correzione di una frase non chiude altri punti aperti nello stesso segmento.
- Testo delle righe e nomi utente non vengono tradotti; etichette passano dal Painter che usa `tr()` e le nuove chiavi sono nelle cinque tabelle.
- Bersagli delle righe ritagliati alla viewport per non coprire controlli/piede; piede ridotto in base ai comandi effettivi, quattro formati in una riga; controlli testo disposti a lato in landscape anche in Solo testo.
- Modalità tastiera libera spazio al testo, mantiene la frase selezionata visibile; le correzioni si salvano alla chiusura/Enter/pausa attività.
- Rimosso il repaint continuo dovuto a `Rivedi::suona`; ridisegno audio temporizzato.
- Timer PCM reso resistente al seek concorrente e niente avanzamento virtuale silenzioso su Android.
- Annullamento non abilita export/riassunto di una trascrizione parziale; eventi tardivi non duplicano righe dopo il completamento.
- Anteprime della coda in sequenza anziché un thread di decodifica per ogni file.

## Gate non eseguiti / rischi da verificare prima della release

Non misurati da questo implementer: tempo dialogo, PSS/RSS aggregato genitore+worker, idle dopo silenzio, APK prima/dopo, prima onda <1,5 s, CPU UI durante Whisper, limite reale di repaint, accuratezza audio e ±10 s a orecchio/posizione, chiamata e cuffie, TalkBack, riassunto 4B sul Mi 9T, continuità background reale, parità testo con audio referto desktop. Non sono risultati PASS impliciti.

Usare esclusivamente Mi 9T e benchmark base misurato **1000,002 s** sul `dialogo.wav` 243,3 s (SHA nel brief), non lo storico 206 s. Confronto +5% tempo e +10% PSS contro base 1.724.202 KiB, con UI/worker aggregati. Ground truth dei parlanti del campione recuperato ancora non confermato: non forzare Auto a tre; prova separata Esattamente 3. Confrontare i due stati idle storici differenti, senza chiamarli automaticamente leak.

I file `android/src/ui/rivedi.rs` e `glue.rs` erano già grandi; sono stati estesi senza ristrutturazioni fuori piano. Valutare con i render l'area testo su schermi bassi/font grandi e verificare l'interazione reale della tastiera. Il ponte focus richiede verifica Android reale; nessun test di sola posizione dimostra suono udibile. Il detector è conservativo ma la fixture reale e il dialogo restano obbligatori prima di dichiarare assenza di regressioni vocali.

## File interessati

Core: `src/whisper_android/mod.rs`, `trascrivi/src/parlato.rs`, `src/trascrizione.rs`, `src/revisione.rs`, `src/ui/review.rs`, `src/ui/player.rs`, `src/audio.rs`, `src/lib.rs`, adattatori eventi in `src/cli.rs` e `src/ui/model.rs`.

Android: `worker.rs`, `glue.rs`, `ui.rs`, `ui/rivedi.rs`, `ui/pdf.rs`, `ui/casa.rs`, `ui/casa/telefono.rs`, fixture/test Voce in `web.rs` e `ui/web.rs`, ponte `CedroActivity.java`, example `prova.rs` e `vista_a.rs`, README e note release 0.53. Cinque TSV, nuovo test condiviso `tests/parlato_android.rs`.

Non modificati: report Task3/Task4/Task8 del controller, `PROMPT-CODEX-tracciadoc.md`, `foto-riga/`, dipendenze/workflow release, tema desktop Task8, segreti, ambienti e telefono.

## Fix round 1 — review `eb033a3`

Lette integralmente le due finding Important in `task-5-review.md` e il riferimento `subagent-driven-development/references/fix-loop.md`. Entrambe accettate; nessuna modifica Task6 o fuori finding.

1. **Avvio riassunto fuori viewport.** Il ramo iniziale del Riassunto non usa più una scheda fissa da 300 dp col comando in fondo al contenuto. Il bottone da 52 dp è ancorato al piede della viewport disponibile; titolo, descrizione completa del 4B e note/errori scorrono nella regione sopra, ritagliata al pannello. `scorri_max` deriva dall'altezza realmente impaginata, senza tagliare il testo a sette righe. Il bottone rimane fermo durante lo scorrimento.
2. **Ripresa apparente dopo errore AAudio.** La transizione `Shared::fail_audio` è condivisa dal ramo Android e dalla regressione host. Lo stato fallito impedisce `Player::toggle` e `Player::seek`, `playing()` resta falso e anche `Shared::set_paused(false)` mantiene la pausa. Non viene riavviato un orologio senza thread audio. Nessun tentativo audio finto o nuova dipendenza; il comportamento desktop non fallito resta invariato.

### Test di copertura aggiunti

- `android/src/ui/rivedi.rs::tests::initial_summary_action_stays_visible_and_reachable_on_short_screens`: renderer reale della scheda Riassunto senza risultato in 360×640, 360×800, 800×360, 800×400, viste Onda e testo/Solo testo. Verifica rettangolo completo dentro schermo, minimo 48 dp, scorrimento reale della descrizione, bottone immobile e percorso tocco giù/su che restituisce `Effetto::Riassumi` e crea il lavoro.
- `src/ui/player.rs::tests::failed_audio_cannot_resume_or_seek_a_virtual_clock`: stato di playback con clock precedentemente avviato, transizione di errore effettivamente usata su Android, ripetuti Play/seek e ripresa interna; verifica stato non in riproduzione, clock fermo e posizione invariata anche dopo un intervallo reale. Copre anche `ended=true` seguito da Play; nessun dispositivo audio richiesto.

### Comandi/esiti dell'implementer

- `git diff --check`: **PASS**, nessun output.
- `uv run --no-project --with tree-sitter --with tree-sitter-rust python ...` sui due sorgenti: output `android/src/ui/rivedi.rs PASS`, `src/ui/player.rs PASS` (parsing sintattico).
- Self-review del diff limitata alle due finding e ai relativi test: comando riassunto non sovrapposto alla clip del corpo; default view e orientamento invariati; nessun ramo Play può riattivare un player con `failed=true`.

**Test Rust non eseguiti dall'implementer**, per istruzione esplicita source-only. Nessuna build remota/locale, adb o fmt. Prima della re-review il controller deve eseguire e allegare comando/output:

```
cargo test --lib ui::player::tests::failed_audio_cannot_resume_or_seek_a_virtual_clock -- --exact
cargo test -p cedro-android --lib ui::rivedi::tests -- --test-threads=1
```

Restano aperti i gate fisici già elencati. Questa appendice certifica modifiche e controlli statici; non attribuisce un PASS runtime ai test nuovi.

### Copertura fix1 eseguita dal controller su f5dcc48 — 17novembre

Mini-PCsourcef5dcc48, `RUST_TEST_THREADS=2 CARGO_TARGET_DIR=/srv/dev/scratch/cedro-android-target-test CARGO_PROFILE_TEST_OPT_LEVEL=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_TEST_LTO=false CARGO_PROFILE_TEST_CODEGEN_UNITS=16 CARGO_PROFILE_TEST_INCREMENTAL=true`; nice10/j2, overridepackagefdk-aac-sysO3. Nonprofilorelease. Logarchiviati `~/Documenti/cedro-android-anteprima/verifiche/f5dcc48/`.

```sh
nice -n 10 cargo --config profile.test.package.fdk-aac-sys.opt-level=3 test -j2 -p cedro --lib ui::player::tests::failed_audio_cannot_resume_or_seek_a_virtual_clock -- --exact
# test ...failed_audio_cannot_resume_or_seek_a_virtual_clock ... ok
# 1 passed;0failed;602filtered;0.02s (build2m43)
nice -n 10 cargo --config profile.test.package.fdk-aac-sys.opt-level=3 test -j2 -p cedro-android --lib ui::rivedi::tests -- --test-threads=1
# initial_summary_action_stays_visible_and_reachable_on_short_screens ... ok
# live_segments_keep_sources_separate_and_manual_scroll_stops_following ... ok
# player_and_exports_remain_visible_in_both_orientations ... ok
# 3 passed;0failed;42filtered;0.51s
```

Confermano le due regressioni ospiti; non ascolto Android reale né benchmark. Altri test/renderer in corso, non ancora PASS.

### Integrazione documentale — detector su `f5dcc48`, suite precedenti e review

Appendice report-only: letti i log locali effettivi in `/home/accorsiluca/Documenti/cedro-android-anteprima/verifiche/f5dcc48/` e `verifiche/08c5280/`, lo script `/home/accorsiluca/prove-cedro-android/verifica-task5-final.py` e `task-5-fix1-review.md`. Nessuna modifica ai sorgenti, comando git, test, build, accesso remoto o adb eseguito in questo followup.

**Detector condiviso su `f5dcc48`: PASS sul mini-PC.** Comandi dello script del controller, directory `/srv/dev/projects/cedro-android-fase1`:

```sh
# Ambiente comune dei test (profilo test opt-level 1, NON release):
export CEDRO_SPEECH_FIXTURE=/srv/dev/scratch/cedro-fleurs-breve.wav
export RUST_TEST_THREADS=2
export CARGO_TARGET_DIR=/srv/dev/scratch/cedro-android-target-test
export CARGO_PROFILE_TEST_OPT_LEVEL=1
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_PROFILE_TEST_LTO=false
export CARGO_PROFILE_TEST_CODEGEN_UNITS=16
export CARGO_PROFILE_TEST_INCREMENTAL=true

nice -n 10 cargo --config profile.test.package.fdk-aac-sys.opt-level=3 test -j2 -p cedro --test parlato_android
# test result: ok. 5 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.03s
# Compilazione del test: 2m 50s.

nice -n 10 cargo --config profile.test.package.fdk-aac-sys.opt-level=3 test -j2 -p cedro --test parlato_android real_voice_passes_shared_detector -- --ignored
# test parlato::tests::real_voice_passes_shared_detector ... ok
# test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out; finished in 0.02s
# Preparazione target già compilato: 0.56s.
```

Log: `cedro-android-f5dcc48-task5-aac3-no-speech.log` e `cedro-android-f5dcc48-task5-aac3-real-voice.log`. Il primo comprende silenzio, tono/ronzio/rumore, sillabe deboli, parola breve e input vuoto/corto; la voce reale è ignorata nel giro standard ed eseguita esplicitamente nel secondo. La fixture dichiarata dal controller è FLEURS breve autorizzata, SHA256 `9afb1037c1e557157322fb488a3bd9514c3ccee7907c52812a79055a27868ae5`. I log confermano l'esito della prova; questa appendice non esegue una nuova verifica dell'impronta remota. Sono presenti warning preesistenti di `rav1d`: non si dichiara output privo di warning.

La prova reale conferma soltanto che **il detector non scarta questa registrazione vocale**. Non esegue Whisper, non misura accuratezza della trascrizione o diarizzazione, non prova il player e non costituisce collaudo sul Mi 9T.

**Suite e check precedenti su `08c5280`:** dal TSV `cedro-android-08c5280-task5-aac3-validazione.tsv` e dalle righe finali dei rispettivi log:

| Passo | Esito | Tempo del solo test / durata totale dello step |
| --- | --- | --- |
| Core check (`cargo check --release -j2 -p cedro --tests`) | exit 0 | totale 22,42 s |
| NDK con YouTube (`bash /srv/dev/scratch/cedro-check-ndk.sh --features youtube`) | exit 0 | totale 27,44 s |
| NDK senza YouTube (`bash /srv/dev/scratch/cedro-check-ndk.sh`) | exit 0 | totale 4,93 s |
| Android host check (`cargo check --release -j2 -p cedro-android --all-targets --features youtube`) | exit 0 | totale 26,23 s |
| Core lib (`cargo test -j2 -p cedro --lib`) | **587 PASS, 0 failed, 15 ignored** | test 42,16 s / totale 236,22 s |
| Android lib senza YouTube (`cargo test -j2 -p cedro-android --lib`) | **44 PASS, 0 failed** | test 2,80 s / totale 193,71 s |
| Android lib con YouTube (`cargo test -j2 -p cedro-android --lib --features youtube`) | **44 PASS, 0 failed** | test 2,51 s / totale 29,55 s |

I comandi test della tabella usano lo stesso ambiente opt-level 1 sopra, prefisso `nice -n 10` e override `--config profile.test.package.fdk-aac-sys.opt-level=3`; per check/NDK lo script usa l'ambiente ordinario con `RUST_TEST_THREADS=2`, senza imporre il profilo test. L'orchestrazione `08c5280` è stata interrotta per passare al fix `f5dcc48` prima che il passo no-speech producesse un esito: il suo log non contiene un risultato finale e non viene conteggiato come PASS. Le suite `08c5280` attestano quel checkpoint, non la suite completa sull'head successivo.

La scoped re-review `task-5-fix1-review.md` approva `eb033a3..f5dcc48`: **2 finding ADDRESSED, 0 NOT ADDRESSED, nessuna nuova Critical/Important/Minor**. La copertura player/rivedi su `f5dcc48` è già registrata dal controller nella sezione precedente. La review non chiude i gate fisici.

**Renderer `f5dcc48`: IN CORSO** al momento di questa appendice; nessun PASS o approvazione visiva dichiarati. Restano invariati benchmark, memoria, ascolto/focus/cuffie, accuratezza Whisper, riassunto 4B e gli altri gate reali del telefono.

### Renderer finale controller f5dcc48

`cargo --config profile.dev.package.fdk-aac-sys.opt-level=3 run -j2 -p cedro-android --features youtube --example vista_a -- /srv/dev/scratch/cedro-ui-task5-f5dcc48`, DEVopt1/debug0/LTOoff/codegen16/incrementaltrue,nice10. Exit0,300PNG15507091B copiati `~/Documenti/cedro-android-anteprima/render-rust/task5-f5dcc48`. NonperformanceAPK. Ispezionati IT360x640referto,DE360x640onda-testo,PT800x400solo-testo,IT360x800onda-live,DE360x640nessun-parlato,PT360x640onda-prima.

**Gap runtime-render da risolvere Task6:** in360x640Ondaetesto/referto il player+tabs+filtri lasciano zero frase leggibile prima del piede export. Controlli visibili e tradotti non bastano: serve area testo fruibile con48dpefont200. Assegnato implementerTask6, nonPASScomplessivolayout. LandscapePT/IT800livemostrano testo econtrolli, esitoneutroDEnessunparlatoleggibile. Nonprovasuono/touchtelefono.
