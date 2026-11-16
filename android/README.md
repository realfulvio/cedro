# Cedro per Android (prototipo)

NativeActivity in Rust puro (`android-activity`, niente Java/Kotlin). Disegna la ruota di `cedro::ui`
(Semplificata, in verticale, palette «Windows essenziale») con tiny-skia; da «Condividi» riceve
immagini e PDF (`content://` letto via JNI/ContentResolver) e, al tocco di una fetta, li converte
col motore di Cedro salvando in `Documenti`.

Costruire: `android/build.sh` → `android/target/cedro-debug.apk` (serve rustup col target
`aarch64-linux-android`, `cargo-ndk`, NDK r27, build-tools, platform android-30 e un JDK; vedi lo script).
Installare: `adb install -r android/target/cedro-debug.apk`. Su Xiaomi/MIUI va prima attivato
«Installa via USB» nelle Opzioni sviluppatore.

Limiti noti: scrive in `/storage/emulated/0/Documents` (target SDK 29 + legacy storage); una nuova
condivisione a app aperta non ricarica l'intento (chiudere e ripetere); solo arm64-v8a.

## Interfaccia (per il tocco)

- **Barra in basso**: Ruota / Lavori / Aspetto.
- **Ruota** a tutta larghezza: il centro è il posto del file (cerchio tratteggiato «+ Scegli un file» se vuoto; foto, prima pagina del PDF o onda audio con tipo e peso se pieno, toccabile per cambiarlo). Sotto: cursore Qualità dove il motore la usa, «Converti in X» e «Ultimi file». Orizzontale e tablet: ruota a sinistra, opzioni a destra.
- **Cosa ne faccio?**: a tutto schermo dopo «Condividi»: il file in grande, griglia a due colonne con tutte le azioni adatte al tipo (per le immagini JPG, PNG, WebP, PDF, Comprimi, Testo/OCR, Dimezza e «Altro» che apre la ruota), Qualità e «Converti in X» sempre in basso.
- **Scegli il file**: Foto, Documenti, Audio, Altri file (selettore di Android, `ACTION_OPEN_DOCUMENT`) e i file recenti.
- **Lavori**: lavori in corso (barra che scorre: il motore non dà una percentuale), risultati con **Apri** e **Condividi**, cronologia (ricordata tra un avvio e l'altro, «Svuota»).
- **Firma digitale** (attività della finestra principale, come sul computer 0.37): «Apri e verifica» sceglie un .p7m, .p7s, .m7m, .tsd, .tsr o un PDF firmato; anche gli allegati aperti dalla mail o da WhatsApp («Apri con», `ACTION_VIEW` e «Condividi») finiscono qui. Il visualizzatore (`src/ui/firma.rs`) usa lo stato del computer (`cedro::ui::firma::FirmaUi`): fascia col giudizio, una scheda per firma con i quattro controlli, poi il documento (pagine preparate da `glue` in un thread); in basso Salva, Apri, Condividi e Rapporto (scritti in Documenti/Cedro come un lavoro). Revoca online ed elenco UE dalla sezione; «Verifica una cartella» resta sul computer.
- **Menu dei formati**: iconcina colorata e suggerimento per ogni voce (`cedro::ui::voci`), gruppi «Converti in» e strumenti; «Togli GPS e dati nascosti» tra gli strumenti delle immagini.
- **Grafica Material 3 Expressive** nel tema base (`src/ui/m3.rs`): toni di superficie ricavati dalla tavolozza, pillole, chip, righe tonali, interruttori 52×32, barra di navigazione 64 dp con pastiglia 56×32, timbro smerlato per il giudizio delle firme. Gli altri temi tengono la loro impaginazione e prendono fogli, firma e navigazione.
- **Animazioni** (`src/ui/anim.rs`): fogli che salgono, linea della categoria e attività che scivolano, interruttori; ferme con «Rimuovi animazioni» di Android.
- **Aspetto**: i cinque temi di Cedro, ricordati; icone delle barre di sistema chiare o scure col tema.
- Bersagli ≥ 48 dp, margini delle barre letti dalla finestra (JNI). Ogni controllo chiama il motore: niente controlli finti.
- `cargo run -p cedro-android --example vista -- cartella` disegna le schermate in PNG sul computer.

## L'unica eccezione: una classe Java

Tutto è Rust tranne `java/it/cedro/android/CedroActivity.java` (una ventina di righe): `NativeActivity` non inoltra
`onActivityResult`, e `onNewIntent`, quindi senza questa classe il selettore di file di Android non potrebbe restituire la scelta e una nuova condivisione a app aperta andrebbe persa.
La classe fa solo da inoltro (`scelto()` restituisce l'ultimo `Uri`, `novita()` dice se è arrivato un nuovo intento); intento, lettura e copia del file sono in Rust (JNI).
`build.sh` la compila con `javac` + `d8` e la mette nell'APK come `classes.dex`.
«Apri» e «Condividi» non usano un FileProvider: il risultato si registra in MediaStore (`MediaScannerConnection`) e si usa il suo `content://`.

## Icona

Lo Spicchio del progetto: icona adattiva (primo piano bianco su viola) in `res/mipmap-anydpi-v26`, più le versioni
mdpi–xxxhdpi quadrata e tonda; `android:icon` e `android:roundIcon` nel manifest; risorse compilate con `aapt2`, senza Gradle.

## Limiti noti

- Scrive in `/storage/emulated/0/Documents` (target SDK 29 + legacy storage, permesso richiesto a runtime).
- `singleTask`: una nuova condivisione a Cedro già aperto arriva da `onNewIntent` (inoltrato dalla classe Java) e sostituisce il file.
- I lavori lunghi girano in un servizio in primo piano con notifica e percentuale.
- «Ritaglia» non esiste nel motore, quindi non c'è. Solo arm64.

## Trascrizione (whisper.cpp nell'APK)

Audio e video hanno «Testo, dalla voce, offline»: whisper.cpp è una libreria dentro l'APK (`src/whisper_android`, thread nel
processo, niente eseguibile da scaricare); al primo uso si scarica solo il modello «Buona» (base q5_1, 60 MB; serve il permesso INTERNET).
Compilato con dotprod+fp16 (`WHISPER_ARCH`), con controllo HWCAP a runtime; l'APK porta `libc++_shared.so`.

Misure sul Mi 9T (Snapdragon 730, 2 core grandi + 6 piccoli, 5,7 GB), voce italiana sintetica di 12 s:
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
nessun campo di testo per scrivere un link (si condivide o si incolla dagli appunti).

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

Il caso/campione viene salvato atomicamente nello stesso `lab/caso.txt` usato dal motore; il codice passa alle `Options`, la nota privata non entra nei nomi o nei lavori. I risultati conservano il caso di quando è partito il lavoro anche se il caso attivo cambia. «Copia per il quaderno» copia soltanto su richiesta i nomi dei risultati odierni del caso attivo (tutti se non c’è un caso). «Togli caso» ripristina i nomi ordinari. I nomi dei download web diretti sono materia del task web successivo: il percorso di conversione e quello della trascrizione ricevono già le opzioni con il caso.

Stato delle prove di questa implementazione: parsing e controlli statici locali, **non ancora collaudo del candidato Android**. DICOM è ancora letto interamente dal motore comune; i piani di microscopia e gli originali di firme staccate possono richiedere molta RAM. Non è imposto un limite arbitrario al file. RAM/tempo sui campioni grandi, font, anteprime e provider SAF reali restano gate prima della pubblicazione. Comandi e risultati effettivi in `.superpowers/sdd/ESECUZIONE/task-3-report.md`.
