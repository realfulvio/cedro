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

**Installazione su Windows più affidabile: sempre una sola copia di Cedro.**

- Niente più due Cedro installati insieme: la procedura non chiede più «per tutti gli utenti / solo per me», che faceva finire una copia in Programmi e poi l'aggiornamento automatico ne metteva una seconda per l'utente (due voci in App, due collegamenti).
- Se Cedro è già installato per tutti gli utenti si aggiorna lì; se lo reinstalli a mano e ci sono due copie, propone di toglierne una.
- Gli aggiornamenti automatici non riaccendono più l'avvio con Windows se l'avevi spento dal menu, e non rimettono il collegamento sul desktop se l'avevi cancellato.
- Disinstallando a mano chiede se cancellare anche i modelli scaricati per trascrizione e Riassunto AI (possono pesare alcuni GB).
