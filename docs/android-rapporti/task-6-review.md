### Spec Compliance

- **❌ Issues found — 3 Important, 0 Critical, 0 Minor.** Il ponte accessibile non porta click/scroll all'input Rust e non riceve gli hover consumati dal native loop (I1); l'attesa del lock nativo non è cancellabile nel player del processo UI (I2); la generazione semantica può emettere ID duplicati (I3).
- **⚠️ Cannot verify from diff:** TalkBack/UIAutomator sul candidato, sei lingue e automatico/fallback inglese, de/pt a 360 dp e font130/200, landscape, almeno una frase raggiungibile in Onda e testo/Referto, header dopo scroll, percorsi intent/completion con tre destinazioni, protocollo fd del ffmpeg concreto, concorrenza dei decoder Android e ripristino degli alias dopo riavvio. La review statica non chiude questi gate.
- **⚠️ Gate di consegna del controller:** APK finale senza asset/stringhe dell'edizione ritirata, licenze/dimensioni/dipendenze, confronto base/finale cold start/PSS aggregato UI+worker/CPU/render/batteria/lavori, clippy e suite/regressione CLI sul checkpoint finale. Xiaomi13 escluso esplicitamente, non richiesto come blocco. Il rapporto distingue correttamente queste prove ancora pendenti (`docs/android-rapporti/task-6-report.md:126`).

### Strengths

- `src/input_file.rs:43`: apertura dei grant vincolata sia al percorso sia al numero del symlink, con rifiuto degli alias non registrati; `:97` e `:111` usano pread e cursori logici per lettore. Il test stale alias (`:189`) copre il caso specifico del fd riusato. Non c'è una nuova materializzazione preventiva del vetrino su disco.
- `src/input_file.rs:63`: lockfile con open file description separata, identità dev+ino, LOCK_NB e guard RAII. `src/mediadec.rs:43`/`:286` mantengono il guard dopo extractor/codec nell'ordine di distruzione; `src/linux_video.rs:65` rilascia il decoder prima dell'audio. La serializzazione della stessa sorgente è la scelta autorizzata, non è un difetto in sé.
- `src/trascrizione.rs:1539`: verifica il protocollo fd nel binario, rifiuta il fallback pathname per i grant e riavvolge il cursore nativo sotto guard; l'ereditarietà del fd viene aperta soltanto nel figlio. Il ramo di cancellazione attende l'uscita del figlio prima di rilasciare il guard (`src/trascrizione.rs:1586`).
- `android/src/ui.rs:699` e `:1164`, `android/src/ui/casa.rs:542`: reset scroll su import e ripristino esplicito della route Casa/Lavori. `android/src/ui.rs:1359` ridisegna dopo il clamp; `:2329` limita i bersagli Aspetto alla viewport. `android/src/ui/casa/telefono.rs:307` misura le famiglie su righe adattive.
- `src/ui/gfx.rs:1295` applica la stessa scala a misura e disegno; `android/src/ui/rivedi.rs:604` invalida le altezze al cambio fontScale e `:535` permette al contenuto di scorrere nelle viewport corte. La prova sorgente `:1105` verifica la raggiungibilità della frase e la presenza dell'export, senza sostituire l'ispezione visiva.
- `build.rs:91`, `src/i18n.rs:268`, `src/edizione.rs:23` e `src/ui/theme.rs:27`: esclusione Android delle tabelle/risorse dell'edizione ritirata, preservando il catalogo Android e le risorse desktop. Il nuovo caso marker Labs (`src/settings.rs:402`) resta limitato alle configurazioni nuove. Nessun cambio di versione desktop/workflow/Cargo dependency/F-Droid nel diff.
- `android/README.md:114` motiva l'eccezione Java e dichiara il limite shaping; `android/note-release-0.53.0.md:1` usa il formato richiesto; `docs/RILASCI.md:118` descrive un candidato, non una pubblicazione già avvenuta. I rapporti precedenti sono archivi storici con checkpoint distinti, non nuova produzione da riesaminare.

