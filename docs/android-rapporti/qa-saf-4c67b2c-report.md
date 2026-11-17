# QA runtime SAF e anteprima NDPI — build 4c67b2c

2026-11-17, circa 01:47–01:51 Europe/Rome. Mi 9T `2f6d6ba7`, app `it.cedro.android`, `versionCode=5299`, `versionName=0.53.0-anteprima`, targetSdk34. APK nuovo indicato dal controller come build `4c67b2c`, già installato e avviato a freddo dal controller prima dell'esclusiva ADB. Questa sessione non ha installato, compilato, modificato sorgenti o impostazioni, né avviato conversioni/export grandi.

## Esito

**PASS: scelta reale SAF, lettura del descriptor nel worker, Panoramica NDPI, area centrale del livello 1 e ritorno a Panoramica.** Il fixture `OS-3.ndpi` da `1473064608` byte viene visualizzato correttamente; metadati visibili `NDPI · 122880 × 77824`. Nessun errore EACCES nell'interfaccia nuova, nessun crash/OOM osservato.

**La causa del guasto precedente è ora discriminata dal probe runtime:** il descriptor ereditato è valido e leggibile, mentre la sua riapertura per pathname restituisce EACCES. L'errore 13 presente nei nuovi log è l'esito atteso del probe `pathname_open`, non un fallimento del rendering corretto attraverso `input_file`.

## Flusso reale

1. Avvio osservato su Home, PID UI nuovo **15820**, con la recente OS-3 ma nessun errore nel pannello. Aperto Laboratorio dalla scheda Home; stato iniziale vuoto e senza errore documentato nello screenshot 02. Questo esclude un EACCES precedente rimasto sullo schermo dopo l'aggiornamento.
2. `Apri file` apre DocumentsUI vero, già posizionato su **Mi 9T → Download → prova-cedro**. Ricerca `OS-3`, atteso l'aggiornamento effettivo dei risultati, screenshot 03 e XML con `OS-3.ndpi`, `1,47 GB`.
3. Il controllo del picker mantiene il limite di automazione precedente sui tap delle righe. Come nella sessione precedente: tap sul risultato, TAB per focus, ENTER per la scelta. Nessun intent artificiale, grant shell o accesso generale ai file.
4. Alle 01:48:24 parte il nuovo worker e compare la Panoramica con tessuto e metadati. Grant SAF persistito verificato per l'URI del fixture.
5. Scroll fino al selettore **Livello → Panoramica**, poi `+` a `[950,1350]`: nuova richiesta, worker nuovo, immagine centrale dettagliata chiaramente differente, selettore `1 · 122880 × 77824`, avviso `Anteprima: area centrale del livello. L’esportazione usa le opzioni del risultato.`
6. `−` a `[110,1350]`: ulteriore nuovo worker e ritorno corretto a Panoramica, senza errore. La nuova richiesta del selettore invalida l'immagine precedente; i nuovi worker/probe e le immagini differenti verificano il ricalcolo, non soltanto un'immagine vecchia in cache UI.

Grant osservato:

```text
content://com.android.externalstorage.documents/document/primary%3ADownload%2Fprova-cedro%2FOS-3.ndpi
sourcePkg=com.android.externalstorage targetPkg=it.cedro.android
mode=0x3 owned=0x3 global=0x0 persistable=0x3 persisted=0x1
```

`persisted=0x1` prova la lettura persistita; la concessione temporanea aggiuntiva di scrittura restituita dal provider non è stata usata.

## Probe discriminante e tempi

| Richiesta | Timestamp locale in log | Worker PID | F_GETFD | fstat | pread iniziale | pathname_open |
| --- | --- | --- | --- | --- | --- | --- |
| Panoramica dopo scelta SAF | 01:48:24.777/.781 | 16269 | `flags=0`, fd87 | `Ok(1473064608)` | `Ok(16)` | `Err(Some(13))` |
| Area centrale, livello1 | 01:49:24.112/.117 | 16345 | `flags=0`, fd87 | `Ok(1473064608)` | `Ok(16)` | `Err(Some(13))` |
| Ritorno a Panoramica | 01:50:09.483/.489 | 16655 | `flags=0`, fd87 | `Ok(1473064608)` | `Ok(16)` | `Err(Some(13))` |

