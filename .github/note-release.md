## Scarica questo

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** (quasi tutti i PC) | [**Cedro-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Setup-{versione}.exe) |
| **Windows on ARM** (Snapdragon X e simili) | [**Cedro-ARM64-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-ARM64-Setup-{versione}.exe) |
| **Linux** (Wayland con KDE Plasma o GNOME) | [**cedro-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-{versione}-linux-x64.tar.gz) |

Scarica il file e fai doppio clic: si installa sopra la versione che hai (su Linux: estrai il tar.gz e lancia `./install.sh`, senza permessi di amministratore). Se hai già Cedro non serve nemmeno questo: si aggiorna da solo («Controlla aggiornamenti» nel menu dell'icona).
Non sai quale ti serve? *Impostazioni di Windows → Sistema → Informazioni*: se in «Tipo di sistema» c'è «basato su ARM», prendi quello ARM; altrimenti l'altro.
I motori della trascrizione non vanno scaricati a mano: Cedro li prende da solo la prima volta che trascrivi.

### Edizione Comune di Bondeno

Per i PC del Comune di Bondeno: stesse funzioni, con lo stemma del Comune. Installata sopra Cedro lo sostituisce, e da lì in poi si aggiorna solo con i pacchetti di questa edizione.

| Il tuo computer | File da scaricare |
| --- | --- |
| **Windows** | [**Cedro-Bondeno-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Bondeno-Setup-{versione}.exe) |
| **Windows on ARM** | [**Cedro-Bondeno-ARM64-Setup-{versione}.exe**](https://github.com/{repo}/releases/download/v{versione}/Cedro-Bondeno-ARM64-Setup-{versione}.exe) |
| **Linux** | [**cedro-bondeno-{versione}-linux-x64.tar.gz**](https://github.com/{repo}/releases/download/v{versione}/cedro-bondeno-{versione}-linux-x64.tar.gz) |

## Novità della {versione}

- **Edizione Comune di Bondeno**: una versione a parte di Cedro con lo stemma ufficiale del Comune nelle Impostazioni, nella scheda Informazioni, nella guida, nel menu dell'icona e nell'installatore. Ognuna delle due edizioni si aggiorna solo con i suoi pacchetti.

Dalla 0.18.5: revisione di tutto il codice, con correzioni di robustezza e sicurezza su Windows, Windows on ARM e Linux.

Dalla 0.18.4: guida iniziale rifatta in sei passi, Linux che si aggiorna da solo, riga rossa degli errori che sparisce da sola.

Dalla 0.18.3: su Linux le finestre restano chiare e leggibili anche col tema scuro di KDE o GNOME.

Dalla 0.18.2: trascrizione italiana molto più precisa e nuova scheda «Video dal web».

Dalla 0.18.0: versione nativa per Windows on ARM, con trascrizione ottimizzata (base, dotprod, i8mm e Vulkan per le schede Adreno), scheda Sistema con le istruzioni del processore, YouTube con strumenti ARM64 e la nuova firma col logo hcok.

L'installatore non è firmato: la prima volta Windows può dire «Windows ha protetto il PC». Clicca «Ulteriori informazioni», poi «Esegui comunque». Con Smart App Control acceso va spento dalle impostazioni di Windows.
