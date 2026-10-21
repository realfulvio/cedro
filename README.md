# Cedro

Cedro converte e modifica file sul PC, senza caricarli da nessuna parte. Il gesto è quello di una ruota al cursore: si trascina un file, si preme un tasto, si molla sulla fetta. Il programma non è Tangerine e non ne usa codice, icone o testi.

Licenza: MIT. Vedi [LICENSE](LICENSE).

## Gesto

1. Avvia `cedro.exe`. Resta nella tray, senza una finestra di lavoro.
2. In Esplora file trascina un file.
3. Premi **Shift** per la conversione, **Ctrl** per gli strumenti, **Alt** per le azioni.
4. Rilascia il tasto: la ruota resta aperta. Sposta il cursore su una fetta e molla.
5. Esc, oppure mollare al centro o fuori dalla ruota, annulla.

La copia viene scritta nella stessa cartella dell’originale. L’originale non viene sovrascritto. Se il nome esiste già, Cedro aggiunge `-2`, `-3`, e così via.

Tieni premuto Shift solo per aprire la ruota, poi rilascialo prima di mollare il file. Su Windows Shift durante un rilascio può essere interpretato da Esplora file come “sposta”.

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
- Gli hook sono `WH_MOUSE_LL` e `WH_KEYBOARD_LL`. Restano nel processo di Cedro e non iniettano una DLL in Esplora file o in altri programmi. Servono a vedere il tasto e il movimento; il file arriva con il drop OLE normale, come quando si molla un file su una finestra.
- Nessuna rete, nessuna telemetria, nessun download all’avvio.
- L’exe di una build locale non è firmato Authenticode. SmartScreen può dire «Windows ha protetto il PC» la prima volta: è la mancanza di firma, non un rilevamento di malware. «Esegui comunque» vale per quella copia. Una firma Authenticode da autorità pubblica costa e non è inclusa.
- Compilare sul proprio PC evita l’hash di un exe scaricato da un posto sconosciuto, che è la causa più comune di falsi positivi.

## Indipendenza

L’interazione (trascinare, tasto, ruota, rilasciare) è un’idea di interfaccia. Cedro è una implementazione separata, scritta da zero.
