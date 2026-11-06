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

**Aggiornamenti davvero silenziosi su Windows.** Cedro si aggiorna da solo senza aprire finestre: alla fine compare solo la notifica «Cedro è stato aggiornato alla {versione}».

- L'aggiornamento automatico a volte non partiva («file utilizzato da un altro processo»): ora l'installatore si scarica ogni volta in una cartella sua.
- Anche il «Sì» da «Controlla aggiornamenti» aggiorna in silenzio, senza la procedura guidata dell'installatore.
- Chi ha una Cedro molto vecchia (fino alla 0.26.2), dopo il «Sì» non vede più la procedura guidata: l'installatore si rilancia da solo in silenzio.

**Componenti aggiornati.** Stessi risultati, librerie più nuove.

- OCR: il motore che legge il testo nelle immagini e nei PDF scansionati (rten) passa alla 0.27, la stessa già usata per riconoscere chi parla. Sui PDF di prova il testo letto è identico a prima.
- MP3: il codificatore LAME passa dalla 3.100 alla 4.0. Sulla prova di 10 minuti l'audio esce identico.
- Aggiornamenti minori di altre librerie (zip, flate2 e altre).
