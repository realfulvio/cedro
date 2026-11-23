## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows portable** (x64, senza installazione) | [**Cedro-Portable-{versione}-x64.zip**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Portable-{versione}-x64.zip) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

## Novità della {versione}

**Cedro parte anche su un computer con Windows appena installato.**

- **Niente più «VCRUNTIME140.dll non è stato trovato».** Su un PC pulito, senza il «Visual C++ Redistributable» di Microsoft, la 0.58.0 e le versioni prima non partivano. Ora il runtime di Visual C++ sta dentro cedro.exe e dentro i motori della trascrizione e del riassunto: l'installatore e la versione portable funzionano ovunque, senza installare altro.
- **Controllato prima di ogni pubblicazione.** Un controllo automatico guarda che nessun file di Cedro importi più `vcruntime140.dll`, `msvcp140.dll` e simili; un altro toglie quelle DLL da un Windows di prova e verifica che l'installatore, la finestra di Cedro, la versione portable e i motori partano lo stesso.
