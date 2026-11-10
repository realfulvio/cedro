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

**Due correzioni: le foto in WebP pesano molto meno e il PDF in Word non trasforma più in immagini i riquadri con il testo.**

- JPG (e altre immagini) in WebP: ora il WebP è «leggero» davvero, con la qualità che scegli. Una foto da 80 KB in JPG pesava 570 KB in WebP, ora ne pesa circa 60. Per loghi e grafica, dove il formato senza perdita pesa meno, Cedro tiene quello; la trasparenza resta esatta.
- PDF in Word: le schede colorate e i riquadri con frasi (per esempio un calendario o un volantino impaginato) restano testo da modificare, invece di diventare un'immagine; il testo chiaro su sfondo scuro prende il colore giusto invece di sparire su bianco.
