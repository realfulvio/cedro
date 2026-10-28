## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows on ARM** (Snapdragon X e simili) | [**Cedro-ARM64-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-ARM64-Setup-{versione}.exe) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
Non sai quale ti serve? *Impostazioni di Windows → Sistema → Informazioni*: se in «Tipo di sistema» c'è «basato su ARM», prendi quello ARM; altrimenti l'altro.
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

### Comune di Bondeno

Il Comune di Bondeno non è più un'edizione a parte: è un tema di Cedro, in Impostazioni → Aspetto, con i colori, il carattere e lo stemma del Comune. Chi aveva l'edizione per il Comune si aggiorna da sola e si ritrova il tema già acceso. I file `Cedro-Bondeno-…` e `cedro-bondeno-…` di questa release sono copie degli stessi installatori, tenute solo perché la vecchia edizione cerca quei nomi: chi installa da zero prende i file qui sopra.

## Novità della {versione}

Correzioni di solidità per audio, trascrizione e gestione dei lavori, senza cambiare funzioni o aspetto.

- **YouTube e Video dal web**: l'aggiornamento automatico aspetta che il download e la pulizia dei file temporanei siano finiti. Un download in attesa del turno si può annullare.
- **Ruota su Linux**: le operazioni rispettano lo stesso turno delle conversioni e della trascrizione, evitando lavori pesanti contemporanei nello stesso processo.
- **Audio**: i file troppo grandi per AIFF vengono segnalati senza scrivere intestazioni errate; la scrittura non duplica più l'intero audio in memoria.
- **Trascrizione**: il motore viene chiuso anche se il lavoro si interrompe con un errore interno. Il ricampionamento rifiuta frequenze nulle e controlla quella effettivamente decodificata; una richiesta con una lingua contenente byte nulli restituisce un errore invece di chiudere il motore.
- **Campo dei link**: corretto un errore di disegno del cursore che interrompeva le build di debug quando si modificava il testo.

Restano le novità della 0.26.8: revisione di solidità di video e VA-API, conversioni, PDF e documenti, download dal web e interfacce Windows e Linux.

Restano le novità della 0.26.7: adattatori per Video dal web (Turbovid, catalogo Eurostream, collegamenti Clicka) basati sul motore yt-dlp già presente.

Resta la novità della 0.26.6: decodifica H.264 via VA-API su Linux per MP4, MOV e MKV, con controllo iniziale dei fotogrammi e ripiego sul processore quando necessario.

Restano le novità della 0.26.5: avanzamento dei video con percentuale e tempo residuo, bitrate più contenuto a 720p, ridimensionamento distribuito fra i core e stato «In attesa» durante gli altri lavori pesanti.
