# Collaudo finale Android — Cedro 0.53.0

Rapporto del 17 novembre 2026 sul Mi 9T. Le prove confermano i sottocasi descritti; **non certificano tutti i 79 ID della matrice**. Il difetto di ripristino del selettore emerso durante il collaudo è stato corretto e riprovato sul pacchetto definitivo dopo due chiusure forzate del processo. Le prove manuali e non eseguite restano distinte dai PASS.

APK definitivo `f0247e8`: versione **0.53.0 / 5300**, **63.617.447 byte**, SHA-256 `5cbbc45c408eeec43d8615a5196711b87cecc868e347ffdce6a14b76469a49a7`, certificato di rilascio esistente `4bb821682d7c150a65219a1aa7bad7824f6c4c318fdec4888e6f3b76855dc0b7`. Compilazione nativa release sul PC di collaudo da `fd3f38c`; il successivo commit cambia soltanto Java/test. Javac+d8, ripacchettamento e zipalign sul PC di collaudo: tutti i **27 elementi ZIP diversi dal DEX sono identici byte per byte** all'APK nativo. Firma sul notebook con chiave esistente, senza trasferirla. Aggiornamento sul Mi 9T riuscito senza cancellazione dei dati.

Il rilascio usa esattamente questo APK firmato e provato. I candidati GitHub Android `43999430758`, `44002009380`, `44006519388` risultano riusciti; l'ultimo ritorno dal selettore è verificato sul pacchetto locale sopra, non attribuito retroattivamente ai candidati CI. Il job automatico di ricompilazione del tag viene annullato per preservare l'asset collaudato; l'aggiornamento F-Droid segue la release pubblicata.


## Candidato e attribuzione delle prove

L'APK CI è cedro-0.53.0.apk (`~/Documenti/cedro-android-anteprima/apk/ci-5a86bf6/cedro-0.53.0.apk`), build `5a86bf6`, 63.609.255 byte, SHA-256 `b7c7c446dd0c0d00cc7d54c2321bcfdc23fab75a28a7d9d9a82e11d7609e7ee5`. Il controller ne registra l'installazione come versione `0.53.0`, versionCode `5300`; il dump del package (`~/Documenti/cedro-android-anteprima/prove-mi9t/finale-5a86bf6/package-after-original.txt`) e la firma (`~/Documenti/cedro-android-anteprima/prove-mi9t/finale-5a86bf6/signature.txt`) sono archiviati. Certificato SHA-256: `4bb821682d7c150a65219a1aa7bad7824f6c4c318fdec4888e6f3b76855dc0b7`.

Il diff `5a86bf6..e9b6a72` contiene soltanto documentazione e la correzione di una fixture nel modulo `cfg(test)` di `rivedi.rs`: il codice di produzione è identico. Questo consente di collegare i test host corretti al codice dell'APK CI, senza affermare che due build siano binariamente identiche. I due fallimenti iniziali della suite `5a86bf6` restano registrati: erano nella costruzione della fixture delle voci, poi corretta senza indebolire le asserzioni.

Le prove del dialogo lungo appartengono al precedente candidato `b4d03e7`; quelle di accessibilità, avvio e riassunto appartengono a `5a86bf6`. Le evidenze intermedie `db3a1a5` e `4c67b2c` conservano la propria attribuzione e non diventano automaticamente prove del finale.

## Risultati fisici verificabili

