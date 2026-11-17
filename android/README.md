# Cedro per Android

App arm64 per Android 8 o successivo (minSdk 26, targetSdk 34). Interfaccia e motore in Rust, disegno tiny-skia; il ponte Java gestisce le callback Android che NativeActivity non espone. Le conversioni lavorano sul telefono.

Costruzione: `android/build.sh` produce `android/target/cedro-debug.apk`; con le variabili di firma documentate nello script produce l'APK di rilascio. Servono target Rust `aarch64-linux-android`, cargo-ndk, NDK r27, build-tools, platform android-30 e JDK. `CEDRO_YOUTUBE=1` include gli eseguibili per YouTube. La versione predefinita è letta dal Cargo.toml principale: 0.53.0 → versionCode 5300; non viene modificata la versione desktop. Installazione: `adb install -r android/target/cedro-debug.apk`; MIUI richiede «Installa via USB».

## Interfaccia

Le quattro tinte Cedro usano la variante A: **Inizio, Lavori, Impostazioni** restano le tre destinazioni. Inizio propone Converti, PDF, Trascrivi, Video dal web, Firma e Laboratorio. Il catalogo completo scorre per famiglie; le opzioni seguono il formato selezionato. I cinque temi precedenti rimangono selezionabili sul telefono.

«Apri con» e «Condividi» aprono il foglio Ricevuto, con i formati e la CTA, poi riportano i risultati nella stessa pagina Lavori. Un nuovo import azzera lo scroll. Il file picker usa ACTION_OPEN_DOCUMENT e conserva il grant URI; le cartelle del Laboratorio usano OPEN_DOCUMENT_TREE. Risultati, errori e copie in attesa di pubblicazione hanno journal recuperabili. Su Android 11+ la pubblicazione usa MediaStore in Documents/Cedro; le copie private non pubblicate restano raggiungibili dai Lavori.

Il renderer usa i margini delle barre di sistema, la scala caratteri del telefono e «Rimuovi animazioni». Il normale avanzamento e l'onda non richiedono un loop continuo a 60 Hz. Gli esempi `vista_a` producono PNG dal renderer reale con fixture separate dall'app.

## Ponte con Android

`CedroActivity.java` inoltra picker/intent, mantiene i grant e l'audiofocus ed espone il provider di accessibilità descritto in fondo. `ServizioLavoro.java` mantiene la notifica dei lavori in background; `ArchiviProvider.java` pubblica in sola lettura le uscite private su richiesta. Sono adattatori OS: conversioni, player, catalogo e stato restano Rust. La pipeline è javac + d8, senza Gradle o nuove dipendenze. MediaStore e il provider privato conservano MIME/nome per Apri e Condividi.

## Icona

Lo Spicchio del progetto: icona adattiva (primo piano bianco su viola) in `res/mipmap-anydpi-v26`, più le versioni
mdpi–xxxhdpi quadrata e tonda; `android:icon` e `android:roundIcon` nel manifest; risorse compilate con `aapt2`, senza Gradle.

## Limiti noti

- Android 8–10 usa la scrittura in Documents con il permesso legacy; Android 11+ usa MediaStore.
- `singleTask`: una nuova condivisione a Cedro già aperto arriva da `onNewIntent` (inoltrato dalla classe Java) e sostituisce il file.
- I lavori lunghi girano in un servizio in primo piano con notifica e percentuale.
- «Ritaglia» non esiste nel motore, quindi non c'è. Solo arm64.

## Trascrizione (whisper.cpp nell'APK)

Audio e video hanno «Testo, dalla voce, offline»: whisper.cpp è una libreria dentro l'APK (`src/whisper_android`, processo worker separato, nessun motore da scaricare); al primo uso si scarica solo il modello «Buona» (base q5_1, 60 MB; serve il permesso INTERNET).
Compilato con dotprod+fp16 (`WHISPER_ARCH`), con controllo HWCAP a runtime; l'APK porta `libc++_shared.so`.

