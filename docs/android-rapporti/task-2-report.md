# Task 2 — Importazioni, lavori, SAF, cancellazione e risultati

Data: 2026-11-16. Ramo `codex/android-allinea-0.53`, base Task1 `a0740da`.
Stato consegna: **DONE_WITH_CONCERNS** per implementazione e review statica; **build Android e collaudo runtime NON ESEGUITI**, restano gate obbligatori. Nessuna dichiarazione di APK funzionante o pronto alla release.

## Vincoli osservati

Letti task-2-brief/global-constraints, contesto e piani 02/08/09 e rapporto revisione01. Nessun carico dev/win-test (lease esclusiva TracciaDoc), nessuna compilazione Rust/Android sul notebook, nessun adb durante le prove del controller, nessun fmt/rustfmt, nessun workflow desktop o chiave di firma toccato. Non modificati PROMPT-CODEX-tracciadoc.md/foto-riga; docs/android-rapporti/02-base.md è una modifica del controller ed è esclusa dal commit.

Il controller ha autorizzato il processo isolato per cancellare davvero i codec non cooperativi. Su sua successiva indicazione il bootstrap è **Rust**, non C. Rimosse le proposte iniziali di limiti fissi 2GiB cache / 256MiB originale firma: la copia usa spazio libero effettivo e la verifica originale conserva il contratto bytes esistente con allocazione fallibile.

## Implementazione nei sorgenti

### SAF e importazione

- `android/src/glue/saf.rs`: responsabilità separata dal glue; `openFileDescriptor`, fd Rust con close-on-exec, alias privati con nome/estensione corretti a `/proc/self/fd/N`.
- Per file seekable >=32MiB e formati vetrino SVS/NDPI/MRXS/CZI/ND2/LIF, niente copia integrale. Registro di lease `Arc<File>` per la sessione; preview e lavori non perdono il descriptor. Il figlio eredita **solo** fd pertinenti alla sorgente/cartella MRXS, togliendo CLOEXEC nel `pre_exec`, senza modificarlo nel genitore. Nessuna dipendenza da `/proc/<PID-genitore>/fd`.
- URI sidecar atomici; riapertura di URI ancora autorizzati all'avvio e da Recenti. Permessi persistenti richiesti solo se concessi da Android; `CedroActivity` gestisce OPEN_DOCUMENT/TREE e intent in ingresso.
- Stream non seekable copiati in blocchi64KiB; piccoli seekable possono usare cache privata. Riserva128MiB sullo spazio libero reale (`StatFs`), copie concorrenti serializzate per non spendere due volte lo stesso budget. UI mostra quando il provider impone copia e lo spazio disponibile. Nessun tetto fisso alla dimensione di un input.
- Directory univoca per importazione, nomi senza separatori/control/path traversal, scrittura `.part` + sync + rename e rimozione dei parziali su errore/annullamento.
- Cartelle ricorsive, sottocartelle preservate, nessun taglio silenzioso a200 file; frame JNI per elemento. MRXS mantiene cartella associata e file ausiliari; questi non diventano lavori separati in coda.
- Importazione su thread transitorio, risultato/errori/progresso/Annulla nella pagina Lavori; snapshot dell'intent catturato prima del thread. `CancellationSignal` sulle aperture/query SAF e poll100ms sui descriptor a pipe.
- Cleanup **solo all'avvio** degli import non referenziati più vecchi di7giorni oltre gli ultimi12. Sono protette tutte le sorgenti dei lavori/revisioni persistenti, anche oltre il limite del cronologico; revisione illeggibile => cleanup rinviato. Nessun servizio permanente.
- La lettura dell'originale per firma staccata è fuori UI, a blocchi e con `try_reserve`: il vecchio contratto `Opts.detached: Vec<u8>` resta invariato, senza tetto arbitrario. Vedere rischio memoria sotto.
- MIME: Office/fogli, HEIC/AVIF/JXL, DICOM e firme; fallback sconosciuto `.bin` invece di `.jpg`. Manifest/chooser allineati.

### Esecuzione e annullamento

