# Review finale del ramo Android

Base `0a3a64b` → head `b4d03e7`, 139 file, 38 commit. Revisione trasversale svolta in più passaggi sul diff e sui sorgenti correnti, con piano `ESECUZIONE.md`, variante A accettata e rapporti/review precedenti come contesto. Nessuna modifica alla produzione, nessun subagent, nessuna esecuzione Rust, suite o adb da questo reviewer. Questo file è l'unico artefatto scritto.

## Strengths

- Il percorso SAF distingue grant registrato e pathname: `input_file` conserva la lease, usa `pread` con cursore logico per lettore e rifiuta su Android gli alias non registrati. I consumatori nativi conservano un lock RAII identificato da device/inode, con open file description separata e attesa cancellabile; il player propaga anche il proprio token. L'eccezione residua è I2 sotto.
- Worker, trasferimento degli fd, cancellazione e pubblicazione sono collegati in modo coerente: processo separato, terminazione del gruppo dopo il periodo di grazia, journal dell'identità e delle transizioni prima degli effetti esterni, conservazione delle copie private dopo errore di pubblicazione e recupero degli output/batch. La struttura MRXS e i relativi namespace non sono appiattiti.
- Le azioni di accessibilità passano dalla coda Java sincronizzata al dispatcher Rust reale; l'assegnazione degli ID considera le occorrenze e le collisioni. La correzione del servizio promuove subito la notifica e gestisce stop/intent nullo; la correttezza del comportamento fisico resta un gate distinto.
- Revisione e Referti persistono stato, conferme e sorgente; gli export condividono lo stato di bozza, e il caso accompagna i risultati senza inserire la nota privata nei nomi. Il catalogo Android deriva dal core, mantenendo formati e strumenti. La migrazione dei cinque temi desktop conserva le altre preferenze; il percorso Android mantiene esplicitamente i suoi nove temi. Anche il precedente Minor dei due marker freschi risulta corretto nel codice e coperto da regressione.
- Le modifiche agli installer desktop sono commenti coerenti con la migrazione; nel range non risulta modificato un workflow di pubblicazione desktop. Le dipendenze aggiunte non introducono un nuovo motore pesante.

## Issues

### Critical

Nessuno individuato.

### Important

**I1 — Le voci e i comandi di “Chi parla” diventano irraggiungibili dopo il nuovo lettore.**

- Riferimenti: `android/src/ui/rivedi.rs:540`, `:575`, `:799–850`; foglio collegato `:758–774`.
- Il layout scorrevole del lettore è limitato a `tab == 0`. Il ramo “Chi parla” ritorna prima del layout delle frasi; `chi_parla` ignora l'altezza della viewport e `rv.scorri`, non aggiorna `scorri_max` e disegna tutte le schede a distanza fissa. Con vista “Onda e testo”, scala 1 e viewport 360×640, il corpo parte a y≈404: la terza voce parte a y≈648, e “Scrivi chi parla nel file” resta ancora più sotto. Trascinare non può portare questi controlli sullo schermo. “Solo testo” riduce l'ingombro ma non rende raggiungibile l'intero elenco e il toggle con tre voci. La tastiera e i font grandi peggiorano il caso.
- Anche il foglio di scelta/unione voce limita la propria altezza a `h * 0.8`, ma continua a disegnare tutte le righe senza scorrimento: con otto voci più “Voce nuova”, a 640 dp l'ultima riga inizia a y=620 e termina fuori schermo. In landscape il problema compare prima.
- Effetto: non si possono rinominare/ascoltare/unire tutte le voci o scegliere tutti i destinatari e cambiare la scrittura delle voci. È una perdita funzionale rispetto al requisito della sezione 6 di `06-TRASCRIVI-ONDA-E-REFERTI.md`, non una preferenza estetica.
- Correzione: dare al corpo di “Chi parla” e al foglio una viewport realmente scorrevole, con offset, limite e clipping anche degli hit target; rendere raggiungibili i controlli con tastiera e font grandi. Verificare tre e otto voci su 360×640 e landscape, inclusi ultimo destinatario, “Voce nuova” e toggle finale. Non basta aumentare l'altezza fissa.

**I2 — La scelta dell'originale di una firma staccata riapre ancora il pathname SAF.**