| Prova | Esito e misura | Limite della conclusione |
| --- | --- | --- |
| Tre avvii COLD, `5a86bf6` | 238, 231, 241 ms; mediana **238 ms**. Dati (`~/Documenti/cedro-android-anteprima/prove-mi9t/finale-5a86bf6/cold.json`). | Baseline 0.45: 251, 213, 275 ms, mediana 251 ms. Due triplette, non una stima statistica generale; nessun nuovo FPS finale. |
| Bridge accessibile, `5a86bf6` | Vere `AccessibilityNodeInfo.performAction`: Lavori → contenuto «Svuota», Impostazioni → «Tinta di Cedro», scroll con firma di testi/bounds cambiata. Unicità verificata su 27/36/27/24 nodi. Log GREEN (`~/Documenti/cedro-android-anteprima/prove-mi9t/finale-5a86bf6/a11y-routes.txt`). | Non è soltanto `accepted=true`: viene verificato l'effetto. `selected=false` è registrato, senza dedurne un difetto. «Svuota» viene cercato, mai azionato. Non copre ogni comando o TalkBack completo. |
| «Chi parla», `5a86bf6` | Centro fisico del nodo `[379,1218][700,1350]` verificato dal controller; elenco fino alla quarta voce e toggle inferiore raggiungibili; toggle acceso/spento mediante action accessibile. Bounds (`~/Documenti/cedro-android-anteprima/prove-mi9t/finale-5a86bf6/physical-center-chi-parla.txt`), fondo elenco (`~/Documenti/cedro-android-anteprima/prove-mi9t/finale-5a86bf6/voices-bottom.png`), tastiera (`~/Documenti/cedro-android-anteprima/prove-mi9t/finale-5a86bf6/voice4-keyboard.png`). | Quattro voci fisiche; otto voci, tutte le geometrie e font grandi hanno copertura host, non una verifica fisica esaustiva. Rename esportato, hover e rotazione restano sottocasi distinti. |
| Dialogo 243,3 s, job 58, `b4d03e7` | Log correlati: completo, una uscita, **893,362 s** contro **1.000,002 s** della base, circa −10,7% in queste singole esecuzioni. TXT 3.111 byte, quattro interlocutori. Misure recuperate (`~/Documenti/cedro-android-anteprima/prove-mi9t/candidato-b4d03e7/dialogo/misure-recuperate.json`), testo (`~/Documenti/cedro-android-anteprima/prove-mi9t/candidato-b4d03e7/dialogo/risultato.txt`). | Una prova per versione, non mediana. Le quattro voci non certificano accuratezza: il riferimento «tre parlanti» non è una ground truth verificata. Riproduzione breve durante il lavoro; ascolto qualitativo non certificato. |
| Background del dialogo, `b4d03e7` | 201 s in background; servizio foreground e notifica presenti durante il lavoro. A fine lavoro controller registra worker e servizio assenti, UI **155.193 KiB PSS**. Servizio durante background (`~/Documenti/cedro-android-anteprima/prove-mi9t/candidato-b4d03e7/dialogo/services-background-3min.txt`). | Il valore UI finale non è il massimo del lavoro. **PSS del dialogo lungo: NA**, campioni persi dall'harness prima del salvataggio. Nessun confronto valido del picco RAM con la base. |
| Riassunto 4B, job 59, `5a86bf6` | Log correlati: completo in **366,961 s**, nessun OOM osservato. 163 campioni NDJSON conservati, massimo UID campionato **3.329.187 KiB**. Log (`~/Documenti/cedro-android-anteprima/prove-mi9t/finale-5a86bf6/riassunto4b/logcat-Cedro.txt`), campioni (`~/Documenti/cedro-android-anteprima/prove-mi9t/finale-5a86bf6/riassunto4b/campioni.ndjson`). | Campioni da 0,447 a **217,274 s**, quindi **non il picco dell'intero job** di 366,961 s. `uscite=0` MediaStore è atteso per il risultato privato; non è un errore né prova dell'export UI, ancora da verificare. Qualità del riassunto non certificata. |

Il problema di raccolta è dell'harness: terminazione locale con stato 143 e acquisizione incompleta, **causa non accertata**. Non è dimostrato che dipenda dal teardown SIGTERM, né è un crash/OOM dell'app. Il controller ha separato il process group di logcat con `start_new_session=True` e reso durevole la scrittura NDJSON; la validazione della correzione su un job breve resta da registrare. Le etichette «picco» e la spiegazione del teardown nel JSON recuperato non prevalgono sui limiti osservabili dei campioni.

Otto import validi con file esistenti risultano inoltre registrati sul candidato `b4d03e7`: provano quel sottocaso di nuovi intent, non la coda di venti lavori misti. Il tentativo precedente con un percorso inesistente non conta come prova riuscita.

## Ripristino del selettore SAF: corretto e riprovato

Il KO osservato era la perdita di `attesa_scelta` quando Android ricreava NativeActivity. Correzione `5580f0f..f0247e8`: contesto atomico, ripristino del viewer della firma e dei grant, callback con identità della richiesta, annullamento separato e nessun replay dell'intento precedente. Review finale mirata approvata, zero Critical/Important aperti.

