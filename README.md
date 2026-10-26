<p align="center">
  <img src="docs/hero.png" alt="Cedro: cambia formato a un file con un clic destro">
</p>

<p align="center">
  <a href="https://github.com/realfulvio/cedro/releases/latest"><img alt="Ultima versione" src="https://img.shields.io/github/v/release/realfulvio/cedro?color=005E8E&label=versione&style=for-the-badge"></a>
  <img alt="Windows 10 e 11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-005E8E?style=for-the-badge">
  <img alt="Linux in arrivo" src="https://img.shields.io/badge/Linux-in%20arrivo-8A99A8?style=for-the-badge">
  <a href="LICENSE"><img alt="Licenza MIT" src="https://img.shields.io/badge/licenza-MIT-005E8E?style=for-the-badge"></a>
</p>

<p align="center">
  <a href="https://github.com/realfulvio/cedro/releases/latest"><b>⬇ Scarica</b></a>
  &nbsp;·&nbsp; <a href="#come-si-usa">Come si usa</a>
  &nbsp;·&nbsp; <a href="#cosa-converte">Formati</a>
  &nbsp;·&nbsp; <a href="#domande">Domande</a>
</p>

<p align="center">
  <img src="docs/feature.png" alt="Tutto sul tuo PC, niente da installare in più, l'originale al sicuro, in italiano">
</p>

## Cos'è

Cedro è un piccolo programma che vive vicino all'orologio. Tieni premuto **Shift**, fai **clic destro** su un file e si apre una ruota: scegli il formato e la copia nuova compare accanto all'originale. Fine.

<p align="center">
  <img src="docs/shots/ruota-shift-foto.png" width="340" alt="La ruota di Cedro su una foto, con la fetta JPEG sotto il mouse">
</p>

## Scarica e installa

