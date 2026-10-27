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
- Il lavoro gira finché l'app è in primo piano: manca ancora un servizio con notifica per i lavori lunghi.
- «Ritaglia» non esiste nel motore, quindi non c'è. Solo arm64; video, trascrizione e OCR non provati su Android.
