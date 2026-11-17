# Task 3 — Review di specifica e qualità

## Spec Compliance

- ❌ **Issues found** sul confronto `89c422f2a65c0257a77a6bbf7ebb17a5056cb5dc..f49241e957fe099211afbbe6bfb329d9aa1e19fe`: invalidazione incompleta della preview quando scompare la sorgente selezionata (`android/src/ui/labs.rs:109`), renderer non compilabile (`android/examples/vista_a.rs:57`, `:61`), orario/data rimossi dalla scheda dei risultati (`android/src/ui.rs:2077`). Dettagli e rimedi sotto.
- ✅ Collegamenti verificabili nel diff: catalogo generato dal motore (`android/src/ui.rs:331`), opzioni Office/vetrini/DICOM/microscopia (`android/src/labs.rs:42`), preview della sorgente con lettori condivisi e area vetrino limitata (`android/src/labs.rs:57`), caso fotografato all'avvio del lavoro (`android/src/ui.rs:1132`) e conservato nel journal/publication (`android/src/ui.rs:702`, `android/src/publication.rs:17`), strumenti aggregati e pacchetti (`android/src/worker.rs:142`).
- ✅ Le eccezioni concordate sono rispettate nel codice: errore contestuale per copia anonima CZI/ND2/LIF prima della lettura (`android/src/worker.rs:78`); selettori di preview distinti dalle opzioni export e ambito esplicitato (`android/src/ui/labs.rs:191`); Options propagate ai percorsi web/trascrizione (`android/src/glue.rs:1062`). Il completamento dei nomi dei download passthrough/YouTube rimane Task 4, non viene riclassificato come difetto di Task 3.
- ✅ Le modifiche condivise restano circoscritte al reexport RX, font Android e correzioni autorizzate dei nomi/gruppi MRXS/tessere e degli alias MRXS; nessuna nuova interpretazione clinica o dato dimostrativo nei moduli di produzione. La fixture DICOM è inclusa da test/esempio (`android/tests/support/dicom.rs:1`, `android/examples/vista_a.rs:9`).

### ⚠️ Cannot verify — gate precisi

- **Compilazione del candidato:** il controller ha comunicato sullo stesso HEAD il fallimento NDK `--all-targets` con E0616 nell'esempio; lib/libtest hanno superato il type-check, ma questo non equivale a test eseguiti o APK valido. Dopo I1 servono NDK tutti i target, renderer host e build APK con/senza YouTube sul mini-PC. Evidenza comunicata dal controller: `/srv/dev/scratch/cedro-android-f49241e-ndk.log`; non rigenerata da questa review.
- **Test del candidato:** risultati host di questo HEAD ancora pendenti al momento della review. Richiesti `cedro-android --lib`, test core nuovi MRXS/tessere, i18n e regressione core/CLI indicati nel report. Il precedente 16/17 su `acc2b39`, il parsing Tree-sitter e il precedente NDK PASS non sono prove di questo HEAD. La modifica del test float è limitata all'uguaglianza della misura 56 dp, con tolleranza 0,001 dp (`android/src/ui/casa/telefono.rs:580`); il suo PASS resta da ottenere.
- **Mi 9T:** cancel effettivo/process group, fd ereditati/provider SAF, nessuna copia privata del vetrino seekable ≥1 GB, correttezza dei risultati MRXS e ripristino del caso dopo chiusura/crash. Registrare tempo e picco PSS complessivo UI+worker per SVS/NDPI/MRXS/CZI/ND2/LIF/DICOM. DICOM e compositi micro possono ancora allocare dati completi (`android/src/labs.rs:73`, `:85`): isolamento del worker non dimostra sufficienza RAM né un errore recuperabile per ogni file grande.
- **Office/HEIC e robustezza:** cinque Word reali contro desktop (±1 pagina/font), Word 100 pagine, Excel 20 fogli/50.000 righe, HEIC reali 12/48 MP con P3/rotazione, `.doc` malevolo con errore <5 s e RAM <500 MB; banco rovinati singolo e in coda con zero crash/ANR e soglie del piano. Sono gate dichiarati nel report, non successi dimostrati dal diff.
- **Firma/PDF/anonimizzazione:** gravità SHA1 e post-modifica, identità dei rapporti desktop/telefono, metadati PDF/allegati e confronto DICOM/vetrino anonimo richiedono fixture e output reali. I relativi lettori/verificatori sono per lo più codice invariato; nessuna review estesa di quei sottosistemi è stata effettuata.
- **UI e lingue:** aggiunte nelle cinque tabelle presenti; fedeltà UI A, assenza di tagli IT/DE/PT a 360 dp e caratteri grandi, accessibilità di ogni formato, prestazioni dello scorrimento e dimensione APK richiedono renderer corretto e screenshot/prove sul Mi 9T. Il test catalogo a `android/src/ui/labs.rs:297` confronta `righe()` con il motore, non tutte le tessere effettivamente raggiungibili nei diversi percorsi UI. Intestazioni fisse/richiamabili delle famiglie e note di release finali non sono dimostrate da questo diff.

