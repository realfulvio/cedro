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

**Unire e dividere i PDF.** Nella scheda PDF, in cima al pannello di destra, ora ci sono tre pulsanti: «Cosa vuoi fare: Converti · Unisci · Dividi».

- **Unisci**: l'ordine lo decidi tu, trascinando le righe dalla maniglia o con le frecce. Il numero colorato su ogni file indica il suo posto nel file unito. Per ogni file puoi scegliere quali pagine entrano (per esempio 1-3,5), e una striscia ti mostra le pagine del file unito. Ogni file diventa un segnalibro; i segnalibri, i moduli compilabili e i link interni restano. Volendo si aggiunge una pagina bianca per la stampa fronte-retro e si protegge il risultato con una password.
- **Dividi**: sotto il file compare la striscia delle sue pagine, e un clic tra due pagine mette o toglie un taglio. Puoi anche dividere ogni pagina, ogni N pagine, per intervalli (1-3, 4-6, 7-), ai segnalibri, oppure togliere delle pagine.
- I PDF protetti chiedono la password. Gli originali non vengono mai toccati.
- **Ruota** (Ctrl sui PDF): «Unisci» mette insieme i PDF scelti nell'ordine dei nomi, «Dividi» fa un file per pagina.
- **Modalità Semplice**: compaiono «Un solo PDF» (con almeno due PDF) e «Dividi».
- Tutto è scritto in Rust (lopdf per le pagine, hayro per le miniature) e funziona su Windows, Windows on ARM e Linux.

Restano le novità della 0.26.9: correzioni di solidità per audio, trascrizione e gestione dei lavori, senza cambiare funzioni o aspetto.

Restano le novità della 0.26.8: revisione di solidità di video e VA-API, conversioni, PDF e documenti, download dal web e interfacce Windows e Linux.

Restano le novità della 0.26.7: adattatori per Video dal web (Turbovid, catalogo Eurostream, collegamenti Clicka) basati sul motore yt-dlp già presente.

Resta la novità della 0.26.6: decodifica H.264 via VA-API su Linux per MP4, MOV e MKV, con controllo iniziale dei fotogrammi e ripiego sul processore quando necessario.

Restano le novità della 0.26.5: avanzamento dei video con percentuale e tempo residuo, bitrate più contenuto a 720p, ridimensionamento distribuito fra i core e stato «In attesa» durante gli altri lavori pesanti.
