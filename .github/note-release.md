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

**Correzione.** PDF → testo o Word su un PDF con password di apertura: ora Cedro dice che il PDF è protetto, invece di «nessuna pagina in questo intervallo».

Restano le novità della 0.27.0:

**Versione stabile: librerie riviste e più solide.** Nessuna funzione nuova: Cedro diventa più robusto con i file difficili.

- **PDF**: un PDF costruito con oggetti annidati in profondità non fa più chiudere Cedro (prima il programma si chiudeva di colpo); il testo estratto dai PDF non incolla più le parole a fine riga; Unisci, Dividi e Pagine aprono come prima i PDF protetti da password.
- **Documenti Word 97-2003 (.doc)**: si aprono anche quelli scritti da programmi che lasciano la struttura interna un po' imperfetta.
- **Opus** (vocali di WhatsApp e Telegram): libreria Opus aggiornata alla 1.6.1, sempre compresa nel programma, anche nel motore di trascrizione.
- **Anteprima audio**: se l'uscita audio non risponde (per esempio con il servizio Audio di Windows fermo) il tasto Play va avanti lo stesso, senza suono.
- **Pagine dei PDF**: le miniature sono più grandi e si adattano allo spazio della scheda.
- **Video WebM VP9**: lettura più veloce, con le immagini identiche a prima.
- Funziona come prima su Windows, Windows on ARM e Linux.

Restano le novità della 0.26.11: numeri di pagina, filigrana, pagine da riordinare e ruotare, compressione dei PDF.

Restano le novità della 0.26.10: unire e dividere i PDF, con l'ordine da trascinare, le pagine da scegliere e la striscia con le forbici.

Restano le novità della 0.26.9: correzioni di solidità per audio, trascrizione e gestione dei lavori, senza cambiare funzioni o aspetto.

Restano le novità della 0.26.8: revisione di solidità di video e VA-API, conversioni, PDF e documenti, download dal web e interfacce Windows e Linux.

Restano le novità della 0.26.7: adattatori per Video dal web (Turbovid, catalogo Eurostream, collegamenti Clicka) basati sul motore yt-dlp già presente.

Resta la novità della 0.26.6: decodifica H.264 via VA-API su Linux per MP4, MOV e MKV, con controllo iniziale dei fotogrammi e ripiego sul processore quando necessario.

Restano le novità della 0.26.5: avanzamento dei video con percentuale e tempo residuo, bitrate più contenuto a 720p, ridimensionamento distribuito fra i core e stato «In attesa» durante gli altri lavori pesanti.