1. Vai alla **[pagina delle versioni](https://github.com/realfulvio/cedro/releases/latest)** e scarica `Cedro-Setup-….exe`. Se il tuo PC è un Windows on ARM (Snapdragon X e simili), scarica invece `Cedro-Setup-…-arm64.exe`: è la versione nativa, più veloce nella trascrizione.
2. Doppio clic. Si installa solo per il tuo utente, **senza diritti di amministratore**.
3. La prima volta Windows può dire «Windows ha protetto il PC», perché l'installatore non è firmato. Clicca *Ulteriori informazioni*, poi *Esegui comunque*.
4. A fine installazione lascia spuntato **Avvia Cedro**. Non si apre una finestra: compare l'icona vicino all'orologio (se non la vedi, apri la freccia delle icone nascoste).


<p align="center">
  <img src="docs/shots/installatore-fine.png" width="460" alt="Ultima pagina dell'installatore: Avvia Cedro e Guarda come funziona">
</p>

Per togliere Cedro: *Impostazioni di Windows, App, Cedro*. Dopo la prima installazione si aggiorna da solo.

## Come si usa

<p align="center">
  <img src="docs/tre-tasti-guida.png" alt="Le tre ruote: Shift converte, Ctrl per gli strumenti, Alt per le azioni">
</p>

<p align="center">
  <img src="docs/shots/ruota-shift-pdf.png" width="250" alt="Ruota Shift su un PDF">
  <img src="docs/shots/ruota-ctrl-foto-ocr.png" width="250" alt="Ruota Ctrl su una foto, con OCR acceso">
  <img src="docs/shots/ruota-alt-zip.png" width="250" alt="Ruota Alt, con ZIP acceso">
  <br><sub>Screenshot veri su Windows 11: Shift su un PDF, Ctrl su una foto, Alt su un file.</sub>
</p>

1. Tieni premuto un tasto.
2. Fai **clic destro** su un file: in Esplora file, sul desktop o in una finestra Apri/Salva.
3. Clic sinistro sulla fetta che vuoi. Esc, o un clic fuori dalla ruota, annulla.

| Tasto | Ruota | Cosa fa |
| --- | --- | --- |
| **Shift** | Conversione | Cambia formato: foto, documenti, PDF, audio, video |
| **Ctrl** | Strumenti | Comprime, scala, bianco e nero, ruota, toglie i metadati, legge il testo (OCR) |
| **Alt** | Azioni | Duplica, copia il percorso, crea uno ZIP |

Le fette cambiano con il tipo di file. Se non sai da dove partire, apri la **Guida** dall'icona: una breve animazione mostra cosa fanno Shift, Ctrl e Alt.


## Cosa converte

| Tipo | Formati | Con Shift |
| --- | --- | --- |
| Immagini | PNG, JPEG, WebP, BMP, GIF, TIFF | altro formato, PDF con testo cercabile |
| Documenti | DOCX, DOC, ODT, RTF | PDF |
| Testo | TXT, MD, CSV, JSON, HTML, XML | altro testo, PDF |
| PDF | PDF | JPG, **Word modificabile** |
| Audio | WAV, MP3, OGG, M4A, AAC, FLAC | WAV, MP3 |
| Video | MP4, WebM, MOV, MKV, AVI | MP4 |

**PDF in Word:** titoli con stili veri, paragrafi, tabelle vere (celle, sfondi, bordi, intestazione ripetuta tra le pagine), elenchi puntati e numerati di Word, testo su più colonne e grafici al loro posto, modificabili come in un editor professionale. File leggeri. Le scansioni vengono lette con l'OCR in italiano.
**Foto dal telefono:** vengono raddrizzate in automatico.

## Il pannello Conversioni

<p align="center">
  <img src="docs/shots/pannello-conversioni.png" width="720" alt="Il pannello Conversioni di Cedro: coda di file, formato, strumenti, qualità e cartella di destinazione">
</p>

Dal menu dell'icona apri **Conversioni**: trascini i file (anche di tipi diversi), scegli formato, strumenti, qualità e dove salvare, e premi il pulsante in basso. Ogni categoria (immagini, documenti, PDF, testo, audio, video) ha le sue opzioni.

## L'icona e le impostazioni

<p align="center">
  <img src="docs/shots/menu-icona-tray.png" width="200" alt="Il menu dell'icona di Cedro">
  &nbsp;&nbsp;
  <img src="docs/shots/impostazioni-generale.png" width="440" alt="La finestra Impostazioni di Cedro">
</p>

Clic destro sull'icona (o doppio clic per le Impostazioni):

- **Impostazioni** (su Windows, sei sezioni a sinistra): avvio con Windows, avviso e cartella aperta a fine conversione; quali tasti aprono la ruota e se vale nelle finestre Apri/Salva; dove mettere la copia (stessa cartella, sottocartella «Cedro» o una cartella fissa) e se sostituire i file che esistono già; qualità di JPEG, WebP, MP3, risoluzione di PDF in JPG e forza di «Comprimi»; lingua e pagine al massimo dell'OCR; ricerca automatica degli aggiornamenti.
- **Conversioni:** un pannello con tutte le conversioni, la coda dei file (trascini i file o li scegli), la qualità e la cartella di destinazione. Ogni categoria ha le sue opzioni: foto (dimensioni, raddrizza, sfondo bianco), documenti (formato pagina, caratteri incorporati, segnalibri dai titoli, PDF/A-1b per l'archivio, password), PDF (OCR, tabelle, colonne, pagine come `1-3,5`, risoluzione, una immagine per pagina), testo (codifica, separatore CSV, JSON leggibile, titoli, PDF con caratteri incorporati o PDF/A), audio (kbps, canali, volume, taglio dell'inizio e del silenzio iniziale, tag) e video (risoluzione, fps, senza audio, ottimizzato per il web). Se il nome di uscita esiste già si sceglie: aggiungi `-2`, sostituisci, salta o chiedi ogni volta. I PDF/A di Cedro passano la validazione di veraPDF (profilo PDF/A-1b).
- **Guida:** l'animazione che spiega i tre tasti.
- **Controlla aggiornamenti:** Cedro lo fa anche da solo, verifica l'installatore e chiede prima di aggiornare.
- **Avvio automatico** ed **Esci**.

<p align="center">
  <img src="docs/shots/guida.png" width="560" alt="La Guida di Cedro: un'animazione mostra cosa fanno Shift, Ctrl e Alt">
</p>


## Domande

**Cambia il file originale?** No, crea sempre una copia nella stessa cartella.
**Serve internet?** Solo per cercare gli aggiornamenti.
**Perché Windows mi avvisa?** L'installatore non ha la firma a pagamento. Il codice è aperto e lo puoi controllare qui.
**E su Linux?** Per ora Cedro è un programma Windows. Linux è in pausa e torna in una prossima versione; chi vuole può già compilarlo (vedi sotto).

---

## Per chi sviluppa

<details>
<summary><b>Compilare, riga di comando, sicurezza, licenze</b></summary>

### Compilare

Serve [Rust](https://rustup.rs).

```
cargo build --release
```

- **Windows:** strumenti di compilazione Microsoft C++. Per l'installatore serve [Inno Setup 6](https://jrsoftware.org/isinfo.php) e `pwsh installer\build.ps1` (con `-Arm64` per Windows on ARM; il motore di trascrizione ARM64 si compila con `trascrivi\build.ps1` su un PC ARM e richiede anche LLVM).
- **Linux:** compilatore C, `pkgconf` e i pacchetti di sviluppo di GTK 4. La prima volta compila anche OpenH264, FDK AAC e LAME. Avvia `cedro` una volta: resta nella tray e scrive i menu del file. Su GNOME serve l'estensione AppIndicator per l'icona.
  - Pacchetto: `installer/build-linux.sh` crea `dist/cedro-<versione>-linux-x64.tar.gz` (programma, licenze e `install.sh`, che installa per l'utente senza permessi di amministratore).
  - Trascrizione: il motore (whisper.cpp) si compila a parte con `trascrivi/build.sh`, una variante per tipo di processore (`base`, `avx2`, `avxvnni`, `avx512`, `avx512vnni`) e `vulkan` per la scheda video. Servono cmake, clang e libopus; per Vulkan gli header, `glslc` e il caricatore. I file `cedro-trascrivi-<versione>-linux-x64-<variante>` vanno nella release: Cedro li scarica alla prima trascrizione.

Le impostazioni stanno in `%APPDATA%\Cedro\config.txt` (Windows) e `~/.config/cedro/config.txt` (Linux).

### Riga di comando

```
cedro convert foto.png webp
cedro tool foto.png compress --quality 70
cedro zip nota.txt
cedro update
cedro show --conversione foto.png   # Linux: apre la ruota
```

### Sicurezza

Codice aperto (MIT), nessun packer né offuscamento, nessuna rete oltre al controllo aggiornamenti. Gli hook del mouse e della tastiera restano nel processo di Cedro e non iniettano DLL in altri programmi. L'exe non è firmato Authenticode: SmartScreen avvisa la prima volta, ma non è un rilevamento di malware.

### Licenze

Cedro è MIT. Alcune parti hanno la propria licenza: LAME (LGPL, MP3), modello OCR ocrs (CC BY-SA 4.0, fonti in `models/FONTI.txt`), [oxideav-vp8](https://github.com/OxideAV/oxideav-vp8) e [oxideav-vp9](https://github.com/OxideAV/oxideav-vp9) (MIT), OpenH264 (BSD-2) e FDK AAC (licenza Fraunhofer). Cedro è un'implementazione scritta da zero e non usa codice, icone o testi di altri convertitori.

</details>

---

<p align="center">
  Powered by Luca "Realhcok" A.
</p>
