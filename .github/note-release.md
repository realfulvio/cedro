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

- **Video dal web: Turbovid**: Cedro include un adattatore che riconosce il player pubblico e il suo MP4, sia nell'analisi del link sia nel download, su Windows e Linux.
- **Catalogo Eurostream**: riconoscimento del catalogo «Matrimonio a prima vista Italia», con titoli e numerazione delle sezioni conservati nella playlist.
- **Collegamenti Clicka**: Cedro segue i collegamenti pubblici riconosciuti verso Turbovid. Se il sito richiede un CAPTCHA o un accesso, occorre aprirlo nel browser e usare poi il link pubblico del player; il download dell'intero catalogo non è garantito.

Gli adattatori inclusi usano il motore yt-dlp già presente in Cedro e mantengono disponibili i plugin dell'utente. StreamingCommunity, DeltaBit e MixDrop non hanno adattatori dedicati in questa versione.

Resta la novità della 0.26.6: decodifica H.264 via VA-API su Linux per MP4, MOV e MKV, con controllo iniziale dei fotogrammi e ripiego sul processore quando necessario.

Restano le novità della 0.26.5: avanzamento dei video con percentuale e tempo residuo, bitrate più contenuto a 720p, ridimensionamento distribuito fra i core e stato «In attesa» durante gli altri lavori pesanti.
