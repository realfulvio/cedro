# Task 2 — Re-review fix round 1

Pacchetto `8aacdb2..89c422f`; fix `89c422f`, più due commit documentali del controller. Ambito limitato ai sei Important della prima review e alle rotture introdotte dal fixdiff.

**Spec: APPROVATA STATICAMENTE nel perimetro dei sei finding. Quality: APPROVATA STATICAMENTE nel fixdiff.** Tutti e sei i finding risultano **ADDRESSED**. Nessun nuovo Critical/Important dimostrato nel fixdiff. Questo giudizio non è un PASS di build, test, APK o telefono.

Letti brief, vincoli globali, sezione «Correzioni review» del rapporto e pacchetto diff in sequenza. Recuperate soltanto le funzioni troncate dall'output del primo blocco; verificato in modo mirato il contratto di riapertura MRXS utilizzato dal test. Nessun git diff aggiuntivo, modifica sorgenti/index/branch, subagent, cargo, adb o accesso agli ambienti riservati. Unico output della review: questo documento.

## Verifica dei sei finding

| # | Verdetto | Evidenza e motivazione |
|---|---|---|
| 1 — Uscite strutturate appiattite/directory trattate come file | **ADDRESSED** | `android/src/publication.rs:57–83,129–134` distingue File/Directory/Insieme, raccoglie solo file regolari e conserva i relativi; il namespace comune comprende sia header MRXS sia cartella dati. `android/src/glue.rs:989–998,1092–1122` passa sottocartella e base pubblica per ogni elemento, fissando la base comune alla prenotazione. Non viene più accodata la directory principale come file. Il test `publication.rs:223–245` esporta e riapre MIRAX col motore, confronta pixel e controlla il tile annidato; il contratto `Slide::read_level` restituisce effettivamente un'immagine RGB (`src/svs.rs:401`). Test **NON ESEGUITO**; il percorso MediaStore reale resta da provare. |
| 2 — Crash durante la pubblicazione perde attribuzione | **ADDRESSED** | `publication.rs:7–49,100–120` persiste identità e transizioni Privato→Preparato→Pubblicato; la cancellazione del privato segue il salvataggio del journal. `glue.rs:1182–1191` registra la destinazione prima della copia; `glue.rs:870–896,919–942` recupera Preparato verificando `is_pending`, conserva gli errori di query e non li scambia per successo. `ui.rs:749–767` riconcilia con la stessa chiave lavoro. Le uscite pubblicate prima dell'interruzione restano nel journal anche senza copia privata. Il test delle transizioni (`publication.rs:247–268`) copre il lotto parziale e il checkpoint Pubblicato precedente alla rimozione; non simula un provider Android. |
| 3 — Race fra code esito/uscite | **ADDRESSED** | `ui.rs:85–96,743–747` introduce un solo Completamento con stato, uscite e sorgente. `glue.rs:2149–2155` drena una sola coda, applica tutto e quindi persiste/ridisegna. L'interleaving della prima review non può più separare lista ed esito. Il test con barrier e riavvio (`ui.rs:2383–2407`) controlla il confine drain/enqueue e l'identità, ma è **NON ESEGUITO**. |
| 4 — Download web lasciato nello staging | **ADDRESSED** | `worker.rs:64–82,112–122` assegna `.scaricato` a Scratch, sposta il passthrough fuori dallo scratch e conserva separatamente il solo input di revisione. `glue.rs:1028–1035` rimuove lo scratch anche nel genitore; `publication.rs:137–148` protegge e recupera l'audio quando una revisione è stata effettivamente salvata, anche se il worker si interrompe prima dell'esito. `glue.rs:1273–1279,1319–1326,1340–1349` aggiorna i percorsi web e YouTube. Il test `worker.rs:226–240` copre spostamento e lifetime di base; non misura spazio reale e non esegue Whisper. |
| 5 — Modello voce usato come qualità media | **ADDRESSED** | `worker.rs:5–19,76–79,124` separa Voce da qualita; `glue.rs:1273` passa qualità media 82 e selezione vocale distinta. La trasformazione web→mp3/mp4 modifica soltanto l'azione. Il test `worker.rs:214–224` controlla sei codici modello/voci, tre azioni e roundtrip JSON. **NON ESEGUITO**. |
| 6 — API29 usata su API26–28 | **ADDRESSED** | `glue.rs:1095–1104` effettua il branch prima dell'accesso MediaStore moderno. `publication.rs:150–199` implementa copia legacy con temporaneo, sync/rename, Number/Replace/Skip e marker di mancato commit; un errore non elimina il privato né scambia un vecchio Replace per il nuovo finché il temporaneo esiste. Il test `publication.rs:270–279` verifica il caso cancel/Replace e la selezione delle API; **NON ESEGUITO**. La compatibilità effettiva del filesystem/provider e dei permessi API26–28 resta un gate. |

