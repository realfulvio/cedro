# Task 3 — Re-review fix round 1

## Finding Verdicts

- **I1 — Renderer E0616, accesso privato ad App.scorri: ADDRESSED.** `android/examples/vista_a.rs:58` e `:62` usano ora `vista_scorri`; il metodo pubblico mirato a `android/src/ui/casa.rs:742` mantiene il campo privato e normalizza valori negativi/non finiti. Il report del controller registra NDK tutti i target con e senza YouTube PASS su `af4e692`. L'esecuzione del renderer e l'ispezione dei PNG restano un gate distinto.
- **I2 — Ultima sorgente Labs rimossa/deselezionata, preview e worker non invalidati: ADDRESSED.** `android/src/ui/labs.rs:106` restituisce l'id precedente quando cambia la richiesta; `:112` pulisce path, focus, id, selezione, info, immagine, errore e flag; il ramo senza sorgente a `:128` esegue quella pulizia. `android/src/glue.rs:2098` applica cancel prima dell'eventuale nuova richiesta e prima di raccogliere gli esiti. Non serve più che esista una richiesta sostitutiva. Il test a `android/src/ui/labs.rs:291` copre rimozione, deselezione, cambio categoria e uscita, id restituito per la cancellazione, rifiuto di successi/errori tardivi e nuovo id alla riselezione. Incluso nel PASS host 29/29 dello stesso HEAD; cancellazione del process group sul Mi 9T ancora da collaudare.
- **M1 — Data/orario scomparsi dalle schede completate: ADDRESSED.** `android/src/ui.rs:2027` calcola testo e altezza temporale; `:2079` lo disegna separatamente prima del messaggio e le quote successive comprendono l'altezza aggiunta. Il test a `android/src/ui/labs.rs:320` cambia soltanto il timestamp e verifica che cambino i pixel, conservando spazio per avviso/caso/percorso e target delle azioni. Incluso nel PASS host 29/29.
- **I3 — Cinque famiglie senza intestazioni fisse/richiamabili: ADDRESSED.** `android/src/ui/casa/telefono.rs:40` riserva una barra fuori dall'area scorrevole per Completa immagini; `:304` disegna i cinque comandi con nomi e colori ricavati dal catalogo condiviso; `:345` registra le posizioni dei gruppi. `android/src/ui/casa.rs:613` applica il salto senza filtrare il catalogo. Il test a `android/src/ui/casa/telefono.rs:539` esercita i tocchi, la permanenza dei cinque comandi e almeno un formato della famiglia raggiungibile, con target ≥48 dp su tre viewport e testo normale/grande. Incluso nel PASS host 29/29. Resa DE/PT e comportamento sul Mi 9T rimangono nei gate visuali/runtime.
- **Fixture DICOM dopo FAIL host precedente — contratto del test preservato: ADDRESSED.** `android/tests/support/dicom.rs:8` rende esplicita la modalità CT e aggiunge rescale/finestra coerenti, con rampe orizzontali/verticali distinguibili. `android/src/labs.rs:155` verifica i metadati; `:159`–`:161` verifica numericamente le tre finestre distinte. Il test conserva il confronto tra frame e l'errore per frame inesistente, aggiunge due campioni nero/bianco, confronta Osso con il render condiviso e richiede che file/Osso/auto producano risultati distinti (`:174`–`:176`). Non è stata rimossa né indebolita l'asserzione di cambiamento dei pixel e il motore DICOM non è modificato dal fix. Incluso nel PASS host 29/29.

## New Breakage in the Fix Diff

- **Critical: nessuno. Important: nessuno. Minor: nessuno rilevato.** Le modifiche riguardano i finding assegnati, i test che li coprono e la fixture DICOM; non ho trovato una regressione introdotta dal delta.
- Il report di validazione elenca warning vendor preesistenti e li distingue dal codice Android. Questa re-review non li presenta come output privo di warning né li riclassifica come nuove regressioni del fix.

## Out-of-Scope Observations

- **Nessuna nuova osservazione su codice intoccato.** Non è stato riesaminato il resto del ramo o riaperta la review generale di core, SAF, publication, Office, firma o download.
- I limiti e gate della review iniziale restano nel registro del controller: memoria dei formati grandi, output/privacy reali, fd/provider e compatibilità desktop non sono automaticamente chiusi dalla correzione dei finding.

## Evidenza e gate esistenti

- Base del fix: `f49241e957fe099211afbbe6bfb329d9aa1e19fe`; HEAD: `af4e6926b49f82073458346c3958179c00691184`. Letti brief, vincoli globali, review precedente, appendice del report e template `re-review-prompt.md`. Diff preparato letto una volta in due sezioni; nessun diff rigenerato e nessuna ricerca estesa nel repository.
- **Evidenza dello stesso HEAD registrata dal controller:** host `cargo test --release -j2 -p cedro-android --lib`, **29/29 PASS, exit 0**, compilazione 5m54s e test 1,15s; include preview senza sorgente, timestamp, cinque famiglie, DICOM e tolleranza float. Log indicato: `/srv/dev/scratch/cedro-android-af4e692-test-ripresa.log`. NDK tutti i target **PASS** con YouTube (5,22s) e senza (4,34s). Sono esiti del controller riportati nell'appendice, non comandi rieseguiti da questa review; il tentativo SSH terminato 255 non viene conteggiato come PASS.
- **PENDING — renderer e core:** esecuzione renderer host e ispezione PNG IT/DE/PT, test core MRXS/tessere/alias, i18n e restante regressione core/CLI. Il controller li ha in coda; nessun loro PASS è attribuito qui.
- **PENDING — APK e Mi 9T:** build APK con/senza YouTube, installazione e comportamento reale; cancel del worker/process group e lifecycle preview; provider SAF/fd e file ≥1 GB senza copia non necessaria; persistenza caso, gruppi output e quaderno; dimensione APK, tempi e PSS UI+worker.
- **PENDING — corpus:** confronto Office desktop/telefono e font, fogli grandi, HEIC P3/rotazione, `.doc` malevolo e banco rovinati, avvisi firma SHA1/post-modifica, metadati/allegati PDF, anonimizzazione DICOM/vetrini e verifica visiva dei dati impressi nei pixel. Soglie e criteri rimangono quelli del piano e della review iniziale.
- Nessun test, build, Rust sul notebook, accesso remoto o adb eseguito; nessun subagent. Nessuna modifica a codice, indice o HEAD. Unico file scritto: questo rapporto.

## Verdict

**Fix round: All findings addressed, no new Critical/Important breakage.** I1, I2, M1, I3 e correzione della fixture DICOM chiusi nel perimetro del fix.

**Spec: ✅** per i finding e il delta sottoposti a re-review; **⚠️** accettazione runtime complessiva ancora subordinata ai gate elencati.

**Task quality: Approved** per questo fix round. L'approvazione del codice corretto non dichiara completati renderer, core test, APK o prove sul Mi 9T e non autorizza a saltare i gate di pubblicazione.
