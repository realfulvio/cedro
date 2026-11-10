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

**Il PDF in Word legge bene i calendari a caselle, e il WebP ha il cursore della qualità.**

- PDF in Word: le tabelle disegnate casella per casella (un calendario con un riquadro per ogni giorno, un planner, un orario) tornano una tabella sola, con le sue colonne e righe, invece di decine di tabelline. Un calendario di prova di 6 pagine, che veniva fuori di 51 pagine, ora resta di 6 con tutte le parole.
- PDF in Word: i titoli bianchi su una fascia colorata non spariscono più sulla carta bianca.
- WebP e AVIF: nella ruota e in Conversioni c'è il cursore «Qualità», come per il JPG; prima la scelta c'era solo per JPG e compressione.
