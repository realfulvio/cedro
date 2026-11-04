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

**Ritocchi ai temi della 0.33.0**, dopo le prove con la finestra stretta e con la coda piena.

- Windows essenziale e Comune di Bondeno: «Rimuovi» non finisce più sopra il numero dei file; se manca spazio i comandi restano con la sola icona.
- Cedro compatto, anche nella finestrella dell'icona: «Altri file in coda» non taglia più l'ultimo tipo di file.
- Documento al centro: in Semplificata la scelta della modalità non copre più la scheda «Video dal web»; l'anteprima dell'originale non esce dal riquadro; per MP3, WAV e MP4 l'icona d'uscita è quella audio/video e «tutte le pagine» compare solo per i PDF.
- Impostazioni: in Aspetto le descrizioni di Standard e Semplificata non toccano più il bordo; su Windows, in Sistema, le istruzioni del processore restano accostate anche a finestra larga.
