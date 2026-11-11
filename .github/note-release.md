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

**Nuovo tema «Cedro 4 Labs», pensato per i laboratori, e l'installatore ti fa scegliere come lavorare.**

- Alla prima installazione a mano compare la pagina «Come lavori con Cedro?»: scegli **Cedro standard** oppure **Cedro 4 Labs**, con due anteprime animate. Gli aggiornamenti e le installazioni silenziose non la mostrano; da riga di comando si può scegliere con `/TEMA=labs`. Su Linux lo script di installazione ha l'opzione `--labs`.
- **Cedro 4 Labs** è un tema nuovo (si sceglie anche da Impostazioni › Aspetto) in verde petrolio, con la sigla «4 LABS» in magenta accanto al nome. Mostra solo ciò che serve in laboratorio e in ufficio: conversioni, trascrizione e firma digitale, senza YouTube e video dal web.
- Nella Guida e nella finestra «Come vuoi usare Cedro?» con questo tema non compare più l'argomento su YouTube e sul web.
- I temi ora sono sette.
