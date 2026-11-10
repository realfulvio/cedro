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

**Il PDF in Word legge fino in fondo i calendari a caselle.**

- Le pagine fitte di caselle con scritte brevi (un calendario con i giorni, le etichette dei ritiri, i colori) non vengono più scambiate per un modulo: restano una tabella in Word, con le sue righe e colonne, invece di centinaia di forme sparse. Il calendario dei rifiuti di prova passa da una tabella a tre, con tutti i mesi.
- La riga dei giorni della settimana (LUN, MAR…) sopra il calendario è la prima riga della tabella, ognuno nella sua cella, anche se gli angoli sono arrotondati.
- Le etichette bianche sui bollini colorati («Umido», «Carta») non spariscono più sulla carta bianca: prendono il colore del bollino. Il numero del giorno e le etichette in basso nella stessa casella sono righe separate, e il numero non sparisce più.
- Il margine in fondo alla pagina lascia posto al piè di pagina, che non finisce da solo su una pagina in più.
