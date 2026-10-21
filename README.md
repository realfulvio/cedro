<p align="center">
  <img src="docs/mark.png" width="120" alt="Segno di Cedro: ruota scura con una fetta verde accesa">
</p>

# Cedro

Tasto destro su un file, più un tasto. Si apre una ruota sul cursore. Si molla sulla fetta e il file viene convertito lì, sul PC. Nessun caricamento.

Cedro 0.4 fa le conversioni da solo. Non serve installare LibreOffice, ffmpeg, Tesseract o Poppler.

Cedro non è Tangerine e non ne usa codice, icone o testi. Licenza [MIT](LICENSE).

## Dopo aver scaricato

Lo zip della [release 0.4.0](https://github.com/realfulvio/cedro/releases/tag/v0.4.0) non è il programma. Dentro c’è il codice. Su Windows si fa così. Non serve Git.

1. Installa Rust da [rustup.rs](https://rustup.rs): scarica `rustup-init.exe` e lancialo. Se chiede gli strumenti di compilazione Microsoft C++, installali e scegli il carico di lavoro «Sviluppo di applicazioni desktop con C++». Senza quelli il comando del punto 4 fallisce. A installazione finita, chiudi e riapri il terminale.
2. Nella release, sotto Assets, scarica **Source code (zip)**. Non c’è un file `.exe`.
3. Estrai lo zip. Apri la cartella che c’è dentro: si chiama `cedro-0.4.0`.
4. Clicca la barra dell’indirizzo di quella cartella, scrivi `powershell` e premi Invio. Poi:

```
cargo build --release
```

La prima volta scarica le librerie, compila il codificatore MP3 e collega i modelli che leggono il testo. Può durare parecchio. Non chiudere la finestra. Finisce senza errori quando torna il cursore del terminale.

5. Il programma è questo file:

```
target\release\cedro.exe
```

6. Doppio clic su `cedro.exe`. Non si apre una finestra. Vicino all’orologio compare l’icona di Cedro. Se non la vedi, apri la freccia delle icone nascoste.
7. La prima volta Windows può dire «Windows ha protetto il PC». L’exe non è firmato. Clicca «Ulteriori informazioni», poi «Esegui comunque».
8. In una cartella qualsiasi, sul desktop o in una finestra Apri/Salva, tieni premuto **Shift** e fai clic destro su un file. Si apre la ruota. Clic sinistro sulla fetta. La copia compare nella stessa cartella. L’originale resta dov’è.
9. Per chiudere: clic sull’icona vicino all’orologio, poi **Esci**.

## Come si usa

<p align="center">
  <img src="docs/desktop.png" alt="Desktop scuro: cursore su foto.png e ruota di conversione con la fetta JPEG accesa">
</p>

<p align="center">
  <img src="docs/passi.png" alt="Tre passi: tasto Shift, clic destro sul file, clic sulla fetta">
</p>

1. Avvia `cedro.exe`. Resta nella tray, senza una finestra di lavoro.
2. In Esplora file, sul desktop o in una finestra Apri/Salva, tieni premuto un tasto e fai clic destro sul file. Vale per qualsiasi cartella, non solo per il desktop.
3. La ruota resta aperta anche se molli il tasto. Vai su una fetta e fai clic sinistro. Oppure tieni premuto il destro, spostati sulla fetta e rilascia.
4. Esc, un clic al centro o fuori dalla ruota annulla.

<p align="center">
  <img src="docs/gesto.png" alt="Tasti Shift, Ctrl e Alt, più il clic destro, aprono la ruota">
</p>

| Tasto | Ruota | A cosa serve |
| --- | --- | --- |
| **Shift** | Conversione | Sceglie il formato nuovo, secondo il tipo di file |
| **Ctrl** | Strumenti | Comprimi, scala, bianco e nero, ruota, metadati, OCR, maiuscole |
| **Alt** | Azioni | Duplica, copia il percorso, ZIP |

<img src="docs/tre-tasti.png" alt="Tre ruote: Shift con JPEG acceso, Ctrl con OCR acceso, Alt con ZIP acceso">

Le fette cambiano con il file. Su una foto, Shift mostra PNG, JPEG, WebP, BMP, GIF, TIFF e PDF. Su un audio mostra WAV e MP3. Su un video mostra MP4. Su un Word mostra PDF. Su un PDF mostra Word e Testo.

Si può anche trascinare il file con il tasto sinistro e premere Shift, Ctrl o Alt. Su Windows, se Shift resta premuto mentre si rilascia un trascinamento, Esplora file può spostare il file. Con il clic destro questo non succede.

Il file viene letto dalla cartella aperta in quel momento: desktop, un disco, una chiavetta, Documenti, Download, una finestra di dialogo. Se sotto il cursore non c’è un file, Cedro lo dice e non fa nulla. Non entra negli altri programmi: in un browser o in un editor il clic destro resta quello di Windows.

## Dove finisce il file

<img src="docs/sul-pc.png" alt="Sul desktop, foto.jpg nasce accanto a foto.png. L'originale resta.">

La copia viene scritta nella stessa cartella dell’originale. L’originale non viene sovrascritto. Se il nome esiste già, Cedro aggiunge `-2`, `-3`, e così via.

## Cosa fa

Immagini (PNG, JPEG, WebP, BMP, GIF, TIFF): conversione, PDF, compressione JPEG, scala 50%, bianco e nero, rotazione 90°, riscrittura senza metadati. Con Ctrl, la fetta OCR legge il testo. Il modello è latino: legge le lettere, non un vocabolario italiano.

Testo (TXT, MD, CSV, TSV, JSON, LOG, XML, HTML): altro testo, PDF, maiuscolo, minuscolo.

Word (DOC, DOCX, ODT, RTF): PDF di testo. Restano paragrafi e parole. Impaginazione, font incorporati e caselle di testo no. Il vecchio DOC viene letto dall’indice pubblico del formato Word.

PDF: con Shift, Word di testo oppure un file di testo. La pagina non viene ricostruita. Con Ctrl, OCR: se il PDF ha già del testo, Cedro lo copia; se non ne ha, disegna al massimo 20 pagine e le legge.

Audio (WAV, MP3, OGG, M4A, AAC, FLAC): WAV e MP3. L’MP3 è LAME, compilato dentro l’eseguibile, licenza LGPL. Un file Opus apre la ruota, ma questo motore non lo decodifica.

Video (MP4, WebM, MOV, MKV, AVI): MP4 con H.264 e AAC, tramite i codificatori già presenti in Windows. Non è un ffmpeg a parte. Se Windows non sa aprire quel file, o manca il codificatore (capita su alcune edizioni N), la ruota lo dice e non scrive un file a metà.

Azioni su qualsiasi file: duplica, copia il percorso, ZIP non compresso (metodo store).

HEIC non è nel motore. Il PDF scritto da Cedro per un’immagine o per un testo semplice non è un PDF/A.

## Riga di comando

La qualità JPEG, da 40 a 95, sta in `%APPDATA%\Cedro\config.txt`:

```
quality=82
```

Si può anche convertire senza la ruota. Su Windows, dalla cartella del programma:

```
target\release\cedro.exe convert foto.png webp
target\release\cedro.exe tool foto.png compress --quality 70
target\release\cedro.exe tool nota.txt upper
target\release\cedro.exe zip nota.txt
```

Senza argomenti, `cedro.exe` apre la ruota nella tray.

## Antivirus

- Codice aperto, licenza MIT, niente packer, niente UPX, niente offuscamento.
- Gli hook sono `WH_MOUSE_LL` e `WH_KEYBOARD_LL`. Restano nel processo di Cedro e non iniettano una DLL in Esplora file o in altri programmi. Il clic destro con Shift, Ctrl o Alt viene intercettato sul desktop, in ogni finestra di Esplora file e nelle finestre Apri/Salva. Il nome del file si legge dalla cartella di quella finestra, senza scrivere nella memoria di un altro processo. Il trascinamento, se lo usi, consegna il file con il drop OLE normale.
- Nessuna rete, nessuna telemetria, nessun download all’avvio. I modelli di lettura sono già nel programma.
- L’exe di una build locale non è firmato Authenticode. SmartScreen può dire «Windows ha protetto il PC» la prima volta: è la mancanza di firma, non un rilevamento di malware. «Esegui comunque» vale per quella copia. Una firma Authenticode da autorità pubblica costa e non è inclusa.
- Compilare sul proprio PC evita l’hash di un exe scaricato da un posto sconosciuto, che è la causa più comune di falsi positivi.

## Licenze nel programma

Il codice di Cedro è MIT. Due pezzi portano la propria licenza e restano separati: LAME, per l’MP3, è LGPL; i pesi del modello OCR sono CC BY-SA 4.0 (Robert Knight, ocrs). I nomi e i link stanno in `models/FONTI.txt`.

## Indipendenza

L’interazione (tasto, clic destro, ruota, rilasciare) è un’idea di interfaccia. Cedro è una implementazione separata, scritta da zero.
