## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Cedro non si chiude più davanti a un file rovinato.**

- Un video MOV o MP4 con un blocco danneggiato dentro non fa più girare Cedro all'infinito (neppure nell'anteprima): dice che il video non si legge.
- Le foto (DDS, BMP e altre) con misure assurde nell'intestazione, i vetrini NDPI e SVS e i TIFF dei microscopi (OME-TIFF) con valori impossibili non fanno più chiudere Cedro per mancanza di memoria: danno un errore e il resto della coda va avanti.
- Nel riconoscimento delle voci un risultato non valido del modello non ferma più la trascrizione.
- Provato rovinando a caso 52 tipi di file (video, audio, foto, Office, PDF, ZIP, vetrini, firme digitali) circa 6.000 volte.
