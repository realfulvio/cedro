# Pubblicare Cedro per Android

Tutto è preparato, **niente è ancora pubblico**: nessuna release, nessuna pagina, nessun secret caricato.

## Le chiavi (fuori dal repository)

Sul computer di Luca, in `~/.cedro-android/` (cartella 700, file 600; da copiare in un posto sicuro: **perdere la chiave dell'app vuol dire non poter più aggiornarla**):

- `release.keystore` + `release.env`: la chiave che firma l'APK (RSA 4096, valida 100 anni; alias `cedro`).
- `fdroid-repo.p12` + `fdroid-repo.env`: la chiave che firma l'indice del repository F-Droid (alias `cedro-repo`). È diversa da quella dell'app.

Fingerprint della chiave dell'app (SHA-256): `4B:B8:21:68:2D:7C:15:0A:65:21:9A:1A:A7:BA:D7:82:4F:6C:4C:31:8F:DE:C4:88:8E:6F:3B:76:85:5D:C0:B7` — da controllare con `apksigner verify --print-certs`.

Costruire un APK firmato a mano: `set -a; . ~/.cedro-android/release.env; set +a; android/build.sh` → `android/target/cedro-<versione>.apk`.

## Passi che richiedono un'azione di Luca

0. **I due workflow stanno in `android/ci/`**, non in `.github/workflows/`: il token di `gh` sul computer non ha lo scope `workflow` e GitHub rifiuta il push. Da fare una volta: `gh auth refresh -s workflow` (apre il browser), poi `git mv android/ci/*.yml .github/workflows/` e push.
1. **Secret del repository** (Settings → Secrets and variables → Actions), oppure dal terminale:
   ```
   . ~/.cedro-android/release.env; . ~/.cedro-android/fdroid-repo.env
   base64 -w0 ~/.cedro-android/release.keystore   | gh secret set ANDROID_KEYSTORE_B64
   printf %s "$CEDRO_KS_PASS"                      | gh secret set ANDROID_KS_PASS
   printf cedro                                    | gh secret set ANDROID_KEY_ALIAS
   base64 -w0 ~/.cedro-android/fdroid-repo.p12    | gh secret set FDROID_KEYSTORE_B64
   printf %s "$FDROID_KS_PASS"                     | gh secret set FDROID_KS_PASS
   ```
2. **GitHub Pages**: Settings → Pages → Source: «GitHub Actions».
3. **Release**: tag `android-v0.21.3` (o Actions → Android → Run workflow con quel tag): il workflow `android.yml` costruisce l'APK firmato e lo allega a una release **bozza**. Quando la bozza è pubblicata, `android-fdroid.yml` aggiorna l'indice e lo mette su Pages: `https://realfulvio.github.io/cedro/fdroid/repo`.
4. **Catalogo ufficiale di F-Droid**: serve un account GitLab (merge request su `gitlab.com/fdroid/fdroiddata`) con `fdroiddata/it.cedro.android.yml` (bozza non provata sul buildserver) e `metadata/it.cedro.android/` (testi e icona). Da chiarire coi revisori: fdk-aac (licenza non libera) e il download del modello alla prima volta.

## Cosa c'è

- `android/ci/android.yml` (da spostare in `.github/workflows/`): APK firmato (senza secret, solo APK di debug come artefatto), release bozza.
- `android/ci/android-fdroid.yml` (idem): repository F-Droid su Pages con `fdroidserver`.
- `android/fdroid/metadata/`: metadati per il repository nostro (e testi in italiano per il catalogo).
- `android/fdroid/fdroiddata/`: bozza della ricetta per il catalogo ufficiale.
