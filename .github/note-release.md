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

Release di sola solidità: una revisione completa del codice di Windows e Linux, senza cambiare né funzioni né aspetto.

- **Video**: su Linux la decodifica sulla scheda video (VA-API) libera la memoria della scheda a ogni fotogramma, invece di accumularla nei video lunghi; un file H.264 rovinato ora ripiega sul processore invece di fermare la conversione. Su Windows il colore dei video con larghezza dispari è corretto.
- **Conversioni**: se un lavoro fallisce o un decodificatore si blocca, non restano più file scritti a metà accanto ai tuoi; il salvataggio delle impostazioni non si rovina più quando ruota, pannello e impostazioni salvano insieme.
- **PDF e documenti**: l'ordine dei paragrafi vuoti negli ODT è corretto, i PDF senza font incorporato mantengono punti elenco e simboli, un PDF con coordinate impazzite o con una pagina anomala non blocca più il PDF → Word.
- **Video dal web e trascrizione**: titoli con lettere accentate non interrompono più il download su Windows, i messaggi di errore di yt-dlp arrivano interi, nessun file temporaneo dell'OCR lasciato in giro, pesi dei motori mostrati correttamente.
- **Interfaccia**: se un lavoro va in crisi, la finestra non resta più bloccata su «conversione in corso» e le anteprime continuano a funzionare; l'icona nell'area di notifica torna da sola se Esplora file si riavvia; incollare e copiare testo non perde più memoria; i file trascinati mentre è aperto un selettore non si perdono; su Linux i percorsi con spazi o «#» si aprono dalle Impostazioni e un salvataggio fallito ora lo dice.

Restano le novità della 0.26.7: adattatori per Video dal web (Turbovid, catalogo Eurostream, collegamenti Clicka) basati sul motore yt-dlp già presente.

Resta la novità della 0.26.6: decodifica H.264 via VA-API su Linux per MP4, MOV e MKV, con controllo iniziale dei fotogrammi e ripiego sul processore quando necessario.

Restano le novità della 0.26.5: avanzamento dei video con percentuale e tempo residuo, bitrate più contenuto a 720p, ridimensionamento distribuito fra i core e stato «In attesa» durante gli altri lavori pesanti.