Misure storiche della base, non del candidato 0.53: Mi 9T (Snapdragon 730, 2 core grandi + 6 piccoli, 5,7 GB), voce italiana sintetica di 12 s:
tiny 4,7 s (228 MB di RAM), base 8–9 s (370 MB), small circa 5× più lento del tempo reale (660 MB: sconsigliato).
Thread: 2 sui core grandi 11,5 s, 7 thread 8,2–9,6 s (per questo ora si usano tutti i core meno uno).
Dotprod+fp16 contro solo NEON: 9,6 s contro 12,5 s (−23 %).
Vulkan: l'Adreno 618 del 9T espone solo Vulkan 1.1 e ggml vuole la 1.2 («Vulkan 1.2 required»): ripiego sul processore.
Non è nell'APK (legarlo richiederebbe anche API 28+).

## Video dal web (senza yt-dlp)

Un link arriva da «Condividi» di un browser (testo con un indirizzo) o dagli appunti («Scegli il file» → «Link dagli appunti»);
il foglio offre Video, Solo audio e Video leggero (`src/webvideo.rs`, Rust puro, nessun programma esterno né codice scaricato,
quindi compatibile con F-Droid). Funziona con: link diretti a file audio/video, pagine con `og:video`, `<video>`/`<source>`/JSON-LD,
Internet Archive (sceglie l'originale, il file più piccolo o l'MP3 già pronto). Provato sul Mi 9T con un oggetto di Internet Archive
(Video 2,0 MB, Solo audio 56 KB). Limiti: **YouTube non è supportato** (dice chiaramente perché), nessun flusso HLS/DASH né video con DRM,
Il pannello A ora offre anche un campo per scrivere il link e il comando Incolla.

## YouTube (con yt-dlp, solo nel nostro APK)

