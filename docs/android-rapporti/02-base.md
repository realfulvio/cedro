# Base Android: misure prima delle modifiche

16 novembre 2026. Mi 9T `davinci_eea`, Android 11 API 30, 1080×2340 pixel, densità 440 dpi (circa 393 dp di larghezza). Pacchetto `it.cedro.android`, attività `CedroActivity`, versione installata 0.45.0, codice 4500, minSdk 26/target 34. Xiaomi 13: NA, escluso per ora dall'utente.

| Misura | Risultato | Metodo/limite |
|---|---:|---|
| Avvio dopo force-stop | 251 / 213 / 275 ms; mediana **251 ms** | `am start -W`, tre ripetizioni; cache filesystem non svuotata. Primo avvio della sessione 361 ms, separato. |
| PSS a riposo | 58.759 KiB (**57,38 MiB**) | Singolo campione meminfo dopo assestamento, non mediana di tre. |
| RSS | 153.764 KiB | Stesso campione. |
| Swap PSS | 808 KiB | Stesso campione. |
| Scorrimento Impostazioni | **38,55 fps** | SurfaceFlinger, 53 frame/52 intervalli in 1,348973 s; campione corto, non misura generale della UI. Mediana intervallo 33,13 ms. |
| gfxinfo | NA | NativeActivity non popola la statistica: zero frame; non interpretare i percentili sentinella. |
| APK installato estratto | **58.734.937 byte** (58,73 MB) | `adb pull base.apk`; il pacchetto pubblicato può differire per signing/alignment. |
| Font 100%/200% | Identici nell'area app | Screenshot `13-home-font100` e `14-home-font200`, confronto pixel esclusa status bar, fontScale ripristinato a 1.0. |
| Nodi accessibili | Nessun controllo nominato | UIAutomator: sola superficie; non sostituisce una prova vocale TalkBack. |
| Condivisione e conversione JPEG | Completata | SEND a processo aperto, output in Documents/Cedro via MediaStore; non è stata completata una prova JPEG→PDF in questa fase. |
| Batteria per lavoro lungo | Non misurata | Telefono in carica; nessuna stima derivata dal livello percentuale. |
| Cinque carichi di riferimento | Da completare | Non indicati come riusciti sulla base di test desktop. |

Ripartizione delle librerie nell'APK estratto: libcedro_android 50.639.320 byte; riassunto 4.738.248; libc++ 1.794.776; QuickJS 916.104; ffmpeg 317.120; ffprobe 233.664; launcher Python 4.384. DEX 7.348 byte. Le risorse grandi scaricate al primo uso non sono incluse in queste dimensioni.

## Ambiente di build

PC di collaudo `dev`, Ryzen 7 5700U, copia isolata `/srv/dev/projects/cedro-android-fase1`. SDK `~/android-sdk`, NDK r27c, build-tools android-14, piattaforma android-11, JDK e cargo-ndk presenti. Cache copiata con reflink dalla build precedente senza modificare il checkout di origine. Nessuna compilazione sul notebook.

Prima compilazione host del crate Android sulla base `0a3a64b`: **fallita E0004**, nuovi `Kind::Number`/`Meaning` non coperti in rivedi.rs. Log conservato nel PC di collaudo in `render.log`. L'APK della nuova base, le due varianti con/senza componenti YouTube e la suite motore vanno misurati dopo la correzione: nessun esito anticipato.

Evidenze locali: `~/Documenti/cedro-android-anteprima/prove-mi9t`. Corpus e script di installazione sono in preparazione in `~/prove-cedro-android/`; i risultati successivi saranno separati da questi valori iniziali.
