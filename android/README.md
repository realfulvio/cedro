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
