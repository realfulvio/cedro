### Spec Compliance

- ❌ Issues found: recupero durevole delle raccolte, riuso dei risultati con qualità diversa e controlli effettivi del percorso generico → Testo richiedono correzioni (I1–I3).
- ⚠️ Cannot verify from diff: test host completi del candidato **46ec8ea**, rendering IT/DE/PT, risoluzioni reali, percorso senza YouTube, notifica in background, primo uso isolato, byte Range dopo ritorno Wi-Fi e annullamento sul Mi 9T. Le prove **33/33 host + NDK** riferite nel rapporto appartengono soltanto al checkpoint **aefda8d**. Il controller ha successivamente comunicato sul candidato **46ec8ea**: core default `cargo check --release -j2 -p cedro --lib` PASS (142,32 s), NDK all-targets con YouTube PASS (20,27 s) e senza YouTube PASS (3,97 s). Questi esiti sono comunicati dal controller, non rieseguiti dal reviewer; test host ancora in corso. Il precedente errore serde risulta quindi risolto nella build core. Resta il nuovo warning M2. Controller: conservare questi gate prima della pubblicazione, senza pm clear, cancellazioni di dati/componenti o Xiaomi 13.
- ⚠️ Cannot verify from diff: MP4 corrotto/durata impossibile con errore <10 s, CPU idle e RAM <300 MB: nessuna prova runtime del candidato è disponibile nel rapporto. Maven riprende la risposta HTTP interrotta durante il download attivo (`src/youtube.rs:1502`); non persiste il progresso ZIP oltre la terminazione. La specifica richiede esplicitamente il ritorno del Wi-Fi a metà download, non esplicitamente la ripresa dopo riavvio: questa limitazione documentata non viene classificata da sola come violazione, ma manca ancora la prova byte prima/dopo.

### Strengths

- `android/src/web.rs:18`, `android/src/worker.rs:142`, `android/src/ui.rs:1149`: snapshot delle scelte separato da qualità media e modello/voci, con test di serializzazione e collegamento UI→Request. Le opzioni YouTube raggiungono il Request condiviso.
- `android/src/worker.rs:54`, `android/src/worker.rs:257`, `android/src/web.rs:215`: protocollo duplex con ACK e cleanup dei parziali per device/inode, senza scansione distruttiva dei componenti preesistenti. Il signal handler modifica soltanto l’atomico; la terminazione del gruppo rimane limitata al figlio.
- `android/src/web.rs:137`, `android/src/glue.rs:1006`: piano batch committato prima dei rename; pubblicazione/riuso passano dal journal prima della rimozione della copia privata. Sono buone basi, anche se l’integrazione di recupero e il fallback Skip presentano I1/I2.
- `android/src/web.rs:57`, `android/src/glue.rs:2339`, `android/src/ui.rs:771`: stato componente per lavoro, scadenza errore, stessa descrizione per UI/notifica e rimozione del redraw continuo motivato dal solo lavoro attivo.
- `src/youtube.rs:1834`, `android/src/ui/casa.rs:656`: fixture deterministica del 429 estesa e delega esplicita a un’altra app nella build senza YouTube; nuovi testi e permesso di rete documentati.

### Issues

#### Critical (Must Fix)

- Nessuno identificato.

#### Important (Should Fix)

- **I1 — [P1] Il recupero batch rende invisibili le proprie ricevute.** `android/src/glue.rs:1785` chiama `materializza()` prima della scansione `recupera_uscite(..., prima)` a riga 1789. `android/src/web.rs:151`–154 riscrive atomicamente i marker di ogni figlio e ne aggiorna il mtime; il filtro invariato in `android/src/glue.rs:884` scarta ogni directory con mtime non precedente a `prima`, senza visitarla. Riproduzione: chiusura dopo creazione di `.pronti` del figlio ma prima del completamento persistito nell’UI. Al riavvio nessuna ricevuta viene associata al batch; il figlio torna InCorso e `android/src/glue.rs:2306`–2308 trova `.pronti`, rimuove il token e continua senza inviare alcun completamento. Il lavoro rimane InCorso senza motore attivo. Recuperare le ricevute prima delle scritture oppure leggere esplicitamente i figli del manifest indipendentemente dal filtro temporale; il ramo `.pronti` deve sempre consegnare uno stato coerente. Aggiungere la regressione integrata con timestamp reale, non soltanto `Batch::materializza` e `App::ricevi_batch` separati.

