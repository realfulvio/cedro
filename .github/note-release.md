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

- **Barra di avanzamento per i video**: nella riga del file che si sta convertendo compaiono percentuale e tempo che resta (in tutti i temi, su Windows e Linux), e la barra del lavoro intero avanza con il file. Prima restava «In corso» senza segni di vita per tutta la durata.
- **File più piccoli a 720p**: se il video di partenza è già molto compresso, il bitrate di uscita non lo supera più di troppo (un 1080p60 da 666 MB diventava 617 MB; ora circa un terzo in meno). Con sorgenti ricche il risultato non cambia.
- **Ridimensionamento più veloce** (stesso risultato bit per bit): le righe si dividono fra i core.
- **«In attesa» vero**: se un altro lavoro pesante occupa il turno (per esempio una trascrizione), il file resta «In attesa» invece di mostrare «In corso»; e la trascrizione lo dice in «Cosa sta succedendo».

Dalla 0.26.4: audio a pacchetti (meno memoria), coda comune dei lavori pesanti, iGPU e Vulkan restano attivi, aggiornamenti silenziosi (invariati).