- `android/src/bin/lavoro.rs`: launcher PIE minimale, `dlopen` della **stessa** libcedro_android.so dell'APK; entrypoint C ABI Rust `cedro_lavoro_main`. Nessuna duplicazione intenzionale del motore nel binario launcher.
- `android/src/worker.rs`: protocollo serde completo delle `Options` e selezioni pagine; conversione/merge/ZIP, trascrizione, web generico (anche risoluzione e download), riassunto in processo separato.
- Canale eventi dedicato UnixStream ereditato: progresso/passaggi/esito non condividono stdout con whisper/codec. Le frasi/revisioni continuano a essere salvate dallo stesso motore; questa fase non introduce ancora il nuovo flusso UI incrementale di Task5.
- Token per ID creato **prima** dello spawn, anche per lavori in coda. Permit cancellabile `cedro::resources::acquire` nel genitore: i processi isolati non moltiplicano la concorrenza CPU/RAM. YouTube mantiene il suo token e kill del process-group già implementato nel motore, con lo stesso permit e messaggi progresso.
- Annulla ferma il process-group e attende il figlio (`kill`/`wait`), chiude il canale e unisce il reader; niente worker nascosto che continui a calcolare dopo l'esito. `PDEATHSIG` nel launcher; crash/panic del figlio diventano errore nel genitore, `catch_unwind` protegge il supervisore.
- Directory output per lavoro, RAII per eliminare staging non completato su errore/panic/cancel. All'avvio vengono puliti solo staging generati precedenti all'avvio, privi di manifest di risultati completi e non referenziati.
- Esportazioni testo/revisioni condividono helper con scrittura a blocchi cancellabile, pubblicazione e cattura panic.

### Pubblicazione e persistenza

- Ogni lavoro pubblica le uscite della **sua** directory, senza confronto di una cartella comune. `App.uscite[id]` conserva ogni `(path/URI,nome,errore eventuale)`, con Apri/Condividi per ciascuna uscita e Copia errore dove serve.
- MediaStore `IS_PENDING`, sync e verifica del numero di righe aggiornate; fallback Documents/Cedro -> Download/Cedro. Cancellazione controllata anche nella copia di pubblicazione; riga pendente eliminata su errore.
- Il privato viene eliminato **solo** dopo pubblicazione riuscita. Fallimenti parziali mostrano “Da salvare”, elenco delle copie disponibili e azioni vere. Il provider esistente espone in sola lettura soltanto la sottocartella privata `files/uscite`, con controllo canonical del confine.
- Replace conserva la vecchia uscita fino al completamento della nuova; serializzazione delle pubblicazioni evita race Number/Skip/Replace.
- Manifest `.pronti` atomico prima di pubblicare: dopo chiusura forzata, i risultati completi ancora privati tornano nei Lavori. Non vengono puliti come temporanei.
- Cronologico JSON v2 atomico, migrazione lettura TSV precedente. Include stati attivi/errori/tutte le uscite; al riavvio gli attivi diventano interrotti. Si conserva il recente100, **oltre a tutti gli attivi e a tutte le uscite ancora da salvare**. Writer transitorio coalescente, nessun fsync nel loop UI.
- Nuovi testi nelle cinque tabelle en/de/es/fr/pt; grafica/temi Task1 preservati, aggiunti solo controllo Annulla e righe risultati pertinenti.

## Dipendenze / file condivisi

`serde`/`serde_json` erano già nel lock transitivo; serde e derive resi diretti nell'Android e opzionali nel motore (`android-worker`). `libc` già presente, reso diretto per fd/process-group Android. Nessuna nuova libreria pesante/versione scaricata nel progetto. License serde MIT/Apache-2.0, libc MIT/Apache-2.0; nessun nuovo componente terzo da aggiungere ai crediti rispetto al lock precedente. Variazione dimensione APK **non misurata**.

Le sole modifiche al motore condiviso sono derive condizionali di serializzazione e `serde(skip)` della callback `on_exists` (non trasportabile fra processi, Android usa Number/Replace/Skip), su Options/PageSel/Exists/PdfPages/PdfEdit e relativi tipi figura/tessere/ufficio/pdf. Nessun algoritmo desktop modificato.

## Verifiche eseguite (solo statiche)

1. `git diff --check` => exit0, nessun output.
2. `bash -n android/build.sh` => exit0, nessun output.
3. Parser tree-sitter-rust0.24.2/tree-sitter0.26.1 in `/tmp/cedro-syntax`, Python3.13 uv già presente; analizzati tutti i sorgenti Android e i file condivisi modificati => nessun nodo errore. **Questo non controlla tipi, borrow checker, JNI ABI o linking e non è un cargo check.**
4. Parsing Python TOML Cargo.toml/Cargo.lock/android/Cargo.toml => valido. Manifest XML => valido. TSV cinque lingue => nessuna chiave duplicata né traduzione vuota.

Output raccolto:

