## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows on ARM** (Snapdragon X e simili) | [**Cedro-ARM64-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-ARM64-Setup-{versione}.exe) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
Non sai quale ti serve? *Impostazioni di Windows → Sistema → Informazioni*: se in «Tipo di sistema» c'è «basato su ARM», prendi quello ARM; altrimenti l'altro.
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Pagine e compressione dei PDF.** Nella scheda PDF i pulsanti in cima al pannello di destra diventano cinque: Converti · Unisci · Dividi · **Pagine** · **Comprimi**.

- **Pagine**: le pagine del PDF compaiono come miniature vere. Un clic le sceglie, trascinandole cambi l'ordine; i comandi in alto le ruotano di 90° a sinistra o a destra, le tolgono (o le rimettono) e le copiano in un PDF nuovo con «Estrai». Il riquadro «Cosa succederà» riassume le modifiche prima di salvare.
- **Numeri di pagina**: nove posizioni sulla pagina, formato «1», «Pagina 1 di N» o «1 / N», primo numero e corpo a scelta.
- **Filigrana**: un testo come «BOZZA», con opacità, direzione (diagonale o orizzontale) e pagine a scelta. Con numeri o filigrana accesi vedi in grande la pagina vera come uscirà.
- **Comprimi**: per ogni PDF vedi il peso di adesso e la stima di dopo, calcolata comprimendo davvero in memoria, con il risparmio e il totale. Tre livelli (Leggera, Equilibrata, Forte), metadati tolti e scansioni in bianco e nero che restano nitide pesando molto meno. Un PDF che non si riesce a ridurre resta com'è: non se ne salva mai uno più pesante.
- **Ruota** (Ctrl sui PDF): compare «Comprimi», con il livello usato l'ultima volta.
- **Modalità Semplice**: compare «Più piccolo · per l'email», che dice subito quanto si risparmia.
- Gli originali non vengono mai toccati. Tutto è scritto in Rust e funziona su Windows, Windows on ARM e Linux.

Restano le novità della 0.26.10: unire e dividere i PDF, con l'ordine da trascinare, le pagine da scegliere e la striscia con le forbici.

Restano le novità della 0.26.9: correzioni di solidità per audio, trascrizione e gestione dei lavori, senza cambiare funzioni o aspetto.

Restano le novità della 0.26.8: revisione di solidità di video e VA-API, conversioni, PDF e documenti, download dal web e interfacce Windows e Linux.

Restano le novità della 0.26.7: adattatori per Video dal web (Turbovid, catalogo Eurostream, collegamenti Clicka) basati sul motore yt-dlp già presente.

Resta la novità della 0.26.6: decodifica H.264 via VA-API su Linux per MP4, MOV e MKV, con controllo iniziale dei fotogrammi e ripiego sul processore quando necessario.

Restano le novità della 0.26.5: avanzamento dei video con percentuale e tempo residuo, bitrate più contenuto a 720p, ridimensionamento distribuito fra i core e stato «In attesa» durante gli altri lavori pesanti.