Ogni `pathname_open=Err(Some(13))` coincide con un messaggio MediaProvider `Couldn't find file: /storage/emulated/0/Download/prova-cedro/OS-3.ndpi`. Le letture fd e il successivo rendering invece riescono. Ciò dimostra il punto di rottura della vecchia strategia di riapertura nel contesto del dispositivo, senza richiedere di inferirlo solo dal codice AOSP. Non discrimina il singolo controllo interno MIUI/FUSE/proc che restituisce EACCES.

Per il centro è stato campionato il tempo monotono dal comando di tap: il primo campione iniziava a 1,073 s, il secondo a 2,385 s. Il primo screenshot, acquisito tra quei due campioni, contiene già la stessa area centrale completa dell'ultimo campione; confronto locale dei pixel nella regione dell'immagine confermato uguale. Quindi l'area centrale era completa **entro 2,4 s dal comando di tap**, un limite osservato comprensivo di overhead ADB, non una misura esatta del decoder. Non è stato misurato un tempo preciso di decodifica per la Panoramica iniziale; era già completa alla cattura successiva e il worker era terminato.

## Dimensioni e memoria: osservazioni e limiti

- Dimensioni native del vetrino esposte correttamente: **122880 × 77824**, circa 9,56 miliardi di pixel; lo screenshot del centro non mostra l'intero livello pieno ma una porzione locale.
- Limite sorgente verificato in `android/src/labs.rs:68–71`: area centrale `min(width,1024) × min(height,1024)`, Panoramica `s.preview(path,1024)`. Questo prova il limite impostato nell'algoritmo della build indicata; **le dimensioni del PNG privato non sono state lette direttamente**, quindi non viene dichiarata una misura indipendente del file PNG prodotto.
- PSS UI prima della scelta: **82451 KB**; Panoramica completata: **55576 KB**; quattro campioni dopo il cambio al centro: **70411, 70399, 70399, 70399 KB**, RSS circa166 MB; dopo il ritorno a Panoramica: **69381 KB**, RSS165476 KB. Il PID UI resta15820.
- I worker hanno vita breve e risultavano già terminati ai campioni raccolti. Questi sono campioni del processo UI, **non il picco complessivo UI+worker**; non si deduce un picco non misurato.
- Nessuna conversione o export completo richiesto. Nessuna misurazione diretta dell'occupazione privata su disco eseguita: non viene dichiarata come osservata l'assenza di una copia privata completa. Il probe e il codice indicano la lettura diretta, ma la RAM stabile da sola non è prova di spazio disco.

## Evidenze

Directory:

`/home/accorsiluca/Documenti/cedro-android-anteprima/prove-mi9t/anteprima-4c67b2c/saf/`

- `01-start.png`: Home dopo cold start.
- `02-lab.png`: Laboratorio iniziale vuoto, senza errore residuo.
- `03-real-picker.png`, `picker-os3.xml`: scelta SAF reale del fixture.
- `04-after-selection.png`, `05-overview-controls.png`: immagine Panoramica e metadati.
- `06-center-sample-0.png` fino a `06-center-sample-3.png`: area centrale del livello1, controlli e avviso; `center-timing-mem.txt` conserva tempi e campioni.
- `07-return-overview.png`: nuovo rendering Panoramica dopo `−`.
- `saf-probe-log.txt`, `saf-grant.txt`, `final-meminfo.txt`: prove testuali.

## Stato finale e handoff

Telefono lasciato in primo piano su **Cedro → Laboratorio → OS-3.ndpi → Livello Panoramica**, immagine visibile e nessun errore. Nessun worker residuo rilevato dopo il ricalcolo; nessun lavoro/export attivo avviato da questa sessione. Il grant SAF è persistito. Esiti probe/Panoramica/centro inviati subito al controller e all'implementer `/root/android_finish`. Esclusiva ADB rilasciata al controller al termine del report.