```
Rust: parsing sintattico senza errori, 12 file; NON compilazione
TOML: parsing valido (3 file)
Manifest: XML valido
Lingue: nessuna chiave duplicata o traduzione vuota nei 5 TSV
Parsing sintattico Android: senza errori (non compilazione).
```

Test Rust aggiunti **ma NON ESEGUITI**:
- `storage::tests::collisioni_budget_e_annullamento_non_lasciano_parziali` (omonimi isolati, budget/cancel senza parziali, byte corretti).
- `storage::tests::nomi_non_possono_uscire_dalla_cartella`.
- `worker::tests::opzioni_worker_senza_perdite` (roundtrip opzioni, inclusi caratteri speciali e Labs/Office).
- `ui::lavori_regressioni::riavvio_interrompe_attivi_e_conserva_errori_e_tutte_le_uscite`.

## Gate rinviati: eseguire SOLO dopo rilascio lease dev

La directory `/srv/dev/projects/cedro-android-fase1` è copia NON git con UI intermedia: prima sincronizzare i file del ramo senza cancellare sorgenti non approvati o cambiare l'ambiente. Non usare questo rapporto come prova di build.

Comandi previsti, **NON ESEGUITI**:

```
cargo test --release -p cedro-android --lib
cargo test --release -p cedro
cargo test --release -p cedro --test i18n_inglese
cargo ndk -t arm64-v8a -P 26 check -p cedro-android --all-targets
android/build.sh
CEDRO_YOUTUBE=1 android/build.sh
cargo run --release -p cedro-android --example vista_a -- <directory-output>
```

Sul Mi9T, dopo build e review e senza azzerare dati personali:
- launcher da nativeLibraryDir, dlopen, entrypoint esportato, socket IPC e fd ereditati; conversione reale + nuova apertura/riavvio.
- SAF seekable>=1GiB per formato vetrino: df/cache prima-dopo, tempo/RAM, preview + conversione; nessuna copia integrale.
- due URI omonime concorrenti; MRXS reale con cartella annidata e >200 file; URI persistente dopo force-stop e provider con permesso revocato; pipe/provider lento con cancel.
- annullamento durante codec/trascrizione/risoluzione web/download/modello/riassunto/YouTube; verificare CPU ritorna idle e assenza processi figli rimasti, anche con secondo lavoro in coda.
- errore e panic/crash figlio: scheda errore, foreground-service si ferma; kill app nel mezzo: stati interrotti e pulizia staging, risultati completi recuperati.
- PDF multipagina e due lavori contemporanei: tutte le uscite attribuite, nomi e byte verificati; MediaStore errore simulato NON distruttivo, fallback, copie private apribili/condivisibili, Replace/Skip/Number e cancel durante pubblicazione.
- UI A Lavori con font grandi e DE, eventuali adattamenti minimi; benchmark avvio/RAM/tempo/dimensione APK.

## Self-review e rischi residui concreti

- Corretto durante self-review l'accesso iniziale al modulo privato `cedro::pdfpages` nel test (`cedro::Plan` reexport corretto), JNI frame per importazioni multiple, propagazione eventi fuori stdout e permit nel genitore/YouTube.
- Corrette perdita del privato su errore MediaStore, cancellazione solo nominale, copia di file grande nel loop UI, output multipli non attribuiti, cancellazione del vecchio file prima del commit Replace, cronologico solo-successi, cap200 cartella e callback JNI su molte voci.
- **Tipi Rust/JNI, build launcher/LTO/export e comportamento SELinux/Android non sono verificati**. Possibili errori compile/link/runtime restano da correggere al gate; il parser non li esclude.
- Verifica firma staccata conserva API finale `Vec<u8>`: import non bufferizza il file, ma questa operazione specialistica deve ancora ricevere byte completi, ora fuori UI con allocazione fallibile. Consumo/misure e possibilità di streaming nel contratto firma vanno al task Office/firma; nessun limite fittizio imposto.
- Un provider esterno può ignorare CancellationSignal o non offrire seek: si mantiene stato attivo finché l'I/O termina, senza fingere che il thread sia già morto. Occorre la prova provider lento/pipe. Il fallback copia ha budget reale; la riserva128MiB non pretende di bloccare scritture di altre app.
- Lease fd mantenute per l'intera sessione, con chiusura alla fine del processo; nessuna eviction di file potenzialmente in uso. Quantità fd/RAM di un grande MRXS va misurata su telefono.
- Il piccolo launcher è previsto riutilizzare la lib esistente, ma dimensioni effettive, ABI e linking vanno misurati, non presunti.
- YouTube è ancora il percorso cooperativo esistente del motore, con token reale e gestione del process-group: primi download componenti e aggiornamenti richiedono la prova di cancellazione runtime.
- Fonte originale UI e meccanismi PDF/Office/Labs/wave avanzati non implementati in questo task: i task successivi possono usare `Richiesta`/`processo`, Options complete, lease SAF e `App.uscite`; nessun comando dimostrativo aggiunto.

