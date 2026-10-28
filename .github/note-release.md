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

- **Preparazione della decodifica video sulla scheda video (Linux)**: il codice per leggere i video H.264 con la GPU è dentro, con il controllo automatico contro OpenH264, ma resta spento finché non è provato su tutte le schede. Per ora i video si leggono come prima: nessuna differenza per chi usa Cedro.

Dalla 0.26.5: barra di avanzamento e tempo che resta nei video, file più piccoli a 720p da sorgenti già compresse, «In attesa» vero (invariati).
