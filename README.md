<p align="center">
  <img src="docs/mark.jpg" width="120" alt="Segno di Cedro, una ruota color cedro">
</p>

# Cedro

Tasto destro su un file, più un tasto. Si apre una ruota sul cursore. Si molla sulla fetta e il file viene convertito lì, sul PC. Nessun caricamento.

Cedro non è Tangerine e non ne usa codice, icone o testi. Licenza [MIT](LICENSE).

<p align="center">
  <img src="docs/desktop.jpg" alt="Desktop: la ruota di Cedro è aperta su foto.png, la fetta JPEG è accesa">
</p>

## Come si usa

<p align="center">
  <img src="docs/passi.jpg" alt="Tre passi: tasto Shift, scelta della fetta JPEG, file nuovo accanto">
</p>

1. Avvia `cedro.exe`. Resta nella tray, senza una finestra di lavoro.
2. Sul desktop o in Esplora file tieni premuto un tasto e fai clic destro sul file.
3. La ruota resta aperta anche se molli il tasto. Vai su una fetta e fai clic sinistro. Oppure tieni premuto il destro, spostati sulla fetta e rilascia.
4. Esc, un clic al centro o fuori dalla ruota annulla.

<p align="center">
  <img src="docs/gesto.jpg" alt="Mouse con il tasto destro acceso, tasti Shift, Ctrl e Alt, ruota con una fetta accesa">
</p>

| Tasto | Ruota | A cosa serve |
| --- | --- | --- |
| **Shift** | Conversione | PNG, JPEG, WebP, BMP, GIF, TIFF, PDF |
| **Ctrl** | Strumenti | Comprimi, scala, bianco e nero, ruota, metadati |
| **Alt** | Azioni | Duplica, copia il percorso, ZIP |

<img src="docs/tre-tasti.png" alt="Tre schede: Shift conversione, Ctrl strumenti, Alt azioni">

Si può anche trascinare il file con il tasto sinistro e premere Shift, Ctrl o Alt. Su Windows, se Shift resta premuto mentre si rilascia un trascinamento, Esplora file può spostare il file. Con il clic destro questo non succede.

Il file viene riconosciuto dal nome sull’icona del desktop, oppure dalla cartella aperta in Esplora file. Se sotto il cursore non c’è un file, Cedro lo dice e non fa nulla.

## Dove finisce il file

<img src="docs/sul-pc.png" alt="La copia foto-2.jpg nasce accanto a foto.png. L’originale non viene toccato.">

La copia viene scritta nella stessa cartella dell’originale. L’originale non viene sovrascritto. Se il nome esiste già, Cedro aggiunge `-2`, `-3`, e così via.

## Cosa fa

Immagini (PNG, JPEG, WebP, BMP, GIF, TIFF): conversione, PDF, compressione JPEG, scala 50%, bianco e nero, rotazione 90°, riscrittura senza metadati.

Testo (TXT, MD, CSV e affini): altro testo, PDF, maiuscolo, minuscolo.

Azioni su qualsiasi file: duplica, copia il percorso, ZIP non compresso (metodo store).

Audio e video solo se `ffmpeg` è già nel PATH. Cedro non lo include: un binario ffmpeg da decine di megabyte è pesante e finisce spesso nelle euristiche degli antivirus. WAV, MP3 e MP4 usano il ffmpeg di sistema e, se manca, la ruota lo dice.

HEIC e i documenti Office non sono nel motore. Il PDF in uscita è testo semplice oppure una pagina con l’immagine, non un PDF/A.

## Compilare

Su Windows, con [rustup](https://rustup.rs):

```
cargo build --release
```

L’eseguibile è `target\release\cedro.exe`. Si può copiare dove si vuole: non ha installer e non scrive nel registro, a parte la tray del processo in esecuzione. Configurazione e log stanno in `%APPDATA%\Cedro\`.

```
quality=82
```

in `%APPDATA%\Cedro\config.txt` è la qualità JPEG, da 40 a 95.

Da riga di comando, anche su Linux per il motore:

```
cedro convert foto.png webp
cedro tool foto.png compress --quality 70
cedro tool nota.txt upper
cedro zip nota.txt
```

## Antivirus

- Codice aperto, licenza MIT, niente packer, niente UPX, niente offuscamento.
- Gli hook sono `WH_MOUSE_LL` e `WH_KEYBOARD_LL`. Restano nel processo di Cedro e non iniettano una DLL in Esplora file o in altri programmi. Il clic destro con Shift, Ctrl o Alt viene intercettato solo sul desktop e sulle finestre di Esplora file. Il nome del file si legge con UI Automation, senza scrivere nella memoria di un altro processo. Il trascinamento, se lo usi, consegna il file con il drop OLE normale.
- Nessuna rete, nessuna telemetria, nessun download all’avvio.
- L’exe di una build locale non è firmato Authenticode. SmartScreen può dire «Windows ha protetto il PC» la prima volta: è la mancanza di firma, non un rilevamento di malware. «Esegui comunque» vale per quella copia. Una firma Authenticode da autorità pubblica costa e non è inclusa.
- Compilare sul proprio PC evita l’hash di un exe scaricato da un posto sconosciuto, che è la causa più comune di falsi positivi.

## Indipendenza

L’interazione (tasto, clic destro, ruota, rilasciare) è un’idea di interfaccia. Cedro è una implementazione separata, scritta da zero.