Sul pacchetto definitivo: selettore della firma aperto, processo **26678** terminato con `am kill` mentre DocumentsUI è in primo piano, scelta della `.p7s`: nuovo processo **27857**, viewer corretto e richiesta dell'originale. Secondo selettore, processo **27857** terminato, scelta SAF dell'originale **33.555.456 byte**: nuovo processo **28249**, stesso viewer, **Documento: non modificato**, firma valida con il previsto avviso del certificato di prova non qualificato. Il messaggio «Manca il file originale» scompare; nessun EACCES o crash nel percorso. Evidenze locali: `prove-mi9t/finale-f0247e8/{firma-picker-kill.json,firma-restored.png,original-picker-kill.json,original33-verified.png,recovery-log.txt}`.

Il codice richiesta usa lo spazio 16 bit senza riuso: 49.152 aperture disponibili prima di un esaurimento esplicito. Limite tecnico dichiarato, non un limite alla dimensione dei file. Il test `.bin` non è un documento visualizzabile: la verifica della firma è separata dalla sua anteprima.

## Verifiche host e review

| Verifica registrata | Checkpoint ed esito | Portata |
| --- | --- | --- |
| Core | `8e98780`: **597 passati, 15 ignorati**, zero falliti. Log (`~/Documenti/cedro-android-anteprima/verifiche/8e98780/cedro-android-8e98780-task6-aac3-core-test.log`). | Gli ignorati restano tali, inclusi file pubblici reali non eseguiti. Non è un rerun del core sul finale. |
| Android host | `e9b6a72`: **55/55** con e senza YouTube. Directory log (`~/Documenti/cedro-android-anteprima/verifiche/e9b6a72`). | Include voci/fogli, centri semantici, MIME e originale SAF di 32 MiB + 17 byte; il registro/fd host non sostituisce un provider Android reale. |
| CLI | `b4d03e7`: **37 OK, zero errori**. Log (`~/Documenti/cedro-android-anteprima/verifiche/b4d03e7/cedro-android-b4d03e7-cli-regression.log`). | Conversioni, PDF/Office, codec, Labs e firma su host; non tutti questi flussi sul Mi 9T. |
| Java e NDK | `e9b6a72`: `AccessibilityCoordinatesTest: PASS`; NDK release all-targets con e senza YouTube, exit 0. | Test Java host delle trasformazioni, distinto dalle action fisiche sopra. |
| Clippy | `e9b6a72`: exit 0, **non privo di avvisi**. | Oltre ai 12 avvisi Android preesistenti, due nuovi `dead_code` host: `storage::mime_da_percorso` e `storage::leggi_originale`; presenti anche avvisi vendor. |

Gli avvisi dei due helper erano presenti su e9b6a72. La correzione successiva applica cfg Android/test mirati: su fd3f38c/f0247e8 clippy termina con gli avvisi preesistenti, senza nuovi dead_code. Nessuna esclusione delle regressioni.

La [review generale](final-review.md) e la [review della correzione](final-fix-review.md) distinguono quattro finding: scorrimento voci/fogli, lettura dell'originale SAF per firma staccata, MIME WebM/MKV, trasformazione dei bounds accessibili. Il delta correttivo è approvato, senza nuovi Critical/Important individuati. L'approvazione del codice non trasforma i gate fisici pendenti in PASS.

## Matrice: copertura effettiva e controlli aperti

L'[audit originale dei 79 ID](task-7-matrix-evidence.md) conserva l'intera matrice e i riferimenti intermedi. È una fotografia precedente: questo rapporto aggiorna soltanto i sottocasi nuovi, senza promuovere in blocco le sue righe.

