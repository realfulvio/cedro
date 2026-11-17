# QA runtime SAF — Mi 9T, APK intermedio db3a1a5

Data: 2026-11-17, circa 01:00–01:09 Europe/Rome. Solo runtime e ispezione mirata; nessuna modifica sorgente, build, installazione, reset, permessi generali o impostazioni OS. Dispositivo `2f6d6ba7`, app `it.cedro.android`, PID UI 31342; versione verificata `versionCode=5299`, `versionName=0.53.0-anteprima`, targetSdk 34.

## Esito

- **PASS: scelta reale SAF e concessione persistente di lettura.** `OS-3.ndpi` è stato scelto da DocumentsUI, senza intent costruiti o grant shell. Android registra il grant e Cedro registra la nuova importazione privata.
- **DIFETTO CONFERMATO: l'anteprima Laboratorio del file scelto con SAF fallisce con `Permission denied (os error 13)`.** L'errore è riproducibile dopo azzeramento esplicito della selezione/anteprima; non è soltanto l'errore precedente della prova con file URI.
- **INCOMPLETO: anteprima visiva e navigazione livelli.** Nessuna immagine generata; nessun export o conversione completa del file da 1,47 GB avviato.
- **Limite di automazione del picker osservato, causa precisa non stabilita:** tap touchscreen/mouse sulle righe e long press non attivano i file/cartelle; menu, ricerca, cambio elenco/griglia e scroll funzionano. Navigazione mediante focus tastiera TAB + ENTER ha consentito la scelta. Non attribuito a Cedro come difetto confermato.

## Percorso di riproduzione del picker

Stato iniziale: DocumentsUI `com.google.android.documentsui/com.android.documentsui.picker.PickActivity`, intent `OPEN_DOCUMENT`, `CATEGORY_OPENABLE`, tipo `*/*`, flags `0x41`, root Mi 9T.

1. Il tap su Download nell'elenco non cambia cartella; anche tap mouse, pressione statica 120/1000 ms ed ENTER dopo focus elenco non hanno inizialmente risolto. Lo scroll invece sposta realmente la lista. XML conferma bounds corretti e righe `enabled=true`, `focusable=true`, `clickable=false`; questo attributo da solo non prova un difetto.
2. Il menu laterale consente di entrare nel root Download. In quel provider la cartella prova-cedro mostra solo CMU-1, pur esistendo tutti i file del corpus: è una vista distinta, apparentemente dipendente dall'indice del provider. Non modificato né aggiornato l'indice.
3. Tornare al root **Mi 9T** dal menu laterale. Passare alla vista griglia dal controllo a destra del titolo. Toccare la griglia per impostare il contesto, quindi TAB per focalizzare il primo elemento; TAB successivi portano al file/cartella voluto, ENTER lo apre. Il focus reale è stato verificato con UI XML prima della scelta finale.
4. In Mi 9T, sei TAB dal contesto griglia hanno focalizzato Download e ENTER l'ha aperta. In Download, dopo tap sulla griglia e TAB iniziale (primo elemento JieLiOTA), due TAB + ENTER hanno aperto prova-cedro.
5. Ricerca `OS-3`; attendere l'aggiornamento dei risultati (il primo XML immediato può contenere ancora la lista precedente). Il solo risultato finale è `OS-3.ndpi`, `1,47 GB`.
6. Tap sul risultato, TAB: XML registra focus su `item_root` `[61,534][534,1148]`. ENTER esegue la vera scelta e torna a Cedro.

Dimensione verificata del fixture: `1473064608` byte, percorso `/sdcard/Download/prova-cedro/OS-3.ndpi`. Nessuna modifica al fixture.

## Prova del grant e nuova origine

Alle 01:05:51 Android registra:

```text
content://com.android.externalstorage.documents/document/primary%3ADownload%2Fprova-cedro%2FOS-3.ndpi
sourcePkg=com.android.externalstorage targetPkg=it.cedro.android
mode=0x3 owned=0x3 global=0x0 persistable=0x3 persisted=0x1
```

