# Revisione Android e proposta grafica

Data: 16 novembre 2026. Sorgenti iniziali: `0a3a64b`, main Cedro 0.53.0. Telefono: Mi 9T, Android 11/API 30, APK installato 0.45.0/4500. Xiaomi 13 non incluso su richiesta dell'utente. Questo rapporto descrive la **base precedente alle modifiche**, non lo stato finale della release.

## Evidenze e limiti

Letti renderer Android, collegamento JNI, importazione/esportazione, servizio, manifest, build, workflow e punti d'ingresso dei motori condivisi. Navigate Inizio, Converti, PDF, Trascrivi, Video dal web, Firma e Impostazioni sul Mi 9T. Provato ingresso SEND a processo già aperto e conversione JPEG con pubblicazione MediaStore. Non sono ancora prove di tutte le conversioni.

Il primo `cargo run --release -p cedro-android --example vista_cedro` sul PC di collaudo fallisce: `android/src/ui/rivedi.rs:151`, match non esaustivo di `Kind::Number` e `Kind::Meaning`. Il nuovo motore non rende automaticamente utilizzabile l'app Android.

Le evidenze grezze locali sono in `~/Documenti/cedro-android-anteprima/prove-mi9t/`: schermate, meminfo, latency, avvii, albero accessibilità e log conversione. I riferimenti di riga sotto sono della base `0a3a64b` e cambieranno durante l'implementazione.

## Problemi trovati

| # | Gravità | Evidenza nella base | Effetto e intervento |
|---|---|---|---|
| 1 | Bloccante | `ui/rivedi.rs:151`, E0004 reale | Android non compila col motore 0.53; gestire numeri e parole decisive. |
| 2 | Alta | `glue.rs:227–316`, `copia_uri` legge in `Vec` | La RAM cresce almeno della dimensione del file importato; usare copia a blocchi. |
| 3 | Alta | `glue.rs:1710,1985`, import sul loop UI | File grandi/provider lenti bloccano eventi e disegno; spostare I/O su worker. |
| 4 | Alta | `copia_uri`, destinazione dal solo nome | Due sorgenti omonime sovrascrivono la copia usata da lavori/recents; isolare importazioni. |
| 5 | Media | directory `condivisi` senza budget/cleanup | Accumulo delle copie private; ciclo di vita esplicito e conservazione dei riferimenti attivi. |
| 6 | Alta | `glue.rs:533–575`, `figli` | Solo primo livello, massimo 200 figli silenzioso; perde struttura MRXS e file oltre il limite. |
| 7 | Media | selettore `glue.rs:460`, manifest SEND | Filtri incompleti per Excel/ODS; allineare import e MIME ai motori supportati. |
| 8 | Media | `glue.rs:61`, estensione da MIME | Ripiego JPEG anche per file non fotografici senza nome completo; mappa coerente. |
| 9 | Alta | `glue.rs:1005–1010`, `consegna` | Il file privato viene eliminato anche dopo pubblicazione fallita; conservarlo e renderlo recuperabile. |
| 10 | Alta | `glue.rs:943–979`, confronto directory comune | Output multipli possono essere attribuiti a un altro lavoro; directory di lavoro dedicata. |
| 11 | Alta | stessi punti, errori aggiuntivi solo nel log | Il risultato principale può mascherare pagine non pubblicate; riportare tutti gli esiti. |
| 12 | Alta | `ui::Effetto`, token locali sempre false | Manca l'annullamento dei lavori lunghi dalla UI; propagare token al motore/processi. |
| 13 | Media | `glue.rs:746–768`, `serializza` | Persistono solo gli ultimi lavori finiti; recuperare errori e segnalare interruzioni dopo chiusura forzata. |
| 14 | Alta | `glue.rs:1315`, glossario None/default | La modalità referti del motore non è collegata ai comandi Android. |
| 15 | Alta | `src/whisper_android/mod.rs` vs `trascrivi/src/parlato.rs` | Il filtro no-parlato nuovo è solo nel percorso desktop; portarlo e provarlo su Android. |
| 16 | Media | `glue.rs:1250`, playlist false | La richiesta web non espone tutte le opzioni disponibili. |
| 17 | Media | `glue.rs:1258–1273`, EventLog | Preparazione dei componenti solo nel log; manca feedback durante il primo uso. |
| 18 | Alta | densità in `glue.rs:1757`, confronto screenshot | Font sistema 1.0 e 2.0 producono lo stesso contenuto pixel per pixel; leggere fontScale e adattare impaginazione. |
| 19 | Alta | UIAutomator: un View senza controlli | La superficie disegnata non espone nomi/azioni semantiche. Servono nodi di accessibilità; ascolto TalkBack non ancora verificato. |
| 20 | Media | `glue.rs:1681` | Rimuovi animazioni letto soltanto all'avvio; aggiornare anche al ritorno/configurazione. |
| 21 | Media | `telefono.rs:494–505`, screenshot Converti | Formati nascosti dopo sette tessere e «Altri»; l'interfaccia approvata li espone per famiglie. |
| 22 | Media | screenshot Video dal web | «Video leggero» è troncato anche in italiano; tessere e righe devono adattarsi alle etichette. |
| 23 | Media | sei destinazioni inferiori, testi fissi | Poco spazio per tedesco/font grandi e nessun accesso inferiore ai Lavori; tre destinazioni stabili nella proposta A. |
| 24 | Media | grandi pannelli vuoti nelle schermate reali | Anteprima decorativa occupa gran parte dell'altezza senza file; scheda compatta e comandi prima. |
| 25 | Alta | `casa.rs:313` defs generiche | Scene, canali e opzioni specialistiche Labs non sono collegate come nel desktop. |
| 26 | Media | README Android | Descrive target 29, scrittura diretta e classi Java non aggiornate; documentare target 34, MediaStore, tre classi effettive. |
| 27 | Da misurare | loop render e SurfaceFlinger | Campione di scorrimento 38,55 fps; distinguere lavoro utile e redraw inutili, misurare dopo. |

