#!/usr/bin/env bash
# Costruisce android/target/cedro-debug.apk (firmato con la chiave di debug) oppure, con CEDRO_KEYSTORE (+ CEDRO_KS_PASS,
# CEDRO_KEY_ALIAS, facoltativa CEDRO_KEY_PASS), android/target/cedro-<versione>.apk firmato con la chiave di rilascio.
# Versione: dal Cargo.toml di Cedro (CEDRO_VERSION_NAME / CEDRO_VERSION_CODE per forzarla). Niente Gradle, niente Java nel codice:
# serve solo il JDK per apksigner/keytool. Variabili: ANDROID_SDK (default ~/android-sdk).
set -euo pipefail
cd "$(dirname "$0")/.."
SDK="${ANDROID_SDK:-$HOME/android-sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$SDK/android-ndk-r27c}"
BT="${ANDROID_BUILD_TOOLS:-$SDK/build-tools/android-14}"
JAR="${ANDROID_JAR:-$SDK/platforms/android-11/android.jar}"
# fdk-aac (AAC per il video) include «log/log.h» con ANDROID definito: un'intestazione vuota basta.
export CFLAGS_aarch64_linux_android="-I$PWD/android/ndk-stubs" CXXFLAGS_aarch64_linux_android="-I$PWD/android/ndk-stubs"
# openh264 è C++: la libreria standard si lega dentro la .so (c++_static), senza libc++_shared.so nell'APK.
export CXXSTDLIB_aarch64_linux_android=c++_static
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-lc++abi"
export JAVA_HOME="${JAVA_HOME:-$SDK/jdk}"
export PATH="$JAVA_HOME/bin:$HOME/.cargo/bin:$PATH"
OUT=android/target; mkdir -p "$OUT/apk/lib/arm64-v8a"
export CFLAGS_aarch64_linux_android="-I$PWD/android/ndk-stubs" CXXFLAGS_aarch64_linux_android="-I$PWD/android/ndk-stubs"
# whisper.cpp con dotprod+fp16 (guardia HWCAP in trascrizione.rs); cmake dell'NDK: serve il file di toolchain e un generatore.
export CMAKE_GENERATOR="Unix Makefiles" CMAKE_TOOLCHAIN_FILE="$ANDROID_NDK_HOME/build/cmake/android.toolchain.cmake" ANDROID_NDK="$ANDROID_NDK_HOME"
# libopus (audiopus_sys) ha un CMakeLists vecchio: cmake 4 vuole il minimo dichiarato.
export CMAKE_POLICY_VERSION_MINIMUM=3.5
export PATH="$PWD/android/tools:$PATH" WHISPER_ARCH="${WHISPER_ARCH:-armv8.2-a+dotprod+fp16}"
cargo ndk -t arm64-v8a -P 26 build --release -p cedro-android
cp target/aarch64-linux-android/release/libcedro_android.so "$OUT/apk/lib/arm64-v8a/"
cp "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" "$OUT/apk/lib/arm64-v8a/"
# Risorse (icona del launcher, adattiva da Android 8) compilate con aapt2, senza Gradle.
rm -rf "$OUT/res.zip" "$OUT/classes"; mkdir -p "$OUT/classes"
"$BT/aapt2" compile --dir android/res -o "$OUT/res.zip"
# Le uniche due classi Java (CedroActivity inoltra onActivityResult/onNewIntent; ServizioLavoro è il servizio con notifica); vedi i commenti.
javac -source 8 -target 8 -Xlint:-options -bootclasspath "$JAR" -d "$OUT/classes" android/java/it/cedro/android/*.java
"$BT/d8" --min-api 26 --lib "$JAR" --output "$OUT/classes" $(find "$OUT/classes" -name '*.class')
VNAME="${CEDRO_VERSION_NAME:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
IFS=. read -r V1 V2 V3 <<< "$VNAME"
VCODE="${CEDRO_VERSION_CODE:-$((V1 * 10000 + V2 * 100 + ${V3%%[!0-9]*}))}"
"$BT/aapt2" link -o "$OUT/base.apk" -I "$JAR" --manifest android/AndroidManifest.xml --min-sdk-version 26 --target-sdk-version 29 --version-name "$VNAME" --version-code "$VCODE" "$OUT/res.zip"
python3 -c "import zipfile,sys; z=zipfile.ZipFile(sys.argv[1],'a'); z.write(sys.argv[2],'lib/arm64-v8a/libcedro_android.so',compress_type=zipfile.ZIP_STORED); z.write(sys.argv[2].replace('libcedro_android.so','libc++_shared.so'),'lib/arm64-v8a/libc++_shared.so',compress_type=zipfile.ZIP_STORED); z.write(sys.argv[3],'classes.dex'); z.close()" "$OUT/base.apk" "$OUT/apk/lib/arm64-v8a/libcedro_android.so" "$OUT/classes/classes.dex"
"$BT/zipalign" -f -p 4 "$OUT/base.apk" "$OUT/aligned.apk"
if [ -n "${CEDRO_UNSIGNED:-}" ]; then
  # Per il buildserver di F-Droid: firma lui.
  cp "$OUT/aligned.apk" "$OUT/cedro-$VNAME-unsigned.apk"
  exit 0
fi
if [ -n "${CEDRO_KEYSTORE:-}" ]; then
  # Chiave di rilascio: fuori dal repository; la password arriva dall'ambiente (in CI, dai secret), mai dalla riga di comando.
  export CEDRO_KS_PASS CEDRO_KEY_PASS="${CEDRO_KEY_PASS:-$CEDRO_KS_PASS}"
  "$BT/apksigner" sign --ks "$CEDRO_KEYSTORE" --ks-key-alias "${CEDRO_KEY_ALIAS:-cedro}" --ks-pass env:CEDRO_KS_PASS --key-pass env:CEDRO_KEY_PASS --out "$OUT/cedro-$VNAME.apk" "$OUT/aligned.apk"
  "$BT/apksigner" verify --print-certs "$OUT/cedro-$VNAME.apk" | head -3
  ls -la "$OUT/cedro-$VNAME.apk"
  exit 0
fi
KS="$OUT/debug.keystore"
[ -f "$KS" ] || keytool -genkeypair -keystore "$KS" -storepass android -keypass android -alias cedro -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Cedro debug"
"$BT/apksigner" sign --ks "$KS" --ks-pass pass:android --key-pass pass:android --out "$OUT/cedro-debug.apk" "$OUT/aligned.apk"
ls -la "$OUT/cedro-debug.apk"
