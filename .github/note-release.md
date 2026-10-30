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

**Rivedi la trascrizione, e da PDF a Excel.**

- **Rivedi** (Trascrizione): a lavoro finito, accanto al file compare il pulsante «Rivedi». Si apre una schermata con l'onda dell'audio e tutte le frasi con il loro tempo: un clic sul tempo fa sentire da quel punto, l'elenco segue l'audio, e una frase si corregge sul posto (con «Ripristina» per tornare indietro).
- **Difetti di whisper**: Cedro segnala le ripetizioni (la stessa frase di fila, o un giro di parole che si ripete dentro la frase), le frasi tipiche che whisper inventa dove non c'è parlato («Sottotitoli a cura di…») e il testo dove l'audio è quasi muto. Si sistemano uno a uno, oppure tutte le ripetizioni in un colpo.
- **Salva come** Word, testo semplice, **sottotitoli .srt o .vtt** (con i tempi giusti, anche dopo le correzioni). La revisione resta salvata: si riapre anche dopo aver chiuso Cedro.
- **PDF → Excel**: nella scheda PDF il nuovo formato **Excel** mette le tabelle in un file .xlsx, una per foglio o tutte in uno. Numeri, percentuali, euro e date all'italiana diventano valori veri, mentre CAP, protocolli e codici restano testo; le tabelle spezzate su più pagine si riuniscono, il testo fuori dalle tabelle può andare in un foglio a parte. Riconosce anche le tabelle senza bordi, a colonne allineate; le scansioni passano dalla lettura del testo (OCR).
- Tutto è scritto in Rust e funziona su Windows, Windows on ARM e Linux; l'originale non viene mai toccato.

Restano le novità della 0.27.0: versione stabile con le librerie riviste, PDF protetti da password e miniature delle pagine più grandi.

Restano le novità della 0.26.11: numeri di pagina, filigrana, pagine da riordinare e ruotare, compressione dei PDF.

Restano le novità della 0.26.10: unire e dividere i PDF, con l'ordine da trascinare, le pagine da scegliere e la striscia con le forbici.

Restano le novità della 0.26.9: correzioni di solidità per audio, trascrizione e gestione dei lavori, senza cambiare funzioni o aspetto.

Restano le novità della 0.26.8: revisione di solidità di video e VA-API, conversioni, PDF e documenti, download dal web e interfacce Windows e Linux.

Restano le novità della 0.26.7: adattatori per Video dal web (Turbovid, catalogo Eurostream, collegamenti Clicka) basati sul motore yt-dlp già presente.

Resta la novità della 0.26.6: decodifica H.264 via VA-API su Linux per MP4, MOV e MKV, con controllo iniziale dei fotogrammi e ripiego sul processore quando necessario.

Restano le novità della 0.26.5: avanzamento dei video con percentuale e tempo residuo, bitrate più contenuto a 720p, ridimensionamento distribuito fra i core e stato «In attesa» durante gli altri lavori pesanti.