## File nel commit

Cargo.toml/Cargo.lock; android/Cargo.toml, AndroidManifest.xml, build.sh; android/src/lib.rs, glue.rs, glue/saf.rs, storage.rs, worker.rs, bin/lavoro.rs, ui.rs; CedroActivity.java e ArchiviProvider.java; derive condizionali in src/engine.rs, figura.rs, svs_lab.rs, pdfedit.rs, pdfpages.rs, ufficio/mod.rs; cinque TSV; questo rapporto.


## Correzioni review — round 1/5

Base della review `8aacdb2`; ricevuta su HEAD `f19eb91`, durante le correzioni il controller ha aggiunto soltanto il rapporto baseline in `ad5877a`. Letti i sei Important di `task-2-review.md`; nessun file del controller modificato. Nessun fmt/rustfmt, cargo, adb o carico remoto. CI avviata dal controller: nessun risultato ricevuto durante questa correzione; non viene considerata PASS.

### Risoluzione dei sei finding

1. **«Le uscite strutturate vengono appiattite e le directory trattate come file»**. Nuovo modulo `android/src/publication.rs`, responsabilità unica di journal e pubblicazione. `Forma` distingue file, directory e insieme; `Elemento` conserva il percorso relativo e contiene solo file regolari. Directory/insiemi hanno un namespace comune univoco; MediaStore e legacy mantengono tutte le sottodirectory e i nomi accoppiati header/dati MRXS. La destinazione pubblica comune si fissa alla prenotazione del primo file: un gruppo non si divide fra Documents e Download. La UI elenca file apribili, mai una directory passata come file. Test sorgente: MIRAX minimo valido costruito con il JPEG reale `tests/fixtures/scan.jpg`, esportazione tramite il ramo legacy, riapertura con `cedro::mrxs::open`, decodifica `read_level` e confronto pixel prima/dopo; directory tiles annidata con confronto byte del JPEG esportato. Test NON ESEGUITO.

2. **«Un'interruzione durante la pubblicazione perde l'attribuzione delle uscite già salvate»**. `.pronti` diventa un journal JSON version3 con identità stabile del lavoro (anche origine, etichetta, timestamp e fonte revisione), struttura e stato di ogni uscita: Privato → Preparato(destinazione) → Pubblicato(destinazione). L'URI del nuovo record MediaStore viene sincronizzato nel journal **prima** di scrivere/committare il provider. Dopo commit viene sincronizzato lo stato Pubblicato **prima** di rimuovere il privato. Il ripristino di Preparato interroga `IS_PENDING`: una destinazione già committata viene attribuita allo stesso lavoro anche se il processo è morto prima dello stato Pubblicato. Errori di query conservano URI e privato; non vengono interpretati come successo. Dopo una prenotazione non si cambia URI con un fallback ambiguo. API legacy distingue il vecchio Replace dal nuovo risultato attraverso il temporaneo creato prima del checkpoint e rimosso dal rename. Test sorgente per tutte le transizioni, due PDF, metà lotto, journal già Pubblicato con privato ancora presente; NON ESEGUITO.

3. **«Esito e lista uscite su due code distinte hanno una race che perde il journal completo»**. Eliminata la coda separata delle uscite. `Completamento` contiene stato, tutte le uscite ed eventuale sorgente; `App::completa` applica l'evento, poi il loop persiste e ridisegna. Identità stabile anche nel journal UI, recupero idempotente senza duplicare il lavoro. Le identità rimosse/superate dal limite del cronologico sono persistite per non ricreare al riavvio lavori deliberatamente rimossi. Test sorgente con barrier fra drain UI e enqueue worker, successivo salvataggio immediato/riavvio, identità e due URI, recupero ripetuto e cronologico svuotato; NON ESEGUITO.