`persisted=0x1` è il permesso di lettura persistito dall'app. Il provider ha restituito anche una concessione temporanea di scrittura; non è stata usata.

Cedro, log 01:05:51.536:

```text
scelto OS-3.ndpi (/data/user/0/it.cedro.android/files/condivisi/18e8ac5967491c1b-1e/OS-3.ndpi)
```

Il percorso e il timestamp sono nuovi e distinti dalla precedente apertura file URI. Allegati: `saf-grant.txt`, `saf-preview-log.txt`, `picker-focused-file.xml`.

## Anteprima e distinzione dall'errore precedente

Subito dopo il rientro da SAF, Laboratorio mostra nome OS-3.ndpi, scheda selezionata `NDPI · 1473,1 MB`, e `Permission denied (os error 13)` senza immagine. Il codice UI corrente azzera l'errore quando cambia il percorso e richiede automaticamente una nuova anteprima; l'anteprima non è un export completo.

Per eliminare ogni dubbio di errore vecchio rimasto sullo schermo:

1. Dopo lo scroll, pressione 200 ms sul checkbox selezionato `[110,1280]`: la scheda passa a `File`, spariscono nome/errore del pannello Laboratorio e la CTA diventa disabilitata. Screenshot `12-deselected-preview-reset.png`.
2. La rimozione del pannello cambia i bounds: il nuovo checkbox è circa `[110,1100]`. Pressione 200 ms su quello riseleziona lo stesso nuovo import SAF.
3. Alle 01:07:24.997 parte un **nuovo worker PID 9252** (log linker della libreria dell'app), poi alle 01:07:25.052/.054 MediaProvider registra `Couldn't find file: /storage/emulated/0/Download/prova-cedro/OS-3.ndpi`. L'errore EACCES ricompare nel nuovo pannello. Screenshot `14-fresh-preview-after-reset.png`.

Anche una pressione 200 ms su Anteprima alle 01:06:48 genera un worker PID 9177 e nuovi errori MediaProvider sul medesimo fixture. Le anteprime Labs non espongono request id o esito nei log raccolti: il reset visibile, il nuovo worker e il timestamp distinguono la nuova richiesta; nessun id è inventato.

La causa interna precisa resta da diagnosticare. Evidenza compatibile con un problema nel percorso descriptor/riapertura del worker su storage Android, ma non basta a dimostrarlo. L'import SAF ha completato la query/dimensione e il descriptor; non è il caso previsto di `file://` senza permesso. Il codice locale consultato usa descriptor per NDPI seekable e lease verso `/proc/self/fd`; non è stata effettuata un'analisi binaria per provare che ogni riga del sorgente locale coincida con l'APK intermedio.

## Memoria, durata e copia

UI PID invariato 31342. PSS prima della scelta: 186139 KB; subito dopo: 185922 KB; dopo tentativo Anteprima: 185990 KB; dopo reset/riselezione: 186075 KB. RSS circa 283 MB. Nessun crash/OOM osservato e tentativi terminati in meno di circa un secondo dopo l'avvio del worker. `final-meminfo.txt` conserva il campione finale. Nessuna copia completa o export è stato richiesto; non è possibile verificare direttamente lo spazio privato perché `run-as` è negato (`package not debuggable`). La stabilità della memoria da sola non dimostra assenza di copia su disco.

## Evidenze e stato finale

Directory screenshot e allegati:

`/home/accorsiluca/Documenti/cedro-android-anteprima/prove-mi9t/anteprima-db3a1a5/saf/`

- 01: picker iniziale elenco Mi 9T.
- 02–05: scroll, pressioni e cambio griglia.
- 06: vista parziale del root Download provider.
- 07: risultato OS-3 nel picker reale ExternalStorage.
- 08–11: rientro SAF e tentativi anteprima.
- 12: selezione rimossa, errore azzerato visivamente.
- 14: nuova selezione, nuovo worker e nuovo EACCES.

Telefono lasciato in primo piano su **Cedro → Laboratorio**, OS-3.ndpi selezionato, errore EACCES visibile; nessun worker residuo rilevato da `ps`, nessun export/conversione attivo. Il grant SAF di lettura è persistito. Non toccata la navigazione legacy nota del Task 6.

## Follow-up: diagnosi sorgente mirata, sola lettura

Analizzati soltanto `android/src/glue/saf.rs`, la richiesta Labs in `android/src/glue.rs`, `android/src/worker.rs`, il launcher `android/src/bin/lavoro.rs`, e i punti di apertura di `android/src/labs.rs`, `src/svs.rs`, `src/ndpi.rs`, `src/mrxs.rs`, `src/imgfmt.rs`. Nessun accesso ADB, build o modifica di produzione nel follow-up.

### Catena verificata nel codice locale

1. `descriptor()` (`saf.rs:78–86`) apre l'URI con ContentResolver in modalità `r`, usa `detachFd()`, prende ownership in `File`, imposta CLOEXEC. `importa()` controlla metadata e seek prima di scegliere il percorso diretto per NDPI: il descriptor esiste e risponde a quelle operazioni nel genitore prima del completamento dell'import.
2. `collega()` (`saf.rs:90–97`) crea un vero symlink privato al percorso `/proc/self/fd/N` e conserva il `File` originale nella mappa globale `APERTI` con chiave uguale al percorso privato. Il symlink non è una copia e `N` rimane il numero del descriptor originale.
3. La nuova richiesta Labs usa lo stesso `path` per `saf::leases(&[path.clone()])` e `Richiesta.file` (`glue.rs:2216–2218`). La funzione `leases()` (`saf.rs:48–50`) comprende sempre l'uguaglianza esatta della chiave e inoltre include eventuali file associati della stessa cartella.
4. `worker::avvia()` (`worker.rs:224–236`) ottiene gli stessi numeri fd dagli Arc, rimuove CLOEXEC per ciascuno nel `pre_exec`, poi esegue il launcher. Non duplica né rinumera gli input. Gli Arc rimangono nella mappa globale e nel parametro `inputs`; non emerge una chiusura anticipata nel percorso normale.
5. Il launcher `android/src/bin/lavoro.rs:13–21` imposta soltanto il segnale alla morte del genitore, carica la libreria con `dlopen`, risolve il simbolo e chiama il motore: non chiude i descriptor. Nella catena esaminata non ci sono `close_range`, `closefrom`, `canonicalize` o risoluzioni manuali con `read_link` dell'input.
6. Nel figlio, `labs::anteprima()` (`labs.rs:62–72`) chiama `Slide::open(path)`. Per un `.ndpi`, `mrxs::is_mirax()` termina al controllo dell'estensione. `Slide::open()` (`svs.rs:227–234`) chiama poi `ndpi::is_ndpi()` (`ndpi.rs:201–209`), che fa **File::open(path)** e trasforma ogni errore in `false`. Se quel primo open fallisce, `Slide::open()` prosegue con il decoder TIFF (`svs.rs:179–184`), che fa un **secondo File::open(path)** e rende visibile l'errore grezzo `Permission denied (os error 13)`. Questa sequenza spiega coerentemente due nuove chiamate MediaProvider prima dell'errore, ma non identifica da sola il syscall fallito sul dispositivo.

### Alternative controllate e limiti

- **FD non passati:** nessuna omissione trovata nella catena esaminata; sia l'uguaglianza della lease sia il `pre_exec` sono presenti. Questo è un risultato dell'ispezione del codice locale, non un dump del fd table del worker dell'APK.
- **FD chiusi troppo presto:** nessuna chiusura esplicita o rilascio dell'ultimo Arc trovato; la mappa di sessione conserva ownership. Un fd perso produrrebbe normalmente un alias mancante/ENOENT o un fd non valido, mentre i log mostrano l'ingresso nel provider sul percorso del fixture: l'evidenza favorisce una nuova apertura negata, ma non è prova esclusiva.
- **Rinumerazione/collisione:** gli input restano aperti quando viene creato il socket eventi, quindi il nuovo fd del socket non può riutilizzare uno di essi. La configurazione di stdin/stdout/stderr riguarda 0/1/2; nessuna rinumerazione degli input è nel codice esaminato. Nessun elemento concreto dimostra questa causa.
- **Canonicalizzazione al percorso esterno:** non presente nella catena. Aggiungerla non è una soluzione: eliminerebbe proprio l'indirezione al descriptor senza trasferire il grant URI al percorso filesystem.
- **Errore vecchio/UI:** già escluso dal reset visibile e nuovo worker nel test runtime. Il controllo del percorso Labs inoltre invalida e cancella l'errore al cambio di input.

### Ipotesi principale: apertura dell'alias diversa dall'uso del descriptor

**Resta un'ipotesi da provare sul dispositivo.** Il descriptor ricevuto via SAF può essere valido mentre una nuova `open()` dell'alias `/proc/self/fd/N` fallisce. La documentazione Linux mostra espressamente che poter leggere un descriptor già aperto non garantisce il diritto di riaprirlo attraverso `/proc/.../fd`. [Linux proc_pid_fd(5)](https://man7.org/linux/man-pages/man5/proc_pid_fd.5.html)

Nel codice AOSP Android 13 di riferimento, `pf_open()` richiama `MediaProvider::OnFileOpen` con il percorso, UID e PID del chiamante e inoltra l'eventuale errore: quindi una nuova apertura FUSE può richiedere una nuova verifica. È coerente con i messaggi MediaProvider osservati, ma **non è stata verificata la versione precisa del provider/FUSE MIUI installato** e non è provato se il blocco sia nel controllo FUSE, nel controllo di dereferenziazione proc o in un altro livello. [AOSP FuseDaemon.cpp, pf_open](https://android.googlesource.com/platform/packages/providers/MediaProvider/+/refs/heads/android13-security-release/jni/FuseDaemon.cpp#1452)

### Minimo intervento consigliato al Task 6

Prima di scegliere una correzione, il cambiamento più piccolo e verificabile è una diagnostica nel **worker già avviato** per un solo input SAF: numero fd e chiave lease attesi, `fcntl(F_GETFD)`/`fstat`, `pread` di pochi byte a offset zero sul descriptor ereditato, poi apertura dell'alias e relativo errno. Conservare anche il contesto nell'errore `NDPI riconoscimento/apertura`, invece di lasciare che il primo open fallito diventi soltanto `false`. Non stampare contenuti o URI estranei al fixture.

- Se `fstat`/`pread` del fd ereditato riescono ma l'apertura dell'alias restituisce EACCES, la riapertura è dimostrata come punto di rottura; la correzione deve far leggere il **descriptor autorizzato** direttamente al lettore, preservando nome/estensione separatamente, senza copiare l'intero file.
- Se il fd manca o è differente nel figlio, correggere prima trasporto/associazione lease. Non c'è attualmente evidenza sufficiente per prescrivere una modifica a quella parte.
- Non proporre come fix validato una semplice sostituzione con `dup()`/`File::try_clone()`: duplicano la stessa open file description e condividono il cursore. I lettori attuali riaprono più volte e alcuni lavorano in parallelo; una sostituzione cieca può creare letture concorrenti errate. Per una sorgente basata su fd servirà una lettura posizionale (`pread`/`FileExt::read_at`) con cursore logico indipendente per ogni reader, oppure un'altra strategia esplicitamente sincronizzata. [Linux dup(2)](https://man7.org/linux/man-pages/man2/dup.2.html)

Non è emersa una correzione certa di una riga. Nessun fix, test di produzione o nuova prova sul telefono è stato eseguito nel follow-up; la raccomandazione tecnica è condizionata al probe discriminante descritto sopra.