Non è stata dimostrata una perdita di riferimenti JNI: il lifecycle degli attach va verificato, senza dedurre una perdita dalla sola assenza di delete espliciti. Non si attribuiscono al target 34 i limiti di servizio introdotti per app che puntano ad Android 15.

## Parità funzionale da verificare

«Motore» significa codice condiviso disponibile, non collaudo sul dispositivo. Le prove finali saranno nel rapporto di collaudo.

| Area | Motore condiviso 0.53 | Android iniziale | Lavoro previsto |
|---|---|---|---|
| Immagini | PNG/JPEG/WebP/AVIF/JXL/BMP/GIF/TIFF/ICO/TGA/QOI/PNM/HDR/EXR/PDF, HEIC in ingresso | Catalogo condiviso, selezione parziale a vista | Tutti i formati, qualità e strumenti, prova HEIC. |
| Strumenti immagini | Comprimi, dimezza, grigio, ruota, togli dati, OCR | Azioni disponibili con UI generica | Raggruppamento e opzioni reali. |
| PDF | Word, Excel, testo, immagini; unisci, dividi, pagine, comprimi, OCR | Viste dedicate già presenti | Conservare funzioni e adattare layout, prova metadati/allegati. |
| Office testo | DOCX/ODT/RTF/DOC, incluso Word 6; PDF/DOCX/ODT/HTML/MD/TXT | Motore raggiungibile da catalogo | MIME, pagina, font di sistema, confronto output. |
| Fogli | XLSX/XLS/ODS verso PDF | Filtri ingresso incompleti | Opzioni foglio/pagina e prova sul telefono. |
| Testo | TXT/HTML/MD/CSV/XLSX/JSON/PDF, maiuscolo/minuscolo | Catalogo condiviso | Accesso a tutte le azioni e opzioni. |
| Audio | MP3/WAV/FLAC/Opus/AIFF, trascrizione | Conversioni esistenti | Nuovo layout, annullamento e controlli output. |
| Video | MP4/MP3, trascrizione | Conversioni esistenti | Impostazioni esplicite e prove encode/decodifica. |
| Archivi | Converti contenuti, verifica pacchetto | Catalogo condiviso | Opzioni e risultati multipli affidabili. |
| Web | Download generico/YouTube, qualità, playlist, accessori | Parte delle opzioni hardcoded | Request completo, progresso componenti, annullamento. |
| Trascrizione | Whisper, voci, revisione, riassunto | Percorso Android separato | Onda/player, persistenza revisione, filtro no-parlato. |
| Referti | Glossario, numeri/parole decisive, bozza | Non collegato; Kind nuovi rompono build | Controlli ed export coerenti. |
| Firma | P7M/PDF/TSD/TSR, rapporto, avvisi SHA-1/modifica | UI dedicata | Preservare e provare avvisi/esportazione. |
| Labs immagini | SVS/NDPI/MRXS/CZI/ND2/LIF/DICOM | Ingresso/opzioni incompleti | Struttura MRXS, preview selettori e azioni appropriate. |
| Labs strumenti | Figura, tessere, mitosi, consulto, RX, anonimizzazione | UI generica | Pannelli dedicati collegati al motore, nessuna diagnosi. |
| Caso e registro | Codice caso/campione nei nomi, elenco lavori | Non equivalente al desktop | Persistenza, rimozione caso e copia registro. |
| Lingue | IT + en/de/es/fr/pt, automatico | Base main contiene tabelle, APK 0.45 precedente | Tradurre nuove UI e provare testo lungo/fallback. |
| Android | SAF, MediaStore, foreground service | Funzioni con rischi sopra | Streaming, cancellazione, permessi, lifecycle e accessibilità. |

## Proposta consegnata

`docs/android-anteprime/rework/index.html`: 20 stati, due alternative, quattro tinte, IT/DE, cinque dimensioni, tre scale del testo, confronti con quattro schermate reali. L'utente ha scelto la proposta A e autorizzato implementazione e pubblicazione. Dettaglio e limiti del prototipo in `PROPOSTA.md` accanto all'HTML. Nessun contenuto dimostrativo entra nell'APK.
