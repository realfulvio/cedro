# Task 2 — Review statica

Base `a0740da`, HEAD dichiarato nel pacchetto `8aacdb2`. Ambito: import/SAF, lavori/worker, pubblicazione e persistenza del Task 2; non è la review finale del ramo.

**Spec: FAIL / correzioni necessarie. Quality: REQUEST_CHANGES.** Nessun Critical dimostrato; sei Important sotto. La review statica resta possibile durante l'esclusiva TracciaDoc, ma non sostituisce i gate di build e dispositivo.

Letti brief, contesto, piani 02/08/09, revisione iniziale e rapporto implementer; diff letto in blocchi sequenziali. I risultati dichiarati dal rapporto sono claims, non prove ripetute dal reviewer. **Runtime, cargo, build Android e adb: NON ESEGUITI.** Nessuna modifica a sorgenti, indice, HEAD o ramo; nessun accesso a dev/win-test. Le letture aggiuntive sono state limitate ai contratti di resource permit, processi riassunto, persistenza revisione, renderer/i18n e risultati Labs.

## Finding

### 1. Important — Le uscite strutturate vengono appiattite e le directory trattate come file

Evidenza: `android/src/glue.rs:911–935`, in particolare `:922` (inserisce sempre `principale`, anche se directory); `:1041–1047` e `:1102` (ogni pubblicazione prende solo basename e destinazione fissa Documents/Cedro o Download/Cedro). Contratti esterni: `src/engine_svslab.rs:26–42,44–59` e `src/mrxs.rs:625–639`.

`tiles` restituisce una directory: questa diventa la prima uscita, arriva a `File::open`/lettura come se fosse un file e viene segnalata «Da salvare», con Apri/Condividi verso un provider che accetta soltanto file. La copia anonima MRXS restituisce invece un `.mrxs` con la propria directory di dati: la scansione ricorsiva raccoglie i file, ma la pubblicazione li colloca tutti nella stessa directory pubblica. Si perde quindi il rapporto `<nome>.mrxs` / `<nome>/Slidedat.ini`, e i file pubblicati non costituiscono più un vetrino apribile. Non è sufficiente attribuire tutte le righe allo stesso ID.

Correggere il modello di uscita per distinguere file e directory/insiemi collegati e mantenere i percorsi relativi; evitare di pubblicare/condividere una directory con il contratto dei singoli file. Regressioni necessarie: tiles e MRXS con directory associata, verifica che il risultato esportato si riapra.

### 2. Important — Un'interruzione durante la pubblicazione perde l'attribuzione delle uscite già salvate

Evidenza: `android/src/glue.rs:925–940`, `:1030–1033`, `:863–874` e `:2087–2093`.

Il manifest `.pronti` conserva solo i percorsi privati. Per ogni uscita pubblicata con successo `consegna` elimina subito il privato; URI e nome pubblico restano nel vettore locale fino alla conclusione dell'intero lotto e al successivo salvataggio UI. Se l'app viene terminata dopo la prima pagina pubblicata ma prima della conclusione del lavoro, al riavvio `recupera_uscite` esclude quella pagina perché il path privato non esiste più. Il file pubblico non è perso, ma non è più attribuito al lavoro e manca dai suoi comandi Apri/Condividi. Nel caso di una singola uscita può restare soltanto un lavoro «interrotto» nonostante il risultato sia stato pubblicato.

Registrare durevolmente la transizione di ogni uscita da path privato a URI pubblico prima di eliminare la copia privata, e recuperare entrambe le forme con l'identità del lavoro. Verificare arresti controllati tra commit MediaStore, registrazione URI e rimozione privato, anche a metà di un PDF multipagina.

### 3. Important — Esito e lista uscite su due code distinte hanno una race che perde il journal completo

Evidenza: `android/src/glue.rs:940`, `:2084–2093`.

La UI svuota prima `consegne.uscite`, poi `consegne.esiti`. Il worker può inserire uscite ed esito tra queste due letture: la UI vede l'esito, marca il lavoro finito e serializza senza la lista uscite. Al giro successivo aggiunge la lista, ma quel blocco non richiede né persistenza né ridisegno. Se non arriva un altro evento che salva, al riavvio mancano le uscite secondarie già pubblicate. Anche senza terminazione nel mezzo della copia, quindi, la promessa di cronologico con tutte le uscite non è garantita.

Consegnare stato e uscite in un solo evento indivisibile per lavoro oppure sincronizzare esplicitamente completamento, inserimento lista e persistenza. Serve una regressione che forzi l'interleaving descritto.

### 4. Important — Il download sorgente dei lavori web completati rimane per sempre nello staging

Evidenza: `android/src/worker.rs:87–91`; `android/src/glue.rs:916`, `:925–928`, `:959–966`, `:854–859`.

Per web-audio/web-leggero con conversione e per web-testo il download resta in `out/.scaricato`. La pubblicazione ignora i nomi che iniziano con punto; lo staging completo riceve `.pronti`, che impedisce sia la rimozione RAII sia la pulizia all'avvio. Non c'è più la rimozione della directory di download presente nel percorso precedente. Di conseguenza ogni conversione web riuscita può conservare l'intero video originale nella memoria privata senza essere elencato tra le copie recuperabili. Ripetendo lavori grandi lo spazio si esaurisce anche quando tutte le uscite sono state salvate correttamente.

Separare il lifetime del materiale temporaneo da quello delle uscite complete, ed eliminare il download quando non serve più. Per web-testo conservare esplicitamente solo ciò che il contratto della revisione richiede, con riferimenti e cleanup coerenti. Prova necessaria: download+conversione riuscita e confronto dello spazio/staging prima, dopo e dopo riavvio.

