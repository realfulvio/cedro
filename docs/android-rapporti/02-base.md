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
| Condivisione e conversione JPEG | Completata | SEND a processo aperto, output in Documents/Cedro via MediaStore; il percorso SEND presenta il difetto descritto sotto; il percorso normale Converti→PDF è stato poi completato. |
| Batteria per lavoro lungo | Non misurata | Telefono in carica; nessuna stima derivata dal livello percentuale. |
| Audio243,3s→Testo, Buona e voci automatiche | **1.000,002s**, PSS massimo1.724.202KiB (~1,64GiB) | Una ripetizione in foreground;5thread effettivi,4voci trovate,120s di diarizzazione. Non è mediana di3prove. |
| JPG12MP→PDF | Tre ripetizioni dopo riscaldamento: mediana compresa fra **1,087 e1,985s** | Intervallo dal comando di tocco al log di pubblicazione; non tempo esatto. PSS massimo campionato414.744KiB. PDF riaperto con pdfinfo:1pagina A4,946.343byte. |
| MP3180s→WAV | Tre ripetizioni: mediana compresa fra **0,160 e1,018s** | Stesso metodo; PSS massimo campionato403.071KiB. WAV verificato:180s,44.100Hz,mono16bit. |
| PDF20pagine→Comprimi | Non raggiungibile nella UI0.45 esaminata | Il catalogo condiviso esclude le azioni pagina dal pannello; la vecchia schermata non espone i pulsanti dedicati. Nessun tempo inventato. |
| Video1080p60s→MP4 480p | Tre ripetizioni: mediana compresa fra **30,947 e31,821s** | PSS massimo309.861KiB; hardware MediaCodec. Output verificato60s,848×480,25fps,H.264/AAC,12.939.065byte (input95.892.995). |

Ripartizione delle librerie nell'APK estratto: libcedro_android 50.639.320 byte; riassunto 4.738.248; libc++ 1.794.776; QuickJS 916.104; ffmpeg 317.120; ffprobe 233.664; launcher Python 4.384. DEX 7.348 byte. Le risorse grandi scaricate al primo uso non sono incluse in queste dimensioni.

## Ambiente di build

Minipc `dev`, Ryzen 7 5700U, copia isolata `/srv/dev/projects/cedro-android-fase1`. SDK `/home/decok/android-sdk`, NDK r27c, build-tools android-14, piattaforma android-11, JDK e cargo-ndk presenti. Cache copiata con reflink dalla build precedente senza modificare il checkout di origine. Nessuna compilazione sul notebook.

Prima compilazione host del crate Android sulla base `0a3a64b`: **fallita E0004**, nuovi `Kind::Number`/`Meaning` non coperti in rivedi.rs. Log conservato nel minipc in `render.log`. L'APK della nuova base, le due varianti con/senza componenti YouTube e la suite motore vanno misurati dopo la correzione: nessun esito anticipato.

Evidenze locali: `~/Documenti/cedro-android-anteprima/prove-mi9t`. Corpus, script di installazione e raccolta misure sono in `~/prove-cedro-android/`; i risultati successivi saranno separati da questi valori iniziali.

## Ulteriori osservazioni durante il collaudo della base

Su una foto sintetica 4000×3000, dopo SEND e selezione dello spicchio PDF la ruota evidenzia PDF, ma il pulsante resta «Converti in JPG» e il log conferma un JPEG pubblicato. Ripetuto con tap e pressione breve. Non si conta quindi come prova JPEG→PDF e non si confrontano quei tempi con la nuova conversione PDF. Il nuovo foglio deve verificare anche la corrispondenza fra selezione ed effetto inviato al motore.