Con `CEDRO_YOUTUBE=1 android/build.sh` (la release su GitHub e il nostro repo F-Droid) i link di YouTube offrono Video, Solo audio,
Video leggero e Testo (audio scaricato e trascritto). Fa tutto `src/youtube.rs`, come sul computer, con le differenze di Android
(modulo `youtube::android`): Android lascia partire programmi solo dalla cartella delle librerie dell'APK, quindi l'APK contiene
quattro eseguibili precompilati di [youtubedl-android](https://github.com/yausername/youtubedl-android) 0.18.1 (pacchetti di
[Termux](https://github.com/termux/termux-packages)): `libpython.so` (Python), `libqjs.so` (QuickJS, il runtime JavaScript che
YouTube chiede), `libffmpeg.so` e `libffprobe.so`. `android/youtube-libs.py` li prende da Maven Central, controllando lo SHA-256.
Al primo uso Cedro scarica da Maven Central solo le librerie di Python (14 MB) e di ffmpeg (35 MB), leggendo dall'archivio la parte
per arm64; yt-dlp (la versione in Python) arriva da GitHub e si aggiorna da solo come sul computer.
La build per il catalogo F-Droid (senza `CEDRO_YOUTUBE`) non contiene niente di tutto questo: lì YouTube passa il link a un'altra app.

## Office e Laboratorio — ramo Android 0.53, verifiche in corso

Il catalogo completo viene dal motore: Word/ODT/RTF/DOC, fogli Excel/Calc e immagini HEIC usano le stesse conversioni del desktop. Le opzioni di stampa Office, vetrino, DICOM e microscopia seguono il file selezionato. Android cerca i font di sistema anche in `/system/fonts`, `/product/fonts` e `/system_ext/fonts`; nessun font Office è incorporato. La fedeltà di impaginazione richiede il confronto sullo stesso corpus desktop/telefono.

Il Laboratorio usa il worker Rust separato anche per le anteprime, cancellabili. Un vetrino SVS/NDPI/MRXS mostra la panoramica oppure un’area centrale di massimo 1024×1024 pixel del livello scelto, letta con `read_region`. CZI/ND2/LIF offrono scena, canale, piano Z e istante; DICOM immagine e finestra. Questi selettori cambiano **solo l’anteprima della sorgente**. Le opzioni del risultato controllano l’esportazione condivisa: il motore esporta tutte le scene di CZI/LIF, con canali e piani composti/separati secondo le scelte. Non esiste ancora nel motore un export della sola scena/canale/Z selezionata in anteprima.

Copia anonima, tessere AI, zona per conta mitotica e consulto nativo di vetrino seguono il supporto del motore (SVS/NDPI/MRXS). La copia anonima **nel formato originale CZI/ND2/LIF non è supportata**: Android lo segnala prima di leggere il file intero; la normale conversione PNG resta disponibile e non viene presentata come copia anonima. DICOM usa l’anonimizzatore condiviso. Eventuali nomi impressi nei pixel richiedono verifica visiva. Cedro non esegue interpretazioni cliniche.

Per MRXS scegliere la cartella contenente l’header `.mrxs` e la cartella dati omonima. Gli alias descriptor SAF e le sottocartelle sono mantenuti anche nella copia anonima e nei pacchetti BagIt; non vengono seguiti collegamenti a directory. Figura multipannello, PDF RX e pacchetto con impronte lavorano sull’insieme dei file selezionati.

Il caso/campione viene salvato atomicamente nello stesso `lab/caso.txt` usato dal motore; il codice passa alle `Options`, la nota privata non entra nei nomi o nei lavori. I risultati conservano il caso di quando è partito il lavoro anche se il caso attivo cambia. «Copia per il quaderno» copia soltanto su richiesta i nomi dei risultati odierni del caso attivo (tutti se non c’è un caso). «Togli caso» ripristina i nomi ordinari. Il modello dei nomi e il caso si applicano anche ai download web diretti, alle raccolte e ai file dei capitoli, una sola volta per risultato.

Stato delle prove di questa implementazione: parsing e controlli statici locali, **non ancora collaudo del candidato Android**. DICOM è ancora letto interamente dal motore comune; i piani di microscopia e gli originali di firme staccate possono richiedere molta RAM. Non è imposto un limite arbitrario al file. RAM/tempo sui campioni grandi, font, anteprime e provider SAF reali restano gate prima della pubblicazione. Comandi e risultati effettivi in [rapporto Task 3](../docs/android-rapporti/task-3-report.md).


## Video dal web — ramo Android 0.53, verifiche in corso

Il pannello A condivide Video, Solo audio, Video leggero e Testo per tutti i link. Qualità, formato, sottotitoli, copertina, capitoli, metadati, divisione capitoli, sponsor, volume, raccolte e file già presenti passano allo snapshot `web::Scelte` e alla richiesta effettiva. Nei link generici il motore supporta MP4 e MP3/Opus/WAV/FLAC: scarica il media sorgente e applica `Options.video_height` o la conversione audio; Originale conserva i byte e il formato del sito. Le opzioni di estrazione specifiche di YouTube rimangono visibili ma disabilitate con spiegazione. La build senza YouTube continua a delegare a un'altra app.

La riga dei componenti riceve eventi reali dal processo worker (modelli/motori) o dall'estrattore nel genitore (Python/ffmpeg/yt-dlp), con byte e percentuale; lo stesso stato alimenta la notifica. Gli errori rimangono 12 secondi, annullamento e crash rimuovono gli stati attivi. Il redraw dei lavori segue i cambi di progresso: un lavoro attivo da solo non richiede più 60 fotogrammi al secondo.

`ACCESS_NETWORK_STATE` serve esclusivamente a distinguere la rete a consumo nella policy download: Chiedi una volta (default), Solo Wi-Fi, Consenti anche rete mobile. Non richiede un nuovo componente Java né autorizzazioni runtime. La scelta si conserva in `rete-download.txt`; il worker chiede l'autorizzazione al genitore mediante IPC duplex. Per 100+ MB mostra dimensione e spazio libero senza aggiungere una conferma obbligatoria; spazio insufficiente produce un errore utile. Durante i componenti la rete si ricontrolla al massimo una volta al secondo. Una rete a consumo sospende il downloader se la policy non la consente, preservando l'offset. Python/ffmpeg riprendono le richieste Range dal byte mancante; yt-dlp e modelli usano il downloader condiviso con retry e checksum.

L'annullamento worker invia SIGUSR1 (handler limitato a un flag atomico), poi termina il gruppo dopo un massimo di 1,5 secondi di grazia. Prima di scrivere un nuovo `.part`, il core lo reclama con `create_new` e registra il percorso attraverso l'IPC con conferma del genitore. Solo questi percorsi, se device e inode corrispondono ancora al file reclamato, vengono rimossi dopo un annullamento forzato: nessuna scansione per differenza, nessuna rimozione di modelli o parziali preesistenti.

Una raccolta destinata a Testo genera un lavoro per ciascun audio, serializzato con il permit esistente. Il manifest `.batch` precede la movimentazione degli audio; le chiavi `<batch>/audio-<indice>` restano stabili anche dopo riavvio. Ogni figlio conserva `.lavoro`, `.batch-child`, `.revisione-source` e il proprio audio in `.revisione`. Il completamento atomico consegna insieme stato del download e manifest. Il main assegna soltanto ID/stato; letture, rename, journal e avvio dei figli restano nei thread di lavoro. Annullare un figlio non elimina l'audio degli altri; le chiavi rimosse dalla cronologia non sono reimportate. Il worker generico per un singolo link mantiene il contratto precedente della revisione.

I log diagnostici riportano soltanto ID, azione, esito e numero uscite: la fine è registrata dopo il journal/pubblicazione, senza nomi, contenuti o URI. La nota sul recupero senza sottotitoli è conservata anche nel risultato YouTube. La suite esistente simula un 429 senza rete.

Gate di collaudo: build con/senza YouTube, test host, rendering IT/DE/PT e prove Mi9T affidati al controller. Non usare `pm clear`, disinstallazioni o cancellazioni dei componenti personali per simulare il primo uso. Gli esempi `vista_a` contengono solo fixture di rendering e non entrano nell'app.

«Evita copie già salvate» è una policy del singolo lavoro: non cambia la preferenza globale. Il marker privato `.web-request` identifica link, opzioni e caso; il flag per evitare duplicati e la modalità Exists non fanno parte della firma di contenuto. Prima di pubblicare vengono consultati i journal `.pronti` compatibili, verificati i nomi relativi e l'esistenza effettiva degli URI (mai `is_pending=1`). Per insiemi identici si riusa il namespace della raccolta. Le copie già presenti passano comunque per prenotazione e commit del nuovo journal prima di rimuovere la copia privata. Un file eliminato viene pubblicato di nuovo. Questo evita copie duplicate, ma il download e l'eventuale trascrizione possono avvenire comunque. Non viene creato un archivio yt-dlp che dia per presenti file cancellati dall'utente.

## Trascrizione e ascolto Android 0.53

Il player condiviso riproduce via cpal/AAudio in Rust. La velocità ricampiona il segnale
(e cambia anche l'altezza della voce); volume con i tasti del telefono. Onda in thread,
600 campioni memorizzati e disegno a risoluzione dello schermo. Il timer del player è 5 Hz;
non tiene attivo il ciclo grafico a 60 Hz.

`CedroActivity` inoltra ora anche perdita di focus audio, MEDIA_PAUSE/STOP e distacco
cuffie: AudioManager richiede un listener Java e NativeActivity non espone queste
callback a Rust. È un'estensione motivata del ponte esistente, senza nuovi file Java,
librerie o motori audio. Il player si mette in pausa alla perdita del focus e quando
l'attività va in pausa (anche schermo spento). La trascrizione e il riassunto continuano
nel worker col servizio di lavoro esistente; quel servizio non dichiara riproduzione
multimediale e non viene usato per tenere vivo l'ascolto. Ripresa audio sempre esplicita.

Referti: la richiesta vocale include il flag e il glossario veterinario; le conferme
sono salvate con la revisione e legate al contenuto della frase. TXT, Word, SRT e VTT
usano l'esportatore condiviso col desktop: stesso nome bozza/rivisto e avviso di bozza
(nota VTT, primo sottotitolo SRT). Il risultato senza parlato è una scheda neutra senza
un file pubblicato. Le prove ad orecchio, chiamata, cuffie, durata e RAM sul Mi 9T sono
gate del controller; questa sezione descrive l'implementazione, non un esito misurato.

## Font e accessibilità della superficie nativa

Il testo usa anche `Configuration.fontScale` del telefono; il Painter misura e va a capo con lo stesso fattore usato per disegnare. Le famiglie del catalogo si dispongono su più righe quando le etichette non entrano. Figtree resta il carattere dei temi Cedro; per nomi cirillici, arabi e CJK si carica un Noto/Roboto di sistema soltanto al primo glifo mancante. Non si aggiungono font all'APK. Il pittore a glifi conserva un limite: non è un motore completo di shaping bidirezionale per paragrafi arabi.

`NativeActivity` non espone i controlli disegnati da Rust. Per questo `CedroActivity` ospita un `AccessibilityNodeProvider` sulla superficie trasparente: riceve testo completo, bounds visibili e bersagli reali dal frame Rust. Click e scorrimento entrano in una coda sincronizzata Java→Rust; il thread UI Rust la consuma nel ciclo eventi e riusa touch/hit-test e dispatcher degli effetti. Non si tenta di reinserire MotionEvent Java nella InputQueue di NativeActivity. Gli hover non gestiti vengono lasciati alle View per l’esplorazione tattile. Non esiste una seconda implementazione dei comandi. Il provider gestisce focus ed esplorazione TalkBack, con stato selezionato delle scelte principali. È un'estensione motivata dell'eccezione Java già necessaria per SAF/audiofocus, senza nuova dipendenza o permesso. La verifica con UIAutomator/TalkBack sul candidato resta nel rapporto finale: l'implementazione da sola non certifica il collaudo.


## Ingressi SAF e verifiche finali

Gli input seekable grandi e i vetrini usano lease su descriptor autorizzati, senza copia preventiva. `input_file` associa ogni percorso privato alla propria lease corrente e legge con pread e cursori logici indipendenti. Un vecchio alias non registrato dopo riavvio viene rifiutato, anche se il numero fd è stato riusato. I decoder nativi con API fd ricevono direttamente il descriptor; un lock interprocesso per la stessa sorgente protegge i loro cursori lseek/read, senza bloccare pread o altri file. Il decoder esterno usa il protocollo seekable fd soltanto se il binario concreto lo supporta. Il nome privato conserva estensione e struttura dei sidecar. I provider non seekable richiedono una copia a flusso con controllo dello spazio e cancellazione. Le operazioni che producono esplicitamente una copia anonima continuano a produrla.

Le tabelle di traduzione dell'APK vengono filtrate in compilazione e le risorse dell'edizione ente ritirata sono escluse dal target Android. Le tabelle desktop non cambiano. Non sono state aggiunte dipendenze, licenze o font incorporati.

Il controllo statico `python3 android/tools/check-i18n.py` verifica le chiavi letterali delle chiamate di testo/traduzione e stampa le mancanze nelle cinque lingue. Non sostituisce il controllo delle stringhe costruite dinamicamente né il collaudo delle sei lingue sul telefono. Rapporti conservati in [docs/android-rapporti](../docs/android-rapporti/); distinguono prove host, APK intermedi e candidato finale. Il controller completa misure, collaudo e review prima della pubblicazione.
