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

**Riassunto AI delle riunioni trascritte.**

- Dopo una trascrizione, accanto a «Rivedi» c'è il pulsante «Riassunto AI». In Rivedi trascrizione c'è la scheda «Riassunto» con quattro parti: In breve, Chi ha detto cosa, Decisioni e Da fare (con la casella da spuntare e il nome di chi se ne occupa).
- Usa i nomi che hai dato agli interlocutori; se li cambi dopo, il riassunto si aggiorna da solo.
- Lo scrive Qwen3.5-4B, un modello che gira tutto sul PC: niente va su internet. Usa la scheda video se c'è (Vulkan), altrimenti il processore.
- Prima di partire controlla la memoria libera: servono circa 4 GB, e se non ci sono non parte e lo dice. Il modello si carica solo per il riassunto e poi si chiude, liberando la memoria. Le riunioni lunghe si leggono a pezzi.
- Il riassunto resta salvato con la trascrizione, si copia con un clic e si può mettere in cima al Word o al testo.
- La prima volta Cedro scarica il motore e il modello, circa 2,7 GB.
