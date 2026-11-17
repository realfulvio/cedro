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