## Valutazione dei contratti più delicati

Il journal delle uscite risolve il buco fondamentale del primo round: il risultato pubblico non dipende più dal completamento dell'intero lotto e dal successivo giro UI. `Preparato` contiene già la destinazione, perciò un commit MediaStore avvenuto prima della morte del processo può essere riconosciuto dopo il riavvio. Dopo la prenotazione il fix evita di sostituire quell'URI con un fallback ambiguo; se l'esito del provider non è accertabile, mantiene il privato. La verifica della query e dell'ordine reale fra scrittura, IS_PENDING e checkpoint non è dimostrata da un test delle sole strutture Rust.

Il gruppo MRXS mantiene lo stesso namespace e i percorsi relativi in entrambe le implementazioni di pubblicazione. La prova sorgente riapre un fixture realmente leggibile dal motore, quindi è più significativa di una semplice asserzione sui nomi. Resta indispensabile il vetrino reale e il percorso MediaStore: il test usa l'esportazione legacy nel filesystem host.

La fonte web-testo ha ora un lifetime esplicito, separato dai byte temporanei di download. Il controllo `revisione::has` prima del cleanup tiene conto del caso in cui Whisper abbia salvato la revisione ma non abbia ancora consegnato l'esito. Il test Scratch controlla lo spostamento e la riapertura del file; non dimostra ancora recupero di una vera revisione dopo annullamento/crash. Questo è un limite delle prove, non un nuovo finding dimostrato nel fixdiff.

## Gate ancora obbligatori

**Runtime/Rust/Android: NON ESEGUITI dal reviewer.** Parsing e controlli statici dichiarati dall'implementer non verificano tipi, borrow checker, JNI, linker, ABI o comportamento del provider. I dati baseline aggiunti dal controller riguardano la versione precedente e non certificano `89c422f`.

Dopo la liberazione del minipc, nella copia autorizzata `/srv/dev/projects/cedro-android-fase1`, eseguire i comandi già prescritti, sulla revisione effettivamente sincronizzata:

```sh
cargo test --release -p cedro-android --lib
cargo test --release -p cedro
cargo test --release -p cedro --test i18n_inglese
cargo ndk -t arm64-v8a -P 26 check -p cedro-android --all-targets
android/build.sh
CEDRO_YOUTUBE=1 android/build.sh
```

Prove mirate ancora necessarie prima dell'APK/release:

- Mi 9T: arresti controllati dopo prenotazione URI, dopo IS_PENDING=0 e prima/dopo Pubblicato, a metà di un lotto; attribuzione e riapertura di tutte le uscite al riavvio. Includere Number/Replace/Skip e errori provider, senza prove distruttive dello spazio reale.
- MRXS reale e tiles: salvataggio e riapertura dal filesystem pubblico, mantenimento dei percorsi relativi e comportamento del gruppo quando il fallback o una singola uscita fallisce.
- Web audio/video: qualità indipendente dal modello vocale, spazio privato prima/dopo successo, annullamento e riavvio; web-testo e YouTube con revisione realmente riapribile e audio conservato.
- API26–28: copia/rename/scansione e permessi effettivi; cancel/Replace lascia il vecchio risultato integro e il nuovo privato recuperabile.
- Restano i gate Task2 già elencati per SAF ≥1GB, launcher/IPC/fd, cancellazione e assenza figli residui, memoria complessiva UI+worker e servizio. Xiaomi 13 escluso come richiesto.

## Fuori perimetro

Non rivalutati l'intero ramo, le UI delle fasi successive o i risultati documentali dei due commit del controller. Non si apre un nuovo ciclo su questi ambiti. L'approvazione qui riguarda la correzione statica dei sei finding; pubblicazione autorizzabile solo dopo i gate richiesti.
