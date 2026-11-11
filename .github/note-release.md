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

**Cedro apre i vetrini digitali (.svs) e li converte in immagini.**

- Metti un vetrino Aperio (`.svs`) nella scheda Immagini e Cedro lo riconosce da solo: in coda compare «Vetrino» con la riga che dice quanti livelli ha, com'è compresso e a quanti ingrandimenti è stato scansionato.
- Nel riquadro «Vetrino» scegli il livello: **Panoramica** (per guardarlo o metterlo in un documento), **Media** (il più grande che sta comodo in memoria, è la scelta di partenza) o **Piena** (ogni dettaglio dello scanner). Accanto a ogni scelta vedi le misure e il peso a occhio.
- L'**etichetta** col codice del vetrino e la **foto del vetrino intero** escono accanto al file, nello stesso formato, e si tolgono con un interruttore.
- Il livello pieno, anche da miliardi di pixel, si scrive a strisce in PNG, JPEG o TIFF senza riempire la memoria, usando tutti i core: 1,5 miliardi di pixel in 15 secondi in PNG e 19 in JPEG su un PC a 4 core. I formati che non reggono quelle misure escono in TIFF, e il messaggio lo dice.
- Funzionano anche le tessere in JPEG 2000. OCR e PDF aprono i vetrini dal livello Media.
- **Salvare in JPEG è fino a 23 volte più veloce**, per qualunque foto: prima la scrittura su disco era a piccoli pezzi.
