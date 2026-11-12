## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Windows on ARM** (Snapdragon X e simili) | [**Cedro-ARM64-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-ARM64-Setup-{versione}.exe) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
Non sai quale ti serve? *Impostazioni di Windows → Sistema → Informazioni*: se in «Tipo di sistema» c'è «basato su ARM», prendi quello ARM; altrimenti l'altro.
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Cedro ora c'è anche in versione portable per Windows: si apre senza installare niente.**

- Nella release c'è `Cedro-Portable-{versione}-x64.zip` (Windows x64). Lo estrai dove vuoi, anche su una chiavetta, e apri `cedro.exe`.
- Impostazioni, registro, modelli della trascrizione e programmi scaricati (motore di trascrizione, video dal web) stanno nella cartella `Dati` accanto a `cedro.exe`: niente nel profilo di Windows. Per spostare Cedro basta portarsi dietro la cartella, per toglierlo basta cancellarla.
- Anche la versione portable si aggiorna da sola: scarica lo zip nuovo, ne controlla lo SHA-256, sostituisce i file della cartella (lasciando com'è `Dati`) e si riapre.
- L'installatore resta com'era: la portable è un pacchetto in più.
