## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Cedro 4 Labs legge i file Zeiss CZI.**

- Si aprono i `.czi` di ZEN (confocali LSM, Axio Observer e Imager, Axio Scan, Celldiscoverer) come già i ND2 e i LIF: anteprima con i canali a colori, conversione in PNG, JPEG, TIFF e PDF, un file per canale o per piano Z, tutti gli istanti, valori originali a 16 bit e barra di scala dalla misura scritta nel file.
- Ogni scena del file (più posizioni o più aree del vetrino) esce come immagine a sé, con il suo nome. I mosaici si ricompongono come in ZEN.
- Si leggono tutte le compressioni di ZEN, compreso il JPEG XR dei vetrini. I vetrini troppo grandi si aprono ridotti, e Cedro dice di quanto.
- I pixel sono gli stessi che dà la libreria ufficiale di Zeiss.