- **I2 — [P1] SkipExisting scarta anche nuovi contenuti che hanno soltanto lo stesso nome.** `android/src/web.rs:266` forza `mode=2` quando il toggle (attivo di default) è acceso. Se `riusi()` non trova una firma compatibile — per esempio stesso video prima a 480p e poi a 1080p — `android/src/glue.rs:1013` passa comunque questo mode al normale salvataggio. Per un risultato singolo senza gruppo, il ramo MediaStore a `android/src/glue.rs:1181` riusa il file trovato per nome; il corrispondente contratto legacy è in `android/src/publication.rs:164`. Poi `Registro::completato` elimina il nuovo privato. Il risultato dichiarato della richiesta 1080p è quindi il vecchio 480p; accade anche tra link diversi con titolo uguale. Quando manca una ricevuta compatibile, mantenere la policy Exists scelta dall’utente (normalmente numerazione) e applicare il toggle soltanto ai riusi verificati. Il test corrente copre solo richieste identiche strutturate e non questo fallback.

- **I3 — [P2] Tre scelte attive non hanno effetto nel web generico → Testo.** `android/src/ui/web.rs:40`, 52–68 e 74 espone qualità audio, formato e Uniforma il volume anche con Testo. In `android/src/worker.rs:149`–155 il ramo `text` esclude esplicitamente la conversione, conserva il download originale e ignora `audio_format`; la qualità assegnata a `r.qualita` non viene usata dal Request di trascrizione a riga 163. Il core trascrizione usa le Options per il nome/uscita e non applica `normalize`. Scegliere WAV rispetto a MP3 o attivare la normalizzazione produce quindi lo stesso audio originale di revisione. Collegare queste scelte a una vera preparazione audio prima della trascrizione oppure renderle chiaramente non applicabili in questo percorso, mantenendole visibili. Occorre una prova del file effettivo, non solo dell’oggetto Request YouTube.

#### Minor (Nice to Have)

- **M1 — [P3, limite di annullamento da misurare]** `src/youtube.rs:1507`, 1515: il token viene controllato prima di GET/read, ma tali chiamate bloccanti usano l’agent con timeout lettura di 60 s (`src/youtube.rs:218`). Python/ffmpeg continuano a girare nel genitore (`android/src/glue.rs:1345`), quindi la grazia di 1,5 s del worker non li interrompe. Durante una connessione che resta aperta senza inviare byte, Annulla può quindi trattenere lavoro e permit fino al timeout. Verificare sul server di prova e accorciare/interrompere le attese di rete se il tempo osservato non è accettabile; non attribuire automaticamente a questo percorso il limite del worker.

- **M2 — [P3] Nuovo warning nella build finale.** `android/src/worker.rs:78`: il controller segnala `function_casts_as_integer` nel cast del signal handler. Correggere il cast tramite il tipo puntatore previsto dal lint e verificare che il warning scompaia. I warning `con_dettagli`/`gia_salvato`/example `m` e quelli core/vendor sono dichiarati preesistenti dal controller e non vengono attribuiti a questo task.

### Focused checks / Cannot-verify boundaries

- Diff letto in una passata per sezioni; recuperate solo le porzioni troncate dall’output. Nessun comando git, cargo, Rust, adb o carico remoto eseguito; nessun test duplicato. Unico file scritto: questo rapporto.
- Rischio nominato **recupero batch e timestamp**: letto il corpo invariato di `glue::recupera_uscite`, tagliato fuori dai relativi hunk, e il contratto `publication::Registro`; verificato anche `storage::atomico` (righe 51–65): crea/rinomina/rimuove sempre file nel parent, senza evitare le scritture quando il contenuto è uguale. Confermato il percorso che conduce a I1.
- Rischio nominato **fallback Skip / verifica provider**: letti `publication::legacy`, `Registro::completato`, `glue::media_committed` e `youtube::build_args`, fuori dai relativi hunk. Il riuso esplicito verifica il row MediaStore non pending; il fallback per nome causa I2.
- Rischio nominato **opzioni del web-testo**: letto `worker::converti_web` perché il diff ne tronca il corpo e verificati gli usi di Options/normalize in `trascrizione::run_cancellable`; riscontro I3.
- Rischio nominato **ownership parziali su retry e altri componenti**: letto il resto di `trascrizione::fetch_inner` e il percorso del modello riassunto. Il retry ricorsivo richiama la registrazione; il modello del riassunto è dentro `trascrizione::data_dir`, quindi l’allowlist IPC non lo esclude. Nessun difetto aggiuntivo dimostrato.
- Rischio nominato **latenza cancel del download nel genitore**: verificati i timeout dell’agent condiviso, riscontro M1. La pulizia dopo cancel forzato e il comportamento del canale duplex restano da provare su Android.
- Rischio nominato **traduzioni dei titoli/tessera consigliata**: ricerca mirata nelle cinque TSV conferma che le chiavi non aggiunte nel diff erano già presenti; nessun buco rilevato per quelle chiavi.

### Assessment

**Task quality: Needs fixes** — 0 Critical, 3 Important, 2 Minor.

**Reasoning:** la separazione delle scelte e il trasporto dello stato sono sensati, ma i test di unità non intercettano due guasti d’integrazione che possono lasciare lavori sospesi o presentare un file diverso da quello richiesto. Correggere I1–I3 e completare i gate del candidato corrente prima di considerare il task pronto.
