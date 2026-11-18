## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Cedro non si chiude più davanti a un file rovinato, e Dividi e Unisci PDF non creano più nomi impossibili su Windows.**

- I vetrini Olympus `.vsi` con misure impossibili nel file non fanno più chiudere Cedro per mancanza di memoria: dà un errore e il resto della coda va avanti.
- Una foto BMP o DDS rovinata nell'elenco dei file non fa più chiudere Cedro quando prepara la miniatura: la miniatura manca e basta.
- Dividi con il nome del segnalibro («%titolo%») e Unisci con il nome scelto per il file unito: se il nome è Con, Aux, Nul, Com1 o Lpt9 (nomi riservati di Windows) ora il file si chiama `_Con.pdf` e non fallisce più il salvataggio.
- Provato rovinando a caso più di 5.000 volte file di ogni tipo (video, audio, foto, Office, PDF, ZIP, vetrini, CD DICOM, referti e impostazioni).