## Strengths

- **Opzioni reali e separazione dei ruoli:** `android/src/labs.rs:42` compone le definizioni del core, evita la carta testuale per i fogli e non apre i file nel loop UI. `android/src/ui/labs.rs:104` gestisce lo stato della preview; `android/src/glue.rs:2105` porta decodifica e staging nel worker con permit, lease e guardia di cleanup. I risultati con identità superata sono ignorati (`android/src/ui/labs.rs:125`), salvo il caso di assenza sorgente descritto in I2.
- **Snapshot del caso e privacy della nota:** codice del caso inserito all'avvio (`android/src/ui.rs:1132`), ripristino dal journal e campo publication con default retrocompatibile. Il test `caso_snapshot_nomi_e_quaderno_restano_dopo_riavvio` controlla Options, esclusione della nota, cambio caso, serializzazione e quaderno (`android/src/ui/labs.rs:231`). La persistenza fuori UI è serializzata e segnala errori (`android/src/glue.rs:1929`).
- **Output strutturati preservati:** MRXS e tessere applicano il modello prima di scegliere il nome del gruppo, senza rinominare dopo la generazione (`src/engine_svslab.rs:28`, `:47`). I nuovi test controllano collisioni, riapertura dei pixel, nomi CSV e caso una sola volta (`src/engine_svslab.rs:121`). La copia MRXS conserva i percorsi relativi, materializza alias di file e rifiuta directory collegate (`src/mrxs.rs:633`); il test usa un fd reale e controlla cleanup dopo errore (`src/mrxs.rs:836`).
- **Test con comportamento osservabile:** anteprima DICOM confrontata in pixel al cambio frame/finestra (`android/src/labs.rs:147`), preview SVS/CZI da fixture (`android/src/labs.rs:127`), BagIt verificato con impronte e sidecar (`android/src/worker.rs:230`). Sono test utili, pur non essendo ancora eseguiti su questo HEAD al momento della review.
- **Avvisi più leggibili:** il risultato conserva messaggio multilinea e copia integrale (`android/src/ui.rs:1321`, `:2077`); nota allegati prima di Dividi/Pagine (`android/src/ui/pdf.rs:673`). Il report distingue correttamente verifiche statiche, limiti del core e prove rinviate.

## Issues

### Critical (Must Fix)

Nessuno rilevato nel perimetro della review.

### Important (Should Fix)

**I1 — Il renderer non compila e blocca il gate di tutti i target.**

- **Dove:** `android/examples/vista_a.rs:57`, `android/examples/vista_a.rs:61`.
- **Causa:** le nuove scene assegnano direttamente `app.scorri`, campo privato di `App`; un esempio è un crate esterno rispetto alla libreria. Il controller ha ottenuto E0616 con NDK `--all-targets` sullo stesso HEAD.
- **Impatto:** il renderer previsto per validare Labs/DICOM/caso non può essere eseguito e il controllo di compilazione richiesto fallisce. Il parsing riportato non rileva questa violazione di visibilità.
- **Rimedio:** usare un'interfaccia pubblica mirata per impostare lo scorrimento nelle viste/esempi, oppure azioni pubbliche già disponibili, evitando di esporre indiscriminatamente lo stato interno. Far rieseguire al controller NDK tutti i target e renderer host, poi verificare le scene prodotte.

**I2 — Rimuovere l'ultima sorgente non invalida né cancella la preview.**