### 5. Important — La qualità della conversione web ora riceve l'indice del modello di trascrizione

Evidenza: `android/src/glue.rs:1218`; `android/src/worker.rs:90–91,109`.

`web` passa `modello as u8` nel campo `qualita` per trasportare il modello di web-testo. Nel worker, però, la conversione web-audio/web-leggero cambia solo `azione` e riusa lo stesso campo per `perform_with`. Il percorso precedente chiamava la conversione con qualità 82; ora il codec riceve l'indice del modello (ed eventualmente i bit delle voci), che non è un valore di qualità scelto per il media. La scelta del modello di trascrizione può dunque cambiare anche l'output audio/video e produrre qualità inattesa.

Separare nel protocollo modello/voci e qualità media; preservare la qualità precedente o quella effettivamente selezionata dall'utente. Regressione: web-audio e web-leggero con diverse scelte di modello, verificando che la qualità media non cambi per questo motivo.

### 6. Important — La pubblicazione ora usa API 29 anche sui dispositivi API 26–28 supportati

Evidenza: `android/src/glue.rs:1018–1033`, `:1045–1047`, `:1102`. Il diff elimina il controllo della versione che lasciava la scrittura diretta ai dispositivi precedenti; tutte le uscite sono ora private e passano da MediaStore con `relative_path` e `is_pending`, con fallback a MediaStore.Downloads.

Queste colonne e la raccolta Downloads sono state introdotte in API 29, mentre il piano/build conserva minSdk 26. Sui dispositivi precedenti entrambi i tentativi non costituiscono un percorso di salvataggio supportato: anche una conversione riuscita resta privata/«Da salvare». Occorre un ramo compatibile API 26–28, mantenendo staging atomico e conservazione del privato in caso d'errore.

Contratto Android verificato nella documentazione ufficiale: [MediaStore.MediaColumns](https://developer.android.com/reference/android/provider/MediaStore.MediaColumns), [MediaStore.Downloads](https://developer.android.com/reference/android/provider/MediaStore.Downloads). Non è stata eseguita una prova su tali versioni.

## Punti solidi

- Importazioni spostate su thread transitori, copie a blocchi, directory uniche, nomi sanitizzati e budget basato sullo spazio disponibile con riserva; non è più presente il vecchio Vec dell'intero input nel percorso ordinario SAF.
- Descriptor SAF posseduti da `File`/`Arc`, CLOEXEC nel genitore e rimozione del flag nel solo figlio con `pre_exec`; raccolta dei descriptor anche per i file associati. Nessun uso di JNI nel launcher.
- Processo isolato, IPC distinto da stdout e guardia che termina il gruppo e attende il figlio; cancellazione disponibile anche per lavori in attesa.
- Il permit è nel genitore e `resources::acquire` è rientrante sullo stesso thread (`src/resources.rs:37–40`): il passaggio YouTube→trascrizione non introduce il deadlock ipotizzato inizialmente.
- La copia privata non viene più eliminata in caso di errore MediaStore; il provider limita le copie private alla directory autorizzata e le espone in sola lettura. Il journal v2 distingue attivi/errori e trasforma gli attivi in interrotti al riavvio.
- Test aggiunti per copie/cancel, nomi, roundtrip opzioni e stati persistenti; sono utili, ma **non eseguiti** e non coprono i casi concorrenti/strutturati sopra.
- Le nuove chiavi delle cinque lingue sono nel diff. Il renderer condiviso applica `tr()` a text/measure, quindi l'assenza di `tr()` esplicito in una singola chiamata `p.text` non basta per dichiarare un difetto i18n.

## Non verificabile staticamente / gate obbligatori

Non sono dimostrati: type-check Rust/JNI, linking/export/dlopen del launcher, esecuzione da nativeLibraryDir sotto SELinux, differenza dimensione APK, accesso diretto SAF di un vetrino reale ≥1 GB, comportamento di provider lenti o con permessi revocati, assenza di processi residui, shutdown del servizio e fedeltà visiva sul telefono. La conservazione delle lease per l'intera sessione va misurata con cartelle MRXS grandi; non è una prova di consumo accettabile. L'API firma bytes resta quella autorizzata dal controller: l'allocazione fallibile fuori UI non dimostra il picco RAM, che rimane gate Task 3.

Dopo il rilascio dell'esclusiva, sincronizzare la revisione corretta nella directory autorizzata `/srv/dev/projects/cedro-android-fase1`; solo sul minipc eseguire:

```sh
cargo test --release -p cedro-android --lib
cargo test --release -p cedro
cargo test --release -p cedro --test i18n_inglese
cargo ndk -t arm64-v8a -P 26 check -p cedro-android --all-targets
android/build.sh
CEDRO_YOUTUBE=1 android/build.sh
```

Sul Mi 9T, senza eliminare dati personali: import omonimi concorrenti; SAF seekable ≥1 GB e provider pipe con cancel; MRXS annidato >200 file; riavvio e revoca URI; annullamento codec/trascrizione/download/riassunto/YouTube e secondo lavoro in attesa; crash del figlio; pubblicazione multipla, fallimenti MediaStore simulati, Replace/Number/Skip, interruzioni nei punti indicati nei finding; RAM/tempo/spazio/CPU/processi/servizio misurati. Aggiungere una verifica del percorso API 26–28 in ambiente appropriato oppure dimostrare il ramo compatibile prima di dichiararlo supportato. Xiaomi 13 rimane escluso.

Le prove del rapporto implementer (parsing Rust/TOML/XML, TSV/bash/diff) non equivalgono a compilazione o collaudo. **Nessun PASS build/test/telefono e nessuna idoneità APK/release sono dichiarati da questa review.**