Corpus pronto in `~/prove-cedro-android/fixture`: 68 file, circa 1,80 GB, con manifest SHA-256 e script di aggiornamento che conserva i dati del Mi 9T. Include cinque DOCX di test Apache POI (origine nel manifest; archivio pubblico https://github.com/apache/poi/tree/trunk/test-data/document), documenti sintetici, i campioni del repository, foto 12 MP e video 1080p/60 s sintetici, toni/silenzio/ronzio e firme. Comprende due campioni reali OME: LIF di Michael Goelzer e ND2 di Karl Annusver, entrambi CC BY 4.0, con origine e SHA nel manifest. Include anche OS-3.ndpi da OpenSlide (1.473.064.608 byte), con SHA-256 verificato contro il manifest della fonte. Campioni CMU NDPI/MRXS più piccoli disponibili nel minipc. Ritrovato sul telefono `/sdcard/Download/dialogo.wav`: 243,3 s, mono16kHz, SHA256 `d3e7de424e19d52396c0c61c957772d58fa8b97261b2b45ba3298e04f11a3d9d`; recuperato nel corpus locale. La provenienza precisa di questo vecchio campione va conservata come non verificata: non viene incluso nella release.

Il selettore Android ha mostrato un limite dei tocchi automatizzati sui risultati: apertura riuscita selezionando il file con Tab e Invio. Non attribuito a Cedro senza isolamento della causa. La navigazione e il ritorno del file SAF sono stati verificati con `dialogo.wav`. La trascrizione con «Buona», Chi parla Automatico e impostazioni preesistenti è terminata: da21:28:38.259 a21:45:18.261, fra inizio lavoro e pubblicazione MediaStore. La UI inizialmente mostrava7thread; il motore ne ha dichiarati5 effettivi. Il numero di voci atteso dal piano (3) non coincide con quello trovato (4): il conteggio del campione va verificato, non forzato. Il risultato TXT è stato recuperato e conservato con i log.

Durante il lavoro, il thread `android_main` occupava circa un core (90,6% nel campione, quasi5min di CPU in5min di osservazione). Temperatura CPU letta dal servizio termico fino a69,3°C, stato termico0 e dispositivo di raffreddamento CPU6 a livello3; risparmio energetico globale disattivato. Nessuna conclusione di assenza di throttling: il confronto finale deve riportare le condizioni. Il valore storico di3min26s non è stato riprodotto su questa configurazione.

Le misure brevi usano timestamp del dispositivo prima e dopo il comando ADB `input tap`, correlati al log MediaStore. Il vero tocco è compreso fra quei due istanti: la mediana è riportata come intervallo, senza precisione fittizia al millisecondo. Il campionamento PSS circa1s può perdere picchi più brevi. Gli output sono stati recuperati dal telefono e riaperti per controllare formato e proprietà.

Nel video480p il thread UI raggiunge100%CPU nel campione; i thread NDK MediaCodec confermano il percorso hardware. Due snapshot dei processi dell’UID mostrano solo il processo principale. La PSS del servizio codec di sistema non è inclusa. Il nuovo worker andrà misurato sommando la PSS di UI e figli, per evitare un confronto ingannevole con la sola memoria della UI. Dopo la trascrizione la0.45 era tornata a49.211KiB PSS: il rilascio della memoria del modello esiste già nella base.

## Silenzio: regressione da correggere

`silenzio.wav` contiene30s mono16kHz di PCM interamente a zero (verificato leggendo tutti i campioni). Con Buona, Una persona sola e lingua automatica, la0.45 carica comunque Whisper, rileva inglese e pubblica il testo «you» dopo101,932s; PSS massimo campionato1.573.803KiB. Esito **KO**: questa prova non contiene voce. Log, TXT e schermata in `prove-mi9t/base-silenzio`. Il nuovo percorso deve dare «Nessun parlato riconosciuto» e non pubblicare un file vuoto o inventato.

A lavoro finito, un controllo alle22:27 (oltre20min dopo la pubblicazione) trova ancora1.545.558KiB PSS, di cui1.518.384KiB heap nativo, nella stessa app0.45 con pagina Trascrivi inattiva. Il log aveva scritto «memoria liberata»; questo campione mostra invece memoria residente ancora elevata. Non dimostra da solo una perdita: può includere memoria trattenuta dall'allocatore. Il risultato differisce dal campione49.211KiB dopo dialogo; entrambi vanno conservati, senza generalizzare il primo a ogni percorso. Evidenze in `prove-mi9t/idle-dopo-silenzio`.