4. **«Il download sorgente dei lavori web completati rimane per sempre nello staging»**. Guardia `Scratch` separata dal journal dei risultati: rimuove `.scaricato` al ritorno del worker. Il passthrough sposta il media nella radice delle uscite; la conversione elimina il download completo dopo l'uso. Solo web-testo sposta la fonte in `.revisione`, con riferimento `.revisione-source` e fonte nel journal/UI, incluso YouTube. La guardia del genitore elimina lo scratch anche se il processo è terminato forzatamente. Self-review aggiuntiva: una revisione può essere salvata da Whisper prima dell'esito finale; `sorgente_revisione` verifica il riferimento reale tramite `revisione::has`, preserva quell'audio su errore/cancellazione e lo recupera insieme all'identità `.lavoro` al riavvio. Le revisioni persistite non sono cache eliminabile. Test sorgente separa cancellazione dello scratch da riapertura audio con riferimento serializzato; NON ESEGUITO. Misure spazio prima/dopo lavoro reale e riavvio restano obbligatorie.

5. **«La qualità della conversione web ora riceve l'indice del modello di trascrizione»**. Protocollo `Richiesta` con `qualita` indipendente da `Voce { modello, voci }`; il glue decodifica la precedente scelta vocale e mantiene qualità media 82. Il passaggio web→mp3/mp4/trascrivi modifica soltanto l'azione. Test sorgente con sei codici modello/voci e tre azioni, anche dopo roundtrip JSON; NON ESEGUITO.

6. **«La pubblicazione ora usa API29 anche sui dispositivi API26–28 supportati»**. Branch esplicito SDK_INT: da29 MediaStore, API26–28 copia privata→temporaneo nella destinazione pubblica, sync, rename, sync directory e scansione media. Nessun accesso a colonne/Downloads API29 nel ramo precedente. Number/Replace/Skip e conservazione privato su errore restano presenti. Il temporaneo di un errore viene svuotato e resta come marker riconoscibile dal journal, senza trattenere byte parziali né scambiare il vecchio file finale per quello nuovo. Test sorgente cancel durante Replace con vecchio file intatto, successivo rename e selezione26/27/28 vs29/30/34; NON ESEGUITO.

### Verifiche effettivamente eseguite in questo round

- Parser tree-sitter-rust sui 18 sorgenti `android/src/**/*.rs`: nessun nodo errore; **non è type-check Rust/JNI**.
- Parsing TOML Cargo.toml, Cargo.lock, android/Cargo.toml e XML manifest: validi.
- Cinque TSV en/de/es/fr/pt: nessuna chiave duplicata/traduzione vuota; aggiunte le nuove stringhe utente (quelle già presenti riutilizzate).
- `bash -n android/build.sh`: exit0, nessun output.
- `git diff --check`: exit0, nessun output.

Output:

```
Rust syntax: 18 files; errors: []
TOML: 3 valid; XML: valid
TSV: 5 languages, no duplicates or empty translations
```

Comandi di test/build elencati nella precedente sezione: **tutti NON ESEGUITI** in questo round. Nessun accesso a dev/win-test e nessun adb. I nuovi test sono in `publication::tests`, `worker::tests` e `ui::lavori_regressioni`; i test storage preesistenti restano richiesti. Non sono aggiunte dipendenze.

### Limiti e gate invariati

Restano obbligatori type-check Android/JNI, esecuzione dei test, build entrambe varianti, comportamento dei provider reali e MediaStore (inclusi crash dopo IS_PENDING0 e prima del journal, Replace/Number/Skip, batch e gruppi), API26–28, MIRAX reale e tiles riaperti dal filesystem pubblico, misure PSS complessive UI+worker, spazio effettivamente liberato e assenza di figli residui. Il fixture MIRAX del test non sostituisce una prova di vetrino clinico grande. La verifica delle transizioni pure non sostituisce crash injection Android. Il journal version3 recupera i lavori prodotti dalla versione corretta; il vecchio manifest sperimentale di `8aacdb2` non conteneva identità/URI, che non possono essere ricostruiti retroattivamente. La baseline del controller è una build precedente, non una prova di questa patch.

Il cleanup conserva destinazioni preparate non risolte se il provider non permette di accertarne lo stato; la copia privata resta disponibile. Un fallimento legacy conserva un marker temporaneo vuoto identificato nel journal (mai spacciato per risultato pubblico completato); la prova API26–28 dovrà includere anche questo caso. Non dichiarata idoneità APK/release.

File di questo round: android/src/{publication.rs,lib.rs,glue.rs,worker.rs,ui.rs}, cinque TSV e questo rapporto. I file del controller e i file estranei non entrano nel commit.