| ID del piano | Aggiornamento documentato | Stato residuo |
| --- | --- | --- |
| A1, A6, J1, J5 | Identità/installazione finale, tre COLD e dimensione APK. | A6 non dispone di un nuovo profilo RAM completo; A2–A5 non sono chiusi da questi dati. |
| B12, I2 | Otto import validi sul `b4d03e7`. | Parziale: non venti lavori misti, non tutti gli ingressi B1–B14 e non stress sul finale. |
| C7, D1 | Dialogo completo e 201 s in background sul `b4d03e7`. | Sottocasi positivi; nessuna estensione automatica a tutti C1–C7 o al finale. |
| D2, D3, D7, H4, H5, H8 | Player fotografato; quattro voci/toggle e bridge action sul finale. | Parziale: ascolto/seek/velocità, diarizzazione corretta, riapertura dopo process death, target e barre esaustivi non dimostrati. |
| D4 | Modello 4B completa senza OOM osservato. | Export UI e qualità aperti; D5 Referto e D6 tutti i campioni senza parlato restano distinti. |
| E1–E9 | Restano le prove intermedie di YouTube/W3C e le prove host dell'audit. | Nessun nuovo PASS dell'intera sezione web. |
| F1–F6, G1–G10 | Host disponibili; NDPI SAF reale con panoramica/centro positivo su `4c67b2c`. | Firma staccata con originale SAF33MiB e due ricreazioni forzate: PASS f0247e8. Restanti firme/Labs mantengono la copertura parziale descritta. Preview NDPI non prova export, anonimizzazione o assenza di copia disco. |
| H1–H8 | Nuove action e centro fisico positivi per i sottocasi indicati. | Font sistema130/200% provato fisicamente sul5a, con scorrimento e ripristino1.0; sei lingue, landscape/tablet, hover e confronto completo restano parziali/host. |
| I1, I3–I7, J2–J4 | Nessuna nuova prova fisica completa in questo rapporto. | Corpus robustezza, process death, spazio, permessi negati, offline, trenta conversioni, FPS e batteria restano aperti o soltanto host. J3 ha carichi parziali su APK diversi, senza PSS completo del dialogo. |

«Manuale», «non eseguito», «solo host» e «NA» non sono PASS. X13 resta escluso dal perimetro concordato: non è una verifica di compatibilità e non dimostra una funzione mancante. Nessuna misura di autonomia deriva dal telefono USB in carica.

## Limiti tecnici mantenuti

Restano i limiti del motore già accettati in review: shaping RTL completo non disponibile; copia anonima CZI/ND2/LIF non supportata, con conversione conservata; risoluzione DNS nativa prima della connessione non preemptibile; massimo otto voci, senza creazione di una nona. Questi limiti non equivalgono a nuove regressioni. Restano separati l'ascolto su cuffie/chiamate, l'accuratezza linguistica e la validazione scientifica dei risultati.

## Ultime verifiche del controller

- `fd3f38c`/`f0247e8`: **56/56** Android con e senza YouTube, NDK release entrambe le configurazioni, clippy exit0; `AccessibilityCoordinatesTest` e `ActivityDeliveryTest` PASS. Compilazioni effettuate sul PC di collaudo. Core e CLI restano attribuiti ai checkpoint della tabella, senza ripetizioni inutili del codice invariato.
- Font sistema **130% e200%** sul Mi9T: testo ridisposto, tessere raggiungibili scorrendo, barra inferiore presente. `font_scale` ripristinato a1.0. Otto voci e geometrie con tastiera restano coperti dai test host; quattro voci, campo con tastiera e toggle sono stati provati sul telefono.
- HEIC reale iPhone13Pro, DisplayP3,4032×3024 → JPEG: job64 completo in **1,470s**, uscita **1.014.929 byte** riaperta, dimensioni4032×3024. Confronto del JPEG con il riferimento indipendente LittleCMS sRGB a512px: errore assoluto RGB medio **0,454**, percentile95 **1**. PSSUID massimo campionato386229KiB nella finestra; non una mediana. Il primo filtro del misuratore era `jpg` mentre il job usa `jpeg`: durata recuperata dagli eventi correlati, senza inventare un risultato del filtro errato.
- Il nuovo harness conserva campioni NDJSON e isola il processo logcat; sul job breve arriva al JSON finale. La causa dei precedenti exit143 non è certificata. Il picco dell'intera trascrizione lunga rimane NA.
- Restano manuali/non eseguiti i sottocasi dettagliati della matrice: ascolto ad orecchio/cuffie/chiamate, accuratezza linguistica e scientifica, referto fisico completo, tutti gli Office e Labs, rete/offline esaustivi,30conversioni/FPS e batteria. La release non dichiara PASS di questi casi.

Release autorizzata: [Cedro Android0.53.0](https://github.com/realfulvio/cedro/releases/tag/android-v0.53.0). Il controllo finale confronta l'asset scaricato con SHA/certificato sopra e l'indice del repository F-Droid. Le decisioni durante l'esecuzione sono archiviate in [decisioni-esecuzione.md](decisioni-esecuzione.md).

Annullamento dopo terza chiusura forzata del processo: ritorno e nuovo avvio riusciti, nessuna importazione fittizia. Probe accessibile sul pacchetto definitivo: `PASS routes=Lavori,Impostazioni scroll=true`, JUnit1/1,3.122s. FontOSripristinato1.0, nessunserviziolavoro rimasto attivo.
