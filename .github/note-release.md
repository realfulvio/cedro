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

**«Aggiungi cartella» prende anche le sottocartelle, e «Togli GPS» toglie anche il video nascosto nelle foto in movimento.**

- «Aggiungi cartella» (in Conversioni, nel verificare e nel firmare le firme) prima prendeva solo i file direttamente nella cartella: ora scende anche nelle sottocartelle, in ordine di nome. Si saltano le cartelle nascoste (come `.git`) e i collegamenti a cartelle, e c'è un tetto di file per non riempire la coda con un disco intero.
- Togli GPS e dati nascosti: nelle foto in movimento (Motion Photo di Google, Samsung) il video, con la sua posizione, sta attaccato in fondo al JPEG e la versione prima lo lasciava dov'era. Ora si toglie, e nel messaggio compare «dati in coda». Le altre immagini dentro il file (come quelle dell'HDR) restano, i pixel restano identici.