- Riferimento principale: `android/src/glue.rs:480`; produzione dell'alias in `android/src/glue/saf.rs:203–209`; consumo in `android/src/ui/firma.rs:99–101`.
- Dopo `copia_uri`, il ramo `Sorgente::Originale` legge con `std::fs::File::open(&f.path)`. Per un originale seekable di almeno 32 MiB, l'importatore ha appena creato un alias al descriptor autorizzato, quindi questa apertura bypassa il registro e il lettore `input_file` introdotti nel ramo. Il rapporto `docs/android-rapporti/task-6-report.md:70` documenta già, sul Mi 9T, `fstat`/`pread` riusciti e `pathname_open` negato con EACCES sul grant: la concessione del provider non rende riapribile il pathname.
- Effetto: “Scegli il file originale” di una `.p7s` o `.tsr` non riesce a completare la verifica per tali originali, benché l'importazione abbia ottenuto un descriptor valido. Prima del nuovo import diretto questo ramo riceveva una copia privata, quindi la combinazione nuova introduce la regressione.
- Correzione: usare `cedro::input_file::File::open` nello stesso loop cancellabile, mantenendo il controllo degli errori e l'accumulo già richiesto dall'interfaccia del verificatore. Aggiungere una regressione del percorso originale con grant registrato/cursore indipendente e verificare una firma staccata con originale SAF ≥32 MiB sul dispositivo. Non serve cambiare il motore di firma.

### Minor

**M1 — WebM e MKV selezionabili non ricevono il MIME video.**

- Riferimenti: `android/src/glue.rs:113–158`, `:1144`, `:1207`; nuovi formati in `android/src/ui/web.rs:66–70`.
- L'interfaccia permette realmente `webm` e `mkv`, ma entrambi cadono in `application/octet-stream` nella mappa usata per il record MediaStore e per Apri/Condividi. Per gli URI MediaStore viene poi riutilizzato il medesimo tipo mediante `getType` (`glue.rs:591–596`).
- Effetto: il file è conservato correttamente, ma viene annunciato come binario generico; le app registrate soltanto per i tipi video possono non comparire nel resolver. Non affermo di aver osservato una specifica app ricevente fallire.
- Correzione circoscritta: aggiungere `webm → video/webm` e `mkv → video/x-matroska` alla mappa di esportazione e verificare tipo pubblicato/intento per questi due formati. Nessun nuovo codec o cambiamento al download.

## Recommendations e limiti delle evidenze

- Eseguire una sola fix wave per l'elenco sopra, con re-review mirata del delta e prove che esercitino raggiungibilità reale e lettura SAF. Non sono richiesti refactoring generali.
- Le suite non sono state rilanciate da questo reviewer. Il controller riferisce sul checkpoint `8e98780`: core 597 passati/15 ignorati, Android 50 con e senza YouTube, detector no-speech 5 più voce reale 1, i18n e NDK in entrambe le configurazioni PASS. Sono evidenze del checkpoint indicato, non una prova automatica di ogni modifica successiva o del comportamento fisico.
- Aggiornamento del controller durante questa review: APK `b4d03e7`/5300 costruito, firmato con certificato esistente e installato con aggiornamento riuscito; probe JAR tramite `Node.performAction` con 27 nodi iniziali univoci, azioni Lavori/Impostazioni con contenuto reale e scroll con firma cambiata, JUnit OK. Risultati riportati dal controller, non rieseguiti qui.
- Lo stress di otto import validi e gli altri gate fisici finali sono ancora in corso al momento della consegna. Il primo tentativo con percorso audio inesistente non è una regressione del sorgente né prova di otto import validi. Prestazioni/limiti PSS, durata e overhead onda, riproduzione/focus, annullamento e recupero reali, ciclo foreground e accettazione del candidato corretto vanno chiusi con gli artefatti del controller. Nessun PASS è inferito dal solo codice o dall'APK installato.

## Declined to judge

- Pubblicazione o collaudo di una release desktop: fuori dall'autorizzazione di questa consegna Android; sono state invece esaminate compatibilità condivisa e migrazione dei temi previste dal piano.
- Nuovo algoritmo di anonimizzazione CZI/ND2/LIF: escluso dal ruling registrato; il ramo mantiene la conversione e dichiara il limite del motore invece di promettere una copia anonima.
- Preemption della risoluzione DNS nativa prima della connessione: limite già esplicitamente accettato dal ruling Task 4; non riaperto come finding. La cancellazione di connessioni stabilite e worker resta nel perimetro.
- Nuovo motore completo di shaping RTL: limite del motore esistente dichiarato negli avvisi/report; non trasformato in richiesta di nuovo motore. Resta da verificare il rendering effettivo richiesto dai gate del piano.
- Identità certa delle tre persone del vecchio `dialogo.wav`: la provenienza/ground truth disponibile non la prova; non uso il numero atteso come certificazione di qualità della diarizzazione. Non esclude i test della gestione UI di tre voci.

## Assessment

**Ready to merge? With fixes.**

L'integrazione principale è coerente, ma I1 e I2 impediscono operazioni utente previste dal piano e vanno corretti prima della pubblicazione. Dopo le correzioni, la readiness di release resta condizionata alla chiusura esplicita dei gate fisici finali sul candidato corretto; questa review non dichiara conclusi test ancora in corso.