- **Dove:** `android/src/ui/labs.rs:104`–`:125`, in particolare `:109`; cancellazione glue a `android/src/glue.rs:2098`–`:2108`; disegno dell'immagine a `android/src/ui/labs.rs:156`.
- **Causa:** quando nessuna voce selezionata è più una sorgente Labs valida, `let path=path?` ritorna senza cancellare `richiesta` né pulire `path`, `info`, `immagine` e `errore`. La pagina resta visibile, quindi il glue non entra nella cancellazione per uscita; non viene neppure avviata una nuova richiesta che cancelli quella precedente.
- **Riproduzione deterministica:** aprire/attendere una preview, togliere o deselezionare l'ultimo vetrino/DICOM restando in Labs. L'immagine e i metadati precedenti continuano a comparire; se il worker era ancora attivo, il risultato viene accettato perché conserva lo stesso id. Lo stesso problema si presenta passando a una categoria senza sorgenti Labs valide.
- **Impatto:** la preview non rappresenta più una sorgente attualmente selezionata e il lavoro costoso continua inutilmente. In un flusso con più campioni è facile confondere il campione mostrato con la selezione corrente.
- **Rimedio:** trattare `None` come una transizione esplicita di sorgente: invalidare l'id, cancellare il token del worker e pulire metadati/immagine/scelta. Conservare il rifiuto degli esiti già in volo. Aggiungere un test che parta con richiesta attiva, rimuova/deselezioni l'ultima sorgente, applichi l'esito tardivo e verifichi stato vuoto; controllare il cancel reale sul Mi 9T. Il test attuale a `android/src/ui/labs.rs:255` copre il cambio selettore, non la scomparsa della sorgente.

### Minor (Nice to Have)

**M1 — La cronologia perde data/orario dei risultati completati.**

- **Dove:** `android/src/ui.rs:2077`.
- **Causa:** la riga precedente includeva `quando(l.quando)` insieme al messaggio; la nuova resa multilinea disegna soltanto `msg` e non reinserisce l'orario altrove nella scheda completata.
- **Impatto:** i risultati del caso mostrano caso e nomi ma non il riferimento temporale richiesto dalla specifica 09 e presente nell'HTML approvato. Anche la cronologia ordinaria perde l'informazione già disponibile.
- **Rimedio:** aggiungere una riga o etichetta temporale distinta dal messaggio multilinea e includerla nel calcolo dell'altezza, conservando visibilità con testi lunghi e caratteri grandi.

## Metodo e controlli mirati

- Review limitata a Task 3, brief/interfacce/vincoli, specifiche 04/07/08/09 e sezioni pertinenti della UI A approvata (`docs/android-anteprime/rework/index.html:59`, `:60`, `:63`). Seguito il template `task-reviewer-prompt.md` completo.
- Diff preparato letto per sezioni; le prime risposte del tool erano troncate, quindi i segmenti interessati sono stati recuperati con output più piccoli. Nessun `git diff` rigenerato e nessun comando git eseguito. Riferimenti numerici ricavati dagli header degli hunk.
- **Rischio alias MRXS nel pacchetto:** lookup unico mirato di `src/pacchetto.rs:30`–`:57`: il raccoglitore accetta symlink a file regolari e preserva i percorsi relativi, rifiutando directory collegate. Non conferma da solo il comportamento del provider Android reale.
- **Rischio lifetime fd associati nel worker preview:** lookup unico di `android/src/glue/saf.rs:40`–`:50`: lease di sessione e raccolta dei file sotto l'importazione includono i dati MRXS, oltre all'header.
- **Rischio fallback font della famiglia sbagliata:** lookup mirato della dipendenza `generic`, assente dall'hunk incompleto di `Fonti`, a `src/ufficio/font.rs:200`–`:208`: i tre rami restituiscono i nomi testati dal nuovo fallback Android. Non sostituisce il confronto di paginazione reale.
- **Rischio orario perso:** completato soltanto il fondo di `scheda_lavoro`, il cui hunk termina a metà funzione (`android/src/ui.rs:2120`–`:2150`): nessuna resa temporale aggiunta dopo i pulsanti.
- Nessuna suite, build, cargo, adb o accesso remoto eseguiti dal reviewer. Nessun codice, indice, HEAD o stato del ramo modificato; unico file scritto: questo rapporto. Il documento `docs/android-rapporti/02-base.md` del controller è escluso dai finding di Task 3.

## Assessment

**Task quality: Needs fixes.**

Due finding Important e uno Minor; nessun Critical. L'integrazione riusa coerentemente motore e infrastruttura Task 2, con test sostanziali per output e snapshot; va corretta la transizione della preview senza sorgente e ripristinata la compilazione del renderer prima di chiudere i gate di test/build/telefono. Questa review non attribuisce PASS a prove ancora pendenti.