### Issues

#### Critical (Must Fix)

- Nessuno dimostrato.

#### Important (Should Fix)

**I1 — [P1] Il ponte accessibile usa il percorso Java che salta l'InputQueue nativa.** `android/java/it/cedro/android/CedroActivity.java:186–190`, `:242–255`; riscontro nel ricevitore `android/src/glue.rs:1877–1895`.

`touch()` chiama direttamente `CedroActivity.this.dispatchTouchEvent(e)`. Questo inoltra l'evento alle View Java; non lo inserisce nella InputQueue dalla quale Rust legge con `app.input_events_iter()`. La View trasparente restituisce false e non esiste qui un inoltro Java→Rust. Di conseguenza ACTION_CLICK e ACTION_SCROLL dichiarano successo (ed emettono anche l'evento click), ma non eseguono il comando Rust. Il test host `android/src/ui/accessibilita.rs:58` chiama direttamente `app.tocca_giu/su` e salta proprio il ponte difettoso. Inoltre il loop Rust conserva `InputStatus::Handled` per gli hover ignorati: ciò impedisce al nuovo `onHoverEvent` Java di ricevere l'esplorazione tattile.

È un'inferenza sul codice applicativo supportata dal percorso AOSP: NativeActivity prende l'InputQueue; NativePostImeInputStage la consegna al native code prima delle View e ferma gli eventi gestiti; Activity.dispatchTouchEvent entra direttamente nella dispatch della finestra. Fonti primarie: [NativeActivity](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/android/app/NativeActivity.java#117), [ViewRootImpl, NativePostImeInputStage](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/android/view/ViewRootImpl.java#7837), [Activity.dispatchTouchEvent](https://raw.githubusercontent.com/aosp-mirror/platform_frameworks_base/master/core/java/android/app/Activity.java). Non è una prova runtime MIUI.

Correggere con una coda di azioni/input Java→Rust consumata nel thread UI Rust, riutilizzando il dispatch esistente, e consentire agli hover di raggiungere il provider (o inoltrarli esplicitamente). Verifica richiesta al controller: ACTION_CLICK su Lavori deve cambiare davvero route, ACTION_SCROLL deve cambiare il contenuto, esplorazione TalkBack deve spostare il focus. Un albero XML popolato o javac/d8 PASS non basta.

**I2 — [P2] Il player genitore non può annullare l'attesa del lock nativo.** `src/mediadec.rs:85` e `:305`; `src/input_file.rs:19–23`, `:76–80`; unico collegamento del callback in `android/src/worker.rs:80`.

I decoder chiamano `lock_native(input_file::cancelled)`, ma il callback globale è impostato soltanto nel processo worker. Il player Android gira anche nel genitore: `android/src/ui/rivedi.rs:117` → `src/ui/player.rs:179`/`:211` → `src/audio.rs:335` → AudioDecoder. In quel processo `cancelled()` rimane false. Se un worker sta decodificando lo stesso grant, aprire e poi chiudere il player lascia il suo thread ad attendere l'intero lock; il `stop` impostato da `Player::drop` (`src/ui/player.rs:197`) non viene consultato. Dopo la chiusura può persino iniziare la decodifica ormai inutile quando il lock si libera. Il lavoro non blocca il thread UI, ma viola la cancellabilità richiesta e conserva thread/lease inutili durante lavori lunghi.

Passare un token/callback del singolo consumatore fino all'acquisizione del lock, combinato dove necessario con la cancellazione del worker. Coprire la chiusura del player mentre un altro processo tiene il lock e verificare che il waiter termini senza acquisirlo dopo la chiusura. Il test `native_lock_is_interprocess_cancellable_and_leaves_pread_free` prova il meccanismo con una closure propria, non questo collegamento reale.

**I3 — [P2] Più bersagli della stessa azione producono nodi virtuali con lo stesso ID.** `android/src/ui/accessibilita.rs:43–46`; consumo in `android/java/it/cedro/android/CedroActivity.java:163–165`, `:213`.

L'ID dipende soltanto da `Debug(action)`, ma viene aggiunto un nodo per ogni hit shape. Il caso reale Rivedi registra `RivAz::Riga(i)` sia per il timestamp sia per il testo (`android/src/ui/rivedi.rs:660`, `:682`). Quando sono entrambi visibili, entrambi i nodi hanno lo stesso ID. Java sovrascrive il primo in SparseArray, conserva due occorrenze in `order` e aggiunge due volte lo stesso figlio: testo/bounds del primo bersaglio sono persi e traversal/focus ricevono una gerarchia ambigua. Non è una collisione probabilistica dell'hash.

Aggregare intenzionalmente le regioni della medesima azione in un unico nodo, oppure assegnare ID stabili distinti alle regioni e conservarne il dispatch. Aggiungere un controllo di unicità degli ID su una revisione reale con frase e timestamp visibili, oltre alla verifica di navigazione assistiva dopo I1.

#### Minor (Nice to Have)

- Nessuno aggiuntivo.

### Evidenze, controlli mirati e limiti

- Review del package `review-f5dcc48..3d1629b.diff`, 13 commit, 79 file: codice letto per sezioni; recuperate soltanto porzioni tagliate dall'output. Gli archivi Task1–5/8 sono stati controllati per natura storica, claim/checkpoint e riferimenti, senza ripetere le loro review.
- Rischio nominato **lifetime/lock/cancellazione nativi**: controllati i callsite AudioDecoder, il player e la sua distruzione, `wait_or_cancel`, `config_dir` e l'ambiente Android condiviso. I body mancanti nel diff di `rivedi` e dei helper esterni sono stati letti solo perché gli hunk erano tagliati prima dei punti necessari a giudicare i lifetime e i bersagli semantici.
- Rischio nominato **dispatch del provider verso NativeActivity**: controllato il loop InputQueue Android, l'assenza di inoltro nel Java e le tre fonti AOSP sopra. Rischio nominato **stringa del tema nell'APK**: controllato il solo `ThemeId::key`, che delega a `edizione::CHIAVE_TEMA`, già reso vuoto su Android; audit binario ancora pendente.
- Letto `qa-saf-4c67b2c-report.md`: SAF reale NDPI 1.473.064.608 byte, panoramica/centro/ritorno PASS; fstat/pread validi contro pathname EACCES. Questo sostiene il modello descriptor/pread, non certifica i successivi guard per percorso o lock nativi. Il rapporto non misura picco UI+worker né occupazione privata su disco, e questa review non glieli attribuisce.
- Evidenze Java su d1aeb88: javac+d8 PASS con nota API deprecated. NDK faf24f3 comunicato dal controller: typing intermedio, non test dell'head finale. `task-6-report.md:67` distingue le prove intermedie; clippy/suite finali sono in carico al controller. Warning vendor già documentati e nota deprecated restano rumore dichiarato, non output presentato come pulito.
- Nessun test/build/Rust/fmt/ADB/comando remoto/git eseguito da questa review. Nessuna mutazione di sorgenti/index/branch; scritto soltanto questo rapporto. Le verifiche raccomandate sopra devono essere integrate nei gate del controller, non duplicate come suite indipendente.

### Assessment

**Spec compliance: Needs fixes; gate fisici e di confezionamento ancora non verificati.**

**Task quality: Needs fixes — 0 Critical, 3 Important, 0 Minor.** La lettura posizionale e il lifetime dei guard sono impostati coerentemente, ma i comandi accessibili attualmente non raggiungono il dispatcher nativo e la cancellazione del lock non copre il player genitore. Correggere questi collegamenti e gli ID semantici prima di considerare chiuso il Task6; nessun PASS di pubblicazione è implicito.
