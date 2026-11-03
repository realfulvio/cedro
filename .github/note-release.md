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

**Barra di avanzamento nella trascrizione.**

- Mentre trascrive, sopra «Annulla» c'è un riquadro con la percentuale grande, il file su cui lavora («file 1 di 3»), la barra e quanto manca, calcolato dalla velocità vera.
- Prima che la trascrizione parta dice cosa sta facendo: scarica il modello, accende il motore, riconosce le voci.
- La scheda video si legge col solo modello: «Quadro RTX 4000», «Radeon integrata», senza sigle tecniche. Col processore c'è scritto «Processore · 8 thread». Lo stesso nel riassunto AI.
- La barra del lavoro intero ora avanza anche durante il file, non solo alla fine.
