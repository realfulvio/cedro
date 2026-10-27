# Merge request per fdroiddata (da aprire dall'account GitLab di Luca)

**Titolo:** `New app: it.cedro.android (Cedro)`

**Descrizione (da incollare):**

```
Adds Cedro (it.cedro.android): file converter (images, PDF, audio, video) and offline speech-to-text, written entirely in Rust.

- Source: https://github.com/realfulvio/cedro (MIT), Android crate in `android/`, tags `android-v*`
- Descriptions, icon and screenshots are fastlane metadata in the upstream repo (`fastlane/metadata/android/it-IT`, `en-US`)
- Build: no Gradle. Rust (pinned with rustup) + cargo-ndk, NDK r27c; `android/build.sh` with CEDRO_UNSIGNED=1.
  All crates come from crates.io (Cargo.lock, `cargo fetch --locked`, then offline) or from `vendor/` (two patched crates, sources in the repo).
- arm64-v8a only (minSdk 26).
- No ads, no analytics, no accounts. INTERNET is used only to download the whisper.cpp speech model (data, ~32-60 MB) from Hugging Face the first time transcription is used.

Things I'd like your opinion on:
1. fdk-aac (Fraunhofer licence) is compiled in for AAC in the video output. If it is not acceptable we can drop it from the F-Droid build.
2. The OCR models (`models/*.onnx`, ocrs, CC BY-SA 4.0) are in the repository as data files.
3. I have not run the recipe on the buildserver; I will fix whatever the pipeline reports.

- [ ] The app complies with the inclusion policy
- [ ] I have read the contributing guide
```

**Prima di aprirla:** pubblicare il tag `android-v0.22.6` (o successivo) con questa cartella `fastlane/` e aggiornare `Builds` (versionName, versionCode, commit) di conseguenza.

**Come si apre (account GitLab):**
1. Fork di https://gitlab.com/fdroid/fdroiddata.
2. Nel fork: ramo `it.cedro.android`, aggiungere `metadata/it.cedro.android.yml` (il file qui accanto, senza le prime tre righe di commento).
3. Merge request verso `fdroid/fdroiddata` `master`, con titolo e descrizione sopra. La pipeline (lint, build) risponde da sola; correggere quello che segnala.
