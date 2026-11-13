## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Video dal web non si blocca più sui sottotitoli, e in alto a destra torna la riga che dice cosa sta facendo Cedro.**

- YouTube in questi giorni rifiuta spesso i sottotitoli («troppe richieste») e il download si fermava con «Il sito ha limitato le richieste». Ora, se i sottotitoli non arrivano, Cedro scarica comunque il video senza e lo scrive nel registro.
- In alto a destra, accanto ai pulsanti della finestra, una riga dice cosa accelera il lavoro in corso (scheda video con la tecnica, oppure processore con istruzioni e thread) e quando Cedro scarica o aggiorna un componente: yt-dlp, ffmpeg, deno, motori e modelli di trascrizione e riassunto, con quanto manca e una barretta. A riposo mostra la scheda video pronta, o «Solo processore».
