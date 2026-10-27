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

- **Foglio dal basso** dopo «Condividi»: il file (miniatura, nome, tipo, peso), le azioni adatte a quel tipo e «Tutte le azioni». Si chiude toccando fuori o col tasto indietro.
- **Ruota a tutto schermo** con spicchi da pollice (anche aprendo Cedro dall'icona: riapre l'ultimo file); selettore Converti / Strumenti e cursore Qualità dove il motore la usa.
- **Aspetto**: i cinque temi di Cedro, ricordati tra un avvio e l'altro; icone delle barre di sistema chiare o scure col tema.
- Bersagli ≥ 48 dp, margini delle barre di stato e dei gesti letti dalla finestra (JNI), orizzontale e verticale. Ogni controllo chiama il motore: niente controlli finti.
- `cargo run -p cedro-android --example vista -- cartella` disegna le schermate in PNG sul computer, per provare i layout senza il telefono.

## Provato su Xiaomi Mi 9T (Android 11, arm64)

Foglio, ruota, conversione di un PNG (da `file://` e da un vero `content://` di MediaStore) e di un PDF, cursore qualità, cambio tema, seconda condivisione, tema scuro: schermate in `docs/shots/android-*.png`.
Memoria dell'app dopo le conversioni: ~34 MB PSS (132 MB RSS); nessun processo figlio.

## Limiti noti

- Scrive in `/storage/emulated/0/Documents` (target SDK 29 + legacy storage, permesso richiesto a runtime).
- `noHistory`: Cedro si chiude quando si esce, così ogni «Condividi» parte da zero (NativeActivity non riceve i nuovi intenti).
- Il lavoro gira finché l'app è in primo piano: manca ancora un servizio con notifica per i lavori lunghi.
- Aprire il risultato o condividerlo dal foglio richiederebbe un FileProvider (codice Java): non c'è, quindi mancano «Apri» e «Condividi».
- Orizzontale: provato solo nelle schermate disegnate sul computer (il telefono non è stato ruotato).
- Solo arm64; PDF → Testo/Word OCR, video, trascrizione e OCR non provati.
